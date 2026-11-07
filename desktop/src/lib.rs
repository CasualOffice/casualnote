//! Everything in the shell that can be tested without a window. `main.rs` only connects these to
//! Tauri commands, and adds no policy of its own (docs/44).
pub mod atomic;
pub mod catalogue;
pub mod confirm;
pub mod download;
pub mod error;
pub mod proxy;
pub mod runtime;
pub mod settings;
pub mod stage;
pub mod text;
