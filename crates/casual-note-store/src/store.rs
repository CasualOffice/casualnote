//! The store's state machine: recovery on open, then group commit.

use crate::format::*;
use crate::host::{ImageFile, IoError, IoRequest};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// An own segment rolls at this size.
pub const ROLL_AT: u64 = 8 * 1024 * 1024;
/// A segment that crashed the import this many times is treated as poison.
pub const POISON_LIMIT: u32 = 3;
const MANIFEST: &str = "{\"format\":\"casualnote.notebook\",\"storage\":{\"major\":1,\"minor\":0},\"crdt_codec\":1,\"op_schema\":1,\"sync_epoch\":0}";

/// What the host supplies for one open.
#[derive(Clone, Debug)]
pub struct StoreConfig {
    /// Used as this device's id only if the folder has none yet.
    pub new_device: [u8; 16],
    /// Fresh and random for each open. Names this open's segments, so that tabs never collide.
    pub open_id: u32,
    /// The CRDT peer id of this open, also fresh.
    pub peer: u64,
    pub wall_ms: i64,
    /// Other opens still running. Their files are read and never repaired.
    pub live_opens: Vec<u32>,
}

/// Why a folder was not opened. Nothing was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRefused {
    pub code: &'static str,
    pub detail: String,
}

/// Something recovery found that a person may need to know about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub code: &'static str,
    pub doc: Option<String>,
    pub path: String,
    pub detail: String,
}

/// One segment's readable contents.
#[derive(Clone, Debug)]
pub struct SegmentPart {
    pub path: String,
    pub device: [u8; 16],
    pub sequence: u64,
    pub own: bool,
    pub sealed: bool,
    pub updates: Vec<Vec<u8>>,
    /// Poison: left out of the loaded state, with its bytes kept.
    pub excluded: bool,
}

/// One document's readable contents.
#[derive(Clone, Debug, Default)]
pub struct RecoveredDoc {
    pub name: String,
    pub snapshots: Vec<Vec<u8>>,
    pub segments: Vec<SegmentPart>,
}

/// Everything that could be read.
#[derive(Clone, Debug, Default)]
pub struct Recovered {
    pub docs: BTreeMap<String, RecoveredDoc>,
    pub issues: Vec<Issue>,
    pub quarantine: Vec<String>,
    /// The newest valid generation of each small local file.
    pub local: BTreeMap<String, Vec<u8>>,
}

impl Recovered {
    /// A workspace container of all of it, or `None` if there is nothing yet. It is framing only:
    /// each snapshot and update stays as written, and importing it applies them in turn.
    pub fn container(&self) -> Option<Vec<u8>> {
        let mut parts: Vec<(&str, &[u8])> = Vec::new();
        for doc in self.docs.values() {
            for s in &doc.snapshots {
                parts.push((&doc.name, s));
            }
            for seg in doc.segments.iter().filter(|s| !s.excluded) {
                for u in &seg.updates {
                    parts.push((&doc.name, u));
                }
            }
        }
        (!parts.is_empty()).then(|| casual_note_merge::build_container(parts))
    }
}

/// Acknowledges an append once it is durable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ticket(pub u64);

/// An ordered list of requests. Complete it with `on_complete`.
#[derive(Clone, Debug)]
pub struct IoBatch {
    pub id: u64,
    pub requests: Vec<IoRequest>,
}

/// What a batch did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BatchOutcome {
    Acknowledged(Vec<Ticket>),
    /// Nothing was acknowledged. The frames are kept, and go to new segments on the next batch.
    NotSaved(IoError),
}

/// What the person is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveState {
    Saved,
    Saving,
    Failed,
}

/// A segment that is complete and may leave this device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedSegment {
    pub name: String,
    pub path: String,
    pub device: [u8; 16],
    pub sequence: u64,
}

#[derive(Clone, Debug)]
struct Frame {
    name: String,
    update: Vec<u8>,
    origin: u8,
    wall_ms: i64,
    ticket: Ticket,
}

#[derive(Clone, Debug)]
enum Queued {
    Frame(Frame),
    Foreign {
        name: String,
        path: String,
        bytes: Vec<u8>,
        ticket: Ticket,
    },
    Raw {
        requests: Vec<IoRequest>,
        ticket: Option<Ticket>,
    },
}

struct Active {
    path: String,
    sequence: u64,
    len: u64,
    frames: u64,
    prev_crc: u32,
    sha: Sha256,
}

