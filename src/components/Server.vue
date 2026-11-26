<script setup lang="ts">
import { ref } from "vue";
import { api } from "../api";

const props = defineProps<{ serverUrl: string }>();
const emit = defineEmits<{ done: [] }>();

const url = ref(props.serverUrl);
const error = ref("");
const busy = ref(false);

async function submit() {
  busy.value = true;
  error.value = "";
  try {
    await api.setServer(url.value);
    emit("done");
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="wrap">
    <form class="panel card" @submit.prevent="submit">
      <h1>Aether<span>Play</span></h1>
      <p class="muted">Adresse de ton serveur RomM. Tu te connecteras ensuite avec ton compte RomM habituel.</p>
      <input v-model="url" class="input" placeholder="https://romm.exemple.fr" required />
      <p v-if="error" class="error">{{ error }}</p>
      <button class="btn primary" :disabled="busy">{{ busy ? "Vérification…" : "Continuer" }}</button>
    </form>
  </div>
</template>

<style scoped>
.wrap { height: 100%; display: grid; place-items: center; }
.card { width: 380px; padding: 32px; display: flex; flex-direction: column; gap: 12px; }
h1 { font-size: 22px; }
h1 span { color: var(--r-color-brand-primary); }
</style>
