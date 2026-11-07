//! The closed set of edits: the only write path. Every op validates and returns its inverse, so
//! undo is replaying inverses. Merging is the CRDT's job, and there is no transform (ADR-005).

use casual_note_merge::Workspace;
use casual_note_model::{
    Block, BlockKind, CanvasItem, Column, Database, Error, Ink, Mark, Note, Row, Run,
};
use serde::{Deserialize, Serialize};

/// One edit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    CreatePage {
        id: String,
        parent: Option<String>,
        title: String,
        #[serde(default)]
        database: Option<String>,
    },
    SetTitle {
        id: String,
        title: String,
    },
    SetFavorite {
        id: String,
        value: bool,
    },
    SetTrashed {
        id: String,
        value: bool,
    },
    MovePage {
        id: String,
        parent: Option<String>,
    },
    InsertBlock {
        page: String,
        after: Option<String>,
        block: Block,
    },
    DeleteBlock {
        page: String,
        block: String,
    },
    MoveBlock {
        page: String,
        block: String,
        after: Option<String>,
    },
    SpliceText {
        page: String,
        block: String,
        at: usize,
        delete: usize,
        insert: String,
    },
    SetMark {
        page: String,
        block: String,
        start: usize,
        end: usize,
        mark: Mark,
        on: bool,
    },
    SetLink {
        page: String,
        block: String,
        start: usize,
        end: usize,
        target: Option<String>,
    },
    SetView {
        page: String,
        view: String,
    },
    SetNote {
        page: String,
        note: Note,
    },
    PutItem {
        page: String,
        item: CanvasItem,
    },
    DeleteItem {
        page: String,
        id: String,
    },
    ChangeInk {
        page: String,
        #[serde(default)]
        add: Vec<Ink>,
        #[serde(default)]
        remove: Vec<String>,
    },
    CreateDatabase {
        database: Database,
    },
    SetDatabaseField {
        id: String,
        key: String,
        value: String,
    },
    AddColumn {
        id: String,
        column: Column,
    },
    SetColumnName {
        id: String,
        column: String,
        name: String,
    },
    SetColumnHidden {
        id: String,
        column: String,
        hidden: bool,
    },
    AddRow {
        id: String,
        after: Option<String>,
        row: Row,
    },
    DeleteRow {
        id: String,
        row: String,
    },
    SetRowField {
        id: String,
        row: String,
        key: String,
        value: String,
    },
    SetCell {
        id: String,
        row: String,
        column: String,
        value: String,
    },
    SetBlock {
        page: String,
        block: String,
        kind: BlockKind,
        checked: bool,
    },
}

