//! The closed set of edits: the only write path. Every op validates and returns its inverse, so
//! undo is replaying inverses. Merging is the CRDT's job, and there is no transform (ADR-005).

use casual_note_merge::Workspace;
use casual_note_model::{Block, BlockKind, Error, Mark, Run};
use serde::{Deserialize, Serialize};

/// One edit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    CreatePage {
        id: String,
        parent: Option<String>,
        title: String,
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
        Op::CreatePage { id, parent, title } => {
            ws.create_page(id, parent.as_deref(), title)?;
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
            }]
        },
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
        let ops = vec![
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

    #[test]
    fn ops_survive_the_json_the_web_app_sends() {
        let op = splice("a", 1, 2, "x");
        let json = serde_json::to_string(&op).unwrap();
        assert_eq!(serde_json::from_str::<Op>(&json).unwrap(), op);
    }
}
