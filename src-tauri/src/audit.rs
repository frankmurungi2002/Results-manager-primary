//! The audit trail (FR-E2).
//!
//! SRS 16.6 is explicit that this should be close to free *if it is designed
//! into the data layer up front rather than bolted on later*. So it lives
//! here, takes the same connection the write took, and is written inside the
//! caller's transaction — an action and its audit entry commit together or not
//! at all.

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::error::AppResult;
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
    let (actor_id, actor_name, actor_role) = match actor {
        Some(session) => (
            Some(session.user_id.as_str()),
            Some(session.full_name.as_str()),
            Some(session.role.as_str()),
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

    Ok(())
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
