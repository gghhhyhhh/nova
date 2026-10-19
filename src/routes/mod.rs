use actix_session::Session;
use uuid::Uuid;
pub mod auth;
pub mod forum;
pub mod home;
pub mod search;

/// Récupère l'id de session anonyme, ou en crée un nouveau si absent.
pub(crate) fn get_or_create_session_id(session: &Session) -> String {
    if let Ok(Some(id)) = session.get::<String>("session_id") {
        id
    } else {
        let id = Uuid::new_v4().to_string();
        let _ = session.insert("session_id", &id);
        id
    }
}
pub mod ia;
pub mod projects;
