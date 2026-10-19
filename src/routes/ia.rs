use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;
use sqlx::sqlite::SqlitePool;

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

/// Enregistre les routes de l'assistant IA et de ses conversations.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/ia/chat", web::post().to(api_ia_chat))
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
        );
}
