//! The knowledge graph: nodes and edges computed from the notes, never stored and never written
//! back, so it cannot disagree with them (ADR-014). A pure function of the notes, sorted, so the
//! same notes give the same bytes. See docs/61.

use casual_note_merge::Workspace;
use casual_note_model::{BlockKind, Database, Mark, Page};
use casual_note_search::{fold, stable_ln, tokenise};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::char::is_combining_mark;

const MAX_TAGS_PER_PAGE: usize = 1000;
const MAX_TAG_CHARS: usize = 128;
const SIMILAR_PER_PAGE: usize = 2;
const SIMILAR_THRESHOLD: f64 = 0.12;
const OVERLAP_TERMS_PER_PAGE: usize = 256;
const OVERLAP_PAGES_PER_TERM: usize = 200;
const OVERLAP_MIN_TERMS: usize = 3;
const OVERLAP_MAX_PAIRS: usize = 2_000_000;

/// The name of the interim similarity method, carried on every computed edge. It compares shared
/// words and understands nothing. A real model replaces it (docs/60).
pub const WORD_OVERLAP_METHOD: &str = "word-overlap-v1";

/// What a node stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Page,
    Tag,
    Row,
    Person,
}

/// A node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    /// The text as written. An untitled page is empty here and localised by the view.
    pub label: String,
    /// The page to open for this node, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// A row's status name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Links on this page to pages that are gone. They are kept in the notes, and are not edges.
    #[serde(skip_serializing_if = "is_zero")]
    pub dangling: u32,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// What an edge means.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    Link,
    Embed,
    Child,
    Tag,
    Row,
    Owner,
    Similar,
}

/// Whether a person made this connection or the computer guessed it. The type keeps them apart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "origin", rename_all = "lowercase")]
pub enum Origin {
    Made,
    Computed {
        method: &'static str,
        /// Thousandths, from 0 to 1000.
        score: u32,
    },
}

/// An edge. Between any pair, at most one per kind, with how many times it occurs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    #[serde(flatten)]
    pub origin: Origin,
    pub count: u32,
}

/// A whole graph, sorted: nodes by id, edges by kind, from and to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || is_combining_mark(c) || matches!(c, '_' | '/' | '-')
}

/// Tags as written, without the `#`. A `#` must follow the start, a space or punctuation, and a
/// closing `#` ends a tag in unspaced text, as in `#标签#之后`. The note is never rewritten.
pub fn tags_in(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(c) = chars.get(at).copied() {
        let before_ok = match at.checked_sub(1).and_then(|p| chars.get(p)) {
            Some(prev) => !prev.is_alphanumeric() && !is_combining_mark(*prev) && *prev != '_',
            None => true,
        };
        at += 1;
        if !matches!(c, '#' | '\u{ff03}') || !before_ok {
            continue;
        }
        let start = at;
        while chars.get(at).is_some_and(|c| is_tag_char(*c)) {
            at += 1;
        }
        let Some(body) = chars.get(start..at) else {
            continue;
        };
        if body.is_empty() {
            continue;
        }
        // `#2026` is a number, not a tag.
        let has_word = body
            .iter()
            .any(|c| !c.is_numeric() && !matches!(c, '_' | '/' | '-'));
        if has_word && out.len() < MAX_TAGS_PER_PAGE && body.len() <= MAX_TAG_CHARS {
            out.push(body.iter().collect());
        }
        if matches!(chars.get(at), Some('#' | '\u{ff03}')) {
            at += 1;
        }
    }
    out
}

/// Column names that mean a person, in the launch languages. A column typed `person` always does.
fn is_person_column(name: &str, kind: &str) -> bool {
    if kind == "person" {
        return true;
    }
    matches!(
        fold(name.trim()).as_str(),
        "owner"
            | "author"
            | "负责人"
            | "作者"
            | "所有者"
            | "担当者"
            | "著者"
            | "作成者"
            | "responsable"
            | "propietario"
            | "autor"
            | "autora"
            | "المالك"
            | "المؤلف"
            | "الكاتب"
            | "स्वामी"
            | "लेखक"
            | "मालिक"
            | "소유자"
            | "작성자"
            | "저자"
    )
}

