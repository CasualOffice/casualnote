//! The browser's handle on the engine. No logic lives here.

use casual_note_merge::{Workspace, build_container, parse_container};
use casual_note_ops::{Op, apply};
use casual_note_search::WorkspaceIndex;
use casual_note_store::{
    CompactOptions, Durable, ImageFile, IoError, IoRequest, Recovered, SaveState, Store,
    StoreConfig, base_snapshot_requests, format, plan_compaction,
};
use casual_note_sync::session_key_record_key as key_record_key;
use casual_note_sync::{
    Config, HostRequest, HostResponse, KdfParams, KeyRecord, Keys, Session, StorageOutcome,
    StorageRequest, new_root, read_root,
};
use std::collections::{BTreeMap, BTreeSet};
use wasm_bindgen::prelude::*;

fn fail(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// One open workspace.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Engine {
    ws: Workspace,
    search: WorkspaceIndex,
    revision: u32,
}

#[wasm_bindgen]
impl Engine {
    /// `peer` must be fresh on every open.
    #[wasm_bindgen(constructor)]
    pub fn new(peer: u64) -> Result<Engine, JsError> {
        Ok(Self {
            ws: Workspace::new(peer).map_err(fail)?,
            search: WorkspaceIndex::new(),
            revision: 0,
        })
    }

    /// Applies an op given as JSON, and returns its inverse as JSON.
    pub fn apply(&mut self, op: &str) -> Result<String, JsError> {
        let op: Op = serde_json::from_str(op).map_err(fail)?;
        let inverse = apply(&mut self.ws, &op).map_err(fail)?;
        self.search.touch(&op);
        self.revision = self.revision.wrapping_add(1);
        serde_json::to_string(&inverse).map_err(fail)
    }

