//! One device's view of a sync folder, and a round of exchange as a state machine. After every
//! step the host performs the requests, pumps the store until idle, and steps again. That is what
//! orders things: bytes are in the outbox before a put, and imports are durable before an ack.

use crate::crypto::{
    self, KIND_ACK, KIND_CHECKPOINT, KIND_REGISTRY, KIND_SEGMENT, Keys, PROTOCOL, Refusal,
};
use casual_note_merge::{Overlap, Workspace, parse_container};
use casual_note_model::Block;
use casual_note_store::format::{self, FRAME_BATCH, hex, scan_segment, unhex};
use casual_note_store::{Durable, IoRequest, SaveState, base_snapshot_requests};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const ROOT_KEY: &str = "casualnote-sync.json";
const DOCS_PER_OBJECT: usize = 4096;
const MAX_FILE: u64 = 64 * 1024 * 1024;
const FETCH_PER_ROUND: usize = 64;
const SELF_CHECK_PER_ROUND: usize = 8;
const KEEP_ACKS: usize = 2;
const MAX_SEQ: u64 = 1 << 48;

/// What storage is asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageRequest {
    /// Everything under a prefix, recursively, in any order. May miss recent files.
    List {
        prefix: String,
    },
    Get {
        key: String,
        max_len: u64,
    },
    PutIfAbsent {
        key: String,
        body: Vec<u8>,
    },
    Delete {
        key: String,
    },
}

/// A file in a listing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub len: u64,
}

/// What storage answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageOutcome {
    Listed(Vec<Entry>),
    Got(Vec<u8>),
    Created,
    AlreadyExists,
    Deleted,
    NotFound,
    Transient(String),
    Unauthorised,
    Forbidden,
    QuotaFull,
    Unreachable,
    TooLarge,
}

/// Something the host must do before the next step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostRequest {
    Storage { id: u64, request: StorageRequest },
    ReadLocal { id: u64, paths: Vec<String> },
}

/// The host's answer. Answers may come in any order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostResponse {
    Storage(StorageOutcome),
    Files(Vec<(String, Vec<u8>)>),
}

/// What a step produced.
#[derive(Clone, Debug, Default)]
pub struct Step {
    pub requests: Vec<HostRequest>,
    pub done: bool,
}

/// How a root stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootState {
    Connected,
    Unreachable,
    RootMissing,
    WrongRoot,
    StorageRegressed,
    AuthFailed,
    QuotaFull,
    Forked,
    LocalFailed,
}

/// One open failure, for the panel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub code: String,
    pub key: String,
    pub attempts: u32,
    pub first_ms: i64,
    pub last_ms: i64,
}

/// A block two devices changed unseen. The merged text is already in the page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewMarker {
    pub id: String,
    pub page: String,
    pub block: String,
    pub base: Option<Block>,
    pub here: Option<Block>,
    pub there: Option<Block>,
    pub merged: Option<Block>,
    pub from_device: String,
    pub found_ms: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Outgoing {
    kind: u8,
    seq: u64,
    key: String,
    path: String,
    sha: String,
    len: u64,
    written: bool,
    /// Local segments this object holds.
    segments: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Peer {
    name: String,
    platform: String,
    announced: bool,
    removed: bool,
    /// Everything up to here is held here, contiguously.
    base: u64,
    /// Held past a gap.
    extra: BTreeSet<u64>,
    /// The hash of the object at `base`, for the chain.
    chain: String,
    /// What this peer says it holds, per device.
    acks: BTreeMap<String, u64>,
    ack_seq: u64,
    observed_ms: Option<i64>,
    claimed_ms: Option<i64>,
    failures: BTreeMap<String, Failure>,
    highest_listed: u64,
    registry_seen: BTreeSet<u64>,
    rejected: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Checkpoint {
    device: String,
    seq: u64,
    key: String,
    /// Device to sequence covered.
    vector: BTreeMap<String, u64>,
    verified: bool,
}

/// Everything durable about this device's view. Holds no key and no passphrase.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SyncState {
    device: String,
    sync_id: String,
    notebook: String,
    name: String,
    platform: String,
    next: BTreeMap<u8, u64>,
    chain: String,
    outbox: BTreeMap<String, Outgoing>,
    packed: BTreeSet<String>,
    peers: BTreeMap<String, Peer>,
    announced: bool,
    pending_registry: Vec<String>,
    checkpoints: Vec<Checkpoint>,
    reviews: Vec<ReviewMarker>,
    dismissed: BTreeSet<String>,
    last_ack_vector: BTreeMap<String, u64>,
    last_round_ms: Option<i64>,
    forked: bool,
}

/// Policy. Proposals, as docs/56 says.
#[derive(Clone, Debug)]
pub struct Config {
    /// Own objects not covered by a checkpoint before one is written.
    pub checkpoint_every: u64,
    /// The largest checkpoint this device will write. Larger means no checkpoint and so no trimming.
    pub checkpoint_limit: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            checkpoint_every: 512,
            checkpoint_limit: crypto::MAX_OBJECT - 1024 * 1024,
        }
    }
}

/// What the last round did.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RoundSummary {
    pub pushed: u32,
    pub fetched: u32,
    pub imported_segments: u32,
    pub duplicates: u32,
    pub refused: u32,
    pub repaired: u32,
    pub trimmed: u32,
    pub waiting: u32,
    pub foreign_files: u32,
    pub changed: bool,
}

/// One row of the panel.
#[derive(Clone, Debug, Serialize)]
pub struct DeviceRow {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub this_device: bool,
    pub removed: bool,
    pub stale: bool,
    pub observed_ms: Option<i64>,
    pub claimed_ms: Option<i64>,
    /// Own objects not yet stored.
    pub not_written: u32,
    /// Own objects stored, and not yet seen by this device (for this device: by every other).
    pub not_seen: u32,
    /// Objects of that device not yet held here.
    pub waiting_from: u32,
    pub failed: Vec<Failure>,
    /// Objects held here that could rebuild what that device lacks.
    pub restorable_objects: u32,
    pub restorable_bytes: u64,
    pub rejected: Vec<String>,
}

/// What the panel shows. Reading it touches no storage.
#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub root: RootState,
    pub detail: String,
    pub this_device: String,
    pub devices: Vec<DeviceRow>,
    pub reviews: Vec<ReviewMarker>,
    pub last_round: RoundSummary,
    pub last_round_ms: Option<i64>,
    pub held_back: Option<String>,
    pub held_back_objects: u32,
    pub checkpoints: u32,
    pub verified_checkpoints: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Preflight,
    PreflightDone,
    Save,
    ReadSegments,
    Pack,
    ReadOutbox,
    Push,
    PushDone,
    List,
    Listed,
    Process,
    Ack,
    AckPut,
    Trim,
    TrimDone,
    Finish,
}

/// The state of one device's sync, unlocked.
#[derive(Debug)]
pub struct Session {
    keys: Keys,
    device: [u8; 16],
    /// Durable state, written to the log's local files.
    pub state: SyncState,
    config: Config,
    phase: Phase,
    next_id: u64,
    pending: BTreeMap<u64, Pendingq>,
    responses: BTreeMap<u64, HostResponse>,
    root: RootState,
    detail: String,
    summary: RoundSummary,
    cache: BTreeMap<String, Vec<u8>>,
    check_at: usize,
    listing: Vec<Entry>,
    now: i64,
    wants_replace: BTreeSet<String>,
    pending_delete: BTreeSet<String>,
    checkpoint_plain: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug)]
enum Pendingq {
    Root,
    Segments(Vec<casual_note_store::SealedSegment>),
    Outbox,
    Put(String),
    List,
    Fetch(String),
    SelfCheck(String),
    Delete,
    CheckpointVerify(String, Vec<u8>),
}

