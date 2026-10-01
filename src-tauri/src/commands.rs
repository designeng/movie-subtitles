use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;
use url::Url;

use crate::error::{msg, Result};
use crate::library::{remove_files_with_prefix, LibraryEntry};
use crate::settings::Settings;
use crate::subtitles::{self, format, format::Cue, SubtitleCandidate, SubtitleQuery, SubtitleSearch};
use crate::translate::{GoogleTranslate, GoogleTranslateFree, Translator};
use crate::video::{DownloadProgress, VideoProvider, VideoSearchResult};
use crate::AppState;

/// Error text of a user-cancelled download; the UI treats it as not an error.
pub const CANCELLED: &str = "cancelled";
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

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    *state.cancel_download.lock().unwrap() = Some(cancel_tx);
    let current_id = std::sync::Mutex::new(None);
    let result = tokio::select! {
        r = download_inner(&state, provider, url, &emit, &current_id) => r,
        // Dropping the download future kills yt-dlp (`kill_on_drop`).
        _ = cancel_rx => Err(msg(CANCELLED)),
    };
    *state.cancel_download.lock().unwrap() = None;
    if result.is_err() {
        // Don't leave fragments of a failed or cancelled download behind.
        if let Some(id) = current_id.lock().unwrap().take() {
            let _ = remove_files_with_prefix(&state.videos_dir(), &id);
        }
    }
    match &result {
        Ok(entry) => notify(&app, "Download complete", &entry.title),
        Err(e) if e.to_string() != CANCELLED => notify(&app, "Download failed", &e.to_string()),
        Err(_) => {}
    }
    result
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

/// Asks the running download to stop.
#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>) {
    if let Some(tx) = state.cancel_download.lock().unwrap().take() {
        let _ = tx.send(());
    }
}

async fn download_inner(
    state: &AppState,
    provider: &dyn VideoProvider,
    url: &str,
    emit: &(dyn Fn(DownloadProgress) + Send + Sync),
    current_id: &std::sync::Mutex<Option<String>>,
) -> Result<LibraryEntry> {
    emit(DownloadProgress::stage("Reading video info"));
    let info = provider.info(url).await?;
    let id = format!("{}-{}", info.provider, info.id);
    *current_id.lock().unwrap() = Some(id.clone());

    if let Ok(existing) = state.library.lock().unwrap().get(&id) {
        if existing.video_path.is_file() {
            return Ok(existing.clone());
        }
    }

    let dir = state.videos_dir();
    std::fs::create_dir_all(&dir)?;
    let max_height = state.settings.lock().unwrap().video_quality.max_height();
    let video_path = provider.download(url, &dir, &id, max_height, emit).await?;

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

#[tauri::command]
pub async fn search_videos(state: State<'_, AppState>, query: String) -> Result<Vec<VideoSearchResult>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    state.videos.search(query).await
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
) -> Result<SubtitleSearch> {
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

/// Downloads subtitles (.srt, .vtt or a ZIP with one) from a direct link.
#[tauri::command]
pub async fn download_subtitle_url(
    state: State<'_, AppState>,
    entry_id: String,
    url: String,
) -> Result<Vec<Cue>> {
    let url = url.trim();
    let parsed = Url::parse(url).map_err(|_| msg("Invalid URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(msg("Only http(s) links are supported"));
    }
    let bytes = subtitles::download(url).await?;
    let label = parsed
        .path_segments()
        .and_then(|mut s| s.next_back())
        .filter(|name| !name.is_empty())
        .unwrap_or(parsed.host_str().unwrap_or(url))
        .to_string();
    attach_subtitles(&state, &entry_id, &bytes, label)
}

/// Subtitle files already on disk for the entry that are not the loaded ones:
/// `{id}*.srt|vtt` next to the video or in the subtitles folder.
#[tauri::command]
pub fn local_subtitles(state: State<'_, AppState>, entry_id: String) -> Result<Vec<PathBuf>> {
    let entry = state.library.lock().unwrap().get(&entry_id)?.clone();
    let mut dirs = vec![state.videos_dir(), state.subtitles_dir()];
    if let Some(dir) = entry.video_path.parent() {
        dirs.insert(0, dir.to_path_buf());
    }
    dirs.dedup();
    let mut found = Vec::new();
    for dir in dirs {
        let Ok(read_dir) = std::fs::read_dir(&dir) else { continue };
        for item in read_dir.flatten() {
            let path = item.path();
            let name = item.file_name().to_string_lossy().to_lowercase();
            let is_subtitle = name.ends_with(".srt") || name.ends_with(".vtt");
            // `{id}.srt`, `{id}.en.srt`, but not `{id}1.srt` of another movie.
            let ours = name
                .strip_prefix(&entry.id.to_lowercase())
                .is_some_and(|rest| rest.starts_with('.'));
            if is_subtitle && ours && path.is_file() && Some(&path) != entry.subtitle_path.as_ref() {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
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
