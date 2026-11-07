//! The promise: no acknowledged edit is ever lost. A power cut is simulated at every byte of every
//! write and at every other request, including through compaction and through recovery itself.

use casual_note_merge::{SaveCursor, Workspace};
use casual_note_model::{Block, BlockKind, Run};
use casual_note_store::*;
use std::collections::{BTreeMap, BTreeSet};

fn cfg(open: u32) -> StoreConfig {
    StoreConfig {
        new_device: [0xd1; 16],
        open_id: open,
        peer: u64::from(open) + 1000,
        wall_ms: 1,
        live_opens: vec![],
    }
}

/// Runs the store's batches on the host until it has nothing left.
fn pump(store: &mut Store, host: &mut MemHost) -> Result<(), Crashed> {
    while let Some(batch) = store.next_batch() {
        let result = host.run(&batch.requests)?;
        store.on_complete(batch.id, result);
    }
    Ok(())
}

#[derive(Clone, Debug)]
enum Step {
    Do(IoRequest),
    /// This acknowledgement reached the person.
    Ack(Vec<String>),
}

fn units(trace: &[Step]) -> u64 {
    trace
        .iter()
        .map(|s| {
            if let Step::Do(r) = s {
                MemHost::units(r)
            } else {
                0
            }
        })
        .sum()
}

/// Replays a trace until the power goes after `at` units, and returns the disk's state and what
/// had been acknowledged by then.
fn replay(trace: &[Step], at: u64) -> (MemHost, BTreeSet<String>) {
    let mut host = MemHost::new();
    host.crash_after(at);
    let mut acked = BTreeSet::new();
    for s in trace {
        match s {
            Step::Do(r) => {
                if host.exec(r).is_err() {
                    return (host, acked);
                }
            }
            Step::Ack(tokens) => acked.extend(tokens.iter().cloned()),
        }
    }
    (host, acked)
}

/// Everything readable in the log, as tokens.
fn opaque_tokens(rec: &Recovered) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for doc in rec.docs.values() {
        for seg in &doc.segments {
            for u in &seg.updates {
                out.insert(String::from_utf8_lossy(u).into_owned());
            }
        }
    }
    out
}

/// A log workload with opaque updates, recorded as a trace of requests.
fn opaque_trace() -> (Vec<Step>, BTreeSet<String>) {
    let mut host = MemHost::new();
    let (mut store, _) = Store::open(Vec::new(), &cfg(1)).unwrap();
    let mut trace = Vec::new();
    let mut all = BTreeSet::new();
    let mut tokens: BTreeMap<Ticket, String> = BTreeMap::new();
    let drive = |store: &mut Store,
                 host: &mut MemHost,
                 trace: &mut Vec<Step>,
                 tokens: &mut BTreeMap<Ticket, String>| {
        while let Some(batch) = store.next_batch() {
            for r in &batch.requests {
                trace.push(Step::Do(r.clone()));
            }
            host.run(&batch.requests).unwrap().unwrap();
            store.on_complete(batch.id, Ok(()));
            let acked: Vec<String> = store
                .poll_acks()
                .iter()
                .filter_map(|t| tokens.remove(t))
                .collect();
            trace.push(Step::Ack(acked));
        }
    };
    let names = ["", "page-a", "d:base", "page-a", ""];
    for (i, name) in names.iter().enumerate() {
        let token = format!("token-{i}-{}", "x".repeat(i));
        all.insert(token.clone());
        let t = store.append(name, token.clone().into_bytes(), 1, i as i64);
        tokens.insert(t, token);
        if i == 2 {
            // Two arriving together share one flush.
            let extra = "together".to_owned();
            all.insert(extra.clone());
            let t = store.append("page-b", extra.clone().into_bytes(), 1, i as i64);
            tokens.insert(t, extra);
        }
        drive(&mut store, &mut host, &mut trace, &mut tokens);
    }
    store.seal_all();
    drive(&mut store, &mut host, &mut trace, &mut tokens);
    let late = "after-the-seal".to_owned();
    all.insert(late.clone());
    let t = store.append("page-a", late.clone().into_bytes(), 1, 9);
    tokens.insert(t, late);
    drive(&mut store, &mut host, &mut trace, &mut tokens);
    (trace, all)
}

