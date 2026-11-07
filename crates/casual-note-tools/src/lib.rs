//! The closed tool set over a workspace, with one meaning on every surface (docs/63).
//!
//! Tools are defined once, in [`TOOLS`]. The JSON Schema a model sees and the check its arguments
//! pass through are both generated from the same table, so they cannot disagree. Descriptions are
//! fixed strings, never built from notes.
//!
//! Reads run at once. A write has two phases: [`prepare_write`] validates and builds a preview and
//! an op batch while changing nothing, and the surface then decides. The batch is one undo step.

use casual_note_graph::{Edge, EdgeKind, Graph, Node, Origin, from_workspace};
use casual_note_merge::Workspace;
use casual_note_model::{Block, BlockKind, Column, Database, Page, PageMeta, Row, Run};
use casual_note_ops::{Op, apply};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

/// Hard ceiling on one result, whatever the tool.
pub const MAX_RESULT_BYTES: usize = 64 * 1024;
/// `read_page` text per call.
pub const MAX_READ_BYTES: usize = 32 * 1024;
/// `list_pages` and `list_tasks` per call.
pub const MAX_LIST: usize = 200;
/// `related_pages` per call.
pub const MAX_RELATED: usize = 20;
/// `graph_neighbors` connections per call.
pub const MAX_CONNECTIONS: usize = 100;
/// Search hits per call.
pub const MAX_HITS: usize = 20;
/// Snippet length, in graphemes.
pub const SNIPPET_GRAPHEMES: usize = 240;
/// `title`, `page`, `task`, `database`, `owner` and similar.
pub const MAX_NAME_BYTES: usize = 1024;
/// `query`.
pub const MAX_QUERY_BYTES: usize = 4096;
/// `text`.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
/// Blocks one write may add.
pub const MAX_BLOCKS: usize = 1000;
/// Blocks shown inline on an approval card.
pub const PREVIEW_BLOCKS: usize = 20;
/// Text shown per block inline on an approval card.
pub const PREVIEW_BYTES: usize = 1024;
/// The raw argument string a surface may hand over.
pub const MAX_ARGS_BYTES: usize = 1024 * 1024;

// Room for the envelope around items counted against the ceiling.
const ENVELOPE_BYTES: usize = 2048;

/// Whether a tool changes anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

/// One string argument.
#[derive(Debug)]
pub struct Field {
    pub name: &'static str,
    pub description: &'static str,
    pub max_bytes: usize,
    pub required: bool,
}

/// One tool.
#[derive(Debug)]
pub struct ToolDef {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub access: Access,
    pub fields: &'static [Field],
}

const fn field(
    name: &'static str,
    description: &'static str,
    max_bytes: usize,
    required: bool,
) -> Field {
    Field {
        name,
        description,
        max_bytes,
        required,
    }
}

const CURSOR: Field = field(
    "cursor",
    "next_cursor from an earlier call, to continue",
    64,
    false,
);

/// The closed set. A new tool here is a change to a public surface.
pub static TOOLS: [ToolDef; 9] = [
    ToolDef {
        name: "list_pages",
        title: "List pages",
        description: "List the pages in this workspace, with id, title, kind and parent. Pages in the trash are not listed.",
        access: Access::Read,
        fields: &[CURSOR],
    },
    ToolDef {
        name: "search_notes",
        title: "Search notes",
        description: "Find text in page titles, blocks, database rows and canvas items. Returns pages with a snippet. Matching ignores case.",
        access: Access::Read,
        fields: &[
            field("query", "The text to find", MAX_QUERY_BYTES, true),
            CURSOR,
        ],
    },
    ToolDef {
        name: "read_page",
        title: "Read a page",
        description: "Read one page as Markdown, with its database rows and canvas text. A long page continues with next_cursor.",
        access: Access::Read,
        fields: &[
            field("page", "Page id, or the exact title", MAX_NAME_BYTES, true),
            CURSOR,
        ],
    },
    ToolDef {
        name: "related_pages",
        title: "Related pages",
        description: "Pages that share words with this page. Computed by comparing words, not meaning, and labelled origin: computed. Nobody made these connections.",
        access: Access::Read,
        fields: &[field(
            "page",
            "Page id, or the exact title",
            MAX_NAME_BYTES,
            true,
        )],
    },
    ToolDef {
        name: "graph_neighbors",
        title: "Graph neighbors",
        description: "What connects to a page, #tag, person or row: links, embeds, parents, tags, rows, owners and computed similarity. Each connection says its origin.",
        access: Access::Read,
        fields: &[field(
            "node",
            "Page id or exact title, #tag, or a person's name",
            MAX_NAME_BYTES,
            true,
        )],
    },
    ToolDef {
        name: "list_tasks",
        title: "List tasks",
        description: "List rows from the databases in this workspace, with status, owner and due date. Filters are optional.",
        access: Access::Read,
        fields: &[
            field(
                "status",
                "Status name, or a group: todo, active, review, done",
                MAX_NAME_BYTES,
                false,
            ),
            field("owner", "Owner name", MAX_NAME_BYTES, false),
            field(
                "database",
                "Database id, or its exact name",
                MAX_NAME_BYTES,
                false,
            ),
            CURSOR,
        ],
    },
    ToolDef {
        name: "create_page",
        title: "Create a page",
        description: "Create a page with text. Each line becomes a block: '# ' and '## ' make headings, '- ' makes a bullet, [[Exact Title]] links a page. No HTML is parsed. Asks the person to approve in the assistant.",
        access: Access::Write,
        fields: &[
            field("title", "The new page's title", MAX_NAME_BYTES, true),
            field("text", "The page's text", MAX_TEXT_BYTES, true),
            field(
                "parent",
                "Parent page id, or the exact title",
                MAX_NAME_BYTES,
                false,
            ),
        ],
    },
    ToolDef {
        name: "append_to_page",
        title: "Add to a page",
        description: "Add text to the end of a page. Each line becomes a block: '# ' and '## ' make headings, '- ' makes a bullet, [[Exact Title]] links a page. No HTML is parsed. Asks the person to approve in the assistant.",
        access: Access::Write,
        fields: &[
            field("page", "Page id, or the exact title", MAX_NAME_BYTES, true),
            field("text", "Text to add", MAX_TEXT_BYTES, true),
        ],
    },
    ToolDef {
        name: "add_task",
        title: "Add a task",
        description: "Add one row to a database. Its status is the database's first to-do status. Asks the person to approve in the assistant.",
        access: Access::Write,
        fields: &[
            field("task", "The task's name", MAX_NAME_BYTES, true),
            field(
                "database",
                "Database id, or its exact name",
                MAX_NAME_BYTES,
                true,
            ),
            field("owner", "Owner name", MAX_NAME_BYTES, false),
            field("due", "Due date, as YYYY-MM-DD", 64, false),
        ],
    },
];

