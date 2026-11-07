//! The reviewed model list, compiled into the app and changed only by an app update (docs/62).
//! Size and SHA-256 are the pins Hugging Face's public JSON API reported for a fixed commit on
//! 2026-11-07. That is the host's own statement. None of these files has been fetched or run here.
use crate::{download::Job, error, error::CnError};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const BUILT_IN: &str = include_str!("../catalogue.json");

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub position: String,
    pub licence: String,
    pub licence_url: String,
    pub url: String,
    pub bytes: u64,
    /// None until a pin exists, and then nothing can be downloaded.
    pub sha256: Option<String>,
    pub hosts: Vec<String>,
}

impl Entry {
    /// The pin, only if it is a full SHA-256.
    pub fn pin(&self) -> Option<&str> {
        self.sha256
            .as_deref()
            .filter(|s| s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
    }

    pub fn source_host(&self) -> String {
        url::Url::parse(&self.url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default()
    }

    pub fn file_name(&self) -> Option<String> {
        self.pin().map(|pin| format!("{pin}.gguf"))
    }

    /// The only route from an entry to a download. A missing pin or size refuses.
    pub fn job(&self, dir: &Path) -> Result<Job, CnError> {
        let pin = self.pin().ok_or_else(|| {
            CnError::new(
                error::PIN_MISSING,
                format!("{} has no SHA-256 pin in this app's catalogue.", self.name),
            )
        })?;
        if self.bytes == 0 {
            return Err(CnError::new(
                error::PIN_MISSING,
                format!("{} has no size pin in this app's catalogue.", self.name),
            ));
        }
        Ok(Job::new(
            &self.id,
            &self.url,
            pin,
            self.bytes,
            self.hosts.clone(),
            dir.to_path_buf(),
        ))
    }
}

pub fn parse(json: &str) -> Result<Vec<Entry>, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_four_models_are_pinned_to_a_commit_and_a_hash() {
        let all = parse(BUILT_IN).unwrap();
        let names: Vec<_> = all.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Qwen3 1.7B", "Qwen3 4B", "Phi-4-mini", "Qwen3 8B"]);
        for entry in &all {
            assert!(entry.pin().is_some(), "{}", entry.name);
            assert!(entry.bytes > 1 << 30);
            assert!(entry.url.starts_with("https://huggingface.co/"));
            let commit = entry
                .url
                .split("/resolve/")
                .nth(1)
                .unwrap()
                .split('/')
                .next()
                .unwrap();
            assert_eq!(
                commit.len(),
                40,
                "{} must name a commit, not a branch",
                entry.name
            );
            assert!(!entry.licence.is_empty() && entry.licence_url.starts_with("https://"));
            assert!(entry.job(Path::new("/tmp")).is_ok());
        }
    }

    #[test]
    fn a_missing_or_malformed_pin_refuses_and_says_so() {
        for sha in [
            "null",
            "\"\"",
            "\"abc\"",
            &format!("\"{}\"", "Z".repeat(64)),
        ] {
            let json = format!(
                r#"[{{"id":"m","name":"M","position":"","licence":"MIT","licence_url":"https://x","url":"https://huggingface.co/a","bytes":9,"sha256":{sha},"hosts":[]}}]"#
            );
            let entry = &parse(&json).unwrap()[0];
            let refused = entry.job(Path::new("/tmp")).unwrap_err();
            assert_eq!(refused.code, error::PIN_MISSING);
            assert!(refused.detail.contains("no SHA-256 pin"));
        }
    }
}
