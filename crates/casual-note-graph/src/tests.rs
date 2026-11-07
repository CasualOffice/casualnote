use super::*;
use casual_note_model::{Block, Canvas, CanvasItem, Column, PageMeta, Row, Run, Status};

fn run(text: &str) -> Run {
    Run {
        text: text.into(),
        marks: vec![],
        link: None,
    }
}

fn link(text: &str, to: &str) -> Run {
    Run {
        text: text.into(),
        marks: vec![],
        link: Some(to.into()),
    }
}

fn block(id: &str, kind: BlockKind, runs: Vec<Run>, target: Option<&str>) -> Block {
    Block {
        id: id.into(),
        kind,
        checked: false,
        runs,
        target: target.map(Into::into),
    }
}

fn page(id: &str, parent: Option<&str>, title: &str, blocks: Vec<Block>) -> Page {
    Page {
        meta: PageMeta {
            id: id.into(),
            parent: parent.map(Into::into),
            title: title.into(),
            favorite: false,
            trashed: false,
            database: None,
        },
        blocks,
        view: "page".into(),
        canvas: Canvas::default(),
    }
}

fn tracker() -> Database {
    Database {
        id: "d1".into(),
        name: "Tracker".into(),
        view: "table".into(),
        statuses: vec![Status {
            id: "s".into(),
            name: "Open".into(),
            group: "todo".into(),
        }],
        columns: vec![
            Column {
                id: "c1".into(),
                name: "Owner".into(),
                kind: "text".into(),
                hidden: false,
            },
            Column {
                id: "c2".into(),
                name: "作者".into(),
                kind: "text".into(),
                hidden: false,
            },
            Column {
                id: "c3".into(),
                name: "Notes".into(),
                kind: "text".into(),
                hidden: false,
            },
            Column {
                id: "c4".into(),
                name: "Author".into(),
                kind: "text".into(),
                hidden: true,
            },
        ],
        rows: vec![
            Row {
                id: "r1".into(),
                name: "Write".into(),
                status: "s".into(),
                values: [
                    ("c1".to_string(), "Mei".to_string()),
                    ("c3".to_string(), "Not a person".to_string()),
                    ("c4".to_string(), "Hidden".to_string()),
                ]
                .into(),
            },
            Row {
                id: "r2".into(),
                name: "Ship".into(),
                status: "s".into(),
                values: [
                    ("c1".to_string(), "mei".to_string()),
                    ("c2".to_string(), "李雷".to_string()),
                ]
                .into(),
            },
            Row {
                id: "r3".into(),
                name: String::new(),
                status: "s".into(),
                values: [("c1".to_string(), "Nobody".to_string())].into(),
            },
        ],
    }
}

fn sample() -> (Vec<Page>, Vec<Database>) {
    let mut holder = page("p3", None, "Tracker", vec![]);
    holder.meta.database = Some("d1".into());
    let mut canvas_page = page("p4", None, "", vec![]);
    canvas_page.canvas.items.push(CanvasItem {
        id: "i".into(),
        kind: "sticky".into(),
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        color: String::new(),
        text: "#مرحبا".into(),
    });
    (
        vec![
            page(
                "p1",
                None,
                "Launch",
                vec![
                    block(
                        "b1",
                        BlockKind::Paragraph,
                        vec![
                            run("see "),
                            link("two", "p2"),
                            run(" and "),
                            link("two again", "p2"),
                            run(" and "),
                            link("self", "p1"),
                            run(" "),
                            link("gone", "nope"),
                        ],
                        None,
                    ),
                    block(
                        "b2",
                        BlockKind::Paragraph,
                        vec![run("#Plan and #plan and #PLAN and #导入")],
                        None,
                    ),
                    block("b3", BlockKind::Code, vec![run("#notatag")], None),
                    block(
                        "b4",
                        BlockKind::Paragraph,
                        vec![Run {
                            text: "#alsonot".into(),
                            marks: vec![Mark::Code],
                            link: None,
                        }],
                        None,
                    ),
                    block("b5", BlockKind::Embed, vec![], Some("p4")),
                    block("b6", BlockKind::Database, vec![], Some("d1")),
                ],
            ),
            page("p2", Some("p1"), "Child", vec![]),
            holder,
            canvas_page,
        ],
        vec![tracker()],
    )
}

