//! `casual-note-bench history|load|fork|peers|hostile|scale|search|embed`. Results print as JSON.

use casual_note_bench::{forks, searchbench, sim};
use casual_note_merge::Workspace;
use casual_note_model::{Block, BlockKind, Run};
use casual_note_ops::{Op, apply};
use serde_json::{Value, json};
use std::time::Instant;

mod embedbench;

fn rss_kb() -> u64 {
    let pid = std::process::id().to_string();
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0)
}

fn pct(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values
        .get(((values.len() as f64 - 1.0) * p).round() as usize)
        .copied()
        .unwrap_or(f64::NAN)
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn history(args: &[String]) -> Value {
    let seed = arg(args, "--seed")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20_261_107);
    let mut cfg = sim::Config::spec(seed);
    if let Some(y) = arg(args, "--years").and_then(|s| s.parse().ok()) {
        cfg.years = y;
    }
    if let Some(d) = arg(args, "--sync-days").and_then(|s| s.parse().ok()) {
        cfg.sync_days = d;
    }
    if let Some(p) = arg(args, "--pages").and_then(|s| s.parse().ok()) {
        cfg.pages = p;
    }
    if args.iter().any(|a| a == "--mini") {
        // The same shape as the browser's `sim_hash`, to compare native and wasm.
        cfg.pages = 60;
        cfg.long_blocks = 300;
        cfg.db_rows = 200;
        cfg.strokes = 100;
    }
    cfg.delta = args.iter().any(|a| a == "--delta");
    cfg.dump_day = arg(args, "--dump-day").and_then(|s| s.parse().ok());
    cfg.dump_prefix = arg(args, "--dump-prefix");
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!(
            "STAGE AT PANIC: day {}: {}",
            sim::DAY.load(std::sync::atomic::Ordering::Relaxed),
            sim::STAGE.lock().map(|s| s.clone()).unwrap_or_default()
        );
        default_hook(info);
    }));
    let started = Instant::now();
    let (devices, out) = sim::run(cfg.clone(), &mut rss_kb);
    let wall = started.elapsed().as_secs_f64();
    let bytes = devices[0].ws.export().expect("export");
    if let Some(path) = arg(args, "--out") {
        std::fs::write(&path, &bytes).expect("write the export");
    }
    let live = sim::live_bytes(&devices[0].ws);
    let live_total = live["live_bytes"].as_u64().unwrap_or(1).max(1);
    let c = &out.counts;
    let by_part: Vec<u64> = sim::parts(&bytes)
        .iter()
        .map(|(_, b)| b.len() as u64)
        .collect();
    json!({
        "seed": cfg.seed, "delta": cfg.delta, "years": cfg.years, "sync_days": cfg.sync_days, "wall_seconds": wall, "pages_target": cfg.pages,
        "peak_rss_kb_at_end": rss_kb(),
        "counts": format!("{c:?}"),
        "errors": out.first_errors,
        "divergent": out.divergent,
        "divergences": c.divergences, "hash_compares": c.hash_compares,
        "merge_after_offline_ms": out.merge_after_offline_ms,
        "yearly": out.yearly,
        "ms": { "edit": out.ms_edit, "sync": out.ms_sync, "check": out.ms_check, "reopen": out.ms_reopen },
        "per_keystroke_us": out.ms_edit * 1000.0 / c.keystrokes.max(1) as f64,
        "export_bytes": bytes.len(),
        "largest_part": by_part.iter().max(),
        "documents": by_part.len(),
        "live": live,
        "ratio_to_live": bytes.len() as f64 / live_total as f64,
        "typed_bytes": c.typed_bytes,
        "ratio_to_typed": bytes.len() as f64 / c.typed_bytes.max(1) as f64,
        "opens": devices.iter().map(|d| d.opens).collect::<Vec<_>>(),
        "final_hash": format!("{:x}", out.final_hash),
    })
}

