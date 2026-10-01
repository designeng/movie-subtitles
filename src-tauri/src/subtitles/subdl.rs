//! SubDL REST API (https://subdl.com/api-doc). Downloads are ZIP archives.

use std::collections::HashMap;
use std::sync::LazyLock;

use async_trait::async_trait;
use regex::Regex;
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use super::{unpack, SubtitleCandidate, SubtitleProvider, SubtitleQuery};
use crate::error::{msg, Result};

const API: &str = "https://api.subdl.com/api/v1/subtitles";
const DOWNLOAD: &str = "https://dl.subdl.com";
pub const ID: &str = "subdl";

/// The API rejects film names with these ("potentially unsafe characters"). They must become
/// spaces, not be dropped: the index stores "Grey's" as the tokens `grey` + `s`.
static UNSAFE_TITLE_CHARS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[<>{}\[\]'"`´’;\\/]"#).unwrap());

pub struct SubDl {
    client: Client,
    api_key: String,
}

impl SubDl {
    pub fn new(api_key: String) -> Self {
        Self { client: Client::new(), api_key }
    }
}

#[async_trait]
impl SubtitleProvider for SubDl {
    fn id(&self) -> &'static str {
        ID
    }

    fn name(&self) -> &'static str {
        "SubDL"
    }

    async fn search(&self, query: &SubtitleQuery) -> Result<Vec<SubtitleCandidate>> {
        let film_name = UNSAFE_TITLE_CHARS.replace_all(query.text.trim(), " ");
        let film_name = film_name.split_whitespace().collect::<Vec<_>>().join(" ");
        let params = [
            ("api_key", self.api_key.clone()),
            ("film_name", film_name),
            ("languages", query.language.to_uppercase()),
            ("subs_per_page", "30".to_string()),
        ];
        let resp = self.client.get(API).query(&params).send().await?;
        if resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN {
            return Err(msg("SubDL rejected the API key. Check it in Settings."));
        }
        let resp: SearchResponse = resp.error_for_status()?.json().await?;
        if !resp.status {
            let error = resp.error.unwrap_or_default();
            // "Can't find movie or tv" is how SubDL reports an empty result.
            if error.to_lowercase().contains("can't find") {
                return Ok(Vec::new());
            }
            return Err(msg(format!("SubDL search failed: {error}")));
        }

        let titles: HashMap<u64, &SearchResult> =
            resp.results.iter().filter_map(|r| Some((r.sd_id?, r))).collect();
        let fallback = resp.results.first();

        Ok(resp
            .subtitles
            .into_iter()
            .map(|s| {
                let film = s.sd_id.and_then(|id| titles.get(&id).copied()).or(fallback);
                SubtitleCandidate {
                    provider: ID.to_string(),
                    id: s.url,
                    title: film.and_then(|f| f.name.clone()).unwrap_or_default(),
                    year: film.and_then(|f| f.year),
                    release: s.release_name.or(s.name).unwrap_or_default(),
                    language: s.language.unwrap_or_default().to_lowercase(),
                    downloads: 0,
                    hearing_impaired: s.hi,
                    fps: None,
                }
            })
            .collect())
    }

    async fn fetch(&self, candidate: &SubtitleCandidate) -> Result<Vec<u8>> {
        let url = format!("{DOWNLOAD}{}", candidate.id);
        let resp = self.client.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(msg(format!("SubDL download failed ({})", resp.status())));
        }
        let bytes = resp.bytes().await?;
        unpack(bytes.to_vec())
    }
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    status: bool,
    error: Option<String>,
    #[serde(default)]
    results: Vec<SearchResult>,
    #[serde(default)]
    subtitles: Vec<SubtitleItem>,
}

#[derive(Deserialize)]
struct SearchResult {
    sd_id: Option<u64>,
    name: Option<String>,
    year: Option<u32>,
}

#[derive(Deserialize)]
struct SubtitleItem {
    sd_id: Option<u64>,
    url: String,
    release_name: Option<String>,
    name: Option<String>,
    /// Upper-case code, e.g. "EN".
    language: Option<String>,
    #[serde(default)]
    hi: bool,
}
