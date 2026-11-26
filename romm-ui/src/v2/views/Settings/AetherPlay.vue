<script setup lang="ts">
// AetherPlay: the desktop app's own settings (emulators, launch options,
// controller, local storage), built from RomM's settings components.
import { RBtn, RSelect } from "@v2/lib";
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  app,
  formatSize,
  type AppSettings,
  type CacheEntry,
  type Emulator,
} from "@/v2/aetherplay/api";
import SettingsSection from "@/v2/components/Settings/SettingsSection.vue";
import SettingsToggleRow from "@/v2/components/Settings/SettingsToggleRow.vue";

const settings = ref<AppSettings | null>(null);
const catalog = ref<Emulator[]>([]);
const installable = ref<string[]>([]);
const saveDirs = ref<Record<string, string[]>>({});
const cache = ref<CacheEntry[]>([]);
/** Emulator id -> download percentage while installing, or an error message. */
const installs = ref<Record<string, number | string>>({});
const error = ref("");
let unlisten: (() => void) | undefined;

const isMac = navigator.userAgent.includes("Mac");
const cacheTotal = computed(() => cache.value.reduce((s, e) => s + e.bytes, 0));

/** Consoles more than one emulator can run, with their candidates. */
const shared = computed(() => {
  const by: Record<string, Emulator[]> = {};
  for (const e of catalog.value) for (const p of e.platforms) (by[p] ??= []).push(e);
  return Object.entries(by)
    .filter(([, emus]) => emus.length > 1)
    .sort(([a], [b]) => a.localeCompare(b));
});

async function refresh() {
  [settings.value, saveDirs.value, cache.value] = await Promise.all([
    app.settings(),
    app.saveDirs(),
    app.cacheList(),
  ]);
}

async function run(action: () => Promise<unknown>) {
  error.value = "";
  try {
    await action();
  } catch (e) {
    error.value = String(e);
  }
  await refresh();
}

async function install(emu: Emulator) {
  installs.value[emu.id] = 0;
  try {
    await app.install(emu.id);
    delete installs.value[emu.id];
  } catch (e) {
    installs.value[emu.id] = String(e);
  }
  await refresh();
}

async function pickExe(emu: Emulator) {
  const path = await app.pickPath(false, `Exécutable de ${emu.name}`);
  if (path) await run(() => app.setEmulatorPath(emu.id, path));
}

async function pickSaves(emu: Emulator) {
  const path = await app.pickPath(true, `Dossier de sauvegardes de ${emu.name}`);
  if (path) await run(() => app.setSaveDir(emu.id, path));
}

const notice = ref("");

const canImport = (emu: Emulator) => Object.keys(emu.config_ini ?? {}).length > 0;

async function importSave(emu: Emulator) {
  const path = await app.pickPath(false, `Sauvegarde à importer dans ${emu.name} (fichier .zip)`);
  if (!path) return;
  error.value = "";
  notice.value = "";
  try {
    const report = await app.importSave(emu.id, path);
    notice.value =
      `Sauvegarde importée (${report.files} fichiers, jeu ${report.title_id}). ` +
      "L'ancienne a été mise de côté. Lance le jeu depuis AetherPlay : à la fermeture, elle sera envoyée sur RomM.";
  } catch (e) {
    error.value = String(e);
  }
}

async function openLog() {
  error.value = "";
  try {
    await app.openLog();
  } catch (e) {
    error.value = String(e);
  }
}

function clearCache() {
  if (confirm("Supprimer toutes les ROMs téléchargées ? Elles seront retéléchargées au prochain lancement.")) {
    run(() => app.cacheClear());
  }
}

const preferredItems = (emus: Emulator[]) => [
  { title: "Automatique", value: "" },
  ...emus.map((e) => ({
    title: settings.value?.emulator_paths[e.id] ? e.name : `${e.name} (non installé)`,
    value: e.id,
  })),
];

