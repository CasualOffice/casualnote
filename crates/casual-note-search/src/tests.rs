use super::*;
use casual_note_model::{Block, BlockKind, Column, Row, Run, Status};
use casual_note_ops::{Op, apply};

fn terms(text: &str) -> Vec<String> {
    tokenise(text)
        .into_iter()
        .filter(|t| t.primary)
        .map(|t| t.term)
        .collect()
}

fn doc(item: &str, text: &str) -> Doc {
    Doc {
        item: item.into(),
        field: Field::Body,
        text: text.into(),
    }
}

fn found(index: &Index, query: &str) -> Vec<String> {
    index
        .search(query, 50)
        .into_iter()
        .map(|h| h.item)
        .collect()
}

#[test]
fn tokeniser_follows_the_script() {
    assert_eq!(terms("Hello, WORLD don't"), ["hello", "world", "don't"]);
    assert_eq!(terms("你好世界"), ["你好", "好世", "世界"]);
    assert_eq!(terms("こんにちは"), ["こん", "んに", "にち", "ちは"]);
    assert_eq!(terms("カタカナ"), ["カタ", "タカ", "カナ"]);
    assert_eq!(terms("iPhone手机"), ["iphone", "手机"]);
    assert_eq!(terms("你好，世界"), ["你好", "世界"]);
    // A Thai vowel sign stays with its consonant, because bigrams are of graphemes.
    assert_eq!(terms("กำลัง"), ["กำลั", "ลัง"]);
    assert_eq!(terms("2026年11月"), ["2026", "年", "11", "月"]);
    assert_eq!(terms("Café 🎉"), ["café", "🎉"]);
}

#[test]
fn matching_copy_normalises_but_keeps_marks() {
    // NFC for matching: composed and decomposed agree.
    assert_eq!(terms("cafe\u{301}"), terms("caf\u{e9}"));
    // Marks are kept: Hindi vowel signs and the virama are part of the word.
    assert_eq!(terms("नमस्ते"), ["नमस्ते"]);
    assert_ne!(terms("नमस्ते"), terms("नमसत"));
    // Arabic keeps its marks in the primary term.
    assert_eq!(terms("مَرْحَبًا"), ["مَرْحَبًا"]);
    // Invisible direction marks do not hide a word.
    assert_eq!(terms("\u{200f}مرحبا\u{200e}"), ["مرحبا"]);
    // Full case folding.
    assert_eq!(fold("Straße"), fold("STRASSE"));
    // Offsets point at the original bytes.
    let t = tokenise("x cafe\u{301} y");
    let cafe = t.iter().find(|t| t.term == "café").unwrap();
    assert_eq!(&"x cafe\u{301} y"[cafe.start..cafe.end], "cafe\u{301}");
}

fn launch_index() -> Index {
    let mut index = Index::new();
    index.update(
        &Owner::Page("p".into()),
        vec![
            doc("ar", "مرحبا بالعالم الكتاب الجديد"),
            doc("ar2", "مَرْحَبًا"),
            doc("hi", "नमस्ते दुनिया और हिंदी लेख"),
            doc("zh", "我们今天发布新的知识图谱功能"),
            doc("ja", "こんにちは世界。日本語のノートを書きます"),
            doc("ko", "한국어를 배우는 학생입니다"),
            doc("es", "El niño comió en la cafetería de Peñalver"),
            doc("en", "The quick brown fox jumps over the lazy dog"),
        ],
    );
    index
}

#[test]
fn every_launch_language_is_findable() {
    let index = launch_index();
    for (query, want) in [
        ("بالعالم", "ar"),
        ("الكتاب", "ar"),
        ("दुनिया", "hi"),
        ("हिंदी लेख", "hi"),
        ("知识图谱", "zh"),
        ("今天", "zh"),
        ("日本語", "ja"),
        ("ノート", "ja"),
        ("こんにちは", "ja"),
        ("한국어", "ko"),
        ("학생", "ko"),
        ("niño", "es"),
        ("NIÑO", "es"),
        ("cafetería", "es"),
        ("quick fox", "en"),
        ("JUMPS", "en"),
        ("qui", "en"),
    ] {
        assert_eq!(found(&index, query), [want], "{query}");
    }
}

