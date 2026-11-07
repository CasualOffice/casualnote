//! FND-20: what happens when two replicas of one workspace write under the same peer id, as a
//! copied folder, a restored backup or a duplicated process would if a peer were not minted fresh.

use casual_note_merge::Workspace;
use casual_note_model::{Block, BlockKind};
use casual_note_ops::{Op, apply};
use serde_json::{Value, json};

fn seeded(peer: u64) -> Workspace {
    let mut ws = Workspace::new(peer).expect("new");
    for op in [
        Op::CreatePage {
            id: "p".into(),
            parent: None,
            title: "Fork".into(),
            database: None,
        },
        Op::InsertBlock {
            page: "p".into(),
            after: None,
            block: Block {
                id: "b".into(),
                kind: BlockKind::Paragraph,
                checked: false,
                runs: vec![],
                target: None,
            },
        },
    ] {
        apply(&mut ws, &op).expect("seed");
    }
    ws
}

fn type_text(ws: &mut Workspace, text: &str) -> Result<(), String> {
    for c in text.chars() {
        let at = ws.page("p").map_err(|e| format!("{e:?}"))?.blocks[0]
            .plain()
            .encode_utf16()
            .count();
        apply(
            ws,
            &Op::SpliceText {
                page: "p".into(),
                block: "b".into(),
                at,
                delete: 0,
                insert: c.to_string(),
            },
        )
        .map_err(|e| format!("{e:?}"))?;
    }
    Ok(())
}

fn text(ws: &Workspace) -> String {
    ws.page("p")
        .map(|p| p.blocks[0].plain())
        .unwrap_or_default()
}

/// Forks `a` into a second replica opened under `b_peer`, writes on both, and merges both ways.
fn case(name: &str, b_peer: u64, restore_older: bool) -> Value {
    let mut a = seeded(7);
    type_text(&mut a, "base ").expect("type");
    let snapshot = a.export().expect("export");
    if restore_older {
        // A writes on after the backup is taken, and B, restored from it, overlaps those counters.
        type_text(&mut a, "after-backup ").expect("type");
    }
    let mut b = Workspace::new(b_peer).expect("new");
    b.import(&snapshot).expect("import");
    let a_text = "AAAAAAAA";
    let b_text = "BBBBBBBB";
    type_text(&mut a, a_text).expect("a types");
    let b_typed = type_text(&mut b, b_text);
    let a_before = text(&a);
    let b_before = text(&b);
    let a_import = a
        .export()
        .and_then(|bytes| b.import(&bytes))
        .map_err(|e| format!("{e:?}"));
    let b_import = b
        .export()
        .and_then(|bytes| a.import(&bytes))
        .map_err(|e| format!("{e:?}"));
    let (ta, tb) = (text(&a), text(&b));
    json!({
        "case": name,
        "a_before": a_before, "b_before": b_before,
        "b_typed": b_typed.is_ok(),
        "a_after": ta, "b_after": tb,
        "converged": ta == tb,
        "a_edit_survives_on_both": ta.contains(a_text) && tb.contains(a_text),
        "b_edit_survives_on_both": ta.contains(b_text) && tb.contains(b_text),
        "import_into_b": format!("{a_import:?}"),
        "import_into_a": format!("{b_import:?}"),
    })
}

/// The same question on the bare library, so the answer is not an artefact of our adapter.
fn raw_loro() -> Value {
    use loro::{ExportMode, LoroDoc};
    let a = LoroDoc::new();
    a.set_peer_id(7).expect("peer");
    a.get_text("t").insert(0, "base ").expect("insert");
    a.commit();
    let snapshot = a.export(ExportMode::Snapshot).expect("export");
    let b = LoroDoc::new();
    b.set_peer_id(7).expect("peer");
    b.import(&snapshot).expect("import");
    a.get_text("t").insert(5, "AAAA").expect("insert");
    a.commit();
    b.get_text("t").insert(5, "BBBB").expect("insert");
    b.commit();
    let into_a = a.import(&b.export(ExportMode::all_updates()).expect("export"));
    let into_b = b.import(&a.export(ExportMode::all_updates()).expect("export"));
    json!({
        "a": a.get_text("t").to_string(), "b": b.get_text("t").to_string(),
        "a_import": format!("{:?}", into_a.map(|s| s.pending.is_some())),
        "b_import": format!("{:?}", into_b.map(|s| s.pending.is_some())),
        "a_frontiers": format!("{:?}", a.oplog_frontiers()), "b_frontiers": format!("{:?}", b.oplog_frontiers()),
    })
}

