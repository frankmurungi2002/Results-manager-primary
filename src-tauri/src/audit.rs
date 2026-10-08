//! The audit trail (FR-E2).
//!
//! SRS 16.6 is explicit that this should be close to free *if it is designed
//! into the data layer up front rather than bolted on later*. So it lives
//! here, takes the same connection the write took, and is written inside the
//! caller's transaction — an action and its audit entry commit together or not
//! at all.

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::security::Session;

/// Records one action. `conn` must be the same connection (and transaction)
/// that performed the write.
pub fn record(
    conn: &Connection,
    actor: Option<&Session>,
    action: &str,
    entity: &str,
    entity_id: &str,
    summary: impl AsRef<str>,
) -> AppResult<()> {
    record_with_details(conn, actor, action, entity, entity_id, summary, None)
}

pub fn record_with_details(
    conn: &Connection,
    actor: Option<&Session>,
    action: &str,
    entity: &str,
    entity_id: &str,
    summary: impl AsRef<str>,
    details: Option<Value>,
) -> AppResult<()> {
    // The actor is recorded by name as well as by id so the trail stays
    // readable years later, after an account has been retired and its holder
    // has left the school (SRS 14.5).
    // The office says more than the account: "head_teacher", not "teacher".
    let (actor_id, actor_name, actor_role) = match actor {
        Some(session) => (
            Some(session.user_id.as_str()),
            Some(session.full_name.as_str()),
            Some(session.office.map(|o| o.as_str()).unwrap_or(session.role.as_str())),
        ),
        None => (None, None, None),
    };

    conn.execute(
        "INSERT INTO audit_log (at, actor_id, actor_name, actor_role, action, entity, entity_id, summary, details)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            chrono::Utc::now().to_rfc3339(),
            actor_id,
            actor_name,
            actor_role,
            action,
            entity,
            entity_id,
            summary.as_ref(),
            details.map(|d| d.to_string()),
        ],
    )?;

    seal(conn, conn.last_insert_rowid())
}

// ---------------------------------------------------------------------------
// The hash chain
// ---------------------------------------------------------------------------
//
// Each entry's seal is SHA-256(previous seal, the entry). Change an entry and
// its seal no longer matches; delete one and the next seal no longer follows
// from the one before; insert one with SQL and it has no seal at all. Every
// seal after the change is wrong too, so the first broken seal says where
// history was altered.
//
// A chain proves nothing on its own if the whole thing can be recomputed, so
// it is anchored outside the database: every printed export carries a
// verification code, the seal at that moment. A rewritten log cannot match a
// code already on paper.

/// What the first entry's "previous seal" is.
const GENESIS: &str = "phantom-school-manager/audit/v1";
/// Set once the entries made before sealing existed have been sealed.
const LEGACY_SEALED: &str = "audit.legacy_sealed";

type Row = (i64, String, Option<String>, Option<String>, Option<String>, String, Option<String>, Option<String>, String, Option<String>);

const ROW_SQL: &str = "SELECT id, at, actor_id, actor_name, actor_role, action, entity, entity_id, summary, details
                       FROM audit_log";

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    Ok((
        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
        row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
    ))
}

fn hash(previous: &str, row: &Row) -> String {
    // A JSON array: unambiguous, so no two different entries hash alike by
    // having their fields run together.
    let canonical = serde_json::json!([
        row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9
    ])
    .to_string();
    let mut digest = Sha256::new();
    digest.update(previous.as_bytes());
    digest.update(b"\n");
    digest.update(canonical.as_bytes());
    digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn last_seal(conn: &Connection) -> AppResult<String> {
    Ok(conn
        .query_row("SELECT hash FROM audit_seal ORDER BY audit_id DESC LIMIT 1", [], |r| r.get(0))
        .optional()?
        .unwrap_or_else(|| GENESIS.to_string()))
}

fn seal(conn: &Connection, id: i64) -> AppResult<()> {
    let row = conn.query_row(&format!("{ROW_SQL} WHERE id = ?1"), params![id], read_row)?;
    let sealed = hash(&last_seal(conn)?, &row);
    conn.execute("INSERT INTO audit_seal (audit_id, hash) VALUES (?1, ?2)", params![id, sealed])?;
    Ok(())
}

/// Seals the entries written before the chain existed. Runs once, ever: an
/// unsealed entry found later was added outside the application and must be
/// reported, not quietly sealed.
pub fn seal_legacy_once(conn: &Connection) -> AppResult<()> {
    let done: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", params![LEGACY_SEALED], |r| r.get(0))
        .optional()?;
    if done.is_some() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    let ids: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT id FROM audit_log WHERE id NOT IN (SELECT audit_id FROM audit_seal) ORDER BY id",
        )?;
        let collected = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        collected
    };
    for id in &ids {
        seal(&tx, *id)?;
    }
    tx.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
        params![LEGACY_SEALED, ids.len().to_string(), chrono::Utc::now().to_rfc3339()],
    )?;
    tx.commit()?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainReport {
    /// Entries in the log.
    pub entries: i64,
    /// True when every entry is sealed and every seal checks.
    pub intact: bool,
    /// The first thing wrong, in plain words, when it is not intact.
    pub problem: Option<String>,
    /// The entry the first problem was found at.
    pub problem_entry: Option<i64>,
    /// The newest entry and its seal; what an export's code is made from.
    pub head_entry: i64,
    pub head_hash: String,
}

