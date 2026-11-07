//! What a command gives back on failure: a stable code, and a detail with no key or note text.
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CnError {
    pub code: &'static str,
    pub detail: String,
}

impl CnError {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for CnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}
impl std::error::Error for CnError {}

/// Codes proposed here for docs/20, as the registry has none for these yet.
pub const PIN_MISSING: &str = "CN-MDL-0011";
pub const NO_RUNTIME: &str = "CN-MDL-0012";
pub const FILE_WRITE: &str = "CN-DSK-0020";
pub const BUSY: &str = "CN-MDL-0013";
