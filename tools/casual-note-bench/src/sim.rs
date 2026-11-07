//! SPK-01: a seeded history from three devices, written through the real ops and merged through
//! the real import. Every edit goes through `casual_note_ops::apply`, as the app's do.

use crate::corpus::{self, LANGUAGES, Language};
use crate::rng::Rng;
use casual_note_merge::{SaveCursor, Workspace};
use casual_note_model::{
    Block, BlockKind, Column, Database, Ink, Mark, Page, Row, Run, Status, encode_samples,
};
use casual_note_ops::{Op, apply};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

/// Where the run is, for the report when something panics. Replays are exact, so this names the spot.
pub static STAGE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
pub static DAY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn stage(text: impl FnOnce() -> String) {
    if let Ok(mut s) = STAGE.lock() {
        *s = text();
    }
}

/// Time, where the platform has it. The wasm build reports state, not timings.
pub fn now_ms() -> f64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::OnceLock;
        use std::time::Instant;
        static START: OnceLock<Instant> = OnceLock::new();
        START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
    }
    #[cfg(target_arch = "wasm32")]
    {
        0.0
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub seed: u64,
    pub years: f64,
    pub pages: usize,
    pub sessions_per_device_year: usize,
    pub sync_days: usize,
    /// A full state hash every this many rounds. Touched pages are hashed every round.
    pub full_check_every: usize,
    pub long_blocks: usize,
    pub db_rows: usize,
    pub db_columns: usize,
    pub strokes: usize,
    /// Saves each device's state at the start of this day, to rebuild a failure in isolation.
    pub dump_day: Option<usize>,
    pub dump_prefix: Option<String>,
    /// Sync by updates since the last exchange (the durable layer's path) instead of by whole
    /// documents (the only path the app's `Engine.import` has).
    pub delta: bool,
}

impl Config {
    /// The workload named in docs/24.
    pub fn spec(seed: u64) -> Self {
        Self {
            seed,
            years: 5.0,
            pages: 500,
            sessions_per_device_year: 2000,
            sync_days: 7,
            full_check_every: 8,
            long_blocks: 5000,
            db_rows: 10_000,
            db_columns: 20,
            strokes: 5000,
            dump_day: None,
            dump_prefix: None,
            delta: false,
        }
    }
}

/// One replica.
pub struct Device {
    pub name: &'static str,
    pub ws: Workspace,
    pub peer: u64,
    pub opens: usize,
    backup: Option<Vec<u8>>,
    seen: BTreeMap<String, u64>,
    recent: Vec<String>,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Device({})", self.name)
    }
}

#[derive(Debug, Default, Clone)]
pub struct Counts {
    pub sessions: u64,
    pub ops: u64,
    pub keystrokes: u64,
    pub typed_bytes: u64,
    pub blocks_inserted: u64,
    pub splits: u64,
    pub merges: u64,
    pub moves: u64,
    pub deletes: u64,
    pub marks: u64,
    pub paste: u64,
    pub undo: u64,
    pub undo_refused: u64,
    pub page_moves: u64,
    pub trash: u64,
    pub duplicates: u64,
    pub rows: u64,
    pub cells: u64,
    pub strokes: u64,
    pub op_errors: u64,
    pub expected_refusals: u64,
    pub rounds: u64,
    pub reopens: u64,
    pub restores: u64,
    pub stale_imports: u64,
    pub import_errors: u64,
    pub hash_compares: u64,
    pub divergences: u64,
}

/// What a run produced, for the report.
#[derive(Debug, Default)]
pub struct Outcome {
    pub counts: Counts,
    pub first_errors: Vec<String>,
    pub divergent: Vec<String>,
    pub merge_after_offline_ms: Vec<(usize, &'static str, f64)>,
    pub yearly: Vec<serde_json::Value>,
    pub final_hash: u64,
    pub ms_edit: f64,
    pub ms_sync: f64,
    pub ms_check: f64,
    pub ms_reopen: f64,
}

fn hash64(v: impl Hash) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// The sections of an exported workspace, as `(name, body)`.
pub fn parts(bytes: &[u8]) -> Vec<(String, &[u8])> {
    let mut out = Vec::new();
    let Some(mut rest) = bytes.strip_prefix(&b"CNWS1"[..]) else {
        return out;
    };
    while !rest.is_empty() {
        let Some((head, tail)) = rest.split_first_chunk::<4>() else {
            break;
        };
        let n = u32::from_le_bytes(*head) as usize;
        if n > tail.len() {
            break;
        }
        let (name, tail) = tail.split_at(n);
        let Some((head, tail)) = tail.split_first_chunk::<8>() else {
            break;
        };
        let m = u64::from_le_bytes(*head) as usize;
        if m > tail.len() {
            break;
        }
        let (body, tail) = tail.split_at(m);
        out.push((String::from_utf8_lossy(name).into_owned(), body));
        rest = tail;
    }
    out
}

/// An exported workspace holding only these sections.
pub fn assemble<'a>(list: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Vec<u8> {
    let mut out = Vec::from(&b"CNWS1"[..]);
    for (name, body) in list {
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(body.len() as u64).to_le_bytes());
        out.extend_from_slice(body);
    }
    out
}

