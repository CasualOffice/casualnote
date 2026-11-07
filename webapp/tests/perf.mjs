#!/usr/bin/env node
// SPK-01, SPK-02, SPK-05 and SPK-06 measurements in real Chrome, with trusted input through CDP
// and no dependencies. Build with release first: a dev build's numbers mean nothing.
//   node webapp/tests/perf.mjs typing [--blocks 5000] [--keys 300] [--profile out.json] [--embedding]
//                                          [--fix reload,visibility] serves a proposed fix
//   node webapp/tests/perf.mjs firstload
//   node webapp/tests/perf.mjs history --file history.cnws
//   node webapp/tests/perf.mjs search --pages 100000 --blocks 10        (needs PERF_BENCH_PKG)
//   node webapp/tests/perf.mjs simhash --seed 7 --years 0.25            (needs PERF_BENCH_PKG)
//   node webapp/tests/perf.mjs embed --dir model/ --model m.onnx --refs refs.json
//   node webapp/tests/perf.mjs product --shots dir/                     (downloads the real model)
// PERF_PKG overrides where /pkg is served from, so a run is safe from another build of webapp/pkg.
// PERF_BENCH_PKG is where tools/casual-note-bench was built for the browser.
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, readFileSync, rmSync, existsSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, extname, normalize } from "node:path";
import { gzipSync, brotliCompressSync } from "node:zlib";

const argv = process.argv.slice(2);
const mode = argv[0];
const opt = (name, fallback) => { const i = argv.indexOf("--" + name); return i < 0 ? fallback : argv[i + 1] ?? true; };
const flag = name => argv.includes("--" + name);
const root = new URL("../", import.meta.url).pathname;
const pkgDir = process.env.PERF_PKG || join(root, "pkg");
const benchDir = process.env.PERF_BENCH_PKG || null;
const MIME = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm", ".json": "application/json" };
const extra = new Map(); // Paths served from anywhere on disk: /__file/<name>.

// --fix NAME,NAME serves a proposed fix, so its effect is measured before anyone edits app.js:
//   reload    syncText updates the one block instead of re-reading and parsing the whole page
//   visibility  content-visibility on rows, tier 1 of docs/42
const fixes = new Set(String(opt("fix", "")).split(",").filter(Boolean));
function patched(path, bytes) {
  if (path === "app.js" && fixes.has("reload")) {
    const from = "const id = b.id; reload();\n  Object.assign(b, blockOf(id) || {});";
    const text = bytes.toString("utf8");
    if (!text.includes(from)) throw new Error("the reload fix no longer applies to app.js");
    return text.replace(from, "if (b.runs.length <= 1) { if (b.runs[0]) b.runs[0].text = now; else b.runs = [{ text: now, marks: [] }]; const held = blockOf(b.id); if (held && held !== b) held.runs = b.runs; } else { const id = b.id; reload(); Object.assign(b, blockOf(id) || {}); }");
  }
  if (path === "style.css" && fixes.has("visibility")) return Buffer.concat([bytes, Buffer.from("\n.blocks > .b { content-visibility: auto; contain-intrinsic-size: auto 30px; }\n")]);
  return bytes;
}
const server = createServer((req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname)).replace(/^\/+/, "") || "index.html";
  if (path === "__blank.html") { res.writeHead(200, { "content-type": "text/html" }); res.end("<!doctype html><title>bench</title>"); return; }
  let file;
  if (path.startsWith("__file/")) file = extra.get(path.slice(7));
  else if (path.startsWith("__bench/") && benchDir) file = join(benchDir, path.slice(8));
  else if (path.startsWith("pkg/")) file = join(pkgDir, path.slice(4));
  else file = join(root, path);
  if (!file || !existsSync(file) || statSync(file).isDirectory()) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { "content-type": MIME[extname(file)] || "application/octet-stream", "cache-control": "no-store" });
  res.end(patched(path, readFileSync(file)));
});
await new Promise(r => server.listen(0, "127.0.0.1", r));
const origin = `http://127.0.0.1:${server.address().port}`;

