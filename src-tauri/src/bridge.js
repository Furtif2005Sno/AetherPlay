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

  // Controllers the webview does not see itself, read by AetherPlay and sent
  // as "aep-gamepad", served through navigator.getGamepads() so RomM's
  // controller navigation works unchanged:
  // * macOS / Linux: every controller (their webviews are unreliable);
  // * Windows: WebView2 sees real controllers, so only the Android one when
  //   it has neither Bluetooth nor a virtual pad, added next to them.
  {
    const windows = navigator.userAgent.includes("Windows");
    const native = navigator.getGamepads?.bind(navigator);
    let pad = null;
    const toGamepad = (s, index) => ({
      id: s.id || "AetherPlay",
      index,
      connected: true,
      mapping: "standard",
      timestamp: performance.now(),
      buttons: s.buttons.map((p) => ({ pressed: p, touched: p, value: p ? 1 : 0 })),
      axes: s.axes,
      vibrationActuator: null,
    });
    navigator.getGamepads = () => {
      if (!windows) return [pad, null, null, null];
      const list = Array.from(native?.() ?? []);
      if (pad) list[list.length < 4 ? 3 : list.length] = pad;
      return list;
    };
    const start = () =>
      tauri().event.listen("aep-gamepad", ({ payload }) => {
        const wasConnected = !!pad;
        pad = payload.connected ? toGamepad(payload, windows ? 3 : 0) : null;
        if (pad && !wasConnected) {
          const event = new Event("gamepadconnected");
          event.gamepad = pad;
          window.dispatchEvent(event);
        }
      });
    if (tauri()) start();
    else window.addEventListener("DOMContentLoaded", start, { once: true });
  }

  const os = navigator.userAgent.includes("Mac")
    ? "darwin"
    : navigator.userAgent.includes("Windows")
      ? "win32"
      : "linux";

  window.rommNative = {
    shellVersion: "AetherPlay 0.2.2",
    os,
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
