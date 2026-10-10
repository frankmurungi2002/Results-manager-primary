//! Inviting staff: a one-time code instead of a password told across a desk.
//!
//! The School Admin (or anyone who manages staff) invites a member of staff.
//! The code goes by SMS when the school is online and the person has a phone,
//! and is always shown once so it can be handed over on a slip or pasted into
//! WhatsApp or email. On the sign-in screen the teacher chooses "I have an
//! invite code", types their username and the code, and sets their own
//! password. The code is stored only as a hash, lasts three days, works once,
//! and five wrong tries spend it. A new invite replaces the old one.

use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::audit;
use crate::commands::reports::{envelope, DocumentEnvelope};
use crate::error::{AppError, AppResult};
use crate::security::password::{self, Secret};
use crate::security::{Office, Permission, Role, Session};
use crate::sms;
use crate::state::AppState;

const LIFETIME_HOURS: i64 = 72;
const MAX_TRIES: i64 = 5;
const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";

fn generate() -> String {
    let mut rng = rand::rngs::OsRng;
    let group = |rng: &mut rand::rngs::OsRng| -> String { (0..4).map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char).collect() };
    format!("INV-{}-{}", group(&mut rng), group(&mut rng))
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
    cleaned.strip_prefix("INV").unwrap_or(&cleaned).to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Invite {
    pub user_id: String,
    pub full_name: String,
    pub username: String,
    pub code: String,
    pub expires_at: String,
    /// The text to send or paste: names the school, the username and the code.
    pub message: String,
    pub phone: Option<String>,
    /// Queued for SMS (offline-only schools and missing phones get none).
    pub sms_queued: bool,
}

