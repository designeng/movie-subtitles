//! Persistent list of downloaded movies with their subtitle state.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{msg, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    /// `{provider}-{video id}`; also the prefix of every file belonging to the entry.
    pub id: String,
    pub provider: String,
    pub url: String,
    pub title: String,
    pub search_query: String,
    pub video_path: PathBuf,
    pub subtitle_path: Option<PathBuf>,
    pub subtitle_label: Option<String>,
    /// Positive values show subtitles later, negative earlier.
    pub offset_ms: i64,
    pub added_at: u64,
    /// Where playback stopped, so it can resume after a restart.
    #[serde(default)]
    pub position_ms: u64,
}

pub struct Library {
    path: PathBuf,
    entries: Vec<LibraryEntry>,
}

impl Library {
    pub fn load(path: &Path) -> Self {
        let mut entries: Vec<LibraryEntry> = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        // Picks up improvements to the title cleanup for older entries.
        for e in &mut entries {
            e.search_query = crate::subtitles::query::from_title(&e.title);
        }
        Self { path: path.to_path_buf(), entries }
    }

    pub fn save(&self) -> Result<()> {
        std::fs::write(&self.path, serde_json::to_vec_pretty(&self.entries)?)?;
        Ok(())
    }

    pub fn entries(&self) -> &[LibraryEntry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Result<&LibraryEntry> {
        self.entries.iter().find(|e| e.id == id).ok_or_else(|| msg(format!("Unknown movie: {id}")))
    }

    /// Applies `f` to the entry and persists the library.
    pub fn update(&mut self, id: &str, f: impl FnOnce(&mut LibraryEntry)) -> Result<LibraryEntry> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| msg(format!("Unknown movie: {id}")))?;
        f(entry);
        let entry = entry.clone();
        self.save()?;
        Ok(entry)
    }

    pub fn upsert(&mut self, entry: LibraryEntry) -> Result<()> {
        self.entries.retain(|e| e.id != entry.id);
        self.entries.insert(0, entry);
        self.save()
    }

    pub fn remove(&mut self, id: &str) -> Result<Option<LibraryEntry>> {
        let pos = self.entries.iter().position(|e| e.id == id);
        let removed = pos.map(|i| self.entries.remove(i));
        self.save()?;
        Ok(removed)
    }
}

/// Deletes every file in `dir` whose name starts with `{prefix}.` — the final
/// video plus yt-dlp leftovers (`.part`, `.ytdl`, `.f137.mp4`, ...).
pub fn remove_files_with_prefix(dir: &Path, prefix: &str) -> Result<usize> {
    let Ok(read_dir) = std::fs::read_dir(dir) else { return Ok(0) };
    let needle = format!("{prefix}.");
    let mut removed = 0;
    for item in read_dir.flatten() {
        if item.file_name().to_string_lossy().starts_with(&needle) && item.path().is_file() {
            std::fs::remove_file(item.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_only_matching_files() {
        let dir = std::env::temp_dir().join(format!("movie-subs-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["vk-1.mp4", "vk-1.f137.mp4.part", "vk-1.ytdl", "vk-12.mp4", "youtube-1.mp4"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        assert_eq!(remove_files_with_prefix(&dir, "vk-1").unwrap(), 3);
        let mut left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["vk-12.mp4", "youtube-1.mp4"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
