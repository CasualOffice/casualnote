//! What the log and sync need from the CRDT, per document: updates since a version, version
//! vectors, a scratch replica for checking, and finding blocks two devices changed unseen.
//! Document names are the same as in the workspace container: "" is the page tree, "d:<id>" a
//! database, and anything else a page.

use super::{Workspace, bad, new_doc, read_block, string, value_string};
use casual_note_model::{Block, Error};
use loro::{ExportMode, LoroDoc, LoroValue, ValueOrContainer, VersionVector};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// The name of the page tree's document.
pub const TREE_DOC: &str = "";

/// How one document took an update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocImport {
    /// Everything in it is now part of the document.
    Applied,
    /// Some of it waits for changes that have not arrived. Nothing is lost: it applies when they do.
    Pending,
}

/// One document's changes since the last save.
#[derive(Clone, Debug)]
pub struct DocUpdate {
    pub name: String,
    pub update: Vec<u8>,
}

/// What has already reached the log, per document.
#[derive(Debug, Default)]
pub struct SaveCursor {
    saved: BTreeMap<String, VersionVector>,
    seen: BTreeMap<String, loro::Frontiers>,
}

impl SaveCursor {
    /// Records that a workspace container's contents are already durable, so they are not written
    /// again. Unreadable parts are left out: the only cost is writing them once more.
    pub fn mark_durable(&mut self, container: &[u8]) {
        let Ok(parts) = parse_container(container) else {
            return;
        };
        for (name, body) in parts {
            self.mark_part(&name, body);
        }
    }

    /// As `mark_durable`, for one document's update.
    pub fn mark_part(&mut self, name: &str, body: &[u8]) {
        if let Ok(meta) = LoroDoc::decode_import_blob_meta(body, false) {
            self.saved
                .entry(name.to_owned())
                .or_default()
                .extend_to_include_vv(meta.partial_end_vv.iter());
            self.seen.remove(name);
        }
    }
}

/// A block two devices changed without either having seen the other's change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overlap {
    /// The same on every device that sees the same two versions.
    pub id: String,
    pub page: String,
    pub block: String,
    pub base: Option<Block>,
    /// This device's version. `None` means it deleted the block.
    pub here: Option<Block>,
    /// The other device's version. `None` means it deleted the block.
    pub there: Option<Block>,
    /// The merged block, which is what the page shows now.
    pub merged: Option<Block>,
}

/// A throwaway copy of one document, for checking bytes without touching the live workspace.
#[derive(Debug)]
pub struct Replica {
    doc: LoroDoc,
}

impl Replica {
    pub fn new(peer: u64) -> Result<Self, Error> {
        Ok(Self {
            doc: new_doc(peer)?,
        })
    }

    pub fn import(&mut self, bytes: &[u8]) -> Result<DocImport, Error> {
        let status = self.doc.import(bytes).map_err(bad)?;
        Ok(if status.pending.is_some() {
            DocImport::Pending
        } else {
            DocImport::Applied
        })
    }

    /// The whole document with its history.
    pub fn snapshot(&self) -> Result<Vec<u8>, Error> {
        self.doc.export(ExportMode::Snapshot).map_err(bad)
    }

    /// The version vector, encoded.
    pub fn version(&self) -> Vec<u8> {
        encode_version(&self.doc.oplog_vv())
    }

    pub fn state_hash(&self) -> [u8; 32] {
        state_hash(&self.doc)
    }
}

/// Canonical bytes of a version vector, sorted by peer.
fn encode_version(vv: &VersionVector) -> Vec<u8> {
    let mut sorted: Vec<(u64, i32)> = vv.iter().map(|(p, c)| (*p, *c)).collect();
    sorted.sort_unstable();
    let mut out = Vec::with_capacity(sorted.len() * 12);
    for (p, c) in sorted {
        out.extend_from_slice(&p.to_le_bytes());
        out.extend_from_slice(&c.to_le_bytes());
    }
    out
}