const chromePath = process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const profile = mkdtempSync(join(tmpdir(), "cn-perf-"));
const proc = spawn(chromePath, ["--headless=new", "--disable-gpu", `--user-data-dir=${profile}`, "--remote-debugging-port=0", "--window-size=1440,900", "--js-flags=--expose-gc", "about:blank"], { stdio: ["ignore", "ignore", "pipe"] });
const wsBase = await new Promise((resolve, reject) => {
  let log = ""; proc.stderr.on("data", d => { log += d; const m = log.match(/ws:\/\/[^\s]+/); if (m) resolve(m[0]); });
  setTimeout(() => reject(new Error("Chrome did not start")), 20000);
});
const port = new URL(wsBase).port;
const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
const target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find(t => t.type === "page");
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise(r => { ws.onopen = r; });
let id = 0; const waiting = new Map(), listeners = new Map(), problems = [];
ws.onmessage = e => {
  const m = JSON.parse(e.data);
  if (m.id) { waiting.get(m.id)?.(m.result ?? m); waiting.delete(m.id); return; }
  listeners.get(m.method)?.forEach(f => f(m.params));
  if (m.method === "Runtime.exceptionThrown") problems.push(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text);
  if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error") problems.push(m.params.args.map(a => a.value ?? a.description).join(" "));
};
const on = (method, f) => { if (!listeners.has(method)) listeners.set(method, []); listeners.get(method).push(f); };
const send = (method, params = {}) => new Promise(r => { const i = ++id; waiting.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async (expression, awaitPromise = false) => {
  const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
  return r.result?.value;
};
const sleep = ms => new Promise(r => setTimeout(r, ms));
await send("Runtime.enable"); await send("Page.enable");
const navigate = async url => {
  const loaded = new Promise(r => { const f = () => { listeners.set("Page.loadEventFired", (listeners.get("Page.loadEventFired") || []).filter(x => x !== f)); r(); }; on("Page.loadEventFired", f); });
  await send("Page.navigate", { url: origin + url });
  await loaded;
};
const pctl = (values, p) => { const s = [...values].sort((a, b) => a - b); return s[Math.round((s.length - 1) * p)] ?? NaN; };
const round = v => Math.round(v * 100) / 100;
const stats = values => ({ n: values.length, p50: round(pctl(values, .5)), p95: round(pctl(values, .95)), max: round(Math.max(...values)) });
const output = {
  chrome: version.Browser, machine: "see report", mode,
  pkg_bytes: existsSync(join(pkgDir, "casual_note_wasm_bg.wasm")) ? statSync(join(pkgDir, "casual_note_wasm_bg.wasm")).size : null,
};

/* ---------- Note text in the launch languages ---------- */
const LINES = [
  "Call the plumber about the leak under the kitchen sink before Friday.",
  "周五之前给水管工打电话，厨房水槽下面还在漏水。",
  "週五之前打電話給水電工，廚房水槽下面還在漏水。",
  "金曜日までに水道屋さんに電話する。台所の流しの下がまだ漏れている。",
  "금요일 전에 배관공에게 전화하기. 부엌 싱크대 밑이 아직 샌다.",
  "Llamar al fontanero por la fuga debajo del fregadero antes del viernes.",
  "Appeler le plombier pour la fuite sous l'évier avant vendredi.",
  "Vor Freitag den Klempner wegen des Lecks unter der Küchenspüle anrufen.",
  "Ligar para o encanador por causa do vazamento debaixo da pia antes de sexta.",
  "الاتصال بالسباك قبل يوم الجمعة بسبب التسرب تحت حوض المطبخ.",
  "शुक्रवार से पहले प्लंबर को फ़ोन करना है, रसोई के सिंक के नीचे अभी भी पानी टपक रहा है।",
];

/* ---------- Seeding the app's store with a real page, built through real ops ---------- */
async function seedLongPage(blocks) {
  await navigate("/__blank.html");
  return evaluate(`(async () => {
    const m = await import("/pkg/casual_note_wasm.js"); await m.default();
    const e = new m.Engine(4242n), lines = ${JSON.stringify(LINES)};
    const t0 = performance.now();
    e.apply(JSON.stringify({ op: "create_page", id: "long", parent: null, title: "Long page", database: null }));
    let after = null;
    for (let i = 0; i < ${blocks}; i++) {
      const id = "b" + i;
      e.apply(JSON.stringify({ op: "insert_block", page: "long", after, block: { id, kind: i % 25 === 0 ? "heading2" : "paragraph", checked: false, runs: [{ text: lines[i % lines.length], marks: [] }] } }));
      after = id;
    }
    const built = performance.now() - t0;
    const bytes = e.export();
    await new Promise((resolve, reject) => {
      const open = indexedDB.open("casualnote", 1);
      open.onupgradeneeded = () => open.result.createObjectStore("workspace");
      open.onerror = () => reject(open.error);
      open.onsuccess = () => { const tx = open.result.transaction("workspace", "readwrite"); tx.objectStore("workspace").put(bytes, "snapshot"); tx.oncomplete = () => { open.result.close(); resolve(); }; tx.onerror = () => reject(tx.error); };
    });
    localStorage.setItem("casualnote-ui", JSON.stringify({ sideW: 248, rightW: 300, side: true, recent: [], right: true, tabs: ["long"], cur: "long", open: {} }));
    return { built_ms: built, snapshot_bytes: bytes.length };
  })()`, true);
}

/** Opens the app and waits until the page's rows exist and the main thread answers. */
async function openApp(blocks) {
  const start = Date.now();
  await send("Page.navigate", { url: origin + "/index.html" });
  for (;;) {
    await sleep(15);
    let n = 0;
    try { n = await evaluate(`document.querySelectorAll(".blocks > .b").length`); } catch { /* navigating */ }
    if (n >= blocks) break;
    if (Date.now() - start > 180000) throw new Error("the page never opened");
  }
  const inner = await evaluate(`performance.now()`);
  return { wall_ms: Date.now() - start, performance_now_ms: inner };
}

/* ---------- Tracing ---------- */
async function traced(body) {
  const events = [];
  const collect = p => events.push(...p.value);
  on("Tracing.dataCollected", collect);
  await send("Tracing.start", { transferMode: "ReportEvents", traceConfig: { includedCategories: ["toplevel", "devtools.timeline", "disabled-by-default-devtools.timeline", "blink.user_timing"], excludedCategories: ["*"] } });
  await body();
  const done = new Promise(r => on("Tracing.tracingComplete", r));
  await send("Tracing.end");
  await done;
  return events;
}

/** Per keystroke: from the input event to the end of main-thread work, from the trace itself. */
function analyseTrace(events) {
  const names = events.filter(e => e.ph === "M" && e.name === "thread_name" && e.args?.name === "CrRendererMain");
  let best = null, bestCount = -1;
  for (const n of names) {
    const c = events.filter(e => e.pid === n.pid && e.tid === n.tid && e.name === "EventDispatch").length;
    if (c > bestCount) { bestCount = c; best = n; }
  }
  if (!best) return { error: "no renderer main thread in the trace" };
  const main = events.filter(e => e.pid === best.pid && e.tid === best.tid && e.ph === "X").sort((a, b) => a.ts - b.ts);
  const tasks = main.filter(e => e.name === "RunTask" || e.name === "ThreadControllerImpl::RunTask");
  const starts = main.filter(e => e.name === "EventDispatch" && e.args?.data?.type === "keydown").map(e => e.ts);
  const inserts = main.filter(e => e.name === "EventDispatch" && e.args?.data?.type === "beforeinput").map(e => e.ts);
  // A keystroke sends keydown then beforeinput within one task; insertText sends only beforeinput.
  const keys = [];
  for (const t of inserts) { const last = keys.at(-1); const k = starts.filter(s => s <= t && t - s < 50000).at(-1); keys.push(k ?? t); void last; }
  const handlerCpu = [], cpuToEnd = [], inputToEnd = [], handler = [], busy = [], frames = [], timers = [];
  keys.forEach((t, i) => {
    const next = keys[i + 1] ?? t + 150000;
    const within = tasks.filter(x => x.ts + x.dur > t && x.ts < next);
    const hit = within.filter(x => x.ts <= t && x.ts + x.dur >= t)[0] ?? within[0];
    if (!hit) return;
    handler.push((hit.ts + hit.dur - t) / 1000);
    handlerCpu.push((hit.tdur ?? hit.dur) / 1000);
    // The first frame after the input: the task holding Paint or Layerize.
    const paint = main.find(x => x.ts >= t && x.ts < next && (x.name === "Paint" || x.name === "Layerize" || x.name === "PrePaint" || x.name === "Commit"));
    const frameTask = paint && within.find(x => x.ts <= paint.ts && x.ts + x.dur >= paint.ts);
    const end = frameTask ? frameTask.ts + frameTask.dur : hit.ts + hit.dur;
    inputToEnd.push((end - t) / 1000);
    cpuToEnd.push(((hit.tdur ?? hit.dur) + (frameTask && frameTask !== hit ? frameTask.tdur ?? frameTask.dur : 0)) / 1000);
    if (frameTask) frames.push(frameTask.dur / 1000);
    busy.push(within.reduce((sum, x) => sum + Math.min(x.ts + x.dur, next) - Math.max(x.ts, t), 0) / 1000);
    for (const x of within) if (x !== hit && x !== frameTask && x.dur > 16000) timers.push(x.dur / 1000);
  });
  // What the long tasks that were not the input itself were running.
  const sources = new Map();
  for (const e of main) {
    if (e.name !== "FunctionCall" && e.name !== "TimerFire" && e.name !== "FireAnimationFrame") continue;
    const d = e.args?.data || {};
    const key = `${e.name} ${d.functionName || ""} ${(d.url || "").split("/").pop()}:${(d.lineNumber ?? -1) + 1}`;
    const v = sources.get(key) || { ms: 0, n: 0 }; v.ms += e.dur / 1000; v.n++; sources.set(key, v);
  }
  const sum = name => main.filter(e => e.name === name).reduce((s, e) => s + e.dur, 0) / 1000;
  return {
    keystrokes: keys.length,
    handler_task_cpu_ms: stats(handlerCpu), input_task_plus_frame_cpu_ms: stats(cpuToEnd), handler_task_ms: stats(handler), input_to_frame_end_ms: stats(inputToEnd), frame_task_ms: frames.length ? stats(frames) : null,
    all_main_thread_work_per_key_ms: stats(busy),
    sources: [...sources.entries()].sort((a, b) => b[1].ms - a[1].ms).slice(0, 15).map(([k, v]) => `${round(v.ms)} ms x${v.n} ${k}`),
    other_tasks_over_16ms: { count: timers.length, ...(timers.length ? stats(timers) : {}) },
    totals_ms: { style: round(sum("UpdateLayoutTree")), layout: round(sum("Layout")), prepaint: round(sum("PrePaint")), paint: round(sum("Paint")), script: round(sum("FunctionCall") + sum("EventDispatch")), timer: round(sum("TimerFire")) },
  };
}

/** Trusted typing into the focused block: keystrokes, with a composition commit every fourth. */
async function typeKeys(count, paceMs) {
  const letters = "the quick brown fox jumps over a lazy dog and keeps typing ";
  const commits = ["你好", "世界", "日本語", "한국어", "نص", "हिंदी"];
  for (let i = 0; i < count; i++) {
    if (i % 4 === 3) {
      await send("Input.insertText", { text: commits[i % commits.length] });
    } else {
      const ch = letters[i % letters.length];
      await send("Input.dispatchKeyEvent", { type: "keyDown", text: ch, key: ch, unmodifiedText: ch, code: ch === " " ? "Space" : "Key" + ch.toUpperCase() });
      await send("Input.dispatchKeyEvent", { type: "keyUp", key: ch, code: ch === " " ? "Space" : "Key" + ch.toUpperCase() });
    }
    await sleep(paceMs);
  }
}

async function focusBlock(index) {
  const rect = await evaluate(`(() => { const n = document.querySelectorAll(".blocks > .b .t")[${index}]; n.scrollIntoView({ block: "center" }); const r = n.getBoundingClientRect(); return { x: r.left + 40, y: r.top + r.height / 2, id: n.parentElement.dataset.id }; })()`);
  await sleep(300);
  const again = await evaluate(`(() => { const n = document.querySelectorAll(".blocks > .b .t")[${index}]; const r = n.getBoundingClientRect(); return { x: r.left + 40, y: r.top + r.height / 2 }; })()`);
  for (const type of ["mousePressed", "mouseReleased"]) await send("Input.dispatchMouseEvent", { type, x: again.x, y: again.y, button: "left", clickCount: 1 });
  await sleep(100);
  return rect.id;
}

function summariseProfile(profile, limit = 30) {
  const byId = new Map(profile.nodes.map(n => [n.id, n]));
  const self = new Map();
  const dt = profile.timeDeltas;
  profile.samples.forEach((sid, i) => { self.set(sid, (self.get(sid) || 0) + (dt[i] || 0)); });
  const merged = new Map();
  for (const [sid, us] of self) {
    const f = byId.get(sid).callFrame;
    const file = f.url.split("/").pop();
    const key = `${f.functionName || "(anonymous)"} ${file}:${f.lineNumber + 1}`;
    merged.set(key, (merged.get(key) || 0) + us);
  }
  const total = [...merged.entries()].filter(([k]) => !k.startsWith("(idle)") && !k.startsWith("(program)")).reduce((s, [, v]) => s + v, 0);
  // Inclusive time per JavaScript function, which is what names a cause when the leaves are wasm.
  const parent = new Map(); for (const n of profile.nodes) for (const c of n.children || []) parent.set(c, n.id);
  const inclusive = new Map();
  for (const [sid, us] of self) {
    const seen = new Set();
    for (let at = sid; at !== undefined; at = parent.get(at)) {
      const f = byId.get(at).callFrame;
      if (!f.url.endsWith(".js") || f.url.includes("_wasm")) continue;
      const key = `${f.functionName || "(anonymous)"} ${f.url.split("/").pop()}:${f.lineNumber + 1}`;
      if (!seen.has(key)) { seen.add(key); inclusive.set(key, (inclusive.get(key) || 0) + us); }
    }
  }
  const js = [...inclusive.entries()].sort((a, b) => b[1] - a[1]).slice(0, limit).map(([k, v]) => `${round(v / 1000)} ms  ${k}`);
  return { inclusive_js: js, total_ms: round(total / 1000), top: [...merged.entries()].filter(([k]) => !k.startsWith("(idle)")).sort((a, b) => b[1] - a[1]).slice(0, limit).map(([k, v]) => `${round(v / 1000)} ms  ${k}`) };
}

/* ---------- The experiments ---------- */
async function typing() {
  const blocks = Number(opt("blocks", 5000)), keys = Number(opt("keys", 300)), pace = Number(opt("pace", 120));
  output.seed = await seedLongPage(blocks);
  if (flag("embedding")) await installModelFiles();
  // Opening: cold, then twice more, on the same stored page.
  const opens = [];
  for (let i = 0; i < 3; i++) opens.push(await openApp(blocks));
  output.open_to_rows_ms = opens.map(o => o.wall_ms);
  output.open = { ...stats(opens.map(o => o.wall_ms)), performance_now_at_answer: opens.map(o => round(o.performance_now_ms)) };
  output.dom = await evaluate(`({ elements: document.querySelectorAll("*").length, rows: document.querySelectorAll(".blocks > .b").length, contentVisibility: getComputedStyle(document.querySelector(".blocks > .b")).contentVisibility })`);
  await sleep(1500);
  if (flag("embedding")) {
    output.embedding_status_before = await evaluate(`document.documentElement.dataset.meaning ?? null`);
  }
  const blockId = await focusBlock(Math.floor(blocks / 2));
  output.typing_in_block = blockId;
  const before = await evaluate(`document.activeElement.textContent.length`);
  const events = await traced(() => typeKeys(keys, pace));
  await sleep(600);
  const after = await evaluate(`document.activeElement.textContent.length`);
  output.chars_landed = { before, after };
  output.trace = analyseTrace(events);
  // Blocked time: the save that follows a burst, and the worst long tasks.
  // A separate, sampled run names the functions that took the time.
  await send("Profiler.enable");
  await send("Profiler.setSamplingInterval", { interval: 200 });
  await send("Profiler.start");
  await typeKeys(Math.min(keys, 150), pace);
  await sleep(400);
  const { profile } = await send("Profiler.stop");
  output.profile = summariseProfile(profile);
  if (opt("profile")) writeFileSync(opt("profile"), JSON.stringify(profile));
  // Cold page open, sampled.
  await send("Profiler.start");
  await openApp(blocks);
  const open = await send("Profiler.stop");
  output.open_profile = summariseProfile(open.profile, 25);
}

async function firstLoad() {
  const wasm = readFileSync(join(pkgDir, "casual_note_wasm_bg.wasm"));
  output.wasm = { bytes: wasm.length, gzip: gzipSync(wasm).length, brotli: brotliCompressSync(wasm).length };
  const runs = [];
  for (let i = 0; i < 3; i++) {
    await send("Network.clearBrowserCache").catch(() => {});
    await evaluate(`indexedDB.deleteDatabase?.("casualnote")`).catch(() => {});
    const start = Date.now();
    await send("Page.navigate", { url: origin + "/index.html" });
    for (;;) { await sleep(10); let up = false; try { up = await evaluate(`!!document.querySelector(".blocks, .doc .title")`); } catch { /* navigating */ } if (up) break; if (Date.now() - start > 60000) throw new Error("never booted"); }
    const timing = await evaluate(`(() => { const r = performance.getEntriesByType("resource").find(x => x.name.endsWith(".wasm")); const n = performance.getEntriesByType("navigation")[0]; return { wasm_fetch_ms: r?.duration, dom_content_loaded: n?.domContentLoadedEventEnd, at: performance.now() }; })()`);
    runs.push({ wall_ms: Date.now() - start, ...timing });
  }
  output.first_load = runs;
  // Compilation alone, on the same bytes.
  await navigate("/__blank.html");
  output.compile_ms = await evaluate(`(async () => { const b = await (await fetch("/pkg/casual_note_wasm_bg.wasm")).arrayBuffer(); const out = []; for (let i = 0; i < 5; i++) { const t = performance.now(); await WebAssembly.compile(b); out.push(performance.now() - t); } return out; })()`, true);
}

async function history() {
  const file = opt("file");
  extra.set("history", file);
  await navigate("/__blank.html");
  output.history = await evaluate(`(async () => {
    const m = await import("/pkg/casual_note_wasm.js"); const inst = await m.default();
    const mem = () => inst.memory.buffer.byteLength;
    const bytes = new Uint8Array(await (await fetch("/__file/history")).arrayBuffer());
    const whole = []; let engine;
    for (let i = 0; i < 3; i++) { const t = performance.now(); const e = new m.Engine(BigInt(100 + i)); e.import(bytes); whole.push(performance.now() - t); if (i === 2) engine = e; else e.free(); }
    const afterBoot = mem();
    const t0 = performance.now(); const metas = JSON.parse(engine.pages()); const pagesMs = performance.now() - t0;
    const sized = metas.map(p => [p.id, JSON.parse(engine.page(p.id)).blocks.length]);
    const typical = [...sized].sort((a, b) => Math.abs(a[1] - 200) - Math.abs(b[1] - 200)).slice(0, 100);
    const reads = []; for (const [id] of typical) { const t = performance.now(); JSON.parse(engine.page(id)); reads.push(performance.now() - t); }
    // Page by page: split the container, as ADR-030's per-page documents allow.
    const view = new DataView(bytes.buffer); const parts = new Map(); let at = 5;
    while (at < bytes.length) { const n = view.getUint32(at, true); at += 4; const name = new TextDecoder().decode(bytes.subarray(at, at + n)); at += n; const len = Number(view.getBigUint64(at, true)); at += 8; parts.set(name, bytes.subarray(at, at + len)); at += len; }
    const assemble = names => { const list = names.map(x => [new TextEncoder().encode(x), parts.get(x)]); const size = 5 + list.reduce((s, [a, b]) => s + 12 + a.length + b.length, 0); const out = new Uint8Array(size); out.set(bytes.subarray(0, 5)); let o = 5; const dv = new DataView(out.buffer); for (const [a, b] of list) { dv.setUint32(o, a.length, true); o += 4; out.set(a, o); o += a.length; dv.setBigUint64(o, BigInt(b.length), true); o += 8; out.set(b, o); o += b.length; } return out; };
    const base = new m.Engine(500n); const tt = performance.now(); base.import(assemble([""])); const treeMs = performance.now() - tt;
    const lazy = []; for (const [id] of typical) { const c = assemble([id]); const t = performance.now(); base.import(c); JSON.parse(base.page(id)); lazy.push(performance.now() - t); }
    const exports = []; for (let i = 0; i < 2; i++) { const t = performance.now(); engine.export(); exports.push(performance.now() - t); }
    return { file_bytes: bytes.length, whole_workspace_import_ms: whole, wasm_memory_after_import: afterBoot, wasm_memory_end: mem(), pages_json_ms: pagesMs, typical_blocks: [typical[0][1], typical.at(-1)[1]], loaded_read_ms: reads, tree_only_import_ms: treeMs, one_page_import_read_ms: lazy, export_ms: exports, doc_count: parts.size };
  })()`, true);
  const h = output.history;
  h.loaded_read_ms = stats(h.loaded_read_ms);
  h.one_page_import_read_ms = stats(h.one_page_import_read_ms);
}

async function search() {
  const pages = Number(opt("pages", 100000)), blocks = Number(opt("blocks", 10));
  await navigate("/__blank.html");
  output.search = await evaluate(`(async () => {
    const m = await import("/__bench/casual_note_bench.js"); const inst = await m.default();
    const t0 = performance.now(); const s = new m.SearchBench(${pages}, ${blocks}, 7); const build = performance.now() - t0;
    const queries = JSON.parse(s.queries()); const times = [], worst = [];
    for (const q of queries) for (let i = 0; i < 3; i++) { const t = performance.now(); const json = s.search(q); const ms = performance.now() - t; times.push(ms); if (i === 2) worst.push([ms, q, JSON.parse(json).length]); }
    worst.sort((a, b) => b[0] - a[0]);
    return { pages: ${pages}, stats: JSON.parse(s.stats()), build_ms: build, wasm_memory: inst.memory.buffer.byteLength, times, slowest: worst.slice(0, 5) };
  })()`, true);
  output.search.times = stats(output.search.times);
}

async function simhash() {
  await navigate("/__blank.html");
  output.sim = await evaluate(`(async () => { const m = await import("/__bench/casual_note_bench.js"); await m.default(); return JSON.parse(m.sim_hash(${Number(opt("seed", 7))}, ${Number(opt("years", 0.25))})); })()`, true);
}

async function embed() {
  const dir = opt("dir"), file = opt("model"), prefix = opt("prefix", "none"), max = Number(opt("max-tokens", 128));
  extra.set("model", join(dir, file)); extra.set("tokenizer", join(dir, "tokenizer.json"));
  const refs = opt("refs") ? JSON.parse(readFileSync(opt("refs"), "utf8")) : [];
  await navigate("/__blank.html");
  output.embed = await evaluate(`(async () => {
    const m = await import("/pkg/embed/casual_note_embed.js"); const inst = await m.default();
    const model = new Uint8Array(await (await fetch("/__file/model")).arrayBuffer());
    const tok = new Uint8Array(await (await fetch("/__file/tokenizer")).arrayBuffer());
    const before = inst.memory.buffer.byteLength;
    const t0 = performance.now(); const M = new m.Model(model, tok, 384, ${JSON.stringify(prefix)}, ${max}); const load = performance.now() - t0;
    const lines = ${JSON.stringify(LINES)}; const short = [], medium = [];
    for (const l of lines) { const t = performance.now(); M.embed(l, false); medium.push(performance.now() - t); }
    for (const q of ["plumber", "预算", "passport renewal", "जाँच", "أفكار للحديقة"]) { const t = performance.now(); M.embed(q, true); short.push(performance.now() - t); }
    const refs = ${JSON.stringify(refs)}; const diffs = [];
    for (const [text, vec] of refs) { const v = M.embed(text, false); let max = 0, same = true, dot = 0; for (let i = 0; i < v.length; i++) { max = Math.max(max, Math.abs(v[i] - vec[i])); if (v[i] !== Float32Array.of(vec[i])[0]) same = false; dot += v[i] * vec[i]; } diffs.push({ max_abs_diff: max, identical: same, cosine: dot }); }
    const store = new m.Store("bench", 384); const scan = {}; const row = new Float32Array(384);
    let seed = 1; const rnd = () => { seed = (seed * 1664525 + 1013904223) >>> 0; return seed / 2147483648 - 1; };
    for (let i = 1; i <= 100000; i++) { for (let j = 0; j < 384; j++) row[j] = rnd(); store.submit("p" + (i % 5000), JSON.stringify({ key: "k" + i, hash: "h", weight: 1 }), row); if (i === 10000 || i === 100000) { const t = []; for (let k = 0; k < 7; k++) { const a = performance.now(); store.nearest(row, 10); t.push(performance.now() - a); } t.sort((a, b) => a - b); scan[i] = t[3]; } }
    return { model_bytes: model.length, load_ms: load, memory_before: before, memory_after_load: inst.memory.buffer.byteLength, embed_note_ms: medium, embed_query_ms: short, refs: diffs, scan_ms: scan, memory_after_store: inst.memory.buffer.byteLength };
  })()`, true);
  output.embed.embed_note_ms = stats(output.embed.embed_note_ms);
  output.embed.embed_query_ms = stats(output.embed.embed_query_ms);
  const wasm = join(pkgDir, "embed", "casual_note_embed_bg.wasm");
  if (existsSync(wasm)) { const b = readFileSync(wasm); output.embed.runtime_wasm = { bytes: b.length, gzip: gzipSync(b).length, brotli: brotliCompressSync(b).length }; }
}

/** The real thing, end to end: Settings, the real download, indexing, Related and search by meaning. */
async function product() {
  const shots = opt("shots");
  const shot = async name => { if (!shots) return; const { data } = await send("Page.captureScreenshot", { format: "png" }); writeFileSync(join(shots, name + ".png"), Buffer.from(data, "base64")); };
  const notes = [
    ["dog", "Bruno", "The vet gave Bruno his rabies shot and booked a booster for next year."],
    ["tyre", "Car", "Picked up a nail on the motorway; the front left tyre went flat and the garage replaced it."],
    ["bread", "Baking", "Feed the sourdough starter twice a day with equal parts flour and water until it doubles."],
    ["gou", "布鲁诺", "兽医给布鲁诺打了狂犬疫苗，并预约了明年的加强针。"],
    ["che", "汽车", "在高速公路上扎了钉子，左前轮胎没气了，修车厂换了一条新的。"],
    ["mian", "面包", "每天给酸面团酵种喂两次等量的面粉和水，直到体积翻倍。"],
  ];
  await navigate("/__blank.html");
  await evaluate(`(async () => {
    const m = await import("/pkg/casual_note_wasm.js"); await m.default();
    const e = new m.Engine(77n);
    for (const [id, title, text] of ${JSON.stringify(notes)}) {
      e.apply(JSON.stringify({ op: "create_page", id, parent: null, title, database: null }));
      e.apply(JSON.stringify({ op: "insert_block", page: id, after: null, block: { id: id + "b", kind: "paragraph", checked: false, runs: [{ text, marks: [] }] } }));
    }
    const bytes = e.export();
    await new Promise((resolve, reject) => {
      const open = indexedDB.open("casualnote", 1);
      open.onupgradeneeded = () => open.result.createObjectStore("workspace");
      open.onerror = () => reject(open.error);
      open.onsuccess = () => { const tx = open.result.transaction("workspace", "readwrite"); tx.objectStore("workspace").put(bytes, "snapshot"); tx.oncomplete = () => { open.result.close(); resolve(); }; tx.onerror = () => reject(tx.error); };
    });
    localStorage.setItem("casualnote-ui", JSON.stringify({ sideW: 248, rightW: 300, side: true, recent: [], right: true, tabs: ["dog"], cur: "dog", open: {} }));
  })()`, true);
  await navigate("/index.html");
  await sleep(1200);
  const relatedBefore = await evaluate(`document.querySelector("#rpRelated [data-method]")?.dataset.method ?? null`);
  await evaluate(`document.querySelector("#settingsBtn").click()`);
  await sleep(200);
  await evaluate(`document.querySelector("#meaningSet").scrollIntoView({ block: "center" })`);
  await shot("settings-off");
  const statusOff = await evaluate(`document.querySelector("#meaningStatus").textContent`);
  const t0 = Date.now();
  await evaluate(`document.querySelector("#meaningGet").click()`);
  const stages = {};
  let last = null, midShot = false;
  for (;;) {
    await sleep(100);
    const st = await evaluate(`document.documentElement.dataset.meaning`);
    if (st !== last) { stages[st] = Date.now() - t0; last = st; }
    if (st === "downloading" && !midShot) { midShot = true; await sleep(800); await shot("settings-downloading"); }
    if (st === "ready" || st === "failed" || st === "downloadfailed" || st === "unavailable") break;
    if (Date.now() - t0 > 600000) throw new Error("never finished: " + st);
  }
  await sleep(200);
  await evaluate(`document.querySelector("#meaningSet")?.scrollIntoView({ block: "center" })`);
  await shot("settings-" + last);
  const statusOn = await evaluate(`document.querySelector("#meaningStatus")?.textContent`);
  const detail = await evaluate(`document.querySelector("#meaningDetail")?.textContent`);
  await evaluate(`document.querySelector("#tree .row[data-id=dog] .open").click()`);
  await sleep(900);
  await shot("related-" + last);
  const related = await evaluate(`({ method: document.querySelector("#rpRelated [data-method]")?.dataset.method, rows: [...document.querySelectorAll("#rpRelated .rel")].map(r => r.textContent) })`);
  const ask = async q => {
    await send("Input.dispatchKeyEvent", { type: "keyDown", key: "k", code: "KeyK", modifiers: 2, windowsVirtualKeyCode: 75 });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key: "k", code: "KeyK", modifiers: 2, windowsVirtualKeyCode: 75 });
    await sleep(100);
    await send("Input.insertText", { text: q });
    const t = Date.now();
    for (;;) { await sleep(20); if (await evaluate(`!!document.querySelector("#paletteMeaning") || [...document.querySelectorAll(".modal .hdr")].length > 0 && false`)) break; if (Date.now() - t > 8000) break; }
    const ms = Date.now() - t;
    await sleep(100);
    const out = await evaluate(`[...document.querySelectorAll(".modal .list > *")].map(n => n.id === "paletteMeaning" ? "[by meaning]" : (n.querySelector(".palt")?.textContent ?? n.textContent))`);
    return { q, wait_ms: ms, list: out };
  };
  const search = [await ask("When does my dog need his next vaccination?")];
  await shot("palette");
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  search.push(await ask("我家的汽车轮胎坏了"));
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  // Reopened: the model is on this device, and the vectors come back.
  await navigate("/index.html");
  await sleep(1500);
  let again = null; const t1 = Date.now();
  for (;;) { await sleep(100); again = await evaluate(`document.documentElement.dataset.meaning ?? null`); if (again === "ready" || again === "failed" || Date.now() - t1 > 120000) break; }
  output.product = { relatedBefore, statusOff, stages_ms: stages, statusOn, detail, related, search, reopen: { state: again, ms: Date.now() - t1 } };
}

