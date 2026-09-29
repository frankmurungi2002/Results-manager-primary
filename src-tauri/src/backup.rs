//! Backups and restore — the simplified FR-B12 from SRS 16.1.
//!
//! A consistent snapshot of the live database goes to the local backups folder
//! and, when one is set, to a folder on a second drive. Snapshots are taken
//! when someone asks, every fifteen minutes while the database is changing,
//! and when RM closes. A snapshot of an unchanged database is skipped.
//!
//! Restore copies a snapshot back into the live database in place. What it
//! replaces is snapshotted first, so a restore can itself be undone.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::audit;
use crate::db::{self, migrations};
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::Session;
use crate::state::AppState;

/// How often the scheduler looks for changes to back up.
pub const INTERVAL: Duration = Duration::from_secs(15 * 60);

/// The newest snapshots are always kept; beyond these, one per day for
/// `KEEP_DAYS` days. Every fifteen minutes would otherwise push a week-old
/// snapshot out within a single busy afternoon.
const KEEP_NEWEST: usize = 20;
const KEEP_DAYS: i64 = 30;

/// Where snapshots go inside the chosen folder on the second drive.
const MIRROR_SUBDIR: &str = "ResultsManager";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Manual,
    Scheduled,
    OnExit,
    BeforeRestore,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub local_path: String,
    pub mirror_path: Option<String>,
    pub bytes: u64,
    pub at: String,
}

/// Takes a snapshot now. Only a manual snapshot or the one before a restore is
/// written to the audit trail; the automatic ones would drown it.
pub fn take(state: &AppState, actor: Option<&Session>, trigger: Trigger) -> AppResult<BackupResult> {
    let stamp = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let filename = match trigger {
        Trigger::BeforeRestore => format!("school-{stamp}-before-restore.rmdb"),
        _ => format!("school-{stamp}.rmdb"),
    };

    let local_path = state.paths.backup_dir.join(&filename);
    state.db.backup_to(&local_path)?;
    let bytes = std::fs::metadata(&local_path).map(|m| m.len()).unwrap_or(0);

    let mirror_dir = {
        let conn = state.db.lock();
        repo::get_setting(&conn, "backup.mirror_path")?
    }
    .map(|dir| Path::new(&dir).join(MIRROR_SUBDIR));

    let mirror_path = match &mirror_dir {
        Some(dir) => {
            let target = dir.join(&filename);
            match state.db.backup_to(&target) {
                Ok(()) => Some(target.display().to_string()),
                Err(err) => {
                    // A missing second drive is a warning, not a failed backup —
                    // the local copy already succeeded.
                    log::warn!("mirror backup failed: {err}");
                    None
                }
            }
        }
        None => None,
    };

    let at = Utc::now().to_rfc3339();
    {
        let conn = state.db.lock();
        repo::set_setting(&conn, "backup.last_at", &at)?;
        repo::set_setting(&conn, "backup.last_path", &local_path.display().to_string())?;

        if matches!(trigger, Trigger::Manual | Trigger::BeforeRestore) {
            let place = match &mirror_path {
                Some(path) => format!("{path} and the local folder"),
                None => "the local folder".to_string(),
            };
            let summary = match trigger {
                Trigger::BeforeRestore => format!("Saved the current data to {place} before a restore"),
                _ => format!("Backed up to {place}"),
            };
            audit::record(&conn, actor, "backup.run", "backup", &filename, summary)?;
        }

        // Read after this snapshot's own bookkeeping writes, so they do not
        // count as a change that needs the next snapshot.
        state.mark_backed_up(conn.total_changes());
    }

    prune(&state.paths.backup_dir);
    if let Some(dir) = &mirror_dir {
        prune(dir);
    }

    Ok(BackupResult {
        local_path: local_path.display().to_string(),
        mirror_path,
        bytes,
        at,
    })
}

/// Takes a snapshot only if something was written since the last one.
pub fn take_if_changed(state: &AppState, trigger: Trigger) -> AppResult<Option<BackupResult>> {
    if !state.has_unbacked_changes(state.db.total_changes()) {
        return Ok(None);
    }
    take(state, None, trigger).map(Some)
}

/// Starts the background thread that snapshots a changing database every
/// `INTERVAL`. It runs for the life of the process.
pub fn spawn_scheduler(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("rm-backup".into())
        .spawn(move || loop {
            std::thread::sleep(INTERVAL);
            let state = app.state::<AppState>();
            if let Err(err) = take_if_changed(&state, Trigger::Scheduled) {
                log::error!("scheduled backup failed: {err}");
            }
        });

    if let Err(err) = spawned {
        log::error!("could not start the backup scheduler: {err}");
    }
}

