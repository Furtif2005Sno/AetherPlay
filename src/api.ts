import { invoke } from "@tauri-apps/api/core";

export interface Settings {
  server_url: string;
  username: string;
  logged_in: boolean;
  emulator_paths: Record<string, string>;
  platform_emulators: Record<string, string>;
  save_dirs: Record<string, string>;
}
export interface CacheEntry { platform: string; name: string; path: string; bytes: number }
export interface Platform { id: number; slug: string; name: string; display_name?: string; rom_count: number }
export interface Rom {
  id: number;
  platform_slug: string;
  name?: string;
  fs_name: string;
  fs_size_bytes: number;
  path_cover_small?: string;
  multi: boolean;
}
export interface Emulator { id: string; name: string; platforms: string[]; experimental: boolean }

export const api = {
  settings: () => invoke<Settings>("get_settings"),
  login: (serverUrl: string, username: string, password: string) =>
    invoke<void>("login", { serverUrl, username, password }),
  platforms: () => invoke<Platform[]>("platforms"),
  roms: (platformId: number) => invoke<Rom[]>("roms", { platformId }),
  catalog: () => invoke<Emulator[]>("catalog"),
  setEmulatorPath: (id: string, path: string | null) => invoke<void>("set_emulator_path", { id, path }),
  setPlatformEmulator: (platform: string, id: string | null) => invoke<void>("set_platform_emulator", { platform, id }),
  setSaveDir: (id: string, path: string | null) => invoke<void>("set_save_dir", { id, path }),
  saveDirs: () => invoke<Record<string, string[]>>("save_dirs"),
  cacheList: () => invoke<CacheEntry[]>("cache_list"),
  cacheDelete: (path: string) => invoke<void>("cache_delete", { path }),
  cacheClear: () => invoke<void>("cache_clear"),
  play: (romId: number) => invoke<void>("play", { romId }),
};

/** URL served by the `romm://` protocol (Windows/Android use http://romm.localhost). */
const isWindows = navigator.userAgent.includes("Windows");
export const coverUrl = (path?: string) =>
  path ? `${isWindows ? "http://romm.localhost" : "romm://localhost"}/${path.replace(/^\//, "")}` : "";

export const formatSize = (b: number) =>
  b > 1 << 30 ? `${(b / (1 << 30)).toFixed(1)} Go` : `${Math.round(b / (1 << 20))} Mo`;
