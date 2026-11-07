// The durable log on the web (docs/23). The Rust store decides every byte and every order. This
// file only carries out its requests: on OPFS in a dedicated worker where the browser has it, and
// otherwise as IndexedDB records per frame. "Saved" shows only after a batch has fully completed,
// which on the web means the browser accepted the flush. Whether that survives power loss is
// browser-defined and unverified (STO-03), so nothing here claims more.
import init, { LogStore, sync_derive_kek } from "./pkg/casual_note_wasm.js";

const LOG_DIR = "casualnote", LOG_DB = "casualnote-log";
// The whole-snapshot store this replaces. It is read for migration, and never deleted here.
const OLD_DB = "casualnote", OLD_STORE = "workspace", OLD_KEY = "snapshot";
const BIG_PREFIXES = ["local/sync/", "local/quarantine/"];
const RETRY_MS = 2000;

const hexOf = bytes => [...bytes].map(b => b.toString(16).padStart(2, "0")).join("");
const randomHex = n => hexOf(crypto.getRandomValues(new Uint8Array(n)));

function joinBlobs(files) {
  let total = 0;
  const index = files.map(([path, bytes]) => { const at = total; total += bytes.length; return [path, at, bytes.length]; });
  const blob = new Uint8Array(total);
  files.forEach(([, bytes], i) => blob.set(bytes, index[i][1]));
  return { index, blob };
}

