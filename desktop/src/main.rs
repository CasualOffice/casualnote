//! The CasualNote desktop shell. A window running `webapp/`, plus what a web page cannot do: real
//! files, native menus and dialogs, downloads, and a supervised local model. Commands here connect
//! the tested modules to Tauri, and paths never come from the webview (docs/44).
//!
//! macOS is built and run. The Windows and Linux paths are written carefully and are unbuilt.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use casual_note_desktop::{
    atomic, catalogue,
    confirm::{self, Gate, Kind},
    download::{self, Control, Manager, Outcome, Phase, Report},
    error::{self, CnError},
    proxy::{self, Frame, Route},
    runtime::{self, Failures, Launch, Running},
    settings::Settings,
    text,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{
    AppHandle, Emitter, Manager as _, RunEvent, State, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
    ipc::{Channel, InvokeBody, Request, Response},
    menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu},
};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tokio::sync::Notify;

const MAX_NOTES: usize = 1 << 30;
const MAX_PACKAGE: u64 = 3 << 29; // 1.5 GiB, as the web app's import accepts.
const OLLAMA: &str = "http://127.0.0.1:11434";
const CLOSE_WAIT: Duration = Duration::from_secs(3);

struct Shell {
    data: PathBuf,
    settings: Mutex<Settings>,
    gate: Mutex<Gate>,
    lang: Mutex<String>,
    downloads: Manager,
    catalogue: Vec<catalogue::Entry>,
    model: tokio::sync::Mutex<Option<Running>>,
    model_id: Mutex<Option<String>>,
    starting: AtomicBool,
    failures: Mutex<Failures>,
    ai: Mutex<HashMap<String, Arc<Notify>>>,
    hosted: reqwest::Client,
    loopback: reqwest::Client,
    closing: AtomicBool,
    test_sources: bool,
}

type Reply<T> = Result<T, CnError>;

