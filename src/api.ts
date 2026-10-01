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
}

export type SubtitleMode = "original" | "translation" | "both";

export const api = {
  listLibrary: () => invoke<LibraryEntry[]>("list_library"),
  downloadVideo: (url: string) => invoke<LibraryEntry>("download_video", { url }),
  deleteEntry: (id: string) => invoke<void>("delete_entry", { id }),
  searchSubtitles: (query: string, language: string) =>
    invoke<SubtitleCandidate[]>("search_subtitles", { query, language }),
  fetchSubtitles: (entryId: string, candidate: SubtitleCandidate) =>
    invoke<Cue[]>("fetch_subtitles", { entryId, candidate }),
  loadSubtitleFile: (entryId: string, path: string) =>
    invoke<Cue[]>("load_subtitle_file", { entryId, path }),
  getSubtitles: (entryId: string) => invoke<Cue[] | null>("get_subtitles", { entryId }),
  removeSubtitles: (entryId: string) => invoke<LibraryEntry>("remove_subtitles", { entryId }),
  setOffset: (entryId: string, offsetMs: number) => invoke<void>("set_offset", { entryId, offsetMs }),
  translateSubtitles: (entryId: string, target: string) =>
    invoke<string[]>("translate_subtitles", { entryId, target }),
  exportSubtitles: (entryId: string, path: string) =>
    invoke<void>("export_subtitles", { entryId, path }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  onDownloadProgress: (cb: (p: DownloadProgress) => void): Promise<UnlistenFn> =>
    listen<DownloadProgress>("download-progress", (e) => cb(e.payload)),
};
