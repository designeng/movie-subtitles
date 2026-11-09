<script setup lang="ts">
import { Menu } from "@tauri-apps/api/menu";
import { ask } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { computed, nextTick, ref } from "vue";
import { api, type LibraryEntry } from "../api";
import { hasMod, isMac, revealLabel } from "../platform";
import { formatTime } from "../subtitles";
import Icon from "./ui/Icon.vue";

const props = defineProps<{ entries: LibraryEntry[]; selectedId: string | null; sizes: Record<string, number> | null }>();
const emit = defineEmits<{ select: [entry: LibraryEntry]; deleted: [id: string]; error: [message: string] }>();

function formatSize(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  return `${Math.round(bytes / 1e6)} MB`;
}

const totalSize = computed(() =>
  props.sizes ? formatSize(Object.values(props.sizes).reduce((a, b) => a + b, 0)) : "",
);

const PROVIDERS: Record<string, string> = { youtube: "YouTube", vk: "VK Video", ok: "OK.ru" };
const providerName = (p: string) => PROVIDERS[p.toLowerCase()] ?? p;

/** The filter appears once the library is long enough to need it. */
const FILTER_THRESHOLD = 8;
const filter = ref("");
const shown = computed(() => {
  const f = filter.value.trim().toLowerCase();
  return f ? props.entries.filter((e) => e.title.toLowerCase().includes(f)) : props.entries;
});

async function reveal(entry: LibraryEntry) {
  try {
    await revealItemInDir(entry.videoPath);
  } catch (e) {
    emit("error", String(e));
  }
}

async function remove(entry: LibraryEntry) {
  const confirmed = await ask(
    `The video file, any partial downloads and the subtitles will be removed from disk. This can't be undone.`,
    { title: `Delete “${entry.title}”?`, kind: "warning", okLabel: "Delete", cancelLabel: "Cancel" },
  );
  if (!confirmed) return;
  try {
    await api.deleteEntry(entry.id);
    emit("deleted", entry.id);
  } catch (e) {
    emit("error", String(e));
  }
}

/** Native context menu, same actions as the row's ⋯ button. */
async function showMenu(entry: LibraryEntry) {
  try {
    const menu = await Menu.new({
      items: [
        { id: "reveal", text: revealLabel, action: () => reveal(entry) },
        { item: "Separator" },
        { id: "delete", text: "Delete…", action: () => remove(entry) },
      ],
    });
    await menu.popup();
  } catch (e) {
    emit("error", String(e));
  }
}

function onContextMenu(entry: LibraryEntry) {
  if (entry.id !== props.selectedId) emit("select", entry);
  showMenu(entry);
}

const list = ref<HTMLElement>();

/** ↑/↓ move the selection; ⌘⌫ (Delete on other platforms) deletes the selected movie. */
async function onKeydown(e: KeyboardEvent) {
  const index = shown.value.findIndex((x) => x.id === props.selectedId);
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    e.stopPropagation();
    const next = shown.value[Math.min(shown.value.length - 1, Math.max(0, index + (e.key === "ArrowDown" ? 1 : -1)))];
    if (next && next.id !== props.selectedId) {
      emit("select", next);
      await nextTick();
      list.value?.querySelector(`[data-id="${CSS.escape(next.id)}"]`)?.scrollIntoView({ block: "nearest" });
    }
  } else if ((isMac ? e.key === "Backspace" && hasMod(e) : e.key === "Delete") && index >= 0) {
    e.preventDefault();
    e.stopPropagation();
    remove(shown.value[index]);
  }
}
</script>

