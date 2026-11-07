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
}

/// A whole page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    pub meta: PageMeta,
    pub blocks: Vec<Block>,
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
