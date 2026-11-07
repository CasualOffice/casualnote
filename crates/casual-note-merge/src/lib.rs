//! The only crate that names the CRDT library (ADR-005, ADR-006). A workspace is one document
//! for the page tree and one per page. Text positions are UTF-16, as the browser counts them.

use casual_note_model::{
    Block, BlockKind, Canvas, CanvasItem, Column, Database, Error, Ink, Mark, Note, Page, PageMeta,
    Row, Run, Status, VIEW_PAGE, decode_samples, encode_samples,
};
use loro::{
    ExpandType, ExportMode, LoroDoc, LoroMap, LoroMovableList, LoroText, LoroValue, StyleConfig,
    StyleConfigMap, TextDelta, TreeID, ValueOrContainer,
};
use std::collections::BTreeMap;

const MAGIC: &[u8; 5] = b"CNWS1";
const MAX_DOCS: usize = 1_000_000;

fn bad(e: impl std::fmt::Debug) -> Error {
    Error::BadData(format!("{e:?}"))
}

fn new_doc(peer: u64) -> Result<LoroDoc, Error> {
    let doc = LoroDoc::new();
    doc.set_peer_id(peer).map_err(bad)?;
    let mut styles = StyleConfigMap::new();
    for mark in Mark::ALL {
        styles.insert(
            mark.as_str().into(),
            StyleConfig {
                expand: ExpandType::After,
            },
        );
    }
    styles.insert(
        "link".into(),
        StyleConfig {
            expand: ExpandType::None,
        },
    );
    doc.config_text_style(styles);
    Ok(doc)
}

/// All of a workspace's documents.
#[derive(Debug)]
pub struct Workspace {
    peer: u64,
    tree: LoroDoc,
    pages: BTreeMap<String, LoroDoc>,
    databases: BTreeMap<String, LoroDoc>,
}

impl Workspace {
    /// An empty workspace. The host supplies a fresh `peer` for every open (ADR-030).
    pub fn new(peer: u64) -> Result<Self, Error> {
        let tree = new_doc(peer)?;
        tree.get_tree("pages").enable_fractional_index(0);
        Ok(Self {
            peer,
            tree,
            pages: BTreeMap::new(),
            databases: BTreeMap::new(),
        })
    }

    fn node(&self, id: &str) -> Option<TreeID> {
        let tree = self.tree.get_tree("pages");
        tree.nodes().into_iter().find(|n| {
            !matches!(tree.is_node_deleted(n), Ok(true))
                && tree
                    .get_meta(*n)
                    .ok()
                    .and_then(|m| string(&m, "id"))
                    .as_deref()
                    == Some(id)
        })
    }

    fn doc(&self, id: &str) -> Result<&LoroDoc, Error> {
        self.pages.get(id).ok_or_else(|| Error::NotFound(id.into()))
    }

    /// Whether a page exists.
    pub fn has_page(&self, id: &str) -> bool {
        self.pages.contains_key(id)
    }

    /// Creates a page and an empty document for it.
    pub fn create_page(
        &mut self,
        id: &str,
        parent: Option<&str>,
        title: &str,
        database: Option<&str>,
    ) -> Result<(), Error> {
        if self.node(id).is_some() {
            return Err(Error::Duplicate(id.into()));
        }
        let tree = self.tree.get_tree("pages");
        let parent_node = match parent {
            Some(p) => Some(self.node(p).ok_or_else(|| Error::NotFound(p.into()))?),
            None => None,
        };
        let node = tree.create(parent_node).map_err(bad)?;
        let meta = tree.get_meta(node).map_err(bad)?;
        meta.insert("id", id).map_err(bad)?;
        meta.insert("title", title).map_err(bad)?;
        meta.insert("favorite", false).map_err(bad)?;
        meta.insert("trashed", false).map_err(bad)?;
        if let Some(d) = database {
            meta.insert("database", d).map_err(bad)?;
        }
        self.tree.commit();
        self.pages.insert(id.into(), new_doc(self.peer)?);
        Ok(())
    }

    /// Sets a page's title.
    pub fn set_title(&mut self, id: &str, title: &str) -> Result<String, Error> {
        self.set_meta_string(id, "title", title)
    }

    fn set_meta_string(&mut self, id: &str, key: &str, value: &str) -> Result<String, Error> {
        let node = self.node(id).ok_or_else(|| Error::NotFound(id.into()))?;
        let meta = self.tree.get_tree("pages").get_meta(node).map_err(bad)?;
        let old = string(&meta, key).unwrap_or_default();
        meta.insert(key, value).map_err(bad)?;
        self.tree.commit();
        Ok(old)
    }

