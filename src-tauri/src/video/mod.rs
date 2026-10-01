//! Video providers: resolve a page URL into metadata and a downloaded local file.

mod ytdlp;

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use async_trait::async_trait;
use regex::Regex;
use serde::Serialize;
use url::Url;

use crate::error::{msg, Result};

pub use ytdlp::YtDlpProvider;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub provider: String,
    pub id: String,
    pub title: String,
    pub duration: Option<f64>,
    pub webpage_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSearchResult {
    pub provider: String,
    pub url: String,
    pub title: String,
    pub channel: Option<String>,
    pub duration: Option<f64>,
    pub views: Option<u64>,
    pub thumbnail: Option<String>,
    /// The title hints at a dubbed (non-original language) version.
    pub likely_dubbed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub stage: String,
    pub percent: Option<f64>,
    pub speed_bps: Option<f64>,
    pub eta_secs: Option<f64>,
}

impl DownloadProgress {
    pub fn stage(stage: impl Into<String>) -> Self {
        Self { stage: stage.into(), percent: None, speed_bps: None, eta_secs: None }
    }
}

pub type ProgressFn<'a> = &'a (dyn Fn(DownloadProgress) + Send + Sync);

#[async_trait]
pub trait VideoProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn supports(&self, url: &Url) -> bool;
    async fn info(&self, url: &str) -> Result<VideoInfo>;
    /// Searches the site by title. Providers without search return nothing.
    async fn search(&self, _query: &str) -> Result<Vec<VideoSearchResult>> {
        Ok(Vec::new())
    }
    /// Downloads into `dest_dir`, preferring the highest resolution up to
    /// `max_height` (no limit when `None`). Every file it creates (including
    /// partial fragments) must be named `{file_prefix}*` so it can be cleaned up later.
    async fn download(
        &self,
        url: &str,
        dest_dir: &Path,
        file_prefix: &str,
        max_height: Option<u32>,
        on_progress: ProgressFn<'_>,
    ) -> Result<PathBuf>;
}

pub struct Registry {
    providers: Vec<Box<dyn VideoProvider>>,
}

impl Registry {
    pub fn new(bin_dir: &Path) -> Self {
        Self {
            providers: vec![
                Box::new(
                    YtDlpProvider::new("youtube", &["youtube.com", "youtu.be"], bin_dir)
                        // `sp` limits results to videos longer than 20 minutes.
                        .with_search("https://www.youtube.com/results?search_query={query}&sp=EgIYAg%3D%3D"),
                ),
                Box::new(YtDlpProvider::new("vk", &["vk.com", "vk.ru", "vkvideo.ru"], bin_dir)),
                Box::new(YtDlpProvider::new("ok", &["ok.ru", "odnoklassniki.ru"], bin_dir)),
            ],
        }
    }

    pub fn find(&self, url: &Url) -> Result<&dyn VideoProvider> {
        self.providers
            .iter()
            .find(|p| p.supports(url))
            .map(|p| p.as_ref())
            .ok_or_else(|| msg(format!("Unsupported video site: {}", url.host_str().unwrap_or("?"))))
    }

    /// Searches every provider that supports it; original-language,
    /// feature-length videos come first.
    pub async fn search(&self, query: &str) -> Result<Vec<VideoSearchResult>> {
        let mut all = Vec::new();
        let mut last_err = None;
        for provider in &self.providers {
            match provider.search(query).await {
                Ok(found) => all.extend(found),
                Err(e) => last_err = Some(e),
            }
        }
        if let Some(e) = last_err.filter(|_| all.is_empty()) {
            return Err(e);
        }
        for r in &mut all {
            r.likely_dubbed = looks_dubbed(&r.title);
        }
        rank(&mut all);
        Ok(all)
    }
}

const FEATURE_LENGTH_SECS: f64 = 60.0 * 60.0;

static DUBBED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\p{Cyrillic}|\b(dubbed|dub|dublado|doblaje|doblada|latino|en español|in hindi|hindi|tamil|telugu)\b")
        .unwrap()
});

/// Movies are wanted in their original (mostly English) language, so titles
/// in Cyrillic or mentioning a dub are likely not the original audio.
fn looks_dubbed(title: &str) -> bool {
    DUBBED_RE.is_match(title)
}

/// Stable sort: keeps the site's relevance order within each group.
fn rank(results: &mut [VideoSearchResult]) {
    results.sort_by_key(|r| (r.likely_dubbed, !r.duration.is_some_and(|d| d >= FEATURE_LENGTH_SECS)));
}

pub fn host_matches(url: &Url, hosts: &[&str]) -> bool {
    let Some(host) = url.host_str() else { return false };
    hosts.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_subdomains() {
        let hosts = ["youtube.com", "youtu.be"];
        assert!(host_matches(&Url::parse("https://www.youtube.com/watch?v=x").unwrap(), &hosts));
        assert!(host_matches(&Url::parse("https://youtu.be/x").unwrap(), &hosts));
        assert!(!host_matches(&Url::parse("https://notyoutube.com/x").unwrap(), &hosts));
    }

    #[test]
    fn detects_dubs() {
        assert!(looks_dubbed("Начало (2010) фильм"));
        assert!(looks_dubbed("Inception Hindi Dubbed Full Movie"));
        assert!(!looks_dubbed("Night of the Living Dead (1968) Full Movie | English"));
        assert!(!looks_dubbed("Dubai Nights"));
    }

    #[test]
    fn ranks_original_feature_length_first() {
        let r = |title: &str, mins: f64| VideoSearchResult {
            provider: "youtube".into(),
            url: title.into(),
            title: title.into(),
            channel: None,
            duration: Some(mins * 60.0),
            views: None,
            thumbnail: None,
            likely_dubbed: looks_dubbed(title),
        };
        let mut results = vec![r("Review", 25.0), r("Фильм", 95.0), r("Movie A", 95.0), r("Movie B", 90.0)];
        rank(&mut results);
        let titles: Vec<_> = results.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["Movie A", "Movie B", "Review", "Фильм"]);
    }
}
