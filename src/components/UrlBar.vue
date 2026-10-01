<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { api, type DownloadProgress, type LibraryEntry, type VideoSearchResult } from "../api";
import { formatTime } from "../subtitles";

const emit = defineEmits<{ downloaded: [entry: LibraryEntry]; openSettings: [] }>();

/** A link to download, or a movie title to search for. */
const input = ref("");
const isLink = computed(() => /^https?:\/\//i.test(input.value.trim()));
const busy = ref(false);
const searching = ref(false);
const results = ref<VideoSearchResult[] | null>(null);
const progress = ref<DownloadProgress | null>(null);
const error = ref("");

let unlisten: (() => void) | undefined;
onMounted(async () => {
  unlisten = await api.onDownloadProgress((p) => (progress.value = p));
});
onUnmounted(() => unlisten?.());

const status = computed(() => {
  const p = progress.value;
  if (!p) return "";
  const parts = [p.stage];
  if (p.percent != null) parts.push(`${p.percent.toFixed(1)}%`);
  if (p.speedBps) parts.push(`${(p.speedBps / 1_048_576).toFixed(1)} MB/s`);
  if (p.etaSecs != null) parts.push(`ETA ${Math.round(p.etaSecs)}s`);
  return parts.join(" · ");
});

function submit() {
  if (isLink.value) download(input.value);
  else search();
}

async function search() {
  const query = input.value.trim();
  if (!query || searching.value) return;
  searching.value = true;
  error.value = "";
  try {
    results.value = await api.searchVideos(query);
  } catch (e) {
    error.value = String(e);
  } finally {
    searching.value = false;
  }
}

function formatViews(n: number): string {
  return Intl.NumberFormat("en", { notation: "compact" }).format(n);
}

async function download(url: string) {
  if (!url.trim() || busy.value) return;
  busy.value = true;
  error.value = "";
  progress.value = null;
  try {
    const entry = await api.downloadVideo(url);
    input.value = "";
    results.value = null;
    emit("downloaded", entry);
  } catch (e) {
    if (String(e) !== "cancelled") error.value = String(e);
  } finally {
    busy.value = false;
    progress.value = null;
  }
}
</script>

<template>
  <header class="bar">
    <form class="row" @submit.prevent="submit">
      <input
        v-model="input"
        class="url"
        placeholder="Paste a video link or search by movie title…"
        :disabled="busy"
        spellcheck="false"
      />
      <button class="primary" type="submit" :disabled="busy || searching || !input.trim()">
        <template v-if="busy">Downloading…</template>
        <template v-else-if="isLink">Download</template>
        <template v-else>{{ searching ? "Searching…" : "Search" }}</template>
      </button>
      <button type="button" class="ghost" title="Settings" @click="emit('openSettings')">⚙︎ Settings</button>
    </form>
    <div v-if="busy" class="progress">
      <div class="track"><div class="fill" :style="{ width: `${progress?.percent ?? 0}%` }" /></div>
      <span class="muted">{{ status || "Starting…" }}</span>
      <button type="button" class="ghost" @click="api.cancelDownload()">Cancel</button>
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <div v-if="results" class="results">
      <div class="results-head muted">
        <span>{{ results.length ? `YouTube · ${results.length} results, 20+ min` : "Nothing found on YouTube." }}</span>
        <button type="button" class="ghost" title="Close" @click="results = null">✕</button>
      </div>
      <ul>
        <li v-for="r in results" :key="r.url" :class="{ dubbed: r.likelyDubbed }">
          <img v-if="r.thumbnail" :src="r.thumbnail" alt="" loading="lazy" />
          <div class="info">
            <div class="title" :title="r.title">{{ r.title }}</div>
            <div class="muted meta">
              <template v-if="r.duration">{{ formatTime(r.duration * 1000) }}</template>
              <template v-if="r.channel"> · {{ r.channel }}</template>
              <template v-if="r.views != null"> · {{ formatViews(r.views) }} views</template>
              <span v-if="r.likelyDubbed" class="badge" title="Title suggests a dubbed version">dub?</span>
            </div>
          </div>
          <button :disabled="busy" @click="download(r.url)">Download</button>
        </li>
      </ul>
    </div>
  </header>
</template>

<style scoped>
.bar {
  padding: 12px 16px;
  border-bottom: 1px solid var(--border);
  background: var(--panel);
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.row {
  display: flex;
  gap: 8px;
}
.url {
  flex: 1;
  min-width: 0;
}
.progress {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 12px;
}
.track {
  flex: 1;
  height: 4px;
  background: var(--panel-2);
  border-radius: 2px;
  overflow: hidden;
}
.results {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.results-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 12px;
}
.results ul {
  list-style: none;
  margin: 0;
  padding: 0;
  max-height: 40vh;
  overflow-y: auto;
}
.results li {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 6px 0;
  border-top: 1px solid var(--border);
}
.results li.dubbed {
  opacity: 0.6;
}
.results img {
  width: 120px;
  aspect-ratio: 16 / 9;
  object-fit: cover;
  border-radius: 4px;
  flex-shrink: 0;
  background: var(--panel-2);
}
.info {
  flex: 1;
  min-width: 0;
}
.title {
  display: block;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.meta {
  font-size: 12px;
}
.badge {
  margin-left: 6px;
  padding: 0 4px;
  border: 1px solid var(--border);
  border-radius: 4px;
}
.fill {
  height: 100%;
  background: var(--accent);
  transition: width 0.2s;
}
</style>