// ---------------------------------------------------------------------------
// Retention
// ---------------------------------------------------------------------------

fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let mut files: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|entry| is_snapshot(&entry.path()))
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));

    let times: Vec<SystemTime> = files.iter().map(|(t, _)| *t).collect();
    for index in to_remove(&times, SystemTime::now()) {
        let _ = std::fs::remove_file(&files[index].1);
    }
}

/// Which snapshots to delete, given their modification times newest first.
/// Keeps the newest `KEEP_NEWEST`, then the newest of each day for `KEEP_DAYS`.
fn to_remove(newest_first: &[SystemTime], now: SystemTime) -> Vec<usize> {
    let now: DateTime<Local> = now.into();
    let mut days_kept = HashSet::new();
    let mut remove = Vec::new();

    for (index, time) in newest_first.iter().enumerate() {
        let when: DateTime<Local> = (*time).into();
        let day = when.date_naive();
        if index < KEEP_NEWEST {
            days_kept.insert(day);
            continue;
        }
        let recent = (now.date_naive() - day).num_days() < KEEP_DAYS;
        if recent && days_kept.insert(day) {
            continue;
        }
        remove.push(index);
    }

    remove
}

fn is_snapshot(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rmdb"))
}

// ---------------------------------------------------------------------------
// Listing and inspecting snapshots
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    pub path: String,
    pub file_name: String,
    /// "local" or "mirror".
    pub location: String,
    pub bytes: u64,
    pub modified_at: String,
}

/// Every snapshot RM can see, newest first: the local folder, then the second
/// drive if it is plugged in.
pub fn list(state: &AppState) -> AppResult<Vec<BackupFile>> {
    let mirror_dir = {
        let conn = state.db.lock();
        repo::get_setting(&conn, "backup.mirror_path")?
    }
    .map(|dir| Path::new(&dir).join(MIRROR_SUBDIR));

    let mut found = Vec::new();
    collect(&state.paths.backup_dir, "local", &mut found);
    if let Some(dir) = &mirror_dir {
        collect(dir, "mirror", &mut found);
    }
    found.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(found)
}

fn collect(dir: &Path, location: &str, into: &mut Vec<BackupFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_snapshot(&path) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else { continue };
        into.push(BackupFile {
            file_name: entry.file_name().to_string_lossy().into_owned(),
            path: path.display().to_string(),
            location: location.to_string(),
            bytes: meta.len(),
            modified_at: DateTime::<Utc>::from(modified).to_rfc3339(),
        });
    }
}

/// What a snapshot holds, so the person restoring it can see they have the
/// right one before anything is replaced.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub path: String,
    pub institution_name: Option<String>,
    pub schema_version: i64,
    pub learners: i64,
    pub marks: i64,
    pub last_activity_at: Option<String>,
    /// False when the snapshot was made by a newer version of RM than this one.
    pub compatible: bool,
}

pub fn inspect(path: &Path) -> AppResult<BackupSummary> {
    let not_a_backup = || {
        AppError::validation("That file is not a Results Manager backup, or it is damaged.")
    };

    if !path.is_file() {
        return Err(AppError::validation("That backup file could not be found."));
    }
    let conn = db::open_snapshot(path).map_err(|_| not_a_backup())?;
    summarise(&conn, path).map_err(|_| not_a_backup())
}

fn summarise(conn: &Connection, path: &Path) -> AppResult<BackupSummary> {
    let ok: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if ok != "ok" {
        return Err(AppError::validation("damaged"));
    }

    let schema_version: i64 =
        conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))?;
    let institution_name: Option<String> = conn
        .query_row("SELECT name FROM institution WHERE id = 1", [], |r| r.get(0))
        .optional()?;
    let learners: i64 = conn.query_row(
        "SELECT COUNT(*) FROM students WHERE status = 'active'",
        [],
        |r| r.get(0),
    )?;
    let marks: i64 = conn.query_row("SELECT COUNT(*) FROM marks", [], |r| r.get(0))?;
    let last_activity_at: Option<String> =
        conn.query_row("SELECT MAX(at) FROM audit_log", [], |r| r.get(0))?;

    let newest_known = migrations::MIGRATIONS.last().map(|m| m.version).unwrap_or(0);

    Ok(BackupSummary {
        path: path.display().to_string(),
        institution_name,
        schema_version,
        learners,
        marks,
        last_activity_at,
        compatible: schema_version <= newest_known,
    })
}

// ---------------------------------------------------------------------------
// Restore
// ---------------------------------------------------------------------------

