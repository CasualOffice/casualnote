//! The host I/O seam, and an in-memory host that can lose power at any byte.
//!
//! The store never does I/O. It hands the host an ordered list of requests, and "saved" is said
//! only when the whole list completed. Writes go to a file's volatile state. `SyncFile` makes a
//! file's contents durable. `SyncDir` makes the names in a directory durable, including removals.

use std::collections::{BTreeMap, BTreeSet};

/// One thing the host must do. A batch runs strictly in order and stops at the first failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IoRequest {
    CreateDir {
        path: String,
    },
    /// Creates the file if it does not exist.
    WriteAt {
        path: String,
        at: u64,
        data: Vec<u8>,
    },
    SyncFile {
        path: String,
    },
    SyncDir {
        path: String,
    },
    SetLen {
        path: String,
        len: u64,
    },
    Remove {
        path: String,
    },
}

/// Why a batch stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IoError {
    /// `CN-STO-0011`: no space or over quota.
    NoSpace,
    /// `CN-STO-0010`: anything else, including a failed flush.
    Failed(String),
}

/// A file as the host holds it, for opening the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// What survives a power cut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CrashMode {
    /// Everything not flushed is gone.
    LoseUnsynced,
    /// The disk got everything, flushed or not.
    KeepUnsynced,
    /// Each unflushed change survives or not, by this seed, in any combination. Later writes can
    /// survive without earlier ones, as a disk may reorder them.
    Scrambled(u64),
}

#[derive(Clone, Debug, Default)]
struct MemFile {
    durable: Vec<u8>,
    volatile: Vec<u8>,
    pending: Vec<Pending>,
}

#[derive(Clone, Debug)]
enum Pending {
    Write(u64, Vec<u8>),
    SetLen(u64),
}

fn apply(buf: &mut Vec<u8>, change: &Pending) {
    match change {
        Pending::Write(at, data) => {
            let at = at_usize(*at);
            if buf.len() < at + data.len() {
                buf.resize(at + data.len(), 0);
            }
            buf[at..at + data.len()].copy_from_slice(data);
        }
        Pending::SetLen(len) => buf.resize(at_usize(*len), 0),
    }
}

fn at_usize(v: u64) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// A power cut happened while running a batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crashed;

/// An in-memory disk. Names are volatile until their directory is flushed, as on a real disk.
#[derive(Clone, Debug, Default)]
pub struct MemHost {
    files: BTreeMap<String, MemFile>,
    /// Names that have been made durable, and so survive a crash.
    durable_names: BTreeSet<String>,
    /// Directories that exist, volatile.
    dirs: BTreeSet<String>,
    durable_dirs: BTreeSet<String>,
    /// Files removed with the removal not yet flushed: a power cut can bring them back.
    ghosts: BTreeMap<String, Vec<u8>>,
    /// Units of work left before the power goes: a byte written, or one other request.
    budget: Option<u64>,
    /// Requests of these kinds fail, for fault injection: (the nth request from now, the error).
    fail_at: Option<(u64, IoError)>,
    /// Every request completed, for inspection.
    pub flushes: u64,
    pub bytes_written: u64,
}