    /// Sets a page flag, `favorite` or `trashed`, and returns its old value.
    pub fn set_flag(&mut self, id: &str, key: &str, value: bool) -> Result<bool, Error> {
        let node = self.node(id).ok_or_else(|| Error::NotFound(id.into()))?;
        let meta = self.tree.get_tree("pages").get_meta(node).map_err(bad)?;
        let old = boolean(&meta, key);
        meta.insert(key, value).map_err(bad)?;
        self.tree.commit();
        Ok(old)
    }

    /// Moves a page under a new parent, and returns the old one. A move into its own subtree is refused.
    pub fn move_page(&mut self, id: &str, parent: Option<&str>) -> Result<Option<String>, Error> {
        let old = self.meta(id)?.parent;
        let tree = self.tree.get_tree("pages");
        let node = self.node(id).ok_or_else(|| Error::NotFound(id.into()))?;
        let target = match parent {
            Some(p) => Some(self.node(p).ok_or_else(|| Error::NotFound(p.into()))?),
            None => None,
        };
        tree.mov(node, target)
            .map_err(|e| Error::BadMove(format!("{id}: {e:?}")))?;
        self.tree.commit();
        Ok(old)
    }

    /// One page's metadata.
    pub fn meta(&self, id: &str) -> Result<PageMeta, Error> {
        let node = self.node(id).ok_or_else(|| Error::NotFound(id.into()))?;
        self.meta_of(node).ok_or_else(|| Error::NotFound(id.into()))
    }

    fn meta_of(&self, node: TreeID) -> Option<PageMeta> {
        let tree = self.tree.get_tree("pages");
        let meta = tree.get_meta(node).ok()?;
        let parent = tree.parent(node).and_then(|p| match p {
            loro::TreeParentId::Node(n) => tree.get_meta(n).ok().and_then(|m| string(&m, "id")),
            _ => None,
        });
        Some(PageMeta {
            id: string(&meta, "id")?,
            parent,
            title: string(&meta, "title").unwrap_or_default(),
            favorite: boolean(&meta, "favorite"),
            trashed: boolean(&meta, "trashed"),
            database: string(&meta, "database"),
        })
    }

    /// Every page, parents first, in tree order, so all replicas list them the same way.
    pub fn pages(&self) -> Vec<PageMeta> {
        let tree = self.tree.get_tree("pages");
        let mut out = Vec::new();
        let mut stack: Vec<TreeID> = tree.roots();
        stack.reverse();
        while let Some(node) = stack.pop() {
            if let Some(meta) = self.meta_of(node) {
                out.push(meta);
            }
            if let Some(mut kids) = tree.children(node) {
                kids.reverse();
                stack.extend(kids);
            }
        }
        out
    }

    /// A whole page, read from merged state. A listed block with no record is skipped, and
    /// a block moved concurrently appears once.
    pub fn page(&self, id: &str) -> Result<Page, Error> {
        let meta = self.meta(id)?;
        let mut blocks = Vec::new();
        if let Some(doc) = self.pages.get(id) {
            let order = doc.get_movable_list("order");
            let records = doc.get_map("blocks");
            let mut seen = std::collections::BTreeSet::new();
            for i in 0..order.len() {
                let Some(block_id) = order.get(i).and_then(|v| value_string(&v)) else {
                    continue;
                };
                if !seen.insert(block_id.clone()) {
                    continue;
                }
                let Some(ValueOrContainer::Container(loro::Container::Map(record))) =
                    records.get(&block_id)
                else {
                    continue;
                };
                blocks.push(read_block(&block_id, &record));
            }
        }
        let (view, canvas) = match self.pages.get(id) {
            Some(doc) => (
                string(&doc.get_map("page"), "view").unwrap_or_else(|| VIEW_PAGE.into()),
                read_canvas(doc),
            ),
            None => (VIEW_PAGE.into(), Canvas::default()),
        };
        Ok(Page {
            meta,
            blocks,
            view,
            canvas,
        })
    }

    fn record(&self, page: &str, block: &str) -> Result<LoroMap, Error> {
        match self.doc(page)?.get_map("blocks").get(block) {
            Some(ValueOrContainer::Container(loro::Container::Map(m))) => Ok(m),
            _ => Err(Error::NotFound(block.into())),
        }
    }

