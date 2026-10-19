use chrono::{Datelike, Local};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SearchSuggestion {
    pub text: String,
    pub kind: String,
}

pub fn generate_suggestions(input: &str) -> Vec<SearchSuggestion> {
    let query = input.trim();
    if query.is_empty() {
        return Vec::new();
    }

    let normalized = query.to_lowercase();
    let mut suggestions: Vec<SearchSuggestion> = Vec::new();

    let mut add = |text: String, kind: &str| {
        if !text.eq_ignore_ascii_case(query)
            && !suggestions
                .iter()
                .any(|s| s.text.eq_ignore_ascii_case(&text))
        {
            suggestions.push(SearchSuggestion {
                text,
                kind: kind.to_string(),
            });
        }
    };

    if let Ok(day) = query.parse::<u32>() {
        if (1..=31).contains(&day) {
            let today = Local::now().date_naive();
            let months = [
                "janvier",
                "février",
                "mars",
                "avril",
                "mai",
                "juin",
                "juillet",
                "août",
                "septembre",
                "octobre",
                "novembre",
                "décembre",
            ];

            for offset in 0..6 {
                let month_number = (today.month0() + offset) % 12 + 1;
                let year = today.year() + ((today.month0() + offset) / 12) as i32;

                if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month_number, day) {
                    add(
                        format!(
                            "{} {} {}",
                            date.day(),
                            months[(month_number - 1) as usize],
                            year
                        ),
                        "Date",
                    );
                }
            }

            add(format!("{} au carré", day), "Calcul");
            add(format!("{} puissance 3", day), "Calcul");
            add(format!("{} kilomètres en miles", day), "Conversion");
            add(format!("{} idées de projets informatiques", day), "Idées");
            add(format!("{} joueurs de football célèbres", day), "Sport");
        }
    }

    if normalized.starts_with("météo") || normalized.starts_with("meteo") {
        add("météo demain".to_string(), "Recherche");
        add("météo cette semaine".to_string(), "Recherche");
        add("météo Paris".to_string(), "Recherche");
    } else if normalized.starts_with("comment") {
        add(format!("{} expliquer simplement", query), "Question");
        add(format!("{} étape par étape", query), "Question");
    } else if normalized.starts_with("pourquoi") {
        add(format!("{} explication détaillée", query), "Question");
        add(format!("{} exemples", query), "Question");
    } else if normalized.starts_with("python") {
        add("Python tutoriel débutant".to_string(), "Informatique");
        add("Python exemples de code".to_string(), "Informatique");
        add("Python projets pour débutants".to_string(), "Informatique");
    } else {
        add(format!("{} définition", query), "Définition");
        add(format!("{} explication", query), "Explication");
        add(format!("{} exemples", query), "Exemples");
        add(format!("{} actualités", query), "Actualités");
    }

    suggestions.truncate(8);
    suggestions
}

/// Récupère les suggestions générales depuis Google.
pub async fn fetch_google_suggestions(input: &str) -> Vec<SearchSuggestion> {
    let query = input.trim();

    if query.is_empty() {
        return Vec::new();
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(client) => client,
        Err(_) => return Vec::new(),
    };

    let response = match client
        .get("https://suggestqueries.google.com/complete/search")
        .query(&[("client", "firefox"), ("q", query)])
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response,
        _ => return Vec::new(),
    };

    let data: serde_json::Value = match response.json().await {
        Ok(data) => data,
        Err(_) => return Vec::new(),
    };

    let Some(items) = data.get(1).and_then(|value| value.as_array()) else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|item| item.as_str())
        .filter(|text| !text.trim().is_empty() && !text.eq_ignore_ascii_case(query))
        .take(8)
        .map(|text| SearchSuggestion {
            text: text.to_string(),
            kind: "Suggestion Google".to_string(),
        })
        .collect()
}
