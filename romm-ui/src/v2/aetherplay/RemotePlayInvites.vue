<script setup lang="ts">
// AetherPlay: invitations to remote play from other users of the RomM server
// (polled every 15 s), and the state of the game this computer hosts.
import { RBtn } from "@v2/lib";
import { computed, onMounted, onUnmounted, ref } from "vue";
import { app, isAetherPlay, type NetplayInvite } from "@/v2/aetherplay/api";

const invites = ref<NetplayInvite[]>([]);
const dismissed = ref(new Set<string>());
const joining = ref("");
const error = ref("");
const hosting = ref<{ status: string; message?: string } | null>(null);
let timer: ReturnType<typeof setInterval> | undefined;
let unlisten: (() => void) | undefined;

const key = (i: NetplayInvite) => `${i.username}:${i.rom_id}:${i.session.session}:${i.session.port}`;
const shown = computed(() => invites.value.filter((i) => !dismissed.value.has(key(i))));

async function refresh() {
  try {
    invites.value = await app.netplayInvites();
  } catch {
    // Offline or logged out: try again later.
  }
}

async function join(i: NetplayInvite) {
  joining.value = key(i);
  error.value = "";
  try {
    await app.netplayJoin(i.rom_id, i.session);
    dismissed.value.add(key(i));
  } catch (e) {
    const err = e as { message?: string };
    error.value = err?.message ?? String(e);
  }
  joining.value = "";
}

onMounted(async () => {
  if (!isAetherPlay()) return;
  await refresh();
  timer = setInterval(refresh, 15000);
  unlisten = await app.onNetplay((s) => {
    hosting.value = s;
    if (s.status === "ready") setTimeout(() => (hosting.value = null), 15000);
  });
});
onUnmounted(() => {
  clearInterval(timer);
  unlisten?.();
});
</script>

<template>
  <div v-if="shown.length || hosting" class="aep-invites">
    <div v-if="hosting" class="aep-invite">
      <span v-if="hosting.status === 'searching'">Partie à distance : création de l'invitation…</span>
      <span v-else-if="hosting.status === 'ready'">
        Invitation envoyée : les autres joueurs de ton serveur peuvent rejoindre depuis AetherPlay.
      </span>
      <span v-else class="aep-invite__error">Partie à distance impossible : {{ hosting.message }}</span>
      <RBtn size="x-small" variant="text" @click="hosting = null">OK</RBtn>
    </div>
    <div v-for="i in shown" :key="key(i)" class="aep-invite">
      <span>
        <strong>{{ i.username }}</strong> t'invite à jouer à <strong>{{ i.rom_name }}</strong>
      </span>
      <RBtn size="small" variant="flat" :loading="joining === key(i)" @click="join(i)">Rejoindre</RBtn>
      <RBtn size="x-small" variant="text" @click="dismissed.add(key(i))">Plus tard</RBtn>
    </div>
    <div v-if="error" class="aep-invite aep-invite__error">{{ error }}</div>
  </div>
</template>

<style scoped>
.aep-invites {
  position: fixed;
  right: 16px;
  bottom: 16px;
  z-index: 2000;
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-width: 420px;
}
.aep-invite {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border-radius: 12px;
  background: color-mix(in srgb, var(--r-color-bg, #121018) 92%, transparent);
  border: 1px solid color-mix(in srgb, var(--r-color-brand-accent, #8b5cf6) 50%, transparent);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  font-size: var(--r-font-size-sm);
}
.aep-invite__error {
  color: var(--r-color-status-error, #ff6b6b);
}
</style>