    fn text(&self, page: &str, block: &str) -> Result<LoroText, Error> {
        match self.record(page, block)?.get("text") {
            Some(ValueOrContainer::Container(loro::Container::Text(t))) => Ok(t),
            _ => Err(Error::NotFound(block.into())),
        }
    }

    fn index(&self, page: &str, block: &str) -> Result<Option<usize>, Error> {
        let order = self.doc(page)?.get_movable_list("order");
        Ok((0..order.len())
            .find(|i| order.get(*i).and_then(|v| value_string(&v)).as_deref() == Some(block)))
    }

    /// Inserts a block after `after`, or first when `after` is `None`.
    pub fn insert_block(
        &mut self,
        page: &str,
        after: Option<&str>,
        block: &Block,
    ) -> Result<(), Error> {
        let doc = self.doc(page)?.clone();
        let records = doc.get_map("blocks");
        if records.get(&block.id).is_some() && self.index(page, &block.id)?.is_some() {
            return Err(Error::Duplicate(block.id.clone()));
        }
        let at = match after {
            Some(a) => {
                self.index(page, a)?
                    .ok_or_else(|| Error::NotFound(a.into()))?
                    + 1
            }
            None => 0,
        };
        let record = records
            .insert_container(&block.id, LoroMap::new())
            .map_err(bad)?;
        record.insert("kind", block.kind.as_str()).map_err(bad)?;
        record.insert("checked", block.checked).map_err(bad)?;
        if let Some(target) = &block.target {
            record.insert("target", target.as_str()).map_err(bad)?;
        }
        let text = record
            .insert_container("text", LoroText::new())
            .map_err(bad)?;
        write_runs(&text, &block.runs)?;
        doc.get_movable_list("order")
            .insert(at, block.id.as_str())
            .map_err(bad)?;
        doc.commit();
        Ok(())
    }

    /// Removes a block, and returns it with the block before it, so it can be put back.
    pub fn delete_block(
        &mut self,
        page: &str,
        block: &str,
    ) -> Result<(Block, Option<String>), Error> {
        let doc = self.doc(page)?.clone();
        let index = self
            .index(page, block)?
            .ok_or_else(|| Error::NotFound(block.into()))?;
        let order = doc.get_movable_list("order");
        let after = if index == 0 {
            None
        } else {
            order.get(index - 1).and_then(|v| value_string(&v))
        };
        let old = read_block(block, &self.record(page, block)?);
        order.delete(index, 1).map_err(bad)?;
        doc.commit();
        Ok((old, after))
    }

    /// Moves a block to just after `after`, or first, and returns where it was.
    pub fn move_block(
        &mut self,
        page: &str,
        block: &str,
        after: Option<&str>,
    ) -> Result<Option<String>, Error> {
        let doc = self.doc(page)?.clone();
        let from = self
            .index(page, block)?
            .ok_or_else(|| Error::NotFound(block.into()))?;
        let order = doc.get_movable_list("order");
        let old = if from == 0 {
            None
        } else {
            order.get(from - 1).and_then(|v| value_string(&v))
        };
        let anchor = match after {
            Some(a) => Some(
                self.index(page, a)?
                    .ok_or_else(|| Error::NotFound(a.into()))?,
            ),
            None => None,
        };
        let to = match anchor {
            None => 0,
            Some(a) if a < from => a + 1,
            Some(a) => a,
        };
        if to != from {
            order.mov(from, to).map_err(bad)?;
            doc.commit();
        }
        Ok(old)
    }

    /// Replaces `delete` UTF-16 units at `at` with `insert`, and returns what was removed.
    pub fn splice_text(
        &mut self,
        page: &str,
        block: &str,
        at: usize,
        delete: usize,
        insert: &str,
    ) -> Result<String, Error> {
        let text = self.text(page, block)?;
        let end = at
            .checked_add(delete)
            .ok_or_else(|| Error::BadRange(block.into()))?;
        if end > text.len_utf16() {
            return Err(Error::BadRange(block.into()));
        }
        let removed = text
            .slice_utf16(at, end)
            .map_err(|_| Error::BadRange(block.into()))?;
        text.splice_utf16(at, delete, insert)
            .map_err(|_| Error::BadRange(block.into()))?;
        self.doc(page)?.commit();
        Ok(removed)
    }

