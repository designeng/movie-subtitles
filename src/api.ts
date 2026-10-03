import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface Cue {
  startMs: number;
  endMs: number;
  text: string;
}

export interface LibraryEntry {
  id: string;
  provider: string;
  url: string;
  title: string;
  searchQuery: string;
  videoPath: string;
  subtitlePath: string | null;
  subtitleLabel: string | null;
  /** Positive values show subtitles later, negative earlier. */
  offsetMs: number;
  addedAt: number;
  /** Where playback stopped. */
  positionMs: number;
}

export interface SubtitleCandidate {
  provider: string;
  id: string;
  title: string;
  year: number | null;
  release: string;
  language: string;
  downloads: number;
  hearingImpaired: boolean;
  fps: number | null;
}

export interface SubtitleSearch {
  candidates: SubtitleCandidate[];
  /** Names of the providers that were asked. */
  searched: string[];
  /** "Provider: error" for providers that failed. */
  errors: string[];
}

export interface VideoSearchResult {
  provider: string;
  url: string;
  title: string;
  channel: string | null;
  /** Seconds. */
  duration: number | null;
  views: number | null;
  thumbnail: string | null;
  /** The title hints at a dubbed (non-original language) version. */
  likelyDubbed: boolean;
}

export interface DownloadProgress {
  stage: string;
  percent: number | null;
  speedBps: number | null;
  etaSecs: number | null;
}

export interface Settings {
  opensubtitlesApiKey: string;
  opensubtitlesUsername: string;
  opensubtitlesPassword: string;
  subdlApiKey: string;
  language: string;
  googleTranslateApiKey: string;
  translateTo: string;
  /** Empty means the default folder inside the app data directory. */
  videosDir: string;
  videoQuality: VideoQuality;
  showFilesInfo: boolean;
}

export type VideoQuality = "best" | "1080p" | "720p";

export type SubtitleMode = "original" | "translation" | "both";

export const api = {
  listLibrary: () => invoke<LibraryEntry[]>("list_library"),
  librarySizes: () => invoke<Record<string, number>>("library_sizes"),
  mediaBaseUrl: () => invoke<string>("media_base_url"),
  downloadVideo: (url: string) => invoke<LibraryEntry>("download_video", { url }),
  cancelDownload: () => invoke<void>("cancel_download"),
  searchVideos: (query: string) => invoke<VideoSearchResult[]>("search_videos", { query }),
  deleteEntry: (id: string) => invoke<void>("delete_entry", { id }),
  searchSubtitles: (query: string, language: string) =>
    invoke<SubtitleSearch>("search_subtitles", { query, language }),
  fetchSubtitles: (entryId: string, candidate: SubtitleCandidate) =>
    invoke<Cue[]>("fetch_subtitles", { entryId, candidate }),
  loadSubtitleFile: (entryId: string, path: string) =>
    invoke<Cue[]>("load_subtitle_file", { entryId, path }),
  downloadSubtitleUrl: (entryId: string, url: string) =>
    invoke<Cue[]>("download_subtitle_url", { entryId, url }),
  localSubtitles: (entryId: string) => invoke<string[]>("local_subtitles", { entryId }),
  getSubtitles: (entryId: string) => invoke<Cue[] | null>("get_subtitles", { entryId }),
  removeSubtitles: (entryId: string) => invoke<LibraryEntry>("remove_subtitles", { entryId }),
  setOffset: (entryId: string, offsetMs: number) => invoke<void>("set_offset", { entryId, offsetMs }),
  setPosition: (entryId: string, positionMs: number) =>
    invoke<void>("set_position", { entryId, positionMs }),
  translateSubtitles: (entryId: string, target: string) =>
    invoke<string[]>("translate_subtitles", { entryId, target }),
  exportSubtitles: (entryId: string, path: string) =>
    invoke<void>("export_subtitles", { entryId, path }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  getVideosDir: () => invoke<string>("get_videos_dir"),
  openVideosDir: () => invoke<void>("open_videos_dir"),
  onDownloadProgress: (cb: (p: DownloadProgress) => void): Promise<UnlistenFn> =>
    listen<DownloadProgress>("download-progress", (e) => cb(e.payload)),
};