impl Shell {
    fn settings_path(&self) -> PathBuf {
        self.data.join("settings.json")
    }
    fn models_dir(&self) -> PathBuf {
        self.data.join("models")
    }
    fn record_path(&self) -> PathBuf {
        self.data.join("local-model.json")
    }
    fn notes_path(&self) -> PathBuf {
        self.settings
            .lock()
            .ok()
            .and_then(|s| s.notes_path.clone())
            .unwrap_or_else(|| self.data.join("notes.casualnote"))
    }
    fn change_settings(&self, change: impl FnOnce(&mut Settings)) -> Reply<()> {
        let mut settings = self.settings.lock().map_err(|_| write_failed("settings"))?;
        change(&mut settings);
        settings.save(&self.settings_path()).map_err(|e| {
            CnError::new(
                error::FILE_WRITE,
                format!("Settings could not be saved: {}", e.kind()),
            )
        })
    }
    fn entry(&self, id: &str) -> Reply<&catalogue::Entry> {
        self.catalogue.iter().find(|e| e.id == id).ok_or_else(|| {
            CnError::new("CN-MDL-0003", "That model is not in this app's catalogue.")
        })
    }
    fn job(&self, id: &str) -> Reply<download::Job> {
        let mut job = self.entry(id)?.job(&self.models_dir())?;
        job.allow_loopback_http = self.test_sources;
        Ok(job)
    }
    fn language(&self) -> String {
        self.lang.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

fn write_failed(what: &str) -> CnError {
    CnError::new(error::FILE_WRITE, format!("{what} could not be written"))
}

/* ---------- The native confirmation: drawn by Rust, Decline first ---------- */

async fn confirm(app: &AppHandle, shell: &Shell, key: &str, kind: Kind, value: &str) -> Reply<()> {
    shell
        .gate
        .lock()
        .map_err(|_| write_failed("gate"))?
        .begin(key, Instant::now())?;
    let words = confirm::words(&shell.language(), kind);
    let body = words.body.replace("{value}", &text_lines(value));
    // rfd directly, on the main thread, with no webview in reach of it. If the main thread cannot
    // take this, or the answer never comes, that is a decline.
    let (send, answer) = tokio::sync::oneshot::channel();
    let shown = app.run_on_main_thread(move || {
        let dialog = rfd::AsyncMessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title(words.title)
            .set_description(body)
            .set_buttons(confirm::buttons(&words))
            .show();
        tauri::async_runtime::spawn(async move {
            let _ = send.send(confirm::is_allowed(&dialog.await, &words));
        });
    });
    let allowed = shown.is_ok() && answer.await.unwrap_or(false);
    shell
        .gate
        .lock()
        .map_err(|_| write_failed("gate"))?
        .finish(key, allowed, Instant::now());
    if allowed {
        Ok(())
    } else {
        Err(CnError::new(
            confirm::DECLINED,
            "You declined, so nothing changed.",
        ))
    }
}

/// Plain text per line, since the value's own line breaks are Rust's and meaningful.
fn text_lines(value: &str) -> String {
    value
        .lines()
        .map(|line| text::plain(line, 800))
        .collect::<Vec<_>>()
        .join("\n")
}

/* ---------- Window and language ---------- */

#[tauri::command]
fn window_set_title(app: AppHandle, title: String) -> Reply<()> {
    let title = text::plain(&title, 300);
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_title(&title)
            .map_err(|e| CnError::new(error::FILE_WRITE, e.to_string()))?;
    }
    Ok(())
}

const MENU_IDS: [&str; 12] = [
    "new_page",
    "open_package",
    "save_package",
    "undo",
    "redo",
    "select_all",
    "toggle_sidebar",
    "assistant",
    "search",
    "settings",
    "shortcuts",
    "local_models",
];

fn build_menu(
    app: &AppHandle,
    labels: &HashMap<String, String>,
) -> tauri::Result<Menu<tauri::Wry>> {
    let label = |id: &str, fallback: &str| {
        labels
            .get(id)
            .map_or_else(|| fallback.to_owned(), |l| text::plain(l, 80))
    };
    let item = |id: &str, fallback: &str, accel: Option<&str>| {
        MenuItem::with_id(app, id, label(id, fallback), true, accel)
    };
    let menu = Menu::new(app)?;
    #[cfg(target_os = "macos")]
    menu.append(&Submenu::with_items(
        app,
        "CasualNote",
        true,
        &[
            &PredefinedMenuItem::about(app, None, Some(AboutMetadata::default()))?,
            &PredefinedMenuItem::separator(app)?,
            &item("settings", "Settings", None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?)?;
    #[cfg(not(target_os = "macos"))]
    let _ = AboutMetadata::default();
    menu.append(&Submenu::with_items(
        app,
        label("menu_file", "File"),
        true,
        &[
            &item("new_page", "New page", None)?,
            &PredefinedMenuItem::separator(app)?,
            &item("open_package", "Import a package", Some("CmdOrCtrl+O"))?,
            &item(
                "save_package",
                "Export workspace",
                Some("CmdOrCtrl+Shift+S"),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?)?;
    // Undo, Redo and Select all have no accelerator here on purpose: the key must reach the editor,
    // whose own undo is the truth (docs/44). The menu entries dispatch the same key.
    menu.append(&Submenu::with_items(
        app,
        label("menu_edit", "Edit"),
        true,
        &[
            &item("undo", "Undo", None)?,
            &item("redo", "Redo", None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &item("select_all", "Select all", None)?,
        ],
    )?)?;
    menu.append(&Submenu::with_items(
        app,
        label("menu_view", "View"),
        true,
        &[
            &item("toggle_sidebar", "Hide sidebar", None)?,
            &item("assistant", "Assistant", None)?,
            &item("search", "Search", None)?,
            &item("local_models", "Local models", None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?)?;
    menu.append(&Submenu::with_items(
        app,
        label("menu_window", "Window"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
        ],
    )?)?;
    menu.append(&Submenu::with_items(
        app,
        label("menu_help", "Help"),
        true,
        &[&item("shortcuts", "Keyboard shortcuts", None)?],
    )?)?;
    Ok(menu)
}

/// The labels come from the same catalogue as the rest of the interface, and the ids from a fixed
/// list. The language also decides the words of native confirmations.
#[tauri::command]
fn menu_set_labels(
    app: AppHandle,
    shell: State<'_, Shell>,
    lang: String,
    labels: HashMap<String, String>,
) -> Reply<()> {
    if let Ok(mut current) = shell.lang.lock() {
        *current = text::plain(&lang, 16);
    }
    let menu =
        build_menu(&app, &labels).map_err(|e| CnError::new(error::FILE_WRITE, e.to_string()))?;
    app.set_menu(menu)
        .map_err(|e| CnError::new(error::FILE_WRITE, e.to_string()))?;
    Ok(())
}

/* ---------- Notes on disk ---------- */

#[derive(Serialize)]
struct NotesInfo {
    path: String,
    exists: bool,
    bytes: u64,
}

fn raw_body(request: &Request<'_>, limit: usize) -> Reply<Vec<u8>> {
    match request.body() {
        InvokeBody::Raw(bytes) if bytes.len() <= limit => Ok(bytes.clone()),
        InvokeBody::Raw(_) => Err(CnError::new(
            error::FILE_WRITE,
            "That is larger than this app accepts.",
        )),
        InvokeBody::Json(_) => Err(CnError::new(error::FILE_WRITE, "Expected bytes.")),
    }
}

fn notes_info_now(shell: &Shell) -> NotesInfo {
    let path = shell.notes_path();
    let meta = std::fs::metadata(&path).ok();
    NotesInfo {
        path: path.display().to_string(),
        exists: meta.is_some(),
        bytes: meta.map_or(0, |m| m.len()),
    }
}

#[tauri::command]
fn notes_info(shell: State<'_, Shell>) -> NotesInfo {
    notes_info_now(&shell)
}

#[tauri::command]
async fn notes_load(shell: State<'_, Shell>) -> Reply<Response> {
    match std::fs::read(shell.notes_path()) {
        Ok(bytes) => Ok(Response::new(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Response::new(Vec::<u8>::new())),
        Err(e) => Err(CnError::new(
            error::FILE_WRITE,
            format!("The notes file could not be read: {}", e.kind()),
        )),
    }
}

/// Temp file, fsync, rename, fsync of the folder. Answers only after that, so nothing shows as
/// saved early.
#[tauri::command]
async fn notes_save(shell: State<'_, Shell>, request: Request<'_>) -> Reply<NotesInfo> {
    let bytes = raw_body(&request, MAX_NOTES)?;
    let path = shell.notes_path();
    tokio::task::spawn_blocking(move || atomic::write(&path, &bytes))
        .await
        .map_err(|e| CnError::new(error::FILE_WRITE, e.to_string()))?
        .map_err(|e| CnError::new(error::FILE_WRITE, format!("{}", e.kind())))?;
    Ok(notes_info_now(&shell))
}

/// Keeps an unreadable notes file under a new name, so a later save cannot overwrite it.
#[tauri::command]
fn notes_set_aside(shell: State<'_, Shell>) -> Reply<String> {
    let path = shell.notes_path();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let aside = path.with_extension(format!("unreadable-{stamp}"));
    std::fs::rename(&path, &aside)
        .map_err(|e| CnError::new(error::FILE_WRITE, format!("{}", e.kind())))?;
    Ok(aside.display().to_string())
}

/// A storage change: the native panel, then a native confirmation showing the real path.
#[tauri::command]
async fn notes_move(app: AppHandle, shell: State<'_, Shell>) -> Reply<Option<NotesInfo>> {
    let dialog = app.dialog().clone();
    let picked = tokio::task::spawn_blocking(move || {
        dialog
            .file()
            .set_file_name("notes.casualnote")
            .blocking_save_file()
    })
    .await
    .ok()
    .flatten();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    confirm(
        &app,
        &shell,
        "notes_move",
        Kind::NotesMove,
        &path.display().to_string(),
    )
    .await?;
    // Copied first, so the old file stays until the new one is safely there.
    let current = shell.notes_path();
    if current.exists() {
        let bytes = std::fs::read(&current)
            .map_err(|e| CnError::new(error::FILE_WRITE, format!("{}", e.kind())))?;
        atomic::write(&path, &bytes)
            .map_err(|e| CnError::new(error::FILE_WRITE, format!("{}", e.kind())))?;
    }
    shell.change_settings(|s| s.notes_path = Some(path))?;
    Ok(Some(notes_info_now(&shell)))
}

/* ---------- Files, through native dialogs ---------- */

#[tauri::command]
async fn package_open(app: AppHandle) -> Reply<Response> {
    let dialog = app.dialog().clone();
    let picked = tokio::task::spawn_blocking(move || {
        dialog
            .file()
            .add_filter("CasualNote package", &["cnotepack", "zip"])
            .blocking_pick_file()
    })
    .await
    .ok()
    .flatten();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(Response::new(Vec::<u8>::new()));
    };
    let meta = std::fs::symlink_metadata(&path)
        .map_err(|e| CnError::new("CN-DSK-0015", format!("{}", e.kind())))?;
    if !meta.is_file() || meta.len() > MAX_PACKAGE {
        return Err(CnError::new(
            "CN-DSK-0015",
            "That is not a regular file within the size limit.",
        ));
    }
    let bytes =
        std::fs::read(&path).map_err(|e| CnError::new("CN-DSK-0015", format!("{}", e.kind())))?;
    Ok(Response::new(bytes))
}

/// Any save the web app asks for. The person picks where. The webview only suggests a name.
#[tauri::command]
async fn file_save(app: AppHandle, request: Request<'_>) -> Reply<Option<String>> {
    let bytes = raw_body(&request, MAX_PACKAGE as usize)?;
    let suggested = request
        .headers()
        .get("x-file-name")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("CasualNote");
    let name = percent_decode(suggested).replace(['/', '\\', ':'], " ");
    let name = text::plain(&name, 200);
    let extension = name
        .rsplit('.')
        .next()
        .filter(|e| e.len() <= 12 && e.chars().all(char::is_alphanumeric))
        .unwrap_or("")
        .to_owned();
    let dialog = app.dialog().clone();
    let shown = name.clone();
    let picked = tokio::task::spawn_blocking(move || {
        let panel = dialog.file().set_file_name(&shown);
        let panel = if extension.is_empty() {
            panel
        } else {
            panel.add_filter(&extension, &[extension.as_str()])
        };
        panel.blocking_save_file()
    })
    .await
    .ok()
    .flatten();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    atomic::write(&path, &bytes)
        .map_err(|e| CnError::new(error::FILE_WRITE, format!("{}", e.kind())))?;
    Ok(path.file_name().map(|n| n.to_string_lossy().into_owned()))
}

/// Undoes the page's `encodeURIComponent`. Anything malformed stays as written.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let (mut out, mut i) = (Vec::with_capacity(bytes.len()), 0);
    while i < bytes.len() {
        let pair = (bytes[i] == b'%').then(|| text.get(i + 1..i + 3)).flatten();
        match pair.and_then(|p| u8::from_str_radix(p, 16).ok()) {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/* ---------- Catalogue and downloads ---------- */

#[derive(Serialize)]
struct ModelRow {
    id: String,
    name: String,
    position: String,
    licence: String,
    licence_url: String,
    bytes: u64,
    source: String,
    pinned: bool,
    sha256: Option<String>,
    installed: bool,
    part_bytes: u64,
    active: bool,
}

#[derive(Serialize)]
struct CatalogueView {
    models: Vec<ModelRow>,
    free_bytes: Option<u64>,
    margin_bytes: u64,
    running: Option<String>,
}

#[tauri::command]
fn catalogue_status(shell: State<'_, Shell>) -> CatalogueView {
    let dir = shell.models_dir();
    let models = shell
        .catalogue
        .iter()
        .map(|entry| {
            let job = entry.job(&dir).ok();
            ModelRow {
                id: entry.id.clone(),
                name: entry.name.clone(),
                position: entry.position.clone(),
                licence: entry.licence.clone(),
                licence_url: entry.licence_url.clone(),
                bytes: entry.bytes,
                source: entry.source_host(),
                pinned: job.is_some(),
                sha256: entry.pin().map(str::to_owned),
                installed: job.as_ref().is_some_and(|j| j.final_path().exists()),
                part_bytes: job.as_ref().map_or(0, download::Job::part_bytes),
                active: shell.downloads.active().as_deref() == Some(entry.id.as_str()),
            }
        })
        .collect();
    let running = shell.model_id.lock().ok().and_then(|m| m.clone());
    CatalogueView {
        models,
        free_bytes: download::free_space(&dir).ok(),
        margin_bytes: download::DISK_MARGIN,
        running,
    }
}

fn failed_report(id: &str, job: &download::Job, e: &CnError) -> Report {
    Report {
        id: id.to_owned(),
        phase: Phase::Failed,
        done: job.part_bytes(),
        total: job.bytes,
        rate: 0,
        restarted: false,
        code: Some(e.code),
        detail: Some(e.detail.clone()),
    }
}

/// Takes a catalogue id only: the source address is never the webview's (docs/44). Starts now and
/// reports through events. Also resumes, since a part file is what resuming continues from.
#[tauri::command]
fn download_start(app: AppHandle, shell: State<'_, Shell>, id: String) -> Reply<()> {
    let job = shell.job(&id)?;
    let control = shell.downloads.begin(&id)?;
    let events = app.clone();
    tauri::async_runtime::spawn(async move {
        let on = {
            let events = events.clone();
            move |report: Report| {
                let _ = events.emit("cn://download", report);
            }
        };
        let outcome = download::run(&job, &control, &download::free_space, &on).await;
        let shell = events.state::<Shell>();
        shell.downloads.finish();
        let last = match &outcome {
            Ok(Outcome::Installed(_)) => Report {
                id: id.clone(),
                phase: Phase::Installed,
                done: job.bytes,
                total: job.bytes,
                rate: 0,
                restarted: false,
                code: None,
                detail: None,
            },
            Ok(Outcome::Paused) => Report {
                id: id.clone(),
                phase: Phase::Paused,
                done: job.part_bytes(),
                total: job.bytes,
                rate: 0,
                restarted: false,
                code: None,
                detail: None,
            },
            Ok(Outcome::Cancelled) => Report {
                id: id.clone(),
                phase: Phase::Cancelled,
                done: 0,
                total: job.bytes,
                rate: 0,
                restarted: false,
                code: None,
                detail: None,
            },
            Err(e) => failed_report(&id, &job, e),
        };
        let _ = events.emit("cn://download", last);
    });
    Ok(())
}

fn control_of(shell: &Shell, id: &str) -> Option<Arc<Control>> {
    shell.downloads.control_of(id)
}

#[tauri::command]
fn download_pause(shell: State<'_, Shell>, id: String) {
    if let Some(control) = control_of(&shell, &id) {
        control.pause();
    }
}

/// Cancels a running download, or discards a paused one. Either way the part file goes.
#[tauri::command]
fn download_cancel(app: AppHandle, shell: State<'_, Shell>, id: String) -> Reply<()> {
    if let Some(control) = control_of(&shell, &id) {
        control.cancel();
        return Ok(());
    }
    let job = shell.job(&id)?;
    job.discard_part();
    let _ = app.emit(
        "cn://download",
        Report {
            id,
            phase: Phase::Cancelled,
            done: 0,
            total: job.bytes,
            rate: 0,
            restarted: false,
            code: None,
            detail: None,
        },
    );
    Ok(())
}

#[tauri::command]
async fn model_delete(shell: State<'_, Shell>, id: String) -> Reply<()> {
    if shell
        .model_id
        .lock()
        .ok()
        .and_then(|m| m.clone())
        .as_deref()
        == Some(id.as_str())
    {
        return Err(CnError::new(
            "CN-MDL-0006",
            "Stop the model before deleting it.",
        ));
    }
    let job = shell.job(&id)?;
    std::fs::remove_file(job.final_path())
        .map_err(|e| CnError::new(error::FILE_WRITE, format!("{}", e.kind())))
}

/// Air-gapped route: a catalogue model from disk, checked against its pin.
#[tauri::command]
async fn model_add_file(app: AppHandle, shell: State<'_, Shell>, id: String) -> Reply<bool> {
    let job = shell.job(&id)?;
    let dialog = app.dialog().clone();
    let picked = tokio::task::spawn_blocking(move || {
        dialog
            .file()
            .add_filter("GGUF model", &["gguf"])
            .blocking_pick_file()
    })
    .await
    .ok()
    .flatten();
    let Some(source) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(false);
    };
    let on_id = id.clone();
    let events = app.clone();
    let total = job.bytes;
    let target = job.clone();
    let from = source.clone();
    tokio::task::spawn_blocking(move || {
        download::add_from_file(&from, &target, |done| {
            let _ = events.emit(
                "cn://download",
                Report {
                    id: on_id.clone(),
                    phase: Phase::Verifying,
                    done,
                    total,
                    rate: 0,
                    restarted: false,
                    code: None,
                    detail: None,
                },
            );
        })
    })
    .await
    .map_err(|e| CnError::new("CN-MDL-0003", e.to_string()))??;
    if !download::is_gguf(&job.final_path()).unwrap_or(false) {
        let _ = std::fs::remove_file(job.final_path());
        return Err(CnError::new(
            "CN-MDL-0004",
            "That file is not a GGUF model. Nothing was added.",
        ));
    }
    let name = shell.entry(&id)?.name.clone();
    let _ = app.emit(
        "cn://download",
        Report {
            id,
            phase: Phase::Installed,
            done: total,
            total,
            rate: 0,
            restarted: false,
            code: None,
            detail: Some(name),
        },
    );
    Ok(true)
}

/* ---------- The local runtime ---------- */

#[derive(Serialize)]
struct RuntimeView {
    found: bool,
    path: Option<String>,
    source: Option<runtime::Source>,
    running: Option<String>,
    sandboxed: bool,
}

fn find_program(shell: &Shell) -> Option<(PathBuf, runtime::Source)> {
    let chosen = shell
        .settings
        .lock()
        .ok()
        .and_then(|s| s.runtime_path.clone());
    runtime::find(
        chosen.as_deref(),
        std::env::var_os("PATH"),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

#[tauri::command]
fn runtime_status(shell: State<'_, Shell>) -> RuntimeView {
    let found = find_program(&shell);
    RuntimeView {
        found: found.is_some(),
        path: found.as_ref().map(|(p, _)| p.display().to_string()),
        source: found.map(|(_, s)| s),
        running: shell.model_id.lock().ok().and_then(|m| m.clone()),
        sandboxed: false,
    }
}

/// A native panel to pick the program, then a native confirmation that says what it means.
#[tauri::command]
async fn runtime_choose(app: AppHandle, shell: State<'_, Shell>) -> Reply<RuntimeView> {
    let dialog = app.dialog().clone();
    let picked = tokio::task::spawn_blocking(move || dialog.file().blocking_pick_file())
        .await
        .ok()
        .flatten();
    if let Some(path) = picked.and_then(|p| p.into_path().ok()) {
        confirm(
            &app,
            &shell,
            "runtime_choose",
            Kind::ChooseProgram,
            &path.display().to_string(),
        )
        .await?;
        shell.change_settings(|s| s.runtime_path = Some(path))?;
    }
    Ok(runtime_status(shell))
}

#[derive(Clone, Serialize)]
struct RuntimeEvent {
    id: String,
    phase: &'static str,
    done: u64,
    total: u64,
    code: Option<&'static str>,
    detail: Option<String>,
}

fn emit_runtime(
    app: &AppHandle,
    id: &str,
    phase: &'static str,
    done: u64,
    total: u64,
    failure: Option<&CnError>,
) {
    let _ = app.emit(
        "cn://runtime",
        RuntimeEvent {
            id: id.to_owned(),
            phase,
            done,
            total,
            code: failure.map(|f| f.code),
            detail: failure.map(|f| f.detail.clone()),
        },
    );
}

async fn start_model(app: &AppHandle, shell: &Shell, id: &str) -> Reply<()> {
    let (program, _) = find_program(shell).ok_or_else(runtime::no_runtime)?;
    let job = shell.job(id)?;
    let model = job.final_path();
    if !model.exists() {
        return Err(CnError::new(
            "CN-MDL-0002",
            "This model is not on this computer yet. Download it first.",
        ));
    }
    // The whole file, whatever its size and date say, before each start.
    let (sha, total) = (job.sha256.clone(), job.bytes);
    let hashing = {
        let (path, events, on_id) = (model.clone(), app.clone(), id.to_owned());
        tokio::task::spawn_blocking(move || {
            let mut last = Instant::now();
            download::verify_before_start(
                &path,
                &sha,
                |done| {
                    if last.elapsed() >= Duration::from_millis(200) {
                        emit_runtime(&events, &on_id, "checking", done, total, None);
                        last = Instant::now();
                    }
                },
                || false,
            )
        })
    };
    hashing
        .await
        .map_err(|e| CnError::new("CN-MDL-0002", e.to_string()))??;
    let program_for_hash = program.clone();
    let program_hash = tokio::task::spawn_blocking(move || {
        download::hash_file(&program_for_hash, |_| {}, || false)
    })
    .await
    .map_err(|e| CnError::new("CN-MDL-0006", e.to_string()))?
    .map_err(|e| {
        CnError::new(
            "CN-MDL-0006",
            format!("{} could not be read: {}", program.display(), e.kind()),
        )
    })?
    .unwrap_or_default();
    let pair = (program_hash.clone(), job.sha256.clone());
    let known = shell
        .settings
        .lock()
        .map_err(|_| write_failed("settings"))?
        .confirmed_runs
        .contains(&pair);
    if !known {
        let name = &shell.entry(id)?.name;
        let value = format!(
            "{}\nSHA-256 {program_hash}\n\n{name}\nSHA-256 {}",
            program.display(),
            job.sha256
        );
        confirm(
            app,
            shell,
            &format!("run:{program_hash}:{}", job.sha256),
            Kind::Run,
            &value,
        )
        .await?;
        shell.change_settings(|s| s.confirmed_runs.push(pair))?;
    }
    emit_runtime(app, id, "starting", 0, 0, None);
    let running = runtime::launch(Launch::new(program.clone(), model)).await?;
    let _ = runtime::write_record(&shell.record_path(), &running, &program);
    *shell.model.lock().await = Some(running);
    if let Ok(mut current) = shell.model_id.lock() {
        *current = Some(id.to_owned());
    }
    Ok(())
}

/// Watches for an unexpected exit. It reports, and there is no restart: the person starts it again.
fn watch_model(app: AppHandle, id: String) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let shell = app.state::<Shell>();
            let mut slot = shell.model.lock().await;
            let Some(running) = slot.as_mut() else { return };
            if running.exited().is_some() {
                *slot = None;
                drop(slot);
                if let Ok(mut current) = shell.model_id.lock() {
                    *current = None;
                }
                runtime::clear_record(&shell.record_path());
                let code = shell.failures.lock().map_or("CN-MDL-0007", |mut f| {
                    f.record(Instant::now(), "CN-MDL-0007")
                });
                let failure = CnError::new(
                    code,
                    "The local model stopped unexpectedly. It does not restart by itself.",
                );
                emit_runtime(&app, &id, "crashed", 0, 0, Some(&failure));
                return;
            }
        }
    });
}

#[tauri::command]
fn runtime_start(app: AppHandle, shell: State<'_, Shell>, id: String) -> Reply<()> {
    if shell.starting.swap(true, Ordering::SeqCst) {
        return Err(CnError::new("CN-MDL-0006", "A model is already starting."));
    }
    shell
        .entry(&id)
        .inspect_err(|_| shell.starting.store(false, Ordering::SeqCst))?;
    emit_runtime(&app, &id, "checking", 0, 0, None);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let shell = handle.state::<Shell>();
        stop_model(&shell).await;
        let started = start_model(&handle, &shell, &id).await;
        shell.starting.store(false, Ordering::SeqCst);
        match started {
            Ok(()) => {
                emit_runtime(&handle, &id, "ready", 0, 0, None);
                watch_model(handle.clone(), id);
            }
            Err(e) => {
                // A refused request is not a failed server.
                let counted = !matches!(
                    e.code,
                    confirm::DECLINED | error::NO_RUNTIME | "CN-MDL-0002"
                );
                let code = if counted {
                    shell
                        .failures
                        .lock()
                        .map_or(e.code, |mut f| f.record(Instant::now(), e.code))
                } else {
                    e.code
                };
                emit_runtime(
                    &handle,
                    &id,
                    "failed",
                    0,
                    0,
                    Some(&CnError::new(code, e.detail)),
                );
            }
        }
    });
    Ok(())
}

async fn stop_model(shell: &Shell) {
    let taken = shell.model.lock().await.take();
    if let Ok(mut current) = shell.model_id.lock() {
        *current = None;
    }
    if let Some(mut running) = taken {
        let _ = tokio::task::spawn_blocking(move || running.stop()).await;
    }
    runtime::clear_record(&shell.record_path());
}

#[tauri::command]
async fn runtime_stop(app: AppHandle, shell: State<'_, Shell>) -> Reply<()> {
    let id = shell
        .model_id
        .lock()
        .ok()
        .and_then(|m| m.clone())
        .unwrap_or_default();
    stop_model(&shell).await;
    emit_runtime(&app, &id, "stopped", 0, 0, None);
    Ok(())
}

/* ---------- Other servers ---------- */

#[derive(Deserialize)]
struct OllamaTags {
    models: Vec<OllamaModel>,
}
#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

/// Only this click sends a request. We never start or configure Ollama.
#[tauri::command]
async fn ollama_detect(shell: State<'_, Shell>) -> Reply<Vec<String>> {
    let missing = || CnError::new("CN-MDL-0009", "Nothing answered at 127.0.0.1:11434.");
    let response = shell
        .loopback
        .get(format!("{OLLAMA}/api/tags"))
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .map_err(|_| missing())?;
    let body = response.bytes().await.map_err(|_| missing())?;
    let tags: OllamaTags = serde_json::from_slice(&body).map_err(|_| missing())?;
    Ok(tags
        .models
        .into_iter()
        .map(|m| text::plain(&m.name, 200))
        .collect())
}

/// An endpoint change: confirmed natively, with the origin Rust parsed.
#[tauri::command]
async fn provider_bind(app: AppHandle, shell: State<'_, Shell>, url: String) -> Reply<String> {
    let (origin, _) = proxy::origin_of(&url)?;
    let known = shell
        .settings
        .lock()
        .map_err(|_| write_failed("settings"))?
        .bindings
        .contains(&origin);
    if !known {
        confirm(&app, &shell, &origin, Kind::Bind, &origin).await?;
        let value = origin.clone();
        shell.change_settings(|s| s.bindings.push(value))?;
    }
    Ok(origin)
}

#[tauri::command]
fn provider_list(shell: State<'_, Shell>) -> Vec<String> {
    shell
        .settings
        .lock()
        .map(|s| s.bindings.clone())
        .unwrap_or_default()
}

#[tauri::command]
fn provider_unbind(shell: State<'_, Shell>, origin: String) -> Reply<()> {
    shell.change_settings(|s| s.bindings.retain(|b| *b != origin))
}

/* ---------- The assistant's transport ---------- */

#[derive(Deserialize)]
struct AiRequest {
    id: String,
    url: String,
    headers: Vec<(String, String)>,
    body: String,
}

fn head_json(status: u16, content_type: &str, retry_after: Option<&str>) -> String {
    serde_json::json!({ "status": status, "contentType": content_type, "retryAfter": retry_after })
        .to_string()
}

/// The webview names a URL and a body. For the local model, Rust supplies address and key after
/// checking the listener is still our child. Anything else must be an origin a person confirmed.
#[tauri::command]
async fn ai_request(
    app: AppHandle,
    shell: State<'_, Shell>,
    request: AiRequest,
    channel: Channel<Response>,
) -> Reply<()> {
    if request.body.len() > proxy::MAX_BODY {
        return Err(CnError::new(
            "CN-AI-0007",
            "That request is larger than this app sends.",
        ));
    }
    let bindings = shell
        .settings
        .lock()
        .map_err(|_| write_failed("settings"))?
        .bindings
        .clone();
    let (url, headers, client) = match proxy::classify(&request.url, &bindings)? {
        Route::Local => {
            let slot = shell.model.lock().await;
            let running = slot
                .as_ref()
                .ok_or_else(|| CnError::new("CN-MDL-0014", "The local model is not running."))?;
            if let Err(failure) = running.verify_identity() {
                drop(slot);
                stop_model(&shell).await;
                emit_runtime(&app, "", "failed", 0, 0, Some(&failure));
                return Err(failure);
            }
            let (url, headers) = proxy::local_request(&running.base_url(), running.key());
            (url, headers, shell.loopback.clone())
        }
        Route::Bound(url) => (
            url.to_string(),
            proxy::bound_headers(&request.headers),
            client_for(&shell, &url),
        ),
        Route::NeedsBinding(origin, url) => {
            confirm(&app, &shell, &origin, Kind::Bind, &origin).await?;
            shell.change_settings(|s| s.bindings.push(origin))?;
            let client = client_for(&shell, &url);
            (
                url.to_string(),
                proxy::bound_headers(&request.headers),
                client,
            )
        }
    };
    let cancel = Arc::new(Notify::new());
    if let Ok(mut live) = shell.ai.lock() {
        live.insert(request.id.clone(), cancel.clone());
    }
    let sent = proxy::forward(
        &client,
        &url,
        &headers,
        request.body,
        &cancel,
        |frame| match frame {
            Frame::Head {
                status,
                content_type,
                retry_after,
            } => {
                let _ = channel.send(Response::new(head_json(
                    status,
                    &content_type,
                    retry_after.as_deref(),
                )));
            }
            Frame::Chunk(bytes) => {
                let _ = channel.send(Response::new(bytes));
            }
        },
    )
    .await;
    if let Ok(mut live) = shell.ai.lock() {
        live.remove(&request.id);
    }
    sent?;
    let _ = channel.send(Response::new(r#"{"done":true}"#.to_owned()));
    Ok(())
}

fn client_for(shell: &Shell, url: &url::Url) -> reqwest::Client {
    if matches!(url.host_str(), Some("127.0.0.1") | Some("[::1]")) {
        shell.loopback.clone()
    } else {
        shell.hosted.clone()
    }
}

#[tauri::command]
fn ai_cancel(shell: State<'_, Shell>, id: String) {
    if let Some(cancel) = shell.ai.lock().ok().and_then(|live| live.get(&id).cloned()) {
        cancel.notify_one();
    }
}

/* ---------- Closing: notes first ---------- */

fn close_now(app: &AppHandle) {
    app.state::<Shell>().closing.store(true, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.close();
    }
}

fn ask_to_close(app: &AppHandle) {
    let _ = app.emit("cn://close", ());
    let later = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CLOSE_WAIT).await;
        close_now(&later);
    });
}

/// The webview has written its notes, or could not. A failed last save is said natively, since the
/// window is about to go. The browser store still has the same notes.
#[tauri::command]
async fn close_ready(app: AppHandle, saved: bool) -> Reply<()> {
    if !saved {
        // Async, because a blocking dialog on the main thread would never return.
        let dialog = app.dialog().clone();
        let _ = tokio::task::spawn_blocking(move || {
            dialog
                .message("The last change could not be written to your notes file. It is kept in the app's own storage, and is written to the file next time.")
                .title("CasualNote")
                .kind(MessageDialogKind::Warning)
                .blocking_show()
        })
        .await;
    }
    close_now(&app);
    Ok(())
}

/// Debug builds only: lets a test run read what the page reports. Does nothing in a release.
#[tauri::command]
fn debug_report(text: String) {
    if cfg!(debug_assertions)
        && let Some(path) = std::env::var_os("CN_E2E_OUT")
    {
        let _ = atomic::write(std::path::Path::new(&path), text.as_bytes());
    }
}

fn data_dir(app: &AppHandle) -> PathBuf {
    if cfg!(debug_assertions)
        && let Some(dir) = std::env::var_os("CN_DATA_DIR")
    {
        return dir.into();
    }
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from(".casualnote"))
}

fn allowed_navigation(url: &url::Url) -> bool {
    // tauri://localhost on macOS and Linux, http://tauri.localhost on Windows (unbuilt).
    matches!(
        (url.scheme(), url.host_str()),
        ("tauri", Some("localhost"))
            | ("http", Some("tauri.localhost"))
            | ("https", Some("tauri.localhost"))
    )
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            window_set_title,
            menu_set_labels,
            notes_info,
            notes_load,
            notes_save,
            notes_set_aside,
            notes_move,
            package_open,
            file_save,
            catalogue_status,
            download_start,
            download_pause,
            download_cancel,
            model_delete,
            model_add_file,
            runtime_status,
            runtime_choose,
            runtime_start,
            runtime_stop,
            ollama_detect,
            provider_bind,
            provider_list,
            provider_unbind,
            ai_request,
            ai_cancel,
            close_ready,
            debug_report,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let data = data_dir(&handle);
            std::fs::create_dir_all(&data)?;
            let settings = Settings::load(&data.join("settings.json"));
            let test_catalogue = if cfg!(debug_assertions) {
                std::env::var("CN_TEST_CATALOGUE").ok()
            } else {
                None
            };
            let list = match &test_catalogue {
                Some(path) => std::fs::read_to_string(path)?,
                None => catalogue::BUILT_IN.to_owned(),
            };
            let catalogue = catalogue::parse(&list).map_err(std::io::Error::other)?;
            let shell = Shell {
                data: data.clone(),
                settings: Mutex::new(settings),
                gate: Mutex::default(),
                lang: Mutex::new("en".into()),
                downloads: Manager::default(),
                catalogue,
                model: tokio::sync::Mutex::new(None),
                model_id: Mutex::new(None),
                starting: AtomicBool::new(false),
                failures: Mutex::default(),
                ai: Mutex::default(),
                hosted: download::build_client(false)?,
                loopback: download::build_client(true)?,
                closing: AtomicBool::new(false),
                test_sources: test_catalogue.is_some(),
            };
            // A leftover server from a crash, if it is still the same process.
            runtime::reap_orphan(&shell.record_path());
            app.manage(shell);
            let menu = build_menu(&handle, &HashMap::new())?;
            app.set_menu(menu)?;

            let url = if cfg!(debug_assertions) {
                std::env::var("CN_E2E_URL").unwrap_or_else(|_| "index.html".into())
            } else {
                "index.html".into()
            };
            let mut window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App(url.into()))
                .title("CasualNote")
                .inner_size(1280.0, 800.0)
                .min_inner_size(640.0, 400.0)
                // The editor needs HTML5 drag to move blocks.
                .disable_drag_drop_handler()
                .on_navigation(allowed_navigation);
            if cfg!(debug_assertions)
                && let Ok(script) = std::env::var("CN_E2E_EVAL")
            {
                window = window.on_page_load(move |view, payload| {
                    if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                        let _ = view.eval(&script);
                    }
                });
            }
            window.build()?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            let id = event.id().0.clone();
            if MENU_IDS.contains(&id.as_str()) {
                let _ = app.emit("cn://menu", id);
            }
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if !app.state::<Shell>().closing.load(Ordering::SeqCst) {
                    api.prevent_close();
                    ask_to_close(app);
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("the application")
        .run(|app, event| match event {
            // Cmd+Q comes here and not through the window, so it gets the same flush.
            RunEvent::ExitRequested { api, .. } => {
                let shell = app.state::<Shell>();
                if !shell.closing.load(Ordering::SeqCst) && app.get_webview_window("main").is_some()
                {
                    api.prevent_exit();
                    ask_to_close(app);
                }
            }
            // The child never outlives a normal exit. A crash of this app leaves the record that
            // the next launch checks and stops. That window is stated in Settings.
            RunEvent::Exit => {
                let shell = app.state::<Shell>();
                tauri::async_runtime::block_on(stop_model(&shell));
            }
            _ => {}
        });
}
