//! The download manager (docs/44, docs/62). Resumable by HTTP Range, with pause, cancel and a disk
//! check. The final file is hashed in full before it is put in place, whatever resume did, and a
//! file that fails is deleted. No saved hash state counts toward that check.
//!
//! Source addresses come only from the catalogue. A redirect is followed only to a host the
//! catalogue entry lists, over HTTPS.
use crate::{atomic, error, error::CnError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Notify;

/// Free space kept beyond what the file needs.
pub const DISK_MARGIN: u64 = 1 << 30;
const MAX_REDIRECTS: usize = 5;
const MAX_RETRIES: u32 = 3;
const IDLE: Duration = Duration::from_secs(60);
const REPORT_EVERY: Duration = Duration::from_millis(200);

#[derive(Clone, Debug)]
pub struct Job {
    pub id: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
    pub hosts: Vec<String>,
    pub dir: PathBuf,
    /// False outside tests and debug test runs: production sources are HTTPS only.
    pub allow_loopback_http: bool,
    pub backoff: Duration,
    pub margin: u64,
}

impl Job {
    pub fn new(
        id: &str,
        url: &str,
        sha256: &str,
        bytes: u64,
        hosts: Vec<String>,
        dir: PathBuf,
    ) -> Self {
        Self {
            id: id.into(),
            url: url.into(),
            sha256: sha256.into(),
            bytes,
            hosts,
            dir,
            allow_loopback_http: false,
            backoff: Duration::from_secs(2),
            margin: DISK_MARGIN,
        }
    }
    pub fn final_path(&self) -> PathBuf {
        self.dir.join(format!("{}.gguf", self.sha256))
    }
    pub fn part_path(&self) -> PathBuf {
        self.dir.join(format!("{}.gguf.part", self.sha256))
    }
    fn state_path(&self) -> PathBuf {
        self.dir.join(format!("{}.gguf.part.json", self.sha256))
    }
    pub fn part_bytes(&self) -> u64 {
        fs::metadata(self.part_path()).map_or(0, |m| m.len())
    }
    pub fn discard_part(&self) {
        let _ = fs::remove_file(self.part_path());
        let _ = fs::remove_file(self.state_path());
    }
}

/* ---------- Pause and cancel ---------- */

const RUN: u8 = 0;
const PAUSE: u8 = 1;
const CANCEL: u8 = 2;

#[derive(Debug, Default)]
pub struct Control {
    wish: AtomicU8,
    notify: Notify,
}

impl Control {
    pub fn pause(&self) {
        let _ = self
            .wish
            .compare_exchange(RUN, PAUSE, Ordering::SeqCst, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
    pub fn cancel(&self) {
        self.wish.store(CANCEL, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
    fn wish(&self) -> u8 {
        self.wish.load(Ordering::SeqCst)
    }
    fn cancelled(&self) -> bool {
        self.wish() == CANCEL
    }
    async fn stopped(&self) {
        loop {
            // Created before the check, so a wake between the two is not lost.
            let wake = self.notify.notified();
            if self.wish() != RUN {
                return;
            }
            wake.await;
        }
    }
}

/* ---------- What the interface is told ---------- */

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Connecting,
    Downloading,
    Retrying,
    Paused,
    Verifying,
    Installed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub id: String,
    pub phase: Phase,
    pub done: u64,
    pub total: u64,
    /// Bytes per second, over the recent past.
    pub rate: u64,
    /// True when the server would not continue, and the download started again from the beginning.
    pub restarted: bool,
    pub code: Option<&'static str>,
    pub detail: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Installed(PathBuf),
    Paused,
    Cancelled,
}

/* ---------- URLs ---------- */

fn host_allowed(host: &str, hosts: &[String]) -> bool {
    let host = host.to_ascii_lowercase();
    hosts
        .iter()
        .any(|allowed| match allowed.strip_prefix("*.") {
            Some(domain) => host
                .strip_suffix(domain)
                .is_some_and(|rest| rest.ends_with('.') && rest.len() > 1),
            None => *allowed == host,
        })
}

/// Every hop passes this: HTTPS, and a host the catalogue entry names.
pub fn check_source(raw: &str, job: &Job) -> Result<url::Url, CnError> {
    let refused = |why: String| CnError::new("CN-MDL-0003", why);
    let url =
        url::Url::parse(raw).map_err(|_| refused("The source address is not valid.".into()))?;
    let host = url.host_str().unwrap_or("").to_owned();
    let loopback = host == "127.0.0.1" || host == "[::1]";
    let scheme_ok =
        url.scheme() == "https" || (job.allow_loopback_http && url.scheme() == "http" && loopback);
    if !scheme_ok {
        return Err(refused(format!(
            "Refused a source that is not HTTPS: {host}"
        )));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(refused(
            "Refused a source address that carries a name or password.".into(),
        ));
    }
    if !host_allowed(&host, &job.hosts) {
        return Err(refused(format!(
            "Refused a source this app's catalogue does not list: {host}"
        )));
    }
    Ok(url)
}

/* ---------- Whole-file checks ---------- */

/// SHA-256 of a whole file, in hex. `stop` is asked between reads, and Ok(None) means it said stop.
pub fn hash_file(
    path: &Path,
    mut on: impl FnMut(u64),
    stop: impl Fn() -> bool,
) -> io::Result<Option<String>> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    let mut seen = 0;
    loop {
        if stop() {
            return Ok(None);
        }
        let n = file.read(&mut buffer)?;
        if n == 0 {
            return Ok(Some(crate::text::hex(&hasher.finalize())));
        }
        hasher.update(&buffer[..n]);
        seen += n as u64;
        on(seen);
    }
}

/// Before each start. A file that does not match is deleted, and nothing runs (CN-MDL-0002).
pub fn verify_before_start(
    path: &Path,
    sha256: &str,
    on: impl FnMut(u64),
    stop: impl Fn() -> bool,
) -> Result<bool, CnError> {
    let hash = hash_file(path, on, stop).map_err(|e| {
        CnError::new(
            "CN-MDL-0002",
            format!("The model file could not be read: {}", e.kind()),
        )
    })?;
    match hash {
        None => Ok(false),
        Some(hash) if hash == sha256 => Ok(true),
        Some(_) => {
            let _ = fs::remove_file(path);
            Err(CnError::new(
                "CN-MDL-0002",
                "The model file no longer matches its pinned SHA-256. It was deleted, and nothing ran.",
            ))
        }
    }
}

/// A bounded look at the header. Model files are data: this reads 8 bytes and runs nothing.
pub fn is_gguf(path: &Path) -> io::Result<bool> {
    let mut head = [0u8; 8];
    let mut file = File::open(path)?;
    if file.read_exact(&mut head).is_err() {
        return Ok(false);
    }
    let version = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
    Ok(&head[..4] == b"GGUF" && (1..=64).contains(&version))
}

/* ---------- The download ---------- */

#[derive(Debug, Default, Deserialize, Serialize)]
struct PartState {
    url: String,
    sha256: String,
    etag: Option<String>,
}

enum Step {
    Complete,
    /// The person paused or cancelled. The loop reads which.
    Stopped,
    /// What is on disk cannot be continued. Start from zero.
    Restart,
}

enum Trouble {
    Transient(String),
    Fatal(CnError),
}

fn io_trouble(what: &str, e: io::Error) -> Trouble {
    if e.kind() == io::ErrorKind::StorageFull {
        Trouble::Fatal(CnError::new(
            "CN-MDL-0001",
            "The disk filled up during the download. What arrived is kept. Free some space, then resume.",
        ))
    } else {
        Trouble::Fatal(CnError::new("CN-MDL-0003", format!("{what}: {}", e.kind())))
    }
}

fn content_range_start(value: &str) -> Option<(u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes ")?;
    let (range, total) = rest.split_once('/')?;
    let start = range.split_once('-')?.0.parse().ok()?;
    Some((start, total.parse().ok()))
}

struct Run<'a> {
    job: &'a Job,
    control: &'a Control,
    on: &'a (dyn Fn(Report) + Send + Sync),
    client: reqwest::Client,
    restarted: bool,
}

impl Run<'_> {
    fn report(&self, phase: Phase, done: u64, rate: u64) {
        (self.on)(Report {
            id: self.job.id.clone(),
            phase,
            done,
            total: self.job.bytes,
            rate,
            restarted: self.restarted,
            code: None,
            detail: None,
        });
    }

    fn saved_etag(&self) -> Option<String> {
        let saved: PartState =
            serde_json::from_slice(&fs::read(self.job.state_path()).ok()?).ok()?;
        (saved.url == self.job.url && saved.sha256 == self.job.sha256)
            .then_some(saved.etag)
            .flatten()
    }

    /// Follows redirects by hand, so each hop is checked before anything is sent to it.
    async fn request(&self, have: u64, etag: Option<&str>) -> Result<reqwest::Response, Trouble> {
        let mut next = self.job.url.clone();
        for _ in 0..=MAX_REDIRECTS {
            let url = check_source(&next, self.job).map_err(Trouble::Fatal)?;
            let mut request = self.client.get(url.clone());
            if have > 0 {
                request = request.header("range", format!("bytes={have}-"));
                if let Some(tag) = etag {
                    request = request.header("if-range", tag);
                }
            }
            let response = request
                .send()
                .await
                .map_err(|e| Trouble::Transient(describe(&e)))?;
            if !response.status().is_redirection() {
                return Ok(response);
            }
            let location = response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            next = url.join(location).map(|u| u.to_string()).map_err(|_| {
                Trouble::Fatal(CnError::new(
                    "CN-MDL-0003",
                    "The server sent a redirect that is not a valid address.",
                ))
            })?;
        }
        Err(Trouble::Fatal(CnError::new(
            "CN-MDL-0003",
            "The server sent too many redirects.",
        )))
    }

    async fn attempt(&mut self, have: u64) -> Result<Step, Trouble> {
        let job = self.job;
        let etag = if have > 0 { self.saved_etag() } else { None };
        if have > 0 && etag.is_none() {
            // Without a validator nothing proves these bytes still belong to the same file.
            return Ok(Step::Restart);
        }
        self.report(Phase::Connecting, have, 0);
        let response = tokio::select! {
            r = self.request(have, etag.as_deref()) => r?,
            () = self.control.stopped() => return Ok(Step::Stopped),
        };
        let status = response.status().as_u16();
        let mut done = have;
        match status {
            206 if have > 0 => {
                let range = response
                    .headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .and_then(content_range_start);
                match range {
                    Some((start, total))
                        if start == have && total.is_none_or(|t| t == job.bytes) => {}
                    _ => return Ok(Step::Restart),
                }
            }
            200 => {
                if have > 0 {
                    return self.body_from_zero(response).await;
                }
                self.check_length(&response)?;
            }
            416 => return Ok(Step::Restart),
            408 | 425 | 429 | 500..=599 => {
                return Err(Trouble::Transient(format!(
                    "The server answered HTTP {status}."
                )));
            }
            _ => {
                return Err(Trouble::Fatal(CnError::new(
                    "CN-MDL-0003",
                    format!("The server refused this file: HTTP {status}."),
                )));
            }
        }
        if have == 0 {
            self.write_state(&response)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(job.part_path())
            .map_err(|e| io_trouble("The download file could not be opened", e))?;
        self.pump(response, &mut file, &mut done).await
    }

    /// The server ignored the range. Say so, and begin again from zero.
    async fn body_from_zero(&mut self, response: reqwest::Response) -> Result<Step, Trouble> {
        self.restarted = true;
        self.job.discard_part();
        self.check_length(&response)?;
        self.write_state(&response)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.job.part_path())
            .map_err(|e| io_trouble("The download file could not be opened", e))?;
        let mut done = 0;
        self.pump(response, &mut file, &mut done).await
    }

    fn check_length(&self, response: &reqwest::Response) -> Result<(), Trouble> {
        match response.content_length() {
            Some(n) if n != self.job.bytes => Err(Trouble::Fatal(CnError::new(
                "CN-MDL-0003",
                format!(
                    "The server offered {n} bytes, and this app's catalogue pins {}. Nothing was kept.",
                    self.job.bytes
                ),
            ))),
            _ => Ok(()),
        }
    }

    fn write_state(&self, response: &reqwest::Response) -> Result<(), Trouble> {
        // A weak validator cannot go in If-Range, so it is the same as none.
        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .filter(|t| !t.starts_with("W/"))
            .map(str::to_owned);
        let state = PartState {
            url: self.job.url.clone(),
            sha256: self.job.sha256.clone(),
            etag,
        };
        let bytes = serde_json::to_vec(&state).unwrap_or_default();
        fs::create_dir_all(&self.job.dir)
            .and_then(|()| atomic::write(&self.job.state_path(), &bytes))
            .map_err(|e| io_trouble("The download state could not be saved", e))
    }

    async fn pump(
        &mut self,
        mut response: reqwest::Response,
        file: &mut File,
        done: &mut u64,
    ) -> Result<Step, Trouble> {
        let job = self.job;
        let (mut last_report, mut mark_at, mut mark_done, mut rate) =
            (Instant::now(), Instant::now(), *done, 0u64);
        self.report(Phase::Downloading, *done, 0);
        loop {
            let chunk = tokio::select! {
                c = tokio::time::timeout(IDLE, response.chunk()) => c,
                () = self.control.stopped() => { let _ = file.sync_all(); return Ok(Step::Stopped); }
            };
            let chunk = match chunk {
                Err(_) => return Err(Trouble::Transient("The server stopped sending.".into())),
                Ok(Err(e)) => return Err(Trouble::Transient(describe(&e))),
                Ok(Ok(None)) => break,
                Ok(Ok(Some(chunk))) => chunk,
            };
            if *done + chunk.len() as u64 > job.bytes {
                job.discard_part();
                return Err(Trouble::Fatal(CnError::new(
                    "CN-MDL-0003",
                    "The server sent more than this app's catalogue pins. Nothing was kept.",
                )));
            }
            file.write_all(&chunk)
                .map_err(|e| io_trouble("The download could not be written", e))?;
            *done += chunk.len() as u64;
            if last_report.elapsed() >= REPORT_EVERY {
                let window = mark_at.elapsed().as_secs_f64();
                if window >= 1.0 {
                    rate = ((*done - mark_done) as f64 / window) as u64;
                    (mark_at, mark_done) = (Instant::now(), *done);
                }
                self.report(Phase::Downloading, *done, rate);
                last_report = Instant::now();
            }
        }
        file.sync_all()
            .map_err(|e| io_trouble("The download could not be written", e))?;
        if *done == job.bytes {
            Ok(Step::Complete)
        } else {
            Err(Trouble::Transient("The connection ended early.".into()))
        }
    }

    async fn verify_and_install(&self) -> Result<Outcome, CnError> {
        let job = self.job;
        self.report(Phase::Verifying, 0, 0);
        let hash = {
            let path = job.part_path();
            let on = self.on;
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<u64>();
            let cancelled = CancelProbe::new();
            let hashing = {
                let cancelled = cancelled.clone();
                tokio::task::spawn_blocking(move || {
                    let mut last = Instant::now();
                    hash_file(
                        &path,
                        |seen| {
                            if last.elapsed() >= REPORT_EVERY {
                                let _ = tx.send(seen);
                                last = Instant::now();
                            }
                        },
                        || cancelled.is_set(),
                    )
                })
            };
            tokio::pin!(hashing);
            loop {
                tokio::select! {
                    joined = &mut hashing => break joined.map_err(|e| CnError::new("CN-MDL-0002", e.to_string()))?,
                    Some(seen) = rx.recv() => on(Report { id: job.id.clone(), phase: Phase::Verifying, done: seen, total: job.bytes, rate: 0, restarted: self.restarted, code: None, detail: None }),
                    () = wait_for_cancel(self.control), if !cancelled.is_set() => cancelled.set(),
                }
            }
        };
        let hash = hash.map_err(|e| {
            CnError::new(
                "CN-MDL-0002",
                format!("The downloaded file could not be read: {}", e.kind()),
            )
        })?;
        let Some(hash) = hash else {
            job.discard_part();
            return Ok(Outcome::Cancelled);
        };
        if hash != job.sha256 {
            job.discard_part();
            return Err(CnError::new(
                "CN-MDL-0002",
                "The finished file does not match its pinned SHA-256. It was deleted, and nothing was installed. Download it again.",
            ));
        }
        if !is_gguf(&job.part_path()).unwrap_or(false) {
            job.discard_part();
            return Err(CnError::new(
                "CN-MDL-0004",
                "The file matches its hash but is not a GGUF model. It was deleted.",
            ));
        }
        let target = job.final_path();
        fs::rename(job.part_path(), &target).map_err(|e| {
            CnError::new(
                "CN-MDL-0003",
                format!("The file could not be put in place: {}", e.kind()),
            )
        })?;
        let _ = atomic::sync_dir(&job.dir);
        let _ = fs::remove_file(job.state_path());
        Ok(Outcome::Installed(target))
    }
}

async fn wait_for_cancel(control: &Control) {
    loop {
        let wake = control.notify.notified();
        if control.cancelled() {
            return;
        }
        wake.await;
    }
}

#[derive(Clone, Debug, Default)]
struct CancelProbe(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl CancelProbe {
    fn new() -> Self {
        Self::default()
    }
    fn set(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Never includes the URL's query or anything else a server sent as text.
fn describe(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "The connection timed out.".into()
    } else if e.is_connect() {
        "Could not connect to the server.".into()
    } else {
        "The connection failed.".into()
    }
}

pub type FreeSpace = dyn Fn(&Path) -> io::Result<u64> + Send + Sync;

pub fn free_space(path: &Path) -> io::Result<u64> {
    let mut at = path;
    while !at.exists() {
        at = at.parent().unwrap_or(Path::new("/"));
    }
    fs4::available_space(at)
}

pub fn build_client(loopback_only: bool) -> reqwest::Result<reqwest::Client> {
    let builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(30));
    if loopback_only {
        builder.no_proxy().build()
    } else {
        builder.build()
    }
}

/// Runs one download until it installs, pauses, cancels or fails.
pub async fn run(
    job: &Job,
    control: &Control,
    free: &FreeSpace,
    on: &(dyn Fn(Report) + Send + Sync),
) -> Result<Outcome, CnError> {
    fs::create_dir_all(&job.dir).map_err(|e| {
        CnError::new(
            "CN-MDL-0003",
            format!("The models folder could not be made: {}", e.kind()),
        )
    })?;
    check_source(&job.url, job)?;
    let client = build_client(job.allow_loopback_http)
        .map_err(|_| CnError::new("CN-MDL-0003", "The network could not be set up."))?;
    let mut run = Run {
        job,
        control,
        on,
        client,
        restarted: false,
    };
    let mut failures = 0;
    loop {
        match control.wish() {
            CANCEL => {
                job.discard_part();
                return Ok(Outcome::Cancelled);
            }
            PAUSE => {
                run.report(Phase::Paused, job.part_bytes(), 0);
                return Ok(Outcome::Paused);
            }
            _ => {}
        }
        let mut have = job.part_bytes();
        if have > job.bytes {
            job.discard_part();
            have = 0;
        }
        if have == job.bytes {
            break;
        }
        let needed = job.bytes - have + job.margin;
        let available = free(&job.dir).unwrap_or(u64::MAX);
        if available < needed {
            return Err(CnError::new(
                "CN-MDL-0001",
                format!("needs={needed} free={available}"),
            ));
        }
        let before = have;
        match run.attempt(have).await {
            Ok(Step::Complete) => break,
            Ok(Step::Stopped) => {}
            Ok(Step::Restart) => {
                run.restarted = true;
                job.discard_part();
            }
            Err(Trouble::Fatal(e)) => return Err(e),
            Err(Trouble::Transient(why)) => {
                failures = if job.part_bytes() > before {
                    0
                } else {
                    failures + 1
                };
                if failures > MAX_RETRIES {
                    return Err(CnError::new(
                        "CN-MDL-0003",
                        format!(
                            "{why} What arrived is kept, so trying again continues from there."
                        ),
                    ));
                }
                run.report(Phase::Retrying, job.part_bytes(), 0);
                tokio::select! {
                    () = tokio::time::sleep(job.backoff * 2u32.pow(failures.min(4))) => {}
                    () = control.stopped() => {}
                }
            }
        }
    }
    run.verify_and_install().await
}

/* ---------- One large file at a time ---------- */

#[derive(Debug, Default)]
pub struct Manager {
    current: Mutex<Option<(String, std::sync::Arc<Control>)>>,
}

impl Manager {
    /// Claims the single slot. A second download is refused, and says which one holds it.
    pub fn begin(&self, id: &str) -> Result<std::sync::Arc<Control>, CnError> {
        let mut slot = self
            .current
            .lock()
            .map_err(|_| CnError::new(error::BUSY, "poisoned"))?;
        if let Some((other, _)) = slot.as_ref() {
            return Err(CnError::new(error::BUSY, other.clone()));
        }
        let control = std::sync::Arc::new(Control::default());
        *slot = Some((id.to_owned(), control.clone()));
        Ok(control)
    }
    pub fn finish(&self) {
        if let Ok(mut slot) = self.current.lock() {
            *slot = None;
        }
    }
    pub fn control_of(&self, id: &str) -> Option<std::sync::Arc<Control>> {
        self.current
            .lock()
            .ok()?
            .as_ref()
            .filter(|(other, _)| other == id)
            .map(|(_, c)| c.clone())
    }
    pub fn active(&self) -> Option<String> {
        self.current.lock().ok()?.as_ref().map(|(id, _)| id.clone())
    }
}

/// Adds a catalogue model from disk, for a machine with no network. Hash first, then an atomic copy.
pub fn add_from_file(source: &Path, job: &Job, on: impl FnMut(u64)) -> Result<PathBuf, CnError> {
    let hash = hash_file(source, on, || false).map_err(|e| {
        CnError::new(
            "CN-MDL-0002",
            format!("That file could not be read: {}", e.kind()),
        )
    })?;
    if hash.as_deref() != Some(job.sha256.as_str()) {
        return Err(CnError::new(
            "CN-MDL-0002",
            "That file does not match this model's pinned SHA-256. Nothing was added.",
        ));
    }
    let target = job.final_path();
    atomic::write_with(&target, |out| {
        let mut input = File::open(source)?;
        input.seek(SeekFrom::Start(0))?;
        io::copy(&mut input, out).map(|_| ())
    })
    .map_err(|e| {
        CnError::new(
            "CN-MDL-0003",
            format!("The file could not be copied: {}", e.kind()),
        )
    })?;
    Ok(target)
}
