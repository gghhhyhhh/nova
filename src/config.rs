use dotenvy::dotenv;

#[derive(Clone)]
pub struct DbConfig {
    pub connection_string: String,
}

impl DbConfig {
    pub fn from_env() -> Self {
        dotenv().ok();
        Self {
            connection_string: "sqlite:./articles.db".to_string(),
        }
    }

    pub fn connection_string(&self) -> &str {
        &self.connection_string
    }
}