//! Reads a package, which is hostile input (docs/36). Checks run in the same order as
//! tools/cnotepack.py, so both refuse a bad file with the same code. Nothing here panics, and
//! nothing runs, renders or fetches anything a package mentions.

use crate::write::{MIMETYPE, RAW_BLOCK};
use crate::zip::{self, Entry};
use crate::{
    MAX_DEPTH, MAX_ENTRY_BYTES, MAX_INK_LINE_BYTES, MAX_JSON_BYTES, MAX_MANIFEST_BYTES,
    MAX_TOTAL_BYTES, PackageError, compact_json, fold_path, id_problem, refuse,
};
use casual_note_model::{
    Block, BlockKind, Canvas, CanvasItem, Column, Database, Ink, Mark, Note, Page, PageMeta, Row,
    Run, Status, VIEW_CANVAS, VIEW_PAGE,
};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::UnicodeNormalization;

const SUPPORTED_MAJOR: u64 = 1;
const S_IFMT: u32 = 0o170_000;
const S_IFLNK: u32 = 0o120_000;
/// How much page and database JSON one read keeps in memory. A browser tab that runs out of
/// memory takes the editor with it, so this is refused well before that point.
const MAX_HELD_JSON_BYTES: u64 = 512 << 20;

/// Something a reader kept or set aside without understanding it. Reported, never hidden.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    /// `[page id, block id, type]` for each block type this version does not know.
    pub unknown_blocks: Vec<[String; 3]>,
    /// Fields this version does not model, so an import cannot keep them.
    pub extra_fields: u64,
    /// Attachments and extensions, which the workspace has nowhere to hold yet.
    pub skipped_files: Vec<String>,
    /// Pages whose parent is missing or part of a cycle. They land at the top level.
    pub moved_to_top: Vec<String>,
}

/// A package, checked and parsed.
#[derive(Clone, Debug, PartialEq)]
pub struct Package {
    pub version: String,
    pub created: String,
    pub scope: String,
    pub pages: Vec<Page>,
    pub databases: Vec<Database>,
    pub files: Vec<(String, u64)>,
    pub report: Report,
}

fn check_path(name: &str) -> Result<(), PackageError> {
    let bytes = name.as_bytes();
    let drive = bytes.len() >= 2
        && bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(1) == Some(&b':');
    let bad = name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || drive
        || name.chars().any(|c| (c as u32) < 32 || c as u32 == 127)
        || name.split('/').any(|part| matches!(part, "" | "." | ".."))
        || name.nfc().collect::<String>() != name;
    if bad {
        return Err(refuse("CN-PKG-0004", "a path is unsafe"));
    }
    Ok(())
}

fn nesting_too_deep(data: &[u8]) -> bool {
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for byte in data {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return true;
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}

fn level_of(value: &Value, level: usize) -> usize {
    if level > MAX_DEPTH {
        return level;
    }
    match value {
        Value::Object(map) => map
            .values()
            .map(|v| level_of(v, level + 1))
            .max()
            .unwrap_or(level),
        Value::Array(list) => list
            .iter()
            .map(|v| level_of(v, level + 1))
            .max()
            .unwrap_or(level),
        _ => level,
    }
}

fn parse_json(data: &[u8], code: &'static str, what: &str) -> Result<Value, PackageError> {
    let deep = || refuse("CN-PKG-0007", &format!("{what} is nested too deeply"));
    if nesting_too_deep(data) {
        return Err(deep());
    }
    let value: Value = serde_json::from_slice(data)
        .map_err(|_| refuse(code, &format!("{what} is not valid JSON")))?;
    if level_of(&value, 1) > MAX_DEPTH {
        return Err(deep());
    }
    Ok(value)
}

/// A path safe to quote: a readable path holds a title, and messages never carry note text.
fn quote(path: &str) -> &str {
    if path.starts_with("pages/")
        || path.starts_with("databases/")
        || path.starts_with("ink/")
        || !path.contains('/')
    {
        path
    } else {
        "a file"
    }
}

fn schema(path: &str, why: &str) -> PackageError {
    refuse("CN-PKG-0010", &format!("{}: {why}", quote(path)))
}

fn string_field(
    object: &Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<String, PackageError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| schema(path, &format!("{key} must be text")))
}

