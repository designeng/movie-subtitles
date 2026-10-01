<script setup lang="ts">
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { save } from "@tauri-apps/plugin-dialog";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { api, type Cue, type LibraryEntry, type Settings, type SubtitleMode } from "./api";
import LibraryList from "./components/LibraryList.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import SubtitlePanel from "./components/SubtitlePanel.vue";
import SyncControls from "./components/SyncControls.vue";
import UrlBar from "./components/UrlBar.vue";
import VideoPlayer from "./components/VideoPlayer.vue";

const entries = ref<LibraryEntry[]>([]);
const selected = ref<LibraryEntry | null>(null);
const cues = ref<Cue[]>([]);
const translations = ref<string[] | null>(null);
const subtitleMode = ref<SubtitleMode>("original");
const offsetMs = ref(0);
const timeMs = ref(0);
const settings = ref<Settings | null>(null);
const showSettings = ref(false);
const fullscreen = ref(false);
const error = ref("");
const player = ref<InstanceType<typeof VideoPlayer>>();

const videoSrc = computed(() => (selected.value ? convertFileSrc(selected.value.videoPath) : ""));

onMounted(async () => {
  [entries.value, settings.value] = await Promise.all([api.listLibrary(), api.getSettings()]);
  if (entries.value.length) await select(entries.value[0]);
  if (!settings.value.opensubtitlesApiKey) showSettings.value = true;
  window.addEventListener("keydown", onKey);
});
onUnmounted(() => window.removeEventListener("keydown", onKey));

async function select(entry: LibraryEntry) {
  selected.value = entry;
  offsetMs.value = entry.offsetMs;
  cues.value = [];
  translations.value = null;
  try {
    cues.value = (await api.getSubtitles(entry.id)) ?? [];
  } catch (e) {
    error.value = String(e);
  }
}

function replaceEntry(entry: LibraryEntry) {
  entries.value = entries.value.map((e) => (e.id === entry.id ? entry : e));
  if (selected.value?.id === entry.id) selected.value = entry;
}

async function onDownloaded(entry: LibraryEntry) {
  entries.value = [entry, ...entries.value.filter((e) => e.id !== entry.id)];
  await select(entry);
}

function onDeleted(id: string) {
  entries.value = entries.value.filter((e) => e.id !== id);
  if (selected.value?.id === id) {
    selected.value = null;
    cues.value = [];
    translations.value = null;
  }
}

async function onSubtitlesLoaded(loaded: Cue[]) {
  cues.value = loaded;
  translations.value = null;
  offsetMs.value = 0;
  // The backend updated the label and reset the offset.
  entries.value = await api.listLibrary();
  selected.value = entries.value.find((e) => e.id === selected.value?.id) ?? null;
}

function onSubtitlesRemoved(entry: LibraryEntry) {
  cues.value = [];
  translations.value = null;
  offsetMs.value = 0;
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

async function exportSrt() {
  const entry = selected.value;
  if (!entry) return;
  clearTimeout(saveTimer);
  await api.setOffset(entry.id, offsetMs.value);
  const path = await save({
    defaultPath: `${entry.searchQuery || entry.title}.srt`,
    filters: [{ name: "SubRip", extensions: ["srt"] }],
  });
  if (!path) return;
  try {
    await api.exportSubtitles(entry.id, path);
  } catch (e) {
    error.value = String(e);
  }
}

async function toggleFullscreen() {
  fullscreen.value = !fullscreen.value;
  await getCurrentWindow().setFullscreen(fullscreen.value);
}

function onKey(e: KeyboardEvent) {
  const target = e.target as HTMLElement;
  if (target.closest("input, textarea, select") || showSettings.value || !selected.value) return;
  const step = e.shiftKey ? 1000 : 100;
  // `code` keeps shortcuts working with non-Latin keyboard layouts.
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
    case "KeyF":
      toggleFullscreen();
      break;
    case "Escape":
      if (fullscreen.value) toggleFullscreen();
      break;
    default:
      return;
  }
  e.preventDefault();
}
</script>

<template>
  <div class="app" :class="{ fullscreen }">
    <UrlBar v-if="!fullscreen" @downloaded="onDownloaded" @open-settings="showSettings = true" />
    <div v-if="error" class="banner error" @click="error = ''">{{ error }} <span class="muted">(click to dismiss)</span></div>
    <main>
      <LibraryList
        v-if="!fullscreen"
        :entries="entries"
        :selected-id="selected?.id ?? null"
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
            @time="timeMs = $event"
            @toggle-fullscreen="toggleFullscreen"
          />
          <SyncControls v-if="!fullscreen" v-model="offsetMs" :disabled="!cues.length" @export="exportSrt" />
        </template>
        <div v-else class="placeholder muted">
          <p>Paste a YouTube or VK Video link above to download a movie.</p>
        </div>
      </section>
      <SubtitlePanel
        v-if="selected && settings && !fullscreen"
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
        @set-offset="offsetMs = $event"
        @translated="translations = $event"
      />
    </main>
    <SettingsDialog
      v-if="showSettings && settings"
      :settings="settings"
      @close="showSettings = false"
      @saved="
        settings = $event;
        showSettings = false;
      "
    />
  </div>
</template>

<style scoped>
.app {
  height: 100%;
  display: flex;
  flex-direction: column;
}
main {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr) 360px;
}
.fullscreen main {
  grid-template-columns: 1fr;
}
.stage {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}
.placeholder {
  flex: 1;
  display: grid;
  place-items: center;
  text-align: center;
  padding: 16px;
}
.banner {
  padding: 8px 16px;
  background: var(--panel-2);
  border-bottom: 1px solid var(--border);
  cursor: pointer;
}
</style>
