//! A School Admin who forgets their password gets back in with a printed
//! recovery code, without a technician and without the owner.
//!
//! The code is shown once, printed on a recovery sheet for the school safe,
//! and stored only as a hash. Using it sets a new password and spends it;
//! printing a new sheet replaces the old code. Five wrong codes lock the
//! account's recovery for an hour, and every use is in the activity log.

use chrono::{Duration, Utc};
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::audit;
use crate::error::{AppError, AppResult};
use crate::security::password::{self, Secret};
use crate::security::{Role, Session};
use crate::state::AppState;

const MAX_TRIES: i64 = 5;
const LOCK_MINUTES: i64 = 60;
/// Crockford-like: no 0/O, 1/I/L, U.
const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";

/// "RCV-7K2M-9QXA-4FTB-HN3P": 80 bits, readable off paper.
fn generate() -> String {
    let mut rng = rand::rngs::OsRng;
    let groups: Vec<String> = (0..4)
        .map(|_| (0..4).map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char).collect())
        .collect();
    format!("RCV-{}", groups.join("-"))
}

fn normalise(code: &str) -> String {
    let cleaned: String = code
        .trim()
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| match c {
            '0' => 'O',
            '1' | 'I' | 'L' => '1',
            other => other,
        })
        .collect();
    cleaned.strip_prefix("RCV").unwrap_or(&cleaned).to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySheet {
    pub code: String,
    pub username: String,
    pub full_name: String,
    pub created_at: String,
}

pub fn create_in(conn: &Connection, session: &Session) -> AppResult<RecoverySheet> {
    if session.role != Role::SchoolAdmin {
        return Err(AppError::validation(
            "Recovery codes are for School Admins. Anyone else's password can be reset by a School Admin.",
        ));
    }
    let code = generate();
    let hash = password::hash(&Secret::new(normalise(&code)))?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE users SET recovery_hash = ?1, recovery_created_at = ?2, recovery_failed_attempts = 0,
                          recovery_locked_until = NULL WHERE id = ?3",
        params![hash, now, session.user_id],
    )?;
    audit::record(conn, Some(session), "auth.recovery_code", "user", &session.user_id, "Printed a new recovery code (any earlier one no longer works)")?;
    Ok(RecoverySheet { code, username: session.username.clone(), full_name: session.full_name.clone(), created_at: now })
}

