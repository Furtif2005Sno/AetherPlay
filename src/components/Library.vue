<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api, coverUrl, formatSize, type Platform, type Rom } from "../api";

const platforms = ref<Platform[]>([]);
const current = ref<Platform | null>(null);
const roms = ref<Rom[]>([]);
const search = ref("");
const error = ref("");
const loading = ref(false);
// rom id -> download percentage, or "running"
const status = ref<Record<number, number | "running">>({});

const filtered = computed(() => {
  const q = search.value.toLowerCase();
  return q ? roms.value.filter((r) => (r.name ?? r.fs_name).toLowerCase().includes(q)) : roms.value;
});

async function selectPlatform(p: Platform) {
  current.value = p;
  loading.value = true;
  error.value = "";
  try {
    roms.value = await api.roms(p.id);
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

async function play(rom: Rom) {
  if (status.value[rom.id] !== undefined) return;
  status.value[rom.id] = 0;
  error.value = "";
  try {
    await api.play(rom.id);
    status.value[rom.id] = "running";
  } catch (e) {
    delete status.value[rom.id];
    error.value = String(e);
  }
}

const unlisten: UnlistenFn[] = [];
onMounted(async () => {
  unlisten.push(
    await listen<{ rom_id: number; done: number; total: number }>("download-progress", ({ payload }) => {
      status.value[payload.rom_id] = payload.total ? Math.round((payload.done / payload.total) * 100) : 0;
    }),
    await listen<{ rom_id: number }>("game-exited", ({ payload }) => delete status.value[payload.rom_id]),
  );
  try {
    platforms.value = (await api.platforms()).filter((p) => p.rom_count > 0);
    if (platforms.value[0]) selectPlatform(platforms.value[0]);
  } catch (e) {
    error.value = String(e);
  }
});
onUnmounted(() => unlisten.forEach((u) => u()));

const label = (s: number | "running" | undefined) =>
  s === undefined ? "Jouer" : s === "running" ? "En cours" : `${s} %`;
</script>

<template>
  <div class="library">
    <aside>
      <button
        v-for="p in platforms"
        :key="p.id"
        class="plat"
        :class="{ active: current?.id === p.id }"
        @click="selectPlatform(p)"
      >
        <span>{{ p.display_name || p.name }}</span>
        <span class="muted">{{ p.rom_count }}</span>
      </button>
    </aside>
    <main>
      <header>
        <h2>{{ current?.display_name || current?.name || "Bibliothèque" }}</h2>
        <input v-model="search" class="input search" placeholder="Rechercher…" />
      </header>
      <p v-if="error" class="error">{{ error }}</p>
      <p v-if="loading" class="muted">Chargement…</p>
      <div class="grid">
        <div v-for="rom in filtered" :key="rom.id" class="card">
          <div class="cover">
            <img v-if="rom.path_cover_small" :src="coverUrl(rom.path_cover_small)" loading="lazy" alt="" />
            <span v-else class="muted">{{ rom.name ?? rom.fs_name }}</span>
            <button class="btn primary play" :class="{ busy: status[rom.id] !== undefined }" @click="play(rom)">
              {{ label(status[rom.id]) }}
            </button>
          </div>
          <div class="title" :title="rom.name ?? rom.fs_name">{{ rom.name ?? rom.fs_name }}</div>
          <div class="muted size">{{ formatSize(rom.fs_size_bytes) }}</div>
        </div>
      </div>
    </main>
  </div>
</template>

<style scoped>
.library { display: flex; height: 100%; }
aside {
  width: 220px; padding: 16px 10px; overflow-y: auto;
  border-right: 1px solid var(--r-color-border); display: flex; flex-direction: column; gap: 2px;
}
.plat {
  display: flex; justify-content: space-between; padding: 8px 12px; border: 0; background: none;
  border-radius: var(--r-radius-md); cursor: pointer; text-align: left;
}
.plat:hover { background: var(--r-color-surface); }
.plat.active { background: color-mix(in srgb, var(--r-color-brand-primary) 25%, transparent); }
main { flex: 1; overflow-y: auto; padding: 20px 24px; }
header { display: flex; align-items: center; justify-content: space-between; gap: 16px; margin-bottom: 16px; }
h2 { font-size: 22px; }
.search { max-width: 280px; }
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 18px; }
.cover {
  position: relative; aspect-ratio: 3 / 4; border-radius: var(--r-radius-card); overflow: hidden;
  background: var(--r-color-surface); display: grid; place-items: center; text-align: center; padding: 0;
  box-shadow: var(--r-shadow);
}
.cover img { width: 100%; height: 100%; object-fit: cover; }
.cover > span { padding: 12px; }
.play { position: absolute; bottom: 10px; left: 50%; transform: translateX(-50%); opacity: 0; transition: opacity 0.15s; }
.card:hover .play, .play.busy { opacity: 1; }
.title { margin-top: 8px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.size { font-size: 11.5px; }
</style>
