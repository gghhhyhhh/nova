use actix_web::web;

use crate::forum;

/// Enregistre les routes du forum.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/reseaux", web::get().to(forum::index))
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
        );
}
