// Copyright (C) 2026 by the Quassel Project and the REVYTECH Rust port authors
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program; if not, write to the Free Software Foundation,
// Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301, USA.

//! The core's logging.
//!
//! The core writes to the standard error and, when `--logfile` says so, to a
//! file as well.  The levels are the ones the C++ core has: Debug, Info,
//! Warning and Error.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

/// The log levels, as numbers.
const DEBUG: u8 = 0;
const INFO: u8 = 1;
const WARNING: u8 = 2;
const ERROR: u8 = 3;

/// The level the core logs at.
static LEVEL: AtomicU8 = AtomicU8::new(INFO);

/// The file the core logs to, when there is one.
static LOGFILE: Mutex<Option<LogFile>> = Mutex::new(None);

/// The file the core writes its log to.
struct LogFile {
    /// The path of the file, for the error message.
    path: PathBuf,
    /// The open file.
    file: File,
}

/// Sets the log level.
///
/// An unknown level is a warning and leaves the level alone.
pub fn set_level(level: &str) {
    let value = match level.to_ascii_lowercase().as_str() {
        "debug" => DEBUG,
        "info" => INFO,
        "warning" | "warn" => WARNING,
        "error" => ERROR,
        _ => {
            eprintln!("Unknown log level \"{level}\", using Info");
            return;
        }
    };
    LEVEL.store(value, Ordering::Relaxed);
}

/// Sets the file the core logs to.
///
/// The file is opened right away: a core that cannot write its log has to
/// say so when it starts, not drop every line of it silently.
pub fn set_file(path: Option<PathBuf>) -> anyhow::Result<()> {
    let Some(path) = path else {
        let mut guard = LOGFILE.lock().expect("the log file lock");
        *guard = None;
        return Ok(());
    };
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| anyhow::anyhow!("cannot open the log file {}: {error}", path.display()))?;
    let mut guard = LOGFILE.lock().expect("the log file lock");
    *guard = Some(LogFile { path, file });
    Ok(())
}

/// Writes a line to the log.
fn write(level: u8, prefix: &str, message: &str) {
    if LEVEL.load(Ordering::Relaxed) > level {
        return;
    }
    let line = format!("{prefix}: {message}");
    eprintln!("{line}");
    let mut guard = LOGFILE.lock().expect("the log file lock");
    let Some(file) = guard.as_mut() else {
        return;
    };
    if let Err(error) = writeln!(file.file, "{line}") {
        // The log file cannot be written to any more; the operator has to
        // know that, so the error goes to the standard error as well.
        eprintln!(
            "Error: cannot write to the log file {}: {error}",
            file.path.display()
        );
    }
}

/// Logs a debug message.
pub fn debug(message: &str) {
    write(DEBUG, "Debug", message);
}

/// Logs an informational message.
pub fn info(message: &str) {
    write(INFO, "Info", message);
}

/// Logs a warning.
pub fn warning(message: &str) {
    write(WARNING, "Warning", message);
}

/// Logs an error.
pub fn error(message: &str) {
    write(ERROR, "Error", message);
}
