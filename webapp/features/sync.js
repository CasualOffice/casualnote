// Encrypted sync through a folder, and the panel that says how it stands (docs/56). The engine does
// the deciding. This file carries its requests to the folder, and shows its status. A panel line
// never says more than the status does: stored in the folder is not the same as seen by a device.
import { SyncSession, sync_new_folder, sync_read_root, sync_record_params, sync_unwrap_key } from "../pkg/casual_note_wasm.js";

const ROOT_KEY = "casualnote-sync.json", CONFIG = "sync-config", STATE = "sync";
const HANDLE_DB = "casualnote-sync", HANDLE_STORE = "folder", HANDLE_KEY = "folder";
const DEFAULT_KDF = { m_kib: 128 * 1024, t: 3, p: 1 };
const MIN_PASSPHRASE = 12, MAX_KEY_RECORDS = 8;
const POLL_FIRST = 30_000, POLL_MOST = 15 * 60_000, AFTER_EDIT = 5_000;

let api = null, section = null;
// What this tab has unlocked. The key stays in memory only, and is gone when the page goes.
let live = null;
let flow = null; // The setup under way: { step, dir, error, busy }.
let auto = true, lastError = null, roundBusy = false, pollDelay = POLL_FIRST, pollTimer = null, editTimer = null;

const randomHex = n => [...crypto.getRandomValues(new Uint8Array(n))].map(b => b.toString(16).padStart(2, "0")).join("");
const utf8 = s => new TextEncoder().encode(s);

/* ---------- A folder, through four verbs ---------- */
function failureKind(e) {
  if (e?.name === "NotAllowedError" || e?.name === "SecurityError") return "denied";
  if (e?.name === "QuotaExceededError") return "full";
  if (e?.name === "NotFoundError") return "unreachable";
  return "transient";
}

/** Any directory handle: a folder a person chose, or an origin-private one. */
export function folderStorage(root) {
  const parts = key => { const p = key.split("/"); return [p.slice(0, -1), p.at(-1)]; };
  async function dirAt(names, create) {
    let d = root;
    for (const n of names) d = await d.getDirectoryHandle(n, { create });
    return d;
  }
  async function walk(dir, prefix, out) {
    for await (const [name, h] of dir.entries()) {
      const key = `${prefix}${name}`;
      if (h.kind === "directory") await walk(h, `${key}/`, out);
      else out.push({ key, len: (await h.getFile()).size });
    }
  }
  return {
    async list(prefix) {
      const names = prefix.split("/").filter(Boolean);
      const out = [];
      try {
        await root.getDirectoryHandle(names[0] ?? "d").catch(e => { if (e.name !== "NotFoundError") throw e; });
        await walk(await dirAt(names, false), prefix, out);
      } catch (e) { if (e.name !== "NotFoundError") throw e; }
      return out;
    },
    async get(key, maxLen) {
      const [dirs, name] = parts(key);
      try {
        const file = await (await (await dirAt(dirs, false)).getFileHandle(name)).getFile();
        if (maxLen && file.size > maxLen) return { kind: "toolarge" };
        return { kind: "got", bytes: new Uint8Array(await file.arrayBuffer()) };
      } catch (e) {
        return e.name === "NotFoundError" ? { kind: "missing" } : { kind: failureKind(e) };
      }
    },
    async put(key, body) {
      const [dirs, name] = parts(key);
      try {
        const dir = await dirAt(dirs, true);
        // Not atomic across computers. Only this device writes this name, and the hash chain and
        // the read-back notice the rest.
        try { await dir.getFileHandle(name); return { kind: "exists" }; } catch (e) { if (e.name !== "NotFoundError") throw e; }
        const w = await (await dir.getFileHandle(name, { create: true })).createWritable();
        await w.write(body);
        await w.close();
        return { kind: "created" };
      } catch (e) { return { kind: failureKind(e) }; }
    },
    async remove(key) {
      const [dirs, name] = parts(key);
      try { await (await dirAt(dirs, false)).removeEntry(name); return { kind: "deleted" }; }
      catch (e) { return e.name === "NotFoundError" ? { kind: "deleted" } : { kind: failureKind(e) }; }
    },
  };
}

/** Runs one round. The engine says what to do, and after each step the log is flushed before the
 * next, which is what keeps a put behind its outbox write and an ack behind its import. */