#[tauri::command]
pub fn create_recovery_code(state: State<'_, AppState>) -> AppResult<crate::commands::reports::DocumentEnvelope<RecoverySheet>> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let sheet = create_in(&conn, &session)?;
    crate::commands::reports::envelope(&conn, "recovery_sheet", sheet)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryStatus {
    pub may_have_code: bool,
    pub created_at: Option<String>,
}

#[tauri::command]
pub fn recovery_status(state: State<'_, AppState>) -> AppResult<RecoveryStatus> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let created_at: Option<String> = conn
        .query_row("SELECT recovery_created_at FROM users WHERE id = ?1 AND recovery_hash IS NOT NULL", params![session.user_id], |r| r.get(0))
        .optional()?
        .flatten();
    Ok(RecoveryStatus { may_have_code: session.role == Role::SchoolAdmin, created_at })
}

/// No session: this is how a locked-out School Admin gets back in.
pub fn use_in(conn: &Connection, username: &str, code: &str, new_password: &Secret) -> AppResult<()> {
    let row: Option<(String, String, Option<String>, i64, Option<String>)> = conn
        .query_row(
            "SELECT id, full_name, recovery_hash, recovery_failed_attempts, recovery_locked_until FROM users
             WHERE username = ?1 COLLATE NOCASE AND status = 'active' AND role = 'school_admin'",
            params![username.trim()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    // Same answer whether the account exists or not.
    let refused = || AppError::validation("That username and recovery code do not match.");
    let Some((id, name, Some(hash), tries, locked)) = row else { return Err(refused()) };
    let now = Utc::now();
    if locked.as_deref().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).is_some_and(|t| t > now) {
        return Err(AppError::validation("Too many wrong codes. Try again in an hour."));
    }
    if !password::verify(&Secret::new(normalise(code)), &hash) {
        let tries = tries + 1;
        let lock = (tries >= MAX_TRIES).then(|| (now + Duration::minutes(LOCK_MINUTES)).to_rfc3339());
        conn.execute(
            "UPDATE users SET recovery_failed_attempts = ?1, recovery_locked_until = ?2 WHERE id = ?3",
            params![if lock.is_some() { 0 } else { tries }, lock, id],
        )?;
        audit::record_system(conn, "auth.recovery_failed", "user", &id, format!("A wrong recovery code was tried for {name}"))?;
        return Err(refused());
    }
    password::check_strength(new_password)?;
    conn.execute(
        "UPDATE users SET password_hash = ?1, must_change_password = 0, failed_attempts = 0, locked_until = NULL,
                          recovery_hash = NULL, recovery_failed_attempts = 0, recovery_locked_until = NULL, updated_at = ?2
         WHERE id = ?3",
        params![password::hash(new_password)?, now.to_rfc3339(), id],
    )?;
    audit::record_system(conn, "auth.recovered", "user", &id, format!("{name} set a new password with their recovery code (the code is now spent)"))?;
    Ok(())
}

#[tauri::command]
pub fn use_recovery_code(state: State<'_, AppState>, username: String, code: String, new_password: Secret) -> AppResult<()> {
    let conn = state.db.lock();
    use_in(&conn, &username, &code, &new_password)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::security::session::test_session;
    use crate::security::Office;

    fn db() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.lock()
            .execute_batch(
                "INSERT INTO users (id, username, full_name, role, password_hash, created_at, updated_at) VALUES
                    ('a', 'admin', 'Frank', 'school_admin', 'x', 'now', 'now'),
                    ('h', 'head', 'Head', 'teacher', 'x', 'now', 'now');",
            )
            .unwrap();
        db
    }

    #[test]
    fn a_code_works_once_and_is_forgiving_about_how_it_is_typed() {
        let db = db();
        let conn = db.lock();
        let admin = test_session("a", Role::SchoolAdmin, None, &[]);
        let sheet = create_in(&conn, &admin).unwrap();
        assert!(sheet.code.starts_with("RCV-") && sheet.code.len() == 23, "{}", sheet.code);
        let typed = sheet.code.to_lowercase().replace('-', " ");
        use_in(&conn, "ADMIN", &typed, &Secret::new("A new long passphrase")).unwrap();
        let hash: String = conn.query_row("SELECT password_hash FROM users WHERE id = 'a'", [], |r| r.get(0)).unwrap();
        assert!(password::verify(&Secret::new("A new long passphrase"), &hash));
        assert!(use_in(&conn, "admin", &sheet.code, &Secret::new("Another long passphrase")).is_err(), "spent");
    }

    #[test]
    fn a_new_sheet_replaces_the_old_code_and_others_cannot_have_one() {
        let db = db();
        let conn = db.lock();
        let admin = test_session("a", Role::SchoolAdmin, None, &[]);
        let old = create_in(&conn, &admin).unwrap();
        let new = create_in(&conn, &admin).unwrap();
        assert!(use_in(&conn, "admin", &old.code, &Secret::new("A new long passphrase")).is_err());
        assert!(use_in(&conn, "admin", &new.code, &Secret::new("A new long passphrase")).is_ok());
        let head = test_session("h", Role::Teacher, Some(Office::HeadTeacher), &[]);
        assert!(create_in(&conn, &head).is_err());
    }

    #[test]
    fn five_wrong_codes_lock_recovery() {
        let db = db();
        let conn = db.lock();
        let admin = test_session("a", Role::SchoolAdmin, None, &[]);
        let sheet = create_in(&conn, &admin).unwrap();
        for _ in 0..MAX_TRIES {
            assert!(use_in(&conn, "admin", "RCV-2222-2222-2222-2222", &Secret::new("A new long passphrase")).is_err());
        }
        let locked = use_in(&conn, "admin", &sheet.code, &Secret::new("A new long passphrase")).unwrap_err();
        assert!(locked.user_message().contains("Too many"));
    }
}
