// The durable log and sync, through the real engine, the real worker and the real interface.
import { Engine, LogStore } from "../pkg/casual_note_wasm.js";
import { folderStorage, runRound, createFolder, inspectFolder, unlockFolder, beginWith, submitPassphrase, testHooks, settle } from "../features/sync.js";
import { idbHost } from "../store.js";

const PHASE = "cn-e2e-phase", SAVED = "cn-e2e-saved";
const FAST_KDF = { m_kib: 64 * 1024, t: 2, p: 1 };
const PASS = "パスフレーズ مرور correct horse";
const randomHex = n => [...crypto.getRandomValues(new Uint8Array(n))].map(b => b.toString(16).padStart(2, "0")).join("");
const newPeer = () => crypto.getRandomValues(new BigUint64Array(1))[0] >> 1n;
const newOpen = () => crypto.getRandomValues(new Uint32Array(1))[0];

function memoryHost() {
  const files = new Map();
  const blobOf = list => {
    let total = 0;
    const index = list.map(([p, b]) => { const at = total; total += b.length; return [p, at, b.length]; });
    const blob = new Uint8Array(total);
    list.forEach(([, b], i) => blob.set(b, index[i][1]));
    return { index, blob };
  };
  return {
    files,
    run(ops, blob) {
      for (const op of ops) {
        if (op.op === "write") {
          const old = files.get(op.path) || new Uint8Array(0);
          const next = new Uint8Array(Math.max(old.length, op.at + op.len));
          next.set(old); next.set(blob.subarray(op.off, op.off + op.len), op.at);
          files.set(op.path, next);
        } else if (op.op === "setlen") files.set(op.path, (files.get(op.path) || new Uint8Array(0)).slice(0, op.len));
        else if (op.op === "remove") files.delete(op.path);
      }
    },
    read: paths => blobOf(paths.filter(p => files.has(p)).map(p => [p, files.get(p)])),
    tree: () => blobOf([...files.entries()]),
  };
}

/** Another device, in this page: its own engine, its own log, on a memory disk. */
function makeDevice() {
  const host = memoryHost();
  const log = LogStore.open("[]", new Uint8Array(0), randomHex(16), newOpen(), newPeer(), BigInt(Date.now()), "[]");
  const engine = new Engine(newPeer());
  const pump = async () => {
    for (let j = log.next_batch(); j; j = log.next_batch()) {
      const { id, ops } = JSON.parse(j);
      host.run(ops, log.batch_blob());
      log.complete(id, undefined, false);
    }
    log.take_broadcast();
  };
  return { host, log, engine, pump, readLocal: async paths => host.read(paths), apply: op => { engine.apply(JSON.stringify(op)); engine.save_into(log, BigInt(Date.now())); } };
}

async function allFiles(dir, prefix = "") {
  const out = [];
  for await (const [name, h] of dir.entries()) {
    if (h.kind === "directory") out.push(...await allFiles(h, `${prefix}${name}/`));
    else out.push([`${prefix}${name}`, h, new Uint8Array(await (await h.getFile()).arrayBuffer())]);
  }
  return out;
}

const pageJson = (engine, id) => JSON.parse(engine.page(id));
const textOfPage = (engine, id) => pageJson(engine, id).blocks.map(b => b.runs.map(r => r.text).join("")).join("\n");