/// Applies an op, and returns the op that undoes it.
pub fn apply(ws: &mut Workspace, op: &Op) -> Result<Op, Error> {
    Ok(match op {
        Op::CreatePage {
            id,
            parent,
            title,
            database,
        } => {
            ws.create_page(id, parent.as_deref(), title, database.as_deref())?;
            Op::SetTrashed {
                id: id.clone(),
                value: true,
            }
        }
        Op::SetTitle { id, title } => Op::SetTitle {
            id: id.clone(),
            title: ws.set_title(id, title)?,
        },
        Op::SetFavorite { id, value } => Op::SetFavorite {
            id: id.clone(),
            value: ws.set_flag(id, "favorite", *value)?,
        },
        Op::SetTrashed { id, value } => Op::SetTrashed {
            id: id.clone(),
            value: ws.set_flag(id, "trashed", *value)?,
        },
        Op::MovePage { id, parent } => Op::MovePage {
            id: id.clone(),
            parent: ws.move_page(id, parent.as_deref())?,
        },
        Op::InsertBlock { page, after, block } => {
            ws.insert_block(page, after.as_deref(), block)?;
            Op::DeleteBlock {
                page: page.clone(),
                block: block.id.clone(),
            }
        }
        Op::DeleteBlock { page, block } => {
            let (old, after) = ws.delete_block(page, block)?;
            Op::InsertBlock {
                page: page.clone(),
                after,
                block: old,
            }
        }
        Op::MoveBlock { page, block, after } => {
            let old = ws.move_block(page, block, after.as_deref())?;
            Op::MoveBlock {
                page: page.clone(),
                block: block.clone(),
                after: old,
            }
        }
        Op::SpliceText {
            page,
            block,
            at,
            delete,
            insert,
        } => {
            let removed = ws.splice_text(page, block, *at, *delete, insert)?;
            Op::SpliceText {
                page: page.clone(),
                block: block.clone(),
                at: *at,
                delete: insert.encode_utf16().count(),
                insert: removed,
            }
        }
        Op::SetMark {
            page,
            block,
            start,
            end,
            mark,
            on,
        } => {
            ws.set_mark(page, block, *start, *end, *mark, *on)?;
            Op::SetMark {
                page: page.clone(),
                block: block.clone(),
                start: *start,
                end: *end,
                mark: *mark,
                on: !*on,
            }
        }
        Op::SetLink {
            page,
            block,
            start,
            end,
            target,
        } => {
            let old = ws.link_at(page, block, *start)?;
            ws.set_link(page, block, *start, *end, target.as_deref())?;
            Op::SetLink {
                page: page.clone(),
                block: block.clone(),
                start: *start,
                end: *end,
                target: old,
            }
        }
        Op::SetView { page, view } => Op::SetView {
            page: page.clone(),
            view: ws.set_view(page, view)?,
        },
        Op::SetNote { page, note } => Op::SetNote {
            page: page.clone(),
            note: ws.set_note(page, note)?,
        },
        Op::PutItem { page, item } => match ws.put_item(page, item)? {
            Some(old) => Op::PutItem {
                page: page.clone(),
                item: old,
            },
            None => Op::DeleteItem {
                page: page.clone(),
                id: item.id.clone(),
            },
        },
        Op::DeleteItem { page, id } => Op::PutItem {
            page: page.clone(),
            item: ws.delete_item(page, id)?,
        },
        Op::ChangeInk { page, add, remove } => {
            let removed = ws.change_ink(page, add, remove)?;
            Op::ChangeInk {
                page: page.clone(),
                add: removed,
                remove: add.iter().map(|s| s.id.clone()).collect(),
            }
        }
        Op::CreateDatabase { database } => {
            ws.create_database(database)?;
            Op::SetDatabaseField {
                id: database.id.clone(),
                key: "name".into(),
                value: database.name.clone(),
            }
        }
        Op::SetDatabaseField { id, key, value } => {
            if key != "name" && key != "view" {
                return Err(Error::BadRange(key.clone()));
            }
            Op::SetDatabaseField {
                id: id.clone(),
                key: key.clone(),
                value: ws.set_database_field(id, key, value)?,
            }
        }
        Op::AddColumn { id, column } => {
            ws.add_column(id, column)?;
            Op::SetColumnHidden {
                id: id.clone(),
                column: column.id.clone(),
                hidden: true,
            }
        }
        Op::SetColumnName { id, column, name } => Op::SetColumnName {
            id: id.clone(),
            column: column.clone(),
            name: ws.set_column_name(id, column, name)?,
        },
        Op::SetColumnHidden { id, column, hidden } => Op::SetColumnHidden {
            id: id.clone(),
            column: column.clone(),
            hidden: ws.set_column_hidden(id, column, *hidden)?,
        },
        Op::AddRow { id, after, row } => {
            ws.add_row(id, after.as_deref(), row)?;
            Op::DeleteRow {
                id: id.clone(),
                row: row.id.clone(),
            }
        }
        Op::DeleteRow { id, row } => {
            let (old, after) = ws.delete_row(id, row)?;
            Op::AddRow {
                id: id.clone(),
                after,
                row: old,
            }
        }
        Op::SetRowField {
            id,
            row,
            key,
            value,
        } => {
            if key != "name" && key != "status" {
                return Err(Error::BadRange(key.clone()));
            }
            Op::SetRowField {
                id: id.clone(),
                row: row.clone(),
                key: key.clone(),
                value: ws.set_row_field(id, row, key, value)?,
            }
        }
        Op::SetCell {
            id,
            row,
            column,
            value,
        } => Op::SetCell {
            id: id.clone(),
            row: row.clone(),
            column: column.clone(),
            value: ws.set_cell(id, row, column, value)?,
        },
        Op::SetBlock {
            page,
            block,
            kind,
            checked,
        } => {
            let (k, c) = ws.set_block(page, block, *kind, *checked)?;
            Op::SetBlock {
                page: page.clone(),
                block: block.clone(),
                kind: k,
                checked: c,
            }
        }
    })
}

