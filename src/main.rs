use actix_files::Files;
use actix_session::storage::CookieSessionStore;
use actix_session::{config::PersistentSession, Session, SessionMiddleware};
use actix_web::cookie::{time::Duration as CookieDuration, Key};
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use nova::search_engine::{SearchCategory, SearchResult};
use nova::workspace::{self, CreateProjectPayload, MAX_MEMBERS_PER_PROJECT, MAX_PROJECTS_PER_DAY};
use nova::{DbConfig, SearchEngine, TfIdfEngine};
use rustls::crypto::aws_lc_rs;
use rustls::ServerConfig;
use rustls_pemfile::{certs, private_key};
use serde::Deserialize;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::sync::Mutex;
use tera::{Context, Tera};
use uuid::Uuid;

mod auth;
mod forum;

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    /// id du nœud parent dans l'arbre de recherche (présent quand on affine une recherche)
    parent: Option<String>,
    /// projet courant (optionnel) : rattache cette recherche à un projet/groupe
    project: Option<String>,
    /// numéro de page pour "Charger plus" (page 1 déjà affichée par la recherche initiale)
    page: Option<u32>,
}

#[derive(Debug, sqlx::FromRow)]
struct ArticleRow {
    id: i64,
    title: String,
    description: Option<String>,
    url: String,
    category: Option<String>,
}

lazy_static::lazy_static! {
    static ref CACHE: Mutex<HashMap<String, Vec<SearchResult>>> =
        Mutex::new(HashMap::new());
}

/// Récupère l'id de session anonyme, ou en crée un nouveau si absent.
fn get_or_create_session_id(session: &Session) -> String {
    if let Ok(Some(id)) = session.get::<String>("session_id") {
        id
    } else {
        let id = Uuid::new_v4().to_string();
        let _ = session.insert("session_id", &id);
        id
    }
}

async fn load_tfidf_engine(pool: &SqlitePool) -> TfIdfEngine {
    let mut engine = TfIdfEngine::new();

    let rows: Vec<ArticleRow> =
        sqlx::query_as("SELECT id, title, description, url, category FROM articles")
            .fetch_all(pool)
            .await
            .unwrap_or_else(|e| {
                eprintln!("⚠️  Impossible de charger articles.db: {e}");
                Vec::new()
            });

    println!("📚 {} article(s) chargé(s) depuis articles.db", rows.len());

    for row in rows {
        engine.add_document(
            row.id,
            &row.title,
            row.description.as_deref().unwrap_or(""),
            row.url,
            row.category,
        );
    }

    engine.compute_idf();
    engine
}

fn local_results(engine: &TfIdfEngine, query: &str, top_k: usize) -> Vec<SearchResult> {
    engine
        .search(query, top_k)
        .into_iter()
        .filter_map(|(id, _score)| engine.get_document(id))
        .map(|doc| SearchResult {
            title: doc.title.clone(),
            url: doc.url.clone(),
            snippet: doc.description.clone(),
            img_src: None,
            thumbnail: None,
            thumbnail_src: None,
            iframe_src: None,
            published_date: None,
            source: Some("Base locale".to_string()),
        })
        .collect()
}

#[derive(Deserialize)]
struct IndexQuery {
    project: Option<String>,
    join_message: Option<String>,
}

async fn index(
    tera: web::Data<Tera>,
    session: Session,
    query: web::Query<IndexQuery>,
) -> impl Responder {
    match session.get::<i64>("user_id") {
        Ok(Some(_)) => {}

        _ => {
            return HttpResponse::Found()
                .append_header(("Location", "/login"))
                .finish();
        }
    }

    let _ = get_or_create_session_id(&session);

    let mut ctx = Context::new();
    ctx.insert("results", &Vec::<SearchResult>::new());
    ctx.insert("query", &"");
    ctx.insert("category", &"web");
    ctx.insert("error", &"");
    ctx.insert("current_node_id", &Option::<String>::None);
    ctx.insert("current_project", &query.project);
    ctx.insert("flash_message", &query.join_message);
    ctx.insert("ancestors", &Vec::<nova::workspace::SearchNode>::new());
    ctx.insert("siblings", &Vec::<nova::workspace::SearchNode>::new());

    let rendered = tera.render("index.html", &ctx).unwrap_or_else(|e| {
        eprintln!("Erreur de rendu Tera: {e}");
        "<h1>Erreur interne du serveur</h1>".to_string()
    });
    HttpResponse::Ok().body(rendered)
}

