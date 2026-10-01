<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import type { Cue, SubtitleMode } from "../api";
import { activeCues } from "../subtitles";

const props = defineProps<{
  src: string;
  cues: Cue[];
  offsetMs: number;
  /** Line-by-line translation aligned with `cues`, if requested. */
  translations: string[] | null;
  mode: SubtitleMode;
}>();
const emit = defineEmits<{ time: [ms: number]; toggleFullscreen: [] }>();

const video = ref<HTMLVideoElement>();
const timeMs = ref(0);
const playError = ref("");

function onError() {
  const err = video.value?.error;
  if (!err) return;
  playError.value =
    err.code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED || err.code === MediaError.MEDIA_ERR_DECODE
      ? "This video can't be played: its codec (for example VP9 or Opus) isn't supported by this version of macOS. " +
        "Delete it from the library and download it again; the app now picks a compatible format."
      : `This video can't be played${err.message ? `: ${err.message}` : ""}. The file may be missing or damaged.`;
}

const visible = computed(() =>
  activeCues(props.cues, timeMs.value, props.offsetMs).map((i) => {
    const translation = props.translations?.[i];
    return {
      key: i,
      original: !translation || props.mode !== "translation" ? props.cues[i].text : null,
      translation: translation && props.mode !== "original" ? translation : null,
    };
  }),
);

// `timeupdate` fires only ~4 times per second, which is too coarse for
// subtitle timing, so poll every animation frame while playing.
let frame = 0;
function tick() {
  if (video.value) {
    timeMs.value = video.value.currentTime * 1000;
    emit("time", timeMs.value);
  }
  frame = requestAnimationFrame(tick);
}
function start() {
  cancelAnimationFrame(frame);
  frame = requestAnimationFrame(tick);
}
function stop() {
  cancelAnimationFrame(frame);
  tick();
  cancelAnimationFrame(frame);
}
onUnmounted(() => cancelAnimationFrame(frame));
watch(
  () => props.src,
  () => {
    timeMs.value = 0;
    playError.value = "";
    emit("time", 0);
  },
);

defineExpose({
  seek(ms: number) {
    if (video.value) video.value.currentTime = Math.max(0, ms / 1000);
  },
  seekBy(ms: number) {
    if (video.value) video.value.currentTime = Math.max(0, video.value.currentTime + ms / 1000);
  },
  togglePlay() {
    const v = video.value;
    if (v) v.paused ? v.play() : v.pause();
  },
});
</script>

<template>
  <div class="player" @dblclick="emit('toggleFullscreen')">
    <video
      ref="video"
      :src="src"
      controls
      controlslist="nofullscreen"
      disablepictureinpicture
      @play="start"
      @pause="stop"
      @ended="stop"
      @seeked="stop"
      @loadedmetadata="stop"
      @error="onError"
    />
    <div v-if="playError" class="play-error">{{ playError }}</div>
    <div class="subtitles">
      <template v-for="line in visible" :key="line.key">
        <p v-if="line.original">{{ line.original }}</p>
        <p v-if="line.translation" class="translation">{{ line.translation }}</p>
      </template>
    </div>
  </div>
</template>

<style scoped>
.player {
  position: relative;
  background: #000;
  flex: 1;
  min-height: 0;
  display: flex;
}
video {
  width: 100%;
  height: 100%;
  object-fit: contain;
}
.play-error {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  padding: 24px;
  text-align: center;
  color: #ff8a80;
  background: rgba(0, 0, 0, 0.75);
}
.subtitles {
  position: absolute;
  left: 5%;
  right: 5%;
  /* Stay above the native control bar. */
  bottom: 64px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  pointer-events: none;
}
.subtitles p {
  margin: 0;
  padding: 2px 10px;
  background: rgba(0, 0, 0, 0.6);
  border-radius: 4px;
  color: #fff;
  font-size: clamp(16px, 2.6vw, 34px);
  line-height: 1.3;
  text-align: center;
  white-space: pre-line;
  text-shadow: 0 1px 2px #000;
}
.subtitles p.translation {
  color: #ffe08a;
  font-size: clamp(14px, 2.2vw, 28px);
}
</style>