/// A new root descriptor. It holds no key.
pub fn new_root(sync_id: &[u8; 16], notebook: &[u8; 16]) -> Vec<u8> {
    serde_json::json!({
        "format": "casualnote.sync",
        "sync_id": hex(sync_id),
        "notebook": hex(notebook),
        "created_protocol": PROTOCOL,
    })
    .to_string()
    .into_bytes()
}

/// Reads a root descriptor: the sync id and notebook id.
pub fn read_root(bytes: &[u8]) -> Option<([u8; 16], [u8; 16])> {
    if bytes.len() > 64 * 1024 {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    if v.get("format")?.as_str()? != "casualnote.sync" {
        return None;
    }
    let sync_id: [u8; 16] = unhex(v.get("sync_id")?.as_str()?)?.try_into().ok()?;
    let notebook: [u8; 16] = unhex(v.get("notebook")?.as_str()?)?.try_into().ok()?;
    Some((sync_id, notebook))
}

#[derive(Serialize, Deserialize)]
struct RegistryRecord {
    #[serde(default)]
    announce: Option<Announce>,
    #[serde(default)]
    remove: Option<String>,
    sealed_at: i64,
}

#[derive(Serialize, Deserialize)]
struct Announce {
    name: String,
    platform: String,
    protocol_min: u16,
    protocol_max: u16,
}

#[derive(Serialize, Deserialize)]
struct AckBody {
    sealed_at: i64,
    protocol_max: u16,
    acked: BTreeMap<String, u64>,
    #[serde(default)]
    extra: BTreeMap<String, Vec<u64>>,
    #[serde(default)]
    rejected: Vec<String>,
}

struct SegmentBody<'a> {
    prev_seq: u64,
    prev_hash: [u8; 32],
    sealed_at: i64,
    docs: Vec<(String, &'a [u8])>,
}

fn encode_segment_body(
    prev_seq: u64,
    prev_hash: &[u8; 32],
    sealed_at: i64,
    docs: &[(String, Vec<u8>)],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&prev_seq.to_le_bytes());
    out.extend_from_slice(prev_hash);
    out.extend_from_slice(&sealed_at.to_le_bytes());
    out.extend_from_slice(&(docs.len() as u32).to_le_bytes());
    for (name, file) in docs {
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(file.len() as u64).to_le_bytes());
        out.extend_from_slice(file);
    }
    out
}

fn decode_segment_body(b: &[u8]) -> Option<SegmentBody<'_>> {
    let take8 = |b: &[u8], at: usize| -> Option<u64> {
        Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
    };
    let prev_seq = take8(b, 0)?;
    let prev_hash: [u8; 32] = b.get(8..40)?.try_into().ok()?;
    let sealed_at = take8(b, 40)? as i64;
    let n = u32::from_le_bytes(b.get(48..52)?.try_into().ok()?) as usize;
    if n > DOCS_PER_OBJECT {
        return None;
    }
    let mut at = 52usize;
    let mut docs = Vec::with_capacity(n);
    for _ in 0..n {
        let len = u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize;
        at += 4;
        let name = std::str::from_utf8(b.get(at..at.checked_add(len)?)?)
            .ok()?
            .to_owned();
        at += len;
        let flen = usize::try_from(take8(b, at)?).ok()?;
        at += 8;
        let file = b.get(at..at.checked_add(flen)?)?;
        at += flen;
        docs.push((name, file));
    }
    Some(SegmentBody {
        prev_seq,
        prev_hash,
        sealed_at,
        docs,
    })
}

fn seq_name(seq: u64) -> String {
    format!("{seq:016x}")
}

fn object_key(device: &str, notebook: &str, kind: u8, seq: u64) -> String {
    match kind {
        KIND_SEGMENT => format!(
            "d/{device}/n/{notebook}/s/{:04x}/{}.cnss",
            seq >> 12,
            seq_name(seq)
        ),
        KIND_ACK => format!("d/{device}/n/{notebook}/a/{}.cnsa", seq_name(seq)),
        KIND_REGISTRY => format!("d/{device}/r/{}.cnsr", seq_name(seq)),
        _ => format!("d/{device}/n/{notebook}/c/{}.cnsc", seq_name(seq)),
    }
}

/// The key of a key record.
pub fn key_record_key(device: &str, seq: u64) -> String {
    format!("d/{device}/k/{}.cnsk", seq_name(seq))
}

/// Reads an object's name exactly. Conflict copies and anything else fail, and are looked at
/// separately.
fn parse_key(key: &str) -> Option<(String, u8, u64)> {
    let parts: Vec<&str> = key.split('/').collect();
    let hexish = |s: &str, n: usize| {
        s.len() == n
            && s.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    };
    let seq_of = |file: &str, ext: &str| -> Option<u64> {
        let stem = file.strip_suffix(ext)?;
        let seq = hexish(stem, 16).then(|| u64::from_str_radix(stem, 16).ok())??;
        // Names are unauthenticated. An absurd sequence is not a sequence.
        (1..MAX_SEQ).contains(&seq).then_some(seq)
    };
    if parts.first() != Some(&"d") || parts.len() < 4 || !hexish(parts[1], 32) {
        return None;
    }
    let device = parts[1].to_owned();
    match parts.as_slice() {
        [_, _, "r", file] => Some((device, KIND_REGISTRY, seq_of(file, ".cnsr")?)),
        [_, _, "n", nb, "s", shard, file] if hexish(nb, 32) && hexish(shard, 4) => {
            Some((device, KIND_SEGMENT, seq_of(file, ".cnss")?))
        }
        [_, _, "n", nb, "a", file] if hexish(nb, 32) => {
            Some((device, KIND_ACK, seq_of(file, ".cnsa")?))
        }
        [_, _, "n", nb, "c", file] if hexish(nb, 32) => {
            Some((device, KIND_CHECKPOINT, seq_of(file, ".cnsc")?))
        }
        _ => None,
    }
}

/// A local path for an object waiting to go out.
fn outbox_path(kind: u8, seq: u64) -> String {
    format!("local/sync/outbox/{kind}/{}.obj", seq_name(seq))
}

fn sha_hex(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}

impl Session {
    /// Starts or resumes a device on a folder. `saved` is the durable state, if there is one.
    pub fn new(
        keys: Keys,
        device: [u8; 16],
        sync_id: &[u8; 16],
        name: &str,
        platform: &str,
        saved: Option<&[u8]>,
        config: Config,
    ) -> Self {
        let mut state = saved
            .and_then(|b| serde_json::from_slice::<SyncState>(b).ok())
            .unwrap_or_default();
        if state.sync_id != hex(sync_id) || state.device != hex(&device) {
            // Another folder or another device id starts over. Nothing local is at risk: every
            // sealed segment is simply packed again, and duplicates do nothing.
            state = SyncState {
                device: hex(&device),
                sync_id: hex(sync_id),
                notebook: hex(&keys.notebook),
                ..Default::default()
            };
        }
        state.name = name.to_owned();
        state.platform = platform.to_owned();
        Self {
            keys,
            device,
            state,
            config,
            phase: Phase::Idle,
            next_id: 1,
            pending: BTreeMap::new(),
            responses: BTreeMap::new(),
            root: RootState::Connected,
            detail: String::new(),
            summary: RoundSummary::default(),
            cache: BTreeMap::new(),
            check_at: 0,
            listing: Vec::new(),
            now: 0,
            wants_replace: BTreeSet::new(),
            pending_delete: BTreeSet::new(),
            checkpoint_plain: BTreeMap::new(),
        }
    }

    pub fn device_hex(&self) -> String {
        hex(&self.device)
    }

    /// Begins a round, unless one is running.
    pub fn begin_round(&mut self) {
        if self.phase == Phase::Idle {
            self.phase = Phase::Preflight;
            self.summary = RoundSummary::default();
        }
    }

