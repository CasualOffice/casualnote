//! The browser's module: inference and the vector store, in a worker, off the typing path.

use crate::runtime::{OnnxEmbedder, Prefix};
use crate::{Chunk, Embedder, Kind, Vectors};
use wasm_bindgen::prelude::*;

fn fail(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// A model in memory.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Model {
    inner: OnnxEmbedder,
}

#[wasm_bindgen]
impl Model {
    /// `prefix` is `none` or `e5`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        model: &[u8],
        tokenizer: &[u8],
        dims: u32,
        prefix: &str,
        max_tokens: u32,
    ) -> Result<Model, JsError> {
        let prefix = if prefix == "e5" {
            Prefix::E5
        } else {
            Prefix::None
        };
        Ok(Self {
            inner: OnnxEmbedder::from_bytes(
                model,
                tokenizer,
                dims as usize,
                prefix,
                max_tokens as usize,
            )
            .map_err(fail)?,
        })
    }

    /// One normalised vector.
    pub fn embed(&mut self, text: &str, query: bool) -> Result<Vec<f32>, JsError> {
        let kind = if query { Kind::Query } else { Kind::Passage };
        self.inner.embed(&[text.to_string()], kind).map_err(fail)
    }
}

/// The derived store for one model.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Store {
    inner: Vectors,
}

#[wasm_bindgen]
impl Store {
    #[wasm_bindgen(constructor)]
    pub fn new(tag: &str, dims: u32) -> Store {
        Self {
            inner: Vectors::new(tag, dims as usize),
        }
    }

    /// Reads a saved store. A damaged or foreign one is an error, and the host starts afresh.
    pub fn load(tag: &str, dims: u32, bytes: &[u8]) -> Result<Store, JsError> {
        Ok(Self {
            inner: Vectors::load(tag, dims as usize, bytes).map_err(fail)?,
        })
    }

    pub fn dump(&self) -> Vec<u8> {
        self.inner.dump()
    }

    pub fn len(&self) -> u32 {
        self.inner.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// The chunks of a page, as JSON from the engine, that still need a vector, as JSON.
    pub fn plan(&mut self, page: &str, chunks: &str) -> Result<String, JsError> {
        let list: Vec<PlainChunk> = serde_json::from_str(chunks).map_err(fail)?;
        let list: Vec<Chunk> = list.into_iter().map(Into::into).collect();
        serde_json::to_string(&self.inner.plan(page, &list)).map_err(fail)
    }

    pub fn submit(&mut self, page: &str, chunk: &str, vector: &[f32]) -> Result<(), JsError> {
        let plain: PlainChunk = serde_json::from_str(chunk).map_err(fail)?;
        self.inner.submit(page, &plain.into(), vector).map_err(fail)
    }

    pub fn remove_page(&mut self, page: &str) {
        self.inner.remove_page(page);
    }

    pub fn page_ids(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.inner.page_ids()).map_err(fail)
    }

    pub fn nearest(&self, query: &[f32], k: u32) -> Result<String, JsError> {
        serde_json::to_string(&self.inner.nearest(query, k as usize).map_err(fail)?).map_err(fail)
    }

    pub fn related(&self, page: &str, k: u32, threshold: f32) -> Result<String, JsError> {
        serde_json::to_string(&self.inner.related(page, k as usize, threshold)).map_err(fail)
    }
}

#[derive(serde::Deserialize)]
struct PlainChunk {
    key: String,
    #[serde(default)]
    input: String,
    hash: String,
    #[serde(default)]
    weight: u32,
}

impl From<PlainChunk> for Chunk {
    fn from(c: PlainChunk) -> Self {
        Chunk {
            key: c.key,
            input: c.input,
            hash: c.hash,
            weight: c.weight,
        }
    }
}
