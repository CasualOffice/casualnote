//! The process lifecycle, against a fake `llama-server` written in this file: a real child process
//! that listens on a real port. `fake_llama_server` below is that program. The test binary runs
//! itself through a small shell wrapper, so no external program or model is involved.
#![cfg(unix)]

use casual_note_desktop::{proxy, runtime};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Notify;

/// Not a test. This is the fake server, run in its own process through the wrapper.
#[test]
#[ignore = "the fake server program, run by the other tests"]
fn fake_llama_server() {
    let Ok(mode) = std::env::var("FAKE_MODE") else {
        return;
    };
    let args: Vec<String> = std::env::var("FAKE_ARGS")
        .unwrap()
        .split(' ')
        .map(str::to_owned)
        .collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .map(|i| args[i + 1].clone())
    };
    let (port, key, log) = (
        value("--port").unwrap(),
        value("--api-key").unwrap(),
        std::env::var("FAKE_LOG").unwrap(),
    );
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .unwrap();
    writeln!(file, "spawned {}", args.join(" ")).unwrap();
    match mode.as_str() {
        "refuses_flag" => {
            eprintln!("error: invalid argument: --cors-origins");
            std::process::exit(1);
        }
        "no_bind" => std::thread::sleep(Duration::from_secs(600)),
        _ => {}
    }
    let host = if mode == "all_interfaces" {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };
    let listener = TcpListener::bind(format!("{host}:{port}")).unwrap();
    for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let (mut head, mut length) = (String::new(), 0);
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length: ") {
                length = v.trim().parse().unwrap_or(0);
            }
            head.push_str(&line);
        }
        let mut body = vec![0; length];
        let _ = reader.read_exact(&mut body);
        let first = head.lines().next().unwrap_or("").to_owned();
        let authorized = head
            .to_ascii_lowercase()
            .contains(&format!("authorization: bearer {key}"));
        let mut extra = String::new();
        let (status, reply) = if first.starts_with("GET /health") {
            if mode == "open_cors" {
                extra.push_str("access-control-allow-origin: *\r\n");
            }
            if mode == "never_ready" {
                ("503 Loading", "{}")
            } else {
                ("200 OK", "{\"status\":\"ok\"}")
            }
        } else if mode != "no_auth" && !authorized {
            ("401 Unauthorized", "{\"error\":\"key\"}")
        } else if body == b"{}" {
            ("400 Bad Request", "{\"error\":\"no messages\"}")
        } else {
            if mode == "crash_on_chat" {
                std::process::exit(9);
            }
            writeln!(file, "chat {}", String::from_utf8_lossy(&body)).unwrap();
            extra.push_str("content-type: text/event-stream\r\n");
            (
                "200 OK",
                "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\ndata: [DONE]\n\n",
            )
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\n{extra}content-length: {}\r\nconnection: close\r\n\r\n{reply}",
            reply.len()
        );
    }
}

struct Fake {
    dir: PathBuf,
    program: PathBuf,
    model: PathBuf,
    log: PathBuf,
}

fn fake(tag: &str, mode: &str) -> Fake {
    let dir = std::env::temp_dir().join(format!("cn-rt-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let (program, model, log) = (
        dir.join("llama-server"),
        dir.join("model.gguf"),
        dir.join("log.txt"),
    );
    fs::write(&model, b"GGUF").unwrap();
    let exe = std::env::current_exe().unwrap();
    let script = format!(
        "#!/bin/sh\nFAKE_MODE={mode} FAKE_LOG='{log}' FAKE_ARGS=\"$*\" exec '{exe}' --exact fake_llama_server --ignored --nocapture -- \"$@\"\n",
        log = log.display(),
        exe = exe.display()
    );
    fs::write(&program, script).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    Fake {
        dir,
        program,
        model,
        log,
    }
}

impl Fake {
    fn launch(&self) -> runtime::Launch {
        let mut launch = runtime::Launch::new(self.program.clone(), self.model.clone());
        launch.startup_timeout = Duration::from_secs(20);
        launch.poll = Duration::from_millis(50);
        launch
    }
    fn lines(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    fn spawned(&self) -> usize {
        self.lines()
            .iter()
            .filter(|l| l.starts_with("spawned"))
            .count()
    }
}

fn gone(pid: u32) -> bool {
    !Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}
fn wait_gone(pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if gone(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[tokio::test]
async fn starts_on_loopback_with_a_random_key_and_restricted_origins_then_stops() {
    let fake = fake("happy", "good");
    let mut first = runtime::launch(fake.launch()).await.unwrap();
    let line = fake.lines().into_iter().next().unwrap();
    assert!(line.contains("--host 127.0.0.1"), "{line}");
    assert!(line.contains(&format!("--port {}", first.port)), "{line}");
    assert!(
        line.contains(&format!("--api-key {}", first.key())),
        "{line}"
    );
    assert!(
        line.contains(&format!("--cors-origins {}", runtime::CORS_ORIGIN)),
        "{line}"
    );
    assert_eq!(first.key().len(), 64, "256 random bits, in hex");
    first.verify_identity().unwrap();
    let (pid, key, port) = (first.pid, first.key().to_owned(), first.port);
    assert!(first.exited().is_none());
    first.stop();
    assert!(wait_gone(pid), "stopped means gone");

    let mut second = runtime::launch(fake.launch()).await.unwrap();
    assert_ne!(second.key(), key, "a key per run");
    assert_ne!(second.port, 0);
    let _ = port;
    second.stop();
}

#[tokio::test]
async fn dropping_the_handle_takes_the_child_with_it() {
    let fake = fake("drop", "good");
    let running = runtime::launch(fake.launch()).await.unwrap();
    let pid = running.pid;
    drop(running);
    assert!(wait_gone(pid));
}

/// A program squatting on the port, recording everything sent to it.
fn squatter() -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream: TcpStream = stream.unwrap();
            let mut buffer = [0; 4096];
            let n = stream.read(&mut buffer).unwrap_or(0);
            log.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buffer[..n]).into_owned());
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}");
        }
    });
    (port, seen)
}

