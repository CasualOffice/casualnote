//! Meaning search: chunking, the vector store and nearest-neighbour search. See docs/60.
//!
//! Inference is behind a seam. This crate's core never opens a model, never touches a network and
//! knows nothing of where vectors come from: a host calls `Vectors::submit`. Vectors are derived,
//! per block, rebuildable and never synced or exported. The optional `runtime` feature holds one
//! host's implementation of the seam, and the main engine does not link it.

use casual_note_model::{BlockKind, Page};
use serde::Serialize;
use std::collections::{BTreeMap, BinaryHeap};
use unicode_segmentation::UnicodeSegmentation;

#[cfg(feature = "runtime")]
pub mod runtime;
#[cfg(feature = "web")]
mod web;

/// Changes when the chunk boundaries or context prefix change, which forces a rebuild.
pub const CHUNKER_VERSION: u16 = 1;
/// The most graphemes one chunk's input holds. A conservative stand-in for tokens: the
/// tokeniser belongs to the host (docs/60).
pub const INPUT_BUDGET: usize = 480;
/// Chunks embedded per block, from docs/60.
pub const MAX_CHUNKS_PER_BLOCK: usize = 64;
/// Chunks in a store, from docs/60.
pub const MAX_CHUNKS: usize = 1_000_000;

const MAGIC: &[u8; 6] = b"CNVEC1";

/// Whether text is a query or a passage: some models want a different prefix for each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Query,
    Passage,
}

/// The seam. A host implements this in whatever runtime suits it: a browser worker, or a separate
/// desktop process.
pub trait Embedder {
    /// How many values a vector has.
    fn dims(&self) -> usize;
    /// One normalised vector per text, joined end to end.
    fn embed(&mut self, texts: &[String], kind: Kind) -> Result<Vec<f32>, EmbedError>;
}

/// Why a vector was refused or could not be made. Codes are in docs/60.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmbedError {
    /// `CN-EMB-0002`: the file was made by another model.
    WrongTag,
    /// `CN-EMB-0003`: the count or dimensions are wrong.
    BadDimensions { expected: usize, got: usize },
    /// `CN-EMB-0004`: a non-finite value, which would poison the top-k.
    NotFinite,
    /// `CN-EMB-0005`: a vector file is damaged. Delete and rebuild.
    Damaged(&'static str),
    /// `CN-EMB-0006`: the embedder failed.
    Runtime(String),
    /// `CN-EMB-0007`: the store is full.
    Full,
}

impl EmbedError {
    /// The stable code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::WrongTag => "CN-EMB-0002",
            Self::BadDimensions { .. } => "CN-EMB-0003",
            Self::NotFinite => "CN-EMB-0004",
            Self::Damaged(_) => "CN-EMB-0005",
            Self::Runtime(_) => "CN-EMB-0006",
            Self::Full => "CN-EMB-0007",
        }
    }
}

impl std::fmt::Display for EmbedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadDimensions { expected, got } => {
                write!(f, "{}: expected {expected} values, got {got}", self.code())
            }
            Self::Damaged(why) => write!(f, "{}: {why}", self.code()),
            Self::Runtime(why) => write!(f, "{}: {why}", self.code()),
            _ => f.write_str(self.code()),
        }
    }
}

impl std::error::Error for EmbedError {}

/// One piece of a page to embed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Chunk {
    /// The block or item, then the ordinal.
    pub key: String,
    /// What the model sees: title, nearest heading and text, inside the budget.
    pub input: String,
    /// Identifies the input, so an edit that restores earlier text, or a move, costs nothing.
    pub hash: String,
    /// Graphemes in the block's own text, the weight in the page vector.
    pub weight: u32,
}

