//! The notes model: plain values, with no storage, CRDT or UI. See docs/22.

use serde::{Deserialize, Serialize};

/// What a block is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockKind {
    Paragraph,
    Heading1,
    Heading2,
    Heading3,
    Bulleted,
    Numbered,
    Todo,
    Quote,
    Callout,
    Code,
    Divider,
    Database,
    Embed,
}

impl BlockKind {
    /// The stored name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::Heading1 => "heading1",
            Self::Heading2 => "heading2",
            Self::Heading3 => "heading3",
            Self::Bulleted => "bulleted",
            Self::Numbered => "numbered",
            Self::Todo => "todo",
            Self::Quote => "quote",
            Self::Callout => "callout",
            Self::Code => "code",
            Self::Divider => "divider",
            Self::Database => "database",
            Self::Embed => "embed",
        }
    }

    /// Reads a stored name. An unknown name from a newer device reads as a paragraph,
    /// so its text stays visible instead of disappearing.
    pub fn parse(name: &str) -> Self {
        match name {
            "heading1" => Self::Heading1,
            "heading2" => Self::Heading2,
            "heading3" => Self::Heading3,
            "bulleted" => Self::Bulleted,
            "numbered" => Self::Numbered,
            "todo" => Self::Todo,
            "quote" => Self::Quote,
            "callout" => Self::Callout,
            "code" => Self::Code,
            "divider" => Self::Divider,
            "database" => Self::Database,
            "embed" => Self::Embed,
            _ => Self::Paragraph,
        }
    }
}

/// Inline formatting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    Bold,
    Italic,
    Code,
    Highlight,
}

impl Mark {
    /// The stored name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::Code => "code",
            Self::Highlight => "highlight",
        }
    }

    /// Every mark.
    pub const ALL: [Mark; 4] = [Mark::Bold, Mark::Italic, Mark::Code, Mark::Highlight];
}

/// A stretch of text with one set of marks. Text is stored exactly as written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<Mark>,
    /// The page this text links to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// One block of a page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    pub kind: BlockKind,
    #[serde(default)]
    pub checked: bool,
    #[serde(default)]
    pub runs: Vec<Run>,
    /// The database or page a `Database` or `Embed` block shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

impl Block {
    /// The block's text with no formatting.
    pub fn plain(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// A page's place in the workspace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageMeta {
    pub id: String,
    pub parent: Option<String>,
    pub title: String,
    pub favorite: bool,
    pub trashed: bool,
    /// Set when this page is a full-page database.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}

/// A whole page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub meta: PageMeta,
    pub blocks: Vec<Block>,
    pub view: String,
    pub canvas: Canvas,
}

/// What can go wrong. Messages carry ids, never note text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// `CN-OPS-0003`: the page or block does not exist.
    NotFound(String),
    /// `CN-OPS-0004`: an id already exists.
    Duplicate(String),
    /// `CN-OPS-0005`: a text range is outside the block or splits a character.
    BadRange(String),
    /// `CN-OPS-0006`: this would break the page tree.
    BadMove(String),
    /// `CN-MRG-0001`: stored or received bytes could not be read.
    BadData(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(m) => write!(f, "CN-OPS-0003: not found: {m}"),
            Self::Duplicate(m) => write!(f, "CN-OPS-0004: already exists: {m}"),
            Self::BadRange(m) => write!(f, "CN-OPS-0005: bad text range: {m}"),
            Self::BadMove(m) => write!(f, "CN-OPS-0006: bad move: {m}"),
            Self::BadData(m) => write!(f, "CN-MRG-0001: unreadable data: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// How a page is shown.
pub const VIEW_PAGE: &str = "page";
/// How a page is shown.
pub const VIEW_CANVAS: &str = "canvas";

/// Where the page's own content sits on its canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub x: f64,
    pub y: f64,
    pub width: f64,
}

impl Default for Note {
    fn default() -> Self {
        Self {
            x: 40.0,
            y: 30.0,
            width: 620.0,
        }
    }
}

/// A sticky note, shape or text box on a canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanvasItem {
    pub id: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub text: String,
}

/// One immutable stroke or arrow (ADR-008). Points are `[x, y, pressure]`. An arrow has two.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ink {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub width: f64,
    pub points: Vec<[f64; 3]>,
}

/// Everything on a page's canvas.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Canvas {
    pub note: Note,
    pub items: Vec<CanvasItem>,
    pub ink: Vec<Ink>,
}

/// A database status. `group` is `todo`, `active`, `review` or `done`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub id: String,
    pub name: String,
    pub group: String,
}

/// A database column. Removing one hides it, so nothing typed into it is lost.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub hidden: bool,
}

/// A database row. Values are text, keyed by column id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub values: std::collections::BTreeMap<String, String>,
}

/// A whole database.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Database {
    pub id: String,
    pub name: String,
    pub view: String,
    pub statuses: Vec<Status>,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
}

const MILLI: f64 = 1000.0;
const COORDINATE_LIMIT: f64 = 1e9;
const MAX_SAMPLES: usize = 65_536;

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7F) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_signed(out: &mut Vec<u8>, v: i64) {
    put_varint(out, ((v << 1) ^ (v >> 63)) as u64);
}

fn get_varint(data: &[u8], at: &mut usize) -> Option<u64> {
    let (mut v, mut shift) = (0u64, 0u32);
    loop {
        let byte = *data.get(*at)?;
        *at += 1;
        if shift >= 64 {
            return None;
        }
        v |= u64::from(byte & 0x7F).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
    }
}

fn get_signed(data: &[u8], at: &mut usize) -> Option<i64> {
    let v = get_varint(data, at)?;
    Some(((v >> 1) as i64) ^ -((v & 1) as i64))
}

/// Encodes samples compactly: fixed point, delta, zigzag, varint. Refuses non-finite or huge values.
pub fn encode_samples(points: &[[f64; 3]]) -> Result<Vec<u8>, Error> {
    if points.len() > MAX_SAMPLES {
        return Err(Error::BadRange("too many samples in a stroke".into()));
    }
    let mut out = Vec::with_capacity(points.len() * 4 + 4);
    put_varint(&mut out, points.len() as u64);
    let mut last = [0i64; 3];
    for p in points {
        for (i, v) in p.iter().enumerate() {
            if !v.is_finite() || v.abs() > COORDINATE_LIMIT {
                return Err(Error::BadRange("a coordinate is out of range".into()));
            }
            let fixed = (v * MILLI).round() as i64;
            put_signed(&mut out, fixed - last[i]);
            last[i] = fixed;
        }
    }
    Ok(out)
}

/// Decodes samples. Damaged data gives an error, never a panic or a huge allocation.
pub fn decode_samples(data: &[u8]) -> Result<Vec<[f64; 3]>, Error> {
    let bad = || Error::BadData("damaged ink samples".into());
    let mut at = 0;
    let n = usize::try_from(get_varint(data, &mut at).ok_or_else(bad)?).map_err(|_| bad())?;
    if n > MAX_SAMPLES || n > data.len() {
        return Err(bad());
    }
    let mut out = Vec::with_capacity(n);
    let mut last = [0i64; 3];
    for _ in 0..n {
        let mut p = [0.0; 3];
        for i in 0..3 {
            let d = get_signed(data, &mut at).ok_or_else(bad)?;
            last[i] = last[i].checked_add(d).ok_or_else(bad)?;
            p[i] = last[i] as f64 / MILLI;
        }
        out.push(p);
    }
    Ok(out)
}