/* ---------- OPFS, inside the worker ---------- */
function workerMain() {
  let rootPromise = null, wasmReady = null;
  const root = () => rootPromise ??= navigator.storage.getDirectory().then(d => d.getDirectoryHandle(LOG_DIR, { create: true }));
  const split = path => { const parts = path.split("/"); return [parts.slice(0, -1), parts.at(-1)]; };
  async function dirOf(parts, create) {
    let d = await root();
    for (const p of parts) d = await d.getDirectoryHandle(p, { create });
    return d;
  }
  // A handle locks its file, so handles live for one batch. Only this open writes its own segments.
  async function runOps(ops, blob) {
    const handles = new Map();
    const open = async path => {
      if (handles.has(path)) return handles.get(path);
      const [dirs, name] = split(path);
      const file = await (await dirOf(dirs, true)).getFileHandle(name, { create: true });
      const h = await file.createSyncAccessHandle();
      handles.set(path, h);
      return h;
    };
    const drop = path => { handles.get(path)?.close(); handles.delete(path); };
    try {
      for (const op of ops) {
        if (op.op === "mkdir") { await dirOf(op.path.split("/"), true); }
        else if (op.op === "write") {
          const h = await open(op.path);
          const wrote = h.write(blob.subarray(op.off, op.off + op.len), { at: op.at });
          if (wrote !== op.len) throw new Error(`short write: ${wrote} of ${op.len}`);
        } else if (op.op === "sync") { (await open(op.path)).flush(); }
        else if (op.op === "setlen") { (await open(op.path)).truncate(op.len); }
        else if (op.op === "remove") {
          drop(op.path);
          const [dirs, name] = split(op.path);
          try { await (await dirOf(dirs, false)).removeEntry(name); } catch (e) { if (e.name !== "NotFoundError") throw e; }
        }
        // "syncdir": OPFS has no separate durability step for a directory entry.
      }
      return { error: null, nospace: false };
    } catch (e) {
      return { error: `${e.name}: ${e.message}`, nospace: e.name === "QuotaExceededError" };
    } finally {
      for (const h of handles.values()) { try { h.close(); } catch { /* already closed */ } }
    }
  }
  async function bytesOf(file) {
    // Another tab may hold this file for a moment during its own batch.
    for (let i = 0; ; i++) {
      try { return new Uint8Array(await (await file.getFile()).arrayBuffer()); }
      catch (e) { if (e.name !== "NoModificationAllowedError" || i > 50) throw e; await new Promise(r => setTimeout(r, 20)); }
    }
  }
  async function walk(dir, prefix, out) {
    for await (const [name, h] of dir.entries()) {
      const path = prefix ? `${prefix}/${name}` : name;
      if (h.kind === "directory") await walk(h, path, out);
      else out.push([path, h]);
    }
  }
  const actions = {
    probe: async () => {
      if (!navigator.storage?.getDirectory || !self.FileSystemSyncAccessHandle) return { ready: false };
      const d = await root();
      const f = await d.getFileHandle("probe", { create: true });
      const h = await f.createSyncAccessHandle();
      h.close();
      await d.removeEntry("probe");
      return { ready: true };
    },
    run: m => runOps(m.ops, m.blob),
    tree: async m => {
      const found = [];
      try { await walk(await dirOf(m.dir ? m.dir.split("/") : [], false), m.dir || "", found); }
      catch (e) { if (e.name !== "NotFoundError") throw e; }
      const files = [];
      for (const [path, h] of found) {
        const skip = !m.dir && BIG_PREFIXES.some(p => path.startsWith(p));
        files.push([path, skip ? new Uint8Array(0) : await bytesOf(h)]);
      }
      return joinBlobs(files);
    },
    read: async m => {
      const out = [];
      for (const path of m.paths) {
        const [dirs, name] = split(path);
        try { out.push([path, await bytesOf(await (await dirOf(dirs, false)).getFileHandle(name))]); }
        catch (e) { if (e.name !== "NotFoundError") throw e; }
      }
      return joinBlobs(out);
    },
    list: async m => {
      const found = [];
      try { await walk(await dirOf(m.dir ? m.dir.split("/") : [], false), m.dir || "", found); }
      catch (e) { if (e.name !== "NotFoundError") throw e; }
      return { paths: found.map(([p]) => p) };
    },
    wipe: async () => {
      const r = await root();
      for await (const [name] of r.entries()) await r.removeEntry(name, { recursive: true });
      return {};
    },
    // Slow by design, so it stays off the page's thread, and its memory stays out of the page's engine.
    kdf: async m => {
      wasmReady ??= init();
      await wasmReady;
      return { kek: sync_derive_kek(m.passphrase, m.salt, m.m_kib, m.t, m.p) };
    },
  };
  let chain = Promise.resolve();
  self.onmessage = e => {
    const m = e.data;
    chain = chain.then(async () => {
      try {
        const result = await actions[m.action](m);
        self.postMessage({ id: m.id, result });
      } catch (err) {
        self.postMessage({ id: m.id, error: `${err.name}: ${err.message}` });
      }
    });
  };
}
const inWorker = typeof WorkerGlobalScope !== "undefined" && self instanceof WorkerGlobalScope;
if (inWorker) workerMain();

/* ---------- The hosts ---------- */
function opfsHost(worker) {
  let next = 1;
  const waiting = new Map();
  worker.onmessage = e => {
    const w = waiting.get(e.data.id);
    if (!w) return;
    waiting.delete(e.data.id);
    if (e.data.error) w.reject(new Error(e.data.error)); else w.resolve(e.data.result);
  };
  const call = (action, data = {}, transfer = []) => new Promise((resolve, reject) => {
    const id = next++;
    waiting.set(id, { resolve, reject });
    worker.postMessage({ id, action, ...data }, transfer);
  });
  return {
    kind: "opfs",
    probe: () => call("probe"),
    run: (ops, blob) => call("run", { ops, blob }),
    tree: () => call("tree"),
    treeOf: dir => call("tree", { dir }),
    read: paths => call("read", { paths }),
    list: dir => call("list", { dir }),
    wipe: () => call("wipe"),
    kdf: params => call("kdf", params),
    worker,
  };
}

function idbRequest(r) {
  return new Promise((resolve, reject) => { r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error); });
}
function idbOpen(name, version, upgrade) {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(name, version);
    open.onupgradeneeded = () => upgrade(open.result);
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
}

