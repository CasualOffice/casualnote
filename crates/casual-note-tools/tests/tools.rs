//! The tool set, driven through its public face, including with hostile arguments.

use casual_note_merge::Workspace;
use casual_note_model::{BlockKind, Column, Database, Row, Run, Status};
use casual_note_ops::{Op, apply, paragraph};
use casual_note_tools::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn page(ws: &mut Workspace, id: &str, title: &str, text: &[&str]) {
    apply(
        ws,
        &Op::CreatePage {
            id: id.into(),
            parent: None,
            title: title.into(),
            database: None,
        },
    )
    .unwrap();
    let mut after = None;
    for (i, t) in text.iter().enumerate() {
        let bid = format!("{id}-b{i}");
        apply(
            ws,
            &Op::InsertBlock {
                page: id.into(),
                after: after.replace(bid.clone()),
                block: paragraph(&bid, t),
            },
        )
        .unwrap();
    }
}

fn database(ws: &mut Workspace) {
    let status = |id: &str, name: &str, group: &str| Status {
        id: id.into(),
        name: name.into(),
        group: group.into(),
    };
    let column = |id: &str, name: &str, kind: &str| Column {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        hidden: false,
    };
    apply(
        ws,
        &Op::CreateDatabase {
            database: Database {
                id: "db1".into(),
                name: "Launch tasks".into(),
                view: "table".into(),
                statuses: vec![
                    status("s0", "Doing", "active"),
                    status("s1", "To do", "todo"),
                ],
                columns: vec![column("c1", "Owner", "text"), column("c2", "Due", "date")],
                rows: vec![Row {
                    id: "r1".into(),
                    name: "Write the plan".into(),
                    status: "s0".into(),
                    values: BTreeMap::from([("c1".to_owned(), "Priya".to_owned())]),
                }],
            },
        },
    )
    .unwrap();
    apply(
        ws,
        &Op::CreatePage {
            id: "dbpage".into(),
            parent: None,
            title: "Launch tasks".into(),
            database: Some("db1".into()),
        },
    )
    .unwrap();
}

fn sample() -> Workspace {
    let mut ws = Workspace::new(7).unwrap();
    page(
        &mut ws,
        "p1",
        "Launch plan",
        &["Ship in June", "مرحبا بالعالم"],
    );
    page(&mut ws, "p2", "Notes", &["first"]);
    page(&mut ws, "p3", "Notes", &["second"]);
    database(&mut ws);
    ws
}

fn ids() -> impl FnMut() -> String {
    let mut n = 0;
    move || {
        n += 1;
        format!("new{n}")
    }
}

fn read(ws: &Workspace, name: &str, args: Value) -> Value {
    run_read(ws, name, &args)
        .map(|o| o.result)
        .unwrap_or_else(|r| r.to_json())
}

fn code(v: &Value) -> &str {
    v["code"].as_str().unwrap_or("")
}

fn refuse(ws: &Workspace, name: &str, args: Value) -> Refusal {
    prepare_write(ws, name, &args, &mut ids()).unwrap_err()
}

#[test]
fn the_set_is_closed_with_flags_and_strict_schemas() {
    let names: Vec<_> = TOOLS.iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "list_pages",
            "search_notes",
            "read_page",
            "related_pages",
            "graph_neighbors",
            "list_tasks",
            "create_page",
            "append_to_page",
            "add_task"
        ]
    );
    assert_eq!(definitions(true).len(), 6);
    assert!(definitions(true).iter().all(|t| t.access == Access::Read));
    for t in &TOOLS {
        let schema = t.input_schema(&[]);
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["type"], "object");
        let shown = t.to_json(&[]);
        assert_eq!(
            shown["annotations"]["readOnlyHint"],
            t.access == Access::Read
        );
        assert_eq!(shown["annotations"]["openWorldHint"], false);
    }
}

