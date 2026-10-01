//! Serves library videos to the player over plain HTTP on the loopback interface.
//!
//! WKWebView plays media from custom URL schemes (Tauri's `asset://`) unreliably: on macOS
//! the video track may play while the audio stays silent. A real `http://127.0.0.1` URL goes
//! through WebKit's regular media loader, so it behaves like any other web page.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::SeekFrom;
use std::path::Path;

use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use crate::AppState;

const MAX_HEADER_BYTES: usize = 16 * 1024;

pub struct MediaServer {
    /// `http://127.0.0.1:<port>/<token>`; videos are served at `<base>/<entry id>`.
    pub base_url: String,
}

impl MediaServer {
    /// Binds a random loopback port and serves on the Tauri async runtime.
    pub fn start(app: AppHandle) -> std::io::Result<Self> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        // Other local processes and web pages can reach the port, so require an unguessable path.
        let token = format!("{:016x}{:016x}", random_u64(), random_u64());
        let prefix = format!("/{token}/");
        tauri::async_runtime::spawn(async move {
            let listener = match TcpListener::from_std(listener) {
                Ok(l) => l,
                Err(e) => return eprintln!("media server: {e}"),
            };
            loop {
                let Ok((stream, _)) = listener.accept().await else { continue };
                let (app, prefix) = (app.clone(), prefix.clone());
                tokio::spawn(async move {
                    let _ = serve_connection(stream, &app, &prefix).await;
                });
            }
        });
        Ok(Self { base_url: format!("http://127.0.0.1:{port}/{token}") })
    }
}

fn random_u64() -> u64 {
    RandomState::new().build_hasher().finish()
}

struct Request {
    head_only: bool,
    path: String,
    range: Option<String>,
}

/// Handles keep-alive requests on one connection until the client closes it.
async fn serve_connection(stream: TcpStream, app: &AppHandle, prefix: &str) -> std::io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    while let Some(req) = read_request(&mut reader).await? {
        let entry_id = req
            .path
            .split('?')
            .next()
            .and_then(|p| p.strip_prefix(prefix))
            .map(percent_decode);
        let video_path = entry_id.and_then(|id| {
            let state = app.state::<AppState>();
            let library = state.library.lock().unwrap();
            library.get(&id).ok().map(|e| e.video_path.clone())
        });
        match video_path {
            Some(path) => send_file(&mut writer, &path, &req).await?,
            None => send_status(&mut writer, "404 Not Found").await?,
        }
    }
    Ok(())
}

/// Returns `None` when the client closed the connection.
async fn read_request<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> std::io::Result<Option<Request>> {
    let mut line = String::new();
    let mut total = 0;
    let mut request_line = None;
    let mut range = None;
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(None);
        }
        total += n;
        if total > MAX_HEADER_BYTES {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "headers too large"));
        }
        let trimmed = line.trim_end();
        if request_line.is_none() {
            if !trimmed.is_empty() {
                request_line = Some(trimmed.to_string());
            }
            continue;
        }
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.eq_ignore_ascii_case("range") {
                range = Some(value.trim().to_string());
            }
        }
    }
    let request_line = request_line.unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default().to_string();
    if method != "GET" && method != "HEAD" {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported method"));
    }
    Ok(Some(Request { head_only: method == "HEAD", path, range }))
}

async fn send_file<W: AsyncWriteExt + Unpin>(writer: &mut W, path: &Path, req: &Request) -> std::io::Result<()> {
    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return send_status(writer, "404 Not Found").await;
    };
    let len = file.metadata().await?.len();
    let content_type = content_type(path);
    let (status, start, end) = match req.range.as_deref() {
        None => ("200 OK", 0, len.saturating_sub(1)),
        Some(range) => match parse_range(range, len) {
            Some((start, end)) => ("206 Partial Content", start, end),
            None => {
                let head = format!(
                    "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\n\r\n"
                );
                return writer.write_all(head.as_bytes()).await;
            }
        },
    };
    let body_len = if len == 0 { 0 } else { end - start + 1 };
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {body_len}\r\n\
         Accept-Ranges: bytes\r\nCache-Control: no-store\r\n"
    );
    if req.range.is_some() {
        head.push_str(&format!("Content-Range: bytes {start}-{end}/{len}\r\n"));
    }
    head.push_str("\r\n");
    writer.write_all(head.as_bytes()).await?;
    if !req.head_only && body_len > 0 {
        file.seek(SeekFrom::Start(start)).await?;
        tokio::io::copy(&mut file.take(body_len), writer).await?;
    }
    writer.flush().await
}

async fn send_status<W: AsyncWriteExt + Unpin>(writer: &mut W, status: &str) -> std::io::Result<()> {
    let head = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n");
    writer.write_all(head.as_bytes()).await
}

/// Parses a single `bytes=` range into inclusive offsets; multi-range requests get the first range.
fn parse_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (start, end) = spec.split_once('-')?;
    if len == 0 {
        return None;
    }
    let (start, end) = if start.is_empty() {
        let suffix: u64 = end.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        (len.saturating_sub(suffix), len - 1)
    } else {
        let start: u64 = start.parse().ok()?;
        let end = if end.is_empty() { len - 1 } else { end.parse::<u64>().ok()?.min(len - 1) };
        (start, end)
    };
    (start <= end && start < len).then_some((start, end))
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("mp4" | "m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        _ => "application/octet-stream",
    }
}

fn percent_decode(s: &str) -> String {
    url::form_urlencoded::parse(format!("x={s}").as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{parse_range, read_request, send_file};

    async fn respond(raw: &str, path: &std::path::Path) -> String {
        let mut reader = tokio::io::BufReader::new(raw.as_bytes());
        let req = read_request(&mut reader).await.unwrap().unwrap();
        let mut out = Vec::new();
        send_file(&mut out, path, &req).await.unwrap();
        String::from_utf8(out).unwrap()
    }

    #[tokio::test]
    async fn serves_ranges() {
        let path = std::env::temp_dir().join(format!("media-server-test-{}.mp4", std::process::id()));
        std::fs::write(&path, b"0123456789").unwrap();

        let full = respond("GET /t/id HTTP/1.1\r\nHost: x\r\n\r\n", &path).await;
        assert!(full.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(full.contains("Content-Type: video/mp4\r\n"));
        assert!(full.ends_with("\r\n\r\n0123456789"));

        let part = respond("GET /t/id HTTP/1.1\r\nRange: bytes=2-4\r\n\r\n", &path).await;
        assert!(part.starts_with("HTTP/1.1 206 Partial Content\r\n"));
        assert!(part.contains("Content-Range: bytes 2-4/10\r\n"));
        assert!(part.contains("Content-Length: 3\r\n"));
        assert!(part.ends_with("\r\n\r\n234"));

        let head = respond("HEAD /t/id HTTP/1.1\r\n\r\n", &path).await;
        assert!(head.contains("Content-Length: 10\r\n") && head.ends_with("\r\n\r\n"));

        let bad = respond("GET /t/id HTTP/1.1\r\nrange: bytes=20-\r\n\r\n", &path).await;
        assert!(bad.starts_with("HTTP/1.1 416 "));

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-", 100), Some((0, 99)));
        assert_eq!(parse_range("bytes=0-1", 100), Some((0, 1)));
        assert_eq!(parse_range("bytes=50-500", 100), Some((50, 99)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=-500", 100), Some((0, 99)));
        assert_eq!(parse_range("bytes=100-", 100), None);
        assert_eq!(parse_range("bytes=5-2", 100), None);
        assert_eq!(parse_range("items=0-1", 100), None);
    }
}
