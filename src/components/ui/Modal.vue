<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref } from "vue";
import Icon from "./Icon.vue";

withDefaults(defineProps<{ title: string; width?: number }>(), { width: 520 });
const emit = defineEmits<{ close: [] }>();

const dialog = ref<HTMLElement>();
let previousFocus: HTMLElement | null = null;

const FOCUSABLE = 'input:not([disabled]), select:not([disabled]), button:not([disabled]), [tabindex]:not([tabindex="-1"])';

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    emit("close");
    return;
  }
  // Keep Tab inside the dialog.
  if (e.key === "Tab" && dialog.value) {
    const items = [...dialog.value.querySelectorAll<HTMLElement>(FOCUSABLE)];
    if (!items.length) return;
    const first = items[0];
    const last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }
}

onMounted(async () => {
  previousFocus = document.activeElement as HTMLElement | null;
  window.addEventListener("keydown", onKey, true);
  await nextTick();
  const body = dialog.value?.querySelector<HTMLElement>(".modal-body");
  (body?.querySelector<HTMLElement>("input:not([readonly]), select") ?? dialog.value)?.focus();
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKey, true);
  previousFocus?.focus?.();
});
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div
      ref="dialog"
      class="modal"
      role="dialog"
      aria-modal="true"
      :aria-label="title"
      tabindex="-1"
      :style="{ width: `min(${width}px, calc(100vw - 48px))` }"
    >
      <header class="modal-header">
        <h2>{{ title }}</h2>
        <button class="ghost icon sm" title="Close (Esc)" aria-label="Close" @click="emit('close')">
          <Icon name="x" :size="14" />
        </button>
      </header>
      <slot name="nav" />
      <div class="modal-body">
        <slot />
      </div>
      <footer v-if="$slots.footer" class="modal-footer">
        <slot name="footer" />
      </footer>
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: grid;
  place-items: center;
  background: var(--bg-backdrop);
  animation: fade var(--dur) var(--ease);
}
.modal {
  display: flex;
  flex-direction: column;
  max-height: calc(100vh - 48px);
  background: var(--bg-elevated);
  border-radius: var(--r-xl);
  box-shadow: var(--shadow-modal);
  animation: rise var(--dur) var(--ease);
  outline: none;
}
.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--s-4) var(--s-4) var(--s-3) var(--s-5);
}
.modal-header h2 {
  font-size: var(--fs-lg);
  font-weight: var(--fw-semibold);
}
.modal-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--s-2) var(--s-5) var(--s-5);
}
.modal-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--s-2);
  padding: var(--s-3) var(--s-5);
  border-top: 1px solid var(--border);
}
@keyframes fade {
  from {
    opacity: 0;
  }
}
@keyframes rise {
  from {
    opacity: 0;
    transform: translateY(6px) scale(0.985);
  }
}
</style>