#[test]
fn schema_and_check_agree_for_every_tool() {
    let ws = sample();
    for t in &TOOLS {
        let schema = t.input_schema(&[]);
        let call = |args: &Value| match t.access {
            Access::Read => run_read(&ws, t.name, args).map(|_| ()),
            Access::Write => prepare_write(&ws, t.name, args, &mut ids()).map(|_| ()),
        };
        // A property the schema does not list is refused.
        let mut extra = json!({ "surprise": "x" });
        for r in schema["required"].as_array().unwrap() {
            extra[r.as_str().unwrap()] = json!("p1");
        }
        assert_eq!(call(&extra).unwrap_err().code, "CN-TOL-0002", "{}", t.name);
        // Every required property is enforced.
        for r in schema["required"].as_array().unwrap() {
            let r = r.as_str().unwrap();
            let mut args = json!({});
            for other in schema["required"].as_array().unwrap() {
                if other != r {
                    args[other.as_str().unwrap()] = json!("p1");
                }
            }
            let got = call(&args).unwrap_err();
            assert_eq!(got.code, "CN-TOL-0002", "{} {r}", t.name);
            assert!(got.message.contains(r));
        }
        // Every limit in the schema is enforced.
        for (name, prop) in schema["properties"].as_object().unwrap() {
            let max = prop["maxLength"].as_u64().unwrap() as usize;
            let args = json!({ name.clone(): "x".repeat(max + 1) });
            if t.fields
                .iter()
                .filter(|f| f.required)
                .all(|f| f.name == name)
            {
                assert_eq!(
                    call(&args).unwrap_err().code,
                    "CN-TOL-0007",
                    "{} {name}",
                    t.name
                );
            }
        }
    }
}

#[test]
fn hostile_arguments_are_refused_without_a_panic() {
    let ws = sample();
    for bad in [
        json!(null),
        json!([]),
        json!(7),
        json!("page"),
        json!({ "page": 7 }),
        json!({ "page": ["p1"] }),
        json!({ "page": { "$ne": "" } }),
        json!({ "page": "a\u{0}b" }),
    ] {
        assert_eq!(code(&read(&ws, "read_page", bad)), "CN-TOL-0002");
    }
    assert_eq!(code(&read(&ws, "rm_rf", json!({}))), "CN-TOL-0001");
    assert_eq!(code(&read(&ws, "create_page", json!({}))), "CN-TOL-0001");
    assert!(prepare_write(&ws, "read_page", &json!({ "page": "p1" }), &mut ids()).is_err());
    // A long tool name is not echoed in full.
    let long = "x".repeat(100_000);
    assert!(run_read(&ws, &long, &json!({})).unwrap_err().message.len() < 400);
    // Malformed, deep and huge raw arguments.
    assert_eq!(parse_args("{not json").unwrap_err().code, "CN-TOL-0002");
    assert_eq!(
        parse_args(&"[".repeat(100_000)).unwrap_err().code,
        "CN-TOL-0002"
    );
    assert_eq!(
        parse_args(&" ".repeat(MAX_ARGS_BYTES + 1))
            .unwrap_err()
            .code,
        "CN-TOL-0007"
    );
    assert_eq!(parse_args("").unwrap(), json!({}));
    // Bad cursors of every shape.
    for c in [
        "x",
        "c",
        "c-1",
        "c99999",
        "c1e9",
        "c340282366920938463463374607431768211456",
        "c٣",
    ] {
        assert_eq!(
            code(&read(&ws, "list_pages", json!({ "cursor": c }))),
            "CN-TOL-0013",
            "{c}"
        );
    }
    for c in ["x", "b", "b999", "b0.", "b.1", "b0.99999", "b-1"] {
        assert_eq!(
            code(&read(
                &ws,
                "read_page",
                json!({ "page": "p1", "cursor": c })
            )),
            "CN-TOL-0013",
            "{c}"
        );
    }
    assert_eq!(
        code(&read(&ws, "search_notes", json!({ "query": "   " }))),
        "CN-TOL-0002"
    );
    // A wildcard is text, and an id that looks like a path resolves to nothing.
    assert_eq!(
        read(&ws, "search_notes", json!({ "query": ".*" }))["data"],
        json!([])
    );
    assert_eq!(
        code(&read(
            &ws,
            "read_page",
            json!({ "page": "../../etc/passwd" })
        )),
        "CN-TOL-0003"
    );
}

