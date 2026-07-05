use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DocumentVector {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub url: String,
    pub category: Option<String>,
    pub tf: HashMap<String, f64>,
    pub raw_text: String,
}

#[derive(Debug, Clone)]
pub struct TfIdfEngine {
    pub documents: Vec<DocumentVector>,
    pub idf: HashMap<String, f64>,
    pub document_count: usize,
}

impl TfIdfEngine {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            idf: HashMap::new(),
            document_count: 0,
        }
    }

    pub fn add_document(
        &mut self,
        id: i64,
        title: &str,
        description: &str,
        url: String,
        category: Option<String>,
    ) {
        let text = format!(
            "{} {} {} {}",
            title,
            description,
            url,
            category.as_deref().unwrap_or("")
        );
        let words = Self::tokenize(&text);
        let mut tf = HashMap::new();

        for word in &words {
            *tf.entry(word.clone()).or_insert(0.0) += 1.0;
        }

        let total_words = words.len() as f64;
        if total_words > 0.0 {
            for count in tf.values_mut() {
                *count /= total_words;
            }
        }

        self.documents.push(DocumentVector {
            id,
            title: title.to_string(),
            description: description.to_string(),
            url: url.clone(),
            category: category.clone(),
            tf,
            raw_text: text.to_lowercase(),
        });

        self.document_count += 1;
    }

    pub fn compute_idf(&mut self) {
        let mut document_frequency: HashMap<String, usize> = HashMap::new();

        for doc in &self.documents {
            for word in doc.tf.keys() {
                *document_frequency.entry(word.clone()).or_insert(0) += 1;
            }
        }

        let n = self.document_count as f64;
        for (word, df) in document_frequency {
            let idf_value = if df > 0 {
                (n / df as f64).ln() + 1.0
            } else {
                1.0
            };
            self.idf.insert(word, idf_value);
        }
    }

    /// Récupère un document par son id (utilisé pour reconstruire un SearchResult
    /// à partir d'un score renvoyé par `search`).
    pub fn get_document(&self, id: i64) -> Option<&DocumentVector> {
        self.documents.iter().find(|d| d.id == id)
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<(i64, f64)> {
        let query_lower = query.to_lowercase();
        let query_words = Self::tokenize(query);

        println!("  🔍 Requête: '{}' | Mots: {:?}", query, query_words);

        if query_words.is_empty() && query.len() < 2 {
            return Vec::new();
        }

        let mut scores: Vec<(i64, f64)> = Vec::new();

        for doc in &self.documents {
            let mut score = 0.0;

            // 1. Correspondance exacte TF-IDF
            for word in &query_words {
                if let Some(tf) = doc.tf.get(word) {
                    let idf = self.idf.get(word).unwrap_or(&1.0);
                    score += tf * idf * 3.0;
                }
            }

            // 2. Sous-chaîne dans le texte brut
            if doc.raw_text.contains(&query_lower) {
                score += 1.0;
            }

            // 3. Correspondance titre (bonus important)
            if doc.title.to_lowercase().contains(&query_lower) {
                score += 2.0;
            }

            // 4. Correspondance description
            if doc.description.to_lowercase().contains(&query_lower) {
                score += 1.0;
            }

            // 5. Correspondance catégorie
            if let Some(cat) = &doc.category {
                if cat.to_lowercase().contains(&query_lower) {
                    score += 0.8;
                }
            }

            // 6. Correspondance URL
            if doc.url.to_lowercase().contains(&query_lower) {
                score += 0.5;
            }

            if score > 0.0 {
                scores.push((doc.id, score));
            }
        }

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scores.truncate(top_k);
        scores
    }

    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|s| !s.is_empty() && s.len() > 2)
            .collect()
    }
}