#[tokio::test]
async fn another_program_on_the_port_gets_nothing_and_our_child_is_stopped() {
    let (port, seen) = squatter();
    let fake = fake("squat", "no_bind");
    let mut launch = fake.launch();
    launch.port = Some(port);
    let refused = runtime::launch(launch).await.unwrap_err();
    assert_eq!(refused.code, "CN-MDL-0010", "{refused}");
    assert!(
        seen.lock().unwrap().is_empty(),
        "not even a health check reached it: {:?}",
        seen.lock().unwrap()
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    let leftover = Command::new("/usr/bin/pgrep")
        .args(["-f", &format!("--port {port}")])
        .output()
        .unwrap();
    assert!(
        leftover.stdout.is_empty(),
        "our child was stopped too: {:?}",
        String::from_utf8_lossy(&leftover.stdout)
    );
}

#[tokio::test]
async fn a_server_that_takes_no_key_is_stopped_before_the_key_is_sent() {
    let fake = fake("noauth", "no_auth");
    let refused = runtime::launch(fake.launch()).await.unwrap_err();
    assert_eq!(refused.code, "CN-MDL-0008", "{refused}");
    assert!(refused.detail.contains("no key"));
}

#[tokio::test]
async fn a_server_open_to_any_website_is_stopped() {
    let fake = fake("cors", "open_cors");
    let refused = runtime::launch(fake.launch()).await.unwrap_err();
    assert_eq!(refused.code, "CN-MDL-0008", "{refused}");
    assert!(refused.detail.contains("any website"));
}

#[tokio::test]
async fn a_server_listening_beyond_this_computer_is_stopped() {
    let fake = fake("wide", "all_interfaces");
    let refused = runtime::launch(fake.launch()).await.unwrap_err();
    assert_eq!(refused.code, "CN-MDL-0008", "{refused}");
    assert!(refused.detail.contains("beyond this computer"));
}

#[tokio::test]
async fn a_failed_start_shows_why_and_says_what_to_do_about_the_flag() {
    let fake = fake("flag", "refuses_flag");
    let refused = runtime::launch(fake.launch()).await.unwrap_err();
    assert_eq!(refused.code, "CN-MDL-0006", "{refused}");
    assert!(refused.detail.contains("Install a newer one"), "{refused}");
}

#[tokio::test]
async fn a_server_never_ready_times_out_and_is_stopped() {
    let fake = fake("slow", "never_ready");
    let mut launch = fake.launch();
    launch.startup_timeout = Duration::from_secs(3);
    let refused = runtime::launch(launch).await.unwrap_err();
    assert_eq!(refused.code, "CN-MDL-0006", "{refused}");
    assert!(refused.detail.contains("did not become ready"), "{refused}");
    assert_eq!(fake.spawned(), 1, "it never restarts by itself");
}

#[tokio::test]
async fn a_program_that_cannot_run_says_so() {
    let fake = fake("missing", "good");
    fs::remove_file(&fake.program).unwrap();
    assert_eq!(
        runtime::launch(fake.launch()).await.unwrap_err().code,
        "CN-MDL-0006"
    );
}

#[tokio::test]
async fn a_crash_is_seen_and_nothing_restarts_it() {
    let fake = fake("crash", "crash_on_chat");
    let mut running = runtime::launch(fake.launch()).await.unwrap();
    let client = casual_note_desktop::download::build_client(true).unwrap();
    let (url, headers) = proxy::local_request(&running.base_url(), running.key());
    let cancel = Notify::new();
    let _ = proxy::forward(
        &client,
        &url,
        &headers,
        "{\"messages\":[]}".into(),
        &cancel,
        |_| {},
    )
    .await;
    let deadline = Instant::now() + Duration::from_secs(5);
    while running.exited().is_none() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(running.exited().is_some(), "the crash is visible");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(fake.spawned(), 1, "no second process appeared");
}

#[tokio::test]
async fn the_chat_path_streams_through_the_shell_with_the_shells_key_only() {
    let fake = fake("chat", "good");
    let mut running = runtime::launch(fake.launch()).await.unwrap();
    let client = casual_note_desktop::download::build_client(true).unwrap();
    let (url, headers) = proxy::local_request(&running.base_url(), running.key());
    let cancel = Notify::new();
    let mut frames = Vec::new();
    proxy::forward(
        &client,
        &url,
        &headers,
        "{\"messages\":[1]}".into(),
        &cancel,
        |f| frames.push(f),
    )
    .await
    .unwrap();
    let proxy::Frame::Head {
        status,
        content_type,
        ..
    } = &frames[0]
    else {
        panic!("{frames:?}")
    };
    assert_eq!((*status, content_type.as_str()), (200, "text/event-stream"));
    let text: Vec<u8> = frames
        .iter()
        .skip(1)
        .flat_map(|f| {
            if let proxy::Frame::Chunk(b) = f {
                b.clone()
            } else {
                vec![]
            }
        })
        .collect();
    assert!(String::from_utf8_lossy(&text).contains("hello"));
    assert!(fake.lines().iter().any(|l| l.starts_with("chat ")));
    running.stop();
}

#[tokio::test]
async fn a_redirect_on_a_request_is_never_followed() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (target, seen) = squatter();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer);
            let _ = write!(
                stream,
                "HTTP/1.1 307 Moved\r\nlocation: http://127.0.0.1:{target}/\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
            );
        }
    });
    let client = casual_note_desktop::download::build_client(true).unwrap();
    let headers = vec![("authorization".to_owned(), "Bearer secret".to_owned())];
    let cancel = Notify::new();
    let refused = proxy::forward(
        &client,
        &format!("http://127.0.0.1:{port}/v1"),
        &headers,
        "{}".into(),
        &cancel,
        |_| {},
    )
    .await
    .unwrap_err();
    assert_eq!(refused.code, proxy::REDIRECT_REFUSED);
    assert!(
        seen.lock().unwrap().is_empty(),
        "the key never reached the other host"
    );
}

