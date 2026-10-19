use actix_files::Files;
use actix_session::storage::CookieSessionStore;
use actix_session::{config::PersistentSession, SessionMiddleware};
use actix_web::cookie::{time::Duration as CookieDuration, Key};
use actix_web::{web, App, HttpServer};
use nova::DbConfig;
use sqlx::sqlite::SqlitePool;
use tera::Tera;

mod auth;
mod engine_loader;
mod forum;
mod ia_launcher;
mod routes;
mod search_suggestions;
mod tls;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    ia_launcher::start_ia_nova();

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

    if let Err(e) = sqlx::migrate!("./migrations").run(&pool).await {
        eprintln!("❌ Erreur lors des migrations SQLx : {e}");
        ::std::process::exit(1);
    }

    forum::init(&pool).await;

    let tfidf_engine = engine_loader::load_tfidf_engine(&pool).await;
    let tfidf_data = web::Data::new(tfidf_engine);
    let pool_data = web::Data::new(pool);

    // Clé de session : générée aléatoirement au démarrage (les sessions ne survivent
    // pas à un redémarrage du serveur ; pour la prod, fixe une clé stable via variable d'env).
    let session_key = Key::generate();

    let tls_config = tls::load_tls_config();

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
            .configure(routes::auth::configure)
            .configure(routes::home::configure)
            .configure(routes::forum::configure)
            .configure(routes::search::configure)
            .configure(routes::projects::configure)
            .configure(routes::ia::configure)
    })
    .bind_rustls_0_23("127.0.0.1:8090", tls_config)?
    .run()
    .await
}