impl std::fmt::Debug for Active {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Active")
            .field("path", &self.path)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct InFlight {
    id: u64,
    queued: Vec<Queued>,
    touched: Vec<String>,
    sealed: Vec<SealedSegment>,
    tickets: Vec<Ticket>,
}

/// The log of one open.
#[derive(Debug)]
pub struct Store {
    device: [u8; 16],
    open_id: u32,
    peer: u64,
    wall_ms: i64,
    next_seq: u32,
    next_ticket: u64,
    next_batch_id: u64,
    queue: VecDeque<Queued>,
    seal_every: bool,
    in_flight: Option<InFlight>,
    acked: Vec<Ticket>,
    active: BTreeMap<String, Active>,
    /// Documents with a valid `doc.id` on disk.
    ready: BTreeSet<String>,
    reason: BTreeMap<String, u8>,
    last_sha: BTreeMap<String, [u8; 32]>,
    known: BTreeMap<String, [u8; 32]>,
    sealed: Vec<SealedSegment>,
    failed: Option<IoError>,
    local_gen: BTreeMap<String, u64>,
    poison: BTreeMap<String, u32>,
    verified: BTreeSet<String>,
    /// Own poison segments: their document shows what came before them.
    blocked: BTreeMap<String, String>,
    created_local: bool,
    /// What recent batches held, for the broadcast to other tabs.
    last_acked_updates: Vec<(String, Vec<u8>)>,
}

/// The path of generation `gen` of a small local file.
pub fn local_path(name: &str, generation: u64) -> String {
    format!("local/{name}-{generation:016x}.idx")
}

fn parse_local_path(path: &str) -> Option<(&str, u64)> {
    let stem = path.strip_prefix("local/")?.strip_suffix(".idx")?;
    if stem.contains('/') {
        return None;
    }
    let (name, g) = stem.rsplit_once('-')?;
    if g.len() != 16 {
        return None;
    }
    Some((name, u64::from_str_radix(g, 16).ok()?))
}

/// The files of one document in an image.
pub fn doc_dir_files<'a>(image: &'a [ImageFile], name: &str) -> Vec<&'a ImageFile> {
    let prefix = format!("{}/", doc_dir(name));
    image
        .iter()
        .filter(|f| f.path.starts_with(&prefix))
        .collect()
}

fn parent_chain(dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut p = dir;
    while let Some((up, _)) = p.rsplit_once('/') {
        out.push(up.to_owned());
        p = up;
    }
    out.push(String::new());
    out
}

fn latest_local<'a>(
    image: &'a [ImageFile],
    want: &str,
) -> (Option<&'a [u8]>, Option<u64>, Vec<String>) {
    let mut best: Option<(u64, &[u8])> = None;
    let mut top = None;
    let mut all = Vec::new();
    for f in image {
        if let Some((name, g)) = parse_local_path(&f.path)
            && name == want
        {
            all.push(f.path.clone());
            top = top.max(Some(g));
            if let Some(body) = parse_local(&f.bytes)
                && best.is_none_or(|(b, _)| g > b)
            {
                best = Some((g, body));
            }
        }
    }
    (best.map(|(_, b)| b), top, all)
}

fn encode_table(table: &BTreeMap<String, u32>) -> Vec<u8> {
    let mut out = Vec::new();
    for (path, n) in table {
        out.extend_from_slice(path.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(n.to_string().as_bytes());
        out.push(b'\n');
    }
    out
}

fn decode_table(bytes: &[u8]) -> BTreeMap<String, u32> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = text.lines();
    let mut out = BTreeMap::new();
    while let (Some(path), Some(n)) = (lines.next(), lines.next()) {
        if let Ok(n) = n.parse() {
            out.insert(path.to_owned(), n);
        }
    }
    out
}

/// Splits a path under a document's directory into the directory and what follows.
fn split_doc_path(path: &str) -> Option<(&str, &str, &str)> {
    let (dir, file) = path.rsplit_once('/')?;
    if let Some(doc_dir) = dir.strip_suffix("/log") {
        return Some((doc_dir, "log", file));
    }
    if let Some(doc_dir) = dir.strip_suffix("/snapshot") {
        return Some((doc_dir, "snapshot", file));
    }
    if file == "doc.id" {
        return Some((dir, "id", file));
    }
    None
}

impl Store {
    /// Reads everything the host holds, and works out what can be recovered. Repairs are queued
    /// as the first batch. Damage is never fatal to the rest: each file is judged on its own.
    pub fn open(
        image: Vec<ImageFile>,
        cfg: &StoreConfig,
    ) -> Result<(Self, Recovered), OpenRefused> {
        Self::open_inner(image, cfg, false)
    }

    /// Reads only: no repair, no writes, and nothing is treated as finished. For checking what is on disk.
    pub fn read_only(image: Vec<ImageFile>, cfg: &StoreConfig) -> Result<Recovered, OpenRefused> {
        Ok(Self::open_inner(image, cfg, true)?.1)
    }