/** Puts a model already on disk where the Settings download would, for typing under embedding. */
async function installModelFiles() {
  const dir = opt("dir"), file = opt("model");
  extra.set("model", join(dir, file)); extra.set("tokenizer", join(dir, "tokenizer.json"));
  await navigate("/__blank.html");
  await evaluate(`(async () => {
    const model = await (await fetch("/__file/model")).arrayBuffer(), tok = await (await fetch("/__file/tokenizer")).arrayBuffer();
    await new Promise((resolve, reject) => {
      const open = indexedDB.open("casualnote-meaning", 1);
      open.onupgradeneeded = () => { open.result.createObjectStore("models"); open.result.createObjectStore("vectors"); };
      open.onerror = () => reject(open.error);
      open.onsuccess = () => { const tx = open.result.transaction("models", "readwrite"); const s = tx.objectStore("models"); s.put(new Blob([model]), "${opt("model-id", "paraphrase-multilingual-MiniLM-L12-v2")}:model"); s.put(new Blob([tok]), "${opt("model-id", "paraphrase-multilingual-MiniLM-L12-v2")}:tokenizer"); tx.oncomplete = () => { open.result.close(); resolve(); }; tx.onerror = () => reject(tx.error); };
    });
    localStorage.setItem("casualnote-meaning", "${opt("model-id", "paraphrase-multilingual-MiniLM-L12-v2")}");
  })()`, true);
}

let failed = false;
try {
  if (mode === "typing") await typing();
  else if (mode === "firstload") await firstLoad();
  else if (mode === "history") await history();
  else if (mode === "search") await search();
  else if (mode === "simhash") await simhash();
  else if (mode === "embed") await embed();
  else if (mode === "product") await product();
  else { console.error("modes: typing firstload history search simhash embed"); failed = true; }
} catch (err) {
  console.error("FAILED:", err.message || err); failed = true;
}
output.problems = problems;
console.log(JSON.stringify(output, null, 1));
ws.close(); proc.kill(); server.close();
try { rmSync(profile, { recursive: true, force: true }); } catch { /* best effort */ }
process.exit(failed ? 1 : 0);