async fn search_handler(
    query: web::Query<SearchQuery>,
    tera: web::Data<Tera>,
    tfidf: web::Data<TfIdfEngine>,
    pool: web::Data<SqlitePool>,
    session: Session,
    category: SearchCategory,
) -> impl Responder {
    let session_id = get_or_create_session_id(&session);
    let trimmed_query = query.q.trim().to_string();
    let project_id = query.project.clone();

    if trimmed_query.is_empty() {
        let mut ctx = Context::new();
        ctx.insert("results", &Vec::<SearchResult>::new());
        ctx.insert("query", &trimmed_query);
        ctx.insert("category", &category.as_str());
        ctx.insert("error", &"Veuillez entrer un terme de recherche");
        ctx.insert("current_node_id", &Option::<String>::None);
        ctx.insert("current_project", &project_id);
        ctx.insert("ancestors", &Vec::<nova::workspace::SearchNode>::new());
        ctx.insert("flash_message", &Option::<String>::None);
        ctx.insert("siblings", &Vec::<nova::workspace::SearchNode>::new());

        let rendered = tera.render("index.html", &ctx).unwrap_or_else(|e| {
            eprintln!("❌ Erreur de rendu Tera dans search_handler: {:#}", e);
            format!("<pre>Erreur de rendu du template:\n{:#}</pre>", e)
        });
        return HttpResponse::Ok().body(rendered);
    }

    // --- Arbre de branches : on enregistre cette étape de recherche ---
    let parent_id = query.parent.clone();
    let node_id = match workspace::record_search_node(
        pool.get_ref(),
        project_id.as_deref(),
        parent_id.as_deref(),
        &session_id,
        &trimmed_query,
        category.as_str(),
    )
    .await
    {
        Ok(id) => Some(id),
        Err(e) => {
            eprintln!("⚠️ Impossible d'enregistrer le nœud de recherche: {e}");
            None
        }
    };

    let ancestors = if let Some(id) = &node_id {
        workspace::get_ancestor_chain(pool.get_ref(), id)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let siblings = if let Some(id) = &node_id {
        workspace::get_sibling_branches(pool.get_ref(), parent_id.as_deref(), id, &session_id)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // La racine de la branche : tout affinage doit toujours repartir d'ici,
    // pour que "cr7 club" et "cr7 ballons d'or" soient deux branches sœurs
    // issues de "cr7", au lieu de s'empiler l'une sur l'autre.
    let root_node_id = ancestors
        .first()
        .map(|n| n.id.clone())
        .or_else(|| node_id.clone());
    let root_query = ancestors
        .first()
        .map(|n| n.query.clone())
        .unwrap_or_else(|| trimmed_query.clone());

    // --- Résultats locaux ---
    let mut local: Vec<SearchResult> = Vec::new();
    if matches!(category, SearchCategory::Web) {
        local = local_results(&tfidf, &trimmed_query, 5);
    }

    let cache_key = format!("{}:{}", category.as_str(), trimmed_query);

    let remote_cached = {
        let cache = CACHE.lock().unwrap();
        cache.get(&cache_key).cloned()
    };

    let remote_results = if let Some(cached) = remote_cached {
        cached
    } else {
        let engine = SearchEngine::new();
        match engine.search_fast(&trimmed_query, category).await {
            Ok(r) => {
                let mut cache = CACHE.lock().unwrap();
                cache.insert(cache_key, r.clone());
                r
            }
            Err(e) => {
                let mut ctx = Context::new();
                ctx.insert("results", &local);
                ctx.insert("query", &trimmed_query);
                ctx.insert("category", &category.as_str());
                ctx.insert(
                    "error",
                    &format!(
                        "Erreur SearXNG: {} (résultats locaux affichés ci-dessous)",
                        e
                    ),
                );
                ctx.insert("current_node_id", &node_id);
                ctx.insert("current_project", &project_id);
                ctx.insert("ancestors", &ancestors);
                ctx.insert("flash_message", &Option::<String>::None);
                ctx.insert("siblings", &siblings);
                ctx.insert("root_node_id", &root_node_id);
                ctx.insert("root_query", &root_query);

                let rendered = tera.render("index.html", &ctx).unwrap_or_else(|e| {
                    eprintln!("❌ Erreur de rendu Tera dans search_handler: {:#}", e);
                    format!("<pre>Erreur de rendu du template:\n{:#}</pre>", e)
                });
                return HttpResponse::Ok().body(rendered);
            }
        }
    };

    let mut merged = local;
    for r in remote_results {
        if !merged.iter().any(|m| m.url == r.url) {
            merged.push(r);
        }
    }

    let mut ctx = Context::new();
    ctx.insert("results", &merged);
    ctx.insert("query", &trimmed_query);
    ctx.insert("category", &category.as_str());
    ctx.insert("error", &"");
    ctx.insert("current_node_id", &node_id);
    ctx.insert("current_project", &project_id);
    ctx.insert("ancestors", &ancestors);
    ctx.insert("flash_message", &Option::<String>::None);
    ctx.insert("siblings", &siblings);
    ctx.insert("root_node_id", &root_node_id);
    ctx.insert("root_query", &root_query);

    let rendered = tera.render("index.html", &ctx).unwrap_or_else(|e| {
        eprintln!("❌ Erreur de rendu Tera dans search_handler: {:#}", e);
        format!("<pre>Erreur de rendu du template:\n{:#}</pre>", e)
    });
    HttpResponse::Ok().body(rendered)
}

async fn load_more(query: web::Query<SearchQuery>) -> impl Responder {
    let trimmed_query = query.q.trim().to_string();
    // Page 1 est déjà affichée par la recherche initiale (search_fast) : on démarre
    // donc à la page 2 par défaut, et on avance d'une page à chaque appel.
    let page = query.page.unwrap_or(2);

    let engine = SearchEngine::new();
    let results = match engine
        .search_specific_page(&trimmed_query, SearchCategory::Web, page)
        .await
    {
        Ok(r) => r,
        Err(_) => Vec::new(),
    };

    HttpResponse::Ok().json(results)
}

async fn search_web(
    query: web::Query<SearchQuery>,
    tera: web::Data<Tera>,
    tfidf: web::Data<TfIdfEngine>,
    pool: web::Data<SqlitePool>,
    session: Session,
) -> impl Responder {
    search_handler(query, tera, tfidf, pool, session, SearchCategory::Web).await
}

async fn search_images(
    query: web::Query<SearchQuery>,
    tera: web::Data<Tera>,
    tfidf: web::Data<TfIdfEngine>,
    pool: web::Data<SqlitePool>,
    session: Session,
) -> impl Responder {
    search_handler(query, tera, tfidf, pool, session, SearchCategory::Images).await
}

async fn search_videos(
    query: web::Query<SearchQuery>,
    tera: web::Data<Tera>,
    tfidf: web::Data<TfIdfEngine>,
    pool: web::Data<SqlitePool>,
    session: Session,
) -> impl Responder {
    search_handler(query, tera, tfidf, pool, session, SearchCategory::Videos).await
}

async fn search_news(
    query: web::Query<SearchQuery>,
    tera: web::Data<Tera>,
    tfidf: web::Data<TfIdfEngine>,
    pool: web::Data<SqlitePool>,
    session: Session,
) -> impl Responder {
    search_handler(query, tera, tfidf, pool, session, SearchCategory::News).await
}

async fn search_maps(
    query: web::Query<SearchQuery>,
    tera: web::Data<Tera>,
    tfidf: web::Data<TfIdfEngine>,
    pool: web::Data<SqlitePool>,
    session: Session,
) -> impl Responder {
    search_handler(query, tera, tfidf, pool, session, SearchCategory::Maps).await
}

// ============================================================
// API Projets / Groupes
// ============================================================

async fn api_create_project(
    payload: web::Json<CreateProjectPayload>,
    pool: web::Data<SqlitePool>,
    session: Session,
) -> impl Responder {
    let session_id = get_or_create_session_id(&session);
    match workspace::create_project(pool.get_ref(), &session_id, payload.name.trim()).await {
        Ok(result) => HttpResponse::Ok().json(result),
        Err(e) => {
            eprintln!("Erreur création projet: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": "Erreur serveur lors de la création du projet."
            }))
        }
    }
}

