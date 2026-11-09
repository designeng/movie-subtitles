<script setup lang="ts">
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { save } from "@tauri-apps/plugin-dialog";
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { api, type Cue, type LibraryEntry, type Settings, type SubtitleMode } from "./api";
import AppToolbar from "./components/AppToolbar.vue";
import LibraryList from "./components/LibraryList.vue";
import PlayerBar from "./components/PlayerBar.vue";
import SettingsDialog, { type SettingsTab } from "./components/SettingsDialog.vue";
import ShortcutsDialog from "./components/ShortcutsDialog.vue";
import SubtitlePanel from "./components/SubtitlePanel.vue";
import VideoPlayer from "./components/VideoPlayer.vue";
import Icon from "./components/ui/Icon.vue";
import Toast from "./components/ui/Toast.vue";
import { usePanel } from "./composables/usePanel";
import { hasMod, modKey } from "./platform";

const entries = ref<LibraryEntry[]>([]);
const selected = ref<LibraryEntry | null>(null);
const cues = ref<Cue[]>([]);
const translations = ref<string[] | null>(null);
const subtitleMode = ref<SubtitleMode>("original");
const offsetMs = ref(0);
const timeMs = ref(0);
const settings = ref<Settings | null>(null);
const settingsTab = ref<SettingsTab | null>(null);
const showShortcuts = ref(false);
const sizes = ref<Record<string, number> | null>(null);
const fullscreen = ref(false);
const error = ref("");
const loaded = ref(false);
const player = ref<InstanceType<typeof VideoPlayer>>();
const toolbar = ref<InstanceType<typeof AppToolbar>>();
const panel = ref<InstanceType<typeof SubtitlePanel>>();

// --- Layout -----------------------------------------------------------------

const library = usePanel("sidebar", { side: "left", initial: 240, min: 200, maxFraction: 0.3 });
const subsPanel = usePanel("subtitlePanel", { side: "right", initial: 360, min: 300, maxFraction: 0.4 });

const panelAvailable = computed(() => !!selected.value && !!settings.value);
const showLibrary = computed(() => !fullscreen.value && library.open.value);
const showPanel = computed(() => !fullscreen.value && subsPanel.open.value && panelAvailable.value);
const columns = computed(() =>
  [showLibrary.value && `${library.width.value}px`, "minmax(0, 1fr)", showPanel.value && `${subsPanel.width.value}px`]
    .filter(Boolean)
    .join(" "),
);
const resizing = computed(() => library.resizing.value || subsPanel.resizing.value);

const MIN_FONT_SCALE = 0.5;
const MAX_FONT_SCALE = 2;
const fontScale = ref(Number(localStorage.getItem("subtitleFontScale")) || 1);
function changeFontScale(delta: number) {
  const next = Math.round((fontScale.value + delta) * 10) / 10;
  fontScale.value = Math.min(MAX_FONT_SCALE, Math.max(MIN_FONT_SCALE, next));
  localStorage.setItem("subtitleFontScale", String(fontScale.value));
}

const mediaBaseUrl = ref("");
// `addedAt` changes when a movie is downloaded again, so the player doesn't reuse the old file.
const videoSrc = computed(() =>
  selected.value && mediaBaseUrl.value
    ? `${mediaBaseUrl.value}/${encodeURIComponent(selected.value.id)}?v=${selected.value.addedAt}`
    : "",
);

watch(
  () => [entries.value, settings.value?.showFilesInfo] as const,
  async ([, show]) => {
    try {
      sizes.value = show ? await api.librarySizes() : null;
    } catch {
      sizes.value = null;
    }
  },
);

// The window title names the movie (visible in the title bar on Windows/Linux and in macOS window lists).
watch(selected, (entry) => {
  getCurrentWindow()
    .setTitle(entry ? `${entry.title} — Movie Subtitles` : "Movie Subtitles")
    .catch(() => {});
});

// --- Lifecycle ----------------------------------------------------------------

let unlistenDrop: (() => void) | undefined;
let unlistenResize: (() => void) | undefined;