fn hash_value(v: &LoroValue, h: &mut Sha256) {
    match v {
        LoroValue::Null => h.update([0]),
        LoroValue::Bool(b) => h.update([1, u8::from(*b)]),
        LoroValue::Double(d) => {
            h.update([2]);
            h.update(d.to_le_bytes());
        }
        LoroValue::I64(i) => {
            h.update([3]);
            h.update(i.to_le_bytes());
        }
        LoroValue::Binary(b) => {
            h.update([4]);
            h.update((b.len() as u64).to_le_bytes());
            h.update(&b[..]);
        }
        LoroValue::String(s) => {
            h.update([5]);
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        LoroValue::List(items) => {
            h.update([6]);
            h.update((items.len() as u64).to_le_bytes());
            for item in items.iter() {
                hash_value(item, h);
            }
        }
        LoroValue::Map(map) => {
            h.update([7]);
            h.update((map.len() as u64).to_le_bytes());
            let sorted: BTreeMap<&String, &LoroValue> = map.iter().collect();
            for (k, item) in sorted {
                h.update((k.len() as u64).to_le_bytes());
                h.update(k.as_bytes());
                hash_value(item, h);
            }
        }
        LoroValue::Container(id) => {
            h.update([8]);
            h.update(id.to_string().as_bytes());
        }
    }
}

/// A document's visible value and version. Marks count through the version: replicas with equal
/// versions hold equal operations.
fn state_hash(doc: &LoroDoc) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(encode_version(&doc.oplog_vv()));
    hash_value(&doc.get_deep_value(), &mut h);
    h.finalize().into()
}

fn visible_blocks(doc: &LoroDoc) -> BTreeMap<String, Block> {
    let order = doc.get_movable_list("order");
    let records = doc.get_map("blocks");
    let mut out = BTreeMap::new();
    for i in 0..order.len() {
        let Some(id) = order.get(i).and_then(|v| value_string(&v)) else {
            continue;
        };
        if let Some(ValueOrContainer::Container(loro::Container::Map(r))) = records.get(&id) {
            let block = read_block(&id, &r);
            out.insert(id, block);
        }
    }
    out
}

fn common_version(a: &VersionVector, b: &VersionVector) -> VersionVector {
    let mut out = VersionVector::default();
    for (peer, counter) in a.iter() {
        if let Some(other) = b.get(peer) {
            let least = (*counter).min(*other);
            if least > 0 {
                out.insert(*peer, least);
            }
        }
    }
    out
}

fn same(a: Option<&Block>, b: Option<&Block>) -> bool {
    a == b
}

fn overlap_id(page: &str, block: &str, base: &VersionVector, a: &Block, b: &Block) -> String {
    let mut h = Sha256::new();
    h.update((page.len() as u64).to_le_bytes());
    h.update(page.as_bytes());
    h.update((block.len() as u64).to_le_bytes());
    h.update(block.as_bytes());
    h.update(encode_version(base));
    // The two versions in a fixed order, so both devices name this marker alike.
    let mut texts = [a.plain(), b.plain()];
    texts.sort();
    for t in &texts {
        h.update((t.len() as u64).to_le_bytes());
        h.update(t.as_bytes());
    }
    let digest = h.finalize();
    digest[..16].iter().map(|b| format!("{b:02x}")).collect()
}

impl Workspace {
    /// The peer id of this open.
    pub fn peer(&self) -> u64 {
        self.peer
    }

    fn named(&self, name: &str) -> Option<&LoroDoc> {
        if name.is_empty() {
            Some(&self.tree)
        } else if let Some(id) = name.strip_prefix("d:") {
            self.databases.get(id)
        } else {
            self.pages.get(name)
        }
    }

    /// Every document that exists, the tree first.
    pub fn doc_names(&self) -> Vec<String> {
        let mut out = vec![TREE_DOC.to_owned()];
        out.extend(self.pages.keys().cloned());
        out.extend(self.databases.keys().map(|id| format!("d:{id}")));
        out
    }