/// Replaces the school's data with the snapshot at `path`.
///
/// The current data is snapshotted first, the second-drive setting survives
/// the restore (it describes this PC, not the data), the restore is audited in
/// the restored database, and everyone is signed out — the accounts in the
/// snapshot may not be the ones signed in now.
pub fn restore(state: &AppState, session: &Session, path: &Path) -> AppResult<BackupSummary> {
    if same_file(path, &state.paths.db_file) {
        return Err(AppError::validation("That is the live database, not a backup."));
    }

    let summary = inspect(path)?;
    if !summary.compatible {
        return Err(AppError::validation(
            "That backup was made by a newer version of Results Manager. Update RM on this computer first.",
        ));
    }

    let safety = take(state, Some(session), Trigger::BeforeRestore)?;

    let mirror_path = {
        let conn = state.db.lock();
        repo::get_setting(&conn, "backup.mirror_path")?
    };

    state.db.restore_from(path)?;

    {
        let conn = state.db.lock();
        match &mirror_path {
            Some(value) => repo::set_setting(&conn, "backup.mirror_path", value)?,
            None => {
                conn.execute("DELETE FROM settings WHERE key = 'backup.mirror_path'", [])?;
            }
        }
        // The restored settings remember the snapshot's own last backup; the
        // newest one is the safety copy just taken.
        repo::set_setting(&conn, "backup.last_at", &safety.at)?;
        repo::set_setting(&conn, "backup.last_path", &safety.local_path)?;
        audit::record(
            &conn,
            Some(session),
            "backup.restore",
            "backup",
            &summary.path,
            format!(
                "Restored the school's data from {}. The data it replaced was saved to {}",
                summary.path, safety.local_path
            ),
        )?;
        state.mark_backed_up(conn.total_changes());
    }

    state.sessions.sign_out();
    Ok(summary)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn days_ago(days: u64, hours: u64) -> SystemTime {
        SystemTime::now() - Duration::from_secs(days * 86_400 + hours * 3_600)
    }

    #[test]
    fn the_newest_snapshots_are_always_kept() {
        let times: Vec<SystemTime> = (0..KEEP_NEWEST as u64).map(|i| days_ago(0, i)).collect();
        assert!(to_remove(&times, SystemTime::now()).is_empty());
    }

    #[test]
    fn older_snapshots_keep_one_per_day() {
        // 20 recent ones today, then three snapshots on each of two older days.
        let mut times: Vec<SystemTime> = (0..KEEP_NEWEST as u64).map(|_| days_ago(0, 0)).collect();
        times.extend([days_ago(3, 1), days_ago(3, 2), days_ago(3, 3)]);
        times.extend([days_ago(4, 1), days_ago(4, 2)]);

        let removed = to_remove(&times, SystemTime::now());
        assert_eq!(removed.len(), 3, "keeps one of each older day");
        assert!(!removed.contains(&KEEP_NEWEST), "keeps the newest of day 3");
        assert!(!removed.contains(&(KEEP_NEWEST + 3)), "keeps the newest of day 4");
    }

    #[test]
    fn snapshots_older_than_the_window_go() {
        let mut times: Vec<SystemTime> = (0..KEEP_NEWEST as u64).map(|_| days_ago(0, 0)).collect();
        times.push(days_ago(KEEP_DAYS as u64 + 5, 0));
        assert_eq!(to_remove(&times, SystemTime::now()), vec![KEEP_NEWEST]);
    }

    #[test]
    fn a_snapshot_round_trips_and_can_be_inspected() {
        let dir = std::env::temp_dir().join(format!("rm-backup-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let live = db::Database::open(&dir.join("school.rmdb")).expect("open");
        {
            let conn = live.lock();
            conn.execute(
                "INSERT INTO institution (id, name, created_at, updated_at) VALUES (1, 'Rainbow', 'now', 'now')",
                [],
            )
            .expect("institution");
        }

        let snapshot = dir.join("snap.rmdb");
        live.backup_to(&snapshot).expect("backup");

        let summary = inspect(&snapshot).expect("inspect");
        assert_eq!(summary.institution_name.as_deref(), Some("Rainbow"));
        assert!(summary.compatible);

        // Change the live data, then restore the snapshot over it.
        live.lock()
            .execute("UPDATE institution SET name = 'Changed' WHERE id = 1", [])
            .expect("update");
        live.restore_from(&snapshot).expect("restore");
        let name: String = live
            .lock()
            .query_row("SELECT name FROM institution WHERE id = 1", [], |r| r.get(0))
            .expect("name");
        assert_eq!(name, "Rainbow");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_is_not_a_backup_is_refused() {
        let path = std::env::temp_dir().join(format!("rm-not-a-backup-{}.rmdb", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"this is not a database").unwrap();
        assert!(inspect(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