    /// Sets or clears a mark over a UTF-16 range.
    pub fn set_mark(
        &mut self,
        page: &str,
        block: &str,
        start: usize,
        end: usize,
        mark: Mark,
        on: bool,
    ) -> Result<(), Error> {
        let text = self.text(page, block)?;
        if start >= end || end > text.len_utf16() {
            return Err(Error::BadRange(block.into()));
        }
        let s = text.to_string();
        let (a, b) = (unicode_index(&s, start), unicode_index(&s, end));
        let (Some(a), Some(b)) = (a, b) else {
            return Err(Error::BadRange(block.into()));
        };
        if on {
            text.mark(a..b, mark.as_str(), true).map_err(bad)?;
        } else {
            text.unmark(a..b, mark.as_str()).map_err(bad)?;
        }
        self.doc(page)?.commit();
        Ok(())
    }

    /// Sets a block's kind and checked state, and returns the old ones.
    pub fn set_block(
        &mut self,
        page: &str,
        block: &str,
        kind: BlockKind,
        checked: bool,
    ) -> Result<(BlockKind, bool), Error> {
        let record = self.record(page, block)?;
        let old = (
            BlockKind::parse(&string(&record, "kind").unwrap_or_default()),
            boolean(&record, "checked"),
        );
        record.insert("kind", kind.as_str()).map_err(bad)?;
        record.insert("checked", checked).map_err(bad)?;
        self.doc(page)?.commit();
        Ok(old)
    }

    /// Sets or clears a link over a UTF-16 range.
    pub fn set_link(
        &mut self,
        page: &str,
        block: &str,
        start: usize,
        end: usize,
        target: Option<&str>,
    ) -> Result<(), Error> {
        let text = self.text(page, block)?;
        if start >= end || end > text.len_utf16() {
            return Err(Error::BadRange(block.into()));
        }
        let s = text.to_string();
        let (Some(a), Some(b)) = (unicode_index(&s, start), unicode_index(&s, end)) else {
            return Err(Error::BadRange(block.into()));
        };
        match target {
            Some(t) => text.mark(a..b, "link", t).map_err(bad)?,
            None => text.unmark(a..b, "link").map_err(bad)?,
        }
        self.doc(page)?.commit();
        Ok(())
    }

    /// Sets whether a page shows as a document or a canvas, and returns the old view.
    pub fn set_view(&mut self, page: &str, view: &str) -> Result<String, Error> {
        let doc = self.doc(page)?.clone();
        let map = doc.get_map("page");
        let old = string(&map, "view").unwrap_or_else(|| VIEW_PAGE.into());
        map.insert("view", view).map_err(bad)?;
        doc.commit();
        Ok(old)
    }

    /// Moves or resizes the page's note card, and returns the old placement.
    pub fn set_note(&mut self, page: &str, note: &Note) -> Result<Note, Error> {
        check_coord(&[note.x, note.y, note.width])?;
        let doc = self.doc(page)?.clone();
        let old = read_canvas(&doc).note;
        let map = doc.get_map("note");
        map.insert("x", note.x).map_err(bad)?;
        map.insert("y", note.y).map_err(bad)?;
        map.insert("width", note.width).map_err(bad)?;
        doc.commit();
        Ok(old)
    }

    /// Adds or replaces a canvas item, and returns what was there.
    pub fn put_item(&mut self, page: &str, item: &CanvasItem) -> Result<Option<CanvasItem>, Error> {
        check_coord(&[item.x, item.y, item.width, item.height])?;
        let doc = self.doc(page)?.clone();
        let old = read_canvas(&doc)
            .items
            .into_iter()
            .find(|i| i.id == item.id);
        let items = doc.get_map("items");
        if old.is_none() {
            doc.get_list("item_order")
                .push(item.id.as_str())
                .map_err(bad)?;
        }
        let record = match child_map(&items, &item.id) {
            Some(m) => m,
            None => items
                .insert_container(&item.id, LoroMap::new())
                .map_err(bad)?,
        };
        record.insert("kind", item.kind.as_str()).map_err(bad)?;
        record.insert("x", item.x).map_err(bad)?;
        record.insert("y", item.y).map_err(bad)?;
        record.insert("width", item.width).map_err(bad)?;
        record.insert("height", item.height).map_err(bad)?;
        record.insert("color", item.color.as_str()).map_err(bad)?;
        record.insert("text", item.text.as_str()).map_err(bad)?;
        doc.commit();
        Ok(old)
    }

