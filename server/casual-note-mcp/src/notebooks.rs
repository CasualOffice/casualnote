//! Notebook files under the roots: finding, opening, reading, and saving atomically.
//!
//! A notebook is a file of `Workspace::export` bytes, named `*.casualnote`. Every call
//! canonicalises its path again. A file changed by someone else is merged before reading and
//! before saving, so a write from another process is kept, because merging is what the format is
//! for. There is no lock yet (docs/63 waits on docs/23), so two savers can still race.

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use casual_note_merge::Workspace;
use casual_note_tools::{
    Access, Field, MAX_NAME_BYTES, PreparedWrite, Refusal, commit_write, definitions, find,
    prepare_write, run_read,
};
use serde_json::{Value, json};

use crate::sandbox::{Sandbox, SandboxError};

/// The file name extension `list_notebooks` looks for.
pub const EXTENSION: &str = "casualnote";
/// Notebooks held open, as `casual-calc-mcp` does.
pub const MAX_OPEN: usize = 16;
/// Largest notebook file opened.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const WALK_DEPTH: usize = 8;
const WALK_LIMIT: usize = 1000;

static NOTEBOOK: Field = Field {
    name: "notebook",
    description: "Path to the notebook file. Needed when several exist under the roots",
    max_bytes: 4096,
    required: false,
};

fn random() -> u64 {
    RandomState::new().build_hasher().finish()
}

type Stamp = (u64, Option<SystemTime>);

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

#[derive(Debug)]
struct Open {
    path: PathBuf,
    ws: Workspace,
    seen: Option<Stamp>,
    used: u64,
}

/// What a session has open.
#[derive(Debug)]
pub struct Notebooks {
    sandbox: Sandbox,
    read_only: bool,
    open: Vec<Open>,
    clock: u64,
}

impl Notebooks {
    /// A session over `sandbox`.
    pub fn new(sandbox: Sandbox, read_only: bool) -> Self {
        Self {
            sandbox,
            read_only,
            open: Vec::new(),
            clock: 0,
        }
    }

    /// The tools this session offers. Write tools are left out in read-only mode.
    pub fn definitions(&self) -> Vec<Value> {
        let mut out: Vec<Value> = definitions(self.read_only)
            .into_iter()
            .map(|t| t.to_json(std::slice::from_ref(&NOTEBOOK)))
            .collect();
        out.push(json!({
            "name": "list_notebooks",
            "title": "List notebooks",
            "description": "List the notebook files found under the allowed roots, with name and path.",
            "inputSchema": { "type": "object", "additionalProperties": false, "required": [], "properties": {} },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
        }));
        out
    }

    /// Drops everything cached, so the next call reads from disk.
    pub fn forget_all(&mut self) {
        self.open.clear();
    }

    /// Runs one tool.
    pub fn call(&mut self, name: &str, args: &Value) -> Result<Value, Refusal> {
        if name == "list_notebooks" {
            return self.list_notebooks(args);
        }
        let Some(def) = find(name) else {
            let shown: String = name.chars().take(64).collect();
            let names: Vec<String> = self
                .definitions()
                .iter()
                .filter_map(|d| d["name"].as_str().map(str::to_owned))
                .collect();
            return Err(Refusal::new(
                "CN-TOL-0001",
                format!("There is no tool {shown:?}. Tools: {}.", names.join(", ")),
            ));
        };
        if def.access == Access::Write && self.read_only {
            return Err(Refusal::new(
                "CN-TOL-0006",
                "This server was started with --read-only, so nothing can be written.",
            ));
        }
        let mut args = args.clone();
        let notebook = match args.as_object_mut().map(|o| o.remove(NOTEBOOK.name)) {
            Some(Some(Value::String(path))) => Some(path),
            Some(None | Some(Value::Null)) => None,
            Some(Some(_)) => return Err(Refusal::new("CN-TOL-0002", "notebook must be a string.")),
            None => {
                return Err(Refusal::new(
                    "CN-TOL-0002",
                    "Arguments must be a JSON object.",
                ));
            }
        };
        if notebook
            .as_ref()
            .is_some_and(|p| p.len() > NOTEBOOK.max_bytes)
        {
            return Err(Refusal::new(
                "CN-TOL-0007",
                format!(
                    "notebook is over its limit of {} bytes.",
                    NOTEBOOK.max_bytes
                ),
            ));
        }
        let path = self.choose(notebook.as_deref())?;
        let at = self.load(&path)?;
        match def.access {
            Access::Read => Ok(run_read(&self.open[at].ws, name, &args)?.result),
            Access::Write => self.write(at, name, &args),
        }
    }

