// The desktop app's side of the web app. In a browser this file does nothing at all. Inside the
// Tauri shell it saves notes to a real file, connects native menus and dialogs, and adds Local
// models to Settings. This is the only file that reads the Tauri global (docs/44).
//
// The shell is the only thing that can reach a model. The assistant's own fetch goes through
// `transport` below: this page never learns the local server's port or key.
import { I18N } from "../data.js";

const SAVE_DELAY_MS = 800;
const LOCAL_ENDPOINT = "https://this-computer.invalid/v1";
const OLLAMA_ENDPOINT = "http://127.0.0.1:11434/v1";
const MENU_LABELS = {
  menu_file: "File", menu_edit: "Edit", menu_view: "View", menu_window: "Window", menu_help: "Help",
  new_page: "New page", open_package: "Import a package", save_package: "Export workspace", undo: "Undo", redo: "Redo", select_all: "Select all",
  toggle_sidebar: "Hide sidebar", assistant: "Assistant", search: "Search", local_models: "Local models", settings: "Settings", shortcuts: "Keyboard shortcuts",
};

/** The shell, or null in a browser. Everything below takes this object, so tests can supply a fake. */
export function bridge() {
  const tauri = window.__TAURI__;
  if (!tauri?.core?.invoke) return null;
  return {
    invoke: (cmd, args, options) => tauri.core.invoke(cmd, args, options),
    listen: (name, fn) => tauri.event.listen(name, e => fn(e.payload)),
    channel: onMessage => { const c = new tauri.core.Channel(); c.onmessage = onMessage; return c; },
  };
}

/* ---------- The assistant's transport ---------- */

const failedResponse = message => new Response(JSON.stringify({ error: { message } }), { status: 503, headers: { "content-type": "application/json" } });

/** A fetch for the assistant: any POST to another origin goes through Rust, as a stream. */
export function transport(host, t, nativeFetch) {
  let next = 0;
  return (input, init = {}) => {
    const raw = String(input?.url || input);
    let url;
    try { url = new URL(raw, location.href); } catch { return nativeFetch(input, init); }
    const outside = /^https?:$/.test(url.protocol) && url.origin !== location.origin;
    if (!outside || String(init.method || "GET").toUpperCase() !== "POST") return nativeFetch(input, init);
    const id = `ai-${++next}`;
    return new Promise((resolve, reject) => {
      if (init.signal?.aborted) return reject(new DOMException("stopped", "AbortError"));
      let sink = null, settled = false;
      const body = new ReadableStream({ start(c) { sink = c; }, cancel() { host.invoke("ai_cancel", { id }); } });
      const channel = host.channel(msg => {
        if (msg instanceof ArrayBuffer) { try { sink.enqueue(new Uint8Array(msg)); } catch { /* reader gone */ } return; }
        if (msg?.done) { try { sink.close(); } catch { /* reader gone */ } return; }
        settled = true;
        const headers = new Headers({ "content-type": msg.contentType || "" });
        if (msg.retryAfter) headers.set("retry-after", msg.retryAfter);
        // A status with no body is a real case here, and Response refuses a stream for some.
        resolve(new Response(msg.status === 204 ? null : body, { status: msg.status, headers }));
      });
      init.signal?.addEventListener("abort", () => {
        host.invoke("ai_cancel", { id });
        const stopped = new DOMException("stopped", "AbortError");
        if (settled) { try { sink.error(stopped); } catch { /* reader gone */ } } else { settled = true; reject(stopped); }
      });
      const headers = Object.entries(init.headers || {}).map(([k, v]) => [k, String(v)]);
      host.invoke("ai_request", { request: { id, url: raw, headers, body: String(init.body ?? "") }, channel }).catch(err => {
        const note = t(AI_FAILURES[err?.code] || "This request could not be sent. Your notes are fine.");
        if (!settled) { settled = true; resolve(failedResponse(err?.detail && !AI_FAILURES[err?.code] ? `${note} ${err.detail}` : note)); return; }
        try { sink.error(new TypeError(note)); } catch { /* reader gone */ }
      });
    });
  };
}
const AI_FAILURES = {
  "CN-MDL-0014": "The local model is not running. Open Settings, go to Local models, and press Start.",
  "CN-MDL-0010": "The local model's port did not check out, so nothing was sent. Press Start in Local models to try again.",
  "CN-DSK-0017": "You declined this address, so nothing was sent.",
  "CN-DSK-0018": "This address was refused, so nothing was sent.",
};

/* ---------- Plain words for what went wrong, and what to do ---------- */