fn id_field(object: &Map<String, Value>, key: &str, path: &str) -> Result<String, PackageError> {
    let id = string_field(object, key, path)?;
    if let Some(problem) = id_problem(&id) {
        return Err(schema(path, &format!("{key} {problem}")));
    }
    Ok(id)
}

fn number_field(object: &Map<String, Value>, key: &str, path: &str) -> Result<f64, PackageError> {
    object
        .get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| schema(path, &format!("{key} must be a number")))
}

fn count_extras(object: &Map<String, Value>, known: &[&str], report: &mut Report) {
    let extra = object
        .keys()
        .filter(|k| !known.contains(&k.as_str()))
        .count();
    report.extra_fields = report.extra_fields.saturating_add(extra as u64);
}

fn marks_from(
    value: Option<&Value>,
    path: &str,
    report: &mut Report,
) -> Result<Vec<Mark>, PackageError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let list = value
        .as_array()
        .ok_or_else(|| schema(path, "marks must be a list"))?;
    let mut out = Vec::new();
    for mark in list {
        let name = mark
            .as_str()
            .ok_or_else(|| schema(path, "a mark must be text"))?;
        match Mark::ALL.into_iter().find(|m| m.as_str() == name) {
            Some(m) if !out.contains(&m) => out.push(m),
            Some(_) => {}
            None => report.extra_fields = report.extra_fields.saturating_add(1),
        }
    }
    Ok(out)
}

fn inline_to_run(value: &Value, path: &str, report: &mut Report) -> Result<Run, PackageError> {
    let object = value
        .as_object()
        .ok_or_else(|| schema(path, "inline content must be objects"))?;
    if let Some(page) = object.get("page") {
        let page = page
            .as_str()
            .ok_or_else(|| schema(path, "a link must name a page"))?
            .to_string();
        count_extras(object, &["page", "x-text", "x-marks"], report);
        return Ok(Run {
            text: object
                .get("x-text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            marks: marks_from(object.get("x-marks"), path, report)?,
            link: Some(page),
        });
    }
    let text = object
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| schema(path, "inline content needs text or a page"))?;
    count_extras(object, &["text", "marks"], report);
    Ok(Run {
        text: text.to_string(),
        marks: marks_from(object.get("marks"), path, report)?,
        link: None,
    })
}

fn content_to_runs(
    object: &Map<String, Value>,
    path: &str,
    report: &mut Report,
) -> Result<Vec<Run>, PackageError> {
    let Some(content) = object.get("content") else {
        return Ok(Vec::new());
    };
    let list = content
        .as_array()
        .ok_or_else(|| schema(path, "content must be a list"))?;
    list.iter()
        .map(|v| inline_to_run(v, path, report))
        .collect()
}

