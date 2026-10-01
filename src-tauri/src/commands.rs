use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager, State};
use url::Url;

use crate::error::{msg, Result};
use crate::library::{remove_files_with_prefix, LibraryEntry};
use crate::settings::Settings;
use crate::subtitles::{self, format, format::Cue, SubtitleCandidate, SubtitleQuery};
use crate::translate::{GoogleTranslate, GoogleTranslateFree, Translator};
use crate::video::DownloadProgress;
use crate::AppState;

pub const PROGRESS_EVENT: &str = "download-progress";

#[tauri::command]
pub fn list_library(state: State<'_, AppState>) -> Vec<LibraryEntry> {
    state.library.lock().unwrap().entries().to_vec()
}

#[tauri::command]
pub async fn download_video(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<LibraryEntry> {
    let url = url.trim();
    let parsed = Url::parse(url).map_err(|_| msg("Invalid URL"))?;
    let provider = state.videos.find(&parsed)?;
    let emit = |p: DownloadProgress| {
        let _ = app.emit(PROGRESS_EVENT, p);
    };

    emit(DownloadProgress::stage("Reading video info"));
    let info = provider.info(url).await?;
    let id = format!("{}-{}", info.provider, info.id);

    if let Ok(existing) = state.library.lock().unwrap().get(&id) {
        if existing.video_path.is_file() {
            return Ok(existing.clone());
        }
    }

    let dir = state.videos_dir();
    std::fs::create_dir_all(&dir)?;
    let video_path = match provider.download(url, &dir, &id, &emit).await {
        Ok(path) => path,
        Err(e) => {
            // Don't leave fragments of a failed download behind.
            let _ = remove_files_with_prefix(&dir, &id);
            return Err(e);
        }
    };

    let entry = LibraryEntry {
        id,
        provider: info.provider,
        url: info.webpage_url,
        search_query: subtitles::query::from_title(&info.title),
        title: info.title,
        video_path,
        subtitle_path: None,
        subtitle_label: None,
        offset_ms: 0,
        added_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
    };
    state.library.lock().unwrap().upsert(entry.clone())?;
    Ok(entry)
}

/// Removes the movie, any partial download fragments and its subtitles.
#[tauri::command]
pub fn delete_entry(state: State<'_, AppState>, id: String) -> Result<()> {
    // The video may live in a previously configured folder.
    let saved_dir = state.library.lock().unwrap().get(&id).ok()
        .and_then(|e| e.video_path.parent().map(PathBuf::from));
    if let Some(dir) = saved_dir {
        remove_files_with_prefix(&dir, &id)?;
    }
    remove_files_with_prefix(&state.videos_dir(), &id)?;
    remove_files_with_prefix(&state.subtitles_dir(), &id)?;
    state.library.lock().unwrap().remove(&id)?;
    Ok(())
}

#[tauri::command]
pub async fn search_subtitles(
    state: State<'_, AppState>,
    query: String,
    language: String,
) -> Result<Vec<SubtitleCandidate>> {
    let registry = state.subtitles.read().unwrap().clone();
    registry.search(&SubtitleQuery { text: query, language }).await
}

#[tauri::command]
pub async fn fetch_subtitles(
    state: State<'_, AppState>,
    entry_id: String,
    candidate: SubtitleCandidate,
) -> Result<Vec<Cue>> {
    let registry = state.subtitles.read().unwrap().clone();
    let bytes = registry.fetch(&candidate).await?;
    let label = format!("{} [{}]", candidate.release, candidate.language);
    attach_subtitles(&state, &entry_id, &bytes, label)
}

#[tauri::command]
pub fn load_subtitle_file(
    state: State<'_, AppState>,
    entry_id: String,
    path: PathBuf,
) -> Result<Vec<Cue>> {
    let bytes = std::fs::read(&path)?;
    let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    attach_subtitles(&state, &entry_id, &bytes, label)
}

fn attach_subtitles(state: &AppState, entry_id: &str, bytes: &[u8], label: String) -> Result<Vec<Cue>> {
    let cues = format::parse(&format::decode(bytes));
    if cues.is_empty() {
        return Err(msg("No subtitles found in the file (expected SRT or WebVTT)"));
    }
    let dir = state.subtitles_dir();
    std::fs::create_dir_all(&dir)?;
    // Drops the previous file and any translations cached for it.
    remove_files_with_prefix(&dir, entry_id)?;
    let path = dir.join(format!("{entry_id}.srt"));
    std::fs::write(&path, format::to_srt(&cues, 0))?;

    state.library.lock().unwrap().update(entry_id, |e| {
        e.subtitle_path = Some(path);
        e.subtitle_label = Some(label);
        e.offset_ms = 0;
    })?;
    Ok(cues)
}

#[tauri::command]
pub fn get_subtitles(state: State<'_, AppState>, entry_id: String) -> Result<Option<Vec<Cue>>> {
    let path = state.library.lock().unwrap().get(&entry_id)?.subtitle_path.clone();
    match path {
        Some(path) if path.is_file() => Ok(Some(format::parse(&format::decode(&std::fs::read(path)?)))),
        _ => Ok(None),
    }
}

#[tauri::command]
pub fn remove_subtitles(state: State<'_, AppState>, entry_id: String) -> Result<LibraryEntry> {
    remove_files_with_prefix(&state.subtitles_dir(), &entry_id)?;
    state.library.lock().unwrap().update(&entry_id, |e| {
        e.subtitle_path = None;
        e.subtitle_label = None;
        e.offset_ms = 0;
    })
}

#[tauri::command]
pub fn set_offset(state: State<'_, AppState>, entry_id: String, offset_ms: i64) -> Result<()> {
    state.library.lock().unwrap().update(&entry_id, |e| e.offset_ms = offset_ms)?;
    Ok(())
}

/// Translates the entry's subtitles line by line; results are cached on disk
/// next to the subtitle file and invalidated when subtitles change.
#[tauri::command]
pub async fn translate_subtitles(
    state: State<'_, AppState>,
    entry_id: String,
    target: String,
) -> Result<Vec<String>> {
    let target = target.trim().to_lowercase();
    if target.is_empty() || !target.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(msg("Invalid target language code"));
    }
    let cache = state.subtitles_dir().join(format!("{entry_id}.tr-{target}.json"));
    if let Ok(bytes) = std::fs::read(&cache) {
        if let Ok(lines) = serde_json::from_slice(&bytes) {
            return Ok(lines);
        }
    }

    let source = state.library.lock().unwrap().get(&entry_id)?.subtitle_path.clone();
    let source = source.ok_or_else(|| msg("This movie has no subtitles"))?;
    let cues = format::parse(&format::decode(&std::fs::read(source)?));
    let texts: Vec<String> = cues.into_iter().map(|c| c.text).collect();

    let api_key = state.settings.lock().unwrap().google_translate_api_key.trim().to_string();
    let translator: Box<dyn Translator> = if api_key.is_empty() {
        Box::new(GoogleTranslateFree::new())
    } else {
        Box::new(GoogleTranslate::new(api_key))
    };
    let lines = translator.translate(&texts, &target).await?;
    std::fs::write(&cache, serde_json::to_vec(&lines)?)?;
    Ok(lines)
}