<template>
  <aside class="library" aria-label="Library">
    <header class="head">
      <span class="section-label">Library</span>
      <span v-if="sizes && entries.length" class="subtle tabular total" title="Disk space used by downloaded movies">
        {{ totalSize }}
      </span>
    </header>

    <div v-if="entries.length > FILTER_THRESHOLD" class="filter">
      <Icon name="search" :size="14" class="lead" />
      <input v-model="filter" placeholder="Filter" aria-label="Filter library" @keydown.esc="filter = ''" />
    </div>

    <div v-if="!entries.length" class="empty">
      <Icon name="film" :size="20" class="subtle" />
      <p class="muted">No movies yet</p>
      <p class="subtle">Search above or paste a link. Downloads show up here.</p>
    </div>
    <p v-else-if="!shown.length" class="no-match subtle">No movies match “{{ filter }}”.</p>

    <ul
      v-if="shown.length"
      ref="list"
      class="list"
      role="listbox"
      tabindex="0"
      aria-label="Movies"
      @keydown="onKeydown"
    >
      <li
        v-for="entry in shown"
        :key="entry.id"
        :data-id="entry.id"
        role="option"
        :aria-selected="entry.id === selectedId"
        :class="{ selected: entry.id === selectedId }"
        @click="emit('select', entry)"
        @contextmenu.prevent="onContextMenu(entry)"
      >
        <div class="info">
          <div class="title truncate" :title="entry.title">{{ entry.title }}</div>
          <div class="meta">
            <span
              class="subs"
              :class="{ on: entry.subtitleLabel }"
              :title="entry.subtitleLabel ? `Subtitles: ${entry.subtitleLabel}` : 'No subtitles yet'"
            >
              <Icon name="captions" :size="12" />
            </span>
            <span class="truncate">{{ providerName(entry.provider) }}</span>
            <template v-if="entry.positionMs > 0">
              <span class="dot">·</span>
              <span class="tabular" title="Resumes from here">{{ formatTime(entry.positionMs) }}</span>
            </template>
            <template v-if="sizes && sizes[entry.id] != null">
              <span class="dot">·</span>
              <span class="tabular">{{ formatSize(sizes[entry.id]) }}</span>
            </template>
          </div>
        </div>
        <button
          class="ghost icon sm more"
          title="More actions"
          aria-label="More actions"
          tabindex="-1"
          @click.stop="showMenu(entry)"
        >
          <Icon name="more" :size="14" />
        </button>
      </li>
    </ul>
  </aside>
</template>

<style scoped>
.library {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--bg-sidebar);
  border-right: 1px solid var(--border);
}
.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s-2);
  height: var(--panel-header-h);
  padding: 0 var(--s-4);
  flex-shrink: 0;
}
.total {
  font-size: var(--fs-xs);
}
.filter {
  position: relative;
  margin: 0 var(--s-2) var(--s-2);
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
  height: var(--h-sm);
  padding-left: 26px;
  font-size: var(--fs-sm);
}
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--s-1);
  padding: var(--s-8) var(--s-5);
  text-align: center;
  font-size: var(--fs-sm);
}
.empty svg {
  margin-bottom: var(--s-1);
}
.no-match {
  padding: var(--s-2) var(--s-4);
  font-size: var(--fs-sm);
}
.list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  list-style: none;
  margin: 0;
  padding: 0 var(--s-2) var(--s-2);
  outline: none;
}
li {
  display: flex;
  align-items: center;
  gap: var(--s-1);
  padding: 7px var(--s-1) 7px var(--s-2);
  border-radius: var(--r-md);
}
li + li {
  margin-top: 1px;
}
li:hover {
  background: var(--bg-hover);
}
li.selected {
  background: var(--bg-selected);
}
.list:focus-visible li.selected {
  background: var(--accent-soft);
}
.info {
  flex: 1;
  min-width: 0;
}
.title {
  font-weight: var(--fw-medium);
}
.meta {
  display: flex;
  align-items: center;
  gap: 5px;
  margin-top: 1px;
  font-size: var(--fs-xs);
  color: var(--text-tertiary);
}
.meta > span {
  flex-shrink: 0;
}
.meta > .truncate {
  flex-shrink: 1;
  min-width: 0;
}
.subs {
  display: grid;
  place-items: center;
  color: var(--text-tertiary);
  opacity: 0.6;
}
.subs.on {
  color: var(--accent-text);
  opacity: 1;
}
.more {
  opacity: 0;
}
li:hover .more,
li.selected .more {
  opacity: 1;
}
</style>
