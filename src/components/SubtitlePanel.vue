<script setup lang="ts">
import { ask, open } from "@tauri-apps/plugin-dialog";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { api, type Cue, type LibraryEntry, type SubtitleCandidate, type SubtitleMode } from "../api";
import { isMac } from "../platform";
import { cueIndexAt, formatTime } from "../subtitles";
import Icon from "./ui/Icon.vue";
import Segmented from "./ui/Segmented.vue";

const props = defineProps<{
  entry: LibraryEntry;
  cues: Cue[];
  timeMs: number;
  offsetMs: number;
  defaultLanguage: string;
  translateTo: string;
  translations: string[] | null;
}>();
const mode = defineModel<SubtitleMode>("mode", { required: true });
const emit = defineEmits<{
  loaded: [cues: Cue[]];
  removed: [entry: LibraryEntry];
  seek: [ms: number];
  /** A line is about to be read aloud: the video should stop talking over it. */
  speak: [];
  setOffset: [ms: number];
  translated: [lines: string[], target: string];
}>();

type Tab = "lines" | "search";
const tab = ref<Tab>(props.cues.length ? "lines" : "search");
const tabs = computed(() => [
  { value: "lines" as Tab, label: "Lines", count: props.cues.length || undefined, disabled: !props.cues.length },
  { value: "search" as Tab, label: props.cues.length ? "Replace" : "Find subtitles" },
]);
const modes: { value: SubtitleMode; label: string }[] = [
  { value: "original", label: "Original" },
  { value: "both", label: "Both" },
  { value: "translation", label: "Translation" },
];

const query = ref(props.entry.searchQuery);
const language = ref(props.defaultLanguage);
const results = ref<SubtitleCandidate[]>([]);
const searching = ref(false);
/** Whether a search ran for the current movie, to tell "no results" from "not searched yet". */
const searched = ref(false);
const fetchingId = ref<string | null>(null);
const error = ref("");
/** Provider failures, shown alongside the results of the providers that worked. */
const providerErrors = ref<string[]>([]);
/** Subtitle files already on disk for this movie, other than the loaded ones. */
const localFiles = ref<string[]>([]);
const link = ref("");
const loadingLink = ref(false);

watch(
  () => props.entry.id,
  () => {
    query.value = props.entry.searchQuery;
    translateError.value = "";
    results.value = [];
    searched.value = false;
    error.value = "";
    providerErrors.value = [];
    link.value = "";
    stopSpeaking();
    speakError.value = "";
    filter.value = "";
    tab.value = props.cues.length ? "lines" : "search";
    init();
  },
);
onMounted(init);

async function init() {
  const id = props.entry.id;
  localFiles.value = [];
  await refreshLocalFiles();
  if (id === props.entry.id) autoSearch();
}

async function refreshLocalFiles() {
  const id = props.entry.id;
  try {
    const found = await api.localSubtitles(id);
    if (id === props.entry.id) localFiles.value = found;
  } catch {
    // Not essential: the search still works.
  }
}

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;
watch(
  () => props.cues.length,
  (n) => {
    if (n) tab.value = "lines";
    else if (tab.value === "lines") tab.value = "search";
  },
);
// Subtitles can also be loaded from outside the panel (dropped onto the window).
watch(() => props.entry.subtitleLabel, refreshLocalFiles);

// Guards against a slow search for a previous movie overwriting the current results.
let searchSeq = 0;

async function search() {
  const seq = ++searchSeq;
  searching.value = true;
  error.value = "";
  providerErrors.value = [];
  results.value = [];
  try {
    const report = await api.searchSubtitles(query.value, language.value);
    if (seq !== searchSeq) return;
    results.value = report.candidates;
    providerErrors.value = report.errors;
    searched.value = true;
    if (!report.candidates.length && !report.errors.length) {
      error.value =
        `No “${language.value}” subtitles for “${query.value.trim()}” on ${report.searched.join(" or ")}. ` +
        "Try a shorter or different title, or paste a direct link below.";
    }
  } catch (e) {
    if (seq === searchSeq) error.value = String(e);
  } finally {
    if (seq === searchSeq) searching.value = false;
  }
}