/// Writes the subtitles with the current offset baked in.
#[tauri::command]
pub fn export_subtitles(state: State<'_, AppState>, entry_id: String, path: PathBuf) -> Result<()> {
    let entry = state.library.lock().unwrap().get(&entry_id)?.clone();
    let source = entry.subtitle_path.ok_or_else(|| msg("This movie has no subtitles"))?;
    let cues = format::parse(&format::decode(&std::fs::read(source)?));
    std::fs::write(path, format::to_srt(&cues, entry.offset_ms))?;
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> Result<()> {
    let videos_dir = settings.videos_dir.trim();
    if !videos_dir.is_empty() {
        let dir = PathBuf::from(videos_dir);
        if !dir.is_absolute() {
            return Err(msg("Videos folder must be an absolute path"));
        }
        std::fs::create_dir_all(&dir)?;
        app.asset_protocol_scope()
            .allow_directory(&dir, true)
            .map_err(|e| msg(format!("Cannot use videos folder: {e}")))?;
    }
    settings.save(&state.data_dir.join("settings.json"))?;
    *state.subtitles.write().unwrap() = Arc::new(subtitles::Registry::from_settings(&settings));
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
pub fn get_videos_dir(state: State<'_, AppState>) -> PathBuf {
    state.videos_dir()
}

#[tauri::command]
pub fn open_videos_dir(state: State<'_, AppState>) -> Result<()> {
    let dir = state.videos_dir();
    std::fs::create_dir_all(&dir)?;
    tauri_plugin_opener::open_path(&dir, None::<&str>).map_err(|e| msg(e.to_string()))
}
