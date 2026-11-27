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
    // The real list, for the AetherPlay settings (detected controllers).
    window.__aetherplayNativeGamepads = () => Array.from(native?.() ?? []).filter(Boolean);
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

  // File uploads (saves, states, covers…): webviews pass the romm:// handler
  // an empty body when the request carries files, so RomM stored 0-byte
  // files. Such requests to the API go through AetherPlay instead
  // (proxy_upload), with the multipart body encoded here. Same answer as before.
  {
    const isUpload = (url, body) => {
      if (!(body instanceof FormData || body instanceof Blob)) return null;
      const u = new URL(url, location.href);
      return u.origin === location.origin && u.pathname.startsWith("/api/") ? u : null;
    };
    const relay = async (method, u, body, headers) => {
      const encoded = new Request(u.href, { method, body });
      const bytes = new Uint8Array(await encoded.arrayBuffer());
      const res = await tauri().core.invoke("proxy_upload", bytes, {
        headers: {
          "x-aep-method": method,
          "x-aep-url": u.pathname + u.search,
          "x-aep-content-type": encoded.headers.get("content-type") || "",
          "x-aep-headers": JSON.stringify(headers || {}),
        },
      });
      const raw = atob(res.body);
      const out = new Uint8Array(raw.length);
      for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
      return { status: res.status, headers: res.headers, body: out, size: bytes.length };
    };

    const XHR = window.XMLHttpRequest.prototype;
    const open = XHR.open;
    const setHeader = XHR.setRequestHeader;
    const send = XHR.send;
    XHR.open = function (method, url, ...rest) {
      this.__aep = { method: String(method).toUpperCase(), url: String(url), headers: {} };
      return open.call(this, method, url, ...rest);
    };
    XHR.setRequestHeader = function (name, value) {
      if (this.__aep) this.__aep.headers[name] = value;
      return setHeader.call(this, name, value);
    };
    XHR.send = function (body) {
      const info = this.__aep;
      const u = info && isUpload(info.url, body);
      if (!u) return send.call(this, body);
      const xhr = this;
      const set = (name, value) => Object.defineProperty(xhr, name, { value, configurable: true });
      const fire = (target, type, extra) => {
        const e = new ProgressEvent(type, extra);
        target.dispatchEvent(e);
      };
      relay(info.method, u, body, info.headers).then(
        (res) => {
          const text = new TextDecoder().decode(res.body);
          let response = text;
          if (xhr.responseType === "json") {
            try { response = JSON.parse(text); } catch { response = null; }
          } else if (xhr.responseType === "arraybuffer") {
            response = res.body.buffer;
          } else if (xhr.responseType === "blob") {
            response = new Blob([res.body]);
          }
          const lines = Object.entries(res.headers).map(([k, v]) => `${k}: ${v}`).join("\r\n");
          set("readyState", 4);
          set("status", res.status);
          set("statusText", res.status < 400 ? "OK" : "Error");
          set("responseURL", u.href);
          set("response", response);
          if (!xhr.responseType || xhr.responseType === "text") set("responseText", text);
          set("getAllResponseHeaders", () => lines);
          set("getResponseHeader", (n) => res.headers[String(n).toLowerCase()] ?? null);
          fire(xhr.upload, "progress", { lengthComputable: true, loaded: res.size, total: res.size });
          fire(xhr.upload, "load", { lengthComputable: true, loaded: res.size, total: res.size });
          xhr.dispatchEvent(new Event("readystatechange"));
          fire(xhr, "load", {});
          fire(xhr, "loadend", {});
        },
        (error) => {
          console.error("[AetherPlay] envoi impossible :", error);
          set("readyState", 4);
          set("status", 0);
          xhr.dispatchEvent(new Event("readystatechange"));
          fire(xhr, "error", {});
          fire(xhr, "loadend", {});
        },
      );
    };

    const fetchNative = window.fetch.bind(window);
    window.fetch = async (input, init = {}) => {
      const url = typeof input === "string" || input instanceof URL ? String(input) : input.url;
      const u = isUpload(url, init.body);
      if (!u) return fetchNative(input, init);
      const res = await relay(String(init.method || "POST").toUpperCase(), u, init.body, Object.fromEntries(new Headers(init.headers || {})));
      const empty = [101, 204, 205, 304].includes(res.status);
      return new Response(empty ? null : res.body, { status: res.status, headers: res.headers });
    };
  }
  const os = navigator.userAgent.includes("Mac")
    ? "darwin"
    : navigator.userAgent.includes("Windows")
      ? "win32"
      : "linux";

  window.rommNative = {
    shellVersion: "AetherPlay 0.4.1",
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