    /// Removes a canvas item, and returns it.
    pub fn delete_item(&mut self, page: &str, id: &str) -> Result<CanvasItem, Error> {
        let doc = self.doc(page)?.clone();
        let old = read_canvas(&doc)
            .items
            .into_iter()
            .find(|i| i.id == id)
            .ok_or_else(|| Error::NotFound(id.into()))?;
        doc.get_map("items").delete(id).map_err(bad)?;
        remove_from_list(&doc.get_list("item_order"), id)?;
        doc.commit();
        Ok(old)
    }

    /// Adds strokes and removes others in one step, which is how a partial erase is recorded.
    pub fn change_ink(
        &mut self,
        page: &str,
        add: &[Ink],
        remove: &[String],
    ) -> Result<Vec<Ink>, Error> {
        let doc = self.doc(page)?.clone();
        let current = read_canvas(&doc).ink;
        let ink = doc.get_map("ink");
        let order = doc.get_list("ink_order");
        let mut encoded = Vec::with_capacity(add.len());
        for stroke in add {
            encoded.push(encode_samples(&stroke.points)?);
        }
        let mut removed = Vec::new();
        for id in remove {
            let old = current
                .iter()
                .find(|s| &s.id == id)
                .ok_or_else(|| Error::NotFound(id.clone()))?;
            removed.push(old.clone());
            ink.delete(id).map_err(bad)?;
            remove_from_list(&order, id)?;
        }
        for (stroke, samples) in add.iter().zip(encoded) {
            let record = ink
                .insert_container(&stroke.id, LoroMap::new())
                .map_err(bad)?;
            record.insert("kind", stroke.kind.as_str()).map_err(bad)?;
            record.insert("color", stroke.color.as_str()).map_err(bad)?;
            record.insert("width", stroke.width).map_err(bad)?;
            record.insert("samples", samples).map_err(bad)?;
            order.push(stroke.id.as_str()).map_err(bad)?;
        }
        doc.commit();
        Ok(removed)
    }

    fn db(&self, id: &str) -> Result<&LoroDoc, Error> {
        self.databases
            .get(id)
            .ok_or_else(|| Error::NotFound(id.into()))
    }

    /// Creates a database.
    pub fn create_database(&mut self, db: &Database) -> Result<(), Error> {
        if self.databases.contains_key(&db.id) {
            return Err(Error::Duplicate(db.id.clone()));
        }
        let doc = new_doc(self.peer)?;
        let meta = doc.get_map("meta");
        meta.insert("name", db.name.as_str()).map_err(bad)?;
        meta.insert("view", db.view.as_str()).map_err(bad)?;
        let statuses = doc.get_map("statuses");
        let order = doc.get_movable_list("status_order");
        for s in &db.statuses {
            let record = statuses
                .insert_container(&s.id, LoroMap::new())
                .map_err(bad)?;
            record.insert("name", s.name.as_str()).map_err(bad)?;
            record.insert("group", s.group.as_str()).map_err(bad)?;
            order.push(s.id.as_str()).map_err(bad)?;
        }
        doc.commit();
        self.databases.insert(db.id.clone(), doc);
        for c in &db.columns {
            self.add_column(&db.id, c)?;
        }
        let mut previous: Option<&str> = None;
        for r in &db.rows {
            self.add_row(&db.id, previous, r)?;
            previous = Some(&r.id);
        }
        Ok(())
    }

    /// Every database id.
    pub fn database_ids(&self) -> Vec<String> {
        self.databases.keys().cloned().collect()
    }

    /// A whole database, read from merged state.
    pub fn database(&self, id: &str) -> Result<Database, Error> {
        let doc = self.db(id)?;
        let meta = doc.get_map("meta");
        let statuses_map = doc.get_map("statuses");
        let statuses = ordered(&doc.get_movable_list("status_order"))
            .into_iter()
            .filter_map(|sid| {
                let r = child_map(&statuses_map, &sid)?;
                Some(Status {
                    name: string(&r, "name").unwrap_or_default(),
                    group: string(&r, "group").unwrap_or_else(|| "todo".into()),
                    id: sid,
                })
            })
            .collect();
        let columns_map = doc.get_map("columns");
        let columns = ordered(&doc.get_movable_list("column_order"))
            .into_iter()
            .filter_map(|cid| {
                let r = child_map(&columns_map, &cid)?;
                Some(Column {
                    name: string(&r, "name").unwrap_or_default(),
                    kind: string(&r, "kind").unwrap_or_else(|| "text".into()),
                    hidden: boolean(&r, "hidden"),
                    id: cid,
                })
            })
            .collect();
        let rows_map = doc.get_map("rows");
        let rows = ordered(&doc.get_movable_list("row_order"))
            .into_iter()
            .filter_map(|rid| {
                let r = child_map(&rows_map, &rid)?;
                let mut values = BTreeMap::new();
                if let Some(cells) = child_map(&r, "values") {
                    cells.for_each(|k, v| {
                        if let Some(text) = value_string(&v) {
                            values.insert(k.to_string(), text);
                        }
                    });
                }
                Some(Row {
                    name: string(&r, "name").unwrap_or_default(),
                    status: string(&r, "status").unwrap_or_default(),
                    values,
                    id: rid,
                })
            })
            .collect();
        Ok(Database {
            id: id.into(),
            name: string(&meta, "name").unwrap_or_default(),
            view: string(&meta, "view").unwrap_or_else(|| "table".into()),
            statuses,
            columns,
            rows,
        })
    }

