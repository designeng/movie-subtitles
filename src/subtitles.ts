import type { Cue } from "./api";

/**
 * Returns indices of cues visible at `timeMs` of the video, given an offset
 * (positive offset = subtitles shown later). `cues` must be sorted by start.
 */
export function activeCues(cues: Cue[], timeMs: number, offsetMs: number): number[] {
  const t = timeMs - offsetMs;
  // Binary search for the last cue starting at or before t.
  let lo = 0;
  let hi = cues.length - 1;
  let idx = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (cues[mid].startMs <= t) {
      idx = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  // Overlapping cues: look back a few entries for ones still on screen.
  const result: number[] = [];
  for (let i = idx; i >= 0 && i > idx - 5; i--) {
    if (cues[i].endMs > t) result.unshift(i);
  }
  return result;
}

/** Index of the cue at or right after `timeMs` (shifted), for list highlighting. */
export function cueIndexAt(cues: Cue[], timeMs: number, offsetMs: number): number {
  const t = timeMs - offsetMs;
  let lo = 0;
  let hi = cues.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (cues[mid].endMs <= t) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

export function formatOffset(ms: number): string {
  const sign = ms > 0 ? "+" : ms < 0 ? "−" : "±";
  return `${sign}${(Math.abs(ms) / 1000).toFixed(1)} s`;
}

export function formatTime(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(2, "0");
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}
