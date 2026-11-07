//! Browser entry points, driven from webapp/tests/perf.mjs.
use crate::{searchbench, sim};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct SearchBench {
    corpus: searchbench::Corpus,
}

#[wasm_bindgen]
impl SearchBench {
    #[wasm_bindgen(constructor)]
    pub fn new(pages: u32, blocks: u32, seed: u32) -> SearchBench {
        SearchBench {
            corpus: searchbench::build(pages as usize, blocks as usize, u64::from(seed)),
        }
    }
    pub fn stats(&self) -> String {
        format!(
            "{{\"docs\":{},\"text_bytes\":{}}}",
            self.corpus.docs, self.corpus.text_bytes
        )
    }
    pub fn queries(&self) -> String {
        serde_json::to_string(&searchbench::queries(&self.corpus.rare)).unwrap_or_default()
    }
    /// Searches and serialises, as the app's `Engine.search` does.
    pub fn search(&self, query: &str) -> String {
        serde_json::to_string(&self.corpus.index.search(query, 40)).unwrap_or_default()
    }
}

/// Runs a short history in wasm and returns the state hash, to compare with a native run.
#[wasm_bindgen]
pub fn sim_hash(seed: u32, years: f64) -> String {
    let mut cfg = sim::Config::spec(u64::from(seed));
    cfg.years = years;
    cfg.pages = 60;
    cfg.long_blocks = 300;
    cfg.db_rows = 200;
    cfg.strokes = 100;
    let (devices, out) = sim::run(cfg, &mut || 0);
    let bytes = devices[0].ws.export().map(|b| b.len()).unwrap_or(0);
    format!(
        "{{\"hash\":\"{:x}\",\"export_bytes\":{},\"divergences\":{},\"errors\":{}}}",
        out.final_hash, bytes, out.counts.divergences, out.counts.op_errors
    )
}
