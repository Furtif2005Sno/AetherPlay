<script setup lang="ts">
import { ref } from "vue";
import { api } from "../api";

const props = defineProps<{ serverUrl: string; username: string }>();
const emit = defineEmits<{ done: [] }>();

const url = ref(props.serverUrl);
const user = ref(props.username);
const pass = ref("");
const error = ref("");
const busy = ref(false);

async function submit() {
  busy.value = true;
  error.value = "";
  try {
    await api.login(url.value, user.value, pass.value);
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
      <h1>RomM <span>Desktop</span></h1>
      <p class="muted">Connecte-toi à ton serveur RomM</p>
      <input v-model="url" class="input" placeholder="https://romm.exemple.fr" required />
      <input v-model="user" class="input" placeholder="Utilisateur" autocomplete="username" required />
      <input v-model="pass" class="input" type="password" placeholder="Mot de passe" autocomplete="current-password" required />
      <p v-if="error" class="error">{{ error }}</p>
      <button class="btn primary" :disabled="busy">{{ busy ? "Connexion…" : "Se connecter" }}</button>
    </form>
  </div>
</template>

<style scoped>
.wrap { height: 100%; display: grid; place-items: center; }
.card { width: 360px; padding: 32px; display: flex; flex-direction: column; gap: 12px; }
h1 { font-size: 22px; }
h1 span { color: var(--r-color-brand-primary); }
</style>
