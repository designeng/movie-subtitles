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

use super::{host_matches, DownloadProgress, ProgressFn, VideoInfo, VideoProvider};
use crate::error::{msg, Result};
use crate::tools;

/// Prefer H.264/AAC: WKWebView can't play VP9/AV1/Opus reliably.
const FORMAT_SORT: &str = "vcodec:h264,acodec:aac,res:1080";
const PROGRESS_TEMPLATE: &str = "download:[dl] %(progress.downloaded_bytes)s %(progress.total_bytes)s \
     %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s";

pub struct YtDlpProvider {
    id: &'static str,
    hosts: &'static [&'static str],
    bin_dir: PathBuf,
}

impl YtDlpProvider {
    pub fn new(id: &'static str, hosts: &'static [&'static str], bin_dir: &Path) -> Self {
        Self { id, hosts, bin_dir: bin_dir.to_path_buf() }
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

    async fn download(
        &self,
        url: &str,
        dest_dir: &Path,
        file_prefix: &str,
        on_progress: ProgressFn<'_>,
    ) -> Result<PathBuf> {
        on_progress(DownloadProgress::stage("Preparing yt-dlp"));
        let yt_dlp = tools::yt_dlp(&self.bin_dir).await?;
        let ffmpeg = tools::ffmpeg()?;
        let output = dest_dir.join(format!("{file_prefix}.%(ext)s"));

        let mut child = Command::new(yt_dlp)
            .arg("--no-playlist")
            .arg("--newline")
            .arg("--progress")
            .args(["--print", "after_move:[file] %(filepath)s"])
            .args(["--progress-template", PROGRESS_TEMPLATE])
            .args(["--progress-template", "postprocess:[pp] %(progress.postprocessor)s"])
            .args(["-S", FORMAT_SORT, "-f", "bv*+ba/b", "--merge-output-format", "mp4"])
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