const ERRORS = {
  "CN-MDL-0001": "There is not enough free disk space for this model. Free up {need}, then try again.",
  "CN-MDL-0002": "This file did not match its pinned hash, so it was deleted and nothing ran. Download it again.",
  "CN-MDL-0003": "The download did not finish. What arrived is kept where possible. Press Download to try again.",
  "CN-MDL-0004": "This file is not a GGUF model, so it was deleted. Download it again.",
  "CN-MDL-0006": "The local model could not start. Try a smaller model, or check that llama-server works in a terminal.",
  "CN-MDL-0007": "The local model stopped unexpectedly. It does not restart by itself. Press Start to try again.",
  "CN-MDL-0008": "The server accepted a request it should refuse, so it was stopped. Update llama-server, then press Start.",
  "CN-MDL-0009": "Ollama did not answer at 127.0.0.1:11434. Start Ollama, then press Detect again.",
  "CN-MDL-0010": "Another program answered on the model's port. Nothing was sent. Press Start to try on a new port.",
  "CN-MDL-0011": "This model cannot be downloaded because its hash pin is missing. An app update has to add it.",
  "CN-MDL-0012": "No llama-server was found. Install one, or choose it below.",
  "CN-MDL-0013": "Another download is running. Pause or cancel it first.",
  "CN-DSK-0011": "The local model has failed three times in 10 minutes. Try a smaller model.",
  "CN-DSK-0015": "That file could not be opened. Choose a regular .cnotepack file.",
  "CN-DSK-0017": "Nothing changed, because this was declined. Ask again in a minute, or restart the app after three declines.",
  "CN-DSK-0020": "Your notes file could not be written. Check the disk has space, or choose another location in Settings.",
};

const bytesText = (n, lang) => {
  const units = [[1e9, "GB"], [1e6, "MB"], [1e3, "KB"]];
  for (const [size, unit] of units) if (n >= size) return `${new Intl.NumberFormat(lang, { maximumFractionDigits: size === 1e9 ? 2 : 0 }).format(n / size)} ${unit}`;
  return `${new Intl.NumberFormat(lang).format(n)} B`;
};

/* ---------- Local models, in Settings ---------- */

