//! Starts and stops a `llama-server` the person already has. This shell never fetches one.
//!
//! Bind, then verify, then send (docs/44, SEC-L06). `llama-server` has no nonce route, so identity
//! comes from the operating system: the process listening on the port must be the child this shell
//! spawned, and it must listen on 127.0.0.1 only. That is checked before any request reaches the
//! port, and again before the key is sent and before each chat request. If the check cannot be
//! made, nothing is sent.
//!
//! **Not sandboxed.** The child runs with this person's rights (CN-DSK-0019). The key is passed
//! with `--api-key`, so another process of the same user can read it from the process list
//! (SEC-L02). These flags are from docs/62, and no real `llama-server` has run against this code.
use crate::{error, error::CnError, text};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// No website can have this origin, so no website gets a CORS pass.
pub const CORS_ORIGIN: &str = "http://casualnote-local.invalid";
const PROBE_ORIGIN: &str = "https://casualnote-probe.invalid";
const LOG_TAIL: usize = 16 * 1024;
const STOP_WAIT: Duration = Duration::from_secs(5);
pub const RESTART_WINDOW: Duration = Duration::from_secs(600);

/* ---------- Finding a program ---------- */

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Chosen,
    Path,
}

fn program_name() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

fn is_program(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    meta.is_file()
}

/// A person's choice first, then PATH. An app opened from Finder gets a short PATH, so the usual
/// install folders are searched too. Not built or run on Windows or Linux yet.
pub fn find(
    chosen: Option<&Path>,
    path_var: Option<OsString>,
    home: Option<PathBuf>,
) -> Option<(PathBuf, Source)> {
    if let Some(path) = chosen.filter(|p| is_program(p)) {
        return Some((path.to_path_buf(), Source::Chosen));
    }
    let mut dirs: Vec<PathBuf> = path_var
        .as_ref()
        .map(|v| std::env::split_paths(v).collect())
        .unwrap_or_default();
    if cfg!(unix) {
        dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    }
    if let Some(home) = home {
        dirs.push(home.join(".local/bin"));
    }
    dirs.into_iter()
        .map(|d| d.join(program_name()))
        .find(|p| is_program(p))
        .map(|p| (p, Source::Path))
}

/* ---------- Who is listening ---------- */

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listener {
    pub pid: u32,
    pub address: String,
}

/// `lsof -nP -iTCP:PORT -sTCP:LISTEN -Fpn`: lines `p1234` then `n127.0.0.1:8080`.
pub fn parse_lsof(output: &str) -> Vec<Listener> {
    let (mut pid, mut found) = (None, Vec::new());
    for line in output.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.parse().ok();
        } else if let (Some(n), Some(pid)) = (line.strip_prefix('n'), pid) {
            found.push(Listener {
                pid,
                address: n.to_owned(),
            });
        }
    }
    found
}

/// `ss -ltnpH "sport = :PORT"`. Linux route, not built or run yet.
pub fn parse_ss(output: &str) -> Vec<Listener> {
    output
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            let address = (*fields.get(3)?).to_owned();
            let pid = line
                .split("pid=")
                .nth(1)?
                .split([',', ')'])
                .next()?
                .parse()
                .ok()?;
            Some(Listener { pid, address })
        })
        .collect()
}

/// `netstat -ano -p TCP`. Windows route, not built or run yet.
pub fn parse_netstat(output: &str, port: u16) -> Vec<Listener> {
    output
        .lines()
        .filter_map(|line| {
            let f: Vec<_> = line.split_whitespace().collect();
            let (&address, &state, &pid) = (f.get(1)?, f.get(3)?, f.get(4)?);
            (state == "LISTENING" && address.ends_with(&format!(":{port}"))).then(|| {
                Some(Listener {
                    pid: pid.parse().ok()?,
                    address: address.to_owned(),
                })
            })?
        })
        .collect()
}