/// PERF-03 on a stored history: boot as the app does today, and page by page as ADR-030 designs.
fn load(args: &[String]) -> Value {
    let path = arg(args, "--file").expect("--file");
    let bytes = std::fs::read(&path).expect("read");
    let before = rss_kb();
    let parts = sim::parts(&bytes);

    // What the app does at start: import the whole workspace, then read the open page.
    let mut boots = Vec::new();
    let mut rss_after = 0;
    let mut pages = Vec::new();
    for i in 0..5 {
        let t = Instant::now();
        let mut ws = Workspace::new(50 + i).expect("new");
        ws.import(&bytes).expect("import");
        let imported = t.elapsed().as_secs_f64() * 1000.0;
        rss_after = rss_after.max(rss_kb());
        if pages.is_empty() {
            for m in ws.pages() {
                let n = ws.page(&m.id).map(|p| p.blocks.len()).unwrap_or(0);
                pages.push((m.id, n));
            }
        }
        boots.push(imported);
    }

    // Typical pages: those nearest the 200-block page of docs/24.
    pages.sort_by_key(|(_, n)| n.abs_diff(200));
    let typical: Vec<(String, usize)> = pages.iter().take(100).cloned().collect();
    let tree = parts.iter().find(|(n, _)| n.is_empty()).expect("tree").1;

    // Page by page: the tree already open, one page's document loaded and read.
    let mut base = Workspace::new(90).expect("new");
    base.import(&sim::assemble([("", tree)])).expect("tree");
    let tree_ms = {
        let t = Instant::now();
        let mut w = Workspace::new(91).expect("new");
        w.import(&sim::assemble([("", tree)])).expect("tree");
        t.elapsed().as_secs_f64() * 1000.0
    };
    let mut lazy = Vec::new();
    let mut read_only = Vec::new();
    let mut sizes = Vec::new();
    for (id, _) in &typical {
        let body = parts.iter().find(|(n, _)| n == id).expect("part").1;
        sizes.push(body.len() as f64);
        let single = sim::assemble([(id.as_str(), body)]);
        let t = Instant::now();
        base.import(&single).expect("page");
        let page = base.page(id).expect("read");
        lazy.push(t.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(serde_json::to_string(&page).ok());
        let t = Instant::now();
        std::hint::black_box(base.page(id).expect("read"));
        read_only.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    // The long page.
    let long = parts.iter().find(|(n, _)| n == "long");
    let long_ms = long.map(|(n, body)| {
        let mut w = Workspace::new(92).expect("new");
        w.import(&sim::assemble([("", tree)])).expect("tree");
        let single = sim::assemble([(n.as_str(), *body)]);
        let t = Instant::now();
        w.import(&single).expect("page");
        let blocks = w.page(n).expect("read").blocks.len();
        (t.elapsed().as_secs_f64() * 1000.0, blocks, body.len())
    });

    let mut ws = Workspace::new(93).expect("new");
    ws.import(&bytes).expect("import");
    let mut exports = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        std::hint::black_box(ws.export().expect("export"));
        exports.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let mut commits = Vec::new();
    let id = &typical[0].0;
    let page = ws.page(id).expect("page");
    let block = page
        .blocks
        .iter()
        .find(|b| !b.plain().is_empty())
        .expect("text")
        .id
        .clone();
    for _ in 0..500 {
        let t = Instant::now();
        apply(
            &mut ws,
            &Op::SpliceText {
                page: id.clone(),
                block: block.clone(),
                at: 0,
                delete: 0,
                insert: "x".into(),
            },
        )
        .expect("type");
        commits.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    let big = pages.iter().map(|(_, n)| *n).max();
    json!({
        "file_bytes": bytes.len(), "rss_kb_before": before, "rss_kb_after_boot": rss_after,
        "whole_workspace_import_ms": { "samples": boots.clone(), "p95": pct(&mut boots, 0.95) },
        "tree_only_import_ms": tree_ms,
        "one_page_import_and_read_ms": { "pages": lazy.len(), "p50": pct(&mut lazy, 0.5), "p95": pct(&mut lazy, 0.95), "max": pct(&mut lazy, 1.0) },
        "typical_page_blocks": { "first": typical.first().map(|x| x.1), "last": typical.last().map(|x| x.1) },
        "typical_page_doc_bytes_p50": pct(&mut sizes, 0.5),
        "loaded_page_read_ms": { "p50": pct(&mut read_only, 0.5), "p95": pct(&mut read_only, 0.95) },
        "long_page": long_ms.map(|(ms, b, n)| json!({ "import_and_read_ms": ms, "blocks": b, "doc_bytes": n })),
        "largest_blocks": big,
        "whole_workspace_export_ms": { "samples": exports.clone(), "p95": pct(&mut exports, 0.95) },
        "one_keystroke_apply_ms": { "p50": pct(&mut commits, 0.5), "p95": pct(&mut commits, 0.95) },
    })
}

/// How the merge layer itself scales in page count: real ops, no shortcuts.
fn scale(args: &[String]) -> Value {
    let limit: usize = arg(args, "--pages")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20_000);
    let mut ws = Workspace::new(1).expect("new");
    let mut out = Vec::new();
    let mut created = 0usize;
    for target in [1000usize, 5000, 10_000, 20_000, 50_000, 100_000] {
        if target > limit {
            break;
        }
        let t = Instant::now();
        let n = target - created;
        for i in created..target {
            apply(
                &mut ws,
                &Op::CreatePage {
                    id: format!("p{i}"),
                    parent: None,
                    title: "t".into(),
                    database: None,
                },
            )
            .expect("page");
            apply(
                &mut ws,
                &Op::InsertBlock {
                    page: format!("p{i}"),
                    after: None,
                    block: Block {
                        id: "b".into(),
                        kind: BlockKind::Paragraph,
                        checked: false,
                        runs: vec![Run {
                            text: "x".into(),
                            marks: vec![],
                            link: None,
                        }],
                        target: None,
                    },
                },
            )
            .expect("block");
        }
        let per = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
        created = target;
        let t = Instant::now();
        std::hint::black_box(ws.pages());
        let pages_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        std::hint::black_box(ws.page(&format!("p{}", target / 2)).expect("page"));
        let page_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        apply(
            &mut ws,
            &Op::SpliceText {
                page: format!("p{}", target / 2),
                block: "b".into(),
                at: 0,
                delete: 0,
                insert: "y".into(),
            },
        )
        .expect("type");
        let splice_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        let bytes = ws.export().expect("export");
        let export_ms = t.elapsed().as_secs_f64() * 1000.0;
        let line = json!({ "pages": target, "create_page_plus_block_ms_each": per, "pages_ms": pages_ms, "page_ms": page_ms, "splice_ms": splice_ms, "export_ms": export_ms, "export_bytes": bytes.len(), "rss_kb": rss_kb() });
        eprintln!("{line}");
        out.push(line);
    }
    json!(out)
}

fn search(args: &[String]) -> Value {
    let pages: usize = arg(args, "--pages")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
    let blocks: usize = arg(args, "--blocks")
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let before = rss_kb();
    let t = Instant::now();
    let corpus = searchbench::build(pages, blocks, 7);
    let build_ms = t.elapsed().as_secs_f64() * 1000.0;
    let after = rss_kb();
    let queries = searchbench::queries(&corpus.rare);
    let mut all = Vec::new();
    let mut per_query = Vec::new();
    for q in &queries {
        let mut times = Vec::new();
        let mut hits = 0;
        for _ in 0..3 {
            let t = Instant::now();
            hits = corpus.index.search(q, 40).len();
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        all.extend_from_slice(&times);
        per_query.push(json!({ "q": q, "ms": pct(&mut times, 0.5), "hits": hits }));
    }
    per_query.sort_by(|a, b| {
        b["ms"]
            .as_f64()
            .partial_cmp(&a["ms"].as_f64())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    json!({
        "pages": pages, "docs": corpus.docs, "text_bytes": corpus.text_bytes, "build_ms": build_ms,
        "index_rss_kb": after.saturating_sub(before),
        "index_rss_over_text": (after.saturating_sub(before) * 1024) as f64 / corpus.text_bytes.max(1) as f64,
        "p50": pct(&mut all, 0.5), "p95": pct(&mut all, 0.95), "max": pct(&mut all, 1.0),
        "slowest": per_query.iter().take(5).collect::<Vec<_>>(),
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("history") => history(&args),
        Some("load") => load(&args),
        Some("fork") => forks::report(),
        Some("peers") => forks::peer_growth(
            arg(&args, "--opens")
                .and_then(|s| s.parse().ok())
                .unwrap_or(5000),
        ),
        Some("hostile") => forks::hostile_imports(
            arg(&args, "--rounds")
                .and_then(|s| s.parse().ok())
                .unwrap_or(5000),
            11,
        ),
        Some("repro") => forks::lazy_blocks(
            arg(&args, "--ops")
                .and_then(|s| s.parse().ok())
                .unwrap_or(40_000),
        ),
        Some("scale") => scale(&args),
        Some("search") => search(&args),
        Some("embed") => embedbench::run(&args),
        _ => {
            eprintln!(
                "usage: casual-note-bench history|load|fork|peers|hostile|scale|search|embed"
            );
            std::process::exit(2);
        }
    };
    println!("{}", serde_json::to_string_pretty(&result).expect("json"));
}