/// Hashes every document as read from merged state. Canonical JSON, not package JSON, of what the
/// app would show, so a difference here is a difference a person would see.
pub fn state_hashes(ws: &Workspace, only: Option<&BTreeSet<String>>) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    if only.is_none() {
        out.insert(
            "tree".to_string(),
            hash64(serde_json::to_string(&ws.pages()).unwrap_or_default()),
        );
        for id in ws.database_ids() {
            if let Ok(d) = ws.database(&id) {
                out.insert(
                    format!("d:{id}"),
                    hash64(serde_json::to_string(&d).unwrap_or_default()),
                );
            }
        }
    }
    let ids: Vec<String> = match only {
        Some(set) => set.iter().cloned().collect(),
        None => ws.pages().into_iter().map(|m| m.id).collect(),
    };
    for id in ids {
        if let Ok(p) = ws.page(&id) {
            out.insert(id, hash64(serde_json::to_string(&p).unwrap_or_default()));
        }
    }
    out
}

/// UTF-8 bytes of live text and compact stroke samples: the denominator in docs/24, strict reading.
pub fn live_bytes(ws: &Workspace) -> serde_json::Value {
    let (mut text, mut strokes, mut blocks) = (0u64, 0u64, 0u64);
    let mut sizes = Vec::new();
    for meta in ws.pages() {
        text += meta.title.len() as u64;
        if let Ok(p) = ws.page(&meta.id) {
            sizes.push(p.blocks.len());
            blocks += p.blocks.len() as u64;
            for b in &p.blocks {
                text += b.plain().len() as u64;
            }
            for item in &p.canvas.items {
                text += item.text.len() as u64;
            }
            for ink in &p.canvas.ink {
                strokes += encode_samples(&ink.points).map_or(0, |v| v.len() as u64);
            }
        }
    }
    for id in ws.database_ids() {
        if let Ok(d) = ws.database(&id) {
            text += d.name.len() as u64;
            for r in &d.rows {
                text += r.name.len() as u64;
                text += r.values.values().map(|v| v.len() as u64).sum::<u64>();
            }
        }
    }
    sizes.sort_unstable();
    let at = |f: f64| {
        sizes
            .get(((sizes.len() as f64 - 1.0) * f) as usize)
            .copied()
            .unwrap_or(0)
    };
    serde_json::json!({
        "text_bytes": text, "stroke_bytes": strokes, "live_bytes": text + strokes,
        "blocks": blocks, "pages": sizes.len(),
        "blocks_per_page": { "p10": at(0.1), "p50": at(0.5), "p90": at(0.9), "max": at(1.0) },
    })
}

struct Sim {
    /// What each device has already sent each other device, in `delta` mode.
    cursors: BTreeMap<(usize, usize), SaveCursor>,
    rng: Rng,
    cfg: Config,
    counts: Counts,
    errors: Vec<String>,
    next_id: u64,
    known_pages: Vec<String>,
    touched: BTreeSet<String>,
    long_done: usize,
    db_done: usize,
    ink_done: usize,
}

const LONG: &str = "long";
const INK: &str = "ink";
const DB: &str = "db";
const DB_PAGE: &str = "dbhost";

