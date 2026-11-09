<script setup lang="ts">
import { ref, watch } from "vue";
import Icon from "./ui/Icon.vue";

const offset = defineModel<number>({ required: true });
defineProps<{ disabled: boolean; fontScale: number; minFontScale: number; maxFontScale: number }>();
const emit = defineEmits<{ export: []; fontScale: [delta: number]; showShortcuts: [] }>();

function format(ms: number): string {
  if (ms === 0) return "0.0";
  return `${ms > 0 ? "+" : "−"}${(Math.abs(ms) / 1000).toFixed(1)}`;
}

// Editable seconds field, kept in sync with the model.
const text = ref("");
watch(offset, (ms) => (text.value = format(ms)), { immediate: true });

function shift(ms: number) {
  offset.value += ms;
}

function apply() {
  const value = Number(text.value.replace(/[−–]/g, "-").replace(",", ".").replace(/[s±\s]/gi, ""));
  if (Number.isFinite(value)) offset.value = Math.round(value * 1000);
  text.value = format(offset.value);
}

function onKeydown(e: KeyboardEvent) {
  const input = e.target as HTMLInputElement;
  if (e.key === "Enter") input.blur();
  else if (e.key === "Escape") {
    text.value = format(offset.value);
    input.blur();
  } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
    // Arrow keys nudge the value, like a native stepper.
    e.preventDefault();
    shift((e.key === "ArrowUp" ? 1 : -1) * (e.shiftKey ? 1000 : 100));
  }
}
</script>

<template>
  <div class="bar" role="toolbar" aria-label="Subtitle controls">
    <div class="group" :class="{ disabled }">
      <span class="label" title="Shift the subtitle timeline to match the audio">Sync</span>
      <div class="stepper">
        <button class="ghost icon" :disabled="disabled" title="Earlier by 1 s  ⇧G" aria-label="Earlier by 1 second" @click="shift(-1000)">
          <Icon name="chevronsLeft" :size="14" />
        </button>
        <button class="ghost icon" :disabled="disabled" title="Earlier by 0.1 s  G" aria-label="Earlier by 0.1 second" @click="shift(-100)">
          <Icon name="chevronLeft" :size="14" />
        </button>
        <label class="value" :class="{ shifted: offset !== 0 }" title="Offset in seconds. Positive shows subtitles later. ↑/↓ to adjust.">
          <input
            v-model="text"
            class="tabular"
            :disabled="disabled"
            aria-label="Subtitle offset in seconds"
            spellcheck="false"
            @keydown="onKeydown"
            @blur="apply"
          />
          <span class="unit">s</span>
        </label>
        <button class="ghost icon" :disabled="disabled" title="Later by 0.1 s  H" aria-label="Later by 0.1 second" @click="shift(100)">
          <Icon name="chevronRight" :size="14" />
        </button>
        <button class="ghost icon" :disabled="disabled" title="Later by 1 s  ⇧H" aria-label="Later by 1 second" @click="shift(1000)">
          <Icon name="chevronsRight" :size="14" />
        </button>
      </div>
      <button
        class="ghost icon"
        :disabled="disabled || offset === 0"
        title="Reset offset"
        aria-label="Reset offset"
        @click="offset = 0"
      >
        <Icon name="reset" :size="14" />
      </button>
    </div>

    <span class="spacer" />

    <div class="group">
      <div class="stepper" title="Subtitle size">
        <button
          class="ghost icon"
          :disabled="fontScale <= minFontScale"
          title="Smaller subtitles  −"
          aria-label="Smaller subtitles"
          @click="emit('fontScale', -0.1)"
        >
          <Icon name="minus" :size="14" />
        </button>
        <span class="size tabular" aria-live="polite"><Icon name="textSize" :size="14" />{{ Math.round(fontScale * 100) }}%</span>
        <button
          class="ghost icon"
          :disabled="fontScale >= maxFontScale"
          title="Larger subtitles  +"
          aria-label="Larger subtitles"
          @click="emit('fontScale', 0.1)"
        >
          <Icon name="plus" :size="14" />
        </button>
      </div>
      <span class="divider" />
      <button class="ghost" :disabled="disabled" title="Save a .srt file with the offset applied" @click="emit('export')">
        <Icon name="export" :size="14" /> Export .srt
      </button>
      <button class="ghost icon" title="Keyboard shortcuts  ?" aria-label="Keyboard shortcuts" @click="emit('showShortcuts')">
        <Icon name="keyboard" :size="16" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.bar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--s-1) var(--s-3);
  min-height: var(--panel-header-h);
  padding: var(--s-1) var(--s-2) var(--s-1) var(--s-4);
  background: var(--bg-toolbar);
  border-top: 1px solid var(--border);
}
.group {
  display: flex;
  align-items: center;
  gap: var(--s-2);
}
.label {
  font-size: var(--fs-sm);
  font-weight: var(--fw-medium);
  color: var(--text-secondary);
}
.group.disabled .label {
  color: var(--text-tertiary);
}
.stepper {
  display: flex;
  align-items: center;
  gap: 1px;
}
.value {
  position: relative;
  display: flex;
  align-items: center;
  margin: 0 2px;
}
.value input {
  width: 68px;
  padding-right: 20px;
  text-align: right;
  font-weight: var(--fw-medium);
}
.value.shifted input {
  color: var(--accent-text);
}
.unit {
  position: absolute;
  right: 8px;
  color: var(--text-tertiary);
  pointer-events: none;
}
.spacer {
  flex: 1;
}
.size {
  display: flex;
  align-items: center;
  gap: 5px;
  min-width: 64px;
  justify-content: center;
  font-size: var(--fs-sm);
  color: var(--text-secondary);
}
.divider {
  width: 1px;
  height: 16px;
  background: var(--border-strong);
}
</style>
