<script setup lang="ts">
import { open } from "@tauri-apps/plugin-dialog";
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { api, type Cue, type LibraryEntry, type SubtitleCandidate, type SubtitleMode } from "../api";
import { cueIndexAt, formatTime } from "../subtitles";

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
  setOffset: [ms: number];
  translated: [lines: string[]];
}>();

const tab = ref<"search" | "lines">(props.cues.length ? "lines" : "search");
const query = ref(props.entry.searchQuery);
const language = ref(props.defaultLanguage);
const results = ref<SubtitleCandidate[]>([]);
const searching = ref(false);
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
    error.value = "";
    providerErrors.value = [];
    link.value = "";
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
  },
);

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
  try {
    emit("removed", await api.removeSubtitles(props.entry.id));
    tab.value = "search";
    await refreshLocalFiles();
  } catch (e) {
    error.value = String(e);
  }
}

// --- Translation -----------------------------------------------------------

const target = ref(props.translateTo);
const translating = ref(false);
const translateError = ref("");

async function translate() {
  translating.value = true;
  translateError.value = "";
  try {
    emit("translated", await api.translateSubtitles(props.entry.id, target.value));
    if (mode.value === "original") mode.value = "both";
  } catch (e) {
    translateError.value = String(e);
  } finally {
    translating.value = false;
  }
}

// --- Lines tab -------------------------------------------------------------

const filter = ref("");
const currentIndex = computed(() => cueIndexAt(props.cues, props.timeMs, props.offsetMs));
const shown = computed(() => {
  const f = filter.value.trim().toLowerCase();
  const all = props.cues.map((cue, index) => ({ cue, index }));
  return f ? all.filter(({ cue }) => cue.text.toLowerCase().includes(f)) : all;
});

const list = ref<HTMLElement>();
const follow = ref(true);
watch(currentIndex, async (i) => {
  if (!follow.value || filter.value || tab.value !== "lines") return;
  await nextTick();
  list.value?.querySelector(`[data-index="${i}"]`)?.scrollIntoView({ block: "center" });
});

/** "This line is being spoken right now" — align the cue's start to the current time. */
function syncHere(cue: Cue) {
  emit("setOffset", Math.round(props.timeMs - cue.startMs));
}
</script>

<template>
  <aside class="panel">
    <nav class="tabs">
      <button :class="{ active: tab === 'search' }" @click="tab = 'search'">Find subtitles</button>
      <button :class="{ active: tab === 'lines' }" :disabled="!cues.length" @click="tab = 'lines'">
        Lines <span v-if="cues.length" class="muted">{{ cues.length }}</span>
      </button>
    </nav>

    <div v-if="entry.subtitleLabel" class="current">
      <span class="muted">Loaded:</span>
      <span class="label" :title="entry.subtitlePath ?? entry.subtitleLabel">{{ entry.subtitleLabel }}</span>
      <button class="ghost danger" title="Delete these subtitles" @click="removeSubtitles">✕</button>
    </div>

    <div v-if="cues.length" class="translate">
      <div class="row">
        <span class="muted">Translate to</span>
        <input v-model="target" class="lang" title="Target language code (ISO 639-1)" />
        <button :disabled="translating || !target.trim()" @click="translate">
          {{ translating ? "Translating…" : "Translate" }}
        </button>
      </div>
      <div v-if="translations" class="modes">
        <button :class="{ active: mode === 'original' }" @click="mode = 'original'">Original</button>
        <button :class="{ active: mode === 'translation' }" @click="mode = 'translation'">Translation</button>
        <button :class="{ active: mode === 'both' }" @click="mode = 'both'">Both</button>
      </div>
      <p v-if="translateError" class="error">{{ translateError }}</p>
    </div>

    <section v-if="tab === 'search'" class="search">
      <form class="row" @submit.prevent="search">
        <input v-model="query" class="grow" placeholder="Movie title" />
        <input v-model="language" class="lang" placeholder="en" title="Language code (ISO 639-1)" />
        <button class="primary" type="submit" :disabled="searching || !query.trim()">
          {{ searching ? "…" : "Search" }}
        </button>
      </form>
      <form class="row" @submit.prevent="loadLink">
        <input v-model="link" class="grow" placeholder="Direct link to .srt / .vtt / .zip" spellcheck="false" />
        <button type="submit" :disabled="loadingLink || !link.trim()">{{ loadingLink ? "…" : "Load" }}</button>
      </form>
      <button class="ghost" @click="openFile">Open local .srt / .vtt…</button>
      <div v-if="localFiles.length" class="on-disk">
        <div class="muted">Already on disk for this movie:</div>
        <div v-for="path in localFiles" :key="path" class="row">
          <span class="grow file" :title="path">{{ fileName(path) }}</span>
          <button @click="loadFile(path)">Use</button>
        </div>
      </div>
      <p v-if="error" class="error">{{ error }}</p>
      <p v-for="e in providerErrors" :key="e" class="error">{{ e }}</p>
      <ul class="results">
        <li v-for="c in results" :key="`${c.provider}-${c.id}`">
          <div class="grow">
            <div class="release" :title="c.release">{{ c.release || c.title }}</div>
            <div class="muted meta">
              {{ c.title }}<template v-if="c.year"> ({{ c.year }})</template> · {{ c.language }}
              <template v-if="c.downloads"> · ⬇ {{ c.downloads.toLocaleString() }}</template>
              <template v-if="c.fps"> · {{ c.fps }} fps</template>
              · {{ c.provider === "subdl" ? "SubDL" : "OpenSubtitles" }}
              <template v-if="c.hearingImpaired"> · HI</template>
            </div>
          </div>
          <button :disabled="fetchingId !== null" @click="pick(c)">
            {{ fetchingId === c.id ? "…" : "Use" }}
          </button>
        </li>
      </ul>
    </section>

    <section v-else class="lines">
      <div class="row">
        <input v-model="filter" class="grow" placeholder="Filter lines…" />
        <label class="muted follow"><input v-model="follow" type="checkbox" /> follow</label>
      </div>
      <p class="muted tip">
        Click a line to jump to it. Press ⏱ while that line is being spoken to sync all subtitles to it.
      </p>
      <ol ref="list">
        <li
          v-for="{ cue, index } in shown"
          :key="index"
          :data-index="index"
          :class="{ current: index === currentIndex }"
          @click="emit('seek', cue.startMs + offsetMs)"
        >
          <span class="time muted">{{ formatTime(cue.startMs + offsetMs) }}</span>
          <span class="text">
            {{ cue.text }}
            <span v-if="translations?.[index]" class="tr">{{ translations[index] }}</span>
          </span>
          <button class="ghost" title="This line is spoken now — sync to it" @click.stop="syncHere(cue)">⏱</button>
        </li>
      </ol>
    </section>
  </aside>
