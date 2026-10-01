//! SRT / WebVTT parsing and SRT serialization.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

/// Decodes subtitle bytes: UTF-8 (with or without BOM), falling back to Windows-1252.
pub fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

/// Parses SRT or WebVTT. Blocks without a `-->` timing line are ignored.
pub fn parse(content: &str) -> Vec<Cue> {
    let content = content.replace("\r\n", "\n").replace('\r', "\n");
    let mut cues = Vec::new();
    let mut lines = content.lines();

    while let Some(line) = lines.next() {
        let Some((start, end)) = line.split_once("-->") else { continue };
        // VTT may append cue settings after the end timestamp.
        let end = end.split_whitespace().next().unwrap_or("");
        let (Some(start_ms), Some(end_ms)) = (parse_timestamp(start.trim()), parse_timestamp(end))
        else {
            continue;
        };

        let text: Vec<&str> = lines.by_ref().take_while(|l| !l.trim().is_empty()).collect();
        let text = clean_text(&text.join("\n"));
        if !text.is_empty() {
            cues.push(Cue { start_ms, end_ms, text });
        }
    }

    cues.sort_by_key(|c| c.start_ms);
    cues
}

/// Accepts `HH:MM:SS,mmm`, `HH:MM:SS.mmm` and `MM:SS.mmm`.
fn parse_timestamp(s: &str) -> Option<i64> {
    let (hms, frac) = s.split_once([',', '.']).unwrap_or((s, "0"));
    let parts: Vec<i64> = hms.split(':').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let (h, m, sec) = match parts[..] {
        [h, m, s] => (h, m, s),
        [m, s] => (0, m, s),
        _ => return None,
    };
    // Normalize fractions like "5" or "5000" to milliseconds.
    let frac = format!("{:0<3}", frac.trim());
    let ms: i64 = frac.get(..3)?.parse().ok()?;
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

static TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>|\{\\[^}]*\}").unwrap());

/// Strips HTML-like and ASS override tags (`<i>`, `{\an8}`).
fn clean_text(s: &str) -> String {
    TAG_RE.replace_all(s, "").trim().to_string()
}

/// Serializes cues to SRT with every timestamp shifted by `offset_ms`.
/// Cues pushed entirely before zero are dropped.
pub fn to_srt(cues: &[Cue], offset_ms: i64) -> String {
    let mut out = String::new();
    let shifted = cues
        .iter()
        .map(|c| (c.start_ms + offset_ms, c.end_ms + offset_ms, &c.text))
        .filter(|(_, end, _)| *end > 0);
    for (i, (start, end, text)) in shifted.enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            format_timestamp(start.max(0)),
            format_timestamp(end),
            text
        ));
    }
    out
}

fn format_timestamp(ms: i64) -> String {
    let (h, rest) = (ms / 3_600_000, ms % 3_600_000);
    let (m, rest) = (rest / 60_000, rest % 60_000);
    format!("{h:02}:{m:02}:{:02},{:03}", rest / 1000, rest % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRT: &str = "1\r\n00:00:01,500 --> 00:00:03,000\r\n<i>Hello</i>\r\nthere\r\n\r\n\
                       2\r\n00:01:02,005 --> 00:01:04,250\r\n{\\an8}Second\r\n";

    #[test]
    fn parses_srt() {
        let cues = parse(SRT);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0], Cue { start_ms: 1500, end_ms: 3000, text: "Hello\nthere".into() });
        assert_eq!(cues[1].start_ms, 62_005);
        assert_eq!(cues[1].text, "Second");
    }

    #[test]
    fn parses_vtt() {
        let vtt = "WEBVTT\n\nNOTE comment\n\n00:05.250 --> 00:07.000 align:start\nHi\n";
        assert_eq!(parse(vtt), vec![Cue { start_ms: 5250, end_ms: 7000, text: "Hi".into() }]);
    }

    #[test]
    fn shifts_on_export() {
        let cues = parse(SRT);
        let srt = to_srt(&cues, -2000);
        assert!(srt.starts_with("1\n00:00:00,000 --> 00:00:01,000\nHello\nthere\n"));
        assert!(srt.contains("00:01:00,005 --> 00:01:02,250"));
        assert_eq!(parse(&to_srt(&cues, 0)), cues);
    }

    #[test]
    fn decodes_latin1_fallback() {
        assert_eq!(decode(b"caf\xe9"), "café");
        assert_eq!(decode("\u{feff}ok".as_bytes()), "ok");
    }
}
