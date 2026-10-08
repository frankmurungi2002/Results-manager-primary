//! Signatures and the school stamp, printed on report cards and permits.
//!
//! A school signs hundreds of documents a term. These let the headteacher,
//! each Class Teacher and the school stamp be applied once, from a photo,
//! instead of by hand on every sheet. Because a stored signature is exactly
//! what a forger wants, three rules hold:
//!
//! * The headteacher's signature and the stamp are set by the office only.
//!   A member of staff sets their own signature, or the office sets it for
//!   them. Nobody sets another teacher's.
//! * Every change is audited, and a replaced image is kept (marked replaced),
//!   so the signature a document carried on a given day can be shown later.
//! * Printing them is a school decision: one switch turns them off on every
//!   document, for a school that wants wet signatures.
//!
//! Images are cleaned in Rust (`crate::ink`): a phone photo of ink on paper
//! becomes ink on a transparent background.

use std::path::Path;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::audit;
use crate::commands::reports::base64_encode;
use crate::domain::ids::new_id;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::ink::{self, Mark};
use crate::security::Session;
use crate::state::AppState;

const ID_PREFIX: &str = "sig";
const PRINT_SETTING: &str = "print.signatures";

/// The three kinds of mark. Parsed once, so nothing else handles raw strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    HeadTeacher,
    Stamp,
    Staff,
}

impl Kind {
    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "head_teacher" => Ok(Kind::HeadTeacher),
            "stamp" => Ok(Kind::Stamp),
            "staff" => Ok(Kind::Staff),
            other => Err(AppError::validation(format!("Unknown kind of signature \"{other}\"."))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Kind::HeadTeacher => "head_teacher",
            Kind::Stamp => "stamp",
            Kind::Staff => "staff",
        }
    }

    fn mark(self) -> Mark {
        if self == Kind::Stamp {
            Mark::Stamp
        } else {
            Mark::Signature
        }
    }
}

/// Who may set or remove which mark. Decided here and nowhere else.
fn authorize(session: &Session, kind: Kind, user_id: Option<&str>) -> AppResult<()> {
    match kind {
        Kind::HeadTeacher | Kind::Stamp => session.require_admin(),
        Kind::Staff => {
            let user_id = user_id.ok_or_else(|| AppError::validation("Whose signature is this?"))?;
            if session.is_admin() || session.user_id == user_id {
                Ok(())
            } else {
                Err(AppError::Forbidden)
            }
        }
    }
}

fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64_encode(png))
}

