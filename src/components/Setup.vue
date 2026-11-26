<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api, type Emulator } from "../api";
import { GROUPS, install, installable, installLabel, installs } from "../installer";

const emit = defineEmits<{ done: [] }>();
const catalog = ref<Record<string, Emulator>>({});
const canInstall = ref<string[]>([]);
const selected = ref<string[]>(["retroarch"]);
const running = ref(false);

onMounted(async () => {
  const [list, ids] = await Promise.all([api.catalog(), installable()]);
  catalog.value = Object.fromEntries(list.map((e) => [e.id, e]));
  canInstall.value = ids;
});

const groups = computed(() =>
  GROUPS.map((g) => ({ ...g, ids: g.ids.filter((id) => catalog.value[id]) })).filter((g) => g.ids.length),
);
const finished = computed(() => selected.value.every((id) => installs[id] === "done"));

async function start() {
  running.value = true;
  // Sequential: keeps bandwidth and disk usage predictable.
  for (const id of selected.value) if (installs[id] !== "done") await install(id);
  running.value = false;
}
</script>

<template>
  <div class="page">
    <div class="panel box">
      <h2>Choisis tes émulateurs</h2>
      <p class="muted">Ils sont téléchargés depuis leurs sources officielles. Tu pourras en ajouter ou en retirer plus tard.</p>
      <section v-for="g in groups" :key="g.name">
        <h3>{{ g.name }}</h3>
        <label v-for="id in g.ids" :key="id" class="emu" :class="{ off: !canInstall.includes(id) }">
          <input v-model="selected" type="checkbox" :value="id" :disabled="!canInstall.includes(id) || running" />
          <strong>{{ catalog[id].name }}</strong>
          <span class="muted">{{ catalog[id].platforms.join(", ") }}</span>
          <span v-if="catalog[id].experimental" class="tag">expérimental</span>
          <span v-if="!canInstall.includes(id)" class="muted">installation manuelle</span>
          <span class="state" :class="{ error: typeof installs[id] === 'object' }" :title="typeof installs[id] === 'object' ? (installs[id] as any).error : ''">
            {{ installLabel(installs[id]) }}
          </span>
        </label>
      </section>
      <div class="actions">
        <button class="btn" :disabled="running" @click="emit('done')">{{ finished ? "Continuer" : "Plus tard" }}</button>
        <button class="btn primary" :disabled="running || !selected.length || finished" @click="start">
          {{ running ? "Installation…" : `Installer (${selected.length})` }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.page { height: 100%; overflow-y: auto; display: grid; place-items: start center; padding: 32px; }
.box { width: min(760px, 100%); padding: 28px; display: flex; flex-direction: column; gap: 14px; }
h2 { font-size: 22px; }
h3 { font-size: 13px; color: var(--r-color-brand-primary); margin: 6px 0; }
.emu { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: var(--r-radius-md); cursor: pointer; }
.emu:hover { background: var(--r-color-surface); }
.emu.off { opacity: 0.55; cursor: default; }
.emu input { accent-color: var(--r-color-brand-primary); }
.state { margin-left: auto; font-size: 11.5px; }
.actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 8px; }
</style>
