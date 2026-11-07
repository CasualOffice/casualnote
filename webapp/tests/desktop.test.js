// The desktop feature, against a fake shell. The real shell's Rust is tested in desktop/tests.
import { panel, transport, bridge } from "../features/desktop.js";

const GB = 1e9;
const model = (over = {}) => ({ id: "m1", name: "Qwen3 4B", position: "Suggested first try", licence: "Apache-2.0", licence_url: "https://x", bytes: 2.5 * GB, source: "huggingface.co", pinned: true, sha256: "a".repeat(64), installed: false, part_bytes: 0, active: false, ...over });

function fakeHost(state) {
  const calls = [];
  return {
    calls,
    invoke: async (cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === "catalogue_status") return { models: state.models, free_bytes: state.free ?? 100 * GB, margin_bytes: GB, running: state.running ?? null };
      if (cmd === "runtime_status") return state.runtime ?? { found: false, path: null, source: null, running: null, sandboxed: false };
      if (cmd === "notes_info") return { path: "/data/notes.casualnote", exists: true, bytes: 10 };
      if (state.fail?.[cmd]) throw state.fail[cmd];
      if (cmd === "ollama_detect") return state.ollama ?? [];
      return null;
    },
  };
}

export async function run({ api, ok, wait, $, $$ }) {
  const mount = async state => {
    api.openView("settings");
    const box = api.el("div", { id: "dskTest" });
    $("#content .agent").append(box);
    const host = fakeHost(state);
    const events = panel(api, host, box);
    await wait(20);
    return { box, host, events, row: id => box.querySelector(`.dsk-model[data-id="${id}"]`), button: (id, act) => box.querySelector(`[data-model="${id}"][data-act="${act}"]`) };
  };

  await ok("desktop: in a browser the feature adds nothing and sends nothing", () => {
    api.openView("settings");
    return bridge() === null && !$("#dskLocal") && window.fetch.name !== "desktopFetch";
  });

  await ok("desktop: size and licence show before anything starts, and nothing starts without a click", async () => {
    const { box, host, row, button } = await mount({ models: [model()] });
    const text = row("m1").textContent;
    const shown = text.includes("2.5 GB") && text.includes("Apache-2.0");
    const quiet = !host.calls.some(([c]) => c === "download_start");
    button("m1", "ask").click();
    await wait(0);
    const ask = row("m1").querySelector(".dsk-ask").textContent;
    const facts = ask.includes("huggingface.co") && ask.includes("Apache-2.0") && ask.includes("3.5 GB") && !host.calls.some(([c]) => c === "download_start");
    button("m1", "download").click();
    await wait(10);
    const sent = host.calls.find(([c]) => c === "download_start");
    box.remove();
    return shown && quiet && facts && sent[1].id === "m1" && Object.keys(sent[1]).length === 1;
  });

  await ok("desktop: a missing pin refuses and says so", async () => {
    const { box, row, button } = await mount({ models: [model({ pinned: false, sha256: null })] });
    const said = row("m1").textContent.includes("hash pin is missing");
    const r = said && !button("m1", "ask") && !button("m1", "download");
    box.remove();
    return r;
  });

  await ok("desktop: too little disk disables the download and names how much to free", async () => {
    const { box, row, button } = await mount({ models: [model()], free: 1 * GB });
    button("m1", "ask").click();
    await wait(0);
    const r = button("m1", "download").disabled && row("m1").querySelector('[role="alert"]').textContent.includes("2.5 GB");
    box.remove();
    return r;
  });

  await ok("desktop: no runtime says exactly what to do, and Start stays off", async () => {
    const { box, button } = await mount({ models: [model({ installed: true })] });
    const words = box.querySelector("#dskNoRuntime").textContent + box.querySelector(".dsk-steps").textContent;
    const r = words.includes("No llama-server was found") && words.includes("brew install llama.cpp") && words.includes("Choose program") && words.includes("Detect Ollama") && button("m1", "start").disabled;
    box.remove();
    return r;
  });

  await ok("desktop: progress is real, pausing keeps focus, and paused says nothing is lost", async () => {
    const { box, host, row, button, events } = await mount({ models: [model()] });
    events.onDownload({ id: "m1", phase: "downloading", done: 0.5 * GB, total: 2.5 * GB, rate: 10e6, restarted: false });
    await wait(0);
    const bar = row("m1").querySelector(".dsk-bar");
    const pause = button("m1", "pause");
    pause.focus();
    events.onDownload({ id: "m1", phase: "downloading", done: 1.25 * GB, total: 2.5 * GB, rate: 10e6, restarted: false });
    const line = row("m1").querySelector(".dsk-state").textContent;
    const shown = bar.getAttribute("aria-valuenow") === "50" && line.includes("1.25 GB of 2.5 GB") && line.includes("10 MB") && document.activeElement === pause;
    pause.click();
    await wait(5);
    const asked = host.calls.some(([c, a]) => c === "download_pause" && a.id === "m1");
    events.onDownload({ id: "m1", phase: "paused", done: 1.25 * GB, total: 2.5 * GB, rate: 0, restarted: false });
    await wait(0);
    const paused = row("m1").textContent.includes("Nothing is lost") && !!button("m1", "resume") && !!button("m1", "cancel");
    box.remove();
    return shown && asked && paused;
  });

  await ok("desktop: a refused hash says what happened and what to do", async () => {
    const { box, row, events } = await mount({ models: [model()] });
    events.onDownload({ id: "m1", phase: "failed", done: 0, total: 2.5 * GB, rate: 0, restarted: false, code: "CN-MDL-0002", detail: "x" });
    await wait(20);
    const alert = row("m1").querySelector('[role="alert"]');
    const r = alert.textContent.includes("did not match") && alert.textContent.includes("Download it again") && alert.textContent.includes("CN-MDL-0002") && !alert.querySelector("button");
    box.remove();
    return r;
  });

  await ok("desktop: one click points the assistant at the local model, with no port or key in the page", async () => {
    // The assistant's own settings form is on this page, and its real listeners take the change.
    const saved = localStorage.getItem("casualnote-ai");
    const { box, button, row } = await mount({ models: [model({ installed: true })], running: "m1", runtime: { found: true, path: "/opt/homebrew/bin/llama-server", source: "path", running: "m1", sandboxed: false } });
    button("m1", "use").click();
    await wait(0);
    const cfg = JSON.parse(localStorage.getItem("casualnote-ai"));
    const set = cfg.provider === "local" && cfg.endpoint === "https://this-computer.invalid/v1" && cfg.model === "m1";
    const status = $("#aiStatus").textContent.includes("this-computer.invalid");
    const clean = !/127\.0\.0\.1:\d|api.key/i.test(JSON.stringify(cfg));
    const says = row("m1").textContent.includes("The assistant is using it");
    box.remove();
    // Back as it was, for the tests after this one.
    const provider = $("#aiProvider");
    provider.value = saved ? JSON.parse(saved).provider : "off"; provider.dispatchEvent(new Event("change"));
    if (saved) { localStorage.setItem("casualnote-ai", saved); }
    return set && status && clean && says;
  });

  await ok("desktop: Not sandboxed is stated on the page", async () => {
    const { box } = await mount({ models: [] });
    const note = box.querySelector("#dskNotSandboxed").textContent;
    box.remove();
    return note.includes("Not sandboxed") && note.includes("read your files") && note.includes("command line");
  });

  await ok("desktop: every string has logical CSS and no emoji", async () => {
    const css = await (await fetch("features/desktop.css")).text();
    const js = await (await fetch("features/desktop.js")).text();
    const physical = /(?:margin|padding|border)-(?:left|right|top|bottom)\b|\b(?:left|right):\s/.test(css);
    return !physical && !/\p{Extended_Pictographic}/u.test(js.replace(/DRAFTS[\s\S]*/, "")) && !/—/.test(js);
  });

  /* The transport: bytes from a fake channel, through a real Response */
  const fakeChannelHost = script => {
    const calls = [];
    return {
      calls,
      channel: on => ({ on }),
      invoke: (cmd, args) => {
        calls.push([cmd, args]);
        return cmd === "ai_request" ? script(args.channel.on, args.request) : Promise.resolve();
      },
    };
  };
  const enc = new TextEncoder();
  const buf = s => enc.encode(s).buffer;
  const noLocal = () => { throw new Error("should not reach the real fetch"); };

  await ok("desktop: the assistant's request goes to the shell, and a streamed reply reads as a Response", async () => {
    const host = fakeChannelHost(async on => {
      on({ status: 200, contentType: "text/event-stream", retryAfter: null });
      on(buf("data: one\n\n")); on(buf("data: two\n\n")); on({ done: true });
    });
    const f = transport(host, k => k, noLocal);
    const res = await f("https://this-computer.invalid/v1/chat/completions", { method: "POST", headers: { "content-type": "application/json" }, body: "{}", signal: new AbortController().signal });
    const text = await res.text();
    const [, args] = host.calls[0];
    return res.ok && /event-stream/.test(res.headers.get("content-type")) && text === "data: one\n\ndata: two\n\n" && args.request.url.startsWith("https://this-computer.invalid/") && args.request.headers[0][0] === "content-type";
  });

  await ok("desktop: Stop cancels in the shell, and the reader sees an abort", async () => {
    const host = fakeChannelHost(on => { on({ status: 200, contentType: "text/event-stream" }); on(buf("data: x\n\n")); return new Promise(() => {}); });
    const ctrl = new AbortController();
    const res = await transport(host, k => k, noLocal)("https://this-computer.invalid/v1/chat/completions", { method: "POST", headers: {}, body: "{}", signal: ctrl.signal });
    const reader = res.body.getReader();
    await reader.read();
    ctrl.abort();
    let aborted = false;
    try { await reader.read(); } catch (e) { aborted = e.name === "AbortError"; }
    return aborted && host.calls.some(([c]) => c === "ai_cancel");
  });

  await ok("desktop: a model that is not running says where to press Start", async () => {
    const host = fakeChannelHost(() => Promise.reject({ code: "CN-MDL-0014", detail: "" }));
    const res = await transport(host, k => k, noLocal)("https://this-computer.invalid/v1/chat/completions", { method: "POST", headers: {}, body: "{}" });
    const j = await res.json();
    return res.status === 503 && j.error.message.includes("press Start");
  });

  await ok("desktop: the page's own files and GET requests do not go through the shell", async () => {
    const host = fakeChannelHost(() => Promise.reject(new Error("no")));
    const seen = [];
    const f = transport(host, k => k, async url => { seen.push(url); return new Response("x"); });
    await f("features/desktop.css");
    await f(`${location.origin}/x`, { method: "POST" });
    await f("https://outside.example/", { method: "GET" });
    return seen.length === 3 && host.calls.length === 0;
  });
  api.openView("settings");
}