/** IndexedDB records per frame. A batch is one strict transaction, so its commit is its flush. */
export async function idbHost(dbName = LOG_DB) {
  const db = await idbOpen(dbName, 1, d => d.createObjectStore("chunks"));
  const STORE = "chunks";
  const range = path => IDBKeyRange.bound([path, 0], [path, Number.MAX_SAFE_INTEGER]);
  async function assemble(store, path) {
    const [keys, values] = await Promise.all([idbRequest(store.getAllKeys(range(path))), idbRequest(store.getAll(range(path)))]);
    let len = 0;
    keys.forEach((k, i) => { len = Math.max(len, k[1] + values[i].length); });
    const out = new Uint8Array(len);
    keys.forEach((k, i) => out.set(values[i], k[1]));
    return out;
  }
  const allPaths = async store => [...new Set((await idbRequest(store.getAllKeys())).map(k => k[0]))];
  return {
    kind: "indexeddb",
    run: (ops, blob) => new Promise(resolve => {
      const tx = db.transaction(STORE, "readwrite", { durability: "strict" });
      const store = tx.objectStore(STORE);
      tx.oncomplete = () => resolve({ error: null, nospace: false });
      tx.onabort = tx.onerror = () => resolve({ error: String(tx.error), nospace: tx.error?.name === "QuotaExceededError" });
      const step = async () => {
        for (const op of ops) {
          if (op.op === "write") {
            // A slice, since structured clone would otherwise keep the whole batch alive.
            store.put(blob.slice(op.off, op.off + op.len), [op.path, op.at]);
            if (op.len === 0) store.delete([op.path, op.at]);
          } else if (op.op === "setlen") {
            const whole = await assemble(store, op.path);
            store.delete(range(op.path));
            const kept = whole.slice(0, op.len);
            if (kept.length) store.put(kept, [op.path, 0]);
          } else if (op.op === "remove") {
            store.delete(range(op.path));
          }
        }
      };
      step().catch(() => { try { tx.abort(); } catch { /* already over */ } });
    }),
    tree: async () => {
      const store = db.transaction(STORE).objectStore(STORE);
      const files = [];
      for (const path of await allPaths(store)) {
        const skip = BIG_PREFIXES.some(p => path.startsWith(p));
        files.push([path, skip ? new Uint8Array(0) : await assemble(store, path)]);
      }
      return joinBlobs(files);
    },
    treeOf: async dir => {
      const store = db.transaction(STORE).objectStore(STORE);
      const files = [];
      for (const path of await allPaths(store)) if (path.startsWith(dir + "/")) files.push([path, await assemble(store, path)]);
      return joinBlobs(files);
    },
    read: async paths => {
      const store = db.transaction(STORE).objectStore(STORE);
      const files = [];
      for (const path of paths) {
        const bytes = await assemble(store, path);
        if (bytes.length || (await idbRequest(store.getAllKeys(range(path)))).length) files.push([path, bytes]);
      }
      return joinBlobs(files);
    },
    list: async dir => {
      const store = db.transaction(STORE).objectStore(STORE);
      const prefix = dir ? dir + "/" : "";
      return { paths: (await allPaths(store)).filter(p => p.startsWith(prefix)) };
    },
    wipe: async () => { const tx = db.transaction(STORE, "readwrite"); tx.objectStore(STORE).clear(); await new Promise(r => { tx.oncomplete = r; }); return {}; },
    kdf: params => Promise.resolve({ kek: sync_derive_kek(params.passphrase, params.salt, params.m_kib, params.t, params.p) }),
  };
}

async function existingIdbLog() {
  try {
    if (!indexedDB.databases) return false;
    return (await indexedDB.databases()).some(d => d.name === LOG_DB);
  } catch { return false; }
}

async function chooseHost() {
  // A log that is already in IndexedDB stays there. Moving between hosts is not built.
  if (await existingIdbLog()) return idbHost();
  let worker = null;
  try { worker = new Worker(new URL(import.meta.url), { type: "module" }); } catch { return idbHost(); }
  const host = opfsHost(worker);
  const failed = new Promise(resolve => { worker.onerror = () => resolve({ ready: false }); });
  const probe = await Promise.race([host.probe().catch(() => ({ ready: false })), failed]);
  if (probe.ready) return host;
  worker.terminate();
  return idbHost();
}