impl ToolDef {
    /// The JSON Schema for this tool's arguments, with any fields a surface adds.
    pub fn input_schema(&self, extra: &[Field]) -> Value {
        let mut properties = Map::new();
        let mut required = Vec::new();
        for f in self.fields.iter().chain(extra) {
            properties.insert(
                f.name.into(),
                json!({ "type": "string", "description": f.description, "maxLength": f.max_bytes }),
            );
            if f.required {
                required.push(f.name);
            }
        }
        json!({ "type": "object", "additionalProperties": false, "required": required, "properties": properties })
    }

    /// The tool as MCP and the assistant list it.
    pub fn to_json(&self, extra: &[Field]) -> Value {
        let read = self.access == Access::Read;
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": self.input_schema(extra),
            "annotations": {
                "readOnlyHint": read,
                "destructiveHint": false,
                "idempotentHint": read,
                "openWorldHint": false,
            },
        })
    }
}

/// The tool of this name.
pub fn find(name: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.name == name)
}

/// The tools a surface offers. Write tools are left out in read-only mode.
pub fn definitions(read_only: bool) -> Vec<&'static ToolDef> {
    TOOLS
        .iter()
        .filter(|t| !read_only || t.access == Access::Read)
        .collect()
}

/// Why a call did nothing. A model can act on the code and the message.
#[derive(Clone, Debug, PartialEq)]
pub struct Refusal {
    pub code: &'static str,
    pub message: String,
    pub candidates: Vec<Value>,
}

impl Refusal {
    /// A refusal with no candidates.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            candidates: vec![],
        }
    }

    /// The error envelope.
    pub fn to_json(&self) -> Value {
        let mut out = json!({ "ok": false, "code": self.code, "message": self.message });
        if !self.candidates.is_empty() {
            out["candidates"] = Value::Array(self.candidates.clone());
        }
        out
    }
}

/// The refusal for a write the person declined.
pub fn declined() -> Refusal {
    Refusal::new("CN-TOL-0005", "The person declined this change.")
}

type Args<'a> = BTreeMap<&'static str, &'a str>;

// Everything below U+0020 except tab and line breaks has no place in notes.
fn bad_control(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t') && (c as u32) < 0x20)
}

/// Checks arguments against the tool's own schema table.
fn check<'a>(def: &ToolDef, args: &'a Value) -> Result<Args<'a>, Refusal> {
    let bad = |m: String| Refusal::new("CN-TOL-0002", m);
    let Some(obj) = args.as_object() else {
        return Err(bad("Arguments must be a JSON object.".into()));
    };
    if let Some(unknown) = obj
        .keys()
        .find(|k| !def.fields.iter().any(|f| f.name == *k))
    {
        let known: Vec<_> = def.fields.iter().map(|f| f.name).collect();
        let shown: String = unknown.chars().take(64).collect();
        return Err(bad(format!(
            "Unknown argument {shown:?}. {} takes: {}.",
            def.name,
            known.join(", ")
        )));
    }
    let mut out = Args::new();
    for f in def.fields {
        let value = match obj.get(f.name) {
            Some(Value::String(s)) if s.is_empty() && !f.required => continue,
            Some(Value::String(s)) => s.as_str(),
            // Models often send null for an optional argument they mean to skip.
            None | Some(Value::Null) if !f.required => continue,
            None | Some(Value::Null) => {
                return Err(bad(format!("{} is required.", f.name)));
            }
            Some(_) => return Err(bad(format!("{} must be a string.", f.name))),
        };
        if value.len() > f.max_bytes {
            return Err(Refusal::new(
                "CN-TOL-0007",
                format!("{} is over its limit of {} bytes.", f.name, f.max_bytes),
            ));
        }
        if bad_control(value) {
            return Err(bad(format!(
                "{} holds control characters, which notes cannot store.",
                f.name
            )));
        }
        out.insert(f.name, value);
    }
    Ok(out)
}

/// Parses a raw argument string from a model. Empty means no arguments.
pub fn parse_args(raw: &str) -> Result<Value, Refusal> {
    if raw.len() > MAX_ARGS_BYTES {
        return Err(Refusal::new(
            "CN-TOL-0007",
            format!("Arguments are over the limit of {MAX_ARGS_BYTES} bytes."),
        ));
    }
    if raw.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(raw)
        .map_err(|_| Refusal::new("CN-TOL-0002", "Arguments are not valid JSON."))
}

/// NFC and lower case, for matching only. Text stays as written everywhere else.
fn fold(s: &str) -> String {
    s.nfc().collect::<String>().to_lowercase()
}

fn clip(s: &str, max: usize) -> (&str, bool) {
    if s.len() <= max {
        return (s, false);
    }
    let mut end = 0;
    for (i, g) in s.grapheme_indices(true) {
        if i + g.len() > max {
            break;
        }
        end = i + g.len();
    }
    (&s[..end], true)
}

fn page_ref(id: &str, title: &str) -> Value {
    json!({ "id": id, "title": clip(title, MAX_NAME_BYTES).0 })
}

fn kind_of(m: &PageMeta, view: Option<&str>) -> &'static str {
    if m.database.is_some() {
        "database"
    } else if view == Some(casual_note_model::VIEW_CANVAS) {
        "canvas"
    } else {
        "page"
    }
}