/** Draws the Local models and Notes file sections into `box`. Returns the event handlers. */
export function panel(api, host, box) {
  const { el, t, icon } = api;
  const say = (key, vars = {}) => t(key).replace(/\{(\w+)\}/g, (_, k) => vars[k] ?? "");
  const size = n => bytesText(n, api.lang);
  const rows = new Map();
  let view = null, runtime = null;
  const live = new Map(); // id -> the latest report, from downloads and from starts

  const errorBox = (e, extra) => {
    const vars = { need: e?.need || "" };
    const line = ERRORS[e?.code] ? say(ERRORS[e.code], vars) : e?.detail || String(e || "");
    return el("div", { class: "dsk-error", role: "alert", dir: "auto" },
      el("div", { class: "dsk-error-line" }, icon("warning"), el("span", { text: line })),
      e?.detail && ERRORS[e?.code] && e.code !== "CN-MDL-0001" ? el("small", { class: "dsk-detail", dir: "auto", text: e.detail }) : null,
      e?.code ? el("small", { class: "dsk-code", text: e.code }) : null, extra || null);
  };
  const asError = (err, fallback) => (err && err.code ? err : { code: fallback, detail: String(err?.message || err || "") });

  /* The runtime: exactly what to do when there is none. */
  const runtimeBox = el("div", { class: "dsk-runtime", id: "dskRuntime" });
  const ollamaOut = el("div", { class: "dsk-ollama", id: "dskOllama" });
  function paintRuntime() {
    runtimeBox.replaceChildren();
    if (runtime?.found) {
      runtimeBox.append(
        el("p", { class: "dsk-line", id: "dskFound", dir: "auto", text: say(runtime.source === "chosen" ? "Using the llama-server you chose: {path}" : "Found llama-server: {path}", { path: runtime.path }) }));
    } else {
      runtimeBox.append(
        el("p", { class: "dsk-line", id: "dskNoRuntime", role: "status" }, el("b", { text: t("No llama-server was found.") }), " ", t("This app never downloads a program for you. To run a model on this computer, do one of these, then press Find again:")),
        el("ol", { class: "dsk-steps" },
          el("li", {}, t("On a Mac with Homebrew, run this in Terminal:"), " ", el("code", { dir: "ltr", text: "brew install llama.cpp" })),
          el("li", {}, t("Or get a release of llama.cpp from its GitHub page, and press Choose program to pick its llama-server file.")),
          el("li", {}, t("Or start Ollama if you have it, and press Detect Ollama."))));
    }
    runtimeBox.append(el("div", { class: "dsk-actions" },
      el("button", { class: "btn", id: "dskFind", text: t("Find again"), onclick: refresh }),
      el("button", { class: "btn", id: "dskChoose", text: t("Choose program"), onclick: async () => {
        try { runtime = await host.invoke("runtime_choose"); paintRuntime(); paintRows(); }
        catch (err) { runtimeBox.append(errorBox(asError(err))); }
      } }),
      el("button", { class: "btn", id: "dskDetect", text: t("Detect Ollama"), onclick: detectOllama })));
  }
  async function detectOllama() {
    ollamaOut.replaceChildren(el("p", { class: "dsk-line", role: "status", text: t("Checking for Ollama.") }));
    try {
      const models = await host.invoke("ollama_detect");
      if (!models.length) { ollamaOut.replaceChildren(el("p", { class: "dsk-line", text: t("Ollama is running, but has no models. Run ollama pull in a terminal, then press Detect again.") })); return; }
      ollamaOut.replaceChildren(el("p", { class: "dsk-line", text: t("Ollama is running. Its port and its security are its own, not this app's.") }),
        ...models.map(name => el("div", { class: "dsk-actions" }, el("bdi", { class: "dsk-name", text: name }),
          el("button", { class: "btn", "data-ollama": name, text: t("Use in assistant"), onclick: async () => {
            try {
              await host.invoke("provider_bind", { url: OLLAMA_ENDPOINT });
              pointAssistant(OLLAMA_ENDPOINT, name);
            } catch (err) { ollamaOut.append(errorBox(asError(err))); }
          } }))));
    } catch (err) { ollamaOut.replaceChildren(errorBox(asError(err, "CN-MDL-0009"))); }
  }

  /* One click to the assistant's OpenAI-compatible adapter, through its own settings form. */
  function pointAssistant(endpoint, model) {
    const provider = api.$("#aiProvider"), address = api.$("#aiEndpoint"), name = api.$("#aiModel");
    if (!provider || !address || !name) { api.toast(t("Open Settings to set up the assistant.")); return false; }
    provider.value = "local"; provider.dispatchEvent(new Event("change"));
    address.value = endpoint; address.dispatchEvent(new Event("input"));
    name.value = model; name.dispatchEvent(new Event("input"));
    api.toast(t("The assistant now uses this model. Nothing leaves this computer for it."));
    paintRows();
    return true;
  }

  /* One row per model. Size and licence are on the row before anything starts. */
  function kindOf(model) {
    const report = live.get(model.id);
    if (!model.pinned) return "nopin";
    if (view.running === model.id && report?.phase !== "stopped") return "running";
    if (report) {
      if (["connecting", "downloading", "retrying"].includes(report.phase)) return "downloading";
      if (report.phase === "verifying") return "verifying";
      if (report.phase === "checking" || report.phase === "starting") return report.phase;
      if (report.phase === "paused") return "paused";
    }
    if (model.installed) return "installed";
    if (model.part_bytes > 0) return "paused";
    return "absent";
  }
  function stateLine(model, kind) {
    const r = live.get(model.id) || {};
    const done = r.done || model.part_bytes || 0, total = model.bytes;
    switch (kind) {
      case "nopin": return say("This model cannot be downloaded because its hash pin is missing.");
      case "absent": return "";
      case "paused": return say("Paused at {done} of {total}. Nothing is lost.", { done: size(done), total: size(total) });
      case "downloading": {
        if (r.phase === "connecting") return say("Connecting to {host}.", { host: model.source });
        if (r.phase === "retrying") return say("The connection dropped. Trying again from {done}.", { done: size(done) });
        const parts = [say("{done} of {total}", { done: size(done), total: size(total) })];
        if (r.rate > 0) {
          parts.push(say("{rate} a second", { rate: size(r.rate) }));
          parts.push(say("about {min} min left", { min: new Intl.NumberFormat(api.lang).format(Math.max(1, Math.ceil((total - done) / r.rate / 60))) }));
        }
        return parts.join(", ") + (r.restarted ? ". " + t("The server would not continue, so this restarted from the beginning.") : ".");
      }
      case "verifying": return say("Checking the whole file against its pinned SHA-256: {done} of {total}.", { done: size(r.done || 0), total: size(total) });
      case "checking": return say("Checking the whole file before starting: {done} of {total}. Large files take a while.", { done: size(r.done || 0), total: size(total) });
      case "starting": return t("Starting. A large model can take a few minutes to load. Nothing has been sent to it yet.");
      case "running": return usingThis(model) ? t("Running on this computer. The assistant is using it.") : t("Running on this computer.");
      case "installed": return t("On this computer. Its hash is checked again before each start.");
      default: return "";
    }
  }
  const usingThis = model => api.$("#aiEndpoint")?.value === LOCAL_ENDPOINT && api.$("#aiModel")?.value === model.id;
  function progressOf(model, kind) {
    const r = live.get(model.id) || {};
    if (kind === "downloading" || kind === "paused") return (r.done || model.part_bytes || 0) / model.bytes;
    if ((kind === "verifying" || kind === "checking") && r.total) return (r.done || 0) / r.total;
    return null;
  }
  const act = (work, row) => async () => {
    row.error = null;
    try { await work(); }
    catch (err) { row.error = asError(err); }
    await refresh();
  };
  function buttons(model, kind, row) {
    const b = (id, label, work, primary) => el("button", { class: "btn" + (primary ? " primary" : ""), "data-act": id, "data-model": model.id, text: t(label), onclick: act(work, row) });
    const startOthers = !!view.models.find(m => m.active && m.id !== model.id);
    switch (kind) {
      case "absent": return [
        el("button", { class: "btn", "data-act": "ask", "data-model": model.id, text: t("Download"), disabled: startOthers, onclick: () => { row.asking = !row.asking; paintRow(model, true); } }),
        b("addfile", "Add from file", () => host.invoke("model_add_file", { id: model.id })),
      ];
      case "paused": return [b("resume", "Resume", () => host.invoke("download_start", { id: model.id }), true), b("cancel", "Discard", () => host.invoke("download_cancel", { id: model.id }))];
      case "downloading": return [b("pause", "Pause", () => host.invoke("download_pause", { id: model.id })), b("cancel", "Cancel", () => host.invoke("download_cancel", { id: model.id }))];
      case "verifying": return [b("cancel", "Cancel", () => host.invoke("download_cancel", { id: model.id }))];
      case "installed": return [
        el("button", { class: "btn primary", "data-act": "start", "data-model": model.id, text: t("Start"), disabled: !runtime?.found, onclick: act(() => host.invoke("runtime_start", { id: model.id }), row) }),
        b("delete", "Delete", () => host.invoke("model_delete", { id: model.id })),
      ];
      case "running": return [
        el("button", { class: "btn primary", "data-act": "use", "data-model": model.id, text: t("Use in assistant"), onclick: () => pointAssistant(LOCAL_ENDPOINT, model.id) }),
        b("stop", "Stop", () => host.invoke("runtime_stop")),
      ];
      default: return [];
    }
  }
  function confirmBlock(model, row) {
    const free = view.free_bytes, need = model.bytes + view.margin_bytes, short = free != null && free < need;
    return el("div", { class: "dsk-ask", role: "group", "aria-label": say("Download {name}", { name: model.name }) },
      el("dl", { class: "dsk-facts" },
        el("div", {}, el("dt", { text: t("Download size") }), el("dd", { text: size(model.bytes) })),
        el("div", {}, el("dt", { text: t("Licence") }), el("dd", { dir: "ltr", text: model.licence })),
        el("div", {}, el("dt", { text: t("Comes from") }), el("dd", { dir: "ltr", text: model.source })),
        el("div", {}, el("dt", { text: t("Free disk") }), el("dd", { text: free == null ? t("Not known") : size(free) })),
        el("div", {}, el("dt", { text: t("Needs free") }), el("dd", { text: size(need) }))),
      short ? errorBox({ code: "CN-MDL-0001", need: size(need - free) }) : null,
      el("p", { class: "dsk-line", text: t("Memory needs and speed have not been measured for this model, and it has not been tried with this app.") }),
      el("div", { class: "dsk-actions" },
        el("button", { class: "btn primary", "data-act": "download", "data-model": model.id, text: say("Download {size}", { size: size(model.bytes) }), disabled: short, onclick: act(async () => { row.asking = false; live.set(model.id, { phase: "connecting", done: model.part_bytes }); await host.invoke("download_start", { id: model.id }); }, row) }),
        el("button", { class: "btn", "data-act": "notnow", "data-model": model.id, text: t("Not now"), onclick: () => { row.asking = false; paintRow(model, true); } })));
  }
  const list = el("div", { class: "dsk-models", id: "dskModels", role: "list" });
  function paintRow(model, force) {
    let row = rows.get(model.id);
    if (!row) {
      const name = el("div", { class: "dsk-title" }, el("b", { text: model.name }), el("span", { class: "dsk-position", text: t(model.position) }));
      const facts = el("div", { class: "dsk-size", dir: "auto", text: `${size(model.bytes)}, ${model.licence}` });
      const line = el("p", { class: "dsk-line dsk-state" });
      const bar = el("div", { class: "dsk-bar", role: "progressbar", "aria-valuemin": "0", "aria-valuemax": "100", hidden: true }, el("span", {}));
      const actions = el("div", { class: "dsk-actions" });
      const extra = el("div", {});
      const node = el("div", { class: "dsk-model", role: "listitem", "data-id": model.id }, name, facts, line, bar, actions, extra);
      row = { node, line, bar, actions, extra, kind: null, asking: false, error: null, errorShown: null };
      rows.set(model.id, row);
      list.append(node);
    }
    const kind = kindOf(model);
    // Only text and the bar change while a download runs, so buttons keep focus.
    row.line.textContent = stateLine(model, kind);
    row.line.hidden = !row.line.textContent;
    const p = progressOf(model, kind);
    row.bar.hidden = p == null;
    if (p != null) {
      row.bar.firstChild.style.transform = `scaleX(${Math.min(1, Math.max(0, p))})`;
      row.bar.setAttribute("aria-valuenow", String(Math.round(p * 100)));
      row.bar.setAttribute("aria-label", row.line.textContent);
    }
    if (force || row.kind !== kind || row.errorShown !== row.error) {
      row.kind = kind; row.errorShown = row.error;
      if (kind !== "absent") row.asking = false;
      row.actions.replaceChildren(...buttons(model, kind, row));
      row.line.setAttribute("role", "status");
      row.extra.replaceChildren(...[row.asking ? confirmBlock(model, row) : null, row.error ? errorBox(row.error) : null].filter(Boolean));
    }
  }
  function paintRows() { if (view) for (const model of view.models) paintRow(model); }

  async function refresh() {
    try {
      [view, runtime] = await Promise.all([host.invoke("catalogue_status"), host.invoke("runtime_status")]);
      paintRuntime();
      paintRows();
    } catch (err) { console.warn("local models", err); list.replaceChildren(errorBox(asError(err))); }
  }

  const notesLine = el("p", { class: "dsk-line", id: "dskNotes", dir: "auto" });
  const notesError = el("div", {});
  async function paintNotes() {
    try {
      const info = await host.invoke("notes_info");
      notesLine.textContent = info.exists ? say("Saved to {path}", { path: info.path }) : say("Your notes will be saved to {path}", { path: info.path });
    } catch (err) { notesError.replaceChildren(errorBox(asError(err))); }
  }

  box.append(
    el("h2", { text: t("Notes file") }),
    el("p", { text: t("Your notes are written to one file on this computer, so that a crash leaves the last complete save in place.") }),
    notesLine, notesError,
    el("div", { class: "dsk-actions" }, el("button", { class: "btn", id: "dskMove", text: t("Move notes file"), onclick: async () => {
      notesError.replaceChildren();
      try { await host.invoke("notes_move"); await paintNotes(); }
      catch (err) { notesError.replaceChildren(errorBox(asError(err))); }
    } })),
    el("h2", { id: "dskLocal", text: t("Local models") }),
    el("p", { text: t("Run an AI model on this computer, so what you ask stays here. Nothing downloads or starts until you press a button.") }),
    runtimeBox, ollamaOut, list,
    el("div", { class: "dsk-notsandbox", id: "dskNotSandboxed", role: "note" },
      el("div", { class: "dsk-error-line dsk-plain" }, icon("warning"), el("b", { text: t("Not sandboxed") })),
      el("p", { class: "dsk-line", text: t("llama-server runs with your rights. It can read your files and use the network, and so could a malicious program or model file. Choose a program you trust. Model hashes are checked, but the program itself is only checked against what you confirmed.") }),
      el("p", { class: "dsk-line", text: t("Its key is on its command line, so other programs you run could read it. If this app crashes, the server keeps running until the app next opens and stops it.") })));

  paintRuntime();
  refresh();
  paintNotes();

  return {
    onDownload(report) {
      if (["installed", "cancelled", "failed"].includes(report.phase)) {
        live.delete(report.id);
        const row = rows.get(report.id);
        if (row) row.error = report.phase === "failed" ? errorFromNeeds(report) : null;
        refresh();
        return;
      }
      live.set(report.id, report);
      const model = view?.models.find(m => m.id === report.id);
      if (model) {
        if (report.phase === "paused") model.part_bytes = report.done;
        paintRow(model);
      }
    },
    onRuntime(event) {
      if (event.phase === "checking" || event.phase === "starting") live.set(event.id, event);
      else {
        live.delete(event.id);
        const row = rows.get(event.id);
        if (row) row.error = event.code ? { code: event.code, detail: event.detail } : null;
        if (!event.id && event.code) for (const r of rows.values()) if (r.kind === "running") r.error = { code: event.code, detail: event.detail };
      }
      const model = view?.models.find(m => m.id === event.id);
      if (model && (event.phase === "checking" || event.phase === "starting")) paintRow(model);
      else refresh();
    },
    refresh, isConnected: () => box.isConnected, pointAssistant, paintNotes,
  };

  function errorFromNeeds(report) {
    if (report.code !== "CN-MDL-0001") return { code: report.code, detail: report.detail };
    const m = /needs=(\d+) free=(\d+)/.exec(report.detail || "");
    return m ? { code: report.code, need: size(Number(m[1]) - Number(m[2])) } : { code: report.code, detail: report.detail };
  }
}