async fn api_list_projects(pool: web::Data<SqlitePool>, session: Session) -> impl Responder {
    let session_id = get_or_create_session_id(&session);
    match workspace::list_projects_for_session(pool.get_ref(), &session_id).await {
        Ok(projects) => HttpResponse::Ok().json(projects),
        Err(e) => {
            eprintln!("Erreur listing projets: {e}");
            HttpResponse::InternalServerError().json(Vec::<()>::new())
        }
    }
}

async fn api_join_project(
    path: web::Path<String>,
    pool: web::Data<SqlitePool>,
    session: Session,
) -> impl Responder {
    let session_id = get_or_create_session_id(&session);
    let project_id = path.into_inner();
    match workspace::join_project(pool.get_ref(), &project_id, &session_id).await {
        Ok(result) => HttpResponse::Ok().json(result),
        Err(e) => {
            eprintln!("Erreur pour rejoindre le projet: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": "Erreur serveur."
            }))
        }
    }
}

async fn api_delete_project(
    path: web::Path<String>,
    pool: web::Data<SqlitePool>,
    session: Session,
) -> impl Responder {
    let session_id = get_or_create_session_id(&session);
    let project_id = path.into_inner();
    match workspace::delete_project(pool.get_ref(), &project_id, &session_id).await {
        Ok(result) => HttpResponse::Ok().json(result),
        Err(e) => {
            eprintln!("Erreur suppression projet: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": "Erreur serveur lors de la suppression."
            }))
        }
    }
}
async fn api_limits() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "max_projects_per_day": MAX_PROJECTS_PER_DAY,
        "max_members_per_project": MAX_MEMBERS_PER_PROJECT,
    }))
}

async fn api_history(pool: web::Data<SqlitePool>, session: Session) -> impl Responder {
    let session_id = get_or_create_session_id(&session);
    match workspace::list_history_for_session(pool.get_ref(), &session_id).await {
        Ok(history) => HttpResponse::Ok().json(history),
        Err(e) => {
            eprintln!("Erreur listing historique: {e}");
            HttpResponse::InternalServerError().json(Vec::<()>::new())
        }
    }
}

