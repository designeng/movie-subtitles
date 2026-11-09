import { onMounted, onUnmounted, ref, watch } from "vue";

function readNumber(key: string): number | null {
  const n = Number(localStorage.getItem(key));
  return Number.isFinite(n) && n > 0 ? n : null;
}

/**
 * A side panel that can be resized by dragging its edge and collapsed.
 * Width and visibility persist across launches.
 *
 * @param side Which edge of the window the panel is attached to.
 * @param maxFraction The panel never takes more than this share of the window.
 */
export function usePanel(key: string, opts: { side: "left" | "right"; initial: number; min: number; maxFraction: number }) {
  const max = () => Math.max(opts.min, Math.floor(window.innerWidth * opts.maxFraction));
  const clamp = (w: number) => Math.min(max(), Math.max(opts.min, w));

  const width = ref(clamp(readNumber(`${key}Width`) ?? opts.initial));
  const open = ref(localStorage.getItem(`${key}Open`) !== "0");
  const resizing = ref(false);

  watch(open, (v) => localStorage.setItem(`${key}Open`, v ? "1" : "0"));

  function startResize(e: PointerEvent) {
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    resizing.value = true;
    const move = (ev: PointerEvent) => {
      width.value = clamp(opts.side === "left" ? ev.clientX : window.innerWidth - ev.clientX);
    };
    const stop = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
      resizing.value = false;
      localStorage.setItem(`${key}Width`, String(width.value));
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  }

  /** Double-clicking the edge restores the default width. */
  function resetWidth() {
    width.value = clamp(opts.initial);
    localStorage.setItem(`${key}Width`, String(width.value));
  }

  const onWindowResize = () => (width.value = clamp(width.value));
  onMounted(() => window.addEventListener("resize", onWindowResize));
  onUnmounted(() => window.removeEventListener("resize", onWindowResize));

  return { width, open, resizing, startResize, resetWidth };
}