/// Asks the operating system, through its own tools at fixed paths. An error means "could not
/// check", which the caller treats as a failed check.
pub fn listeners(port: u16) -> Result<Vec<Listener>, String> {
    let run = |program: &str, args: &[&str]| {
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    if cfg!(windows) {
        return run("netstat", &["-ano", "-p", "TCP"])
            .map(|o| parse_netstat(&o, port))
            .map_err(|e| e.to_string());
    }
    for lsof in ["/usr/sbin/lsof", "/usr/bin/lsof"] {
        if Path::new(lsof).exists() {
            return run(
                lsof,
                &["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fpn"],
            )
            .map(|o| parse_lsof(&o))
            .map_err(|e| e.to_string());
        }
    }
    for ss in ["/usr/bin/ss", "/bin/ss", "/usr/sbin/ss"] {
        if Path::new(ss).exists() {
            return run(ss, &["-ltnpH", &format!("sport = :{port}")])
                .map(|o| parse_ss(&o))
                .map_err(|e| e.to_string());
        }
    }
    Err("this computer has no lsof or ss".into())
}

pub type ListenerCheck = fn(u16) -> Result<Vec<Listener>, String>;

#[derive(Debug, PartialEq, Eq)]
pub enum Holder {
    Nobody,
    OurChild,
}

/// Err when anything but our child holds the port, or it listens beyond this computer.
pub fn judge(found: &[Listener], child: u32) -> Result<Holder, CnError> {
    if found.is_empty() {
        return Ok(Holder::Nobody);
    }
    if found.iter().any(|l| l.pid != child) {
        return Err(CnError::new(
            "CN-MDL-0010",
            "Another program answered on the model's port. Nothing was sent. Press Start to try again on a new port.",
        ));
    }
    if found
        .iter()
        .any(|l| l.address != format!("127.0.0.1:{}", port_of(&l.address)))
    {
        return Err(CnError::new(
            "CN-MDL-0008",
            "The server listens beyond this computer, so it was stopped. Nothing was sent.",
        ));
    }
    Ok(Holder::OurChild)
}
fn port_of(address: &str) -> &str {
    address.rsplit(':').next().unwrap_or("")
}

/* ---------- The child ---------- */

fn random_key() -> Result<String, CnError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| CnError::new("CN-MDL-0006", "The operating system gave no random bytes."))?;
    Ok(text::hex(&bytes))
}

fn pick_port() -> Result<u16, CnError> {
    let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| {
        CnError::new(
            "CN-MDL-0006",
            format!("No free port on this computer: {}", e.kind()),
        )
    })?;
    probe
        .local_addr()
        .map(|a| a.port())
        .map_err(|e| CnError::new("CN-MDL-0006", e.to_string()))
}

pub fn arguments(model: &Path, port: u16, key: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    for (flag, value) in [
        ("--host", "127.0.0.1"),
        ("--port", &port.to_string()),
        ("--api-key", key),
        ("--cors-origins", CORS_ORIGIN),
    ] {
        args.push(flag.into());
        args.push(value.into());
    }
    args.push("--model".into());
    args.push(model.into());
    // Tool calling needs the model's own chat template.
    args.push("--jinja".into());
    args
}

type Tail = Arc<Mutex<VecDeque<u8>>>;

/// Bounded and in memory, and read only to explain a failed start. Output is thrown away once the
/// server is ready, so a prompt can never be kept or logged here.
fn drain(mut from: impl Read + Send + 'static, tail: Tail) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        while let Ok(n) = from.read(&mut buffer) {
            if n == 0 {
                return;
            }
            if let Ok(mut tail) = tail.lock() {
                if tail.capacity() == 0 {
                    continue;
                }
                tail.extend(&buffer[..n]);
                while tail.len() > LOG_TAIL {
                    tail.pop_front();
                }
            }
        }
    });
}

pub struct Running {
    child: Child,
    pub port: u16,
    pub pid: u32,
    key: String,
    tail: Tail,
    check: ListenerCheck,
}