#[derive(Default)]
struct Builder {
    nodes: BTreeMap<String, Node>,
    edges: BTreeMap<(EdgeKind, String, String), u32>,
    spellings: BTreeMap<String, BTreeMap<String, u32>>,
}

impl Builder {
    fn edge(&mut self, kind: EdgeKind, from: &str, to: &str) {
        if from != to {
            let n = self
                .edges
                .entry((kind, from.to_string(), to.to_string()))
                .or_insert(0);
            *n = n.saturating_add(1);
        }
    }

    fn node(&mut self, id: String, kind: NodeKind, page: Option<String>, status: Option<String>) {
        self.nodes.entry(id.clone()).or_insert(Node {
            id,
            kind,
            label: String::new(),
            page,
            status,
            dangling: 0,
        });
    }

    fn spelled(&mut self, id: &str, written: &str) {
        let n = self
            .spellings
            .entry(id.to_string())
            .or_default()
            .entry(written.to_string())
            .or_insert(0);
        *n = n.saturating_add(1);
    }
}

/// Text a page's tags are read from: blocks except code, without code marks, and canvas items.
fn taggable(page: &Page) -> Vec<String> {
    let mut out = Vec::new();
    for block in &page.blocks {
        if block.kind == BlockKind::Code {
            continue;
        }
        let mut text = String::new();
        for run in &block.runs {
            if run.marks.contains(&Mark::Code) {
                text.push(' ');
            } else {
                text.push_str(&run.text);
            }
        }
        out.push(text);
    }
    out.extend(page.canvas.items.iter().map(|i| i.text.clone()));
    out
}

