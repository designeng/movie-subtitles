//! Locating and provisioning external binaries (yt-dlp, ffmpeg).

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{msg, Result};

/// Pinned yt-dlp release. To update: bump the version and paste the hash of
/// `yt-dlp_macos` from that release's `SHA2-256SUMS` file.
/// Nightly build: the OK.ru extractor fix (yt-dlp#17739) isn't in a stable release yet.
/// Switch back to `yt-dlp/yt-dlp` once a stable release newer than 2026.09.27 ships.
const YT_DLP_REPO: &str = "yt-dlp/yt-dlp-nightly-builds";
const YT_DLP_VERSION: &str = "2026.09.27.232945";
const YT_DLP_MACOS_SHA256: &str =
    "1b1a6420f23af38b4eff225d77e3839d0f5630e1e3a3e913bd29818d70b252df";

/// GUI apps on macOS don't inherit the shell PATH, so Homebrew dirs are checked explicitly.
const SEARCH_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

fn env_override(name: &str) -> Option<PathBuf> {
    let key = format!("MOVIE_SUBTITLES_{}", name.to_uppercase().replace('-', "_"));
    std::env::var_os(key).map(PathBuf::from).filter(|p| p.is_file())
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = SEARCH_DIRS.iter().map(PathBuf::from).collect();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}

pub fn ffmpeg() -> Result<PathBuf> {
    env_override("ffmpeg")
        .or_else(|| find_in_path("ffmpeg"))
        .ok_or_else(|| msg("ffmpeg not found. Install it with `brew install ffmpeg`."))
}

/// Returns a verified yt-dlp binary, downloading the pinned release on first use.
/// `MOVIE_SUBTITLES_YT_DLP` overrides the managed binary.
pub async fn yt_dlp(bin_dir: &Path) -> Result<PathBuf> {
    if let Some(p) = env_override("yt-dlp") {
        return Ok(p);
    }
    let path = bin_dir.join(format!("yt-dlp-{YT_DLP_VERSION}"));
    if path.is_file() {
        return Ok(path);
    }

    let url = format!(
        "https://github.com/{YT_DLP_REPO}/releases/download/{YT_DLP_VERSION}/yt-dlp_macos"
    );
    let bytes = reqwest::get(&url).await?.error_for_status()?.bytes().await?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if hash != YT_DLP_MACOS_SHA256 {
        return Err(msg(format!(
            "yt-dlp checksum mismatch (expected {YT_DLP_MACOS_SHA256}, got {hash}); refusing to use it"
        )));
    }

    tokio::fs::create_dir_all(bin_dir).await?;
    let tmp = path.with_extension("download");
    tokio::fs::write(&tmp, &bytes).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).await?;
    }
    tokio::fs::rename(&tmp, &path).await?;
    Ok(path)
}