fn internal(e: impl std::fmt::Display) -> Refusal {
    Refusal::new(
        "CN-TOL-0015",
        format!("Internal error, nothing changed: {e}"),
    )
}

/// Resolves a `page` argument: an exact id wins, then a unique title.
fn resolve_page(ws: &Workspace, arg: &str) -> Result<PageMeta, Refusal> {
    let all = ws.pages();
    let live = |m: PageMeta| {
        if m.trashed {
            let (id, title) = (m.id.clone(), m.title.clone());
            let (shown, _) = clip(&title, 200);
            Err(Refusal::new(
                "CN-TOL-0009",
                format!("Page {id} ({shown}) is in the trash. A person can restore it."),
            ))
        } else {
            Ok(m)
        }
    };
    if let Some(m) = all.iter().find(|m| m.id == arg) {
        return live(m.clone());
    }
    let wanted = fold(arg);
    let same: Vec<&PageMeta> = all.iter().filter(|m| fold(&m.title) == wanted).collect();
    let open: Vec<&&PageMeta> = same.iter().filter(|m| !m.trashed).collect();
    match (open.as_slice(), same.first()) {
        ([one], _) => Ok((**one).clone()),
        ([], Some(gone)) => live((*gone).clone()),
        ([], None) => Err(Refusal::new(
            "CN-TOL-0003",
            "No page has that id or title. Try search_notes or list_pages.",
        )),
        (several, _) => {
            let mut refusal = Refusal::new(
                "CN-TOL-0004",
                "Several pages have that title. Call again with one of these ids.",
            );
            refusal.candidates = several
                .iter()
                .take(20)
                .map(|m| {
                    let parent = m
                        .parent
                        .as_ref()
                        .and_then(|p| all.iter().find(|x| &x.id == p));
                    json!({ "id": m.id, "title": clip(&m.title, 200).0, "parent": parent.map(|p| clip(&p.title, 200).0) })
                })
                .collect();
            Err(refusal)
        }
    }
}

fn database_trashed(all: &[PageMeta], id: &str) -> bool {
    all.iter()
        .any(|m| m.database.as_deref() == Some(id) && m.trashed)
}

/// The live databases, in a stable order.
fn databases(ws: &Workspace) -> Result<Vec<Database>, Refusal> {
    let all = ws.pages();
    let mut out = Vec::new();
    for id in ws.database_ids() {
        if !database_trashed(&all, &id) {
            out.push(ws.database(&id).map_err(internal)?);
        }
    }
    Ok(out)
}

fn resolve_database(ws: &Workspace, arg: &str) -> Result<Database, Refusal> {
    let dbs = databases(ws)?;
    if let Some(d) = dbs.iter().find(|d| d.id == arg) {
        return Ok(d.clone());
    }
    let wanted = fold(arg);
    let same: Vec<&Database> = dbs.iter().filter(|d| fold(&d.name) == wanted).collect();
    match same.as_slice() {
        [one] => Ok((*one).clone()),
        [] => {
            let mut refusal = Refusal::new(
                "CN-TOL-0008",
                "No database has that id or name. These exist.",
            );
            refusal.candidates = dbs
                .iter()
                .take(20)
                .map(|d| json!({ "id": d.id, "name": clip(&d.name, 200).0 }))
                .collect();
            Err(refusal)
        }
        several => {
            let mut refusal = Refusal::new(
                "CN-TOL-0004",
                "Several databases have that name. Call again with one of these ids.",
            );
            refusal.candidates = several
                .iter()
                .take(20)
                .map(|d| json!({ "id": d.id, "name": clip(&d.name, 200).0 }))
                .collect();
            Err(refusal)
        }
    }
}

fn owner_column(db: &Database) -> Option<&Column> {
    db.columns
        .iter()
        .find(|c| c.kind == "person")
        .or_else(|| db.columns.iter().find(|c| fold(&c.name) == "owner"))
}

fn due_column(db: &Database) -> Option<&Column> {
    db.columns
        .iter()
        .find(|c| c.kind == "date" && fold(&c.name) == "due")
        .or_else(|| db.columns.iter().find(|c| c.kind == "date"))
        .or_else(|| db.columns.iter().find(|c| fold(&c.name) == "due"))
}

fn offset_cursor(cursor: Option<&str>, total: usize) -> Result<usize, Refusal> {
    let bad = || {
        Refusal::new(
            "CN-TOL-0013",
            "That cursor is not valid. Start again without one.",
        )
    };
    match cursor {
        None => Ok(0),
        Some(c) => {
            let n: usize = c
                .strip_prefix('c')
                .filter(|d| d.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|d| d.parse().ok())
                .ok_or_else(bad)?;
            if n > total { Err(bad()) } else { Ok(n) }
        }
    }
}

/// Takes items from `start` until the count or the byte ceiling is reached, and says so.
fn paged(items: &[Value], start: usize, max: usize, notes: Vec<String>) -> Value {
    let mut data = Vec::new();
    let mut used = ENVELOPE_BYTES + notes.iter().map(String::len).sum::<usize>();
    for item in &items[start..] {
        let size = item.to_string().len() + 1;
        if data.len() == max || (used + size > MAX_RESULT_BYTES && !data.is_empty()) {
            break;
        }
        used += size;
        data.push(item.clone());
    }
    let returned = data.len();
    let mut out = json!({ "ok": true, "data": data });
    if start + returned < items.len() {
        out["truncated"] = json!({
            "returned": returned,
            "total": items.len(),
            "next_cursor": format!("c{}", start + returned),
        });
    }
    if !notes.is_empty() {
        out["notes"] = json!(notes);
    }
    out
}

/// A page whose text went to the caller, for the approval card's list of sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageRead {
    pub id: String,
    pub title: String,
}

/// What a read returns.
#[derive(Debug)]
pub struct ReadOutput {
    pub result: Value,
    pub pages_read: Vec<PageRead>,
}

