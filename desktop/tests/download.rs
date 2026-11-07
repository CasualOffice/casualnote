//! The download manager, against a real HTTP server on 127.0.0.1 written here, with a small fixture
//! file. No request leaves this computer.
use casual_note_desktop::{
    catalogue,
    download::{self, Control, Job, Outcome, Phase, Report},
    error,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

fn fixture(len: usize) -> Vec<u8> {
    let mut bytes = b"GGUF\x03\0\0\0".to_vec();
    bytes.extend((0..len - 8).map(|i| (i % 251) as u8));
    bytes
}
fn sha(bytes: &[u8]) -> String {
    casual_note_desktop::text::hex(&Sha256::digest(bytes))
}

#[derive(Clone, Default)]
struct Behaviour {
    /// Serve this many bytes of the body in total, then drop the connection, once.
    cut_after: Option<usize>,
    ignore_range: bool,
    etag: Option<&'static str>,
    /// Sleep between pieces, to give a pause or cancel time to arrive.
    slow: bool,
    /// Redirect everything to this address.
    redirect: Option<String>,
    status: Option<u16>,
}

struct Server {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    served: Arc<AtomicUsize>,
}

fn serve(body: Vec<u8>, behaviour: Behaviour) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let served = Arc::new(AtomicUsize::new(0));
    let (log, count) = (requests.clone(), served.clone());
    let cut = Arc::new(Mutex::new(behaviour.cut_after));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let (body, behaviour, log, count, cut) = (
                body.clone(),
                behaviour.clone(),
                log.clone(),
                count.clone(),
                cut.clone(),
            );
            std::thread::spawn(move || answer(stream, &body, &behaviour, &log, &count, &cut));
        }
    });
    Server {
        addr,
        requests,
        served,
    }
}

fn answer(
    mut stream: TcpStream,
    body: &[u8],
    b: &Behaviour,
    log: &Mutex<Vec<String>>,
    count: &AtomicUsize,
    cut: &Mutex<Option<usize>>,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut head = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        head.push_str(&line);
    }
    log.lock().unwrap().push(head.clone());
    if let Some(status) = b.status {
        let _ = write!(
            stream,
            "HTTP/1.1 {status} X\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        );
        return;
    }
    if let Some(target) = &b.redirect {
        let _ = write!(
            stream,
            "HTTP/1.1 302 Found\r\nlocation: {target}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        );
        return;
    }
    let header = |name: &str| {
        head.lines().find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix(&format!("{name}: "))
                .map(str::to_owned)
        })
    };
    let mut start = 0;
    let mut partial = false;
    if !b.ignore_range
        && let Some(range) = header("range")
    {
        let validator_ok = header("if-range")
            .is_none_or(|tag| Some(tag.as_str()) == b.etag.map(str::to_ascii_lowercase).as_deref());
        if validator_ok {
            start = range
                .trim_start_matches("bytes=")
                .trim_end_matches('-')
                .parse()
                .unwrap();
            partial = true;
        }
    }
    let etag = b.etag.map_or(String::new(), |t| format!("etag: {t}\r\n"));
    let rest = &body[start..];
    let status = if partial {
        format!(
            "206 Partial\r\ncontent-range: bytes {start}-{}/{}",
            body.len() - 1,
            body.len()
        )
    } else {
        "200 OK".to_owned()
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\n{etag}content-length: {}\r\nconnection: close\r\n\r\n",
        rest.len()
    );
    let limit = cut.lock().unwrap().take();
    let mut sent = 0;
    for piece in rest.chunks(4096) {
        if limit.is_some_and(|l| sent >= l) {
            return; // A dropped connection, partway.
        }
        if stream.write_all(piece).is_err() {
            return;
        }
        sent += piece.len();
        count.fetch_add(piece.len(), Ordering::SeqCst);
        if b.slow {
            std::thread::sleep(Duration::from_millis(15));
        }
    }
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cn-dl-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn job_for(server: &Server, body: &[u8], dir: &Path) -> Job {
    let mut job = Job::new(
        "m",
        &format!("http://{}/model.gguf", server.addr),
        &sha(body),
        body.len() as u64,
        vec!["127.0.0.1".into()],
        dir.to_path_buf(),
    );
    job.allow_loopback_http = true;
    job.backoff = Duration::from_millis(5);
    job.margin = 0;
    job
}

