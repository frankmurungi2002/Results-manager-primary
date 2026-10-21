//! A term's examinations, changed after setup (FR-B3).
//!
//! The setup wizard asks for BOT, Mid Term and End of Term and how much each
//! counts. Schools change their minds: Mid Term 30% instead of 40%, a fourth
//! test, a different name. Here the office edits one term's examinations at
//! once, so the shares are checked together:
//!
//! * the shares add up to 100%, or are all zero (then the end-of-term paper
//!   alone makes the term mark);
//! * exactly one examination is the end-of-term one;
//! * an examination that already has marks can be renamed or re-weighted but
//!   not removed, so no mark is ever orphaned;
//! * a sealed year cannot be changed: its report cards are history.
//!
//! The report card works the term mark out from the shares when it is
//! printed, so a change shows on the next print.

use std::collections::HashSet;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use tauri::State;

use crate::audit;
use crate::domain::ids::{new_id, prefix};
use crate::error::{AppError, AppResult};
use crate::security::{Permission, Session};
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamEdit {
    /// Empty for a new examination.
    #[serde(default)]
    pub id: Option<String>,
    pub code: String,
    pub name: String,
    /// Share of the term mark, 0 to 1.
    pub weight: f64,
    pub is_final: bool,
}

pub fn save_term_exams_in(conn: &mut Connection, session: &Session, term_id: &str, exams: &[ExamEdit]) -> AppResult<()> {
    session.require(Permission::ManageCalendar)?;
    let (term_name, term_status, year_status): (String, String, String) = conn
        .query_row(
            "SELECT t.name, t.status, y.status FROM terms t JOIN academic_years y ON y.id = t.academic_year_id WHERE t.id = ?1",
            params![term_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That term"))?;
    if year_status == "sealed" {
        return Err(AppError::conflict("That school year is finished and sealed, so its examinations stay as they were."));
    }
    if exams.is_empty() {
        return Err(AppError::validation("A term needs at least one examination."));
    }
    let mut codes = HashSet::new();
    for exam in exams {
        let code = exam.code.trim().to_uppercase();
        if code.is_empty() || exam.name.trim().is_empty() {
            return Err(AppError::validation("Every examination needs a short code (like MID) and a name."));
        }
        if code.chars().count() > 8 {
            return Err(AppError::validation(format!("\"{code}\" is too long for a code. Use up to 8 letters, like MID or EOT.")));
        }
        if !codes.insert(code.clone()) {
            return Err(AppError::validation(format!("Two examinations have the code {code}.")));
        }
        if !(0.0..=1.0).contains(&exam.weight) || !exam.weight.is_finite() {
            return Err(AppError::validation(format!("{}: the share must be between 0% and 100%.", exam.name.trim())));
        }
    }
    let total: f64 = exams.iter().map(|e| e.weight).sum();
    if total > 0.0 && (total - 1.0).abs() > 0.005 {
        return Err(AppError::validation(format!(
            "The shares add up to {:.0}%. Make them add up to 100%, or set them all to 0% to use only the end-of-term paper.",
            total * 100.0
        )));
    }
    if exams.iter().filter(|e| e.is_final).count() != 1 {
        return Err(AppError::validation("Choose exactly one end-of-term examination: it makes the end-of-term report card."));
    }

    let existing: Vec<(String, String)> = {
        let mut stmt = conn.prepare("SELECT id, name FROM exams WHERE term_id = ?1")?;
        let rows = stmt.query_map(params![term_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let collected = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        collected
    };
    let kept: HashSet<&str> = exams.iter().filter_map(|e| e.id.as_deref()).collect();
    for id in &kept {
        if !existing.iter().any(|(e, _)| e == id) {
            return Err(AppError::validation("One of these examinations belongs to another term. Reload and try again."));
        }
    }
    let new_status = match term_status.as_str() {
        "open" => "open",
        "closed" => "closed",
        _ => "planning",
    };
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction()?;
    for (id, name) in &existing {
        if kept.contains(id.as_str()) {
            continue;
        }
        let marks: i64 = tx.query_row("SELECT COUNT(*) FROM marks WHERE exam_id = ?1", params![id], |r| r.get(0))?;
        if marks > 0 {
            return Err(AppError::conflict(format!("{name} already has {marks} mark(s), so it cannot be removed. You can rename it or set its share to 0%.")));
        }
        let papers: i64 = tx.query_row("SELECT COUNT(*) FROM exam_papers WHERE exam_id = ?1", params![id], |r| r.get(0))?;
        if papers > 0 {
            return Err(AppError::conflict(format!("{name} is on the exam timetable. Remove its papers there first, then remove it here.")));
        }
        tx.execute("DELETE FROM exams WHERE id = ?1", params![id])?;
    }
    // Codes are unique in a term: park the kept ones first so two can swap.
    for id in &kept {
        tx.execute("UPDATE exams SET code = '~' || id WHERE id = ?1", params![id])?;
    }
    for (seq, exam) in exams.iter().enumerate() {
        let code = exam.code.trim().to_uppercase();
        let name = exam.name.trim();
        match &exam.id {
            Some(id) => {
                tx.execute(
                    "UPDATE exams SET seq = ?2, code = ?3, name = ?4, weight = ?5, is_final = ?6, updated_at = ?7 WHERE id = ?1",
                    params![id, seq as i64 + 1, code, name, exam.weight, exam.is_final, now],
                )?;
            }
            None => {
                tx.execute(
                    "INSERT INTO exams (id, term_id, seq, code, name, weight, is_final, status, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                    params![new_id(prefix::EXAM), term_id, seq as i64 + 1, code, name, exam.weight, exam.is_final, new_status, now],
                )?;
            }
        }
    }
    let summary: Vec<String> = exams
        .iter()
        .map(|e| format!("{} {:.0}%{}", e.name.trim(), e.weight * 100.0, if e.is_final { " (end of term)" } else { "" }))
        .collect();
    audit::record(&tx, Some(session), "term.exams", "term", term_id, format!("{term_name} examinations: {}", summary.join(", ")))?;
    tx.commit()?;
    Ok(())
}

#[tauri::command]
pub fn save_term_exams(state: State<'_, AppState>, term_id: String, exams: Vec<ExamEdit>) -> AppResult<()> {
    let session = state.sessions.require()?;
    let mut conn = state.db.lock();
    save_term_exams_in(&mut conn, &session, &term_id, &exams)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    fn school() -> crate::db::Database {
        let db = crate::fees::tests::fee_school();
        db.lock()
            .execute_batch(
                "INSERT INTO subjects (id, code, name, created_at, updated_at) VALUES ('mtc', 'MTC', 'Mathematics', 'now', 'now');
                 INSERT INTO class_subjects (id, class_id, subject_id, created_at, updated_at) VALUES ('p5mtc', 'p5', 'mtc', 'now', 'now');
                 INSERT INTO exams (id, term_id, seq, code, name, weight, is_final, status, created_at, updated_at) VALUES
                     ('bot', 't262', 1, 'BOT', 'Beginning of Term', 0, 0, 'open', 'now', 'now'),
                     ('mid', 't262', 2, 'MID', 'Mid Term', 0.4, 0, 'open', 'now', 'now'),
                     ('eot', 't262', 3, 'EOT', 'End of Term', 0.6, 1, 'open', 'now', 'now'),
                     ('old', 't253', 1, 'EOT', 'End of Term', 1, 1, 'closed', 'now', 'now');
                 INSERT INTO marks (id, student_id, class_subject_id, exam_id, score, is_absent, entered_by, entered_at, updated_by, updated_at)
                     VALUES ('m1', 'musa', 'p5mtc', 'mid', 70, 0, 'head', 'now', 'head', 'now');",
            )
            .unwrap();
        db
    }

    fn edit(id: Option<&str>, code: &str, name: &str, weight: f64, is_final: bool) -> ExamEdit {
        ExamEdit { id: id.map(String::from), code: code.into(), name: name.into(), weight, is_final }
    }

    fn head() -> Session {
        test_session("head", Role::Teacher, Some(Office::HeadTeacher), &[])
    }

    #[test]
    fn shares_change_and_a_new_test_is_added_open_like_its_term() {
        let db = school();
        let plan = [
            edit(Some("bot"), "BOT", "Beginning of Term", 0.0, false),
            edit(Some("mid"), "MID", "Mid Term Exam", 0.3, false),
            edit(None, "wk6", "Week 6 Test", 0.1, false),
            edit(Some("eot"), "EOT", "End of Term", 0.6, true),
        ];
        save_term_exams_in(&mut db.lock(), &head(), "t262", &plan).unwrap();
        let conn = db.lock();
        let rows: Vec<(String, String, f64, String)> = {
            let mut stmt = conn.prepare("SELECT code, name, weight, status FROM exams WHERE term_id = 't262' ORDER BY seq").unwrap();
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
            let collected = rows.collect::<rusqlite::Result<Vec<_>>>().unwrap();
            collected
        };
        assert_eq!(rows[1], ("MID".into(), "Mid Term Exam".into(), 0.3, "open".into()));
        assert_eq!(rows[2], ("WK6".into(), "Week 6 Test".into(), 0.1, "open".into()));
        let marks: i64 = conn.query_row("SELECT COUNT(*) FROM marks WHERE exam_id = 'mid'", [], |r| r.get(0)).unwrap();
        assert_eq!(marks, 1, "marks stay with their examination");
    }

    #[test]
    fn bad_plans_are_refused_with_the_reason() {
        let db = school();
        let msg = |plan: &[ExamEdit]| save_term_exams_in(&mut db.lock(), &head(), "t262", plan).unwrap_err().to_string();
        assert!(msg(&[edit(Some("mid"), "MID", "Mid", 0.5, false), edit(Some("eot"), "EOT", "End", 0.6, true)]).contains("add up to 110%"));
        assert!(msg(&[edit(Some("mid"), "MID", "Mid", 0.4, false), edit(Some("eot"), "EOT", "End", 0.6, false)]).contains("exactly one end-of-term"));
        assert!(msg(&[edit(Some("mid"), "EOT", "Mid", 0.4, false), edit(Some("eot"), "eot", "End", 0.6, true)]).contains("Two examinations have the code EOT"));
        assert!(
            msg(&[edit(Some("bot"), "BOT", "BOT", 0.4, false), edit(Some("eot"), "EOT", "End", 0.6, true)]).contains("Mid Term already has 1 mark"),
            "removing an examination with marks"
        );
        // Removing one with no marks is fine; codes may swap.
        save_term_exams_in(&mut db.lock(), &head(), "t262", &[edit(Some("mid"), "EOT", "Mid", 0.0, false), edit(Some("eot"), "MID", "End", 0.0, true)]).unwrap();
        let bot: i64 = db.lock().query_row("SELECT COUNT(*) FROM exams WHERE id = 'bot'", [], |r| r.get(0)).unwrap();
        assert_eq!(bot, 0);

        db.lock().execute("UPDATE academic_years SET status = 'sealed' WHERE id = 'y25'", []).unwrap();
        assert!(save_term_exams_in(&mut db.lock(), &head(), "t253", &[edit(Some("old"), "EOT", "End", 1.0, true)]).unwrap_err().to_string().contains("sealed"));
        let teacher = test_session("t", Role::Teacher, None, &[]);
        assert!(matches!(save_term_exams_in(&mut db.lock(), &teacher, "t262", &[edit(None, "X", "X", 1.0, true)]), Err(AppError::Forbidden)));
    }
}