</template>

<style scoped>
.panel {
  background: var(--panel);
  border-left: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.tabs {
  display: flex;
  border-bottom: 1px solid var(--border);
}
.tabs button {
  flex: 1;
  border: none;
  border-radius: 0;
  background: transparent;
  padding: 12px;
  border-bottom: 2px solid transparent;
}
.tabs button.active {
  border-bottom-color: var(--accent);
}
.current {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border);
  font-size: 12px;
}
.current .label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.translate {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
}
.translate .error {
  margin: 0;
}
.modes {
  display: flex;
}
.modes button {
  flex: 1;
  border-radius: 0;
}
.modes button:first-child {
  border-radius: 6px 0 0 6px;
}
.modes button:last-child {
  border-radius: 0 6px 6px 0;
}
.modes button.active {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-text);
}
.tr {
  display: block;
  color: #e3c565;
}
section {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  min-height: 0;
  flex: 1;
}
.row {
  display: flex;
  gap: 6px;
  align-items: center;
}
.grow {
  flex: 1;
  min-width: 0;
}
.lang {
  width: 48px;
}
.search > .ghost {
  align-self: flex-start;
  padding-left: 0;
  color: var(--accent);
}
.on-disk {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
}
.file {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
ul,
ol {
  list-style: none;
  margin: 0;
  padding: 0;
  overflow-y: auto;
  flex: 1;
  min-height: 0;
}
.results li {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 8px 0;
  border-bottom: 1px solid var(--border);
}
.release {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.meta {
  font-size: 12px;
}
.follow {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
}
.tip {
  font-size: 12px;
  margin: 0;
}
.lines li {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  padding: 6px;
  border-radius: 6px;
  cursor: pointer;
}
.lines li:hover {
  background: var(--panel-2);
}
.lines li.current {
  background: var(--panel-2);
  box-shadow: inset 2px 0 0 var(--accent);
}
.time {
  font-variant-numeric: tabular-nums;
  font-size: 12px;
  padding-top: 2px;
  min-width: 52px;
}
.text {
  flex: 1;
  white-space: pre-line;
}
.lines li button {
  padding: 0 6px;
  opacity: 0.4;
}
.lines li:hover button,
.lines li.current button {
  opacity: 1;
}
</style>