export async function run({ api, ok, wait, type, first, pageNow, engine, store, setCaret, $, $$ }) {
  testHooks.setAuto(false); // Rounds run only when a check asks, so each sees a known folder.
  const opfs = await navigator.storage.getDirectory();
  const folder = await opfs.getDirectoryHandle(`sync-test-${randomHex(4)}`, { create: true });
  const storage = folderStorage(folder);

  await ok("store: this browser uses private files through a worker, and frames reach per-document segments", async () => {
    await store.flush();
    const t = await store.host.list("");
    return store.kind === "opfs" && t.paths.some(p => p.startsWith("pages/") && p.endsWith(".seg")) && t.paths.some(p => p.startsWith("tree/log/")) && t.paths.includes("manifest.json");
  });

  // The cost of one keystroke, in this browser, each flushed on its own.
  api.edit({ op: "create_page", id: "cost-page", parent: null, title: "Cost" }, { op: "insert_block", page: "cost-page", after: null, block: { id: "cost", kind: "paragraph", checked: false, runs: [] } });
  await store.flush();
  const before = { ...store.metrics };
  const N = 60;
  const t0 = performance.now();
  for (let i = 0; i < N; i++) {
    api.engine.apply(JSON.stringify({ op: "splice_text", page: "cost-page", block: "cost", at: i, delete: 0, insert: "x" }));
    store.changed();
    await store.flush();
  }
  const perKey = (store.metrics.bytes - before.bytes) / N, flushMs = (store.metrics.ms - before.ms) / (store.metrics.flushes - before.flushes), allMs = (performance.now() - t0) / N;
  await ok(`store: write cost per keystroke, each flushed alone: ${Math.round(perKey)} bytes written, ${flushMs.toFixed(2)} ms in the worker, ${allMs.toFixed(2)} ms all told`, () => perKey < 400 && store.metrics.flushes - before.flushes <= N + 1);

  await ok("store: a burst of typing shares flushes, and saved shows only afterwards", async () => {
    const start = store.metrics.flushes;
    for (let i = 0; i < 40; i++) { api.engine.apply(JSON.stringify({ op: "splice_text", page: "cost-page", block: "cost", at: 0, delete: 0, insert: "y" })); store.changed(); }
    if ($("#saved").textContent === "Saved on this device") return false;
    await store.flush();
    return store.metrics.flushes - start <= 3 && $("#saved").textContent === "Saved on this device";
  });

  await ok("store: edits reloaded from the disk equal the page, read by a separate engine", async () => {
    const other = new Engine(newPeer());
    other.import(await store.load());
    return other.page("cost-page") === engine.page("cost-page");
  });

  await ok("store: compaction folds sealed segments into a checked snapshot, and nothing changes", async () => {
    for (let round = 0; round < 3; round++) {
      api.edit({ op: "splice_text", page: "cost-page", block: "cost", at: 0, delete: 0, insert: `r${round}` });
      await store.flush();
      await store.sealAll();
    }
    const want = engine.page("cost-page");
    const report = await store.compact({ min: 2 });
    await store.flush();
    const t = await store.host.list("");
    const other = new Engine(newPeer());
    other.import(await store.load());
    return report.documents >= 1 && report.removed >= 2 && t.paths.some(p => p.endsWith(".snap")) && other.page("cost-page") === want;
  });

  await ok("store: the IndexedDB fallback keeps records per frame, and reads back through the same log", async () => {
    const name = `casualnote-log-test-${randomHex(4)}`;
    const host = await idbHost(name);
    const open = async () => {
      const t = await host.tree();
      return LogStore.open(JSON.stringify(t.index), t.blob, "ab".repeat(16), newOpen(), newPeer(), 1n, "[]");
    };
    const drain = async log => {
      for (let j = log.next_batch(); j; j = log.next_batch()) {
        const { id, ops } = JSON.parse(j);
        const r = await host.run(ops, log.batch_blob());
        log.complete(id, r.error ?? undefined, r.nospace);
        if (r.error) throw new Error(r.error);
      }
    };
    const log = await open();
    await drain(log);
    const e = new Engine(newPeer());
    e.apply(JSON.stringify({ op: "create_page", id: "i", parent: null, title: "فهرس" }));
    e.apply(JSON.stringify({ op: "insert_block", page: "i", after: null, block: { id: "b", kind: "paragraph", checked: false, runs: [{ text: "one" }] } }));
    e.save_into(log, 1n); await drain(log);
    e.apply(JSON.stringify({ op: "splice_text", page: "i", block: "b", at: 3, delete: 0, insert: " two" }));
    e.save_into(log, 2n); await drain(log);
    const again = await open();
    await drain(again);
    const f = new Engine(newPeer());
    f.import(again.container());
    indexedDB.deleteDatabase(name);
    return host.kind === "indexeddb" && f.page("i") === e.page("i") && JSON.parse(f.page("i")).blocks[0].runs[0].text === "one two";
  });

  /* ---------- The panel, before anything is set up ---------- */
  $("#settingsBtn").click();
  await wait(10);
  await ok("sync panel: says plainly that the encryption has had no outside review, and offers a folder", () => {
    const text = $("#syncPanel").textContent;
    return text.includes("has not had any outside review") && !/\baudited\b|independently reviewed by|reviewed by experts/i.test(text) && !!$("#syncChoose");
  });
  await ok("sync panel: says what saved means on the web, without claiming more", () => {
    const t = $("#syncStorage").textContent;
    return t.includes("Saved means the browser accepted the flush") && t.includes("has not been measured");
  });

  /* ---------- Setting up, through the form ---------- */
  await beginWith(folder);
  await ok("sync setup: an empty folder is set up only after a passphrase, and nothing is written yet", async () => !!$("#syncPass") && !!$("#syncPass2") && (await allFiles(folder)).length === 0 && $("#syncPanel").textContent.includes("there is no recovery key yet"));
  await ok("sync setup: a short passphrase is refused, and says what to do", async () => {
    await submitPassphrase("short", "short", "Laptop", FAST_KDF);
    return $("#syncPanel .syncerror")?.textContent.includes("at least") && (await allFiles(folder)).length === 0;
  });
  await ok("sync setup: two different passphrases are refused", async () => {
    await submitPassphrase(PASS, PASS + "x", "Laptop", FAST_KDF);
    return $("#syncPanel .syncerror")?.textContent.includes("differ") && (await allFiles(folder)).length === 0;
  });
  const tKdf = performance.now();
  await submitPassphrase(PASS, PASS, "Laptop", FAST_KDF);
  const kdfMs = performance.now() - tKdf;
  const until = async (f, ms = 15000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f()) return true; await wait(30); } return !!(await f()); };
  await until(() => testHooks.live());
  await testHooks.syncNow();
  await ok(`sync setup: real Argon2id in the worker, ${Math.round(kdfMs)} ms in this development build at 64 MiB`, () => !!testHooks.live());

  const SECRET = "A-Secret-Title-秘密";
  await ok("sync: the folder holds ciphertext only, with no title and no text in any file or name", async () => {
    api.edit({ op: "create_page", id: "sy-page", parent: null, title: SECRET }, { op: "insert_block", page: "sy-page", after: null, block: { id: "sy1", kind: "paragraph", checked: false, runs: [{ text: "base" }] } });
    await testHooks.syncNow();
    const files = await allFiles(folder), needle = new TextEncoder().encode("A-Secret-Title");
    const leaked = files.some(([p, , b]) => p.includes("Secret") || b.some((_, i) => needle.every((n, k) => b[i + k] === n)));
    return files.some(([p]) => p.endsWith(".cnss")) && files.some(([p]) => p === "casualnote-sync.json") && !leaked;
  });
  $("#settingsBtn").click(); await wait(10);
  await ok("sync panel: with no other device, nothing claims another copy exists", () => $("#syncPanel").textContent.includes("no other copy is confirmed"));

  /* ---------- A second device ---------- */
  const b = makeDevice();
  await ok("sync join: a wrong passphrase is refused, with no key and no change", async () => {
    try { await unlockFolder({ storage, kdf: store.kdf, passphrase: "wrong passphrase, nearly" }); return false; }
    catch (e) { return e.message === "CN-CRY-0001"; }
  });
  const { root } = await inspectFolder(storage);
  const workspaceKey = await unlockFolder({ storage, kdf: store.kdf, passphrase: PASS.normalize("NFD") });
  await ok("sync join: the same passphrase in another Unicode form opens the folder", () => !!workspaceKey && !!root);
  const { SyncSession } = await import("../pkg/casual_note_wasm.js");
  b.sess = new SyncSession(workspaceKey, root.notebook, root.sync_id, b.log.device(), "Tablet", "test", undefined, 512);
  const roundB = () => runRound({ engine: b.engine, log: b.log, sess: b.sess, storage, pump: b.pump, readLocal: b.readLocal });
  await roundB();
  await ok("sync: the second device receives every script exactly", () => pageJson(b.engine, "sy-page").meta.title === SECRET && textOfPage(b.engine, "sy-page") === "base");

  await ok("sync panel: a stored file is not called seen until the other device confirms it", async () => {
    await testHooks.syncNow();
    api.edit({ op: "insert_block", page: "sy-page", after: "sy1", block: { id: "sy2", kind: "paragraph", checked: false, runs: [{ text: "second, 第二" }] } });
    await testHooks.syncNow();
    $("#settingsBtn").click(); await wait(10);
    const row = $(`#syncPanel .syncdevice[data-device="${b.log.device()}"]`);
    const unseen = !!row && row.textContent.includes("Not yet seen by this device");
    await roundB();
    await testHooks.syncNow();
    testHooks.paint();
    const after = $(`#syncPanel .syncdevice[data-device="${b.log.device()}"]`);
    return unseen && after.textContent.includes("Has confirmed everything") && $("#syncPanel").textContent.includes("Your other devices have confirmed everything") && textOfPage(b.engine, "sy-page").includes("第二");
  });

  await ok("sync: edits from the other device reach the page on view", async () => {
    b.apply({ op: "insert_block", page: "sy-page", after: "sy2", block: { id: "sy3", kind: "paragraph", checked: false, runs: [{ text: "from the tablet" }] } });
    await b.pump();
    await roundB();
    await testHooks.syncNow();
    api.openPage("sy-page");
    await wait(20);
    return pageNow().blocks.some(x => x.id === "sy3");
  });

  /* ---------- Tampering ---------- */
  await ok("sync: a tampered file is refused and visible, nothing from it is used, and its owner repairs it", async () => {
    b.apply({ op: "insert_block", page: "sy-page", after: "sy3", block: { id: "evil", kind: "paragraph", checked: false, runs: [{ text: "the tablet wrote this" }] } });
    await b.pump();
    await roundB();
    const seg = (await allFiles(folder)).filter(([p]) => p.includes(b.log.device()) && p.endsWith(".cnss")).sort((x, y) => (x[0] < y[0] ? 1 : -1))[0];
    const bad = seg[2].slice(); bad[bad.length - 40] ^= 1;
    const w = await seg[1].createWritable(); await w.write(bad); await w.close();
    await testHooks.syncNow();
    $("#settingsBtn").click(); await wait(10); testHooks.paint();
    const panel = $("#syncPanel").textContent;
    const refused = panel.includes("failed its check") && panel.includes("CN-SYN-0004") && !JSON.parse(engine.page("sy-page")).blocks.some(x => x.id === "evil");
    await roundB(); // The tablet reads its own file back and puts its bytes back.
    await roundB();
    await testHooks.syncNow();
    const panel2 = $("#syncPanel").textContent;
    return refused && JSON.parse(engine.page("sy-page")).blocks.some(x => x.id === "evil") && !panel2.includes("CN-SYN-0004");
  });
  await ok("sync: a truncated file is refused the same way", async () => {
    b.apply({ op: "insert_block", page: "sy-page", after: "evil", block: { id: "cut", kind: "paragraph", checked: false, runs: [{ text: "cut short" }] } });
    await b.pump();
    await roundB();
    const seg = (await allFiles(folder)).filter(([p]) => p.includes(b.log.device()) && p.endsWith(".cnss")).sort((x, y) => (x[0] < y[0] ? 1 : -1))[0];
    const w = await seg[1].createWritable(); await w.write(seg[2].slice(0, seg[2].length >> 1)); await w.close();
    await testHooks.syncNow();
    testHooks.paint();
    const refused = $("#syncPanel").textContent.includes("failed its check") && !JSON.parse(engine.page("sy-page")).blocks.some(x => x.id === "cut");
    await roundB(); await roundB();
    await testHooks.syncNow();
    return refused && JSON.parse(engine.page("sy-page")).blocks.some(x => x.id === "cut");
  });

  /* ---------- Merge review ---------- */
  const review = () => { testHooks.paint(); return $$("#syncReviews .syncreview"); };
  const syncBoth = async () => { await testHooks.syncNow(); await roundB(); await testHooks.syncNow(); await roundB(); };
  await ok("sync review: two devices changing one block are marked, with each version and a choice", async () => {
    await syncBoth();
    api.edit({ op: "splice_text", page: "sy-page", block: "sy1", at: 4, delete: 0, insert: " from laptop" });
    await store.flush();
    b.apply({ op: "splice_text", page: "sy-page", block: "sy1", at: 4, delete: 0, insert: " from tablet" });
    await b.pump();
    await roundB();
    await testHooks.syncNow();
    $("#settingsBtn").click(); await wait(10);
    const cards = review();
    if (cards.length !== 1) return false;
    const versions = [...cards[0].querySelectorAll(".syncversion p")].map(p => p.textContent);
    const merged = textOfPage(engine, "sy-page");
    return versions[0] === "base from laptop" && versions[1] === "base from tablet" && versions[2].includes("laptop") && versions[2].includes("tablet") && merged.includes("laptop") && merged.includes("tablet") && cards[0].querySelectorAll("button.btn").length === 3;
  });
  await ok("sync review: the block is marked on the page, and the side panel lists it", async () => {
    api.openPage("sy-page"); await wait(30);
    testHooks.paintMarks("sy-page");
    return !!$('.b.syncmarked[data-id="sy1"]') && !!$("#rpSync button");
  });
  await ok("sync review: one click keeps a version, and it clears on the other device", async () => {
    $("#settingsBtn").click(); await wait(10);
    const card = review()[0];
    [...card.querySelectorAll(".syncversion")][1].querySelector("button").click();
    await wait(10);
    await store.flush();
    const kept = JSON.parse(engine.page("sy-page")).blocks.find(x => x.id === "sy1").runs.map(r => r.text).join("");
    testHooks.paint();
    const gone = !$("#syncReviews");
    await syncBoth();
    const onTablet = JSON.parse(b.engine.sync_status(b.sess)).reviews;
    return kept === "base from tablet" && gone && onTablet.length === 0 && textOfPage(b.engine, "sy-page").split("\n")[0] === "base from tablet";
  });
  await ok("sync review: keeping the merge changes nothing and clears it", async () => {
    api.edit({ op: "splice_text", page: "sy-page", block: "sy2", at: 0, delete: 0, insert: "L " });
    await store.flush();
    b.apply({ op: "splice_text", page: "sy-page", block: "sy2", at: 0, delete: 0, insert: "T " });
    await b.pump();
    await roundB();
    await testHooks.syncNow();
    const status = JSON.parse(engine.sync_status(testHooks.live().sess));
    const before = engine.page("sy-page");
    settle(status.reviews[0], "merged");
    return status.reviews.length === 1 && JSON.parse(engine.page("sy-page")).blocks.length === JSON.parse(before).blocks.length && JSON.parse(engine.sync_status(testHooks.live().sess)).reviews.length === 0;
  });

  /* ---------- Holding back, and removal ---------- */
  await ok("sync panel: a device that has not confirmed holds files back, by name, and removal is a person's act", async () => {
    await testHooks.syncNow();
    api.edit({ op: "insert_block", page: "sy-page", after: "sy1", block: { id: "sy9", kind: "paragraph", checked: false, runs: [{ text: "not yet seen" }] } });
    await testHooks.syncNow();
    $("#settingsBtn").click(); await wait(10); testHooks.paint();
    const held = $("#syncHeld")?.textContent.includes("Tablet") && $("#syncHeld").textContent.includes("No timer");
    const row = $(`#syncPanel .syncdevice[data-device="${b.log.device()}"]`);
    [...row.querySelectorAll("button")].find(x => x.textContent.includes("Remove")).click();
    await wait(10);
    const dialog = $(".modal.pkgmodal")?.textContent || "";
    $("#syncRemoveConfirm").click();
    await testHooks.syncNow();
    const status = JSON.parse(engine.sync_status(testHooks.live().sess));
    return held && dialog.includes("change the storage's password") && status.devices.find(d => d.id === b.log.device()).removed;
  });

  await ok("sync panel: a missing folder file changes nothing, and says what to do", async () => {
    const pages = engine.pages();
    await folder.removeEntry("casualnote-sync.json");
    await testHooks.syncNow();
    testHooks.paint();
    const t = $("#syncPanel").textContent;
    return t.includes("no longer holds the sync file") && t.includes("Nothing has changed on this device") && engine.pages() === pages;
  });

  await ok("sync panel: no style attributes, and the panel's sheet uses logical properties", async () => {
    const css = await (await fetch("features/sync.css")).text();
    return !$("#syncPanel").querySelector("[style]") && !/(margin|padding|border)-(left|right)|text-align: *(left|right)|\b(left|right): /.test(css);
  });

  testHooks.forget();
  testHooks.setAuto(true);
  await opfs.removeEntry(folder.name, { recursive: true });
  void first; void type; void setCaret;
}