fn plenty(_: &Path) -> std::io::Result<u64> {
    Ok(u64::MAX)
}

type Log = Arc<Mutex<Vec<Report>>>;
fn collector() -> (Log, impl Fn(Report) + Send + Sync) {
    let log: Log = Arc::default();
    let sink = log.clone();
    (log, move |r| sink.lock().unwrap().push(r))
}
fn phases(log: &Log) -> Vec<Phase> {
    log.lock().unwrap().iter().map(|r| r.phase).collect()
}
fn leftover(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

#[tokio::test]
async fn a_whole_download_installs_with_progress() {
    let body = fixture(600_000);
    let server = serve(
        body.clone(),
        Behaviour {
            etag: Some("\"v1\""),
            slow: true,
            ..Default::default()
        },
    );
    let dir = scratch("whole");
    let job = job_for(&server, &body, &dir);
    let (log, on) = collector();
    let outcome = download::run(&job, &Control::default(), &plenty, &on)
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Installed(job.final_path()));
    assert_eq!(fs::read(job.final_path()).unwrap(), body);
    assert_eq!(
        leftover(&dir),
        [format!("{}.gguf", job.sha256)],
        "no part or state file stays"
    );
    let seen = phases(&log);
    assert!(seen.contains(&Phase::Downloading) && seen.contains(&Phase::Verifying));
    let rising: Vec<u64> = log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.phase == Phase::Downloading)
        .map(|r| r.done)
        .collect();
    assert!(
        rising.windows(2).all(|w| w[0] <= w[1]) && rising.len() > 1,
        "{rising:?}"
    );
}

#[tokio::test]
async fn pause_then_resume_continues_by_range_without_sending_it_all_again() {
    let body = fixture(1_000_000);
    let server = serve(
        body.clone(),
        Behaviour {
            etag: Some("\"v1\""),
            slow: true,
            ..Default::default()
        },
    );
    let dir = scratch("resume");
    let job = job_for(&server, &body, &dir);
    let control = Arc::new(Control::default());
    let (_log, on) = collector();
    let pauser = control.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        pauser.pause();
    });
    assert_eq!(
        download::run(&job, &control, &plenty, &on).await.unwrap(),
        Outcome::Paused
    );
    let have = job.part_bytes();
    assert!(have > 0 && have < body.len() as u64, "{have}");
    assert!(
        !job.final_path().exists(),
        "nothing is installed by a pause"
    );

    // A new run, as after quitting the app, continues from the part file.
    let (log, on) = collector();
    let outcome = download::run(&job, &Control::default(), &plenty, &on)
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Installed(job.final_path()));
    assert_eq!(fs::read(job.final_path()).unwrap(), body);
    let requests = server.requests.lock().unwrap();
    let last = requests.last().unwrap().to_ascii_lowercase();
    assert!(last.contains(&format!("range: bytes={have}-")), "{last}");
    assert!(last.contains("if-range: \"v1\""), "{last}");
    assert!(!log.lock().unwrap().iter().any(|r| r.restarted));
}

#[tokio::test]
async fn a_dropped_connection_resumes_by_itself_and_the_file_is_right() {
    let body = fixture(300_000);
    let server = serve(
        body.clone(),
        Behaviour {
            etag: Some("\"v1\""),
            cut_after: Some(100_000),
            ..Default::default()
        },
    );
    let dir = scratch("dropped");
    let job = job_for(&server, &body, &dir);
    let (log, on) = collector();
    assert!(matches!(
        download::run(&job, &Control::default(), &plenty, &on)
            .await
            .unwrap(),
        Outcome::Installed(_)
    ));
    assert_eq!(fs::read(job.final_path()).unwrap(), body);
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    assert!(phases(&log).contains(&Phase::Retrying));
}

