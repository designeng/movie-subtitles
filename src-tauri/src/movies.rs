//! Resolves a movie title typed in any language (e.g. "Начало") into the
//! original title and year ("Inception", 2010) using Wikidata. Keyless.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{msg, Result};

const API: &str = "https://www.wikidata.org/w/api.php";
const SEARCH_LIMIT: &str = "15";
const MAX_MOVIES: usize = 6;

/// `instance of` values that count as a movie or a show.
const MOVIE_CLASSES: &[&str] = &[
    "Q11424",    // film
    "Q24869",    // feature film
    "Q202866",   // animated film
    "Q29168811", // animated feature film
    "Q226730",   // silent film
    "Q506240",   // television film
    "Q5398426",  // television series
    "Q581714",   // animated series
    "Q1259759",  // miniseries
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Movie {
    /// Wikidata item id, e.g. "Q25188".
    pub id: String,
    /// Title in the original language.
    pub title: String,
    /// ISO 639 code of the original title, if known.
    pub language: Option<String>,
    pub year: Option<u32>,
    /// Name and description in the user's language, for picking among matches.
    pub label: String,
    pub description: String,
}

/// Movies matching `query`, best match first: exact title matches, then the
/// most widely covered (number of Wikipedia articles).
pub async fn lookup(query: &str, language: &str) -> Result<Vec<Movie>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent(concat!("MovieSubtitles/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let found: SearchResponse = get(&client, &[
        ("action", "wbsearchentities"),
        ("search", query),
        ("language", language),
        ("uselang", language),
        ("type", "item"),
        ("limit", SEARCH_LIMIT),
        ("format", "json"),
    ])
    .await?;
    if found.search.is_empty() {
        return Ok(Vec::new());
    }
    let ids = found.search.iter().map(|h| h.id.as_str()).collect::<Vec<_>>().join("|");
    let details: EntitiesResponse = get(&client, &[
        ("action", "wbgetentities"),
        ("ids", &ids),
        ("props", "claims|sitelinks|labels"),
        ("languages", "en"),
        ("format", "json"),
    ])
    .await?;

    let wanted = normalize(query);
    let mut ranked: Vec<(bool, usize, Movie)> = found
        .search
        .into_iter()
        .filter_map(|hit| {
            let entity = details.entities.get(&hit.id)?;
            let movie = to_movie(&hit, entity)?;
            let exact = normalize(hit.matched.as_ref().map_or(&hit.label, |m| &m.text)) == wanted;
            Some((exact, entity.sitelinks.len(), movie))
        })
        .collect();
    ranked.sort_by_key(|(exact, popularity, _)| (!exact, std::cmp::Reverse(*popularity)));
    Ok(ranked.into_iter().take(MAX_MOVIES).map(|(_, _, m)| m).collect())
}

async fn get<T: for<'de> Deserialize<'de>>(client: &Client, params: &[(&str, &str)]) -> Result<T> {
    let resp = client.get(API).query(params).send().await?;
    if !resp.status().is_success() {
        return Err(msg(format!("Wikidata request failed ({})", resp.status())));
    }
    Ok(resp.json().await?)
}

fn to_movie(hit: &SearchHit, entity: &Entity) -> Option<Movie> {
    let classes: HashSet<&str> = claim_values(entity, "P31").filter_map(|v| v["id"].as_str()).collect();
    if !MOVIE_CLASSES.iter().any(|c| classes.contains(c)) {
        return None;
    }
    // P1476 "title" holds the original title with its language.
    let original = claim_values(entity, "P1476").next();
    let title = original
        .and_then(|v| v["text"].as_str())
        .map(String::from)
        .or_else(|| entity.labels.get("en").map(|l| l.value.clone()))
        .unwrap_or_else(|| hit.label.clone());
    Some(Movie {
        id: hit.id.clone(),
        title,
        language: original.and_then(|v| v["language"].as_str()).map(String::from),
        // P577 "publication date": the earliest release, e.g. "+2010-07-08T00:00:00Z".
        year: claim_values(entity, "P577")
            .filter_map(|v| v["time"].as_str()?.get(1..5)?.parse().ok())
            .min(),
        label: hit.label.clone(),
        description: hit.description.clone().unwrap_or_default(),
    })
}