/** Searches by the movie title right away when the movie has no subtitles yet. */
function autoSearch() {
  if (!props.entry.subtitlePath && !localFiles.value.length && query.value.trim()) {
    search();
  } else {
    searchSeq++;
    searching.value = false;
  }
}

async function pick(candidate: SubtitleCandidate) {
  fetchingId.value = candidate.id;
  error.value = "";
  try {
    emit("loaded", await api.fetchSubtitles(props.entry.id, candidate));
  } catch (e) {
    error.value = String(e);
  } finally {
    fetchingId.value = null;
  }
}

async function openFile() {
  const path = await open({
    multiple: false,
    filters: [{ name: "Subtitles", extensions: ["srt", "vtt"] }],
  });
  if (path) await loadFile(path);
}

async function loadFile(path: string) {
  error.value = "";
  try {
    emit("loaded", await api.loadSubtitleFile(props.entry.id, path));
    await refreshLocalFiles();
  } catch (e) {
    error.value = String(e);
  }
}

async function loadLink() {
  loadingLink.value = true;
  error.value = "";
  try {
    emit("loaded", await api.downloadSubtitleUrl(props.entry.id, link.value));
    link.value = "";
  } catch (e) {
    error.value = String(e);
  } finally {
    loadingLink.value = false;
  }
}

async function removeSubtitles() {
  const confirmed = await ask(
    "The subtitle file, its translations and the sync offset will be deleted. You can search for subtitles again afterwards.",
    { title: "Remove subtitles?", kind: "warning", okLabel: "Remove", cancelLabel: "Cancel" },
  );
  if (!confirmed) return;
  try {
    emit("removed", await api.removeSubtitles(props.entry.id));
    tab.value = "search";
    await refreshLocalFiles();
  } catch (e) {
    error.value = String(e);
  }
}

const providerName = (p: string) => (p === "subdl" ? "SubDL" : "OpenSubtitles");

// --- Translation -----------------------------------------------------------

const target = ref(props.translateTo);
const translating = ref(false);
const translateError = ref("");

async function translate() {
  translating.value = true;
  translateError.value = "";
  try {
    const lang = target.value.trim().toLowerCase();
    emit("translated", await api.translateSubtitles(props.entry.id, lang), lang);
    if (mode.value === "original") mode.value = "both";
  } catch (e) {
    translateError.value = String(e);
  } finally {
    translating.value = false;
  }
}

// --- Lines tab -------------------------------------------------------------

const filter = ref("");
const filterInput = ref<HTMLInputElement>();
const currentIndex = computed(() => cueIndexAt(props.cues, props.timeMs, props.offsetMs));
// The list follows the same Original / Translation / Both switch as the video.
const showOriginal = (i: number) => !props.translations?.[i] || mode.value !== "translation";
const showTranslation = (i: number) => !!props.translations?.[i] && mode.value !== "original";
const shown = computed(() => {
  const f = filter.value.trim().toLowerCase();
  const all = props.cues.map((cue, index) => ({ cue, index }));
  return f
    ? all.filter(({ cue, index }) =>
        [showOriginal(index) && cue.text, showTranslation(index) && props.translations?.[index]].some(
          (t) => t && t.toLowerCase().includes(f),
        ),
      )
    : all;
});

const list = ref<HTMLElement>();
const follow = ref(true);
watch(currentIndex, async (i) => {
  if (!follow.value || filter.value || tab.value !== "lines") return;
  await nextTick();
  list.value?.querySelector(`[data-index="${i}"]`)?.scrollIntoView({ block: "center" });
});

/** Shows the ⏱ buttons: find the line being spoken (e.g. with the filter) and snap the subtitles to it. */
const syncMode = ref(localStorage.getItem("syncMode") === "1");
watch(syncMode, (on) => localStorage.setItem("syncMode", on ? "1" : "0"));