#[tokio::test]
async fn a_server_that_ignores_the_range_restarts_and_says_so() {
    let body = fixture(200_000);
    let server = serve(
        body.clone(),
        Behaviour {
            etag: Some("\"v1\""),
            ignore_range: true,
            ..Default::default()
        },
    );
    let dir = scratch("norange");
    let job = job_for(&server, &body, &dir);
    fs::write(job.part_path(), &body[..50_000]).unwrap();
    fs::write(
        dir.join(format!("{}.gguf.part.json", job.sha256)),
        serde_json::json!({"url": job.url, "sha256": job.sha256, "etag": "\"v1\""}).to_string(),
    )
    .unwrap();
    let (log, on) = collector();
    assert!(matches!(
        download::run(&job, &Control::default(), &plenty, &on)
            .await
            .unwrap(),
        Outcome::Installed(_)
    ));
    assert_eq!(
        fs::read(job.final_path()).unwrap(),
        body,
        "no duplicated bytes from the old part"
    );
    assert!(log.lock().unwrap().iter().any(|r| r.restarted));
}

#[tokio::test]
async fn a_changed_file_on_the_server_starts_over_rather_than_mixing_versions() {
    let body = fixture(200_000);
    let server = serve(
        body.clone(),
        Behaviour {
            etag: Some("\"new\""),
            ..Default::default()
        },
    );
    let dir = scratch("changed");
    let job = job_for(&server, &body, &dir);
    // Bytes from an older version on disk, with the older validator.
    fs::write(job.part_path(), vec![9u8; 50_000]).unwrap();
    fs::write(
        dir.join(format!("{}.gguf.part.json", job.sha256)),
        serde_json::json!({"url": job.url, "sha256": job.sha256, "etag": "\"old\""}).to_string(),
    )
    .unwrap();
    let (_log, on) = collector();
    assert!(matches!(
        download::run(&job, &Control::default(), &plenty, &on)
            .await
            .unwrap(),
        Outcome::Installed(_)
    ));
    assert_eq!(fs::read(job.final_path()).unwrap(), body);
}

#[tokio::test]
async fn a_wrong_hash_is_refused_deleted_and_never_installed() {
    let body = fixture(100_000);
    let server = serve(
        body.clone(),
        Behaviour {
            etag: Some("\"v1\""),
            ..Default::default()
        },
    );
    let dir = scratch("wrong");
    let mut job = job_for(&server, &body, &dir);
    job.sha256 = sha(b"the pinned file is some other file");
    let (_log, on) = collector();
    let refused = download::run(&job, &Control::default(), &plenty, &on)
        .await
        .unwrap_err();
    assert_eq!(refused.code, "CN-MDL-0002");
    assert!(leftover(&dir).is_empty(), "{:?}", leftover(&dir));
}

#[tokio::test]
async fn a_flipped_byte_in_a_resumed_part_is_refused() {
    // Everything about the resume looks valid. Only the full hash can catch this.
    let body = fixture(100_000);
    let server = serve(
        body.clone(),
        Behaviour {
            etag: Some("\"v1\""),
            ..Default::default()
        },
    );
    let dir = scratch("flip");
    let job = job_for(&server, &body, &dir);
    let mut part = body[..50_000].to_vec();
    part[1234] ^= 1;
    fs::write(job.part_path(), part).unwrap();
    fs::write(
        dir.join(format!("{}.gguf.part.json", job.sha256)),
        serde_json::json!({"url": job.url, "sha256": job.sha256, "etag": "\"v1\""}).to_string(),
    )
    .unwrap();
    let (_log, on) = collector();
    assert_eq!(
        download::run(&job, &Control::default(), &plenty, &on)
            .await
            .unwrap_err()
            .code,
        "CN-MDL-0002"
    );
    assert!(!job.final_path().exists());
    assert!(leftover(&dir).is_empty());
}