    /// Sets a database's `name` or `view`, and returns the old value.
    pub fn set_database_field(
        &mut self,
        id: &str,
        key: &str,
        value: &str,
    ) -> Result<String, Error> {
        let doc = self.db(id)?.clone();
        let meta = doc.get_map("meta");
        let old = string(&meta, key).unwrap_or_default();
        meta.insert(key, value).map_err(bad)?;
        doc.commit();
        Ok(old)
    }

    /// Adds a column.
    pub fn add_column(&mut self, id: &str, column: &Column) -> Result<(), Error> {
        let doc = self.db(id)?.clone();
        let record = doc
            .get_map("columns")
            .insert_container(&column.id, LoroMap::new())
            .map_err(bad)?;
        record.insert("name", column.name.as_str()).map_err(bad)?;
        record.insert("kind", column.kind.as_str()).map_err(bad)?;
        record.insert("hidden", column.hidden).map_err(bad)?;
        doc.get_movable_list("column_order")
            .push(column.id.as_str())
            .map_err(bad)?;
        doc.commit();
        Ok(())
    }

    /// Hides or restores a column, and returns the old state. Its cells always stay.
    pub fn set_column_hidden(
        &mut self,
        id: &str,
        column: &str,
        hidden: bool,
    ) -> Result<bool, Error> {
        let doc = self.db(id)?.clone();
        let record = child_map(&doc.get_map("columns"), column)
            .ok_or_else(|| Error::NotFound(column.into()))?;
        let old = boolean(&record, "hidden");
        record.insert("hidden", hidden).map_err(bad)?;
        doc.commit();
        Ok(old)
    }

    /// Adds a row after `after`, or first when `after` is `None`.
    pub fn add_row(&mut self, id: &str, after: Option<&str>, row: &Row) -> Result<(), Error> {
        let doc = self.db(id)?.clone();
        let record = doc
            .get_map("rows")
            .insert_container(&row.id, LoroMap::new())
            .map_err(bad)?;
        record.insert("name", row.name.as_str()).map_err(bad)?;
        record.insert("status", row.status.as_str()).map_err(bad)?;
        let cells = record
            .insert_container("values", LoroMap::new())
            .map_err(bad)?;
        for (k, v) in &row.values {
            cells.insert(k, v.as_str()).map_err(bad)?;
        }
        let order = doc.get_movable_list("row_order");
        let at = match after {
            Some(a) => ordered(&order)
                .iter()
                .position(|x| x == a)
                .map_or(order.len(), |i| i + 1),
            None => 0,
        };
        order.insert(at, row.id.as_str()).map_err(bad)?;
        doc.commit();
        Ok(())
    }

    /// Removes a row, and returns it with the row before it.
    pub fn delete_row(&mut self, id: &str, row: &str) -> Result<(Row, Option<String>), Error> {
        let old = self
            .database(id)?
            .rows
            .into_iter()
            .find(|r| r.id == row)
            .ok_or_else(|| Error::NotFound(row.into()))?;
        let doc = self.db(id)?.clone();
        let order = doc.get_movable_list("row_order");
        let list = ordered(&order);
        let at = list
            .iter()
            .position(|x| x == row)
            .ok_or_else(|| Error::NotFound(row.into()))?;
        order.delete(at, 1).map_err(bad)?;
        doc.commit();
        Ok((
            old,
            if at == 0 {
                None
            } else {
                list.get(at - 1).cloned()
            },
        ))
    }