impl std::fmt::Debug for Running {
    // Never prints the key.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running")
            .field("port", &self.port)
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

impl Running {
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    /// Err unless our child holds the port now. Called before anything that carries the key.
    pub fn verify_identity(&self) -> Result<(), CnError> {
        let found = (self.check)(self.port).map_err(|why| {
            CnError::new(
                "CN-MDL-0010",
                format!("Could not check who holds the model's port, so nothing was sent: {why}"),
            )
        })?;
        match judge(&found, self.pid)? {
            Holder::OurChild => Ok(()),
            Holder::Nobody => Err(CnError::new(
                "CN-MDL-0010",
                "Nothing is listening on the model's port now. Nothing was sent.",
            )),
        }
    }
    pub fn exited(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }
    fn output_tail(&self) -> String {
        let tail = self
            .tail
            .lock()
            .map(|t| t.iter().copied().collect::<Vec<u8>>())
            .unwrap_or_default();
        text::plain(&String::from_utf8_lossy(&tail).replace('\n', " "), 2000)
            .trim()
            .chars()
            .rev()
            .take(600)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
    fn forget_output(&self) {
        if let Ok(mut tail) = self.tail.lock() {
            *tail = VecDeque::new();
        }
    }
    /// SIGTERM, then a kill after 5 s. Safe to call on a child that has already gone.
    pub fn stop(&mut self) {
        if self.exited().is_some() {
            return;
        }
        terminate(self.pid);
        let deadline = Instant::now() + STOP_WAIT;
        while Instant::now() < deadline {
            if self.exited().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Running {
    /// However this value goes away, the child goes too.
    fn drop(&mut self) {
        if self.exited().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(unix)]
fn terminate(pid: u32) {
    if let Some(pid) = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
    }
}
#[cfg(not(unix))]
fn terminate(_pid: u32) {}

#[derive(Debug)]
pub struct Launch {
    pub program: PathBuf,
    pub model: PathBuf,
    /// Tests name a port. Otherwise the operating system picks a free one.
    pub port: Option<u16>,
    pub startup_timeout: Duration,
    pub poll: Duration,
    pub check: ListenerCheck,
}

impl Launch {
    pub fn new(program: PathBuf, model: PathBuf) -> Self {
        Self {
            program,
            model,
            port: None,
            startup_timeout: Duration::from_secs(180),
            poll: Duration::from_millis(250),
            check: listeners,
        }
    }
}

fn start_failed(running: &Running, status: Option<ExitStatus>) -> CnError {
    let output = running.output_tail();
    let what = status.map_or("did not become ready in time".to_owned(), |s| {
        format!("stopped during start ({s})")
    });
    let hint = if output.contains("cors-origins") {
        " This llama-server does not accept --cors-origins, which this app requires. Install a newer one."
    } else {
        ""
    };
    CnError::new(
        "CN-MDL-0006",
        format!("llama-server {what}.{hint} {output}")
            .trim()
            .to_owned(),
    )
}

/// Spawns, verifies, and returns a server that passed every check. On any Err the child is gone.
pub async fn launch(options: Launch) -> Result<Running, CnError> {
    let port = match options.port {
        Some(port) => port,
        None => pick_port()?,
    };
    let key = random_key()?;
    let mut command = Command::new(&options.program);
    command.args(arguments(&options.model, port, &key));
    // Nothing of this app's environment passes through, such as provider keys.
    command.env_clear().env("PATH", "/usr/bin:/bin");
    for name in ["HOME", "TMPDIR", "SystemRoot", "TEMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    if let Some(dir) = options.model.parent() {
        command.current_dir(dir);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| {
        CnError::new(
            "CN-MDL-0006",
            format!(
                "{} could not be started: {}",
                options.program.display(),
                e.kind()
            ),
        )
    })?;
    let tail: Tail = Arc::new(Mutex::new(VecDeque::with_capacity(LOG_TAIL)));
    if let Some(out) = child.stdout.take() {
        drain(out, tail.clone());
    }
    if let Some(err) = child.stderr.take() {
        drain(err, tail.clone());
    }
    let pid = child.id();
    let mut running = Running {
        child,
        port,
        pid,
        key,
        tail,
        check: options.check,
    };
    let deadline = Instant::now() + options.startup_timeout;
    let http = crate::download::build_client(true)
        .map_err(|_| CnError::new("CN-MDL-0006", "The network could not be set up."))?;

    // First the operating system says who holds the port. No request before that.
    loop {
        if let Some(status) = running.exited() {
            return Err(start_failed(&running, Some(status)));
        }
        let found = (options.check)(port).map_err(|why| {
            CnError::new(
                "CN-MDL-0010",
                format!("Could not check who holds the model's port, so nothing was sent: {why}"),
            )
        })?;
        if judge(&found, pid)? == Holder::OurChild {
            break;
        }
        if Instant::now() >= deadline {
            return Err(start_failed(&running, None));
        }
        tokio::time::sleep(options.poll).await;
    }

    // Loading a large model answers 503 for a while.
    let base = running.base_url();
    loop {
        if let Some(status) = running.exited() {
            return Err(start_failed(&running, Some(status)));
        }
        running.verify_identity()?;
        let health = http
            .get(format!("{base}/health"))
            .timeout(Duration::from_secs(5))
            .send()
            .await;
        if health.is_ok_and(|r| r.status().is_success()) {
            break;
        }
        if Instant::now() >= deadline {
            return Err(start_failed(&running, None));
        }
        tokio::time::sleep(options.poll).await;
    }

    probe(&http, &running).await?;
    running.forget_output();
    Ok(running)
}

/// The port test from docs/62: no key and a wrong key refused, a foreign origin not allowed, and
/// only then, with identity checked once more, the real key.
async fn probe(http: &reqwest::Client, running: &Running) -> Result<(), CnError> {
    let chat = format!("{}/v1/chat/completions", running.base_url());
    let leaky = |why: &str| {
        CnError::new(
            "CN-MDL-0008",
            format!("The server {why}, so it was stopped. Local assistant is off."),
        )
    };
    let timeout = Duration::from_secs(10);
    let empty = || {
        http.post(&chat)
            .header("content-type", "application/json")
            .body("{}")
            .timeout(timeout)
    };
    let status = |r: Result<reqwest::Response, reqwest::Error>| {
        r.map(|r| r.status().as_u16()).map_err(|_| {
            CnError::new(
                "CN-MDL-0006",
                "The server stopped answering during its checks.",
            )
        })
    };

    if !matches!(status(empty().send().await)?, 401 | 403) {
        return Err(leaky("accepted a request with no key"));
    }
    let wrong = random_key()?;
    if !matches!(status(empty().bearer_auth(wrong).send().await)?, 401 | 403) {
        return Err(leaky("accepted a request with a wrong key"));
    }
    let cross = http
        .get(format!("{}/health", running.base_url()))
        .header("origin", PROBE_ORIGIN)
        .timeout(timeout)
        .send()
        .await
        .map_err(|_| {
            CnError::new(
                "CN-MDL-0006",
                "The server stopped answering during its checks.",
            )
        })?;
    let allowed = cross
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if allowed == "*" || allowed == PROBE_ORIGIN {
        return Err(leaky("allowed requests from any website"));
    }
    running.verify_identity()?;
    if matches!(
        status(empty().bearer_auth(running.key()).send().await)?,
        401 | 403
    ) {
        return Err(CnError::new(
            "CN-MDL-0006",
            "The server refused the key it was started with.",
        ));
    }
    Ok(())
}

/* ---------- Failures: one rule, the person starts it ---------- */

/// Counts crashes and failed starts. There is no restart anywhere in this file.
#[derive(Debug, Default)]
pub struct Failures(VecDeque<Instant>);

impl Failures {
    /// The code to show. The third failure within 10 minutes changes the message (docs/44).
    pub fn record(&mut self, now: Instant, usual: &'static str) -> &'static str {
        self.0.retain(|at| now.duration_since(*at) < RESTART_WINDOW);
        self.0.push_back(now);
        if self.0.len() >= 3 {
            "CN-DSK-0011"
        } else {
            usual
        }
    }
}

/* ---------- An orphan from a crash ---------- */

#[derive(Debug, Deserialize, Serialize)]
pub struct Record {
    pub pid: u32,
    pub port: u16,
    pub program: PathBuf,
}

pub fn write_record(path: &Path, running: &Running, program: &Path) -> std::io::Result<()> {
    let record = Record {
        pid: running.pid,
        port: running.port,
        program: program.to_path_buf(),
    };
    crate::atomic::write(
        path,
        &serde_json::to_vec(&record).map_err(std::io::Error::other)?,
    )
}

pub fn clear_record(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Does this command line belong to the recorded server? A reused process id will not match.
pub fn is_same_process(command_line: &str, record: &Record) -> bool {
    command_line.contains(&*record.program.to_string_lossy())
        && command_line.contains(&format!("--port {}", record.port))
        && command_line.contains("--api-key")
}

/// At launch. Stops a leftover server only if the process still matches its record.
pub fn reap_orphan(path: &Path) -> bool {
    let Some(record) = fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Record>(&b).ok())
    else {
        clear_record(path);
        return false;
    };
    clear_record(path);
    let Ok(output) = Command::new("/bin/ps")
        .args(["-ww", "-p", &record.pid.to_string(), "-o", "command="])
        .output()
    else {
        return false;
    };
    let matches = is_same_process(&String::from_utf8_lossy(&output.stdout), &record);
    if matches {
        terminate(record.pid);
    }
    matches
}

pub fn no_runtime() -> CnError {
    CnError::new(error::NO_RUNTIME, "No llama-server was found.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_systems_listing() {
        assert_eq!(
            parse_lsof("p42\nf7\nn127.0.0.1:8080\n"),
            [Listener {
                pid: 42,
                address: "127.0.0.1:8080".into()
            }]
        );
        let ss = "LISTEN 0 4096 127.0.0.1:8080 0.0.0.0:* users:((\"llama-server\",pid=77,fd=3))\n";
        assert_eq!(
            parse_ss(ss),
            [Listener {
                pid: 77,
                address: "127.0.0.1:8080".into()
            }]
        );
        let net = "  TCP    127.0.0.1:8080   0.0.0.0:0   LISTENING   901\n  TCP    127.0.0.1:9   0.0.0.0:0   LISTENING   3\n";
        assert_eq!(
            parse_netstat(net, 8080),
            [Listener {
                pid: 901,
                address: "127.0.0.1:8080".into()
            }]
        );
    }

    #[test]
    fn only_our_child_on_loopback_passes() {
        let at = |pid, address: &str| Listener {
            pid,
            address: address.into(),
        };
        assert_eq!(judge(&[], 5).unwrap(), Holder::Nobody);
        assert_eq!(
            judge(&[at(5, "127.0.0.1:80")], 5).unwrap(),
            Holder::OurChild
        );
        assert_eq!(
            judge(&[at(6, "127.0.0.1:80")], 5).unwrap_err().code,
            "CN-MDL-0010"
        );
        assert_eq!(
            judge(&[at(5, "127.0.0.1:80"), at(6, "127.0.0.1:80")], 5)
                .unwrap_err()
                .code,
            "CN-MDL-0010"
        );
        assert_eq!(judge(&[at(5, "*:80")], 5).unwrap_err().code, "CN-MDL-0008");
        assert_eq!(
            judge(&[at(5, "0.0.0.0:80")], 5).unwrap_err().code,
            "CN-MDL-0008"
        );
    }

    #[test]
    fn the_third_failure_in_ten_minutes_changes_the_message() {
        let (mut failures, start) = (Failures::default(), Instant::now());
        assert_eq!(failures.record(start, "CN-MDL-0007"), "CN-MDL-0007");
        assert_eq!(failures.record(start, "CN-MDL-0007"), "CN-MDL-0007");
        assert_eq!(failures.record(start, "CN-MDL-0007"), "CN-DSK-0011");
        let mut later = Failures::default();
        later.record(start, "x");
        later.record(start, "x");
        assert_eq!(later.record(start + RESTART_WINDOW, "x"), "x");
    }

    #[test]
    fn a_reused_process_id_does_not_match_its_record() {
        let record = Record {
            pid: 1,
            port: 8123,
            program: "/opt/homebrew/bin/llama-server".into(),
        };
        assert!(is_same_process(
            "/opt/homebrew/bin/llama-server --host 127.0.0.1 --port 8123 --api-key k",
            &record
        ));
        assert!(!is_same_process("/usr/bin/vim --port 8123", &record));
        assert!(!is_same_process(
            "/opt/homebrew/bin/llama-server --port 9999 --api-key k",
            &record
        ));
    }
}
