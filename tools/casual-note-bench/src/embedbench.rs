//! SPK-06: a real model on the labelled set, against the real word-overlap method and exact search.

use casual_note_bench::corpus::LANGUAGES;
use casual_note_bench::labelled::TOPICS;
use casual_note_embed::runtime::{OnnxEmbedder, Prefix};
use casual_note_embed::{Chunk, Embedder, Kind, Vectors, quantise};
use casual_note_model::{Block, BlockKind, Canvas, Page, PageMeta, Run};
use casual_note_search::{Doc, Field, Index, Owner};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Instant;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

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

/// A pool of notes one language: every topic's note, plus the corpus's sentences as distractors.
struct Pool {
    lang: &'static str,
    /// `(topic or none, text)`
    docs: Vec<(Option<usize>, String)>,
}

fn pools() -> Vec<Pool> {
    let mut out: BTreeMap<&str, Pool> = BTreeMap::new();
    for (t, topic) in TOPICS.iter().enumerate() {
        for (lang, note, _) in *topic {
            out.entry(lang)
                .or_insert_with(|| Pool {
                    lang,
                    docs: Vec::new(),
                })
                .docs
                .push((Some(t), (*note).to_string()));
        }
    }
    for l in LANGUAGES {
        let code = if l.code == "zh-Hans" { "zh" } else { l.code };
        if let Some(p) = out.get_mut(code) {
            for s in l.sentences {
                p.docs.push((None, (*s).to_string()));
            }
        }
    }
    out.into_values().collect()
}

fn page(id: &str, text: &str) -> Page {
    Page {
        meta: PageMeta {
            id: id.into(),
            parent: None,
            title: String::new(),
            favorite: false,
            trashed: false,
            database: None,
        },
        blocks: vec![Block {
            id: format!("{id}b"),
            kind: BlockKind::Paragraph,
            checked: false,
            runs: vec![Run {
                text: text.into(),
                marks: vec![],
                link: None,
            }],
            target: None,
        }],
        view: "page".into(),
        canvas: Canvas::default(),
    }
}

/// Scores of each doc in a pool against the query, by the real word-overlap method.
fn word_overlap(query: &str, pool: &Pool) -> Vec<f64> {
    let mut pages = vec![page("q", query)];
    for (i, (_, text)) in pool.docs.iter().enumerate() {
        pages.push(page(&format!("d{i}"), text));
    }
    let graph = casual_note_graph::derive(&pages, &[]);
    let mut scores = vec![0.0; pool.docs.len()];
    for e in graph
        .edges
        .iter()
        .filter(|e| format!("{:?}", e.kind).eq_ignore_ascii_case("similar"))
    {
        let other = if e.from == "page:q" {
            &e.to
        } else if e.to == "page:q" {
            &e.from
        } else {
            continue;
        };
        if let Some(i) = other
            .strip_prefix("page:d")
            .and_then(|s| s.parse::<usize>().ok())
        {
            scores[i] = serde_json::to_value(e)
                .ok()
                .and_then(|v| v["score"].as_f64())
                .unwrap_or(0.0);
        }
    }
    scores
}

/// The same formula with its caps and threshold off: weighted shared terms over all terms, the
/// fairest reading of word overlap. Otherwise a one-sentence note never reaches its minimum terms.
fn word_overlap_uncapped(query: &str, pool: &Pool) -> Vec<f64> {
    use std::collections::BTreeSet;
    let terms = |text: &str| -> BTreeSet<String> {
        casual_note_search::tokenise(text)
            .into_iter()
            .map(|t| t.term)
            .collect()
    };
    let sets: Vec<BTreeSet<String>> = pool.docs.iter().map(|(_, t)| terms(t)).collect();
    let q = terms(query);
    let n = (sets.len() + 1) as f64;
    let weight = |t: &str| {
        let df = sets.iter().filter(|s| s.contains(t)).count() + usize::from(q.contains(t));
        (1.0 + n / df.max(1) as f64).ln()
    };
    sets.iter()
        .map(|s| {
            let shared: f64 = q.intersection(s).map(|t| weight(t)).sum();
            let all: f64 = q.union(s).map(|t| weight(t)).sum();
            if all > 0.0 { shared / all } else { 0.0 }
        })
        .collect()
}

/// Scores by exact search, BM25.
fn exact(query: &str, pool: &Pool) -> Vec<f64> {
    let mut index = Index::new();
    for (i, (_, text)) in pool.docs.iter().enumerate() {
        index.update(
            &Owner::Page(format!("d{i}")),
            vec![Doc {
                item: "b".into(),
                field: Field::Body,
                text: text.clone(),
            }],
        );
    }
    let mut scores = vec![0.0; pool.docs.len()];
    for hit in index.search(query, 200) {
        if let Some(i) = hit
            .id
            .strip_prefix('d')
            .and_then(|s| s.parse::<usize>().ok())
        {
            scores[i] = hit.score as f64;
        }
    }
    scores
}

