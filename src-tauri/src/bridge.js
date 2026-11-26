// Injected into every page of the main window. On the bundled RomM interface it
// defines `window.rommNative`, RomM's desktop-shell contract
// (frontend/src/types/rommNative.d.ts), backed by AetherPlay's Rust commands.
(() => {
  if (!/^(http:\/\/romm\.localhost|romm:\/\/localhost)/.test(location.href)) return;

  const tauri = () => window.__TAURI__;
  const listeners = new Set();
  let subscribed = false;

  function subscribe() {
    if (subscribed) return;
    subscribed = true;
    tauri().event.listen("launch-state", ({ payload }) => {
      for (const listener of listeners) {
        try {
          listener(payload);
        } catch (error) {
          console.error("[AetherPlay] launch-state listener failed:", error);
        }
      }
    });
  }

  window.rommNative = {
    shellVersion: "AetherPlay 0.1.4",
    os: navigator.userAgent.includes("Mac") ? "darwin" : "win32",
    capabilities: ["launch-stage", "platform-support-all", "save-sync", "multi-disc", "launch-fullscreen"],
    launch: (request) => tauri().core.invoke("native_launch", { request }),
    cancel: (romId) => tauri().core.invoke("native_cancel", { romId }),
    getPlatformSupport: (query) =>
      tauri()
        .core.invoke("native_platform_support", { queries: [query] })
        .then((answers) => answers[query.platformSlug] ?? { supported: false, reason: "unsupported-platform" }),
    getPlatformSupportAll: (queries) => tauri().core.invoke("native_platform_support", { queries }),
    onLaunchState(listener) {
      subscribe();
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    openSettings: () => tauri().core.invoke("open_settings"),
  };
})();