export async function runRound({ engine, log, sess, storage, pump, readLocal }) {
  let changed = false;
  sess.begin_round();
  for (let guard = 0; guard < 500; guard++) {
    const step = JSON.parse(engine.sync_step(log, sess, BigInt(Date.now())));
    await pump();
    changed ||= step.changed;
    for (const r of step.requests) {
      if (r.op === "read_local") {
        const files = await readLocal(r.paths);
        sess.respond_files(r.id, JSON.stringify(files.index), files.blob);
        continue;
      }
      let out;
      try {
        if (r.op === "list") out = { kind: "listed", listed: JSON.stringify(await storage.list(r.prefix)) };
        else if (r.op === "get") out = await storage.get(r.key, r.max_len);
        else if (r.op === "put") out = await storage.put(r.key, sess.take_body(r.id));
        else out = await storage.remove(r.key);
      } catch (e) { out = { kind: failureKind(e) }; }
      sess.respond(r.id, out.kind, out.bytes, out.listed);
    }
    if (step.done) break;
  }
  await pump();
  return changed;
}

/** Makes a new sync folder. Writes nothing into the folder until the key is derived. */
export async function createFolder({ storage, kdf, device, passphrase, params = DEFAULT_KDF }) {
  const salt = randomHex(32);
  const { kek } = await kdf({ passphrase, salt, ...params });
  const made = JSON.parse(sync_new_folder(kek, salt, params.m_kib, params.t, params.p, device));
  const root = await storage.put(ROOT_KEY, utf8(made.root));
  if (root.kind !== "created") throw new Error("CN-SYN-0002: this folder could not be written");
  const rec = await storage.put(made.record_key, utf8(made.record));
  if (rec.kind !== "created") throw new Error("CN-SYN-0002: this folder could not be written");
  return { workspace: made.workspace, notebook: made.notebook, sync_id: made.sync_id };
}

/** Reads what a folder holds: `{ root }` if it is a sync folder, and `{}` if it is not yet one. */
export async function inspectFolder(storage) {
  const got = await storage.get(ROOT_KEY, 64 * 1024);
  if (got.kind === "missing") return {};
  if (got.kind !== "got") throw new Error("CN-SYN-0001: this folder could not be read");
  const root = sync_read_root(got.bytes);
  if (!root) throw new Error("CN-SYN-0003: this folder holds a sync file that cannot be read");
  return { root: JSON.parse(root) };
}

/** Opens the workspace key with a passphrase. Cost parameters from storage are checked first. */
export async function unlockFolder({ storage, kdf, passphrase }) {
  const entries = (await storage.list("d/")).filter(e => e.key.endsWith(".cnsk") && e.len < 64 * 1024);
  if (!entries.length) throw new Error("CN-CRY-0002");
  const tried = new Set();
  for (const e of entries.sort((a, b) => (a.key < b.key ? 1 : -1)).slice(0, MAX_KEY_RECORDS)) {
    const got = await storage.get(e.key, 64 * 1024);
    if (got.kind !== "got") continue;
    const record = new TextDecoder().decode(got.bytes);
    let params;
    try { params = JSON.parse(sync_record_params(record)); } catch { continue; }
    if (tried.has(params.salt)) continue;
    tried.add(params.salt);
    const { kek } = await kdf({ passphrase, ...params });
    const workspace = sync_unwrap_key(record, kek);
    if (workspace) return workspace;
  }
  throw new Error("CN-CRY-0001");
}

/* ---------- Where the folder handle is kept ---------- */
function handleDb() {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(HANDLE_DB, 1);
    open.onupgradeneeded = () => open.result.createObjectStore(HANDLE_STORE);
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
}
async function saveHandle(handle) {
  const db = await handleDb();
  await new Promise((resolve, reject) => {
    const tx = db.transaction(HANDLE_STORE, "readwrite");
    tx.objectStore(HANDLE_STORE).put(handle, HANDLE_KEY);
    tx.oncomplete = resolve; tx.onerror = () => reject(tx.error);
  });
  db.close();
}
async function loadHandle() {
  const db = await handleDb();
  try {
    return await new Promise((resolve, reject) => {
      const r = db.transaction(HANDLE_STORE).objectStore(HANDLE_STORE).get(HANDLE_KEY);
      r.onsuccess = () => resolve(r.result || null); r.onerror = () => reject(r.error);
    });
  } finally { db.close(); }
}

/* ---------- Connecting this tab ---------- */
const deviceName = () => { try { return localStorage.getItem("casualnote-device-name") || ""; } catch { return ""; } };
const defaultName = () => navigator.userAgentData?.platform || navigator.platform || api.t("This device");
const platform = () => navigator.userAgentData?.platform || navigator.platform || "";

function savedConfig() {
  const b = api.store.log?.local(CONFIG);
  if (!b) return null;
  try { return JSON.parse(new TextDecoder().decode(b)); } catch { return null; }
}

