//! The shell's security configuration, read as it ships. See docs/44 and docs/62.
use casual_note_desktop::stage;
use serde_json::Value;
use std::fs;

fn conf() -> Value {
    serde_json::from_str(&fs::read_to_string("tauri.conf.json").unwrap()).unwrap()
}

#[test]
fn the_csp_names_no_outside_address_and_no_inline_script() {
    let conf = conf();
    let csp = conf["app"]["security"]["csp"].as_str().unwrap();
    for banned in [
        "https:",
        "http://127",
        "localhost:",
        "'unsafe-inline'",
        "'unsafe-eval'",
        " *",
        "data: https",
    ] {
        assert!(!csp.contains(banned), "{banned} in {csp}");
    }
    let connect = csp
        .split(';')
        .find(|d| d.trim().starts_with("connect-src"))
        .unwrap();
    assert_eq!(
        connect.trim(),
        "connect-src 'self' ipc: http://ipc.localhost blob:"
    );
    assert!(
        csp.contains("default-src 'none'")
            && csp.contains("frame-src 'none'")
            && csp.contains("object-src 'none'")
    );
    assert!(
        conf["app"]["security"]
            .get("dangerousDisableAssetCspModification")
            .is_none()
    );
}

#[test]
fn the_webview_gets_no_plugin_api() {
    let caps: Value =
        serde_json::from_str(&fs::read_to_string("capabilities/default.json").unwrap()).unwrap();
    let granted: Vec<&str> = caps["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert_eq!(
        granted,
        ["core:event:allow-listen", "core:event:allow-unlisten"]
    );
    assert_eq!(
        fs::read_dir("capabilities").unwrap().count(),
        1,
        "no other capability file"
    );
}

#[test]
fn the_staged_page_carries_the_desktop_policy() {
    let page = fs::read_to_string("../webapp/index.html").unwrap();
    let conf = conf();
    let csp = conf["app"]["security"]["csp"].as_str().unwrap();
    let staged = stage::with_csp(&page, csp).expect("webapp/index.html has a CSP meta tag");
    assert!(staged.contains(&format!("content=\"{csp}\"")));
    assert!(
        !staged.contains("connect-src 'self' https:"),
        "the web policy is gone"
    );
    assert_eq!(staged.matches("Content-Security-Policy").count(), 1);
    assert!(
        stage::with_csp("<html></html>", csp).is_none(),
        "a page with no policy stops the build"
    );
    let on_disk = fs::read_to_string("web-dist/index.html").unwrap();
    assert!(on_disk.contains(&format!("content=\"{csp}\"")));
}