fn fnv(parts: &[&str]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ u64::from(CHUNKER_VERSION);
    for p in parts {
        for b in p.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
        h ^= 0xFF;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

/// Cuts text at sentence boundaries so that no piece is over the budget.
fn split(text: &str, budget: usize) -> Vec<String> {
    if text.graphemes(true).count() <= budget {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut piece = String::new();
    let mut used = 0;
    for sentence in text.split_sentence_bounds() {
        let n = sentence.graphemes(true).count();
        if used + n > budget && used > 0 {
            out.push(std::mem::take(&mut piece));
            used = 0;
        }
        if n > budget {
            // One sentence longer than the budget: a hard cut, on graphemes.
            let all: Vec<&str> = sentence.graphemes(true).collect();
            for cut in all.chunks(budget) {
                out.push(cut.concat());
            }
            continue;
        }
        piece.push_str(sentence);
        used += n;
    }
    if !piece.is_empty() {
        out.push(piece);
    }
    out
}

/// The chunks of a page, in order: from the rules in docs/60. Blocks with no letters or numbers,
/// dividers and references give none.
pub fn chunks(page: &Page) -> Vec<Chunk> {
    let mut out = Vec::new();
    let title = page.meta.title.as_str();
    let mut heading = String::new();
    let mut push = |item: &str, text: &str, heading: &str| {
        if !text.chars().any(char::is_alphanumeric) {
            return;
        }
        let prefix = match (title.is_empty(), heading.is_empty()) {
            (false, false) => format!("{title}. {heading}. "),
            (false, true) => format!("{title}. "),
            (true, false) => format!("{heading}. "),
            (true, true) => String::new(),
        };
        let room = INPUT_BUDGET
            .saturating_sub(prefix.graphemes(true).count())
            .max(64);
        for (i, piece) in split(text, room)
            .into_iter()
            .take(MAX_CHUNKS_PER_BLOCK)
            .enumerate()
        {
            let input = format!("{prefix}{piece}");
            let hash = format!("{:016x}", fnv(&[&input]));
            let weight = u32::try_from(piece.graphemes(true).count()).unwrap_or(u32::MAX);
            out.push(Chunk {
                key: format!("{item}#{i}"),
                input,
                hash,
                weight,
            });
        }
    };
    if page.blocks.is_empty() && page.canvas.items.is_empty() && !title.is_empty() {
        push("", title, "");
    }
    for block in &page.blocks {
        if matches!(
            block.kind,
            BlockKind::Divider | BlockKind::Database | BlockKind::Embed
        ) {
            continue;
        }
        let text = block.plain();
        push(&block.id, &text, &heading);
        if matches!(
            block.kind,
            BlockKind::Heading1 | BlockKind::Heading2 | BlockKind::Heading3
        ) {
            heading = text;
        }
    }
    for item in &page.canvas.items {
        push(&item.id, &item.text, "");
    }
    out
}

/// A nearest-neighbour result.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Near {
    pub page: String,
    pub key: String,
    /// Cosine similarity in millionths, because vectors are normalised.
    pub score: i64,
}

#[derive(Clone, Debug)]
struct Entry {
    hash: String,
    weight: u32,
    at: usize,
}

/// A store for one model. The tag is checked on load, so vectors from one model are never compared
/// with another's.
#[derive(Debug)]
pub struct Vectors {
    tag: String,
    dims: usize,
    pages: BTreeMap<String, BTreeMap<String, Entry>>,
    data: Vec<f32>,
    owners: Vec<(String, String)>,
    free: Vec<usize>,
}

#[derive(PartialEq)]
struct Scored(f32, usize);
impl Eq for Scored {}
impl Ord for Scored {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // A min-heap on score, and ties go to the lower slot, so results do not depend on heap order.
        other
            .0
            .total_cmp(&self.0)
            .then_with(|| self.1.cmp(&other.1))
    }
}
impl PartialOrd for Scored {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    // Plain loops the compiler can vectorise: docs/60 forbids intrinsics here.
    let mut sum = 0.0;
    for (x, y) in a.iter().zip(b) {
        sum += x * y;
    }
    sum
}

fn normalise(v: &mut [f32]) {
    let n = dot(v, v).sqrt();
    if n > 0.0 {
        for x in v {
            *x /= n;
        }
    }
}

fn millionths(v: f32) -> i64 {
    (f64::from(v) * 1_000_000.0).round() as i64
}

impl Vectors {
    /// An empty store. `tag` names the model, its revision and the chunker.
    pub fn new(tag: &str, dims: usize) -> Self {
        Self {
            tag: tag.to_string(),
            dims,
            pages: BTreeMap::new(),
            data: Vec::new(),
            owners: Vec::new(),
            free: Vec::new(),
        }
    }

    /// Live chunks.
    pub fn len(&self) -> usize {
        self.owners.len() - self.free.len()
    }

    /// Whether nothing is stored.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Which of these chunks need embedding, and forgets any the page no longer has.
    pub fn plan(&mut self, page: &str, chunks: &[Chunk]) -> Vec<Chunk> {
        let existing = self.pages.entry(page.to_string()).or_default();
        let wanted: std::collections::BTreeSet<&str> =
            chunks.iter().map(|c| c.key.as_str()).collect();
        let gone: Vec<String> = existing
            .keys()
            .filter(|k| !wanted.contains(k.as_str()))
            .cloned()
            .collect();
        for key in gone {
            if let Some(entry) = existing.remove(&key) {
                self.free.push(entry.at);
                if let Some(owner) = self.owners.get_mut(entry.at) {
                    *owner = (String::new(), String::new());
                }
            }
        }
        chunks
            .iter()
            .filter(|c| existing.get(&c.key).is_none_or(|e| e.hash != c.hash))
            .cloned()
            .collect()
    }

    /// Forgets a page, as when it is trashed or purged.
    pub fn remove_page(&mut self, page: &str) {
        if let Some(list) = self.pages.remove(page) {
            for entry in list.values() {
                self.free.push(entry.at);
                if let Some(owner) = self.owners.get_mut(entry.at) {
                    *owner = (String::new(), String::new());
                }
            }
        }
    }

    /// The pages this store has any vector for.
    pub fn page_ids(&self) -> Vec<String> {
        self.pages.keys().cloned().collect()
    }

    /// Stores one chunk's vector. Bad vectors are refused, so one cannot poison a search.
    pub fn submit(&mut self, page: &str, chunk: &Chunk, vector: &[f32]) -> Result<(), EmbedError> {
        if vector.len() != self.dims {
            return Err(EmbedError::BadDimensions {
                expected: self.dims,
                got: vector.len(),
            });
        }
        if vector.iter().any(|x| !x.is_finite()) {
            return Err(EmbedError::NotFinite);
        }
        let mut normal = vector.to_vec();
        normalise(&mut normal);
        let existing = self.pages.entry(page.to_string()).or_default();
        let at = match existing.get(&chunk.key) {
            Some(e) => e.at,
            None => match self.free.pop() {
                Some(at) => at,
                None => {
                    if self.owners.len() >= MAX_CHUNKS {
                        return Err(EmbedError::Full);
                    }
                    self.owners.push((String::new(), String::new()));
                    self.data.resize(self.data.len() + self.dims, 0.0);
                    self.owners.len() - 1
                }
            },
        };
        let start = at * self.dims;
        self.data[start..start + self.dims].copy_from_slice(&normal);
        self.owners[at] = (page.to_string(), chunk.key.clone());
        existing.insert(
            chunk.key.clone(),
            Entry {
                hash: chunk.hash.clone(),
                weight: chunk.weight,
                at,
            },
        );
        Ok(())
    }

    /// The nearest chunks to a query, brute force and exact.
    pub fn nearest(&self, query: &[f32], k: usize) -> Result<Vec<Near>, EmbedError> {
        if query.len() != self.dims {
            return Err(EmbedError::BadDimensions {
                expected: self.dims,
                got: query.len(),
            });
        }
        let mut heap: BinaryHeap<Scored> = BinaryHeap::with_capacity(k + 1);
        for (at, row) in self.data.chunks_exact(self.dims).enumerate() {
            if self.owners[at].0.is_empty() {
                continue;
            }
            let s = dot(query, row);
            if heap.len() < k {
                heap.push(Scored(s, at));
            } else if heap.peek().is_some_and(|m| s > m.0) {
                heap.pop();
                heap.push(Scored(s, at));
            }
        }
        let mut list = heap.into_vec();
        list.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| self.owners[a.1].cmp(&self.owners[b.1]))
        });
        Ok(list
            .into_iter()
            .map(|Scored(s, at)| Near {
                page: self.owners[at].0.clone(),
                key: self.owners[at].1.clone(),
                score: millionths(s),
            })
            .collect())
    }

    /// A page's vector: the length-weighted mean of its chunks, normalised.
    pub fn page_vector(&self, page: &str) -> Option<Vec<f32>> {
        let list = self.pages.get(page)?;
        if list.is_empty() {
            return None;
        }
        let mut v = vec![0.0f32; self.dims];
        for entry in list.values() {
            let row = &self.data[entry.at * self.dims..(entry.at + 1) * self.dims];
            let w = entry.weight.max(1) as f32;
            for (a, b) in v.iter_mut().zip(row) {
                *a += w * b;
            }
        }
        normalise(&mut v);
        Some(v)
    }

    /// Pages nearest a page, above `threshold`, best first, ties by id.
    pub fn related(&self, page: &str, k: usize, threshold: f32) -> Vec<Near> {
        let Some(me) = self.page_vector(page) else {
            return Vec::new();
        };
        let mut out: Vec<Near> = self
            .pages
            .keys()
            .filter(|id| id.as_str() != page)
            .filter_map(|id| {
                let s = dot(&me, &self.page_vector(id)?);
                (s >= threshold).then(|| Near {
                    page: id.clone(),
                    key: String::new(),
                    score: millionths(s),
                })
            })
            .collect();
        out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.page.cmp(&b.page)));
        out.truncate(k);
        out
    }

    /// The whole store, for the host to keep in derived storage. Never part of an export.
    pub fn dump(&self) -> Vec<u8> {
        let mut out = Vec::from(&MAGIC[..]);
        let put = |out: &mut Vec<u8>, s: &str| {
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        };
        put(&mut out, &self.tag);
        out.extend_from_slice(&(self.dims as u32).to_le_bytes());
        out.extend_from_slice(&(self.len() as u32).to_le_bytes());
        for (page, list) in &self.pages {
            for (key, entry) in list {
                put(&mut out, page);
                put(&mut out, key);
                put(&mut out, &entry.hash);
                out.extend_from_slice(&entry.weight.to_le_bytes());
                for x in &self.data[entry.at * self.dims..(entry.at + 1) * self.dims] {
                    out.extend_from_slice(&x.to_le_bytes());
                }
            }
        }
        out
    }

    /// Reads what `dump` wrote. Damage gives an error and never a panic, and the host rebuilds.
    pub fn load(tag: &str, dims: usize, bytes: &[u8]) -> Result<Self, EmbedError> {
        struct Reader<'a>(&'a [u8]);
        impl<'a> Reader<'a> {
            fn take(&mut self, n: usize) -> Result<&'a [u8], EmbedError> {
                if n > self.0.len() {
                    return Err(EmbedError::Damaged("truncated"));
                }
                let (a, b) = self.0.split_at(n);
                self.0 = b;
                Ok(a)
            }
            fn u32(&mut self) -> Result<u32, EmbedError> {
                let raw = self.take(4)?;
                Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
            }
            fn string(&mut self) -> Result<String, EmbedError> {
                let n = self.u32()? as usize;
                std::str::from_utf8(self.take(n)?)
                    .map(str::to_string)
                    .map_err(|_| EmbedError::Damaged("not text"))
            }
        }
        let mut r = Reader(bytes);
        if r.take(MAGIC.len())? != MAGIC {
            return Err(EmbedError::Damaged("not a vector file"));
        }
        if r.string()? != tag {
            return Err(EmbedError::WrongTag);
        }
        if r.u32()? as usize != dims {
            return Err(EmbedError::WrongTag);
        }
        let count = r.u32()? as usize;
        if count > MAX_CHUNKS {
            return Err(EmbedError::Damaged("too many chunks"));
        }
        let mut out = Self::new(tag, dims);
        let mut row = vec![0.0f32; dims];
        for _ in 0..count {
            let page = r.string()?;
            let key = r.string()?;
            let hash = r.string()?;
            let weight = r.u32()?;
            for x in &mut row {
                let raw = r.take(4)?;
                *x = f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            }
            out.submit(
                &page,
                &Chunk {
                    key,
                    input: String::new(),
                    hash,
                    weight,
                },
                &row,
            )?;
        }
        Ok(out)
    }
}

/// Scalar 8-bit quantisation, the form SPK-06 weighs against 32-bit.
pub fn quantise(v: &[f32]) -> Vec<i8> {
    v.iter()
        .map(|x| (x.clamp(-1.0, 1.0) * 127.0).round() as i8)
        .collect()
}

#[cfg(test)]
mod tests;