impl Sim {
    fn id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }

    fn op(&mut self, dev: &mut Device, op: &Op) -> Option<Op> {
        self.counts.ops += 1;
        match apply(&mut dev.ws, op) {
            Ok(inverse) => Some(inverse),
            Err(e) => {
                let text = format!("{e:?}");
                // A move into a page's own subtree is meant to be refused.
                if text.starts_with("BadMove") {
                    self.counts.expected_refusals += 1;
                } else {
                    self.counts.op_errors += 1;
                    if self.errors.len() < 12 {
                        self.errors.push(format!("{}: {text}", dev.name));
                    }
                }
                None
            }
        }
    }

    fn lang_of(page: &str) -> &'static Language {
        corpus::language_for(hash64(page))
    }

    /// Types `text` the way a person does: a keystroke or a commit at a time, with slips.
    fn type_into(
        &mut self,
        dev: &mut Device,
        page: &str,
        block: &str,
        at: usize,
        text: &str,
        lang: &Language,
    ) -> usize {
        let mut pos = at;
        for piece in corpus::edits(text, lang.input, &mut self.rng) {
            if self.rng.chance(3.0) && lang.input == corpus::Input::Keys {
                let slip = "x";
                self.op(
                    dev,
                    &Op::SpliceText {
                        page: page.into(),
                        block: block.into(),
                        at: pos,
                        delete: 0,
                        insert: slip.into(),
                    },
                );
                self.op(
                    dev,
                    &Op::SpliceText {
                        page: page.into(),
                        block: block.into(),
                        at: pos,
                        delete: 1,
                        insert: String::new(),
                    },
                );
                self.counts.keystrokes += 2;
            }
            let n = piece.encode_utf16().count();
            self.counts.keystrokes += 1;
            self.counts.typed_bytes += piece.len() as u64;
            self.op(
                dev,
                &Op::SpliceText {
                    page: page.into(),
                    block: block.into(),
                    at: pos,
                    delete: 0,
                    insert: piece,
                },
            );
            pos += n;
        }
        pos
    }

    fn pick_page(&mut self, dev: &Device) -> Option<String> {
        if self.known_pages.is_empty() {
            return None;
        }
        // Most work is on a device's recent pages, and some pages everybody touches.
        let r = self.rng.below(100);
        if r < 25 && !dev.recent.is_empty() {
            return Some(dev.recent[self.rng.below(dev.recent.len())].clone());
        }
        if r < 45 {
            let i = self.rng.skewed(self.known_pages.len());
            return Some(self.known_pages[i].clone());
        }
        Some(self.known_pages[self.rng.below(self.known_pages.len())].clone())
    }

    fn create_page(&mut self, dev: &mut Device) -> String {
        let id = self.id("p");
        let lang = Self::lang_of(&id);
        let title = corpus::rare_word(lang, &mut self.rng);
        // A parent has to be a page this device has heard of.
        let parent = if self.rng.chance(30.0) && !self.known_pages.is_empty() {
            Some(self.known_pages[self.rng.below(self.known_pages.len())].clone())
                .filter(|p| dev.ws.has_page(p))
        } else {
            None
        };
        self.op(
            dev,
            &Op::CreatePage {
                id: id.clone(),
                parent,
                title,
                database: None,
            },
        );
        self.known_pages.push(id.clone());
        id
    }

    fn session(&mut self, dev: &mut Device) {
        self.counts.sessions += 1;
        let (name, n) = (dev.name, self.counts.sessions);
        stage(|| format!("session {n} on {name}"));
        let page_id = if self.known_pages.len() < self.cfg.pages && self.rng.chance(12.0) {
            self.create_page(dev)
        } else {
            match self.pick_page(dev) {
                Some(p) => p,
                None => self.create_page(dev),
            }
        };
        // Pages created elsewhere and not yet delivered cannot be opened here.
        if !dev.ws.has_page(&page_id) {
            return;
        }
        self.touched.insert(page_id.clone());
        dev.recent.push(page_id.clone());
        if dev.recent.len() > 12 {
            dev.recent.remove(0);
        }
        let lang = Self::lang_of(&page_id);
        let Ok(mut page) = dev.ws.page(&page_id) else {
            return;
        };
        let mut undo: Vec<Op> = Vec::new();
        let actions = self.rng.range(2, 9);
        for _ in 0..actions {
            self.action(dev, &page_id, &mut page, lang, &mut undo);
        }
        if self.rng.chance(1.0) {
            self.page_level(dev, &page_id);
        }
        // Bulk structures fill over the years, as they would.
        self.grow(dev);
    }

    fn action(
        &mut self,
        dev: &mut Device,
        pid: &str,
        page: &mut Page,
        lang: &'static Language,
        undo: &mut Vec<Op>,
    ) {
        let roll = self.rng.below(1000);
        let n = page.blocks.len();
        let pick = |rng: &mut Rng| rng.below(n.max(1));
        match roll {
            // A new block, typed.
            0..=560 => {
                let after = if n == 0 {
                    None
                } else if self.rng.chance(70.0) {
                    page.blocks.last().map(|b| b.id.clone())
                } else {
                    Some(page.blocks[pick(&mut self.rng)].id.clone())
                };
                let id = self.id("b");
                let kind = match self.rng.below(20) {
                    0 => BlockKind::Heading2,
                    1 | 2 => BlockKind::Bulleted,
                    3 => BlockKind::Todo,
                    4 => BlockKind::Numbered,
                    _ => BlockKind::Paragraph,
                };
                let block = Block {
                    id: id.clone(),
                    kind,
                    checked: false,
                    runs: vec![],
                    target: None,
                };
                if self
                    .op(
                        dev,
                        &Op::InsertBlock {
                            page: pid.into(),
                            after: after.clone(),
                            block: block.clone(),
                        },
                    )
                    .is_some()
                {
                    self.counts.blocks_inserted += 1;
                    let text = corpus::line(lang, &mut self.rng);
                    self.type_into(dev, pid, &id, 0, &text, lang);
                    let mut b = block;
                    b.runs = vec![Run {
                        text,
                        marks: vec![],
                        link: None,
                    }];
                    let i = after
                        .and_then(|a| page.blocks.iter().position(|x| x.id == a))
                        .map_or(0, |i| i + 1);
                    page.blocks.insert(i, b);
                }
            }
            // A correction in existing text.
            561..=780 if n > 0 => {
                let i = pick(&mut self.rng);
                if !has_text(page.blocks[i].kind) {
                    return;
                }
                let id = page.blocks[i].id.clone();
                let plain = page.blocks[i].plain();
                let len = plain.encode_utf16().count();
                let at = self.rng.below(len + 1);
                let delete = if self.rng.chance(40.0) {
                    self.rng.below((len - at).min(12) + 1)
                } else {
                    0
                };
                let word = corpus::rare_word(lang, &mut self.rng);
                if delete > 0
                    && let Some(inv) = self.op(
                        dev,
                        &Op::SpliceText {
                            page: pid.into(),
                            block: id.clone(),
                            at,
                            delete,
                            insert: String::new(),
                        },
                    )
                {
                    undo.push(inv);
                }
                self.type_into(dev, pid, &id, at, &word, lang);
                if let Ok(p) = dev.ws.page(pid) {
                    *page = p;
                }
            }
            // Enter in the middle of a block, as the app does it.
            781..=830 if n > 0 => {
                let i = pick(&mut self.rng);
                let b = page.blocks[i].clone();
                if !has_text(b.kind) {
                    return;
                }
                let plain = b.plain();
                let units: Vec<u16> = plain.encode_utf16().collect();
                let at = self.rng.below(units.len() + 1);
                let tail = String::from_utf16_lossy(&units[at..]);
                self.op(
                    dev,
                    &Op::SpliceText {
                        page: pid.into(),
                        block: b.id.clone(),
                        at,
                        delete: units.len() - at,
                        insert: String::new(),
                    },
                );
                let id = self.id("b");
                let runs = if tail.is_empty() {
                    vec![]
                } else {
                    vec![Run {
                        text: tail,
                        marks: vec![],
                        link: None,
                    }]
                };
                self.op(
                    dev,
                    &Op::InsertBlock {
                        page: pid.into(),
                        after: Some(b.id),
                        block: Block {
                            id,
                            kind: BlockKind::Paragraph,
                            checked: false,
                            runs,
                            target: None,
                        },
                    },
                );
                self.counts.splits += 1;
                if let Ok(p) = dev.ws.page(pid) {
                    *page = p;
                }
            }
            // Backspace at the start of a block: merge into the one above.
            831..=860 if n > 1 => {
                let i = 1 + self.rng.below(n - 1);
                let (a, b) = (page.blocks[i - 1].clone(), page.blocks[i].clone());
                if !has_text(a.kind) || !has_text(b.kind) {
                    return;
                }
                let at = a.plain().encode_utf16().count();
                let text = b.plain();
                if !text.is_empty() {
                    self.op(
                        dev,
                        &Op::SpliceText {
                            page: pid.into(),
                            block: a.id.clone(),
                            at,
                            delete: 0,
                            insert: text,
                        },
                    );
                }
                self.op(
                    dev,
                    &Op::DeleteBlock {
                        page: pid.into(),
                        block: b.id,
                    },
                );
                self.counts.merges += 1;
                if let Ok(p) = dev.ws.page(pid) {
                    *page = p;
                }
            }
            // Reorder.
            861..=900 if n > 1 => {
                let (i, j) = (pick(&mut self.rng), pick(&mut self.rng));
                if i != j {
                    let block = page.blocks[i].id.clone();
                    let after = Some(page.blocks[j].id.clone());
                    if let Some(inv) = self.op(
                        dev,
                        &Op::MoveBlock {
                            page: pid.into(),
                            block,
                            after,
                        },
                    ) {
                        undo.push(inv);
                    }
                    self.counts.moves += 1;
                    if let Ok(p) = dev.ws.page(pid) {
                        *page = p;
                    }
                }
            }
            // Delete a block.
            901..=925 if n > 3 => {
                let i = pick(&mut self.rng);
                let block = page.blocks[i].id.clone();
                if let Some(inv) = self.op(
                    dev,
                    &Op::DeleteBlock {
                        page: pid.into(),
                        block,
                    },
                ) {
                    undo.push(inv);
                }
                self.counts.deletes += 1;
                page.blocks.remove(i);
            }
            // Formatting.
            926..=960 if n > 0 => {
                let i = pick(&mut self.rng);
                let len = page.blocks[i].plain().encode_utf16().count();
                if len > 2 && has_text(page.blocks[i].kind) {
                    let start = self.rng.below(len - 1);
                    let end = start + 1 + self.rng.below(len - start - 1);
                    let mark = Mark::ALL[self.rng.below(Mark::ALL.len())];
                    let block = page.blocks[i].id.clone();
                    let on = self.rng.chance(80.0);
                    self.op(
                        dev,
                        &Op::SetMark {
                            page: pid.into(),
                            block,
                            start,
                            end,
                            mark,
                            on,
                        },
                    );
                    self.counts.marks += 1;
                }
            }
            // Cut from one block and paste into another: one edit carries the whole text.
            961..=985 if n > 1 => {
                let (i, j) = (pick(&mut self.rng), pick(&mut self.rng));
                if i == j || !has_text(page.blocks[i].kind) || !has_text(page.blocks[j].kind) {
                    return;
                }
                let units: Vec<u16> = page.blocks[i].plain().encode_utf16().collect();
                if units.len() < 4 {
                    return;
                }
                let start = self.rng.below(units.len() - 2);
                let end = (start + 1 + self.rng.below(40)).min(units.len());
                let cut = String::from_utf16_lossy(&units[start..end]);
                let (a, b) = (page.blocks[i].id.clone(), page.blocks[j].id.clone());
                self.op(
                    dev,
                    &Op::SpliceText {
                        page: pid.into(),
                        block: a,
                        at: start,
                        delete: end - start,
                        insert: String::new(),
                    },
                );
                self.op(
                    dev,
                    &Op::SpliceText {
                        page: pid.into(),
                        block: b,
                        at: 0,
                        delete: 0,
                        insert: cut,
                    },
                );
                self.counts.paste += 1;
                if let Ok(p) = dev.ws.page(pid) {
                    *page = p;
                }
            }
            // Undo: an inverse, replayed through the same path.
            _ => {
                if let Some(inv) = undo.pop() {
                    self.counts.undo += 1;
                    if apply(&mut dev.ws, &inv).is_err() {
                        self.counts.undo_refused += 1;
                    }
                    if let Ok(p) = dev.ws.page(pid) {
                        *page = p;
                    }
                }
            }
        }
    }

    fn page_level(&mut self, dev: &mut Device, pid: &str) {
        match self.rng.below(5) {
            0 => {
                self.op(
                    dev,
                    &Op::SetTrashed {
                        id: pid.into(),
                        value: true,
                    },
                );
                self.op(
                    dev,
                    &Op::SetTrashed {
                        id: pid.into(),
                        value: false,
                    },
                );
                self.counts.trash += 1;
            }
            1 => {
                let parent = self.known_pages[self.rng.below(self.known_pages.len())].clone();
                if parent != pid && dev.ws.has_page(&parent) {
                    self.op(
                        dev,
                        &Op::MovePage {
                            id: pid.into(),
                            parent: Some(parent),
                        },
                    );
                    self.counts.page_moves += 1;
                }
            }
            2 => {
                let title = corpus::rare_word(Self::lang_of(pid), &mut self.rng);
                self.op(
                    dev,
                    &Op::SetTitle {
                        id: pid.into(),
                        title,
                    },
                );
            }
            3 => {
                let value = self.rng.chance(50.0);
                self.op(
                    dev,
                    &Op::SetFavorite {
                        id: pid.into(),
                        value,
                    },
                );
            }
            _ => {
                // Duplicate a page: read it, write a new one, as the app's menu does.
                if let Ok(src) = dev.ws.page(pid) {
                    let id = self.id("p");
                    self.op(
                        dev,
                        &Op::CreatePage {
                            id: id.clone(),
                            parent: src.meta.parent.clone(),
                            title: src.meta.title.clone(),
                            database: None,
                        },
                    );
                    let mut after = None;
                    for b in src.blocks.iter().take(40) {
                        let nid = self.id("b");
                        let mut copy = b.clone();
                        copy.id = nid.clone();
                        self.op(
                            dev,
                            &Op::InsertBlock {
                                page: id.clone(),
                                after: after.clone(),
                                block: copy,
                            },
                        );
                        after = Some(nid);
                    }
                    self.known_pages.push(id);
                    self.counts.duplicates += 1;
                }
            }
        }
    }

    /// One long page, one big database and one drawn canvas grow by small bursts over the years.
    fn grow(&mut self, dev: &mut Device) {
        if !self.rng.chance(4.0) {
            return;
        }
        let lang = &LANGUAGES[self.rng.below(LANGUAGES.len())];
        if self.long_done < self.cfg.long_blocks && dev.ws.has_page(LONG) {
            let mut after = None;
            if let Ok(p) = dev.ws.page(LONG) {
                after = p.blocks.last().map(|b| b.id.clone());
            }
            for _ in 0..self.bulk(self.cfg.long_blocks) {
                let id = self.id("b");
                let text = corpus::line(lang, &mut self.rng);
                self.counts.typed_bytes += text.len() as u64;
                let block = Block {
                    id: id.clone(),
                    kind: BlockKind::Paragraph,
                    checked: false,
                    runs: vec![Run {
                        text,
                        marks: vec![],
                        link: None,
                    }],
                    target: None,
                };
                self.op(
                    dev,
                    &Op::InsertBlock {
                        page: LONG.into(),
                        after,
                        block,
                    },
                );
                after = Some(id);
                self.long_done += 1;
            }
            self.touched.insert(LONG.into());
        }
        if self.db_done < self.cfg.db_rows && dev.ws.database_ids().iter().any(|d| d == DB) {
            for _ in 0..self.bulk(self.cfg.db_rows) {
                let id = self.id("r");
                let name = corpus::line(lang, &mut self.rng);
                self.counts.typed_bytes += name.len() as u64;
                let mut values = BTreeMap::new();
                for c in 0..self.cfg.db_columns {
                    let v = corpus::rare_word(lang, &mut self.rng);
                    self.counts.typed_bytes += v.len() as u64;
                    values.insert(format!("c{c}"), v);
                    self.counts.cells += 1;
                }
                self.op(
                    dev,
                    &Op::AddRow {
                        id: DB.into(),
                        after: None,
                        row: Row {
                            id,
                            name,
                            status: "s1".into(),
                            values,
                        },
                    },
                );
                self.counts.rows += 1;
                self.db_done += 1;
            }
            self.touched.insert(DB_PAGE.into());
        }
        if self.ink_done < self.cfg.strokes && dev.ws.has_page(INK) {
            let mut add = Vec::new();
            for _ in 0..self.bulk(self.cfg.strokes) {
                let id = self.id("s");
                let (mut x, mut y) = (self.rng.below(2000) as f64, self.rng.below(2000) as f64);
                let n = self.rng.range(20, 120);
                let mut points = Vec::with_capacity(n);
                for _ in 0..n {
                    x += self.rng.below(7) as f64 - 3.0;
                    y += self.rng.below(7) as f64 - 3.0;
                    points.push([x, y, 0.5]);
                }
                add.push(Ink {
                    id,
                    kind: "pen".into(),
                    color: "#222222".into(),
                    width: 2.0,
                    points,
                });
                self.counts.strokes += 1;
                self.ink_done += 1;
            }
            self.op(
                dev,
                &Op::ChangeInk {
                    page: INK.into(),
                    add,
                    remove: vec![],
                },
            );
            self.touched.insert(INK.into());
        }
    }

    /// How much one burst adds, so each structure fills by about the third year.
    fn bulk(&mut self, target: usize) -> usize {
        let total_sessions = self.cfg.sessions_per_device_year as f64 * 3.0 * 3.0 * 0.04;
        ((target as f64 / total_sessions).ceil() as usize).max(1)
    }

    /// The fixtures one device creates on day one.
    fn seed_structures(&mut self, dev: &mut Device) {
        self.op(
            dev,
            &Op::CreatePage {
                id: LONG.into(),
                parent: None,
                title: "Long page".into(),
                database: None,
            },
        );
        self.op(
            dev,
            &Op::CreatePage {
                id: INK.into(),
                parent: None,
                title: "Sketchbook".into(),
                database: None,
            },
        );
        let columns = (0..self.cfg.db_columns)
            .map(|c| Column {
                id: format!("c{c}"),
                name: format!("Column {c}"),
                kind: "text".into(),
                hidden: false,
            })
            .collect();
        let database = Database {
            id: DB.into(),
            name: "Tracker".into(),
            view: "table".into(),
            statuses: vec![Status {
                id: "s1".into(),
                name: "Open".into(),
                group: "todo".into(),
            }],
            columns,
            rows: vec![],
        };
        self.op(dev, &Op::CreateDatabase { database });
        self.op(
            dev,
            &Op::CreatePage {
                id: DB_PAGE.into(),
                parent: None,
                title: "Tracker".into(),
                database: Some(DB.into()),
            },
        );
    }
}