/// A new paragraph block.
pub fn paragraph(id: &str, text: &str) -> Block {
    Block {
        id: id.into(),
        kind: BlockKind::Paragraph,
        checked: false,
        runs: if text.is_empty() {
            vec![]
        } else {
            vec![Run {
                text: text.into(),
                marks: vec![],
                link: None,
            }]
        },
        target: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded(peer: u64) -> Workspace {
        let mut ws = Workspace::new(peer).unwrap();
        apply(
            &mut ws,
            &Op::CreatePage {
                id: "p".into(),
                parent: None,
                title: "Notes".into(),
                database: None,
            },
        )
        .unwrap();
        apply(
            &mut ws,
            &Op::InsertBlock {
                page: "p".into(),
                after: None,
                block: paragraph("a", "hello"),
            },
        )
        .unwrap();
        ws
    }
    fn text(ws: &Workspace, b: &str) -> String {
        ws.page("p")
            .unwrap()
            .blocks
            .iter()
            .find(|x| x.id == b)
            .unwrap()
            .plain()
    }
    fn ids(ws: &Workspace) -> Vec<String> {
        ws.page("p")
            .unwrap()
            .blocks
            .into_iter()
            .map(|b| b.id)
            .collect()
    }
    fn splice(block: &str, at: usize, delete: usize, insert: &str) -> Op {
        Op::SpliceText {
            page: "p".into(),
            block: block.into(),
            at,
            delete,
            insert: insert.into(),
        }
    }
    fn sync(a: &mut Workspace, b: &mut Workspace) {
        let (x, y) = (a.export().unwrap(), b.export().unwrap());
        a.import(&y).unwrap();
        b.import(&x).unwrap();
    }

    #[test]
    fn every_edit_undoes_exactly() {
        let mut ws = seeded(1);
        let before = ws.page("p").unwrap();
        let ops = [
            splice("a", 5, 0, " world"),
            Op::SetBlock {
                page: "p".into(),
                block: "a".into(),
                kind: BlockKind::Todo,
                checked: true,
            },
            Op::InsertBlock {
                page: "p".into(),
                after: Some("a".into()),
                block: paragraph("b", "second"),
            },
            Op::MoveBlock {
                page: "p".into(),
                block: "b".into(),
                after: None,
            },
            Op::SetTitle {
                id: "p".into(),
                title: "Renamed".into(),
            },
            Op::SetFavorite {
                id: "p".into(),
                value: true,
            },
            Op::SetMark {
                page: "p".into(),
                block: "a".into(),
                start: 0,
                end: 5,
                mark: Mark::Bold,
                on: true,
            },
            Op::DeleteBlock {
                page: "p".into(),
                block: "b".into(),
            },
        ];
        let mut undo: Vec<Op> = ops.iter().map(|o| apply(&mut ws, o).unwrap()).collect();
        assert_ne!(ws.page("p").unwrap(), before);
        while let Some(inverse) = undo.pop() {
            apply(&mut ws, &inverse).unwrap();
        }
        assert_eq!(ws.page("p").unwrap(), before);
    }

    #[test]
    fn concurrent_edits_from_three_offline_devices_converge_and_keep_everything() {
        let mut a = seeded(1);
        let mut b = Workspace::new(2).unwrap();
        let mut c = Workspace::new(3).unwrap();
        b.import(&a.export().unwrap()).unwrap();
        c.import(&a.export().unwrap()).unwrap();

        apply(&mut a, &splice("a", 5, 0, " from a")).unwrap();
        apply(&mut b, &splice("a", 0, 0, "b says ")).unwrap();
        apply(
            &mut c,
            &Op::InsertBlock {
                page: "p".into(),
                after: Some("a".into()),
                block: paragraph("c", "new from c"),
            },
        )
        .unwrap();
        apply(
            &mut b,
            &Op::SetTitle {
                id: "p".into(),
                title: "Title from b".into(),
            },
        )
        .unwrap();

        sync(&mut a, &mut b);
        sync(&mut b, &mut c);
        sync(&mut a, &mut c);
        sync(&mut a, &mut b);

        assert_eq!(a.page("p").unwrap(), b.page("p").unwrap());
        assert_eq!(b.page("p").unwrap(), c.page("p").unwrap());
        assert_eq!(text(&a, "a"), "b says hello from a");
        assert_eq!(text(&a, "c"), "new from c");
        assert_eq!(a.meta("p").unwrap().title, "Title from b");
    }

    #[test]
    fn a_block_moved_on_two_devices_appears_once() {
        let mut a = seeded(1);
        apply(
            &mut a,
            &Op::InsertBlock {
                page: "p".into(),
                after: Some("a".into()),
                block: paragraph("b", "b"),
            },
        )
        .unwrap();
        apply(
            &mut a,
            &Op::InsertBlock {
                page: "p".into(),
                after: Some("b".into()),
                block: paragraph("c", "c"),
            },
        )
        .unwrap();
        let mut b = Workspace::new(2).unwrap();
        b.import(&a.export().unwrap()).unwrap();
        apply(
            &mut a,
            &Op::MoveBlock {
                page: "p".into(),
                block: "c".into(),
                after: None,
            },
        )
        .unwrap();
        apply(
            &mut b,
            &Op::MoveBlock {
                page: "p".into(),
                block: "c".into(),
                after: Some("a".into()),
            },
        )
        .unwrap();
        sync(&mut a, &mut b);
        assert_eq!(ids(&a), ids(&b));
        assert_eq!(ids(&a).iter().filter(|x| *x == "c").count(), 1);
        assert_eq!(ids(&a).len(), 3);
    }

    #[test]
    fn export_and_import_keep_every_script_exactly() {
        let mut a = seeded(1);
        let samples = [
            "مرحبا بالعالم",
            "नमस्ते दुनिया",
            "你好，世界",
            "こんにちは",
            "Café 🎉 cafe\u{301}",
        ];
        let mut after = "a".to_string();
        for (i, s) in samples.iter().enumerate() {
            let id = format!("s{i}");
            apply(
                &mut a,
                &Op::InsertBlock {
                    page: "p".into(),
                    after: Some(after.clone()),
                    block: paragraph(&id, s),
                },
            )
            .unwrap();
            after = id;
        }
        let mut b = Workspace::new(2).unwrap();
        b.import(&a.export().unwrap()).unwrap();
        assert_eq!(a.page("p").unwrap(), b.page("p").unwrap());
        for (i, s) in samples.iter().enumerate() {
            assert_eq!(&text(&b, &format!("s{i}")), s);
        }
    }

    #[test]
    fn utf16_positions_work_around_emoji_and_refuse_splitting_one() {
        let mut ws = seeded(1);
        apply(&mut ws, &splice("a", 0, 5, "a🎉b")).unwrap();
        assert_eq!(text(&ws, "a"), "a🎉b");
        apply(&mut ws, &splice("a", 3, 0, "X")).unwrap();
        assert_eq!(text(&ws, "a"), "a🎉Xb");
        let split = Op::SetMark {
            page: "p".into(),
            block: "a".into(),
            start: 2,
            end: 4,
            mark: Mark::Bold,
            on: true,
        };
        assert!(matches!(apply(&mut ws, &split), Err(Error::BadRange(_))));
        assert!(matches!(
            apply(&mut ws, &splice("a", 99, 1, "x")),
            Err(Error::BadRange(_))
        ));
    }

    #[test]
    fn marks_read_back_as_runs_and_merge() {
        let mut a = seeded(1);
        let mut b = Workspace::new(2).unwrap();
        b.import(&a.export().unwrap()).unwrap();
        apply(
            &mut a,
            &Op::SetMark {
                page: "p".into(),
                block: "a".into(),
                start: 0,
                end: 2,
                mark: Mark::Bold,
                on: true,
            },
        )
        .unwrap();
        apply(
            &mut b,
            &Op::SetMark {
                page: "p".into(),
                block: "a".into(),
                start: 3,
                end: 5,
                mark: Mark::Italic,
                on: true,
            },
        )
        .unwrap();
        sync(&mut a, &mut b);
        let page = a.page("p").unwrap();
        assert_eq!(
            page.blocks[0]
                .runs
                .iter()
                .map(|r| (r.text.as_str(), r.marks.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("he", vec![Mark::Bold]),
                ("l", vec![]),
                ("lo", vec![Mark::Italic])
            ]
        );
        assert_eq!(page, b.page("p").unwrap());
    }

    #[test]
    fn page_tree_refuses_cycles_and_nests() {
        let mut ws = seeded(1);
        apply(
            &mut ws,
            &Op::CreatePage {
                id: "q".into(),
                parent: Some("p".into()),
                title: "Child".into(),
                database: None,
            },
        )
        .unwrap();
        assert_eq!(ws.meta("q").unwrap().parent.as_deref(), Some("p"));
        assert!(
            apply(
                &mut ws,
                &Op::MovePage {
                    id: "p".into(),
                    parent: Some("q".into())
                }
            )
            .is_err()
        );
        assert_eq!(
            ws.pages().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["p", "q"]
        );
    }

    #[test]
    fn damaged_bytes_are_refused_and_never_panic() {
        let a = seeded(1);
        let good = a.export().unwrap();
        let mut ws = Workspace::new(2).unwrap();
        assert!(ws.import(b"nonsense").is_err());
        assert!(ws.import(&good[..good.len() / 2]).is_err());
        for at in (0..good.len()).step_by(37) {
            let mut bad = good.clone();
            bad[at] ^= 0xFF;
            let mut ws = Workspace::new(2).unwrap();
            let _ = ws.import(&bad);
            let _ = ws.pages();
        }
    }

    fn empty_db(id: &str) -> Database {
        Database {
            id: id.into(),
            name: "Tasks".into(),
            view: "table".into(),
            statuses: vec![casual_note_model::Status {
                id: "s1".into(),
                name: "To do".into(),
                group: "todo".into(),
            }],
            columns: vec![Column {
                id: "owner".into(),
                name: "Owner".into(),
                kind: "text".into(),
                hidden: false,
            }],
            rows: vec![],
        }
    }
    fn row(id: &str) -> Row {
        Row {
            id: id.into(),
            name: id.into(),
            status: "s1".into(),
            values: Default::default(),
        }
    }

    #[test]
    fn database_edits_undo_and_cells_from_two_devices_both_survive() {
        let mut a = seeded(1);
        apply(
            &mut a,
            &Op::CreateDatabase {
                database: empty_db("d"),
            },
        )
        .unwrap();
        apply(
            &mut a,
            &Op::AddRow {
                id: "d".into(),
                after: None,
                row: row("r1"),
            },
        )
        .unwrap();
        apply(
            &mut a,
            &Op::AddColumn {
                id: "d".into(),
                column: Column {
                    id: "due".into(),
                    name: "Due".into(),
                    kind: "date".into(),
                    hidden: false,
                },
            },
        )
        .unwrap();
        let mut b = Workspace::new(2).unwrap();
        b.import(&a.export().unwrap()).unwrap();
        assert_eq!(a.database("d").unwrap(), b.database("d").unwrap());

        apply(
            &mut a,
            &Op::SetCell {
                id: "d".into(),
                row: "r1".into(),
                column: "owner".into(),
                value: "Mei".into(),
            },
        )
        .unwrap();
        apply(
            &mut b,
            &Op::SetCell {
                id: "d".into(),
                row: "r1".into(),
                column: "due".into(),
                value: "2026-12-01".into(),
            },
        )
        .unwrap();
        apply(
            &mut b,
            &Op::AddRow {
                id: "d".into(),
                after: Some("r1".into()),
                row: row("r2"),
            },
        )
        .unwrap();
        sync(&mut a, &mut b);
        let db = a.database("d").unwrap();
        assert_eq!(db, b.database("d").unwrap());
        assert_eq!(db.rows[0].values["owner"], "Mei");
        assert_eq!(db.rows[0].values["due"], "2026-12-01");
        assert_eq!(db.rows.len(), 2);

        let inverse = apply(
            &mut a,
            &Op::DeleteRow {
                id: "d".into(),
                row: "r1".into(),
            },
        )
        .unwrap();
        assert_eq!(a.database("d").unwrap().rows.len(), 1);
        apply(&mut a, &inverse).unwrap();
        let back = a.database("d").unwrap();
        assert_eq!(back.rows[0].id, "r1");
        assert_eq!(back.rows[0].values["owner"], "Mei");
    }

    #[test]
    fn a_hidden_column_keeps_its_cells() {
        let mut a = seeded(1);
        apply(
            &mut a,
            &Op::CreateDatabase {
                database: empty_db("d"),
            },
        )
        .unwrap();
        apply(
            &mut a,
            &Op::AddRow {
                id: "d".into(),
                after: None,
                row: row("r1"),
            },
        )
        .unwrap();
        apply(
            &mut a,
            &Op::SetCell {
                id: "d".into(),
                row: "r1".into(),
                column: "owner".into(),
                value: "Mei".into(),
            },
        )
        .unwrap();
        apply(
            &mut a,
            &Op::SetColumnHidden {
                id: "d".into(),
                column: "owner".into(),
                hidden: true,
            },
        )
        .unwrap();
        let db = a.database("d").unwrap();
        assert!(db.columns[0].hidden);
        assert_eq!(db.rows[0].values["owner"], "Mei");
    }

    #[test]
    fn canvas_items_and_ink_undo_merge_and_keep_order() {
        let mut a = seeded(1);
        let item = CanvasItem {
            id: "i".into(),
            kind: "sticky".into(),
            x: 1.5,
            y: 2.0,
            width: 170.0,
            height: 120.0,
            color: "amber".into(),
            text: "你好".into(),
        };
        let inverse = apply(
            &mut a,
            &Op::PutItem {
                page: "p".into(),
                item: item.clone(),
            },
        )
        .unwrap();
        assert_eq!(a.page("p").unwrap().canvas.items, vec![item.clone()]);
        apply(&mut a, &inverse).unwrap();
        assert!(a.page("p").unwrap().canvas.items.is_empty());
        apply(
            &mut a,
            &Op::PutItem {
                page: "p".into(),
                item,
            },
        )
        .unwrap();

        let stroke = |id: &str, x: f64| Ink {
            id: id.into(),
            kind: "pen".into(),
            color: "red".into(),
            width: 3.0,
            points: vec![[x, 1.0, 0.5], [x + 1.25, 2.5, 0.75]],
        };
        apply(
            &mut a,
            &Op::ChangeInk {
                page: "p".into(),
                add: vec![stroke("s1", 10.0)],
                remove: vec![],
            },
        )
        .unwrap();
        let mut b = Workspace::new(2).unwrap();
        b.import(&a.export().unwrap()).unwrap();
        apply(
            &mut a,
            &Op::ChangeInk {
                page: "p".into(),
                add: vec![stroke("s2", 20.0), stroke("s3", 30.0)],
                remove: vec!["s1".into()],
            },
        )
        .unwrap();
        apply(
            &mut b,
            &Op::ChangeInk {
                page: "p".into(),
                add: vec![stroke("s4", 40.0)],
                remove: vec![],
            },
        )
        .unwrap();
        sync(&mut a, &mut b);
        assert_eq!(a.page("p").unwrap().canvas, b.page("p").unwrap().canvas);
        let ids: Vec<_> = a
            .page("p")
            .unwrap()
            .canvas
            .ink
            .iter()
            .map(|s| s.id.clone())
            .collect();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"s4".to_string()));
        assert!(!ids.contains(&"s1".to_string()));
        assert_eq!(
            a.page("p")
                .unwrap()
                .canvas
                .ink
                .iter()
                .find(|s| s.id == "s2")
                .unwrap()
                .points[1],
            [21.25, 2.5, 0.75]
        );
    }

    #[test]
    fn damaged_or_huge_ink_is_refused() {
        assert!(casual_note_model::decode_samples(&[0xFF; 20]).is_err());
        assert!(casual_note_model::encode_samples(&[[f64::NAN, 0.0, 0.0]]).is_err());
        assert!(casual_note_model::encode_samples(&[[2e9, 0.0, 0.0]]).is_err());
        let round = [[1.5, -2.25, 0.5], [100.0, 200.0, 1.0]];
        assert_eq!(
            casual_note_model::decode_samples(&casual_note_model::encode_samples(&round).unwrap())
                .unwrap(),
            round
        );
    }

    #[test]
    fn links_read_back_and_embeds_keep_their_target() {
        let mut a = seeded(1);
        apply(
            &mut a,
            &Op::CreatePage {
                id: "q".into(),
                parent: None,
                title: "Other".into(),
                database: None,
            },
        )
        .unwrap();
        apply(
            &mut a,
            &Op::SetLink {
                page: "p".into(),
                block: "a".into(),
                start: 0,
                end: 5,
                target: Some("q".into()),
            },
        )
        .unwrap();
        let mut embed = paragraph("e", "");
        embed.kind = BlockKind::Embed;
        embed.target = Some("q".into());
        apply(
            &mut a,
            &Op::InsertBlock {
                page: "p".into(),
                after: Some("a".into()),
                block: embed,
            },
        )
        .unwrap();
        let page = a.page("p").unwrap();
        assert_eq!(page.blocks[0].runs[0].link.as_deref(), Some("q"));
        assert_eq!(page.blocks[1].target.as_deref(), Some("q"));
    }

    #[test]
    fn redo_restores_a_link_and_undo_restores_the_previous_one() {
        let mut a = seeded(1);
        let link = |target: Option<&str>| Op::SetLink {
            page: "p".into(),
            block: "a".into(),
            start: 0,
            end: 5,
            target: target.map(Into::into),
        };
        let undo = apply(&mut a, &link(Some("q"))).unwrap();
        assert_eq!(undo, link(None));
        let redo = apply(&mut a, &undo).unwrap();
        assert_eq!(redo, link(Some("q")));
        apply(&mut a, &redo).unwrap();
        assert_eq!(
            a.page("p").unwrap().blocks[0].runs[0].link.as_deref(),
            Some("q")
        );
        let back = apply(&mut a, &link(Some("r"))).unwrap();
        assert_eq!(back, link(Some("q")));
    }

    #[test]
    fn a_column_can_be_renamed_and_undone() {
        let mut a = seeded(1);
        apply(
            &mut a,
            &Op::CreateDatabase {
                database: empty_db("d"),
            },
        )
        .unwrap();
        let undo = apply(
            &mut a,
            &Op::SetColumnName {
                id: "d".into(),
                column: "owner".into(),
                name: "负责人".into(),
            },
        )
        .unwrap();
        assert_eq!(a.database("d").unwrap().columns[0].name, "负责人");
        apply(&mut a, &undo).unwrap();
        assert_eq!(a.database("d").unwrap().columns[0].name, "Owner");
    }

    #[test]
    fn ops_survive_the_json_the_web_app_sends() {
        let op = splice("a", 1, 2, "x");
        let json = serde_json::to_string(&op).unwrap();
        assert_eq!(serde_json::from_str::<Op>(&json).unwrap(), op);
    }
}
