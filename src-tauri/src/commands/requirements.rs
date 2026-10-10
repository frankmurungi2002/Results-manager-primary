//! Term requirements: the reams, brooms, toilet paper and books each learner
//! brings at the start of term, ticked off as they arrive.
//!
//! The office sets the list for the term (for every class, or one class),
//! with what it accepts in cash instead where it does. Class Teachers tick
//! what each learner brought. The list prints on report cards as "School
//! requirements", and a class's outstanding list prints for the parents'
//! meeting.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::domain::ids::new_id;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::{Permission, Session};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementItem {
    pub id: String,
    pub term_id: String,
    pub class_id: Option<String>,
    pub class_name: Option<String>,
    pub name: String,
    pub quantity: i64,
    pub cash_value: Option<i64>,
}

fn items(conn: &Connection, term_id: &str, class_id: Option<&str>) -> AppResult<Vec<RequirementItem>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.term_id, r.class_id, c.name, r.name, r.quantity, r.cash_value
         FROM requirement_items r LEFT JOIN classes c ON c.id = r.class_id
         WHERE r.term_id = ?1 AND r.status = 'active' AND (?2 IS NULL OR r.class_id IS NULL OR r.class_id = ?2)
         ORDER BY r.class_id IS NOT NULL, r.position, r.name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map(params![term_id, class_id], |r| {
            Ok(RequirementItem {
                id: r.get(0)?,
                term_id: r.get(1)?,
                class_id: r.get(2)?,
                class_name: r.get(3)?,
                name: r.get(4)?,
                quantity: r.get(5)?,
                cash_value: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn term_or_current(conn: &Connection, term_id: Option<String>) -> AppResult<String> {
    match term_id {
        Some(t) => Ok(t),
        None => repo::current_term_id(conn)?.ok_or_else(|| AppError::validation("Open a term first.")),
    }
}

#[tauri::command]
pub fn list_requirement_items(state: State<'_, AppState>, term_id: Option<String>) -> AppResult<Vec<RequirementItem>> {
    state.sessions.require()?;
    let conn = state.db.lock();
    let term = term_or_current(&conn, term_id)?;
    items(&conn, &term, None)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementInput {
    pub id: Option<String>,
    pub term_id: Option<String>,
    pub class_id: Option<String>,
    pub name: String,
    pub quantity: i64,
    pub cash_value: Option<i64>,
}

fn may_set(session: &Session) -> AppResult<()> {
    if session.has(Permission::ManageAcademics) || session.has(Permission::ManageLearners) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

pub fn save_item_in(conn: &Connection, session: &Session, input: &RequirementInput) -> AppResult<String> {
    may_set(session)?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::validation("Name the item, e.g. Ream of paper."));
    }
    if !(1..=100).contains(&input.quantity) {
        return Err(AppError::validation("The number each learner brings must be between 1 and 100."));
    }
    let now = Utc::now().to_rfc3339();
    let id = match &input.id {
        Some(id) => {
            conn.execute(
                "UPDATE requirement_items SET name = ?1, quantity = ?2, cash_value = ?3, class_id = ?4, updated_at = ?5 WHERE id = ?6",
                params![name, input.quantity, input.cash_value, input.class_id, now, id],
            )?;
            id.clone()
        }
        None => {
            let term = term_or_current(conn, input.term_id.clone())?;
            let id = new_id("req");
            conn.execute(
                "INSERT INTO requirement_items (id, term_id, class_id, name, quantity, cash_value, position, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, (SELECT COALESCE(MAX(position), 0) + 1 FROM requirement_items WHERE term_id = ?2), ?7, ?7)",
                params![id, term, input.class_id, name, input.quantity, input.cash_value, now],
            )?;
            id
        }
    };
    audit::record(conn, Some(session), "requirements.item", "requirement", &id, format!("Set the requirement \"{name}\" ({} each)", input.quantity))?;
    Ok(id)
}

#[tauri::command]
pub fn save_requirement_item(state: State<'_, AppState>, input: RequirementInput) -> AppResult<String> {
    let session = state.sessions.require()?;
    save_item_in(&state.db.lock(), &session, &input)
}

#[tauri::command]
pub fn retire_requirement_item(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    may_set(&session)?;
    let conn = state.db.lock();
    conn.execute("UPDATE requirement_items SET status = 'retired', updated_at = ?1 WHERE id = ?2", params![Utc::now().to_rfc3339(), id])?;
    audit::record(&conn, Some(&session), "requirements.retire", "requirement", &id, "Removed a requirement from the term's list")?;
    Ok(())
}

/// Copies last term's list into this one.
#[tauri::command]
pub fn copy_requirements(state: State<'_, AppState>, from_term_id: String, to_term_id: String) -> AppResult<usize> {
    let session = state.sessions.require()?;
    may_set(&session)?;
    let conn = state.db.lock();
    let now = Utc::now().to_rfc3339();
    let n = conn.execute(
        "INSERT INTO requirement_items (id, term_id, class_id, name, quantity, cash_value, position, created_at, updated_at)
         SELECT 'req_' || lower(hex(randomblob(10))), ?2, class_id, name, quantity, cash_value, position, ?3, ?3
         FROM requirement_items WHERE term_id = ?1 AND status = 'active'",
        params![from_term_id, to_term_id, now],
    )?;
    audit::record(&conn, Some(&session), "requirements.copy", "term", &to_term_id, format!("Copied {n} requirement(s) from another term"))?;
    Ok(n)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementEntry {
    pub item_id: String,
    pub quantity: i64,
    pub cash_paid: i64,
    /// Brought in full, or paid the cash instead.
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementRow {
    pub student_id: String,
    pub full_name: String,
    pub entries: Vec<RequirementEntry>,
    pub outstanding: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementSheet {
    pub term_id: String,
    pub term_name: String,
    pub class_id: String,
    pub class_name: String,
    pub items: Vec<RequirementItem>,
    pub rows: Vec<RequirementRow>,
    pub complete: usize,
}

pub fn sheet(conn: &Connection, class_id: &str, term_id: &str) -> AppResult<RequirementSheet> {
    let items = items(conn, term_id, Some(class_id))?;
    let year: String = conn.query_row("SELECT academic_year_id FROM terms WHERE id = ?1", params![term_id], |r| r.get(0))?;
    let (class_name, term_name): (String, String) = conn.query_row(
        "SELECT (SELECT name FROM classes WHERE id = ?1), (SELECT name FROM terms WHERE id = ?2)",
        params![class_id, term_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.full_name FROM enrollments e JOIN students s ON s.id = e.student_id
         WHERE e.class_id = ?1 AND e.academic_year_id = ?2 AND e.status = 'active' ORDER BY s.full_name COLLATE NOCASE",
    )?;
    let learners: Vec<(String, String)> = stmt.query_map(params![class_id, year], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut rows = Vec::new();
    let mut complete = 0;
    for (id, name) in learners {
        let mut entries = Vec::new();
        let mut outstanding = Vec::new();
        for item in &items {
            let got: Option<(i64, i64)> = conn
                .query_row(
                    "SELECT quantity, cash_paid FROM requirement_received WHERE student_id = ?1 AND item_id = ?2",
                    params![id, item.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let (quantity, cash_paid) = got.unwrap_or((0, 0));
            let done = quantity >= item.quantity || item.cash_value.is_some_and(|v| v > 0 && cash_paid >= v);
            if !done {
                outstanding.push(if item.quantity > 1 && quantity > 0 {
                    format!("{} more {} (of {})", item.quantity - quantity, item.name, item.quantity)
                } else if item.quantity > 1 {
                    format!("{} {}", item.quantity, item.name)
                } else {
                    item.name.clone()
                });
            }
            entries.push(RequirementEntry { item_id: item.id.clone(), quantity, cash_paid, complete: done });
        }
        if outstanding.is_empty() {
            complete += 1;
        }
        rows.push(RequirementRow { student_id: id, full_name: name, entries, outstanding });
    }
    Ok(RequirementSheet { term_id: term_id.into(), term_name, class_id: class_id.into(), class_name, items, rows, complete })
}

#[tauri::command]
pub fn requirements_sheet(state: State<'_, AppState>, class_id: String, term_id: Option<String>) -> AppResult<RequirementSheet> {
    let session = state.sessions.require()?;
    session.require_view_learners_of(&class_id)?;
    let conn = state.db.lock();
    let term = term_or_current(&conn, term_id)?;
    sheet(&conn, &class_id, &term)
}

pub fn record_in(conn: &Connection, session: &Session, student_id: &str, item_id: &str, quantity: i64, cash_paid: i64) -> AppResult<()> {
    let class_id: String = conn
        .query_row(
            "SELECT e.class_id FROM enrollments e JOIN terms t ON t.academic_year_id = e.academic_year_id
             JOIN requirement_items r ON r.term_id = t.id
             WHERE e.student_id = ?1 AND r.id = ?2 AND e.status = 'active'",
            params![student_id, item_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That learner or item"))?;
    if !(session.may_manage_class(&class_id) || session.has(Permission::ManageLearners)) {
        return Err(AppError::Forbidden);
    }
    if !(0..=1000).contains(&quantity) || cash_paid < 0 {
        return Err(AppError::validation("Check the number."));
    }
    conn.execute(
        "INSERT INTO requirement_received (student_id, item_id, quantity, cash_paid, updated_by, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (student_id, item_id) DO UPDATE SET quantity = excluded.quantity, cash_paid = excluded.cash_paid,
             updated_by = excluded.updated_by, updated_at = excluded.updated_at",
        params![student_id, item_id, quantity, cash_paid, session.user_id, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

#[tauri::command]
pub fn record_requirement(
    state: State<'_, AppState>,
    student_id: String,
    item_id: String,
    quantity: i64,
    cash_paid: i64,
) -> AppResult<()> {
    let session = state.sessions.require()?;
    record_in(&state.db.lock(), &session, &student_id, &item_id, quantity, cash_paid)
}

#[tauri::command]
pub fn build_requirements_list(
    state: State<'_, AppState>,
    class_id: String,
    term_id: Option<String>,
) -> AppResult<crate::commands::reports::DocumentEnvelope<RequirementSheet>> {
    let session = state.sessions.require()?;
    session.require_view_learners_of(&class_id)?;
    let conn = state.db.lock();
    let term = term_or_current(&conn, term_id)?;
    let body = sheet(&conn, &class_id, &term)?;
    crate::commands::reports::envelope(&conn, "requirements", body)
}

/// The list as words, for the report card's "School requirements" line.
pub fn as_text(conn: &Connection, term_id: &str, class_id: &str) -> AppResult<Option<String>> {
    let list = items(conn, term_id, Some(class_id))?;
    if list.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        list.iter()
            .map(|i| if i.quantity > 1 { format!("{} x {}", i.quantity, i.name) } else { i.name.clone() })
            .collect::<Vec<_>>()
            .join(", "),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fees::tests::fee_school;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    #[test]
    fn a_learner_is_complete_with_the_items_or_the_cash_instead() {
        let db = fee_school();
        let conn = db.lock();
        let head = test_session("head", Role::Teacher, Some(Office::HeadTeacher), &[]);
        let ream = save_item_in(&conn, &head, &RequirementInput { id: None, term_id: Some("t262".into()), class_id: None, name: "Ream of paper".into(), quantity: 1, cash_value: Some(20_000) }).unwrap();
        let brooms = save_item_in(&conn, &head, &RequirementInput { id: None, term_id: Some("t262".into()), class_id: Some("p5".into()), name: "Broom".into(), quantity: 2, cash_value: None }).unwrap();
        save_item_in(&conn, &head, &RequirementInput { id: None, term_id: Some("t262".into()), class_id: Some("p4".into()), name: "Only P4".into(), quantity: 1, cash_value: None }).unwrap();

        let teacher = test_session("bursar", Role::Teacher, None, &["p5"]);
        record_in(&conn, &teacher, "amina", &ream, 0, 20_000).unwrap();
        record_in(&conn, &teacher, "amina", &brooms, 2, 0).unwrap();
        record_in(&conn, &teacher, "musa", &brooms, 1, 0).unwrap();
        let other = test_session("o", Role::Teacher, None, &["p4"]);
        assert!(matches!(record_in(&conn, &other, "musa", &brooms, 2, 0), Err(AppError::Forbidden)));

        let s = sheet(&conn, "p5", "t262").unwrap();
        assert_eq!(s.items.len(), 2, "P4's item is not P5's");
        let amina = s.rows.iter().find(|r| r.student_id == "amina").unwrap();
        assert!(amina.outstanding.is_empty(), "paid cash for the ream");
        let musa = s.rows.iter().find(|r| r.student_id == "musa").unwrap();
        assert_eq!(musa.outstanding, vec!["Ream of paper".to_string(), "1 more Broom (of 2)".to_string()]);
        assert_eq!(s.complete, 1);
        assert_eq!(as_text(&conn, "t262", "p5").unwrap().as_deref(), Some("Ream of paper, 2 x Broom"));
    }
}
