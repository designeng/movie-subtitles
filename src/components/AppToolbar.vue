<script setup lang="ts">
import { ref } from "vue";
import type { LibraryEntry } from "../api";
import { isMac, modKey } from "../platform";
import UrlBar from "./UrlBar.vue";
import Icon from "./ui/Icon.vue";

defineProps<{ libraryOpen: boolean; panelOpen: boolean; panelAvailable: boolean }>();
const emit = defineEmits<{
  toggleLibrary: [];
  togglePanel: [];
  openSettings: [];
  downloaded: [entry: LibraryEntry];
  error: [message: string];
}>();

const urlBar = ref<InstanceType<typeof UrlBar>>();
defineExpose({ focusSearch: () => urlBar.value?.focus() });
</script>

<template>
  <!-- Empty areas of the toolbar drag the window, like a native title bar. -->
  <header class="toolbar" :class="{ mac: isMac }" data-tauri-drag-region>
    <div class="side" data-tauri-drag-region>
      <button
        class="ghost icon"
        :aria-pressed="libraryOpen"
        :title="`${libraryOpen ? 'Hide' : 'Show'} library  [`"
        aria-label="Toggle library"
        @click="emit('toggleLibrary')"
      >
        <Icon name="sidebarLeft" />
      </button>
    </div>
    <div class="center" data-tauri-drag-region>
      <UrlBar ref="urlBar" @downloaded="emit('downloaded', $event)" @error="emit('error', $event)" />
    </div>
    <div class="side end" data-tauri-drag-region>
      <button
        class="ghost icon"
        :aria-pressed="panelOpen && panelAvailable"
        :disabled="!panelAvailable"
        :title="`${panelOpen ? 'Hide' : 'Show'} subtitles panel  ]`"
        aria-label="Toggle subtitles panel"
        @click="emit('togglePanel')"
      >
        <Icon name="sidebarRight" />
      </button>
      <button class="ghost icon" :title="`Settings  ${modKey},`" aria-label="Settings" @click="emit('openSettings')">
        <Icon name="sliders" />
      </button>
    </div>
  </header>
</template>

<style scoped>
.toolbar {
  position: relative;
  z-index: 20;
  display: flex;
  align-items: center;
  gap: var(--s-3);
  height: var(--toolbar-h);
  padding: 0 var(--s-3);
  flex-shrink: 0;
  background: var(--bg-toolbar);
  border-bottom: 1px solid var(--border);
}
/* Room for the macOS traffic lights drawn over the toolbar. */
.toolbar.mac {
  padding-left: 84px;
}
.side {
  display: flex;
  align-items: center;
  gap: 2px;
  flex: 0 0 auto;
  min-width: 64px;
}
.side.end {
  justify-content: flex-end;
}
.center {
  flex: 1;
  min-width: 0;
  display: flex;
  justify-content: center;
}
/* Toggled panels read as "on" without the accent fill used by tool toggles. */
.toolbar button[aria-pressed="true"] {
  background: transparent;
  color: var(--text);
}
.toolbar button[aria-pressed="true"]:hover {
  background: var(--bg-control);
}
</style>