/* ---------- The old snapshot ---------- */
async function oldSnapshot() {
  const db = await idbOpen(OLD_DB, 1, d => d.createObjectStore(OLD_STORE));
  try { return (await idbRequest(db.transaction(OLD_STORE).objectStore(OLD_STORE).get(OLD_KEY))) || null; }
  finally { db.close(); }
}

async function sha256Hex(bytes) {
  return hexOf(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)));
}

/* ---------- Opens: which others are alive ---------- */
async function holdOpen(openId) {
  if (!navigator.locks) return [];
  const name = `casualnote-open-${openId}`;
  let held;
  const got = new Promise(r => { held = r; });
  navigator.locks.request(name, () => { held(); return new Promise(() => {}); });
  await got;
  const state = await navigator.locks.query();
  return (state.held || [])
    .map(l => /^casualnote-open-(\d+)$/.exec(l.name || ""))
    .filter(m => m && +m[1] !== openId)
    .map(m => +m[1]);
}

/* ---------- The store ---------- */
export async function openStore(snapshot, onState, onRemote) {
  const host = await chooseHost();
  const peer = crypto.getRandomValues(new BigUint64Array(1))[0] >> 1n;
  const openId = crypto.getRandomValues(new Uint32Array(1))[0];
  const live = await holdOpen(openId);
  const tree = await host.tree();
  const log = LogStore.open(JSON.stringify(tree.index), tree.blob, randomHex(16), openId, peer, BigInt(Date.now()), JSON.stringify(live));

  const channel = "BroadcastChannel" in self ? new BroadcastChannel("casualnote") : null;
  const metrics = { flushes: 0, frames: 0, bytes: 0, ms: 0, lastMs: 0 };
  let engine = null, dirty = false, pumping = null, retry = null, persisted = null, lastError = null, halted = false;
  const waiters = [];
  const settled = () => { for (const w of waiters.splice(0)) w(); };
  const report = () => {
    if (lastError) onState("failed");
    else if (dirty || !log.is_idle()) onState("saving");
    else onState("saved");
  };

  async function runBatch(json) {
    const { id, ops } = JSON.parse(json);
    const blob = log.batch_blob();
    const t0 = performance.now();
    const result = await host.run(ops, blob);
    const ms = performance.now() - t0;
    metrics.flushes++; metrics.bytes += blob.length; metrics.ms += ms; metrics.lastMs = ms;
    log.complete(id, result.error ?? undefined, result.nospace);
    if (result.error) {
      // Nothing was acknowledged, and the edits are still held. They go to a new file on the retry.
      lastError = result;
      console.warn("not saved", result.error);
      return false;
    }
    lastError = null;
    const updates = log.take_broadcast();
    if (updates && channel) channel.postMessage(updates.buffer, [updates.buffer]);
    return true;
  }

  function pump() {
    if (pumping) return pumping;
    pumping = (async () => {
      await null; // So that `pumping` is set before the finally below can clear it.
      try {
        for (;;) {
          if (halted) return;
          if (engine && dirty) { dirty = false; metrics.frames += engine.save_into(log, BigInt(Date.now())); }
          const json = log.next_batch();
          if (!json) { if (dirty && engine) continue; return; }
          if (!await runBatch(json)) {
            clearTimeout(retry);
            retry = setTimeout(() => { retry = null; pump(); }, RETRY_MS);
            return;
          }
        }
      } catch (err) {
        lastError = { error: String(err), nospace: false };
        console.error(err);
      } finally {
        pumping = null;
        report();
        settled();
      }
    })();
    return pumping;
  }
  const scheduleSave = (() => {
    let queued = false;
    return () => { if (queued) return; queued = true; setTimeout(() => { queued = false; pump(); }, 0); };
  })();

  // Recovery's repairs, then an old snapshot if there is one, then a guarded look at each new segment.
  await pump();
  let migrated = null;
  const old = await oldSnapshot();
  if (old) {
    const sha = await sha256Hex(old);
    const marker = log.local("migrated");
    if (!marker || new TextDecoder().decode(marker) !== sha) {
      // Safe to repeat: a merge of what is already there changes nothing.
      log.migrate(old, sha);
      await pump();
      if (log.state() === "failed") throw new Error("CN-STO-0010: the old notes could not be moved to the log");
      migrated = sha;
    }
  }
  for (let path = log.guard_next(); path; path = log.guard_next()) {
    await pump();
    log.guard_try();
  }
  log.guard_finish();
  await pump();

  const firstContainer = log.container() || null;
  let firstLoad = true;

  if (channel) channel.onmessage = e => {
    if (e.data === "seal") { log.seal_all(); pump(); return; }
    if (!(e.data instanceof ArrayBuffer)) return;
    const bytes = new Uint8Array(e.data);
    log.mark_durable(bytes); // Another tab saved these. They are not written a second time.
    onRemote(bytes);
  };
  addEventListener("pagehide", () => { pump(); });
  navigator.storage?.persist?.().then(v => { persisted = v; }).catch(() => { persisted = false; });

  const store = {
    kind: host.kind,
    host,
    log,
    metrics,
    get persisted() { return persisted; },
    get migrated() { return migrated; },
    get lastError() { return lastError; },
    get engine() { return engine; },
    /** What is on disk now. The first call hands over recovery's result. */
    async load() {
      if (firstLoad) { firstLoad = false; return firstContainer; }
      const t = await host.tree();
      return LogStore.read_only(JSON.stringify(t.index), t.blob, randomHex(16), JSON.stringify([openId, ...live])) || null;
    },
    /** Gives the store the engine it saves from. What load returned is already durable. */
    bind(e) {
      engine = e;
      if (firstContainer) log.mark_durable(firstContainer);
      log.release();
      if (dirty) scheduleSave();
    },
    changed() {
      dirty = true;
      onState("saving");
      scheduleSave();
    },
    /** Resolves once everything so far is durable, or has failed and is being retried. */
    flush() {
      if (engine) dirty = true;
      const p = new Promise(r => waiters.push(r));
      pump();
      return p.then(() => pumping ? pumping : undefined);
    },
    pump,
    kdf: params => host.kdf(params),
    /** Seals what this tab has written, and asks other tabs to do the same, so sync can carry it. */
    sealAll() { log.seal_all(); channel?.postMessage("seal"); return pump(); },
    /** Redraws from the engine by the app's careful path, which leaves a composition alone. */
    refreshView() { onRemote(new TextEncoder().encode("CNWS1")); },
    /** Tells other tabs about changes from elsewhere. */
    broadcast() {
      if (!channel || !engine) return;
      const bytes = engine.export();
      channel.postMessage(bytes.buffer, [bytes.buffer]);
    },
    /** Off the typing path. Replaces sealed segments with a snapshot, after checking it. */
    async compact({ min = 8, keep = [] } = {}) {
      const out = { documents: 0, removed: 0, before: 0, after: 0 };
      const work = async () => {
        for (const [name, dir] of JSON.parse(log.compaction_candidates(min))) {
          const files = await host.treeOf(dir);
          const plan = JSON.parse(log.compact(name, JSON.stringify(files.index), files.blob, JSON.stringify(keep), min));
          if (!plan) continue;
          out.documents++; out.removed += plan.removed; out.before += plan.before; out.after += plan.after;
          await pump();
        }
      };
      if (navigator.locks) await navigator.locks.request("casualnote-compact", { ifAvailable: true }, lock => lock ? work() : null);
      else await work();
      return out;
    },
    /** For tests: stop writing, so that the disk can be rearranged before a reload. */
    halt() { halted = true; },
  };
  void snapshot; // The log saves from the engine, by version. A whole snapshot is no longer written.
  return store;
}
