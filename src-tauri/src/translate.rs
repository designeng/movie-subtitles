//! Subtitle translation. Other engines (DeepL, local models) can implement `Translator`.
//!
//! - `GoogleTranslate`: official Cloud Translation v2, needs an API key.
//! - `GoogleTranslateFree`: the keyless `translate.googleapis.com` endpoint used by
//!   browser extensions. Unofficial and rate-limited; fine for personal use only.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use crate::error::{msg, Result};

#[async_trait]
pub trait Translator: Send + Sync {
    /// Translates each text into `target` (ISO 639-1), preserving order.
    async fn translate(&self, texts: &[String], target: &str) -> Result<Vec<String>>;
}

pub struct GoogleTranslate {
    client: Client,
    api_key: String,
}

/// API limits: 128 segments per request; keep the payload well under 30k chars.
const MAX_SEGMENTS: usize = 128;
const MAX_CHARS: usize = 20_000;

impl GoogleTranslate {
    pub fn new(api_key: String) -> Self {
        Self { client: Client::new(), api_key }
    }

    async fn translate_batch(&self, batch: &[String], target: &str) -> Result<Vec<String>> {
        let resp = self
            .client
            .post("https://translation.googleapis.com/language/translate/v2")
            .query(&[("key", &self.api_key)])
            .json(&json!({ "q": batch, "target": target, "format": "text" }))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(msg(format!(
                "Google Translate failed ({}): {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        let body: Response = resp.json().await?;
        let out: Vec<String> = body.data.translations.into_iter().map(|t| t.translated_text).collect();
        if out.len() != batch.len() {
            return Err(msg("Google Translate returned an unexpected number of lines"));
        }
        Ok(out)
    }
}

#[async_trait]
impl Translator for GoogleTranslate {
    async fn translate(&self, texts: &[String], target: &str) -> Result<Vec<String>> {
        let mut out = Vec::with_capacity(texts.len());
        for batch in batches(texts) {
            out.extend(self.translate_batch(batch, target).await?);
        }
        Ok(out)
    }
}

pub struct GoogleTranslateFree {
    client: Client,
}

/// Text goes in the query string, so keep URLs short; the endpoint also
/// throttles large or rapid requests.
const FREE_MAX_CHARS: usize = 1_500;
const FREE_DELAY: std::time::Duration = std::time::Duration::from_millis(250);

impl GoogleTranslateFree {
    pub fn new() -> Self {
        // Requires the platform TLS stack (reqwest `native-tls`): Google answers
        // rustls connections to this endpoint with a 429 bot check.
        let client = Client::builder()
            .user_agent(concat!("MovieSubtitles/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// Translates newline-separated text, returning the translated text.
    async fn translate_text(&self, text: &str, target: &str) -> Result<String> {
        let resp = self
            .client
            .get("https://translate.googleapis.com/translate_a/single")
            .query(&[("client", "gtx"), ("sl", "auto"), ("tl", target), ("dt", "t"), ("q", text)])
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(msg(format!(
                "Free Google Translate failed ({status}). It may be rate-limited; try again later \
                 or add an API key in Settings.\n{}",
                body.chars().take(300).collect::<String>()
            )));
        }
        // Response: [[["translated", "original", ...], ...], ...]
        let body: serde_json::Value = resp.json().await?;
        let segments = body[0].as_array().ok_or_else(|| msg("Unexpected Google Translate response"))?;
        Ok(segments.iter().filter_map(|seg| seg[0].as_str()).collect())
    }
}

#[async_trait]
impl Translator for GoogleTranslateFree {
    async fn translate(&self, texts: &[String], target: &str) -> Result<Vec<String>> {
        // One cue per line: inner line breaks become spaces.
        let lines: Vec<String> = texts.iter().map(|t| t.replace('\n', " ")).collect();
        let mut out = Vec::with_capacity(lines.len());
        for (i, batch) in batches_with(&lines, MAX_SEGMENTS, FREE_MAX_CHARS).into_iter().enumerate() {
            if i > 0 {
                tokio::time::sleep(FREE_DELAY).await;
            }
            let translated = self.translate_text(&batch.join("\n"), target).await?;
            let parts: Vec<String> = translated.split('\n').map(|l| l.trim().to_string()).collect();
            if parts.len() == batch.len() {
                out.extend(parts);
            } else {
                // Line structure got lost; fall back to one request per cue.
                for line in batch {
                    tokio::time::sleep(FREE_DELAY).await;
                    out.push(self.translate_text(line, target).await?.trim().to_string());
                }
            }
        }
        Ok(out)
    }
}

fn batches(texts: &[String]) -> Vec<&[String]> {
    batches_with(texts, MAX_SEGMENTS, MAX_CHARS)
}

fn batches_with(texts: &[String], max_segments: usize, max_chars: usize) -> Vec<&[String]> {
    let mut result = Vec::new();
    let (mut start, mut chars) = (0, 0);
    for (i, text) in texts.iter().enumerate() {
        if i > start && (i - start == max_segments || chars + text.len() > max_chars) {
            result.push(&texts[start..i]);
            (start, chars) = (i, 0);
        }
        chars += text.len();
    }
    if start < texts.len() {
        result.push(&texts[start..]);
    }
    result
}

#[derive(Deserialize)]
struct Response {
    data: Data,
}

#[derive(Deserialize)]
struct Data {
    translations: Vec<Translation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Translation {
    translated_text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_by_count_and_size() {
        let texts: Vec<String> = (0..300).map(|_| "x".repeat(10)).collect();
        let sizes: Vec<usize> = batches(&texts).iter().map(|b| b.len()).collect();
        assert_eq!(sizes, [128, 128, 44]);

        let big: Vec<String> = (0..5).map(|_| "x".repeat(8_000)).collect();
        let sizes: Vec<usize> = batches(&big).iter().map(|b| b.len()).collect();
        assert_eq!(sizes, [2, 2, 1]);
        assert!(batches(&[]).is_empty());
    }

    /// Hits the real endpoint: `cargo test -- --ignored free_translate`.
    #[tokio::test]
    #[ignore]
    async fn free_translate_keeps_line_alignment() {
        let texts: Vec<String> = ["Where are you going?", "I don't know.\nMaybe home.", "Wait!", "OK."]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let out = GoogleTranslateFree::new().translate(&texts, "ru").await.unwrap();
        println!("{out:?}");
        assert_eq!(out.len(), texts.len());
        assert!(out.iter().all(|l| !l.is_empty()));
    }
}
