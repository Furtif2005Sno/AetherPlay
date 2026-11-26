import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Shared install state: emulator id -> percentage, "done" or an error message. */
export const installs = reactive<Record<string, number | "done" | { error: string }>>({});

listen<{ id: string; done: number; total: number }>("install-progress", ({ payload }) => {
  installs[payload.id] = payload.total ? Math.round((payload.done / payload.total) * 100) : 0;
});

export const installable = () => invoke<string[]>("installable");

export async function install(id: string): Promise<boolean> {
  installs[id] = 0;
  try {
    await invoke<string>("install_emulator", { id });
    installs[id] = "done";
    return true;
  } catch (e) {
    installs[id] = { error: String(e) };
    return false;
  }
}

export const GROUPS: { name: string; ids: string[] }[] = [
  { name: "Rétro (NES, SNES, N64, GB/GBA, Mega Drive, arcade…)", ids: ["retroarch"] },
  { name: "Sony", ids: ["duckstation", "pcsx2", "ppsspp", "rpcs3", "vita3k", "shadps4"] },
  { name: "Nintendo", ids: ["dolphin", "cemu", "melonds", "azahar", "eden", "eglantine"] },
  { name: "Sega", ids: ["flycast"] },
  { name: "Microsoft", ids: ["xemu", "xenia"] },
];

export const installLabel = (s: (typeof installs)[string] | undefined) =>
  s === undefined ? "" : s === "done" ? "Installé" : typeof s === "number" ? `${s} %` : "Erreur";
