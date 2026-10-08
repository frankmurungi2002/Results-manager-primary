//! Institution identity, the audit screen, backups, and the admin dashboard.
//!
//! FR-B2, FR-B9, FR-B10, FR-E2, and the simplified backup from SRS 16.1.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit::{self, AuditEntry, ChainReport, CodeCheck};
use crate::commands::reports::DocumentEnvelope;
use crate::backup::{self, BackupFile, BackupResult, BackupSummary, Trigger};
use crate::domain::models::Institution;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::security::{Permission, Session};

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
    session.require(Permission::ManageSchool)?;

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
    session.require(Permission::ManageSchool)?;

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

// ---------------------------------------------------------------------------
// Sign-in pictures: the school's own photos for the sign-in screen
// ---------------------------------------------------------------------------

const LOGIN_IMAGE_SLOTS: usize = 4;
const MAX_LOGIN_IMAGE_BYTES: u64 = 4 * 1024 * 1024;

/// The four pictures the sign-in screen rotates through, as data URLs; None
/// where the school has not chosen one and the built-in picture shows.
/// Read before anyone signs in, so it needs no session.
#[tauri::command]
pub fn get_login_images(state: State<'_, AppState>) -> AppResult<Vec<Option<String>>> {
    let conn = state.db.lock();
    (1..=LOGIN_IMAGE_SLOTS)
        .map(|slot| repo::get_setting(&conn, &format!("login.image.{slot}")))
        .collect()
}

/// Sets one sign-in picture from a PNG or JPEG file, or clears it (`path`
/// None) so the built-in picture shows again.
#[tauri::command]
pub fn set_login_image(state: State<'_, AppState>, slot: usize, path: Option<String>) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageSchool)?;
    if !(1..=LOGIN_IMAGE_SLOTS).contains(&slot) {
        return Err(AppError::validation("There are four sign-in pictures."));
    }

    let conn = state.db.lock();
    let key = format!("login.image.{slot}");
    let Some(path) = path else {
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        audit::record(&conn, Some(&session), "login.image", "setting", &key, "Removed a sign-in picture")?;
        return Ok(());
    };

    let file = std::path::Path::new(&path);
    let size = std::fs::metadata(file).map(|m| m.len()).map_err(|_| AppError::not_found("That file"))?;
    if size > MAX_LOGIN_IMAGE_BYTES {
        return Err(AppError::validation("That picture is larger than 4 MB. Please use a smaller one."));
    }
    let bytes = std::fs::read(file)?;
    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else {
        return Err(AppError::validation("That file is not a PNG or JPEG picture."));
    };
    let data_url = format!("data:{mime};base64,{}", crate::commands::reports::base64_encode(&bytes));
    repo::set_setting(&conn, &key, &data_url)?;
    audit::record(&conn, Some(&session), "login.image", "setting", &key, "Set a sign-in picture")?;
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
    session.require(Permission::ViewAudit)?;

    let conn = state.db.lock();
    audit::search(
        &conn,
        query.as_deref(),
        limit.unwrap_or(100),
        offset.unwrap_or(0),
    )
}

/// Whether the activity log is intact, and the code a printout made now
/// would carry.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditIntegrity {
    pub report: ChainReport,
    pub current_code: String,
}

#[tauri::command]
pub fn audit_integrity(state: State<'_, AppState>) -> AppResult<AuditIntegrity> {
    let session = state.sessions.require()?;
    session.require(Permission::ViewAudit)?;
    let conn = state.db.lock();
    let report = audit::verify_chain(&conn)?;
    let current_code = audit::verification_code(report.head_entry, &report.head_hash);
    Ok(AuditIntegrity { report, current_code })
}

/// The printed activity log: entries in order, the state of the chain, and
/// the verification code that lets anyone check later that history has not
/// been rewritten since this sheet was printed.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditExport {
    pub range_label: String,
    pub search: Option<String>,
    pub entries: Vec<AuditEntry>,
    /// More entries matched than one printout holds.
    pub truncated: bool,
    pub report: ChainReport,
    pub verification_code: String,
    pub printed_by: String,
}

const EXPORT_LIMIT: i64 = 3000;

