//! Exact search: an inverted index with BM25 over pages, blocks and database rows. It reads the
//! model and never writes to it, and it is rebuildable at any time. See docs/60.

mod tokenise;

use casual_note_merge::Workspace;
use casual_note_model::{BlockKind, Database, Page};
use casual_note_ops::Op;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub use tokenise::{
    MAX_TEXT_BYTES, TOKENISER_VERSION, Token, fold, query_terms, stable_ln, tokenise,
};

const K1: f64 = 1.2;
const B: f64 = 0.75;
const MAX_PREFIX_TERMS: usize = 64;
const EXCERPT_CHARS: usize = 120;
const EXCERPT_LEAD: usize = 30;
/// The most a search returns, whatever is asked for.
pub const MAX_RESULTS: usize = 200;

/// Where a piece of text sits, which sets its weight. Starting values from docs/60, not tuned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Field {
    Title,
    Heading,
    Body,
    Row,
    Cell,
    Canvas,
}

impl Field {
    fn weight(self) -> f64 {
        match self {
            Self::Title => 4.0,
            Self::Heading | Self::Row => 2.0,
            Self::Body | Self::Cell | Self::Canvas => 1.0,
        }
    }
}

/// What owns a group of documents. A change replaces the whole group.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Owner {
    Page(String),
    Database(String),
}

/// One piece of text with an edit identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doc {
    /// The block, canvas item or row this text belongs to. Empty for a page title.
    pub item: String,
    pub field: Field,
    pub text: String,
}

/// One result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Hit {
    /// `page` or `database`.
    pub owner: &'static str,
    pub id: String,
    /// The block, canvas item or row, when the match is not the page title.
    pub item: String,
    pub field: Field,
    /// BM25 score in millionths, so results compare exactly across platforms.
    pub score: i64,
    /// The matching stretch of text, exactly as stored.
    pub text: String,
    /// Ranges of `text` that matched, in UTF-16 units.
    pub marks: Vec<[u32; 2]>,
    /// Whether `text` starts or ends part way through the original.
    pub cut_start: bool,
    pub cut_end: bool,
}

#[derive(Debug)]
struct Stored {
    owner: Owner,
    seq: usize,
    item: String,
    field: Field,
    text: String,
    length: u32,
    terms: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Posting {
    primary: u32,
    secondary: u32,
}

/// The index. Its results depend only on what is in it, never on the order it got there.
#[derive(Debug, Default)]
pub struct Index {
    docs: BTreeMap<u64, Stored>,
    by_owner: BTreeMap<Owner, Vec<u64>>,
    postings: BTreeMap<String, BTreeMap<u64, Posting>>,
    total_length: u64,
    next: u64,
}

impl Index {
    /// An empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many documents are indexed.
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// Whether nothing is indexed.
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Removes everything an owner contributed.
    pub fn remove(&mut self, owner: &Owner) {
        for id in self.by_owner.remove(owner).unwrap_or_default() {
            let Some(doc) = self.docs.remove(&id) else {
                continue;
            };
            self.total_length = self.total_length.saturating_sub(u64::from(doc.length));
            for term in &doc.terms {
                if let Some(list) = self.postings.get_mut(term) {
                    list.remove(&id);
                    if list.is_empty() {
                        self.postings.remove(term);
                    }
                }
            }
        }
    }

    /// Replaces what an owner contributes. Only this owner's text is tokenised again.
    pub fn update(&mut self, owner: &Owner, docs: Vec<Doc>) {
        self.remove(owner);
        let mut ids = Vec::with_capacity(docs.len());
        for (seq, doc) in docs.into_iter().enumerate() {
            let tokens = tokenise(&doc.text);
            if tokens.is_empty() {
                continue;
            }
            let id = self.next;
            self.next = self.next.saturating_add(1);
            let mut counts: BTreeMap<String, Posting> = BTreeMap::new();
            let mut length = 0u32;
            for token in tokens {
                let entry = counts.entry(token.term).or_default();
                if token.primary {
                    entry.primary = entry.primary.saturating_add(1);
                    length = length.saturating_add(1);
                } else {
                    entry.secondary = entry.secondary.saturating_add(1);
                }
            }
            let terms: Vec<String> = counts.keys().cloned().collect();
            for (term, posting) in counts {
                self.postings.entry(term).or_default().insert(id, posting);
            }
            self.total_length = self.total_length.saturating_add(u64::from(length));
            self.docs.insert(
                id,
                Stored {
                    owner: owner.clone(),
                    seq,
                    item: doc.item,
                    field: doc.field,
                    text: doc.text,
                    length,
                    terms,
                },
            );
            ids.push(id);
        }
        if !ids.is_empty() {
            self.by_owner.insert(owner.clone(), ids);
        }
    }

