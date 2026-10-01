//! Subtitle providers, parsing and search-query helpers.

pub mod format;
mod opensubtitles;
pub mod query;
mod subdl;

use std::io::{Cursor, Read};
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

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSearch {
    pub candidates: Vec<SubtitleCandidate>,
    /// Names of the providers that were asked.
    pub searched: Vec<String>,
    /// "Provider: error" for providers that failed.
    pub errors: Vec<String>,
}

#[async_trait]
pub trait SubtitleProvider: Send + Sync {
    fn id(&self) -> &'static str;
    /// Human-readable name for messages.
    fn name(&self) -> &'static str;
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

    /// Searches every provider. A provider that fails is reported in `errors`
    /// instead of failing the whole search.
    pub async fn search(&self, query: &SubtitleQuery) -> Result<SubtitleSearch> {
        if self.providers.is_empty() {
            return Err(msg("No subtitle providers configured. Add an OpenSubtitles or SubDL API key in Settings."));
        }
        let mut report = SubtitleSearch::default();
        for provider in &self.providers {
            report.searched.push(provider.name().to_string());
            match provider.search(query).await {
                Ok(found) => report.candidates.extend(found),
                Err(e) => report.errors.push(format!("{}: {e}", provider.name())),
            }
        }
        Ok(report)
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

/// Downloads a subtitle file (or a ZIP archive with one) from a direct link.
pub async fn download(url: &str) -> Result<Vec<u8>> {
    let resp = reqwest::Client::new()
        .get(url)
        .header(reqwest::header::USER_AGENT, concat!("MovieSubtitles/", env!("CARGO_PKG_VERSION")))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(msg(format!("Subtitle download failed ({})", resp.status())));
    }
    unpack(resp.bytes().await?.to_vec())
}

/// Returns the subtitle file itself, or the first .srt (failing that, .vtt) inside a ZIP archive.
pub fn unpack(bytes: Vec<u8>) -> Result<Vec<u8>> {
    if !bytes.starts_with(b"PK") {
        return Ok(bytes);
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| msg(format!("Broken subtitle archive: {e}")))?;
    let names: Vec<String> = archive.file_names().map(str::to_string).collect();
    let name = [".srt", ".vtt"]
        .iter()
        .find_map(|ext| names.iter().find(|n| n.to_lowercase().ends_with(ext)))
        .ok_or_else(|| msg("The archive has no .srt or .vtt file"))?;
    let mut file = archive.by_name(name).map_err(|e| msg(format!("Subtitle archive: {e}")))?;
    let mut out = Vec::new();
    file.read_to_end(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn extracts_srt_from_zip() {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("readme.txt", opts).unwrap();
            zip.write_all(b"hello").unwrap();
            zip.start_file("Movie.2010.srt", opts).unwrap();
            zip.write_all(b"1\n00:00:01,000 --> 00:00:02,000\nHi\n").unwrap();
            zip.finish().unwrap();
        }
        let out = unpack(buf.into_inner()).unwrap();
        assert!(out.starts_with(b"1\n00:00:01"));
    }

    #[test]
    fn passes_plain_files_through() {
        assert_eq!(unpack(b"WEBVTT\n".to_vec()).unwrap(), b"WEBVTT\n");
    }
}
