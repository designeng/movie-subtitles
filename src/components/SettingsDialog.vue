<script lang="ts">
export type SettingsTab = "subtitles" | "translation" | "library";
</script>

<script setup lang="ts">
import { open } from "@tauri-apps/plugin-dialog";
import { computed, onMounted, ref } from "vue";
import { api, type Settings } from "../api";
import { modKey, revealLabel } from "../platform";
import Icon from "./ui/Icon.vue";
import Modal from "./ui/Modal.vue";
import Segmented from "./ui/Segmented.vue";

const props = defineProps<{ settings: Settings; initialTab?: SettingsTab }>();
const emit = defineEmits<{ close: []; saved: [settings: Settings] }>();

const form = ref<Settings>({ ...props.settings });
const error = ref("");
const saving = ref(false);
/** Effective folder, shown as a placeholder when the default is used. */
const defaultVideosDir = ref("");

const tab = ref<SettingsTab>(props.initialTab ?? "subtitles");
const tabs: { value: SettingsTab; label: string }[] = [
  { value: "subtitles", label: "Subtitles" },
  { value: "translation", label: "Translation" },
  { value: "library", label: "Library" },
];

const noSubtitleSource = computed(() => !form.value.opensubtitlesApiKey.trim() && !form.value.subdlApiKey.trim());

onMounted(async () => {
  if (!form.value.videosDir) defaultVideosDir.value = await api.getVideosDir();
});

async function chooseVideosDir() {
  const dir = await open({ directory: true, defaultPath: form.value.videosDir || defaultVideosDir.value });
  if (typeof dir === "string") form.value.videosDir = dir;
}

async function openVideosDir() {
  try {
    await api.openVideosDir();
  } catch (e) {
    error.value = String(e);
  }
}

async function save() {
  saving.value = true;
  error.value = "";
  try {
    await api.saveSettings(form.value);
    emit("saved", { ...form.value });
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}

function onKeydown(e: KeyboardEvent) {
  // ⌘↵ saves from anywhere in the form.
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    save();
  }
}
</script>