async fn join_via_link(
    path: web::Path<String>,
    pool: web::Data<SqlitePool>,
    session: Session,
) -> impl Responder {
    let session_id = get_or_create_session_id(&session);
    let project_id = path.into_inner();

    let result = workspace::join_project(pool.get_ref(), &project_id, &session_id).await;

    let (message, project_id_for_redirect) = match result {
        Ok(r) => (r.message, Some(project_id.clone())),
        Err(e) => {
            eprintln!("Erreur pour rejoindre le projet via lien: {e}");
            (
                "Erreur serveur lors de la tentative de rejoindre le projet.".to_string(),
                None,
            )
        }
    };

    // On redirige vers l'accueil avec le projet actif et un message à afficher
    let redirect_url = match project_id_for_redirect {
        Some(id) => format!(
            "/?project={}&join_message={}",
            id,
            urlencoding::encode(&message)
        ),
        None => format!("/?join_message={}", urlencoding::encode(&message)),
    };

    HttpResponse::Found()
        .append_header(("Location", redirect_url))
        .finish()
}

#[derive(Deserialize)]
struct IaChatRequest {
    message: String,
    category: Option<String>,
    conversation_id: Option<i64>,
}

#[derive(Deserialize)]
struct IaCreateConversationRequest {
    title: Option<String>,
    category: Option<String>,
}

async fn api_ia_chat(
    pool: web::Data<SqlitePool>,
    session: Session,
    form: web::Json<IaChatRequest>,
) -> impl Responder {
    let user_id = match session.get::<i64>("user_id") {
        Ok(Some(user_id)) => user_id,
        _ => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "ok": false,
                "error": "Vous devez être connecté."
            }));
        }
    };

    let message = form.message.trim();

    if message.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "ok": false,
            "error": "Le message est obligatoire."
        }));
    }

    let category = form.category.as_deref().unwrap_or("chat").trim();

    let category = if category.is_empty() {
        "chat"
    } else {
        category
    };

    let conversation_id = if let Some(id) = form.conversation_id {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT id
             FROM ia_conversations
             WHERE id = ? AND user_id = ?",
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool.get_ref())
        .await;

        match exists {
            Ok(Some(id)) => id,
            Ok(None) => {
                return HttpResponse::NotFound().json(serde_json::json!({
                    "ok": false,
                    "error": "Conversation introuvable."
                }));
            }
            Err(e) => {
                eprintln!("Erreur vérification conversation IA : {e}");
                return HttpResponse::InternalServerError().json(serde_json::json!({
                    "ok": false,
                    "error": "Erreur lors de la vérification de la conversation."
                }));
            }
        }
    } else {
        match sqlx::query(
            "INSERT INTO ia_conversations (user_id, title, category)
             VALUES (?, 'Nouveau chat', ?)",
        )
        .bind(user_id)
        .bind(category)
        .execute(pool.get_ref())
        .await
        {
            Ok(result) => result.last_insert_rowid(),
            Err(e) => {
                eprintln!("Erreur création conversation IA : {e}");
                return HttpResponse::InternalServerError().json(serde_json::json!({
                    "ok": false,
                    "error": "Impossible de créer la conversation."
                }));
            }
        }
    };

    if let Err(e) = sqlx::query(
        "UPDATE ia_conversations
         SET category = ?, updated_at = CURRENT_TIMESTAMP
         WHERE id = ? AND user_id = ?",
    )
    .bind(category)
    .bind(conversation_id)
    .bind(user_id)
    .execute(pool.get_ref())
    .await
    {
        eprintln!("Erreur mise à jour conversation IA : {e}");
    }

    if let Err(e) = sqlx::query(
        "INSERT INTO ia_messages (conversation_id, role, content)
         VALUES (?, 'user', ?)",
    )
    .bind(conversation_id)
    .bind(message)
    .execute(pool.get_ref())
    .await
    {
        eprintln!("Erreur enregistrement message IA : {e}");

        return HttpResponse::InternalServerError().json(serde_json::json!({
            "ok": false,
            "error": "Impossible d'enregistrer le message."
        }));
    }

    let history_rows = match sqlx::query_as::<_, (String, String)>(
        "SELECT role, content
         FROM (
             SELECT role, content, id
             FROM ia_messages
             WHERE conversation_id = ?
             ORDER BY id DESC
             LIMIT 20
         )
         ORDER BY id ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool.get_ref())
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("Erreur récupération historique IA : {e}");

            return HttpResponse::InternalServerError().json(serde_json::json!({
                "ok": false,
                "error": "Impossible de récupérer l'historique."
            }));
        }
    };

    let history: Vec<serde_json::Value> = history_rows
        .into_iter()
        .map(|(role, content)| {
            serde_json::json!({
                "role": role,
                "content": content
            })
        })
        .collect();

    let client = reqwest::Client::new();

    let response = match client
        .post("http://127.0.0.1:3020/chat")
        .json(&serde_json::json!({
            "message": message,
            "category": category,
            "history": history
        }))
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            eprintln!("Erreur connexion ia_nova : {e}");

            return HttpResponse::ServiceUnavailable().json(serde_json::json!({
                "ok": false,
                "error": "L'assistant IA est indisponible."
            }));
        }
    };

    let status = response.status();

    let mut data = match response.json::<serde_json::Value>().await {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Erreur lecture réponse ia_nova : {e}");

            return HttpResponse::BadGateway().json(serde_json::json!({
                "ok": false,
                "error": "Réponse invalide de l'assistant IA."
            }));
        }
    };

    if !status.is_success() {
        return HttpResponse::BadGateway().json(data);
    }

    let assistant_response = match data.get("response").and_then(|value| value.as_str()) {
        Some(response) => response.to_string(),
        None => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "ok": false,
                "error": "L'assistant IA n'a pas fourni de réponse."
            }));
        }
    };

    if let Err(e) = sqlx::query(
        "INSERT INTO ia_messages (conversation_id, role, content)
         VALUES (?, 'assistant', ?)",
    )
    .bind(conversation_id)
    .bind(&assistant_response)
    .execute(pool.get_ref())
    .await
    {
        eprintln!("Erreur enregistrement réponse IA : {e}");

        return HttpResponse::InternalServerError().json(serde_json::json!({
            "ok": false,
            "error": "Impossible d'enregistrer la réponse de l'IA."
        }));
    }

    let title: String = message.chars().take(60).collect();

    if let Err(e) = sqlx::query(
        "UPDATE ia_conversations
         SET title = CASE
                 WHEN title = 'Nouveau chat' THEN ?
                 ELSE title
             END,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = ? AND user_id = ?",
    )
    .bind(&title)
    .bind(conversation_id)
    .bind(user_id)
    .execute(pool.get_ref())
    .await
    {
        eprintln!("Erreur mise à jour titre conversation IA : {e}");
    }

    if let Some(object) = data.as_object_mut() {
        object.insert(
            "conversation_id".to_string(),
            serde_json::json!(conversation_id),
        );
    }

    HttpResponse::Ok().json(data)
}

