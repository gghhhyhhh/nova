use super::get_or_create_session_id;
use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use nova::search_engine::{SearchCategory, SearchResult};
use nova::workspace;
use nova::{SearchEngine, TfIdfEngine};
use serde::Deserialize;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use std::sync::Mutex;
use tera::{Context, Tera};

lazy_static::lazy_static! {
    static ref CACHE: Mutex<HashMap<String, Vec<SearchResult>>> =
        Mutex::new(HashMap::new());
}

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
        match engine.search_full(&trimmed_query, category).await {
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

#[derive(Deserialize)]
struct SuggestionsQuery {
    q: String,
}

async fn search_suggestions_api(query: web::Query<SuggestionsQuery>) -> impl Responder {
    let local = crate::search_suggestions::generate_suggestions(&query.q);
    let google = crate::search_suggestions::fetch_google_suggestions(&query.q).await;

    let mut suggestions = Vec::new();

    for suggestion in google.into_iter().chain(local.into_iter()) {
        if !suggestions
            .iter()
            .any(|existing: &crate::search_suggestions::SearchSuggestion| {
                existing.text.eq_ignore_ascii_case(&suggestion.text)
            })
        {
            suggestions.push(suggestion);
        }

        if suggestions.len() >= 8 {
            break;
        }
    }

    HttpResponse::Ok().json(suggestions)
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/search", web::get().to(search_web))
        .route("/search/images", web::get().to(search_images))
        .route("/search/videos", web::get().to(search_videos))
        .route("/search/news", web::get().to(search_news))
        .route("/search/maps", web::get().to(search_maps))
        .route("/api/load-more", web::get().to(load_more))
        .route("/api/suggestions", web::get().to(search_suggestions_api));
}
