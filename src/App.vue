<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, type Settings } from "./api";
import Server from "./components/Server.vue";
import Emulators from "./components/Emulators.vue";
import Setup from "./components/Setup.vue";
import Storage from "./components/Storage.vue";

// The main window shows these pages only until RomM's own interface takes over
// (first run); the `#settings` window holds the app's settings.
const isSettingsWindow = location.hash === "#settings";
const settings = ref<Settings | null>(null);
const view = ref<"emulators" | "storage" | "server">("emulators");
const setupDone = ref(localStorage.getItem("setupDone") === "1");

async function refresh() {
  settings.value = await api.settings();
  if (!isSettingsWindow && settings.value.configured && setupDone.value) await api.openRomm();
}

async function finishSetup() {
  localStorage.setItem("setupDone", "1");
  setupDone.value = true;
  await refresh();
}

onMounted(refresh);
</script>

<template>
  <template v-if="settings">
    <div v-if="isSettingsWindow" class="shell">
      <nav>
        <strong class="logo">Aether<span>Play</span></strong>
        <button class="btn" :class="{ primary: view === 'emulators' }" @click="view = 'emulators'">Émulateurs</button>
        <button class="btn" :class="{ primary: view === 'storage' }" @click="view = 'storage'">Stockage</button>
        <button class="btn" :class="{ primary: view === 'server' }" @click="view = 'server'">Serveur</button>
      </nav>
      <div class="content">
        <Emulators v-if="view === 'emulators'" :settings="settings" @changed="refresh" />
        <Storage v-else-if="view === 'storage'" />
        <Server v-else :server-url="settings.server_url" @done="refresh" />
      </div>
    </div>
    <Server v-else-if="!settings.configured" :server-url="settings.server_url" @done="refresh" />
    <Setup v-else-if="!setupDone" @done="finishSetup" />
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
.content { flex: 1; min-height: 0; }
</style>