    fn open_inner(
        image: Vec<ImageFile>,
        cfg: &StoreConfig,
        read_only: bool,
    ) -> Result<(Self, Recovered), OpenRefused> {
        let any_log = image
            .iter()
            .any(|f| f.path.ends_with(".seg") || f.path.ends_with(".snap"));
        let manifest = image.iter().find(|f| f.path == "manifest.json");
        let manifest_valid = match manifest {
            Some(f) => {
                let text = String::from_utf8_lossy(&f.bytes);
                // A torn manifest says nothing about versions. Only a whole one can refuse.
                let whole =
                    text.starts_with("{\"format\":\"casualnote.notebook\"") && text.ends_with('}');
                if whole && !text.contains("\"major\":1,") {
                    return Err(OpenRefused {
                        code: "CN-STO-0002",
                        detail: "written by a newer version".into(),
                    });
                }
                whole
            }
            None => false,
        };
        if !manifest_valid && any_log && manifest.is_some() {
            return Err(OpenRefused {
                code: "CN-STO-0001",
                detail: "the manifest is invalid".into(),
            });
        }
        let (device_body, device_top, device_paths) = latest_local(&image, "device");
        let device: Option<[u8; 16]> = device_body.and_then(|b| b.try_into().ok());
        let (poison_body, poison_top, poison_paths) = latest_local(&image, "poison");
        let (verified_body, verified_top, verified_paths) = latest_local(&image, "verified");
        let mut store = Store {
            device: device.unwrap_or(cfg.new_device),
            open_id: cfg.open_id,
            peer: cfg.peer,
            wall_ms: cfg.wall_ms,
            next_seq: 1,
            next_ticket: 1,
            next_batch_id: 1,
            queue: VecDeque::new(),
            seal_every: false,
            in_flight: None,
            acked: Vec::new(),
            active: BTreeMap::new(),
            ready: BTreeSet::new(),
            reason: BTreeMap::new(),
            last_sha: BTreeMap::new(),
            known: BTreeMap::new(),
            sealed: Vec::new(),
            failed: None,
            local_gen: BTreeMap::new(),
            poison: poison_body.map(decode_table).unwrap_or_default(),
            verified: verified_body
                .map(decode_table)
                .unwrap_or_default()
                .into_keys()
                .collect(),
            blocked: BTreeMap::new(),
            created_local: false,
            last_acked_updates: Vec::new(),
        };
        store
            .local_gen
            .insert("device".into(), device_top.unwrap_or(0));
        store
            .local_gen
            .insert("poison".into(), poison_top.unwrap_or(0));
        store
            .local_gen
            .insert("verified".into(), verified_top.unwrap_or(0));
        let mut repairs: Vec<IoRequest> = Vec::new();
        let mut rec = Recovered::default();
        // Every other local file: the newest valid generation is what counts, and generations rise.
        let mut others = BTreeSet::new();
        for f in &image {
            if let Some((name, _)) = parse_local_path(&f.path)
                && !["device", "poison", "verified"].contains(&name)
            {
                others.insert(name.to_owned());
            }
        }
        for name in others {
            let (body, top, paths) = latest_local(&image, &name);
            let keep = top.unwrap_or(0);
            if let Some(b) = body {
                rec.local.insert(name.clone(), b.to_vec());
            }
            store.local_gen.insert(name.clone(), keep);
            for p in paths {
                // An older generation is removed only after a newer one has been read as valid.
                if body.is_some() && parse_local_path(&p).is_some_and(|(_, g)| g + 1 < keep) {
                    repairs.push(IoRequest::Remove { path: p });
                }
            }
        }

        // A manifest and a device id are durable before any segment can exist.
        if !manifest_valid {
            repairs.extend([
                IoRequest::WriteAt {
                    path: "manifest.json".into(),
                    at: 0,
                    data: MANIFEST.as_bytes().to_vec(),
                },
                IoRequest::SetLen {
                    path: "manifest.json".into(),
                    len: MANIFEST.len() as u64,
                },
                IoRequest::SyncFile {
                    path: "manifest.json".into(),
                },
                IoRequest::SyncDir {
                    path: String::new(),
                },
            ]);
        }
        repairs.push(IoRequest::CreateDir {
            path: "local/quarantine".into(),
        });
        repairs.push(IoRequest::SyncDir {
            path: "local".into(),
        });
        repairs.push(IoRequest::SyncDir {
            path: String::new(),
        });
        if device.is_none() {
            store.created_local = true;
            let id = store.device;
            store.local_write("device", &id, &mut repairs);
        }
        // Older generations of local files, and ones whose write was torn.
        for (paths, name) in [
            (&device_paths, "device"),
            (&poison_paths, "poison"),
            (&verified_paths, "verified"),
        ] {
            let keep = store.local_gen.get(name).copied().unwrap_or(0);
            for p in paths {
                if p != &local_path(name, keep)
                    && parse_local_path(p).is_some_and(|(_, g)| g + 1 < keep)
                {
                    repairs.push(IoRequest::Remove { path: p.clone() });
                }
            }
        }

        // The document names, from their doc.id files.
        let mut names: BTreeMap<&str, String> = BTreeMap::new();
        names.insert("tree", String::new());
        for f in &image {
            if let Some((dir, "id", _)) = split_doc_path(&f.path)
                && let Ok(id) = std::str::from_utf8(&f.bytes)
            {
                let name = if dir.starts_with("databases/") {
                    format!("d:{id}")
                } else {
                    id.to_owned()
                };
                if doc_dir(&name) == dir {
                    names.insert(dir, name);
                }
            }
        }
        for name in names.values() {
            store.ready.insert(name.clone());
        }
        store.ready.remove("");
        store.ready.insert(String::new());
        // The tree's directories are not there until something made them. Assume not.
        let tree_exists = image.iter().any(|f| f.path.starts_with("tree/log/"));
        if !tree_exists {
            store.ready.remove("");
        }
        for f in &image {
            if f.path.starts_with("local/quarantine/") {
                rec.quarantine.push(f.path.clone());
            }
        }
        for f in &image {
            let Some((dir, kind, file)) = split_doc_path(&f.path) else {
                continue;
            };
            let Some(name) = names.get(dir).cloned() else {
                if kind != "id" && !f.bytes.is_empty() {
                    rec.issues.push(Issue {
                        code: "CN-STO-0019",
                        doc: None,
                        path: f.path.clone(),
                        detail: "this file has no document name".into(),
                    });
                }
                continue;
            };
            match kind {
                "snapshot" => {
                    if let Some(s) = parse_snapshot(&f.bytes)
                        && s.doc_key == doc_key(&name)
                    {
                        rec.docs
                            .entry(name.clone())
                            .or_insert_with(|| RecoveredDoc {
                                name,
                                ..Default::default()
                            })
                            .snapshots
                            .push(s.payload);
                    } else if !f.bytes.is_empty() {
                        // A torn or stale snapshot is harmless. Compaction removes it.
                        rec.issues.push(Issue {
                            code: "CN-STO-0012",
                            doc: Some(name),
                            path: f.path.clone(),
                            detail: "ignored".into(),
                        });
                    }
                }
                "log" => {
                    let Some((device, sequence)) = parse_segment_name(file) else {
                        rec.issues.push(Issue {
                            code: "CN-STO-0004",
                            doc: Some(name),
                            path: f.path.clone(),
                            detail: "unexpected file name".into(),
                        });
                        continue;
                    };
                    store.read_segment(
                        &name,
                        dir,
                        &f.path,
                        device,
                        sequence,
                        &f.bytes,
                        cfg,
                        read_only,
                        &mut repairs,
                        &mut rec,
                    );
                }
                _ => {}
            }
        }
        // Poison: count what crashed, and act on it. Own bytes are never moved.
        let poison = std::mem::take(&mut store.poison);
        for (path, attempts) in poison {
            let Some((doc, idx)) = rec.docs.iter().find_map(|(n, d)| {
                d.segments
                    .iter()
                    .position(|s| s.path == path)
                    .map(|i| (n.clone(), i))
            }) else {
                continue;
            };
            if attempts < POISON_LIMIT {
                store.poison.insert(path, attempts);
                continue;
            }
            let own = rec.docs[&doc].segments[idx].own;
            if own {
                store.poison.insert(path.clone(), attempts);
                store.blocked.insert(doc.clone(), path.clone());
                if let Some(d) = rec.docs.get_mut(&doc) {
                    d.segments[idx].excluded = true;
                }
                rec.issues.push(Issue {
                    code: "CN-STO-0021",
                    doc: Some(doc),
                    path,
                    detail: "this device's own segment crashed the import 3 times".into(),
                });
            } else if let Some(f) = image.iter().find(|f| f.path == path) {
                let target = format!("local/quarantine/{}", path.replace('/', "_"));
                repairs.extend([
                    IoRequest::WriteAt {
                        path: target.clone(),
                        at: 0,
                        data: f.bytes.clone(),
                    },
                    IoRequest::SyncFile {
                        path: target.clone(),
                    },
                    IoRequest::SyncDir {
                        path: "local/quarantine".into(),
                    },
                    IoRequest::Remove { path: path.clone() },
                    IoRequest::SyncDir {
                        path: path.rsplit_once('/').map_or("", |(d, _)| d).to_owned(),
                    },
                ]);
                if let Some(d) = rec.docs.get_mut(&doc) {
                    d.segments.remove(idx);
                }
                store.sealed.retain(|s| s.path != path);
                rec.quarantine.push(target);
                rec.issues.push(Issue {
                    code: "CN-STO-0016",
                    doc: Some(doc),
                    path,
                    detail: "another device's segment crashed the import 3 times".into(),
                });
            }
        }
        if !read_only && !repairs.is_empty() {
            store.queue.push_back(Queued::Raw {
                requests: repairs,
                ticket: None,
            });
        }
        Ok((store, rec))
    }