function connect(handle, config, workspace) {
  const log = api.store.log;
  const saved = log.local(STATE) ?? undefined;
  const sess = new SyncSession(workspace, config.notebook, config.sync_id, log.device(), deviceName() || defaultName(), platform(), saved, 512);
  live = { sess, storage: folderStorage(handle), handle, config };
  syncNow();
}

async function readLocal(paths) {
  return api.store.host.read(paths);
}

let running = null;
/** One round. Asked for during a round, it runs once more afterwards, so nothing just typed waits. */
function syncNow() {
  if (!live) return Promise.resolve();
  if (running) return running.then(syncNow);
  running = oneRound().finally(() => { running = null; });
  return running;
}

async function oneRound() {
  roundBusy = true; lastError = null;
  paint();
  const round = async () => {
    await api.store.sealAll();
    const changed = await runRound({ engine: api.engine, log: api.store.log, sess: live.sess, storage: live.storage, pump: api.store.pump, readLocal });
    if (changed) {
      api.store.broadcast();
      api.store.refreshView();
      pollDelay = POLL_FIRST;
    }
  };
  try {
    // One tab syncs at a time. The others keep writing their own segments, and say so.
    if (navigator.locks) await navigator.locks.request("casualnote-sync-leader", { ifAvailable: true }, lock => { if (lock) return round(); lastError = "another-tab"; });
    else await round();
  } catch (err) {
    console.warn("sync", err);
    lastError = String(err?.message || err);
  }
  roundBusy = false;
  paint();
  paintMarks();
  schedulePoll();
}

function schedulePoll() {
  clearTimeout(pollTimer);
  if (!live || !auto) return;
  pollTimer = setTimeout(() => { pollDelay = Math.min(pollDelay * 2, POLL_MOST); syncNow(); }, pollDelay);
}

function afterEdit() {
  if (!live || !auto) return;
  clearTimeout(editTimer);
  editTimer = setTimeout(() => { pollDelay = POLL_FIRST; syncNow(); }, AFTER_EDIT);
}

/* ---------- Words ---------- */
const FAILURE_WORDS = {
  "CN-SYN-0001": "A file could not be reached. It is tried again on the next sync.",
  "CN-SYN-0004": "A file failed its check and was refused. Nothing from it was used. The device that wrote it puts it back when it next syncs. If that device is gone, remove it here.",
  "CN-SYN-0006": "Two different files claim the same place. A copy of this device, or a restored backup, may be running. Nothing from the second file was used.",
  "CN-SYN-0007": "This file was written by a newer version. Update this app to read it.",
  "CN-SYN-0008": "A file is over the size limit and was not read.",
  "CN-SYN-0009": "A file was refused while merging it. Its bytes are kept on this device, and the other documents are unaffected.",
  "CN-CRY-0005": "A file is sealed under a key this device does not hold. Enter the passphrase again.",
};
const ROOT_WORDS = {
  unreachable: "The folder cannot be reached. Check that the drive is connected, then sync again. Nothing has changed on this device.",
  root_missing: "This folder no longer holds the sync file. The drive may be unmounted or a storage app signed out. Nothing has changed on this device. Reconnect it, or choose another folder.",
  wrong_root: "This folder belongs to a different sync setup. Nothing was written to it. Choose the right folder.",
  auth_failed: "The browser or the folder refused access. Reconnect the folder and allow it.",
  quota_full: "The folder is full. Free some space there, then sync again. Your notes on this device are safe.",
  forked: "Another copy of this device is writing to the folder, perhaps from a restored backup. This device has stopped writing there, and still reads. Check your other devices. Moving to a new device identity is not built yet.",
  local_failed: "This device could not save, so nothing was sent. See the message at the top of the page.",
};
const CRYPTO_WORDS = {
  "CN-CRY-0001": "That passphrase does not open this folder, or its key file is damaged. Check the passphrase and try again.",
  "CN-CRY-0002": "This folder has no key file, so it cannot be opened. Choose another folder.",
};

function when(ms) {
  if (ms == null) return api.t("not yet");
  return new Intl.DateTimeFormat(api.lang, { dateStyle: "medium", timeStyle: "short" }).format(new Date(Number(ms)));
}
const count = n => new Intl.NumberFormat(api.lang).format(n);
function bytesText(n) {
  const f = new Intl.NumberFormat(api.lang, { maximumFractionDigits: 1 });
  if (n < 1024) return api.tf("{n} bytes", { n: f.format(n) });
  if (n < 1024 * 1024) return api.tf("{n} KB", { n: f.format(n / 1024) });
  return api.tf("{n} MB", { n: f.format(n / 1024 / 1024) });
}
const errorWords = message => {
  const code = /CN-[A-Z]+-\d{4}/.exec(message)?.[0];
  return api.t(CRYPTO_WORDS[code] || FAILURE_WORDS[code] || "That did not work. Your notes on this device are unaffected. Try again.");
};