#[test]
fn pages_are_found_by_id_or_exact_title_and_never_guessed() {
    let mut ws = sample();
    let r = read(&ws, "read_page", json!({ "page": "launch PLAN" }));
    assert_eq!(r["data"]["id"], "p1");
    assert!(
        r["data"]["markdown"]
            .as_str()
            .unwrap()
            .contains("مرحبا بالعالم")
    );
    let two = run_read(&ws, "read_page", &json!({ "page": "Notes" })).unwrap_err();
    assert_eq!(two.code, "CN-TOL-0004");
    assert_eq!(two.candidates.len(), 2);
    assert_eq!(
        code(&read(&ws, "read_page", json!({ "page": "Launch" }))),
        "CN-TOL-0003"
    );
    apply(
        &mut ws,
        &Op::SetTrashed {
            id: "p1".into(),
            value: true,
        },
    )
    .unwrap();
    assert_eq!(
        code(&read(&ws, "read_page", json!({ "page": "p1" }))),
        "CN-TOL-0009"
    );
    assert_eq!(
        code(&read(&ws, "read_page", json!({ "page": "Launch plan" }))),
        "CN-TOL-0009"
    );
    let list = read(&ws, "list_pages", json!({}));
    assert!(
        list["data"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["id"] != "p1")
    );
}

#[test]
fn search_is_bounded_truncation_is_stated_and_the_cursor_continues() {
    let mut ws = Workspace::new(9).unwrap();
    let many: Vec<String> = (0..30).map(|i| format!("alpha {i}")).collect();
    let refs: Vec<&str> = many.iter().map(String::as_str).collect();
    page(&mut ws, "p", "Page", &refs);
    let first = read(&ws, "search_notes", json!({ "query": "ALPHA" }));
    assert_eq!(first["data"].as_array().unwrap().len(), MAX_HITS);
    assert_eq!(first["truncated"]["total"], 30);
    assert_eq!(first["truncated"]["returned"], 20);
    assert!(first["notes"][0].as_str().unwrap().contains("meaning"));
    let next = first["truncated"]["next_cursor"].as_str().unwrap();
    let second = read(
        &ws,
        "search_notes",
        json!({ "query": "alpha", "cursor": next }),
    );
    assert_eq!(second["data"].as_array().unwrap().len(), 10);
    assert!(second.get("truncated").is_none());
    // A snippet is bounded and keeps the match.
    let long = format!("{}needle{}", "é".repeat(5000), "क्ष".repeat(5000));
    page(&mut ws, "q", "Long", &[&long]);
    let hit = read(&ws, "search_notes", json!({ "query": "needle" }));
    let snip = hit["data"][0]["snippet"].as_str().unwrap();
    assert!(snip.contains("needle"));
    assert!(snip.chars().count() < 800);
}

#[test]
fn search_folds_composed_and_decomposed_text() {
    let mut ws = Workspace::new(9).unwrap();
    page(&mut ws, "p", "Page", &["Cafe\u{301} au lait"]);
    let hit = read(&ws, "search_notes", json!({ "query": "CAFÉ" }));
    assert_eq!(hit["data"].as_array().unwrap().len(), 1);
    // Text comes back as written, not normalised.
    assert!(
        hit["data"][0]["snippet"]
            .as_str()
            .unwrap()
            .contains("Cafe\u{301}")
    );
}