#[test]
fn marks_are_optional_only_where_the_table_says() {
    let index = launch_index();
    // A search without harakat finds both, the exact spelling first.
    assert_eq!(found(&index, "مرحبا"), ["ar", "ar2"]);
    // A search with harakat means what it says.
    assert_eq!(found(&index, "مَرْحَبًا"), ["ar2"]);
    assert_eq!(found(&index, "nino"), ["es"]);
    assert!(found(&index, "नमसत").is_empty());
    assert!(found(&index, "cat").is_empty());
}

#[test]
fn single_characters_and_punctuation() {
    let index = launch_index();
    assert_eq!(found(&index, "能"), ["zh"], "the last character of a run");
    assert_eq!(found(&index, "图"), ["zh"], "a character in the middle");
    assert!(found(&index, "!!!").is_empty());
    assert!(found(&index, "").is_empty());
}

#[test]
fn bm25_ranks_rarer_and_weightier_first_with_exact_scores() {
    let mut index = Index::new();
    index.update(
        &Owner::Page("a".into()),
        vec![
            Doc {
                item: String::new(),
                field: Field::Title,
                text: "Import plan".into(),
            },
            doc(
                "b1",
                "We talk about import and import and import here at some length today",
            ),
            doc("b2", "Nothing related at all in this block of text"),
        ],
    );
    let hits = index.search("import", 10);
    assert_eq!(hits[0].field, Field::Title, "a title outranks body text");
    assert_eq!(hits[0].marks, [[0, 6]]);
    assert!(hits[1].score > 0 && hits[0].score > hits[1].score);
    // Same score whenever it runs: an exact value is what makes native and wasm comparable.
    assert_eq!(
        (hits[0].score, hits[1].score),
        (
            index.search("import", 10)[0].score,
            index.search("import", 10)[1].score
        )
    );
}

#[test]
fn stable_ln_is_accurate() {
    for x in [0.5, 1.0, 2.0, 3.5, 10.0, 1e6, 1e-6] {
        assert!((stable_ln(x) - f64::ln(x)).abs() < 1e-11, "{x}");
    }
    assert_eq!(stable_ln(0.0), 0.0);
    assert_eq!(stable_ln(-1.0), 0.0);
    assert_eq!(stable_ln(f64::NAN), 0.0);
}

#[test]
fn terms_spread_over_one_page_still_find_it() {
    let mut index = Index::new();
    index.update(
        &Owner::Page("a".into()),
        vec![doc("1", "alpha one"), doc("2", "beta two")],
    );
    index.update(&Owner::Page("b".into()), vec![doc("3", "alpha only")]);
    let hits = index.search("alpha beta", 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "a");
}

#[test]
fn incremental_update_equals_a_fresh_build() {
    let (mut live, mut fresh) = (Index::new(), Index::new());
    let a = Owner::Page("a".into());
    let b = Owner::Page("b".into());
    live.update(&a, vec![doc("1", "old words here 旧文本")]);
    live.update(&b, vec![doc("2", "stable text about words")]);
    live.update(&a, vec![doc("1", "new words here 新文本")]);
    fresh.update(&b, vec![doc("2", "stable text about words")]);
    fresh.update(&a, vec![doc("1", "new words here 新文本")]);
    assert!(live.search("old", 10).is_empty());
    assert!(live.search("旧文", 10).is_empty());
    for q in ["words", "new", "新文", "stable", "text"] {
        assert_eq!(live.search(q, 10), fresh.search(q, 10), "{q}");
    }
    assert_eq!(
        live.postings.len(),
        fresh.postings.len(),
        "nothing stale is left behind"
    );
    live.remove(&a);
    live.remove(&b);
    assert!(live.is_empty() && live.postings.is_empty() && live.total_length == 0);
}