/// Recomputes every seal from the entries themselves.
pub fn verify_chain(conn: &Connection) -> AppResult<ChainReport> {
    // Same columns as ROW_SQL, in the same order, plus the stored seal.
    let mut stmt = conn.prepare(
        "SELECT a.id, a.at, a.actor_id, a.actor_name, a.actor_role, a.action, a.entity,
                a.entity_id, a.summary, a.details, s.hash
         FROM audit_log a LEFT JOIN audit_seal s ON s.audit_id = a.id
         ORDER BY a.id",
    )?;
    let mut rows = stmt.query([])?;

    let mut previous = GENESIS.to_string();
    let mut entries = 0i64;
    let mut head_entry = 0i64;
    let mut problem: Option<(i64, String)> = None;

    while let Some(r) = rows.next()? {
        let row = read_row(r)?;
        let stored: Option<String> = r.get(10)?;
        entries += 1;
        let expected = hash(&previous, &row);
        match stored {
            None if problem.is_none() => {
                problem = Some((row.0, format!("Entry {} was added outside Phantom School Manager.", row.0)));
            }
            Some(stored) if stored != expected && problem.is_none() => {
                // Was the entry changed, or is one missing before it? If the
                // stored seal follows from this entry and *some* other
                // previous seal, the break is a deletion; we cannot tell which
                // without the deleted row, so say both plainly.
                problem = Some((
                    row.0,
                    format!(
                        "Entry {} does not match its seal: it, or an entry just before it, was changed or removed outside Phantom School Manager.",
                        row.0
                    ),
                ));
            }
            _ => {}
        }
        // Follow the stored seal when there is one, so one bad entry is
        // reported once rather than breaking every check after it.
        previous = r.get::<_, Option<String>>(10)?.unwrap_or(expected);
        head_entry = row.0;
    }

    // Seals whose entries are gone.
    if problem.is_none() {
        let orphan: Option<i64> = conn
            .query_row(
                "SELECT audit_id FROM audit_seal WHERE audit_id NOT IN (SELECT id FROM audit_log) ORDER BY audit_id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = orphan {
            problem = Some((id, format!("Entry {id} was removed outside Phantom School Manager.")));
        }
    }

    Ok(ChainReport {
        entries,
        intact: problem.is_none(),
        problem_entry: problem.as_ref().map(|p| p.0),
        problem: problem.map(|p| p.1),
        head_entry,
        head_hash: previous,
    })
}

// ---------------------------------------------------------------------------
// Verification codes on printed exports
// ---------------------------------------------------------------------------

