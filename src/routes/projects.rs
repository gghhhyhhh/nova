use crate::routes;
use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use nova::workspace::{self, CreateProjectPayload, MAX_MEMBERS_PER_PROJECT, MAX_PROJECTS_PER_DAY};
use sqlx::sqlite::SqlitePool;

// API Projets / Groupes
// ============================================================

async fn api_create_project(
    payload: web::Json<CreateProjectPayload>,
    pool: web::Data<SqlitePool>,
    session: Session,
) -> impl Responder {
    let session_id = routes::get_or_create_session_id(&session);
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
    let session_id = routes::get_or_create_session_id(&session);
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
    let session_id = routes::get_or_create_session_id(&session);
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
    let session_id = routes::get_or_create_session_id(&session);
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
    let session_id = routes::get_or_create_session_id(&session);
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
    let session_id = routes::get_or_create_session_id(&session);
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

/// Enregistre les routes liées aux projets, aux groupes et à l'historique.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/join/{id}", web::get().to(join_via_link))
        .route("/api/projects", web::post().to(api_create_project))
        .route("/api/projects", web::get().to(api_list_projects))
        .route("/api/projects/{id}/join", web::post().to(api_join_project))
        .route("/api/projects/{id}", web::delete().to(api_delete_project))
        .route("/api/limits", web::get().to(api_limits))
        .route("/api/history", web::get().to(api_history));
}
