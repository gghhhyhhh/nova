use futures::future::join_all;
use reqwest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub img_src: Option<String>,
    pub thumbnail: Option<String>,
    pub thumbnail_src: Option<String>,
    pub iframe_src: Option<String>,
    pub published_date: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum SearchCategory {
    Web,
    Images,
    Videos,
    News,
    Maps,
}

impl SearchCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            SearchCategory::Web => "general",
            SearchCategory::Images => "images",
            SearchCategory::Videos => "videos",
            SearchCategory::News => "news",
            SearchCategory::Maps => "map",
        }
    }
}

pub struct SearchEngine {
    client: reqwest::Client,
    searxng_url: String,
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchEngine {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap();

        SearchEngine {
            client,
            searxng_url: "http://localhost:8081".to_string(),
        }
    }

    /// Recherche rapide : 1 seule page (résultats immédiats)
    pub async fn search_fast(
        &self,
        query: &str,
        category: SearchCategory,
    ) -> Result<Vec<SearchResult>, reqwest::Error> {
        self.search_page(query, category, 1).await
    }

    /// Recherche complète : 3 pages en parallèle (plus de résultats)
    pub async fn search_full(
        &self,
        query: &str,
        category: SearchCategory,
    ) -> Result<Vec<SearchResult>, reqwest::Error> {
        let futures = (1..=3).map(|page| self.search_page(query, category, page));

        let pages = join_all(futures).await;
        let mut all_results = Vec::new();

        for page_result in pages {
            if let Ok(page_results) = page_result {
                for result in page_results {
                    if !all_results
                        .iter()
                        .any(|r: &SearchResult| r.url == result.url)
                    {
                        all_results.push(result);
                    }
                }
            }
        }

        Ok(all_results)
    }

    async fn search_page(
        &self,
        query: &str,
        category: SearchCategory,
        page: u32,
    ) -> Result<Vec<SearchResult>, reqwest::Error> {
        let encoded = urlencoding::encode(query);
        let url = format!(
            "{}/search?q={}&format=json&language=fr&pageno={}&categories={}",
            self.searxng_url,
            encoded,
            page,
            category.as_str()
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64)")
            .send()
            .await?;

        let json_text = response.text().await?;
        let results = self.parse_searxng_json(&json_text);

        Ok(results)
    }

    fn parse_searxng_json(&self, json: &str) -> Vec<SearchResult> {
        if json.trim().starts_with('<') {
            return Vec::new();
        }

        #[derive(Debug, Deserialize)]
        struct SearxngResponse {
            #[serde(default)]
            results: Vec<SearxngResult>,
        }

        #[derive(Debug, Deserialize)]
        struct SearxngResult {
            title: String,
            url: String,
            #[serde(default)]
            content: String,
            #[serde(default)]
            img_src: Option<String>,
            #[serde(default)]
            thumbnail: Option<String>,
            #[serde(default)]
            thumbnail_src: Option<String>,
            #[serde(default)]
            iframe_src: Option<String>,
            #[serde(default, rename = "publishedDate")]
            published_date: Option<String>,
            #[serde(default)]
            source: Option<String>,
        }

        let data: SearxngResponse = match serde_json::from_str(json) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };

        let total_before = data.results.len();

        let filtered = data.results
            .into_iter()
            .filter(|r| {
                let url = r.url.trim();
                !url.is_empty() && (url.starts_with("http://") || url.starts_with("https://") || url.starts_with("//"))
            })
            .map(|r| SearchResult {
                title: r.title,
                url: if r.url.starts_with("//") {
                    format!("https:{}", r.url)
                } else {
                    r.url
                },
                snippet: r.content,
                img_src: r.img_src,
                thumbnail: r.thumbnail,
                thumbnail_src: r.thumbnail_src,
                iframe_src: r.iframe_src,
                published_date: r.published_date,
                source: r.source,
            })
            .collect::<Vec<_>>();

        println!(
            "  🔗 {} résultat(s) reçus, {} conservé(s) après filtrage des URLs invalides",
            total_before,
            filtered.len()
        );

        filtered
    }
}