onMounted(async () => {
  [catalog.value, installable.value] = await Promise.all([app.catalog(), app.installable()]);
  await refresh();
  unlisten = await app.onInstallProgress(({ id, done, total }) => {
    if (typeof installs.value[id] === "number" || installs.value[id] === undefined) {
      installs.value[id] = total ? Math.round((done / total) * 100) : 0;
    }
  });
});
onUnmounted(() => unlisten?.());
</script>

<template>
  <div v-if="settings" class="r-v2-section-stack">
    <p v-if="error" class="aep-error">{{ error }}</p>
    <p v-if="notice" class="aep-notice">{{ notice }}</p>

    <SettingsSection title="Lancement des jeux" icon="mdi-rocket-launch-outline">
      <div class="aep-toggles">
        <SettingsToggleRow
          :model-value="settings.fullscreen"
          title="Plein écran"
          description="Démarre les émulateurs en plein écran."
          @update:model-value="(v: boolean) => run(() => app.setOption('fullscreen', v))"
        />
      </div>
    </SettingsSection>

    <SettingsSection title="Manette" icon="mdi-controller">
      <div class="aep-toggles">
        <SettingsToggleRow
          :model-value="settings.quit_combo"
          title="Quitter le jeu avec Select + Start"
          description="Maintenir Select + Start 1,5 seconde ferme l'émulateur et revient à AetherPlay. L'interface se pilote entièrement à la manette."
          @update:model-value="(v: boolean) => run(() => app.setOption('quit_combo', v))"
        />
      </div>
    </SettingsSection>

    <SettingsSection title="Émulateurs" icon="mdi-gamepad-variant-outline">
      <div class="aep-list">
        <div v-for="emu in catalog" :key="emu.id" class="aep-row">
          <div class="aep-row__info">
            <div class="aep-row__title">
              {{ emu.name }}
              <span v-if="emu.experimental" class="aep-tag">expérimental</span>
            </div>
            <div class="aep-muted">{{ emu.platforms.join(", ") }}</div>
            <div class="aep-muted aep-path">
              {{ settings.emulator_paths[emu.id] || "Non installé" }}
            </div>
            <div class="aep-muted aep-path">
              Sauvegardes :
              {{ saveDirs[emu.id]?.join(" · ") || "pas de synchronisation" }}
              <a href="#" @click.prevent="pickSaves(emu)">modifier</a>
              <a v-if="settings.save_dirs[emu.id]" href="#" @click.prevent="run(() => app.setSaveDir(emu.id, null))">
                par défaut
              </a>
            </div>
            <div v-if="typeof installs[emu.id] === 'string'" class="aep-error">{{ installs[emu.id] }}</div>
          </div>
          <div class="aep-row__actions">
            <RBtn
              v-if="installable.includes(emu.id)"
              size="small"
              :loading="typeof installs[emu.id] === 'number'"
              @click="install(emu)"
            >
              {{
                typeof installs[emu.id] === "number"
                  ? `${installs[emu.id]} %`
                  : settings.emulator_paths[emu.id]
                    ? "Réinstaller"
                    : "Installer"
              }}
            </RBtn>
            <RBtn
              v-if="canImport(emu) && settings.emulator_paths[emu.id]"
              size="small"
              variant="outlined"
              prepend-icon="mdi-file-import-outline"
              @click="importSave(emu)"
            >
              Importer une sauvegarde…
            </RBtn>
            <RBtn size="small" variant="outlined" @click="pickExe(emu)">
              {{ isMac ? "Choisir l'app…" : "Choisir…" }}
            </RBtn>
            <RBtn
              v-if="settings.emulator_paths[emu.id]"
              size="small"
              variant="text"
              @click="run(() => app.setEmulatorPath(emu.id, null))"
            >
              Retirer
            </RBtn>
          </div>
        </div>
      </div>
    </SettingsSection>

    <SettingsSection title="Émulateur par console" icon="mdi-swap-horizontal">
      <div class="aep-prefs">
        <div v-for="[platform, emus] in shared" :key="platform" class="aep-pref">
          <span class="aep-pref__label">{{ platform }}</span>
          <RSelect
            :model-value="settings.platform_emulators[platform] ?? ''"
            :items="preferredItems(emus)"
            density="compact"
            hide-details
            @update:model-value="(v: unknown) => run(() => app.setPlatformEmulator(platform, (v as string) || null))"
          />
        </div>
      </div>
    </SettingsSection>

    <SettingsSection title="Stockage local" icon="mdi-harddisk">
      <template #header-actions>
        <RBtn size="small" variant="outlined" :disabled="!cache.length" @click="clearCache">Tout supprimer</RBtn>
      </template>
      <div class="aep-list">
        <p class="aep-muted aep-pad">
          ROMs téléchargées : {{ formatSize(cacheTotal) }}. Les sauvegardes ne sont pas touchées.
        </p>
        <div v-for="item in cache" :key="item.path" class="aep-row">
          <div class="aep-row__info">
            <div class="aep-row__title aep-path">{{ item.name }}</div>
            <div class="aep-muted">{{ item.platform }} · {{ formatSize(item.bytes) }}</div>
          </div>
          <RBtn size="small" variant="text" @click="run(() => app.cacheDelete(item.path))">Supprimer</RBtn>
        </div>
      </div>
    </SettingsSection>

    <SettingsSection title="Serveur" icon="mdi-server-network">
      <div class="aep-row">
        <div class="aep-row__info">
          <div class="aep-row__title">{{ settings.server_url }}</div>
          <div class="aep-muted">Serveur RomM utilisé par AetherPlay.</div>
        </div>
        <RBtn size="small" variant="outlined" @click="app.openAppSettings()">Changer…</RBtn>
      </div>
    </SettingsSection>

    <SettingsSection title="Diagnostic" icon="mdi-file-document-outline">
      <div class="aep-row">
        <div class="aep-row__info">
          <div class="aep-row__title">Journal de diagnostic</div>
          <div class="aep-muted">
            Connexion au serveur, lancements et synchronisation des sauvegardes (sans mot de passe). À envoyer en cas de problème.
          </div>
        </div>
        <RBtn size="small" variant="outlined" @click="openLog">Afficher le fichier</RBtn>
      </div>
    </SettingsSection>
  </div>