    fn average_length(&self) -> f64 {
        if self.docs.is_empty() {
            return 1.0;
        }
        (self.total_length as f64 / self.docs.len() as f64).max(1.0)
    }

    /// The best score for one query term, per document. The last term also matches as a prefix.
    fn term_scores(
        &self,
        term: &str,
        prefix: bool,
        matched: &mut BTreeSet<String>,
    ) -> BTreeMap<u64, f64> {
        let total = self.docs.len() as f64;
        let average = self.average_length();
        let mut best = BTreeMap::new();
        let mut consider = |key: &str, list: &BTreeMap<u64, Posting>| {
            let df = list.len() as f64;
            let idf = stable_ln(1.0 + (total - df + 0.5) / (df + 0.5));
            for (id, posting) in list {
                let Some(doc) = self.docs.get(id) else {
                    continue;
                };
                let tf = f64::from(posting.primary) + 0.5 * f64::from(posting.secondary);
                let norm = 1.0 - B + B * f64::from(doc.length) / average;
                let score = doc.field.weight() * idf * tf * (K1 + 1.0) / (tf + K1 * norm);
                let slot: &mut f64 = best.entry(*id).or_insert(0.0);
                if score > *slot {
                    *slot = score;
                }
            }
            matched.insert(key.to_string());
        };
        if let Some(list) = self.postings.get(term) {
            consider(term, list);
        }
        if prefix {
            let range = self
                .postings
                .range::<str, _>((std::ops::Bound::Excluded(term), std::ops::Bound::Unbounded));
            for (key, list) in range
                .take_while(|(k, _)| k.starts_with(term))
                .take(MAX_PREFIX_TERMS)
            {
                consider(key, list);
            }
        }
        best
    }

    fn sort_key(&self, id: u64) -> Option<(&Owner, usize)> {
        self.docs.get(&id).map(|d| (&d.owner, d.seq))
    }

    /// Searches. Every term must appear in one piece of text, or failing that, somewhere on one page.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let terms = query_terms(query);
        if terms.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut matched = BTreeSet::new();
        let last = terms.len() - 1;
        let per_term: Vec<BTreeMap<u64, f64>> = terms
            .iter()
            .enumerate()
            .map(|(i, t)| self.term_scores(t, i == last, &mut matched))
            .collect();

        let mut scored: Vec<(i64, u64)> = Vec::new();
        if let Some((first, rest)) = per_term.split_first() {
            for (id, score) in first {
                let mut sum = *score;
                let all = rest.iter().all(|m| m.get(id).map(|s| sum += *s).is_some());
                if all {
                    scored.push(((sum * 1e6).round() as i64, *id));
                }
            }
        }
        if scored.is_empty() && per_term.len() > 1 {
            scored = self.across_one_owner(&per_term);
        }
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| self.sort_key(a.1).cmp(&self.sort_key(b.1)))
        });
        scored.truncate(limit.min(MAX_RESULTS));
        scored
            .into_iter()
            .filter_map(|(score, id)| self.hit(id, score, &matched))
            .collect()
    }

    /// The terms were spread across one page's blocks: show that page's strongest block.
    fn across_one_owner(&self, per_term: &[BTreeMap<u64, f64>]) -> Vec<(i64, u64)> {
        let mut owners: BTreeMap<&Owner, (usize, f64, u64, f64)> = BTreeMap::new();
        for (term_index, scores) in per_term.iter().enumerate() {
            let mut seen = BTreeSet::new();
            for (id, score) in scores {
                let Some(doc) = self.docs.get(id) else {
                    continue;
                };
                let slot = owners.entry(&doc.owner).or_insert((0, 0.0, *id, 0.0));
                if seen.insert(&doc.owner) && slot.0 == term_index {
                    slot.0 += 1;
                    slot.1 += *score;
                } else if !seen.contains(&doc.owner) {
                    continue;
                }
                if *score > slot.3 {
                    slot.2 = *id;
                    slot.3 = *score;
                }
            }
        }
        owners
            .into_values()
            .filter(|o| o.0 == per_term.len())
            .map(|o| ((o.1 * 1e6).round() as i64, o.2))
            .collect()
    }

    fn hit(&self, id: u64, score: i64, matched: &BTreeSet<String>) -> Option<Hit> {
        let doc = self.docs.get(&id)?;
        let (owner, owner_id) = match &doc.owner {
            Owner::Page(p) => ("page", p.clone()),
            Owner::Database(d) => ("database", d.clone()),
        };
        let (text, marks, cut_start, cut_end) = excerpt(&doc.text, matched);
        Some(Hit {
            owner,
            id: owner_id,
            item: doc.item.clone(),
            field: doc.field,
            score,
            text,
            marks,
            cut_start,
            cut_end,
        })
    }
}

