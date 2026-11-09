//! Offline speech synthesis for subtitle lines: Kokoro-82M (ONNX) run by
//! sherpa-onnx, which also does the text-to-phoneme step with an English lexicon.
//! The model files are downloaded on first use.
//!
//! Synthesis is slower than real time on older CPUs, so finished lines are
//! cached and the lines likely to be asked for next are rendered in the background.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig,
};
use tokio::io::AsyncWriteExt;
use tokio::sync::OnceCell;

use crate::error::{msg, Result};

/// Pinned revision of the `csukuangfj/kokoro-multi-lang-v1_0` repository;
/// the hashes below belong to it.
const REVISION: &str = "f7b96bb6bef5c5da4d3aa4f4e0498fbbf62dc78b";

/// The full-precision model. The int8 one is smaller but about twice as slow on
/// CPUs without VNNI instructions (measured on a Broadwell i5), because
/// onnxruntime falls back to slow int8 kernels there.
const MODEL: &str = "model.onnx";

/// The files needed for English speech, with their SHA-256.
const FILES: &[(&str, &str)] = &[
    (MODEL, "b40f62b166ac8164b0627ef48a0b358eda0985e272fb03ef5252e7206305da11"),
    ("voices.bin", "1c5a5b983d3d50d8586d437a51f3faa2da7919ce76a013c081e65671a3447c29"),
    ("tokens.txt", "6ebb6bb288f20f3ae8d004d3c2ca27697da27c037d75e81a60e2a6a663f95425"),
    ("lexicon-us-en.txt", "7daaab53a181be9885b853a8582bf1838186317e5dadacbcef9c426d6fa0da14"),
];

/// Model files of earlier versions, deleted once the current model is in place.
const STALE_FILES: &[&str] = &["model.int8.onnx"];

/// American voices: name and speaker id inside `voices.bin` (voices are stored in
/// alphabetical order).
pub const VOICES: &[(&str, i32)] =
    &[("af_heart", 3), ("af_bella", 2), ("af_nicole", 6), ("am_adam", 11), ("am_michael", 16)];
pub const DEFAULT_VOICE: &str = "af_heart";

/// How many rendered lines to keep; a line is ~50 KB per second of speech.
const CACHE_SIZE: usize = 64;

/// One line to render: cleaned text, speaker and speed (in percent, to be hashable).
#[derive(Clone, PartialEq, Eq, Debug)]
struct Line {
    text: String,
    sid: i32,
    speed: u16,
}

impl Line {
    fn new(text: &str, voice: &str, speed: f32) -> Self {
        let sid = VOICES
            .iter()
            .find(|(name, _)| *name == voice)
            .or_else(|| VOICES.iter().find(|(name, _)| *name == DEFAULT_VOICE))
            .map(|(_, sid)| *sid)
            .expect("default voice is listed");
        let speed = (speed.clamp(0.5, 1.5) * 100.0).round() as u16;
        Self { text: clean(text), sid, speed }
    }
}

/// Speech engine shared by all commands: loaded lazily, renders one line at a time.
pub struct Tts {
    inner: Arc<Inner>,
}

struct Inner {
    dir: PathBuf,
    engine: OnceCell<Arc<OfflineTts>>,
    /// Held while a line is rendered. Rendering uses all cores, so running two at
    /// once only slows both down. Tokio's mutex is fair: a line the user clicked
    /// goes ahead of the next background line.
    rendering: tokio::sync::Mutex<()>,
    /// Most recently used last.
    cache: Mutex<VecDeque<(Line, Vec<u8>)>>,
    /// Lines to render in the background, in order.
    wanted: Mutex<VecDeque<Line>>,
    prefetching: AtomicBool,
}

