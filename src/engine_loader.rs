use nova::TfIdfEngine;
use sqlx::sqlite::SqlitePool;

#[derive(Debug, sqlx::FromRow)]
struct ArticleRow {
    id: i64,
    title: String,
    description: Option<String>,
    url: String,
    category: Option<String>,
}

/// Charge les articles SQLite et construit le moteur TF-IDF.
pub async fn load_tfidf_engine(pool: &SqlitePool) -> TfIdfEngine {
    let mut engine = TfIdfEngine::new();

    let rows: Vec<ArticleRow> =
        sqlx::query_as("SELECT id, title, description, url, category FROM articles")
            .fetch_all(pool)
            .await
            .unwrap_or_else(|e| {
                eprintln!("⚠️ Impossible de charger articles.db : {e}");
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