#[test]
fn a_long_page_reads_in_pieces_at_block_boundaries_with_nothing_lost() {
    let mut ws = Workspace::new(9).unwrap();
    let blocks: Vec<String> = (0..1500)
        .map(|i| format!("line {i:05} {}", "x".repeat(40)))
        .collect();
    let refs: Vec<&str> = blocks.iter().map(String::as_str).collect();
    let huge = "界".repeat(50_000);
    let mut with_huge = refs.clone();
    with_huge.push(&huge);
    page(&mut ws, "p", "Long", &with_huge);
    let mut cursor = None;
    let mut all = String::new();
    let mut calls = 0;
    loop {
        let mut args = json!({ "page": "p" });
        if let Some(c) = &cursor {
            args["cursor"] = json!(c);
        }
        let r = read(&ws, "read_page", args);
        assert!(r.to_string().len() <= MAX_RESULT_BYTES * 3 / 2, "bounded");
        let text = r["data"]["markdown"].as_str().unwrap();
        assert!(text.len() <= MAX_READ_BYTES);
        all.push_str(text);
        calls += 1;
        match r["truncated"]["next_cursor"].as_str() {
            Some(c) => cursor = Some(c.to_owned()),
            None => break,
        }
    }
    assert!(calls > 1);
    assert!(all.contains("line 01499"));
    assert_eq!(all.matches('界').count(), 50_000);
}

#[test]
fn list_is_bounded_by_count_and_bytes() {
    let mut ws = Workspace::new(9).unwrap();
    for i in 0..205 {
        page(&mut ws, &format!("p{i}"), "t", &[]);
    }
    let r = read(&ws, "list_pages", json!({}));
    assert_eq!(r["data"].as_array().unwrap().len(), MAX_LIST);
    assert_eq!(r["truncated"]["total"], 205);
    let mut ws = Workspace::new(9).unwrap();
    for i in 0..100 {
        page(&mut ws, &format!("p{i}"), &"t".repeat(5000), &[]);
    }
    let r = read(&ws, "list_pages", json!({}));
    assert!(r.to_string().len() <= MAX_RESULT_BYTES);
    assert_eq!(r["truncated"]["total"], 100);
}