    /// Every page's metadata, as JSON.
    pub fn pages(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.ws.pages()).map_err(fail)
    }

    /// One page, as JSON.
    pub fn page(&self, id: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.ws.page(id).map_err(fail)?).map_err(fail)
    }

    /// Every database id, as JSON.
    pub fn databases(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.ws.database_ids()).map_err(fail)
    }

    /// One database, as JSON.
    pub fn database(&self, id: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.ws.database(id).map_err(fail)?).map_err(fail)
    }

    /// The closed tool set, as JSON.
    pub fn tool_definitions(&self, read_only: bool) -> String {
        casual_note_tools::definitions_json(read_only)
    }

    /// `read`, `write` or `unknown`.
    pub fn tool_access(&self, name: &str) -> String {
        casual_note_tools::access_json(name).into()
    }

    /// Runs a read tool on a model's raw arguments. Always answers with JSON.
    pub fn tool_read(&self, name: &str, args: &str) -> String {
        casual_note_tools::read_json(&self.ws, name, args)
    }

    /// Prepares a write tool on a model's raw arguments, and changes nothing. Always answers
    /// with JSON: the preview and op batch, or a refusal.
    pub fn tool_prepare(&self, name: &str, args: &str, seed: &str) -> String {
        casual_note_tools::prepare_json(&self.ws, name, args, seed)
    }

    /// Raised by every change, so a view can tell whether what it derived is still current.
    pub fn revision(&self) -> u32 {
        self.revision
    }

    /// Searches every live page, block and row, as JSON. Catches up on edits first.
    pub fn search(&mut self, query: &str, limit: u32) -> Result<String, JsError> {
        let limit = usize::try_from(limit).map_err(fail)?;
        serde_json::to_string(&self.search.search(&self.ws, query, limit)).map_err(fail)
    }

    /// The knowledge graph, derived from the notes, as JSON.
    pub fn graph(&self) -> Result<String, JsError> {
        serde_json::to_string(&casual_note_graph::from_workspace(&self.ws)).map_err(fail)
    }

    /// What meaning search would embed for one page, as JSON. The vectors live with the host, in
    /// derived storage, and are never in an export. The inference runtime is not linked here.
    pub fn embed_chunks(&self, id: &str) -> Result<String, JsError> {
        serde_json::to_string(&casual_note_embed::chunks(&self.ws.page(id).map_err(fail)?))
            .map_err(fail)
    }

    /// A CasualNote Package of the workspace, or of one page. `created` is supplied by the caller.
    pub fn package_export(&self, created: &str, page: Option<String>) -> Result<Vec<u8>, JsError> {
        casual_note_package::export(&self.ws, created, page.as_deref()).map_err(fail)
    }

    /// One page as Markdown.
    pub fn page_markdown(&self, id: &str) -> Result<String, JsError> {
        casual_note_package::workspace_page_markdown(&self.ws, id).map_err(fail)
    }

    /// Plans importing a package, as `{ ops, report }` JSON, trying it on a scratch copy made with
    /// a fresh `peer`. Changes nothing. The caller runs the ops as one edit.
    pub fn package_plan(&self, bytes: &[u8], peer: u64) -> Result<String, JsError> {
        let package = casual_note_package::read_package(bytes).map_err(fail)?;
        let plan = casual_note_package::plan_import(&self.ws, package, peer).map_err(fail)?;
        serde_json::to_string(&plan).map_err(fail)
    }

    /// Everything, for storage or another replica.
    pub fn export(&self) -> Result<Vec<u8>, JsError> {
        self.ws.export().map_err(fail)
    }

    /// Adds whatever changed since the last save to the log. Returns how many documents changed.
    pub fn save_into(&mut self, log: &mut LogStore, wall_ms: i64) -> Result<u32, JsError> {
        Ok(log.log.save(&self.ws, wall_ms).map_err(fail)?.len() as u32)
    }

    /// One step of a sync round, as JSON `{ done, changed, requests }`. Do the requests, run the
    /// log's batches until idle, then call again. Bodies come from `take_body`.
    pub fn sync_step(
        &mut self,
        log: &mut LogStore,
        sess: &mut SyncSession,
        now: i64,
    ) -> Result<String, JsError> {
        let step = sess
            .session
            .step(&mut self.ws, &mut log.log, now, &mut |b| fill_random(b));
        let changed = sess.session.status(&self.ws).last_round.changed;
        if changed {
            self.search.touch_all();
            self.revision = self.revision.wrapping_add(1);
        }
        let mut requests = Vec::with_capacity(step.requests.len());
        for r in step.requests {
            requests.push(match r {
                HostRequest::ReadLocal { id, paths } => {
                    serde_json::json!({ "id": id, "op": "read_local", "paths": paths })
                }
                HostRequest::Storage { id, request } => match request {
                    StorageRequest::List { prefix } => {
                        serde_json::json!({ "id": id, "op": "list", "prefix": prefix })
                    }
                    StorageRequest::Get { key, max_len } => {
                        serde_json::json!({ "id": id, "op": "get", "key": key, "max_len": max_len })
                    }
                    StorageRequest::Delete { key } => {
                        serde_json::json!({ "id": id, "op": "delete", "key": key })
                    }
                    StorageRequest::PutIfAbsent { key, body } => {
                        sess.bodies.insert(id, body);
                        serde_json::json!({ "id": id, "op": "put", "key": key })
                    }
                },
            });
        }
        Ok(
            serde_json::json!({ "done": step.done, "changed": changed, "requests": requests })
                .to_string(),
        )
    }

    /// What the sync panel shows, as JSON.
    pub fn sync_status(&self, sess: &SyncSession) -> Result<String, JsError> {
        serde_json::to_string(&sess.session.status(&self.ws)).map_err(fail)
    }

    /// Settles a review in the page, so it clears on every device. Content changes come separately,
    /// as ordinary ops through the app.
    pub fn review_settle(
        &mut self,
        sess: &mut SyncSession,
        page: &str,
        id: &str,
        choice: &str,
    ) -> Result<(), JsError> {
        self.ws.resolve_review(page, id, choice).map_err(fail)?;
        sess.session.close_review(id);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Merges stored or received bytes.
    pub fn import(&mut self, bytes: &[u8]) -> Result<(), JsError> {
        self.ws.import(bytes).map_err(fail)?;
        self.search.touch_all();
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = crypto, js_name = getRandomValues)]
    fn fill_random(buf: &mut [u8]);
}

fn hex_array<const N: usize>(s: &str) -> Result<[u8; N], JsError> {
    format::unhex(s)
        .and_then(|v| v.try_into().ok())
        .ok_or_else(|| JsError::new("CN-SYN-0002: a value is malformed"))
}

