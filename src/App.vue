<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, type Settings } from "./api";
import Login from "./components/Login.vue";
import Library from "./components/Library.vue";
import Emulators from "./components/Emulators.vue";
import Setup from "./components/Setup.vue";

const settings = ref<Settings | null>(null);
const view = ref<"library" | "emulators">("library");
const editingServer = ref(false);
const setupDone = ref(localStorage.getItem("setupDone") === "1");

async function finishSetup() {
  localStorage.setItem("setupDone", "1");
  setupDone.value = true;
  await refresh();
}

async function refresh() {
  settings.value = await api.settings();
  editingServer.value = false;
}
onMounted(refresh);
</script>

<template>
  <template v-if="settings">
    <Login
      v-if="!settings.logged_in || editingServer"
      :server-url="settings.server_url"
      :username="settings.username"
      @done="refresh"
    />
    <div v-else class="shell">
      <nav>
        <strong class="logo">RomM <span>Desktop</span></strong>
        <button class="btn" :class="{ primary: view === 'library' }" @click="view = 'library'">Bibliothèque</button>
        <button class="btn" :class="{ primary: view === 'emulators' }" @click="view = 'emulators'">Émulateurs</button>
        <span class="spacer" />
        <button class="btn" :title="settings.server_url" @click="editingServer = true">{{ settings.username }}</button>
      </nav>
      <div class="content">
        <Setup v-if="!setupDone" @done="finishSetup" />
        <Library v-else-if="view === 'library'" />
        <Emulators v-else :paths="settings.emulator_paths" @changed="refresh" />
      </div>
    </div>
  </template>
</template>

<style scoped>
.shell { display: flex; flex-direction: column; height: 100%; }
nav {
  display: flex; align-items: center; gap: 8px; padding: 10px 16px;
  background: color-mix(in srgb, var(--r-color-bg) 78%, transparent); backdrop-filter: blur(20px);
  border-bottom: 1px solid var(--r-color-border);
}
.logo { font-size: 15px; margin-right: 12px; }
.logo span { color: var(--r-color-brand-primary); }
.spacer { flex: 1; }
.content { flex: 1; min-height: 0; }
</style>
