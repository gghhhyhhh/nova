use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeEntry {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelatedLink {
    pub url: String,
    pub title: String,
    pub type_: String,
}

#[derive(Debug, Clone)]
pub struct KnowledgeBase {
    pub entries: HashMap<i64, KnowledgeEntry>,
}

impl KnowledgeBase {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn add_entry(&mut self, entry: KnowledgeEntry) {
        self.entries.insert(entry.id, entry);
    }

    pub fn search(&self, query: &str) -> Vec<&KnowledgeEntry> {
        let query_lower = query.to_lowercase();
        self.entries
            .values()
            .filter(|e| {
                e.title.to_lowercase().contains(&query_lower)
                    || e.content.to_lowercase().contains(&query_lower)
                    || e.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}