fn image_from(index: &str, blob: &[u8]) -> Result<Vec<ImageFile>, JsError> {
    let entries: Vec<(String, usize, usize)> = serde_json::from_str(index).map_err(fail)?;
    let mut out = Vec::with_capacity(entries.len());
    for (path, at, len) in entries {
        let end = at
            .checked_add(len)
            .ok_or_else(|| JsError::new("bad image"))?;
        let bytes = blob
            .get(at..end)
            .ok_or_else(|| JsError::new("bad image"))?
            .to_vec();
        out.push(ImageFile { path, bytes });
    }
    Ok(out)
}

/// The durable log of this open. The host runs its batches, in order, and reports back.
#[wasm_bindgen]
#[derive(Debug)]
pub struct LogStore {
    log: Durable,
    rec: Recovered,
    in_flight_blob: Vec<u8>,
    wall_ms: i64,
    peer: u64,
}

#[wasm_bindgen]
impl LogStore {
    /// Opens from what the host read. `index` is JSON `[[path, at, len]]` into `blob`.
    pub fn open(
        index: &str,
        blob: &[u8],
        new_device: &str,
        open_id: u32,
        peer: u64,
        wall_ms: i64,
        live: &str,
    ) -> Result<LogStore, JsError> {
        let cfg = StoreConfig {
            new_device: hex_array(new_device)?,
            open_id,
            peer,
            wall_ms,
            live_opens: serde_json::from_str(live).map_err(fail)?,
        };
        let (log, rec) = Durable::open(image_from(index, blob)?, &cfg)
            .map_err(|e| JsError::new(&format!("{}: {}", e.code, e.detail)))?;
        Ok(Self {
            log,
            rec,
            in_flight_blob: Vec::new(),
            wall_ms,
            peer,
        })
    }

    /// What is on disk now, as a workspace container, without repairing or changing anything.
    pub fn read_only(
        index: &str,
        blob: &[u8],
        new_device: &str,
        live: &str,
    ) -> Result<Option<Vec<u8>>, JsError> {
        let cfg = StoreConfig {
            new_device: hex_array(new_device)?,
            open_id: 0,
            peer: 1,
            wall_ms: 0,
            live_opens: serde_json::from_str(live).map_err(fail)?,
        };
        let rec = Store::read_only(image_from(index, blob)?, &cfg)
            .map_err(|e| JsError::new(&format!("{}: {}", e.code, e.detail)))?;
        Ok(rec.container())
    }

    /// Everything recovered, as a workspace container.
    pub fn container(&self) -> Option<Vec<u8>> {
        self.rec.container()
    }

    /// Lets go of what recovery read, once it has been loaded.
    pub fn release(&mut self) {
        self.rec.docs.clear();
    }

    /// The next segment to try before loading. Run the batch this queues, then call `guard_try`.
    pub fn guard_next(&mut self) -> Option<String> {
        self.log.guard_next(&self.rec)
    }

    pub fn guard_try(&mut self) {
        self.log.guard_try(&mut self.rec, |_, _| Ok(()));
    }

    pub fn guard_finish(&mut self) {
        self.log.guard_finish();
    }

    /// Writes an old whole-workspace snapshot into the log as base snapshots, with a marker after
    /// them. Each is checked first. Nothing old is deleted.
    pub fn migrate(&mut self, bytes: &[u8], marker: &str) -> Result<(), JsError> {
        let parts = parse_container(bytes).map_err(fail)?;
        let device = self.log.store.device();
        let mut requests = Vec::new();
        for (i, (name, body)) in parts.iter().enumerate() {
            let seq = (self.wall_ms as u64)
                .wrapping_mul(1024)
                .wrapping_add(i as u64);
            requests.extend(
                base_snapshot_requests(name, body, &device, seq, self.wall_ms)
                    .map_err(|e| JsError::new(&e))?,
            );
        }
        self.log.store.submit(requests);
        self.log.store.put_local("migrated", marker.as_bytes());
        for (name, body) in parts {
            let doc = self.rec.docs.entry(name.clone()).or_default();
            doc.name = name;
            doc.snapshots.push(body.to_vec());
        }
        Ok(())
    }

    /// A small local file's newest valid contents.
    pub fn local(&self, name: &str) -> Option<Vec<u8>> {
        self.rec.local.get(name).cloned()
    }

