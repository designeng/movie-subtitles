//! Offline speech synthesis for subtitle lines: Kokoro-82M (int8 ONNX) run by
//! sherpa-onnx, which also does the text-to-phoneme step with an English lexicon.
//! The model files are downloaded on first use.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig,
};
use tokio::io::AsyncWriteExt;
use tokio::sync::OnceCell;

use crate::error::{msg, Result};

/// Pinned revision of the `csukuangfj/kokoro-int8-multi-lang-v1_0` repository;
/// the hashes below belong to it.
const REVISION: &str = "57adb37fc0e8d4bd430cfc1b8f8c2294945b9af2";

/// The files needed for English speech, with their SHA-256.
const FILES: &[(&str, &str)] = &[
    ("model.int8.onnx", "75635ea3b85a32d00094dcdd61a258aba87c160f7c264c304a7b3ae9d734195e"),
    ("voices.bin", "1c5a5b983d3d50d8586d437a51f3faa2da7919ce76a013c081e65671a3447c29"),
    ("tokens.txt", "6ebb6bb288f20f3ae8d004d3c2ca27697da27c037d75e81a60e2a6a663f95425"),
    ("lexicon-us-en.txt", "7daaab53a181be9885b853a8582bf1838186317e5dadacbcef9c426d6fa0da14"),
];

/// American voices: name and speaker id inside `voices.bin` (voices are stored in
/// alphabetical order).
pub const VOICES: &[(&str, i32)] =
    &[("af_heart", 3), ("af_bella", 2), ("af_nicole", 6), ("am_adam", 11), ("am_michael", 16)];
pub const DEFAULT_VOICE: &str = "af_heart";

/// Lazily loaded speech engine, shared by all commands.
#[derive(Default)]
pub struct Tts {
    engine: OnceCell<Arc<OfflineTts>>,
}

impl Tts {
    /// Speaks `text` and returns a 16-bit mono WAV file.
    pub async fn synthesize(
        &self,
        data_dir: &Path,
        text: &str,
        voice: &str,
        speed: f32,
    ) -> Result<Vec<u8>> {
        let sid = VOICES
            .iter()
            .find(|(name, _)| *name == voice)
            .or_else(|| VOICES.iter().find(|(name, _)| *name == DEFAULT_VOICE))
            .map(|(_, sid)| *sid)
            .expect("default voice is listed");

        let engine = self
            .engine
            .get_or_try_init(|| async {
                let dir = ensure_model(&data_dir.join("tts")).await?;
                // Loading the model is CPU-heavy; keep it off the async threads.
                tokio::task::spawn_blocking(move || load(&dir))
                    .await
                    .map_err(|e| msg(e.to_string()))?
                    .map(Arc::new)
            })
            .await?
            .clone();

        let text = clean(text);
        if text.is_empty() {
            return Err(msg("Nothing to say in this line"));
        }
        let speed = speed.clamp(0.5, 1.5);
        tokio::task::spawn_blocking(move || {
            let config = GenerationConfig { sid, speed, ..Default::default() };
            let audio = engine
                .generate_with_config(&text, &config, None::<fn(&[f32], f32) -> bool>)
                .ok_or_else(|| msg("Speech synthesis failed"))?;
            Ok(wav(audio.samples(), audio.sample_rate() as u32))
        })
        .await
        .map_err(|e| msg(e.to_string()))?
    }
}

fn load(dir: &Path) -> Result<OfflineTts> {
    let path = |name: &str| Some(dir.join(name).to_string_lossy().into_owned());
    let config = OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            kokoro: OfflineTtsKokoroModelConfig {
                model: path("model.int8.onnx"),
                voices: path("voices.bin"),
                tokens: path("tokens.txt"),
                lexicon: path("lexicon-us-en.txt"),
                ..Default::default()
            },
            num_threads: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    OfflineTts::create(&config).ok_or_else(|| msg("Could not load the speech model"))
}

/// Subtitle decoration that shouldn't be pronounced: music notes, speaker dashes, newlines.
fn clean(text: &str) -> String {
    let text: String = text.chars().filter(|c| !matches!(c, '♪' | '♫' | '#')).collect();
    text.split_whitespace()
        .map(|w| w.trim_start_matches('-'))
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

// --- Audio ---------------------------------------------------------------------

/// Encodes mono float samples as a 16-bit PCM WAV file.
fn wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend(b"RIFF");
    out.extend((36 + data_len).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes()); // PCM
    out.extend(1u16.to_le_bytes()); // mono
    out.extend(sample_rate.to_le_bytes());
    out.extend((sample_rate * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data_len.to_le_bytes());
    for s in samples {
        out.extend(((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

// --- Downloads -----------------------------------------------------------------

/// Returns the folder holding the model files, downloading and verifying
/// whatever is missing.
async fn ensure_model(dir: &Path) -> Result<PathBuf> {
    for (name, sha256) in FILES {
        ensure_file(dir, name, sha256).await?;
    }
    Ok(dir.to_path_buf())
}

async fn ensure_file(dir: &Path, name: &str, sha256: &str) -> Result<()> {
    let path = dir.join(name);
    if path.is_file() {
        return Ok(());
    }
    let url = format!(
        "https://huggingface.co/csukuangfj/kokoro-int8-multi-lang-v1_0/resolve/{REVISION}/{name}"
    );
    let mut response = reqwest::get(&url).await?.error_for_status()?;
    tokio::fs::create_dir_all(dir).await?;
    // Stream to disk: the model is over 100 MB.
    let tmp = path.with_extension("download");
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut hasher = Sha256::new();
    while let Some(chunk) = response.chunk().await? {
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);

    let hash = format!("{:x}", hasher.finalize());
    if hash != sha256 {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(msg(format!(
            "{name} checksum mismatch (expected {sha256}, got {hash}); refusing to use it"
        )));
    }
    tokio::fs::rename(&tmp, &path).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_subtitle_decoration() {
        assert_eq!(clean("- Hello there.\n- ♪ La la ♪"), "Hello there. La la");
        assert_eq!(clean("♪♪"), "");
    }

    /// Downloads the model (~150 MB) into `TTS_TEST_DIR` and speaks a few lines.
    /// Run with `TTS_TEST_DIR=/some/dir cargo test speaks -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "downloads the speech model"]
    async fn speaks_lines() {
        let dir = PathBuf::from(std::env::var("TTS_TEST_DIR").expect("set TTS_TEST_DIR"));
        let tts = Tts::default();
        for (i, (text, speed)) in [
            ("I ain't gonna lie to you, mate.", 1.0),
            ("I ain't gonna lie to you, mate.", 0.7),
            ("Gotta bounce, Szymanski's waiting!", 1.0),
        ]
        .into_iter()
        .enumerate()
        {
            let started = std::time::Instant::now();
            let bytes = tts.synthesize(&dir, text, "af_heart", speed).await.unwrap();
            let samples: Vec<i16> =
                bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
            let secs = samples.len() as f32 / 24_000.0;
            let rms = (samples.iter().map(|&s| (s as f32).powi(2)).sum::<f32>()
                / samples.len() as f32)
                .sqrt();
            println!("{text:?} x{speed}: {secs:.2}s audio in {:?}, rms {rms:.0}", started.elapsed());
            assert!(secs > 0.5 && rms > 100.0);
            std::fs::write(dir.join(format!("out{i}.wav")), bytes).unwrap();
        }
    }

    #[test]
    fn wav_has_header_and_samples() {
        let bytes = wav(&[0.0, 1.0, -1.0], 24_000);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 24_000);
        assert_eq!(i16::from_le_bytes([bytes[46], bytes[47]]), i16::MAX);
    }
}