    /// A document's changes since `cursor`, which moves on. Cheap when nothing changed.
    pub fn changed_docs(&self, cursor: &mut SaveCursor) -> Result<Vec<DocUpdate>, Error> {
        let mut out = Vec::new();
        let mut each = |name: &str, doc: &LoroDoc| -> Result<(), Error> {
            let now = doc.oplog_frontiers();
            if cursor.seen.get(name) == Some(&now) {
                return Ok(());
            }
            let vv = doc.oplog_vv();
            let saved = cursor.saved.entry(name.to_owned()).or_default();
            if !saved.includes_vv(&vv) {
                let update = doc.export(ExportMode::updates(saved)).map_err(bad)?;
                out.push(DocUpdate {
                    name: name.to_owned(),
                    update,
                });
                saved.extend_to_include_vv(vv.iter());
            }
            cursor.seen.insert(name.to_owned(), now);
            Ok(())
        };
        each(TREE_DOC, &self.tree)?;
        for (id, doc) in &self.pages {
            each(id, doc)?;
        }
        for (id, doc) in &self.databases {
            each(&format!("d:{id}"), doc)?;
        }
        Ok(out)
    }

    /// One document's whole state with history.
    pub fn doc_snapshot(&self, name: &str) -> Result<Vec<u8>, Error> {
        let doc = self
            .named(name)
            .ok_or_else(|| Error::NotFound(name.into()))?;
        doc.export(ExportMode::Snapshot).map_err(bad)
    }

    /// A document's state hash, for comparing replicas.
    pub fn doc_state_hash(&self, name: &str) -> Option<[u8; 32]> {
        self.named(name).map(state_hash)
    }

    /// Imports an update into one document, creating it if needed, and reports the blocks that
    /// two devices changed unseen. The merge has already happened when this returns, and the
    /// merged text is what the page shows.
    pub fn import_doc(
        &mut self,
        name: &str,
        bytes: &[u8],
    ) -> Result<(DocImport, Vec<Overlap>), Error> {
        if self.named(name).is_none() {
            let doc = new_doc(self.peer)?;
            if let Some(id) = name.strip_prefix("d:") {
                self.databases.insert(id.to_owned(), doc);
            } else {
                self.pages.insert(name.to_owned(), doc);
            }
        }
        let doc = self
            .named(name)
            .ok_or_else(|| Error::NotFound(name.into()))?
            .clone();
        let here_before = doc.oplog_vv();
        let status = doc.import(bytes).map_err(bad)?;
        let pending = status.pending.is_some();
        let overlaps = if name.is_empty() || name.starts_with("d:") {
            Vec::new()
        } else {
            find_overlaps(name, &doc, &here_before, bytes).unwrap_or_default()
        };
        for meta in self.pages() {
            if !self.pages.contains_key(&meta.id) {
                self.pages.insert(meta.id, new_doc(self.peer)?);
            }
        }
        let outcome = if pending {
            DocImport::Pending
        } else {
            DocImport::Applied
        };
        Ok((outcome, overlaps))
    }

    /// Records in the page that a review is settled, so it clears on every device and does not
    /// reopen when a late import arrives. Any content change is an ordinary op from the caller.
    pub fn resolve_review(&mut self, page: &str, id: &str, choice: &str) -> Result<(), Error> {
        let doc = self.doc(page)?.clone();
        doc.get_map("reviews").insert(id, choice).map_err(bad)?;
        doc.commit();
        Ok(())
    }

    /// How a review was settled, if it was.
    pub fn review_choice(&self, page: &str, id: &str) -> Option<String> {
        string(&self.pages.get(page)?.get_map("reviews"), id)
    }
}

/// What the writer of an update had seen: the update's own end, and what its first change named.
fn writer_version(doc: &LoroDoc, bytes: &[u8]) -> Option<VersionVector> {
    let meta = LoroDoc::decode_import_blob_meta(bytes, false).ok()?;
    let mut version = doc.frontiers_to_vv(&meta.start_frontiers)?;
    version.extend_to_include_vv(meta.partial_end_vv.iter());
    Some(version)
}

