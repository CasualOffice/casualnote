//! Writes a package from the engine, for checking with tools/cnotepack.py:
//!
//!     cargo run -p casual-note-package --example sample -- out.cnotepack
//!     python3 tools/cnotepack.py verify out.cnotepack

use casual_note_merge::Workspace;
use casual_note_model::{Block, BlockKind, Ink, Run};
use casual_note_ops::{Op, apply};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: sample <out.cnotepack>")?;
    let mut ws = Workspace::new(1)?;
    let samples = [
        "مرحبا بالعالم",
        "नमस्ते दुनिया",
        "你好，世界",
        "こんにちは",
        "한국어",
        "¿Qué tal?",
        "Hello",
    ];
    apply(
        &mut ws,
        &Op::CreatePage {
            id: "p1".into(),
            parent: None,
            title: "مرحبا 你好".into(),
            database: None,
        },
    )?;
    let mut after = None;
    for (i, text) in samples.iter().enumerate() {
        let id = format!("b{i}");
        let block = Block {
            id: id.clone(),
            kind: BlockKind::Paragraph,
            checked: false,
            runs: vec![Run {
                text: (*text).into(),
                marks: vec![],
                link: None,
            }],
            target: None,
        };
        apply(
            &mut ws,
            &Op::InsertBlock {
                page: "p1".into(),
                after,
                block,
            },
        )?;
        after = Some(id);
    }
    apply(
        &mut ws,
        &Op::ChangeInk {
            page: "p1".into(),
            add: vec![Ink {
                id: "s1".into(),
                kind: "pen".into(),
                color: "red".into(),
                width: 3.0,
                points: vec![[1.0, 2.0, 0.5]],
            }],
            remove: vec![],
        },
    )?;
    std::fs::write(
        &path,
        casual_note_package::export(&ws, "2026-11-07T12:00:00Z", None)?,
    )?;
    Ok(())
}