#[test]
fn hostile_input_is_bounded() {
    let mut index = Index::new();
    let huge = "a".repeat(5_000_000);
    let many = "x ".repeat(2_000_000);
    let marks = format!("e{}", "\u{301}".repeat(100_000));
    let han = "漢".repeat(300_000);
    index.update(
        &Owner::Page("p".into()),
        vec![
            doc("1", &huge),
            doc("2", &many),
            doc("3", &marks),
            doc("4", &han),
            doc("5", "\u{0}\u{1}\u{202e}\u{feff}"),
        ],
    );
    assert!(index.search("x", 5).len() == 1);
    assert_eq!(index.search(&"q ".repeat(1_000_000), 5).len(), 0);
    assert_eq!(index.search("漢漢", 500_000).len(), 1);
    assert!(tokenise(&huge).iter().all(|t| t.term.chars().count() <= 64));
    assert!(tokenise(&many).len() <= 200_000);
    assert!(query_terms(&"a b c d e f g h ".repeat(100)).len() <= 32);
}

fn workspace() -> Workspace {
    let mut ws = Workspace::new(7).unwrap();
    let block = |id: &str, kind, text: &str| Block {
        id: id.into(),
        kind,
        checked: false,
        runs: vec![Run {
            text: text.into(),
            marks: vec![],
            link: None,
        }],
        target: None,
    };
    let database = Database {
        id: "d1".into(),
        name: "Tasks".into(),
        view: "table".into(),
        statuses: vec![Status {
            id: "s".into(),
            name: "Open".into(),
            group: "todo".into(),
        }],
        columns: vec![
            Column {
                id: "owner".into(),
                name: "Owner".into(),
                kind: "text".into(),
                hidden: false,
            },
            Column {
                id: "gone".into(),
                name: "Gone".into(),
                kind: "text".into(),
                hidden: true,
            },
        ],
        rows: vec![Row {
            id: "r1".into(),
            name: "Write the release notes".into(),
            status: "s".into(),
            values: [
                ("owner".to_string(), "Mei".to_string()),
                ("gone".to_string(), "removedvalue".to_string()),
            ]
            .into(),
        }],
    };
    let ops = [
        Op::CreatePage {
            id: "p1".into(),
            parent: None,
            title: "Launch 计划".into(),
            database: None,
        },
        Op::InsertBlock {
            page: "p1".into(),
            after: None,
            block: block("b1", BlockKind::Heading1, "مرحبا"),
        },
        Op::InsertBlock {
            page: "p1".into(),
            after: Some("b1".into()),
            block: block("b2", BlockKind::Paragraph, "一二三四 hello"),
        },
        Op::CreateDatabase { database },
        Op::CreatePage {
            id: "p2".into(),
            parent: None,
            title: "Tracker".into(),
            database: Some("d1".into()),
        },
    ];
    for op in &ops {
        apply(&mut ws, op).unwrap();
    }
    ws
}