onMounted(async () => {
  window.addEventListener("keydown", onKey);
  try {
    [entries.value, settings.value, mediaBaseUrl.value] = await Promise.all([
      api.listLibrary(),
      api.getSettings(),
      api.mediaBaseUrl(),
    ]);
  } catch (e) {
    error.value = String(e);
  }
  loaded.value = true;
  const lastId = localStorage.getItem("lastEntryId");
  const last = entries.value.find((e) => e.id === lastId) ?? entries.value[0];
  if (last) await select(last);
  if (settings.value && !settings.value.opensubtitlesApiKey && !settings.value.subdlApiKey) settingsTab.value = "subtitles";

  unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => onDragDrop(event.payload));
  unlistenResize = await getCurrentWindow().onResized(onWindowResized);
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
  unlistenDrop?.();
  unlistenResize?.();
});

// Leaving fullscreen through the system (green button, ⌃⌘F) must restore the app chrome too.
let resizeTimer: number | undefined;
function onWindowResized() {
  if (!fullscreen.value) return;
  clearTimeout(resizeTimer);
  resizeTimer = window.setTimeout(async () => {
    if (fullscreen.value && !(await getCurrentWindow().isFullscreen())) fullscreen.value = false;
  }, 400);
}

// --- Selection & subtitles ----------------------------------------------------

// The translation shown for each movie survives restarts; the lines come from the backend's disk cache.
interface SavedTranslation {
  target: string;
  mode: SubtitleMode;
}
const translationKey = (id: string) => `translation:${id}`;
function loadSavedTranslation(id: string): SavedTranslation | null {
  try {
    return JSON.parse(localStorage.getItem(translationKey(id)) ?? "null");
  } catch {
    return null;
  }
}
function saveTranslation(id: string, saved: SavedTranslation | null) {
  if (saved) localStorage.setItem(translationKey(id), JSON.stringify(saved));
  else localStorage.removeItem(translationKey(id));
}
const translateTarget = ref("");

async function select(entry: LibraryEntry) {
  selected.value = entry;
  localStorage.setItem("lastEntryId", entry.id);
  offsetMs.value = entry.offsetMs;
  cues.value = [];
  translations.value = null;
  subtitleMode.value = "original";
  translateTarget.value = "";
  try {
    const loadedCues = (await api.getSubtitles(entry.id)) ?? [];
    if (selected.value?.id !== entry.id) return;
    cues.value = loadedCues;
  } catch (e) {
    error.value = String(e);
  }
  const saved = loadSavedTranslation(entry.id);
  if (!saved || !cues.value.length) return;
  try {
    const lines = await api.translateSubtitles(entry.id, saved.target);
    if (selected.value?.id !== entry.id) return;
    translations.value = lines;
    translateTarget.value = saved.target;
    subtitleMode.value = saved.mode;
  } catch {
    // Offline or quota exceeded: the user can translate again from the panel.
  }
}

function onTranslated(lines: string[], target: string) {
  translations.value = lines;
  translateTarget.value = target;
  if (selected.value) saveTranslation(selected.value.id, { target, mode: subtitleMode.value });
}

watch(subtitleMode, (mode) => {
  if (selected.value && translations.value && translateTarget.value) {
    saveTranslation(selected.value.id, { target: translateTarget.value, mode });
  }
});

function replaceEntry(entry: LibraryEntry) {
  entries.value = entries.value.map((e) => (e.id === entry.id ? entry : e));
  if (selected.value?.id === entry.id) selected.value = entry;
}

async function onDownloaded(entry: LibraryEntry) {
  entries.value = [entry, ...entries.value.filter((e) => e.id !== entry.id)];
  await select(entry);
}

function onDeleted(id: string) {
  const index = entries.value.findIndex((e) => e.id === id);
  entries.value = entries.value.filter((e) => e.id !== id);
  saveTranslation(id, null);
  if (selected.value?.id === id) {
    selected.value = null;
    cues.value = [];
    translations.value = null;
    // Keep the user's place in the list: select the neighbour.
    const next = entries.value[Math.min(index, entries.value.length - 1)];
    if (next) select(next);
  }
}

async function onSubtitlesLoaded(loadedCues: Cue[]) {
  cues.value = loadedCues;
  translations.value = null;
  subtitleMode.value = "original";
  offsetMs.value = 0;
  if (selected.value) saveTranslation(selected.value.id, null);
  // The backend updated the label and reset the offset.
  entries.value = await api.listLibrary();
  selected.value = entries.value.find((e) => e.id === selected.value?.id) ?? null;
}