    pub fn put_local(&mut self, name: &str, bytes: &[u8]) {
        self.log.store.put_local(name, bytes);
        self.rec.local.insert(name.to_owned(), bytes.to_vec());
    }

    /// Next group commit: JSON `{ id, ops }`, with bytes in `batch_blob`. `None` if nothing is due.
    pub fn next_batch(&mut self) -> Result<Option<String>, JsError> {
        let Some(batch) = self.log.store.next_batch() else {
            return Ok(None);
        };
        let mut blob = Vec::new();
        let mut ops = Vec::with_capacity(batch.requests.len());
        for r in batch.requests {
            ops.push(match r {
                IoRequest::CreateDir { path } => serde_json::json!({ "op": "mkdir", "path": path }),
                IoRequest::WriteAt { path, at, data } => {
                    let off = blob.len();
                    blob.extend_from_slice(&data);
                    serde_json::json!({ "op": "write", "path": path, "at": at, "off": off, "len": data.len() })
                }
                IoRequest::SyncFile { path } => serde_json::json!({ "op": "sync", "path": path }),
                IoRequest::SyncDir { path } => serde_json::json!({ "op": "syncdir", "path": path }),
                IoRequest::SetLen { path, len } => serde_json::json!({ "op": "setlen", "path": path, "len": len }),
                IoRequest::Remove { path } => serde_json::json!({ "op": "remove", "path": path }),
            });
        }
        self.in_flight_blob = blob;
        Ok(Some(
            serde_json::json!({ "id": batch.id, "ops": ops }).to_string(),
        ))
    }

    pub fn batch_blob(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.in_flight_blob)
    }

    /// The host reports a batch. No error means every request completed, flushes included.
    pub fn complete(&mut self, id: f64, error: Option<String>, no_space: bool) {
        let result = match error {
            None => Ok(()),
            Some(_) if no_space => Err(IoError::NoSpace),
            Some(e) => Err(IoError::Failed(e)),
        };
        self.log.store.on_complete(id as u64, result);
        let _ = self.log.store.poll_acks();
    }

    /// Updates that became durable since the last call, as a container for other tabs.
    pub fn take_broadcast(&mut self) -> Option<Vec<u8>> {
        let updates = self.log.store.take_acked_updates();
        (!updates.is_empty())
            .then(|| build_container(updates.iter().map(|(n, u)| (n.as_str(), u.as_slice()))))
    }

    /// Records that a container's contents are already durable, so they are not written again.
    pub fn mark_durable(&mut self, bytes: &[u8]) {
        self.log.cursor.mark_durable(bytes);
    }

    pub fn seal_all(&mut self) {
        self.log.store.seal_all();
    }

    pub fn is_idle(&self) -> bool {
        self.log.store.is_idle()
    }

    /// `saved`, `saving` or `failed`.
    pub fn state(&self) -> String {
        match self.log.store.state() {
            SaveState::Saved => "saved",
            SaveState::Saving => "saving",
            SaveState::Failed => "failed",
        }
        .to_owned()
    }

    pub fn device(&self) -> String {
        format::hex(&self.log.store.device())
    }

    /// What recovery found, as JSON.
    pub fn issues(&self) -> String {
        let issues: Vec<serde_json::Value> = self
            .rec
            .issues
            .iter()
            .map(|i| serde_json::json!({ "code": i.code, "doc": i.doc, "path": i.path, "detail": i.detail }))
            .collect();
        serde_json::Value::Array(issues).to_string()
    }

    /// What has been moved aside, as JSON paths.
    pub fn quarantine(&self) -> String {
        serde_json::to_string(&self.rec.quarantine).unwrap_or_default()
    }

    /// Own segments held back as poison, as JSON `{ doc: path }`.
    pub fn blocked(&self) -> String {
        serde_json::to_string(self.log.store.blocked()).unwrap_or_default()
    }

    /// Documents with at least this many sealed segments, as JSON `[[name, dir]]`.
    pub fn compaction_candidates(&self, min_segments: u32) -> String {
        let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
        for s in self.log.store.sealed_segments() {
            *counts.entry(s.name.as_str()).or_insert(0) += 1;
        }
        let list: Vec<(String, String)> = counts
            .into_iter()
            .filter(|(_, n)| *n >= min_segments)
            .map(|(name, _)| (name.to_owned(), format::doc_dir(name)))
            .collect();
        serde_json::to_string(&list).unwrap_or_default()
    }

    /// Plans and queues compaction of one document from its files, read by the host. Returns JSON
    /// of what it will do, or `null` when nothing should be done yet.
    pub fn compact(
        &mut self,
        name: &str,
        index: &str,
        blob: &[u8],
        keep: &str,
        min_segments: u32,
    ) -> Result<String, JsError> {
        let files = image_from(index, blob)?;
        let keep: BTreeSet<String> = serde_json::from_str(keep).map_err(fail)?;
        let o = CompactOptions {
            min_segments: min_segments as usize,
            keep,
            device: self.log.store.device(),
            wall_ms: self.wall_ms,
            peer: self.peer ^ 0x1234,
        };
        let Some(plan) = plan_compaction(name, &files, &o).map_err(|e| JsError::new(&e))? else {
            return Ok("null".into());
        };
        let report = serde_json::json!({ "removed": plan.removed.len(), "before": plan.bytes_before, "after": plan.bytes_after });
        self.log.store.submit(plan.requests);
        self.log.store.forget(&plan.removed);
        Ok(report.to_string())
    }

    /// Own segments that must not be compacted yet, as a JSON list.
    pub fn compaction_keep(&self, sess: &SyncSession) -> String {
        serde_json::to_string(&sess.session.compaction_keep(&self.log)).unwrap_or_default()
    }
}