/* ---------- Installing into the real shell ---------- */

export function install(api) {
  const host = bridge();
  if (!host) return; // A browser: nothing changes, and nothing is sent.
  const { t, $ } = api;
  for (const [lang, table] of Object.entries(DRAFTS)) I18N[lang] = { ...table, ...I18N[lang] };

  const nativeFetch = window.fetch.bind(window);
  window.fetch = transport(host, t, nativeFetch);

  // Window title follows the page.
  const sendTitle = () => host.invoke("window_set_title", { title: document.title }).catch(() => {});
  new MutationObserver(sendTitle).observe(document.querySelector("title"), { childList: true, characterData: true, subtree: true });
  sendTitle();

  // Native menus speak the interface's language, and so do native confirmations.
  const sendMenu = () => {
    const labels = Object.fromEntries(Object.entries(MENU_LABELS).map(([id, key]) => [id, t(key)]));
    host.invoke("menu_set_labels", { lang: api.lang, labels }).catch(err => console.warn("menu", err));
  };
  new MutationObserver(sendMenu).observe(document.documentElement, { attributes: true, attributeFilter: ["lang"] });
  sendMenu();

  /* Notes, in a real file */
  let timer = null, saving = false, again = false, failed = false;
  const fileLost = () => {
    if (failed) return;
    failed = true;
    api.toast(t(ERRORS["CN-DSK-0020"]));
    let banner = $("#dskBanner");
    if (!banner) {
      banner = api.el("div", { id: "dskBanner", class: "dsk-banner", role: "alert" }, api.icon("warning"), api.el("span", { text: t(ERRORS["CN-DSK-0020"]) }));
      document.body.append(banner);
    }
  };
  const fileBack = () => { failed = false; $("#dskBanner")?.remove(); };
  async function saveNow() {
    clearTimeout(timer); timer = null;
    if (saving) { again = true; return savingDone; }
    saving = true;
    let finish;
    savingDone = new Promise(r => { finish = r; });
    let ok = true;
    try {
      do {
        again = false;
        await host.invoke("notes_save", api.engine.export(), { headers: { "content-type": "application/octet-stream" } });
      } while (again);
      fileBack();
    } catch (err) {
      console.warn("notes file", err);
      fileLost();
      ok = false;
    } finally {
      saving = false;
      finish(ok);
    }
    current?.paintNotes();
    return ok;
  }
  let savingDone = Promise.resolve(true);
  api.ext.onChange.push(() => { clearTimeout(timer); timer = setTimeout(saveNow, SAVE_DELAY_MS); });

  // The file and this app's own storage are both replicas, so opening merges them.
  async function loadFile() {
    const whenReady = () => new Promise(r => { const tick = () => $("#side[inert]") ? setTimeout(tick, 20) : r(); tick(); });
    let bytes;
    try { bytes = new Uint8Array(await host.invoke("notes_load")); }
    catch (err) { console.warn("notes file", err); fileLost(); return; }
    await whenReady();
    if (bytes.length) {
      try {
        api.engine.import(bytes);
        api.renderAll();
      } catch (err) {
        // Never let a save overwrite notes that could not be read.
        console.warn("notes file unreadable", err);
        const aside = await host.invoke("notes_set_aside").catch(() => "");
        api.toast(t("The notes file could not be read. It was kept unchanged, under a new name.") + (aside ? ` ${aside}` : ""));
      }
    }
    saveNow();
  }
  loadFile();

  host.listen("cn://close", async () => {
    let saved = true;
    try { await api.store.flush?.(); } catch { /* the file below is what is asked about */ }
    try { saved = await saveNow(); } catch { saved = false; }
    host.invoke("close_ready", { saved });
  });

  /* Files through native dialogs. package.js and app.js save through a download link and import
     through a file input, which a native window would otherwise drop or draw as a web panel. */
  document.addEventListener("click", async e => {
    const a = e.target.closest?.("a[download]");
    if (a && a.href.startsWith("blob:")) {
      e.preventDefault();
      try {
        const bytes = new Uint8Array(await (await nativeFetch(a.href)).arrayBuffer());
        const name = await host.invoke("file_save", bytes, { headers: { "content-type": "application/octet-stream", "x-file-name": encodeURIComponent(a.download) } });
        if (name) api.toast(`${t("Saved")}: ${name}`);
      } catch (err) { api.toast(t(ERRORS[err?.code] || "That file could not be saved.")); }
      return;
    }
    const input = e.target.closest?.("#pkgFile");
    if (input) { e.preventDefault(); openPackage(input); }
  }, true);
  async function openPackage(input) {
    try {
      const bytes = new Uint8Array(await host.invoke("package_open"));
      if (!bytes.length) return;
      const list = new DataTransfer();
      list.items.add(new File([bytes], "package.cnotepack"));
      input.files = list.files;
      input.dispatchEvent(new Event("change"));
    } catch (err) { api.toast(t(ERRORS[err?.code] || "That file could not be opened.")); }
  }

  /* Menu commands take the same path as the buttons and keys they stand for. */
  const editorKey = (key, extra = {}) => (document.activeElement || document.body).dispatchEvent(new KeyboardEvent("keydown", { key, code: `Key${key.toUpperCase()}`, bubbles: true, cancelable: true, ...api.ui.MOD, ...extra }));
  const openSettingsAt = id => { api.openView("settings"); requestAnimationFrame(() => $(id)?.scrollIntoView({ block: "start" })); };
  const withSettingsButton = id => { api.openView("settings"); requestAnimationFrame(() => $(id)?.click()); };
  const MENU = {
    new_page: () => $("#newBtn")?.click(),
    open_package: () => withSettingsButton("#pkgImport"),
    save_package: () => withSettingsButton("#pkgExport"),
    undo: () => editorKey("z"),
    redo: () => editorKey("z", { shiftKey: true }),
    select_all: () => editorKey("a"),
    toggle_sidebar: () => $("#collapseBtn")?.click(),
    assistant: () => $("#assistantBtn")?.click(),
    search: () => $("#searchBtn")?.click(),
    local_models: () => openSettingsAt("#dskLocal"),
    settings: () => $("#settingsBtn")?.click(),
    shortcuts: () => openSettingsAt("#shortcuts"),
  };
  host.listen("cn://menu", id => MENU[id]?.());

  /* Settings */
  let current = null;
  api.ext.settings.push(box => { current = panel(api, host, box); });
  host.listen("cn://download", report => { if (current?.isConnected()) current.onDownload(report); });
  host.listen("cn://runtime", event => { if (current?.isConnected()) current.onRuntime(event); });
}