async fn api_ia_history(pool: web::Data<SqlitePool>, session: Session) -> impl Responder {
    let user_id = match session.get::<i64>("user_id") {
        Ok(Some(user_id)) => user_id,
        _ => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "ok": false,
                "error": "Vous devez être connecté."
            }));
        }
    };

    let conversations = match sqlx::query_as::<_, (i64, String, String, String, String, i64)>(
        "SELECT id, title, category, created_at, updated_at, pinned
         FROM ia_conversations
         WHERE user_id = ?
         ORDER BY pinned DESC, updated_at DESC, id DESC",
    )
    .bind(user_id)
    .fetch_all(pool.get_ref())
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("Erreur listing conversations IA : {e}");

            return HttpResponse::InternalServerError().json(serde_json::json!({
                "ok": false,
                "error": "Impossible de récupérer les conversations."
            }));
        }
    };

    let result: Vec<serde_json::Value> = conversations
        .into_iter()
        .map(|(id, title, category, created_at, updated_at, pinned)| {
            serde_json::json!({
                "id": id,
                "title": title,
                "category": category,
                "created_at": created_at,
                "updated_at": updated_at,
                "pinned": pinned == 1
            })
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "conversations": result
    }))
}

async fn api_ia_create_conversation(
    pool: web::Data<SqlitePool>,
    session: Session,
    form: web::Json<IaCreateConversationRequest>,
) -> impl Responder {
    let user_id = match session.get::<i64>("user_id") {
        Ok(Some(user_id)) => user_id,
        _ => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "ok": false,
                "error": "Vous devez être connecté."
            }));
        }
    };

    let category = form.category.as_deref().unwrap_or("chat").trim();

    let category = if category.is_empty() {
        "chat"
    } else {
        category
    };

    let title = form.title.as_deref().unwrap_or("Nouveau chat").trim();

    let title = if title.is_empty() {
        "Nouveau chat"
    } else {
        title
    };

    let result = match sqlx::query(
        "INSERT INTO ia_conversations (user_id, title, category)
         VALUES (?, ?, ?)",
    )
    .bind(user_id)
    .bind(title)
    .bind(category)
    .execute(pool.get_ref())
    .await
    {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Erreur création conversation IA : {e}");

            return HttpResponse::InternalServerError().json(serde_json::json!({
                "ok": false,
                "error": "Impossible de créer la conversation."
            }));
        }
    };

    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "conversation": {
            "id": result.last_insert_rowid(),
            "title": title,
            "category": category
        }
    }))
}

