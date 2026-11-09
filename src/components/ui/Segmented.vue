<script setup lang="ts" generic="T extends string">
/** A segmented control: one choice out of a few, like macOS NSSegmentedControl. */
defineProps<{
  options: { value: T; label: string; count?: number; disabled?: boolean; title?: string }[];
  /** Stretch the segments to fill the width. */
  block?: boolean;
  ariaLabel?: string;
}>();
const model = defineModel<T>({ required: true });
</script>

<template>
  <div class="segmented" :class="{ block }" role="radiogroup" :aria-label="ariaLabel">
    <button
      v-for="o in options"
      :key="o.value"
      type="button"
      role="radio"
      :aria-checked="model === o.value"
      :class="{ active: model === o.value }"
      :disabled="o.disabled"
      :title="o.title"
      @click="model = o.value"
    >
      {{ o.label }}
      <span v-if="o.count != null" class="count tabular">{{ o.count }}</span>
    </button>
  </div>
</template>

<style scoped>
.segmented {
  display: inline-flex;
  gap: 2px;
  padding: 2px;
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--r-lg);
}
.segmented.block {
  display: flex;
}
.segmented.block button {
  flex: 1;
}
button {
  height: calc(var(--h-md) - 6px);
  padding: 0 var(--s-3);
  font-size: var(--fs-sm);
  color: var(--text-secondary);
  background: transparent;
  border-radius: var(--r-md);
}
button:hover:not(:disabled) {
  background: var(--bg-hover);
  color: var(--text);
}
button.active,
button.active:hover:not(:disabled) {
  background: var(--bg-control-active);
  color: var(--text);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
}
.count {
  font-size: var(--fs-xs);
  color: var(--text-tertiary);
}
</style>
