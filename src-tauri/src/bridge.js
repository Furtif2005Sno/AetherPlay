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
  // Update pop-up: AetherPlay found a newer version (updater.rs). Nothing is
  // installed until "Installer et redémarrer" is chosen; "Plus tard" waits
  // for the next check. Works with mouse, keyboard and controller (A clicks
  // the focused button, B / Escape means later, left / right switch).
  {
    const style = `
      #aep-update{position:fixed;inset:0;z-index:2147483000;display:flex;align-items:center;justify-content:center;
        background:var(--r-color-overlay-scrim-strong,rgba(0,0,0,.6));font-family:var(--r-font-family-sans,system-ui,sans-serif)}
      #aep-update .box{width:min(440px,90vw);padding:24px;border-radius:var(--r-radius-card,16px);
        background:var(--r-color-bg-elevated,#17141f);color:var(--r-color-fg,#fff);border:1px solid var(--r-color-border,#2b2735);
        box-shadow:0 20px 60px rgba(0,0,0,.5)}
      #aep-update h2{margin:0 0 8px;font-size:20px;color:var(--r-color-fg-heading,#fff)}
      #aep-update p{margin:0 0 16px;color:var(--r-color-fg-secondary,#cfc9dc);line-height:1.45}
      #aep-update .bar{height:6px;border-radius:999px;background:var(--r-color-surface,#2b2735);overflow:hidden;margin:0 0 16px;display:none}
      #aep-update .bar i{display:block;height:100%;width:0;background:var(--r-color-brand-primary,#8b5cf6);transition:width .2s}
      #aep-update .row{display:flex;gap:10px;justify-content:flex-end}
      #aep-update button{font:inherit;font-weight:600;padding:9px 16px;border-radius:var(--r-radius-pill,999px);cursor:pointer;
        border:1px solid var(--r-color-border-strong,#3a3546);background:transparent;color:var(--r-color-fg,#fff)}
      #aep-update button.primary{background:var(--r-color-brand-primary,#8b5cf6);border-color:transparent;color:#fff}
      #aep-update button:focus-visible,#aep-update button:focus{outline:3px solid var(--r-color-brand-primary,#8b5cf6);outline-offset:2px}
      #aep-update button:disabled{opacity:.5;cursor:default}`;
    let shown = false;
    const show = (offer) => {
      if (shown || !offer) return;
      shown = true;
      const root = document.createElement("div");
      root.id = "aep-update";
      root.innerHTML = `<style>${style}</style><div class="box" role="dialog" aria-modal="true" aria-labelledby="aep-update-title">
        <h2 id="aep-update-title">Mise à jour disponible</h2>
        <p>AetherPlay ${offer.version} est disponible (version actuelle : ${offer.current}). L'installation prend une minute, puis AetherPlay redémarre.</p>
        <div class="bar"><i></i></div>
        <div class="row"><button class="later">Plus tard</button><button class="primary install">Installer et redémarrer</button></div></div>`;
      document.body.appendChild(root);
      const later = root.querySelector(".later");
      const install = root.querySelector(".install");
      const text = root.querySelector("p");
      const bar = root.querySelector(".bar");
      const close = () => { root.remove(); shown = false; };
      later.onclick = () => { tauri().core.invoke("update_later"); close(); };
      install.onclick = () => {
        later.disabled = install.disabled = true;
        text.textContent = "Téléchargement de la mise à jour…";
        bar.style.display = "block";
        tauri().core.invoke("update_install").catch((e) => {
          text.textContent = "La mise à jour a échoué : " + e;
          later.disabled = false;
          later.textContent = "Fermer";
        });
      };
      // Controller / keyboard: keep focus inside, left/right switch, B or Escape = later.
      root.addEventListener("keydown", (e) => {
        if (e.key === "Escape" && !later.disabled) later.click();
        else if (e.key === "ArrowLeft") later.focus();
        else if (e.key === "ArrowRight") install.focus();
        else if (e.key !== "Enter" && e.key !== " ") return;
        else return;
        e.preventDefault();
        e.stopPropagation();
      }, true);
      install.focus();
    };
    const start = () => {
      const t = tauri();
      t.event.listen("aep-update", ({ payload }) => show(payload));
      t.event.listen("aep-update-progress", ({ payload }) => {
        const fill = document.querySelector("#aep-update .bar i");
        if (fill && payload.total) fill.style.width = `${Math.round((payload.done / payload.total) * 100)}%`;
      });
      // Found before this page loaded (start-up, reload).
      t.core.invoke("update_status").then(show).catch(() => {});
    };
    if (document.readyState === "loading") window.addEventListener("DOMContentLoaded", start, { once: true });
    else start();
  }
  const os = navigator.userAgent.includes("Mac")
    ? "darwin"
    : navigator.userAgent.includes("Windows")
      ? "win32"
      : "linux";

  window.rommNative = {
    shellVersion: "AetherPlay 0.4.6",
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