fn excerpt(text: &str, matched: &BTreeSet<String>) -> (String, Vec<[u32; 2]>, bool, bool) {
    let mut ranges: Vec<(usize, usize)> = tokenise(text)
        .into_iter()
        .filter(|t| matched.contains(&t.term))
        .map(|t| (t.start, t.end))
        .collect();
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    let focus = merged.first().map_or(0, |r| r.0);
    let lead: Vec<(usize, char)> = text
        .get(..focus)
        .unwrap_or("")
        .char_indices()
        .rev()
        .take(EXCERPT_LEAD)
        .collect();
    let begin = if lead.len() < EXCERPT_LEAD {
        0
    } else {
        lead.last().map_or(0, |c| c.0)
    };
    let tail = text.get(begin..).unwrap_or("");
    let end = tail
        .char_indices()
        .nth(EXCERPT_CHARS)
        .map_or(text.len(), |(i, _)| begin + i);
    let piece = text.get(begin..end).unwrap_or("").to_string();
    let units = |from: usize, to: usize| -> u32 {
        let n: usize = text.get(from..to).unwrap_or("").encode_utf16().count();
        u32::try_from(n).unwrap_or(u32::MAX)
    };
    let marks = merged
        .into_iter()
        .filter(|r| r.0 >= begin && r.1 <= end)
        .map(|(s, e)| [units(begin, s), units(begin, e)])
        .collect();
    (piece, marks, begin > 0, end < text.len())
}

/// The search documents one page contributes.
pub fn page_docs(page: &Page) -> Vec<Doc> {
    let mut out = vec![Doc {
        item: String::new(),
        field: Field::Title,
        text: page.meta.title.clone(),
    }];
    for block in &page.blocks {
        let field = match block.kind {
            BlockKind::Heading1 | BlockKind::Heading2 | BlockKind::Heading3 => Field::Heading,
            _ => Field::Body,
        };
        out.push(Doc {
            item: block.id.clone(),
            field,
            text: block.plain(),
        });
    }
    for item in &page.canvas.items {
        out.push(Doc {
            item: item.id.clone(),
            field: Field::Canvas,
            text: item.text.clone(),
        });
    }
    out
}

/// The search documents one database contributes. A removed column stays out of results.
pub fn database_docs(database: &Database) -> Vec<Doc> {
    let mut out = Vec::new();
    for row in &database.rows {
        out.push(Doc {
            item: row.id.clone(),
            field: Field::Row,
            text: row.name.clone(),
        });
        for column in database.columns.iter().filter(|c| !c.hidden) {
            if let Some(value) = row.values.get(&column.id) {
                out.push(Doc {
                    item: row.id.clone(),
                    field: Field::Cell,
                    text: value.clone(),
                });
            }
        }
    }
    out
}

/// An index that follows a workspace. Edits mark what changed, and the next search catches up,
/// so typing pays only for a set insert.
#[derive(Debug, Default)]
pub struct WorkspaceIndex {
    index: Index,
    pages: BTreeSet<String>,
    databases: BTreeSet<String>,
    everything: bool,
    built: bool,
    hosted_by_page: BTreeMap<String, Vec<String>>,
}

