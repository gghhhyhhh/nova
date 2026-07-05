use serde::{Deserialize, Serialize};

// Si tu as déjà des modèles, garde-les et ajoute juste :
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}