/* ---------- The panel ---------- */
const { el, btn, stateLine } = (() => {
  const el = (...a) => api.el(...a);
  const btn = (text, onclick, opts = {}) => el("button", { class: `btn${opts.primary ? " primary" : ""}`, onclick, ...(opts.id ? { id: opts.id } : {}), ...(opts.disabled ? { disabled: true } : {}) }, opts.icon ? api.icon(opts.icon) : null, el("span", { text }));
  const stateLine = (text, kind = "") => el("p", { class: `syncstate${kind === "bad" ? " bad" : ""}`, ...(kind === "bad" ? { role: "alert" } : {}) }, api.icon(kind === "bad" ? "warning" : kind === "ok" ? "check-circle" : "circle-dashed"), el("span", { text }));
  return { el, btn, stateLine };
})();

function honesty() {
  return el("p", { class: "syncnote", id: "syncHonesty", text: api.t("Notes are encrypted on this device before they reach the folder. This encryption has not had any outside review, so do not rely on it to keep something secret from a person who can read that folder.") });
}
function visibleTo() {
  return el("p", { class: "syncnote", text: api.t("Whoever can read the folder can see file sizes, how many devices you use and when they write. They cannot see titles, text or structure. A removed device that kept access to the folder can still list and delete files there.") });
}

function storageFacts() {
  const s = api.store;
  if (!s.log) return el("p", { class: "syncstate bad", text: api.t("This session is not being saved on this device.") });
  const where = s.kind === "opfs" ? "Saved in this browser's private files." : "Saved in this browser's database, because private files are not available here.";
  const held = s.persisted === false ? " The browser has not promised to keep this storage, so export or sync regularly." : "";
  return el("p", { class: "syncnote", id: "syncStorage", text: api.t(where + held) + " " + api.t("Saved means the browser accepted the flush. Whether that survives a power cut depends on the browser, and has not been measured.") });
}

function restorePanel() {
  const log = api.store.log;
  if (!log) return null;
  const kept = JSON.parse(log.quarantine()), blocked = JSON.parse(log.blocked()), issues = JSON.parse(log.issues()).filter(i => i.code !== "CN-STO-0012");
  if (!kept.length && !Object.keys(blocked).length && !issues.length) return null;
  const box = el("div", { class: "syncsec", id: "syncRestore" }, el("h3", { text: api.t("Set aside on this device") }));
  for (const [doc, path] of Object.entries(blocked)) {
    box.append(stateLine(api.tf("A saved change to ⁨{doc}⁩ keeps failing to load. This page shows what came before it. The change is kept here, and has not been moved or deleted.", { doc: api.metaOf(doc)?.title || api.t("Untitled") }), "bad"),
      el("div", { class: "syncactions" }, btn(api.t("Save the kept copy"), () => saveKept(path), { icon: "download-simple" })));
  }
  for (const i of issues) if (!blocked[i.doc]) box.append(el("p", { class: "syncnote" }, el("span", { text: api.t(i.code === "CN-STO-0005" ? "Part of a saved file was damaged. What came before the damage was kept, and the whole file was set aside." : "A saved file needed attention when opening.") + " " }), el("code", { dir: "ltr", text: i.code })));
  if (kept.length) {
    box.append(el("p", { class: "syncnote", text: api.tf("Files set aside rather than deleted: {n}. Most are the unfinished end of a write that was never shown as saved.", { n: count(kept.length) }) }));
    box.append(el("div", { class: "syncactions" }, ...kept.slice(0, 20).map((p, i) => btn(api.tf("Save file {n}", { n: count(i + 1) }), () => saveKept(p), { icon: "download-simple" }))));
  }
  return box;
}

async function saveKept(path) {
  const files = await api.store.host.read([path]);
  if (!files.index.length) { api.toast(api.t("That file is no longer here.")); return; }
  api.saveFile(path.split("/").at(-1), files.blob, "application/octet-stream");
}

function failureLine(f) {
  return el("li", { class: "bad" }, el("span", { text: api.t(FAILURE_WORDS[f.code] || "A file could not be used, and is kept.") + " " }),
    el("code", { dir: "ltr", text: `${f.code} ${api.tf("attempts: {n}", { n: count(f.attempts) })}` }));
}

