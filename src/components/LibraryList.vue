<script setup lang="ts">
import { computed } from "vue";
import { ask } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, type LibraryEntry } from "../api";

const props = defineProps<{ entries: LibraryEntry[]; selectedId: string | null; sizes: Record<string, number> | null }>();

function formatSize(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  return `${Math.round(bytes / 1e6)} MB`;
}

const totalSize = computed(() =>
  props.sizes ? formatSize(Object.values(props.sizes).reduce((a, b) => a + b, 0)) : "",
);
const emit = defineEmits<{ select: [entry: LibraryEntry]; deleted: [id: string]; error: [message: string] }>();

async function reveal(entry: LibraryEntry) {
  try {
    await revealItemInDir(entry.videoPath);
  } catch (e) {
    emit("error", String(e));
  }
}

async function remove(entry: LibraryEntry) {
  const confirmed = await ask(`Delete "${entry.title}"?\n\nThe video file, any partial downloads and the subtitles will be removed from disk.`, {
    title: "Delete movie",
    kind: "warning",
    okLabel: "Delete",
  });
  if (!confirmed) return;
  try {
    await api.deleteEntry(entry.id);
    emit("deleted", entry.id);
  } catch (e) {
    emit("error", String(e));
  }
}
</script>

<template>
  <aside class="library">
    <h2>Library <span v-if="sizes" class="size">· {{ totalSize }}</span></h2>
    <p v-if="!entries.length" class="muted empty">Downloaded movies will appear here.</p>
    <ul>
      <li
        v-for="entry in entries"
        :key="entry.id"
        :class="{ selected: entry.id === selectedId }"
        @click="emit('select', entry)"
      >
        <div class="info">
          <div class="title" :title="entry.title">
            {{ entry.title }}<span v-if="sizes && sizes[entry.id] != null" class="size"> · {{ formatSize(sizes[entry.id]) }}</span>
          </div>
          <div class="meta muted">
            {{ entry.provider }} · {{ entry.subtitleLabel ? "subtitles" : "no subtitles" }}
          </div>
        </div>
        <button class="ghost" title="Show in Finder" @click.stop="reveal(entry)">⌕</button>
        <button class="ghost danger" title="Delete movie and subtitles" @click.stop="remove(entry)">✕</button>
      </li>
    </ul>
  </aside>
</template>

<style scoped>
.library {
  background: var(--panel);
  border-right: 1px solid var(--border);
  overflow-y: auto;
  min-height: 0;
}
h2 {
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--muted);
  margin: 16px 16px 8px;
}
.size {
  color: var(--muted);
  text-transform: none;
  letter-spacing: 0;
  font-size: 12px;
}
.empty {
  margin: 0 16px;
  font-size: 13px;
}
ul {
  list-style: none;
  margin: 0;
  padding: 0 8px 8px;
}
li {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 8px;
  border-radius: 6px;
  cursor: pointer;
}
li:hover {
  background: var(--panel-2);
}
li.selected {
  background: var(--panel-2);
  box-shadow: inset 2px 0 0 var(--accent);
}
.info {
  flex: 1;
  min-width: 0;
}
.title {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.meta {
  font-size: 12px;
}
li button {
  opacity: 0;
  padding: 4px 8px;
}
li:hover button {
  opacity: 1;
}
</style>