fn block_from(
    value: &Value,
    page_id: &str,
    path: &str,
    report: &mut Report,
) -> Result<Block, PackageError> {
    let object = value
        .as_object()
        .ok_or_else(|| schema(path, "a block is malformed"))?;
    let id = id_field(object, "id", path)?;
    let kind_name = object
        .get("type")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| schema(path, "a block is malformed"))?;
    let needs_content = matches!(
        kind_name,
        "paragraph" | "heading" | "bulleted" | "numbered" | "todo" | "quote" | "callout" | "code"
    );
    if needs_content && !object.get("content").is_some_and(Value::is_array) {
        return Err(schema(path, &format!("block {id} has no content")));
    }
    let kind = match kind_name {
        "paragraph" => BlockKind::Paragraph,
        "heading" => match object.get("level").and_then(Value::as_u64) {
            Some(1) => BlockKind::Heading1,
            Some(2) => BlockKind::Heading2,
            Some(3) => BlockKind::Heading3,
            _ => {
                return Err(schema(
                    path,
                    &format!("block {id} needs a level from 1 to 3"),
                ));
            }
        },
        "bulleted" => BlockKind::Bulleted,
        "numbered" => BlockKind::Numbered,
        "todo" => BlockKind::Todo,
        "quote" => BlockKind::Quote,
        "callout" => BlockKind::Callout,
        "code" => BlockKind::Code,
        "divider" => BlockKind::Divider,
        "database" => BlockKind::Database,
        "embed" => BlockKind::Embed,
        other => {
            // Kept whole, inside a paragraph so that any text it has stays visible. The model has
            // no block that holds a foreign type, and this keeps it through a re-export.
            report
                .unknown_blocks
                .push([page_id.to_string(), id.clone(), other.to_string()]);
            let runs = content_to_runs(object, path, &mut Report::default()).unwrap_or_default();
            let raw = String::from_utf8(compact_json(value)).unwrap_or_default();
            return Ok(Block {
                id,
                kind: BlockKind::Paragraph,
                checked: false,
                runs,
                target: Some(format!("{RAW_BLOCK}{raw}")),
            });
        }
    };
    let target = match kind {
        BlockKind::Database => Some(id_field(object, "database", path)?),
        BlockKind::Embed => Some(id_field(object, "page", path)?),
        _ => None,
    };
    count_extras(
        object,
        &[
            "id", "type", "level", "checked", "content", "database", "page",
        ],
        report,
    );
    Ok(Block {
        id,
        kind,
        checked: object
            .get("checked")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        runs: content_to_runs(object, path, report)?,
        target,
    })
}

