//! The log and the CRDT together: saving a workspace's changes, a guarded import on open, and
//! compaction, which replaces sealed segments with a snapshot that has been checked first.

use crate::format::{self, *};
use crate::host::{ImageFile, IoRequest};
use crate::store::{OpenRefused, Recovered, Store, StoreConfig, Ticket};
use casual_note_merge::{DocImport, Replica, SaveCursor, Workspace};
use std::collections::BTreeSet;

/// The result of trying one segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuardVerdict {
    Passed,
    /// Refused without a crash. An own segment stays, and its document is listed; another
    /// device's goes to quarantine.
    Rejected(String),
}

/// A log and what it has saved.
#[derive(Debug)]
pub struct Durable {
    pub store: Store,
    pub cursor: SaveCursor,
    guard: Vec<(String, usize)>,
    guard_replica: Option<(String, Replica)>,
    guard_seed: u64,
    guard_now: Option<(String, usize)>,
}

impl Durable {
    pub fn open(
        image: Vec<ImageFile>,
        cfg: &StoreConfig,
    ) -> Result<(Self, Recovered), OpenRefused> {
        let (store, rec) = Store::open(image, cfg)?;
        let mut guard = Vec::new();
        for (name, doc) in &rec.docs {
            for (i, seg) in doc.segments.iter().enumerate() {
                if !seg.excluded && !store.is_verified(&seg.path) {
                    guard.push((name.clone(), i));
                }
            }
        }
        Ok((
            Self {
                store,
                cursor: SaveCursor::default(),
                guard,
                guard_replica: None,
                guard_seed: cfg.peer ^ 0x5bd1_e995,
                guard_now: None,
            },
            rec,
        ))
    }

    /// Whatever changed since the last save goes to the log. Returns the tickets.
    pub fn save(
        &mut self,
        ws: &Workspace,
        wall_ms: i64,
    ) -> Result<Vec<Ticket>, casual_note_model::Error> {
        let updates = ws.changed_docs(&mut self.cursor)?;
        Ok(updates
            .into_iter()
            .map(|u| self.store.append(&u.name, u.update, 1, wall_ms))
            .collect())
    }

    /// The next segment to try, if any. Its attempt is queued: flush the store, then call
    /// `guard_try`. A crash in between counts, which is the point.
    pub fn guard_next(&mut self, rec: &Recovered) -> Option<String> {
        let (name, i) = self.guard.pop()?;
        let path = rec.docs.get(&name)?.segments.get(i)?.path.clone();
        self.store.poison_begin(&path);
        self.guard_now = Some((name, i));
        Some(path)
    }

    /// Tries the segment `guard_next` chose against everything before it in its document. Returns
    /// the verdict. A real trap here never returns, and the next open sees the count.
    pub fn guard_try(
        &mut self,
        rec: &mut Recovered,
        mut trial: impl FnMut(&str, &[u8]) -> Result<(), String>,
    ) -> Option<(String, GuardVerdict)> {
        let (name, i) = self.guard_now.take()?;
        if self.guard_replica.as_ref().is_none_or(|(n, _)| n != &name) {
            self.guard_seed = self.guard_seed.wrapping_add(1);
            let mut replica = Replica::new(self.guard_seed & 0x7FFF_FFFF_FFFF_FFFF).ok()?;
            let doc = rec.docs.get(&name)?;
            for s in &doc.snapshots {
                let _ = replica.import(s);
            }
            for seg in &doc.segments {
                if !seg.excluded && self.store.is_verified(&seg.path) {
                    for u in &seg.updates {
                        let _ = replica.import(u);
                    }
                }
            }
            self.guard_replica = Some((name.clone(), replica));
        }
        let doc = rec.docs.get_mut(&name)?;
        let seg = doc.segments.get_mut(i)?;
        let path = seg.path.clone();
        let mut verdict = GuardVerdict::Passed;
        for u in &seg.updates {
            let imported = trial(&path, u).and_then(|()| {
                let replica = &mut self.guard_replica.as_mut().ok_or("no replica")?.1;
                replica.import(u).map(|_| ()).map_err(|e| e.to_string())
            });
            if let Err(e) = imported {
                verdict = GuardVerdict::Rejected(e);
                break;
            }
        }
        match &verdict {
            GuardVerdict::Passed => self.store.poison_passed(&path),
            GuardVerdict::Rejected(e) => {
                seg.excluded = true;
                rec.issues.push(crate::store::Issue {
                    code: if seg.own {
                        "CN-STO-0021"
                    } else {
                        "CN-SYN-0009"
                    },
                    doc: Some(name.clone()),
                    path: path.clone(),
                    detail: e.clone(),
                });
                self.store.poison_reject(&path);
            }
        }
        Some((path, verdict))
    }

