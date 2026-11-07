//! The gates in docs/36, on bytes this engine wrote: an independent reader accepts them, export
//! then import then export gives identical bytes, and every launch script survives unchanged.

use casual_note_merge::Workspace;
use casual_note_model::{
    Block, BlockKind, CanvasItem, Column, Database, Ink, Mark, Note, Page, PageMeta, Row, Run,
    Status,
};
use casual_note_ops::{Op, apply};
use casual_note_package::{
    Snapshot, export, import_package, inspect, read_package, write_package, zip,
};
use std::path::PathBuf;
use std::process::Command;

const CREATED: &str = "2026-11-07T12:00:00Z";
const SCRIPTS: &[&str] = &[
    "مرحبا بالعالم",
    "مَرْحَبًا",
    "नमस्ते दुनिया",
    "你好，世界",
    "繁體中文",
    "こんにちは世界",
    "カタカナ",
    "한국어 문서",
    "Español: ¿qué tal, niño?",
    "Plain English",
    "Café decomposed: cafe\u{301}",
    "RTL marks \u{200f}kept\u{200e}, 🎉 too",
    "สวัสดีครับ",
    "a|b \"quoted\" \\ slash\nnew line\ttab",
];

fn run(text: &str) -> Run {
    Run {
        text: text.into(),
        marks: vec![],
        link: None,
    }
}

fn block(id: &str, kind: BlockKind, runs: Vec<Run>, target: Option<&str>, checked: bool) -> Block {
    Block {
        id: id.into(),
        kind,
        checked,
        runs,
        target: target.map(Into::into),
    }
}