/* ---------- Across real reloads ---------- */
const OLD_DB = "casualnote", OLD_STORE = "workspace", OLD_KEY = "snapshot";
function oldStore(mode, f) {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(OLD_DB, 1);
    open.onupgradeneeded = () => open.result.createObjectStore(OLD_STORE);
    open.onerror = () => reject(open.error);
    open.onsuccess = () => {
      const tx = open.result.transaction(OLD_STORE, mode);
      const r = f(tx.objectStore(OLD_STORE));
      tx.oncomplete = () => { open.result.close(); resolve(r?.result); };
      tx.onerror = () => reject(tx.error);
    };
  });
}

function everyPage(engine) {
  const out = {};
  for (const p of JSON.parse(engine.pages())) out[p.id] = engine.page(p.id);
  return out;
}

function saved() { return JSON.parse(sessionStorage.getItem(SAVED) || "{}"); }
function advance(phase, data) { sessionStorage.setItem(SAVED, JSON.stringify(data)); sessionStorage.setItem(PHASE, phase); location.reload(); }

/** At the end of the suite: remember every page, and reload for real. */
export async function reloadChecks({ out, engine, store }) {
  await store.flush();
  advance("reload", { out, pages: everyPage(engine), databases: engine.databases() });
  return true;
}

function finish(out) {
  sessionStorage.removeItem(PHASE);
  sessionStorage.removeItem(SAVED);
  document.body.setAttribute("data-e2e", out.join("\n"));
}

