//! Size-capped log file: when the current file reaches the limit it becomes
//! `<name>.old.log` (replacing the previous one), so the log never takes more
//! than about twice the limit on disk, however long the agent runs.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub struct RotatingLog {
    path: PathBuf,
    file: File,
    len: u64,
    limit: u64,
}

impl RotatingLog {
    pub fn open(path: &Path, limit: u64) -> io::Result<Self> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let len = file.metadata()?.len();
        let mut log = Self { path: path.to_owned(), file, len, limit };
        if log.len >= limit {
            log.rotate()?;
        }
        Ok(log)
    }

    pub fn old_path(&self) -> PathBuf {
        self.path.with_extension("old.log")
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        // Windows cannot rename an open file over another; drop our handle first.
        self.file = File::open(if cfg!(windows) { Path::new("NUL") } else { Path::new("/dev/null") })?;
        let _ = fs::remove_file(self.old_path());
        fs::rename(&self.path, self.old_path())?;
        self.file = OpenOptions::new().create(true).append(true).open(&self.path)?;
        self.len = 0;
        Ok(())
    }
}

impl Write for RotatingLog {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.len > 0 && self.len + buf.len() as u64 > self.limit {
            // Keep logging even if rotation fails (e.g. the old file is locked by a viewer).
            if let Err(e) = self.rotate() {
                self.file = OpenOptions::new().create(true).append(true).open(&self.path)?;
                let _ = writeln!(self.file, "log rotation failed: {e}");
            }
        }
        let n = self.file.write(buf)?;
        self.len += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_within_twice_the_limit() {
        let dir = std::env::temp_dir().join(format!("inputsmith-log-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("agent.log");
        let mut log = RotatingLog::open(&path, 1000).unwrap();
        for i in 0..500 {
            writeln!(log, "line {i:04} with some padding to fill the file").unwrap();
        }
        log.flush().unwrap();
        let cur = fs::metadata(&path).unwrap().len();
        let old = fs::metadata(log.old_path()).unwrap().len();
        assert!(cur <= 1000 && old <= 1000, "{cur} {old}");
        assert!(fs::read_to_string(&path).unwrap().contains("line 0499"));
        drop(log);
        // Reopening a full file rotates it right away.
        fs::write(&path, vec![b'x'; 1500]).unwrap();
        let log = RotatingLog::open(&path, 1000).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        assert_eq!(fs::metadata(log.old_path()).unwrap().len(), 1500);
        drop(log);
        let _ = fs::remove_dir_all(dir);
    }
}
