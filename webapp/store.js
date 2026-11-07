// Durable storage in IndexedDB. "Saved" shows only after the transaction has committed (PR-1).
// This stores whole snapshots for now. The append-only log in docs/23 replaces it (STO-01).
const DB = "casualnote", STORE = "workspace", KEY = "snapshot";

function database() {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(DB, 1);
    open.onupgradeneeded = () => open.result.createObjectStore(STORE);
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
}

export async function openStore(snapshot, onState, onRemote) {
  const db = await database();
  const channel = "BroadcastChannel" in self ? new BroadcastChannel("casualnote") : null;
  if (channel) channel.onmessage = e => onRemote(new Uint8Array(e.data));
  let timer = null, writing = false, dirty = false;

  const write = bytes => new Promise((resolve, reject) => {
    const tx = db.transaction(STORE, "readwrite", { durability: "strict" });
    tx.objectStore(STORE).put(bytes, KEY);
    tx.oncomplete = () => resolve();
    tx.onerror = tx.onabort = () => reject(tx.error);
  });
  async function flush() {
    timer = null;
    if (writing) { dirty = true; return; }
    writing = true; dirty = false;
    try {
      const bytes = snapshot();
      await write(bytes);
      channel?.postMessage(bytes.buffer);
      onState(dirty || timer ? "saving" : "saved");
    } catch (err) {
      console.error(err); onState("failed");
    } finally {
      writing = false;
      if (dirty) flush();
    }
  }
  addEventListener("pagehide", () => { if (timer) { clearTimeout(timer); flush(); } });

  return {
    load: () => new Promise((resolve, reject) => {
      const get = db.transaction(STORE).objectStore(STORE).get(KEY);
      get.onsuccess = () => resolve(get.result || null);
      get.onerror = () => reject(get.error);
    }),
    changed() {
      onState("saving");
      if (!timer) timer = setTimeout(flush, 250);
    },
    flush,
  };
}
