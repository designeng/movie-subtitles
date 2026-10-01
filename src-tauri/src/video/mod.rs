//! Video providers: resolve a page URL into metadata and a downloaded local file.

mod ytdlp;

use std::path::{Path, PathBuf};

use async_trait::async_trait;
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
    /// Downloads into `dest_dir`. Every file it creates (including partial
    /// fragments) must be named `{file_prefix}*` so it can be cleaned up later.
    async fn download(
        &self,
        url: &str,
        dest_dir: &Path,
        file_prefix: &str,
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
                Box::new(YtDlpProvider::new("youtube", &["youtube.com", "youtu.be"], bin_dir)),
                Box::new(YtDlpProvider::new("vk", &["vk.com", "vk.ru", "vkvideo.ru"], bin_dir)),
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
}