export async function resumeAfterReload({ engine, store, $, api, wait }) {
  const phase = sessionStorage.getItem(PHASE);
  if (!phase) return false;
  const data = saved();
  const out = data.out;
  const check = async (name, f) => { try { out.push(((await f()) === false ? "FAIL " : "ok ") + name); } catch (e) { out.push("THROW " + name + ": " + (e.message || e)); } };

  if (phase === "reload") {
    await check("reload: after a real reload, all notes are there, block for block", () => {
      const now = everyPage(engine), ids = Object.keys(data.pages);
      return ids.length > 10 && ids.every(id => now[id] === data.pages[id]) && engine.databases() === data.databases;
    });
    await check("reload: a new open writes to a new segment", async () => {
      const before = (await store.host.list("tree/log")).paths.length;
      api.edit({ op: "create_page", id: "after-reload", parent: null, title: "After reload" });
      await store.flush();
      return (await store.host.list("tree/log")).paths.length === before + 1;
    });
    await check("reload: the passphrase and key are not kept, and sync says it is locked", async () => {
      $("#settingsBtn").click(); await wait(10);
      const panel = $("#syncPanel").textContent;
      return !!$("#syncUnlock") && panel.includes("Sync is locked");
    });
    // Now the other way in: an old-format snapshot and no log at all.
    const old = new Engine(newPeer());
    old.apply(JSON.stringify({ op: "create_page", id: "old-page", parent: null, title: "ملاحظة قديمة 古いメモ" }));
    old.apply(JSON.stringify({ op: "insert_block", page: "old-page", after: null, block: { id: "o1", kind: "paragraph", checked: false, runs: [{ text: "written before the log existed, 日本語" }] } }));
    old.apply(JSON.stringify({ op: "create_database", database: { id: "old-db", name: "Tasks", view: "table", statuses: [{ id: "s", name: "Open", group: "todo" }], columns: [], rows: [] } }));
    const bytes = old.export();
    store.halt();
    await store.host.wipe();
    await oldStore("readwrite", s => s.put(bytes, OLD_KEY));
    advance("migrated", { out, oldPage: old.page("old-page"), oldDb: old.database("old-db") });
    return true;
  }
  if (phase === "migrated") {
    await check("migrate: an old snapshot, with no log, opens with every page and database", () => engine.page("old-page") === data.oldPage && engine.database("old-db") === data.oldDb && !!store.migrated);
    await check("migrate: its contents are now in the log as snapshots, and the old copy is untouched", async () => {
      const paths = (await store.host.list("")).paths;
      return paths.some(p => p.startsWith("pages/") && p.endsWith(".snap")) && paths.some(p => p.startsWith("databases/") && p.endsWith(".snap")) && !!(await oldStore("readonly", s => s.get(OLD_KEY)));
    });
    await check("migrate: an edit after migration saves to the log", async () => {
      api.edit({ op: "splice_text", page: "old-page", block: "o1", at: 0, delete: 0, insert: "edited after, " });
      await store.flush();
      return $("#saved").textContent === "Saved on this device";
    });
    advance("migrated-again", { out, oldPage: engine.page("old-page") });
    return true;
  }
  if (phase === "migrated-again") {
    await check("migrate: the next open does not migrate twice, and the edit is there", () => !store.migrated && engine.page("old-page") === data.oldPage);
    // The final proof: the log alone is enough.
    await oldStore("readwrite", s => s.delete(OLD_KEY));
    advance("log-only", { out, oldPage: data.oldPage });
    return true;
  }
  if (phase === "log-only") {
    await check("migrate: with the old snapshot gone, every note still loads from the log alone", () => engine.page("old-page") === data.oldPage);
    finish(out);
    return true;
  }
  return false;
}