#[derive(Default, Clone, Copy)]
struct Tally {
    n: u32,
    top1: u32,
    top3: u32,
    rr: f64,
}

impl Tally {
    /// The target's rank, counting docs at a tie pessimistically, and a score of 0 as not found.
    fn add(&mut self, scores: &[f64], target: usize) {
        self.n += 1;
        let s = scores[target];
        if s <= 0.0 && scores.iter().all(|x| *x <= 0.0) {
            return;
        }
        if s <= 0.0 {
            return;
        }
        let rank = 1 + scores
            .iter()
            .enumerate()
            .filter(|(i, x)| *i != target && **x >= s)
            .count();
        if rank == 1 {
            self.top1 += 1;
        }
        if rank <= 3 {
            self.top3 += 1;
        }
        self.rr += 1.0 / rank as f64;
    }
    fn json(&self) -> Value {
        let n = f64::from(self.n.max(1));
        json!({ "queries": self.n, "hit1": f64::from(self.top1) / n, "hit3": f64::from(self.top3) / n, "mrr": self.rr / n })
    }
}

fn dot(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b).map(|(x, y)| f64::from(x * y)).sum()
}

fn doti8(a: &[i8], b: &[i8]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| f64::from(i32::from(*x) * i32::from(*y)))
        .sum::<f64>()
        + 1e9
}

