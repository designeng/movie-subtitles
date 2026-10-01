<script setup lang="ts">
import { ref, watch } from "vue";
import { formatOffset } from "../subtitles";

const offset = defineModel<number>({ required: true });
defineProps<{ disabled: boolean }>();
const emit = defineEmits<{ export: [] }>();

// Editable seconds field, kept in sync with the model.
const seconds = ref("");
watch(offset, (ms) => (seconds.value = (ms / 1000).toFixed(1)), { immediate: true });

function shift(ms: number) {
  offset.value += ms;
}

function applySeconds() {
  const value = Number(seconds.value.replace(",", "."));
  if (Number.isFinite(value)) offset.value = Math.round(value * 1000);
  else seconds.value = (offset.value / 1000).toFixed(1);
}
</script>

<template>
  <div class="sync" :class="{ disabled }">
    <span class="label">Subtitle sync</span>
    <div class="group" title="Sync backward: show subtitles earlier (G / Shift+G)">
      <button :disabled="disabled" @click="shift(-1000)">« 1 s</button>
      <button :disabled="disabled" @click="shift(-100)">‹ 0.1 s</button>
    </div>
    <div class="value">
      <strong>{{ formatOffset(offset) }}</strong>
      <input
        v-model="seconds"
        :disabled="disabled"
        aria-label="Offset in seconds"
        @keydown.enter="($event.target as HTMLInputElement).blur()"
        @blur="applySeconds"
      />
    </div>
    <div class="group" title="Sync forward: show subtitles later (H / Shift+H)">
      <button :disabled="disabled" @click="shift(100)">0.1 s ›</button>
      <button :disabled="disabled" @click="shift(1000)">1 s »</button>
    </div>
    <button class="ghost" :disabled="disabled || offset === 0" @click="offset = 0">Reset</button>
    <span class="spacer" />
    <span class="hint muted"><kbd>G</kbd> earlier · <kbd>H</kbd> later · <kbd>⇧</kbd> ×10 · <kbd>−</kbd>/<kbd>+</kbd> text size</span>
    <button :disabled="disabled" title="Save a .srt with the offset applied" @click="emit('export')">
      Export .srt
    </button>
  </div>
</template>

<style scoped>
.sync {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 16px;
  border-top: 1px solid var(--border);
  background: var(--panel);
  flex-wrap: wrap;
}
.label {
  font-weight: 600;
}
.group {
  display: flex;
  gap: 4px;
}
.value {
  display: flex;
  align-items: center;
  gap: 8px;
}
.value strong {
  min-width: 64px;
  text-align: center;
  font-variant-numeric: tabular-nums;
}
.value input {
  width: 64px;
  text-align: right;
}
.spacer {
  flex: 1;
}
.hint {
  font-size: 12px;
}
</style>
