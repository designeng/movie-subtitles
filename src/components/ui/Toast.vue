<script setup lang="ts">
import Icon from "./Icon.vue";

defineProps<{ message: string }>();
const emit = defineEmits<{ dismiss: [] }>();
</script>

<template>
  <div class="toast" role="alert">
    <Icon name="alert" :size="16" class="icon" />
    <p class="message selectable">{{ message }}</p>
    <button class="ghost icon sm" title="Dismiss" aria-label="Dismiss" @click="emit('dismiss')">
      <Icon name="x" :size="14" />
    </button>
  </div>
</template>

<style scoped>
.toast {
  position: fixed;
  left: 50%;
  bottom: 64px;
  z-index: 90;
  transform: translateX(-50%);
  display: flex;
  align-items: flex-start;
  gap: var(--s-2);
  width: max-content;
  max-width: min(560px, calc(100vw - 32px));
  max-height: 40vh;
  padding: var(--s-2) var(--s-2) var(--s-2) var(--s-3);
  background: var(--bg-elevated);
  border-radius: var(--r-lg);
  box-shadow: var(--shadow-popover);
  animation: in var(--dur) var(--ease);
}
.icon {
  margin-top: 3px;
  color: var(--danger);
}
.message {
  flex: 1;
  min-width: 0;
  padding: 2px 0;
  overflow-y: auto;
  white-space: pre-wrap;
  word-break: break-word;
}
@keyframes in {
  from {
    opacity: 0;
    transform: translate(-50%, 8px);
  }
}
</style>