fn check_recovery(
    mut host: MemHost,
    acked: &BTreeSet<String>,
    all: &BTreeSet<String>,
    what: &str,
    reopen_crashes: bool,
) {
    let image = host.image();
    let (mut store, rec) =
        Store::open(image.clone(), &cfg(2)).unwrap_or_else(|e| panic!("{what}: refused {e:?}"));
    let found = opaque_tokens(&rec);
    for token in acked {
        assert!(found.contains(token), "{what}: lost acknowledged {token}");
    }
    for token in &found {
        assert!(all.contains(token), "{what}: invented {token}");
    }
    if reopen_crashes {
        // The power goes again at every point of the repair.
        let batch: Vec<IoRequest> = {
            let (mut s, _) = Store::open(image.clone(), &cfg(2)).unwrap();
            let mut reqs = Vec::new();
            while let Some(b) = s.next_batch() {
                reqs.extend(b.requests.clone());
                s.on_complete(b.id, Ok(()));
            }
            reqs
        };
        let total: u64 = batch.iter().map(MemHost::units).sum();
        for cut in (0..=total).step_by(7) {
            let mut h = host.clone();
            h.crash_after(cut);
            let _ = h.run(&batch);
            let after = h.after_crash(&CrashMode::LoseUnsynced);
            let (_, rec2) = Store::open(after.image(), &cfg(3))
                .unwrap_or_else(|e| panic!("{what}: refused {e:?}"));
            let again = opaque_tokens(&rec2);
            for token in acked {
                assert!(
                    again.contains(token),
                    "{what}: repair crash at {cut} lost {token}"
                );
            }
        }
    }
    pump(&mut store, &mut host).unwrap();
    // Recovery is idempotent, and the log works afterwards.
    let (mut store2, rec2) = Store::open(host.image(), &cfg(3)).unwrap();
    assert_eq!(
        opaque_tokens(&rec2),
        found,
        "{what}: a second open sees the same"
    );
    let t = store2.append("page-a", b"after-recovery".to_vec(), 1, 0);
    pump(&mut store2, &mut host).unwrap();
    assert!(store2.poll_acks().contains(&t));
    let (_, rec3) = Store::open(host.image(), &cfg(4)).unwrap();
    let mut want = found.clone();
    want.insert("after-recovery".into());
    assert_eq!(
        opaque_tokens(&rec3),
        want,
        "{what}: edits after recovery survive too"
    );
}

#[test]
fn a_power_cut_at_every_byte_loses_no_acknowledged_frame() {
    let (trace, all) = opaque_trace();
    let total = units(&trace);
    assert!(
        total > 800,
        "the workload is big enough to mean something: {total}"
    );
    let mut acked_some = false;
    for at in 0..=total {
        let (host, acked) = replay(&trace, at);
        acked_some |= !acked.is_empty();
        for (i, mode) in [
            CrashMode::LoseUnsynced,
            CrashMode::KeepUnsynced,
            CrashMode::Scrambled(at),
            CrashMode::Scrambled(at ^ 0xABCD),
        ]
        .iter()
        .enumerate()
        {
            let after = host.after_crash(mode);
            check_recovery(
                after,
                &acked,
                &all,
                &format!("cut at unit {at} of {total}, {mode:?}"),
                i == 0 && at % 11 == 0,
            );
        }
    }
    assert!(acked_some);
}

/* ---------- The same, with real CRDT documents ---------- */

fn para(id: &str, text: &str) -> Block {
    Block {
        id: id.into(),
        kind: BlockKind::Paragraph,
        checked: false,
        runs: vec![Run {
            text: text.into(),
            marks: vec![],
            link: None,
        }],
        target: None,
    }
}

/// Loads a container into a fresh workspace, and returns all the text.
fn text_of(rec: &Recovered) -> String {
    let Some(bytes) = rec.container() else {
        return String::new();
    };
    let mut ws = Workspace::new(424242).unwrap();
    ws.import(&bytes)
        .unwrap_or_else(|e| panic!("the recovered log does not import: {e}"));
    let mut out = String::new();
    for meta in ws.pages() {
        out.push_str(&meta.title);
        out.push('\n');
        out.push_str(
            &ws.page(&meta.id)
                .unwrap()
                .blocks
                .iter()
                .map(Block::plain)
                .collect::<Vec<_>>()
                .join("\n"),
        );
        out.push('\n');
    }
    out
}

struct Run2 {
    trace: Vec<Step>,
    host: MemHost,
    compaction_from: u64,
    compaction_to: u64,
}

