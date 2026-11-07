//! Stages `../webapp` into `web-dist`, which Tauri embeds. The sources stay unchanged. The copy
//! differs in two ways: its CSP meta tag carries the desktop policy from `tauri.conf.json`, so IPC
//! is allowed and the web policy's `https:` is gone, and a release copy leaves the tests out.
#[path = "src/stage.rs"]
mod stage;

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../webapp");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-changed=capabilities");
    let conf: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string("tauri.conf.json").expect("tauri.conf.json"))
            .expect("tauri.conf.json parses");
    let csp = conf["app"]["security"]["csp"].as_str().expect("a CSP");
    let release = std::env::var("PROFILE").as_deref() == Ok("release");
    stage::stage(Path::new("../webapp"), Path::new("web-dist"), csp, !release).expect("web-dist");
    let engine = Path::new("web-dist/pkg/casual_note_wasm_bg.wasm");
    if !engine.exists() {
        // A green build with a blank window is the failure this exists to prevent.
        let how = "Run tools/build-wasm.sh first: the web app has no engine without webapp/pkg.";
        if release {
            panic!("{how}");
        }
        println!("cargo:warning={how}");
    }
    tauri_build::build();
}