#[tokio::test]
async fn a_right_hash_on_something_that_is_not_gguf_is_refused() {
    let body = vec![7u8; 50_000];
    let server = serve(
        body.clone(),
        Behaviour {
            etag: Some("\"v1\""),
            ..Default::default()
        },
    );
    let dir = scratch("notgguf");
    let job = job_for(&server, &body, &dir);
    let (_log, on) = collector();
    assert_eq!(
        download::run(&job, &Control::default(), &plenty, &on)
            .await
            .unwrap_err()
            .code,
        "CN-MDL-0004"
    );
    assert!(leftover(&dir).is_empty());
}

#[tokio::test]
async fn cancel_stops_at_once_and_leaves_nothing() {
    let body = fixture(2_000_000);
    let server = serve(
        body.clone(),
        Behaviour {
            etag: Some("\"v1\""),
            slow: true,
            ..Default::default()
        },
    );
    let dir = scratch("cancel");
    let job = job_for(&server, &body, &dir);
    let control = Arc::new(Control::default());
    let canceller = control.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        canceller.cancel();
    });
    let (_log, on) = collector();
    assert_eq!(
        download::run(&job, &control, &plenty, &on).await.unwrap(),
        Outcome::Cancelled
    );
    assert!(leftover(&dir).is_empty(), "{:?}", leftover(&dir));
    assert!(server.served.load(Ordering::SeqCst) < body.len());
}

