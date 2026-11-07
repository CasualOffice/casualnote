use super::*;
use casual_note_model::{Block, Canvas, PageMeta, Run};

fn page(title: &str, blocks: &[(&str, BlockKind, &str)]) -> Page {
    Page {
        meta: PageMeta {
            id: "p".into(),
            parent: None,
            title: title.into(),
            favorite: false,
            trashed: false,
            database: None,
        },
        blocks: blocks
            .iter()
            .map(|(id, kind, text)| Block {
                id: (*id).into(),
                kind: *kind,
                checked: false,
                runs: vec![Run {
                    text: (*text).into(),
                    marks: vec![],
                    link: None,
                }],
                target: None,
            })
            .collect(),
        view: "page".into(),
        canvas: Canvas::default(),
    }
}

fn chunk(key: &str, hash: &str) -> Chunk {
    Chunk {
        key: key.into(),
        input: String::new(),
        hash: hash.into(),
        weight: 1,
    }
}

#[test]
fn chunks_carry_the_nearest_heading_and_skip_what_has_no_words() {
    let p = page(
        "旅行",
        &[
            ("h", BlockKind::Heading2, "持ち物"),
            ("a", BlockKind::Paragraph, "パスポート"),
            ("d", BlockKind::Divider, ""),
            ("n", BlockKind::Paragraph, "---"),
        ],
    );
    let c = chunks(&p);
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].key, "h#0");
    assert_eq!(c[1].input, "旅行. 持ち物. パスポート");
}

#[test]
fn a_long_block_splits_on_graphemes_in_every_script_and_loses_nothing() {
    for unit in [
        "नमस्ते दुनिया। ",
        "你好，世界。",
        "مرحبا بالعالم. ",
        "Hello world. ",
    ] {
        let text = unit.repeat(200);
        let c = chunks(&page("", &[("a", BlockKind::Paragraph, &text)]));
        assert!(c.len() > 1, "{unit}");
        assert!(
            c.iter()
                .all(|x| x.input.graphemes(true).count() <= INPUT_BUDGET)
        );
        assert_eq!(c.iter().map(|x| x.input.as_str()).collect::<String>(), text);
    }
}

#[test]
fn an_edit_replans_only_the_changed_chunk_and_restoring_costs_nothing() {
    let mut v = Vectors::new("m", 2);
    let before = chunks(&page(
        "t",
        &[
            ("a", BlockKind::Paragraph, "one"),
            ("b", BlockKind::Paragraph, "two"),
        ],
    ));
    assert_eq!(v.plan("p", &before).len(), 2);
    for c in &before {
        v.submit("p", c, &[1.0, 0.0]).unwrap();
    }
    assert!(v.plan("p", &before).is_empty());
    let edited = chunks(&page(
        "t",
        &[
            ("a", BlockKind::Paragraph, "one!"),
            ("b", BlockKind::Paragraph, "two"),
        ],
    ));
    let todo = v.plan("p", &edited);
    assert_eq!(todo.len(), 1);
    assert_eq!(todo[0].key, "a#0");
}

#[test]
fn a_deleted_block_leaves_results_and_its_slot_is_reused() {
    let mut v = Vectors::new("m", 2);
    v.submit("p", &chunk("a", "1"), &[1.0, 0.0]).unwrap();
    v.submit("p", &chunk("b", "2"), &[0.0, 1.0]).unwrap();
    assert!(v.plan("p", &[chunk("b", "2")]).is_empty());
    assert_eq!(v.len(), 1);
    assert_eq!(
        v.nearest(&[1.0, 0.0], 5)
            .unwrap()
            .iter()
            .filter(|n| n.key == "a")
            .count(),
        0
    );
    v.submit("q", &chunk("c", "3"), &[1.0, 0.0]).unwrap();
    assert_eq!(v.data.len(), 4);
    assert_eq!(v.nearest(&[1.0, 0.0], 1).unwrap()[0].page, "q");
}

#[test]
fn nearest_is_exact_and_ties_do_not_depend_on_insertion_order() {
    let mut a = Vectors::new("m", 2);
    let mut b = Vectors::new("m", 2);
    let rows = [("x", [1.0, 0.0]), ("y", [1.0, 0.0]), ("z", [0.0, 1.0])];
    for (k, r) in rows {
        a.submit("p", &chunk(k, k), &r).unwrap();
    }
    for (k, r) in rows.iter().rev() {
        b.submit("p", &chunk(k, k), r).unwrap();
    }
    assert_eq!(
        a.nearest(&[1.0, 0.0], 2).unwrap(),
        b.nearest(&[1.0, 0.0], 2).unwrap()
    );
}

#[test]
fn bad_vectors_are_refused_with_their_codes() {
    let mut v = Vectors::new("m", 2);
    assert_eq!(
        v.submit("p", &chunk("a", "1"), &[1.0]).unwrap_err().code(),
        "CN-EMB-0003"
    );
    assert_eq!(
        v.submit("p", &chunk("a", "1"), &[f32::NAN, 0.0])
            .unwrap_err()
            .code(),
        "CN-EMB-0004"
    );
    assert!(v.is_empty());
}

#[test]
fn dump_round_trips_and_another_models_file_is_not_read() {
    let mut v = Vectors::new("m", 2);
    v.submit("p", &chunk("a", "1"), &[0.6, 0.8]).unwrap();
    v.submit("q", &chunk("b", "2"), &[0.0, 1.0]).unwrap();
    let bytes = v.dump();
    let back = Vectors::load("m", 2, &bytes).unwrap();
    assert_eq!(
        back.nearest(&[0.0, 1.0], 2).unwrap(),
        v.nearest(&[0.0, 1.0], 2).unwrap()
    );
    assert_eq!(
        Vectors::load("other", 2, &bytes).unwrap_err(),
        EmbedError::WrongTag
    );
    assert_eq!(
        Vectors::load("m", 3, &bytes).unwrap_err(),
        EmbedError::WrongTag
    );
}

#[test]
fn damaged_files_error_and_never_panic() {
    let mut v = Vectors::new("m", 2);
    v.submit("p", &chunk("a", "1"), &[0.6, 0.8]).unwrap();
    let good = v.dump();
    for cut in 0..good.len() {
        assert!(Vectors::load("m", 2, &good[..cut]).is_err());
    }
    let mut huge = good.clone();
    let at = MAGIC.len() + 4 + 1 + 4;
    huge[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        Vectors::load("m", 2, &huge).unwrap_err().code(),
        "CN-EMB-0005"
    );
    let mut nan = good;
    let end = nan.len();
    nan[end - 4..].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(Vectors::load("m", 2, &nan).is_err());
}

#[test]
fn related_uses_page_means_and_respects_the_threshold() {
    let mut v = Vectors::new("m", 2);
    v.submit("a", &chunk("1", "1"), &[1.0, 0.0]).unwrap();
    v.submit("b", &chunk("2", "2"), &[0.9, 0.1]).unwrap();
    v.submit("c", &chunk("3", "3"), &[0.0, 1.0]).unwrap();
    let r = v.related("a", 5, 0.5);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].page, "b");
    assert!(v.related("missing", 5, 0.0).is_empty());
}