/// One device's unlocked view of a sync folder. The key lives here, in memory, until the page goes.
#[wasm_bindgen]
#[derive(Debug)]
pub struct SyncSession {
    session: Session,
    bodies: BTreeMap<u64, Vec<u8>>,
}

#[wasm_bindgen]
impl SyncSession {
    #[allow(clippy::too_many_arguments)]
    #[wasm_bindgen(constructor)]
    pub fn new(
        workspace_key: &str,
        notebook: &str,
        sync_id: &str,
        device: &str,
        name: &str,
        platform: &str,
        saved: Option<Vec<u8>>,
        checkpoint_every: u32,
    ) -> Result<SyncSession, JsError> {
        let keys = Keys::new(hex_array(workspace_key)?, hex_array(notebook)?);
        let config = Config {
            checkpoint_every: u64::from(checkpoint_every.max(1)),
            ..Config::default()
        };
        let session = Session::new(
            keys,
            hex_array(device)?,
            &hex_array(sync_id)?,
            name,
            platform,
            saved.as_deref(),
            config,
        );
        Ok(Self {
            session,
            bodies: BTreeMap::new(),
        })
    }

    pub fn begin_round(&mut self) {
        self.session.begin_round();
    }

    pub fn is_running(&self) -> bool {
        self.session.is_running()
    }

    pub fn take_body(&mut self, id: f64) -> Vec<u8> {
        self.bodies.remove(&(id as u64)).unwrap_or_default()
    }

    /// Storage answered. `kind` is `got`, `listed`, `created`, `exists`, `deleted`, `missing`,
    /// `transient`, `denied`, `full`, `unreachable` or `toolarge`.
    pub fn respond(
        &mut self,
        id: f64,
        kind: &str,
        bytes: Option<Vec<u8>>,
        listed: Option<String>,
    ) -> Result<(), JsError> {
        let outcome = match kind {
            "got" => StorageOutcome::Got(bytes.unwrap_or_default()),
            "listed" => StorageOutcome::Listed(
                serde_json::from_str(&listed.unwrap_or_default()).map_err(fail)?,
            ),
            "created" => StorageOutcome::Created,
            "exists" => StorageOutcome::AlreadyExists,
            "deleted" => StorageOutcome::Deleted,
            "missing" => StorageOutcome::NotFound,
            "denied" => StorageOutcome::Forbidden,
            "full" => StorageOutcome::QuotaFull,
            "unreachable" => StorageOutcome::Unreachable,
            "toolarge" => StorageOutcome::TooLarge,
            other => StorageOutcome::Transient(other.to_owned()),
        };
        self.session
            .on_response(id as u64, HostResponse::Storage(outcome));
        Ok(())
    }