fn claim_values<'a>(entity: &'a Entity, property: &str) -> impl Iterator<Item = &'a Value> {
    entity
        .claims
        .get(property)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|c| &c.mainsnak.datavalue.value)
        .filter(|v| !v.is_null())
}

/// Case- and "ё"-insensitive comparison key.
fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase().replace('ё', "е")
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    search: Vec<SearchHit>,
}

#[derive(Deserialize)]
struct SearchHit {
    id: String,
    #[serde(default)]
    label: String,
    description: Option<String>,
    /// The label or alias that matched the query.
    #[serde(rename = "match")]
    matched: Option<MatchedText>,
}

#[derive(Deserialize)]
struct MatchedText {
    text: String,
}

#[derive(Deserialize)]
struct EntitiesResponse {
    #[serde(default)]
    entities: HashMap<String, Entity>,
}

#[derive(Deserialize)]
struct Entity {
    #[serde(default)]
    claims: HashMap<String, Vec<Claim>>,
    #[serde(default)]
    sitelinks: HashMap<String, Value>,
    #[serde(default)]
    labels: HashMap<String, Label>,
}

#[derive(Deserialize)]
struct Claim {
    mainsnak: Snak,
}

#[derive(Deserialize)]
struct Snak {
    /// Absent for "no value" / "unknown value" claims.
    #[serde(default)]
    datavalue: DataValue,
}

#[derive(Deserialize, Default)]
struct DataValue {
    #[serde(default)]
    value: Value,
}

#[derive(Deserialize)]
struct Label {
    value: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn entity(v: Value) -> Entity {
        serde_json::from_value(v).unwrap()
    }

    fn hit(label: &str) -> SearchHit {
        SearchHit { id: "Q1".into(), label: label.into(), description: None, matched: None }
    }

    #[test]
    fn reads_original_title_and_year() {
        let e = entity(json!({
            "claims": {
                "P31": [{"mainsnak": {"datavalue": {"value": {"id": "Q11424"}}}}],
                "P1476": [{"mainsnak": {"datavalue": {"value": {"text": "Inception", "language": "en"}}}}],
                "P577": [
                    {"mainsnak": {"datavalue": {"value": {"time": "+2010-07-29T00:00:00Z"}}}},
                    {"mainsnak": {"datavalue": {"value": {"time": "+2010-07-08T00:00:00Z"}}}},
                    {"mainsnak": {}}
                ]
            }
        }));
        let m = to_movie(&hit("Начало"), &e).unwrap();
        assert_eq!((m.title.as_str(), m.year, m.language.as_deref()), ("Inception", Some(2010), Some("en")));
    }

    #[test]
    fn skips_non_movies_and_falls_back_to_english_label() {
        let place = entity(json!({"claims": {"P31": [{"mainsnak": {"datavalue": {"value": {"id": "Q486972"}}}}]}}));
        assert!(to_movie(&hit("Начало"), &place).is_none());

        let e = entity(json!({
            "claims": {"P31": [{"mainsnak": {"datavalue": {"value": {"id": "Q24869"}}}}]},
            "labels": {"en": {"value": "Brother"}}
        }));
        assert_eq!(to_movie(&hit("Брат"), &e).unwrap().title, "Brother");
    }

    /// Hits the real endpoint: `cargo test -- --ignored finds_original_titles`.
    #[tokio::test]
    #[ignore]
    async fn finds_original_titles() {
        for (query, title, year) in [
            ("Начало", "Inception", 2010),
            ("Крестный отец", "The Godfather", 1972),
            ("Амели", "Le Fabuleux Destin d'Amélie Poulain", 2001),
            ("Брат", "Брат", 1997),
        ] {
            let found = lookup(query, "ru").await.unwrap();
            let best = found.first().unwrap();
            assert_eq!((best.title.as_str(), best.year), (title, Some(year)), "{query}: {found:?}");
        }
    }

    #[test]
    fn normalizes_titles() {
        assert_eq!(normalize("Крёстный  Отец"), normalize("крестный отец"));
    }
}
