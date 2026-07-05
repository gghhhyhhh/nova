use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use nova::search_engine::{SearchCategory, SearchResult};
use nova::{DbConfig, SearchEngine, TfIdfEngine};
use serde::Deserialize;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use std::sync::Mutex;
use tera::{Context, Tera};

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

// Ligne brute lue depuis la table `articles` (voir schema.sql)
#[derive(Debug, sqlx::FromRow)]
struct ArticleRow {
    id: i64,
    title: String,
    description: Option<String>,
    url: String,
    category: Option<String>,
}

// Cache simple en mémoire (résultats web distants)
lazy_static::lazy_static! {
    static ref CACHE: Mutex<HashMap<String, Vec<SearchResult>>> =
        Mutex::new(HashMap::new());
}

/// Charge tous les articles de la base sqlite locale et construit le moteur TF-IDF.
/// Appelé une seule fois au démarrage du serveur.
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

/// Convertit les meilleurs résultats locaux (base sqlite) au format SearchResult
/// utilisé par le template, pour pouvoir les fusionner avec les résultats SearXNG.
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

async fn index(tera: web::Data<Tera>) -> impl Responder {
    let mut ctx = Context::new();
    ctx.insert("results", &Vec::<SearchResult>::new());
    ctx.insert("query", &"");
    ctx.insert("category", &"web");
    ctx.insert("error", &"");

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
    category: SearchCategory,
) -> impl Responder {
    let trimmed_query = query.q.trim().to_string();

    if trimmed_query.is_empty() {
        let mut ctx = Context::new();
        ctx.insert("results", &Vec::<SearchResult>::new());
        ctx.insert("query", &trimmed_query);
        ctx.insert("category", &category.as_str());
        ctx.insert("error", &"Veuillez entrer un terme de recherche");

        let rendered = tera.render("index.html", &ctx).unwrap_or_default();
        return HttpResponse::Ok().body(rendered);
    }

    // Résultats locaux (uniquement pertinents pour l'onglet "web")
    let mut local: Vec<SearchResult> = Vec::new();
    if matches!(category, SearchCategory::Web) {
        local = local_results(&tfidf, &trimmed_query, 5);
    }

    // Clé de cache (sur la partie distante uniquement)
    let cache_key = format!("{}:{}", category.as_str(), trimmed_query);

    // Vérifier le cache des résultats distants
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
                ctx.insert("error", &format!("Erreur SearXNG: {} (résultats locaux affichés ci-dessous)", e));

                let rendered = tera.render("index.html", &ctx).unwrap_or_default();
                return HttpResponse::Ok().body(rendered);
            }
        }
    };

    // Fusion : résultats locaux en premier, puis résultats distants (dédupliqués par URL)
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

    let rendered = tera.render("index.html", &ctx).unwrap_or_default();
    HttpResponse::Ok().body(rendered)
}

// API pour charger plus de résultats (AJAX) — reste purement distant
async fn load_more(query: web::Query<SearchQuery>) -> impl Responder {
    let trimmed_query = query.q.trim().to_string();

    let engine = SearchEngine::new();
    let results = match engine
        .search_full(&trimmed_query, SearchCategory::Web)
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
) -> impl Responder {
    search_handler(query, tera, tfidf, SearchCategory::Web).await
}

async fn search_images(
    query: web::Query<SearchQuery>,
    tera: web::Data<Tera>,
    tfidf: web::Data<TfIdfEngine>,
) -> impl Responder {
    search_handler(query, tera, tfidf, SearchCategory::Images).await
}

async fn search_videos(
    query: web::Query<SearchQuery>,
    tera: web::Data<Tera>,
    tfidf: web::Data<TfIdfEngine>,
) -> impl Responder {
    search_handler(query, tera, tfidf, SearchCategory::Videos).await
}

async fn search_news(
    query: web::Query<SearchQuery>,
    tera: web::Data<Tera>,
    tfidf: web::Data<TfIdfEngine>,
) -> impl Responder {
    search_handler(query, tera, tfidf, SearchCategory::News).await
}

async fn search_maps(
    query: web::Query<SearchQuery>,
    tera: web::Data<Tera>,
    tfidf: web::Data<TfIdfEngine>,
) -> impl Responder {
    search_handler(query, tera, tfidf, SearchCategory::Maps).await
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let tera = match Tera::new("templates/**/*.html") {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Erreur Tera: {}", e);
            ::std::process::exit(1);
        }
    };

    // Connexion à la base sqlite locale + chargement du moteur TF-IDF
    let db_config = DbConfig::from_env();
    let pool = SqlitePool::connect(db_config.connection_string())
        .await
        .expect("❌ Impossible de se connecter à articles.db (as-tu lancé `sqlite3 articles.db < schema.sql` ?)");
    let tfidf_engine = load_tfidf_engine(&pool).await;
    let tfidf_data = web::Data::new(tfidf_engine);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(tera.clone()))
            .app_data(tfidf_data.clone())
            .route("/", web::get().to(index))
            .route("/search", web::get().to(search_web))
            .route("/search/images", web::get().to(search_images))
            .route("/search/videos", web::get().to(search_videos))
            .route("/search/news", web::get().to(search_news))
            .route("/search/maps", web::get().to(search_maps))
            .route("/api/load-more", web::get().to(load_more))
    })
    .bind("127.0.0.1:8090")?
    .run()
    .await
}
