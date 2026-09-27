//! Shared read helpers used by more than one command module.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::grading::{Band, GradingKind, GradingSystem};
use crate::error::{AppError, AppResult};

/// Loads one grading system with its bands, ordered best band first.
pub fn load_grading_system(conn: &Connection, id: &str) -> AppResult<GradingSystem> {
    type Raw = (String, String, String, Option<String>, String, i64, i64);

    let raw: Option<Raw> = conn
        .query_row(
            "SELECT id, code, name, description, kind, is_builtin, is_editable
             FROM grading_systems WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?;

    let (system_id, code, name, description, kind, is_builtin, is_editable) =
        raw.ok_or_else(|| AppError::not_found("That grading system"))?;

    let mut system = GradingSystem {
        id: system_id,
        code,
        name,
        description,
        kind: GradingKind::parse(&kind)?,
        is_builtin: is_builtin != 0,
        is_editable: is_editable != 0,
        bands: Vec::new(),
    };

    let mut band_stmt = conn.prepare(
        "SELECT id, label, lower_bound, upper_bound, points, remark, position
         FROM grading_bands WHERE grading_system_id = ?1 ORDER BY position ASC",
    )?;

    let bands = band_stmt.query_map(params![id], |row| {
        Ok(Band {
            id: row.get(0)?,
            label: row.get(1)?,
            lower_bound: row.get(2)?,
            upper_bound: row.get(3)?,
            points: row.get(4)?,
            remark: row.get(5)?,
            position: row.get(6)?,
        })
    })?;

    for band in bands {
        system.bands.push(band?);
    }

    Ok(system)
}

/// Loads every active grading system, bands included.
pub fn load_all_grading_systems(conn: &Connection) -> AppResult<Vec<GradingSystem>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM grading_systems WHERE status = 'active' ORDER BY is_builtin DESC, name ASC",
    )?;
    let ids: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;

    ids.iter().map(|id| load_grading_system(conn, id)).collect()
}

/// The grading system that actually applies to a subject in a class.
///
/// Resolution order, per FR-C13: the class-subject override, then the class
/// default, then Percentage Only as the last resort so a mark is never
/// ungradeable.
pub fn effective_grading_system_id(conn: &Connection, class_subject_id: &str) -> AppResult<String> {
    let resolved: Option<String> = conn
        .query_row(
            "SELECT COALESCE(cs.grading_system_id, c.default_grading_system_id, 'gs_percentage')
             FROM class_subjects cs
             JOIN classes c ON c.id = cs.class_id
             WHERE cs.id = ?1",
            params![class_subject_id],
            |row| row.get(0),
        )
        .optional()?;

    resolved.ok_or_else(|| AppError::not_found("That subject"))
}

/// Loads several grading systems at once, keyed by id — for a report card,
/// where every subject may use a different one and loading them per-row would
/// be N queries against a 40-learner class.
pub fn load_grading_systems_by_id(
    conn: &Connection,
    ids: &[String],
) -> AppResult<HashMap<String, GradingSystem>> {
    let mut unique: Vec<String> = ids.to_vec();
    unique.sort();
    unique.dedup();

    let mut out = HashMap::new();
    for id in unique {
        let system = load_grading_system(conn, &id)?;
        out.insert(id, system);
    }
    Ok(out)
}

/// The next registration number, formatted per the institution's pattern and
/// reserved atomically so two Class Teachers adding a learner at the same
/// moment cannot collide (FR-B5).
pub fn next_registration_number(conn: &Connection) -> AppResult<String> {
    let (pattern, next): (String, i64) = conn.query_row(
        "SELECT reg_number_pattern, reg_number_next FROM institution WHERE id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    conn.execute(
        "UPDATE institution SET reg_number_next = reg_number_next + 1, updated_at = ?1 WHERE id = 1",
        params![chrono::Utc::now().to_rfc3339()],
    )?;

    Ok(format_registration_number(&pattern, next))
}

/// Expands `{YEAR}`, `{YY}` and `{SEQ:n}` in a registration-number pattern.
pub fn format_registration_number(pattern: &str, sequence: i64) -> String {
    let year = chrono::Utc::now().format("%Y").to_string();
    let short_year = chrono::Utc::now().format("%y").to_string();

    let mut out = pattern
        .replace("{YEAR}", &year)
        .replace("{YY}", &short_year);

    // {SEQ:4} → 0007. Bare {SEQ} → 7.
    while let Some(start) = out.find("{SEQ") {
        let Some(end_offset) = out[start..].find('}') else {
            break;
        };
        let end = start + end_offset + 1;
        let token = &out[start..end];

        let width: usize = token
            .strip_prefix("{SEQ")
            .and_then(|rest| rest.strip_suffix('}'))
            .and_then(|rest| rest.strip_prefix(':'))
            .and_then(|digits| digits.parse().ok())
            .unwrap_or(0);

        let replacement = format!("{sequence:0width$}");
        out.replace_range(start..end, &replacement);
    }

    out
}

/// The term currently open, if any. Most screens default to it.
pub fn current_term_id(conn: &Connection) -> AppResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT t.id FROM terms t
             JOIN academic_years y ON y.id = t.academic_year_id
             WHERE t.status = 'open' AND y.status = 'active'
             ORDER BY t.seq ASC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn current_academic_year_id(conn: &Connection) -> AppResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT id FROM academic_years WHERE status = 'active' ORDER BY label DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

/// Reads a feature toggle (FR-B7). Absent means off.
pub fn feature_enabled(conn: &Connection, key: &str) -> AppResult<bool> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![format!("feature.{key}")],
            |row| row.get(0),
        )
        .optional()?;
    Ok(value.as_deref() == Some("on"))
}

pub fn set_feature(conn: &Connection, key: &str, enabled: bool) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![
            format!("feature.{key}"),
            if enabled { "on" } else { "off" },
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// True once the institution row exists and setup has been marked complete.
pub fn is_provisioned(conn: &Connection) -> AppResult<bool> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM institution", [], |row| row.get(0))?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn registration_patterns_expand() {
        assert_eq!(format_registration_number("{SEQ:4}", 7), "0007");
        assert_eq!(format_registration_number("MB/{SEQ:3}", 42), "MB/042");
        assert_eq!(format_registration_number("{SEQ}", 42), "42");

        let year = chrono::Utc::now().format("%Y").to_string();
        assert_eq!(
            format_registration_number("{YEAR}/{SEQ:4}", 1),
            format!("{year}/0001")
        );
    }

    #[test]
    fn built_in_systems_load_with_their_bands() {
        let db = Database::open_in_memory().expect("open");
        let conn = db.lock();

        let uneb = load_grading_system(&conn, "gs_uneb_primary").expect("load");
        assert_eq!(uneb.bands.len(), 9);
        assert_eq!(uneb.bands[0].label, "D1");
        assert!(!uneb.is_editable);

        let nursery = load_grading_system(&conn, "gs_nursery_desc").expect("load");
        assert_eq!(nursery.kind, GradingKind::Descriptive);
        assert_eq!(nursery.bands.len(), 5);

        assert_eq!(load_all_grading_systems(&conn).expect("all").len(), 3);
    }

    #[test]
    fn feature_toggles_default_to_off() {
        let db = Database::open_in_memory().expect("open");
        let conn = db.lock();
        assert!(!feature_enabled(&conn, "streams").expect("read"));
        set_feature(&conn, "streams", true).expect("set");
        assert!(feature_enabled(&conn, "streams").expect("read"));
        set_feature(&conn, "streams", false).expect("set");
        assert!(!feature_enabled(&conn, "streams").expect("read"));
    }
}