fn rich_workspace() -> Workspace {
    let mut ws = Workspace::new(11).unwrap();
    let database = Database {
        id: "d-launch".into(),
        name: "مهام 任务".into(),
        view: "board".into(),
        statuses: vec![
            Status {
                id: "s1".into(),
                name: "未开始".into(),
                group: "todo".into(),
            },
            Status {
                id: "s2".into(),
                name: "Hecho".into(),
                group: "done".into(),
            },
        ],
        columns: vec![
            Column {
                id: "owner".into(),
                name: "Owner".into(),
                kind: "text".into(),
                hidden: false,
            },
            Column {
                id: "due".into(),
                name: "Due".into(),
                kind: "date".into(),
                hidden: false,
            },
            Column {
                id: "old".into(),
                name: "Old".into(),
                kind: "text".into(),
                hidden: true,
            },
        ],
        rows: vec![Row {
            id: "r1".into(),
            name: "إطلاق".into(),
            status: "s2".into(),
            values: [
                ("owner".to_string(), "Mei".to_string()),
                ("due".to_string(), "2026-11-20".to_string()),
                ("old".to_string(), "kept".to_string()),
            ]
            .into(),
        }],
    };
    let mut ops = vec![
        Op::CreateDatabase { database },
        Op::CreatePage {
            id: "p-root".into(),
            parent: None,
            title: "مرحبا 你好".into(),
            database: None,
        },
        Op::SetFavorite {
            id: "p-root".into(),
            value: true,
        },
        Op::CreatePage {
            id: "p-child".into(),
            parent: Some("p-root".into()),
            title: "".into(),
            database: None,
        },
        Op::CreatePage {
            id: "p-same".into(),
            parent: None,
            title: "STRASSE".into(),
            database: None,
        },
        Op::CreatePage {
            id: "p-same2".into(),
            parent: None,
            title: "straße".into(),
            database: None,
        },
        Op::CreatePage {
            id: "p-unsafe".into(),
            parent: None,
            title: "../a:b*c?.".into(),
            database: None,
        },
        Op::CreatePage {
            id: "p-db".into(),
            parent: None,
            title: "Tracker".into(),
            database: Some("d-launch".into()),
        },
        Op::CreatePage {
            id: "p-gone".into(),
            parent: None,
            title: "Trashed".into(),
            database: None,
        },
        Op::SetTrashed {
            id: "p-gone".into(),
            value: true,
        },
    ];
    let mut after: Option<String> = None;
    let mut add = |b: Block| {
        let id = b.id.clone();
        ops.push(Op::InsertBlock {
            page: "p-root".into(),
            after: after.clone(),
            block: b,
        });
        after = Some(id);
    };
    for (i, text) in SCRIPTS.iter().enumerate() {
        add(block(
            &format!("t{i}"),
            BlockKind::Paragraph,
            vec![run(text)],
            None,
            false,
        ));
    }
    add(block(
        "h1",
        BlockKind::Heading1,
        vec![run("One")],
        None,
        false,
    ));
    add(block(
        "h2",
        BlockKind::Heading2,
        vec![run("Two")],
        None,
        false,
    ));
    add(block(
        "h3",
        BlockKind::Heading3,
        vec![run("Three")],
        None,
        false,
    ));
    add(block(
        "ul",
        BlockKind::Bulleted,
        vec![run("b")],
        None,
        false,
    ));
    add(block(
        "ol",
        BlockKind::Numbered,
        vec![run("n")],
        None,
        false,
    ));
    add(block("td", BlockKind::Todo, vec![run("done")], None, true));
    add(block("tn", BlockKind::Todo, vec![], None, false));
    add(block("qu", BlockKind::Quote, vec![run("q")], None, false));
    add(block("ca", BlockKind::Callout, vec![run("c")], None, false));
    add(block(
        "co",
        BlockKind::Code,
        vec![run("let x = 1;\n#notatag")],
        None,
        false,
    ));
    add(block("hr", BlockKind::Divider, vec![], None, false));
    add(block(
        "dbb",
        BlockKind::Database,
        vec![],
        Some("d-launch"),
        false,
    ));
    add(block(
        "em",
        BlockKind::Embed,
        vec![],
        Some("p-child"),
        false,
    ));
    add(block(
        "emg",
        BlockKind::Embed,
        vec![],
        Some("p-nowhere"),
        false,
    ));
    add(block(
        "ma",
        BlockKind::Paragraph,
        vec![
            run("plain "),
            Run {
                text: "粗体".into(),
                marks: vec![Mark::Bold],
                link: None,
            },
            Run {
                text: " both".into(),
                marks: vec![Mark::Bold, Mark::Italic],
                link: None,
            },
            Run {
                text: " code".into(),
                marks: vec![Mark::Code],
                link: None,
            },
            Run {
                text: " hi".into(),
                marks: vec![Mark::Highlight],
                link: None,
            },
            Run {
                text: "لينك".into(),
                marks: vec![],
                link: Some("p-child".into()),
            },
            Run {
                text: "marked".into(),
                marks: vec![Mark::Bold],
                link: Some("p-same".into()),
            },
            Run {
                text: "gone".into(),
                marks: vec![],
                link: Some("p-gone".into()),
            },
        ],
        None,
        false,
    ));
    ops.push(Op::InsertBlock {
        page: "p-child".into(),
        after: None,
        block: block(
            "c1",
            BlockKind::Paragraph,
            vec![
                run("back "),
                Run {
                    text: "up".into(),
                    marks: vec![],
                    link: Some("p-root".into()),
                },
            ],
            None,
            false,
        ),
    });
    ops.push(Op::SetView {
        page: "p-child".into(),
        view: "canvas".into(),
    });
    ops.push(Op::SetNote {
        page: "p-child".into(),
        note: Note {
            x: 10.5,
            y: -3.25,
            width: 430.0,
        },
    });
    ops.push(Op::PutItem {
        page: "p-child".into(),
        item: CanvasItem {
            id: "i1".into(),
            kind: "sticky".into(),
            x: 530.0,
            y: 40.125,
            width: 180.0,
            height: 120.0,
            color: "amber".into(),
            text: "ملاحظة 付箋".into(),
        },
    });
    ops.push(Op::ChangeInk {
        page: "p-child".into(),
        add: vec![
            Ink {
                id: "s1".into(),
                kind: "pen".into(),
                color: "red".into(),
                width: 3.0,
                points: vec![[720.0, 240.0, 0.5], [722.125, 243.5, 0.625]],
            },
            Ink {
                id: "s2".into(),
                kind: "arrow".into(),
                color: String::new(),
                width: 0.0,
                points: vec![[252.0, 362.0, 0.5], [376.0, 362.0, 0.5]],
            },
            Ink {
                id: "s3".into(),
                kind: "highlighter".into(),
                color: "hi".into(),
                width: 12.0,
                points: vec![[1.0, 2.0, 1.0]],
            },
        ],
        remove: vec![],
    });
    for op in &ops {
        apply(&mut ws, op).unwrap();
    }
    ws
}