fn canvas_item(value: &Value, path: &str, report: &mut Report) -> Result<CanvasItem, PackageError> {
    let object = value
        .as_object()
        .ok_or_else(|| schema(path, "a canvas item is malformed"))?;
    count_extras(
        object,
        &["id", "kind", "x", "y", "width", "height", "color", "text"],
        report,
    );
    Ok(CanvasItem {
        id: id_field(object, "id", path)?,
        kind: string_field(object, "kind", path)?,
        x: number_field(object, "x", path)?,
        y: number_field(object, "y", path)?,
        width: number_field(object, "width", path)?,
        height: number_field(object, "height", path)?,
        color: object
            .get("color")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        text: object
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

fn page_from(
    value: &Value,
    expected: &str,
    path: &str,
    report: &mut Report,
    ink_named: &mut BTreeMap<String, String>,
    known_files: &BTreeSet<&str>,
) -> Result<Page, PackageError> {
    let object = value
        .as_object()
        .ok_or_else(|| schema(path, "not an object"))?;
    if object.get("id").and_then(Value::as_str) != Some(expected) {
        return Err(schema(path, "id differs from the manifest"));
    }
    if let Some(problem) = id_problem(expected) {
        return Err(schema(path, &format!("id {problem}")));
    }
    let title = string_field(object, "title", path)?;
    let kind = object.get("kind").and_then(Value::as_str);
    if !matches!(kind, Some("document" | "database")) {
        return Err(schema(path, "unknown kind"));
    }
    let list = object
        .get("blocks")
        .and_then(Value::as_array)
        .ok_or_else(|| schema(path, "blocks must be a list"))?;
    let mut blocks = Vec::with_capacity(list.len());
    let mut seen = BTreeSet::new();
    for block in list {
        let block = block_from(block, expected, path, report)?;
        if !seen.insert(block.id.clone()) {
            return Err(schema(path, &format!("block {} appears twice", block.id)));
        }
        blocks.push(block);
    }
    let parent = match object.get("parent") {
        None | Some(Value::Null) => None,
        Some(_) => Some(id_field(object, "parent", path)?),
    };
    let database = if kind == Some("database") {
        Some(id_field(object, "database", path)?)
    } else {
        None
    };
    let mut canvas = Canvas::default();
    if let Some(value) = object.get("canvas") {
        let c = value
            .as_object()
            .ok_or_else(|| schema(path, "canvas must be an object"))?;
        if c.contains_key("ink") {
            return Err(schema(
                path,
                "ink belongs in ink/<page-id>.jsonl, named by inkFile",
            ));
        }
        if let Some(note) = c.get("note").and_then(Value::as_object) {
            canvas.note = Note {
                x: number_field(note, "x", path)?,
                y: number_field(note, "y", path)?,
                width: number_field(note, "width", path)?,
            };
        }
        if let Some(items) = c.get("items") {
            let items = items
                .as_array()
                .ok_or_else(|| schema(path, "canvas items must be a list"))?;
            canvas.items = items
                .iter()
                .map(|i| canvas_item(i, path, report))
                .collect::<Result<_, _>>()?;
        }
        if let Some(file) = c.get("inkFile").filter(|v| !v.is_null()) {
            let own = format!("ink/{expected}.jsonl");
            if file.as_str() != Some(own.as_str()) {
                return Err(schema(
                    path,
                    "inkFile must name this page's own file under ink/",
                ));
            }
            if !known_files.contains(own.as_str()) {
                return Err(schema(
                    path,
                    "inkFile names a file that is not in the package",
                ));
            }
            ink_named.insert(own, expected.to_string());
        }
        count_extras(c, &["note", "items", "inkFile"], report);
    }
    count_extras(
        object,
        &[
            "id", "title", "parent", "kind", "database", "view", "favorite", "blocks", "canvas",
        ],
        report,
    );
    Ok(Page {
        meta: PageMeta {
            id: expected.to_string(),
            parent,
            title,
            favorite: object
                .get("favorite")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            trashed: false,
            database,
        },
        blocks,
        view: if object.get("view").and_then(Value::as_str) == Some(VIEW_CANVAS) {
            VIEW_CANVAS.into()
        } else {
            VIEW_PAGE.into()
        },
        canvas,
    })
}

fn database_from(
    value: &Value,
    expected: &str,
    path: &str,
    report: &mut Report,
) -> Result<Database, PackageError> {
    let object = value
        .as_object()
        .ok_or_else(|| schema(path, "not an object"))?;
    if object.get("id").and_then(Value::as_str) != Some(expected) {
        return Err(schema(path, "id differs from the manifest"));
    }
    if let Some(problem) = id_problem(expected) {
        return Err(schema(path, &format!("id {problem}")));
    }
    let list = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_array)
            .ok_or_else(|| schema(path, &format!("{key} must be a list")))
    };
    let (statuses, columns, rows) = (list("statuses")?, list("columns")?, list("rows")?);
    let record = |v: &'_ Value| -> Result<Map<String, Value>, PackageError> {
        v.as_object()
            .cloned()
            .ok_or_else(|| schema(path, "a record is malformed"))
    };
    let mut out_statuses = Vec::with_capacity(statuses.len());
    for s in statuses {
        let s = record(s)?;
        out_statuses.push(Status {
            id: id_field(&s, "id", path)?,
            name: s
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            group: s
                .get("group")
                .and_then(Value::as_str)
                .unwrap_or("todo")
                .to_string(),
        });
    }
    let mut out_columns = Vec::with_capacity(columns.len());
    for c in columns {
        let c = record(c)?;
        let kind = c
            .get("x-kind")
            .or_else(|| c.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("text")
            .to_string();
        out_columns.push(Column {
            id: id_field(&c, "id", path)?,
            name: c
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            kind,
            hidden: c.get("hidden").and_then(Value::as_bool).unwrap_or(false),
        });
    }
    let mut out_rows = Vec::with_capacity(rows.len());
    for r in rows {
        let r = record(r)?;
        let mut values = BTreeMap::new();
        if let Some(cells) = r.get("values") {
            let cells = cells
                .as_object()
                .ok_or_else(|| schema(path, "row values must be an object"))?;
            for (key, cell) in cells {
                let text = cell
                    .as_str()
                    .ok_or_else(|| schema(path, "a row value must be text"))?;
                values.insert(key.clone(), text.to_string());
            }
        }
        out_rows.push(Row {
            id: id_field(&r, "id", path)?,
            name: r
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            status: r
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            values,
        });
    }
    count_extras(
        object,
        &[
            "id",
            "name",
            "defaultView",
            "x-view",
            "statuses",
            "columns",
            "rows",
        ],
        report,
    );
    let view = object
        .get("x-view")
        .or_else(|| object.get("defaultView"))
        .and_then(Value::as_str)
        .unwrap_or("table")
        .to_string();
    Ok(Database {
        id: expected.to_string(),
        name: object
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        view,
        statuses: out_statuses,
        columns: out_columns,
        rows: out_rows,
    })
}

fn finite_points(value: Option<&Value>, size: usize) -> Option<Vec<f64>> {
    let list = value?.as_array()?;
    if list.len() != size {
        return None;
    }
    list.iter()
        .map(|v| v.as_f64().filter(|n| n.is_finite()))
        .collect()
}

fn ink_from_line(line: &[u8], where_: &str) -> Result<Ink, PackageError> {
    let value = parse_json(line, "CN-PKG-0010", where_)?;
    let bad = |why: &str| refuse("CN-PKG-0010", &format!("{where_}: {why}"));
    let object = value.as_object().ok_or_else(|| bad("needs an id"))?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("needs an id"))?;
    let kind = object.get("kind").and_then(Value::as_str);
    let allowed: &[&str] = match kind {
        Some("pen" | "highlighter") => &["id", "kind", "color", "width", "points"],
        Some("arrow") => &["id", "kind", "from", "to"],
        _ => return Err(bad("unknown kind")),
    };
    if object
        .keys()
        .any(|k| !allowed.contains(&k.as_str()) && !k.starts_with("x-"))
    {
        return Err(bad("unexpected field"));
    }
    if kind == Some("arrow") {
        let from = finite_points(object.get("from"), 2)
            .ok_or_else(|| bad("from and to must be [x, y]"))?;
        let to =
            finite_points(object.get("to"), 2).ok_or_else(|| bad("from and to must be [x, y]"))?;
        let point = |p: &[f64]| {
            [
                p.first().copied().unwrap_or(0.0),
                p.get(1).copied().unwrap_or(0.0),
                0.5,
            ]
        };
        let color = object
            .get("x-color")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let width = object
            .get("x-width")
            .and_then(Value::as_f64)
            .filter(|w| w.is_finite() && *w > 0.0)
            .unwrap_or(0.0);
        return Ok(Ink {
            id: id.to_string(),
            kind: "arrow".into(),
            color,
            width,
            points: vec![point(&from), point(&to)],
        });
    }
    let color = object
        .get("color")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("needs a color"))?;
    let width = object
        .get("width")
        .and_then(Value::as_f64)
        .filter(|w| w.is_finite() && *w > 0.0)
        .ok_or_else(|| bad("needs a width above 0"))?;
    let list = object
        .get("points")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("points must be [x, y, pressure]"))?;
    let mut points = Vec::with_capacity(list.len());
    for p in list {
        let p = finite_points(Some(p), 3).ok_or_else(|| bad("points must be [x, y, pressure]"))?;
        let [x, y, pressure] = p.as_slice() else {
            return Err(bad("points must be [x, y, pressure]"));
        };
        if !(0.0..=1.0).contains(pressure) {
            return Err(bad("pressure must be from 0 to 1"));
        }
        points.push([*x, *y, *pressure]);
    }
    let kind = object
        .get("x-kind")
        .and_then(Value::as_str)
        .or(kind)
        .unwrap_or("pen")
        .to_string();
    Ok(Ink {
        id: id.to_string(),
        kind,
        color: color.to_string(),
        width,
        points,
    })
}