fn has(graph: &Graph, kind: EdgeKind, from: &str, to: &str) -> Option<u32> {
    graph
        .edges
        .iter()
        .find(|e| e.kind == kind && e.from == from && e.to == to)
        .map(|e| e.count)
}

#[test]
fn nodes_and_edges_are_exactly_what_the_notes_say() {
    let (pages, dbs) = sample();
    let g = derive(&pages, &dbs);
    let ids: Vec<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "page:p1",
            "page:p2",
            "page:p3",
            "page:p4",
            "person:mei",
            "person:李雷",
            "row:d1:r1",
            "row:d1:r2",
            "tag:plan",
            "tag:مرحبا",
            "tag:导入",
        ]
    );
    assert_eq!(
        has(&g, EdgeKind::Link, "page:p1", "page:p2"),
        Some(2),
        "two links are one edge with a count"
    );
    assert_eq!(
        has(&g, EdgeKind::Link, "page:p1", "page:p1"),
        None,
        "a self link is dropped"
    );
    assert_eq!(has(&g, EdgeKind::Child, "page:p1", "page:p2"), Some(1));
    assert_eq!(has(&g, EdgeKind::Embed, "page:p1", "page:p4"), Some(1));
    assert_eq!(
        has(&g, EdgeKind::Embed, "page:p1", "page:p3"),
        Some(1),
        "a database block points at its host"
    );
    assert_eq!(has(&g, EdgeKind::Tag, "page:p1", "tag:plan"), Some(3));
    assert_eq!(has(&g, EdgeKind::Tag, "page:p4", "tag:مرحبا"), Some(1));
    assert_eq!(has(&g, EdgeKind::Row, "page:p3", "row:d1:r1"), Some(1));
    assert_eq!(has(&g, EdgeKind::Owner, "row:d1:r1", "person:mei"), Some(1));
    assert_eq!(
        has(&g, EdgeKind::Owner, "row:d1:r2", "person:mei"),
        Some(1),
        "Mei and mei are one node"
    );
    assert_eq!(
        has(&g, EdgeKind::Owner, "row:d1:r2", "person:李雷"),
        Some(1)
    );
    let node = |id: &str| g.nodes.iter().find(|n| n.id == id).unwrap();
    assert_eq!(
        node("page:p1").dangling,
        1,
        "a link to a gone page is counted, not an edge"
    );
    assert_eq!(
        node("tag:plan").label,
        "#PLAN",
        "ties go to code point order"
    );
    assert_eq!(node("person:mei").label, "Mei");
    assert_eq!(node("row:d1:r1").page.as_deref(), Some("p3"));
    assert_eq!(node("row:d1:r1").status.as_deref(), Some("Open"));
    assert_eq!(
        node("page:p4").label,
        "",
        "an untitled page stays empty in data"
    );
}

#[test]
fn output_is_deterministic_whatever_the_input_order() {
    let (mut pages, dbs) = sample();
    let a = serde_json::to_string(&derive(&pages, &dbs)).unwrap();
    pages.reverse();
    let b = serde_json::to_string(&derive(&pages, &dbs)).unwrap();
    assert_eq!(a, b);
    assert!(a.contains("\"origin\":\"made\""));
}

#[test]
fn tag_grammar_in_every_script() {
    assert_eq!(tags_in("#مرحبا بالعالم"), ["مرحبا"]);
    assert_eq!(
        tags_in("x #हिन्दी y"),
        ["हिन्दी"],
        "marks are part of the tag"
    );
    assert_eq!(tags_in("#标签之后继续写"), ["标签之后继续写"]);
    assert_eq!(tags_in("#标签#之后继续写"), ["标签"], "a closing # ends it");
    assert_eq!(tags_in("＃タグ です"), ["タグ"], "fullwidth is accepted");
    assert_eq!(
        tags_in("a#b c#d"),
        Vec::<String>::new(),
        "must not follow a letter"
    );
    assert_eq!(
        tags_in("(#one), #two/three-four_five."),
        ["one", "two/three-four_five"]
    );
    assert_eq!(tags_in("# #  ##"), Vec::<String>::new());
    assert_eq!(
        tags_in(&format!("#{}", "a".repeat(129))),
        Vec::<String>::new(),
        "too long"
    );
    assert_eq!(tags_in(&"#a ".repeat(5000)).len(), 1000);
}