async fn api_ia_toggle_pin(
    pool: web::Data<SqlitePool>,
    session: Session,
    path: web::Path<i64>,
) -> impl Responder {
    let user_id = match session.get::<i64>("user_id") {
        Ok(Some(user_id)) => user_id,
        _ => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "ok": false,
                "error": "Vous devez être connecté."
            }));
        }
    };

    let conversation_id = path.into_inner();

    let conversation = match sqlx::query_as::<_, (i64,)>(
        "SELECT pinned
         FROM ia_conversations
         WHERE id = ? AND user_id = ?",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_optional(pool.get_ref())
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "ok": false,
                "error": "Conversation introuvable."
            }));
        }
        Err(e) => {
            eprintln!("Erreur récupération épinglage conversation IA : {e}");

            return HttpResponse::InternalServerError().json(serde_json::json!({
                "ok": false,
                "error": "Impossible de récupérer la conversation."
            }));
        }
    };

    let new_pinned = if conversation.0 == 1 { 0 } else { 1 };

    if let Err(e) = sqlx::query(
        "UPDATE ia_conversations
         SET pinned = ?
         WHERE id = ? AND user_id = ?",
    )
    .bind(new_pinned)
    .bind(conversation_id)
    .bind(user_id)
    .execute(pool.get_ref())
    .await
    {
        eprintln!("Erreur modification épinglage conversation IA : {e}");

        return HttpResponse::InternalServerError().json(serde_json::json!({
            "ok": false,
            "error": "Impossible de modifier l'épinglage."
        }));
    }

    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "conversation_id": conversation_id,
        "pinned": new_pinned == 1
    }))
}

async fn api_ia_get_conversation(
    pool: web::Data<SqlitePool>,
    session: Session,
    path: web::Path<i64>,
) -> impl Responder {
    let user_id = match session.get::<i64>("user_id") {
        Ok(Some(user_id)) => user_id,
        _ => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "ok": false,
                "error": "Vous devez être connecté."
            }));
        }
    };

    let conversation_id = path.into_inner();

    let conversation = match sqlx::query_as::<_, (i64, String, String, String, String)>(
        "SELECT id, title, category, created_at, updated_at
         FROM ia_conversations
         WHERE id = ? AND user_id = ?",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_optional(pool.get_ref())
    .await
    {
        Ok(Some(conversation)) => conversation,
        Ok(None) => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "ok": false,
                "error": "Conversation introuvable."
            }));
        }
        Err(e) => {
            eprintln!("Erreur récupération conversation IA : {e}");

            return HttpResponse::InternalServerError().json(serde_json::json!({
                "ok": false,
                "error": "Impossible de récupérer la conversation."
            }));
        }
    };

    let messages = match sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT id, role, content, created_at
         FROM ia_messages
         WHERE conversation_id = ?
         ORDER BY id ASC",
    )
    .bind(conversation_id)
    .fetch_all(pool.get_ref())
    .await
    {
        Ok(messages) => messages,
        Err(e) => {
            eprintln!("Erreur récupération messages IA : {e}");

            return HttpResponse::InternalServerError().json(serde_json::json!({
                "ok": false,
                "error": "Impossible de récupérer les messages."
            }));
        }
    };

    let messages: Vec<serde_json::Value> = messages
        .into_iter()
        .map(|(id, role, content, created_at)| {
            serde_json::json!({
                "id": id,
                "role": role,
                "content": content,
                "created_at": created_at
            })
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "conversation": {
            "id": conversation.0,
            "title": conversation.1,
            "category": conversation.2,
            "created_at": conversation.3,
            "updated_at": conversation.4,
            "messages": messages
        }
    }))
}