fn has_text(kind: BlockKind) -> bool {
    !matches!(
        kind,
        BlockKind::Divider | BlockKind::Database | BlockKind::Embed
    )
}

fn fresh_peer(rng: &mut Rng) -> u64 {
    // Loro keeps the top values of the peer space for itself.
    rng.next_u64() >> 2
}

/// Hands one device's document changes to another, with sections shuffled, as delivery can be.
fn deliver(rng: &mut Rng, from_bytes: &[u8], to: &mut Device) -> Result<(), String> {
    let mut list: Vec<(&str, &[u8])> = Vec::new();
    let all = parts(from_bytes);
    for (name, body) in &all {
        let h = hash64(body);
        if to.seen.get(name) != Some(&h) {
            to.seen.insert(name.clone(), h);
            list.push((name, body));
        }
    }
    for i in (1..list.len()).rev() {
        list.swap(i, rng.below(i + 1));
    }
    // One section at a time, so a failure names the document.
    for (name, body) in list {
        stage(|| {
            format!(
                "deliver to {} (peer {:x}), document {name:?}, {} bytes",
                to.name,
                to.peer,
                body.len()
            )
        });
        to.ws
            .import(&assemble([(name, body)]))
            .map_err(|e| format!("{e:?}"))?;
    }
    Ok(())
}