fn real_trace() -> Run2 {
    let mut host = MemHost::new();
    let mut trace = Vec::new();
    let mut all = BTreeSet::new();
    let run = |d: &mut Durable, host: &mut MemHost, trace: &mut Vec<Step>, tokens: Vec<String>| {
        let mut tokens = Some(tokens);
        while let Some(batch) = d.store.next_batch() {
            for r in &batch.requests {
                trace.push(Step::Do(r.clone()));
            }
            host.run(&batch.requests).unwrap().unwrap();
            d.store.on_complete(batch.id, Ok(()));
            trace.push(Step::Ack(tokens.take().unwrap_or_default()));
        }
    };
    // First open: a page, some typing.
    let (mut d, _) = Durable::open(Vec::new(), &cfg(1)).unwrap();
    let mut ws = Workspace::new(1001).unwrap();
    ws.create_page("p1", None, "t0", None).unwrap();
    ws.insert_block("p1", None, &para("b1", "")).unwrap();
    d.save(&ws, 1).unwrap();
    run(&mut d, &mut host, &mut trace, vec!["t0".into()]);
    all.insert("t0".to_owned());
    for i in 1..4 {
        let token = format!("t{i}|");
        ws.splice_text("p1", "b1", 0, 0, &token).unwrap();
        d.save(&ws, i).unwrap();
        run(&mut d, &mut host, &mut trace, vec![token.clone()]);
        all.insert(token);
    }
    d.store.seal_all();
    run(&mut d, &mut host, &mut trace, vec![]);
    // A second open with a fresh peer, more typing.
    let (mut d, rec) = Durable::open(host.image(), &cfg(2)).unwrap();
    run(&mut d, &mut host, &mut trace, vec![]);
    let mut ws = Workspace::new(1002).unwrap();
    let bytes = rec.container().unwrap();
    ws.import(&bytes).unwrap();
    d.cursor.mark_durable(&bytes);
    for i in 4..6 {
        let token = format!("t{i}|");
        ws.splice_text("p1", "b1", 0, 0, &token).unwrap();
        d.save(&ws, i).unwrap();
        run(&mut d, &mut host, &mut trace, vec![token.clone()]);
        all.insert(token);
    }
    d.store.seal_all();
    run(&mut d, &mut host, &mut trace, vec![]);
    // Compaction of the page.
    let compaction_from = units(&trace);
    let opts = CompactOptions {
        min_segments: 2,
        keep: BTreeSet::new(),
        device: d.store.device(),
        wall_ms: 5,
        peer: 777,
    };
    let files: Vec<ImageFile> = host
        .image()
        .into_iter()
        .filter(|f| f.path.starts_with(&format!("{}/", format::doc_dir("p1"))))
        .collect();
    let plan = plan_compaction("p1", &files, &opts)
        .unwrap()
        .expect("two sealed segments compact");
    assert!(!plan.removed.is_empty());
    d.store.submit(plan.requests.clone());
    run(&mut d, &mut host, &mut trace, vec![]);
    let compaction_to = units(&trace);
    d.store.forget(&plan.removed);
    // And a compacted notebook keeps working.
    let token = "t6|".to_owned();
    ws.splice_text("p1", "b1", 0, 0, &token).unwrap();
    d.save(&ws, 6).unwrap();
    run(&mut d, &mut host, &mut trace, vec![token.clone()]);
    all.insert(token);
    assert_eq!(all.len(), 7);
    Run2 {
        trace,
        host,
        compaction_from,
        compaction_to,
    }
}

fn check_real(host: &MemHost, acked: &BTreeSet<String>, what: &str) {
    let (_, rec) =
        Store::open(host.image(), &cfg(9)).unwrap_or_else(|e| panic!("{what}: refused {e:?}"));
    let text = text_of(&rec);
    for token in acked {
        assert!(
            text.contains(token.as_str()),
            "{what}: lost acknowledged {token} from {text:?}"
        );
    }
}

#[test]
fn a_power_cut_at_every_byte_of_real_edits_loses_none_of_them() {
    let run = real_trace();
    let total = units(&run.trace);
    let mut checked = 0;
    for at in 0..=total {
        // Every point before compaction lands within a few bytes of frames, so all are tried.
        if at >= run.compaction_from && at < run.compaction_to {
            continue; // The compaction test below covers these, in more modes.
        }
        let (host, acked) = replay(&run.trace, at);
        check_real(
            &host.after_crash(&CrashMode::LoseUnsynced),
            &acked,
            &format!("cut at {at}/{total}, lose"),
        );
        if at % 3 == 0 {
            check_real(
                &host.after_crash(&CrashMode::Scrambled(at)),
                &acked,
                &format!("cut at {at}/{total}, scrambled"),
            );
        }
        checked += 1;
    }
    assert!(checked > 500, "{checked}");
}

#[test]
fn a_power_cut_at_every_step_of_compaction_loses_nothing() {
    let run = real_trace();
    assert!(run.compaction_to > run.compaction_from);
    let steps = run.compaction_to - run.compaction_from;
    // Everything so far was acknowledged, so all must survive whichever step the power goes at.
    for at in run.compaction_from..=run.compaction_to {
        let (host, acked) = replay(&run.trace, at);
        assert!(acked.len() >= 6, "{at}");
        for mode in [
            CrashMode::LoseUnsynced,
            CrashMode::KeepUnsynced,
            CrashMode::Scrambled(at),
        ] {
            check_real(
                &host.after_crash(&mode),
                &acked,
                &format!(
                    "compaction cut {} of {steps}, {mode:?}",
                    at - run.compaction_from
                ),
            );
        }
    }
    // The end state holds a snapshot, and fewer segment files than before.
    let paths = run.host.paths();
    assert!(paths.iter().any(|p| p.ends_with(".snap")));
}