async fn api_ia_delete_conversation(
    pool: web::Data<SqlitePool>,
    session: Session,
    path: web::Path<i64>,
) -> impl Responder {
    let user_id = match session.get::<i64>("user_id") {
        Ok(Some(user_id)) => user_id,
        _ => {
            return HttpResponse::Unauthorized().json(serde_json::json!({
                "ok": false,
                "error": "Vous devez être connecté."
            }));
        }
    };

    let conversation_id = path.into_inner();

    let exists = match sqlx::query_scalar::<_, i64>(
        "SELECT id
         FROM ia_conversations
         WHERE id = ? AND user_id = ?",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_optional(pool.get_ref())
    .await
    {
        Ok(Some(id)) => id,
        Ok(None) => {
            return HttpResponse::NotFound().json(serde_json::json!({
                "ok": false,
                "error": "Conversation introuvable."
            }));
        }
        Err(e) => {
            eprintln!("Erreur vérification suppression conversation IA : {e}");

            return HttpResponse::InternalServerError().json(serde_json::json!({
                "ok": false,
                "error": "Erreur lors de la vérification de la conversation."
            }));
        }
    };

    if let Err(e) = sqlx::query(
        "DELETE FROM ia_messages
         WHERE conversation_id = ?",
    )
    .bind(exists)
    .execute(pool.get_ref())
    .await
    {
        eprintln!("Erreur suppression messages IA : {e}");

        return HttpResponse::InternalServerError().json(serde_json::json!({
            "ok": false,
            "error": "Impossible de supprimer les messages."
        }));
    }

    if let Err(e) = sqlx::query(
        "DELETE FROM ia_conversations
         WHERE id = ? AND user_id = ?",
    )
    .bind(conversation_id)
    .bind(user_id)
    .execute(pool.get_ref())
    .await
    {
        eprintln!("Erreur suppression conversation IA : {e}");

        return HttpResponse::InternalServerError().json(serde_json::json!({
            "ok": false,
            "error": "Impossible de supprimer la conversation."
        }));
    }

    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "message": "Conversation supprimée."
    }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    start_ia_nova();

    let tera = match Tera::new("templates/**/*.html") {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Erreur Tera: {}", e);
            ::std::process::exit(1);
        }
    };

    let db_config = DbConfig::from_env();
    let pool = SqlitePool::connect(db_config.connection_string())
        .await
        .expect("❌ Impossible de se connecter à articles.db (as-tu lancé `sqlite3 articles.db < schema.sql` ?)");

    // Les nouvelles tables (projets, groupes, arbre de recherche) sont créées si absentes
    if let Err(e) = sqlx::query(
        "CREATE TABLE IF NOT EXISTS search_nodes (
            id TEXT PRIMARY KEY, project_id TEXT, parent_id TEXT, session_id TEXT NOT NULL,
            query TEXT NOT NULL, category TEXT NOT NULL, created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    {
        eprintln!("⚠️ Erreur création table search_nodes: {e}");
    }
    if let Err(e) = sqlx::query(
        "CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, owner_session TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    {
        eprintln!("⚠️ Erreur création table projects: {e}");
    }
    if let Err(e) = sqlx::query(
        "CREATE TABLE IF NOT EXISTS project_members (
            project_id TEXT NOT NULL, session_id TEXT NOT NULL,
            joined_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (project_id, session_id)
        )",
    )
    .execute(&pool)
    .await
    {
        eprintln!("⚠️ Erreur création table project_members: {e}");
    }

    if let Err(e) = sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            email TEXT NOT NULL UNIQUE,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&pool)
    .await
    {
        eprintln!("⚠️ Erreur création table users: {e}");
    }

    if let Err(e) = sqlx::query(
        "CREATE TABLE IF NOT EXISTS ia_conversations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        user_id INTEGER NOT NULL,
        title TEXT NOT NULL DEFAULT 'Nouveau chat',
        category TEXT NOT NULL DEFAULT 'chat',
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        FOREIGN KEY (user_id) REFERENCES users(id)
    )",
    )
    .execute(&pool)
    .await
    {
        eprintln!("⚠️ Erreur création table ia_conversations: {e}");
    }

    if let Err(e) = sqlx::query(
        "CREATE TABLE IF NOT EXISTS ia_messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        conversation_id INTEGER NOT NULL,
        role TEXT NOT NULL,
        content TEXT NOT NULL,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        FOREIGN KEY (conversation_id) REFERENCES ia_conversations(id) ON DELETE CASCADE
    )",
    )
    .execute(&pool)
    .await
    {
        eprintln!("⚠️ Erreur création table ia_messages: {e}");
    }

    if let Err(e) = sqlx::query(
        "CREATE TABLE IF NOT EXISTS login_verification_codes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            code TEXT NOT NULL,
            expires_at DATETIME NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id)
        )",
    )
    .execute(&pool)
    .await
    {
        eprintln!("⚠️ Erreur création table login_verification_codes: {e}");
    }

    if let Err(e) = sqlx::query(
        "CREATE TABLE IF NOT EXISTS password_reset_tokens (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            token TEXT NOT NULL UNIQUE,
            expires_at DATETIME NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (user_id) REFERENCES users(id)
        )",
    )
    .execute(&pool)
    .await
    {
        eprintln!("⚠️ Erreur création table password_reset_tokens: {e}");
    }

    if let Err(e) = sqlx::query(
        "CREATE TABLE IF NOT EXISTS oauth_accounts (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        user_id INTEGER NOT NULL,
        provider TEXT NOT NULL,
        provider_user_id TEXT NOT NULL,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        UNIQUE(provider, provider_user_id),
        FOREIGN KEY (user_id) REFERENCES users(id)
    )",
    )
    .execute(&pool)
    .await
    {
        eprintln!("⚠️ Erreur création table oauth_accounts: {e}");
    }

    forum::init(&pool).await;

    let tfidf_engine = load_tfidf_engine(&pool).await;
    let tfidf_data = web::Data::new(tfidf_engine);
    let pool_data = web::Data::new(pool);

    // Clé de session : générée aléatoirement au démarrage (les sessions ne survivent
    // pas à un redémarrage du serveur ; pour la prod, fixe une clé stable via variable d'env).
    let session_key = Key::generate();

    let cert_file = File::open("certs/nova.pem").expect("Impossible d'ouvrir certs/nova.pem");

    let key_file =
        File::open("certs/nova-key.pem").expect("Impossible d'ouvrir certs/nova-key.pem");

    let mut cert_reader = BufReader::new(cert_file);
    let mut key_reader = BufReader::new(key_file);

    let cert_chain = certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .expect("Impossible de lire le certificat");

    let private_key = private_key(&mut key_reader)
        .expect("Impossible de lire la clé privée")
        .expect("Aucune clé privée trouvée");

    aws_lc_rs::default_provider()
        .install_default()
        .expect("Impossible d'installer le provider crypto Rustls");

    let tls_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .expect("Impossible de créer la configuration TLS");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(tera.clone()))
            .app_data(tfidf_data.clone())
            .app_data(pool_data.clone())
            .service(Files::new("/static", "static"))
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), session_key.clone())
                    .cookie_secure(false) // mets `true` derrière HTTPS en production
                    .session_lifecycle(
                        PersistentSession::default().session_ttl(CookieDuration::days(30)),
                    )
                    .build(),
            )
            .route("/login", web::get().to(auth::login_page))
            .route("/login", web::post().to(auth::login))
            .route("/register", web::get().to(auth::register_page))
            .route("/register", web::post().to(auth::register))
            .route("/verify-code", web::get().to(auth::verify_code_page))
            .route("/verify-code", web::post().to(auth::verify_code))
            .route("/resend-code", web::post().to(auth::resend_code))
            .route("/logout", web::get().to(auth::logout))
            .route(
                "/forgot-password",
                web::get().to(auth::forgot_password_page),
            )
            .route("/forgot-password", web::post().to(auth::forgot_password))
            .route(
                "/reset-password/{token}",
                web::get().to(auth::reset_password_page),
            )
            .route(
                "/reset-password/{token}",
                web::post().to(auth::reset_password),
            )
            .route("/", web::get().to(index))
            .route("/reseaux", web::get().to(forum::index))
            .route("/reseaux/create", web::get().to(forum::create_topic_page))
            .route("/reseaux/create", web::post().to(forum::create_topic))
            .route("/reseaux/{topic_id}", web::get().to(forum::topic))
            .route(
                "/reseaux/{topic_id}/comment",
                web::post().to(forum::add_comment),
            )
            .route(
                "/reseaux/post/{post_id}/like",
                web::post().to(forum::like_post),
            )
            .route("/join/{id}", web::get().to(join_via_link))
            .route("/search", web::get().to(search_web))
            .route("/search/images", web::get().to(search_images))
            .route("/search/videos", web::get().to(search_videos))
            .route("/search/news", web::get().to(search_news))
            .route("/search/maps", web::get().to(search_maps))
            .route("/api/load-more", web::get().to(load_more))
            .route("/api/projects", web::post().to(api_create_project))
            .route("/api/projects", web::get().to(api_list_projects))
            .route("/api/projects/{id}/join", web::post().to(api_join_project))
            .route("/api/projects/{id}", web::delete().to(api_delete_project))
            .route("/api/limits", web::get().to(api_limits))
            .route("/api/history", web::get().to(api_history))
            .route("/api/ia/chat", web::post().to(api_ia_chat))
            .route("/api/ia/history", web::get().to(api_ia_history))
            .route(
                "/api/ia/history",
                web::post().to(api_ia_create_conversation),
            )
            .route(
                "/api/ia/history/{conversation_id}",
                web::get().to(api_ia_get_conversation),
            )
            .route(
                "/api/ia/history/{conversation_id}/pin",
                web::post().to(api_ia_toggle_pin),
            )
            .route(
                "/api/ia/history/{conversation_id}",
                web::delete().to(api_ia_delete_conversation),
            )
    })
    .bind_rustls_0_23("127.0.0.1:8080", tls_config)?
    .run()
    .await
}