fn out_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cnotepack");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn python_verify(path: &std::path::Path) -> String {
    let tool = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/cnotepack.py");
    let out = Command::new("python3")
        .arg(&tool)
        .arg("verify")
        .arg(path)
        .output()
        .expect("python3 is needed for this gate");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success(),
        "cnotepack.py refused the engine's bytes: {text}"
    );
    text
}

fn roundtrip(bytes: &[u8], peer: u64) -> Workspace {
    let mut ws = Workspace::new(peer).unwrap();
    let report = import_package(&mut ws, bytes, peer + 1000).unwrap();
    assert!(!report.copied);
    ws
}

#[test]
fn gate_the_independent_reader_verifies_engine_bytes() {
    let ws = rich_workspace();
    let bytes = export(&ws, CREATED, None).unwrap();
    let path = out_dir().join("rich.cnotepack");
    std::fs::write(&path, &bytes).unwrap();
    let said = python_verify(&path);
    assert!(said.starts_with("OK."), "{said}");
    assert!(said.contains("6 pages"), "{said}");
    assert!(!said.contains("Note."), "{said}");
    let one = export(&ws, CREATED, Some("p-root")).unwrap();
    let path = out_dir().join("one.cnotepack");
    std::fs::write(&path, &one).unwrap();
    assert!(python_verify(&path).contains("1 pages, 1 databases"));
}

#[test]
fn gate_export_import_export_gives_identical_bytes() {
    let ws = rich_workspace();
    let first = export(&ws, CREATED, None).unwrap();
    let again = export(&ws, CREATED, None).unwrap();
    assert_eq!(first, again, "writing twice is the same");
    let second_ws = roundtrip(&first, 21);
    let second = export(&second_ws, CREATED, None).unwrap();
    if first != second {
        let (a, b) = (inspect(&first).unwrap(), inspect(&second).unwrap());
        assert_eq!(a.files, b.files);
        let (pa, pb) = (
            read_package(&first).unwrap(),
            read_package(&second).unwrap(),
        );
        assert_eq!(pa.pages, pb.pages);
        panic!("the bytes differ");
    }
    let third = export(&roundtrip(&second, 31), CREATED, None).unwrap();
    assert_eq!(first, third);
    // A page on its own, too.
    let page = export(&ws, CREATED, Some("p-child")).unwrap();
    assert_eq!(
        page,
        export(&roundtrip(&page, 41), CREATED, None)
            .map(|_| export(&roundtrip(&page, 42), CREATED, Some("p-child")).unwrap())
            .unwrap()
    );
}

