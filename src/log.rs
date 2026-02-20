use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static LOG_FILE: OnceLock<Mutex<File>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

/// Initialise the optional file logger.
///
/// Reads `CURSOR_WRAPPER_LOG_FILE`; when set to a non-empty path the file is
/// opened in append mode and all subsequent `wlog!()` calls write to it.
pub fn init() {
    if let Ok(path) = std::env::var("CURSOR_WRAPPER_LOG_FILE") {
        if !path.is_empty() {
            if let Ok(file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = LOG_FILE.set(Mutex::new(file));
                START.get_or_init(Instant::now);
            }
        }
    }
}

pub fn write(msg: &str) {
    if let Some(file) = LOG_FILE.get() {
        if let Ok(mut f) = file.lock() {
            let elapsed = START.get().map_or(0.0, |s| s.elapsed().as_secs_f64());
            let _ = writeln!(f, "[{elapsed:>10.3}] {msg}");
            let _ = f.flush();
        }
    }
}

#[macro_export]
macro_rules! wlog {
    ($($arg:tt)*) => {
        $crate::log::write(&format!($($arg)*))
    };
}
