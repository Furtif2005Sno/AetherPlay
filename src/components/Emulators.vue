<script setup lang="ts">
import { onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type Emulator } from "../api";
import { install, installable, installLabel, installs } from "../installer";

const props = defineProps<{ paths: Record<string, string> }>();
const emit = defineEmits<{ changed: [] }>();
const list = ref<Emulator[]>([]);
const isMac = navigator.userAgent.includes("Mac");

const canInstall = ref<string[]>([]);

onMounted(async () => {
  [list.value, canInstall.value] = await Promise.all([api.catalog(), installable()]);
});

async function doInstall(emu: Emulator) {
  if (await install(emu.id)) emit("changed");
}

async function pick(emu: Emulator) {
  // On macOS the user picks the .app bundle (a directory).
  const path = await open({ directory: isMac, title: `Exécutable de ${emu.name}` });
  if (typeof path === "string") {
    await api.setEmulatorPath(emu.id, path);
    emit("changed");
  }
}

async function clear(emu: Emulator) {
  await api.setEmulatorPath(emu.id, null);
  emit("changed");
}
</script>

<template>
  <div class="page">
    <h2>Émulateurs</h2>
    <p class="muted">Installe un émulateur en un clic, ou indique où se trouve une version déjà installée.</p>
    <div v-for="emu in list" :key="emu.id" class="row panel">
      <div class="info">
        <strong>{{ emu.name }}</strong>
        <span v-if="emu.experimental" class="tag">expérimental</span>
        <div class="muted small">{{ emu.platforms.join(", ") }}</div>
        <div class="small path" :class="{ muted: !props.paths[emu.id] }">
          {{ props.paths[emu.id] || "Non configuré" }}
        </div>
      </div>
      <span class="small" :class="{ error: typeof installs[emu.id] === 'object' }" :title="typeof installs[emu.id] === 'object' ? (installs[emu.id] as any).error : ''">
        {{ installLabel(installs[emu.id]) }}
      </span>
      <button
        v-if="canInstall.includes(emu.id)"
        class="btn primary"
        :disabled="typeof installs[emu.id] === 'number'"
        @click="doInstall(emu)"
      >
        {{ props.paths[emu.id] ? "Réinstaller" : "Installer" }}
      </button>
      <button class="btn" @click="pick(emu)">Choisir…</button>
      <button v-if="props.paths[emu.id]" class="btn" @click="clear(emu)">Retirer</button>
    </div>
  </div>
</template>

<style scoped>
.page { padding: 24px; overflow-y: auto; height: 100%; display: flex; flex-direction: column; gap: 10px; }
h2 { font-size: 22px; }
.row { display: flex; align-items: center; gap: 10px; padding: 14px 18px; }
.info { flex: 1; min-width: 0; }
.small { font-size: 11.5px; }
.path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tag { margin-left: 8px; }
</style>