fn read_ink(data: &[u8], entry: &Entry) -> Result<Vec<Ink>, PackageError> {
    let mut out = Vec::new();
    let mut pending: Vec<u8> = Vec::new();
    let mut number = 0usize;
    let too_long = || {
        refuse(
            "CN-PKG-0007",
            &format!("{}: a line is over the 16 MiB limit", quote(&entry.name)),
        )
    };
    zip::stream(data, entry, MAX_ENTRY_BYTES, &mut |piece| {
        let mut rest = piece;
        while let Some(end) = rest.iter().position(|b| *b == b'\n') {
            let (line, after) = rest.split_at(end);
            if (pending.len() + line.len()) as u64 > MAX_INK_LINE_BYTES {
                return Err(too_long());
            }
            pending.extend_from_slice(line);
            number += 1;
            out.push(ink_from_line(
                &pending,
                &format!("{} line {number}", quote(&entry.name)),
            )?);
            pending.clear();
            rest = after.get(1..).unwrap_or(&[]);
        }
        if (pending.len() + rest.len()) as u64 > MAX_INK_LINE_BYTES {
            return Err(too_long());
        }
        pending.extend_from_slice(rest);
        Ok(())
    })?;
    if !pending.is_empty() {
        return Err(refuse(
            "CN-PKG-0010",
            &format!("{}: the last line has no line ending", quote(&entry.name)),
        ));
    }
    Ok(out)
}

