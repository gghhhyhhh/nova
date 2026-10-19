use actix_web::web;

use crate::auth;

/// Enregistre les routes liées à l'authentification.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/login", web::get().to(auth::login_page))
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
            "/verify-reset-code",
            web::get().to(auth::verify_reset_code_page),
        )
        .route(
            "/verify-reset-code",
            web::post().to(auth::verify_reset_code),
        )
        .route("/reset-password", web::get().to(auth::reset_password_page))
        .route("/reset-password", web::post().to(auth::reset_password));
}
