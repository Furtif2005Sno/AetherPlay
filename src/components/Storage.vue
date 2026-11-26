<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api, formatSize, type CacheEntry } from "../api";

const items = ref<CacheEntry[]>([]);
const error = ref("");
const total = computed(() => items.value.reduce((s, i) => s + i.bytes, 0));

async function load() {
  items.value = await api.cacheList();
}
onMounted(load);

async function run(action: Promise<void>) {
  error.value = "";
  try {
    await action;
  } catch (e) {
    error.value = String(e);
  }
  await load();
}

function clearAll() {
  if (confirm("Supprimer toutes les ROMs téléchargées ? Elles seront retéléchargées au prochain lancement.")) {
    run(api.cacheClear());
  }
}
</script>

<template>
  <div class="page">
    <header>
      <div>
        <h2>Stockage</h2>
        <p class="muted">ROMs téléchargées : {{ formatSize(total) }} au total. Les sauvegardes ne sont pas touchées.</p>
      </div>
      <button class="btn" :disabled="!items.length" @click="clearAll">Tout supprimer</button>
    </header>
    <p v-if="error" class="error">{{ error }}</p>
    <p v-if="!items.length" class="muted">Aucune ROM en cache.</p>
    <div v-for="item in items" :key="item.path" class="row panel">
      <div class="info">
        <strong>{{ item.name }}</strong>
        <div class="muted small">{{ item.platform }}</div>
      </div>
      <span>{{ formatSize(item.bytes) }}</span>
      <button class="btn" @click="run(api.cacheDelete(item.path))">Supprimer</button>
    </div>
  </div>
</template>

<style scoped>
.page { padding: 24px; overflow-y: auto; height: 100%; display: flex; flex-direction: column; gap: 10px; }
header { display: flex; justify-content: space-between; align-items: center; }
h2 { font-size: 22px; }
.row { display: flex; align-items: center; gap: 14px; padding: 12px 18px; }
.info { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.small { font-size: 11.5px; }
</style>
