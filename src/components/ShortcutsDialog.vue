<script setup lang="ts">
import { shortcut } from "../platform";
import Modal from "./ui/Modal.vue";

const emit = defineEmits<{ close: [] }>();

/** Each entry lists alternative key combinations; each combination is a list of keys. */
const groups: { title: string; items: { label: string; keys: string[][] }[] }[] = [
  {
    title: "Playback",
    items: [
      { label: "Play / pause", keys: [["Space"]] },
      { label: "Back / forward 5 s", keys: [["←"], ["→"]] },
      { label: "Fullscreen", keys: [["F"]] },
      { label: "Exit fullscreen", keys: [["Esc"]] },
    ],
  },
  {
    title: "Subtitles",
    items: [
      { label: "Show earlier by 0.1 s", keys: [["G"]] },
      { label: "Show later by 0.1 s", keys: [["H"]] },
      { label: "Shift by 1 s", keys: [["⇧", "G"], ["⇧", "H"]] },
      { label: "Original → both → translation", keys: [["T"]] },
      { label: "Smaller / larger text", keys: [["−"], ["+"]] },
      { label: "Sync to a line in the list", keys: [shortcut("⇧", "Mod", "Click")] },
    ],
  },
  {
    title: "Window",
    items: [
      { label: "Search or paste a link", keys: [shortcut("Mod", "L")] },
      { label: "Filter subtitle lines", keys: [shortcut("Mod", "F")] },
      { label: "Toggle library", keys: [["["]] },
      { label: "Toggle subtitles panel", keys: [["]"]] },
      { label: "Settings", keys: [shortcut("Mod", ",")] },
      { label: "Keyboard shortcuts", keys: [["?"]] },
    ],
  },
];
</script>

<template>
  <Modal title="Keyboard shortcuts" :width="640" @close="emit('close')">
    <div class="grid">
      <section v-for="g in groups" :key="g.title">
        <h3 class="section-label">{{ g.title }}</h3>
        <dl>
          <div v-for="item in g.items" :key="item.label" class="row">
            <dt>{{ item.label }}</dt>
            <dd>
              <template v-for="(combo, i) in item.keys" :key="i">
                <span v-if="i" class="subtle sep">/</span>
                <span class="combo"><kbd v-for="k in combo" :key="k">{{ k }}</kbd></span>
              </template>
            </dd>
          </div>
        </dl>
      </section>
    </div>
  </Modal>
</template>

<style scoped>
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: var(--s-5) var(--s-8);
}
h3 {
  margin-bottom: var(--s-2);
}
dl {
  margin: 0;
}
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--s-3);
  min-height: 30px;
  border-bottom: 1px solid var(--border);
}
.row:last-child {
  border-bottom: none;
}
dt {
  color: var(--text-secondary);
}
dd {
  display: flex;
  align-items: center;
  gap: 4px;
  margin: 0;
  flex-shrink: 0;
}
.combo {
  display: flex;
  gap: 2px;
}
.sep {
  font-size: var(--fs-xs);
}
</style>