/** "This line is being spoken right now" — align the cue's start to the current time. */
async function syncHere(cue: Cue, index: number) {
  emit("setOffset", Math.round(props.timeMs - cue.startMs));
  // Back to the full list, centred on the line that is now current, for fine-tuning.
  filter.value = "";
  await nextTick();
  list.value?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "center" });
}

// --- Reading lines aloud ---------------------------------------------------

/** Slower speech, to make out the words. */
const slow = ref(localStorage.getItem("ttsSlow") === "1");
watch(slow, (on) => localStorage.setItem("ttsSlow", on ? "1" : "0"));

/** The line being generated or played. */
const speakingIndex = ref<number | null>(null);
/** The audio of that line is still being generated (the first one also downloads the model). */
const speakLoading = ref(false);
const speakError = ref("");
let speakSeq = 0;
let audio: HTMLAudioElement | null = null;
let audioUrl = "";

function stopSpeaking() {
  speakSeq++; // drops a generation still in flight
  audio?.pause();
  audio = null;
  if (audioUrl) URL.revokeObjectURL(audioUrl);
  audioUrl = "";
  speakingIndex.value = null;
  speakLoading.value = false;
}
onBeforeUnmount(stopSpeaking);

/** Reads the line aloud; clicking the same line again stops it. */
async function speak(cue: Cue, index: number) {
  const wasSpeaking = speakingIndex.value === index;
  stopSpeaking();
  if (wasSpeaking) return;

  const seq = speakSeq;
  speakingIndex.value = index;
  speakLoading.value = true;
  speakError.value = "";
  emit("speak");
  try {
    const wav = await api.speak(cue.text, slow.value ? 0.7 : 1);
    if (seq !== speakSeq) return;
    audioUrl = URL.createObjectURL(new Blob([wav], { type: "audio/wav" }));
    audio = new Audio(audioUrl);
    audio.onended = () => seq === speakSeq && stopSpeaking();
    speakLoading.value = false;
    await audio.play();
  } catch (e) {
    if (seq !== speakSeq) return;
    speakError.value = String(e);
    stopSpeaking();
  }
}

function onLineClick(e: MouseEvent, cue: Cue, index: number) {
  if (e.shiftKey && (e.metaKey || e.ctrlKey)) syncHere(cue, index);
  else emit("seek", cue.startMs + props.offsetMs);
}

const syncClick = isMac ? "⇧⌘-click" : "Ctrl+Shift-click";

defineExpose({
  async focusFilter() {
    if (!props.cues.length) return;
    tab.value = "lines";
    await nextTick();
    filterInput.value?.focus();
    filterInput.value?.select();
  },
});
</script>

