mod commands;
mod error;
mod library;
mod settings;
mod subtitles;
mod tools;
mod translate;
mod video;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use tauri::Manager;

use library::Library;
use settings::Settings;

pub struct AppState {
    pub data_dir: PathBuf,
    pub videos: video::Registry,
    pub library: Mutex<Library>,
    pub settings: Mutex<Settings>,
    pub subtitles: RwLock<Arc<subtitles::Registry>>,
    /// Cancels the download in progress, if any.
    pub cancel_download: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl AppState {
    fn new(data_dir: PathBuf) -> Self {
        let settings = Settings::load(&data_dir.join("settings.json"));
        Self {
            videos: video::Registry::new(&data_dir.join("bin")),
            library: Mutex::new(Library::load(&data_dir.join("library.json"))),
            subtitles: RwLock::new(Arc::new(subtitles::Registry::from_settings(&settings))),
            settings: Mutex::new(settings),
            cancel_download: Mutex::new(None),
            data_dir,
        }
    }

    pub fn videos_dir(&self) -> PathBuf {
        let custom = self.settings.lock().unwrap().videos_dir.trim().to_string();
        if custom.is_empty() {
            self.data_dir.join("videos")
        } else {
            PathBuf::from(custom)
        }
    }

    pub fn subtitles_dir(&self) -> PathBuf {
        self.data_dir.join("subtitles")
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let state = AppState::new(data_dir);
            // The static assetProtocol scope only covers the default folder; the player also
            // needs a custom videos folder and wherever earlier downloads were saved.
            let scope = app.asset_protocol_scope();
            scope.allow_directory(state.videos_dir(), true)?;
            for entry in state.library.lock().unwrap().entries() {
                if let Some(dir) = entry.video_path.parent() {
                    scope.allow_directory(dir, true)?;
                }
            }
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_library,
            commands::download_video,
            commands::cancel_download,
            commands::search_videos,
            commands::delete_entry,
            commands::search_subtitles,
            commands::fetch_subtitles,
            commands::load_subtitle_file,
            commands::get_subtitles,
            commands::remove_subtitles,
            commands::set_offset,
            commands::translate_subtitles,
            commands::export_subtitles,
            commands::get_settings,
            commands::save_settings,
            commands::get_videos_dir,
            commands::open_videos_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