function deviceRow(d, status) {
  const name = d.this_device ? api.tf("{name} (this device)", { name: d.name || defaultName() }) : d.name || api.t("A device that has not named itself yet");
  const facts = el("ul", { class: "syncfacts" });
  const line = text => facts.append(el("li", { text }));
  if (d.this_device) {
    line(d.not_written ? api.tf("Files not yet in the folder: {n}. They are kept here and tried again.", { n: count(d.not_written) }) : api.t("Everything sealed so far is in the folder. That alone does not mean another device has seen it."));
    if (status.devices.length > 1) line(d.not_seen ? api.tf("Files in the folder and not yet seen by every other device: {n}.", { n: count(d.not_seen) }) : api.t("Every other device has confirmed what is in the folder."));
  } else {
    line(d.not_seen ? api.tf("Not yet seen by this device, of the files sent from here: {n}.", { n: count(d.not_seen) }) : api.t("Has confirmed everything this device has put in the folder."));
    if (d.waiting_from) line(api.tf("Files still to fetch from it: {n}.", { n: count(d.waiting_from) }));
  }
  if (d.restorable_objects) line(api.tf("Files that could be restored from here if it lost them: {n}, {size}.", { n: count(d.restorable_objects), size: bytesText(d.restorable_bytes) }));
  for (const f of d.failed) facts.append(failureLine(f));
  for (const r of d.rejected) facts.append(el("li", { class: "bad", dir: "ltr", text: r }));
  const row = el("div", { class: "syncdevice", "data-device": d.id },
    el("header", {}, el("b", { dir: "auto", text: name }), el("small", { text: d.this_device ? api.tf("Last sync: {when}", { when: when(status.last_round_ms) }) : d.removed ? api.t("Removed") : api.tf("Last heard from: {when}", { when: when(d.observed_ms) }) })),
    facts);
  if (!d.this_device && !d.removed) row.append(el("div", { class: "syncactions" }, btn(api.t("Remove this device"), () => confirmRemove(d, status), {})));
  return row;
}

function confirmRemove(d, status) {
  const name = d.name || api.t("this device");
  const m = modal(api.tf("Remove ⁨{name}⁩?", { name }));
  m.body.append(
    el("p", { text: api.t("Do this only if the device is gone. Its files in the folder stay until they have been covered, and it no longer holds back clean-up. Anything it never put in the folder cannot be reached from here.") }),
    el("p", { class: "pkgwarn", text: api.t("Access to the folder is not ours to take away. If the device was lost or stolen, change the storage's password too. Changing the encryption key is not built yet.") }));
  void status;
  m.foot.append(btn(api.t("Cancel"), m.close), btn(api.t("Remove device"), () => { live.sess.remove_device(d.id); m.close(); syncNow(); }, { primary: true, id: "syncRemoveConfirm" }));
  m.foot.lastChild.focus();
}

function connectedPanel(box) {
  const status = JSON.parse(api.engine.sync_status(live.sess));
  if (status.root !== "connected") box.append(stateLine(api.t(ROOT_WORDS[status.root] || "The folder needs attention. Nothing has changed on this device."), "bad"));
  else if (lastError === "another-tab") box.append(stateLine(api.t("Another tab of this browser is syncing. This tab's edits are saved, and go out with that tab.")));
  else if (lastError) box.append(stateLine(api.t("The last sync did not finish, and is tried again shortly. Your notes on this device are unaffected."), "bad"));
  else if (roundBusy) box.append(stateLine(api.t("Syncing")));
  else {
    const others = status.devices.filter(d => !d.this_device && !d.removed);
    const self = status.devices.find(d => d.this_device);
    if (!others.length) box.append(stateLine(api.t("In the folder. No other device has joined yet, so no other copy is confirmed."), "ok"));
    else if (self.not_written || self.not_seen) box.append(stateLine(api.t("Some of this device's files are not yet confirmed by your other devices. The list below says which.")));
    else box.append(stateLine(api.t("Your other devices have confirmed everything this device has sent."), "ok"));
  }
  const summary = status.last_round;
  if (summary.refused) box.append(stateLine(api.tf("Files refused in the last sync because they failed their checks: {n}. Nothing from them was used.", { n: count(summary.refused) }), "bad"));
  box.append(el("div", { class: "syncactions" }, btn(api.t("Sync now"), syncNow, { id: "syncNow", icon: "arrow-clockwise", disabled: roundBusy })));
  if (status.held_back) {
    const who = status.devices.find(d => d.id === status.held_back);
    box.append(el("p", { class: "syncnote", id: "syncHeld", text: api.tf("Files ⁨{name}⁩ has not confirmed: {n}. They stay in the folder until it has seen them, or until you remove that device. No timer does this.", { name: who?.name || api.t("A device"), n: count(status.held_back_objects) }) }));
  }
  box.append(el("div", { class: "syncdevices" }, ...status.devices.map(d => deviceRow(d, status))));
  if (status.reviews.length) box.append(reviewList(status.reviews));
}

