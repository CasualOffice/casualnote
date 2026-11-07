//! The browser's handle on the engine. No logic lives here.

use casual_note_merge::Workspace;
use casual_note_ops::{Op, apply};
use wasm_bindgen::prelude::*;

fn fail(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// One open workspace.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Engine {
    ws: Workspace,
}

#[wasm_bindgen]
impl Engine {
    /// `peer` must be fresh on every open.
    #[wasm_bindgen(constructor)]
    pub fn new(peer: u64) -> Result<Engine, JsError> {
        Ok(Self {
            ws: Workspace::new(peer).map_err(fail)?,
        })
    }

    /// Applies an op given as JSON, and returns its inverse as JSON.
    pub fn apply(&mut self, op: &str) -> Result<String, JsError> {
        let op: Op = serde_json::from_str(op).map_err(fail)?;
        serde_json::to_string(&apply(&mut self.ws, &op).map_err(fail)?).map_err(fail)
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

    /// Everything, for storage or another replica.
    pub fn export(&self) -> Result<Vec<u8>, JsError> {
        self.ws.export().map_err(fail)
    }

    /// Merges stored or received bytes.
    pub fn import(&mut self, bytes: &[u8]) -> Result<(), JsError> {
        self.ws.import(bytes).map_err(fail)
    }
}
