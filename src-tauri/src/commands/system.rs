//! Institution identity, the audit screen, backups, and the admin dashboard.
//!
//! FR-B2, FR-B9, FR-B10, FR-E2, and the simplified backup from SRS 16.1.

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit::{self, AuditEntry};
use crate::backup::{self, BackupFile, BackupResult, BackupSummary, Trigger};
use crate::domain::models::Institution;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const MAX_LOGO_BYTES: usize = 2 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Institution identity (FR-B2)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_institution(state: State<'_, AppState>) -> AppResult<Institution> {
    state.sessions.require()?;
    let conn = state.db.lock();
    Ok(conn.query_row(
        "SELECT name, motto, LENGTH(logo_png) AS logo_len, accent_color, address, phone, email,
                reg_number_pattern, setup_completed_at
         FROM institution WHERE id = 1",
        [],
        Institution::from_row,
    )?)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstitutionRequest {
    pub name: String,
    #[serde(default)]
    pub motto: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    pub accent_color: String,
}

#[tauri::command]
pub fn update_institution(
    state: State<'_, AppState>,
    request: UpdateInstitutionRequest,
) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    // FR-B2: the name is compulsory. The logo never is.
    if request.name.trim().is_empty() {
        return Err(AppError::validation("The school's name is required."));
    }
    if !is_hex_color(&request.accent_color) {
        return Err(AppError::validation(
            "The accent colour must be a hex value such as #4F63D2.",
        ));
    }

    let conn = state.db.lock();
    conn.execute(
        "UPDATE institution
         SET name = ?1, motto = ?2, address = ?3, phone = ?4, email = ?5,
             accent_color = ?6, updated_at = ?7
         WHERE id = 1",
        params![
            request.name.trim(),
            request.motto.as_deref().map(str::trim),
            request.address.as_deref().map(str::trim),
            request.phone.as_deref().map(str::trim),
            request.email.as_deref().map(str::trim),
            request.accent_color.trim(),
            Utc::now().to_rfc3339(),
        ],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "institution.update",
        "institution",
        "1",
        "Updated the school's identity",
    )?;

    Ok(())
}

#[tauri::command]
pub fn set_institution_logo(state: State<'_, AppState>, png: Option<Vec<u8>>) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    if let Some(bytes) = &png {
        if bytes.len() > MAX_LOGO_BYTES {
            return Err(AppError::validation(
                "That logo is larger than 2 MB. Please use a smaller one.",
            ));
        }
        if !bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Err(AppError::validation("That file is not a PNG image."));
        }
    }

    let conn = state.db.lock();
    conn.execute(
        "UPDATE institution SET logo_png = ?1, updated_at = ?2 WHERE id = 1",
        params![png, Utc::now().to_rfc3339()],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "institution.logo",
        "institution",
        "1",
        if png.is_some() {
            "Set the school logo"
        } else {
            "Removed the school logo"
        },
    )?;

    Ok(())
}

#[tauri::command]
pub fn get_institution_logo(state: State<'_, AppState>) -> AppResult<Option<Vec<u8>>> {
    let conn = state.db.lock();
    Ok(conn
        .query_row(
            "SELECT logo_png FROM institution WHERE id = 1",
            [],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .unwrap_or(None))
}

// ---------------------------------------------------------------------------
// Preferences
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_theme(state: State<'_, AppState>) -> AppResult<String> {
    let conn = state.db.lock();
    Ok(repo::get_setting(&conn, "ui.theme")?.unwrap_or_else(|| "system".into()))
}

#[tauri::command]
pub fn set_theme(state: State<'_, AppState>, theme: String) -> AppResult<()> {
    if !matches!(theme.as_str(), "light" | "dark" | "system") {
        return Err(AppError::validation("Unknown theme."));
    }
    let conn = state.db.lock();
    repo::set_setting(&conn, "ui.theme", &theme)
}

// ---------------------------------------------------------------------------
// Audit trail (FR-E2)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn search_audit_log(
    state: State<'_, AppState>,
    query: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> AppResult<Vec<AuditEntry>> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let conn = state.db.lock();
    audit::search(
        &conn,
        query.as_deref(),
        limit.unwrap_or(100),
        offset.unwrap_or(0),
    )
}