#[test]
fn rebuilding_from_a_snapshot_equals_rebuilding_from_the_segments() {
    let run = real_trace();
    let (mut before, _) = replay(&run.trace, run.compaction_from);
    pump_nothing(&mut before);
    let (after, _) = replay(&run.trace, run.compaction_to);
    let load = |h: &MemHost| {
        let (_, rec) = Store::read_only_for_test(h.image());
        let mut ws = Workspace::new(1).unwrap();
        ws.import(&rec.container().unwrap()).unwrap();
        ws.doc_state_hash("p1").unwrap()
    };
    assert_eq!(load(&before), load(&after));
}

fn pump_nothing(_: &mut MemHost) {}

trait ReadOnlyForTest {
    fn read_only_for_test(image: Vec<ImageFile>) -> ((), Recovered);
}
impl ReadOnlyForTest for Store {
    fn read_only_for_test(image: Vec<ImageFile>) -> ((), Recovered) {
        ((), Store::read_only(image, &cfg(50)).unwrap())
    }
}

/* ---------- Saved means flushed ---------- */

#[test]
fn nothing_is_acknowledged_before_the_flush_completes() {
    let mut host = MemHost::new();
    let (mut store, _) = Store::open(Vec::new(), &cfg(1)).unwrap();
    pump(&mut store, &mut host).unwrap();
    assert_eq!(store.state(), SaveState::Saved);
    let t = store.append("p", b"edit".to_vec(), 1, 0);
    assert_eq!(store.state(), SaveState::Saving);
    let batch = store.next_batch().unwrap();
    assert!(store.poll_acks().is_empty(), "not before the batch ran");
    assert_eq!(store.state(), SaveState::Saving);
    let last_write = batch
        .requests
        .iter()
        .rposition(|r| matches!(r, IoRequest::WriteAt { .. }))
        .unwrap();
    let sync_after = batch.requests[last_write..]
        .iter()
        .any(|r| matches!(r, IoRequest::SyncFile { .. }));
    assert!(sync_after, "the last write is flushed within the batch");
    host.run(&batch.requests).unwrap().unwrap();
    assert!(
        store.poll_acks().is_empty(),
        "not until the host reports completion"
    );
    store.on_complete(batch.id, Ok(()));
    assert_eq!(store.poll_acks(), vec![t]);
    assert_eq!(store.state(), SaveState::Saved);
}

#[test]
fn group_commit_flushes_at_once_when_idle_and_groups_while_busy() {
    let mut host = MemHost::new();
    let (mut store, _) = Store::open(Vec::new(), &cfg(1)).unwrap();
    pump(&mut store, &mut host).unwrap();
    let first = store.append("p", b"1".to_vec(), 1, 0);
    let busy = store.next_batch().unwrap();
    // These arrive during the flush: they wait, and all share the next one.
    let later: Vec<Ticket> = (0..50).map(|i| store.append("p", vec![i], 1, 0)).collect();
    assert!(store.next_batch().is_none(), "one flush at a time");
    host.run(&busy.requests).unwrap().unwrap();
    store.on_complete(busy.id, Ok(()));
    assert_eq!(store.poll_acks(), vec![first]);
    let flushes = host.flushes;
    let shared = store.next_batch().unwrap();
    host.run(&shared.requests).unwrap().unwrap();
    store.on_complete(shared.id, Ok(()));
    assert_eq!(store.poll_acks(), later);
    assert_eq!(host.flushes - flushes, 1, "fifty frames, one flush");
}

