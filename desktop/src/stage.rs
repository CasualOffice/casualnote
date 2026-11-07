//! Copies the web app for embedding. Used by `build.rs` and by tests, so it needs no other crate.
use std::{fs, io, path::Path};

/// Replaces the content of the page's CSP meta tag. Returns None if the page has no such tag, since
/// shipping a page without its policy must stop the build rather than pass quietly.
pub fn with_csp(html: &str, csp: &str) -> Option<String> {
    let at = html.find("http-equiv=\"Content-Security-Policy\"")?;
    let open = at + html[at..].find("content=\"")? + "content=\"".len();
    let close = open + html[open..].find('"')?;
    Some(format!("{}{}{}", &html[..open], csp, &html[close..]))
}

fn copy_tree(from: &Path, to: &Path, tests: bool) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') || (!tests && name == "tests") {
            continue;
        }
        let (source, target) = (entry.path(), to.join(&name));
        if entry.file_type()?.is_dir() {
            copy_tree(&source, &target, tests)?;
        } else {
            let same =
                fs::read(&target).is_ok_and(|old| fs::read(&source).is_ok_and(|new| old == new));
            if !same {
                fs::copy(&source, &target)?;
            }
        }
    }
    Ok(())
}

pub fn stage(from: &Path, to: &Path, csp: &str, tests: bool) -> io::Result<()> {
    copy_tree(from, to, tests)?;
    let page = fs::read_to_string(from.join("index.html"))?;
    let page = with_csp(&page, csp)
        .ok_or_else(|| io::Error::other("webapp/index.html has no CSP meta tag"))?;
    fs::write(to.join("index.html"), page)
}