/// Peer table growth: one small page, a new peer for each open.
pub fn peer_growth(opens: usize) -> Value {
    let mut ws = seeded(1000);
    let mut series = Vec::new();
    for i in 0..opens {
        let bytes = ws.export().expect("export");
        if i % (opens / 8).max(1) == 0 {
            series.push(json!({ "opens": i, "bytes": bytes.len(), "live": text(&ws).len() }));
        }
        let mut next = Workspace::new(2000 + i as u64).expect("new");
        next.import(&bytes).expect("import");
        ws = next;
        let at = text(&ws).encode_utf16().count();
        apply(
            &mut ws,
            &Op::SpliceText {
                page: "p".into(),
                block: "b".into(),
                at,
                delete: 0,
                insert: "x".into(),
            },
        )
        .expect("type");
    }
    json!(series)
}

/// Hostile bytes: flipped, truncated and spliced containers must error, never panic.
pub fn hostile_imports(rounds: usize, seed: u64) -> Value {
    let mut rng = crate::rng::Rng::new(seed);
    let mut ws = seeded(3);
    type_text(&mut ws, "hostile input target").expect("type");
    let good = ws.export().expect("export");
    let (mut errors, mut accepted, mut panics) = (0u32, 0u32, 0u32);
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut first_panic = None;
    for r in 0..rounds {
        let mut bytes = good.clone();
        match r % 3 {
            0 => {
                for _ in 0..rng.range(1, 8) {
                    let i = rng.below(bytes.len());
                    bytes[i] ^= 1 << rng.below(8);
                }
            }
            1 => bytes.truncate(rng.below(good.len())),
            _ => {
                let i = rng.below(bytes.len());
                let n = rng.below(64).min(bytes.len() - i);
                for b in &mut bytes[i..i + n] {
                    *b = rng.next_u64() as u8;
                }
            }
        }
        let result = std::panic::catch_unwind(|| {
            let mut target = seeded(4);
            target.import(&bytes).map(|_| target.page("p").is_ok())
        });
        match result {
            Ok(Ok(_)) => accepted += 1,
            Ok(Err(_)) => errors += 1,
            Err(e) => {
                panics += 1;
                if first_panic.is_none() {
                    first_panic = e
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_string()));
                }
            }
        }
    }
    std::panic::set_hook(hook);
    json!({ "rounds": rounds, "refused": errors, "accepted": accepted, "panics": panics, "first_panic": first_panic })
}

/// Everything FND-20 asks.
pub fn report() -> Value {
    json!({
        "same_peer_concurrent_clone": case("same peer, cloned, both write", 7, false),
        "same_peer_restored_backup": case("same peer, restored an older backup, both write", 7, true),
        "fresh_peer_clone": case("fresh peer, cloned, both write", 8, false),
        "fresh_peer_restored_backup": case("fresh peer, restored an older backup, both write", 8, true),
        "bare_loro_same_peer": raw_loro(),
    })
}

/// Tries to reach "counter should be continuous" on the bare library: a replica that opened a
/// snapshot reads old history, so only some of a peer's blocks are parsed, then gets that peer's
/// newer changes. See docs/30, SPK-01.
pub fn lazy_blocks(ops: usize) -> Value {
    use loro::{ExportMode, Frontiers, ID, LoroDoc};
    let mut results = Vec::new();
    for variant in [
        "read-state",
        "checkout-old",
        "export-old-updates",
        "fork-at-old",
    ] {
        let r = std::panic::catch_unwind(|| {
            let a = LoroDoc::new();
            a.set_peer_id(1).expect("peer");
            let t = a.get_text("t");
            for i in 0..ops {
                t.insert(i, "x").expect("insert");
                a.commit();
            }
            let snapshot = a.export(ExportMode::Snapshot).expect("export");
            let b = LoroDoc::new();
            b.set_peer_id(2).expect("peer");
            b.import(&snapshot).expect("import");
            let old = Frontiers::from(ID::new(1, 5));
            match variant {
                "read-state" => {
                    b.get_text("t").to_string();
                }
                "checkout-old" => {
                    b.checkout(&old).expect("checkout");
                    b.checkout_to_latest();
                }
                "export-old-updates" => {
                    let _ = b.export(ExportMode::updates(&Default::default()));
                }
                _ => {
                    let _ = b.fork_at(&old);
                }
            }
            let vv = a.oplog_vv();
            for i in 0..50 {
                t.insert(ops + i, "y").expect("insert");
                a.commit();
            }
            let update = a.export(ExportMode::updates(&vv)).expect("update");
            b.import(&update).expect("import");
            // Reopen and take the same peer's newer changes again.
            let c = LoroDoc::new();
            c.set_peer_id(3).expect("peer");
            c.import(&b.export(ExportMode::Snapshot).expect("export"))
                .expect("import");
            let _ = c.fork_at(&old);
            let vv = a.oplog_vv();
            t.insert(ops + 50, "z").expect("insert");
            a.commit();
            c.import(&a.export(ExportMode::updates(&vv)).expect("update"))
                .expect("import");
            c.get_text("t").len_unicode() == a.get_text("t").len_unicode()
        });
        results.push(json!({ "variant": variant, "panicked": r.is_err(), "equal": r.ok() }));
    }
    json!(results)
}