#[tokio::test]
async fn too_little_disk_refuses_before_any_request() {
    let body = fixture(50_000);
    let server = serve(body.clone(), Behaviour::default());
    let dir = scratch("disk");
    let mut job = job_for(&server, &body, &dir);
    job.margin = 1000;
    let (_log, on) = collector();
    let tight = |_: &Path| Ok(1000u64);
    let refused = download::run(&job, &Control::default(), &tight, &on)
        .await
        .unwrap_err();
    assert_eq!(refused.code, "CN-MDL-0001");
    assert_eq!(refused.detail, format!("needs={} free=1000", 51_000));
    assert!(server.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn sources_are_https_and_listed_at_every_hop() {
    let body = fixture(50_000);
    let dir = scratch("sources");
    // Production never allows plain http, even to a listed host.
    let server = serve(body.clone(), Behaviour::default());
    let mut job = job_for(&server, &body, &dir);
    job.allow_loopback_http = false;
    let (_log, on) = collector();
    let refused = download::run(&job, &Control::default(), &plenty, &on)
        .await
        .unwrap_err();
    assert!(refused.detail.contains("not HTTPS"), "{}", refused.detail);
    assert!(server.requests.lock().unwrap().is_empty());

    // A redirect to a host the entry does not list is refused before anything goes there.
    let elsewhere = serve(body.clone(), Behaviour::default());
    let bouncing = serve(
        body.clone(),
        Behaviour {
            redirect: Some("https://elsewhere.invalid/x".into()),
            ..Default::default()
        },
    );
    let job = job_for(&bouncing, &body, &dir);
    let refused = download::run(&job, &Control::default(), &plenty, &on)
        .await
        .unwrap_err();
    assert!(
        refused.detail.contains("does not list"),
        "{}",
        refused.detail
    );

    // Plain http on a redirect is refused even on this computer, unless the host is listed.
    let plain = serve(
        body.clone(),
        Behaviour {
            redirect: Some(format!("http://localhost:{}/x", elsewhere.addr.port())),
            ..Default::default()
        },
    );
    let job = job_for(&plain, &body, &dir);
    assert!(
        download::run(&job, &Control::default(), &plenty, &on)
            .await
            .is_err()
    );
    assert!(elsewhere.requests.lock().unwrap().is_empty());

    // A listed one is followed.
    let target = serve(
        body.clone(),
        Behaviour {
            etag: Some("\"v1\""),
            ..Default::default()
        },
    );
    let hop = serve(
        body.clone(),
        Behaviour {
            redirect: Some(format!("http://127.0.0.1:{}/y", target.addr.port())),
            ..Default::default()
        },
    );
    let job = job_for(&hop, &body, &dir);
    assert!(matches!(
        download::run(&job, &Control::default(), &plenty, &on)
            .await
            .unwrap(),
        Outcome::Installed(_)
    ));
}

#[tokio::test]
async fn a_server_offering_the_wrong_size_or_more_than_pinned_is_refused() {
    let body = fixture(50_000);
    let server = serve(body.clone(), Behaviour::default());
    let dir = scratch("size");
    let mut job = job_for(&server, &body, &dir);
    job.bytes = 40_000;
    let (_log, on) = collector();
    assert_eq!(
        download::run(&job, &Control::default(), &plenty, &on)
            .await
            .unwrap_err()
            .code,
        "CN-MDL-0003"
    );
    assert!(leftover(&dir).is_empty());
}

#[tokio::test]
async fn a_refusal_from_the_server_says_so_and_a_failure_keeps_what_arrived() {
    let body = fixture(50_000);
    let server = serve(
        body.clone(),
        Behaviour {
            status: Some(403),
            ..Default::default()
        },
    );
    let dir = scratch("refused");
    let job = job_for(&server, &body, &dir);
    let (_log, on) = collector();
    let refused = download::run(&job, &Control::default(), &plenty, &on)
        .await
        .unwrap_err();
    assert!(refused.detail.contains("403"));
}

#[test]
fn before_each_start_a_changed_model_is_refused_and_deleted() {
    let dir = scratch("start");
    let path = dir.join("model.gguf");
    let body = fixture(10_000);
    fs::write(&path, &body).unwrap();
    assert!(download::verify_before_start(&path, &sha(&body), |_| {}, || false).unwrap());
    assert!(path.exists());

    let mut changed = body.clone();
    changed[9_999] ^= 0x80;
    fs::write(&path, &changed).unwrap();
    let refused = download::verify_before_start(&path, &sha(&body), |_| {}, || false).unwrap_err();
    assert_eq!(refused.code, "CN-MDL-0002");
    assert!(!path.exists(), "the file that failed is gone");
}

#[test]
fn add_from_file_checks_the_pin_before_copying_anything() {
    let dir = scratch("add");
    let body = fixture(10_000);
    let source = dir.join("my-copy.gguf");
    fs::write(&source, &body).unwrap();
    let models = dir.join("models");
    let good = Job::new(
        "m",
        "https://huggingface.co/x",
        &sha(&body),
        body.len() as u64,
        vec![],
        models.clone(),
    );
    let placed = download::add_from_file(&source, &good, |_| {}).unwrap();
    assert_eq!(fs::read(placed).unwrap(), body);
    let wrong = Job::new(
        "m",
        "https://huggingface.co/x",
        &sha(b"other"),
        body.len() as u64,
        vec![],
        dir.join("elsewhere"),
    );
    assert_eq!(
        download::add_from_file(&source, &wrong, |_| {})
            .unwrap_err()
            .code,
        "CN-MDL-0002"
    );
    assert!(!wrong.final_path().exists());
}

#[test]
fn only_one_download_at_a_time() {
    let manager = download::Manager::default();
    let first = manager.begin("a").unwrap();
    assert_eq!(manager.begin("b").unwrap_err().code, error::BUSY);
    assert!(Arc::ptr_eq(&manager.control_of("a").unwrap(), &first));
    assert!(manager.control_of("b").is_none());
    manager.finish();
    manager.begin("b").unwrap();
}

#[test]
fn the_built_in_catalogue_cannot_download_over_plain_http() {
    let entries = catalogue::parse(catalogue::BUILT_IN).unwrap();
    for entry in &entries {
        let job = entry.job(Path::new("/tmp/x")).unwrap();
        assert!(!job.allow_loopback_http);
        assert!(download::check_source(&job.url, &job).is_ok());
        assert!(download::check_source(&job.url.replace("https://", "http://"), &job).is_err());
        assert!(download::check_source("https://cdn-lfs.huggingface.co/x", &job).is_ok());
        assert!(download::check_source("https://cas-bridge.xethub.hf.co/x", &job).is_ok());
        assert!(download::check_source("https://huggingface.co.evil.example/x", &job).is_err());
        assert!(download::check_source("https://evilhuggingface.co/x", &job).is_err());
        assert!(download::check_source("https://user@huggingface.co/x", &job).is_err());
    }
}
