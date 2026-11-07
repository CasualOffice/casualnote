//! Three simulated devices through a folder that delays, drops, duplicates, reorders and tampers.
//! The real store, the real CRDT and the real crypto. Everything comes from the seed.

use casual_note_merge::Workspace;
use casual_note_model::{Block, BlockKind, Run};
use casual_note_store::*;
use casual_note_sync::*;
use std::collections::{BTreeMap, BTreeSet};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn fill(&mut self, b: &mut [u8]) {
        for x in b.iter_mut() {
            *x = self.next() as u8;
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Faults {
    delay: u64,
    drop_put: u64,
    duplicate: u64,
    transient: u64,
    lag_list: u64,
    tamper: u64,
    reorder: bool,
}

const NOISY: Faults = Faults {
    delay: 15,
    drop_put: 8,
    duplicate: 10,
    transient: 8,
    lag_list: 10,
    tamper: 6,
    reorder: true,
};

#[derive(Default)]
struct Folder {
    files: BTreeMap<String, Vec<u8>>,
    delayed: Vec<(u64, String, Vec<u8>)>,
    tick: u64,
    deleted: Vec<String>,
}

impl Folder {
    fn tick(&mut self, rng: &mut Rng, f: &Faults) {
        self.tick += 1;
        let now = self.tick;
        let (ready, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.delayed)
            .into_iter()
            .partition(|(t, ..)| *t <= now);
        self.delayed = later;
        for (_, k, v) in ready {
            self.files.entry(k).or_insert(v);
        }
        if rng.chance(f.tamper) {
            let keys: Vec<String> = self
                .files
                .keys()
                .filter(|k| k.as_str() != ROOT_KEY)
                .cloned()
                .collect();
            if !keys.is_empty() {
                let k = keys[rng.below(keys.len() as u64) as usize].clone();
                let body = self.files.get_mut(&k).unwrap();
                match rng.below(3) {
                    0 if body.len() > 1 => {
                        let i = rng.below(body.len() as u64) as usize;
                        body[i] ^= 1 << rng.below(8);
                    }
                    1 if body.len() > 1 => {
                        let keep = rng.below(body.len() as u64) as usize;
                        body.truncate(keep);
                    }
                    _ => {
                        if !body.is_empty() {
                            body[0] ^= 0xFF;
                        }
                    }
                }
            }
        }
    }

    fn exec(&mut self, r: &StorageRequest, rng: &mut Rng, f: &Faults) -> StorageOutcome {
        if rng.chance(f.transient) {
            return StorageOutcome::Transient("simulated".into());
        }
        match r {
            StorageRequest::List { prefix } => {
                let mut entries: Vec<Entry> = self
                    .files
                    .iter()
                    .filter(|(k, _)| k.starts_with(prefix.as_str()))
                    .filter(|_| !rng.chance(f.lag_list))
                    .map(|(k, v)| Entry {
                        key: k.clone(),
                        len: v.len() as u64,
                    })
                    .collect();
                if f.reorder {
                    for i in (1..entries.len()).rev() {
                        entries.swap(i, rng.below(i as u64 + 1) as usize);
                    }
                }
                StorageOutcome::Listed(entries)
            }
            StorageRequest::Get { key, .. } => self
                .files
                .get(key)
                .map_or(StorageOutcome::NotFound, |b| StorageOutcome::Got(b.clone())),
            StorageRequest::PutIfAbsent { key, body } => {
                if self.files.contains_key(key) {
                    return StorageOutcome::AlreadyExists;
                }
                if rng.chance(f.drop_put) {
                    return StorageOutcome::Created; // Reported stored, and lost.
                }
                if rng.chance(f.delay) {
                    self.delayed
                        .push((self.tick + 1 + rng.below(5), key.clone(), body.clone()));
                    return StorageOutcome::Created;
                }
                if rng.chance(f.duplicate) {
                    self.files
                        .insert(format!("{key} (conflicted copy)"), body.clone());
                }
                self.files.insert(key.clone(), body.clone());
                StorageOutcome::Created
            }
            StorageRequest::Delete { key } => {
                self.files.remove(key);
                self.deleted.push(key.clone());
                StorageOutcome::Deleted
            }
        }
    }
}

fn pump(log: &mut Durable, host: &mut MemHost) {
    while let Some(b) = log.store.next_batch() {
        let r = host.run(&b.requests).unwrap();
        log.store.on_complete(b.id, r);
    }
}

fn para(id: &str, text: &str) -> Block {
    Block {
        id: id.into(),
        kind: BlockKind::Paragraph,
        checked: false,
        runs: vec![Run {
            text: text.into(),
            marks: vec![],
            link: None,
        }],
        target: None,
    }
}

struct Device {
    id: [u8; 16],
    name: String,
    host: MemHost,
    log: Durable,
    ws: Workspace,
    sess: Session,
    keys: Keys,
    opens: u32,
    /// Tokens the person was told are saved.
    acked: BTreeSet<String>,
    config: Config,
}

const SYNC_ID: [u8; 16] = [0x51; 16];

fn keys() -> Keys {
    Keys::new([0x77; 32], [0x42; 16])
}

impl Device {
    fn new(n: u8, config: Config) -> Self {
        let id = [n; 16];
        let cfg = StoreConfig {
            new_device: id,
            open_id: u32::from(n) << 24 | 1,
            peer: u64::from(n) << 40 | 1,
            wall_ms: 0,
            live_opens: vec![],
        };
        let (log, _) = Durable::open(Vec::new(), &cfg).unwrap();
        let keys = keys();
        let sess = Session::new(
            keys.clone(),
            id,
            &SYNC_ID,
            &format!("device{n}"),
            "test",
            None,
            config.clone(),
        );
        let mut d = Self {
            id,
            name: format!("device{n}"),
            host: MemHost::new(),
            log,
            ws: Workspace::new(cfg.peer).unwrap(),
            sess,
            keys,
            opens: 1,
            acked: BTreeSet::new(),
            config,
        };
        pump(&mut d.log, &mut d.host);
        d
    }

    /// A power cut, then a reopen with a fresh peer, as an app restart does.
    fn crash_and_reopen(&mut self, mode: &CrashMode) {
        self.opens += 1;
        let host = self.host.after_crash(mode);
        let n = self.id[0];
        let cfg = StoreConfig {
            new_device: self.id,
            open_id: u32::from(n) << 24 | self.opens,
            peer: u64::from(n) << 40 | u64::from(self.opens),
            wall_ms: 0,
            live_opens: vec![],
        };
        let (mut log, mut rec) = Durable::open(host.image(), &cfg).unwrap();
        self.host = host;
        pump(&mut log, &mut self.host);
        while log.guard_next(&rec).is_some() {
            pump(&mut log, &mut self.host);
            log.guard_try(&mut rec, |_, _| Ok(()));
        }
        log.guard_finish();
        pump(&mut log, &mut self.host);
        let mut ws = Workspace::new(cfg.peer).unwrap();
        if let Some(bytes) = rec.container() {
            ws.import(&bytes).unwrap();
            log.cursor.mark_durable(&bytes);
        }
        self.sess = Session::new(
            self.keys.clone(),
            self.id,
            &SYNC_ID,
            &self.name,
            "test",
            rec.local.get("sync").map(Vec::as_slice),
            self.config.clone(),
        );
        self.log = log;
        self.ws = ws;
    }

    fn round(&mut self, folder: &mut Folder, rng: &mut Rng, f: &Faults) {
        self.sess.begin_round();
        let mut guard = 0;
        loop {
            guard += 1;
            assert!(guard < 200, "a round did not finish");
            let mut seal_rng = Rng(rng.next() | 1);
            let step = self
                .sess
                .step(&mut self.ws, &mut self.log, folder.tick as i64, &mut |b| {
                    seal_rng.fill(b)
                });
            pump(&mut self.log, &mut self.host);
            let mut requests = step.requests;
            if f.reorder {
                for i in (1..requests.len()).rev() {
                    requests.swap(i, rng.below(i as u64 + 1) as usize);
                }
            }
            for r in requests {
                match r {
                    HostRequest::Storage { id, request } => {
                        let out = folder.exec(&request, rng, f);
                        self.sess.on_response(id, HostResponse::Storage(out));
                    }
                    HostRequest::ReadLocal { id, paths } => {
                        let files = paths
                            .iter()
                            .filter_map(|p| self.host.read(p).map(|b| (p.clone(), b.to_vec())))
                            .collect();
                        self.sess.on_response(id, HostResponse::Files(files));
                    }
                }
            }
            if step.done {
                break;
            }
        }
    }

    /// Types a token, and it counts once the log says it is durable.
    fn type_token(&mut self, token: &str, rng: &mut Rng) {
        let page = "shared";
        if !self.ws.has_page(page) {
            self.ws.create_page(page, None, "Shared", None).unwrap();
        }
        let blocks = self.ws.page(page).unwrap().blocks;
        if blocks.is_empty() || rng.chance(20) {
            let id = format!("b{token}");
            self.ws
                .insert_block(
                    page,
                    blocks.last().map(|b| b.id.as_str()),
                    &para(&id, token),
                )
                .unwrap();
        } else {
            let b = &blocks[rng.below(blocks.len() as u64) as usize];
            self.ws
                .splice_text(page, &b.id, 0, 0, &format!("{token} "))
                .unwrap();
        }
        self.log.save(&self.ws, 0).unwrap();
        pump(&mut self.log, &mut self.host);
        self.acked.insert(token.to_owned());
    }

    fn own_page(&mut self, token: &str) {
        let id = format!("page-{token}");
        self.ws.create_page(&id, None, token, None).unwrap();
        self.ws.insert_block(&id, None, &para("x", token)).unwrap();
        self.log.save(&self.ws, 0).unwrap();
        pump(&mut self.log, &mut self.host);
        self.acked.insert(token.to_owned());
    }

    fn text(&self) -> String {
        let mut out = String::new();
        for meta in self.ws.pages() {
            out.push_str(&meta.title);
            out.push('\n');
            for b in self.ws.page(&meta.id).unwrap().blocks {
                out.push_str(&b.plain());
                out.push('\n');
            }
        }
        out
    }

    /// What a person reads, per page, so devices with the same notes compare equal.
    fn view(&self) -> BTreeMap<String, String> {
        self.ws
            .pages()
            .into_iter()
            .map(|m| {
                let body: Vec<String> = self
                    .ws
                    .page(&m.id)
                    .unwrap()
                    .blocks
                    .iter()
                    .map(Block::plain)
                    .collect();
                (m.id, format!("{}\n{}", m.title, body.join("\n")))
            })
            .collect()
    }
}

fn folder_with_root() -> Folder {
    let mut f = Folder::default();
    f.files
        .insert(ROOT_KEY.into(), new_root(&SYNC_ID, &[0x42; 16]));
    f
}

fn settle(
    devices: &mut [Device],
    folder: &mut Folder,
    rng: &mut Rng,
    rounds: usize,
    what: &str,
) -> usize {
    let quiet = Faults::default();
    let want: BTreeSet<String> = devices
        .iter()
        .flat_map(|d| d.acked.iter().cloned())
        .collect();
    for r in 0..rounds {
        folder.tick(rng, &quiet);
        for d in devices.iter_mut() {
            d.round(folder, rng, &quiet);
        }
        let first = devices[0].view();
        let same = devices.iter().all(|d| d.view() == first);
        let has_all = devices.iter().all(|d| {
            let t = d.text();
            want.iter().all(|w| t.contains(w.as_str()))
        });
        if same && has_all && r > 1 {
            return r;
        }
    }
    for d in devices.iter() {
        let t = d.text();
        for w in &want {
            assert!(t.contains(w.as_str()), "{what}: {} lost {w}", d.name);
        }
    }
    let first = devices[0].view();
    for d in devices.iter() {
        assert_eq!(d.view(), first, "{what}: {} did not converge", d.name);
    }
    rounds
}

fn scenario(seed: u64, faults: Faults, crash: bool) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut folder = folder_with_root();
    let config = Config {
        checkpoint_every: 4,
        checkpoint_limit: 60 * 1024 * 1024,
    };
    let mut devices: Vec<Device> = (1..=3).map(|n| Device::new(n, config.clone())).collect();
    let mut n = 0;
    let steps = 14 + rng.below(10);
    for _ in 0..steps {
        folder.tick(&mut rng, &faults);
        let who = rng.below(3) as usize;
        for _ in 0..rng.below(3) {
            n += 1;
            let token = format!("tok{seed}x{n}y");
            if rng.chance(30) {
                devices[who].own_page(&token);
            } else {
                devices[who].type_token(&token, &mut rng);
            }
        }
        if rng.chance(70) {
            let d = rng.below(3) as usize;
            devices[d].round(&mut folder, &mut rng, &faults);
        }
        if crash && rng.chance(8) {
            let d = rng.below(3) as usize;
            let mode = if rng.chance(50) {
                CrashMode::LoseUnsynced
            } else {
                CrashMode::Scrambled(rng.next())
            };
            devices[d].crash_and_reopen(&mode);
        }
    }
    let what = format!("seed {seed}");
    settle(&mut devices, &mut folder, &mut rng, 80, &what);
    for d in &devices {
        let t = d.text();
        for w in &d.acked {
            assert!(
                t.contains(w.as_str()),
                "{what}: {} lost its own acknowledged {w}",
                d.name
            );
        }
    }
}

#[test]
fn clean_storage_converges() {
    for seed in 1..6 {
        scenario(seed, Faults::default(), false);
    }
}

#[test]
fn each_fault_on_its_own_loses_nothing() {
    let cases = [
        Faults {
            delay: 40,
            ..Default::default()
        },
        Faults {
            drop_put: 25,
            ..Default::default()
        },
        Faults {
            duplicate: 40,
            ..Default::default()
        },
        Faults {
            reorder: true,
            lag_list: 25,
            ..Default::default()
        },
        Faults {
            transient: 30,
            ..Default::default()
        },
        Faults {
            tamper: 30,
            ..Default::default()
        },
    ];
    for (i, f) in cases.iter().enumerate() {
        for seed in 1..6 {
            scenario(100 * (i as u64 + 1) + seed, *f, false);
        }
    }
}

#[test]
fn all_faults_together_with_crashes_lose_no_acknowledged_edit() {
    let seeds: u64 = std::env::var("CN_SYNC_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(40);
    for seed in 1..=seeds {
        scenario(1000 + seed, NOISY, true);
    }
}

#[test]
fn a_tampered_or_truncated_object_is_refused_visibly_then_repaired_by_its_owner() {
    let mut rng = Rng(99);
    let mut folder = folder_with_root();
    let quiet = Faults::default();
    let mut a = Device::new(1, Config::default());
    let mut b = Device::new(2, Config::default());
    a.own_page("alpha");
    a.round(&mut folder, &mut rng, &quiet);
    let segment = folder
        .files
        .keys()
        .find(|k| k.ends_with(".cnss"))
        .unwrap()
        .clone();
    let good = folder.files[&segment].clone();
    for damage in 0..2 {
        match damage {
            0 => {
                let f = folder.files.get_mut(&segment).unwrap();
                let at = f.len() / 2;
                f[at] ^= 1;
            }
            _ => folder
                .files
                .get_mut(&segment)
                .unwrap()
                .truncate(good.len() / 2),
        }
        b.round(&mut folder, &mut rng, &quiet);
        assert!(
            !b.text().contains("alpha"),
            "nothing from a refused object is imported"
        );
        let status = b.sess.status(&b.ws);
        let row = status
            .devices
            .iter()
            .find(|r| r.id == a.sess.device_hex())
            .unwrap();
        assert!(
            row.failed
                .iter()
                .any(|x| x.code == "CN-SYN-0004" && x.key == segment),
            "visible: {:?}",
            row.failed
        );
        // The owner reads its own object back, sees the change, and puts its bytes back.
        for _ in 0..3 {
            a.round(&mut folder, &mut rng, &quiet);
        }
        assert_eq!(folder.files.get(&segment), Some(&good), "damage {damage}");
        if damage == 1 {
            b.round(&mut folder, &mut rng, &quiet);
            assert!(b.text().contains("alpha"));
            let status = b.sess.status(&b.ws);
            let row = status
                .devices
                .iter()
                .find(|r| r.id == a.sess.device_hex())
                .unwrap();
            assert!(
                row.failed.is_empty(),
                "cleared once it opened: {:?}",
                row.failed
            );
        }
    }
}

#[test]
fn storage_holds_no_plaintext() {
    let mut rng = Rng(5);
    let mut folder = folder_with_root();
    let mut a = Device::new(1, Config::default());
    a.own_page("a secret page title");
    a.round(&mut folder, &mut rng, &Faults::default());
    assert!(folder.files.len() > 2);
    for (k, v) in &folder.files {
        for needle in [&b"secret"[..], b"page title", b"device1"] {
            assert!(
                !v.windows(needle.len()).any(|w| w == needle),
                "{k} holds plaintext"
            );
        }
        assert!(!k.contains("secret"));
    }
}

#[test]
fn a_device_writes_only_under_its_own_prefix() {
    let mut rng = Rng(7);
    let mut folder = folder_with_root();
    let mut a = Device::new(
        1,
        Config {
            checkpoint_every: 1,
            ..Default::default()
        },
    );
    let mut b = Device::new(
        2,
        Config {
            checkpoint_every: 1,
            ..Default::default()
        },
    );
    a.own_page("one");
    a.round(&mut folder, &mut rng, &Faults::default());
    let a_id = a.sess.device_hex();
    let before: BTreeMap<String, Vec<u8>> = folder
        .files
        .iter()
        .filter(|(k, _)| k.contains(&a_id))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    b.own_page("two");
    for _ in 0..4 {
        b.round(&mut folder, &mut rng, &Faults::default());
    }
    for (k, v) in before {
        assert_eq!(folder.files.get(&k), Some(&v), "b must not touch {k}");
    }
    let b_id = b.sess.device_hex();
    assert!(
        folder.deleted.iter().all(|k| k.contains(&b_id)),
        "b deletes only its own: {:?}",
        folder.deleted
    );
}

#[test]
fn nothing_is_trimmed_past_the_oldest_registered_device() {
    let mut rng = Rng(11);
    let mut folder = folder_with_root();
    let quiet = Faults::default();
    let config = Config {
        checkpoint_every: 2,
        checkpoint_limit: 60 * 1024 * 1024,
    };
    let mut a = Device::new(1, config.clone());
    let mut b = Device::new(2, config.clone());
    let mut c = Device::new(3, config.clone());
    // All three register. Then c goes quiet after seeing the first edit.
    a.own_page("first");
    a.round(&mut folder, &mut rng, &quiet);
    c.round(&mut folder, &mut rng, &quiet);
    b.round(&mut folder, &mut rng, &quiet);
    a.round(&mut folder, &mut rng, &quiet);
    let c_id = c.sess.device_hex();
    let a_id = a.sess.device_hex();
    let c_held_at_quiet = c.sess.state_base(&a_id);
    assert!(c_held_at_quiet >= 1);
    for i in 0..12 {
        a.own_page(&format!("later{i}"));
        a.round(&mut folder, &mut rng, &quiet);
        b.round(&mut folder, &mut rng, &quiet);
        a.round(&mut folder, &mut rng, &quiet);
        // Every object c has not acknowledged is still in the folder.
        for seq in (c_held_at_quiet + 1)..=a.sess.segment_seq() {
            let key = format!(
                "d/{a_id}/n/{}/s/{:04x}/{seq:016x}.cnss",
                "42".repeat(16),
                seq >> 12
            );
            assert!(
                folder.files.contains_key(&key),
                "round {i}: {key} was trimmed past {c_id}"
            );
        }
    }
    assert!(a.sess.floor() <= c_held_at_quiet);
    let held = a.sess.status(&a.ws);
    assert_eq!(
        held.held_back.as_deref(),
        Some(c_id.as_str()),
        "the panel names who holds trimming back"
    );
    // c returns, and everything arrives.
    let mut devices = vec![a, b, c];
    settle(&mut devices, &mut folder, &mut rng, 30, "return");
    // Now trimming can proceed.
    for _ in 0..6 {
        for d in devices.iter_mut() {
            d.round(&mut folder, &mut rng, &quiet);
        }
    }
    assert!(devices[0].sess.floor() > c_held_at_quiet);
    let a_id = devices[0].sess.device_hex();
    let early = format!("d/{a_id}/n/{}/s/{:04x}/{:016x}.cnss", "42".repeat(16), 0, 1);
    assert!(
        !folder.files.contains_key(&early),
        "trimmed once everyone has it"
    );
    // A brand-new device starts from a checkpoint and misses nothing.
    let mut d = Device::new(4, config);
    for _ in 0..4 {
        d.round(&mut folder, &mut rng, &quiet);
    }
    assert!(d.text().contains("first"), "{}", d.text());
    devices.push(d);
    settle(
        &mut devices,
        &mut folder,
        &mut rng,
        30,
        "joining after a trim",
    );
    // The newcomer's base snapshot survives its own restart.
    let last = devices.last_mut().unwrap();
    last.crash_and_reopen(&CrashMode::LoseUnsynced);
    assert!(last.text().contains("later11"));
}

#[test]
fn removing_a_device_is_explicit_and_lets_trimming_move() {
    let mut rng = Rng(13);
    let mut folder = folder_with_root();
    let quiet = Faults::default();
    let config = Config {
        checkpoint_every: 1,
        checkpoint_limit: 60 * 1024 * 1024,
    };
    let mut a = Device::new(1, config.clone());
    let mut gone = Device::new(2, config);
    gone.round(&mut folder, &mut rng, &quiet);
    a.own_page("x");
    for _ in 0..6 {
        a.round(&mut folder, &mut rng, &quiet);
    }
    assert_eq!(a.sess.floor(), 0, "a silent device holds it, for ever");
    let target = gone.sess.device_hex();
    a.sess.remove_device(&target);
    a.round(&mut folder, &mut rng, &quiet);
    assert!(a.sess.floor() >= 1);
    // The removal travels.
    let mut other = Device::new(3, Config::default());
    other.round(&mut folder, &mut rng, &quiet);
    other.round(&mut folder, &mut rng, &quiet);
    let status = other.sess.status(&other.ws);
    assert!(status.devices.iter().any(|r| r.id == target && r.removed));
}

#[test]
fn an_empty_or_missing_root_changes_nothing_locally_and_writes_nothing() {
    let mut rng = Rng(17);
    let mut folder = folder_with_root();
    let quiet = Faults::default();
    let mut a = Device::new(1, Config::default());
    a.own_page("kept");
    a.round(&mut folder, &mut rng, &quiet);
    let mut empty = Folder::default();
    a.round(&mut empty, &mut rng, &quiet);
    assert!(
        empty.files.is_empty(),
        "nothing is written to a folder that has lost its root"
    );
    assert!(a.text().contains("kept"));
    assert_eq!(
        serde_json::to_value(a.sess.status(&a.ws).root).unwrap(),
        "root_missing"
    );
    // Another folder's root is refused too.
    let mut other = Folder::default();
    other
        .files
        .insert(ROOT_KEY.into(), new_root(&[9; 16], &[0x42; 16]));
    a.round(&mut other, &mut rng, &quiet);
    assert_eq!(
        serde_json::to_value(a.sess.status(&a.ws).root).unwrap(),
        "wrong_root"
    );
    assert_eq!(other.files.len(), 1);
}

#[test]
fn hostile_file_names_are_counted_and_cannot_break_a_round() {
    let mut rng = Rng(31);
    let mut folder = folder_with_root();
    let quiet = Faults::default();
    let mut a = Device::new(1, Config::default());
    a.own_page("safe");
    a.round(&mut folder, &mut rng, &quiet);
    let me = a.sess.device_hex();
    let nb = "42".repeat(16);
    for key in [
        format!("d/{me}/n/{nb}/s/0000/ffffffffffffffff.cnss"),
        format!("d/{me}/n/{nb}/s/0000/0000000000000000.cnss"),
        format!("d/{}/r/ffffffffffffffff.cnsr", "ab".repeat(16)),
        format!("d/{}/n/{nb}/s/ffff/ffffffffffffffff.cnss", "ab".repeat(16)),
        "d/../../etc/passwd".to_owned(),
        "d/x".to_owned(),
    ] {
        folder.files.insert(key, b"not an object".to_vec());
    }
    let next = a.sess.segment_seq();
    a.round(&mut folder, &mut rng, &quiet);
    assert_eq!(
        a.sess.segment_seq(),
        next,
        "an unauthenticated name cannot move this device's sequence"
    );
    assert!(a.text().contains("safe"));
    assert!(a.sess.status(&a.ws).last_round.foreign_files >= 6);
}

#[test]
fn a_device_restored_from_an_old_backup_is_noticed() {
    let mut rng = Rng(19);
    let mut folder = folder_with_root();
    let quiet = Faults::default();
    let mut a = Device::new(1, Config::default());
    a.own_page("original");
    a.round(&mut folder, &mut rng, &quiet);
    let original: Vec<String> = folder
        .files
        .keys()
        .filter(|k| k.ends_with(".cnss"))
        .cloned()
        .collect();
    let bytes = folder.files[&original[0]].clone();
    // A restored backup: the same id, knowing nothing of what it wrote.
    let mut clone = Device::new(1, Config::default());
    clone.own_page("clone");
    clone.round(&mut folder, &mut rng, &quiet);
    assert_eq!(
        folder.files[&original[0]], bytes,
        "the original object is never overwritten"
    );
    clone.round(&mut folder, &mut rng, &quiet);
    assert_eq!(folder.files[&original[0]], bytes);
    assert_eq!(
        serde_json::to_value(clone.sess.status(&clone.ws).root).unwrap(),
        "forked",
        "and it says so"
    );
}

#[test]
fn two_devices_changing_one_block_make_a_review_on_both() {
    let mut rng = Rng(23);
    let mut folder = folder_with_root();
    let quiet = Faults::default();
    let mut a = Device::new(1, Config::default());
    let mut b = Device::new(2, Config::default());
    a.ws.create_page("p", None, "T", None).unwrap();
    a.ws.insert_block("p", None, &para("blk", "base")).unwrap();
    a.round(&mut folder, &mut rng, &quiet);
    b.round(&mut folder, &mut rng, &quiet);
    assert_eq!(b.ws.page("p").unwrap().blocks[0].plain(), "base");
    a.ws.splice_text("p", "blk", 4, 0, " from a").unwrap();
    b.ws.splice_text("p", "blk", 4, 0, " from b").unwrap();
    a.round(&mut folder, &mut rng, &quiet);
    b.round(&mut folder, &mut rng, &quiet);
    a.round(&mut folder, &mut rng, &quiet);
    let (sa, sb) = (a.sess.status(&a.ws), b.sess.status(&b.ws));
    assert_eq!(sa.reviews.len(), 1, "{:?}", sa.reviews);
    assert_eq!(sb.reviews.len(), 1, "{:?}", sb.reviews);
    assert_eq!(sa.reviews[0].id, sb.reviews[0].id);
    let here = sa.reviews[0].here.as_ref().unwrap().plain();
    let there = sa.reviews[0].there.as_ref().unwrap().plain();
    assert_eq!(
        (here.as_str(), there.as_str()),
        ("base from a", "base from b")
    );
    // Settling on one device clears it on the other, through the page.
    a.ws.resolve_review("p", &sa.reviews[0].id, "here").unwrap();
    a.round(&mut folder, &mut rng, &quiet);
    b.round(&mut folder, &mut rng, &quiet);
    assert!(a.sess.status(&a.ws).reviews.is_empty());
    assert!(b.sess.status(&b.ws).reviews.is_empty());
}

#[test]
fn compaction_with_sync_keeps_what_has_not_been_stored_and_loses_nothing() {
    let mut rng = Rng(29);
    let mut folder = folder_with_root();
    let quiet = Faults::default();
    let mut a = Device::new(1, Config::default());
    let mut b = Device::new(2, Config::default());
    a.own_page("one");
    a.round(&mut folder, &mut rng, &quiet);
    a.type_token("two", &mut rng);
    a.log.store.seal_all();
    pump(&mut a.log, &mut a.host);
    // "two" is sealed and has not been stored, so it must stay.
    let keep = a.sess.compaction_keep(&a.log);
    assert!(!keep.is_empty(), "unstored segments stay");
    for name in a.ws.doc_names() {
        let prefix = format!("{}/", format::doc_dir(&name));
        let files: Vec<ImageFile> = a
            .host
            .image()
            .into_iter()
            .filter(|f| f.path.starts_with(&prefix))
            .collect();
        let o = CompactOptions {
            min_segments: 1,
            keep: keep.clone(),
            device: a.id,
            wall_ms: 0,
            peer: 3,
        };
        if let Ok(Some(plan)) = plan_compaction(&name, &files, &o) {
            for r in &plan.removed {
                assert!(!keep.contains(r));
            }
            a.log.store.submit(plan.requests);
            pump(&mut a.log, &mut a.host);
            a.log.store.forget(&plan.removed);
        }
    }
    a.round(&mut folder, &mut rng, &quiet);
    b.round(&mut folder, &mut rng, &quiet);
    assert!(
        b.text().contains("one") && b.text().contains("two"),
        "{}",
        b.text()
    );
    a.crash_and_reopen(&CrashMode::LoseUnsynced);
    assert!(a.text().contains("one") && a.text().contains("two"));
}