    #[allow(clippy::too_many_arguments)]
    fn read_segment(
        &mut self,
        name: &str,
        dir: &str,
        path: &str,
        device: [u8; 16],
        sequence: u64,
        bytes: &[u8],
        cfg: &StoreConfig,
        read_only: bool,
        repairs: &mut Vec<IoRequest>,
        rec: &mut Recovered,
    ) {
        let scan = scan_segment(bytes);
        let own = device == self.device;
        let live = own && cfg.live_opens.contains(&((sequence >> 32) as u32));
        let log_dir = format!("{dir}/log");
        let mut updates = Vec::new();
        let mut peer_bad = false;
        for f in &scan.frames {
            if f.frame_type == FRAME_BATCH {
                match batch_update(&bytes[f.payload.0..f.payload.1]) {
                    Some(u) => updates.push(u.to_vec()),
                    None => peer_bad = true,
                }
            }
        }
        if peer_bad {
            rec.issues.push(Issue {
                code: "CN-STO-0005",
                doc: Some(name.to_owned()),
                path: path.to_owned(),
                detail: "a frame's contents are malformed".into(),
            });
        }
        let quarantine_name = path.replace('/', "_");
        match &scan.header {
            Ok(h) if h.doc_key != doc_key(name) || h.sequence != sequence || h.device != device => {
                // The file does not belong where it was found: a segment cannot be filed under another document.
                rec.issues.push(Issue {
                    code: "CN-STO-0004",
                    doc: Some(name.to_owned()),
                    path: path.to_owned(),
                    detail: "name and contents disagree".into(),
                });
                if !read_only && !own && !live {
                    let target = format!("local/quarantine/{quarantine_name}");
                    repairs.extend([
                        IoRequest::WriteAt {
                            path: target.clone(),
                            at: 0,
                            data: bytes.to_vec(),
                        },
                        IoRequest::SyncFile {
                            path: target.clone(),
                        },
                        IoRequest::SyncDir {
                            path: "local/quarantine".into(),
                        },
                        IoRequest::Remove {
                            path: path.to_owned(),
                        },
                        IoRequest::SyncDir { path: log_dir },
                    ]);
                    rec.quarantine.push(target);
                }
                return;
            }
            _ => {}
        }
        if live {
            // Another tab is writing this file. Read what is whole, and leave it alone.
        } else if scan.sealed && !scan.damage {
            if let Some(sha) = scan.sha {
                self.known.insert(path.to_owned(), sha);
            }
            self.sealed.push(SealedSegment {
                name: name.to_owned(),
                path: path.to_owned(),
                device,
                sequence,
            });
        } else if !own && !scan.sealed && !scan.seal_mismatch && scan.header.is_ok() {
            // A foreign file is only ever filed whole, so this one was torn while being filed, and
            // was never acknowledged. Filing it again replaces it. Sealing it here would fork.
        } else if !own && (scan.seal_mismatch || scan.damage) {
            rec.issues.push(Issue {
                code: "CN-STO-0006",
                doc: Some(name.to_owned()),
                path: path.to_owned(),
                detail: "a sealed segment's hash does not match".into(),
            });
            if !read_only {
                let target = format!("local/quarantine/{quarantine_name}");
                repairs.extend([
                    IoRequest::WriteAt {
                        path: target.clone(),
                        at: 0,
                        data: bytes.to_vec(),
                    },
                    IoRequest::SyncFile {
                        path: target.clone(),
                    },
                    IoRequest::SyncDir {
                        path: "local/quarantine".into(),
                    },
                    IoRequest::Remove {
                        path: path.to_owned(),
                    },
                    IoRequest::SyncDir { path: log_dir },
                ]);
                rec.quarantine.push(target);
            }
            return;
        } else if scan.header.is_err() && !scan.frames.is_empty() {
            // The header is bad but frames are readable: they may be acknowledged edits. Keep the
            // file as it is, use the frames, and say so.
            rec.issues.push(Issue {
                code: "CN-STO-0004",
                doc: Some(name.to_owned()),
                path: path.to_owned(),
                detail: "the segment's header is damaged and its frames were used".into(),
            });
        } else if scan.frames.iter().all(|f| f.frame_type != FRAME_BATCH) {
            // Never reached an acknowledgement. Bytes are moved aside before anything is cut.
            if !read_only {
                if !bytes.is_empty() && scan.good_end < bytes.len() {
                    let target = format!("local/quarantine/{quarantine_name}.tail");
                    repairs.extend([
                        IoRequest::WriteAt {
                            path: target.clone(),
                            at: 0,
                            data: bytes.to_vec(),
                        },
                        IoRequest::SyncFile {
                            path: target.clone(),
                        },
                        IoRequest::SyncDir {
                            path: "local/quarantine".into(),
                        },
                    ]);
                    rec.quarantine.push(target);
                }
                repairs.extend([
                    IoRequest::Remove {
                        path: path.to_owned(),
                    },
                    IoRequest::SyncDir { path: log_dir },
                ]);
            }
            return;
        } else {
            // A log that a dead open left: find the last good frame, set the rest aside, and seal.
            let damaged = scan.damage || scan.seal_mismatch;
            let keep_end = if scan.seal_mismatch || scan.sealed {
                scan.frames.last().map_or(HEADER_LEN, |f| f.offset)
            } else {
                scan.good_end
            };
            let kept: Vec<&ScannedFrame> = scan
                .frames
                .iter()
                .filter(|f| f.frame_type != FRAME_SEAL)
                .collect();
            if !read_only {
                if damaged {
                    let target = format!("local/quarantine/{quarantine_name}.whole");
                    repairs.extend([
                        IoRequest::WriteAt {
                            path: target.clone(),
                            at: 0,
                            data: bytes.to_vec(),
                        },
                        IoRequest::SyncFile {
                            path: target.clone(),
                        },
                        IoRequest::SyncDir {
                            path: "local/quarantine".into(),
                        },
                    ]);
                    rec.quarantine.push(target);
                    self.reason.insert(name.to_owned(), 7);
                    rec.issues.push(Issue {
                        code: "CN-STO-0005",
                        doc: Some(name.to_owned()),
                        path: path.to_owned(),
                        detail:
                            "damage in the middle of a segment: frames after it are in quarantine"
                                .into(),
                    });
                } else if keep_end < bytes.len() {
                    let target = format!("local/quarantine/{quarantine_name}.tail");
                    repairs.extend([
                        IoRequest::WriteAt {
                            path: target.clone(),
                            at: 0,
                            data: bytes[keep_end..].to_vec(),
                        },
                        IoRequest::SyncFile {
                            path: target.clone(),
                        },
                        IoRequest::SyncDir {
                            path: "local/quarantine".into(),
                        },
                    ]);
                    rec.quarantine.push(target);
                    self.reason.insert(name.to_owned(), 2);
                }
                if keep_end < bytes.len() {
                    repairs.extend([
                        IoRequest::SetLen {
                            path: path.to_owned(),
                            len: keep_end as u64,
                        },
                        IoRequest::SyncFile {
                            path: path.to_owned(),
                        },
                    ]);
                }
                let sha: [u8; 32] = Sha256::digest(&bytes[..keep_end]).into();
                let prev = kept.last().map_or(0, |f| f.crc);
                let seal = seal_payload(kept.len() as u64, keep_end as u64, &sha);
                let frame = encode_frame(FRAME_SEAL, kept.len() as u64, prev, &seal);
                repairs.extend([
                    IoRequest::WriteAt {
                        path: path.to_owned(),
                        at: keep_end as u64,
                        data: frame.clone(),
                    },
                    IoRequest::SyncFile {
                        path: path.to_owned(),
                    },
                ]);
                let mut h = Sha256::new();
                h.update(&bytes[..keep_end]);
                h.update(&frame);
                self.known.insert(path.to_owned(), h.finalize().into());
                self.sealed.push(SealedSegment {
                    name: name.to_owned(),
                    path: path.to_owned(),
                    device,
                    sequence,
                });
            }
        }
        rec.docs
            .entry(name.to_owned())
            .or_insert_with(|| RecoveredDoc {
                name: name.to_owned(),
                ..Default::default()
            })
            .segments
            .push(SegmentPart {
                path: path.to_owned(),
                device,
                sequence,
                own,
                sealed: scan.sealed,
                updates,
                excluded: false,
            });
    }