#[test]
fn tasks_filter_and_name_their_database() {
    let ws = sample();
    let all = read(&ws, "list_tasks", json!({}));
    assert_eq!(all["data"][0]["name"], "Write the plan");
    assert_eq!(all["data"][0]["owner"], "Priya");
    assert_eq!(all["data"][0]["database"]["name"], "Launch tasks");
    assert_eq!(
        read(&ws, "list_tasks", json!({ "status": "active" }))["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        read(&ws, "list_tasks", json!({ "status": "done" }))["data"],
        json!([])
    );
    assert_eq!(
        read(&ws, "list_tasks", json!({ "owner": "priya" }))["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        code(&read(&ws, "list_tasks", json!({ "database": "nope" }))),
        "CN-TOL-0008"
    );
}

#[test]
fn prepare_changes_nothing_and_commit_is_one_undo_step() {
    let mut ws = sample();
    let before = ws.page("p1").unwrap();
    let pages_before = ws.pages();
    let args = json!({ "page": "p1", "text": "# Head\n\n- bullet\n* star\nplain <img src=x onerror=alert(1)>\n" });
    let p = prepare_write(&ws, "append_to_page", &args, &mut ids()).unwrap();
    assert_eq!(ws.page("p1").unwrap(), before);
    assert_eq!(ws.pages(), pages_before);
    assert_eq!(p.change["kind"], "append_to_page");
    assert_eq!(p.change["page"]["title"], "Launch plan");
    assert_eq!(p.change["total_blocks"], 4);
    let (result, undo) = commit_write(&mut ws, &p).unwrap();
    assert_eq!(result["data"]["blocks_added"], 4);
    let after = ws.page("p1").unwrap();
    let kinds: Vec<_> = after.blocks[2..].iter().map(|b| b.kind).collect();
    assert_eq!(
        kinds,
        [
            BlockKind::Heading1,
            BlockKind::Bulleted,
            BlockKind::Bulleted,
            BlockKind::Paragraph
        ]
    );
    // No HTML is parsed: tags stay literal text.
    assert_eq!(
        after.blocks[5].plain(),
        "plain <img src=x onerror=alert(1)>"
    );
    for op in &undo {
        apply(&mut ws, op).unwrap();
    }
    assert_eq!(ws.page("p1").unwrap(), before);
}

#[test]
fn create_page_previews_and_undoes_as_one_step() {
    let mut ws = sample();
    let args = json!({ "title": "Plan B", "text": "See [[Launch plan]] and [[Notes]] and [[Nothing]]", "parent": "p1" });
    let p = prepare_write(&ws, "create_page", &args, &mut ids()).unwrap();
    assert_eq!(p.change["title"], "Plan B");
    assert_eq!(p.change["parent"]["id"], "p1");
    assert!(ws.pages().iter().all(|m| m.title != "Plan B"));
    let (result, undo) = commit_write(&mut ws, &p).unwrap();
    let id = result["data"]["id"].as_str().unwrap();
    let made = ws.page(id).unwrap();
    assert_eq!(made.meta.parent.as_deref(), Some("p1"));
    let runs = &made.blocks[0].runs;
    // Resolves uniquely: a link. Ambiguous or missing: literal text.
    let link: Vec<&Run> = runs.iter().filter(|r| r.link.is_some()).collect();
    assert_eq!(link.len(), 1);
    assert_eq!(link[0].link.as_deref(), Some("p1"));
    assert_eq!(
        made.blocks[0].plain(),
        "See Launch plan and [[Notes]] and [[Nothing]]"
    );
    for op in &undo {
        apply(&mut ws, op).unwrap();
    }
    assert!(ws.meta(id).unwrap().trashed);
}

#[test]
fn a_new_page_with_no_text_still_has_a_block() {
    let mut ws = sample();
    let p = prepare_write(
        &ws,
        "create_page",
        &json!({ "title": "Empty", "text": "\n\n" }),
        &mut ids(),
    )
    .unwrap();
    assert_eq!(p.change["total_blocks"], 0);
    let (result, _) = commit_write(&mut ws, &p).unwrap();
    assert_eq!(result["data"]["blocks_added"], 0);
    assert_eq!(
        ws.page(result["data"]["id"].as_str().unwrap())
            .unwrap()
            .blocks
            .len(),
        1
    );
}

#[test]
fn writes_refuse_with_the_documented_codes() {
    let mut ws = sample();
    assert_eq!(
        refuse(
            &ws,
            "append_to_page",
            json!({ "page": "dbpage", "text": "x" })
        )
        .code,
        "CN-TOL-0011"
    );
    assert_eq!(
        refuse(
            &ws,
            "append_to_page",
            json!({ "page": "nope", "text": "x" })
        )
        .code,
        "CN-TOL-0003"
    );
    assert_eq!(
        refuse(
            &ws,
            "append_to_page",
            json!({ "page": "Notes", "text": "x" })
        )
        .code,
        "CN-TOL-0004"
    );
    assert_eq!(
        refuse(
            &ws,
            "append_to_page",
            json!({ "page": "p1", "text": "  \n " })
        )
        .code,
        "CN-TOL-0002"
    );
    assert_eq!(
        refuse(&ws, "create_page", json!({ "title": " ", "text": "x" })).code,
        "CN-TOL-0002"
    );
    assert_eq!(
        refuse(&ws, "create_page", json!({ "title": "a\nb", "text": "x" })).code,
        "CN-TOL-0002"
    );
    assert_eq!(
        refuse(
            &ws,
            "create_page",
            json!({ "title": "t", "text": "x", "parent": "nope" })
        )
        .code,
        "CN-TOL-0003"
    );
    assert_eq!(
        refuse(
            &ws,
            "create_page",
            json!({ "title": "t", "text": "a\u{1b}[31m" })
        )
        .code,
        "CN-TOL-0002"
    );
    let lines = "a\n".repeat(MAX_BLOCKS + 1);
    assert_eq!(
        refuse(
            &ws,
            "append_to_page",
            json!({ "page": "p1", "text": lines })
        )
        .code,
        "CN-TOL-0007"
    );
    assert_eq!(
        refuse(
            &ws,
            "append_to_page",
            json!({ "page": "p1", "text": "界".repeat(30_000) })
        )
        .code,
        "CN-TOL-0007"
    );
    assert_eq!(
        refuse(&ws, "add_task", json!({ "task": "t", "database": "nope" })).code,
        "CN-TOL-0008"
    );
    let names = refuse(&ws, "add_task", json!({ "task": "t", "database": "nope" }));
    assert_eq!(names.candidates[0]["name"], "Launch tasks");
    for due in [
        "tomorrow",
        "2026-13-01",
        "2026-02-30",
        "2025-02-29",
        "26-01-01",
        "2026/01/01",
        "२०२६-०१-०१",
        "2026-1-01",
    ] {
        assert_eq!(
            refuse(
                &ws,
                "add_task",
                json!({ "task": "t", "database": "db1", "due": due })
            )
            .code,
            "CN-TOL-0002",
            "{due}"
        );
    }
    apply(
        &mut ws,
        &Op::SetTrashed {
            id: "p1".into(),
            value: true,
        },
    )
    .unwrap();
    assert_eq!(
        refuse(&ws, "append_to_page", json!({ "page": "p1", "text": "x" })).code,
        "CN-TOL-0009"
    );
    assert_eq!(declined().code, "CN-TOL-0005");
}

#[test]
fn add_task_uses_the_first_todo_status_and_real_columns() {
    let mut ws = sample();
    let args = json!({ "task": "Test resume", "database": "launch TASKS", "owner": "Ana", "due": "2028-02-29" });
    let p = prepare_write(&ws, "add_task", &args, &mut ids()).unwrap();
    assert_eq!(p.ops.len(), 1);
    assert_eq!(p.change["status"], "To do");
    commit_write(&mut ws, &p).unwrap();
    let db = ws.database("db1").unwrap();
    let row = db.rows.last().unwrap();
    assert_eq!(row.name, "Test resume");
    assert_eq!(row.status, "s1");
    assert_eq!(row.values["c1"], "Ana");
    assert_eq!(row.values["c2"], "2028-02-29");
    // A database without the column refuses, with nothing written.
    apply(
        &mut ws,
        &Op::CreateDatabase {
            database: Database {
                id: "db2".into(),
                name: "Bare".into(),
                view: "table".into(),
                statuses: vec![],
                columns: vec![],
                rows: vec![],
            },
        },
    )
    .unwrap();
    assert_eq!(
        refuse(
            &ws,
            "add_task",
            json!({ "task": "t", "database": "db2", "owner": "A" })
        )
        .code,
        "CN-TOL-0014"
    );
    assert_eq!(
        refuse(
            &ws,
            "add_task",
            json!({ "task": "t", "database": "db2", "due": "2026-01-01" })
        )
        .code,
        "CN-TOL-0014"
    );
}

#[test]
fn the_preview_is_bounded_and_flags_hidden_direction_marks() {
    let ws = sample();
    let text: String = (0..30)
        .map(|i| format!("{i} {}\n", "क्ष".repeat(200)))
        .collect();
    let p = prepare_write(
        &ws,
        "create_page",
        &json!({ "title": "Sneaky \u{202E}txt.exe", "text": text }),
        &mut ids(),
    )
    .unwrap();
    assert_eq!(p.change["invisible_controls"], true);
    assert_eq!(p.change["total_blocks"], 30);
    let shown = p.change["blocks"].as_array().unwrap();
    assert_eq!(shown.len(), PREVIEW_BLOCKS);
    assert_eq!(shown[0]["truncated"], true);
    assert!(shown[0]["text"].as_str().unwrap().len() <= PREVIEW_BYTES);
    assert!(shown[0]["text_bytes"].as_u64().unwrap() > PREVIEW_BYTES as u64);
    let tag = prepare_write(
        &ws,
        "append_to_page",
        &json!({ "page": "p1", "text": "hello\u{E0041}" }),
        &mut ids(),
    )
    .unwrap();
    assert_eq!(tag.change["blocks"][0]["invisible_controls"], true);
}

#[test]
fn a_failed_batch_rolls_back_whole() {
    let mut ws = sample();
    let mut p = prepare_write(
        &ws,
        "append_to_page",
        &json!({ "page": "p1", "text": "a\nb" }),
        &mut ids(),
    )
    .unwrap();
    let before = ws.page("p1").unwrap();
    if let Op::InsertBlock { after, .. } = &mut p.ops[1] {
        *after = Some("gone".into());
    }
    assert_eq!(commit_write(&mut ws, &p).unwrap_err().code, "CN-TOL-0010");
    assert_eq!(ws.page("p1").unwrap(), before);
}

#[test]
fn json_faces_always_answer_and_ops_read_back() {
    let mut ws = sample();
    let defs: Value = serde_json::from_str(&definitions_json(false)).unwrap();
    assert_eq!(defs.as_array().unwrap().len(), 9);
    let r: Value = serde_json::from_str(&read_json(&ws, "read_page", r#"{"page":"p1"}"#)).unwrap();
    assert_eq!(r["pages_read"][0]["id"], "p1");
    let r: Value = serde_json::from_str(&read_json(&ws, "read_page", "{oops")).unwrap();
    assert_eq!(r["result"]["code"], "CN-TOL-0002");
    let p: Value = serde_json::from_str(&prepare_json(
        &ws,
        "append_to_page",
        r#"{"page":"p1","text":"x"}"#,
        "seed",
    ))
    .unwrap();
    assert_eq!(p["ok"], true);
    let again: Value = serde_json::from_str(&prepare_json(
        &ws,
        "append_to_page",
        r#"{"page":"p1","text":"x"}"#,
        "seed",
    ))
    .unwrap();
    assert_eq!(p, again);
    for op in p["ops"].as_array().unwrap() {
        let op: Op = serde_json::from_value(op.clone()).unwrap();
        apply(&mut ws, &op).unwrap();
    }
    assert_eq!(ws.page("p1").unwrap().blocks.last().unwrap().plain(), "x");
    assert_eq!(access_json("read_page"), "read");
    assert_eq!(access_json("add_task"), "write");
    assert_eq!(access_json("nope"), "unknown");
}

#[test]
fn graph_tools_label_what_was_computed_and_refuse_a_guess() {
    let mut ws = Workspace::new(9).unwrap();
    let shared = "launch beta import resume attachment schedule rollout window";
    page(&mut ws, "a", "Alpha", &[shared, "See #roadmap"]);
    page(&mut ws, "b", "Beta", &[shared]);
    apply(
        &mut ws,
        &Op::CreatePage {
            id: "c".into(),
            parent: Some("a".into()),
            title: "Child".into(),
            database: None,
        },
    )
    .unwrap();

    let related = read(&ws, "related_pages", json!({ "page": "Alpha" }));
    let first = &related["data"][0];
    assert_eq!(first["title"], "Beta", "{related}");
    assert_eq!(first["origin"], "computed");
    assert!(first["score"].as_f64().unwrap() > 0.0);
    assert!(
        related["notes"][0]
            .as_str()
            .unwrap()
            .contains("shared words")
    );
    assert_eq!(
        code(&read(&ws, "related_pages", json!({ "page": "Nope" }))),
        "CN-TOL-0003"
    );

    let near = read(&ws, "graph_neighbors", json!({ "node": "Alpha" }));
    let list = near["data"]["connections"].as_array().unwrap();
    assert_eq!(near["data"]["node"]["label"], "Alpha", "{near}");
    assert!(
        list.iter().any(|c| c["relation"] == "child"
            && c["to"]["label"] == "Child"
            && c["origin"] == "made"),
        "{near}"
    );
    assert!(
        list.iter()
            .any(|c| c["relation"] == "similar" && c["origin"] == "computed")
    );
    assert!(list.iter().any(|c| c["relation"] == "tag"));
    let tag = read(&ws, "graph_neighbors", json!({ "node": "#roadmap" }));
    assert_eq!(
        tag["data"]["connections"][0]["to"]["label"], "Alpha",
        "{tag}"
    );
    // Part of a name is not a guess.
    assert_eq!(
        code(&read(&ws, "graph_neighbors", json!({ "node": "Alp" }))),
        "CN-TOL-0003"
    );
    page(&mut ws, "d", "Beta", &[]);
    assert_eq!(
        code(&read(&ws, "graph_neighbors", json!({ "node": "Beta" }))),
        "CN-TOL-0004"
    );
}