#[tauri::command]
pub fn build_audit_export(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    query: Option<String>,
) -> AppResult<DocumentEnvelope<AuditExport>> {
    let session = state.sessions.require()?;
    session.require(Permission::ViewAudit)?;
    let conn = state.db.lock();

    let day = |d: &Option<String>| d.as_deref().map(str::trim).filter(|d| d.len() == 10).map(str::to_string);
    let (from, to) = (day(&from), day(&to));
    let pattern = query.as_deref().map(str::trim).filter(|q| q.len() > 2).map(|q| format!("%{q}%"));

    let range_label = match (&from, &to) {
        (Some(f), Some(t)) => format!("{f} to {t}"),
        (Some(f), None) => format!("From {f}"),
        (None, Some(t)) => format!("Up to {t}"),
        (None, None) => "Every entry".to_string(),
    };

    // The printing is itself recorded first, so the code below covers it.
    audit::record(
        &conn,
        Some(&session),
        "audit.export",
        "audit_log",
        "-",
        format!("Printed the activity log ({range_label})"),
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, at, actor_name, actor_role, action, entity, entity_id, summary
         FROM audit_log
         WHERE (?1 IS NULL OR substr(at, 1, 10) >= ?1)
           AND (?2 IS NULL OR substr(at, 1, 10) <= ?2)
           AND (?3 IS NULL OR summary LIKE ?3 COLLATE NOCASE OR actor_name LIKE ?3 COLLATE NOCASE
                OR action LIKE ?3 COLLATE NOCASE)
         ORDER BY id ASC
         LIMIT ?4",
    )?;
    let mut entries = stmt
        .query_map(params![from, to, pattern, EXPORT_LIMIT + 1], |row| {
            Ok(AuditEntry {
                id: row.get(0)?,
                at: row.get(1)?,
                actor_name: row.get(2)?,
                actor_role: row.get(3)?,
                action: row.get(4)?,
                entity: row.get(5)?,
                entity_id: row.get(6)?,
                summary: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let truncated = entries.len() as i64 > EXPORT_LIMIT;
    entries.truncate(EXPORT_LIMIT as usize);

    let report = audit::verify_chain(&conn)?;
    let verification_code = audit::verification_code(report.head_entry, &report.head_hash);
    let body = AuditExport {
        range_label,
        search: query.filter(|q| q.trim().len() > 2),
        entries,
        truncated,
        report,
        verification_code,
        printed_by: session.full_name.clone(),
    };
    crate::commands::reports::envelope(&conn, "audit_log", body)
}

/// Checks a code copied from a printed activity log.
#[tauri::command]
pub fn verify_audit_code(state: State<'_, AppState>, code: String) -> AppResult<CodeCheck> {
    let session = state.sessions.require()?;
    session.require(Permission::ViewAudit)?;
    let conn = state.db.lock();
    let check = audit::check_code(&conn, &code)?;
    audit::record(
        &conn,
        Some(&session),
        "audit.verify",
        "audit_log",
        &check.entry.to_string(),
        format!(
            "Checked a printed activity log up to entry {}: {}",
            check.entry,
            if check.matches { "it matches" } else { "it does NOT match" }
        ),
    )?;
    Ok(check)
}

// ---------------------------------------------------------------------------
// FR-B11 — the subscription
// ---------------------------------------------------------------------------

/// The subscription's state, re-checked against today. Asked before sign-in
/// too, so the sign-in screen can say a school is read-only; it reveals
/// nothing but the school code, which is not secret.
#[tauri::command]
pub fn licence_status(state: State<'_, AppState>) -> AppResult<crate::licence::LicenceStatus> {
    let conn = state.db.lock();
    crate::licence::refresh(&conn)
}

/// Installs a key the owner sent. Works while read-only: that is how a
/// school gets out of it.
#[tauri::command]
pub fn install_licence_key(state: State<'_, AppState>, key: String) -> AppResult<crate::licence::LicenceStatus> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageSchool)?;
    let conn = state.db.lock();
    let claims = crate::licence::install(&conn, &key)?;
    audit::record(
        &conn,
        Some(&session),
        "licence.install",
        "licence",
        &claims.id,
        format!(
            "Installed licence key {} ({} plan, {} to {})",
            claims.id, claims.plan, claims.from, claims.until
        ),
    )?;
    crate::licence::refresh(&conn)
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

/// Points Phantom School Manager at the second drive. SRS 16.1 replaces the transactional mirror
/// with a scheduled copy for the pilot; this is where that copy goes.
#[tauri::command]
pub fn set_mirror_path(state: State<'_, AppState>, path: Option<String>) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageBackups)?;

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
    session.require(Permission::ManageBackups)?;
    backup::take(&state, Some(&session), Trigger::Manual)
}

/// Every snapshot on this computer and on the second drive, newest first.
#[tauri::command]
pub fn list_backups(state: State<'_, AppState>) -> AppResult<Vec<BackupFile>> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageBackups)?;
    backup::list(&state)
}

/// What a snapshot holds, shown before anyone restores it.
#[tauri::command]
pub fn inspect_backup(state: State<'_, AppState>, path: String) -> AppResult<BackupSummary> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageBackups)?;
    backup::inspect(std::path::Path::new(&path))
}

/// Replaces the school's data with a snapshot. The current data is saved
/// first, and everyone is signed out afterwards.
#[tauri::command]
pub fn restore_backup(state: State<'_, AppState>, path: String) -> AppResult<BackupSummary> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageBackups)?;
    let summary = backup::restore(&state, &session, std::path::Path::new(&path))?;
    // The restored database carries its own subscription.
    crate::licence::refresh(&state.db.lock())?;
    Ok(summary)
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
    /// FR-G22: learners out on a pass-out right now, and those overdue.
    pub pass_outs_out: i64,
    pub overdue_pass_outs: Vec<crate::commands::passouts::OverduePassOut>,
    /// What each office needs on its own dashboard. Every field is filled only
    /// for someone allowed to see it, so a bursar's dashboard never carries
    /// a class's marks.
    pub desk: Desk,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Desk {
    /// Learners under the fees block (whoever manages fees).
    pub fees_blocked: Option<i64>,
    /// The classes furthest behind with this term's marks (academic offices).
    pub marks_behind: Vec<ClassMarks>,
    /// Registers taken today across the school (school-wide view).
    pub registers_today: Option<RegistersToday>,
    /// The signed-in teacher's own classes with no register yet today.
    pub my_registers_pending: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassMarks {
    pub class_id: String,
    pub class_name: String,
    pub entered: i64,
    pub expected: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistersToday {
    pub taken: i64,
    pub classes: i64,
}

fn desk(conn: &Connection, session: &Session, term_id: &Option<String>, year_id: &Option<String>) -> AppResult<Desk> {
    let mut desk = Desk::default();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    if session.has(Permission::ManageFees) {
        desk.fees_blocked = Some(conn.query_row(
            "SELECT COUNT(*) FROM students WHERE status = 'active' AND fees_blocked = 1",
            [],
            |row| row.get(0),
        )?);
    }

    if session.has(Permission::ManageAcademics) || session.has(Permission::EditAllMarks) {
        if let (Some(term), Some(year)) = (term_id, year_id) {
            let mut stmt = conn.prepare(
                "SELECT c.id, c.name,
                        (SELECT COUNT(*) FROM marks m
                          JOIN class_subjects cs ON cs.id = m.class_subject_id
                          JOIN exams e ON e.id = m.exam_id
                          WHERE cs.class_id = c.id AND e.term_id = ?1),
                        (SELECT COUNT(*) FROM enrollments en
                          WHERE en.class_id = c.id AND en.academic_year_id = ?2 AND en.status = 'active')
                        * (SELECT COUNT(*) FROM class_subjects cs WHERE cs.class_id = c.id AND cs.status = 'active')
                        * (SELECT COUNT(*) FROM exams ex WHERE ex.term_id = ?1)
                 FROM classes c WHERE c.status = 'active'",
            )?;
            let mut rows = stmt
                .query_map(params![term, year], |row| {
                    Ok(ClassMarks {
                        class_id: row.get(0)?,
                        class_name: row.get(1)?,
                        entered: row.get(2)?,
                        expected: row.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows.retain(|c| c.expected > 0 && c.entered < c.expected);
            rows.sort_by(|a, b| {
                let share = |c: &ClassMarks| c.entered as f64 / c.expected as f64;
                share(a).total_cmp(&share(b))
            });
            rows.truncate(5);
            desk.marks_behind = rows;
        }
    }

    if session.has(Permission::ViewAllClasses) {
        let taken: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT class_id) FROM attendance WHERE on_date = ?1",
            params![today],
            |row| row.get(0),
        )?;
        let classes: i64 = conn.query_row(
            "SELECT COUNT(*) FROM classes WHERE status = 'active'",
            [],
            |row| row.get(0),
        )?;
        desk.registers_today = Some(RegistersToday { taken, classes });
    }

    if !session.class_teacher_of.is_empty() {
        let mut stmt = conn.prepare(
            "SELECT c.name FROM classes c
             WHERE c.id = ?1 AND NOT EXISTS (
                 SELECT 1 FROM attendance a WHERE a.class_id = c.id AND a.on_date = ?2)",
        )?;
        let mut pending = Vec::new();
        let mut mine: Vec<&String> = session.class_teacher_of.iter().collect();
        mine.sort();
        for class_id in mine {
            if let Some(name) = stmt
                .query_row(params![class_id, today], |row| row.get::<_, String>(0))
                .optional()?
            {
                pending.push(name);
            }
        }
        desk.my_registers_pending = pending;
    }

    Ok(desk)
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
        pass_outs_out: conn.query_row(
            "SELECT COUNT(*) FROM pass_outs WHERE status = 'out'",
            [],
            |row| row.get(0),
        )?,
        overdue_pass_outs: crate::commands::passouts::overdue(&conn)?,
        desk: desk(&conn, &session, &term_id, &year_id)?,
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