    /// Sets a row's `name` or `status`, and returns the old value.
    pub fn set_row_field(
        &mut self,
        id: &str,
        row: &str,
        key: &str,
        value: &str,
    ) -> Result<String, Error> {
        let doc = self.db(id)?.clone();
        let record =
            child_map(&doc.get_map("rows"), row).ok_or_else(|| Error::NotFound(row.into()))?;
        let old = string(&record, key).unwrap_or_default();
        record.insert(key, value).map_err(bad)?;
        doc.commit();
        Ok(old)
    }

    /// Sets a cell, and returns the old value. Concurrent edits to different cells both survive.
    pub fn set_cell(
        &mut self,
        id: &str,
        row: &str,
        column: &str,
        value: &str,
    ) -> Result<String, Error> {
        let doc = self.db(id)?.clone();
        let record =
            child_map(&doc.get_map("rows"), row).ok_or_else(|| Error::NotFound(row.into()))?;
        let cells = child_map(&record, "values").ok_or_else(|| Error::NotFound(row.into()))?;
        let old = string(&cells, column).unwrap_or_default();
        cells.insert(column, value).map_err(bad)?;
        doc.commit();
        Ok(old)
    }

    /// Everything, as one bounded container: the form stored on disk and sent between replicas.
    pub fn export(&self) -> Result<Vec<u8>, Error> {
        let mut out = Vec::from(&MAGIC[..]);
        let mut put = |name: &str, doc: &LoroDoc| -> Result<(), Error> {
            let bytes = doc.export(ExportMode::Snapshot).map_err(bad)?;
            out.extend_from_slice(&u32::try_from(name.len()).map_err(bad)?.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&u64::try_from(bytes.len()).map_err(bad)?.to_le_bytes());
            out.extend_from_slice(&bytes);
            Ok(())
        };
        put("", &self.tree)?;
        for (id, doc) in &self.pages {
            put(id, doc)?;
        }
        for (id, doc) in &self.databases {
            put(&format!("d:{id}"), doc)?;
        }
        Ok(out)
    }