/// Lance automatiquement l'API Python ia_nova sur le port 3020.
/// Si l'API est déjà lancée, on ne démarre pas une deuxième instance.
fn start_ia_nova() {
    std::thread::spawn(|| {
        let Ok(home) = std::env::var("HOME") else {
            eprintln!("⚠️ Variable HOME introuvable, ia_nova non démarré.");
            return;
        };

        let ia_nova_dir = format!("{home}/nova/ia_nova");
        let api_file = std::path::Path::new(&ia_nova_dir).join("api.py");

        if !api_file.exists() {
            eprintln!(
                "⚠️ {} introuvable — ia_nova ne sera pas démarré.",
                api_file.display()
            );
            return;
        }

        // Vérifie si ia_nova est déjà lancé sur le port 3020.
        if std::net::TcpStream::connect("127.0.0.1:3020").is_ok() {
            println!("✅ ia_nova est déjà lancé sur http://127.0.0.1:3020");
            return;
        }

        println!("🤖 Démarrage automatique de ia_nova...");

        match std::process::Command::new("python3")
            .arg("api.py")
            .current_dir(&ia_nova_dir)
            .spawn()
        {
            Ok(_) => {
                println!("✅ ia_nova démarré sur http://127.0.0.1:3020");
            }

            Err(e) => {
                eprintln!("⚠️ Impossible de démarrer ia_nova : {e}");
                eprintln!("Lancement manuel : cd {ia_nova_dir} && python3 api.py");
            }
        }
    });
}