function onSubtitlesRemoved(entry: LibraryEntry) {
  cues.value = [];
  translations.value = null;
  subtitleMode.value = "original";
  offsetMs.value = 0;
  saveTranslation(entry.id, null);
  replaceEntry(entry);
}

// Persist the offset shortly after the user stops adjusting it.
let saveTimer: number | undefined;
watch(offsetMs, (ms) => {
  const entry = selected.value;
  if (!entry || entry.offsetMs === ms) return;
  entry.offsetMs = ms;
  clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    api.setOffset(entry.id, ms).catch((e) => (error.value = String(e)));
  }, 400);
});

function savePosition(id: string, ms: number) {
  const entry = entries.value.find((e) => e.id === id);
  if (entry) entry.positionMs = ms;
  api.setPosition(id, ms).catch((e) => (error.value = String(e)));
}

async function exportSrt() {
  const entry = selected.value;
  if (!entry) return;
  clearTimeout(saveTimer);
  try {
    await api.setOffset(entry.id, offsetMs.value);
    const path = await save({
      defaultPath: `${entry.searchQuery || entry.title}.srt`,
      filters: [{ name: "SubRip", extensions: ["srt"] }],
    });
    if (path) await api.exportSubtitles(entry.id, path);
  } catch (e) {
    error.value = String(e);
  }
}

// --- Drag and drop ------------------------------------------------------------

const isSubtitleFile = (path: string) => /\.(srt|vtt)$/i.test(path);
const dropActive = ref(false);

async function onDragDrop(payload: DragDropEvent) {
  if (payload.type === "enter") {
    dropActive.value = payload.paths.some(isSubtitleFile);
  } else if (payload.type === "leave") {
    dropActive.value = false;
  } else if (payload.type === "drop") {
    dropActive.value = false;
    const path = payload.paths.find(isSubtitleFile);
    if (!path) {
      if (payload.paths.length) error.value = "Only .srt and .vtt subtitle files can be dropped here.";
      return;
    }
    const entry = selected.value;
    if (!entry) {
      error.value = "Select a movie in the library first, then drop its subtitles.";
      return;
    }
    try {
      const loadedCues = await api.loadSubtitleFile(entry.id, path);
      if (selected.value?.id === entry.id) await onSubtitlesLoaded(loadedCues);
    } catch (e) {
      error.value = String(e);
    }
  }
}

// --- Commands & keyboard --------------------------------------------------------

async function toggleFullscreen() {
  fullscreen.value = !fullscreen.value;
  try {
    await getCurrentWindow().setFullscreen(fullscreen.value);
  } catch (e) {
    fullscreen.value = !fullscreen.value;
    error.value = String(e);
  }
}

function openSettings(tab: SettingsTab = "subtitles") {
  settingsTab.value = tab;
}

function togglePanel() {
  if (panelAvailable.value) subsPanel.open.value = !subsPanel.open.value;
}

async function focusLineFilter() {
  if (!cues.value.length || fullscreen.value) return;
  subsPanel.open.value = true;
  // Wait for the panel to mount if it was hidden.
  await nextTick();
  await panel.value?.focusFilter();
}

const modalOpen = computed(() => settingsTab.value !== null || showShortcuts.value);

function onKey(e: KeyboardEvent) {
  if (modalOpen.value || e.defaultPrevented) return;

  // App-wide commands with the platform modifier work even while typing.
  if (hasMod(e) && !e.altKey) {
    switch (e.code) {
      case "Comma":
        openSettings();
        break;
      case "KeyL":
        if (fullscreen.value) return;
        toolbar.value?.focusSearch();
        break;
      case "KeyF":
        if (!cues.value.length || fullscreen.value) return;
        focusLineFilter();
        break;
      default:
        return;
    }
    e.preventDefault();
    return;
  }
  if (e.metaKey || e.ctrlKey || e.altKey) return;

  const target = e.target as HTMLElement;
  if (target.closest("input, textarea, select")) return;

  // `code` keeps shortcuts working with non-Latin keyboard layouts.
  switch (e.code) {
    case "Slash":
      if (!e.shiftKey) return;
      showShortcuts.value = true;
      e.preventDefault();
      return;
    case "BracketLeft":
      if (fullscreen.value) return;
      library.open.value = !library.open.value;
      e.preventDefault();
      return;
    case "BracketRight":
      if (fullscreen.value) return;
      togglePanel();
      e.preventDefault();
      return;
  }

  if (!selected.value) return;
  const step = e.shiftKey ? 1000 : 100;
  switch (e.code) {
    case "KeyG":
      if (cues.value.length) offsetMs.value -= step;
      break;
    case "KeyH":
      if (cues.value.length) offsetMs.value += step;
      break;
    case "Space":
      player.value?.togglePlay();
      break;
    case "ArrowLeft":
      player.value?.seekBy(-5000);
      break;
    case "ArrowRight":
      player.value?.seekBy(5000);
      break;
    case "KeyT":
      if (translations.value) {
        const order: SubtitleMode[] = ["original", "both", "translation"];
        subtitleMode.value = order[(order.indexOf(subtitleMode.value) + 1) % order.length];
      }
      break;
    case "Minus":
      changeFontScale(-0.1);
      break;
    case "Equal":
      changeFontScale(0.1);
      break;
    case "KeyF":
      toggleFullscreen();
      break;
    case "Escape":
      if (!fullscreen.value) return;
      toggleFullscreen();
      break;
    default:
      return;
  }
  e.preventDefault();
}