function setupPanel(box) {
  const cfg = savedConfig();
  const supported = "showDirectoryPicker" in window;
  if (flow?.step === "new" || flow?.step === "join") { box.append(passphraseForm()); return; }
  if (cfg) {
    box.append(stateLine(api.t("Sync is locked. The passphrase is kept in memory only, so it is asked for each time this page opens. Edits are saved here in the meantime.")));
    if (!supported) { box.append(stateLine(api.t("This browser cannot open a folder. Use Chrome or Edge on a computer, or move notes with Export workspace."), "bad")); return; }
    box.append(el("div", { class: "syncactions" }, btn(api.t("Unlock"), () => reopen(cfg), { primary: true, id: "syncUnlock", icon: "arrow-clockwise" }), btn(api.t("Choose another folder"), pickFolder, {})));
    if (flow?.error) box.append(el("p", { class: "syncerror", role: "alert", text: flow.error }));
    return;
  }
  if (!supported) {
    box.append(stateLine(api.t("This browser cannot open a folder. Use Chrome or Edge on a computer, or move notes with Export workspace."), "bad"));
    return;
  }
  box.append(
    el("p", { class: "syncnote", text: api.t("Keeps this workspace the same across your devices, through a folder you choose, such as one a storage app keeps in step. Each device only ever adds its own new files there.") }),
    el("div", { class: "syncactions" }, btn(api.t("Choose a folder"), pickFolder, { primary: true, id: "syncChoose" })));
  if (flow?.error) box.append(el("p", { class: "syncerror", role: "alert", text: flow.error }));
}

async function pickFolder() {
  let dir;
  try { dir = await window.showDirectoryPicker({ mode: "readwrite", id: "casualnote-sync" }); }
  catch (e) { if (e.name !== "AbortError") { flow = { error: errorWords(String(e)) }; paint(); } return; }
  beginWith(dir);
}

export async function beginWith(dir) {
  try {
    const { root } = await inspectFolder(folderStorage(dir));
    flow = root ? { step: "join", dir, root } : { step: "new", dir };
  } catch (e) { flow = { error: errorWords(e.message) }; }
  paint();
}

async function reopen(cfg) {
  try {
    const dir = await loadHandle();
    if (!dir) { flow = { error: api.t("The folder is no longer known to this browser. Choose it again.") }; paint(); return; }
    if ((await dir.queryPermission({ mode: "readwrite" })) !== "granted" && (await dir.requestPermission({ mode: "readwrite" })) !== "granted") {
      flow = { error: api.t("The browser was not allowed to use the folder. Unlock again and allow it.") }; paint(); return;
    }
    flow = { step: "join", dir, root: { sync_id: cfg.sync_id, notebook: cfg.notebook } };
  } catch (e) { flow = { error: errorWords(e.message) }; }
  paint();
}

function passphraseForm() {
  const isNew = flow.step === "new";
  const pass = el("input", { type: "password", id: "syncPass", autocomplete: isNew ? "new-password" : "current-password", dir: "auto" });
  const again = isNew ? el("input", { type: "password", id: "syncPass2", autocomplete: "new-password", dir: "auto" }) : null;
  const name = el("input", { type: "text", id: "syncName", dir: "auto", value: deviceName() || defaultName(), maxlength: "80" });
  const form = el("form", { class: "syncsec", onsubmit: e => { e.preventDefault(); submitPassphrase(pass.value, again ? again.value : pass.value, name.value); } });
  form.append(
    el("p", { class: "syncnote", text: api.t(isNew ? "This folder is empty of CasualNote files. Setting it up writes a small description and a wrapped key there." : "This folder is already set up. Enter its passphrase to join.") }),
    el("label", { class: "syncfield" }, el("span", { text: api.t("This device's name, shown to your other devices in sealed files") }), name),
    el("label", { class: "syncfield" }, el("span", { text: isNew ? api.tf("Passphrase, at least {n} characters, in any language", { n: count(MIN_PASSPHRASE) }) : api.t("Passphrase") }), pass));
  if (again) form.append(el("label", { class: "syncfield" }, el("span", { text: api.t("The same passphrase again") }), again));
  if (isNew) form.append(el("p", { class: "pkgwarn", text: api.t("If this passphrase is forgotten, nobody can open what is in the folder, and there is no recovery key yet. Your notes on each device are not affected.") }));
  if (flow.error) form.append(el("p", { class: "syncerror", role: "alert", text: flow.error }));
  form.append(el("div", { class: "syncactions" },
    el("button", { class: "btn primary", id: "syncGo", type: "submit", ...(flow.busy ? { disabled: true } : {}) }, el("span", { text: flow.busy ? api.t("Working. This takes a few seconds by design.") : api.t(isNew ? "Set up this folder" : "Join") })),
    btn(api.t("Cancel"), e => { e.preventDefault(); flow = null; paint(); }, {})));
  queueMicrotask(() => { if (!flow?.busy) pass.focus(); });
  return form;
}