    pub fn is_running(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// The local segments that must not be compacted away yet: sealed and not stored.
    pub fn compaction_keep(&self, log: &Durable) -> BTreeSet<String> {
        let written: BTreeSet<&String> = self
            .state
            .outbox
            .values()
            .filter(|o| o.written)
            .flat_map(|o| o.segments.iter())
            .collect();
        let mut keep = BTreeSet::new();
        for s in log.store.sealed_segments() {
            if s.device == self.device
                && !(self.state.packed.contains(&s.path)
                    && (written.contains(&s.path) || !self.outbox_holds(&s.path)))
            {
                keep.insert(s.path.clone());
            }
        }
        keep
    }

    fn outbox_holds(&self, path: &str) -> bool {
        self.state
            .outbox
            .values()
            .any(|o| o.segments.iter().any(|p| p == path))
    }

    /// An explicit act by a person. Removal is never automatic.
    pub fn remove_device(&mut self, target: &str) {
        self.state.pending_registry.push(target.to_owned());
        self.state
            .peers
            .entry(target.to_owned())
            .or_default()
            .removed = true;
    }

    /// Marks a review settled, once the page has it too.
    pub fn close_review(&mut self, id: &str) {
        self.state.reviews.retain(|r| r.id != id);
        self.state.dismissed.insert(id.to_owned());
    }

    fn request(&mut self, out: &mut Vec<HostRequest>, kind: Pendingq, request: StorageRequest) {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(id, kind);
        out.push(HostRequest::Storage { id, request });
    }

    fn read_local(&mut self, out: &mut Vec<HostRequest>, kind: Pendingq, paths: Vec<String>) {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(id, kind);
        out.push(HostRequest::ReadLocal { id, paths });
    }

    /// The host answers a request.
    pub fn on_response(&mut self, id: u64, response: HostResponse) {
        if self.pending.contains_key(&id) {
            self.responses.insert(id, response);
        }
    }

    fn take_responses(&mut self) -> Vec<(Pendingq, HostResponse)> {
        let mut out = Vec::new();
        for (id, kind) in std::mem::take(&mut self.pending) {
            if let Some(r) = self.responses.remove(&id) {
                out.push((kind, r));
            }
        }
        out
    }

    fn peer(&mut self, device: &str) -> &mut Peer {
        self.state.peers.entry(device.to_owned()).or_default()
    }

    fn fail(&mut self, device: &str, key: &str, code: &str) {
        let now = self.now;
        let p = self.peer(device);
        let f = p.failures.entry(key.to_owned()).or_insert_with(|| Failure {
            code: code.to_owned(),
            key: key.to_owned(),
            attempts: 0,
            first_ms: now,
            last_ms: now,
        });
        f.attempts += 1;
        f.last_ms = now;
        f.code = code.to_owned();
    }

    fn clear_failure(&mut self, device: &str, key: &str) {
        self.peer(device).failures.remove(key);
    }

    fn storage_problem(&mut self, outcome: &StorageOutcome) -> bool {
        let (root, detail) = match outcome {
            StorageOutcome::Unreachable => (RootState::Unreachable, "unreachable".to_owned()),
            StorageOutcome::Unauthorised | StorageOutcome::Forbidden => {
                (RootState::AuthFailed, "refused".to_owned())
            }
            StorageOutcome::QuotaFull => (RootState::QuotaFull, "full".to_owned()),
            _ => return false,
        };
        self.root = root;
        self.detail = detail;
        true
    }

    fn write_out(
        &mut self,
        log: &mut Durable,
        kind: u8,
        body: &[u8],
        segments: Vec<String>,
        rng: &mut dyn FnMut(&mut [u8]),
    ) -> String {
        let seq = {
            let n = self.state.next.entry(kind).or_insert(1);
            let s = *n;
            *n += 1;
            s
        };
        let bytes = crypto::seal(&self.keys, kind, &self.device, seq, body, rng);
        let key = object_key(&self.state.device, &self.state.notebook, kind, seq);
        let path = outbox_path(kind, seq);
        let sha = sha_hex(&bytes);
        if kind == KIND_SEGMENT {
            self.state.chain = sha.clone();
        }
        let dir = path.rsplit_once('/').map_or("", |(d, _)| d).to_owned();
        log.store.submit(vec![
            IoRequest::CreateDir { path: dir.clone() },
            IoRequest::WriteAt {
                path: path.clone(),
                at: 0,
                data: bytes.clone(),
            },
            IoRequest::SetLen {
                path: path.clone(),
                len: bytes.len() as u64,
            },
            IoRequest::SyncFile { path: path.clone() },
            IoRequest::SyncDir { path: dir },
            IoRequest::SyncDir {
                path: "local/sync/outbox".into(),
            },
            IoRequest::SyncDir {
                path: "local/sync".into(),
            },
            IoRequest::SyncDir {
                path: "local".into(),
            },
        ]);
        self.state.outbox.insert(
            key.clone(),
            Outgoing {
                kind,
                seq,
                key: key.clone(),
                path,
                sha,
                len: bytes.len() as u64,
                written: false,
                segments,
            },
        );
        self.cache.insert(key.clone(), bytes);
        key
    }

    fn persist(&mut self, log: &mut Durable) {
        if let Ok(bytes) = serde_json::to_vec(&self.state) {
            log.store.put_local("sync", &bytes);
        }
    }

    fn pack(
        &mut self,
        log: &mut Durable,
        candidates: &[casual_note_store::SealedSegment],
        files: Vec<(String, Vec<u8>)>,
        rng: &mut dyn FnMut(&mut [u8]),
    ) {
        let map: BTreeMap<String, Vec<u8>> = files.into_iter().collect();
        let mut docs: Vec<(String, Vec<u8>)> = Vec::new();
        let mut paths = Vec::new();
        let mut size = 0u64;
        let limit = crypto::MAX_OBJECT - 1024 * 1024;
        let flush = |this: &mut Self,
                     log: &mut Durable,
                     docs: &mut Vec<(String, Vec<u8>)>,
                     paths: &mut Vec<String>,
                     rng: &mut dyn FnMut(&mut [u8])| {
            if docs.is_empty() {
                return;
            }
            let seq = this.state.next.get(&KIND_SEGMENT).copied().unwrap_or(1);
            let chain: [u8; 32] = unhex(&this.state.chain)
                .and_then(|v| v.try_into().ok())
                .unwrap_or([0; 32]);
            let body = encode_segment_body(seq - 1, &chain, this.now, docs);
            let segs = std::mem::take(paths);
            this.state.packed.extend(segs.iter().cloned());
            this.write_out(log, KIND_SEGMENT, &body, segs, rng);
            docs.clear();
        };
        for c in candidates {
            let Some(bytes) = map.get(&c.path) else {
                continue;
            };
            // Only a whole sealed file leaves the device.
            let scan = scan_segment(bytes);
            if !scan.sealed || scan.damage {
                continue;
            }
            if bytes.len() as u64 > MAX_FILE + 1024 * 1024 {
                self.fail(&hex(&self.device), &c.path, "CN-SYN-0008");
                continue;
            }
            if size + bytes.len() as u64 > limit || docs.len() >= DOCS_PER_OBJECT {
                flush(self, log, &mut docs, &mut paths, rng);
                size = 0;
            }
            size += bytes.len() as u64;
            docs.push((c.name.clone(), bytes.clone()));
            paths.push(c.path.clone());
        }
        flush(self, log, &mut docs, &mut paths, rng);
    }

    fn held(&self, device: &str, seq: u64) -> bool {
        self.state
            .peers
            .get(device)
            .is_some_and(|p| seq <= p.base || p.extra.contains(&seq))
    }

    fn mark_held(&mut self, device: &str, seq: u64, sha: &str) {
        let p = self.peer(device);
        if seq == p.base + 1 {
            p.base = seq;
            p.chain = sha.to_owned();
            while p.extra.remove(&(p.base + 1)) {
                p.base += 1;
                p.chain.clear();
            }
        } else if seq > p.base {
            p.extra.insert(seq);
        }
    }

    /// Files one opened object and imports each document. A refused document stays held here and
    /// is named in the next ack. Other documents go on.
    #[allow(clippy::too_many_arguments)]
    fn apply_segment(
        &mut self,
        ws: &mut Workspace,
        log: &mut Durable,
        device: &str,
        seq: u64,
        sha: &str,
        plain: &[u8],
        key: &str,
    ) {
        let Some(body) = decode_segment_body(plain) else {
            self.fail(device, key, "CN-SYN-0004");
            self.summary.refused += 1;
            return;
        };
        // The hash chain: a differing predecessor is a fork, and nothing from it is imported.
        let (chain, base) = {
            let p = self.peer(device);
            (p.chain.clone(), p.base)
        };
        if body.prev_seq == seq - 1
            && seq - 1 == base
            && !chain.is_empty()
            && hex(&body.prev_hash) != chain
        {
            self.fail(device, key, "CN-SYN-0006");
            self.summary.refused += 1;
            return;
        }
        self.peer(device).claimed_ms = Some(body.sealed_at);
        for (name, file) in &body.docs {
            let (ok, code) = match log.store.file_foreign(name, file.to_vec()) {
                Ok(_) => (true, ""),
                Err(c) => (false, c),
            };
            if !ok {
                let p = self.peer(device);
                p.rejected.push(format!("{code}:{name}"));
                continue;
            }
            let scan = scan_segment(file);
            let mut refused = false;
            for f in scan.frames.iter().filter(|f| f.frame_type == FRAME_BATCH) {
                let Some(update) = format::batch_update(&file[f.payload.0..f.payload.1]) else {
                    continue;
                };
                match ws.import_doc(name, update) {
                    Ok((_, overlaps)) => {
                        log.cursor.mark_part(name, update);
                        for o in overlaps {
                            self.add_review(o, device);
                        }
                        self.summary.changed = true;
                    }
                    Err(_) => {
                        refused = true;
                    }
                }
            }
            if refused {
                self.peer(device)
                    .rejected
                    .push(format!("CN-SYN-0009:{name}"));
                self.fail(device, key, "CN-SYN-0009");
            }
        }
        self.summary.imported_segments += 1;
        self.mark_held(device, seq, sha);
        self.clear_failure(device, key);
    }

    fn add_review(&mut self, o: Overlap, device: &str) {
        if self.state.dismissed.contains(&o.id) {
            return;
        }
        // A later overlap on the same block replaces the earlier one, and links by being newer.
        self.state
            .reviews
            .retain(|r| !(r.page == o.page && r.block == o.block));
        self.state.reviews.push(ReviewMarker {
            id: o.id,
            page: o.page,
            block: o.block,
            base: o.base,
            here: o.here,
            there: o.there,
            merged: o.merged,
            from_device: device.to_owned(),
            found_ms: self.now,
        });
    }

    fn apply_registry(&mut self, device: &str, seq: u64, plain: &[u8], key: &str) {
        let Ok(rec) = serde_json::from_slice::<RegistryRecord>(plain) else {
            self.fail(device, key, "CN-SYN-0004");
            return;
        };
        let p = self.peer(device);
        p.registry_seen.insert(seq);
        if let Some(a) = rec.announce {
            p.name = a.name;
            p.platform = a.platform;
            p.announced = true;
        }
        if let Some(target) = rec.remove {
            // Removed beats active, for good.
            self.peer(&target).removed = true;
        }
        self.clear_failure(device, key);
    }

    fn apply_ack(&mut self, device: &str, seq: u64, plain: &[u8], key: &str) {
        let Ok(ack) = serde_json::from_slice::<AckBody>(plain) else {
            self.fail(device, key, "CN-SYN-0004");
            return;
        };
        let me = self.state.device.clone();
        let p = self.peer(device);
        if seq >= p.ack_seq {
            p.ack_seq = seq;
            for (d, n) in ack.acked {
                // An ack only ever moves forward, so a replayed old one changes nothing.
                let e = p.acks.entry(d).or_insert(0);
                *e = (*e).max(n);
            }
            p.claimed_ms = Some(ack.sealed_at);
        }
        let _ = ack.rejected;
        let _ = me;
        self.clear_failure(device, key);
    }

    fn apply_checkpoint(
        &mut self,
        ws: &mut Workspace,
        log: &mut Durable,
        device: &str,
        seq: u64,
        plain: &[u8],
        key: &str,
    ) {
        if plain.len() < 4 {
            self.fail(device, key, "CN-SYN-0004");
            return;
        }
        let n = u32::from_le_bytes([plain[0], plain[1], plain[2], plain[3]]) as usize;
        let Some(vector) = plain
            .get(4..4usize.saturating_add(n))
            .and_then(|v| serde_json::from_slice::<BTreeMap<String, u64>>(v).ok())
        else {
            self.fail(device, key, "CN-SYN-0004");
            return;
        };
        let container = &plain[4 + n..];
        let Ok(parts) = parse_container(container) else {
            self.fail(device, key, "CN-SYN-0004");
            return;
        };
        // A new device's base. Every part is checked before any is written.
        let mut staged: Vec<(String, Vec<u8>, Vec<IoRequest>)> = Vec::new();
        let owner: [u8; 16] = unhex(device)
            .and_then(|v| v.try_into().ok())
            .unwrap_or([0; 16]);
        for (name, body) in &parts {
            let unique = (self.now as u64).wrapping_mul(4096)
                ^ seq
                ^ u64::from(format::crc32c(key.as_bytes()));
            match base_snapshot_requests(name, body, &owner, unique, self.now) {
                Ok(requests) => staged.push((name.clone(), body.to_vec(), requests)),
                Err(_) => {
                    self.fail(device, key, "CN-SYN-0009");
                    return;
                }
            }
        }
        for (name, body, requests) in staged {
            log.store.submit(requests);
            if ws.import_doc(&name, &body).is_ok() {
                log.cursor.mark_part(&name, &body);
                self.summary.changed = true;
            }
        }
        for (d, n) in &vector {
            if d == &self.state.device {
                continue;
            }
            let p = self.peer(d);
            if *n > p.base {
                p.base = *n;
                p.chain.clear();
                let keep: BTreeSet<u64> = p.extra.iter().copied().filter(|s| s > n).collect();
                p.extra = keep;
                while p.extra.remove(&(p.base + 1)) {
                    p.base += 1;
                }
            }
        }
        if !self.state.checkpoints.iter().any(|c| c.key == key) {
            self.state.checkpoints.push(Checkpoint {
                device: device.to_owned(),
                seq,
                key: key.to_owned(),
                vector,
                verified: false,
            });
        }
        self.clear_failure(device, key);
    }

    /// The acked vector: what this device durably holds, contiguously, per device.
    fn ack_vector(&self) -> BTreeMap<String, u64> {
        self.state
            .peers
            .iter()
            .filter(|(d, _)| **d != self.state.device)
            .map(|(d, p)| (d.clone(), p.base))
            .collect()
    }

    /// The lowest acknowledgement of this device's objects across registered devices. A registered
    /// device that has never acked holds it at zero. Nothing else, and no timer, moves it.
    pub fn floor(&self) -> u64 {
        let me = &self.state.device;
        let highest = self.state.next.get(&KIND_SEGMENT).copied().unwrap_or(1) - 1;
        let mut floor = highest;
        for (d, p) in &self.state.peers {
            // A device known only from a listing counts. Absence is never an instruction.
            if d == me || p.removed {
                continue;
            }
            floor = floor.min(p.acks.get(me).copied().unwrap_or(0));
        }
        floor
    }

    /// One step of the round. Performs what it can against the workspace and log, and returns what
    /// the host must do. Call again after doing it and pumping the store until idle.
    pub fn step(
        &mut self,
        ws: &mut Workspace,
        log: &mut Durable,
        now: i64,
        rng: &mut dyn FnMut(&mut [u8]),
    ) -> Step {
        self.now = now;
        let mut out = Vec::new();
        if log.store.state() == SaveState::Failed && self.phase != Phase::Idle {
            // Nothing is acknowledged on bytes that may not be on disk.
            self.root = RootState::LocalFailed;
            self.detail = "local".into();
            self.pending.clear();
            self.responses.clear();
            self.phase = Phase::Idle;
            return Step {
                requests: out,
                done: true,
            };
        }
        match self.phase {
            Phase::Idle => {
                return Step {
                    requests: out,
                    done: true,
                };
            }
            Phase::Preflight => {
                self.request(
                    &mut out,
                    Pendingq::Root,
                    StorageRequest::Get {
                        key: ROOT_KEY.into(),
                        max_len: 64 * 1024,
                    },
                );
                self.phase = Phase::PreflightDone;
            }
            Phase::PreflightDone => {
                let mut stop = true;
                for (_, r) in self.take_responses() {
                    match r {
                        HostResponse::Storage(StorageOutcome::Got(bytes)) => {
                            match read_root(&bytes) {
                                Some((id, _)) if hex(&id) == self.state.sync_id => {
                                    stop = false;
                                    self.root = RootState::Connected;
                                    self.detail.clear();
                                }
                                _ => self.root = RootState::WrongRoot,
                            }
                        }
                        // Absence is never an instruction. Nothing changes locally, and nothing is written.
                        HostResponse::Storage(StorageOutcome::NotFound) => {
                            self.root = RootState::RootMissing
                        }
                        HostResponse::Storage(o) => {
                            if !self.storage_problem(&o) {
                                self.root = RootState::Unreachable;
                            }
                        }
                        HostResponse::Files(_) => {}
                    }
                }
                self.phase = if stop { Phase::Finish } else { Phase::Save };
            }
            Phase::Save => {
                // Everything typed so far gets into a sealed segment.
                if log.save(ws, now).is_ok() {
                    log.store.seal_all();
                }
                self.phase = Phase::ReadSegments;
            }
            Phase::ReadSegments => {
                if self.state.forked {
                    self.phase = Phase::List;
                    return Step {
                        requests: out,
                        done: false,
                    };
                }
                let me = self.device;
                let candidates: Vec<casual_note_store::SealedSegment> = log
                    .store
                    .sealed_segments()
                    .iter()
                    .filter(|s| s.device == me && !self.state.packed.contains(&s.path))
                    .take(256)
                    .cloned()
                    .collect();
                if candidates.is_empty() {
                    self.phase = Phase::ReadOutbox;
                } else {
                    let paths = candidates.iter().map(|c| c.path.clone()).collect();
                    self.read_local(&mut out, Pendingq::Segments(candidates), paths);
                    self.phase = Phase::Pack;
                }
            }
            Phase::Pack => {
                for (k, r) in self.take_responses() {
                    if let (Pendingq::Segments(c), HostResponse::Files(files)) = (k, r) {
                        self.pack(log, &c, files, rng);
                    }
                }
                self.persist(log);
                self.phase = Phase::ReadOutbox;
            }
            Phase::ReadOutbox => {
                if !self.state.announced {
                    let rec = RegistryRecord {
                        announce: Some(Announce {
                            name: self.state.name.clone(),
                            platform: self.state.platform.clone(),
                            protocol_min: PROTOCOL,
                            protocol_max: PROTOCOL,
                        }),
                        remove: None,
                        sealed_at: now,
                    };
                    if let Ok(body) = serde_json::to_vec(&rec) {
                        self.write_out(log, KIND_REGISTRY, &body, Vec::new(), rng);
                        self.state.announced = true;
                    }
                }
                for target in std::mem::take(&mut self.state.pending_registry) {
                    let rec = RegistryRecord {
                        announce: None,
                        remove: Some(target),
                        sealed_at: now,
                    };
                    if let Ok(body) = serde_json::to_vec(&rec) {
                        self.write_out(log, KIND_REGISTRY, &body, Vec::new(), rng);
                    }
                }
                self.persist(log);
                let paths: Vec<String> = self
                    .state
                    .outbox
                    .values()
                    .filter(|o| !o.written && !self.cache.contains_key(&o.key))
                    .map(|o| o.path.clone())
                    .collect();
                if !paths.is_empty() {
                    self.read_local(&mut out, Pendingq::Outbox, paths);
                }
                self.phase = Phase::Push;
            }
            Phase::Push => {
                for (k, r) in self.take_responses() {
                    if let (Pendingq::Outbox, HostResponse::Files(files)) = (k, r) {
                        let by_path: BTreeMap<String, String> = self
                            .state
                            .outbox
                            .values()
                            .map(|o| (o.path.clone(), o.key.clone()))
                            .collect();
                        for (path, bytes) in files {
                            if let Some(key) = by_path.get(&path) {
                                self.cache.insert(key.clone(), bytes);
                            }
                        }
                    }
                }
                let todo: Vec<String> = self
                    .state
                    .outbox
                    .values()
                    .filter(|o| !o.written)
                    .map(|o| o.key.clone())
                    .collect();
                for key in todo {
                    if let Some(body) = self.cache.get(&key).cloned() {
                        self.request(
                            &mut out,
                            Pendingq::Put(key.clone()),
                            StorageRequest::PutIfAbsent { key, body },
                        );
                    }
                }
                self.phase = Phase::PushDone;
            }
            Phase::PushDone => {
                let me = self.state.device.clone();
                for (k, r) in self.take_responses() {
                    let Pendingq::Put(key) = k else { continue };
                    let HostResponse::Storage(outcome) = r else {
                        continue;
                    };
                    match outcome {
                        StorageOutcome::Created => {
                            if let Some(o) = self.state.outbox.get_mut(&key) {
                                o.written = true;
                            }
                            self.summary.pushed += 1;
                            self.clear_failure(&me, &key);
                        }
                        StorageOutcome::AlreadyExists => {
                            // A retry after a crash, or a clone. The self-check tells them apart.
                            if let Some(o) = self.state.outbox.get_mut(&key) {
                                o.written = true;
                            }
                            self.wants_replace.insert(key);
                        }
                        other => {
                            self.storage_problem(&other);
                            self.fail(&me, &key, "CN-SYN-0001");
                        }
                    }
                }
                self.persist(log);
                self.phase = Phase::List;
            }
            Phase::List => {
                self.request(
                    &mut out,
                    Pendingq::List,
                    StorageRequest::List {
                        prefix: "d/".into(),
                    },
                );
                self.phase = Phase::Listed;
            }
            Phase::Listed => {
                for (_, r) in self.take_responses() {
                    if let HostResponse::Storage(o) = r {
                        match o {
                            StorageOutcome::Listed(entries) => self.listing = entries,
                            other => {
                                self.storage_problem(&other);
                                self.listing.clear();
                                self.phase = Phase::Finish;
                                return Step {
                                    requests: out,
                                    done: false,
                                };
                            }
                        }
                    }
                }
                self.discover(&mut out);
                self.phase = Phase::Process;
            }
            Phase::Process => {
                self.process(ws, log);
                self.persist(log);
                self.phase = Phase::Ack;
            }
            Phase::Ack => {
                self.maybe_checkpoint(ws, log, rng);
                let vector = self.ack_vector();
                if vector != self.state.last_ack_vector && !self.state.forked {
                    let extra: BTreeMap<String, Vec<u64>> = self
                        .state
                        .peers
                        .iter()
                        .filter(|(_, p)| !p.extra.is_empty())
                        .map(|(d, p)| (d.clone(), p.extra.iter().copied().collect()))
                        .collect();
                    let rejected: Vec<String> = self
                        .state
                        .peers
                        .values()
                        .flat_map(|p| p.rejected.iter().cloned())
                        .collect();
                    let body = AckBody {
                        sealed_at: now,
                        protocol_max: PROTOCOL,
                        acked: vector.clone(),
                        extra,
                        rejected,
                    };
                    if let Ok(bytes) = serde_json::to_vec(&body) {
                        self.write_out(log, KIND_ACK, &bytes, Vec::new(), rng);
                        self.state.last_ack_vector = vector;
                    }
                }
                self.persist(log);
                self.phase = Phase::AckPut;
            }
            Phase::AckPut => {
                let todo: Vec<String> = self
                    .state
                    .outbox
                    .values()
                    .filter(|o| !o.written && o.kind != KIND_SEGMENT)
                    .map(|o| o.key.clone())
                    .collect();
                for key in todo {
                    if let Some(body) = self.cache.get(&key).cloned() {
                        self.request(
                            &mut out,
                            Pendingq::Put(key.clone()),
                            StorageRequest::PutIfAbsent { key, body },
                        );
                    }
                }
                self.phase = Phase::Trim;
            }
            Phase::Trim => {
                let me = self.state.device.clone();
                for (k, r) in self.take_responses() {
                    if let (Pendingq::Put(key), HostResponse::Storage(o)) = (k, r) {
                        match o {
                            StorageOutcome::Created | StorageOutcome::AlreadyExists => {
                                if let Some(x) = self.state.outbox.get_mut(&key) {
                                    x.written = true;
                                }
                            }
                            other => {
                                self.storage_problem(&other);
                                self.fail(&me, &key, "CN-SYN-0001");
                            }
                        }
                    }
                }
                self.verify_checkpoints(&mut out);
                self.trim(&mut out, log);
                self.persist(log);
                self.phase = Phase::TrimDone;
            }
            Phase::TrimDone => {
                for (k, r) in self.take_responses() {
                    if let (
                        Pendingq::CheckpointVerify(key, plain),
                        HostResponse::Storage(StorageOutcome::Got(bytes)),
                    ) = (k, r)
                        && let Ok((_, opened)) = crypto::open(&self.keys, &bytes)
                        && opened == plain
                        && let Some(c) = self.state.checkpoints.iter_mut().find(|c| c.key == key)
                    {
                        c.verified = true;
                    }
                }
                self.persist(log);
                self.phase = Phase::Finish;
            }
            Phase::Finish => {
                self.state.last_round_ms = Some(now);
                self.peer_self_seen(now);
                self.persist(log);
                self.phase = Phase::Idle;
                return Step {
                    requests: out,
                    done: true,
                };
            }
        }
        Step {
            requests: out,
            done: false,
        }
    }

    fn peer_self_seen(&mut self, now: i64) {
        let me = self.state.device.clone();
        let (name, platform) = (self.state.name.clone(), self.state.platform.clone());
        let p = self.peer(&me);
        p.name = name;
        p.platform = platform;
        p.announced = true;
        p.observed_ms = Some(now);
    }

    /// Works out from a listing what to fetch.
    fn discover(&mut self, out: &mut Vec<HostRequest>) {
        let me = self.state.device.clone();
        let entries = std::mem::take(&mut self.listing);
        let mut own_listed: BTreeMap<String, u64> = BTreeMap::new();
        let mut wanted: Vec<(u8, u64, String, String, u64)> = Vec::new();
        let mut candidates: Vec<(String, u64)> = Vec::new();
        for e in &entries {
            if e.key.contains("/tmp-") || e.key.ends_with(".cnsk") {
                continue;
            }
            let Some((device, kind, seq)) = parse_key(&e.key) else {
                candidates.push((e.key.clone(), e.len));
                continue;
            };
            if device == me {
                own_listed.insert(e.key.clone(), e.len);
                // A sequence this device has no record of: it was restored, or a clone is running.
                let next = self.state.next.entry(kind).or_insert(1);
                if seq >= *next {
                    *next = seq + 1;
                }
                continue;
            }
            let p = self.peer(&device);
            if kind == KIND_SEGMENT {
                p.highest_listed = p.highest_listed.max(seq);
            }
            match kind {
                KIND_SEGMENT if self.held(&device, seq) => {}
                KIND_REGISTRY if self.peer(&device).registry_seen.contains(&seq) => {}
                KIND_ACK if seq <= self.peer(&device).ack_seq => {}
                KIND_CHECKPOINT if self.state.checkpoints.iter().any(|c| c.key == e.key) => {}
                _ => wanted.push((kind, seq, device, e.key.clone(), e.len)),
            }
        }
        // Own objects: one that vanished goes up again with the same bytes. One that storage lost
        // is the owner's to replace.
        let own: Vec<(String, u64, bool)> = self
            .state
            .outbox
            .values()
            .map(|o| (o.key.clone(), o.len, o.written))
            .collect();
        for (key, len, written) in &own {
            if *written && !own_listed.contains_key(key) {
                if let Some(o) = self.state.outbox.get_mut(key) {
                    o.written = false;
                }
                self.summary.repaired += 1;
            } else if own_listed.get(key).is_some_and(|l| l != len) {
                self.wants_replace.insert(key.clone());
            }
        }
        // Reading own unseen objects back: storage may have changed them, and only the owner can fix that.
        let unseen: Vec<String> = self
            .state
            .outbox
            .values()
            .filter(|o| o.written)
            .map(|o| o.key.clone())
            .collect();
        if !unseen.is_empty() {
            for i in 0..SELF_CHECK_PER_ROUND.min(unseen.len()) {
                self.wants_replace
                    .insert(unseen[(self.check_at + i) % unseen.len()].clone());
            }
            self.check_at = (self.check_at + SELF_CHECK_PER_ROUND) % unseen.len();
        }
        for key in std::mem::take(&mut self.wants_replace) {
            if self.state.outbox.get(&key).is_some_and(|o| o.written) {
                self.request(
                    out,
                    Pendingq::SelfCheck(key.clone()),
                    StorageRequest::Get {
                        key,
                        max_len: crypto::MAX_OBJECT,
                    },
                );
            }
        }
        // Newest checkpoint before segments, and registry and acks first of all: they are small,
        // and they say what the rest means.
        wanted.sort_by_key(|(kind, seq, device, ..)| {
            let rank = match *kind {
                KIND_REGISTRY => 0,
                KIND_ACK => 1,
                KIND_CHECKPOINT => 2,
                _ => 3,
            };
            (rank, device.clone(), *seq)
        });
        // A checkpoint is needed only when segments it covers are gone from the listing.
        let has_seg: BTreeSet<(String, u64)> = entries
            .iter()
            .filter_map(|e| parse_key(&e.key))
            .filter(|(_, k, _)| *k == KIND_SEGMENT)
            .map(|(d, _, s)| (d, s))
            .collect();
        let mut fetched = 0;
        let mut waiting = 0;
        for (kind, _seq, device, key, len) in wanted {
            if len > crypto::MAX_OBJECT {
                self.fail(&device, &key, "CN-SYN-0008");
                continue;
            }
            if kind == KIND_CHECKPOINT {
                // Needed only when what it covers has gone from the listing, or by this device's first look.
                let any_gap = self.state.peers.iter().any(|(d, p)| {
                    *d != me
                        && !has_seg.contains(&(d.clone(), p.base + 1))
                        && p.highest_listed > p.base
                });
                let base = self.peer(&device).base;
                if !(any_gap || (base == 0 && !has_seg.contains(&(device.clone(), 1)))) {
                    continue;
                }
            }
            if fetched >= FETCH_PER_ROUND {
                waiting += 1;
                continue;
            }
            fetched += 1;
            self.request(
                out,
                Pendingq::Fetch(key.clone()),
                StorageRequest::Get {
                    key,
                    max_len: crypto::MAX_OBJECT,
                },
            );
        }
        self.summary.waiting = waiting;
        // A file that fails the name grammar may be a conflict copy. Its identity is in its header.
        for (key, len) in candidates.into_iter().take(256) {
            if key.ends_with(".json") || len > crypto::MAX_OBJECT {
                self.summary.foreign_files += 1;
                continue;
            }
            self.summary.foreign_files += 1;
            self.request(
                out,
                Pendingq::Fetch(key.clone()),
                StorageRequest::Get {
                    key,
                    max_len: crypto::MAX_OBJECT,
                },
            );
        }
    }

    fn process(&mut self, ws: &mut Workspace, log: &mut Durable) {
        let me = self.state.device.clone();
        let mut opened: Vec<(u8, u64, String, String, String, Vec<u8>)> = Vec::new();
        for (kind, r) in self.take_responses() {
            match (kind, r) {
                (Pendingq::SelfCheck(key), HostResponse::Storage(StorageOutcome::Got(bytes))) => {
                    let Some(o) = self.state.outbox.get(&key).cloned() else {
                        continue;
                    };
                    if sha_hex(&bytes) == o.sha {
                        continue;
                    }
                    if crypto::open(&self.keys, &bytes).is_ok() {
                        // Valid under our key and not what we wrote: another copy of this device.
                        self.state.forked = true;
                        self.root = RootState::Forked;
                        self.fail(&me, &key, "CN-SYN-0006");
                    } else {
                        // Changed or truncated in storage. This device owns the name and holds the
                        // bytes: put them back.
                        self.fail(&me, &key, "CN-SYN-0004");
                        self.summary.repaired += 1;
                        self.replace_own(key);
                    }
                }
                (Pendingq::SelfCheck(key), HostResponse::Storage(StorageOutcome::NotFound)) => {
                    if let Some(o) = self.state.outbox.get_mut(&key) {
                        o.written = false;
                    }
                }
                (Pendingq::Fetch(key), HostResponse::Storage(StorageOutcome::Got(bytes))) => {
                    self.summary.fetched += 1;
                    let named = parse_key(&key);
                    match crypto::open(&self.keys, &bytes) {
                        Ok((h, plain)) => {
                            let device = hex(&h.device);
                            if !(1..MAX_SEQ).contains(&h.seq) {
                                self.summary.refused += 1;
                                self.fail(&device, &key, "CN-SYN-0004");
                                continue;
                            }
                            // The sealed header is the identity. The name is only a hint.
                            match &named {
                                Some((d, k, s)) if *d == device && *k == h.kind && *s == h.seq => {}
                                Some(_) => {
                                    self.fail(&device, &key, "CN-SYN-0004");
                                    self.summary.refused += 1;
                                    continue;
                                }
                                None => {}
                            }
                            if device == me {
                                continue; // A copy of our own. Our own bytes are already here.
                            }
                            let sha = sha_hex(&bytes);
                            if named.is_none()
                                && (self.held(&device, h.seq) && h.kind == KIND_SEGMENT)
                            {
                                self.summary.duplicates += 1;
                                continue;
                            }
                            opened.push((h.kind, h.seq, device, sha, key, plain));
                        }
                        Err(refusal) => {
                            let owner = named
                                .as_ref()
                                .map(|n| n.0.clone())
                                .or_else(|| {
                                    crypto::parse_header(&bytes).ok().map(|h| hex(&h.device))
                                })
                                .unwrap_or_else(|| "unknown".into());
                            if named.is_none() {
                                continue; // A stray file, counted and left alone.
                            }
                            if bytes.is_empty() {
                                self.fail(&owner, &key, "CN-SYN-0004");
                                continue;
                            }
                            self.summary.refused += 1;
                            self.fail(&owner, &key, refusal_code(refusal));
                        }
                    }
                }
                (Pendingq::Fetch(key), HostResponse::Storage(outcome)) => {
                    if let Some((d, ..)) = parse_key(&key) {
                        if matches!(outcome, StorageOutcome::NotFound) {
                            continue;
                        }
                        self.storage_problem(&outcome);
                        self.fail(&d, &key, "CN-SYN-0001");
                    }
                }
                _ => {}
            }
        }
        // In order where possible. Whatever is out of order imports anyway, and waits in the CRDT.
        opened.sort_by_key(|(kind, seq, device, ..)| (*kind == KIND_SEGMENT, device.clone(), *seq));
        for (kind, seq, device, sha, key, plain) in opened {
            self.peer(&device).observed_ms = Some(self.now);
            match kind {
                KIND_SEGMENT => self.apply_segment(ws, log, &device, seq, &sha, &plain, &key),
                KIND_REGISTRY => self.apply_registry(&device, seq, &plain, &key),
                KIND_ACK => self.apply_ack(&device, seq, &plain, &key),
                KIND_CHECKPOINT => self.apply_checkpoint(ws, log, &device, seq, &plain, &key),
                _ => self.summary.foreign_files += 1,
            }
        }
    }

    fn replace_own(&mut self, key: String) {
        // Delete then put, across two rounds' worth of requests: the next round's Push sees it unwritten.
        if let Some(o) = self.state.outbox.get_mut(&key) {
            o.written = false;
        }
        self.pending_delete.insert(key);
    }

    fn maybe_checkpoint(
        &mut self,
        ws: &Workspace,
        log: &mut Durable,
        rng: &mut dyn FnMut(&mut [u8]),
    ) {
        let me = self.state.device.clone();
        let highest = self.state.next.get(&KIND_SEGMENT).copied().unwrap_or(1) - 1;
        let covered = self
            .state
            .checkpoints
            .iter()
            .filter(|c| c.device == me && c.verified)
            .filter_map(|c| c.vector.get(&me))
            .copied()
            .max()
            .unwrap_or(0);
        if highest == 0
            || highest - covered < self.config.checkpoint_every
            || !self.checkpoint_plain.is_empty()
        {
            return;
        }
        // Only state that is durable may go in, so only what the log has already saved.
        if !log.store.is_idle() {
            return;
        }
        let Ok(container) = ws.export() else { return };
        let mut vector = self.ack_vector();
        vector.insert(me.clone(), highest);
        let Ok(v) = serde_json::to_vec(&vector) else {
            return;
        };
        let mut body = (v.len() as u32).to_le_bytes().to_vec();
        body.extend_from_slice(&v);
        body.extend_from_slice(&container);
        if body.len() as u64 > self.config.checkpoint_limit {
            return; // Too large for one object. No checkpoint means no trimming, the safe direction.
        }
        let key = self.write_out(log, KIND_CHECKPOINT, &body, Vec::new(), rng);
        let seq = self.state.next.get(&KIND_CHECKPOINT).copied().unwrap_or(1) - 1;
        self.state.checkpoints.push(Checkpoint {
            device: me,
            seq,
            key: key.clone(),
            vector,
            verified: false,
        });
        self.checkpoint_plain.insert(key, body);
    }

    fn verify_checkpoints(&mut self, out: &mut Vec<HostRequest>) {
        let plain = std::mem::take(&mut self.checkpoint_plain);
        for (key, body) in plain {
            if self.state.outbox.get(&key).is_some_and(|o| o.written) {
                self.request(
                    out,
                    Pendingq::CheckpointVerify(key.clone(), body),
                    StorageRequest::Get {
                        key,
                        max_len: crypto::MAX_OBJECT,
                    },
                );
            } else {
                self.checkpoint_plain.insert(key, body);
            }
        }
    }

    /// Deletes own objects that nobody can still need: seen by every registered device, and covered
    /// by a checkpoint this device has read back. Absence of an ack holds this at zero.
    fn trim(&mut self, out: &mut Vec<HostRequest>, log: &mut Durable) {
        for key in std::mem::take(&mut self.pending_delete) {
            self.request(out, Pendingq::Delete, StorageRequest::Delete { key });
        }
        let me = self.state.device.clone();
        let floor = self.floor();
        let verified = self
            .state
            .checkpoints
            .iter()
            .filter(|c| c.device == me && c.verified)
            .filter_map(|c| c.vector.get(&me))
            .copied()
            .max()
            .unwrap_or(0);
        let limit = floor.min(verified);
        let mut gone = Vec::new();
        for o in self.state.outbox.values() {
            let done = match o.kind {
                KIND_SEGMENT => o.written && o.seq <= limit,
                _ => false,
            };
            if done {
                gone.push(o.key.clone());
            }
        }
        for key in gone {
            if let Some(o) = self.state.outbox.remove(&key) {
                self.cache.remove(&key);
                self.summary.trimmed += 1;
                self.request(out, Pendingq::Delete, StorageRequest::Delete { key });
                log.store.submit(vec![
                    IoRequest::Remove {
                        path: o.path.clone(),
                    },
                    IoRequest::SyncDir {
                        path: o.path.rsplit_once('/').map_or("", |(d, _)| d).to_owned(),
                    },
                ]);
            }
        }
        // Older checkpoints of this device: gone once a newer verified one covers them and the floor has passed.
        let newest = self
            .state
            .checkpoints
            .iter()
            .filter(|c| c.device == me && c.verified)
            .map(|c| c.seq)
            .max();
        if let Some(newest) = newest {
            let old: Vec<String> = self
                .state
                .checkpoints
                .iter()
                .filter(|c| {
                    c.device == me
                        && c.seq < newest
                        && c.vector.get(&me).copied().unwrap_or(0) <= floor
                })
                .map(|c| c.key.clone())
                .collect();
            for key in old {
                self.state.checkpoints.retain(|c| c.key != key);
                if let Some(o) = self.state.outbox.remove(&key) {
                    log.store.submit(vec![
                        IoRequest::Remove {
                            path: o.path.clone(),
                        },
                        IoRequest::SyncDir {
                            path: o.path.rsplit_once('/').map_or("", |(d, _)| d).to_owned(),
                        },
                    ]);
                }
                self.cache.remove(&key);
                self.request(out, Pendingq::Delete, StorageRequest::Delete { key });
            }
        }
        // Old acks: keep the newest two.
        let mut acks: Vec<(u64, String, String)> = self
            .state
            .outbox
            .values()
            .filter(|o| o.kind == KIND_ACK && o.written)
            .map(|o| (o.seq, o.key.clone(), o.path.clone()))
            .collect();
        acks.sort();
        if acks.len() > KEEP_ACKS {
            for (_, key, path) in &acks[..acks.len() - KEEP_ACKS] {
                self.state.outbox.remove(key);
                self.cache.remove(key);
                self.request(
                    out,
                    Pendingq::Delete,
                    StorageRequest::Delete { key: key.clone() },
                );
                log.store.submit(vec![
                    IoRequest::Remove { path: path.clone() },
                    IoRequest::SyncDir {
                        path: path.rsplit_once('/').map_or("", |(d, _)| d).to_owned(),
                    },
                ]);
            }
        }
        // Keep written bytes out of memory once nothing needs them soon.
        let written: Vec<String> = self
            .state
            .outbox
            .values()
            .filter(|o| o.written)
            .map(|o| o.key.clone())
            .collect();
        for k in written {
            self.cache.remove(&k);
        }
    }

    /// What the panel shows.
    pub fn status(&self, ws: &Workspace) -> Status {
        let me = &self.state.device;
        let mut devices = Vec::new();
        let mut ids: BTreeSet<&String> = self.state.peers.keys().collect();
        ids.insert(me);
        let segments: Vec<&Outgoing> = self
            .state
            .outbox
            .values()
            .filter(|o| o.kind == KIND_SEGMENT)
            .collect();
        let highest = segments.iter().map(|o| o.seq).max().unwrap_or(0);
        for id in ids {
            let p = self.state.peers.get(id).cloned().unwrap_or_default();
            let this = id == me;
            let (not_written, not_seen) = if this {
                let nw = segments.iter().filter(|o| !o.written).count() as u32;
                let floor = self.floor();
                let ns = segments
                    .iter()
                    .filter(|o| o.written && o.seq > floor)
                    .count() as u32;
                (nw, ns)
            } else {
                let acked = p.acks.get(me).copied().unwrap_or(0);
                (
                    0,
                    segments
                        .iter()
                        .filter(|o| o.written && o.seq > acked)
                        .count() as u32,
                )
            };
            let waiting_from = if this {
                0
            } else {
                p.highest_listed
                    .saturating_sub(p.base + p.extra.len() as u64) as u32
            };
            let (restorable_objects, restorable_bytes) = if this {
                (segments.len() as u32, segments.iter().map(|o| o.len).sum())
            } else {
                let acked = p.acks.get(me).copied().unwrap_or(0);
                let those: Vec<&&Outgoing> = segments.iter().filter(|o| o.seq > acked).collect();
                (those.len() as u32, those.iter().map(|o| o.len).sum())
            };
            let stale = !this
                && p.observed_ms
                    .is_some_and(|t| self.now - t > 30 * 24 * 3600 * 1000);
            devices.push(DeviceRow {
                id: id.clone(),
                name: if this {
                    self.state.name.clone()
                } else {
                    p.name.clone()
                },
                platform: if this {
                    self.state.platform.clone()
                } else {
                    p.platform.clone()
                },
                this_device: this,
                removed: p.removed,
                stale,
                observed_ms: p.observed_ms,
                claimed_ms: p.claimed_ms,
                not_written,
                not_seen,
                waiting_from,
                failed: p.failures.values().cloned().collect(),
                restorable_objects,
                restorable_bytes,
                rejected: p.rejected.clone(),
            });
        }
        let reviews = self
            .state
            .reviews
            .iter()
            .filter(|r| ws.review_choice(&r.page, &r.id).is_none())
            .cloned()
            .collect();
        let holder = self
            .state
            .peers
            .iter()
            .filter(|(d, p)| *d != me && !p.removed)
            .min_by_key(|(_, p)| p.acks.get(me).copied().unwrap_or(0));
        let floor = self.floor();
        let (held_back, held_back_objects) = match holder {
            Some((d, p)) if p.acks.get(me).copied().unwrap_or(0) < highest => (
                Some(d.clone()),
                segments.iter().filter(|o| o.seq > floor).count() as u32,
            ),
            _ => (None, 0),
        };
        Status {
            root: self.root,
            detail: self.detail.clone(),
            this_device: me.clone(),
            devices,
            reviews,
            last_round: self.summary.clone(),
            last_round_ms: self.state.last_round_ms,
            held_back,
            held_back_objects,
            checkpoints: self.state.checkpoints.len() as u32,
            verified_checkpoints: self.state.checkpoints.iter().filter(|c| c.verified).count()
                as u32,
        }
    }

    /// Own objects not yet seen by all registered devices. For tests and the panel.
    pub fn outbox_len(&self) -> usize {
        self.state.outbox.len()
    }

    /// How much of a device's objects is held here, contiguously.
    pub fn state_base(&self, device: &str) -> u64 {
        self.state.peers.get(device).map_or(0, |p| p.base)
    }

    pub fn segment_seq(&self) -> u64 {
        self.state.next.get(&KIND_SEGMENT).copied().unwrap_or(1) - 1
    }
}

fn refusal_code(r: Refusal) -> &'static str {
    r.code()
}
