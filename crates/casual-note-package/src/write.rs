//! Turns notes into package bytes. Deterministic: the same notes and `created` give the same
//! bytes, with nothing taken from the clock, the interface language or the input order.

use crate::zip;
use crate::{
    MAX_INK_LINE_BYTES, MAX_JSON_BYTES, MAX_MANIFEST_BYTES, PackageError, compact_json, fold_path,
    id_problem, pretty_json, refuse,
};
use casual_note_merge::Workspace;
use casual_note_model::{
    Block, BlockKind, Canvas, Database, Ink, Note, Page, Run, VIEW_CANVAS, VIEW_PAGE,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::UnicodeNormalization;

pub(crate) const MIMETYPE: &str = "application/vnd.casualnote.package+zip";
/// Marks a paragraph that holds a block type this version does not know, kept whole. See `read.rs`.
pub(crate) const RAW_BLOCK: &str = "x-cnotepack-raw:";
const GENERATOR: &str = "CasualNote 0.1";
const MAX_PATH_CHARS: usize = 1000;
const UNTITLED: &str = "Untitled";
const REMOVED: &str = "Removed page";
const README: &str = "CasualNote Package

This file holds notes. You do not need CasualNote to read them.

1. Rename this file so it ends in .zip, and open it with any ZIP tool.
2. Open the readable folder. Every page is a Markdown file there, which any text editor can open.

The pages and databases folders hold the same notes as JSON, for software. The ink folder holds
drawings, one stroke per line. The format is documented in docs/36-EXPORT-FORMAT.md in the
CasualNote repository.
";

/// A number with no trailing `.0` when it is whole, so the bytes read as people write them.
pub(crate) fn number(v: f64) -> Value {
    if !v.is_finite() {
        return json!(0);
    }
    if v.fract() == 0.0 && v.abs() < 9_007_199_254_740_992.0 {
        return json!(v as i64);
    }
    json!(v)
}

fn run_to_inline(run: &Run) -> Value {
    let marks: Vec<&str> = run.marks.iter().map(|m| m.as_str()).collect();
    match &run.link {
        // The format's page link has no text of its own, so its words travel in an x- field.
        Some(page) => {
            let mut object = Map::new();
            object.insert("page".into(), json!(page));
            object.insert("x-text".into(), json!(run.text));
            if !marks.is_empty() {
                object.insert("x-marks".into(), json!(marks));
            }
            Value::Object(object)
        }
        None if marks.is_empty() => json!({ "text": run.text }),
        None => json!({ "text": run.text, "marks": marks }),
    }
}

fn block_to_json(block: &Block) -> Value {
    if block.kind == BlockKind::Paragraph
        && let Some(raw) = block
            .target
            .as_deref()
            .and_then(|t| t.strip_prefix(RAW_BLOCK))
        && let Ok(Value::Object(mut original)) = serde_json::from_str::<Value>(raw)
    {
        // Text someone typed into a kept block wins over the text it arrived with.
        let content: Vec<Value> = block.runs.iter().map(run_to_inline).collect();
        if original.contains_key("content") || !content.is_empty() {
            original.insert("content".into(), Value::Array(content));
        }
        original.insert("id".into(), json!(block.id));
        return Value::Object(original);
    }
    let mut object = Map::new();
    object.insert("id".into(), json!(block.id));
    let kind = match block.kind {
        BlockKind::Paragraph => "paragraph",
        BlockKind::Heading1 | BlockKind::Heading2 | BlockKind::Heading3 => "heading",
        BlockKind::Bulleted => "bulleted",
        BlockKind::Numbered => "numbered",
        BlockKind::Todo => "todo",
        BlockKind::Quote => "quote",
        BlockKind::Callout => "callout",
        BlockKind::Code => "code",
        BlockKind::Divider => "divider",
        BlockKind::Database => "database",
        BlockKind::Embed => "embed",
    };
    // A database or embed that points nowhere has nothing the schema can hold, so it reads as text.
    let target = block.target.as_deref().filter(|t| !t.is_empty());
    let dead_end = matches!(block.kind, BlockKind::Database | BlockKind::Embed) && target.is_none();
    object.insert(
        "type".into(),
        json!(if dead_end { "paragraph" } else { kind }),
    );
    match block.kind {
        BlockKind::Heading1 => drop(object.insert("level".into(), json!(1))),
        BlockKind::Heading2 => drop(object.insert("level".into(), json!(2))),
        BlockKind::Heading3 => drop(object.insert("level".into(), json!(3))),
        BlockKind::Database if !dead_end => drop(object.insert("database".into(), json!(target))),
        BlockKind::Embed if !dead_end => drop(object.insert("page".into(), json!(target))),
        _ => {}
    }
    if block.kind == BlockKind::Todo || block.checked {
        object.insert("checked".into(), json!(block.checked));
    }
    let carries_text = !matches!(
        block.kind,
        BlockKind::Divider | BlockKind::Database | BlockKind::Embed
    ) || dead_end;
    if carries_text || !block.runs.is_empty() {
        object.insert(
            "content".into(),
            Value::Array(block.runs.iter().map(run_to_inline).collect()),
        );
    }
    Value::Object(object)
}

fn canvas_is_empty(canvas: &Canvas) -> bool {
    canvas.note == Note::default() && canvas.items.is_empty() && canvas.ink.is_empty()
}

fn canvas_to_json(page: &Page) -> Value {
    let canvas = &page.canvas;
    let items: Vec<Value> = canvas
        .items
        .iter()
        .map(|i| {
            let mut object = Map::new();
            object.insert("id".into(), json!(i.id));
            object.insert("kind".into(), json!(i.kind));
            object.insert("x".into(), number(i.x));
            object.insert("y".into(), number(i.y));
            object.insert(
                "width".into(),
                number(if i.width > 0.0 { i.width } else { 1.0 }),
            );
            object.insert(
                "height".into(),
                number(if i.height > 0.0 { i.height } else { 1.0 }),
            );
            object.insert("text".into(), json!(i.text));
            if !i.color.is_empty() {
                object.insert("color".into(), json!(i.color));
            }
            Value::Object(object)
        })
        .collect();
    let mut object = Map::new();
    object.insert(
        "note".into(),
        json!({ "x": number(canvas.note.x), "y": number(canvas.note.y), "width": number(if canvas.note.width > 0.0 { canvas.note.width } else { 1.0 }) }),
    );
    object.insert("items".into(), Value::Array(items));
    if !canvas.ink.is_empty() {
        object.insert(
            "inkFile".into(),
            json!(format!("ink/{}.jsonl", page.meta.id)),
        );
    }
    Value::Object(object)
}

fn page_to_json(page: &Page, parent: Option<&str>) -> Value {
    let mut object = Map::new();
    object.insert("id".into(), json!(page.meta.id));
    object.insert("title".into(), json!(page.meta.title));
    object.insert("parent".into(), json!(parent));
    object.insert(
        "kind".into(),
        json!(if page.meta.database.is_some() {
            "database"
        } else {
            "document"
        }),
    );
    if let Some(database) = &page.meta.database {
        object.insert("database".into(), json!(database));
    }
    object.insert(
        "view".into(),
        json!(if page.view == VIEW_CANVAS {
            VIEW_CANVAS
        } else {
            VIEW_PAGE
        }),
    );
    object.insert("favorite".into(), json!(page.meta.favorite));
    object.insert(
        "blocks".into(),
        Value::Array(page.blocks.iter().map(block_to_json).collect()),
    );
    if !canvas_is_empty(&page.canvas) {
        object.insert("canvas".into(), canvas_to_json(page));
    }
    Value::Object(object)
}

fn ink_line(stroke: &Ink, index: usize) -> Vec<u8> {
    let id = if stroke.id.is_empty() {
        format!("s{index}")
    } else {
        stroke.id.clone()
    };
    let mut object = Map::new();
    object.insert("id".into(), json!(id));
    if stroke.kind == "arrow" {
        let point = |p: Option<&[f64; 3]>| match p {
            Some(p) => json!([number(p[0]), number(p[1])]),
            None => json!([0, 0]),
        };
        object.insert("kind".into(), json!("arrow"));
        object.insert("from".into(), point(stroke.points.first()));
        object.insert("to".into(), point(stroke.points.last()));
        // The format's arrow has no style, so these travel in x- fields.
        if !stroke.color.is_empty() {
            object.insert("x-color".into(), json!(stroke.color));
        }
        if stroke.width > 0.0 {
            object.insert("x-width".into(), number(stroke.width));
        }
    } else {
        let kind = match stroke.kind.as_str() {
            "pen" => "pen",
            "highlighter" | "hi" => "highlighter",
            other => {
                object.insert("x-kind".into(), json!(other));
                "pen"
            }
        };
        object.insert("kind".into(), json!(kind));
        object.insert("color".into(), json!(stroke.color));
        object.insert(
            "width".into(),
            number(if stroke.width > 0.0 {
                stroke.width
            } else {
                1.0
            }),
        );
        let points: Vec<Value> = stroke
            .points
            .iter()
            .map(|p| json!([number(p[0]), number(p[1]), number(p[2].clamp(0.0, 1.0))]))
            .collect();
        object.insert("points".into(), Value::Array(points));
    }
    let mut line = compact_json(&Value::Object(object));
    line.push(b'\n');
    line
}

fn database_to_json(db: &Database) -> Value {
    let mut statuses: Vec<Value> = db
        .statuses
        .iter()
        .map(|s| {
            let group = if matches!(s.group.as_str(), "todo" | "active" | "review" | "done") {
                s.group.as_str()
            } else {
                "todo"
            };
            json!({ "id": s.id, "name": s.name, "group": group })
        })
        .collect();
    // The format needs at least one status, and a row must name one.
    let first = db.statuses.first().map_or("todo", |s| s.id.as_str());
    if statuses.is_empty() {
        statuses.push(json!({ "id": "todo", "name": "To do", "group": "todo" }));
    }
    let columns: Vec<Value> = db
        .columns
        .iter()
        .map(|c| {
            let mut object = Map::new();
            object.insert("id".into(), json!(c.id));
            object.insert("name".into(), json!(c.name));
            object.insert(
                "type".into(),
                json!(if c.kind == "date" { "date" } else { "text" }),
            );
            if c.kind != "date" && c.kind != "text" {
                object.insert("x-kind".into(), json!(c.kind));
            }
            if c.hidden {
                object.insert("hidden".into(), json!(true));
            }
            Value::Object(object)
        })
        .collect();
    let rows: Vec<Value> = db
        .rows
        .iter()
        .map(|r| {
            let values: Map<String, Value> = r
                .values
                .iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect();
            let status = if r.status.is_empty() {
                first
            } else {
                r.status.as_str()
            };
            json!({ "id": r.id, "name": r.name, "status": status, "values": values })
        })
        .collect();
    let mut object = Map::new();
    object.insert("id".into(), json!(db.id));
    object.insert("name".into(), json!(db.name));
    object.insert(
        "defaultView".into(),
        json!(if db.view == "board" { "board" } else { "table" }),
    );
    if db.view != "board" && db.view != "table" {
        object.insert("x-view".into(), json!(db.view));
    }
    object.insert("statuses".into(), Value::Array(statuses));
    object.insert("columns".into(), Value::Array(columns));
    object.insert("rows".into(), Value::Array(rows));
    Value::Object(object)
}

fn safe_name(title: &str) -> String {
    let clean: String = title
        .nfc()
        .map(|c| {
            if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cut: String = clean.trim_matches(['.', ' ']).chars().take(80).collect();
    let name = cut.trim_matches(['.', ' ']);
    if name.is_empty() {
        UNTITLED.to_string()
    } else {
        name.to_string()
    }
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\n', '\r'], " ")
}

fn database_table(db: &Database) -> String {
    let columns: Vec<_> = db.columns.iter().filter(|c| !c.hidden).collect();
    let mut head = vec!["Name".to_string(), "Status".to_string()];
    head.extend(columns.iter().map(|c| cell(&c.name)));
    let mut out = format!(
        "| {} |\n| {} |",
        head.join(" | "),
        vec!["---"; head.len()].join(" | ")
    );
    for row in &db.rows {
        let status = db
            .statuses
            .iter()
            .find(|s| s.id == row.status)
            .map_or("", |s| s.name.as_str());
        let mut line = vec![cell(&row.name), cell(status)];
        line.extend(
            columns
                .iter()
                .map(|c| cell(row.values.get(&c.id).map_or("", String::as_str))),
        );
        out.push_str(&format!("\n| {} |", line.join(" | ")));
    }
    out
}

fn inline_markdown(block: &Block) -> String {
    use casual_note_model::Mark;
    let mut out = String::new();
    for run in &block.runs {
        if block.kind == BlockKind::Code {
            out.push_str(&run.text);
        } else if run.link.is_some() {
            out.push_str(&format!("[[{}]]", run.text));
        } else if run.marks.contains(&Mark::Code) {
            out.push_str(&format!("`{}`", run.text));
        } else if run.marks.contains(&Mark::Bold) {
            out.push_str(&format!("**{}**", run.text));
        } else if run.marks.contains(&Mark::Italic) {
            out.push_str(&format!("*{}*", run.text));
        } else if run.marks.contains(&Mark::Highlight) {
            out.push_str(&format!("=={}==", run.text));
        } else {
            out.push_str(&run.text);
        }
    }
    out
}

/// A page as Markdown. English words only, so the interface language cannot change the bytes.
/// `titles` and `databases` hold only what the package holds, so a re-export agrees.
pub fn page_markdown(
    page: &Page,
    titles: &BTreeMap<&str, &str>,
    databases: &BTreeMap<&str, &Database>,
) -> String {
    let title_of = |id: &str| match titles.get(id) {
        Some(t) if !t.is_empty() => (*t).to_string(),
        Some(_) => UNTITLED.to_string(),
        None => REMOVED.to_string(),
    };
    let heading = if page.meta.title.is_empty() {
        UNTITLED
    } else {
        page.meta.title.as_str()
    };
    let mut out = vec![format!("# {heading}"), String::new()];
    if let Some(db) = page.meta.database.as_deref().and_then(|d| databases.get(d)) {
        out.push(database_table(db));
        out.push(String::new());
    }
    let mut n = 0u32;
    for block in &page.blocks {
        n = if block.kind == BlockKind::Numbered {
            n.saturating_add(1)
        } else {
            0
        };
        let x = inline_markdown(block);
        out.push(match block.kind {
            BlockKind::Paragraph => x,
            BlockKind::Heading1 => format!("# {x}"),
            BlockKind::Heading2 => format!("## {x}"),
            BlockKind::Heading3 => format!("### {x}"),
            BlockKind::Bulleted => format!("- {x}"),
            BlockKind::Numbered => format!("{n}. {x}"),
            BlockKind::Todo => format!("- [{}] {x}", if block.checked { "x" } else { " " }),
            BlockKind::Quote | BlockKind::Callout => format!("> {x}"),
            BlockKind::Code => format!("```\n{x}\n```"),
            BlockKind::Divider => "---".to_string(),
            BlockKind::Database => block
                .target
                .as_deref()
                .and_then(|d| databases.get(d))
                .map(|db| database_table(db))
                .unwrap_or_default(),
            BlockKind::Embed => format!("![[{}]]", title_of(block.target.as_deref().unwrap_or(""))),
        });
        out.push(String::new());
    }
    let notes: Vec<&str> = page
        .canvas
        .items
        .iter()
        .map(|i| i.text.as_str())
        .filter(|t| !t.is_empty())
        .collect();
    if !notes.is_empty() {
        out.push("---".to_string());
        out.push(String::new());
        for text in notes {
            out.push(format!("- {}", text.replace('\n', " ")));
        }
        out.push(String::new());
    }
    out.join("\n")
}

/// The notes a package is made from.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub pages: Vec<Page>,
    pub databases: Vec<Database>,
    pub scope_page: bool,
}

/// Reads the live state of a workspace, or of one page and the databases it shows.
pub fn snapshot(ws: &Workspace, page: Option<&str>) -> Result<Snapshot, PackageError> {
    let unreadable = |e: casual_note_model::Error| refuse("CN-PKG-0010", &e.to_string());
    let Some(id) = page else {
        let pages = ws
            .pages()
            .into_iter()
            .filter(|m| !m.trashed)
            .map(|m| ws.page(&m.id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(unreadable)?;
        let databases = ws
            .database_ids()
            .iter()
            .map(|d| ws.database(d))
            .collect::<Result<Vec<_>, _>>()
            .map_err(unreadable)?;
        return Ok(Snapshot {
            pages,
            databases,
            scope_page: false,
        });
    };
    let one = ws.page(id).map_err(unreadable)?;
    let mut wanted: BTreeSet<String> = one.meta.database.iter().cloned().collect();
    wanted.extend(
        one.blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Database)
            .filter_map(|b| b.target.clone()),
    );
    let databases = wanted.iter().filter_map(|d| ws.database(d).ok()).collect();
    Ok(Snapshot {
        pages: vec![one],
        databases,
        scope_page: true,
    })
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn valid_created(created: &str) -> bool {
    let bytes = created.as_bytes();
    let digits = |r: std::ops::Range<usize>| {
        bytes
            .get(r)
            .is_some_and(|s| s.iter().all(u8::is_ascii_digit))
    };
    digits(0..4)
        && bytes.get(4) == Some(&b'-')
        && digits(5..7)
        && bytes.get(7) == Some(&b'-')
        && digits(8..10)
        && bytes.get(10) == Some(&b'T')
        && bytes.len() > 12
        && bytes.last() == Some(&b'Z')
        && bytes.get(11..bytes.len() - 1).is_some_and(|s| {
            s.iter()
                .all(|b| b.is_ascii_digit() || matches!(b, b':' | b'.'))
        })
}

fn readable_paths(
    pages: &[&Page],
    in_package: &BTreeMap<&str, &Page>,
    scope_page: bool,
) -> BTreeMap<String, String> {
    let mut files = BTreeSet::new();
    let mut dirs = BTreeSet::new();
    let mut out = BTreeMap::new();
    for page in pages {
        let mut chain = vec![safe_name(&page.meta.title)];
        let mut seen = BTreeSet::from([page.meta.id.as_str()]);
        let mut parent = page.meta.parent.as_deref();
        while let (false, Some(id)) = (scope_page, parent) {
            let Some(up) = in_package.get(id) else { break };
            if !seen.insert(id) {
                break;
            }
            chain.push(safe_name(&up.meta.title));
            parent = up.meta.parent.as_deref();
        }
        chain.reverse();
        let mut base = format!("readable/{}", chain.join("/"));
        if base.chars().count() > MAX_PATH_CHARS {
            base = format!("readable/{}", safe_name(&page.meta.title));
        }
        let mut path: String = format!("{base}.md").nfc().collect();
        let folders = |p: &str| -> Vec<String> {
            let parts: Vec<&str> = p.split('/').collect();
            (1..parts.len())
                .map(|n| fold_path(&parts[..n].join("/")))
                .collect()
        };
        let clash = |p: &str, files: &BTreeSet<String>, dirs: &BTreeSet<String>| {
            let key = fold_path(p);
            files.contains(&key)
                || dirs.contains(&key)
                || folders(p).iter().any(|d| files.contains(d))
        };
        if clash(&path, &files, &dirs) {
            let flat = safe_name(&page.meta.title);
            path = format!("readable/{flat} ({}).md", page.meta.id)
                .nfc()
                .collect();
        }
        files.insert(fold_path(&path));
        dirs.extend(folders(&path));
        out.insert(page.meta.id.clone(), path);
    }
    out
}

/// Writes a package. `created` is supplied, because time is never read here.
pub fn write_package(snapshot: &Snapshot, created: &str) -> Result<Vec<u8>, PackageError> {
    if !valid_created(created) {
        return Err(refuse(
            "CN-PKG-0002",
            "created must look like 2026-11-07T12:00:00Z",
        ));
    }
    let mut pages: Vec<&Page> = snapshot.pages.iter().collect();
    pages.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
    let mut databases: Vec<&Database> = snapshot.databases.iter().collect();
    databases.sort_by(|a, b| a.id.cmp(&b.id));
    let by_id: BTreeMap<&str, &Page> = pages.iter().map(|p| (p.meta.id.as_str(), *p)).collect();
    let titles: BTreeMap<&str, &str> = pages
        .iter()
        .map(|p| (p.meta.id.as_str(), p.meta.title.as_str()))
        .collect();
    let db_by_id: BTreeMap<&str, &Database> =
        databases.iter().map(|d| (d.id.as_str(), *d)).collect();
    let readable = readable_paths(&pages, &by_id, snapshot.scope_page);

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut listed_pages = Vec::with_capacity(pages.len());
    let mut used_ids = BTreeSet::new();
    for page in &pages {
        let id = &page.meta.id;
        if let Some(problem) = id_problem(id) {
            return Err(refuse(
                "CN-PKG-0004",
                &format!("page id cannot be a file name: {problem}"),
            ));
        }
        if !used_ids.insert(fold_path(id)) {
            return Err(refuse("CN-PKG-0005", "two page ids differ only by case"));
        }
        // A parent outside the package would dangle, so a page export starts its own tree.
        let parent = page
            .meta
            .parent
            .as_deref()
            .filter(|p| by_id.contains_key(p));
        let path = format!("pages/{id}.json");
        let bytes = pretty_json(&page_to_json(page, parent));
        if bytes.len() as u64 > MAX_JSON_BYTES {
            return Err(refuse("CN-PKG-0007", "a page is over the 64 MiB limit"));
        }
        files.insert(path.clone(), bytes);
        if !page.canvas.ink.is_empty() {
            let mut ink = Vec::new();
            for (i, stroke) in page.canvas.ink.iter().enumerate() {
                let line = ink_line(stroke, i);
                if line.len() as u64 > MAX_INK_LINE_BYTES {
                    return Err(refuse(
                        "CN-PKG-0007",
                        "an ink line is over the 16 MiB limit",
                    ));
                }
                ink.extend_from_slice(&line);
            }
            files.insert(format!("ink/{id}.jsonl"), ink);
        }
        let mut entry = json!({ "id": id, "title": page.meta.title, "path": path });
        if let (Some(md_path), Value::Object(object)) = (readable.get(id), &mut entry) {
            object.insert("readable".into(), json!(md_path));
            files.insert(
                md_path.clone(),
                page_markdown(page, &titles, &db_by_id).into_bytes(),
            );
        }
        listed_pages.push(entry);
    }
    let mut listed_databases = Vec::with_capacity(databases.len());
    let mut used_db_ids = BTreeSet::new();
    for db in &databases {
        if let Some(problem) = id_problem(&db.id) {
            return Err(refuse(
                "CN-PKG-0004",
                &format!("database id cannot be a file name: {problem}"),
            ));
        }
        if !used_db_ids.insert(fold_path(&db.id)) {
            return Err(refuse(
                "CN-PKG-0005",
                "two database ids differ only by case",
            ));
        }
        let path = format!("databases/{}.json", db.id);
        let bytes = pretty_json(&database_to_json(db));
        if bytes.len() as u64 > MAX_JSON_BYTES {
            return Err(refuse("CN-PKG-0007", "a database is over the 64 MiB limit"));
        }
        files.insert(path.clone(), bytes);
        listed_databases.push(json!({ "id": db.id, "name": db.name, "path": path }));
    }
    files.insert("README.txt".into(), README.as_bytes().to_vec());

    let listed: Vec<Value> = files
        .iter()
        .map(|(path, bytes)| {
            json!({ "path": path, "sha256": hex(&Sha256::digest(bytes)), "bytes": bytes.len() })
        })
        .collect();
    let manifest = json!({
        "format": "casualnote.package",
        "version": "1.0",
        "created": created,
        "generator": GENERATOR,
        "scope": if snapshot.scope_page { "page" } else { "workspace" },
        "languages": [],
        "pages": listed_pages,
        "databases": listed_databases,
        "files": listed,
    });
    let manifest = pretty_json(&manifest);
    if manifest.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(refuse(
            "CN-PKG-0007",
            "the manifest is over the 512 MiB limit",
        ));
    }
    let mut entries = Vec::with_capacity(files.len() + 2);
    entries.push(("mimetype".to_string(), MIMETYPE.as_bytes().to_vec()));
    entries.push(("manifest.json".to_string(), manifest));
    entries.extend(files);
    if entries.len() > zip::MAX_ENTRIES {
        return Err(refuse("CN-PKG-0007", "more than 1,000,000 entries"));
    }
    zip::write(&entries)
}