#[test]
fn workspace_search_follows_edits_and_trash_and_rows() {
    let mut ws = workspace();
    let mut idx = WorkspaceIndex::new();
    let go = |idx: &mut WorkspaceIndex, ws: &Workspace, q: &str| -> Vec<(String, String)> {
        idx.search(ws, q, 10)
            .into_iter()
            .map(|r| (r.page, r.hit.item))
            .collect()
    };
    assert_eq!(
        go(&mut idx, &ws, "مرحبا"),
        [("p1".to_string(), "b1".to_string())]
    );
    assert_eq!(
        go(&mut idx, &ws, "计划"),
        [("p1".to_string(), String::new())]
    );
    assert_eq!(
        go(&mut idx, &ws, "release"),
        [("p2".to_string(), "r1".to_string())],
        "a row opens its page"
    );
    assert_eq!(
        go(&mut idx, &ws, "Mei"),
        [("p2".to_string(), "r1".to_string())],
        "a cell opens its page"
    );
    assert!(
        go(&mut idx, &ws, "removedvalue").is_empty(),
        "a removed column is not searched"
    );

    let edit = Op::SpliceText {
        page: "p1".into(),
        block: "b2".into(),
        at: 0,
        delete: 0,
        insert: "zebra ".into(),
    };
    apply(&mut ws, &edit).unwrap();
    assert!(
        go(&mut idx, &ws, "zebra").is_empty(),
        "an untouched index has not seen the edit"
    );
    idx.touch(&edit);
    assert_eq!(go(&mut idx, &ws, "zebra").len(), 1);

    let trash = Op::SetTrashed {
        id: "p1".into(),
        value: true,
    };
    apply(&mut ws, &trash).unwrap();
    idx.touch(&trash);
    assert!(
        go(&mut idx, &ws, "zebra").is_empty(),
        "trash leaves results"
    );
    let restore = Op::SetTrashed {
        id: "p1".into(),
        value: false,
    };
    apply(&mut ws, &restore).unwrap();
    idx.touch(&restore);
    assert_eq!(go(&mut idx, &ws, "zebra").len(), 1);

    let fresh = WorkspaceIndex::new();
    let mut fresh = fresh;
    fresh.sync(&ws);
    idx.sync(&ws);
    for q in ["zebra", "مرحبا", "release", "tracker", "hello"] {
        assert_eq!(
            idx.index().search(q, 10),
            fresh.index().search(q, 10),
            "{q}"
        );
    }
}

/// webapp/tests/knowledge.test.js checks the same corpus for the same scores in real wasm.
#[test]
fn golden_scores_shared_with_the_browser_test() {
    let mut ws = Workspace::new(9).unwrap();
    for (id, title, text) in [
        (
            "g1",
            "Import 导入",
            "导入 العربية नमस्ते こんにちは 한국어 niño import",
        ),
        ("g2", "Notes", "import import plan 导入数据"),
        ("g3", "Other", "nothing here"),
    ] {
        apply(
            &mut ws,
            &Op::CreatePage {
                id: id.into(),
                parent: None,
                title: title.into(),
                database: None,
            },
        )
        .unwrap();
        apply(
            &mut ws,
            &Op::InsertBlock {
                page: id.into(),
                after: None,
                block: Block {
                    id: format!("{id}b"),
                    kind: BlockKind::Paragraph,
                    checked: false,
                    runs: vec![Run {
                        text: text.into(),
                        marks: vec![],
                        link: None,
                    }],
                    target: None,
                },
            },
        )
        .unwrap();
    }
    let mut idx = WorkspaceIndex::new();
    let got: Vec<String> = [
        "import",
        "导入",
        "العربية",
        "한국어",
        "नमस्ते",
        "こんにちは",
        "nino",
    ]
    .iter()
    .flat_map(|q| {
        idx.search(&ws, q, 10)
            .into_iter()
            .map(|r| format!("{q}:{}:{}:{}", r.page, r.hit.item, r.hit.score))
            .collect::<Vec<_>>()
    })
    .collect();
    assert_eq!(got, golden());
}

fn golden() -> Vec<String> {
    GOLDEN.iter().map(|s| (*s).to_string()).collect()
}

const GOLDEN: &[&str] = &[
    "import:g1::3405921",
    "import:g2:g2b:808393",
    "import:g1:g1b:406154",
    "导入:g1::3405921",
    "导入:g2:g2b:549973",
    "导入:g1:g1b:406154",
    "العربية:g1:g1b:902634",
    "한국어:g1:g1b:1041306",
    "नमस्ते:g1:g1b:902634",
    "こんにちは:g1:g1b:3610535",
    "nino:g1:g1b:520653",
];
