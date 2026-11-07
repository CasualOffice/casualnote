//! What the shell remembers, in its own file. No command writes this directly: every change that
//! could redirect notes, prompts or a process goes through a native confirmation first.
use crate::atomic;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    /// Origins a person confirmed, such as `https://api.openai.com`.
    pub bindings: Vec<String>,
    /// A `llama-server` a person chose through the native dialog.
    pub runtime_path: Option<PathBuf>,
    /// (program hash, model hash) pairs a person has confirmed running.
    pub confirmed_runs: Vec<(String, String)>,
    /// Where notes are saved, if a person moved them.
    pub notes_path: Option<PathBuf>,
}

impl Settings {
    /// An unreadable file gives defaults, which asks again rather than trusting anything.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        atomic::write(
            path,
            &serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?,
        )
    }
}