export async function submitPassphrase(passphrase, repeat, name, params) {
  const f = flow;
  if (!f || f.busy) return;
  if (f.step === "new") {
    if ([...passphrase.normalize("NFC")].length < MIN_PASSPHRASE) { f.error = api.tf("Use at least {n} characters.", { n: count(MIN_PASSPHRASE) }); paint(); return; }
    if (passphrase !== repeat) { f.error = api.t("The two passphrases differ. Type them again."); paint(); return; }
  }
  f.busy = true; f.error = null;
  try { localStorage.setItem("casualnote-device-name", (name || "").trim()); } catch { /* optional */ }
  paint();
  try {
    const storage = folderStorage(f.dir), log = api.store.log;
    let config, workspace;
    if (f.step === "new") {
      const made = await createFolder({ storage, kdf: api.store.kdf, device: log.device(), passphrase, params: params || DEFAULT_KDF });
      workspace = made.workspace;
      config = { notebook: made.notebook, sync_id: made.sync_id };
    } else {
      workspace = await unlockFolder({ storage, kdf: api.store.kdf, passphrase });
      config = { notebook: f.root.notebook, sync_id: f.root.sync_id };
    }
    log.put_local(CONFIG, utf8(JSON.stringify(config)));
    await api.store.pump();
    try { await saveHandle(f.dir); } catch (e) { console.warn("the folder could not be remembered", e); }
    flow = null;
    connect(f.dir, config, workspace);
  } catch (e) {
    f.busy = false;
    f.error = errorWords(e.message);
    paint();
  }
}

function paint() {
  if (!section) return;
  const box = el("div", { class: "syncsec" });
  // Before a passphrase is asked for, the limits of the encryption come first.
  if (live) { connectedPanel(box); box.append(honesty(), visibleTo()); }
  else { box.append(honesty(), visibleTo()); setupPanel(box); }
  box.append(storageFacts());
  const restore = restorePanel();
  if (restore) box.append(restore);
  section.replaceChildren(box);
}

/* ---------- Merge review ---------- */
function plainOf(block) { return block ? api.plain(block) : null; }
function versionCard(title, block, buttonText, onclick, id) {
  const text = plainOf(block);
  const card = el("div", { class: "syncversion" }, el("h5", { dir: "auto", text: title }), el("p", { dir: "auto", class: text === null ? "none" : "", text: text === null ? api.t("Deleted this block") : text || api.t("Empty") }));
  card.append(btn(buttonText, onclick, id ? { id } : {}));
  return card;
}

function reviewList(reviews) {
  const box = el("div", { class: "syncreviews", id: "syncReviews" }, el("h3", { text: api.t("To review") }),
    el("p", { class: "syncnote", text: api.t("Two devices changed the same block without seeing each other. Both changes are already merged into the page, so nothing is held back. Choose a version only if the merge reads wrong.") }));
  for (const r of reviews) box.append(reviewCard(r));
  return box;
}

function reviewCard(r) {
  const status = JSON.parse(api.engine.sync_status(live.sess));
  const other = status.devices.find(d => d.id === r.from_device)?.name || api.t("Another device");
  const title = api.metaOf(r.page)?.title || api.t("Untitled");
  return el("div", { class: "syncreview", "data-review": r.id },
    el("div", {}, el("button", { class: "textlink", dir: "auto", text: title, onclick: () => api.openPage(r.page) })),
    el("div", { class: "syncversions" },
      versionCard(api.t("This device"), r.here, api.t("Keep this one"), () => settle(r, "here")),
      versionCard(other, r.there, api.t("Keep this one"), () => settle(r, "there")),
      versionCard(api.t("Merged, as the page shows it now"), r.merged, api.t("Keep merged"), () => settle(r, "merged"))));
}

/** The ops that make a block read as a chosen version: one undo step, in ordinary operations. */
function opsFor(page, id, version) {
  const now = api.pageData(page).blocks;
  const current = now.find(b => b.id === id);
  if (!version) return current ? [{ op: "delete_block", page, block: id }] : [];
  if (!current) return [{ op: "insert_block", page, after: now.at(-1)?.id ?? null, block: version }];
  const units = s => s.length; // UTF-16, which is how positions are counted.
  const ops = [
    { op: "splice_text", page, block: id, at: 0, delete: units(api.plain(current)), insert: api.plain(version) },
    { op: "set_block", page, block: id, kind: version.kind, checked: !!version.checked },
  ];
  let at = 0;
  for (const run of version.runs) {
    const end = at + units(run.text);
    if (end > at) {
      for (const mark of run.marks || []) ops.push({ op: "set_mark", page, block: id, start: at, end, mark, on: true });
      if (run.link) ops.push({ op: "set_link", page, block: id, start: at, end, target: run.link });
    }
    at = end;
  }
  return ops;
}