/// Runs a read tool. Note text only ever appears inside JSON strings.
pub fn run_read(ws: &Workspace, name: &str, args: &Value) -> Result<ReadOutput, Refusal> {
    let def = known(name)?;
    if def.access != Access::Read {
        return Err(Refusal::new(
            "CN-TOL-0001",
            format!("{name} is a write, and goes through prepare_write."),
        ));
    }
    let args = check(def, args)?;
    let mut pages_read = Vec::new();
    let result = match name {
        "list_pages" => list_pages(ws, &args)?,
        "search_notes" => search_notes(ws, &args, &mut pages_read)?,
        "read_page" => read_page(ws, &args, &mut pages_read)?,
        "list_tasks" => list_tasks(ws, &args, &mut pages_read)?,
        "related_pages" => related_pages(ws, &args)?,
        "graph_neighbors" => graph_neighbors(ws, &args)?,
        _ => return Err(unknown(name)),
    };
    Ok(ReadOutput { result, pages_read })
}

fn unknown(name: &str) -> Refusal {
    let shown: String = name.chars().take(64).collect();
    let names: Vec<_> = TOOLS.iter().map(|t| t.name).collect();
    Refusal::new(
        "CN-TOL-0001",
        format!("There is no tool {shown:?}. Tools: {}.", names.join(", ")),
    )
}

fn known(name: &str) -> Result<&'static ToolDef, Refusal> {
    find(name).ok_or_else(|| unknown(name))
}

fn list_pages(ws: &Workspace, args: &Args) -> Result<Value, Refusal> {
    let live: Vec<PageMeta> = ws.pages().into_iter().filter(|m| !m.trashed).collect();
    let start = offset_cursor(args.get("cursor").copied(), live.len())?;
    let mut items = Vec::with_capacity(live.len());
    for m in &live {
        let view = ws.page(&m.id).map_err(internal)?.view;
        items.push(json!({
            "id": m.id,
            "title": clip(&m.title, MAX_NAME_BYTES).0,
            "kind": kind_of(m, Some(&view)),
            "parent": m.parent,
        }));
    }
    Ok(paged(&items, start, MAX_LIST, vec![]))
}

// A window of the original text around the first match, or None when it does not match.
fn snippet(text: &str, needle: &str) -> Option<String> {
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    let mut folded = String::new();
    let mut starts = Vec::with_capacity(graphemes.len());
    for g in &graphemes {
        starts.push(folded.len());
        folded.push_str(&fold(g));
    }
    let at = folded.find(needle)?;
    let index = starts.partition_point(|s| *s <= at).saturating_sub(1);
    let from = index.saturating_sub(SNIPPET_GRAPHEMES / 3);
    let to = (from + SNIPPET_GRAPHEMES).min(graphemes.len());
    Some(graphemes[from..to].concat())
}

fn search_notes(ws: &Workspace, args: &Args, read: &mut Vec<PageRead>) -> Result<Value, Refusal> {
    let needle = fold(args["query"].trim());
    if needle.is_empty() {
        return Err(Refusal::new("CN-TOL-0002", "query must not be blank."));
    }
    let mut hits: Vec<(PageRead, Value)> = Vec::new();
    for m in ws.pages().into_iter().filter(|m| !m.trashed) {
        let who = PageRead {
            id: m.id.clone(),
            title: m.title.clone(),
        };
        let mut hit = |extra: Value, text: String| {
            let mut item = json!({
                "id": who.id, "title": clip(&who.title, MAX_NAME_BYTES).0,
                "match": "text", "snippet": text,
            });
            if let (Some(obj), Some(more)) = (item.as_object_mut(), extra.as_object()) {
                obj.extend(more.clone());
            }
            hits.push((who.clone(), item));
        };
        if let Some(s) = snippet(&m.title, &needle) {
            hit(json!({ "in": "title" }), s);
        }
        let page = ws.page(&m.id).map_err(internal)?;
        for b in &page.blocks {
            if let Some(s) = snippet(&b.plain(), &needle) {
                hit(json!({ "in": "block", "block": b.id }), s);
            }
        }
        for item in &page.canvas.items {
            if let Some(s) = snippet(&item.text, &needle) {
                hit(json!({ "in": "canvas", "item": item.id }), s);
            }
        }
        if let Some(db) = &m.database {
            let db = ws.database(db).map_err(internal)?;
            for r in &db.rows {
                let cells: Vec<&str> = std::iter::once(r.name.as_str())
                    .chain(r.values.values().map(String::as_str))
                    .collect();
                if let Some(s) = snippet(&cells.join(" "), &needle) {
                    hit(json!({ "in": "row", "row": r.id }), s);
                }
            }
        }
    }
    let start = offset_cursor(args.get("cursor").copied(), hits.len())?;
    let items: Vec<Value> = hits.iter().map(|(_, v)| v.clone()).collect();
    let out = paged(
        &items,
        start,
        MAX_HITS,
        vec!["Only exact text was searched. Search by meaning is not available.".into()],
    );
    let returned = out["data"].as_array().map_or(0, Vec::len);
    for (who, _) in hits.iter().skip(start).take(returned) {
        remember(read, who);
    }
    Ok(out)
}

fn remember(read: &mut Vec<PageRead>, who: &PageRead) {
    if !read.iter().any(|r| r.id == who.id) {
        read.push(who.clone());
    }
}

fn runs_markdown(runs: &[Run]) -> String {
    let mut out = String::new();
    for r in runs {
        let mut piece = r.text.clone();
        for m in &r.marks {
            let wrap = match m {
                casual_note_model::Mark::Bold => "**",
                casual_note_model::Mark::Italic => "*",
                casual_note_model::Mark::Code => "`",
                casual_note_model::Mark::Highlight => "==",
            };
            piece = format!("{wrap}{piece}{wrap}");
        }
        match &r.link {
            Some(id) => out.push_str(&format!("[{piece}](page:{id})")),
            None => out.push_str(&piece),
        }
    }
    out
}

fn row_line(db: &Database, r: &Row) -> String {
    let status = db
        .statuses
        .iter()
        .find(|s| s.id == r.status)
        .map_or(r.status.as_str(), |s| s.name.as_str());
    let mut line = format!("- {} | status: {}", r.name, status);
    for c in db.columns.iter().filter(|c| !c.hidden) {
        if let Some(v) = r.values.get(&c.id).filter(|v| !v.is_empty()) {
            line.push_str(&format!(" | {}: {}", c.name, v));
        }
    }
    line
}