// ---------------------------------------------------------------------------
// Backup (simplified FR-B12, per SRS 16.1)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub last_backup_at: Option<String>,
    pub last_backup_path: Option<String>,
    pub mirror_path: Option<String>,
    pub local_backup_dir: String,
    /// Cloud backup is built but switched off at launch (FR-W8, FR-B10).
    pub cloud_enabled: bool,
}

#[tauri::command]
pub fn backup_status(state: State<'_, AppState>) -> AppResult<BackupStatus> {
    state.sessions.require()?;
    let conn = state.db.lock();

    Ok(BackupStatus {
        last_backup_at: repo::get_setting(&conn, "backup.last_at")?,
        last_backup_path: repo::get_setting(&conn, "backup.last_path")?,
        mirror_path: repo::get_setting(&conn, "backup.mirror_path")?,
        local_backup_dir: state.paths.backup_dir.display().to_string(),
        cloud_enabled: false,
    })
}

/// Points RM at the second drive. SRS 16.1 replaces the transactional mirror
/// with a scheduled copy for the pilot; this is where that copy goes.
#[tauri::command]
pub fn set_mirror_path(state: State<'_, AppState>, path: Option<String>) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let conn = state.db.lock();
    match path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(value) => {
            let target = std::path::Path::new(value);
            if !target.is_dir() {
                return Err(AppError::validation(
                    "That folder could not be found. Plug in the drive and choose it again.",
                ));
            }
            repo::set_setting(&conn, "backup.mirror_path", value)?;
            audit::record(
                &conn,
                Some(&session),
                "backup.mirror_path",
                "setting",
                "backup.mirror_path",
                format!("Set the backup drive to {value}"),
            )?;
        }
        None => {
            conn.execute(
                "DELETE FROM settings WHERE key = 'backup.mirror_path'",
                [],
            )?;
            audit::record(
                &conn,
                Some(&session),
                "backup.mirror_path",
                "setting",
                "backup.mirror_path",
                "Cleared the backup drive",
            )?;
        }
    }

    Ok(())
}

/// Takes a consistent snapshot of the live database, locally and — if a second
/// drive is configured — onto that drive too.
#[tauri::command]
pub fn run_backup(state: State<'_, AppState>) -> AppResult<BackupResult> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    backup::take(&state, Some(&session), Trigger::Manual)
}

/// Every snapshot on this computer and on the second drive, newest first.
#[tauri::command]
pub fn list_backups(state: State<'_, AppState>) -> AppResult<Vec<BackupFile>> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    backup::list(&state)
}

/// What a snapshot holds, shown before anyone restores it.
#[tauri::command]
pub fn inspect_backup(state: State<'_, AppState>, path: String) -> AppResult<BackupSummary> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    backup::inspect(std::path::Path::new(&path))
}