/// Sends what `to` has not had from `from`, one document's update at a time, shuffled.
fn deliver_delta(
    sim: &mut Sim,
    devices: &mut [Device],
    from: usize,
    to: usize,
) -> Result<usize, String> {
    let cursor = sim.cursors.entry((from, to)).or_default();
    let mut updates = devices[from]
        .ws
        .changed_docs(cursor)
        .map_err(|e| format!("{e:?}"))?;
    for i in (1..updates.len()).rev() {
        updates.swap(i, sim.rng.below(i + 1));
    }
    let mut bytes = 0;
    for u in &updates {
        bytes += u.update.len();
        let (name, peer, target, n) = (
            u.name.clone(),
            devices[to].peer,
            devices[to].name,
            u.update.len(),
        );
        stage(|| format!("update to {target} (peer {peer:x}), document {name:?}, {n} bytes"));
        devices[to]
            .ws
            .import_doc(&u.name, &u.update)
            .map_err(|e| format!("{e:?}"))?;
    }
    Ok(bytes)
}

fn offline_windows(total_days: usize) -> [(usize, &'static [(f64, usize)]); 3] {
    let _ = total_days;
    [
        // Desktop: one long absence, in the fourth year.
        (0, &[(0.70, 90)]),
        // Laptop: 14 days inside the tablet's window, so three-way merges occur.
        (1, &[(0.262, 14), (0.60, 14)]),
        // Tablet: two 90-day absences, editing pages the others edit.
        (2, &[(0.22, 90), (0.56, 90)]),
    ]
}

fn offline(dev: usize, day: usize, total: usize) -> bool {
    offline_windows(total)[dev].1.iter().any(|(start, len)| {
        let s = (*start * total as f64) as usize;
        day >= s && day < s + len
    })
}

/// Runs the whole history. `probe` reports memory where the host can measure it.
pub fn run(cfg: Config, probe: &mut dyn FnMut() -> u64) -> (Vec<Device>, Outcome) {
    let mut sim = Sim {
        cursors: BTreeMap::new(),
        rng: Rng::new(cfg.seed),
        cfg: cfg.clone(),
        counts: Counts::default(),
        errors: Vec::new(),
        next_id: 0,
        known_pages: Vec::new(),
        touched: BTreeSet::new(),
        long_done: 0,
        db_done: 0,
        ink_done: 0,
    };
    let mut out = Outcome::default();
    let mut devices: Vec<Device> = ["desktop", "laptop", "tablet"]
        .iter()
        .map(|name| {
            let peer = fresh_peer(&mut sim.rng);
            Device {
                name,
                ws: Workspace::new(peer).expect("a new workspace"),
                peer,
                opens: 1,
                backup: None,
                seen: BTreeMap::new(),
                recent: Vec::new(),
            }
        })
        .collect();
    sim.seed_structures(&mut devices[0]);
    let start = devices[0].ws.export().expect("export");
    for d in &mut devices[1..] {
        d.ws.import(&start).expect("import");
    }

    let total_days = (cfg.years * 365.0) as usize;
    let per_day = cfg.sessions_per_device_year as f64 / 365.0;
    let mut stale: Vec<Vec<u8>> = Vec::new();
    let mut was_offline = [false; 3];
    let mut round = 0usize;

    for day in 0..total_days {
        DAY.store(day, std::sync::atomic::Ordering::Relaxed);
        if cfg.dump_day == Some(day) {
            for d in &devices {
                let bytes = d.ws.export().expect("export");
                let _ = (d.name, &bytes);
                if let Some(path) = &cfg.dump_prefix {
                    #[cfg(not(target_arch = "wasm32"))]
                    std::fs::write(format!("{path}-{}-{:x}.cnws", d.name, d.peer), &bytes)
                        .expect("dump");
                    let _ = path;
                }
            }
        }
        // Edit.
        stage(|| format!("day {day}, editing"));
        let t = now_ms();
        for i in 0..3 {
            // The laptop is used on weekdays; the desktop most days; the tablet in bursts.
            let weight = match i {
                0 => 1.0,
                1 if day % 7 >= 5 => 0.2,
                1 => 1.28,
                _ => 0.9 + 0.2 * ((day / 30) % 2) as f64,
            };
            let mut n = (per_day * weight) as usize;
            if sim.rng.chance((per_day * weight).fract() * 100.0) {
                n += 1;
            }
            for _ in 0..n {
                sim.session(&mut devices[i]);
            }
        }
        out.ms_edit += now_ms() - t;

        // Sync, through the only sync the engine has today: export and import of whole documents.
        if day % cfg.sync_days == 0 {
            let t = now_ms();
            sync_round(
                &mut sim,
                &mut devices,
                day,
                total_days,
                &mut stale,
                &mut was_offline,
                &mut out,
                round,
            );
            out.ms_sync += now_ms() - t;
            round += 1;
        }

        if day > 0 && day % 365 == 0 {
            let t = now_ms();
            let bytes = devices[0].ws.export().expect("export");
            let export_ms = now_ms() - t;
            let t = now_ms();
            let mut fresh = Workspace::new(fresh_peer(&mut sim.rng)).expect("new");
            fresh.import(&bytes).expect("import");
            let boot_ms = now_ms() - t;
            drop(fresh);
            out.yearly.push(serde_json::json!({
                "year": day / 365, "bytes": bytes.len(), "export_ms": export_ms, "boot_import_ms": boot_ms,
                "rss_kb": probe(), "keystrokes": sim.counts.keystrokes,
                "live": live_bytes(&devices[0].ws),
            }));
        }
    }

    // A last full exchange, everyone online, then the deciding comparison.
    let t = now_ms();
    sync_round(
        &mut sim,
        &mut devices,
        total_days,
        total_days * 2,
        &mut stale,
        &mut was_offline,
        &mut out,
        0,
    );
    out.ms_sync += now_ms() - t;
    out.counts = sim.counts;
    out.first_errors = sim.errors;
    // Through text, because hashing a map feeds in its length as usize, which differs on wasm32.
    let portable: String = state_hashes(&devices[0].ws, None)
        .iter()
        .map(|(k, v)| format!("{k}={v:x};"))
        .collect();
    out.final_hash = hash64(portable.as_str());
    (devices, out)
}

#[allow(clippy::too_many_arguments)]
fn sync_round(
    sim: &mut Sim,
    devices: &mut [Device],
    day: usize,
    total: usize,
    stale: &mut Vec<Vec<u8>>,
    was_offline: &mut [bool; 3],
    out: &mut Outcome,
    round: usize,
) {
    sim.counts.rounds += 1;
    stage(|| format!("day {day}, sync round begins"));
    let online: Vec<usize> = (0..3).filter(|i| !offline(*i, day, total)).collect();
    let Some(&hub) = online.first() else {
        return;
    };
    // Spokes to the hub, in a shuffled order.
    let mut spokes: Vec<usize> = online.iter().copied().filter(|i| *i != hub).collect();
    if spokes.len() == 2 && sim.rng.chance(50.0) {
        spokes.swap(0, 1);
    }
    if sim.cfg.delta {
        for &s in &spokes {
            let t = now_ms();
            let r = deliver_delta(sim, devices, s, hub);
            if was_offline[s] {
                out.merge_after_offline_ms
                    .push((day, devices[s].name, now_ms() - t));
            }
            if let Err(e) = r {
                sim.counts.import_errors += 1;
                sim.errors.push(format!("import at day {day}: {e}"));
            }
        }
        for &s in &spokes {
            let t = now_ms();
            let r = deliver_delta(sim, devices, hub, s);
            if was_offline[s] {
                out.merge_after_offline_ms
                    .push((day, devices[s].name, now_ms() - t));
            }
            if let Err(e) = r {
                sim.counts.import_errors += 1;
                sim.errors.push(format!("import at day {day}: {e}"));
            }
        }
    } else {
        for &s in &spokes {
            let returning = was_offline[s];
            stage(|| format!("day {day}, export from {}", devices[s].name));
            let bytes = devices[s].ws.export().expect("export");
            let t = now_ms();
            let r = deliver(&mut sim.rng, &bytes, &mut devices[hub]);
            if returning {
                out.merge_after_offline_ms
                    .push((day, devices[s].name, now_ms() - t));
            }
            if let Err(e) = r {
                sim.counts.import_errors += 1;
                sim.errors.push(format!("import at day {day}: {e}"));
            }
            if sim.rng.chance(8.0) && stale.len() < 4 {
                stale.push(bytes);
            }
        }
        stage(|| format!("day {day}, export from hub {}", devices[hub].name));
        let bytes = devices[hub].ws.export().expect("export");
        for &s in &spokes {
            let t = now_ms();
            let r = deliver(&mut sim.rng, &bytes, &mut devices[s]);
            if was_offline[s] {
                out.merge_after_offline_ms
                    .push((day, devices[s].name, now_ms() - t));
            }
            if let Err(e) = r {
                sim.counts.import_errors += 1;
                sim.errors.push(format!("import at day {day}: {e}"));
            }
        }
    }
    // A delayed, duplicate delivery: an old batch arrives again, on a few documents.
    if sim.rng.chance(10.0) && !stale.is_empty() {
        let old = stale.remove(sim.rng.below(stale.len()));
        let target = online[sim.rng.below(online.len())];
        let all = parts(&old);
        let mut some: Vec<(&str, &[u8])> = Vec::new();
        for _ in 0..12 {
            if !all.is_empty() {
                let (n, b) = &all[sim.rng.below(all.len())];
                some.push((n, b));
            }
        }
        sim.counts.stale_imports += 1;
        stage(|| format!("day {day}, stale import to {}", devices[target].name));
        if let Err(e) = devices[target].ws.import(&assemble(some)) {
            sim.counts.import_errors += 1;
            sim.errors.push(format!("stale import at day {day}: {e:?}"));
        }
    }
    for i in 0..3 {
        was_offline[i] = !online.contains(&i);
    }

    // State comparison among whoever just exchanged.
    stage(|| format!("day {day}, comparing"));
    let t = now_ms();
    let full = round.is_multiple_of(sim.cfg.full_check_every);
    let touched = std::mem::take(&mut sim.touched);
    let filter = if full { None } else { Some(&touched) };
    let base = state_hashes(&devices[hub].ws, filter);
    for &s in &spokes {
        let other = state_hashes(&devices[s].ws, filter);
        sim.counts.hash_compares += 1;
        if other != base {
            sim.counts.divergences += 1;
            for (k, v) in &base {
                if other.get(k) != Some(v) && out.divergent.len() < 10 {
                    out.divergent.push(format!(
                        "day {day}, {} against {}: {k}",
                        devices[s].name, devices[hub].name
                    ));
                }
            }
        }
    }
    out.ms_check += now_ms() - t;

    // Reopening mints a fresh peer (ADR-030), from what is stored. A restored backup is the same
    // but older, so what the device had since then comes back from the others.
    stage(|| format!("day {day}, reopening"));
    let t = now_ms();
    for &i in &online {
        if sim.rng.chance(25.0) {
            let restoring = sim.rng.chance(2.0) && devices[i].backup.is_some();
            let bytes = if restoring {
                sim.counts.restores += 1;
                devices[i].backup.clone().unwrap_or_default()
            } else {
                devices[i].ws.export().expect("export")
            };
            let peer = fresh_peer(&mut sim.rng);
            let mut ws = Workspace::new(peer).expect("new");
            if let Err(e) = ws.import(&bytes) {
                sim.counts.import_errors += 1;
                sim.errors.push(format!("reopen at day {day}: {e:?}"));
                continue;
            }
            devices[i].ws = ws;
            devices[i].peer = peer;
            devices[i].opens += 1;
            devices[i].seen.clear();
            if devices[i].backup.is_none() || sim.rng.chance(10.0) {
                devices[i].backup = Some(bytes);
            }
            sim.counts.reopens += 1;
            if restoring {
                // What was sent there, and what it sent, are no longer true of the older copy.
                sim.cursors.retain(|(a, b), _| *a != i && *b != i);
            }
        }
    }
    out.ms_reopen += now_ms() - t;
}
