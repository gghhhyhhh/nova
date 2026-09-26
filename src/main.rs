use actix_files::Files;
use actix_session::storage::CookieSessionStore;
use actix_session::{config::PersistentSession, Session, SessionMiddleware};
use actix_web::cookie::{time::Duration as CookieDuration, Key};
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use nova::search_engine::{SearchCategory, SearchResult};
use nova::workspace::{self, CreateProjectPayload, MAX_MEMBERS_PER_PROJECT, MAX_PROJECTS_PER_DAY};
use nova::{DbConfig, SearchEngine, TfIdfEngine};
use serde::Deserialize;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use std::sync::Mutex;
use tera::{Context, Tera};
use uuid::Uuid;

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

/// Lance `docker-compose up -d` dans ~/ia-locale, en arrière-plan (ne bloque pas
/// le démarrage du serveur si Docker est lent ou absent).
fn start_ia_locale_background() {
    std::thread::spawn(|| {
        let Ok(home) = std::env::var("HOME") else {
            eprintln!("⚠️  Variable HOME introuvable, IA locale non démarrée automatiquement.");
            return;
        };
        let ia_dir = format!("{home}/ia-locale");

        if !std::path::Path::new(&ia_dir)
            .join("docker-compose.yml")
            .exists()
        {
            eprintln!("ℹ️  ~/ia-locale non trouvé (ou incomplet) — assistant IA non démarré automatiquement.");
            return;
        }

        println!("🤖 Démarrage de l'assistant IA locale (docker-compose up -d)...");

        // On essaie d'abord `docker-compose` (ancien binaire), puis `docker compose` (plugin récent)
        let result = std::process::Command::new("docker-compose")
            .args(["up", "-d"])
            .current_dir(&ia_dir)
            .status();

        let success = match result {
            Ok(status) if status.success() => true,
            _ => std::process::Command::new("docker")
                .args(["compose", "up", "-d"])
                .current_dir(&ia_dir)
                .status()
                .map(|s| s.success())
                .unwrap_or(false),
        };

        if success {
            let _ = std::process::Command::new("bash")
                .arg(format!("{home}/nova/ia_nova/set-default-model.sh"))
                .status();
            println!("✅ Assistant IA locale démarré (http://localhost:3010).");
        } else {
            eprintln!("⚠️  Impossible de démarrer l'IA locale automatiquement (Docker absent ou erreur). Le bouton 🤖 IA affichera un message d'indisponibilité.");
        }
    });
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    start_ia_locale_background();

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

    forum::init(&pool).await;

    let tfidf_engine = load_tfidf_engine(&pool).await;
    let tfidf_data = web::Data::new(tfidf_engine);
    let pool_data = web::Data::new(pool);

    // Clé de session : générée aléatoirement au démarrage (les sessions ne survivent
    // pas à un redémarrage du serveur ; pour la prod, fixe une clé stable via variable d'env).
    let session_key = Key::generate();

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
            .route("/", web::get().to(index))
            .route("/reseaux", web::get().to(forum::index))
            .route("/reseaux/create", web::get().to(forum::create_topic_page))
            .route("/reseaux/create", web::post().to(forum::create_topic))
            .route("/reseaux/{topic_id}", web::get().to(forum::topic))
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
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

/// Lance l'assistant Python `ia_nova` dans une nouvelle fenêtre de terminal
/// (il est interactif : il faut un vrai terminal pour taper les messages,
/// contrairement à Docker qui tourne silencieusement en fond).
fn start_ia_nova_terminal() {
    std::thread::spawn(|| {
        let Ok(home) = std::env::var("HOME") else {
            eprintln!("⚠️  Variable HOME introuvable, ia_nova non démarré automatiquement.");
            return;
        };
        let ia_nova_dir = format!("{home}/nova/ia_nova");

        if !std::path::Path::new(&ia_nova_dir).exists() {
            eprintln!(
                "ℹ️  ~/nova/ia_nova introuvable — assistant ia_nova non démarré automatiquement."
            );
            return;
        }

        // Adapte "ia_complete.py" ci-dessous si tu préfères lancer "main.py"
        let script = "ia_complete.py";

        println!("🧠 Démarrage de ia_nova dans une nouvelle fenêtre de terminal...");

        // On essaie plusieurs émulateurs de terminal courants, dans l'ordre,
        // jusqu'à ce que l'un d'eux fonctionne.
        let attempts: Vec<(&str, Vec<&str>)> = vec![
            ("gnome-terminal", vec!["--", "python3", script]),
            ("konsole", vec!["-e", "python3", script]),
            ("xfce4-terminal", vec!["-e", "python3", script]),
            ("xterm", vec!["-e", "python3", script]),
        ];

        let mut launched = false;
        for (terminal, args) in attempts {
            let result = std::process::Command::new(terminal)
                .args(&args)
                .current_dir(&ia_nova_dir)
                .spawn();

            if result.is_ok() {
                println!("✅ ia_nova démarré via {terminal}.");
                launched = true;
                break;
            }
        }

        if !launched {
            eprintln!(
                "⚠️  Impossible de trouver un terminal graphique pour lancer ia_nova automatiquement.\n\
                 Lance-le toi-même avec : cd ~/nova/ia_nova && python3 {script}"
            );
        }
    });
}
