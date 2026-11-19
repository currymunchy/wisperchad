//! A small log of what went wrong, for working out problems afterwards. It holds errors and the
//! version only, never anything the user said, read or wrote.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

const FILE: &str = "wisperchad.log";

/// Starts over past this, so the log can't grow forever.
const MAX_BYTES: u64 = 1_000_000;

static LOG: OnceLock<PathBuf> = OnceLock::new();

pub fn init(dir: &Path) {
    if std::fs::create_dir_all(dir).is_ok() {
        let _ = LOG.set(dir.join(FILE));
    }
}

pub fn write(line: &str) {
    let Some(path) = LOG.get() else {
        return;
    };
    if std::fs::metadata(path).is_ok_and(|file| file.len() > MAX_BYTES) {
        let _ = std::fs::remove_file(path);
    }
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs());
    if let Ok(mut file) = std::fs::File::options()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{seconds} {line}");
    }
}
