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
  type PhoneStatus,
} from "@/v2/aetherplay/api";
import SettingsSection from "@/v2/components/Settings/SettingsSection.vue";
import SettingsToggleRow from "@/v2/components/Settings/SettingsToggleRow.vue";

const settings = ref<AppSettings | null>(null);
const catalog = ref<Emulator[]>([]);
const installable = ref<string[]>([]);
const saveDirs = ref<Record<string, string[]>>({});
const cache = ref<CacheEntry[]>([]);
const imageCache = ref(0);
/** Switch emulators set up here and whether they have decryption keys. */
const switchKeys = ref<{ id: string; name: string; keys: boolean; dir: string }[]>([]);
/** Emulator id -> download percentage while installing, or an error message. */
const installs = ref<Record<string, number | string>>({});
const error = ref("");
let unlisten: (() => void) | undefined;
const phone = ref<PhoneStatus | null>(null);
let phoneTimer: ReturnType<typeof setInterval> | undefined;
/** A controller seen on this computer; `phone`: the Android app over Bluetooth. */
interface DetectedPad {
  name: string;
  phone: boolean;
}
const detected = ref<DetectedPad[]>([]);

const refreshPhone = async () => {
  phone.value = await app.phoneStatus();
  // The webview's own list (Windows: every controller, after a button press),
  // plus what AetherPlay reads itself. The Android app's Bluetooth controller
  // has its own report layout: 17 buttons, 4 axes.
  const native = (
    (window as unknown as { __aetherplayNativeGamepads?: () => Gamepad[] }).__aetherplayNativeGamepads?.() ?? []
  ).map((p) => ({ name: p.id.replace(/\s*\(.*\)\s*$/, "") || p.id, phone: p.buttons.length === 17 && p.axes.length === 4 }));
  const seen = new Set(native.map((p) => p.name));
  detected.value = [...native, ...phone.value.pads.filter((n) => !seen.has(n)).map((name) => ({ name, phone: false }))];
};
const playerItems = [1, 2, 3, 4].map((n) => ({ title: `Joueur ${n}`, value: n }));
async function setPlayer(id: number, player: number) {
  await app.phoneSetPlayer(id, player);
  await refreshPhone();
}
async function setBtPlayer(name: string, player: number) {
  await app.phoneSetBtPlayer(name, player || null);
  await refreshPhone();
}

const serverCode = ref<{ server: string; code: string; expires_in: number } | null>(null);
async function linkPhoneToServer() {
  error.value = "";
  try {
    serverCode.value = await app.phoneServerCode();
  } catch (e) {
    error.value = String(e);
  }
}

const installingVigem = ref(false);
async function installVigem() {
  installingVigem.value = true;
  error.value = "";
  try {
    await app.installVigem();
    notice.value = "Pilote installé : la manette Android pilote maintenant les jeux en Wi-Fi.";
  } catch (e) {
    error.value = String(e);
  }
  installingVigem.value = false;
  await refreshPhone();
}

async function forgetPhones() {
  if (confirm("Oublier tous les téléphones associés ? Ils devront retaper le code.")) {
    await run(() => app.phoneForget());
  }
}

const isMac = navigator.userAgent.includes("Mac");

