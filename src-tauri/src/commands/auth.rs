//! Sign-in, sign-out, mode switching, password changes.
//!
//! FR-B1 (one login frame, role-based routing), FR-C1 (scoped teacher access),
//! FR-C2 (dual-mode Subject / Class Teacher).

use std::collections::HashSet;

use chrono::{Duration, Utc};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::audit;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::password::{self, Secret};
use crate::security::{Role, Session, SessionView, TeacherMode};
use crate::state::AppState;

/// After this many consecutive failures an account is locked for a while.
/// Slows credential guessing on a machine anyone in the staffroom can reach.
const MAX_FAILED_ATTEMPTS: i64 = 5;
const LOCKOUT_MINUTES: i64 = 15;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupState {
    /// False on a brand-new install: the frontend shows the setup wizard.
    pub provisioned: bool,
    pub institution_name: Option<String>,
    pub session: Option<SessionView>,
    pub app_version: String,
}

#[tauri::command]
pub fn startup_state(state: State<'_, AppState>) -> AppResult<StartupState> {
    let conn = state.db.lock();
    let provisioned = repo::is_provisioned(&conn)?;

    let institution_name: Option<String> = if provisioned {
        conn.query_row("SELECT name FROM institution WHERE id = 1", [], |row| {
            row.get(0)
        })
        .optional()?
    } else {
        None
    };

    Ok(StartupState {
        provisioned,
        institution_name,
        session: state.sessions.peek().as_ref().map(SessionView::from),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[tauri::command]
pub fn sign_in(
    state: State<'_, AppState>,
    username: String,
    password: Secret,
) -> AppResult<SessionView> {
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err(AppError::BadCredentials);
    }

    let conn = state.db.lock();

    type Account = (String, String, String, String, String, i64, i64, Option<String>);
    let account: Option<Account> = conn
        .query_row(
            "SELECT id, username, full_name, role, password_hash, must_change_password,
                    failed_attempts, locked_until
             FROM users
             WHERE username = ?1 COLLATE NOCASE AND status = 'active'",
            params![username],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .optional()?;

    let Some((
        user_id,
        stored_username,
        full_name,
        role_text,
        stored_hash,
        must_change,
        failed_attempts,
        locked_until,
    )) = account
    else {
        // Hash anyway so a missing username and a wrong password take
        // comparable time, and an attacker cannot enumerate accounts.
        let _ = password::verify(
            &password,
            "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHR2YWx1ZQ$3H8LRLnBVnBfZLXVYFMHwLZKcVYQmFXlrMHiCJOkBnE",
        );
        audit::record_system(
            &conn,
            "auth.sign_in_failed",
            "user",
            &username,
            format!("Failed sign-in for unknown username '{username}'"),
        )?;
        return Err(AppError::BadCredentials);
    };

    if let Some(until) = locked_until.as_deref() {
        if let Ok(until) = chrono::DateTime::parse_from_rfc3339(until) {
            if until.with_timezone(&Utc) > Utc::now() {
                return Err(AppError::AccountLocked);
            }
        }
    }

    if !password::verify(&password, &stored_hash) {
        let attempts = failed_attempts + 1;
        let lock_until = (attempts >= MAX_FAILED_ATTEMPTS)
            .then(|| (Utc::now() + Duration::minutes(LOCKOUT_MINUTES)).to_rfc3339());

        conn.execute(
            "UPDATE users SET failed_attempts = ?1, locked_until = ?2, updated_at = ?3 WHERE id = ?4",
            params![attempts, lock_until, Utc::now().to_rfc3339(), user_id],
        )?;

        audit::record_system(
            &conn,
            "auth.sign_in_failed",
            "user",
            &user_id,
            format!("Failed sign-in for {full_name} (attempt {attempts})"),
        )?;

        return Err(if attempts >= MAX_FAILED_ATTEMPTS {
            AppError::AccountLocked
        } else {
            AppError::BadCredentials
        });
    }

    let role = Role::parse(&role_text)?;
    let (class_ids, class_teacher_of) = load_teacher_scope(&conn, &user_id, role)?;

    conn.execute(
        "UPDATE users SET failed_attempts = 0, locked_until = NULL, last_login_at = ?1, updated_at = ?1
         WHERE id = ?2",
        params![Utc::now().to_rfc3339(), user_id],
    )?;

    // A teacher who is the Class Teacher somewhere lands in that view; everyone
    // else starts as a Subject Teacher (FR-C2).
    let mode = match role {
        Role::SchoolAdmin => None,
        Role::Teacher => Some(if class_teacher_of.is_empty() {
            TeacherMode::SubjectTeacher
        } else {
            TeacherMode::ClassTeacher
        }),
    };

    let session = Session {
        user_id: user_id.clone(),
        username: stored_username,
        full_name: full_name.clone(),
        role,
        must_change_password: must_change != 0,
        mode,
        class_ids,
        class_teacher_of,
        signed_in_at: Utc::now(),
        last_activity: Utc::now(),
    };

    audit::record(
        &conn,
        Some(&session),
        "auth.sign_in",
        "user",
        &user_id,
        format!("{full_name} signed in"),
    )?;

    let view = SessionView::from(&session);
    state.sessions.sign_in(session);
    Ok(view)
}

#[tauri::command]
pub fn sign_out(state: State<'_, AppState>) -> AppResult<()> {
    if let Some(session) = state.sessions.peek() {
        let conn = state.db.lock();
        audit::record(
            &conn,
            Some(&session),
            "auth.sign_out",
            "user",
            &session.user_id,
            format!("{} signed out", session.full_name),
        )?;
    }
    state.sessions.sign_out();
    Ok(())
}

#[tauri::command]
pub fn current_session(state: State<'_, AppState>) -> AppResult<Option<SessionView>> {
    Ok(state.sessions.peek().as_ref().map(SessionView::from))
}

/// FR-C2: switch between the Subject Teacher and Class Teacher views.
#[tauri::command]
pub fn set_teacher_mode(state: State<'_, AppState>, mode: TeacherMode) -> AppResult<SessionView> {
    let session = state.sessions.set_mode(mode)?;
    Ok(SessionView::from(&session))
}

#[tauri::command]
pub fn change_password(
    state: State<'_, AppState>,
    current_password: Secret,
    new_password: Secret,
) -> AppResult<()> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();

    let stored_hash: String = conn.query_row(
        "SELECT password_hash FROM users WHERE id = ?1",
        params![session.user_id],
        |row| row.get(0),
    )?;

    if !password::verify(&current_password, &stored_hash) {
        return Err(AppError::BadCredentials);
    }

    password::check_strength(&new_password)?;
    if password::verify(&new_password, &stored_hash) {
        return Err(AppError::validation(
            "The new password must be different from the current one.",
        ));
    }

    let new_hash = password::hash(&new_password)?;
    conn.execute(
        "UPDATE users SET password_hash = ?1, must_change_password = 0, updated_at = ?2 WHERE id = ?3",
        params![new_hash, Utc::now().to_rfc3339(), session.user_id],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "auth.change_password",
        "user",
        &session.user_id,
        format!("{} changed their own password", session.full_name),
    )?;

    state.sessions.clear_password_change_flag();
    Ok(())
}

/// A School Admin resets someone else's password and reads the new one out to
/// them. The account is forced to change it at next sign-in.
#[tauri::command]
pub fn reset_user_password(state: State<'_, AppState>, user_id: String) -> AppResult<String> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let conn = state.db.lock();

    let target_name: String = conn
        .query_row(
            "SELECT full_name FROM users WHERE id = ?1 AND status = 'active'",
            params![user_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That account"))?;

    let generated = password::generate_initial();
    let hash = password::hash(&generated)?;

    conn.execute(
        "UPDATE users
         SET password_hash = ?1, must_change_password = 1, failed_attempts = 0,
             locked_until = NULL, updated_at = ?2
         WHERE id = ?3",
        params![hash, Utc::now().to_rfc3339(), user_id],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "auth.reset_password",
        "user",
        &user_id,
        format!("{} reset the password for {target_name}", session.full_name),
    )?;

    Ok(password::reveal(&generated))
}

/// Which classes a user may touch, and where they are the Class Teacher.
///
/// A School Admin gets empty sets because `Session::may_view_class` short-
/// circuits on role — the institution is not enumerated into every session.
pub fn load_teacher_scope(
    conn: &rusqlite::Connection,
    user_id: &str,
    role: Role,
) -> AppResult<(HashSet<String>, HashSet<String>)> {
    if role == Role::SchoolAdmin {
        return Ok((HashSet::new(), HashSet::new()));
    }

    let mut stmt = conn.prepare(
        "SELECT class_id, role FROM teacher_assignments
         WHERE user_id = ?1 AND status = 'active'",
    )?;

    let rows = stmt.query_map(params![user_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut all = HashSet::new();
    let mut managed = HashSet::new();
    for row in rows {
        let (class_id, assignment_role) = row?;
        if assignment_role == "class_teacher" || assignment_role == "assistant_class_teacher" {
            managed.insert(class_id.clone());
        }
        all.insert(class_id);
    }

    Ok((all, managed))
}