    pub fn device(&self) -> [u8; 16] {
        self.device
    }

    pub fn peer(&self) -> u64 {
        self.peer
    }

    fn local_write(&mut self, name: &str, payload: &[u8], out: &mut Vec<IoRequest>) {
        let old = self.local_gen.get(name).copied().unwrap_or(0);
        let generation = old + 1;
        self.local_gen.insert(name.to_owned(), generation);
        let path = local_path(name, generation);
        out.extend([
            IoRequest::CreateDir {
                path: "local".into(),
            },
            IoRequest::WriteAt {
                path: path.clone(),
                at: 0,
                data: encode_local(payload),
            },
            IoRequest::SyncFile { path: path.clone() },
            IoRequest::SyncDir {
                path: "local".into(),
            },
            IoRequest::SyncDir {
                path: String::new(),
            },
        ]);
        if old > 0 {
            out.push(IoRequest::Remove {
                path: local_path(name, old),
            });
            out.push(IoRequest::SyncDir {
                path: "local".into(),
            });
        }
    }

    /// Queues a small local file, the newest valid generation of which wins on the next open.
    pub fn put_local(&mut self, name: &str, payload: &[u8]) -> Ticket {
        let mut requests = Vec::new();
        self.local_write(name, payload, &mut requests);
        self.submit(requests)
    }

