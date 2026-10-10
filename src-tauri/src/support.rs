//! What support needs when something goes wrong at a school: a log file.
//!
//! Termbook writes what it does and every error to a log in Windows' own log
//! folder for the app (`%LOCALAPPDATA%\ug.co.termbook\logs`), kept to five
//! files of 2 MB so it never fills a disk. A crash is written there too.
//! Settings → Help & support saves the logs as one text file the school can
//! send on WhatsApp or email. The log holds no marks, fees or passwords: only
//! what the app did and what failed.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tauri::State;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

use crate::commands::exports::{self, staged, StagedFile};
use crate::error::AppResult;
use crate::state::AppState;

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

const LOG_FILE: &str = "termbook";
const MAX_BYTES: u128 = 2 * 1024 * 1024;

/// The log plugin, configured for a school computer.
pub fn plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_log::Builder::new()
        .clear_targets()
        .targets([
            Target::new(TargetKind::LogDir { file_name: Some(LOG_FILE.into()) }),
            Target::new(TargetKind::Stdout),
        ])
        .level(log::LevelFilter::Info)
        // The window and network libraries are chatty; only their problems matter.
        .level_for("tao", log::LevelFilter::Warn)
        .level_for("wry", log::LevelFilter::Warn)
        .level_for("ureq", log::LevelFilter::Warn)
        .level_for("rustls", log::LevelFilter::Warn)
        .max_file_size(MAX_BYTES)
        .rotation_strategy(RotationStrategy::KeepSome(5))
        .timezone_strategy(TimezoneStrategy::UseLocal)
        .build()
}

/// Remembers where the logs are, and sends any crash to the log as well as
/// to the screen.
pub fn init(log_dir: PathBuf) {
    let _ = LOG_DIR.set(log_dir);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("Termbook stopped unexpectedly: {info}");
        previous(info);
    }));
}

/// Every log file, oldest first, joined under a short header.
pub fn collect(log_dir: &Path, header: &str) -> std::io::Result<String> {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("log") {
                let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
                files.push((modified, path));
            }
        }
    }
    files.sort();
    let mut out = String::from(header);
    if files.is_empty() {
        out.push_str("\n(no log files yet)\n");
    }
    for (_, path) in files {
        out.push_str(&format!("\n===== {} =====\n", path.file_name().and_then(|n| n.to_str()).unwrap_or("")));
        out.push_str(&String::from_utf8_lossy(&std::fs::read(&path)?));
    }
    Ok(out)
}

/// The logs as one text file, ready for the Save dialog.
#[tauri::command]
pub fn save_support_log(state: State<'_, AppState>) -> AppResult<StagedFile> {
    state.sessions.require()?;
    let (school, schema): (String, i64) = {
        let conn = state.db.lock();
        let school = conn.query_row("SELECT name FROM institution LIMIT 1", [], |r| r.get(0)).unwrap_or_default();
        let schema = conn.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0)).unwrap_or(0);
        (school, schema)
    };
    let db_size = std::fs::metadata(&state.paths.db_file).map(|m| m.len()).unwrap_or(0);
    let header = format!(
        "Termbook support log\nSchool: {school}\nVersion: {}\nDatabase: version {schema}, {} KB\nSaved: {}\nSystem: {} {}\n",
        env!("CARGO_PKG_VERSION"),
        db_size / 1024,
        chrono::Local::now().format("%Y-%m-%d %H:%M"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    let dir = LOG_DIR.get().cloned().unwrap_or_else(|| state.paths.data_dir.join("logs"));
    let text = collect(&dir, &header)?;
    let name = format!("Termbook support log {}.txt", chrono::Local::now().format("%Y-%m-%d"));
    let file = exports::stage(&state.paths.export_dir, &name, text.as_bytes())?;
    log::info!("support log saved for sending");
    Ok(staged(file, "txt", "the support log"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_are_joined_oldest_first_under_the_header() {
        let dir = std::env::temp_dir().join(format!("tb-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("termbook_2026-10-01.log"), "old line\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("termbook.log"), "new line\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a log").unwrap();
        let text = collect(&dir, "HEADER\n").unwrap();
        assert!(text.starts_with("HEADER"));
        assert!(text.find("old line").unwrap() < text.find("new line").unwrap());
        assert!(!text.contains("not a log"));
        assert!(collect(&dir.join("missing"), "H\n").unwrap().contains("no log files yet"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