#[test]
fn similar_edges_are_computed_labelled_and_skip_made_pairs() {
    let shared = "bridge pylon tension cable span anchor deck";
    let pages = vec![
        page(
            "a",
            None,
            "One",
            vec![block("1", BlockKind::Paragraph, vec![run(shared)], None)],
        ),
        page(
            "b",
            None,
            "Two",
            vec![block("1", BlockKind::Paragraph, vec![run(shared)], None)],
        ),
        page(
            "c",
            None,
            "Three",
            vec![block(
                "1",
                BlockKind::Paragraph,
                vec![run("完全不同的内容 orange lemon lime grape")],
                None,
            )],
        ),
        page(
            "d",
            None,
            "Four",
            vec![
                block("1", BlockKind::Paragraph, vec![run(shared)], None),
                block("2", BlockKind::Paragraph, vec![link("one", "a")], None),
            ],
        ),
    ];
    let g = derive(&pages, &[]);
    let sim: Vec<&Edge> = g
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Similar)
        .collect();
    assert!(sim.iter().any(|e| e.from == "page:a" && e.to == "page:b"));
    assert!(sim.iter().all(|e| matches!(e.origin, Origin::Computed { method, score } if method == WORD_OVERLAP_METHOD && score > 0)));
    assert!(
        sim.iter()
            .all(|e| !(e.from == "page:a" && e.to == "page:d")),
        "a linked pair gets no similar edge"
    );
    assert!(
        sim.iter().all(|e| e.from != "page:c" && e.to != "page:c"),
        "unrelated text stays apart"
    );
    assert!(
        g.edges
            .iter()
            .filter(|e| e.kind != EdgeKind::Similar)
            .all(|e| e.origin == Origin::Made)
    );
    let json = serde_json::to_string(&sim[0]).unwrap();
    assert!(
        json.contains("\"origin\":\"computed\"") && json.contains("word-overlap-v1"),
        "{json}"
    );
    assert!(!WORD_OVERLAP_METHOD.contains("embed"));
}

#[test]
fn word_overlap_works_without_spaces_and_is_bounded() {
    let text = "今天我们讨论知识图谱的设计与实现方案";
    let pages = vec![
        page(
            "a",
            None,
            "",
            vec![block("1", BlockKind::Paragraph, vec![run(text)], None)],
        ),
        page(
            "b",
            None,
            "",
            vec![block("1", BlockKind::Paragraph, vec![run(text)], None)],
        ),
    ];
    assert!(
        derive(&pages, &[])
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Similar)
    );

    // A thousand identical pages would be half a million pairs. Common terms are skipped instead.
    let many: Vec<Page> = (0..1000)
        .map(|i| {
            page(
                &format!("p{i:04}"),
                None,
                "same",
                vec![block(
                    "1",
                    BlockKind::Paragraph,
                    vec![run("same words on every single page")],
                    None,
                )],
            )
        })
        .collect();
    let g = derive(&many, &[]);
    assert!(g.edges.len() <= 2000, "{}", g.edges.len());
}

#[test]
fn hostile_graphs_do_not_panic() {
    let cyc1 = page(
        "a",
        Some("b"),
        "",
        vec![block("1", BlockKind::Embed, vec![], Some("a"))],
    );
    let cyc2 = page(
        "b",
        Some("a"),
        "",
        vec![
            block("1", BlockKind::Database, vec![], Some("none")),
            block("2", BlockKind::Embed, vec![], None),
        ],
    );
    let huge = page(
        "c",
        None,
        &"#".repeat(100_000),
        vec![block(
            "1",
            BlockKind::Paragraph,
            vec![run(&"#t ".repeat(100_000))],
            None,
        )],
    );
    let g = derive(&[cyc1, cyc2, huge], &[]);
    assert_eq!(has(&g, EdgeKind::Embed, "page:a", "page:a"), None);
    assert_eq!(has(&g, EdgeKind::Tag, "page:c", "tag:t"), Some(1000));
}
