// AetherPlay: calls into the desktop app (Tauri) from RomM's interface. Only
// meaningful inside AetherPlay; `isAetherPlay()` gates every entry point.

interface TauriGlobal {
  core: { invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> };
  event: {
    listen<T>(event: string, handler: (e: { payload: T }) => void): Promise<() => void>;
  };
}

function tauri(): TauriGlobal {
  return (window as unknown as { __TAURI__: TauriGlobal }).__TAURI__;
}

export const isAetherPlay = () =>
  typeof (window as unknown as { __TAURI__?: unknown }).__TAURI__ === "object";

export interface AppSettings {
  server_url: string;
  username: string;
  emulator_paths: Record<string, string>;
  platform_emulators: Record<string, string>;
  save_dirs: Record<string, string>;
  fullscreen: boolean;
  quit_combo: boolean;
}

export interface Emulator {
  id: string;
  name: string;
  platforms: string[];
  experimental: boolean;
  /** Present for yuzu-family emulators (Eden, Eglantine): save import works there. */
  config_ini?: Record<string, string>;
}

export interface ImportReport {
  title_id: string;
  destinations: string[];
  files: number;
}

export interface CacheEntry {
  platform: string;
  name: string;
  path: string;
  bytes: number;
}

const invoke = <T>(cmd: string, args?: Record<string, unknown>) => tauri().core.invoke<T>(cmd, args);

/** Android controller link (pairing code, connected phones = players). */
export interface PhoneStatus {
  pin: string;
  port: number;
  phones: { name: string; player: number }[];
  /** Wi-Fi input can reach the games (virtual controller available). */
  virtual: boolean;
  os: "windows" | "macos" | "linux";
}

export const app = {
  settings: () => invoke<AppSettings>("get_settings"),
  catalog: () => invoke<Emulator[]>("catalog"),
  installable: () => invoke<string[]>("installable"),
  install: (id: string) => invoke<string>("install_emulator", { id }),
  setEmulatorPath: (id: string, path: string | null) => invoke<void>("set_emulator_path", { id, path }),
  setPlatformEmulator: (platform: string, id: string | null) =>
    invoke<void>("set_platform_emulator", { platform, id }),
  setSaveDir: (id: string, path: string | null) => invoke<void>("set_save_dir", { id, path }),
  saveDirs: () => invoke<Record<string, string[]>>("save_dirs"),
  setOption: (name: "fullscreen" | "quit_combo", value: boolean) => invoke<void>("set_option", { name, value }),
  pickPath: (directory: boolean, title: string) => invoke<string | null>("pick_path", { directory, title }),
  cacheList: () => invoke<CacheEntry[]>("cache_list"),
  cacheDelete: (path: string) => invoke<void>("cache_delete", { path }),
  cacheClear: () => invoke<void>("cache_clear"),
  openAppSettings: () => invoke<void>("open_settings"),
  importSave: (id: string, path: string) => invoke<ImportReport>("import_save", { id, path }),
  openLog: () => invoke<void>("open_log"),
  phoneStatus: () => invoke<PhoneStatus>("phone_status"),
  phoneForget: () => invoke<void>("phone_forget"),
  installVigem: () => invoke<void>("install_vigem"),
  onInstallProgress: (handler: (p: { id: string; done: number; total: number }) => void) =>
    tauri().event.listen<{ id: string; done: number; total: number }>("install-progress", (e) => handler(e.payload)),
};

export const formatSize = (b: number) =>
  b >= 1 << 30 ? `${(b / (1 << 30)).toFixed(1)} Go` : `${Math.max(1, Math.round(b / (1 << 20)))} Mo`;
