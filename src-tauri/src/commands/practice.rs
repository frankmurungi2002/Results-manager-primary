//! The practice school: a made-up school, full of learners, marks and money,
//! for training staff, showing Termbook to a headteacher, and testing.
//!
//! The setup wizard offers it before a school sets itself up. The interface
//! fills it through the same commands a real school uses, so practising
//! exercises the real thing. While it is a practice school:
//!
//! * it is marked as one, and every screen says so;
//! * it is offline only and cannot be switched back, so no SMS ever reaches
//!   the made-up parents' numbers;
//! * "Start my real school" clears it (a backup is kept first) and opens the
//!   setup wizard again.

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::audit;
use crate::backup::{self, Trigger};
use crate::db::Database;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::network;
use crate::security::Role;
use crate::state::AppState;

const KEY: &str = "practice.school";

pub fn is_practice(conn: &Connection) -> AppResult<bool> {
    Ok(repo::get_setting(conn, KEY)?.as_deref() == Some("1"))
}

/// Marks a newly set-up school as the practice school, before the interface
/// fills it. Only for a school with no learners yet.
pub fn begin_in(conn: &Connection, role: Role, session: Option<&crate::security::Session>) -> AppResult<()> {
    if role != Role::SchoolAdmin {
        return Err(AppError::Forbidden);
    }
    let learners: i64 = conn.query_row("SELECT COUNT(*) FROM students", [], |r| r.get(0))?;
    if learners > 0 {
        return Err(AppError::validation("This school already has learners, so it cannot become the practice school."));
    }
    repo::set_setting(conn, KEY, "1")?;
    network::set_offline_only(conn, true)?;
    audit::record(conn, session, "practice.begin", "school", "practice", "Made this the practice school (offline only, nothing here is real)")?;
    Ok(())
}

#[tauri::command]
pub fn begin_practice_school(state: State<'_, AppState>) -> AppResult<()> {
    let session = state.sessions.require()?;
    begin_in(&state.db.lock(), session.role, Some(&session))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PracticeStatus {
    pub practice: bool,
}

/// Asked before anyone signs in, so the sign-in screen can say so.
#[tauri::command]
pub fn practice_status(state: State<'_, AppState>) -> AppResult<PracticeStatus> {
    Ok(PracticeStatus { practice: is_practice(&state.db.lock())? })
}

/// Clears the practice school and opens the setup wizard. The practice data
/// is backed up first, like any restore.
#[tauri::command]
pub fn start_real_school(state: State<'_, AppState>, confirm: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    if session.role != Role::SchoolAdmin {
        return Err(AppError::Forbidden);
    }
    if !is_practice(&state.db.lock())? {
        return Err(AppError::validation("This is a real school. Only the practice school can be cleared."));
    }
    if confirm.trim().to_uppercase() != "START" {
        return Err(AppError::validation("Type START to confirm."));
    }
    backup::take(&state, Some(&session), Trigger::BeforeRestore)?;
    // An empty database with every migration applied, copied over the live one.
    let fresh = state.paths.export_dir.join("fresh-school.rmdb");
    let _ = std::fs::remove_file(&fresh);
    {
        let empty = Database::open(&fresh)?;
        drop(empty);
    }
    let result = state.db.restore_from(&fresh);
    let _ = std::fs::remove_file(&fresh);
    for suffix in ["-wal", "-shm"] {
        let mut side = fresh.clone().into_os_string();
        side.push(suffix);
        let _ = std::fs::remove_file(side);
    }
    result?;
    state.sessions.sign_out();
    log::info!("practice school cleared; the setup wizard opens next");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_practice_school_is_offline_only_and_needs_an_empty_school() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.lock();
        assert!(!is_practice(&conn).unwrap());
        assert!(matches!(begin_in(&conn, Role::Teacher, None), Err(AppError::Forbidden)));
        begin_in(&conn, Role::SchoolAdmin, None).unwrap();
        assert!(is_practice(&conn).unwrap());
        assert!(network::offline_only(&conn).unwrap());

        let db = crate::fees::tests::fee_school();
        let conn = db.lock();
        assert!(begin_in(&conn, Role::SchoolAdmin, None).is_err(), "a school with learners is not made practice");
    }
}