</template>

<style scoped>
.aep-toggles {
  display: flex;
  flex-direction: column;
}
.aep-list {
  display: flex;
  flex-direction: column;
}
.aep-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  border-top: 1px solid var(--r-color-border);
}
.aep-list > .aep-row:first-child,
.aep-list > .aep-pad + .aep-row {
  border-top: none;
}
.aep-row__info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.aep-row__title {
  color: var(--r-color-fg);
  font-weight: var(--r-font-weight-semibold, 600);
}
.aep-row__actions {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}
.aep-muted {
  color: var(--r-color-fg-muted);
  font-size: var(--r-font-size-sm);
}
.aep-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.aep-pad {
  padding: 12px 16px 4px;
}
.aep-tag {
  margin-left: 6px;
  font-size: var(--r-font-size-xs);
  padding: 1px 8px;
  border-radius: var(--r-radius-pill);
  color: var(--r-color-brand-accent);
  background: color-mix(in srgb, var(--r-color-brand-accent) 20%, transparent);
}
.aep-error {
  color: var(--r-color-status-error, #ff6b6b);
  font-size: var(--r-font-size-sm);
}
.aep-notice {
  color: var(--r-color-status-success, #4ade80);
  font-size: var(--r-font-size-sm);
}
a {
  color: var(--r-color-brand-primary);
  margin-left: 6px;
}
.aep-prefs {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 10px 24px;
  padding: 14px 16px;
}
.aep-pref {
  display: flex;
  align-items: center;
  gap: 12px;
}
.aep-pref__label {
  width: 120px;
  color: var(--r-color-fg-secondary);
}
.aep-pref :deep(> :last-child) {
  flex: 1;
}
</style>
