<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type Emulator, type Settings } from "../api";
import { install, installable, installLabel, installs } from "../installer";

const props = defineProps<{ settings: Settings }>();
const emit = defineEmits<{ changed: [] }>();
const list = ref<Emulator[]>([]);
const canInstall = ref<string[]>([]);
const saveDirs = ref<Record<string, string[]>>({});
const isMac = navigator.userAgent.includes("Mac");

const loadSaveDirs = async () => (saveDirs.value = await api.saveDirs());
onMounted(async () => {
  [list.value, canInstall.value] = await Promise.all([api.catalog(), installable()]);
  loadSaveDirs();
});
watch(() => props.settings, loadSaveDirs);

/** Platforms more than one emulator can run, with their candidates. */
const shared = computed(() => {
  const by: Record<string, Emulator[]> = {};
  for (const e of list.value) for (const p of e.platforms) (by[p] ??= []).push(e);
  return Object.entries(by).filter(([, emus]) => emus.length > 1).sort(([a], [b]) => a.localeCompare(b));
});

async function change(action: Promise<unknown>) {
  await action;
  emit("changed");
}

async function doInstall(emu: Emulator) {
  if (await install(emu.id)) emit("changed");
}

async function pick(emu: Emulator) {
  // On macOS the user picks the .app bundle (a directory).
  const path = await open({ directory: isMac, title: `Exécutable de ${emu.name}` });
  if (typeof path === "string") change(api.setEmulatorPath(emu.id, path));
}

async function pickSaves(emu: Emulator) {
  const path = await open({ directory: true, title: `Dossier de sauvegardes de ${emu.name}` });
  if (typeof path === "string") change(api.setSaveDir(emu.id, path));
}

function setPreferred(platform: string, ev: Event) {
  const id = (ev.target as HTMLSelectElement).value;
  change(api.setPlatformEmulator(platform, id || null));
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
        <div class="small path" :class="{ muted: !settings.emulator_paths[emu.id] }">
          {{ settings.emulator_paths[emu.id] || "Non configuré" }}
        </div>
        <div class="small path muted">
          Saves : {{ saveDirs[emu.id]?.length ? saveDirs[emu.id].join(" · ") : "pas de synchronisation" }}
          <a href="#" @click.prevent="pickSaves(emu)">modifier</a>
          <a v-if="settings.save_dirs[emu.id]" href="#" @click.prevent="change(api.setSaveDir(emu.id, null))">par défaut</a>
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
        {{ settings.emulator_paths[emu.id] ? "Réinstaller" : "Installer" }}
      </button>
      <button class="btn" @click="pick(emu)">Choisir…</button>
      <button v-if="settings.emulator_paths[emu.id]" class="btn" @click="change(api.setEmulatorPath(emu.id, null))">Retirer</button>
    </div>

    <h3>Émulateur par console</h3>
    <p class="muted">Quand plusieurs émulateurs gèrent une console. « Automatique » prend le premier installé.</p>
    <div class="panel prefs">
      <label v-for="[platform, emus] in shared" :key="platform" class="pref">
        <span>{{ platform }}</span>
        <select class="input" :value="settings.platform_emulators[platform] ?? ''" @change="setPreferred(platform, $event)">
          <option value="">Automatique</option>
          <option v-for="e in emus" :key="e.id" :value="e.id" :disabled="!settings.emulator_paths[e.id]">
            {{ e.name }}{{ settings.emulator_paths[e.id] ? "" : " (non installé)" }}
          </option>
        </select>
      </label>
    </div>
  </div>
</template>

<style scoped>
.page { padding: 24px; overflow-y: auto; height: 100%; display: flex; flex-direction: column; gap: 10px; }
h2 { font-size: 22px; }
h3 { font-size: 17px; margin-top: 14px; }
.row { display: flex; align-items: center; gap: 10px; padding: 14px 18px; }
.info { flex: 1; min-width: 0; }
.small { font-size: 11.5px; }
.path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tag { margin-left: 8px; }
a { color: var(--r-color-brand-primary); margin-left: 6px; }
.prefs { padding: 14px 18px; display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 10px 20px; }
.pref { display: flex; align-items: center; gap: 10px; }
.pref span { width: 90px; }
.pref select { padding: 6px 10px; }
</style>