export function settle(r, choice) {
  const ops = choice === "here" ? opsFor(r.page, r.block, r.here) : choice === "there" ? opsFor(r.page, r.block, r.there) : [];
  if (ops.length) api.edit(...ops);
  api.engine.review_settle(live.sess, r.page, r.id, choice);
  api.store.changed();
  document.querySelector(".modalbg")?.remove();
  api.renderAll();
  paint();
  paintMarks();
  afterEdit();
  api.toast(api.t(choice === "merged" ? "Kept the merged version." : "Kept that version. Undo brings the merge back."));
}

function pageReviews(pageId) {
  if (!live) return [];
  return JSON.parse(api.engine.sync_status(live.sess)).reviews.filter(r => r.page === pageId);
}

function paintMarks(page = api.state.cur) {
  const reviews = pageReviews(page);
  document.querySelectorAll(".b.syncmarked").forEach(n => n.classList.remove("syncmarked"));
  for (const r of reviews) document.querySelector(`.b[data-id="${CSS.escape(r.block)}"]`)?.classList.add("syncmarked");
  const holder = api.$("#rpDetails");
  if (!holder) return;
  let sec = api.$("#rpSync");
  if (!reviews.length) { sec?.remove(); return; }
  if (!sec) { sec = el("div", { class: "rp", id: "rpSync" }); holder.append(sec); }
  sec.replaceChildren(el("h4", { text: api.t("To review") }),
    ...reviews.map(r => el("button", { dir: "auto", text: plainOf(r.merged) || plainOf(r.here) || plainOf(r.there) || api.t("Empty"), onclick: () => openReview(r) })));
}

function openReview(r) {
  const m = modal(api.t("Two devices changed this block"));
  m.body.append(el("p", { class: "pkgnote", text: api.t("Both changes are merged into the page already. Choose a version only if the merge reads wrong.") }), reviewCard(r));
  m.foot.append(btn(api.t("Close"), m.close));
  m.foot.lastChild.focus();
}

function modal(label) {
  api.$(".modalbg")?.remove();
  const before = document.activeElement;
  const body = el("div", { class: "pkgbody" }), foot = el("div", { class: "foot" });
  const box = el("div", { class: "modal pkgmodal", role: "dialog", "aria-modal": "true", "aria-label": label, tabindex: "-1" }, el("h3", { dir: "auto", text: label }), body, foot);
  const bg = el("div", { class: "modalbg", onpointerdown: e => { if (e.target === bg) close(); } }, box);
  const close = () => { bg.remove(); if (before?.isConnected) before.focus?.(); };
  box.addEventListener("keydown", e => {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); return; }
    if (e.key !== "Tab") return;
    const stops = [...box.querySelectorAll("button:not([disabled]),input")];
    if (!stops.length) return;
    const first = stops[0], last = stops[stops.length - 1];
    if (e.shiftKey && (document.activeElement === first || document.activeElement === box)) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  });
  document.body.append(bg);
  return { body, foot, close };
}

/* ---------- Compaction, off the typing path ---------- */
async function compactWhenIdle() {
  try {
    const s = api.store;
    if (!s.log || !s.compact) return;
    // With sync set up and locked, what has not been stored must not be folded away unseen.
    if (savedConfig() && !live) return;
    const keep = live ? JSON.parse(s.log.compaction_keep(live.sess)) : [];
    await s.compact({ min: 8, keep });
  } catch (err) { console.warn("compaction", err); }
}

export function install(a) {
  api = a;
  api.ext.settings.push(box => {
    box.append(api.el("h2", { text: api.t("Sync") }));
    section = api.el("div", { id: "syncPanel" });
    box.append(section);
    paint();
  });
  api.ext.panels.push(page => { if (live) paintMarks(page.meta.id); });
  api.ext.onChange.push(afterEdit);
  setInterval(compactWhenIdle, 5 * 60_000);
  setTimeout(compactWhenIdle, 20_000);
  addEventListener("focus", () => { if (live && auto && !roundBusy) syncNow(); });
}

/** For the tests: what this tab has unlocked. */
export const testHooks = {
  live: () => live,
  syncNow,
  paint,
  paintMarks,
  setAuto: v => { auto = v; },
  forget: () => { live = null; flow = null; clearTimeout(pollTimer); clearTimeout(editTimer); paint(); },
  openReview,
};