fn find_overlaps(
    page: &str,
    doc: &LoroDoc,
    here: &VersionVector,
    bytes: &[u8],
) -> Option<Vec<Overlap>> {
    let there = writer_version(doc, bytes)?;
    // A dependency has not arrived. The later import of this same data looks again.
    if !doc.oplog_vv().includes_vv(&there) {
        return None;
    }
    if here.includes_vv(&there) || there.includes_vv(here) {
        return Some(Vec::new());
    }
    let base = common_version(here, &there);
    let at = |vv: &VersionVector| -> Option<BTreeMap<String, Block>> {
        let forked = doc.fork_at(&doc.vv_to_frontiers(vv)).ok()?;
        Some(visible_blocks(&forked))
    };
    let (base_blocks, here_blocks, there_blocks) = (at(&base)?, at(here)?, at(&there)?);
    let merged = visible_blocks(doc);
    let mut ids: BTreeSet<&String> = here_blocks.keys().collect();
    ids.extend(there_blocks.keys());
    ids.extend(base_blocks.keys());
    let mut out = Vec::new();
    for id in ids {
        let (b, h, t) = (
            base_blocks.get(id),
            here_blocks.get(id),
            there_blocks.get(id),
        );
        if same(b, h) || same(b, t) || same(h, t) {
            continue;
        }
        let empty = || Block {
            id: id.clone(),
            kind: casual_note_model::BlockKind::Paragraph,
            checked: false,
            runs: Vec::new(),
            target: None,
        };
        let (left, right) = (
            h.cloned().unwrap_or_else(empty),
            t.cloned().unwrap_or_else(empty),
        );
        out.push(Overlap {
            id: overlap_id(page, id, &base, &left, &right),
            page: page.to_owned(),
            block: id.clone(),
            base: b.cloned(),
            here: h.cloned(),
            there: t.cloned(),
            merged: merged.get(id).cloned(),
        });
    }
    Some(out)
}

/// The named parts of a workspace container, in order. Bounded by the bytes given.
pub fn parse_container(bytes: &[u8]) -> Result<Vec<(String, &[u8])>, Error> {
    let mut rest = bytes
        .strip_prefix(&super::MAGIC[..])
        .ok_or_else(|| Error::BadData("not a workspace".into()))?;
    let mut parts = Vec::new();
    while !rest.is_empty() {
        if parts.len() >= super::MAX_DOCS {
            return Err(Error::BadData("too many documents".into()));
        }
        let (head, tail) = rest
            .split_first_chunk::<4>()
            .ok_or_else(|| Error::BadData("truncated".into()))?;
        let n = usize::try_from(u32::from_le_bytes(*head)).map_err(bad)?;
        if n > tail.len() {
            return Err(Error::BadData("truncated".into()));
        }
        let (name, tail) = tail.split_at(n);
        let name = std::str::from_utf8(name).map_err(bad)?.to_owned();
        let (head, tail) = tail
            .split_first_chunk::<8>()
            .ok_or_else(|| Error::BadData("truncated".into()))?;
        let n = usize::try_from(u64::from_le_bytes(*head)).map_err(bad)?;
        if n > tail.len() {
            return Err(Error::BadData("truncated".into()));
        }
        let (body, tail) = tail.split_at(n);
        parts.push((name, body));
        rest = tail;
    }
    Ok(parts)
}

