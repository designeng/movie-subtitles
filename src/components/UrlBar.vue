<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { api, type DownloadProgress, type LibraryEntry } from "../api";

const emit = defineEmits<{ downloaded: [entry: LibraryEntry]; openSettings: [] }>();

const url = ref("");
const busy = ref(false);
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

async function download() {
  if (!url.value.trim() || busy.value) return;
  busy.value = true;
  error.value = "";
  progress.value = null;
  try {
    const entry = await api.downloadVideo(url.value);
    url.value = "";
    emit("downloaded", entry);
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
    progress.value = null;
  }
}
</script>

<template>
  <header class="bar">
    <form class="row" @submit.prevent="download">
      <input
        v-model="url"
        class="url"
        type="url"
        placeholder="Paste a YouTube or VK Video link…"
        :disabled="busy"
        spellcheck="false"
      />
      <button class="primary" type="submit" :disabled="busy || !url.trim()">
        {{ busy ? "Downloading…" : "Download" }}
      </button>
      <button type="button" class="ghost" title="Settings" @click="emit('openSettings')">⚙︎ Settings</button>
    </form>
    <div v-if="busy" class="progress">
      <div class="track"><div class="fill" :style="{ width: `${progress?.percent ?? 0}%` }" /></div>
      <span class="muted">{{ status || "Starting…" }}</span>
    </div>
    <div v-if="error" class="error">{{ error }}</div>
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
.fill {
  height: 100%;
  background: var(--accent);
  transition: width 0.2s;
}
</style>