#[test]
fn gate_text_in_every_launch_script_is_unchanged() {
    let ws = rich_workspace();
    let bytes = export(&ws, CREATED, None).unwrap();
    let back = roundtrip(&bytes, 51);
    let page = back.page("p-root").unwrap();
    for (i, text) in SCRIPTS.iter().enumerate() {
        let b = page
            .blocks
            .iter()
            .find(|b| b.id == format!("t{i}"))
            .unwrap();
        assert_eq!(b.plain().as_bytes(), text.as_bytes(), "block {i}");
    }
    assert_eq!(page.meta.title, "مرحبا 你好");
    assert_eq!(back.page("p-root").unwrap(), ws.page("p-root").unwrap());
    assert_eq!(
        back.page("p-child").unwrap(),
        ws.page("p-child").unwrap(),
        "canvas and ink too"
    );
    assert_eq!(
        back.database("d-launch").unwrap(),
        ws.database("d-launch").unwrap(),
        "rows, statuses and hidden columns"
    );
    assert!(back.page("p-gone").is_err(), "the trash does not travel");
    // Marks stay on the words they were on, and do not spread to the runs after them.
    let ma = page.blocks.iter().find(|b| b.id == "ma").unwrap();
    let marks = |text: &str| &ma.runs.iter().find(|r| r.text == text).unwrap().marks;
    assert_eq!(ma.runs[0].text, "plain ");
    assert!(ma.runs[0].marks.is_empty());
    assert_eq!(*marks("粗体"), [Mark::Bold]);
    assert_eq!(*marks(" both"), [Mark::Bold, Mark::Italic]);
    assert_eq!(*marks(" code"), [Mark::Code]);
    assert_eq!(*marks(" hi"), [Mark::Highlight]);
    assert!(marks("لينك").is_empty());
}

#[test]
fn nothing_is_lost_at_the_format_level() {
    let bytes = export(&rich_workspace(), CREATED, None).unwrap();
    let names: Vec<String> = zip::central_directory(&bytes)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names[0], "mimetype");
    assert_eq!(names[1], "manifest.json");
    let mut rest = names[2..].to_vec();
    rest.sort();
    assert_eq!(rest, names[2..], "entries are sorted");
    assert!(names.contains(&"ink/p-child.jsonl".to_string()));
    assert!(
        names.iter().any(|n| n.starts_with("readable/مرحبا 你好")),
        "titles stay readable"
    );
    assert!(
        names
            .iter()
            .filter(|n| n.starts_with("readable/"))
            .all(|n| !n.contains(".."))
    );
    assert_eq!(&bytes[30..38], b"mimetype");
    assert_eq!(
        &bytes[38..38 + 38],
        b"application/vnd.casualnote.package+zip"
    );
    let ink = casual_note_package::entry_bytes(&bytes, "ink/p-child.jsonl").unwrap();
    assert_eq!(
        String::from_utf8(ink).unwrap(),
        "{\"color\":\"red\",\"id\":\"s1\",\"kind\":\"pen\",\"points\":[[720,240,0.5],[722.125,243.5,0.625]],\"width\":3}\n\
         {\"from\":[252,362],\"id\":\"s2\",\"kind\":\"arrow\",\"to\":[376,362]}\n\
         {\"color\":\"hi\",\"id\":\"s3\",\"kind\":\"highlighter\",\"points\":[[1,2,1]],\"width\":12}\n"
    );
    let manifest =
        String::from_utf8(casual_note_package::entry_bytes(&bytes, "manifest.json").unwrap())
            .unwrap();
    assert!(manifest.contains("\"scope\": \"workspace\""));
    assert!(!manifest.contains("Trashed"));
    let keys: Vec<usize> = [
        "\"created\"",
        "\"databases\"",
        "\"files\"",
        "\"format\"",
        "\"generator\"",
        "\"languages\"",
        "\"pages\"",
        "\"scope\"",
        "\"version\"",
    ]
    .iter()
    .map(|k| manifest.find(k).unwrap())
    .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "keys are sorted");
}

fn synthetic(n: usize) -> Snapshot {
    let pages = (0..n)
        .map(|i| Page {
            meta: PageMeta {
                id: format!("p{i:06}"),
                parent: None,
                title: format!("t{i}"),
                favorite: false,
                trashed: false,
                database: None,
            },
            blocks: vec![],
            view: "page".into(),
            canvas: Default::default(),
        })
        .collect();
    Snapshot {
        pages,
        databases: vec![],
        scope_page: false,
    }
}

