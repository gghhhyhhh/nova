use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use nova::search_engine::SearchResult;
use serde::Deserialize;
use tera::{Context, Tera};

use crate::routes;

#[derive(Deserialize)]
pub struct IndexQuery {
    project: Option<String>,
    join_message: Option<String>,
}

/// Affiche la page d'accueil de NOVA.
pub async fn index(
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

    let _ = routes::get_or_create_session_id(&session);

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

/// Enregistre la route de la page d'accueil.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/", web::get().to(index));
}