struct Opened {
    by_name: BTreeMap<String, Entry>,
    manifest: Map<String, Value>,
    version: String,
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Everything through the structure of the ZIP and the manifest's own fields.
fn open(data: &[u8]) -> Result<Opened, PackageError> {
    let entries = zip::central_directory(data)?;
    let first_is_right = match entries.first() {
        None => return Err(refuse("CN-PKG-0001", "the ZIP is empty")),
        Some(first) => {
            first.name == "mimetype"
                && first.method == 0
                && first.size == MIMETYPE.len() as u64
                && first.offset == 0
                && zip::read_all(data, first, MIMETYPE.len() as u64)
                    .is_ok_and(|b| b == MIMETYPE.as_bytes())
        }
    };
    if !first_is_right {
        return Err(refuse(
            "CN-PKG-0001",
            "mimetype must be the first entry, stored, with the exact value",
        ));
    }
    if entries.len() > zip::MAX_ENTRIES {
        return Err(refuse("CN-PKG-0007", "more than 1,000,000 entries"));
    }
    let mut seen = BTreeSet::new();
    let mut by_name = BTreeMap::new();
    for entry in entries {
        if !entry.name_is_utf8 {
            return Err(refuse("CN-PKG-0004", "a path is not UTF-8"));
        }
        check_path(entry.name.strip_suffix('/').unwrap_or(&entry.name))?;
        if !seen.insert(fold_path(&entry.name)) {
            return Err(refuse(
                "CN-PKG-0005",
                "two entries have the same path, or differ only by case or Unicode form",
            ));
        }
        if entry.flags & 0x41 != 0 {
            return Err(refuse("CN-PKG-0006", "an entry is encrypted"));
        }
        if (entry.attributes >> 16) & S_IFMT == S_IFLNK {
            return Err(refuse("CN-PKG-0006", "an entry is a symbolic link"));
        }
        if entry.size > MAX_ENTRY_BYTES {
            return Err(refuse("CN-PKG-0007", "an entry is over the 2 GiB limit"));
        }
        if !entry.is_dir() {
            by_name.insert(entry.name.clone(), entry);
        }
    }
    let manifest_entry = by_name
        .get("manifest.json")
        .ok_or_else(|| refuse("CN-PKG-0002", "manifest.json is missing"))?;
    let bytes = zip::read_all(data, manifest_entry, MAX_MANIFEST_BYTES)?;
    let manifest = parse_json(&bytes, "CN-PKG-0002", "manifest.json")?;
    drop(bytes);
    let Value::Object(manifest) = manifest else {
        return Err(refuse(
            "CN-PKG-0002",
            "this is not a CasualNote Package manifest",
        ));
    };
    if manifest.get("format").and_then(Value::as_str) != Some("casualnote.package") {
        return Err(refuse(
            "CN-PKG-0002",
            "this is not a CasualNote Package manifest",
        ));
    }
    let version = manifest
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let Some((major, minor)) = version.split_once('.') else {
        return Err(refuse("CN-PKG-0002", "version must look like 1.0"));
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(major) || !digits(minor) {
        return Err(refuse("CN-PKG-0002", "version must look like 1.0"));
    }
    // A number too long to parse is certainly newer than anything here.
    if major.parse::<u64>().map_or(true, |m| m > SUPPORTED_MAJOR) {
        return Err(refuse(
            "CN-PKG-0003",
            &format!("package version {version} is newer than this reader supports"),
        ));
    }
    for key in ["pages", "databases", "files"] {
        if !manifest.get(key).is_some_and(Value::is_array) {
            return Err(refuse(
                "CN-PKG-0002",
                &format!("manifest {key} must be a list"),
            ));
        }
    }
    Ok(Opened {
        by_name,
        manifest,
        version,
    })
}

fn check_files(data: &[u8], opened: &Opened) -> Result<Vec<(String, u64)>, PackageError> {
    let mut listed = BTreeSet::new();
    let mut out = Vec::new();
    let mut budget = MAX_TOTAL_BYTES;
    let empty = Vec::new();
    for item in opened
        .manifest
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
    {
        let object = item
            .as_object()
            .ok_or_else(|| refuse("CN-PKG-0002", "a manifest file entry is malformed"))?;
        let name = object
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| refuse("CN-PKG-0002", "a manifest file entry is malformed"))?;
        check_path(name)?;
        listed.insert(name);
        let entry = opened.by_name.get(name).ok_or_else(|| {
            refuse(
                "CN-PKG-0008",
                &format!("{} is listed but missing", quote(name)),
            )
        })?;
        let mut digest = Sha256::new();
        let limit = MAX_ENTRY_BYTES.min(budget);
        let size = zip::stream(data, entry, limit, &mut |piece| {
            digest.update(piece);
            Ok(())
        })?;
        budget = budget.saturating_sub(size);
        let want_hash = object.get("sha256").and_then(Value::as_str);
        let want_size = object.get("bytes").and_then(Value::as_u64);
        if want_hash != Some(hex(&digest.finalize()).as_str()) || want_size != Some(size) {
            return Err(refuse(
                "CN-PKG-0008",
                &format!("{} does not match its hash or size", quote(name)),
            ));
        }
        out.push((name.to_string(), size));
    }
    for name in opened.by_name.keys() {
        if !listed.contains(name.as_str()) && name != "mimetype" && name != "manifest.json" {
            return Err(refuse(
                "CN-PKG-0009",
                &format!("not in the manifest: {}", quote(name)),
            ));
        }
    }
    Ok(out)
}

/// What a check alone tells, without parsing every page. Used to show a package before importing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Inspection {
    pub version: String,
    pub created: String,
    pub scope: String,
    pub pages: Vec<(String, String)>,
    pub databases: Vec<(String, String)>,
    pub files: Vec<(String, u64)>,
    pub report: Report,
}