/// Derives the graph from these pages and databases. Trashed pages must already be left out.
pub fn derive(pages: &[Page], databases: &[Database]) -> Graph {
    let mut pages: Vec<&Page> = pages.iter().collect();
    pages.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
    let live: BTreeSet<&str> = pages.iter().map(|p| p.meta.id.as_str()).collect();
    let mut b = Builder::default();

    for page in &pages {
        let id = format!("page:{}", page.meta.id);
        b.node(id.clone(), NodeKind::Page, Some(page.meta.id.clone()), None);
        if let Some(node) = b.nodes.get_mut(&id) {
            node.label.clone_from(&page.meta.title);
        }
    }

    // A full-page database is hosted by its page, otherwise by the first page that shows it.
    let mut host: BTreeMap<String, String> = BTreeMap::new();
    for page in &pages {
        if let Some(database) = &page.meta.database {
            host.entry(database.clone())
                .or_insert_with(|| page.meta.id.clone());
        }
    }
    for page in &pages {
        for block in &page.blocks {
            if block.kind == BlockKind::Database
                && let Some(database) = &block.target
            {
                host.entry(database.clone())
                    .or_insert_with(|| page.meta.id.clone());
            }
        }
    }

    for page in &pages {
        let me = format!("page:{}", page.meta.id);
        if let Some(parent) = &page.meta.parent
            && live.contains(parent.as_str())
        {
            b.edge(EdgeKind::Child, &format!("page:{parent}"), &me);
        }
        let mut dangling = 0u32;
        for block in &page.blocks {
            let mut seen_link = None;
            for run in &block.runs {
                let Some(target) = &run.link else {
                    seen_link = None;
                    continue;
                };
                if seen_link == Some(target) {
                    continue;
                }
                seen_link = Some(target);
                if live.contains(target.as_str()) {
                    b.edge(EdgeKind::Link, &me, &format!("page:{target}"));
                } else {
                    dangling = dangling.saturating_add(1);
                }
            }
            match (block.kind, &block.target) {
                (BlockKind::Embed, Some(target)) => {
                    if live.contains(target.as_str()) {
                        b.edge(EdgeKind::Embed, &me, &format!("page:{target}"));
                    } else {
                        dangling = dangling.saturating_add(1);
                    }
                }
                (BlockKind::Database, Some(database)) => {
                    if let Some(other) = host.get(database) {
                        b.edge(EdgeKind::Embed, &me, &format!("page:{other}"));
                    }
                }
                _ => {}
            }
        }
        if let Some(node) = b.nodes.get_mut(&me) {
            node.dangling = dangling;
        }
        let mut counted = 0;
        for text in taggable(page) {
            for written in tags_in(&text) {
                counted += 1;
                if counted > MAX_TAGS_PER_PAGE {
                    break;
                }
                let id = format!("tag:{}", fold(&written));
                b.node(id.clone(), NodeKind::Tag, None, None);
                b.spelled(&id, &written);
                b.edge(EdgeKind::Tag, &me, &id);
            }
        }
    }

    let mut databases: Vec<&Database> = databases.iter().collect();
    databases.sort_by(|a, b| a.id.cmp(&b.id));
    for database in databases {
        let Some(page) = host.get(&database.id) else {
            continue;
        };
        let people: Vec<&str> = database
            .columns
            .iter()
            .filter(|c| !c.hidden && is_person_column(&c.name, &c.kind))
            .map(|c| c.id.as_str())
            .collect();
        for row in database.rows.iter().filter(|r| !r.name.is_empty()) {
            let id = format!("row:{}:{}", database.id, row.id);
            let status = database
                .statuses
                .iter()
                .find(|s| s.id == row.status)
                .map(|s| s.name.clone());
            b.node(id.clone(), NodeKind::Row, Some(page.clone()), status);
            if let Some(node) = b.nodes.get_mut(&id) {
                node.label.clone_from(&row.name);
            }
            b.edge(EdgeKind::Row, &format!("page:{page}"), &id);
            for column in &people {
                let Some(value) = row.values.get(*column).map(|v| v.trim()) else {
                    continue;
                };
                if value.is_empty() {
                    continue;
                }
                let person = format!("person:{}", fold(value));
                b.node(person.clone(), NodeKind::Person, None, None);
                b.spelled(&person, value);
                b.edge(EdgeKind::Owner, &id, &person);
            }
        }
    }

    // The label is the spelling used most, ties by code point, so the choice is deterministic.
    for (id, spellings) in &b.spellings {
        let mut best: Option<(&String, u32)> = None;
        for (written, count) in spellings {
            if best.is_none_or(|(_, n)| *count > n) {
                best = Some((written, *count));
            }
        }
        if let (Some(node), Some((written, _))) = (b.nodes.get_mut(id), best) {
            node.label = if node.kind == NodeKind::Tag {
                format!("#{written}")
            } else {
                written.clone()
            };
        }
    }

    let made_pairs: BTreeSet<(&str, &str)> = b
        .edges
        .keys()
        .filter(|(kind, _, _)| matches!(kind, EdgeKind::Link | EdgeKind::Embed | EdgeKind::Child))
        .map(|(_, from, to)| {
            if from < to {
                (from.as_str(), to.as_str())
            } else {
                (to.as_str(), from.as_str())
            }
        })
        .collect();
    let similar = word_overlap_edges(&pages, &made_pairs);

    let mut edges: Vec<Edge> = b
        .edges
        .iter()
        .map(|((kind, from, to), count)| Edge {
            from: from.clone(),
            to: to.clone(),
            kind: *kind,
            origin: Origin::Made,
            count: *count,
        })
        .collect();
    edges.extend(similar);
    edges.sort_by(|x, y| (x.kind, &x.from, &x.to).cmp(&(y.kind, &y.from, &y.to)));
    Graph {
        nodes: b.nodes.into_values().collect(),
        edges,
    }
}

fn page_terms(page: &Page) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    let mut add = |text: &str| {
        terms.extend(
            tokenise(text)
                .into_iter()
                .filter(|t| t.primary)
                .map(|t| t.term),
        );
    };
    add(&page.meta.title);
    for block in &page.blocks {
        add(&block.plain());
    }
    for item in &page.canvas.items {
        add(&item.text);
    }
    terms
}