fn active(conn: &Connection, kind: Kind, user_id: Option<&str>) -> AppResult<Option<Vec<u8>>> {
    Ok(conn
        .query_row(
            "SELECT png FROM signature_images
             WHERE kind = ?1 AND IFNULL(user_id, '-') = IFNULL(?2, '-') AND status = 'active'",
            params![kind.as_str(), user_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// Whether the school prints signatures and the stamp at all. On unless the
/// office has switched it off.
pub fn printing_on(conn: &Connection) -> AppResult<bool> {
    Ok(repo::get_setting(conn, PRINT_SETTING)?.map(|v| v != "0").unwrap_or(true))
}

/// The headteacher's signature and the stamp for the print pipeline, or
/// nothing when printing them is switched off.
pub fn institution_marks(conn: &Connection) -> AppResult<(Option<String>, Option<String>)> {
    if !printing_on(conn)? {
        return Ok((None, None));
    }
    Ok((
        active(conn, Kind::HeadTeacher, None)?.map(|b| data_url(&b)),
        active(conn, Kind::Stamp, None)?.map(|b| data_url(&b)),
    ))
}

/// One staff signature for a document, or nothing when printing is off.
pub fn staff_signature(conn: &Connection, user_id: &str) -> AppResult<Option<String>> {
    if !printing_on(conn)? {
        return Ok(None);
    }
    Ok(active(conn, Kind::Staff, Some(user_id))?.map(|b| data_url(&b)))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Reads the photo the user picked and returns it cleaned, for a preview.
/// Nothing is stored until `save_signature`.
#[tauri::command]
pub fn prepare_signature_image(
    state: State<'_, AppState>,
    path: String,
    kind: String,
) -> AppResult<Vec<u8>> {
    state.sessions.require()?;
    let kind = Kind::parse(&kind)?;
    let file = Path::new(&path);
    if !file.is_file() {
        return Err(AppError::not_found("That file"));
    }
    let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    if size as usize > ink::MAX_FILE_BYTES {
        return Err(AppError::validation("That image is larger than 8 MB. Please use a smaller photo."));
    }
    let bytes = std::fs::read(file)?;
    ink::clean(&bytes, kind.mark())
}

#[tauri::command]
pub fn save_signature(
    state: State<'_, AppState>,
    kind: String,
    user_id: Option<String>,
    png: Vec<u8>,
) -> AppResult<()> {
    let session = state.sessions.require()?;
    let kind = Kind::parse(&kind)?;
    let user_id = if kind == Kind::Staff { user_id } else { None };
    authorize(&session, kind, user_id.as_deref())?;
    ink::validate_png(&png)?;
    let conn = state.db.lock();
    store(&conn, &session, kind, user_id.as_deref(), &png)
}

fn describe(conn: &Connection, kind: Kind, user_id: Option<&str>) -> AppResult<String> {
    Ok(match kind {
        Kind::HeadTeacher => "the headteacher's signature".into(),
        Kind::Stamp => "the school stamp".into(),
        Kind::Staff => {
            let name: String = conn
                .query_row(
                    "SELECT full_name FROM users WHERE id = ?1 AND status = 'active'",
                    params![user_id],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| AppError::not_found("staff member"))?;
            format!("{name}'s signature")
        }
    })
}

fn store(conn: &Connection, session: &Session, kind: Kind, user_id: Option<&str>, png: &[u8]) -> AppResult<()> {
    let what = describe(conn, kind, user_id)?;
    let now = Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    let replaced = tx.execute(
        "UPDATE signature_images SET status = 'replaced', retired_at = ?1
         WHERE kind = ?2 AND IFNULL(user_id, '-') = IFNULL(?3, '-') AND status = 'active'",
        params![now, kind.as_str(), user_id],
    )?;
    let id = new_id(ID_PREFIX);
    tx.execute(
        "INSERT INTO signature_images (id, kind, user_id, png, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, kind.as_str(), user_id, png, session.user_id, now],
    )?;
    audit::record(
        &tx,
        Some(session),
        "signature.set",
        "signature",
        &id,
        format!("{} {what}", if replaced > 0 { "Replaced" } else { "Added" }),
    )?;
    tx.commit()?;
    Ok(())
}

#[tauri::command]
pub fn remove_signature(state: State<'_, AppState>, kind: String, user_id: Option<String>) -> AppResult<()> {
    let session = state.sessions.require()?;
    let kind = Kind::parse(&kind)?;
    let user_id = if kind == Kind::Staff { user_id } else { None };
    authorize(&session, kind, user_id.as_deref())?;
    let conn = state.db.lock();
    let what = describe(&conn, kind, user_id.as_deref())?;
    let tx = conn.unchecked_transaction()?;
    let removed = tx.execute(
        "UPDATE signature_images SET status = 'removed', retired_at = ?1
         WHERE kind = ?2 AND IFNULL(user_id, '-') = IFNULL(?3, '-') AND status = 'active'",
        params![Utc::now().to_rfc3339(), kind.as_str(), user_id],
    )?;
    if removed > 0 {
        audit::record(&tx, Some(&session), "signature.remove", "signature", kind.as_str(), format!("Removed {what}"))?;
    }
    tx.commit()?;
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffSignature {
    pub user_id: String,
    pub full_name: String,
    pub data_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureOverview {
    pub print_signatures: bool,
    /// Only the office sees these two.
    pub head_teacher: Option<String>,
    pub stamp: Option<String>,
    pub can_manage_school: bool,
    /// Every active staff member for the office; just yourself otherwise.
    pub staff: Vec<StaffSignature>,
}

#[tauri::command]
pub fn get_signatures(state: State<'_, AppState>) -> AppResult<SignatureOverview> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let admin = session.is_admin();
    let mut stmt = conn.prepare(
        "SELECT u.id, u.full_name, s.png
         FROM users u
         LEFT JOIN signature_images s
                ON s.user_id = u.id AND s.kind = 'staff' AND s.status = 'active'
         WHERE u.status = 'active' AND (?1 OR u.id = ?2)
         ORDER BY u.full_name COLLATE NOCASE",
    )?;
    let staff = stmt
        .query_map(params![admin, session.user_id], |r| {
            let png: Option<Vec<u8>> = r.get(2)?;
            Ok(StaffSignature {
                user_id: r.get(0)?,
                full_name: r.get(1)?,
                data_url: png.map(|b| data_url(&b)),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(SignatureOverview {
        print_signatures: printing_on(&conn)?,
        head_teacher: if admin { active(&conn, Kind::HeadTeacher, None)?.map(|b| data_url(&b)) } else { None },
        stamp: if admin { active(&conn, Kind::Stamp, None)?.map(|b| data_url(&b)) } else { None },
        can_manage_school: admin,
        staff,
    })
}

#[tauri::command]
pub fn set_print_signatures(state: State<'_, AppState>, on: bool) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    let conn = state.db.lock();
    repo::set_setting(&conn, PRINT_SETTING, if on { "1" } else { "0" })?;
    audit::record(
        &conn,
        Some(&session),
        "signature.printing",
        "setting",
        PRINT_SETTING,
        if on { "Turned on printed signatures and stamp" } else { "Turned off printed signatures and stamp" },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    use crate::db::Database;
    use crate::security::Role;

    fn person(id: &str, role: Role) -> Session {
        Session {
            user_id: id.into(),
            username: id.into(),
            full_name: id.into(),
            role,
            must_change_password: false,
            mode: None,
            class_ids: HashSet::new(),
            class_teacher_of: HashSet::new(),
            signed_in_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }

    fn db() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.lock()
            .execute_batch(
                "INSERT INTO users (id, username, full_name, role, password_hash, created_at, updated_at) VALUES
                     ('admin',  'admin',  'Office',    'school_admin', 'x', 'now', 'now'),
                     ('okello', 'okello', 'Mr Okello', 'teacher',      'x', 'now', 'now'),
                     ('achen',  'achen',  'Ms Achen',  'teacher',      'x', 'now', 'now');",
            )
            .unwrap();
        db
    }

    #[test]
    fn only_the_office_sets_the_headteachers_signature_and_the_stamp() {
        let teacher = person("okello", Role::Teacher);
        let admin = person("admin", Role::SchoolAdmin);
        for kind in [Kind::HeadTeacher, Kind::Stamp] {
            assert!(matches!(authorize(&teacher, kind, None), Err(AppError::Forbidden)));
            assert!(authorize(&admin, kind, None).is_ok());
        }
    }

    #[test]
    fn a_teacher_sets_their_own_signature_and_nobody_elses() {
        let okello = person("okello", Role::Teacher);
        assert!(authorize(&okello, Kind::Staff, Some("okello")).is_ok());
        assert!(matches!(authorize(&okello, Kind::Staff, Some("achen")), Err(AppError::Forbidden)));
        assert!(authorize(&person("admin", Role::SchoolAdmin), Kind::Staff, Some("achen")).is_ok());
    }

    #[test]
    fn replacing_keeps_the_old_signature_on_record() {
        let db = db();
        let conn = db.lock();
        let admin = person("admin", Role::SchoolAdmin);
        store(&conn, &admin, Kind::HeadTeacher, None, b"first").unwrap();
        store(&conn, &admin, Kind::HeadTeacher, None, b"second").unwrap();
        let rows: Vec<(String, Vec<u8>)> = conn
            .prepare("SELECT status, png FROM signature_images ORDER BY created_at, rowid")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(rows, vec![("replaced".into(), b"first".to_vec()), ("active".into(), b"second".to_vec())]);
        assert_eq!(active(&conn, Kind::HeadTeacher, None).unwrap().unwrap(), b"second");
        let audited: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit_log WHERE action = 'signature.set'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(audited, 2);
    }

    #[test]
    fn staff_signatures_are_kept_apart() {
        let db = db();
        let conn = db.lock();
        let admin = person("admin", Role::SchoolAdmin);
        store(&conn, &admin, Kind::Staff, Some("okello"), b"okello").unwrap();
        store(&conn, &admin, Kind::Staff, Some("achen"), b"achen").unwrap();
        assert_eq!(active(&conn, Kind::Staff, Some("okello")).unwrap().unwrap(), b"okello");
        assert_eq!(active(&conn, Kind::Staff, Some("achen")).unwrap().unwrap(), b"achen");
    }

    #[test]
    fn switching_printing_off_keeps_every_mark_off_every_document() {
        let db = db();
        let conn = db.lock();
        let admin = person("admin", Role::SchoolAdmin);
        store(&conn, &admin, Kind::HeadTeacher, None, b"head").unwrap();
        store(&conn, &admin, Kind::Stamp, None, b"stamp").unwrap();
        store(&conn, &admin, Kind::Staff, Some("okello"), b"okello").unwrap();
        let (head, stamp) = institution_marks(&conn).unwrap();
        assert!(head.is_some() && stamp.is_some());
        assert!(staff_signature(&conn, "okello").unwrap().is_some());

        repo::set_setting(&conn, PRINT_SETTING, "0").unwrap();
        assert_eq!(institution_marks(&conn).unwrap(), (None, None));
        assert_eq!(staff_signature(&conn, "okello").unwrap(), None);
    }

    #[test]
    fn the_database_refuses_a_staff_signature_with_no_owner() {
        let db = db();
        let result = db.lock().execute(
            "INSERT INTO signature_images (id, kind, user_id, png, created_by, created_at)
             VALUES ('x', 'staff', NULL, x'00', 'admin', 'now')",
            [],
        );
        assert!(result.is_err());
    }
}
