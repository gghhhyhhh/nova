use actix_session::Session;
use actix_web::{web, HttpResponse, Responder};
use chrono::{FixedOffset, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tera::{Context, Tera};

#[derive(Serialize)]
pub struct ForumTopic {
    pub id: i64,
    pub title: String,
    pub username: String,
    pub created_at: Option<String>,
}

#[derive(Serialize)]
pub struct ForumPost {
    pub id: i64,
    pub content: String,
    pub username: String,
    pub created_at: Option<String>,
    pub likes: i64,
}

#[derive(Deserialize)]
pub struct CreateTopicForm {
    pub title: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct CreateCommentForm {
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
    let topics = match sqlx::query_as::<_, (i64, String, String, Option<String>)>(
        "SELECT forum_topics.id,
                forum_topics.title,
                users.username,
                forum_topics.created_at
         FROM forum_topics
         LEFT JOIN users ON users.id = forum_topics.user_id
         ORDER BY forum_topics.created_at DESC",
    )
    .fetch_all(pool.get_ref())
    .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|(id, title, username, created_at)| ForumTopic {
                id,
                title,
                username,
                created_at: format_date_fr(created_at),
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

pub async fn add_comment(
    pool: web::Data<SqlitePool>,
    topic_id: web::Path<i64>,
    session: Session,
    form: web::Form<CreateCommentForm>,
) -> impl Responder {
    let user_id = match session.get::<i64>("user_id") {
        Ok(Some(id)) => id,
        _ => {
            return HttpResponse::Unauthorized().body("Vous devez être connecté pour commenter.");
        }
    };

    let topic_id = topic_id.into_inner();
    let content = form.content.trim();

    if content.is_empty() {
        return HttpResponse::BadRequest().body("Le commentaire est obligatoire.");
    }

    let result = sqlx::query(
        "INSERT INTO forum_posts (topic_id, user_id, content)
         VALUES (?, ?, ?)",
    )
    .bind(topic_id)
    .bind(user_id)
    .bind(content)
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Found()
            .append_header(("Location", format!("/reseaux/{}", topic_id)))
            .finish(),

        Err(e) => {
            eprintln!("Erreur création commentaire : {e}");
            HttpResponse::InternalServerError().body("Impossible de créer le commentaire")
        }
    }
}

pub async fn like_post(
    pool: web::Data<SqlitePool>,
    post_id: web::Path<i64>,
    session: Session,
) -> impl Responder {
    let user_id = match session.get::<i64>("user_id") {
        Ok(Some(id)) => id,
        _ => {
            return HttpResponse::Unauthorized().body("Vous devez être connecté pour liker.");
        }
    };

    let post_id = post_id.into_inner();

    let topic_id = match sqlx::query_as::<_, (i64,)>(
        "SELECT topic_id
     FROM forum_posts
     WHERE id = ?",
    )
    .bind(post_id)
    .fetch_optional(pool.get_ref())
    .await
    {
        Ok(Some((topic_id,))) => topic_id,

        Ok(None) => {
            return HttpResponse::NotFound().body("Message introuvable.");
        }

        Err(e) => {
            eprintln!("Erreur récupération du sujet : {e}");
            return HttpResponse::InternalServerError().body("Erreur interne du serveur");
        }
    };

    let result = sqlx::query(
        "INSERT OR IGNORE INTO forum_post_likes (post_id, user_id)
     VALUES (?, ?)",
    )
    .bind(post_id)
    .bind(user_id)
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => HttpResponse::Found()
            .append_header(("Location", format!("/reseaux/{}", topic_id)))
            .finish(),

        Err(e) => {
            eprintln!("Erreur ajout like : {e}");
            HttpResponse::InternalServerError().body("Impossible d'ajouter le like")
        }
    }
}

pub async fn topic(
    pool: web::Data<SqlitePool>,
    tera: web::Data<Tera>,
    topic_id: web::Path<i64>,
) -> impl Responder {
    let topic_id = topic_id.into_inner();

    let topic = match sqlx::query_as::<_, (i64, String, String, Option<String>)>(
        "SELECT forum_topics.id,
            forum_topics.title,
            users.username,
            forum_topics.created_at
     FROM forum_topics
     LEFT JOIN users ON users.id = forum_topics.user_id
     WHERE forum_topics.id = ?",
    )
    .bind(topic_id)
    .fetch_optional(pool.get_ref())
    .await
    {
        Ok(Some((id, title, username, created_at))) => ForumTopic {
            id,
            title,
            username,
            created_at: format_date_fr(created_at),
        },

        Ok(None) => {
            return HttpResponse::NotFound().body("Message introuvable.");
        }

        Err(e) => {
            eprintln!("Erreur récupération du sujet : {e}");
            return HttpResponse::InternalServerError().body("Erreur interne du serveur");
        }
    };

    let posts = match sqlx::query_as::<_, (i64, String, String, Option<String>, i64)>(
        "SELECT forum_posts.id,
            forum_posts.content,
            users.username,
            forum_posts.created_at,
            COUNT(forum_post_likes.id)
     FROM forum_posts
     LEFT JOIN users ON users.id = forum_posts.user_id
     LEFT JOIN forum_post_likes ON forum_post_likes.post_id = forum_posts.id
     WHERE forum_posts.topic_id = ?
     GROUP BY forum_posts.id
     ORDER BY forum_posts.created_at ASC",
    )
    .bind(topic_id)
    .fetch_all(pool.get_ref())
    .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|(id, content, username, created_at, likes)| ForumPost {
                id,
                content,
                username,
                created_at,
                likes,
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

fn format_date_fr(date: Option<String>) -> Option<String> {
    let date = date?;

    let naive = NaiveDateTime::parse_from_str(&date, "%Y-%m-%d %H:%M:%S").ok()?;

    let utc = Utc.from_utc_datetime(&naive);

    let paris = FixedOffset::east_opt(2 * 60 * 60)?;

    Some(
        utc.with_timezone(&paris)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
    )
}
