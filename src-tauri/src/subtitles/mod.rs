//! Subtitle providers, parsing and search-query helpers.

pub mod format;
mod opensubtitles;
pub mod query;
mod subdl;

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{msg, Result};
use crate::settings::Settings;

pub use opensubtitles::OpenSubtitles;
pub use subdl::SubDl;

#[derive(Debug, Clone)]
pub struct SubtitleQuery {
    pub text: String,
    /// ISO 639-1 code, e.g. "en".
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleCandidate {
    pub provider: String,
    /// Provider-specific file id.
    pub id: String,
    pub title: String,
    pub year: Option<u32>,
    pub release: String,
    pub language: String,
    pub downloads: u64,
    pub hearing_impaired: bool,
    pub fps: Option<f64>,
}

#[async_trait]
pub trait SubtitleProvider: Send + Sync {
    fn id(&self) -> &'static str;
    async fn search(&self, query: &SubtitleQuery) -> Result<Vec<SubtitleCandidate>>;
    /// Returns the raw subtitle file bytes.
    async fn fetch(&self, candidate: &SubtitleCandidate) -> Result<Vec<u8>>;
}

pub struct Registry {
    providers: Vec<Arc<dyn SubtitleProvider>>,
}

impl Registry {
    pub fn from_settings(settings: &Settings) -> Self {
        let mut providers: Vec<Arc<dyn SubtitleProvider>> = Vec::new();
        let key = settings.opensubtitles_api_key.trim();
        if !key.is_empty() {
            let user = settings.opensubtitles_username.trim();
            let credentials = (!user.is_empty() && !settings.opensubtitles_password.is_empty())
                .then(|| (user.to_string(), settings.opensubtitles_password.clone()));
            providers.push(Arc::new(OpenSubtitles::new(key.to_string(), credentials)));
        }
        let key = settings.subdl_api_key.trim();
        if !key.is_empty() {
            providers.push(Arc::new(SubDl::new(key.to_string())));
        }
        Self { providers }
    }

    pub async fn search(&self, query: &SubtitleQuery) -> Result<Vec<SubtitleCandidate>> {
        if self.providers.is_empty() {
            return Err(msg("No subtitle providers configured. Add an OpenSubtitles or SubDL API key in Settings."));
        }
        let mut all = Vec::new();
        let mut last_err = None;
        for provider in &self.providers {
            match provider.search(query).await {
                Ok(found) => all.extend(found),
                Err(e) => last_err = Some(e),
            }
        }
        match last_err {
            Some(e) if all.is_empty() => Err(e),
            _ => Ok(all),
        }
    }

    pub async fn fetch(&self, candidate: &SubtitleCandidate) -> Result<Vec<u8>> {
        let provider = self
            .providers
            .iter()
            .find(|p| p.id() == candidate.provider)
            .ok_or_else(|| msg(format!("Subtitle provider '{}' is not configured", candidate.provider)))?;
        provider.fetch(candidate).await
    }
}