impl MemHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// The power goes after this many units of work.
    pub fn crash_after(&mut self, units: u64) {
        self.budget = Some(units);
    }

    /// The nth request from now fails, once.
    pub fn fail_request(&mut self, nth: u64, error: IoError) {
        self.fail_at = Some((nth, error));
    }

    /// How much work a request is, in `crash_after` units.
    pub fn units(request: &IoRequest) -> u64 {
        match request {
            IoRequest::WriteAt { data, .. } => data.len() as u64 + 1,
            _ => 1,
        }
    }

    fn spend(&mut self, n: u64) -> Result<(), Crashed> {
        match self.budget {
            Some(left) if left < n => {
                self.budget = Some(0);
                Err(Crashed)
            }
            Some(left) => {
                self.budget = Some(left - n);
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Runs one request. A partial write happens when the power goes during it.
    pub fn exec(&mut self, request: &IoRequest) -> Result<Result<(), IoError>, Crashed> {
        if let Some((n, e)) = &mut self.fail_at {
            if *n == 0 {
                let e = e.clone();
                self.fail_at = None;
                return Ok(Err(e));
            }
            *n -= 1;
        }
        match request {
            IoRequest::CreateDir { path } => {
                self.spend(1)?;
                let mut p = path.as_str();
                while !p.is_empty() {
                    self.dirs.insert(p.to_owned());
                    p = parent(p);
                }
            }
            IoRequest::WriteAt { path, at, data } => {
                let whole = data.len() as u64;
                let left = self.budget.unwrap_or(u64::MAX);
                if left < whole + 1 {
                    // The power goes after this many bytes of the write reached the page cache.
                    let kept = usize::try_from(left).unwrap_or(0).min(data.len());
                    let f = self.files.entry(path.clone()).or_default();
                    let change = Pending::Write(*at, data[..kept].to_vec());
                    apply(&mut f.volatile, &change);
                    f.pending.push(change);
                    self.budget = Some(0);
                    return Err(Crashed);
                }
                self.spend(whole + 1)?;
                self.bytes_written += whole;
                let f = self.files.entry(path.clone()).or_default();
                let change = Pending::Write(*at, data.clone());
                apply(&mut f.volatile, &change);
                f.pending.push(change);
            }
            IoRequest::SyncFile { path } => {
                self.spend(1)?;
                self.flushes += 1;
                if let Some(f) = self.files.get_mut(path) {
                    f.durable = f.volatile.clone();
                    f.pending.clear();
                }
            }
            IoRequest::SyncDir { path } => {
                self.spend(1)?;
                let prefix = if path.is_empty() {
                    String::new()
                } else {
                    format!("{path}/")
                };
                let direct = |p: &str| p.strip_prefix(&prefix).is_some_and(|r| !r.contains('/'));
                let live: Vec<String> = self.files.keys().filter(|p| direct(p)).cloned().collect();
                self.durable_names.retain(|p| !direct(p));
                self.durable_names.extend(live);
                let dirs: Vec<String> = self.dirs.iter().filter(|p| direct(p)).cloned().collect();
                self.durable_dirs.extend(dirs);
                self.ghosts.retain(|p, _| !direct(p));
            }
            IoRequest::SetLen { path, len } => {
                self.spend(1)?;
                if let Some(f) = self.files.get_mut(path) {
                    let change = Pending::SetLen(*len);
                    apply(&mut f.volatile, &change);
                    f.pending.push(change);
                }
            }
            IoRequest::Remove { path } => {
                self.spend(1)?;
                if let Some(f) = self.files.remove(path)
                    && self.durable_names.contains(path)
                {
                    self.ghosts.insert(path.clone(), f.durable);
                }
            }
        }
        Ok(Ok(()))
    }

    /// Runs a batch in order.
    pub fn run(&mut self, requests: &[IoRequest]) -> Result<Result<(), IoError>, Crashed> {
        for r in requests {
            if let Err(e) = self.exec(r)? {
                return Ok(Err(e));
            }
        }
        Ok(Ok(()))
    }

    /// The disk as it would be found after a power cut now. In `LoseUnsynced`, a file survives
    /// only if its name and every parent directory were flushed, and a removal not yet flushed
    /// comes undone.
    pub fn after_crash(&self, mode: &CrashMode) -> MemHost {
        let mut out = MemHost::new();
        let mut rng = match mode {
            CrashMode::Scrambled(seed) => seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            _ => 1,
        };
        let mut flip = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng & 1 == 1
        };
        for (path, f) in &self.files {
            let keep_name = match mode {
                CrashMode::LoseUnsynced => {
                    self.durable_names.contains(path) && self.ancestors_durable(path)
                }
                CrashMode::KeepUnsynced => true,
                CrashMode::Scrambled(_) => self.durable_names.contains(path) || flip(),
            };
            if !keep_name {
                continue;
            }
            let bytes = match mode {
                CrashMode::LoseUnsynced => f.durable.clone(),
                CrashMode::KeepUnsynced => f.volatile.clone(),
                CrashMode::Scrambled(_) => {
                    let mut b = f.durable.clone();
                    for change in &f.pending {
                        if flip() {
                            apply(&mut b, change);
                        }
                    }
                    b
                }
            };
            out.files.insert(
                path.clone(),
                MemFile {
                    durable: bytes.clone(),
                    volatile: bytes,
                    pending: Vec::new(),
                },
            );
            out.durable_names.insert(path.clone());
        }
        for (path, bytes) in &self.ghosts {
            let back = match mode {
                CrashMode::LoseUnsynced => self.ancestors_durable(path),
                CrashMode::KeepUnsynced => false,
                CrashMode::Scrambled(_) => flip(),
            };
            if back && !out.files.contains_key(path) {
                out.files.insert(
                    path.clone(),
                    MemFile {
                        durable: bytes.clone(),
                        volatile: bytes.clone(),
                        pending: Vec::new(),
                    },
                );
                out.durable_names.insert(path.clone());
            }
        }
        // What is found after the crash is on the disk, so its parents are too.
        let mut dirs = self.durable_dirs.clone();
        for path in out.files.keys() {
            let mut p = parent(path);
            while !p.is_empty() {
                dirs.insert(p.to_owned());
                p = parent(p);
            }
        }
        out.dirs = dirs.clone();
        out.durable_dirs = dirs;
        out
    }

    fn ancestors_durable(&self, path: &str) -> bool {
        let mut p = parent(path);
        while !p.is_empty() {
            if !self.durable_dirs.contains(p) {
                return false;
            }
            p = parent(p);
        }
        true
    }

    /// Every file with its current bytes.
    pub fn image(&self) -> Vec<ImageFile> {
        self.files
            .iter()
            .map(|(path, f)| ImageFile {
                path: path.clone(),
                bytes: f.volatile.clone(),
            })
            .collect()
    }

    pub fn read(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(|f| f.volatile.as_slice())
    }

    pub fn paths(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    /// Changes a file's bytes directly, as bit rot or a person would.
    pub fn poke(&mut self, path: &str, f: impl FnOnce(&mut Vec<u8>)) {
        if let Some(file) = self.files.get_mut(path) {
            f(&mut file.volatile);
            file.durable = file.volatile.clone();
            file.pending.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &str, data: &[u8]) -> IoRequest {
        IoRequest::WriteAt {
            path: path.into(),
            at: 0,
            data: data.to_vec(),
        }
    }

    #[test]
    fn a_file_needs_its_flush_and_its_directory_flush_to_survive() {
        let mut h = MemHost::new();
        h.run(&[write("a/x", b"hello")]).unwrap().unwrap();
        assert_eq!(h.after_crash(&CrashMode::LoseUnsynced).read("a/x"), None);
        h.run(&[IoRequest::SyncFile { path: "a/x".into() }])
            .unwrap()
            .unwrap();
        assert_eq!(
            h.after_crash(&CrashMode::LoseUnsynced).read("a/x"),
            None,
            "the name is not durable yet"
        );
        h.run(&[
            IoRequest::CreateDir { path: "a".into() },
            IoRequest::SyncDir { path: "a".into() },
        ])
        .unwrap()
        .unwrap();
        assert_eq!(
            h.after_crash(&CrashMode::LoseUnsynced).read("a/x"),
            None,
            "the directory itself is not durable yet"
        );
        h.run(&[IoRequest::SyncDir { path: "".into() }])
            .unwrap()
            .unwrap();
        assert_eq!(
            h.after_crash(&CrashMode::LoseUnsynced).read("a/x"),
            Some(&b"hello"[..])
        );
    }

    #[test]
    fn a_removal_is_undone_until_its_directory_is_flushed() {
        let mut h = MemHost::new();
        h.run(&[
            write("x", b"a"),
            IoRequest::SyncFile { path: "x".into() },
            IoRequest::SyncDir { path: "".into() },
        ])
        .unwrap()
        .unwrap();
        h.run(&[IoRequest::Remove { path: "x".into() }])
            .unwrap()
            .unwrap();
        assert_eq!(h.read("x"), None);
        assert_eq!(
            h.after_crash(&CrashMode::LoseUnsynced).read("x"),
            Some(&b"a"[..])
        );
        h.run(&[IoRequest::SyncDir { path: "".into() }])
            .unwrap()
            .unwrap();
        assert_eq!(h.after_crash(&CrashMode::LoseUnsynced).read("x"), None);
    }

    #[test]
    fn a_crash_tears_a_write_at_the_exact_byte() {
        let mut h = MemHost::new();
        h.crash_after(3);
        assert_eq!(h.run(&[write("x", b"hello")]), Err(Crashed));
        assert_eq!(h.read("x"), Some(&b"hel"[..]));
    }
}