// Drafts, not yet native-reviewed (I18N-05), kept here until data.js takes them.
const DRAFTS = {
  "zh-Hans": {
    "File": "文件", "Edit": "编辑", "View": "视图", "Window": "窗口", "Help": "帮助", "Select all": "全选", "Undo": "撤销", "Redo": "重做",
    "Local models": "本地模型", "Notes file": "笔记文件", "Download": "下载", "Add from file": "从文件添加", "Resume": "继续", "Discard": "丢弃", "Pause": "暂停", "Delete": "删除", "Start": "启动", "Use in assistant": "用于助手", "Not now": "暂不", "Find again": "重新查找", "Choose program": "选择程序", "Detect Ollama": "检测 Ollama", "Licence": "许可证", "Download size": "下载大小", "Comes from": "来源", "Free disk": "可用磁盘", "Needs free": "需要可用", "Not sandboxed": "没有沙盒", "Move notes file": "移动笔记文件",
    "No llama-server was found.": "未找到 llama-server。",
    "Run an AI model on this computer, so what you ask stays here. Nothing downloads or starts until you press a button.": "在这台电脑上运行 AI 模型，你的提问留在本机。在你按下按钮之前，不会下载或启动任何内容。",
    "This app never downloads a program for you. To run a model on this computer, do one of these, then press Find again:": "此应用不会替你下载程序。要在这台电脑上运行模型，请执行以下任一操作，然后按“重新查找”：",
    "llama-server runs with your rights. It can read your files and use the network, and so could a malicious program or model file. Choose a program you trust. Model hashes are checked, but the program itself is only checked against what you confirmed.": "llama-server 以你的权限运行。它可以读取你的文件并使用网络，恶意程序或模型文件也可以。请选择你信任的程序。模型哈希会被检查，而程序本身只与你确认过的版本比对。",
  },
  ja: {
    "File": "ファイル", "Edit": "編集", "View": "表示", "Window": "ウィンドウ", "Help": "ヘルプ", "Select all": "すべて選択", "Undo": "元に戻す", "Redo": "やり直す",
    "Local models": "ローカルモデル", "Notes file": "ノートのファイル", "Download": "ダウンロード", "Add from file": "ファイルから追加", "Resume": "再開", "Discard": "破棄", "Pause": "一時停止", "Delete": "削除", "Start": "開始", "Use in assistant": "アシスタントで使う", "Not now": "今はしない", "Find again": "再検索", "Choose program": "プログラムを選択", "Detect Ollama": "Ollama を検出", "Licence": "ライセンス", "Download size": "ダウンロードサイズ", "Comes from": "入手元", "Free disk": "ディスクの空き", "Needs free": "必要な空き", "Not sandboxed": "サンドボックスなし", "Move notes file": "ノートのファイルを移動",
    "No llama-server was found.": "llama-server が見つかりません。",
    "Run an AI model on this computer, so what you ask stays here. Nothing downloads or starts until you press a button.": "AI モデルをこのコンピューターで実行します。質問はここに留まります。ボタンを押すまで何もダウンロードも開始もされません。",
    "This app never downloads a program for you. To run a model on this computer, do one of these, then press Find again:": "このアプリがプログラムをダウンロードすることはありません。次のいずれかを行ってから「再検索」を押してください。",
    "llama-server runs with your rights. It can read your files and use the network, and so could a malicious program or model file. Choose a program you trust. Model hashes are checked, but the program itself is only checked against what you confirmed.": "llama-server はあなたの権限で動作します。ファイルの読み取りとネットワークの利用が可能で、悪意のあるプログラムやモデルファイルも同様です。信頼できるプログラムを選んでください。モデルのハッシュは検査されますが、プログラム自体は確認済みのものと照合されるだけです。",
  },
  es: {
    "File": "Archivo", "Edit": "Editar", "View": "Ver", "Window": "Ventana", "Help": "Ayuda", "Select all": "Seleccionar todo", "Undo": "Deshacer", "Redo": "Rehacer",
    "Local models": "Modelos locales", "Notes file": "Archivo de notas", "Download": "Descargar", "Add from file": "Añadir desde archivo", "Resume": "Reanudar", "Discard": "Descartar", "Pause": "Pausar", "Delete": "Eliminar", "Start": "Iniciar", "Use in assistant": "Usar en el asistente", "Not now": "Ahora no", "Find again": "Buscar de nuevo", "Choose program": "Elegir programa", "Detect Ollama": "Detectar Ollama", "Licence": "Licencia", "Download size": "Tamaño de descarga", "Comes from": "Procede de", "Free disk": "Disco libre", "Needs free": "Necesita libre", "Not sandboxed": "Sin aislamiento", "Move notes file": "Mover archivo de notas",
    "No llama-server was found.": "No se encontró llama-server.",
    "Run an AI model on this computer, so what you ask stays here. Nothing downloads or starts until you press a button.": "Ejecuta un modelo de IA en este ordenador, para que lo que preguntes se quede aquí. No se descarga ni se inicia nada hasta que pulses un botón.",
    "This app never downloads a program for you. To run a model on this computer, do one of these, then press Find again:": "Esta app nunca descarga un programa por ti. Para ejecutar un modelo aquí, haz una de estas cosas y pulsa Buscar de nuevo:",
    "llama-server runs with your rights. It can read your files and use the network, and so could a malicious program or model file. Choose a program you trust. Model hashes are checked, but the program itself is only checked against what you confirmed.": "llama-server se ejecuta con tus permisos. Puede leer tus archivos y usar la red, y también podría un programa o modelo malicioso. Elige un programa de confianza. Se comprueban los hashes de los modelos, pero el programa solo se compara con lo que confirmaste.",
  },
  ar: {
    "File": "ملف", "Edit": "تحرير", "View": "عرض", "Window": "نافذة", "Help": "مساعدة", "Select all": "تحديد الكل", "Undo": "تراجع", "Redo": "إعادة",
    "Local models": "النماذج المحلية", "Notes file": "ملف الملاحظات", "Download": "تنزيل", "Add from file": "إضافة من ملف", "Resume": "استئناف", "Discard": "تجاهل", "Pause": "إيقاف مؤقت", "Delete": "حذف", "Start": "بدء", "Use in assistant": "استخدام في المساعد", "Not now": "ليس الآن", "Find again": "البحث مجددًا", "Choose program": "اختيار برنامج", "Detect Ollama": "اكتشاف Ollama", "Licence": "الترخيص", "Download size": "حجم التنزيل", "Comes from": "المصدر", "Free disk": "المساحة الحرة", "Needs free": "المساحة المطلوبة", "Not sandboxed": "غير معزول", "Move notes file": "نقل ملف الملاحظات",
    "No llama-server was found.": "لم يُعثر على llama-server.",
    "Run an AI model on this computer, so what you ask stays here. Nothing downloads or starts until you press a button.": "شغّل نموذج ذكاء اصطناعي على هذا الحاسوب، فتبقى أسئلتك هنا. لا يُنزّل أو يبدأ شيء حتى تضغط زرًا.",
    "This app never downloads a program for you. To run a model on this computer, do one of these, then press Find again:": "لا ينزّل هذا التطبيق أي برنامج نيابة عنك. لتشغيل نموذج هنا، افعل أحد ما يلي ثم اضغط البحث مجددًا:",
    "llama-server runs with your rights. It can read your files and use the network, and so could a malicious program or model file. Choose a program you trust. Model hashes are checked, but the program itself is only checked against what you confirmed.": "يعمل llama-server بصلاحياتك. يمكنه قراءة ملفاتك واستخدام الشبكة، وكذلك يمكن لبرنامج أو ملف نموذج خبيث. اختر برنامجًا تثق به. تُفحص تجزئات النماذج، أما البرنامج فيُقارن فقط بما أكدته.",
  },
  hi: {
    "File": "फ़ाइल", "Edit": "संपादन", "View": "दृश्य", "Window": "विंडो", "Help": "सहायता", "Select all": "सब चुनें", "Undo": "पूर्ववत", "Redo": "फिर करें",
    "Local models": "स्थानीय मॉडल", "Notes file": "नोट्स फ़ाइल", "Download": "डाउनलोड", "Add from file": "फ़ाइल से जोड़ें", "Resume": "जारी रखें", "Discard": "हटाएँ", "Pause": "रोकें", "Delete": "मिटाएँ", "Start": "शुरू करें", "Use in assistant": "सहायक में उपयोग करें", "Not now": "अभी नहीं", "Find again": "फिर खोजें", "Choose program": "प्रोग्राम चुनें", "Detect Ollama": "Ollama खोजें", "Licence": "लाइसेंस", "Download size": "डाउनलोड आकार", "Comes from": "स्रोत", "Free disk": "खाली डिस्क", "Needs free": "चाहिए खाली", "Not sandboxed": "सैंडबॉक्स नहीं", "Move notes file": "नोट्स फ़ाइल ले जाएँ",
    "No llama-server was found.": "llama-server नहीं मिला।",
    "Run an AI model on this computer, so what you ask stays here. Nothing downloads or starts until you press a button.": "इस कंप्यूटर पर AI मॉडल चलाएँ, ताकि आपके सवाल यहीं रहें। बटन दबाने तक कुछ भी डाउनलोड या शुरू नहीं होता।",
    "This app never downloads a program for you. To run a model on this computer, do one of these, then press Find again:": "यह ऐप आपके लिए कोई प्रोग्राम डाउनलोड नहीं करता। यहाँ मॉडल चलाने के लिए इनमें से एक करें, फिर फिर खोजें दबाएँ:",
    "llama-server runs with your rights. It can read your files and use the network, and so could a malicious program or model file. Choose a program you trust. Model hashes are checked, but the program itself is only checked against what you confirmed.": "llama-server आपके अधिकारों से चलता है। यह आपकी फ़ाइलें पढ़ सकता है और नेटवर्क उपयोग कर सकता है, और दुर्भावनापूर्ण प्रोग्राम या मॉडल फ़ाइल भी। भरोसेमंद प्रोग्राम चुनें। मॉडल के हैश जाँचे जाते हैं, पर प्रोग्राम केवल आपकी पुष्टि से मिलाया जाता है।",
  },
};
