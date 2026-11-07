//! The browser's handle on the engine. No logic lives here.

use casual_note_merge::Workspace;
use casual_note_ops::{Op, apply};
use casual_note_search::WorkspaceIndex;
use wasm_bindgen::prelude::*;

fn fail(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// One open workspace.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Engine {
    ws: Workspace,
    search: WorkspaceIndex,
    revision: u32,
}

#[wasm_bindgen]
impl Engine {
    /// `peer` must be fresh on every open.
    #[wasm_bindgen(constructor)]
    pub fn new(peer: u64) -> Result<Engine, JsError> {
        Ok(Self {
            ws: Workspace::new(peer).map_err(fail)?,
            search: WorkspaceIndex::new(),
            revision: 0,
        })
    }

    /// Applies an op given as JSON, and returns its inverse as JSON.
    pub fn apply(&mut self, op: &str) -> Result<String, JsError> {
        let op: Op = serde_json::from_str(op).map_err(fail)?;
        let inverse = apply(&mut self.ws, &op).map_err(fail)?;
        self.search.touch(&op);
        self.revision = self.revision.wrapping_add(1);
        serde_json::to_string(&inverse).map_err(fail)
    }

    /// Every page's metadata, as JSON.
    pub fn pages(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.ws.pages()).map_err(fail)
    }

    /// One page, as JSON.
    pub fn page(&self, id: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.ws.page(id).map_err(fail)?).map_err(fail)
    }

    /// Every database id, as JSON.
    pub fn databases(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.ws.database_ids()).map_err(fail)
    }

    /// One database, as JSON.
    pub fn database(&self, id: &str) -> Result<String, JsError> {
        serde_json::to_string(&self.ws.database(id).map_err(fail)?).map_err(fail)
    }

    /// The closed tool set, as JSON.
    pub fn tool_definitions(&self, read_only: bool) -> String {
        casual_note_tools::definitions_json(read_only)
    }

    /// `read`, `write` or `unknown`.
    pub fn tool_access(&self, name: &str) -> String {
        casual_note_tools::access_json(name).into()
    }

    /// Runs a read tool on a model's raw arguments. Always answers with JSON.
    pub fn tool_read(&self, name: &str, args: &str) -> String {
        casual_note_tools::read_json(&self.ws, name, args)
    }

    /// Prepares a write tool on a model's raw arguments, and changes nothing. Always answers
    /// with JSON: the preview and op batch, or a refusal.
    pub fn tool_prepare(&self, name: &str, args: &str, seed: &str) -> String {
        casual_note_tools::prepare_json(&self.ws, name, args, seed)
    }

    /// Raised by every change, so a view can tell whether what it derived is still current.
    pub fn revision(&self) -> u32 {
        self.revision
    }

    /// Searches every live page, block and row, as JSON. Catches up on edits first.
    pub fn search(&mut self, query: &str, limit: u32) -> Result<String, JsError> {
        let limit = usize::try_from(limit).map_err(fail)?;
        serde_json::to_string(&self.search.search(&self.ws, query, limit)).map_err(fail)
    }

    /// The knowledge graph, derived from the notes, as JSON.
    pub fn graph(&self) -> Result<String, JsError> {
        serde_json::to_string(&casual_note_graph::from_workspace(&self.ws)).map_err(fail)
    }

    /// A CasualNote Package of the workspace, or of one page. `created` is supplied by the caller.
    pub fn package_export(&self, created: &str, page: Option<String>) -> Result<Vec<u8>, JsError> {
        casual_note_package::export(&self.ws, created, page.as_deref()).map_err(fail)
    }

    /// One page as Markdown.
    pub fn page_markdown(&self, id: &str) -> Result<String, JsError> {
        casual_note_package::workspace_page_markdown(&self.ws, id).map_err(fail)
    }

    /// Plans importing a package, as `{ ops, report }` JSON, trying it on a scratch copy made with
    /// a fresh `peer`. Changes nothing. The caller runs the ops as one edit.
    pub fn package_plan(&self, bytes: &[u8], peer: u64) -> Result<String, JsError> {
        let package = casual_note_package::read_package(bytes).map_err(fail)?;
        let plan = casual_note_package::plan_import(&self.ws, package, peer).map_err(fail)?;
        serde_json::to_string(&plan).map_err(fail)
    }

    /// Everything, for storage or another replica.
    pub fn export(&self) -> Result<Vec<u8>, JsError> {
        self.ws.export().map_err(fail)
    }

    /// Merges stored or received bytes.
    pub fn import(&mut self, bytes: &[u8]) -> Result<(), JsError> {
        self.ws.import(bytes).map_err(fail)?;
        self.search.touch_all();
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
}

/// Checks a package and describes it, as JSON. Works on no workspace.
#[wasm_bindgen]
pub fn package_inspect(bytes: &[u8]) -> Result<String, JsError> {
    serde_json::to_string(&casual_note_package::inspect(bytes).map_err(fail)?).map_err(fail)
}

/// One file from a package, for showing its Markdown.
#[wasm_bindgen]
pub fn package_entry(bytes: &[u8], path: &str) -> Result<Vec<u8>, JsError> {
    casual_note_package::entry_bytes(bytes, path).map_err(fail)
}