    /// All segments have been tried.
    pub fn guard_finish(&mut self) {
        self.guard_replica = None;
        self.store.guard_finish();
    }
}

/// The requests that make a document's state durable as a base snapshot, with its name. A valid new
/// file is the commit point. Used by migration, and by a new device starting from a checkpoint.
pub fn base_snapshot_requests(
    name: &str,
    payload: &[u8],
    device: &[u8; 16],
    seq: u64,
    wall_ms: i64,
) -> Result<Vec<IoRequest>, String> {
    let mut replica = Replica::new(1).map_err(|e| e.to_string())?;
    replica
        .import(payload)
        .map_err(|e| format!("CN-STO-0012: {e}"))?;
    let state_hash = replica.state_hash();
    let snapshot = Snapshot {
        doc_class: doc_class(name),
        kind: SNAPSHOT_BASE,
        doc_key: doc_key(name),
        created_ms: wall_ms,
        device: *device,
        seq,
        state_hash,
        covers: Vec::new(),
        version: replica.version(),
        payload: replica.snapshot().map_err(|e| e.to_string())?,
    };
    let dir = format::doc_dir(name);
    let snap_dir = format!("{dir}/snapshot");
    let mut requests = vec![IoRequest::CreateDir {
        path: snap_dir.clone(),
    }];
    if !name.is_empty() {
        let id = name.strip_prefix("d:").unwrap_or(name).as_bytes().to_vec();
        let path = format!("{dir}/doc.id");
        requests.extend([
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
    let path = format!("{snap_dir}/base-{}-{seq:016x}.snap", hex(device));
    let data = encode_snapshot(&snapshot);
    requests.extend([
        IoRequest::WriteAt {
            path: path.clone(),
            at: 0,
            data: data.clone(),
        },
        IoRequest::SetLen {
            path: path.clone(),
            len: data.len() as u64,
        },
        IoRequest::SyncFile { path },
        IoRequest::SyncDir { path: snap_dir },
        IoRequest::SyncDir { path: dir.clone() },
    ]);
    let mut up = dir.as_str();
    while let Some((parent, _)) = up.rsplit_once('/') {
        requests.push(IoRequest::SyncDir {
            path: parent.to_owned(),
        });
        up = parent;
    }
    requests.push(IoRequest::SyncDir {
        path: String::new(),
    });
    Ok(requests)
}

/// How to compact one document.
#[derive(Clone, Debug)]
pub struct CompactOptions {
    /// Do nothing below this many sealed segments.
    pub min_segments: usize,
    /// Segments that must stay: not yet uploaded, or waiting on something.
    pub keep: BTreeSet<String>,
    pub device: [u8; 16],
    pub wall_ms: i64,
    pub peer: u64,
}

/// The requests that replace covered segments with a checked base snapshot, in crash-safe order.
#[derive(Clone, Debug)]
pub struct CompactionPlan {
    pub requests: Vec<IoRequest>,
    pub removed: Vec<String>,
    pub snapshot: String,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// Plans compaction of one document, from its files alone and never from the live document, which
/// may have moved. `Ok(None)` means nothing should be done yet. An `Err` means a check failed and
/// no segment may be deleted.
pub fn plan_compaction(
    name: &str,
    files: &[ImageFile],
    o: &CompactOptions,
) -> Result<Option<CompactionPlan>, String> {
    let dir = format::doc_dir(name);
    let log_dir = format!("{dir}/log");
    let snap_dir = format!("{dir}/snapshot");
    let mut parts: Vec<Vec<u8>> = Vec::new();
    let mut old_snapshots = Vec::new();
    let mut seq = 0u64;
    let mut bytes_before = 0u64;
    for f in files
        .iter()
        .filter(|f| f.path.starts_with(&format!("{snap_dir}/")))
    {
        old_snapshots.push(f.path.clone());
        if let Some(s) = parse_snapshot(&f.bytes) {
            seq = seq.max(s.seq);
            parts.push(s.payload);
            bytes_before += f.bytes.len() as u64;
        }
    }
    let mut covers = Vec::new();
    let mut removed = Vec::new();
    for f in files
        .iter()
        .filter(|f| f.path.starts_with(&format!("{log_dir}/")))
    {
        if o.keep.contains(&f.path) {
            continue;
        }
        let Some((device, sequence)) = f.path.rsplit('/').next().and_then(parse_segment_name)
        else {
            continue;
        };
        let scan = scan_segment(&f.bytes);
        if !scan.sealed || scan.damage {
            continue;
        }
        for fr in &scan.frames {
            if fr.frame_type == FRAME_BATCH {
                let Some(u) = batch_update(&f.bytes[fr.payload.0..fr.payload.1]) else {
                    return Err("CN-STO-0005: a frame's contents are malformed".into());
                };
                parts.push(u.to_vec());
            }
        }
        covers.push((device, sequence, scan.frames.len() as u64));
        removed.push(f.path.clone());
        bytes_before += f.bytes.len() as u64;
    }
    if removed.len() < o.min_segments || removed.is_empty() {
        return Ok(None);
    }
    let mut built = Replica::new(o.peer).map_err(|e| e.to_string())?;
    for pass in 0..2 {
        for p in &parts {
            let status = built.import(p).map_err(|e| format!("CN-STO-0012: {e}"))?;
            // The second pass shows what is still missing: such a part is not in the state, so
            // deleting its segment would lose it.
            if pass == 1 && status == DocImport::Pending {
                return Ok(None);
            }
        }
    }
    let payload = built.snapshot().map_err(|e| e.to_string())?;
    let version = built.version();
    let state_hash = built.state_hash();
    let snap = Snapshot {
        doc_class: doc_class(name),
        kind: SNAPSHOT_BASE,
        doc_key: doc_key(name),
        created_ms: o.wall_ms,
        device: o.device,
        seq: seq + 1,
        state_hash,
        covers,
        version: version.clone(),
        payload,
    };
    let bytes = encode_snapshot(&snap);
    // Checked from what will actually be on disk, against a separate rebuild.
    let parsed = parse_snapshot(&bytes).ok_or("CN-STO-0012: the snapshot does not read back")?;
    let mut check = Replica::new(o.peer ^ 1).map_err(|e| e.to_string())?;
    check
        .import(&parsed.payload)
        .map_err(|e| format!("CN-STO-0012: {e}"))?;
    if check.state_hash() != state_hash || check.version() != version {
        return Err("CN-STO-0012: the snapshot does not match its segments".into());
    }
    let path = format!("{snap_dir}/base-{}-{:016x}.snap", hex(&o.device), seq + 1);
    let bytes_after = bytes.len() as u64;
    let mut requests = vec![
        IoRequest::CreateDir {
            path: snap_dir.clone(),
        },
        IoRequest::WriteAt {
            path: path.clone(),
            at: 0,
            data: bytes,
        },
        IoRequest::SyncFile { path: path.clone() },
        IoRequest::SyncDir {
            path: snap_dir.clone(),
        },
        IoRequest::SyncDir { path: dir.clone() },
    ];
    for r in &removed {
        requests.push(IoRequest::Remove { path: r.clone() });
    }
    requests.push(IoRequest::SyncDir { path: log_dir });
    for s in &old_snapshots {
        if s != &path {
            requests.push(IoRequest::Remove { path: s.clone() });
        }
    }
    requests.push(IoRequest::SyncDir { path: snap_dir });
    Ok(Some(CompactionPlan {
        requests,
        removed,
        snapshot: path,
        bytes_before,
        bytes_after,
    }))
}