impl WorkspaceIndex {
    /// An index that fills itself on the first search.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks what an edit touched.
    pub fn touch(&mut self, op: &Op) {
        match op {
            Op::CreatePage { id, .. } | Op::SetTitle { id, .. } | Op::SetTrashed { id, .. } => {
                self.pages.insert(id.clone());
            }
            Op::InsertBlock { page, .. }
            | Op::DeleteBlock { page, .. }
            | Op::SpliceText { page, .. }
            | Op::SetBlock { page, .. }
            | Op::PutItem { page, .. }
            | Op::DeleteItem { page, .. } => {
                self.pages.insert(page.clone());
            }
            Op::CreateDatabase { database } => {
                self.databases.insert(database.id.clone());
            }
            Op::AddColumn { id, .. }
            | Op::SetColumnHidden { id, .. }
            | Op::AddRow { id, .. }
            | Op::DeleteRow { id, .. }
            | Op::SetRowField { id, .. }
            | Op::SetCell { id, .. } => {
                self.databases.insert(id.clone());
            }
            Op::SetFavorite { .. }
            | Op::MovePage { .. }
            | Op::MoveBlock { .. }
            | Op::SetMark { .. }
            | Op::SetLink { .. }
            | Op::SetView { .. }
            | Op::SetNote { .. }
            | Op::ChangeInk { .. }
            | Op::SetDatabaseField { .. } => {}
            #[allow(unreachable_patterns)]
            _ => self.everything = true,
        }
    }

    /// Everything may have changed, as after merging another device.
    pub fn touch_all(&mut self) {
        self.everything = true;
    }

    fn refresh_page(&mut self, ws: &Workspace, id: &str) {
        let owner = Owner::Page(id.to_string());
        match ws.page(id) {
            Ok(page) if !page.meta.trashed => {
                let mut hosted: Vec<String> = page.meta.database.iter().cloned().collect();
                hosted.extend(
                    page.blocks
                        .iter()
                        .filter(|b| b.kind == BlockKind::Database)
                        .filter_map(|b| b.target.clone()),
                );
                self.hosted_by_page.insert(id.to_string(), hosted);
                self.index.update(&owner, page_docs(&page));
            }
            _ => {
                self.hosted_by_page.remove(id);
                self.index.remove(&owner);
            }
        }
    }

    fn refresh_database(&mut self, ws: &Workspace, id: &str) {
        let owner = Owner::Database(id.to_string());
        match ws.database(id) {
            Ok(database) => self.index.update(&owner, database_docs(&database)),
            Err(_) => self.index.remove(&owner),
        }
    }

    /// Brings the index up to date. Only what was marked is read again.
    pub fn sync(&mut self, ws: &Workspace) {
        if self.everything || !self.built {
            self.index = Index::new();
            self.hosted_by_page.clear();
            self.pages = ws.pages().into_iter().map(|m| m.id).collect();
            self.databases = ws.database_ids().into_iter().collect();
            self.everything = false;
            self.built = true;
        }
        for id in std::mem::take(&mut self.pages) {
            self.refresh_page(ws, &id);
        }
        for id in std::mem::take(&mut self.databases) {
            self.refresh_database(ws, &id);
        }
    }

    /// Searches the workspace. A row is returned only if a live page shows its database,
    /// and `id` is then that page.
    pub fn search(&mut self, ws: &Workspace, query: &str, limit: usize) -> Vec<SearchResult> {
        self.sync(ws);
        let mut host: BTreeMap<&str, &str> = BTreeMap::new();
        for (page, hosted) in &self.hosted_by_page {
            for database in hosted {
                host.entry(database).or_insert(page);
            }
        }
        let capped = limit.min(MAX_RESULTS);
        let mut out = Vec::new();
        for hit in self.index.search(query, MAX_RESULTS) {
            let page = if hit.owner == "page" {
                Some(hit.id.clone())
            } else {
                host.get(hit.id.as_str()).map(|p| (*p).to_string())
            };
            if let Some(page) = page {
                out.push(SearchResult { page, hit });
                if out.len() == capped {
                    break;
                }
            }
        }
        out
    }

    /// The bare index, for tests and tools.
    pub fn index(&self) -> &Index {
        &self.index
    }
}

/// A hit, with the page to open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SearchResult {
    pub page: String,
    #[serde(flatten)]
    pub hit: Hit,
}

#[cfg(test)]
mod tests;