/** What each controller button does in AetherPlay (RomM's pad navigation). */
const legend = [
  ["Croix / stick gauche", "se déplacer"],
  ["A", "valider"],
  ["B", "retour"],
  ["LB / RB", "section précédente / suivante"],
  ["Start", "menu"],
  ["Stick droit", "faire défiler"],
  ["Select + Start (1,5 s)", "quitter le jeu"],
];
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
  switchKeys.value = await app.switchKeysStatus().catch(() => []);
  [settings.value, saveDirs.value, cache.value, imageCache.value] = await Promise.all([
    app.settings(),
    app.saveDirs(),
    app.cacheList(),
    app.imageCacheSize(),
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

async function importKeys() {
  const path = await app.pickPath(false, "Fichier prod.keys de ta Switch");
  if (!path) return;
  error.value = "";
  notice.value = "";
  try {
    const count = await app.importSwitchKeys(path);
    notice.value = `Clés installées dans ${count} émulateur(s) Switch (title.keys aussi, s'il était à côté).`;
  } catch (e) {
    error.value = String(e);
  }
  await refresh();
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
  await refreshPhone();
  phoneTimer = setInterval(refreshPhone, 2000);
  await refresh();
  unlisten = await app.onInstallProgress(({ id, done, total }) => {
    if (typeof installs.value[id] === "number" || installs.value[id] === undefined) {
      installs.value[id] = total ? Math.round((done / total) * 100) : 0;
    }
  });
});
onUnmounted(() => {
  unlisten?.();
  clearInterval(phoneTimer);
});
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
        <div class="aep-legend">
          <span v-for="[button, action] in legend" :key="button" class="aep-legend__item">
            <kbd>{{ button }}</kbd> {{ action }}
          </span>
        </div>
      </div>
    </SettingsSection>

    <SettingsSection v-if="phone" title="Manette Android · RetroTogether" icon="mdi-cellphone-link">
      <template #header-actions>
        <RBtn size="small" variant="text" @click="forgetPhones">Oublier les téléphones</RBtn>
      </template>
      <div class="aep-row">
        <div class="aep-row__info">
          <div class="aep-row__title">Code d'association : <span class="aep-pin">{{ phone.pin }}</span></div>
          <div class="aep-muted">
            Dans l'app AetherPlay sur Android (même Wi-Fi), choisis cet ordinateur puis tape ce code, une seule fois.
            Chaque téléphone connecté est un joueur : plusieurs téléphones = partie à plusieurs.
          </div>
          <div class="aep-muted">
            Adresse de cet ordinateur : <span class="aep-pin">{{ phone.addresses.join(" · ") || "aucune" }}</span>.
            Réseau d'école ou public (appareils isolés) : active le partage de connexion USB du téléphone, ou connecte
            l'ordinateur au point d'accès du téléphone, puis tape cette adresse dans l'app si besoin.
          </div>
        </div>
      </div>
      <div class="aep-row">
        <div class="aep-row__info">
          <div class="aep-row__title">
            Manette en Wi-Fi pour les jeux :
            {{ phone.virtual ? "disponible" : phone.os === "macos" ? "impossible sur Mac" : "indisponible" }}
          </div>
          <div class="aep-muted">
            <template v-if="phone.virtual">Sans Bluetooth, le téléphone pilote aussi les jeux en Wi-Fi.</template>
            <template v-else-if="phone.os === 'windows'">
              Sans Bluetooth, le téléphone ne pilote que les menus. Pour les jeux en Wi-Fi, installe le pilote gratuit
              ViGEmBus (Windows demandera l'autorisation), ou connecte le téléphone en Bluetooth.
            </template>
            <template v-else-if="phone.os === 'macos'">
              macOS n'autorise pas les manettes virtuelles : connecte le téléphone en Bluetooth pour jouer.
            </template>
            <template v-else>Accès à /dev/uinput refusé : connecte le téléphone en Bluetooth pour jouer.</template>
          </div>
        </div>
        <RBtn
          v-if="!phone.virtual && phone.os === 'windows'"
          size="small"
          :loading="installingVigem"
          prepend-icon="mdi-download"
          @click="installVigem"
        >
          Installer le pilote
        </RBtn>
      </div>
      <div class="aep-row">
        <div class="aep-row__info">
          <div class="aep-row__title">Téléphone en Bluetooth seul (sans réseau commun)</div>
          <div class="aep-muted">
            Le téléphone lit le jeu en cours et son numéro de joueur directement sur ton serveur RomM. À faire une fois :
            génère un code, puis dans l'app (Connexion > Serveur RomM) tape l'adresse du serveur et ce code.
          </div>
          <div v-if="serverCode" class="aep-muted">
            Serveur : <span class="aep-pin">{{ serverCode.server }}</span> · Code :
            <span class="aep-pin">{{ serverCode.code }}</span>
            <template v-if="serverCode.expires_in"> (valable {{ Math.round(serverCode.expires_in / 60) }} min)</template>
          </div>
        </div>
        <RBtn size="small" variant="outlined" prepend-icon="mdi-link-variant" @click="linkPhoneToServer">
          Générer un code
        </RBtn>
      </div>
      <div class="aep-row">
        <div class="aep-row__info">
          <div class="aep-row__title">Manettes détectées</div>
          <div v-for="(p, i) in detected" :key="i" class="aep-muted">
            <span class="aep-tag">{{ p.phone ? "Android · Bluetooth" : "manette" }}</span> {{ p.name }}
          </div>
          <div class="aep-muted">
            {{ detected.length ? "" : "Aucune pour l'instant. " }}Une manette Bluetooth apparaît après un appui sur l'un de
            ses boutons. En Bluetooth seul, l'émulateur numérote les joueurs dans l'ordre de connexion des manettes ;
            pour J1 / J2 garantis, connecte aussi les téléphones en Wi-Fi (ou partage de connexion USB).
          </div>
        </div>
      </div>
      <div v-for="p in phone.phones" :key="p.id" class="aep-row">
        <div class="aep-row__info">
          <div class="aep-row__title"><span class="aep-tag">J{{ p.player }}</span> {{ p.name }}</div>
        </div>
        <RSelect
          class="aep-player"
          :model-value="p.player"
          :items="playerItems"
          density="compact"
          hide-details
          @update:model-value="(v: unknown) => setPlayer(p.id, v as number)"
        />
      </div>
      <div v-for="name in phone.bluetooth_phones" :key="'btn' + name" class="aep-row">
        <div class="aep-row__info">
          <div class="aep-row__title">{{ name }} <span class="aep-tag">Bluetooth</span></div>
          <div class="aep-muted">
            Numéro de joueur appliqué au prochain lancement (Eden, Eglantine, PCSX2, DuckStation, Dolphin ; RetroArch
            numérote dans l'ordre de connexion des manettes).
          </div>
        </div>
        <RSelect
          class="aep-player"
          :model-value="phone.overrides[name] ?? 0"
          :items="[{ title: 'Auto', value: 0 }, ...playerItems]"
          density="compact"
          hide-details
          @update:model-value="(v: unknown) => setBtPlayer(name, v as number)"
        />
      </div>
      <div v-for="n in phone.bluetooth" :key="'bt' + n" class="aep-row">
        <div class="aep-row__info">
          <div class="aep-row__title"><span class="aep-tag">J{{ n }}</span> Téléphone en Bluetooth</div>
        </div>
      </div>
      <p v-if="!phone.phones.length && !phone.bluetooth.length" class="aep-muted aep-pad">Aucun téléphone connecté.</p>
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
            <RBtn size="small" variant="text" prepend-icon="mdi-folder-sync-outline" @click="pickSaves(emu)">
              Dossier de sauvegardes…
            </RBtn>
            <RBtn
              v-if="settings.save_dirs[emu.id]"
              size="small"
              variant="text"
              @click="run(() => app.setSaveDir(emu.id, null))"
            >
              Dossier par défaut
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

    <SettingsSection v-if="switchKeys.length" title="Clés Switch" icon="mdi-key-variant">
      <template #header-actions>
        <RBtn size="small" variant="outlined" @click="importKeys">Importer mes clés…</RBtn>
      </template>
      <div class="aep-list">
        <p class="aep-muted aep-pad">
          Les jeux Switch ont besoin du fichier prod.keys de ta console (et de title.keys s'il est à côté). Il est
          installé dans tous tes émulateurs Switch. Au lancement, un émulateur sans clés reçoit celles d'un autre.
        </p>
        <div v-for="k in switchKeys" :key="k.id" class="aep-row">
          <div class="aep-row__info">
            <div class="aep-row__title">{{ k.name }}</div>
            <div class="aep-muted aep-path">{{ k.dir }}</div>
          </div>
          <span :class="k.keys ? 'aep-notice' : 'aep-error'">{{ k.keys ? "Clés présentes" : "Clés manquantes" }}</span>
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
        <div class="aep-row">
          <div class="aep-row__info">
            <div class="aep-row__title">Jaquettes et images en cache : {{ formatSize(imageCache) }}</div>
            <div class="aep-muted">Gardées sur l'ordinateur pour faire défiler la bibliothèque sans les retélécharger.</div>
          </div>
          <RBtn size="small" variant="text" :disabled="!imageCache" @click="run(() => app.imageCacheClear())">Vider</RBtn>
        </div>
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
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 6px;
  max-width: 60%;
}
.aep-player {
  max-width: 140px;
}
.aep-pin {
  font-family: var(--r-font-mono, monospace);
  letter-spacing: 0.2em;
  color: var(--r-color-brand-primary);
}
.aep-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 18px;
  padding: 4px 16px 14px;
  color: var(--r-color-fg-muted);
  font-size: var(--r-font-size-sm);
}
.aep-legend kbd {
  font: inherit;
  font-weight: var(--r-font-weight-semibold, 600);
  color: var(--r-color-fg);
  padding: 1px 8px;
  border-radius: var(--r-radius-pill);
  border: 1px solid var(--r-color-border);
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
