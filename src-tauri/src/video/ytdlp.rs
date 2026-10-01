//! Generic provider backed by yt-dlp; one instance per supported site.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use url::Url;

use super::{host_matches, DownloadProgress, ProgressFn, VideoInfo, VideoProvider, VideoSearchResult};
use crate::error::{msg, Result};
use crate::tools;

/// Sort order after resolution.
const FORMAT_SORT_TAIL: &str = "fps,vcodec:h264,acodec:aac";
/// WKWebView plays MP4 files through AVFoundation, which on older macOS (12 and earlier)
/// decodes neither VP9 nor Opus, and AV1 only with a hardware decoder (M3+). So prefer
/// H.264 + AAC, then muxed formats of unknown codecs that are not VP9/AV1/Opus (ok.ru's
/// HLS streams), and fall back to anything only when nothing else exists.
const FORMAT: &str = "bv*[vcodec~='^(avc|h264)']+ba[acodec~='^(mp4a|aac)']\
     /b[vcodec!~=?'^(vp|av01)'][acodec!~=?'^(opus|vorbis)']\
     /bv*[vcodec!^=av01]+ba/b";
const PROGRESS_TEMPLATE: &str = "download:[dl] %(progress.downloaded_bytes)s %(progress.total_bytes)s \
     %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s";
const SEARCH_LIMIT: &str = "20";

pub struct YtDlpProvider {
    id: &'static str,
    hosts: &'static [&'static str],
    bin_dir: PathBuf,
    /// Search results page URL with a `{query}` placeholder.
    search_url: Option<&'static str>,
}

impl YtDlpProvider {
    pub fn new(id: &'static str, hosts: &'static [&'static str], bin_dir: &Path) -> Self {
        Self { id, hosts, bin_dir: bin_dir.to_path_buf(), search_url: None }
    }

    pub fn with_search(mut self, url_template: &'static str) -> Self {
        self.search_url = Some(url_template);
        self
    }

    fn search_result(&self, entry: &Value) -> Option<VideoSearchResult> {
        Some(VideoSearchResult {
            provider: self.id.to_string(),
            url: entry["url"].as_str()?.to_string(),
            title: entry["title"].as_str()?.to_string(),
            channel: entry["channel"].as_str().map(String::from),
            duration: entry["duration"].as_f64(),
            views: entry["view_count"].as_u64(),
            thumbnail: entry["thumbnails"]
                .as_array()
                .and_then(|t| t.last())
                .and_then(|t| t["url"].as_str())
                .map(String::from),
            likely_dubbed: false,
        })
    }
}

#[async_trait]
impl VideoProvider for YtDlpProvider {
    fn id(&self) -> &'static str {
        self.id
    }

    fn supports(&self, url: &Url) -> bool {
        host_matches(url, self.hosts)
    }

