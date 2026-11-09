<script setup lang="ts">
/**
 * Stroke icons on a 24×24 grid (Lucide-style geometry), rendered at 14 or 16 px.
 * Kept inline so the app has no icon font or extra dependency.
 */
const ICONS = {
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  link: '<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>',
  sliders:
    '<path d="M20 5h-8M8 5H4M20 12h-6M10 12H4M20 19h-4M12 19H4"/><path d="M8 3v4M14 10v4M12 17v4"/>',
  sidebarLeft: '<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M9 4v16"/>',
  sidebarRight: '<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M15 4v16"/>',
  x: '<path d="M17 7 7 17M7 7l10 10"/>',
  trash:
    '<path d="M4 7h16M10 11v6M14 11v6"/><path d="M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12"/><path d="M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>',
  folder:
    '<path d="M4 20h16a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1h-7.5a1 1 0 0 1-.8-.4L10.3 4.4a1 1 0 0 0-.8-.4H4a1 1 0 0 0-1 1v14a1 1 0 0 0 1 1Z"/>',
  download: '<path d="M12 4v11M7 10l5 5 5-5M5 20h14"/>',
  export: '<path d="M12 15V4M7 9l5-5 5 5M5 14v5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-5"/>',
  timer: '<circle cx="12" cy="14" r="7"/><path d="M10 3h4M12 14l3-3"/>',
  locate: '<circle cx="12" cy="12" r="7"/><circle cx="12" cy="12" r="2.5"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3"/>',
  reset: '<path d="M4 12a8 8 0 1 0 2.4-5.7L4 8.5"/><path d="M4 4v4.5h4.5"/>',
  chevronLeft: '<path d="m14 17-5-5 5-5"/>',
  chevronRight: '<path d="m10 17 5-5-5-5"/>',
  chevronsLeft: '<path d="m11 17-5-5 5-5M18 17l-5-5 5-5"/>',
  chevronsRight: '<path d="m6 17 5-5-5-5M13 17l5-5-5-5"/>',
  chevronDown: '<path d="m7 10 5 5 5-5"/>',
  film: '<rect x="3" y="3" width="18" height="18" rx="2.5"/><path d="M7.5 3v18M16.5 3v18M3 8h4.5M3 12h18M3 16h4.5M16.5 8H21M16.5 16H21"/>',
  captions: '<rect x="3" y="5" width="18" height="14" rx="2.5"/><path d="M7 15h4M14 15h3M7 11h2M12 11h5"/>',
  languages: '<path d="m5 8 6 6M4 14l6-6 2-3M2 5h12M7 2h1M22 22l-5-10-5 10M14 18h6"/>',
  file: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8Z"/><path d="M14 3v5h5"/>',
  keyboard:
    '<rect x="2.5" y="5" width="19" height="14" rx="2.5"/><path d="M6.5 9h.01M10 9h.01M14 9h.01M17.5 9h.01M8 12.5h.01M12 12.5h.01M16 12.5h.01M8 16h8"/>',
  alert: '<circle cx="12" cy="12" r="9"/><path d="M12 8v4.5M12 16h.01"/>',
  check: '<path d="M5 12.5 10 17l9-10"/>',
  minus: '<path d="M6 12h12"/>',
  plus: '<path d="M6 12h12M12 6v12"/>',
  more: '<circle cx="5.5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="18.5" cy="12" r="1"/>',
  textSize: '<path d="M3 19 8 6l5 13M4.8 14.5h6.4M15 19l3-8 3 8M15.9 16.5h4.2"/>',
  play: '<path d="M8 5.5v13a.5.5 0 0 0 .77.42l10-6.5a.5.5 0 0 0 0-.84l-10-6.5A.5.5 0 0 0 8 5.5Z"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  volume: '<path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5Z"/><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a8 8 0 0 1 0 11"/>',
  stop: '<rect x="6.5" y="6.5" width="11" height="11" rx="1.5"/>',
  turtle: '<path d="M4 15a8 7 0 0 1 16 0Z"/><path d="M8 15v3M16 15v3M20 13.5l2 .5"/>',
  drop:'<path d="M12 3v12M7 10l5 5 5-5"/><path d="M4 15v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3"/>',
} as const;

export type IconName = keyof typeof ICONS;

withDefaults(defineProps<{ name: IconName; size?: number }>(), { size: 16 });
</script>

<template>
  <svg
    class="icon"
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.75"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    v-html="ICONS[name]"
  />
</template>

<style scoped>
.icon {
  flex-shrink: 0;
  display: block;
}
</style>
