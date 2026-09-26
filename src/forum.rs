use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tera::{Context, Tera};

#[derive(Serialize)]
pub struct ForumTopic {
    pub id: i64,
    pub title: String,
    pub created_at: Option<String>,
}

#[derive(Serialize)]
pub struct ForumPost {
    pub id: i64,
    pub content: String,
    pub created_at: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateTopicForm {
    pub title: String,
    pub content: String,
}

pub async fn init(pool: &SqlitePool) {
    if let Err(e) = sqlx::query(
        "CREATE TABLE IF NOT EXISTS forum_categories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            description TEXT
        )",
    )
    .execute(pool)
    .await
    {
        eprintln!("Erreur création table forum_categories: {e}");
    }

    if let Err(e) = sqlx::query(
        "CREATE TABLE IF NOT EXISTS forum_topics (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            category_id INTEGER NOT NULL,
            title TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (category_id) REFERENCES forum_categories(id)
        )",
    )
    .execute(pool)
    .await
    {
        eprintln!("Erreur création table forum_topics: {e}");
    }

    if let Err(e) = sqlx::query(
        "CREATE TABLE IF NOT EXISTS forum_posts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            topic_id INTEGER NOT NULL,
            content TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (topic_id) REFERENCES forum_topics(id)
        )",
    )
    .execute(pool)
    .await
    {
        eprintln!("Erreur création table forum_posts: {e}");
    }

    if let Err(e) = sqlx::query(
        "INSERT OR IGNORE INTO forum_categories (id, name, description)
         VALUES (1, 'Forum', 'Forum public de NOVA')",
    )
    .execute(pool)
    .await
    {
        eprintln!("Erreur création catégorie interne du forum: {e}");
    }
}

pub async fn index(pool: web::Data<SqlitePool>, tera: web::Data<Tera>) -> impl Responder {
    let topics = match sqlx::query_as::<_, (i64, String, Option<String>)>(
        "SELECT id, title, created_at
         FROM forum_topics
         ORDER BY created_at DESC",
    )
    .fetch_all(pool.get_ref())
    .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|(id, title, created_at)| ForumTopic {
                id,
                title,
                created_at,
            })
            .collect::<Vec<ForumTopic>>(),

        Err(e) => {
            eprintln!("Erreur récupération des sujets : {e}");
            return HttpResponse::InternalServerError().body("Erreur interne du serveur");
        }
    };

    let mut context = Context::new();
    context.insert("topics", &topics);

    let rendered = tera.render("reseaux.html", &context).unwrap_or_else(|e| {
        eprintln!("Erreur de rendu Tera : {e}");
        "<h1>Erreur interne du serveur</h1>".to_string()
    });

    HttpResponse::Ok().body(rendered)
}

pub async fn create_topic_page(tera: web::Data<Tera>) -> impl Responder {
    let context = Context::new();

    let rendered = tera
        .render("forum-create-topic.html", &context)
        .unwrap_or_else(|e| {
            eprintln!("Erreur de rendu Tera : {e}");
            "<h1>Erreur interne du serveur</h1>".to_string()
        });

    HttpResponse::Ok().body(rendered)
}

pub async fn create_topic(
    pool: web::Data<SqlitePool>,
    form: web::Form<CreateTopicForm>,
) -> impl Responder {
    let title = form.title.trim();
    let content = form.content.trim();

    if title.is_empty() || content.is_empty() {
        return HttpResponse::BadRequest().body("Le titre et le contenu sont obligatoires.");
    }

    let mut transaction = match pool.begin().await {
        Ok(transaction) => transaction,

        Err(e) => {
            eprintln!("Erreur démarrage transaction : {e}");
            return HttpResponse::InternalServerError().body("Erreur interne du serveur");
        }
    };

    let topic_id = match sqlx::query(
        "INSERT INTO forum_topics (category_id, title)
         VALUES (1, ?)",
    )
    .bind(title)
    .execute(&mut *transaction)
    .await
    {
        Ok(result) => result.last_insert_rowid(),

        Err(e) => {
            eprintln!("Erreur création sujet : {e}");
            return HttpResponse::InternalServerError().body("Impossible de créer le sujet");
        }
    };

    if let Err(e) = sqlx::query(
        "INSERT INTO forum_posts (topic_id, content)
         VALUES (?, ?)",
    )
    .bind(topic_id)
    .bind(content)
    .execute(&mut *transaction)
    .await
    {
        eprintln!("Erreur création message : {e}");
        return HttpResponse::InternalServerError().body("Impossible de créer le message");
    }

    if let Err(e) = transaction.commit().await {
        eprintln!("Erreur validation transaction : {e}");
        return HttpResponse::InternalServerError().body("Erreur interne du serveur");
    }

    HttpResponse::Found()
        .append_header(("Location", format!("/reseaux/{}", topic_id)))
        .finish()
}

pub async fn topic(
    pool: web::Data<SqlitePool>,
    tera: web::Data<Tera>,
    topic_id: web::Path<i64>,
) -> impl Responder {
    let topic_id = topic_id.into_inner();

    let topic = match sqlx::query_as::<_, (i64, String, Option<String>)>(
        "SELECT id, title, created_at
         FROM forum_topics
         WHERE id = ?",
    )
    .bind(topic_id)
    .fetch_optional(pool.get_ref())
    .await
    {
        Ok(Some((id, title, created_at))) => ForumTopic {
            id,
            title,
            created_at,
        },

        Ok(None) => {
            return HttpResponse::NotFound().body("Sujet introuvable");
        }

        Err(e) => {
            eprintln!("Erreur récupération sujet : {e}");
            return HttpResponse::InternalServerError().body("Erreur interne du serveur");
        }
    };

    let posts = match sqlx::query_as::<_, (i64, String, Option<String>)>(
        "SELECT id, content, created_at
         FROM forum_posts
         WHERE topic_id = ?
         ORDER BY created_at ASC",
    )
    .bind(topic_id)
    .fetch_all(pool.get_ref())
    .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|(id, content, created_at)| ForumPost {
                id,
                content,
                created_at,
            })
            .collect::<Vec<ForumPost>>(),

        Err(e) => {
            eprintln!("Erreur récupération messages : {e}");
            return HttpResponse::InternalServerError().body("Erreur interne du serveur");
        }
    };

    let mut context = Context::new();

    context.insert("topic", &topic);
    context.insert("posts", &posts);

    let rendered = tera
        .render("forum-topic.html", &context)
        .unwrap_or_else(|e| {
            eprintln!("Erreur de rendu Tera : {e}");
            "<h1>Erreur interne du serveur</h1>".to_string()
        });

    HttpResponse::Ok().body(rendered)
}