/// Replaces the school's data with a snapshot. The current data is saved
/// first, and everyone is signed out afterwards.
#[tauri::command]
pub fn restore_backup(state: State<'_, AppState>, path: String) -> AppResult<BackupSummary> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    backup::restore(&state, &session, std::path::Path::new(&path))
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSummary {
    pub institution_name: String,
    pub academic_year: Option<String>,
    pub current_term: Option<String>,
    pub learners: i64,
    pub boys: i64,
    pub girls: i64,
    pub classes: i64,
    pub teachers: i64,
    pub subjects: i64,
    pub marks_entered_this_term: i64,
    pub marks_expected_this_term: i64,
    pub last_backup_at: Option<String>,
    pub recent_activity: Vec<AuditEntry>,
    /// FR-G22: learners out on a pass-out right now, and those overdue.
    pub pass_outs_out: i64,
    pub overdue_pass_outs: Vec<crate::commands::passouts::OverduePassOut>,
}

#[tauri::command]
pub fn dashboard_summary(state: State<'_, AppState>) -> AppResult<DashboardSummary> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();

    let institution_name: String =
        conn.query_row("SELECT name FROM institution WHERE id = 1", [], |row| {
            row.get(0)
        })?;

    let year_id = repo::current_academic_year_id(&conn)?;
    let term_id = repo::current_term_id(&conn)?;

    let academic_year: Option<String> = match &year_id {
        Some(id) => conn
            .query_row(
                "SELECT label FROM academic_years WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .ok(),
        None => None,
    };

    let current_term: Option<String> = match &term_id {
        Some(id) => conn
            .query_row("SELECT name FROM terms WHERE id = ?1", params![id], |row| {
                row.get(0)
            })
            .ok(),
        None => None,
    };

    let (learners, boys, girls): (i64, i64, i64) = conn.query_row(
        "SELECT COUNT(*),
                SUM(CASE WHEN s.gender = 'M' THEN 1 ELSE 0 END),
                SUM(CASE WHEN s.gender = 'F' THEN 1 ELSE 0 END)
         FROM enrollments e JOIN students s ON s.id = e.student_id
         WHERE e.status = 'active' AND (?1 IS NULL OR e.academic_year_id = ?1)",
        params![year_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            ))
        },
    )?;

    let classes: i64 = conn.query_row(
        "SELECT COUNT(*) FROM classes WHERE status = 'active'",
        [],
        |row| row.get(0),
    )?;
    let teachers: i64 = conn.query_row(
        "SELECT COUNT(*) FROM users WHERE role = 'teacher' AND status = 'active'",
        [],
        |row| row.get(0),
    )?;
    let subjects: i64 = conn.query_row(
        "SELECT COUNT(*) FROM subjects WHERE status = 'active'",
        [],
        |row| row.get(0),
    )?;

    // How full the current term's marks are: entered against what the roster
    // and subject list say should exist.
    let (marks_entered, marks_expected): (i64, i64) = match (&term_id, &year_id) {
        (Some(term), Some(year)) => {
            let entered: i64 = conn.query_row(
                "SELECT COUNT(*) FROM marks m
                 JOIN exams e ON e.id = m.exam_id
                 WHERE e.term_id = ?1",
                params![term],
                |row| row.get(0),
            )?;
            let expected: i64 = conn.query_row(
                "SELECT COALESCE(SUM(cnt), 0) FROM (
                    SELECT (SELECT COUNT(*) FROM enrollments en
                             WHERE en.class_id = cs.class_id
                               AND en.academic_year_id = ?2
                               AND en.status = 'active')
                           * (SELECT COUNT(*) FROM exams ex WHERE ex.term_id = ?1) AS cnt
                    FROM class_subjects cs WHERE cs.status = 'active'
                 )",
                params![term, year],
                |row| row.get(0),
            )?;
            (entered, expected)
        }
        _ => (0, 0),
    };

    // Teachers see the school's shape but not its audit trail.
    let recent_activity = if session.is_admin() {
        audit::search(&conn, None, 8, 0)?
    } else {
        Vec::new()
    };

    Ok(DashboardSummary {
        institution_name,
        academic_year,
        current_term,
        learners,
        boys,
        girls,
        classes,
        teachers,
        subjects,
        marks_entered_this_term: marks_entered,
        marks_expected_this_term: marks_expected,
        last_backup_at: repo::get_setting(&conn, "backup.last_at")?,
        recent_activity,
        pass_outs_out: conn.query_row(
            "SELECT COUNT(*) FROM pass_outs WHERE status = 'out'",
            [],
            |row| row.get(0),
        )?,
        overdue_pass_outs: crate::commands::passouts::overdue(&conn)?,
    })
}

fn is_hex_color(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.len() == 7
        && trimmed.starts_with('#')
        && trimmed[1..].chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_are_checked() {
        assert!(is_hex_color("#4F63D2"));
        assert!(is_hex_color("#000000"));
        assert!(!is_hex_color("4F63D2"));
        assert!(!is_hex_color("#4F63D"));
        assert!(!is_hex_color("#GGGGGG"));
        assert!(!is_hex_color("red"));
    }
}