<template>
  <aside class="panel" aria-label="Subtitles">
    <header class="head">
      <Segmented v-model="tab" :options="tabs" block aria-label="Subtitle panel view" />
    </header>

    <!-- Currently loaded subtitles -->
    <div v-if="entry.subtitleLabel" class="source">
      <Icon name="captions" :size="14" class="source-icon" />
      <span class="truncate grow" :title="entry.subtitlePath ?? entry.subtitleLabel">{{ entry.subtitleLabel }}</span>
      <button class="ghost danger icon sm" title="Remove subtitles…" aria-label="Remove subtitles" @click="removeSubtitles">
        <Icon name="trash" :size="14" />
      </button>
    </div>

    <!-- Lines -->
    <section v-if="tab === 'lines'" class="lines">
      <div class="block translate">
        <div class="row">
          <Icon name="languages" :size="14" class="subtle" />
          <span class="muted grow">Translate to</span>
          <input
            v-model="target"
            class="lang"
            placeholder="ru"
            title="Target language code (ISO 639-1)"
            aria-label="Target language"
            @keydown.enter="target.trim() && !translating && translate()"
          />
          <button class="sm" :disabled="translating || !target.trim()" @click="translate">
            <span v-if="translating" class="spinner" />
            {{ translating ? "Translating" : "Translate" }}
          </button>
        </div>
        <Segmented v-if="translations" v-model="mode" :options="modes" block aria-label="Subtitle language (T)" />
        <div v-if="translateError" class="notice error">
          <Icon name="alert" :size="14" />
          <span class="grow">{{ translateError }}</span>
          <button class="ghost icon sm" title="Dismiss" aria-label="Dismiss error" @click="translateError = ''">
            <Icon name="x" :size="12" />
          </button>
        </div>
      </div>

      <div class="block filter-row">
        <div class="filter">
          <Icon name="search" :size="14" class="lead" />
          <input
            ref="filterInput"
            v-model="filter"
            placeholder="Filter lines"
            aria-label="Filter lines"
            @keydown.esc="filter ? (filter = '') : ($event.target as HTMLInputElement).blur()"
          />
        </div>
        <button
          class="ghost icon"
          :aria-pressed="follow"
          title="Follow playback: keep the current line in view"
          aria-label="Follow playback"
          @click="follow = !follow"
        >
          <Icon name="locate" :size="15" />
        </button>
        <button
          class="ghost icon"
          :aria-pressed="syncMode"
          title="Sync mode: show ⏱ buttons to snap the subtitles to a line"
          aria-label="Sync mode"
          @click="syncMode = !syncMode"
        >
          <Icon name="timer" :size="15" />
        </button>
        <button
          class="ghost icon"
          :aria-pressed="slow"
          title="Read lines aloud slowly"
          aria-label="Slow speech"
          @click="slow = !slow"
        >
          <Icon name="turtle" :size="15" />
        </button>
      </div>
      <div v-if="speakError" class="notice error">
        <Icon name="alert" :size="14" />
        <span class="grow">{{ speakError }}</span>
        <button class="ghost icon sm" title="Dismiss" aria-label="Dismiss error" @click="speakError = ''">
          <Icon name="x" :size="12" />
        </button>
      </div>
      <p v-if="syncMode" class="tip">
        Find the line being spoken right now and press <Icon name="timer" :size="12" class="inline" /> (or
        {{ syncClick }} it). All subtitles shift to match.
      </p>

      <ol v-if="shown.length" ref="list" class="cue-list">
        <li
          v-for="{ cue, index } in shown"
          :key="index"
          :data-index="index"
          :class="{ current: index === currentIndex }"
          @click="onLineClick($event, cue, index)"
          @mousedown="$event.shiftKey && $event.preventDefault()"
        >
          <span class="time tabular">{{ formatTime(cue.startMs + offsetMs) }}</span>
          <span class="text">
            <template v-if="showOriginal(index)">{{ cue.text }}</template>
            <span v-if="showTranslation(index)" class="tr">{{ translations?.[index] }}</span>
          </span>
          <button
            class="ghost icon sm speak"
            :class="{ on: speakingIndex === index }"
            :title="
              speakingIndex !== index
                ? 'Read this line aloud'
                : speakLoading
                  ? 'Preparing speech… (the first time this downloads the speech model, ~150 MB). Click to cancel.'
                  : 'Stop'
            "
            :aria-label="speakingIndex === index ? 'Stop reading' : 'Read this line aloud'"
            @click.stop="speak(cue, index)"
          >
            <span v-if="speakingIndex === index && speakLoading" class="spinner" />
            <Icon v-else :name="speakingIndex === index ? 'stop' : 'volume'" :size="14" />
          </button>
          <button
            v-if="syncMode"
            class="ghost icon sm sync"
            :title="`This line is spoken now: sync to it (${syncClick})`"
            aria-label="Sync to this line"
            @click.stop="syncHere(cue, index)"
          >
            <Icon name="timer" :size="14" />
          </button>
        </li>
      </ol>
      <p v-else class="empty subtle">No lines match “{{ filter }}”.</p>
    </section>

    <!-- Find / replace subtitles -->
    <section v-else class="search">
      <form class="block row" @submit.prevent="search">
        <input v-model="query" class="grow" placeholder="Movie title" aria-label="Movie title" />
        <input v-model="language" class="lang" placeholder="en" title="Language code (ISO 639-1)" aria-label="Language" />
        <button class="primary icon" type="submit" :disabled="searching || !query.trim()" title="Search" aria-label="Search">
          <span v-if="searching" class="spinner" />
          <Icon v-else name="search" :size="14" />
        </button>
      </form>

      <div class="scroll">
        <div v-if="error || providerErrors.length" class="block messages">
          <div v-if="error" class="notice error">
            <Icon name="alert" :size="14" />
            <span class="grow">{{ error }}</span>
          </div>
          <div v-for="e in providerErrors" :key="e" class="notice error">
            <Icon name="alert" :size="14" />
            <span class="grow">{{ e }}</span>
          </div>
        </div>

        <div v-if="searching" class="loading muted"><span class="spinner" /> Searching OpenSubtitles and SubDL…</div>

        <ul v-if="results.length" class="results">
          <li v-for="c in results" :key="`${c.provider}-${c.id}`">
            <div class="grow">
              <div class="release truncate" :title="c.release || c.title">{{ c.release || c.title }}</div>
              <div class="meta">
                <span class="badge">{{ c.language.toUpperCase() }}</span>
                <span v-if="c.hearingImpaired" class="badge" title="For the hearing impaired">HI</span>
                <span class="truncate">
                  {{ c.title }}<template v-if="c.year"> ({{ c.year }})</template>
                </span>
              </div>
              <div class="meta subtle">
                {{ providerName(c.provider) }}
                <template v-if="c.downloads"> · {{ c.downloads.toLocaleString() }} downloads</template>
                <template v-if="c.fps"> · {{ c.fps }} fps</template>
              </div>
            </div>
            <button class="sm" :disabled="fetchingId !== null" @click="pick(c)">
              <span v-if="fetchingId === c.id" class="spinner" />
              <template v-else>Use</template>
            </button>
          </li>
        </ul>
        <p v-else-if="!searching && !searched && !error" class="empty subtle">
          Search by title, or use one of the options below.
        </p>

        <div class="block other">
          <div class="section-label">Other sources</div>
          <div v-if="localFiles.length" class="on-disk">
            <div v-for="path in localFiles" :key="path" class="file-row">
              <Icon name="file" :size="14" class="subtle" />
              <span class="grow truncate" :title="path">{{ fileName(path) }}</span>
              <button class="sm" @click="loadFile(path)">Use</button>
            </div>
          </div>
          <form class="row" @submit.prevent="loadLink">
            <input
              v-model="link"
              class="grow"
              placeholder="Link to .srt, .vtt or .zip"
              aria-label="Subtitle link"
              spellcheck="false"
            />
            <button type="submit" class="sm" :disabled="loadingLink || !link.trim()">
              <span v-if="loadingLink" class="spinner" />
              <template v-else>Load</template>
            </button>
          </form>
          <button class="open-file" @click="openFile"><Icon name="file" :size="14" /> Open subtitle file…</button>
          <p class="subtle hint"><Icon name="drop" :size="12" class="inline" /> Or drop an .srt / .vtt file onto the window.</p>
        </div>
      </div>
    </section>
  </aside>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--bg-panel);
  border-left: 1px solid var(--border);
}
.head {
  display: flex;
  align-items: center;
  height: var(--panel-header-h);
  padding: 0 var(--s-3);
  flex-shrink: 0;
}
.source {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  margin: 0 var(--s-3) var(--s-2);
  padding: 4px 4px 4px var(--s-2);
  font-size: var(--fs-sm);
  color: var(--text-secondary);
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--r-md);
}
.source-icon {
  color: var(--accent-text);
}
.grow {
  flex: 1;
  min-width: 0;
}
.row {
  display: flex;
  align-items: center;
  gap: 6px;
}
.block {
  padding: 0 var(--s-3);
}
.lang {
  width: 44px;
  text-align: center;
}
.inline {
  display: inline-block;
  vertical-align: -2px;
}

