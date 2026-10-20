//! The shared logger — timestamped lines into the log file (always
//! everything) with an optional verbose mirror on stderr.

use std::io::Write as _;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Log levels (the file gets everything; `-v` mirrors to stderr).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Verbose,
}

pub struct Log {
    file: Mutex<Option<std::fs::File>>,
    verbose_stderr: bool,
}

impl Log {
    /// Open (or create) the log file. A failed open degrades to
    /// stderr-only logging — a bad log path must never kill a
    /// recovery run.
    pub fn open(path: Option<&str>, verbose: bool) -> Self {
        let file = path.and_then(|p| {
            match std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(p)
            {
                Ok(f) => Some(f),
                Err(e) => {
                    eprintln!("dw-cli: cannot open log {p}: {e} (logging to stderr only)");
                    None
                }
            }
        });
        Self {
            file: Mutex::new(file),
            verbose_stderr: verbose,
        }
    }

    /// One timestamped line: always into the file (when open), on
    /// stderr when verbose or when there is no file.
    pub fn line(&self, level: Level, msg: &str) {
        let ts = timestamp();
        let line = format!("{ts} {msg}\n");
        let has_file = self.file.lock().is_ok_and(|mut guard| {
            if let Some(f) = guard.as_mut() {
                let _ = f.write_all(line.as_bytes());
            }
            guard.is_some()
        });
        if self.verbose_stderr || !has_file {
            eprint!("{line}");
        }
        let _ = level;
    }

    pub fn info(&self, msg: &str) {
        self.line(Level::Info, msg);
    }

    pub fn verbose(&self, msg: &str) {
        self.line(Level::Verbose, msg);
    }

    pub fn error(&self, msg: &str) {
        self.line(Level::Info, &format!("ERROR {msg}"));
    }
}

fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    format!("[{h:02}:{m:02}:{s:02}.{millis:03}]")
}
