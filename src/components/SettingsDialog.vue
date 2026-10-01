<script setup lang="ts">
import { ref } from "vue";
import { api, type Settings } from "../api";

const props = defineProps<{ settings: Settings }>();
const emit = defineEmits<{ close: []; saved: [settings: Settings] }>();

const form = ref<Settings>({ ...props.settings });
const error = ref("");

async function save() {
  try {
    await api.saveSettings(form.value);
    emit("saved", { ...form.value });
  } catch (e) {
    error.value = String(e);
  }
}
</script>

<template>
  <div class="backdrop" @click.self="emit('close')" @keydown.esc="emit('close')">
    <form class="dialog" @submit.prevent="save">
      <h2>Settings</h2>
      <fieldset>
        <legend>OpenSubtitles.com</legend>
        <label>
          API key
          <input v-model="form.opensubtitlesApiKey" autocomplete="off" spellcheck="false" />
        </label>
        <p class="muted hint">
          Create a free consumer at opensubtitles.com → profile → API consumers.
          Username and password are optional and only raise the daily download limit.
        </p>
        <label>
          Username
          <input v-model="form.opensubtitlesUsername" autocomplete="off" />
        </label>
        <label>
          Password
          <input v-model="form.opensubtitlesPassword" type="password" autocomplete="off" />
        </label>
      </fieldset>
      <fieldset>
        <legend>SubDL.com</legend>
        <label>
          API key <span class="muted">(optional)</span>
          <input v-model="form.subdlApiKey" autocomplete="off" spellcheck="false" />
        </label>
        <p class="muted hint">
          Second subtitle source, searched together with OpenSubtitles. Get a free key at
          subdl.com → sign up → profile → API key.
        </p>
      </fieldset>
      <fieldset>
        <legend>Google Cloud Translation</legend>
        <label>
          API key <span class="muted">(optional)</span>
          <input v-model="form.googleTranslateApiKey" autocomplete="off" spellcheck="false" placeholder="Leave empty to use the free endpoint" />
        </label>
        <p class="muted hint">
          Without a key, the free unofficial Google endpoint is used: fine for personal use, but it is
          rate-limited and may stop working. With a key, the official Cloud Translation API is used
          (Google Cloud console → enable “Cloud Translation API” → Credentials → API key; first 500,000
          characters per month are free, a movie is ~50–100k).
        </p>
        <label>
          Translate to
          <input v-model="form.translateTo" placeholder="ru" />
        </label>
      </fieldset>
      <label>
        Default subtitle language
        <input v-model="form.language" placeholder="en" />
      </label>
      <p v-if="error" class="error">{{ error }}</p>
      <div class="actions">
        <button type="button" @click="emit('close')">Cancel</button>
        <button class="primary" type="submit">Save</button>
      </div>
    </form>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: grid;
  place-items: center;
  z-index: 10;
}
.dialog {
  width: min(460px, calc(100vw - 32px));
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 20px;
  max-height: calc(100vh - 32px);
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
h2 {
  margin: 0;
  font-size: 18px;
}
fieldset {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px;
}
legend {
  color: var(--muted);
  padding: 0 4px;
}
label {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.hint {
  font-size: 12px;
  margin: 0;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