    /// The host read local files. `index` is JSON `[[path, at, len]]` into `blob`.
    pub fn respond_files(&mut self, id: f64, index: &str, blob: &[u8]) -> Result<(), JsError> {
        let files = image_from(index, blob)?
            .into_iter()
            .map(|f| (f.path, f.bytes))
            .collect();
        self.session
            .on_response(id as u64, HostResponse::Files(files));
        Ok(())
    }

    /// A person's explicit removal of a device.
    pub fn remove_device(&mut self, target: &str) {
        self.session.remove_device(target);
    }
}

/// Stretches a passphrase. Slow by design, so the web app calls this in a worker.
#[wasm_bindgen]
pub fn sync_derive_kek(
    passphrase: &str,
    salt: &str,
    m_kib: u32,
    t: u32,
    p: u32,
) -> Result<String, JsError> {
    let kek = casual_note_sync::crypto::derive_kek(
        passphrase,
        &hex_array(salt)?,
        KdfParams { m_kib, t, p },
    )
    .map_err(JsError::new)?;
    Ok(format::hex(&kek))
}

/// Everything a new sync folder needs, as JSON: `{ root, record, workspace, notebook, sync_id }`.
#[wasm_bindgen]
pub fn sync_new_folder(
    kek: &str,
    salt: &str,
    m_kib: u32,
    t: u32,
    p: u32,
    device: &str,
) -> Result<String, JsError> {
    let (mut workspace, mut sync_id, mut notebook) = ([0u8; 32], [0u8; 16], [0u8; 16]);
    fill_random(&mut workspace);
    fill_random(&mut sync_id);
    fill_random(&mut notebook);
    let keys = Keys::new(workspace, notebook);
    let device = hex_array::<16>(device)?;
    let record = KeyRecord::new(
        &hex_array(kek)?,
        &workspace,
        &keys.key_id,
        &device,
        KdfParams { m_kib, t, p },
        &hex_array(salt)?,
        &mut |b| fill_random(b),
    );
    Ok(serde_json::json!({
        "root": String::from_utf8_lossy(&new_root(&sync_id, &notebook)),
        "record": serde_json::to_string(&record).map_err(fail)?,
        "record_key": key_record_key(&format::hex(&device), 1),
        "workspace": format::hex(&workspace),
        "notebook": format::hex(&notebook),
        "sync_id": format::hex(&sync_id),
    })
    .to_string())
}

/// Reads a root descriptor, as JSON `{ sync_id, notebook }`, or `None` if it is not one.
#[wasm_bindgen]
pub fn sync_read_root(bytes: &[u8]) -> Option<String> {
    let (sync_id, notebook) = read_root(bytes)?;
    Some(
        serde_json::json!({ "sync_id": format::hex(&sync_id), "notebook": format::hex(&notebook) })
            .to_string(),
    )
}

/// Reads a key record's Argon2 parameters and salt, as JSON, checked against the limits. Nothing is
/// derived from parameters that hostile storage could have set too low or too high.
#[wasm_bindgen]
pub fn sync_record_params(record: &str) -> Result<String, JsError> {
    let r: KeyRecord = serde_json::from_str(record).map_err(fail)?;
    r.params().check().map_err(JsError::new)?;
    Ok(serde_json::json!({ "salt": r.salt, "m_kib": r.m_kib, "t": r.t, "p": r.p }).to_string())
}

/// Opens the workspace key, or `None`: a wrong passphrase, or a damaged record.
#[wasm_bindgen]
pub fn sync_unwrap_key(record: &str, kek: &str) -> Result<Option<String>, JsError> {
    let r: KeyRecord = serde_json::from_str(record).map_err(fail)?;
    Ok(r.unwrap_key(&hex_array(kek)?).map(|k| format::hex(&k)))
}

/// Checks a package and describes it, as JSON. Works on no workspace.
#[wasm_bindgen]
pub fn package_inspect(bytes: &[u8]) -> Result<String, JsError> {
    serde_json::to_string(&casual_note_package::inspect(bytes).map_err(fail)?).map_err(fail)
}

/// One file from a package, for showing its Markdown.
#[wasm_bindgen]
pub fn package_entry(bytes: &[u8], path: &str) -> Result<Vec<u8>, JsError> {
    casual_note_package::entry_bytes(bytes, path).map_err(fail)
}
