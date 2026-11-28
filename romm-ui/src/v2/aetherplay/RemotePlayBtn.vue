<script setup lang="ts">
// AetherPlay: "Jouer à distance" on a game's page. Starts RetroArch as netplay
// host and invites the other users of the RomM server (see netplay.rs).
import { RBtn } from "@v2/lib";
import { ref } from "vue";
import { app, isAetherPlay } from "@/v2/aetherplay/api";

const props = defineProps<{ rom: { id: number } }>();
const busy = ref(false);
const error = ref("");

async function host() {
  busy.value = true;
  error.value = "";
  try {
    await app.netplayHost(props.rom.id);
  } catch (e) {
    const err = e as { message?: string };
    error.value = err?.message ?? String(e);
  }
  busy.value = false;
}
</script>

<template>
  <div v-if="isAetherPlay()" class="aep-remote">
    <RBtn size="small" variant="outlined" prepend-icon="mdi-account-multiple-outline" :loading="busy" @click="host">
      Jouer à distance
    </RBtn>
    <span class="aep-remote__hint">Les autres joueurs de ton serveur RomM pourront rejoindre depuis AetherPlay.</span>
    <span v-if="error" class="aep-remote__error">{{ error }}</span>
  </div>
</template>

<style scoped>
.aep-remote {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 12px;
  margin-top: 8px;
}
.aep-remote__hint {
  font-size: var(--r-font-size-xs);
  opacity: 0.7;
}
.aep-remote__error {
  width: 100%;
  color: var(--r-color-status-error, #ff6b6b);
  font-size: var(--r-font-size-sm);
}
</style>
