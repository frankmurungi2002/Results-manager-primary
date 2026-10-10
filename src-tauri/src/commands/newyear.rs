//! Starting a new school year: every learner moves up a class, repeats, or
//! leaves, in one step the office checks before it happens.
//!
//! Nothing from the old year is touched except to close it: its marks,
//! report cards, attendance and fees stay exactly as they were, and fees
//! owed carry into the new year on their own (the ledger runs across years).
//!
//! The plan is shown first, class by class. Each learner moves up by
//! default; a learner whose average for the year's last term was under the
//! school's mark is flagged "consider repeating", never held back silently.
//! The top class (P7) leaves; they are kept as former pupils.

use std::collections::HashMap;

use chrono::{NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::domain::ids::{new_id, prefix};
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::{Permission, Session};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotionLearner {
    pub student_id: String,
    pub full_name: String,
    pub reg_number: String,
    /// Mean percentage in the year's last term with marks.
    pub average: Option<f64>,
    /// `promote`, `repeat` or `leave`.
    pub action: String,
    /// Below the school's mark: shown, never applied on its own.
    pub consider_repeating: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotionClass {
    pub class_id: String,
    pub class_name: String,
    /// Where learners who move up go; none for the top class.
    pub next_class_id: Option<String>,
    pub next_class_name: Option<String>,
    pub learners: Vec<PromotionLearner>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotionPlan {
    pub from_year_id: String,
    pub from_year: String,
    pub suggested_label: String,
    /// The term whose results the averages come from.
    pub results_term: Option<String>,
    pub repeat_below: f64,
    pub classes: Vec<PromotionClass>,
}

/// The order classes go up in: nursery to P7.
fn ladder(conn: &Connection) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT id, name FROM classes WHERE status = 'active' ORDER BY ladder_position ASC")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn promotion_plan(conn: &Connection, repeat_below: f64) -> AppResult<PromotionPlan> {
    let (year_id, label): (String, String) = conn
        .query_row(
            "SELECT id, label FROM academic_years WHERE status = 'active' ORDER BY label DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::validation("There is no school year running."))?;
    let suggested_label = label.parse::<i64>().map(|y| (y + 1).to_string()).unwrap_or_default();
    // The last term of the year that has any marks.
    let results_term: Option<(String, String)> = conn
        .query_row(
            "SELECT t.id, t.name FROM terms t
             WHERE t.academic_year_id = ?1
               AND EXISTS (SELECT 1 FROM exams e JOIN marks m ON m.exam_id = e.id WHERE e.term_id = t.id)
             ORDER BY t.seq DESC LIMIT 1",
            params![year_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let classes = ladder(conn)?;
    let mut out = Vec::new();
    for (index, (class_id, class_name)) in classes.iter().enumerate() {
        let next = classes.get(index + 1);
        let averages = match &results_term {
            Some((term, _)) => crate::commands::reports::term_averages(conn, class_id, term).unwrap_or_default(),
            None => HashMap::new(),
        };
        let mut stmt = conn.prepare(
            "SELECT s.id, s.full_name, s.reg_number FROM enrollments e JOIN students s ON s.id = e.student_id
             WHERE e.academic_year_id = ?1 AND e.class_id = ?2 AND e.status = 'active'
             ORDER BY s.full_name COLLATE NOCASE",
        )?;
        let learners = stmt
            .query_map(params![year_id, class_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(|(id, name, reg)| {
                let average = averages.get(&id).copied().flatten();
                PromotionLearner {
                    consider_repeating: next.is_some() && average.is_some_and(|a| a < repeat_below),
                    action: if next.is_some() { "promote".into() } else { "leave".into() },
                    student_id: id,
                    full_name: name,
                    reg_number: reg,
                    average,
                }
            })
            .collect::<Vec<_>>();
        if learners.is_empty() {
            continue;
        }
        out.push(PromotionClass {
            class_id: class_id.clone(),
            class_name: class_name.clone(),
            next_class_id: next.map(|n| n.0.clone()),
            next_class_name: next.map(|n| n.1.clone()),
            learners,
        });
    }
    Ok(PromotionPlan {
        from_year_id: year_id,
        from_year: label,
        suggested_label,
        results_term: results_term.map(|t| t.1),
        repeat_below,
        classes: out,
    })
}

#[tauri::command]
pub fn get_promotion_plan(state: State<'_, AppState>, repeat_below: Option<f64>) -> AppResult<PromotionPlan> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageCalendar)?;
    promotion_plan(&state.db.lock(), repeat_below.unwrap_or(40.0))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTerm {
    pub name: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub student_id: String,
    /// `promote`, `repeat` or `leave`.
    pub action: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewYearRequest {
    pub label: String,
    pub terms: Vec<NewTerm>,
    /// Changes from the plan's defaults; everyone not named follows the plan.
    pub decisions: Vec<Decision>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewYearResult {
    pub year_label: String,
    pub promoted: usize,
    pub repeating: usize,
    pub left: usize,
    pub first_term: String,
}

pub fn start_new_year_in(conn: &Connection, session: &Session, request: &NewYearRequest) -> AppResult<NewYearResult> {
    session.require(Permission::ManageCalendar)?;
    let label = request.label.trim();
    if label.is_empty() {
        return Err(AppError::validation("Name the new year, e.g. 2027."));
    }
    let exists: bool = conn.query_row("SELECT EXISTS (SELECT 1 FROM academic_years WHERE label = ?1)", params![label], |r| r.get(0))?;
    if exists {
        return Err(AppError::conflict(format!("There is already a school year called {label}.")));
    }
    if request.terms.is_empty() || request.terms.len() > 4 {
        return Err(AppError::validation("A school year has between one and four terms."));
    }
    for term in &request.terms {
        if term.name.trim().is_empty() {
            return Err(AppError::validation("Name every term."));
        }
        for date in [&term.start_date, &term.end_date].into_iter().flatten() {
            NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| AppError::validation("A term date is not a date."))?;
        }
    }
    let plan = promotion_plan(conn, 0.0)?;
    let overrides: HashMap<&str, &str> = request.decisions.iter().map(|d| (d.student_id.as_str(), d.action.as_str())).collect();
    for action in overrides.values() {
        if !matches!(*action, "promote" | "repeat" | "leave") {
            return Err(AppError::validation("Each learner moves up, repeats or leaves."));
        }
    }

    let now = Utc::now().to_rfc3339();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    // Close the old year: the open term closed, the year sealed. A term that
    // never opened stays unopened: closing it would bill learners its fees
    // for a term that never happened.
    conn.execute(
        "UPDATE exams SET status = 'closed', updated_at = ?1
         WHERE status = 'open' AND term_id IN (SELECT id FROM terms WHERE academic_year_id = ?2)",
        params![now, plan.from_year_id],
    )?;
    conn.execute(
        "UPDATE terms SET status = 'closed', updated_at = ?1 WHERE academic_year_id = ?2 AND status = 'open'",
        params![now, plan.from_year_id],
    )?;
    conn.execute(
        "UPDATE academic_years SET status = 'sealed', sealed_at = ?1, updated_at = ?1 WHERE id = ?2",
        params![now, plan.from_year_id],
    )?;

    // The new year, its terms (the first open), and the same examinations
    // as the old year's first term.
    let year_id = new_id(prefix::YEAR);
    conn.execute(
        "INSERT INTO academic_years (id, label, status, created_at, updated_at) VALUES (?1, ?2, 'active', ?3, ?3)",
        params![year_id, label, now],
    )?;
    let exams: Vec<(String, String, f64, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT e.code, e.name, e.weight, e.is_final FROM exams e JOIN terms t ON t.id = e.term_id
             WHERE t.academic_year_id = ?1 AND t.seq = (SELECT MIN(seq) FROM terms WHERE academic_year_id = ?1)
             ORDER BY e.seq",
        )?;
        let rows = stmt.query_map(params![plan.from_year_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let mut first_term = String::new();
    for (index, term) in request.terms.iter().enumerate() {
        let term_id = new_id(prefix::TERM);
        let status = if index == 0 { "open" } else { "planning" };
        if index == 0 {
            first_term = term.name.trim().to_string();
        }
        conn.execute(
            "INSERT INTO terms (id, academic_year_id, seq, name, start_date, end_date, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![term_id, year_id, index as i64 + 1, term.name.trim(), term.start_date, term.end_date, status, now],
        )?;
        for (seq, (code, name, weight, is_final)) in exams.iter().enumerate() {
            conn.execute(
                "INSERT INTO exams (id, term_id, seq, code, name, weight, is_final, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                params![new_id(prefix::EXAM), term_id, seq as i64 + 1, code, name, weight, is_final, status, now],
            )?;
        }
    }

    // Every learner: up a class, the same class again, or out.
    let streams: HashMap<(String, String), String> = {
        let mut stmt = conn.prepare("SELECT class_id, name, id FROM streams WHERE status = 'active'")?;
        let rows = stmt
            .query_map([], |r| Ok(((r.get::<_, String>(0)?, r.get::<_, String>(1)?), r.get::<_, String>(2)?)))?
            .collect::<rusqlite::Result<HashMap<_, _>>>()?;
        rows
    };
    let (mut promoted, mut repeating, mut left) = (0, 0, 0);
    for class in &plan.classes {
        for learner in &class.learners {
            let action = overrides.get(learner.student_id.as_str()).copied().unwrap_or(&learner.action);
            let (old_enrollment, stream_name): (String, Option<String>) = conn.query_row(
                "SELECT e.id, st.name FROM enrollments e LEFT JOIN streams st ON st.id = e.stream_id
                 WHERE e.student_id = ?1 AND e.academic_year_id = ?2",
                params![learner.student_id, plan.from_year_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let target = match action {
                "promote" => match &class.next_class_id {
                    Some(next) => Some(next.clone()),
                    None => None,
                },
                "repeat" => Some(class.class_id.clone()),
                _ => None,
            };
            match target {
                Some(target) => {
                    conn.execute(
                        "UPDATE enrollments SET status = 'promoted', updated_at = ?1 WHERE id = ?2",
                        params![now, old_enrollment],
                    )?;
                    let stream_id = stream_name.and_then(|n| streams.get(&(target.clone(), n)).cloned());
                    conn.execute(
                        "INSERT INTO enrollments (id, student_id, class_id, stream_id, academic_year_id, status, joined_at, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6, ?6)",
                        params![new_id(prefix::ENROLLMENT), learner.student_id, target, stream_id, year_id, now],
                    )?;
                    if target == class.class_id {
                        repeating += 1;
                    } else {
                        promoted += 1;
                    }
                }
                None => {
                    // The top class graduates; anyone else who leaves is
                    // dropped, with the reason on record.
                    let graduated = class.next_class_id.is_none();
                    conn.execute(
                        "UPDATE enrollments SET status = ?1, left_at = ?2, drop_reason = ?3, updated_at = ?4 WHERE id = ?5",
                        params![
                            if graduated { "graduated" } else { "dropped" },
                            today,
                            if graduated { "Completed the top class" } else { "Left at the end of the year" },
                            now,
                            old_enrollment
                        ],
                    )?;
                    conn.execute(
                        "UPDATE students SET status = ?1, updated_at = ?2 WHERE id = ?3",
                        params![if graduated { "graduated" } else { "dropped" }, now, learner.student_id],
                    )?;
                    left += 1;
                }
            }
        }
    }
    audit::record(
        conn,
        Some(session),
        "year.start",
        "academic_year",
        &year_id,
        format!(
            "Started the {label} school year: {promoted} moved up, {repeating} repeating, {left} left; {} closed",
            plan.from_year
        ),
    )?;
    Ok(NewYearResult { year_label: label.into(), promoted, repeating, left, first_term })
}

#[tauri::command]
pub fn start_new_year(state: State<'_, AppState>, request: NewYearRequest) -> AppResult<NewYearResult> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let tx = conn.unchecked_transaction()?;
    let result = start_new_year_in(&tx, &session, &request)?;
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fees::tests::fee_school;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    fn school() -> crate::db::Database {
        let db = fee_school();
        db.lock()
            .execute_batch(
                "INSERT INTO classes (id, code, name, level_kind, ladder_position, created_at, updated_at) VALUES
                     ('p7', 'P7', 'P7', 'primary', 10, 'now', 'now');
                 INSERT INTO students (id, reg_number, full_name, created_at, updated_at) VALUES
                     ('kato', 'R9', 'Kato P7', 'now', 'now'),
                     ('rep', 'R10', 'Repeater P4', 'now', 'now');
                 INSERT INTO enrollments (id, student_id, class_id, academic_year_id, status, joined_at, created_at, updated_at) VALUES
                     ('e9', 'kato', 'p7', 'y26', 'active', 'now', 'now', 'now'),
                     ('e10', 'rep', 'p4', 'y26', 'active', 'now', 'now', 'now');
                 INSERT INTO exams (id, term_id, seq, code, name, weight, is_final, status, created_at, updated_at) VALUES
                     ('x1', 't261', 1, 'EOT', 'End of Term', 1, 1, 'closed', 'now', 'now');",
            )
            .unwrap();
        db
    }

    fn head() -> Session {
        test_session("head", Role::Teacher, Some(Office::HeadTeacher), &[])
    }

    #[test]
    fn the_plan_moves_everyone_up_and_the_top_class_out() {
        let db = school();
        let conn = db.lock();
        let plan = promotion_plan(&conn, 40.0).unwrap();
        assert_eq!(plan.suggested_label, "2027");
        let p4 = plan.classes.iter().find(|c| c.class_id == "p4").unwrap();
        assert_eq!(p4.next_class_id.as_deref(), Some("p5"));
        let p7 = plan.classes.iter().find(|c| c.class_id == "p7").unwrap();
        assert_eq!(p7.next_class_id, None);
        assert_eq!(p7.learners[0].action, "leave");
    }

    #[test]
    fn starting_the_year_moves_learners_keeps_history_and_carries_fees() {
        let db = school();
        let conn = db.lock();
        let balance_before = crate::commands::fees::current_balance(&conn, "musa").unwrap();
        let request = NewYearRequest {
            label: "2027".into(),
            terms: vec![
                NewTerm { name: "Term 1".into(), start_date: Some("2027-02-01".into()), end_date: None },
                NewTerm { name: "Term 2".into(), start_date: None, end_date: None },
            ],
            decisions: vec![Decision { student_id: "rep".into(), action: "repeat".into() }],
        };
        let result = start_new_year_in(&conn, &head(), &request).unwrap();
        assert_eq!((result.promoted, result.repeating, result.left), (2, 1, 1));

        let class_of = |id: &str| -> Option<String> {
            conn.query_row(
                "SELECT e.class_id FROM enrollments e JOIN academic_years y ON y.id = e.academic_year_id
                 WHERE e.student_id = ?1 AND y.label = '2027'",
                [id],
                |r| r.get(0),
            )
            .optional()
            .unwrap()
        };
        assert_eq!(class_of("amina").as_deref(), Some("p7"), "P5 moves to the next class up");
        assert_eq!(class_of("rep").as_deref(), Some("p4"), "repeating");
        assert_eq!(class_of("kato"), None, "P7 left");
        let status: String = conn.query_row("SELECT status FROM students WHERE id = 'kato'", [], |r| r.get(0)).unwrap();
        assert_eq!(status, "graduated");
        let old: String = conn.query_row("SELECT status FROM academic_years WHERE id = 'y26'", [], |r| r.get(0)).unwrap();
        assert_eq!(old, "sealed");
        assert!(repo::current_academic_year_id(&conn).unwrap().is_some());
        let exams: i64 = conn
            .query_row("SELECT COUNT(*) FROM exams e JOIN terms t ON t.id = e.term_id JOIN academic_years y ON y.id = t.academic_year_id WHERE y.label = '2027'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(exams, 2, "the old year's examinations, for each new term");
        // What Musa owed is still owed.
        assert_eq!(crate::commands::fees::current_balance(&conn, "musa").unwrap(), balance_before);
        // And the year cannot be started twice.
        assert!(start_new_year_in(&conn, &head(), &request).is_err());
    }

    #[test]
    fn only_those_who_run_the_calendar_may_start_a_year() {
        let db = school();
        let conn = db.lock();
        let bursar = test_session("bursar", Role::Teacher, Some(Office::Bursar), &[]);
        let request = NewYearRequest { label: "2027".into(), terms: vec![NewTerm { name: "Term 1".into(), start_date: None, end_date: None }], decisions: vec![] };
        assert!(matches!(start_new_year_in(&conn, &bursar, &request), Err(AppError::Forbidden)));
    }
}