section {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

/* --- Lines --- */
.translate {
  display: flex;
  flex-direction: column;
  gap: var(--s-2);
  padding-bottom: var(--s-3);
  border-bottom: 1px solid var(--border);
}
.translate .lang {
  height: var(--h-sm);
  font-size: var(--fs-sm);
}
.translate .row {
  font-size: var(--fs-sm);
}
.filter-row {
  display: flex;
  align-items: center;
  gap: 2px;
  padding-top: var(--s-2);
  padding-bottom: var(--s-2);
}
.filter {
  position: relative;
  flex: 1;
  min-width: 0;
  margin-right: var(--s-1);
}
.filter .lead {
  position: absolute;
  left: 8px;
  top: 7px;
  color: var(--text-tertiary);
  pointer-events: none;
}
.filter input {
  width: 100%;
  padding-left: 28px;
}
.tip {
  margin: 0 var(--s-3) var(--s-2);
  padding: var(--s-2) var(--s-3);
  font-size: var(--fs-sm);
  color: var(--text-secondary);
  background: var(--accent-soft);
  border-radius: var(--r-md);
}
.cue-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  list-style: none;
  margin: 0;
  padding: 0 var(--s-2) var(--s-2);
}
.cue-list li {
  position: relative;
  display: flex;
  align-items: flex-start;
  gap: var(--s-2);
  padding: 6px var(--s-2);
  border-radius: var(--r-md);
}
.cue-list li:hover {
  background: var(--bg-hover);
}
.cue-list li.current {
  background: var(--bg-selected);
}
.cue-list li.current::before {
  content: "";
  position: absolute;
  left: 0;
  top: 8px;
  bottom: 8px;
  width: 2px;
  border-radius: 1px;
  background: var(--accent);
}
.time {
  min-width: 52px;
  padding-top: 1px;
  font-size: var(--fs-xs);
  color: var(--text-tertiary);
}
.current .time {
  color: var(--accent-text);
}
.text {
  flex: 1;
  min-width: 0;
  white-space: pre-line;
  color: var(--text-secondary);
}
.current .text {
  color: var(--text);
}
.tr {
  display: block;
  color: var(--translation);
  opacity: 0.85;
}
.cue-list .sync,
.cue-list .speak {
  margin: -3px -2px -3px 0;
  opacity: 0.35;
}
.cue-list li:hover .sync,
.cue-list li.current .sync,
.cue-list li:hover .speak,
.cue-list li.current .speak,
.cue-list .speak.on {
  opacity: 1;
}
.empty {
  padding: var(--s-3) var(--s-4);
  font-size: var(--fs-sm);
}