    /// Queues requests to run in order as part of the next batch. Their ticket says when they finished.
    pub fn submit(&mut self, requests: Vec<IoRequest>) -> Ticket {
        let ticket = self.ticket();
        self.queue.push_back(Queued::Raw {
            requests,
            ticket: Some(ticket),
        });
        ticket
    }

    fn ticket(&mut self) -> Ticket {
        let t = Ticket(self.next_ticket);
        self.next_ticket += 1;
        t
    }

    /// Adds an update to the log. The ticket comes back once it is durable.
    pub fn append(&mut self, name: &str, update: Vec<u8>, origin: u8, wall_ms: i64) -> Ticket {
        let ticket = self.ticket();
        self.queue.push_back(Queued::Frame(Frame {
            name: name.to_owned(),
            update,
            origin,
            wall_ms,
            ticket,
        }));
        ticket
    }

    /// Files a sealed segment that came from another device, unchanged. Its seal must check, and
    /// it must name this document. The same bytes twice do nothing; different bytes are a fork.
    pub fn file_foreign(&mut self, name: &str, bytes: Vec<u8>) -> Result<Ticket, &'static str> {
        let scan = scan_segment(&bytes);
        let Ok(header) = &scan.header else {
            return Err("CN-STO-0004");
        };
        if !scan.sealed || scan.damage {
            return Err("CN-STO-0006");
        }
        if header.doc_key != doc_key(name) {
            return Err("CN-STO-0004");
        }
        let path = format!(
            "{}/log/{}",
            doc_dir(name),
            segment_name(&header.device, header.sequence)
        );
        let sha = scan.sha.unwrap_or([0; 32]);
        let ticket = self.ticket();
        match self.known.get(&path) {
            Some(known) if *known == sha => {
                self.acked.push(ticket);
                return Ok(ticket);
            }
            Some(_) => return Err("CN-STO-0007"),
            None => {}
        }
        self.known.insert(path.clone(), sha);
        self.sealed.push(SealedSegment {
            name: name.to_owned(),
            path: path.clone(),
            device: header.device,
            sequence: header.sequence,
        });
        self.queue.push_back(Queued::Foreign {
            name: name.to_owned(),
            path,
            bytes,
            ticket,
        });
        Ok(ticket)
    }

    /// Seals every active segment on the next batch, so it may leave this device.
    pub fn seal_all(&mut self) {
        self.seal_every = true;
    }

    /// Forgets files removed by compaction.
    pub fn forget(&mut self, paths: &[String]) {
        for p in paths {
            self.known.remove(p);
            self.verified.remove(p);
        }
        self.sealed.retain(|s| !paths.contains(&s.path));
    }

    /// Every sealed segment known, in the order sealed. The bytes are read from the host.
    pub fn sealed_segments(&self) -> &[SealedSegment] {
        &self.sealed
    }

    fn ensure_doc(&mut self, name: &str, reqs: &mut Vec<IoRequest>) {
        let dir = doc_dir(name);
        let log = format!("{dir}/log");
        if self.ready.contains(name) {
            return;
        }
        reqs.push(IoRequest::CreateDir { path: log.clone() });
        if !name.is_empty() {
            let id = name.strip_prefix("d:").unwrap_or(name).as_bytes().to_vec();
            let path = format!("{dir}/doc.id");
            reqs.extend([
                IoRequest::WriteAt {
                    path: path.clone(),
                    at: 0,
                    data: id.clone(),
                },
                IoRequest::SetLen {
                    path: path.clone(),
                    len: id.len() as u64,
                },
                IoRequest::SyncFile { path },
            ]);
        }
        reqs.push(IoRequest::SyncDir { path: log.clone() });
        reqs.push(IoRequest::SyncDir { path: dir.clone() });
        for up in parent_chain(&dir) {
            reqs.push(IoRequest::SyncDir { path: up });
        }
        self.ready.insert(name.to_owned());
    }

    fn start_segment(&mut self, name: &str, reqs: &mut Vec<IoRequest>, touched: &mut Vec<String>) {
        if self.active.contains_key(name) {
            return;
        }
        self.ensure_doc(name, reqs);
        let dir = doc_dir(name);
        let log = format!("{dir}/log");
        let sequence = (u64::from(self.open_id) << 32) | u64::from(self.next_seq);
        self.next_seq += 1;
        let header = SegmentHeader {
            doc_class: doc_class(name),
            device: self.device,
            sequence,
            created_ms: self.wall_ms,
            doc_key: doc_key(name),
            prev_sha: self.last_sha.get(name).copied().unwrap_or([0; 32]),
        };
        let bytes = encode_segment_header(&header);
        let path = format!("{log}/{}", segment_name(&self.device, sequence));
        reqs.extend([
            IoRequest::WriteAt {
                path: path.clone(),
                at: 0,
                data: bytes.to_vec(),
            },
            IoRequest::SyncFile { path: path.clone() },
            IoRequest::SyncDir { path: log },
        ]);
        let reason = self.reason.remove(name).unwrap_or(6);
        let payload = peer_payload(self.peer, reason);
        let frame = encode_frame(FRAME_PEER, 0, 0, &payload);
        reqs.extend([
            IoRequest::WriteAt {
                path: path.clone(),
                at: HEADER_LEN as u64,
                data: frame.clone(),
            },
            IoRequest::SyncFile { path: path.clone() },
        ]);
        let mut sha = Sha256::new();
        sha.update(bytes);
        sha.update(&frame);
        self.active.insert(
            name.to_owned(),
            Active {
                path,
                sequence,
                len: (HEADER_LEN + frame.len()) as u64,
                frames: 1,
                prev_crc: crc32c(&payload),
                sha,
            },
        );
        touched.push(name.to_owned());
    }

    /// Group commit. If no flush is running, every frame waiting goes into one batch, and is
    /// acknowledged together when it completes. There is no timer here: an idle system flushes at
    /// once, and a slow disk groups more.
    pub fn next_batch(&mut self) -> Option<IoBatch> {
        if self.in_flight.is_some() || (self.queue.is_empty() && !self.seal_every) {
            return None;
        }
        let queued: Vec<Queued> = self.queue.drain(..).collect();
        let mut reqs: Vec<IoRequest> = Vec::new();
        let mut touched: Vec<String> = Vec::new();
        let mut sealed_now: Vec<SealedSegment> = Vec::new();
        let mut tickets = Vec::new();
        let mut buffers: BTreeMap<String, (String, u64, Vec<u8>)> = BTreeMap::new();
        let flush_buffer = |reqs: &mut Vec<IoRequest>, buf: (String, u64, Vec<u8>)| {
            reqs.push(IoRequest::WriteAt {
                path: buf.0.clone(),
                at: buf.1,
                data: buf.2,
            });
            reqs.push(IoRequest::SyncFile { path: buf.0 });
        };
        let mut seen_touch = BTreeSet::new();
        for item in &queued {
            match item {
                Queued::Raw { requests, ticket } => {
                    reqs.extend(requests.iter().cloned());
                    if let Some(t) = ticket {
                        tickets.push(*t);
                    }
                }
                Queued::Foreign {
                    name,
                    path,
                    bytes,
                    ticket,
                } => {
                    self.ensure_doc(name, &mut reqs);
                    reqs.extend([
                        IoRequest::WriteAt {
                            path: path.clone(),
                            at: 0,
                            data: bytes.clone(),
                        },
                        IoRequest::SyncFile { path: path.clone() },
                        IoRequest::SyncDir {
                            path: format!("{}/log", doc_dir(name)),
                        },
                    ]);
                    tickets.push(*ticket);
                }
                Queued::Frame(frame) => {
                    let needs_roll = self.active.get(&frame.name).is_some_and(|a| {
                        a.frames > 1 && a.len + frame.update.len() as u64 + 88 > ROLL_AT
                    });
                    if needs_roll {
                        if let Some(buf) = buffers.remove(&frame.name) {
                            flush_buffer(&mut reqs, buf);
                        }
                        self.seal_active(&frame.name, &mut reqs, &mut sealed_now, &mut touched);
                    }
                    self.start_segment(&frame.name, &mut reqs, &mut touched);
                    if seen_touch.insert(frame.name.clone()) {
                        touched.push(frame.name.clone());
                    }
                    let Some(active) = self.active.get_mut(&frame.name) else {
                        continue;
                    };
                    let mut batch_id = [0u8; 16];
                    batch_id[..4].copy_from_slice(&self.open_id.to_le_bytes());
                    batch_id[8..16].copy_from_slice(&frame.ticket.0.to_le_bytes());
                    let payload =
                        batch_payload(batch_id, frame.origin, frame.wall_ms, &frame.update);
                    let bytes = encode_frame(FRAME_BATCH, active.frames, active.prev_crc, &payload);
                    active.sha.update(&bytes);
                    active.prev_crc = crc32c(&payload);
                    active.frames += 1;
                    let at = active.len;
                    active.len += bytes.len() as u64;
                    let path = active.path.clone();
                    buffers
                        .entry(frame.name.clone())
                        .or_insert_with(|| (path, at, Vec::new()))
                        .2
                        .extend_from_slice(&bytes);
                    tickets.push(frame.ticket);
                }
            }
        }
        for (_, buf) in std::mem::take(&mut buffers) {
            flush_buffer(&mut reqs, buf);
        }
        // After the frames above, so that a segment they started is sealed too.
        let seals: Vec<String> = if std::mem::take(&mut self.seal_every) {
            self.active.keys().cloned().collect()
        } else {
            Vec::new()
        };
        for name in seals {
            if self.active.get(&name).is_some_and(|a| a.frames > 1) {
                self.seal_active(&name, &mut reqs, &mut sealed_now, &mut touched);
            }
        }
        if reqs.is_empty() && tickets.is_empty() {
            return None;
        }
        let id = self.next_batch_id;
        self.next_batch_id += 1;
        self.in_flight = Some(InFlight {
            id,
            queued,
            touched,
            sealed: sealed_now,
            tickets,
        });
        Some(IoBatch { id, requests: reqs })
    }

    fn seal_active(
        &mut self,
        name: &str,
        reqs: &mut Vec<IoRequest>,
        sealed: &mut Vec<SealedSegment>,
        touched: &mut Vec<String>,
    ) {
        let Some(active) = self.active.remove(name) else {
            return;
        };
        let digest: [u8; 32] = active.sha.clone().finalize().into();
        let seal = seal_payload(active.frames, active.len, &digest);
        let frame = encode_frame(FRAME_SEAL, active.frames, active.prev_crc, &seal);
        reqs.extend([
            IoRequest::WriteAt {
                path: active.path.clone(),
                at: active.len,
                data: frame.clone(),
            },
            IoRequest::SyncFile {
                path: active.path.clone(),
            },
        ]);
        let mut h = active.sha;
        h.update(&frame);
        let whole: [u8; 32] = h.finalize().into();
        self.last_sha.insert(name.to_owned(), whole);
        self.known.insert(active.path.clone(), whole);
        sealed.push(SealedSegment {
            name: name.to_owned(),
            path: active.path,
            device: self.device,
            sequence: active.sequence,
        });
        touched.push(name.to_owned());
    }

    /// The host reports how a batch went. A failed flush is never retried on the same file: the
    /// operating system may have dropped the dirty pages, and a retry could falsely succeed.
    pub fn on_complete(&mut self, id: u64, result: Result<(), IoError>) -> BatchOutcome {
        let Some(flight) = self.in_flight.take() else {
            return BatchOutcome::Acknowledged(Vec::new());
        };
        debug_assert_eq!(flight.id, id);
        match result {
            Ok(()) => {
                self.failed = None;
                self.sealed.extend(flight.sealed);
                for item in &flight.queued {
                    if let Queued::Frame(f) = item {
                        self.last_acked_updates
                            .push((f.name.clone(), f.update.clone()));
                    }
                }
                self.acked.extend(flight.tickets.iter().copied());
                BatchOutcome::Acknowledged(flight.tickets)
            }
            Err(error) => {
                for name in &flight.touched {
                    // Left as it is. The next open finds its good end and seals it.
                    self.active.remove(name);
                    self.reason.insert(name.clone(), 5);
                    self.last_sha.remove(name);
                    self.ready.remove(name);
                }
                for item in flight.queued.into_iter().rev() {
                    self.queue.push_front(item);
                }
                self.failed = Some(error.clone());
                BatchOutcome::NotSaved(error)
            }
        }
    }

    /// Tickets that have become durable since the last call.
    pub fn poll_acks(&mut self) -> Vec<Ticket> {
        std::mem::take(&mut self.acked)
    }

    /// Updates that have become durable since the last call, for other tabs of this browser.
    pub fn take_acked_updates(&mut self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.last_acked_updates)
    }

    pub fn is_idle(&self) -> bool {
        self.in_flight.is_none() && self.queue.is_empty() && !self.seal_every
    }

    pub fn state(&self) -> SaveState {
        if self.failed.is_some() {
            SaveState::Failed
        } else if self.is_idle() {
            SaveState::Saved
        } else {
            SaveState::Saving
        }
    }

    pub fn last_error(&self) -> Option<&IoError> {
        self.failed.as_ref()
    }

    /// Whether a segment already passed a guarded import on an earlier open.
    pub fn is_verified(&self, path: &str) -> bool {
        self.verified.contains(path)
    }

    /// How many times a segment crashed the import.
    pub fn attempts(&self, path: &str) -> u32 {
        self.poison.get(path).copied().unwrap_or(0)
    }

    /// Own segments held back as poison, per document.
    pub fn blocked(&self) -> &BTreeMap<String, String> {
        &self.blocked
    }

    /// Records one more attempt on a segment, durably, before trying it.
    pub fn poison_begin(&mut self, path: &str) {
        *self.poison.entry(path.to_owned()).or_insert(0) += 1;
        let body = encode_table(&self.poison);
        self.put_local("poison", &body);
    }

    /// The attempt finished.
    pub fn poison_passed(&mut self, path: &str) {
        self.poison.remove(path);
        self.verified.insert(path.to_owned());
    }

    /// The segment was refused without a crash. It is treated as poison from the next open on.
    pub fn poison_reject(&mut self, path: &str) {
        self.poison.insert(path.to_owned(), POISON_LIMIT);
    }

    /// Writes what passed, after a whole pass. A crash before this only repeats harmless work.
    pub fn guard_finish(&mut self) {
        let keep: BTreeMap<String, u32> = self.verified.iter().map(|p| (p.clone(), 1)).collect();
        self.put_local("verified", &encode_table(&keep));
        let body = encode_table(&self.poison);
        self.put_local("poison", &body);
    }
}
