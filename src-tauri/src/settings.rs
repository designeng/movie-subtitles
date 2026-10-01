use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub opensubtitles_api_key: String,
    pub opensubtitles_username: String,
    pub opensubtitles_password: String,
    pub subdl_api_key: String,
    /// Default subtitle language (ISO 639-1).
    pub language: String,
    pub google_translate_api_key: String,
    /// Target language for translated subtitles (ISO 639-1).
    pub translate_to: String,
    /// Where downloaded videos go; empty means `<app data>/videos`.
    pub videos_dir: String,
    pub video_quality: VideoQuality,
}

/// Upper limit for downloaded video resolution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoQuality {
    #[default]
    #[serde(rename = "best")]
    Best,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "720p")]
    P720,
}

impl VideoQuality {
    pub fn max_height(self) -> Option<u32> {
        match self {
            Self::Best => None,
            Self::P1080 => Some(1080),
            Self::P720 => Some(720),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            opensubtitles_api_key: String::new(),
            opensubtitles_username: String::new(),
            opensubtitles_password: String::new(),
            subdl_api_key: String::new(),
            language: "en".into(),
            google_translate_api_key: String::new(),
            translate_to: "ru".into(),
            videos_dir: String::new(),
            video_quality: VideoQuality::Best,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}