/* --- Search --- */
.search > form {
  padding-bottom: var(--s-3);
  border-bottom: 1px solid var(--border);
}
.scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding-top: var(--s-2);
}
.messages {
  display: flex;
  flex-direction: column;
  gap: var(--s-2);
  margin-bottom: var(--s-2);
}
.loading {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  padding: var(--s-3) var(--s-4);
  font-size: var(--fs-sm);
}
.results {
  list-style: none;
  margin: 0;
  padding: 0 var(--s-2);
}
.results li {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  padding: var(--s-2);
  border-radius: var(--r-md);
}
.results li:hover {
  background: var(--bg-hover);
}
.release {
  font-weight: var(--fw-medium);
}
.meta {
  display: flex;
  align-items: center;
  gap: 5px;
  margin-top: 2px;
  font-size: var(--fs-xs);
  color: var(--text-secondary);
}
.meta .badge {
  flex-shrink: 0;
}
.results button {
  min-width: 48px;
}
.other {
  display: flex;
  flex-direction: column;
  gap: var(--s-2);
  margin-top: var(--s-2);
  padding-top: var(--s-3);
  padding-bottom: var(--s-4);
  border-top: 1px solid var(--border);
}
.on-disk {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.file-row {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  font-size: var(--fs-sm);
}
.other .row button {
  min-width: 48px;
}
.open-file {
  align-self: stretch;
}
.hint {
  font-size: var(--fs-xs);
}
</style>