fn database_lines(ws: &Workspace, id: &str) -> Vec<String> {
    match ws.database(id) {
        Ok(db) => std::iter::once(format!("Database {} ({}):", db.name, db.id))
            .chain(db.rows.iter().map(|r| row_line(&db, r)))
            .collect(),
        Err(_) => vec![format!("Database {id} could not be read.")],
    }
}

fn block_lines(ws: &Workspace, b: &Block, number: usize) -> Vec<String> {
    let text = runs_markdown(&b.runs);
    let target = b.target.as_deref().unwrap_or("");
    let line = match b.kind {
        BlockKind::Paragraph | BlockKind::Callout => text,
        BlockKind::Heading1 => format!("# {text}"),
        BlockKind::Heading2 => format!("## {text}"),
        BlockKind::Heading3 => format!("### {text}"),
        BlockKind::Bulleted => format!("- {text}"),
        BlockKind::Numbered => format!("{number}. {text}"),
        BlockKind::Todo => format!("- [{}] {text}", if b.checked { "x" } else { " " }),
        BlockKind::Quote => format!("> {text}"),
        BlockKind::Code => format!("```\n{}\n```", b.plain()),
        BlockKind::Divider => "---".into(),
        BlockKind::Database => return database_lines(ws, target),
        BlockKind::Embed => format!("[embedded page](page:{target})"),
    };
    vec![line]
}

fn read_cursor(cursor: Option<&str>) -> Result<(usize, usize), Refusal> {
    let bad = || {
        Refusal::new(
            "CN-TOL-0013",
            "That cursor is not valid. Start again without one.",
        )
    };
    let Some(c) = cursor else { return Ok((0, 0)) };
    let rest = c.strip_prefix('b').ok_or_else(bad)?;
    let (line, offset) = rest.split_once('.').unwrap_or((rest, "0"));
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(line) || !digits(offset) {
        return Err(bad());
    }
    Ok((
        line.parse().map_err(|_| bad())?,
        offset.parse().map_err(|_| bad())?,
    ))
}

fn page_lines(ws: &Workspace, page: &Page) -> Vec<String> {
    let mut lines = Vec::new();
    let mut number = 0;
    for b in &page.blocks {
        number = if b.kind == BlockKind::Numbered {
            number + 1
        } else {
            0
        };
        lines.extend(block_lines(ws, b, number));
    }
    if let Some(db) = &page.meta.database {
        lines.extend(database_lines(ws, db));
    }
    for item in page.canvas.items.iter().filter(|i| !i.text.is_empty()) {
        lines.push(format!("Canvas {}: {}", item.kind, item.text));
    }
    lines
}

fn read_page(ws: &Workspace, args: &Args, read: &mut Vec<PageRead>) -> Result<Value, Refusal> {
    let m = resolve_page(ws, args["page"])?;
    let page = ws.page(&m.id).map_err(internal)?;
    let lines = page_lines(ws, &page);
    let (first, skip) = read_cursor(args.get("cursor").copied())?;
    let bad_cursor = || {
        Refusal::new(
            "CN-TOL-0013",
            "That cursor is not valid. Start again without one.",
        )
    };
    if first > lines.len() {
        return Err(bad_cursor());
    }
    let mut text = String::new();
    let mut next = None;
    for (i, line) in lines.iter().enumerate().skip(first) {
        let offset = if i == first { skip } else { 0 };
        let rest = line.get(offset..).ok_or_else(bad_cursor)?;
        if text.len() + rest.len() < MAX_READ_BYTES {
            text.push_str(rest);
            text.push('\n');
            continue;
        }
        // Ends at a block boundary, unless one block alone is over the limit.
        if text.is_empty() {
            let (part, _) = clip(rest, MAX_READ_BYTES);
            text.push_str(part);
            next = Some(format!("b{i}.{}", offset + part.len()));
        } else {
            next = Some(format!("b{i}"));
        }
        break;
    }
    let mut notes = Vec::new();
    let texts = page
        .canvas
        .items
        .iter()
        .filter(|i| !i.text.is_empty())
        .count();
    if texts > 0 {
        notes.push(format!("Canvas has {texts} text items, listed."));
    }
    if !page.canvas.ink.is_empty() {
        notes.push(format!(
            "{} ink strokes are not shown.",
            page.canvas.ink.len()
        ));
    }
    let mut out = json!({
        "ok": true,
        "data": {
            "id": m.id,
            "title": clip(&m.title, MAX_NAME_BYTES).0,
            "kind": kind_of(&m, Some(&page.view)),
            "parent": m.parent,
            "markdown": text,
        },
    });
    if let Some(cursor) = next {
        out["truncated"] = json!({ "returned_bytes": text.len(), "next_cursor": cursor });
    }
    if !notes.is_empty() {
        out["notes"] = json!(notes);
    }
    remember(
        read,
        &PageRead {
            id: m.id,
            title: m.title,
        },
    );
    Ok(out)
}

fn list_tasks(ws: &Workspace, args: &Args, read: &mut Vec<PageRead>) -> Result<Value, Refusal> {
    let dbs = match args.get("database") {
        Some(d) => vec![resolve_database(ws, d)?],
        None => databases(ws)?,
    };
    let wanted_status = args.get("status").map(|s| fold(s));
    let wanted_owner = args.get("owner").map(|s| fold(s));
    let all = ws.pages();
    let mut items = Vec::new();
    for db in &dbs {
        let (owner, due) = (owner_column(db), due_column(db));
        let cell = |r: &Row, c: Option<&Column>| c.and_then(|c| r.values.get(&c.id)).cloned();
        for r in &db.rows {
            let status = db.statuses.iter().find(|s| s.id == r.status);
            let name = status.map_or(r.status.as_str(), |s| s.name.as_str());
            let group = status.map_or("", |s| s.group.as_str());
            if let Some(w) = &wanted_status
                && *w != fold(name)
                && *w != fold(group)
            {
                continue;
            }
            let who = cell(r, owner);
            if let Some(w) = &wanted_owner
                && Some(w.clone()) != who.as_deref().map(fold)
            {
                continue;
            }
            items.push(json!({
                "id": r.id, "name": r.name, "status": name, "status_group": group,
                "owner": who, "due": cell(r, due),
                "database": { "id": db.id, "name": clip(&db.name, MAX_NAME_BYTES).0 },
            }));
        }
        if let Some(m) = all.iter().find(|m| m.database.as_deref() == Some(&db.id)) {
            remember(
                read,
                &PageRead {
                    id: m.id.clone(),
                    title: m.title.clone(),
                },
            );
        }
    }
    let start = offset_cursor(args.get("cursor").copied(), items.len())?;
    Ok(paged(&items, start, MAX_LIST, vec![]))
}