const hasSubtitleKeys = computed(() => !!settings.value && (!!settings.value.opensubtitlesApiKey || !!settings.value.subdlApiKey));
</script>

<template>
  <div class="app" :class="{ fullscreen, resizing }">
    <AppToolbar
      v-if="!fullscreen"
      ref="toolbar"
      :library-open="library.open.value"
      :panel-open="subsPanel.open.value"
      :panel-available="panelAvailable"
      @toggle-library="library.open.value = !library.open.value"
      @toggle-panel="togglePanel"
      @open-settings="openSettings()"
      @downloaded="onDownloaded"
      @error="error = $event"
    />

    <main :style="{ gridTemplateColumns: columns }">
      <LibraryList
        v-if="showLibrary"
        :entries="entries"
        :selected-id="selected?.id ?? null"
        :sizes="sizes"
        @select="select"
        @deleted="onDeleted"
        @error="error = $event"
      />

      <section class="stage">
        <template v-if="selected">
          <VideoPlayer
            ref="player"
            :src="videoSrc"
            :cues="cues"
            :offset-ms="offsetMs"
            :translations="translations"
            :mode="subtitleMode"
            :font-scale="fontScale"
            :start-ms="selected.positionMs"
            :overlay-controls="fullscreen"
            @font-scale="changeFontScale"
            @time="timeMs = $event"
            @position="savePosition(selected.id, $event)"
            @toggle-fullscreen="toggleFullscreen"
          />
          <PlayerBar
            v-if="!fullscreen"
            v-model="offsetMs"
            :disabled="!cues.length"
            :font-scale="fontScale"
            :min-font-scale="MIN_FONT_SCALE"
            :max-font-scale="MAX_FONT_SCALE"
            @font-scale="changeFontScale"
            @export="exportSrt"
            @show-shortcuts="showShortcuts = true"
          />
        </template>

        <div v-else-if="loaded" class="welcome">
          <div class="welcome-card">
            <div class="welcome-icon"><Icon name="film" :size="24" /></div>
            <h1>{{ entries.length ? "Pick a movie" : "Watch anything with subtitles" }}</h1>
            <p class="muted">
              <template v-if="entries.length">Choose a movie from the library, or find a new one.</template>
              <template v-else>
                Search for a movie by its title in any language — the original is found automatically — or paste a
                link from YouTube, VK Video or OK.ru.
              </template>
            </p>
            <div class="welcome-actions">
              <button class="primary" @click="toolbar?.focusSearch()">
                <Icon name="search" :size="14" /> Find a movie
                <span class="keys"><kbd>{{ modKey }}</kbd><kbd>L</kbd></span>
              </button>
              <button v-if="!hasSubtitleKeys" @click="openSettings('subtitles')">
                <Icon name="sliders" :size="14" /> Set up subtitle sources
              </button>
            </div>
          </div>
        </div>
      </section>

      <SubtitlePanel
        v-if="showPanel && selected && settings"
        ref="panel"
        :entry="selected"
        :cues="cues"
        :time-ms="timeMs"
        :offset-ms="offsetMs"
        v-model:mode="subtitleMode"
        :default-language="settings.language"
        :translate-to="settings.translateTo"
        :translations="translations"
        @loaded="onSubtitlesLoaded"
        @removed="onSubtitlesRemoved"
        @seek="player?.seek($event)"
        @speak="player?.pause()"
        @set-offset="offsetMs = $event"
        @translated="onTranslated"
      />

      <!-- Drag handles on the panel edges; double-click restores the default width. -->
      <div
        v-if="showLibrary"
        class="resizer"
        :class="{ active: library.resizing.value }"
        :style="{ left: `${library.width.value - 3}px` }"
        title="Drag to resize"
        @pointerdown.prevent="library.startResize"
        @dblclick="library.resetWidth"
      />
      <div
        v-if="showPanel"
        class="resizer"
        :class="{ active: subsPanel.resizing.value }"
        :style="{ right: `${subsPanel.width.value - 3}px` }"
        title="Drag to resize"
        @pointerdown.prevent="subsPanel.startResize"
        @dblclick="subsPanel.resetWidth"
      />
    </main>

    <div v-if="dropActive" class="drop-overlay">
      <div class="drop-card">
        <Icon name="captions" :size="28" />
        <p v-if="selected">Drop to use these subtitles for<br /><strong>{{ selected.title }}</strong></p>
        <p v-else>Select a movie first</p>
      </div>
    </div>

    <Toast v-if="error" :message="error" @dismiss="error = ''" />

    <SettingsDialog
      v-if="settingsTab && settings"
      :settings="settings"
      :initial-tab="settingsTab"
      @close="settingsTab = null"
      @saved="
        settings = $event;
        settingsTab = null;
      "
    />
    <ShortcutsDialog v-if="showShortcuts" @close="showShortcuts = false" />
  </div>
