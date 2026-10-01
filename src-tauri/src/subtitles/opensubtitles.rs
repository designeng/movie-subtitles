//! OpenSubtitles.com REST API (https://opensubtitles.stoplight.io/docs/opensubtitles-api).

use async_trait::async_trait;
use reqwest::{Client, RequestBuilder, StatusCode};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Mutex;

use super::{SubtitleCandidate, SubtitleProvider, SubtitleQuery};
use crate::error::{msg, Result};

const API: &str = "https://api.opensubtitles.com/api/v1";
const USER_AGENT: &str = concat!("MovieSubtitles v", env!("CARGO_PKG_VERSION"));
pub const ID: &str = "opensubtitles";

pub struct OpenSubtitles {
    client: Client,
    api_key: String,
    /// Optional account; anonymous downloads have a much lower daily quota.
    credentials: Option<(String, String)>,
    token: Mutex<Option<String>>,
}

impl OpenSubtitles {
    pub fn new(api_key: String, credentials: Option<(String, String)>) -> Self {
        Self { client: Client::new(), api_key, credentials, token: Mutex::new(None) }
    }

    fn request(&self, builder: RequestBuilder) -> RequestBuilder {
        builder
            .header("Api-Key", &self.api_key)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::ACCEPT, "application/json")
    }

    async fn token(&self) -> Result<Option<String>> {
        let Some((username, password)) = &self.credentials else { return Ok(None) };
        let mut token = self.token.lock().await;
        if token.is_none() {
            let resp = self
                .request(self.client.post(format!("{API}/login")))
                .json(&json!({ "username": username, "password": password }))
                .send()
                .await?;
            if !resp.status().is_success() {
                return Err(msg(format!("OpenSubtitles login failed: {}", resp.text().await?)));
            }
            *token = Some(resp.json::<LoginResponse>().await?.token);
        }
        Ok(token.clone())
    }
}

#[async_trait]
impl SubtitleProvider for OpenSubtitles {
    fn id(&self) -> &'static str {
        ID
    }

    async fn search(&self, query: &SubtitleQuery) -> Result<Vec<SubtitleCandidate>> {
        // The API redirects unless parameters are lowercase and alphabetically sorted.
        let params = [
            ("languages", query.language.to_lowercase()),
            ("order_by", "download_count".to_string()),
            ("query", query.text.trim().to_lowercase()),
        ];
        let resp = self
            .request(self.client.get(format!("{API}/subtitles")))
            .query(&params)
            .send()
            .await?;
        if resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN {
            return Err(msg("OpenSubtitles rejected the API key. Check it in Settings."));
        }
        let resp: SearchResponse = resp.error_for_status()?.json().await?;

        Ok(resp
            .data
            .into_iter()
            .flat_map(|item| {
                let a = item.attributes;
                let feature = a.feature_details.unwrap_or_default();
                a.files.into_iter().map(move |f| SubtitleCandidate {
                    provider: ID.to_string(),
                    id: f.file_id.to_string(),
                    title: feature.title.clone().or(feature.movie_name.clone()).unwrap_or_default(),
                    year: feature.year,
                    release: a.release.clone().or(f.file_name).unwrap_or_default(),
                    language: a.language.clone().unwrap_or_default(),
                    downloads: a.download_count,
                    hearing_impaired: a.hearing_impaired,
                    fps: a.fps.filter(|fps| *fps > 0.0),
                })
            })
            .collect())
    }

    async fn fetch(&self, candidate: &SubtitleCandidate) -> Result<Vec<u8>> {
        let file_id: u64 = candidate.id.parse().map_err(|_| msg("Invalid OpenSubtitles file id"))?;
        let mut req = self
            .request(self.client.post(format!("{API}/download")))
            .json(&json!({ "file_id": file_id }));
        if let Some(token) = self.token().await? {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Err(msg(format!(
                "OpenSubtitles download failed ({}): {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        let link = resp.json::<DownloadResponse>().await?.link;
        Ok(self.client.get(link).send().await?.error_for_status()?.bytes().await?.to_vec())
    }
}

#[derive(Deserialize)]
struct LoginResponse {
    token: String,
}

#[derive(Deserialize)]
struct DownloadResponse {
    link: String,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    data: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    attributes: Attributes,
}

#[derive(Deserialize)]
struct Attributes {
    language: Option<String>,
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    hearing_impaired: bool,
    fps: Option<f64>,
    release: Option<String>,
    feature_details: Option<FeatureDetails>,
    #[serde(default)]
    files: Vec<SubtitleFile>,
}

#[derive(Deserialize, Default, Clone)]
struct FeatureDetails {
    title: Option<String>,
    movie_name: Option<String>,
    year: Option<u32>,
}

#[derive(Deserialize)]
struct SubtitleFile {
    file_id: u64,
    file_name: Option<String>,
}