fn relation(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Link => "link",
        EdgeKind::Embed => "embed",
        EdgeKind::Child => "child",
        EdgeKind::Tag => "tag",
        EdgeKind::Row => "row",
        EdgeKind::Owner => "owner",
        EdgeKind::Similar => "similar",
    }
}

fn origin_fields(item: &mut Value, origin: &Origin) {
    match origin {
        Origin::Made => item["origin"] = json!("made"),
        Origin::Computed { method, score } => {
            item["origin"] = json!("computed");
            item["method"] = json!(method);
            item["score"] = json!(f64::from(*score) / 1000.0);
        }
    }
}

// The other end of an edge at `node`, or None if the edge is elsewhere.
fn other_end<'a>(edge: &'a Edge, node: &str) -> Option<&'a str> {
    if edge.from == node {
        Some(&edge.to)
    } else if edge.to == node {
        Some(&edge.from)
    } else {
        None
    }
}

fn page_node<'a>(g: &'a Graph, page: &str) -> Option<&'a Node> {
    g.nodes.iter().find(|n| {
        n.page.as_deref() == Some(page) && matches!(n.kind, casual_note_graph::NodeKind::Page)
    })
}

fn related_pages(ws: &Workspace, args: &Args) -> Result<Value, Refusal> {
    let m = resolve_page(ws, args["page"])?;
    let g = from_workspace(ws);
    let node = page_node(&g, &m.id)
        .map(|n| n.id.clone())
        .unwrap_or_default();
    let mut items = Vec::new();
    for e in g.edges.iter().filter(|e| e.kind == EdgeKind::Similar) {
        let Some(to) = other_end(e, &node) else {
            continue;
        };
        let Some(n) = g.nodes.iter().find(|n| n.id == to) else {
            continue;
        };
        let mut item = json!({ "id": n.page, "title": clip(&n.label, MAX_NAME_BYTES).0 });
        origin_fields(&mut item, &e.origin);
        items.push(item);
    }
    Ok(paged(
        &items,
        0,
        MAX_RELATED,
        vec!["Computed from shared words. It understands nothing about meaning.".into()],
    ))
}

fn resolve_node<'a>(ws: &Workspace, g: &'a Graph, arg: &str) -> Result<&'a Node, Refusal> {
    if let Some(n) = g.nodes.iter().find(|n| n.id == arg) {
        return Ok(n);
    }
    let bare = fold(arg.trim_start_matches('#'));
    let same: Vec<&Node> = g
        .nodes
        .iter()
        .filter(|n| fold(n.label.trim_start_matches('#')) == bare)
        .collect();
    match same.as_slice() {
        [one] => Ok(one),
        [] => match resolve_page(ws, arg) {
            Ok(m) => page_node(g, &m.id)
                .ok_or_else(|| Refusal::new("CN-TOL-0003", "That page has no place in the graph.")),
            Err(_) => Err(Refusal::new(
                "CN-TOL-0003",
                "Nothing in the graph has that name. Try search_notes.",
            )),
        },
        several => {
            let mut refusal = Refusal::new(
                "CN-TOL-0004",
                "Several things have that name. Call again with one of these ids.",
            );
            refusal.candidates = several
                .iter()
                .take(20)
                .map(|n| json!({ "id": n.id, "kind": n.kind, "label": clip(&n.label, 200).0 }))
                .collect();
            Err(refusal)
        }
    }
}

fn graph_neighbors(ws: &Workspace, args: &Args) -> Result<Value, Refusal> {
    let g = from_workspace(ws);
    let n = resolve_node(ws, &g, args["node"])?;
    let mut items = Vec::new();
    for e in &g.edges {
        let Some(to) = other_end(e, &n.id) else {
            continue;
        };
        let Some(other) = g.nodes.iter().find(|x| x.id == to) else {
            continue;
        };
        let mut item = json!({
            "relation": relation(e.kind),
            "direction": if e.from == n.id { "out" } else { "in" },
            "to": { "id": other.id, "kind": other.kind, "label": clip(&other.label, MAX_NAME_BYTES).0, "page": other.page },
            "count": e.count,
        });
        origin_fields(&mut item, &e.origin);
        items.push(item);
    }
    let mut out = paged(&items, 0, MAX_CONNECTIONS, vec![]);
    let connections = out["data"].take();
    out["data"] = json!({
        "node": { "id": n.id, "kind": n.kind, "label": clip(&n.label, MAX_NAME_BYTES).0, "page": n.page },
        "connections": connections,
    });
    // There is no cursor on this tool, so a cut says how to narrow instead of how to continue.
    if let Some(cut) = out.get_mut("truncated").and_then(Value::as_object_mut) {
        cut.remove("next_cursor");
    }
    Ok(out)
}

/* ---------- Writes ---------- */

/// A validated write. Nothing has changed yet.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedWrite {
    /// Structured facts for the approval card. It has no field for a model's words.
    pub change: Value,
    /// One undo step.
    pub ops: Vec<Op>,
    /// What the model is told once the batch has been applied.
    pub result: Value,
}

// Bidirectional and invisible controls, which can make text read differently from what it holds.
fn invisible(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(c as u32,
            0xAD | 0x61C | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2069
            | 0xFEFF | 0xE0000..=0xE007F)
    })
}