    fn write(&mut self, at: usize, name: &str, args: &Value) -> Result<Value, Refusal> {
        let seed = format!("{:016x}{:016x}", random(), random());
        let mut n = 0u32;
        let mut ids = || {
            n += 1;
            format!("{seed}{n:04x}")
        };
        let prepared: PreparedWrite = prepare_write(&self.open[at].ws, name, args, &mut ids)?;
        let (result, _undo) = commit_write(&mut self.open[at].ws, &prepared)?;
        let saved = self.open[at]
            .ws
            .export()
            .map_err(|e| e.to_string())
            .and_then(|bytes| save(&self.open[at].path, &bytes).map_err(|e| e.to_string()));
        match saved {
            Ok(()) => {
                self.open[at].seen = stamp(&self.open[at].path);
                Ok(result)
            }
            Err(why) => {
                // Memory now holds a change the file does not, so it is dropped.
                self.open.remove(at);
                Err(Refusal::new(
                    "CN-TOL-0015",
                    format!("The change could not be saved, so it was not made. {why}"),
                ))
            }
        }
    }

    fn resolve(&self, path: &str) -> Result<PathBuf, Refusal> {
        let shown: String = path.chars().take(200).collect();
        self.sandbox.resolve_existing(path).map_err(|e| match e {
            SandboxError::NoRoots => no_roots(),
            SandboxError::Outside(_) => Refusal::new(
                "CN-MCP-0002",
                format!("{shown} is outside every allowed root. Nothing was read."),
            ),
            SandboxError::Unresolvable(..) | SandboxError::NotAFile(_) => Refusal::new(
                "CN-MCP-0003",
                format!("{shown} is not a notebook file. Try list_notebooks."),
            ),
        })
    }

    fn choose(&self, notebook: Option<&str>) -> Result<PathBuf, Refusal> {
        if let Some(path) = notebook {
            return self.resolve(path);
        }
        if self.sandbox.roots().is_empty() {
            return Err(no_roots());
        }
        let (found, _) = self.walk();
        match found.as_slice() {
            [one] => Ok(one.clone()),
            [] => Err(Refusal::new(
                "CN-MCP-0003",
                "No notebook file was found under the allowed roots.",
            )),
            several => {
                let mut refusal = Refusal::new(
                    "CN-MCP-0004",
                    "Several notebooks exist. Call again with notebook set to one of these.",
                );
                refusal.candidates = several.iter().take(50).map(|p| notebook_json(p)).collect();
                Err(refusal)
            }
        }
    }

    /// Opens a notebook, or merges it again if the file changed. Returns its slot.
    fn load(&mut self, path: &Path) -> Result<usize, Refusal> {
        self.clock += 1;
        let now = stamp(path);
        if let Some(at) = self.open.iter().position(|o| o.path == path) {
            self.open[at].used = self.clock;
            if self.open[at].seen != now {
                let bytes = read_notebook(path)?;
                self.open[at].ws.import(&bytes).map_err(not_notebook)?;
                self.open[at].seen = now;
            }
            return Ok(at);
        }
        let bytes = read_notebook(path)?;
        // A fresh peer on every open (ADR-030).
        let mut ws = Workspace::new(random() >> 1).map_err(not_notebook)?;
        ws.import(&bytes).map_err(not_notebook)?;
        if self.open.len() >= MAX_OPEN
            && let Some(oldest) = (0..self.open.len()).min_by_key(|i| self.open[*i].used)
        {
            self.open.remove(oldest);
        }
        self.open.push(Open {
            path: path.to_owned(),
            ws,
            seen: now,
            used: self.clock,
        });
        Ok(self.open.len() - 1)
    }