impl Tts {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            inner: Arc::new(Inner {
                dir: data_dir.join("tts"),
                engine: OnceCell::new(),
                rendering: tokio::sync::Mutex::new(()),
                cache: Mutex::new(VecDeque::new()),
                wanted: Mutex::new(VecDeque::new()),
                prefetching: AtomicBool::new(false),
            }),
        }
    }

    /// Speaks `text` and returns a 16-bit mono WAV file. The first call downloads
    /// the model if needed.
    pub async fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<Vec<u8>> {
        let line = Line::new(text, voice, speed);
        if line.text.is_empty() {
            return Err(msg("Nothing to say in this line"));
        }
        self.inner.render(&line).await
    }

    /// Loads the model in the background if it is already downloaded, so that
    /// the first line doesn't wait for it. Never downloads anything.
    pub fn warm_up(&self) {
        let inner = self.inner.clone();
        if inner.engine.initialized() || !installed(&inner.dir) {
            return;
        }
        tauri::async_runtime::spawn(async move {
            if let Err(e) = inner.engine().await {
                eprintln!("speech model warm-up failed: {e}");
            }
        });
    }

    /// Renders `texts` in the background, replacing whatever was queued before.
    /// Does nothing until the model has been loaded by a real request or a warm-up.
    pub fn prefetch(&self, texts: &[String], voice: &str, speed: f32) {
        let inner = self.inner.clone();
        if !inner.engine.initialized() {
            return;
        }
        let lines: VecDeque<Line> = texts
            .iter()
            .map(|t| Line::new(t, voice, speed))
            .filter(|l| !l.text.is_empty())
            .collect();
        *inner.wanted.lock().unwrap() = lines;
        if !inner.prefetching.swap(true, Ordering::AcqRel) {
            tauri::async_runtime::spawn(inner.prefetch_loop());
        }
    }
}

impl Inner {
    async fn engine(&self) -> Result<Arc<OfflineTts>> {
        self.engine
            .get_or_try_init(|| async {
                let dir = ensure_model(&self.dir).await?;
                // Loading the model is CPU-heavy; keep it off the async threads.
                tokio::task::spawn_blocking(move || load(&dir))
                    .await
                    .map_err(|e| msg(e.to_string()))?
                    .map(Arc::new)
            })
            .await
            .cloned()
    }

    fn cached(&self, line: &Line) -> Option<Vec<u8>> {
        let mut cache = self.cache.lock().unwrap();
        let pos = cache.iter().position(|(l, _)| l == line)?;
        let entry = cache.remove(pos)?;
        let wav = entry.1.clone();
        cache.push_back(entry);
        Some(wav)
    }

    fn store(&self, line: Line, wav: Vec<u8>) {
        let mut cache = self.cache.lock().unwrap();
        cache.retain(|(l, _)| *l != line);
        cache.push_back((line, wav));
        while cache.len() > CACHE_SIZE {
            cache.pop_front();
        }
    }

    async fn render(&self, line: &Line) -> Result<Vec<u8>> {
        if let Some(wav) = self.cached(line) {
            return Ok(wav);
        }
        let engine = self.engine().await?;
        let _turn = self.rendering.lock().await;
        // The background renderer may have produced it while we waited.
        if let Some(wav) = self.cached(line) {
            return Ok(wav);
        }
        let config = GenerationConfig {
            sid: line.sid,
            speed: line.speed as f32 / 100.0,
            ..Default::default()
        };
        let text = line.text.clone();
        let wav = tokio::task::spawn_blocking(move || {
            let audio = engine
                .generate_with_config(&text, &config, None::<fn(&[f32], f32) -> bool>)
                .ok_or_else(|| msg("Speech synthesis failed"))?;
            Ok::<_, crate::error::AppError>(wav(audio.samples(), audio.sample_rate() as u32))
        })
        .await
        .map_err(|e| msg(e.to_string()))??;
        self.store(line.clone(), wav.clone());
        Ok(wav)
    }

    /// Renders queued lines until the queue is empty.
    async fn prefetch_loop(self: Arc<Self>) {
        loop {
            let next = self.wanted.lock().unwrap().pop_front();
            match next {
                Some(line) => {
                    if self.cached(&line).is_none() {
                        let _ = self.render(&line).await;
                    }
                }
                None => {
                    self.prefetching.store(false, Ordering::Release);
                    // A prefetch call may have queued lines after the pop above but
                    // seen the flag still set; take them over unless another loop did.
                    if self.wanted.lock().unwrap().is_empty()
                        || self.prefetching.swap(true, Ordering::AcqRel)
                    {
                        return;
                    }
                }
            }
        }
    }
}