/// Reads and checks a whole package.
pub fn read_package(data: &[u8]) -> Result<Package, PackageError> {
    let opened = open(data)?;
    let files = check_files(data, &opened)?;
    let known: BTreeSet<&str> = opened.by_name.keys().map(String::as_str).collect();
    let mut report = Report::default();
    let mut ink_named = BTreeMap::new();
    let mut pages: Vec<Page> = Vec::new();
    let mut databases = Vec::new();
    let mut held = 0u64;
    let mut seen_ids = BTreeSet::new();
    let empty = Vec::new();
    let mut load = |item: &Value, kind: &str| -> Result<(String, String, Value), PackageError> {
        let malformed = || {
            refuse(
                "CN-PKG-0002",
                &format!("a manifest {kind} entry is malformed"),
            )
        };
        let object = item.as_object().ok_or_else(malformed)?;
        let path = object
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(malformed)?;
        let entry = opened.by_name.get(path).ok_or_else(malformed)?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(malformed)?;
        let bytes = zip::read_all(data, entry, MAX_JSON_BYTES)?;
        held = held.saturating_add(bytes.len() as u64);
        if held > MAX_HELD_JSON_BYTES {
            return Err(refuse(
                "CN-PKG-0007",
                "these pages are too large to read at once",
            ));
        }
        Ok((
            id.to_string(),
            path.to_string(),
            parse_json(&bytes, "CN-PKG-0010", quote(path))?,
        ))
    };
    for item in opened
        .manifest
        .get("pages")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
    {
        let (id, path, value) = load(item, "pages")?;
        if !seen_ids.insert(id.clone()) {
            return Err(schema(&path, "a page is listed twice"));
        }
        pages.push(page_from(
            &value,
            &id,
            &path,
            &mut report,
            &mut ink_named,
            &known,
        )?);
    }
    seen_ids.clear();
    for item in opened
        .manifest
        .get("databases")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
    {
        let (id, path, value) = load(item, "databases")?;
        if !seen_ids.insert(id.clone()) {
            return Err(schema(&path, "a database is listed twice"));
        }
        databases.push(database_from(&value, &id, &path, &mut report)?);
    }
    let mut ink: BTreeMap<String, Vec<Ink>> = BTreeMap::new();
    for (name, entry) in &opened.by_name {
        if name.starts_with("ink/") {
            let Some(page) = ink_named.get(name) else {
                return Err(schema(name, "no page names this ink file"));
            };
            ink.insert(page.clone(), read_ink(data, entry)?);
        }
        if name.starts_with("assets/") || name.starts_with("extensions/") {
            report.skipped_files.push(name.clone());
        }
    }
    // A plain page link has no words of its own. It shows the page's title, or says it is gone.
    let titles: BTreeMap<String, String> = pages
        .iter()
        .map(|p| (p.meta.id.clone(), p.meta.title.clone()))
        .collect();
    for page in &mut pages {
        if let Some(strokes) = ink.remove(&page.meta.id) {
            page.canvas.ink = strokes;
        }
        for block in &mut page.blocks {
            for run in &mut block.runs {
                if let (true, Some(target)) = (run.text.is_empty(), &run.link) {
                    run.text = match titles.get(target) {
                        Some(t) if !t.is_empty() => t.clone(),
                        Some(_) => "Untitled".to_string(),
                        None => "Removed page".to_string(),
                    };
                }
            }
        }
    }
    let field = |key: &str, default: &str| {
        opened
            .manifest
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_string()
    };
    Ok(Package {
        version: opened.version.clone(),
        created: field("created", ""),
        scope: field("scope", "workspace"),
        pages,
        databases,
        files,
        report,
    })
}

/// Checks a package and describes it, changing nothing.
pub fn inspect(data: &[u8]) -> Result<Inspection, PackageError> {
    let package = read_package(data)?;
    Ok(Inspection {
        version: package.version,
        created: package.created,
        scope: package.scope,
        pages: package
            .pages
            .iter()
            .map(|p| (p.meta.id.clone(), p.meta.title.clone()))
            .collect(),
        databases: package
            .databases
            .iter()
            .map(|d| (d.id.clone(), d.name.clone()))
            .collect(),
        files: package.files,
        report: package.report,
    })
}

/// One file's bytes, for showing a package's Markdown.
pub fn entry_bytes(data: &[u8], path: &str) -> Result<Vec<u8>, PackageError> {
    let opened = open(data)?;
    let entry = opened
        .by_name
        .get(path)
        .ok_or_else(|| refuse("CN-PKG-0008", "that file is not in the package"))?;
    zip::read_all(data, entry, MAX_JSON_BYTES)
}