    fn walk(&self) -> (Vec<PathBuf>, bool) {
        let mut found = Vec::new();
        let mut cut = false;
        for root in self.sandbox.roots() {
            let mut stack = vec![(root.clone(), 0usize)];
            while let Some((dir, depth)) = stack.pop() {
                let Ok(entries) = fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    // Symlinks are not followed, so a walk cannot leave a root.
                    let Ok(kind) = entry.file_type() else {
                        continue;
                    };
                    let path = entry.path();
                    if kind.is_dir() && depth < WALK_DEPTH {
                        stack.push((path, depth + 1));
                    } else if kind.is_file()
                        && path.extension().and_then(|e| e.to_str()) == Some(EXTENSION)
                    {
                        if found.len() == WALK_LIMIT {
                            cut = true;
                        } else if !found.contains(&path) {
                            found.push(path);
                        }
                    }
                }
            }
        }
        found.sort();
        (found, cut)
    }

    fn list_notebooks(&self, args: &Value) -> Result<Value, Refusal> {
        if !args.as_object().is_some_and(serde_json::Map::is_empty) {
            return Err(Refusal::new(
                "CN-TOL-0002",
                "list_notebooks takes no arguments.",
            ));
        }
        if self.sandbox.roots().is_empty() {
            return Err(no_roots());
        }
        let (found, cut) = self.walk();
        let data: Vec<Value> = found.iter().map(|p| notebook_json(p)).collect();
        let mut out = json!({ "ok": true, "data": data });
        if cut {
            out["truncated"] = json!({ "returned": data_len(&out) });
        }
        Ok(out)
    }
}

fn data_len(v: &Value) -> usize {
    v["data"].as_array().map_or(0, Vec::len)
}

fn notebook_json(path: &Path) -> Value {
    let name = path.file_stem().map(|n| n.to_string_lossy().into_owned());
    let name = name.map(|n| n.chars().take(MAX_NAME_BYTES).collect::<String>());
    json!({ "name": name, "path": path.display().to_string() })
}

fn no_roots() -> Refusal {
    Refusal::new(
        "CN-MCP-0001",
        "No notebook can be reached: the server was started without --root.",
    )
}

fn not_notebook(e: impl std::fmt::Display) -> Refusal {
    Refusal::new("CN-MCP-0003", format!("That file is not a notebook. {e}"))
}

fn read_notebook(path: &Path) -> Result<Vec<u8>, Refusal> {
    let size = fs::metadata(path).map_err(not_notebook)?.len();
    if size > MAX_FILE_BYTES {
        return Err(Refusal::new(
            "CN-TOL-0007",
            format!("That file is over the limit of {MAX_FILE_BYTES} bytes."),
        ));
    }
    fs::read(path).map_err(not_notebook)
}

/// Replaces a notebook file so a reader sees the old bytes or the new ones, never a mix. The rename
/// replaces the name itself, so nothing is written through a link swapped in after the check.
pub fn save(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent directory"))?;
    let name = path
        .file_name()
        .map_or_else(Default::default, |n| n.to_string_lossy());
    let temp = dir.join(format!(".{name}.{:016x}.tmp", random()));
    let attempt = || -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        if let Ok(meta) = fs::metadata(path) {
            file.set_permissions(meta.permissions())?;
        }
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        // Makes the rename itself durable, where the platform allows opening a directory.
        if let Ok(d) = fs::File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(())
    };
    attempt().inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}