/// Crockford's Base32: no I, L, O or U, so a code copied by hand off paper
/// cannot be misread.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// "1234-K7QX-3MZT-9PRH": the entry number, then 60 bits of that entry's seal.
pub fn verification_code(entry: i64, hash_hex: &str) -> String {
    let mut bits: u64 = 0;
    for c in hash_hex.chars().take(15) {
        bits = (bits << 4) | c.to_digit(16).unwrap_or(0) as u64;
    }
    let mut chars = Vec::with_capacity(12);
    for i in (0..12).rev() {
        chars.push(ALPHABET[((bits >> (i * 5)) & 31) as usize] as char);
    }
    let body: String = chars.into_iter().collect();
    format!("{entry}-{}-{}-{}", &body[0..4], &body[4..8], &body[8..12])
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeCheck {
    pub matches: bool,
    pub entry: i64,
    pub message: String,
}

/// Checks a code copied from a printed export against the log as it is now.
pub fn check_code(conn: &Connection, code: &str) -> AppResult<CodeCheck> {
    let cleaned: String = code
        .trim()
        .to_uppercase()
        .chars()
        .map(|c| match c {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect();
    let (entry_text, _) = cleaned
        .split_once('-')
        .ok_or_else(|| AppError::validation("A verification code looks like 1234-K7QX-3MZT-9PRH."))?;
    let entry: i64 = entry_text
        .parse()
        .map_err(|_| AppError::validation("A verification code starts with the entry number, like 1234-…"))?;

    // Recompute the chain up to that entry from the entries themselves, so
    // the code is checked against history, not against stored seals that
    // could have been rewritten with it.
    let mut stmt = conn.prepare(&format!("{ROW_SQL} WHERE id <= ?1 ORDER BY id"))?;
    let mut rows = stmt.query(params![entry])?;
    let mut previous = GENESIS.to_string();
    let mut last = 0;
    while let Some(r) = rows.next()? {
        let row = read_row(r)?;
        previous = hash(&previous, &row);
        last = row.0;
    }
    if last != entry {
        return Ok(CodeCheck {
            matches: false,
            entry,
            message: format!("Entry {entry} is not in the log. Either the code was copied wrongly, or the log has lost entries."),
        });
    }
    let expected = verification_code(entry, &previous);
    let matches = expected.replace('-', "") == cleaned.replace('-', "");
    Ok(CodeCheck {
        matches,
        entry,
        message: if matches {
            format!("The printout matches. Nothing in the first {entry} entries has been changed, removed or added since it was printed.")
        } else {
            format!("The printout does NOT match. Either the code was copied wrongly, or the first {entry} entries have been altered since it was printed.")
        },
    })
}

/// Records something that happened with no signed-in user: first run,
/// provisioning, a failed sign-in attempt.
pub fn record_system(
    conn: &Connection,
    action: &str,
    entity: &str,
    entity_id: &str,
    summary: impl AsRef<str>,
) -> AppResult<()> {
    record(conn, None, action, entity, entity_id, summary)
}

/// One row as the audit screen shows it.
#[derive(Debug, serde::Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub at: String,
    pub actor_name: Option<String>,
    pub actor_role: Option<String>,
    pub action: String,
    pub entity: Option<String>,
    pub entity_id: Option<String>,
    pub summary: String,
}

/// Searchable audit trail for the School Admin dashboard (FR-E2).
pub fn search(
    conn: &Connection,
    query: Option<&str>,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<AuditEntry>> {
    let pattern = query
        .map(|q| format!("%{}%", q.trim()))
        .filter(|p| p.len() > 2);

    let mut stmt = conn.prepare(
        "SELECT id, at, actor_name, actor_role, action, entity, entity_id, summary
         FROM audit_log
         WHERE ?1 IS NULL
            OR summary LIKE ?1 COLLATE NOCASE
            OR actor_name LIKE ?1 COLLATE NOCASE
            OR action LIKE ?1 COLLATE NOCASE
         ORDER BY id DESC
         LIMIT ?2 OFFSET ?3",
    )?;

    let rows = stmt.query_map(params![pattern, limit.clamp(1, 500), offset.max(0)], |row| {
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
    })?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    /// What someone with a copy of SQLite does first.
    fn drop_guards(conn: &Connection) {
        conn.execute_batch(
            "DROP TRIGGER trg_audit_no_update; DROP TRIGGER trg_audit_no_delete;
             DROP TRIGGER trg_audit_seal_no_update; DROP TRIGGER trg_audit_seal_no_delete;",
        )
        .unwrap();
    }

    fn with_entries(n: usize) -> Database {
        let db = Database::open_in_memory().unwrap();
        {
            let conn = db.lock();
            for i in 0..n {
                record_system(&conn, "marks.save", "mark", &format!("m{i}"), format!("Saved mark {i}")).unwrap();
            }
        }
        db
    }

    fn id_of(conn: &Connection, summary: &str) -> i64 {
        conn.query_row("SELECT id FROM audit_log WHERE summary = ?1", [summary], |r| r.get(0)).unwrap()
    }

    #[test]
    fn an_untouched_log_is_intact() {
        let db = with_entries(5);
        let report = verify_chain(&db.lock()).unwrap();
        assert!(report.intact, "{:?}", report.problem);
        assert!(report.entries >= 5);
    }

    #[test]
    fn changing_an_entry_is_caught_at_that_entry() {
        let db = with_entries(5);
        let conn = db.lock();
        drop_guards(&conn);
        let target = id_of(&conn, "Saved mark 2");
        conn.execute("UPDATE audit_log SET summary = 'Saved mark 99' WHERE id = ?1", [target]).unwrap();
        let report = verify_chain(&conn).unwrap();
        assert!(!report.intact);
        assert_eq!(report.problem_entry, Some(target));
    }

    #[test]
    fn deleting_an_entry_is_caught() {
        let db = with_entries(5);
        let conn = db.lock();
        drop_guards(&conn);
        let target = id_of(&conn, "Saved mark 2");
        // The seal's foreign key stops a plain delete, so an attacker turns
        // foreign keys off first and leaves the seal behind...
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        conn.execute("DELETE FROM audit_log WHERE id = ?1", [target]).unwrap();
        assert!(!verify_chain(&conn).unwrap().intact);
        // ...or deletes the seal too, and then the next entry no longer
        // follows from the one before it.
        conn.execute("DELETE FROM audit_seal WHERE audit_id = ?1", [target]).unwrap();
        let report = verify_chain(&conn).unwrap();
        assert!(!report.intact);
        assert_eq!(report.problem_entry, Some(target + 1));
    }

    #[test]
    fn an_entry_inserted_with_sql_is_caught() {
        let db = with_entries(3);
        let conn = db.lock();
        conn.execute(
            "INSERT INTO audit_log (at, action, summary) VALUES ('2026-01-01T00:00:00Z', 'marks.save', 'Forged')",
            [],
        )
        .unwrap();
        let report = verify_chain(&conn).unwrap();
        assert!(!report.intact);
        assert!(report.problem.unwrap().contains("added outside"));
    }

    #[test]
    fn a_full_rewrite_fools_the_chain_but_not_the_printed_code() {
        // The honest limit: someone who knows the algorithm can change an
        // entry and recompute every seal after it. The chain then checks.
        // What they cannot change is a code already printed on paper.
        let db = with_entries(5);
        let conn = db.lock();
        let before = verify_chain(&conn).unwrap();
        let printed = verification_code(before.head_entry, &before.head_hash);
        assert!(check_code(&conn, &printed).unwrap().matches);

        drop_guards(&conn);
        let target = id_of(&conn, "Saved mark 1");
        conn.execute("UPDATE audit_log SET summary = 'Saved mark 77' WHERE id = ?1", [target]).unwrap();
        conn.execute("DELETE FROM audit_seal", []).unwrap();
        conn.execute("DELETE FROM settings WHERE key = ?1", [LEGACY_SEALED]).unwrap();
        seal_legacy_once(&conn).unwrap();

        assert!(verify_chain(&conn).unwrap().intact, "the rewrite fooled the chain");
        assert!(!check_code(&conn, &printed).unwrap().matches, "but not the paper");
    }

    #[test]
    fn cutting_off_the_newest_entries_is_caught_by_the_printed_code() {
        let db = with_entries(5);
        let conn = db.lock();
        let report = verify_chain(&conn).unwrap();
        let printed = verification_code(report.head_entry, &report.head_hash);
        drop_guards(&conn);
        conn.execute("DELETE FROM audit_seal WHERE audit_id = ?1", [report.head_entry]).unwrap();
        conn.execute("DELETE FROM audit_log WHERE id = ?1", [report.head_entry]).unwrap();
        assert!(verify_chain(&conn).unwrap().intact, "a truncated chain is still a valid chain");
        let check = check_code(&conn, &printed).unwrap();
        assert!(!check.matches);
        assert!(check.message.contains("not in the log"));
    }

    #[test]
    fn entries_from_before_sealing_are_sealed_once_and_only_once() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.lock();
        drop_guards(&conn);
        // As if these were written before migration 012.
        conn.execute("DELETE FROM settings WHERE key = ?1", [LEGACY_SEALED]).unwrap();
        conn.execute("DELETE FROM audit_seal", []).unwrap();
        conn.execute("INSERT INTO audit_log (at, action, summary) VALUES ('2026-01-01', 'old', 'Old entry')", []).unwrap();
        seal_legacy_once(&conn).unwrap();
        assert!(verify_chain(&conn).unwrap().intact);

        // Anything unsealed after that was not written by the application.
        conn.execute("INSERT INTO audit_log (at, action, summary) VALUES ('2026-01-02', 'x', 'Slipped in')", []).unwrap();
        seal_legacy_once(&conn).unwrap();
        assert!(!verify_chain(&conn).unwrap().intact, "a later start must not launder it");
    }

    #[test]
    fn codes_survive_being_copied_by_hand() {
        let db = with_entries(2);
        let conn = db.lock();
        let report = verify_chain(&conn).unwrap();
        let code = verification_code(report.head_entry, &report.head_hash);
        assert_eq!(code.len(), code.split('-').next().unwrap().len() + 15);
        // Lower case, and the letter O typed for zero, still check.
        let sloppy = code.to_lowercase().replace('0', "o");
        assert!(check_code(&conn, &sloppy).unwrap().matches);
        assert!(check_code(&conn, "not a code").is_err());
    }
}