    /// Merges a container from `export`. Hostile bytes give an error, never a panic, and
    /// nothing is applied unless the whole container parses.
    pub fn import(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let mut rest = bytes
            .strip_prefix(&MAGIC[..])
            .ok_or_else(|| Error::BadData("not a workspace".into()))?;
        let mut parts: Vec<(String, &[u8])> = Vec::new();
        while !rest.is_empty() {
            if parts.len() >= MAX_DOCS {
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
        for (name, body) in parts {
            if name.is_empty() {
                self.tree.import(body).map_err(bad)?;
            } else if let Some(id) = name.strip_prefix("d:") {
                if !self.databases.contains_key(id) {
                    self.databases.insert(id.to_owned(), new_doc(self.peer)?);
                }
                if let Some(doc) = self.databases.get(id) {
                    doc.import(body).map_err(bad)?;
                }
            } else {
                if !self.pages.contains_key(&name) {
                    self.pages.insert(name.clone(), new_doc(self.peer)?);
                }
                self.doc(&name)?.import(body).map_err(bad)?;
            }
        }
        // A page known to the tree always has a document, even if its edits have not arrived.
        for meta in self.pages() {
            if !self.pages.contains_key(&meta.id) {
                self.pages.insert(meta.id, new_doc(self.peer)?);
            }
        }
        Ok(())
    }
}

fn value_string(v: &ValueOrContainer) -> Option<String> {
    match v {
        ValueOrContainer::Value(LoroValue::String(s)) => Some(s.to_string()),
        _ => None,
    }
}

fn string(map: &LoroMap, key: &str) -> Option<String> {
    map.get(key).as_ref().and_then(value_string)
}

fn boolean(map: &LoroMap, key: &str) -> bool {
    matches!(
        map.get(key),
        Some(ValueOrContainer::Value(LoroValue::Bool(true)))
    )
}

fn read_block(id: &str, record: &LoroMap) -> Block {
    let mut runs = Vec::new();
    if let Some(ValueOrContainer::Container(loro::Container::Text(text))) = record.get("text") {
        for delta in text.to_delta() {
            if let TextDelta::Insert { insert, attributes } = delta {
                let marks = Mark::ALL
                    .into_iter()
                    .filter(|m| {
                        matches!(
                            attributes.as_ref().and_then(|a| a.get(m.as_str())),
                            Some(LoroValue::Bool(true))
                        )
                    })
                    .collect();
                let link = attributes.as_ref().and_then(|a| match a.get("link") {
                    Some(LoroValue::String(s)) => Some(s.to_string()),
                    _ => None,
                });
                runs.push(Run {
                    text: insert,
                    marks,
                    link,
                });
            }
        }
    }
    Block {
        id: id.into(),
        kind: BlockKind::parse(&string(record, "kind").unwrap_or_default()),
        checked: boolean(record, "checked"),
        runs,
        target: string(record, "target"),
    }
}

fn write_runs(text: &LoroText, runs: &[Run]) -> Result<(), Error> {
    let mut at = 0;
    for run in runs {
        text.insert(at, &run.text).map_err(bad)?;
        let end = at + run.text.chars().count();
        if at < end {
            for mark in &run.marks {
                text.mark(at..end, mark.as_str(), true).map_err(bad)?;
            }
            if let Some(link) = &run.link {
                text.mark(at..end, "link", link.as_str()).map_err(bad)?;
            }
        }
        at = end;
    }
    Ok(())
}

/// The character index at a UTF-16 offset, or `None` inside a surrogate pair.
fn unicode_index(s: &str, utf16: usize) -> Option<usize> {
    let mut units = 0;
    for (i, c) in s.chars().enumerate() {
        if units == utf16 {
            return Some(i);
        }
        units += c.len_utf16();
        if units > utf16 {
            return None;
        }
    }
    (units == utf16).then(|| s.chars().count())
}

fn check_coord(values: &[f64]) -> Result<(), Error> {
    if values.iter().all(|v| v.is_finite() && v.abs() <= 1e9) {
        Ok(())
    } else {
        Err(Error::BadRange("a coordinate is out of range".into()))
    }
}

fn number(map: &LoroMap, key: &str, default: f64) -> f64 {
    match map.get(key) {
        Some(ValueOrContainer::Value(LoroValue::Double(v))) if v.is_finite() => v,
        Some(ValueOrContainer::Value(LoroValue::I64(v))) => v as f64,
        _ => default,
    }
}

fn child_map(map: &LoroMap, key: &str) -> Option<LoroMap> {
    match map.get(key) {
        Some(ValueOrContainer::Container(loro::Container::Map(m))) => Some(m),
        _ => None,
    }
}

/// A movable list's ids, each once, in order.
fn ordered(list: &LoroMovableList) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    (0..list.len())
        .filter_map(|i| list.get(i).as_ref().and_then(value_string))
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn list_ids(list: &loro::LoroList) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    (0..list.len())
        .filter_map(|i| list.get(i).as_ref().and_then(value_string))
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn remove_from_list(list: &loro::LoroList, id: &str) -> Result<(), Error> {
    let hits: Vec<usize> = (0..list.len())
        .filter(|i| list.get(*i).as_ref().and_then(value_string).as_deref() == Some(id))
        .collect();
    for i in hits.into_iter().rev() {
        list.delete(i, 1).map_err(bad)?;
    }
    Ok(())
}

fn read_canvas(doc: &LoroDoc) -> Canvas {
    let note_map = doc.get_map("note");
    let d = Note::default();
    let note = Note {
        x: number(&note_map, "x", d.x),
        y: number(&note_map, "y", d.y),
        width: number(&note_map, "width", d.width),
    };
    let items_map = doc.get_map("items");
    let items = list_ids(&doc.get_list("item_order"))
        .into_iter()
        .filter_map(|id| {
            let r = child_map(&items_map, &id)?;
            Some(CanvasItem {
                kind: string(&r, "kind").unwrap_or_else(|| "text".into()),
                x: number(&r, "x", 0.0),
                y: number(&r, "y", 0.0),
                width: number(&r, "width", 170.0),
                height: number(&r, "height", 100.0),
                color: string(&r, "color").unwrap_or_default(),
                text: string(&r, "text").unwrap_or_default(),
                id,
            })
        })
        .collect();
    let ink_map = doc.get_map("ink");
    let ink = list_ids(&doc.get_list("ink_order"))
        .into_iter()
        .filter_map(|id| {
            let r = child_map(&ink_map, &id)?;
            let points = match r.get("samples") {
                Some(ValueOrContainer::Value(LoroValue::Binary(b))) => decode_samples(&b).ok()?,
                _ => return None,
            };
            Some(Ink {
                kind: string(&r, "kind").unwrap_or_else(|| "pen".into()),
                color: string(&r, "color").unwrap_or_default(),
                width: number(&r, "width", 3.0),
                points,
                id,
            })
        })
        .collect();
    Canvas { note, items, ink }
}