pub fn invite_in(conn: &Connection, session: &Session, user_id: &str, send_sms: bool, now: DateTime<Utc>) -> AppResult<Invite> {
    session.require(Permission::ManageStaff)?;
    let (full_name, username, role, office, phone): (String, String, String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT full_name, username, role, office, phone FROM users WHERE id = ?1 AND status = 'active'",
            params![user_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That member of staff"))?;
    // An invite is a way into the account, like a password reset.
    session.require_act_on(Role::parse(&role)?, office.as_deref().map(Office::parse).transpose()?)?;
    if user_id == session.user_id {
        return Err(AppError::validation("Change your own password in Settings."));
    }

    let code = generate();
    let expires = now + Duration::hours(LIFETIME_HOURS);
    conn.execute(
        "INSERT INTO staff_invites (user_id, code_hash, expires_at, failed_attempts, used_at, created_by, created_at)
         VALUES (?1, ?2, ?3, 0, NULL, ?4, ?5)
         ON CONFLICT (user_id) DO UPDATE SET code_hash = excluded.code_hash, expires_at = excluded.expires_at,
             failed_attempts = 0, used_at = NULL, created_by = excluded.created_by, created_at = excluded.created_at",
        params![user_id, password::hash(&Secret::new(normalise(&code)))?, expires.to_rfc3339(), session.user_id, now.to_rfc3339()],
    )?;

    let school: String = conn.query_row("SELECT name FROM institution LIMIT 1", [], |r| r.get(0)).optional()?.unwrap_or_default();
    let first = full_name.split_whitespace().next().unwrap_or(&full_name);
    let message = format!(
        "{school}: {first}, your Phantom School Manager account is ready. Username: {username}. Invite code: {code} (valid 3 days). On the school computer choose \"I have an invite code\" and set your own password."
    );
    let sms_queued = if send_sms {
        sms::queue(conn, phone.as_deref(), &message, "invite", user_id, Some(&session.user_id))?.is_some()
    } else {
        false
    };
    audit::record(
        conn,
        Some(session),
        "user.invite",
        "user",
        user_id,
        format!("Invited {full_name} to set their own password{}", if sms_queued { " (code sent by SMS)" } else { "" }),
    )?;
    if sms_queued {
        sms::wake();
    }
    Ok(Invite { user_id: user_id.into(), full_name, username, code, expires_at: expires.to_rfc3339(), message, phone, sms_queued })
}

/// Returns the invite as a printable slip as well as the code to show.
#[tauri::command]
pub fn invite_staff(state: State<'_, AppState>, user_id: String, send_sms: bool) -> AppResult<DocumentEnvelope<Invite>> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let invite = invite_in(&conn, &session, &user_id, send_sms, Utc::now())?;
    envelope(&conn, "invite_slip", invite)
}

/// No session: the invited person sets their own password.
pub fn accept_in(conn: &Connection, username: &str, code: &str, new_password: &Secret, now: DateTime<Utc>) -> AppResult<String> {
    let row: Option<(String, String, String, String, i64, Option<String>)> = conn
        .query_row(
            "SELECT u.id, u.full_name, i.code_hash, i.expires_at, i.failed_attempts, i.used_at
             FROM staff_invites i JOIN users u ON u.id = i.user_id
             WHERE u.username = ?1 COLLATE NOCASE AND u.status = 'active'",
            params![username.trim()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()?;
    let refused = || AppError::validation("That username and invite code do not match. Ask for a new invite if it is more than three days old.");
    let Some((id, name, hash, expires, tries, used)) = row else { return Err(refused()) };
    let expired = DateTime::parse_from_rfc3339(&expires).map(|t| t <= now).unwrap_or(true);
    if used.is_some() || expired || tries >= MAX_TRIES {
        return Err(refused());
    }
    if !password::verify(&Secret::new(normalise(code)), &hash) {
        conn.execute("UPDATE staff_invites SET failed_attempts = failed_attempts + 1 WHERE user_id = ?1", params![id])?;
        audit::record_system(conn, "auth.invite_failed", "user", &id, format!("A wrong invite code was tried for {name}"))?;
        return Err(refused());
    }
    password::check_strength(new_password)?;
    conn.execute(
        "UPDATE users SET password_hash = ?1, must_change_password = 0, failed_attempts = 0, locked_until = NULL, updated_at = ?2 WHERE id = ?3",
        params![password::hash(new_password)?, now.to_rfc3339(), id],
    )?;
    conn.execute("UPDATE staff_invites SET used_at = ?1 WHERE user_id = ?2", params![now.to_rfc3339(), id])?;
    audit::record_system(conn, "auth.invite_accepted", "user", &id, format!("{name} set their own password with an invite code"))?;
    Ok(name)
}

#[tauri::command]
pub fn accept_invite(state: State<'_, AppState>, username: String, code: String, new_password: Secret) -> AppResult<String> {
    let conn = state.db.lock();
    accept_in(&conn, &username, &code, &new_password, Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::security::session::test_session;

    fn db() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.lock()
            .execute_batch(
                "INSERT INTO users (id, username, full_name, role, password_hash, phone, created_at, updated_at) VALUES
                    ('a', 'admin', 'Frank M', 'school_admin', 'x', NULL, 'now', 'now'),
                    ('a2', 'admin2', 'Second Admin', 'school_admin', 'x', NULL, 'now', 'now'),
                    ('t', 'jane', 'Jane Akello', 'teacher', 'x', '0772123456', 'now', 'now');",
            )
            .unwrap();
        db
    }

    #[test]
    fn an_invite_sets_a_password_once_within_three_days() {
        let db = db();
        let conn = db.lock();
        let admin = test_session("a", Role::SchoolAdmin, None, &[]);
        let now = Utc::now();
        let invite = invite_in(&conn, &admin, "t", true, now).unwrap();
        assert!(invite.code.starts_with("INV-") && invite.code.len() == 13, "{}", invite.code);
        assert!(invite.message.contains("Username: jane") && invite.message.contains(&invite.code));
        assert!(invite.sms_queued);
        let kind: String = conn.query_row("SELECT kind FROM sms_outbox", [], |r| r.get(0)).unwrap();
        assert_eq!(kind, "invite");

        let typed = invite.code.to_lowercase().replace('-', " ");
        assert!(accept_in(&conn, "JANE", &typed, &Secret::new("short"), now).is_err(), "the password rules still apply");
        assert_eq!(accept_in(&conn, "JANE", &typed, &Secret::new("Chalk and blackboards 7"), now).unwrap(), "Jane Akello");
        let (hash, must): (String, i64) = conn.query_row("SELECT password_hash, must_change_password FROM users WHERE id = 't'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert!(password::verify(&Secret::new("Chalk and blackboards 7"), &hash));
        assert_eq!(must, 0, "they chose it, so they are not asked to change it");
        assert!(accept_in(&conn, "jane", &invite.code, &Secret::new("Another long passphrase"), now).is_err(), "spent");

        let late = invite_in(&conn, &admin, "t", false, now).unwrap();
        assert!(accept_in(&conn, "jane", &late.code, &Secret::new("Chalk and blackboards 8"), now + Duration::hours(73)).is_err(), "expired");
    }

    #[test]
    fn wrong_codes_spend_an_invite_and_only_staff_managers_invite() {
        let db = db();
        let conn = db.lock();
        let admin = test_session("a", Role::SchoolAdmin, None, &[]);
        let now = Utc::now();
        let invite = invite_in(&conn, &admin, "t", false, now).unwrap();
        for _ in 0..MAX_TRIES {
            assert!(accept_in(&conn, "jane", "INV-2222-2222", &Secret::new("Chalk and blackboards 7"), now).is_err());
        }
        assert!(accept_in(&conn, "jane", &invite.code, &Secret::new("Chalk and blackboards 7"), now).is_err(), "five wrong tries spend it");
        // A new invite starts again.
        let again = invite_in(&conn, &admin, "t", false, now).unwrap();
        assert!(accept_in(&conn, "jane", &again.code, &Secret::new("Chalk and blackboards 7"), now).is_ok());

        let teacher = test_session("t", Role::Teacher, None, &[]);
        assert!(matches!(invite_in(&conn, &teacher, "a2", false, now), Err(AppError::Forbidden)));
        let head = test_session("t", Role::Teacher, Some(Office::HeadTeacher), &[]);
        assert!(matches!(invite_in(&conn, &head, "a2", false, now), Err(AppError::Forbidden)), "nobody invites themselves into an account above them");
    }
}