pub fn run(args: &[String]) -> Value {
    let dir = arg(args, "--dir").expect("--dir with model and tokenizer.json");
    let file = arg(args, "--model").unwrap_or_else(|| "model.onnx".into());
    let prefix = if arg(args, "--prefix").as_deref() == Some("e5") {
        Prefix::E5
    } else {
        Prefix::None
    };
    let max: usize = arg(args, "--max-tokens")
        .and_then(|s| s.parse().ok())
        .unwrap_or(128);
    let dims = 384;
    let model_bytes = std::fs::read(format!("{dir}/{file}")).expect("model");
    let tok_bytes = std::fs::read(format!("{dir}/tokenizer.json")).expect("tokenizer");
    let before = rss_kb();
    let t = Instant::now();
    let mut model =
        OnnxEmbedder::from_bytes(&model_bytes, &tok_bytes, dims, prefix, max).expect("load");
    let load_ms = t.elapsed().as_secs_f64() * 1000.0;
    drop(model_bytes);
    let load_rss = rss_kb().saturating_sub(before);

    // Vectors for everything once.
    let pools = pools();
    let mut embed_ms = Vec::new();
    let mut passage_vectors: Vec<Vec<Vec<f32>>> = Vec::new();
    let mut tokens = Vec::new();
    for pool in &pools {
        let mut list = Vec::new();
        for (_, text) in &pool.docs {
            let t = Instant::now();
            list.push(model.embed(&[text.clone()], Kind::Passage).expect("embed"));
            embed_ms.push(t.elapsed().as_secs_f64() * 1000.0);
            tokens.push(model.token_count(text));
        }
        passage_vectors.push(list);
    }
    let mut queries: BTreeMap<(&str, usize), (&str, Vec<f32>)> = BTreeMap::new();
    let mut query_ms = Vec::new();
    for (t_id, topic) in TOPICS.iter().enumerate() {
        for (lang, _, q) in *topic {
            let t = Instant::now();
            let v = model
                .embed(&[(*q).to_string()], Kind::Query)
                .expect("embed");
            query_ms.push(t.elapsed().as_secs_f64() * 1000.0);
            queries.insert((lang, t_id), (q, v));
        }
    }

    let langs: Vec<&str> = pools.iter().map(|p| p.lang).collect();
    let mut table: BTreeMap<String, BTreeMap<&str, Tally>> = BTreeMap::new();
    let mut positive = Vec::new();
    let mut negative = Vec::new();
    for ((qlang, topic), (qtext, qv)) in &queries {
        for (pi, pool) in pools.iter().enumerate() {
            let Some(target) = pool.docs.iter().position(|(t, _)| *t == Some(*topic)) else {
                continue;
            };
            let same = pool.lang == *qlang;
            let group = if same {
                format!("same:{qlang}")
            } else {
                "cross".to_string()
            };
            let groups: Vec<String> = if same {
                vec![group, "same:all".into()]
            } else if *qlang == "en" && pool.lang == "zh" {
                vec![group, "cross:en>zh".into()]
            } else if *qlang == "zh" && pool.lang == "en" {
                vec![group, "cross:zh>en".into()]
            } else {
                vec![group]
            };
            let ml: Vec<f64> = passage_vectors[pi].iter().map(|v| dot(qv, v)).collect();
            let ml8: Vec<f64> = {
                let q8 = quantise(qv);
                passage_vectors[pi]
                    .iter()
                    .map(|v| doti8(&q8, &quantise(v)))
                    .collect()
            };
            for (i, s) in ml.iter().enumerate() {
                if i == target {
                    positive.push(*s)
                } else if same {
                    negative.push(*s)
                }
            }
            let words = word_overlap(qtext, pool);
            let bm25 = exact(qtext, pool);
            let uncapped = word_overlap_uncapped(qtext, pool);
            for g in &groups {
                let row = table.entry(g.clone()).or_default();
                row.entry("model").or_default().add(&ml, target);
                row.entry("model_i8").or_default().add(&ml8, target);
                row.entry("word_overlap").or_default().add(&words, target);
                row.entry("word_overlap_uncapped")
                    .or_default()
                    .add(&uncapped, target);
                row.entry("exact_bm25").or_default().add(&bm25, target);
            }
        }
    }
    let quality: BTreeMap<String, Value> = table
        .iter()
        .map(|(g, row)| {
            (
                g.clone(),
                json!(
                    row.iter()
                        .map(|(k, v)| (k.to_string(), v.json()))
                        .collect::<BTreeMap<_, _>>()
                ),
            )
        })
        .collect();

    // A vector store at scale, with synthetic unit vectors: the scan alone.
    let mut scale = Vec::new();
    for n in [10_000usize, 100_000, 500_000] {
        let mut store = Vectors::new("bench", dims);
        let mut rng = casual_note_bench::rng::Rng::new(9);
        let mut row = vec![0f32; dims];
        let rss0 = rss_kb();
        for i in 0..n {
            for x in row.iter_mut() {
                *x = (rng.next_u64() % 2000) as f32 / 1000.0 - 1.0;
            }
            store
                .submit(
                    &format!("p{}", i / 20),
                    &Chunk {
                        key: format!("{i}"),
                        input: String::new(),
                        hash: String::new(),
                        weight: 1,
                    },
                    &row,
                )
                .expect("submit");
        }
        let mut times = Vec::new();
        for _ in 0..7 {
            let t = Instant::now();
            std::hint::black_box(store.nearest(&row, 10).expect("nearest"));
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        scale.push(json!({ "chunks": n, "scan_ms_p50": pct(&mut times, 0.5), "rss_kb": rss_kb().saturating_sub(rss0), "file_bytes": store.dump().len() }));
    }

    // Rebuild: the same input twice gives the same bits.
    let probe = "Rebuild check 重建检查 إعادة البناء".to_string();
    let a = model
        .embed(std::slice::from_ref(&probe), Kind::Passage)
        .expect("embed");
    let b = model
        .embed(std::slice::from_ref(&probe), Kind::Passage)
        .expect("embed");

    // Reference vectors, for comparing with the wasm build.
    let refs: Vec<(String, Vec<f32>)> = [
        "Call the plumber about the leak.",
        "周五之前给水管工打电话。",
        "الاتصال بالسباك قبل يوم الجمعة.",
        "शुक्रवार से पहले प्लंबर को फ़ोन करना है।",
    ]
    .iter()
    .map(|s| {
        (
            (*s).to_string(),
            model
                .embed(&[(*s).to_string()], Kind::Passage)
                .expect("embed"),
        )
    })
    .collect();
    if let Some(path) = arg(args, "--refs") {
        std::fs::write(path, serde_json::to_string(&refs).expect("json")).expect("write refs");
    }

    let mut long_ms = Vec::new();
    let long_text = LANGUAGES[0].sentences.join(" ").repeat(3);
    for _ in 0..5 {
        let t = Instant::now();
        std::hint::black_box(
            model
                .embed(std::slice::from_ref(&long_text), Kind::Passage)
                .expect("embed"),
        );
        long_ms.push(t.elapsed().as_secs_f64() * 1000.0);
    }

    positive.sort_by(f64::total_cmp);
    negative.sort_by(f64::total_cmp);
    let mut token_values: Vec<f64> = tokens.iter().map(|x| *x as f64).collect();
    let tokens_p50 = pct(&mut token_values, 0.5);
    json!({
        "model": file, "dir": dir, "languages": langs, "topics": TOPICS.len(),
        "load_ms": load_ms, "open_length_error": model.open_length_error(), "load_rss_kb": load_rss, "final_rss_kb": rss_kb(),
        "embed_passage_ms": { "p50": pct(&mut embed_ms, 0.5), "p95": pct(&mut embed_ms, 0.95) },
        "embed_query_ms": { "p50": pct(&mut query_ms, 0.5), "p95": pct(&mut query_ms, 0.95) },
        "embed_at_token_cap_ms": pct(&mut long_ms, 0.5),
        "tokens_p50": tokens_p50,
        "quality": quality,
        "cosine": {
            "relevant_p10": positive.get(positive.len() / 10), "relevant_p50": positive.get(positive.len() / 2),
            "irrelevant_p50": negative.get(negative.len() / 2), "irrelevant_p90": negative.get(negative.len() * 9 / 10), "irrelevant_p99": negative.get(negative.len() * 99 / 100),
        },
        "scan": scale,
        "same_bits_on_repeat": a == b,
    })
}