<template>
  <Modal title="Settings" :width="560" @close="emit('close')">
    <template #nav>
      <div class="nav">
        <Segmented v-model="tab" :options="tabs" block aria-label="Settings section" />
      </div>
    </template>

    <form id="settings-form" class="form" @submit.prevent="save" @keydown="onKeydown">
      <!-- Subtitles -->
      <template v-if="tab === 'subtitles'">
        <div v-if="noSubtitleSource" class="notice">
          <Icon name="alert" :size="14" />
          <span class="grow">Add an OpenSubtitles or SubDL key to search for subtitles. Both are free.</span>
        </div>

        <div class="field">
          <label for="s-lang">Default language</label>
          <div>
            <input id="s-lang" v-model="form.language" class="short" placeholder="en" />
            <p class="hint">ISO 639-1 code used for subtitle search, e.g. en, de, fr.</p>
          </div>
        </div>

        <h3 class="group">OpenSubtitles.com</h3>
        <div class="field">
          <label for="s-os-key">API key</label>
          <div>
            <input id="s-os-key" v-model="form.opensubtitlesApiKey" autocomplete="off" spellcheck="false" />
            <p class="hint">Create a free consumer at opensubtitles.com → profile → API consumers.</p>
          </div>
        </div>
        <div class="field">
          <label for="s-os-user">Username</label>
          <input id="s-os-user" v-model="form.opensubtitlesUsername" autocomplete="off" placeholder="Optional" />
        </div>
        <div class="field">
          <label for="s-os-pass">Password</label>
          <div>
            <input id="s-os-pass" v-model="form.opensubtitlesPassword" type="password" autocomplete="off" placeholder="Optional" />
            <p class="hint">Signing in only raises the daily download limit.</p>
          </div>
        </div>

        <h3 class="group">SubDL.com</h3>
        <div class="field">
          <label for="s-subdl">API key</label>
          <div>
            <input id="s-subdl" v-model="form.subdlApiKey" autocomplete="off" spellcheck="false" placeholder="Optional" />
            <p class="hint">Searched together with OpenSubtitles. Get a free key at subdl.com → profile → API key.</p>
          </div>
        </div>
      </template>

      <!-- Translation -->
      <template v-else-if="tab === 'translation'">
        <div class="field">
          <label for="s-to">Translate to</label>
          <div>
            <input id="s-to" v-model="form.translateTo" class="short" placeholder="ru" />
            <p class="hint">Default target language for subtitle translation.</p>
          </div>
        </div>

        <h3 class="group">Google Cloud Translation</h3>
        <div class="field">
          <label for="s-gt">API key</label>
          <div>
            <input
              id="s-gt"
              v-model="form.googleTranslateApiKey"
              autocomplete="off"
              spellcheck="false"
              placeholder="Optional — free endpoint is used without it"
            />
            <p class="hint">
              Without a key, the free unofficial Google endpoint is used: fine for personal use, but rate-limited. With a key,
              the official Cloud Translation API is used (Cloud console → enable “Cloud Translation API” → Credentials). The
              first 500,000 characters a month are free; a movie is about 50–100k.
            </p>
          </div>
        </div>

        <h3 class="group">MyMemory fallback</h3>
        <div class="field">
          <label for="s-mm">Email</label>
          <div>
            <input
              id="s-mm"
              v-model="form.mymemoryEmail"
              type="email"
              autocomplete="off"
              spellcheck="false"
              placeholder="Optional"
            />
            <p class="hint">Used when the free Google endpoint is blocked. An email raises the daily quota from ~5,000 to ~50,000 characters.</p>
          </div>
        </div>
      </template>

      <!-- Library -->
      <template v-else>
        <div class="field">
          <label for="s-dir">Download folder</label>
          <div>
            <input
              id="s-dir"
              :value="form.videosDir"
              :placeholder="defaultVideosDir || 'Default folder (app data)'"
              :title="form.videosDir || defaultVideosDir"
              readonly
            />
            <div class="buttons">
              <button type="button" class="sm" @click="chooseVideosDir">Choose…</button>
              <button type="button" class="sm" :disabled="!form.videosDir" @click="form.videosDir = ''">Use default</button>
              <button type="button" class="ghost sm" @click="openVideosDir">
                <Icon name="folder" :size="14" /> {{ revealLabel }}
              </button>
            </div>
            <p class="hint">New downloads go here. Movies already downloaded stay where they are.</p>
          </div>
        </div>
        <div class="field">
          <label for="s-q">Max quality</label>
          <div>
            <select id="s-q" v-model="form.videoQuality">
              <option value="best">Best available</option>
              <option value="1080p">1080p</option>
              <option value="720p">720p</option>
            </select>
            <p class="hint">A 2-hour movie in 4K can take 10 GB or more. Applies to new downloads.</p>
          </div>
        </div>
        <div class="field">
          <span class="label">Sidebar</span>
          <label class="check">
            <input v-model="form.showFilesInfo" type="checkbox" />
            Show file sizes
          </label>
        </div>
      </template>
    </form>

    <template #footer>
      <p v-if="error" class="error footer-error">{{ error }}</p>
      <span v-else class="subtle footer-hint"><kbd>{{ modKey }}</kbd><kbd>↵</kbd> to save</span>
      <button type="button" @click="emit('close')">Cancel</button>
      <button class="primary" type="submit" form="settings-form" :disabled="saving">Save</button>
    </template>
  </Modal>
</template>

<style scoped>
.nav {
  padding: 0 var(--s-5) var(--s-3);
}
.form {
  display: flex;
  flex-direction: column;
  gap: var(--s-3);
  padding-top: var(--s-2);
  /* Keep the dialog height stable while switching tabs. */
  min-height: 340px;
}
.field {
  display: grid;
  grid-template-columns: 128px minmax(0, 1fr);
  gap: var(--s-3);
  align-items: start;
}
.field > label,
.field > .label {
  padding-top: 5px;
  text-align: right;
  color: var(--text-secondary);
}
.field input:not([type="checkbox"]),
.field select {
  width: 100%;
}
.field input.short {
  width: 72px;
}
.field select {
  width: auto;
  min-width: 160px;
}
.hint {
  margin-top: 6px;
  font-size: var(--fs-sm);
  color: var(--text-tertiary);
}
.group {
  margin-top: var(--s-3);
  padding-top: var(--s-4);
  border-top: 1px solid var(--border);
  font-size: var(--fs-sm);
  font-weight: var(--fw-semibold);
}
.buttons {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: var(--s-2);
}
.check {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  height: var(--h-md);
}
.footer-hint {
  display: flex;
  align-items: center;
  gap: 2px;
  margin-right: auto;
  font-size: var(--fs-xs);
}
.footer-error {
  flex: 1;
  min-width: 0;
  font-size: var(--fs-sm);
}
</style>
