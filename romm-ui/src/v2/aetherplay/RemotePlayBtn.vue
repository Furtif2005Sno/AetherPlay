<script setup lang="ts">
// AetherPlay: "Jouer à distance" on a game's page. Pick who is invited, then
// RetroArch starts as netplay host and the invitation goes through RomM
// (see netplay.rs).
import { RBtn } from "@v2/lib";
import { computed, ref } from "vue";
import { app, isAetherPlay } from "@/v2/aetherplay/api";

const props = defineProps<{ rom: { id: number } }>();
const open = ref(false);
const users = ref<string[]>([]);
/** RomM refused the user list (not an admin): names are typed instead. */
const typed = ref("");
const listRefused = ref(false);
const picked = ref<string[]>([]);
const busy = ref(false);
const error = ref("");

const recipients = computed(() =>
  listRefused.value
    ? typed.value
        .split(",")
        .map((n) => n.trim())
        .filter(Boolean)
    : picked.value,
);

async function toggle() {
  open.value = !open.value;
  if (!open.value || users.value.length || listRefused.value) return;
  try {
    users.value = await app.netplayUsers();
  } catch {
    listRefused.value = true;
  }
}

async function host() {
  busy.value = true;
  error.value = "";
  try {
    await app.netplayHost(props.rom.id, recipients.value);
    open.value = false;
  } catch (e) {
    const err = e as { message?: string };
    error.value = err?.message ?? String(e);
  }
  busy.value = false;
}
</script>

<template>
  <div v-if="isAetherPlay()" class="aep-remote">
    <RBtn size="small" variant="outlined" prepend-icon="mdi-account-multiple-outline" @click="toggle">
      Jouer à distance
    </RBtn>
    <div v-if="open" class="aep-remote__panel">
      <div class="aep-remote__title">Qui inviter ?</div>
      <template v-if="!listRefused">
        <label v-for="u in users" :key="u" class="aep-remote__user">
          <input v-model="picked" type="checkbox" :value="u" />
          {{ u }}
        </label>
        <p v-if="!users.length" class="aep-remote__hint">Aucun autre utilisateur sur le serveur.</p>
      </template>
      <template v-else>
        <input v-model="typed" class="aep-remote__input" placeholder="Pseudos RomM, séparés par des virgules" />
      </template>
      <p class="aep-remote__hint">
        {{
          recipients.length
            ? `Invitation pour : ${recipients.join(", ")}.`
            : "Personne de coché : tous les utilisateurs du serveur sont invités."
        }}
        Ils rejoignent depuis AetherPlay.
      </p>
      <RBtn size="small" variant="flat" :loading="busy" @click="host">Lancer et inviter</RBtn>
    </div>
    <span v-if="error" class="aep-remote__error">{{ error }}</span>
  </div>
</template>

<style scoped>
.aep-remote {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 8px;
  margin-top: 8px;
}
.aep-remote__panel {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px 14px;
  border-radius: 12px;
  background: color-mix(in srgb, var(--r-color-bg, #121018) 85%, transparent);
  border: 1px solid color-mix(in srgb, var(--r-color-brand-accent, #8b5cf6) 40%, transparent);
  min-width: 280px;
}
.aep-remote__title {
  font-weight: 600;
}
.aep-remote__user {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}
.aep-remote__input {
  padding: 6px 10px;
  border-radius: 8px;
  border: 1px solid color-mix(in srgb, currentColor 30%, transparent);
  background: transparent;
  color: inherit;
}
.aep-remote__hint {
  margin: 0;
  font-size: var(--r-font-size-xs);
  opacity: 0.7;
}
.aep-remote__error {
  color: var(--r-color-status-error, #ff6b6b);
  font-size: var(--r-font-size-sm);
}
</style>