fn split_links(ws: &Workspace, text: &str) -> Vec<Run> {
    let all = ws.pages();
    let plain = |t: &str| Run {
        text: t.into(),
        marks: vec![],
        link: None,
    };
    let mut runs: Vec<Run> = Vec::new();
    let mut rest = text;
    let mut literal = String::new();
    while let Some(open) = rest.find("[[") {
        let after = &rest[open + 2..];
        let Some(close) = after.find("]]") else { break };
        let wanted = fold(&after[..close]);
        let mut same = all
            .iter()
            .filter(|m| !m.trashed && !wanted.is_empty() && fold(&m.title) == wanted);
        literal.push_str(&rest[..open]);
        match (same.next(), same.next()) {
            (Some(m), None) => {
                if !literal.is_empty() {
                    runs.push(plain(&literal));
                    literal.clear();
                }
                runs.push(Run {
                    text: after[..close].into(),
                    marks: vec![],
                    link: Some(m.id.clone()),
                });
            }
            // Stays literal when it does not resolve to exactly one page.
            _ => {
                literal.push_str("[[");
                literal.push_str(&after[..close]);
                literal.push_str("]]");
            }
        }
        rest = &after[close + 2..];
    }
    literal.push_str(rest);
    if !literal.is_empty() {
        runs.push(plain(&literal));
    }
    runs
}

/// Turns `text` into blocks: blank lines are skipped, and no HTML is parsed.
fn parse_blocks(
    ws: &Workspace,
    text: &str,
    ids: &mut dyn FnMut() -> String,
) -> Result<Vec<Block>, Refusal> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if out.len() == MAX_BLOCKS {
            return Err(Refusal::new(
                "CN-TOL-0007",
                format!("text is over the limit of {MAX_BLOCKS} blocks."),
            ));
        }
        let (kind, body) = if let Some(b) = line.strip_prefix("# ") {
            (BlockKind::Heading1, b)
        } else if let Some(b) = line.strip_prefix("## ") {
            (BlockKind::Heading2, b)
        } else if let Some(b) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            (BlockKind::Bulleted, b)
        } else {
            (BlockKind::Paragraph, line)
        };
        out.push(Block {
            id: ids(),
            kind,
            checked: false,
            runs: split_links(ws, body),
            target: None,
        });
    }
    Ok(out)
}

fn block_previews(blocks: &[Block]) -> Value {
    let shown: Vec<Value> = blocks
        .iter()
        .take(PREVIEW_BLOCKS)
        .map(|b| {
            let text = b.plain();
            let (part, truncated) = clip(&text, PREVIEW_BYTES);
            json!({
                "kind": b.kind.as_str(),
                "text": part,
                "text_bytes": text.len(),
                "truncated": truncated,
                "invisible_controls": invisible(&text),
            })
        })
        .collect();
    let chars: usize = blocks.iter().map(|b| b.plain().chars().count()).sum();
    json!({
        "blocks": shown,
        "total_blocks": blocks.len(),
        "total_chars": chars,
    })
}

fn one_line(name: &str, value: &str) -> Result<(), Refusal> {
    if value.trim().is_empty() {
        return Err(Refusal::new(
            "CN-TOL-0002",
            format!("{name} must not be blank."),
        ));
    }
    if value.contains(['\n', '\r']) {
        return Err(Refusal::new(
            "CN-TOL-0002",
            format!("{name} must be one line."),
        ));
    }
    Ok(())
}

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = s.get(r)?;
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    let (Some(y), Some(m), Some(d)) = (num(0..4), num(5..7), num(8..10)) else {
        return false;
    };
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d)
}

/// Validates a write and builds its preview and op batch. Changes nothing.
pub fn prepare_write(
    ws: &Workspace,
    name: &str,
    args: &Value,
    ids: &mut dyn FnMut() -> String,
) -> Result<PreparedWrite, Refusal> {
    let def = known(name)?;
    if def.access != Access::Write {
        return Err(Refusal::new(
            "CN-TOL-0001",
            format!("{name} is a read, and goes through run_read."),
        ));
    }
    let args = check(def, args)?;
    match name {
        "create_page" => prepare_create(ws, &args, ids),
        "append_to_page" => prepare_append(ws, &args, ids),
        "add_task" => prepare_task(ws, &args, ids),
        _ => Err(unknown(name)),
    }
}

fn insert_ops(page: &str, mut after: Option<String>, blocks: Vec<Block>) -> Vec<Op> {
    blocks
        .into_iter()
        .map(|block| {
            let id = block.id.clone();
            Op::InsertBlock {
                page: page.into(),
                after: after.replace(id),
                block,
            }
        })
        .collect()
}

fn prepare_create(
    ws: &Workspace,
    args: &Args,
    ids: &mut dyn FnMut() -> String,
) -> Result<PreparedWrite, Refusal> {
    let title = args["title"];
    one_line("title", title)?;
    let parent = match args.get("parent") {
        Some(p) => Some(resolve_page(ws, p)?),
        None => None,
    };
    let mut blocks = parse_blocks(ws, args["text"], ids)?;
    let added = blocks.len();
    let preview = block_previews(&blocks);
    // A page always has a block to type in, as a new page in the app does.
    if blocks.is_empty() {
        blocks.push(Block {
            id: ids(),
            kind: BlockKind::Paragraph,
            checked: false,
            runs: vec![],
            target: None,
        });
    }
    let id = ids();
    let mut ops = vec![Op::CreatePage {
        id: id.clone(),
        parent: parent.as_ref().map(|p| p.id.clone()),
        title: title.into(),
        database: None,
    }];
    ops.extend(insert_ops(&id, None, blocks));
    let mut change = json!({
        "kind": "create_page",
        "title": title,
        "parent": parent.as_ref().map(|p| page_ref(&p.id, &p.title)),
        "invisible_controls": invisible(title),
    });
    merge(&mut change, preview);
    Ok(PreparedWrite {
        change,
        ops,
        result: json!({ "ok": true, "data": { "id": id, "title": title, "blocks_added": added } }),
    })
}

