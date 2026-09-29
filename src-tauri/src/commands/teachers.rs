//! Teacher and School Admin accounts, and their class/subject assignments.
//!
//! FR-B4 (up to three School Admins), FR-B6 (one Class Teacher and at most one
//! Assistant per class; everyone else is a Subject Teacher).

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::domain::ids::{new_id, prefix};
use crate::domain::models::UserSummary;
use crate::error::{AppError, AppResult};
use crate::security::password;
use crate::state::AppState;

const MAX_SCHOOL_ADMINS: i64 = 3;
const MAX_PHOTO_BYTES: usize = 2 * 1024 * 1024;

/// A staff member's photo for their ID card (FR-G16). `None` removes it.
#[tauri::command]
pub fn set_staff_photo(
    state: State<'_, AppState>,
    user_id: String,
    png: Option<Vec<u8>>,
) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    if let Some(bytes) = &png {
        if bytes.len() > MAX_PHOTO_BYTES {
            return Err(AppError::validation(
                "That photo is larger than 2 MB. Please use a smaller one.",
            ));
        }
        if !bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Err(AppError::validation("That file is not a PNG image."));
        }
    }

    let conn = state.db.lock();
    let full_name: String = conn
        .query_row(
            "SELECT full_name FROM users WHERE id = ?1",
            params![user_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That staff member"))?;

    conn.execute(
        "UPDATE users SET photo_png = ?1, updated_at = ?2 WHERE id = ?3",
        params![png, Utc::now().to_rfc3339(), user_id],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "staff.photo",
        "user",
        &user_id,
        if png.is_some() {
            format!("Updated {full_name}'s photo")
        } else {
            format!("Removed {full_name}'s photo")
        },
    )?;

    Ok(())
}

#[tauri::command]
pub fn get_staff_photo(state: State<'_, AppState>, user_id: String) -> AppResult<Option<Vec<u8>>> {
    state.sessions.require()?;
    let conn = state.db.lock();
    Ok(conn
        .query_row(
            "SELECT photo_png FROM users WHERE id = ?1",
            params![user_id],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .optional()?
        .flatten())
}

#[tauri::command]
pub fn list_staff(state: State<'_, AppState>) -> AppResult<Vec<UserSummary>> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let conn = state.db.lock();
    let mut stmt = conn.prepare(
        "SELECT id, username, full_name, email, phone, role, status,
                must_change_password, last_login_at
         FROM users
         WHERE status = 'active'
         ORDER BY role ASC, full_name COLLATE NOCASE ASC",
    )?;

    let collected = stmt
        .query_map([], UserSummary::from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(collected)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStaffRequest {
    pub full_name: String,
    pub username: String,
    pub role: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStaffResult {
    pub id: String,
    pub username: String,
    /// Shown once, so the School Admin can hand it over. Never stored in clear.
    pub initial_password: String,
}

#[tauri::command]
pub fn create_staff(
    state: State<'_, AppState>,
    request: CreateStaffRequest,
) -> AppResult<CreateStaffResult> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    if request.full_name.trim().is_empty() {
        return Err(AppError::validation("A name is required."));
    }

    let username = request.username.trim().to_string();
    if username.len() < 3 {
        return Err(AppError::validation(
            "The username must be at least 3 characters long.",
        ));
    }
    if !matches!(request.role.as_str(), "school_admin" | "teacher") {
        return Err(AppError::validation("Choose School Admin or Teacher."));
    }

    let conn = state.db.lock();

    let taken: i64 = conn.query_row(
        "SELECT COUNT(*) FROM users WHERE username = ?1 COLLATE NOCASE",
        params![username],
        |row| row.get(0),
    )?;
    if taken > 0 {
        return Err(AppError::conflict(format!(
            "The username '{username}' is already taken."
        )));
    }

    // FR-B4: the cap is three, and it is checked here rather than trusted to
    // the interface.
    if request.role == "school_admin" {
        let admins: i64 =
            conn.query_row("SELECT COUNT(*) FROM v_active_school_admins", [], |row| {
                row.get(0)
            })?;
        if admins >= MAX_SCHOOL_ADMINS {
            return Err(AppError::conflict(
                "A school can have at most three School Admins. Retire one first.",
            ));
        }
    }

    let generated = password::generate_initial();
    let hash = password::hash(&generated)?;
    let id = new_id(prefix::USER);
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO users
            (id, username, full_name, email, phone, role, password_hash,
             must_change_password, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, 'active', ?8, ?8)",
        params![
            id,
            username,
            request.full_name.trim(),
            request.email.as_deref().map(str::trim),
            request.phone.as_deref().map(str::trim),
            request.role,
            hash,
            now,
        ],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "user.create",
        "user",
        &id,
        format!(
            "Created {} account for {}",
            request.role.replace('_', " "),
            request.full_name.trim()
        ),
    )?;

    Ok(CreateStaffResult {
        id,
        username,
        initial_password: password::reveal(&generated),
    })
}

/// Retires an account. Never deletes: the audit trail must keep resolving the
/// person who made a change years ago (SRS 14.5).
#[tauri::command]
pub fn retire_staff(state: State<'_, AppState>, user_id: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    if session.user_id == user_id {
        return Err(AppError::validation("You cannot retire your own account."));
    }

    let conn = state.db.lock();
    let (full_name, role): (String, String) = conn.query_row(
        "SELECT full_name, role FROM users WHERE id = ?1",
        params![user_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    if role == "school_admin" {
        let admins: i64 =
            conn.query_row("SELECT COUNT(*) FROM v_active_school_admins", [], |row| {
                row.get(0)
            })?;
        if admins <= 1 {
            return Err(AppError::conflict(
                "A school must keep at least one School Admin.",
            ));
        }
    }

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE users SET status = 'retired', updated_at = ?1 WHERE id = ?2",
        params![now, user_id],
    )?;
    // Their assignments retire with them, freeing the class for a successor.
    conn.execute(
        "UPDATE teacher_assignments SET status = 'retired', updated_at = ?1 WHERE user_id = ?2",
        params![now, user_id],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "user.retire",
        "user",
        &user_id,
        format!("Retired {full_name}'s account"),
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Assignments (FR-B6)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignmentRow {
    pub id: String,
    pub user_id: String,
    pub teacher_name: String,
    pub class_id: String,
    pub class_name: String,
    pub stream_id: Option<String>,
    pub stream_name: Option<String>,
    pub subject_id: Option<String>,
    pub subject_name: Option<String>,
    pub role: String,
}

#[tauri::command]
pub fn list_assignments(
    state: State<'_, AppState>,
    class_id: Option<String>,
) -> AppResult<Vec<AssignmentRow>> {
    let session = state.sessions.require()?;
    if let Some(class) = class_id.as_deref() {
        session.require_view_class(class)?;
    } else {
        session.require_admin()?;
    }

    let conn = state.db.lock();
    let mut stmt = conn.prepare(
        "SELECT ta.id, ta.user_id, u.full_name, ta.class_id, c.name,
                ta.stream_id, st.name, ta.subject_id, s.name, ta.role
         FROM teacher_assignments ta
         JOIN users u ON u.id = ta.user_id
         JOIN classes c ON c.id = ta.class_id
         LEFT JOIN streams st ON st.id = ta.stream_id
         LEFT JOIN subjects s ON s.id = ta.subject_id
         WHERE ta.status = 'active' AND (?1 IS NULL OR ta.class_id = ?1)
         ORDER BY c.ladder_position ASC, ta.role ASC, u.full_name ASC",
    )?;

    let collected = stmt
        .query_map(params![class_id], |row| {
            Ok(AssignmentRow {
                id: row.get(0)?,
                user_id: row.get(1)?,
                teacher_name: row.get(2)?,
                class_id: row.get(3)?,
                class_name: row.get(4)?,
                stream_id: row.get(5)?,
                stream_name: row.get(6)?,
                subject_id: row.get(7)?,
                subject_name: row.get(8)?,
                role: row.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(collected)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignRequest {
    pub user_id: String,
    pub class_id: String,
    #[serde(default)]
    pub stream_id: Option<String>,
    /// Required for `subject_teacher`, ignored otherwise.
    #[serde(default)]
    pub subject_id: Option<String>,
    pub role: String,
}

#[tauri::command]
pub fn assign_teacher(state: State<'_, AppState>, request: AssignRequest) -> AppResult<String> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    if !matches!(
        request.role.as_str(),
        "class_teacher" | "assistant_class_teacher" | "subject_teacher"
    ) {
        return Err(AppError::validation("Unknown assignment role."));
    }
    if request.role == "subject_teacher" && request.subject_id.is_none() {
        return Err(AppError::validation(
            "Choose the subject this teacher will teach.",
        ));
    }

    let conn = state.db.lock();

    let teacher_name: String = conn
        .query_row(
            "SELECT full_name FROM users WHERE id = ?1 AND status = 'active'",
            params![request.user_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That teacher"))?;

    let class_name: String = conn.query_row(
        "SELECT name FROM classes WHERE id = ?1",
        params![request.class_id],
        |row| row.get(0),
    )?;

    // The caps in FR-B6 are enforced by unique indexes, so a race between two
    // School Admins cannot produce two Class Teachers. Translate the index
    // violation into something a headteacher can act on.
    let id = new_id(prefix::ASSIGNMENT);
    let now = Utc::now().to_rfc3339();
    let subject_id = if request.role == "subject_teacher" {
        request.subject_id.clone()
    } else {
        None
    };

    let insert = conn.execute(
        "INSERT INTO teacher_assignments
            (id, user_id, class_id, stream_id, subject_id, role, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?7)",
        params![
            id,
            request.user_id,
            request.class_id,
            request.stream_id,
            subject_id,
            request.role,
            now,
        ],
    );

    if let Err(rusqlite::Error::SqliteFailure(error, _)) = &insert {
        if error.code == rusqlite::ErrorCode::ConstraintViolation {
            return Err(AppError::conflict(match request.role.as_str() {
                "class_teacher" => format!("{class_name} already has a Class Teacher."),
                "assistant_class_teacher" => {
                    format!("{class_name} already has an Assistant Class Teacher.")
                }
                _ => format!("That subject already has a teacher in {class_name}."),
            }));
        }
    }
    insert?;

    audit::record(
        &conn,
        Some(&session),
        "assignment.create",
        "assignment",
        &id,
        format!(
            "Assigned {teacher_name} to {class_name} as {}",
            request.role.replace('_', " ")
        ),
    )?;

    Ok(id)
}

#[tauri::command]
pub fn unassign_teacher(state: State<'_, AppState>, assignment_id: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let conn = state.db.lock();
    let (teacher_name, class_name, role): (String, String, String) = conn.query_row(
        "SELECT u.full_name, c.name, ta.role
         FROM teacher_assignments ta
         JOIN users u ON u.id = ta.user_id
         JOIN classes c ON c.id = ta.class_id
         WHERE ta.id = ?1",
        params![assignment_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;

    conn.execute(
        "UPDATE teacher_assignments SET status = 'retired', updated_at = ?1 WHERE id = ?2",
        params![Utc::now().to_rfc3339(), assignment_id],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "assignment.remove",
        "assignment",
        &assignment_id,
        format!(
            "Removed {teacher_name} as {} of {class_name}",
            role.replace('_', " ")
        ),
    )?;

    Ok(())
}