/// Whether all model files are on disk.
fn installed(dir: &Path) -> bool {
    FILES.iter().all(|(name, _)| dir.join(name).is_file())
}

fn load(dir: &Path) -> Result<OfflineTts> {
    let path = |name: &str| Some(dir.join(name).to_string_lossy().into_owned());
    let config = OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            kokoro: OfflineTtsKokoroModelConfig {
                model: path(MODEL),
                voices: path("voices.bin"),
                tokens: path("tokens.txt"),
                lexicon: path("lexicon-us-en.txt"),
                ..Default::default()
            },
            // More threads than physical cores makes synthesis slower, not faster.
            num_threads: physical_cores().clamp(1, 4),
            ..Default::default()
        },
        ..Default::default()
    };
    OfflineTts::create(&config).ok_or_else(|| msg("Could not load the speech model"))
}

/// Physical core count; hyper-threads don't help a compute-bound model.
fn physical_cores() -> i32 {
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("sysctl").args(["-n", "hw.physicalcpu"]).output() {
        if let Ok(n) = String::from_utf8_lossy(&out.stdout).trim().parse::<i32>() {
            return n;
        }
    }
    // Elsewhere assume two hardware threads per core.
    std::thread::available_parallelism().map_or(2, |n| (n.get() as i32 / 2).max(1))
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
    for name in STALE_FILES {
        let _ = tokio::fs::remove_file(dir.join(name)).await;
    }
    Ok(dir.to_path_buf())
}

async fn ensure_file(dir: &Path, name: &str, sha256: &str) -> Result<()> {
    let path = dir.join(name);
    if path.is_file() {
        return Ok(());
    }
    let url = format!(
        "https://huggingface.co/csukuangfj/kokoro-multi-lang-v1_0/resolve/{REVISION}/{name}"
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

    /// Downloads the model (~360 MB) into `TTS_TEST_DIR` and speaks a few lines.
    /// Run with `TTS_TEST_DIR=/some/dir cargo test speaks -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "downloads the speech model"]
    async fn speaks_lines() {
        let dir = PathBuf::from(std::env::var("TTS_TEST_DIR").expect("set TTS_TEST_DIR"));
        let tts = Tts::new(&dir);
        for (i, (text, speed)) in [
            ("I ain't gonna lie to you, mate.", 1.0),
            ("I ain't gonna lie to you, mate.", 0.7),
            ("Gotta bounce, Szymanski's waiting!", 1.0),
        ]
        .into_iter()
        .enumerate()
        {
            let started = std::time::Instant::now();
            let bytes = tts.synthesize(text, "af_heart", speed).await.unwrap();
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
        let started = std::time::Instant::now();
        tts.synthesize("I ain't gonna lie to you, mate.", "af_heart", 1.0).await.unwrap();
        println!("cached line in {:?}", started.elapsed());
        assert!(started.elapsed() < std::time::Duration::from_millis(50));
    }

    #[test]
    fn lines_pick_voice_and_round_speed() {
        let line = Line::new("- Hi ♪", "am_adam", 0.7);
        assert_eq!(line, Line { text: "Hi".into(), sid: 11, speed: 70 });
        assert_eq!(Line::new("Hi", "unknown", 9.0).sid, 3);
        assert_eq!(Line::new("Hi", "unknown", 9.0).speed, 150);
    }

    #[test]
    fn cache_keeps_recently_used_lines() {
        let tts = Tts::new(Path::new("/nonexistent"));
        let line = |i: usize| Line::new(&format!("line {i}"), DEFAULT_VOICE, 1.0);
        for i in 0..CACHE_SIZE {
            tts.inner.store(line(i), vec![i as u8]);
        }
        // Touching the oldest line saves it from eviction.
        assert_eq!(tts.inner.cached(&line(0)), Some(vec![0]));
        tts.inner.store(line(CACHE_SIZE), vec![0xff]);
        assert!(tts.inner.cached(&line(0)).is_some());
        assert!(tts.inner.cached(&line(1)).is_none());
        assert_eq!(tts.inner.cache.lock().unwrap().len(), CACHE_SIZE);
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