#[tokio::test]
async fn an_orphan_from_a_crash_is_stopped_only_if_it_is_the_same_process() {
    let fake = fake("orphan", "good");
    let running = runtime::launch(fake.launch()).await.unwrap();
    let record = fake.dir.join("record.json");

    // A record whose program does not match: untouched.
    fs::write(
        &record,
        serde_json::to_vec(&runtime::Record {
            pid: running.pid,
            port: running.port,
            program: "/not/this/program".into(),
        })
        .unwrap(),
    )
    .unwrap();
    assert!(!runtime::reap_orphan(&record));
    assert!(!gone(running.pid));
    assert!(!record.exists());

    // Its real record: stopped.
    runtime::write_record(&record, &running, &std::env::current_exe().unwrap()).unwrap();
    let pid = running.pid;
    assert!(
        runtime::reap_orphan(&record),
        "matched by program, port and key flag"
    );
    assert!(!record.exists());
    drop(running);
    assert!(wait_gone(pid));
}

#[test]
fn finding_a_program_prefers_the_choice_then_path_and_ignores_what_cannot_run() {
    let fake = fake("find", "good");
    let path_var = Some(std::ffi::OsString::from(fake.dir.as_os_str()));
    let (found, source) = runtime::find(None, path_var.clone(), None).unwrap();
    assert_eq!(
        (found, source),
        (fake.program.clone(), runtime::Source::Path)
    );
    let chosen = fake.dir.join("my-server");
    fs::copy(&fake.program, &chosen).unwrap();
    fs::set_permissions(&chosen, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        runtime::find(Some(&chosen), path_var.clone(), None).unwrap(),
        (chosen.clone(), runtime::Source::Chosen)
    );
    fs::set_permissions(&chosen, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        runtime::find(Some(&chosen), path_var, None).unwrap().1,
        runtime::Source::Path,
        "not executable, so not chosen"
    );
}