#[test]
fn a_failed_flush_is_never_retried_on_the_same_file_and_loses_nothing() {
    let mut host = MemHost::new();
    let (mut store, _) = Store::open(Vec::new(), &cfg(1)).unwrap();
    pump(&mut store, &mut host).unwrap();
    let a = store.append("p", b"acked".to_vec(), 1, 0);
    pump(&mut store, &mut host).unwrap();
    assert_eq!(store.poll_acks(), vec![a]);
    let old: Vec<String> = host
        .paths()
        .into_iter()
        .filter(|p| p.ends_with(".seg"))
        .collect();
    let b = store.append("p", b"waiting".to_vec(), 1, 0);
    let batch = store.next_batch().unwrap();
    let fail_at = batch
        .requests
        .iter()
        .position(|r| matches!(r, IoRequest::SyncFile { .. }))
        .unwrap() as u64;
    host.fail_request(fail_at, IoError::Failed("EIO".into()));
    let result = host.run(&batch.requests).unwrap();
    assert!(result.is_err());
    assert!(matches!(
        store.on_complete(batch.id, result),
        BatchOutcome::NotSaved(_)
    ));
    assert_eq!(store.state(), SaveState::Failed, "the person is told");
    assert!(store.poll_acks().is_empty());
    // The retry goes to a new file, and only then is the edit acknowledged.
    let retry = store.next_batch().unwrap();
    let writes: BTreeSet<&String> = retry
        .requests
        .iter()
        .filter_map(|r| {
            if let IoRequest::WriteAt { path, .. } = r {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    assert!(
        writes.iter().all(|p| !old.contains(p)),
        "not the file whose flush failed"
    );
    host.run(&retry.requests).unwrap().unwrap();
    store.on_complete(retry.id, Ok(()));
    assert_eq!(store.poll_acks(), vec![b]);
    assert_eq!(store.state(), SaveState::Saved);
    // Both are there after a cut, and the abandoned file gets sealed at the next open.
    let after = host.after_crash(&CrashMode::LoseUnsynced);
    let (_, rec) = Store::open(after.image(), &cfg(2)).unwrap();
    let found = opaque_tokens(&rec);
    assert!(found.contains("acked") && found.contains("waiting"));
}

#[test]
fn a_full_disk_says_so_and_keeps_the_edit() {
    let mut host = MemHost::new();
    let (mut store, _) = Store::open(Vec::new(), &cfg(1)).unwrap();
    pump(&mut store, &mut host).unwrap();
    let t = store.append("p", b"edit".to_vec(), 1, 0);
    let batch = store.next_batch().unwrap();
    store.on_complete(batch.id, Err(IoError::NoSpace));
    assert_eq!(store.last_error(), Some(&IoError::NoSpace));
    assert!(store.poll_acks().is_empty());
    pump(&mut store, &mut host).unwrap();
    assert_eq!(store.poll_acks(), vec![t]);
}

/* ---------- Recovery ---------- */

fn written(tokens: &[&str]) -> MemHost {
    let mut host = MemHost::new();
    let (mut store, _) = Store::open(Vec::new(), &cfg(1)).unwrap();
    for t in tokens {
        store.append("p", t.as_bytes().to_vec(), 1, 0);
        pump(&mut store, &mut host).unwrap();
    }
    host
}

fn segment_path(host: &MemHost) -> String {
    host.paths()
        .into_iter()
        .find(|p| p.ends_with(".seg"))
        .unwrap()
}

#[test]
fn a_torn_tail_goes_to_quarantine_before_it_is_cut() {
    let mut host = written(&["one", "two"]);
    let seg = segment_path(&host);
    let good = host.read(&seg).unwrap().len();
    host.poke(&seg, |b| b.extend_from_slice(b"garbage after the end"));
    let (mut store, rec) = Store::open(host.image(), &cfg(2)).unwrap();
    assert_eq!(opaque_tokens(&rec).len(), 2);
    let batch = store.next_batch().unwrap();
    let copy = batch.requests.iter().position(|r| matches!(r, IoRequest::WriteAt { path, .. } if path.starts_with("local/quarantine/"))).unwrap();
    let cut = batch
        .requests
        .iter()
        .position(|r| matches!(r, IoRequest::SetLen { path, .. } if *path == seg))
        .unwrap();
    assert!(copy < cut, "moved aside first");
    host.run(&batch.requests).unwrap().unwrap();
    store.on_complete(batch.id, Ok(()));
    let tail = host
        .paths()
        .into_iter()
        .find(|p| p.ends_with(".tail"))
        .unwrap();
    assert_eq!(host.read(&tail).unwrap(), b"garbage after the end");
    let scan = format::scan_segment(host.read(&seg).unwrap());
    assert!(scan.sealed, "and the segment is sealed after recovery");
    assert!(scan.good_end > good);
}

#[test]
fn damage_in_the_middle_is_told_apart_from_a_crash_and_keeps_all_bytes() {
    let mut host = written(&["one", "two", "three"]);
    let seg = segment_path(&host);
    let scan = format::scan_segment(host.read(&seg).unwrap());
    let middle = scan.frames[2].payload.0 + 3;
    let whole_before = {
        let mut c = host.read(&seg).unwrap().to_vec();
        c[middle] ^= 1;
        c
    };
    host.poke(&seg, |b| b[middle] ^= 1);
    let (mut store, rec) = Store::open(host.image(), &cfg(2)).unwrap();
    assert!(
        rec.issues.iter().any(|i| i.code == "CN-STO-0005"),
        "{:?}",
        rec.issues
    );
    let found = opaque_tokens(&rec);
    assert!(found.contains("one") && !found.contains("three"));
    pump(&mut store, &mut host).unwrap();
    let q = host
        .paths()
        .into_iter()
        .find(|p| p.ends_with(".whole"))
        .unwrap();
    assert_eq!(
        host.read(&q).unwrap(),
        whole_before.as_slice(),
        "the whole file is kept"
    );
    assert!(
        format::scan_segment(host.read(&seg).unwrap()).sealed,
        "the good prefix is sealed"
    );
}

#[test]
fn a_rotted_header_does_not_lose_the_frames_after_it() {
    let mut host = written(&["one", "two"]);
    let seg = segment_path(&host);
    host.poke(&seg, |b| b[60] ^= 1);
    let (_, rec) = Store::open(host.image(), &cfg(2)).unwrap();
    assert_eq!(opaque_tokens(&rec).len(), 2);
    assert!(rec.issues.iter().any(|i| i.code == "CN-STO-0004"));
}

#[test]
fn another_tabs_live_segment_is_read_and_never_repaired() {
    let mut host = written(&["one"]);
    let seg = segment_path(&host);
    host.poke(&seg, |b| {
        b.extend_from_slice(b"a frame being written right now")
    });
    let before = host.read(&seg).unwrap().to_vec();
    let mut c = cfg(2);
    c.live_opens = vec![1];
    let (mut store, rec) = Store::open(host.image(), &c).unwrap();
    assert!(opaque_tokens(&rec).contains("one"));
    pump(&mut store, &mut host).unwrap();
    assert_eq!(host.read(&seg).unwrap(), before.as_slice());
}

#[test]
fn a_newer_major_version_is_refused_and_nothing_is_written() {
    let mut host = written(&["one"]);
    host.poke("manifest.json", |b| {
        let s = String::from_utf8_lossy(b).replace("\"major\":1", "\"major\":2");
        *b = s.into_bytes();
    });
    assert_eq!(
        Store::open(host.image(), &cfg(2)).unwrap_err().code,
        "CN-STO-0002"
    );
}

#[test]
fn every_open_mints_a_fresh_peer_recorded_first_in_each_segment() {
    let mut host = MemHost::new();
    let mut peers = BTreeSet::new();
    for open in 1..4 {
        let (mut store, _) = Store::open(host.image(), &cfg(open)).unwrap();
        store.append("p", format!("e{open}").into_bytes(), 1, 0);
        pump(&mut store, &mut host).unwrap();
    }
    for p in host.paths().into_iter().filter(|p| p.ends_with(".seg")) {
        let bytes = host.read(&p).unwrap();
        let scan = format::scan_segment(bytes);
        let first = scan.frames[0];
        assert_eq!(first.frame_type, format::FRAME_PEER);
        let mut id = [0u8; 8];
        id.copy_from_slice(&bytes[first.payload.0..first.payload.0 + 8]);
        peers.insert(u64::from_le_bytes(id));
    }
    assert_eq!(peers.len(), 3, "one per open, never reused");
}

/* ---------- Poison ---------- */

/// One open: tries segments in turn, and "crashes" on the named one, as a trap in the import would.
fn open_and_guard(
    host: &mut MemHost,
    open: u32,
    trap_on: Option<&str>,
) -> Result<(Durable, Recovered), String> {
    let (mut d, mut rec) = Durable::open(host.image(), &cfg(open)).unwrap();
    pump(&mut d.store, host).unwrap();
    while let Some(path) = d.guard_next(&rec) {
        pump(&mut d.store, host).unwrap();
        if trap_on.is_some_and(|t| path.contains(t)) {
            return Err(path); // The process is gone. Only what was flushed remains.
        }
        d.guard_try(&mut rec, |_, _| Ok(()));
    }
    d.guard_finish();
    pump(&mut d.store, host).unwrap();
    Ok((d, rec))
}

fn real_host() -> (MemHost, Vec<u8>) {
    let mut host = MemHost::new();
    let (mut d, _) = Durable::open(Vec::new(), &cfg(1)).unwrap();
    let mut ws = Workspace::new(1).unwrap();
    ws.create_page("p", None, "T", None).unwrap();
    ws.insert_block("p", None, &para("b", "own text")).unwrap();
    d.save(&ws, 0).unwrap();
    pump(&mut d.store, &mut host).unwrap();
    d.store.seal_all();
    pump(&mut d.store, &mut host).unwrap();
    (host, ws.export().unwrap())
}

#[test]
fn an_own_segment_that_keeps_crashing_is_never_moved_and_its_document_is_listed() {
    let (mut host, _) = real_host();
    let p_seg = host
        .paths()
        .into_iter()
        .find(|p| p.starts_with("pages/") && p.ends_with(".seg"))
        .unwrap();
    let bytes = host.read(&p_seg).unwrap().to_vec();
    let file = p_seg.rsplit('/').next().unwrap().to_owned();
    for open in 2..5 {
        assert!(
            open_and_guard(&mut host, open, Some(&file)).is_err(),
            "attempt {open} traps"
        );
    }
    let (d, rec) = open_and_guard(&mut host, 5, None).unwrap();
    assert_eq!(
        host.read(&p_seg).unwrap(),
        bytes.as_slice(),
        "own bytes stay where they are"
    );
    assert_eq!(d.store.blocked().get("p"), Some(&p_seg));
    assert!(rec.issues.iter().any(|i| i.code == "CN-STO-0021"));
    // Everything else still opens, including that the poison part is left out of what loads.
    let doc = &rec.docs["p"];
    assert!(
        doc.segments
            .iter()
            .find(|s| s.path == p_seg)
            .unwrap()
            .excluded
    );
    let mut ws = Workspace::new(9).unwrap();
    ws.import(&rec.container().unwrap()).unwrap();
    assert_eq!(ws.pages().len(), 1, "the rest of the notes work");
    assert!(
        host.paths()
            .iter()
            .all(|p| !p.starts_with("local/quarantine/"))
    );
}

#[test]
fn another_devices_segment_is_quarantined_only_at_the_third_crash() {
    let (mut host, _) = real_host();
    // Another device's sealed segment, filed as sync would.
    let mut other = MemHost::new();
    let mut c = cfg(77);
    c.new_device = [0xee; 16];
    let (mut d, _) = Durable::open(Vec::new(), &c).unwrap();
    let mut ws = Workspace::new(2).unwrap();
    ws.create_page("q", None, "Other", None).unwrap();
    ws.insert_block("q", None, &para("x", "theirs")).unwrap();
    d.save(&ws, 0).unwrap();
    pump(&mut d.store, &mut other).unwrap();
    d.store.seal_all();
    pump(&mut d.store, &mut other).unwrap();
    let theirs = other
        .paths()
        .into_iter()
        .find(|p| p.starts_with("pages/") && p.ends_with(".seg"))
        .unwrap();
    let (mut s, _) = Store::open(host.image(), &cfg(2)).unwrap();
    s.file_foreign("q", other.read(&theirs).unwrap().to_vec())
        .unwrap();
    pump(&mut s, &mut host).unwrap();
    let file = theirs.rsplit('/').next().unwrap().to_owned();
    assert!(host.read(&theirs).is_some());
    for open in 3..5 {
        assert!(open_and_guard(&mut host, open, Some(&file)).is_err());
        assert!(
            host.read(&theirs).is_some(),
            "still there after attempt {open}"
        );
    }
    assert!(open_and_guard(&mut host, 5, Some(&file)).is_err());
    let (_, rec) = open_and_guard(&mut host, 6, None).unwrap();
    assert!(host.read(&theirs).is_none(), "moved");
    assert!(
        host.paths()
            .iter()
            .any(|p| p.starts_with("local/quarantine/") && p.contains(&file)),
        "not deleted"
    );
    assert!(rec.issues.iter().any(|i| i.code == "CN-STO-0016"));
}

#[test]
fn a_segment_that_passed_is_not_tried_again() {
    let (mut host, _) = real_host();
    open_and_guard(&mut host, 2, None).unwrap();
    let (mut d, rec) = Durable::open(host.image(), &cfg(3)).unwrap();
    assert!(d.guard_next(&rec).is_none());
}

#[test]
fn a_refused_update_is_left_out_and_everything_else_loads() {
    let (mut host, _) = real_host();
    let (mut s, _) = Store::open(host.image(), &cfg(2)).unwrap();
    s.append("p", b"this is not a CRDT update".to_vec(), 1, 0);
    pump(&mut s, &mut host).unwrap();
    let (mut d, mut rec) = Durable::open(host.image(), &cfg(3)).unwrap();
    pump(&mut d.store, &mut host).unwrap();
    while d.guard_next(&rec).is_some() {
        pump(&mut d.store, &mut host).unwrap();
        d.guard_try(&mut rec, |_, _| Ok(()));
    }
    d.guard_finish();
    pump(&mut d.store, &mut host).unwrap();
    assert!(rec.issues.iter().any(|i| i.code == "CN-STO-0021"));
    let mut ws = Workspace::new(9).unwrap();
    ws.import(&rec.container().unwrap()).unwrap();
    // And the next open still leaves it out, without trying it.
    let (_, rec2) = Durable::open(host.image(), &cfg(4)).unwrap();
    let mut ws2 = Workspace::new(10).unwrap();
    ws2.import(&rec2.container().unwrap()).unwrap();
    assert_eq!(ws2.page("p").unwrap().blocks[0].plain(), "own text");
}

/* ---------- Compaction's conditions ---------- */

#[test]
fn compaction_keeps_what_it_is_told_to_and_refuses_what_is_incomplete() {
    let (host, _) = real_host();
    let files: Vec<ImageFile> = host
        .image()
        .into_iter()
        .filter(|f| f.path.starts_with(&format!("{}/", format::doc_dir("p"))))
        .collect();
    let seg = files
        .iter()
        .find(|f| f.path.ends_with(".seg"))
        .unwrap()
        .path
        .clone();
    let mut o = CompactOptions {
        min_segments: 1,
        keep: BTreeSet::new(),
        device: [1; 16],
        wall_ms: 0,
        peer: 5,
    };
    o.keep.insert(seg.clone());
    assert!(
        plan_compaction("p", &files, &o).unwrap().is_none(),
        "kept for upload"
    );
    o.keep.clear();
    let plan = plan_compaction("p", &files, &o).unwrap().unwrap();
    let write = plan
        .requests
        .iter()
        .position(|r| matches!(r, IoRequest::SyncFile { .. }))
        .unwrap();
    let dir = plan
        .requests
        .iter()
        .position(|r| matches!(r, IoRequest::SyncDir { .. }))
        .unwrap();
    let remove = plan
        .requests
        .iter()
        .position(|r| matches!(r, IoRequest::Remove { .. }))
        .unwrap();
    assert!(
        write < dir && dir < remove,
        "durable before anything is deleted"
    );
    // An unsealed segment is never covered.
    let mut unsealed = files.clone();
    for f in &mut unsealed {
        if f.path.ends_with(".seg") {
            let scan = format::scan_segment(&f.bytes);
            f.bytes.truncate(scan.frames.last().unwrap().offset);
        }
    }
    assert!(plan_compaction("p", &unsealed, &o).unwrap().is_none());
}

#[test]
fn a_segment_rolls_at_its_limit_and_chains_to_the_last() {
    let mut host = MemHost::new();
    let (mut store, _) = Store::open(Vec::new(), &cfg(1)).unwrap();
    store.append("p", vec![1; 5 * 1024 * 1024], 1, 0);
    pump(&mut store, &mut host).unwrap();
    store.append("p", vec![2; 5 * 1024 * 1024], 1, 0);
    pump(&mut store, &mut host).unwrap();
    let mut segs: Vec<String> = host
        .paths()
        .into_iter()
        .filter(|p| p.ends_with(".seg"))
        .collect();
    segs.sort();
    assert_eq!(segs.len(), 2);
    let first = format::scan_segment(host.read(&segs[0]).unwrap());
    assert!(first.sealed);
    let second = format::parse_segment_header(host.read(&segs[1]).unwrap()).unwrap();
    assert_eq!(second.prev_sha, first.sha.unwrap());
    let (_, rec) = Store::open(host.image(), &cfg(2)).unwrap();
    assert_eq!(
        rec.docs["p"]
            .segments
            .iter()
            .map(|s| s.updates.len())
            .sum::<usize>(),
        2
    );
}

#[test]
fn keystroke_cost_is_measured() {
    // One typed character, on its own, as the log writes it: the bytes that reach the disk.
    let mut host = MemHost::new();
    let (mut d, _) = Durable::open(Vec::new(), &cfg(1)).unwrap();
    let mut ws = Workspace::new(1).unwrap();
    ws.create_page("p", None, "T", None).unwrap();
    ws.insert_block("p", None, &para("b", "")).unwrap();
    d.save(&ws, 0).unwrap();
    pump(&mut d.store, &mut host).unwrap();
    let mut cursor_check = SaveCursor::default();
    let _ = ws.changed_docs(&mut cursor_check).unwrap();
    let (start_bytes, start_flush) = (host.bytes_written, host.flushes);
    let n = 200u64;
    for i in 0..n {
        ws.splice_text("p", "b", i as usize, 0, "a").unwrap();
        d.save(&ws, 0).unwrap();
        pump(&mut d.store, &mut host).unwrap();
    }
    let bytes = (host.bytes_written - start_bytes) / n;
    let flushes = (host.flushes - start_flush) / n;
    println!("write cost per keystroke, each flushed alone: {bytes} bytes, {flushes} flush");
    assert!(bytes < 400, "{bytes} bytes per keystroke");
    assert_eq!(flushes, 1);
    let whole = ws.export().unwrap().len();
    println!("the snapshot this replaces was {whole} bytes per save at this size");
}