/// A workspace container from named parts. A name may repeat: each part imports in turn, so a
/// snapshot followed by updates is a valid container.
pub fn build_container<'a>(parts: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Vec<u8> {
    let mut out = Vec::from(&super::MAGIC[..]);
    for (name, body) in parts {
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(body.len() as u64).to_le_bytes());
        out.extend_from_slice(body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_note_model::{Block, BlockKind, Run};

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

    #[test]
    fn saving_exports_only_what_changed_and_nothing_twice() {
        let mut ws = Workspace::new(1).unwrap();
        let mut cursor = SaveCursor::default();
        ws.create_page("p", None, "T", None).unwrap();
        ws.create_page("q", None, "U", None).unwrap();
        let first = ws.changed_docs(&mut cursor).unwrap();
        assert_eq!(first.len(), 1, "only the tree has operations");
        assert!(ws.changed_docs(&mut cursor).unwrap().is_empty());
        ws.insert_block("p", None, &para("b", "hello")).unwrap();
        let second = ws.changed_docs(&mut cursor).unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].name, "p");
        // The updates alone rebuild the page in another replica.
        let mut other = Workspace::new(2).unwrap();
        for u in first.iter().chain(&second) {
            other.import_doc(&u.name, &u.update).unwrap();
        }
        assert_eq!(other.page("p").unwrap().blocks[0].plain(), "hello");
    }

    #[test]
    fn a_marked_container_is_not_written_again() {
        let mut ws = Workspace::new(1).unwrap();
        ws.create_page("p", None, "T", None).unwrap();
        ws.insert_block("p", None, &para("b", "x")).unwrap();
        let bytes = ws.export().unwrap();
        let mut other = Workspace::new(2).unwrap();
        other.import(&bytes).unwrap();
        let mut cursor = SaveCursor::default();
        cursor.mark_durable(&bytes);
        assert!(other.changed_docs(&mut cursor).unwrap().is_empty());
        other.splice_text("p", "b", 0, 0, "y").unwrap();
        assert_eq!(other.changed_docs(&mut cursor).unwrap().len(), 1);
    }

    #[test]
    fn a_container_may_hold_a_snapshot_then_updates() {
        let mut ws = Workspace::new(1).unwrap();
        ws.create_page("p", None, "T", None).unwrap();
        ws.insert_block("p", None, &para("b", "x")).unwrap();
        let snap = ws.doc_snapshot("p").unwrap();
        let tree = ws.doc_snapshot(TREE_DOC).unwrap();
        let mut cursor = SaveCursor::default();
        cursor.mark_part("p", &snap);
        ws.splice_text("p", "b", 1, 0, "y").unwrap();
        let later = ws.changed_docs(&mut cursor).unwrap();
        let tail = later.iter().find(|u| u.name == "p").unwrap();
        let bytes = build_container([
            (TREE_DOC, tree.as_slice()),
            ("p", snap.as_slice()),
            ("p", tail.update.as_slice()),
        ]);
        let mut other = Workspace::new(2).unwrap();
        other.import(&bytes).unwrap();
        assert_eq!(other.page("p").unwrap().blocks[0].plain(), "xy");
    }

    fn pair() -> (Workspace, Workspace) {
        let mut a = Workspace::new(1).unwrap();
        a.create_page("p", None, "T", None).unwrap();
        a.insert_block("p", None, &para("b", "base")).unwrap();
        a.insert_block("p", Some("b"), &para("c", "other")).unwrap();
        let mut b = Workspace::new(2).unwrap();
        b.import(&a.export().unwrap()).unwrap();
        (a, b)
    }

    #[test]
    fn two_devices_changing_one_block_is_found_on_both_with_one_id() {
        let (mut a, mut b) = pair();
        let (mut ca, mut cb) = (SaveCursor::default(), SaveCursor::default());
        ca.mark_durable(&a.export().unwrap());
        cb.mark_durable(&b.export().unwrap());
        a.splice_text("p", "b", 4, 0, " from a").unwrap();
        b.splice_text("p", "b", 4, 0, " from b").unwrap();
        let ua = a.changed_docs(&mut ca).unwrap().remove(0);
        let ub = b.changed_docs(&mut cb).unwrap().remove(0);
        let (_, on_b) = b.import_doc("p", &ua.update).unwrap();
        let (_, on_a) = a.import_doc("p", &ub.update).unwrap();
        assert_eq!(on_a.len(), 1);
        assert_eq!(on_b.len(), 1);
        assert_eq!(on_a[0].id, on_b[0].id);
        assert_eq!(on_a[0].block, "b");
        assert_eq!(on_a[0].here.as_ref().unwrap().plain(), "base from a");
        assert_eq!(on_a[0].there.as_ref().unwrap().plain(), "base from b");
        assert_eq!(on_a[0].base.as_ref().unwrap().plain(), "base");
        // The merge happened first, and nothing was held back.
        let merged = on_a[0].merged.as_ref().unwrap().plain();
        assert!(merged.contains("from a") && merged.contains("from b"));
        assert_eq!(a.page("p").unwrap().blocks[0].plain(), merged);
    }

    #[test]
    fn different_blocks_and_seen_changes_are_not_marked() {
        let (mut a, mut b) = pair();
        let (mut ca, mut cb) = (SaveCursor::default(), SaveCursor::default());
        ca.mark_durable(&a.export().unwrap());
        cb.mark_durable(&b.export().unwrap());
        a.splice_text("p", "b", 0, 0, "A").unwrap();
        b.splice_text("p", "c", 0, 0, "B").unwrap();
        let ua = a.changed_docs(&mut ca).unwrap().remove(0);
        let (_, found) = b.import_doc("p", &ua.update).unwrap();
        assert!(found.is_empty(), "different blocks merge quietly");
        // b has now seen a's change, so a later edit by b to the same block is not concurrent.
        let ub = b.changed_docs(&mut cb).unwrap().remove(0);
        a.import_doc("p", &ub.update).unwrap();
        cb.mark_part("p", &ua.update);
        b.splice_text("p", "b", 0, 0, "seen").unwrap();
        let later = b.changed_docs(&mut cb).unwrap().remove(0);
        let (_, found) = a.import_doc("p", &later.update).unwrap();
        assert!(found.is_empty(), "b had seen a's edit");
    }

    #[test]
    fn delete_against_edit_is_marked() {
        let (mut a, mut b) = pair();
        let (mut ca, mut cb) = (SaveCursor::default(), SaveCursor::default());
        ca.mark_durable(&a.export().unwrap());
        cb.mark_durable(&b.export().unwrap());
        a.delete_block("p", "b").unwrap();
        b.splice_text("p", "b", 0, 0, "kept ").unwrap();
        let ub = b.changed_docs(&mut cb).unwrap().remove(0);
        let _ = a.changed_docs(&mut ca).unwrap();
        let (_, found) = a.import_doc("p", &ub.update).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].here.is_none());
        assert_eq!(found[0].there.as_ref().unwrap().plain(), "kept base");
    }

    #[test]
    fn a_settled_review_travels_with_the_page() {
        let (mut a, mut b) = pair();
        let mut ca = SaveCursor::default();
        ca.mark_durable(&a.export().unwrap());
        a.resolve_review("p", "m1", "here").unwrap();
        let u = a.changed_docs(&mut ca).unwrap().remove(0);
        b.import_doc("p", &u.update).unwrap();
        assert_eq!(b.review_choice("p", "m1").as_deref(), Some("here"));
        assert_eq!(b.review_choice("p", "m2"), None);
    }

    #[test]
    fn replicas_with_equal_updates_hash_alike_and_hostile_bytes_are_refused() {
        let (a, _) = pair();
        let snap = a.doc_snapshot("p").unwrap();
        let mut one = Replica::new(5).unwrap();
        one.import(&snap).unwrap();
        let mut two = Replica::new(6).unwrap();
        two.import(&one.snapshot().unwrap()).unwrap();
        assert_eq!(one.state_hash(), two.state_hash());
        assert_eq!(one.version(), two.version());
        assert_eq!(a.doc_state_hash("p").unwrap(), one.state_hash());
        assert!(Replica::new(7).unwrap().import(b"not an update").is_err());
    }

    #[test]
    fn a_missing_dependency_is_pending_and_applies_when_it_arrives() {
        let (mut a, _) = pair();
        let mut cursor = SaveCursor::default();
        cursor.mark_durable(&a.export().unwrap());
        a.splice_text("p", "b", 0, 0, "1").unwrap();
        let one = a.changed_docs(&mut cursor).unwrap().remove(0);
        a.splice_text("p", "b", 0, 0, "2").unwrap();
        let two = a.changed_docs(&mut cursor).unwrap().remove(0);
        let mut b = Workspace::new(9).unwrap();
        b.import(&a.export().unwrap()).unwrap();
        let mut c = Replica::new(10).unwrap();
        assert_eq!(c.import(&two.update).unwrap(), DocImport::Pending);
        assert_eq!(
            c.import(&one.update).unwrap(),
            DocImport::Pending,
            "the rest of the history is still missing"
        );
        c.import(&a.doc_snapshot("p").unwrap()).unwrap();
        assert_eq!(c.state_hash(), a.doc_state_hash("p").unwrap());
    }
}