    async fn info(&self, url: &str) -> Result<VideoInfo> {
        let out = Command::new(tools::yt_dlp(&self.bin_dir).await?)
            .args(["-J", "--no-playlist", "--no-warnings", url])
            .output()
            .await?;
        if !out.status.success() {
            return Err(msg(format!(
                "yt-dlp could not read the video: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let v: Value = serde_json::from_slice(&out.stdout)?;
        let id = v["id"].as_str().ok_or_else(|| msg("yt-dlp returned no video id"))?;
        Ok(VideoInfo {
            provider: self.id.to_string(),
            id: id.to_string(),
            title: v["title"].as_str().unwrap_or(id).to_string(),
            duration: v["duration"].as_f64(),
            webpage_url: v["webpage_url"].as_str().unwrap_or(url).to_string(),
        })
    }

    async fn search(&self, query: &str) -> Result<Vec<VideoSearchResult>> {
        let Some(template) = self.search_url else { return Ok(Vec::new()) };
        let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
        let out = Command::new(tools::yt_dlp(&self.bin_dir).await?)
            .args(["-J", "--flat-playlist", "--no-warnings", "--playlist-end", SEARCH_LIMIT])
            .arg(template.replace("{query}", &encoded))
            .output()
            .await?;
        if !out.status.success() {
            return Err(msg(format!(
                "yt-dlp search failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let v: Value = serde_json::from_slice(&out.stdout)?;
        let entries = v["entries"].as_array().map(Vec::as_slice).unwrap_or_default();
        Ok(entries.iter().filter_map(|e| self.search_result(e)).collect())
    }

    async fn download(
        &self,
        url: &str,
        dest_dir: &Path,
        file_prefix: &str,
        max_height: Option<u32>,
        on_progress: ProgressFn<'_>,
    ) -> Result<PathBuf> {
        on_progress(DownloadProgress::stage("Preparing yt-dlp"));
        let yt_dlp = tools::yt_dlp(&self.bin_dir).await?;
        let ffmpeg = tools::ffmpeg()?;
        let output = dest_dir.join(format!("{file_prefix}.%(ext)s"));
        // `res:N` prefers the largest resolution not above N, falling back to larger ones.
        let format_sort = match max_height {
            Some(h) => format!("res:{h},{FORMAT_SORT_TAIL}"),
            None => format!("res,{FORMAT_SORT_TAIL}"),
        };

        let mut child = Command::new(yt_dlp)
            .arg("--no-playlist")
            .arg("--newline")
            .arg("--progress")
            .args(["--print", "after_move:[file] %(filepath)s"])
            .args(["--progress-template", PROGRESS_TEMPLATE])
            .args(["--progress-template", "postprocess:[pp] %(progress.postprocessor)s"])
            .args(["-S", &format_sort, "-f", FORMAT, "--merge-output-format", "mp4"])
            .arg("--ffmpeg-location")
            .arg(ffmpeg)
            .arg("-o")
            .arg(output)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let (tx, mut rx) = mpsc::unbounded_channel();
        forward_lines(child.stdout.take(), tx.clone());
        forward_lines(child.stderr.take(), tx);

        let mut file = None;
        let mut log_tail = VecDeque::new();
        on_progress(DownloadProgress::stage("Downloading"));
        while let Some(line) = rx.recv().await {
            if let Some(rest) = line.strip_prefix("[dl] ") {
                on_progress(parse_progress(rest));
            } else if line.starts_with("[pp] ") {
                on_progress(DownloadProgress::stage("Merging video and audio"));
            } else if let Some(path) = line.strip_prefix("[file] ") {
                file = Some(PathBuf::from(path.trim()));
            } else if !line.trim().is_empty() {
                if log_tail.len() == 8 {
                    log_tail.pop_front();
                }
                log_tail.push_back(line);
            }
        }

        let status = child.wait().await?;
        if !status.success() {
            let log: Vec<_> = log_tail.into_iter().collect();
            return Err(msg(format!("yt-dlp failed: {}", log.join("\n"))));
        }
        file.filter(|p| p.is_file())
            .ok_or_else(|| msg("yt-dlp finished but the downloaded file was not found"))
    }
}

fn forward_lines<R>(reader: Option<R>, tx: mpsc::UnboundedSender<String>)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let Some(reader) = reader else { return };
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
}

/// Parses `downloaded total total_estimate speed eta`, where missing values are `NA`.
fn parse_progress(s: &str) -> DownloadProgress {
    let nums: Vec<Option<f64>> = s.split_whitespace().map(|t| t.parse().ok()).collect();
    let get = |i: usize| nums.get(i).copied().flatten();
    let total = get(1).or(get(2));
    DownloadProgress {
        stage: "Downloading".into(),
        percent: match (get(0), total) {
            (Some(done), Some(total)) if total > 0.0 => Some((done / total * 100.0).min(100.0)),
            _ => None,
        },
        speed_bps: get(3),
        eta_secs: get(4),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_line() {
        let p = parse_progress("154644 309288 NA 13595136.19 3");
        assert_eq!(p.percent, Some(50.0));
        assert_eq!(p.eta_secs, Some(3.0));

        let p = parse_progress("1024 NA 2048 NA NA");
        assert_eq!(p.percent, Some(50.0));
        assert_eq!(p.speed_bps, None);
    }
}
