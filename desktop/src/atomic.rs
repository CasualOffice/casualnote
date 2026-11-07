//! Writing a file so that a crash leaves the old bytes or the new bytes, never a mixture:
//! temp file in the same directory, fsync, rename, fsync of the directory.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_beside(path: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?
        .to_string_lossy();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    Ok(path.with_file_name(format!(".{name}.{}.{nanos}.tmp", std::process::id())))
}

/// Makes a rename durable. Windows has no directory handle for this, and `rename` there goes
/// through `MoveFileExW`. Not built or run on Windows yet.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// Fills a temp file through `fill`, then puts it in place. If anything fails, the old file is
/// untouched and the temp file is gone.
pub fn write_with(path: &Path, fill: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = temp_beside(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let result = (|| {
        let mut file = options.open(&temp)?;
        fill(&mut file)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    sync_dir(path.parent().unwrap_or(Path::new(".")))
}

pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_with(path, |file| file.write_all(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cn-atomic-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn replaces_the_whole_file_and_leaves_nothing_beside_it() {
        let dir = scratch("replace");
        let path = dir.join("notes.bin");
        write(&path, b"first, and longer").unwrap();
        write(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_write_keeps_the_old_bytes_and_cleans_up() {
        let dir = scratch("fail");
        let path = dir.join("notes.bin");
        write(&path, b"precious").unwrap();
        let failed = write_with(&path, |file| {
            file.write_all(b"half of the new")?;
            Err(io::Error::from(io::ErrorKind::StorageFull))
        });
        assert!(failed.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"precious");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_rename_keeps_what_was_there() {
        let dir = scratch("rename");
        let target = dir.join("is-a-directory");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("inside"), b"kept").unwrap();
        assert!(write(&target, b"new").is_err());
        assert_eq!(fs::read(target.join("inside")).unwrap(), b"kept");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