/// Word overlap between pages: shared terms over all terms, rare words counting more. This is an
/// interim stand-in and not a model of meaning. It pairs pages only through terms they share, and
/// the caps below stop it ever comparing all pairs of a big workspace.
fn word_overlap_edges(pages: &[&Page], made: &BTreeSet<(&str, &str)>) -> Vec<Edge> {
    let sets: Vec<BTreeSet<String>> = pages.iter().map(|p| page_terms(p)).collect();
    let mut df: BTreeMap<&str, usize> = BTreeMap::new();
    for set in &sets {
        for term in set {
            *df.entry(term.as_str()).or_insert(0) += 1;
        }
    }
    let total = pages.len() as f64;
    let weight = |term: &str| -> f64 {
        let n = df.get(term).copied().unwrap_or(1).max(1) as f64;
        stable_ln(1.0 + total / n)
    };
    // Each page keeps its rarest terms, ties by term.
    let mut kept: Vec<Vec<(&str, f64)>> = Vec::with_capacity(sets.len());
    for set in &sets {
        let mut terms: Vec<&str> = set.iter().map(String::as_str).collect();
        terms.sort_by_key(|t| (df.get(t).copied().unwrap_or(0), *t));
        terms.truncate(OVERLAP_TERMS_PER_PAGE);
        kept.push(terms.into_iter().map(|t| (t, weight(t))).collect());
    }
    let mut lists: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut sums = Vec::with_capacity(kept.len());
    for (i, terms) in kept.iter().enumerate() {
        sums.push(terms.iter().map(|t| t.1).sum::<f64>());
        if terms.len() < OVERLAP_MIN_TERMS {
            continue;
        }
        for (term, _) in terms {
            lists.entry(term).or_default().push(i);
        }
    }
    let mut shared: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    'terms: for (term, list) in &lists {
        if list.len() < 2 || list.len() > OVERLAP_PAGES_PER_TERM {
            continue;
        }
        let w = weight(term);
        for (x, a) in list.iter().enumerate() {
            for c in list.iter().skip(x + 1) {
                if shared.len() >= OVERLAP_MAX_PAIRS && !shared.contains_key(&(*a, *c)) {
                    break 'terms;
                }
                *shared.entry((*a, *c)).or_insert(0.0) += w;
            }
        }
    }
    let mut per_page: Vec<Vec<(u32, usize)>> = vec![Vec::new(); pages.len()];
    for ((a, c), overlap) in &shared {
        let (Some(sa), Some(sc)) = (sums.get(*a), sums.get(*c)) else {
            continue;
        };
        let union = sa + sc - overlap;
        if union <= 0.0 {
            continue;
        }
        let score = overlap / union;
        if score < SIMILAR_THRESHOLD {
            continue;
        }
        let milli = (score * 1000.0).round().clamp(0.0, 1000.0) as u32;
        if let Some(list) = per_page.get_mut(*a) {
            list.push((milli, *c));
        }
        if let Some(list) = per_page.get_mut(*c) {
            list.push((milli, *a));
        }
    }
    let mut chosen: BTreeMap<(usize, usize), u32> = BTreeMap::new();
    for (i, list) in per_page.iter_mut().enumerate() {
        list.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)));
        for (milli, other) in list.iter().take(SIMILAR_PER_PAGE) {
            chosen.insert((i.min(*other), i.max(*other)), *milli);
        }
    }
    let mut out = Vec::new();
    for ((a, c), milli) in chosen {
        let (Some(pa), Some(pc)) = (pages.get(a), pages.get(c)) else {
            continue;
        };
        let (from, to) = (
            format!("page:{}", pa.meta.id),
            format!("page:{}", pc.meta.id),
        );
        let pair = if from < to {
            (from.as_str(), to.as_str())
        } else {
            (to.as_str(), from.as_str())
        };
        if made.contains(&pair) {
            continue;
        }
        let (from, to) = (pair.0.to_string(), pair.1.to_string());
        out.push(Edge {
            from,
            to,
            kind: EdgeKind::Similar,
            origin: Origin::Computed {
                method: WORD_OVERLAP_METHOD,
                score: milli,
            },
            count: 1,
        });
    }
    out
}

/// Derives the graph of a workspace's live pages.
pub fn from_workspace(ws: &Workspace) -> Graph {
    let pages: Vec<Page> = ws
        .pages()
        .into_iter()
        .filter(|m| !m.trashed)
        .filter_map(|m| ws.page(&m.id).ok())
        .collect();
    let databases: Vec<Database> = ws
        .database_ids()
        .into_iter()
        .filter_map(|id| ws.database(&id).ok())
        .collect();
    derive(&pages, &databases)
}

#[cfg(test)]
mod tests;
