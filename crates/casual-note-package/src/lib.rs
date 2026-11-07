//! The CasualNote Package: one file that takes notes out of CasualNote, and brings them back, or
//! is read with no CasualNote software at all. No CRDT concept appears in it. See docs/36.

mod import;
mod read;
mod write;
pub mod zip;

pub use import::{ImportPlan, ImportReport, Renamed, import_package, plan_import};
pub use read::{Inspection, Package, Report, entry_bytes, inspect, read_package};
pub use write::{Snapshot, page_markdown, snapshot, write_package};

use casual_note_merge::Workspace;
use casual_note_model::Database;
use serde_json::Value;
use std::collections::BTreeMap;
use unicode_normalization::UnicodeNormalization;

/// 64 MiB, for one page or database.
pub const MAX_JSON_BYTES: u64 = 64 << 20;
/// 512 MiB, for the manifest.
pub const MAX_MANIFEST_BYTES: u64 = 512 << 20;
/// 16 MiB, for one line of ink.
pub const MAX_INK_LINE_BYTES: u64 = 16 << 20;
/// 2 GiB, for any entry.
pub const MAX_ENTRY_BYTES: u64 = 2 << 30;
/// 20 GiB, for everything.
pub const MAX_TOTAL_BYTES: u64 = 20 << 30;
/// JSON nesting, in levels.
pub const MAX_DEPTH: usize = 64;

/// A refusal, by the code in docs/36. Messages carry counts and ids, never note text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageError {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PackageError {}

pub(crate) fn refuse(code: &'static str, message: &str) -> PackageError {
    PackageError {
        code,
        message: message.to_string(),
    }
}

/// The form of a path used to spot two that collide on some file system.
pub(crate) fn fold_path(path: &str) -> String {
    let once: String = path.nfc().collect::<String>().to_lowercase();
    once.to_uppercase().to_lowercase().nfc().collect()
}

/// Why an id cannot be used, following the schema's `id` definition and the path rules.
pub(crate) fn id_problem(id: &str) -> Option<&'static str> {
    if id.is_empty() {
        return Some("empty");
    }
    if id.chars().count() > 128 {
        return Some("over 128 characters");
    }
    if id
        .chars()
        .any(|c| matches!(c, '/' | '\\') || c.is_control())
    {
        return Some("has a slash or a control character");
    }
    if id.nfc().collect::<String>() != id {
        return Some("is not NFC");
    }
    None
}

fn write_value(out: &mut Vec<u8>, value: &Value, indent: Option<usize>) {
    let newline = |out: &mut Vec<u8>, level: usize| {
        out.push(b'\n');
        out.resize(out.len() + level * 2, b' ');
    };
    match value {
        Value::Array(list) if !list.is_empty() => {
            out.push(b'[');
            for (i, item) in list.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                if let Some(level) = indent {
                    newline(out, level + 1);
                }
                write_value(out, item, indent.map(|l| l + 1));
            }
            if let Some(level) = indent {
                newline(out, level);
            }
            out.push(b']');
        }
        Value::Object(map) if !map.is_empty() => {
            // Sorted here, whatever order the map keeps.
            let sorted: BTreeMap<&String, &Value> = map.iter().collect();
            out.push(b'{');
            for (i, (key, item)) in sorted.into_iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                if let Some(level) = indent {
                    newline(out, level + 1);
                }
                out.extend_from_slice(Value::String(key.clone()).to_string().as_bytes());
                out.push(b':');
                if indent.is_some() {
                    out.push(b' ');
                }
                write_value(out, item, indent.map(|l| l + 1));
            }
            if let Some(level) = indent {
                newline(out, level);
            }
            out.push(b'}');
        }
        other => out.extend_from_slice(other.to_string().as_bytes()),
    }
}

/// Sorted keys, two-space indent, and a final line feed.
pub(crate) fn pretty_json(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write_value(&mut out, value, Some(0));
    out.push(b'\n');
    out
}

/// Sorted keys and no spaces.
pub(crate) fn compact_json(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write_value(&mut out, value, None);
    out
}

/// Exports a workspace, or with `page`, one page and the databases it shows.
pub fn export(ws: &Workspace, created: &str, page: Option<&str>) -> Result<Vec<u8>, PackageError> {
    write_package(&snapshot(ws, page)?, created)
}

/// One page as Markdown, with the rest of the workspace for titles.
pub fn workspace_page_markdown(ws: &Workspace, id: &str) -> Result<String, PackageError> {
    let snap = snapshot(ws, None)?;
    let page = ws
        .page(id)
        .map_err(|e| refuse("CN-PKG-0010", &e.to_string()))?;
    let titles: BTreeMap<&str, &str> = snap
        .pages
        .iter()
        .map(|p| (p.meta.id.as_str(), p.meta.title.as_str()))
        .collect();
    let dbs: BTreeMap<&str, &Database> =
        snap.databases.iter().map(|d| (d.id.as_str(), d)).collect();
    Ok(page_markdown(&page, &titles, &dbs))
}