fn merge(target: &mut Value, more: Value) {
    if let (Some(t), Value::Object(m)) = (target.as_object_mut(), more) {
        t.extend(m);
    }
}

fn prepare_append(
    ws: &Workspace,
    args: &Args,
    ids: &mut dyn FnMut() -> String,
) -> Result<PreparedWrite, Refusal> {
    let m = resolve_page(ws, args["page"])?;
    if m.database.is_some() {
        return Err(Refusal::new(
            "CN-TOL-0011",
            "That page is a database. Use add_task to add a row.",
        ));
    }
    let blocks = parse_blocks(ws, args["text"], ids)?;
    if blocks.is_empty() {
        return Err(Refusal::new("CN-TOL-0002", "text must not be blank."));
    }
    let added = blocks.len();
    let mut change = json!({
        "kind": "append_to_page",
        "page": page_ref(&m.id, &m.title),
        "invisible_controls": invisible(&m.title),
    });
    merge(&mut change, block_previews(&blocks));
    let last = ws
        .page(&m.id)
        .map_err(internal)?
        .blocks
        .last()
        .map(|b| b.id.clone());
    Ok(PreparedWrite {
        change,
        ops: insert_ops(&m.id, last, blocks),
        result: json!({ "ok": true, "data": { "id": m.id, "title": clip(&m.title, MAX_NAME_BYTES).0, "blocks_added": added } }),
    })
}

fn prepare_task(
    ws: &Workspace,
    args: &Args,
    ids: &mut dyn FnMut() -> String,
) -> Result<PreparedWrite, Refusal> {
    let task = args["task"];
    one_line("task", task)?;
    let db = resolve_database(ws, args["database"])?;
    let mut values = BTreeMap::new();
    let mut fill = |col: Option<&Column>, what: &str, value: &str| match col {
        Some(c) => {
            values.insert(c.id.clone(), value.to_owned());
            Ok(())
        }
        None => Err(Refusal::new(
            "CN-TOL-0014",
            format!("This database has no {what} column. Nothing was written."),
        )),
    };
    if let Some(owner) = args.get("owner") {
        one_line("owner", owner)?;
        fill(owner_column(&db), "owner", owner)?;
    }
    if let Some(due) = args.get("due") {
        if !valid_date(due) {
            return Err(Refusal::new(
                "CN-TOL-0002",
                "due must be a real date, as YYYY-MM-DD.",
            ));
        }
        fill(due_column(&db), "due date", due)?;
    }
    let status = db
        .statuses
        .iter()
        .find(|s| s.group == "todo")
        .or_else(|| db.statuses.first());
    let id = ids();
    let row = Row {
        id: id.clone(),
        name: task.into(),
        status: status.map_or(String::new(), |s| s.id.clone()),
        values,
    };
    let after = db.rows.last().map(|r| r.id.clone());
    Ok(PreparedWrite {
        change: json!({
            "kind": "add_row",
            "database": { "id": db.id, "name": clip(&db.name, MAX_NAME_BYTES).0 },
            "name": task,
            "status": status.map(|s| s.name.clone()),
            "owner": args.get("owner"),
            "due": args.get("due"),
            "invisible_controls": invisible(task),
        }),
        ops: vec![Op::AddRow {
            id: db.id.clone(),
            after,
            row,
        }],
        result: json!({ "ok": true, "data": { "row": id, "database": { "id": db.id, "name": clip(&db.name, MAX_NAME_BYTES).0 } } }),
    })
}

/// Applies a prepared batch all or nothing, and returns the result with the undo step.
pub fn commit_write(
    ws: &mut Workspace,
    prepared: &PreparedWrite,
) -> Result<(Value, Vec<Op>), Refusal> {
    let mut undo = Vec::with_capacity(prepared.ops.len());
    for op in &prepared.ops {
        match apply(ws, op) {
            Ok(inverse) => undo.push(inverse),
            Err(e) => {
                for inverse in undo.iter().rev() {
                    let _ = apply(ws, inverse);
                }
                return Err(Refusal::new(
                    "CN-TOL-0010",
                    format!("The change could not be applied. Nothing changed. {e}"),
                ));
            }
        }
    }
    undo.reverse();
    Ok((prepared.result.clone(), undo))
}

/* ---------- JSON faces, for surfaces that hold no Rust types ---------- */

/// Every tool as JSON, for the web app.
pub fn definitions_json(read_only: bool) -> String {
    let all: Vec<Value> = definitions(read_only)
        .into_iter()
        .map(|t| t.to_json(&[]))
        .collect();
    Value::Array(all).to_string()
}

/// Runs a read from a model's raw argument string. Always answers with JSON.
pub fn read_json(ws: &Workspace, name: &str, raw: &str) -> String {
    let out = parse_args(raw).and_then(|args| run_read(ws, name, &args));
    match out {
        Ok(o) => {
            let pages: Vec<Value> = o
                .pages_read
                .iter()
                .map(|p| json!({ "id": p.id, "title": p.title }))
                .collect();
            json!({ "result": o.result, "pages_read": pages }).to_string()
        }
        Err(r) => json!({ "result": r.to_json(), "pages_read": [] }).to_string(),
    }
}

/// Prepares a write from a model's raw argument string. Ids come from `seed`, so preparing again
/// with the same seed gives the same ids.
pub fn prepare_json(ws: &Workspace, name: &str, raw: &str, seed: &str) -> String {
    let mut n = 0u32;
    let mut ids = || {
        n += 1;
        format!("{seed}{n:04x}")
    };
    let out = parse_args(raw).and_then(|args| prepare_write(ws, name, &args, &mut ids));
    match out {
        Ok(p) => {
            let ops = serde_json::to_value(&p.ops).unwrap_or(Value::Null);
            json!({ "ok": true, "change": p.change, "ops": ops, "result": p.result }).to_string()
        }
        Err(r) => r.to_json().to_string(),
    }
}

/// The access of a tool, as its name in JSON, or `null` if there is no such tool.
pub fn access_json(name: &str) -> &'static str {
    match find(name).map(|t| t.access) {
        Some(Access::Read) => "read",
        Some(Access::Write) => "write",
        None => "unknown",
    }
}