#[test]
fn gate_zip64_above_65534_entries() {
    // README + mimetype + manifest + 2 per page.
    let at_limit = write_package(&synthetic(32_765), CREATED).unwrap();
    assert_eq!(zip::central_directory(&at_limit).unwrap().len(), 65_533);
    assert_ne!(
        at_limit[at_limit.len() - 22 - 20 - 56..at_limit.len() - 22 - 20 - 52],
        [0x50, 0x4b, 0x06, 0x06],
        "no ZIP64 when not needed"
    );
    let bytes = write_package(&synthetic(32_767), CREATED).unwrap();
    assert_eq!(zip::central_directory(&bytes).unwrap().len(), 65_537);
    assert_eq!(
        bytes[bytes.len() - 22 - 20 - 56..bytes.len() - 22 - 20 - 52],
        [0x50, 0x4b, 0x06, 0x06],
        "a ZIP64 record is written"
    );
    let path = out_dir().join("zip64.cnotepack");
    std::fs::write(&path, &bytes).unwrap();
    assert!(python_verify(&path).contains("32767 pages"));
    assert_eq!(read_package(&bytes).unwrap().pages.len(), 32_767);
}

#[test]
fn import_into_a_used_workspace_makes_copies_and_rewrites_links() {
    let source = rich_workspace();
    let bytes = export(&source, CREATED, None).unwrap();
    let mut target = rich_workspace();
    let before = target.export().unwrap().len();
    let report = import_package(&mut target, &bytes, 900).unwrap();
    assert!(report.copied && report.pages == 6 && report.databases == 1);
    assert_eq!(report.renamed.len(), 7);
    assert!(target.export().unwrap().len() > before);
    let new_id = |old: &str| {
        report
            .renamed
            .iter()
            .find(|r| r.from == old)
            .map(|r| r.to.clone())
            .unwrap()
    };
    let copy = target.page(&new_id("p-root")).unwrap();
    let ma = copy.blocks.iter().find(|b| b.id == "ma").unwrap();
    assert_eq!(
        ma.runs.iter().find(|r| r.text == "لينك").unwrap().link,
        Some(new_id("p-child")),
        "a link inside the package follows the copy"
    );
    assert_eq!(
        ma.runs
            .iter()
            .find(|r| r.text == "gone")
            .unwrap()
            .link
            .as_deref(),
        Some("p-gone"),
        "a link out of the package is kept"
    );
    assert_eq!(
        copy.blocks.iter().find(|b| b.id == "dbb").unwrap().target,
        Some(new_id("d-launch"))
    );
    assert_eq!(
        copy.blocks.iter().find(|b| b.id == "em").unwrap().target,
        Some(new_id("p-child"))
    );
    assert_eq!(
        target.page(&new_id("p-child")).unwrap().meta.parent,
        Some(new_id("p-root"))
    );
    assert_eq!(
        target.page(&new_id("p-db")).unwrap().meta.database,
        Some(new_id("d-launch"))
    );
    // The originals are untouched, and a third import gets ids of its own.
    assert_eq!(
        target.page("p-root").unwrap(),
        source.page("p-root").unwrap()
    );
    let third = import_package(&mut target, &bytes, 901).unwrap();
    assert!(
        third
            .renamed
            .iter()
            .all(|r| !report.renamed.iter().any(|o| o.to == r.to))
    );
}

#[test]
fn import_reports_what_it_cannot_resolve() {
    let bytes = export(&rich_workspace(), CREATED, None).unwrap();
    let mut empty = Workspace::new(77).unwrap();
    let report = import_package(&mut empty, &bytes, 78).unwrap();
    assert_eq!(
        report.dangling,
        ["p-gone", "p-nowhere"],
        "kept, shown as removed, and listed"
    );
}