</template>

<style scoped>
.app {
  height: 100%;
  display: flex;
  flex-direction: column;
}
.app.resizing {
  cursor: col-resize;
}
.app.resizing * {
  pointer-events: none;
}
.app.resizing .resizer.active {
  pointer-events: auto;
}
main {
  position: relative;
  flex: 1;
  min-height: 0;
  display: grid;
}
.resizer {
  position: absolute;
  top: 0;
  bottom: 0;
  z-index: 10;
  width: 6px;
  cursor: col-resize;
  touch-action: none;
}
.resizer::after {
  content: "";
  position: absolute;
  top: 0;
  bottom: 0;
  left: 2px;
  width: 2px;
  background: var(--accent);
  opacity: 0;
  transition: opacity var(--dur) var(--ease);
}
.resizer:hover::after {
  opacity: 0.6;
  transition-delay: 150ms;
}
.resizer.active::after {
  opacity: 1;
  transition-delay: 0ms;
}
.stage {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: #000;
}

.welcome {
  flex: 1;
  display: grid;
  place-items: center;
  padding: var(--s-6);
  background: var(--bg-app);
}
.welcome-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  max-width: 420px;
  text-align: center;
}
.welcome-icon {
  display: grid;
  place-items: center;
  width: 52px;
  height: 52px;
  margin-bottom: var(--s-4);
  border-radius: var(--r-xl);
  background: var(--bg-control);
  color: var(--text-secondary);
}
.welcome h1 {
  font-size: var(--fs-xl);
  font-weight: var(--fw-semibold);
  letter-spacing: -0.01em;
}
.welcome p {
  margin-top: var(--s-2);
}
.welcome-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: var(--s-2);
  margin-top: var(--s-5);
}
.welcome-actions button {
  height: var(--h-lg);
}
.welcome-actions .keys {
  display: flex;
  gap: 2px;
  margin-left: var(--s-1);
}
.welcome-actions .keys kbd {
  color: rgba(255, 255, 255, 0.85);
  background: rgba(255, 255, 255, 0.15);
  border-color: transparent;
}

.drop-overlay {
  position: fixed;
  inset: 0;
  z-index: 80;
  display: grid;
  place-items: center;
  padding: var(--s-6);
  background: rgba(10, 12, 18, 0.7);
  pointer-events: none;
  animation: fade var(--dur) var(--ease);
}
.drop-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--s-3);
  padding: var(--s-8);
  min-width: 320px;
  text-align: center;
  color: var(--text-secondary);
  border: 2px dashed var(--border-focus);
  border-radius: var(--r-xl);
  background: var(--accent-soft);
}
.drop-card svg {
  color: var(--accent-text);
}
.drop-card strong {
  color: var(--text);
}
@keyframes fade {
  from {
    opacity: 0;
  }
}
</style>
