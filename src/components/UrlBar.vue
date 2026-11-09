<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { api, type DownloadProgress, type LibraryEntry, type Movie, type VideoSearchResult } from "../api";
import { modKey } from "../platform";
import { formatTime } from "../subtitles";
import Icon from "./ui/Icon.vue";

const emit = defineEmits<{ downloaded: [entry: LibraryEntry]; error: [message: string] }>();

/** A link to download, or a movie title to search for. */
const input = ref("");
const inputEl = ref<HTMLInputElement>();
const root = ref<HTMLElement>();
const isLink = computed(() => /^https?:\/\//i.test(input.value.trim()));
const busy = ref(false);
const searching = ref(false);
const results = ref<VideoSearchResult[] | null>(null);
/** Movies matching the typed title; `movie` is the one searched for (null: the text as typed). */
const movies = ref<Movie[]>([]);
const movie = ref<Movie | null>(null);
const typedQuery = ref("");
const progress = ref<DownloadProgress | null>(null);
const searchError = ref("");
/** Whether the results popover is shown; results are kept while it's closed. */
const open = ref(false);
/** Result highlighted with the arrow keys. */
const active = ref(0);
const list = ref<HTMLElement>();

let unlisten: (() => void) | undefined;
onMounted(async () => {
  unlisten = await api.onDownloadProgress((p) => (progress.value = p));
  document.addEventListener("mousedown", onDocumentMouseDown);
});
onUnmounted(() => {
  unlisten?.();
  document.removeEventListener("mousedown", onDocumentMouseDown);
});

function onDocumentMouseDown(e: MouseEvent) {
  if (open.value && root.value && !root.value.contains(e.target as Node)) open.value = false;
}

const status = computed(() => {
  const p = progress.value;
  if (!p) return "Starting download…";
  const parts = [p.stage];
  if (p.speedBps) parts.push(`${(p.speedBps / 1_048_576).toFixed(1)} MB/s`);
  if (p.etaSecs != null) parts.push(`${formatTime(p.etaSecs * 1000)} left`);
  return parts.join(" · ");
});

const showPopover = computed(() => open.value && (searching.value || results.value !== null || !!searchError.value));

function submit() {
  if (isLink.value) download(input.value);
  else if (open.value && results.value?.length && !searching.value) download(results.value[active.value].url);
  else search();
}

async function search() {
  const query = input.value.trim();
  if (!query || searching.value) return;
  typedQuery.value = query;
  searching.value = true;
  searchError.value = "";
  results.value = null;
  open.value = true;
  // The original title finds the original-language video; if the lookup fails, search as typed.
  movies.value = await api.findMovies(query).catch(() => []);
  await searchFor(movies.value[0] ?? null);
}

function movieQuery(m: Movie): string {
  return m.year ? `${m.title} ${m.year}` : m.title;
}

function movieLabel(m: Movie): string {
  const name = m.label && m.label !== m.title ? `${m.title} — ${m.label}` : m.title;
  return [name, m.year, m.description].filter(Boolean).join(" · ");
}

async function searchFor(m: Movie | null) {
  movie.value = m;
  searching.value = true;
  searchError.value = "";
  active.value = 0;
  try {
    results.value = m
      ? await api.searchVideos(movieQuery(m), m.language)
      : await api.searchVideos(typedQuery.value);
  } catch (e) {
    searchError.value = String(e);
    results.value = null;
  } finally {
    searching.value = false;
  }
}

function pickMovie(id: string) {
  searchFor(movies.value.find((m) => m.id === id) ?? null);
}

function formatViews(n: number): string {
  return Intl.NumberFormat("en", { notation: "compact" }).format(n);
}

async function download(url: string) {
  if (!url.trim() || busy.value) return;
  busy.value = true;
  open.value = false;
  progress.value = null;
  try {
    const entry = await api.downloadVideo(url);
    input.value = "";
    results.value = null;
    movies.value = [];
    emit("downloaded", entry);
  } catch (e) {
    if (String(e) !== "cancelled") emit("error", String(e));
  } finally {
    busy.value = false;
    progress.value = null;
  }
}

function clear() {
  input.value = "";
  results.value = null;
  movies.value = [];
  searchError.value = "";
  open.value = false;
  inputEl.value?.focus();
}

function onFocus() {
  if (results.value || searchError.value) open.value = true;
}

async function moveActive(delta: number) {
  const n = results.value?.length ?? 0;
  if (!n) return;
  open.value = true;
  active.value = (active.value + delta + n) % n;
  await nextTick();
  list.value?.children[active.value]?.scrollIntoView({ block: "nearest" });
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    moveActive(1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    moveActive(-1);
  } else if (e.key === "Escape") {
    e.preventDefault();
    if (open.value) open.value = false;
    else inputEl.value?.blur();
  }
}

// Typing a new query hides stale results until the next search.
watch(input, () => {
  if (results.value && input.value.trim() !== typedQuery.value) open.value = false;
});

defineExpose({
  focus() {
    inputEl.value?.focus();
    inputEl.value?.select();
  },
});
</script>

<template>
  <div ref="root" class="urlbar">
    <form class="field" :class="{ busy, focused: showPopover }" @submit.prevent="submit">
      <template v-if="busy">
        <span class="spinner" />
        <span class="status truncate tabular" :title="status">{{ status }}</span>
        <span v-if="progress?.percent != null" class="percent tabular">{{ progress.percent.toFixed(0) }}%</span>
        <button type="button" class="ghost sm" @click="api.cancelDownload()">Cancel</button>
        <div class="progress" :style="{ transform: `scaleX(${(progress?.percent ?? 0) / 100})` }" />
      </template>
      <template v-else>
        <Icon :name="isLink ? 'link' : 'search'" :size="14" class="lead" />
        <input
          ref="inputEl"
          v-model="input"
          :placeholder="`Search a movie in any language, or paste a video link`"
          spellcheck="false"
          autocomplete="off"
          aria-label="Search movies or paste a link"
          :aria-expanded="showPopover"
          @focus="onFocus"
          @keydown="onKeydown"
        />
        <span v-if="!input" class="hint"><kbd>{{ modKey }}</kbd><kbd>L</kbd></span>
        <button v-else-if="!searching" type="button" class="ghost icon sm" title="Clear" aria-label="Clear" @click="clear">
          <Icon name="x" :size="14" />
        </button>
        <span v-if="searching" class="spinner" />
        <button v-if="isLink && input.trim()" type="submit" class="primary sm">
          <Icon name="download" :size="14" /> Download
        </button>
      </template>
    </form>

    <div v-if="showPopover" class="popover">
      <div v-if="movies.length && !searchError" class="movie">
        <div class="truncate">
          <template v-if="movie">
            <span class="subtle">Original title</span>
            <strong>{{ movie.title }}</strong><span v-if="movie.year" class="muted"> · {{ movie.year }}</span>
          </template>
          <span v-else class="muted">Searching “{{ typedQuery }}” as typed</span>
        </div>
        <select
          title="Other movies with this title"
          aria-label="Other movies with this title"
          :value="movie?.id ?? ''"
          :disabled="searching"
          @change="pickMovie(($event.target as HTMLSelectElement).value)"
        >
          <option v-for="m in movies" :key="m.id" :value="m.id">{{ movieLabel(m) }}</option>
          <option value="">“{{ typedQuery }}” as typed</option>
        </select>
      </div>

      <div v-if="searchError" class="state">
        <div class="notice error">
          <Icon name="alert" :size="14" />
          <span class="grow">{{ searchError }}</span>
        </div>
      </div>
      <div v-else-if="searching" class="state muted">
        <span class="spinner" /> Searching YouTube for “{{ movie ? movieQuery(movie) : typedQuery }}”…
      </div>
      <div v-else-if="results && !results.length" class="state muted">
        Nothing found on YouTube. Try the original title, or paste a link from VK Video or OK.ru.
      </div>
      <ul v-else-if="results" ref="list" class="results" role="listbox">
        <li
          v-for="(r, i) in results"
          :key="r.url"
          role="option"
          :aria-selected="i === active"
          :class="{ active: i === active, dubbed: r.likelyDubbed }"
          @mousemove="active = i"
          @click="download(r.url)"
        >
          <div class="thumb">
            <img v-if="r.thumbnail" :src="r.thumbnail" alt="" loading="lazy" />
            <span v-if="r.duration" class="duration tabular">{{ formatTime(r.duration * 1000) }}</span>
          </div>
          <div class="info">
            <div class="title truncate" :title="r.title">{{ r.title }}</div>
            <div class="meta truncate">
              <template v-if="r.channel">{{ r.channel }}</template>
              <template v-if="r.views != null"> · {{ formatViews(r.views) }} views</template>
            </div>
            <span v-if="r.likelyDubbed" class="badge warning" title="The title suggests a dubbed version">Dubbed?</span>
          </div>
          <span class="go"><Icon name="download" :size="14" /></span>
        </li>
      </ul>

      <footer v-if="results?.length && !searching" class="foot subtle">
        <span>YouTube · {{ results.length }} results longer than 20 min</span>
        <span class="keys">
          <span><kbd>↑</kbd><kbd>↓</kbd> select</span>
          <span><kbd>↵</kbd> download</span>
          <span><kbd>esc</kbd> close</span>
        </span>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.urlbar {
  position: relative;
  flex: 1;
  min-width: 0;
  max-width: 680px;
}
.field {
  position: relative;
  display: flex;
  align-items: center;
  gap: var(--s-2);
  height: var(--h-lg);
  padding: 0 var(--s-1) 0 var(--s-3);
  background: var(--bg-input);
  border: 1px solid var(--border-strong);
  border-radius: var(--r-lg);
  overflow: hidden;
  transition:
    border-color var(--dur-fast) var(--ease),
    box-shadow var(--dur-fast) var(--ease);
}
.field:focus-within,
.field.focused {
  border-color: var(--border-focus);
  box-shadow: 0 0 0 3px var(--accent-soft);
}
.field.busy {
  padding-left: var(--s-3);
  color: var(--text-secondary);
  font-size: var(--fs-sm);
}
.field input {
  flex: 1;
  height: 100%;
  padding: 0;
  background: none;
  border: none;
  box-shadow: none;
}
.lead {
  color: var(--text-tertiary);
}
.hint {
  display: flex;
  gap: 2px;
  padding-right: var(--s-1);
}
.status {
  flex: 1;
  min-width: 0;
}
.percent {
  color: var(--text);
  font-weight: var(--fw-medium);
}
.progress {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 2px;
  background: var(--accent);
  transform-origin: left;
  transition: transform 0.3s linear;
}

.popover {
  position: absolute;
  top: calc(100% + 6px);
  left: 50%;
  transform: translateX(-50%);
  width: max(100%, min(640px, calc(100vw - 32px)));
  z-index: 50;
  display: flex;
  flex-direction: column;
  max-height: min(560px, calc(100vh - 96px));
  background: var(--bg-elevated);
  border-radius: var(--r-xl);
  box-shadow: var(--shadow-popover);
  overflow: hidden;
  animation: drop var(--dur) var(--ease);
}
@keyframes drop {
  from {
    opacity: 0;
    transform: translate(-50%, -4px);
  }
}
.movie {
  display: flex;
  align-items: center;
  gap: var(--s-3);
  padding: var(--s-2) var(--s-2) var(--s-2) var(--s-4);
  border-bottom: 1px solid var(--border);
  font-size: var(--fs-sm);
}
.movie > div {
  flex: 1;
  min-width: 0;
}
.movie strong {
  margin-left: 6px;
  font-weight: var(--fw-semibold);
}
.movie select {
  flex: 0 1 260px;
  height: var(--h-sm);
  font-size: var(--fs-sm);
}
.state {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  padding: var(--s-5) var(--s-4);
}
.state .notice {
  flex: 1;
}
.results {
  list-style: none;
  margin: 0;
  padding: var(--s-1);
  overflow-y: auto;
}
.results li {
  display: flex;
  align-items: center;
  gap: var(--s-3);
  padding: 6px;
  border-radius: var(--r-lg);
}
.results li.active {
  background: var(--bg-selected);
}
.results li.dubbed .thumb,
.results li.dubbed .title {
  opacity: 0.6;
}
.thumb {
  position: relative;
  flex-shrink: 0;
  width: 112px;
  aspect-ratio: 16 / 9;
  border-radius: var(--r-md);
  background: var(--bg-control);
  overflow: hidden;
}
.thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
.duration {
  position: absolute;
  right: 4px;
  bottom: 4px;
  padding: 0 4px;
  font-size: 10px;
  font-weight: var(--fw-semibold);
  line-height: 16px;
  color: #fff;
  background: rgba(0, 0, 0, 0.75);
  border-radius: 3px;
}
.info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
}
.info .title {
  max-width: 100%;
  font-weight: var(--fw-medium);
}
.meta {
  max-width: 100%;
  font-size: var(--fs-sm);
  color: var(--text-secondary);
}
.go {
  display: grid;
  place-items: center;
  width: var(--h-md);
  height: var(--h-md);
  border-radius: var(--r-md);
  color: var(--text-on-accent);
  background: var(--accent);
  opacity: 0;
  transition: opacity var(--dur-fast) var(--ease);
}
li.active .go {
  opacity: 1;
}
.foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s-3);
  padding: var(--s-2) var(--s-4);
  border-top: 1px solid var(--border);
  font-size: var(--fs-xs);
}
.keys {
  display: flex;
  gap: var(--s-3);
  white-space: nowrap;
}
.keys > span {
  display: flex;
  align-items: center;
  gap: 3px;
}
</style>
