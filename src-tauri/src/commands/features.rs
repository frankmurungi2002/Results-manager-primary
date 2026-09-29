//! The optional features behind the FR-B7 toggles:
//!
//! * FR-C11 — streams: up to 20 per class, each with its own roster and
//!   Class Teacher. The roster and teacher links already live on
//!   `enrollments.stream_id` and `teacher_assignments.stream_id`; this file
//!   manages the streams themselves.
//! * FR-C12 — weekly assignments: a per-learner, per-subject score for each
//!   week of a term, summarised on the back of the report card.
//!
//! Exam permits (the third toggle) are a document, so they live with the
//! other documents in `reports.rs`.

use std::collections::HashMap;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::domain::ids::{new_id, prefix};
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::session::Session;
use crate::state::AppState;

const MAX_STREAMS_PER_CLASS: i64 = 20;
const MAX_WEEKS: i64 = 20;

fn require_feature(conn: &Connection, key: &str, label: &str) -> AppResult<()> {
    if repo::feature_enabled(conn, key)? {
        Ok(())
    } else {
        Err(AppError::conflict(format!(
            "{label} is switched off. Turn it on under Settings, Optional features."
        )))
    }
}

// ---------------------------------------------------------------------------
// FR-C11 — streams
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamRow {
    pub id: String,
    pub class_id: String,
    pub name: String,
    pub learner_count: i64,
    pub class_teacher_name: Option<String>,
}

#[tauri::command]
pub fn list_streams(state: State<'_, AppState>, class_id: String) -> AppResult<Vec<StreamRow>> {
    let session = state.sessions.require()?;
    session.require_view_class(&class_id)?;
    let conn = state.db.lock();
    let year_id = repo::current_academic_year_id(&conn)?;

    let mut stmt = conn.prepare(
        "SELECT st.id, st.class_id, st.name,
                (SELECT COUNT(*) FROM enrollments e
                  WHERE e.stream_id = st.id AND e.status = 'active'
                    AND (?2 IS NULL OR e.academic_year_id = ?2)),
                (SELECT u.full_name FROM teacher_assignments ta
                  JOIN users u ON u.id = ta.user_id
                  WHERE ta.stream_id = st.id AND ta.role = 'class_teacher'
                    AND ta.status = 'active' LIMIT 1)
         FROM streams st
         WHERE st.class_id = ?1 AND st.status = 'active'
         ORDER BY st.name COLLATE NOCASE ASC",
    )?;
    let rows = stmt
        .query_map(params![class_id, year_id], |row| {
            Ok(StreamRow {
                id: row.get(0)?,
                class_id: row.get(1)?,
                name: row.get(2)?,
                learner_count: row.get(3)?,
                class_teacher_name: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Adds a stream to a class, or renames one when `id` is given.
#[tauri::command]
pub fn save_stream(
    state: State<'_, AppState>,
    class_id: String,
    id: Option<String>,
    name: String,
) -> AppResult<String> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    let conn = state.db.lock();
    require_feature(&conn, "streams", "Streams")?;

    let name = name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err(AppError::validation("A stream name is 1 to 40 characters, e.g. \"Blue\" or \"East\"."));
    }

    let clash: Option<String> = conn
        .query_row(
            "SELECT id FROM streams WHERE class_id = ?1 AND name = ?2 COLLATE NOCASE",
            params![class_id, name],
            |row| row.get(0),
        )
        .optional()?;
    if clash.is_some() && clash != id {
        return Err(AppError::conflict(format!("This class already has a stream called {name}.")));
    }

    let now = Utc::now().to_rfc3339();
    let class_name: String = conn
        .query_row("SELECT name FROM classes WHERE id = ?1", params![class_id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That class"))?;

    match id {
        Some(stream_id) => {
            conn.execute(
                "UPDATE streams SET name = ?1, updated_at = ?2 WHERE id = ?3 AND class_id = ?4",
                params![name, now, stream_id, class_id],
            )?;
            audit::record(
                &conn,
                Some(&session),
                "stream.rename",
                "stream",
                &stream_id,
                format!("Renamed a {class_name} stream to {name}"),
            )?;
            Ok(stream_id)
        }
        None => {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM streams WHERE class_id = ?1 AND status = 'active'",
                params![class_id],
                |row| row.get(0),
            )?;
            if count >= MAX_STREAMS_PER_CLASS {
                return Err(AppError::conflict(format!(
                    "A class can have at most {MAX_STREAMS_PER_CLASS} streams."
                )));
            }
            // A retired stream of the same name comes back rather than duplicating.
            if let Some(existing) = clash {
                conn.execute(
                    "UPDATE streams SET status = 'active', updated_at = ?1 WHERE id = ?2",
                    params![now, existing],
                )?;
                return Ok(existing);
            }
            let stream_id = new_id(prefix::STREAM);
            conn.execute(
                "INSERT INTO streams (id, class_id, name, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'active', ?4, ?4)",
                params![stream_id, class_id, name, now],
            )?;
            audit::record(
                &conn,
                Some(&session),
                "stream.add",
                "stream",
                &stream_id,
                format!("Added stream {name} to {class_name}"),
            )?;
            Ok(stream_id)
        }
    }
}

/// Retires an empty stream. Its history stays; a stream with learners in it
/// must be emptied first so nobody is silently left without one.
#[tauri::command]
pub fn retire_stream(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    let conn = state.db.lock();

    let (name, learners): (String, i64) = conn
        .query_row(
            "SELECT st.name,
                    (SELECT COUNT(*) FROM enrollments e WHERE e.stream_id = st.id AND e.status = 'active')
             FROM streams st WHERE st.id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That stream"))?;
    if learners > 0 {
        return Err(AppError::conflict(format!(
            "{name} still has {learners} learner{}. Move them to another stream first.",
            if learners == 1 { "" } else { "s" }
        )));
    }

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE streams SET status = 'retired', updated_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    conn.execute(
        "UPDATE teacher_assignments SET status = 'retired', updated_at = ?1
         WHERE stream_id = ?2 AND status = 'active'",
        params![now, id],
    )?;
    audit::record(&conn, Some(&session), "stream.retire", "stream", &id, format!("Retired stream {name}"))?;
    Ok(())
}

/// Puts a learner in a stream of their current class, or takes them out.
#[tauri::command]
pub fn set_student_stream(
    state: State<'_, AppState>,
    student_id: String,
    stream_id: Option<String>,
) -> AppResult<()> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    require_feature(&conn, "streams", "Streams")?;
    let year_id = repo::current_academic_year_id(&conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;

    let (class_id, full_name): (String, String) = conn
        .query_row(
            "SELECT e.class_id, s.full_name FROM enrollments e
             JOIN students s ON s.id = e.student_id
             WHERE e.student_id = ?1 AND e.academic_year_id = ?2 AND e.status = 'active'",
            params![student_id, year_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::validation("That learner is not enrolled this year."))?;
    session.require_manage_class(&class_id)?;

    let stream_name: Option<String> = match &stream_id {
        Some(id) => Some(
            conn.query_row(
                "SELECT name FROM streams WHERE id = ?1 AND class_id = ?2 AND status = 'active'",
                params![id, class_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::validation("That stream is not part of this learner's class."))?,
        ),
        None => None,
    };

    conn.execute(
        "UPDATE enrollments SET stream_id = ?1, updated_at = ?2
         WHERE student_id = ?3 AND academic_year_id = ?4 AND status = 'active'",
        params![stream_id, Utc::now().to_rfc3339(), student_id, year_id],
    )?;
    audit::record(
        &conn,
        Some(&session),
        "student.stream",
        "student",
        &student_id,
        match stream_name {
            Some(name) => format!("Moved {full_name} to stream {name}"),
            None => format!("Took {full_name} out of their stream"),
        },
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// FR-C12 — weekly assignments
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklySheet {
    pub class_subject_id: String,
    pub class_name: String,
    pub subject_name: String,
    pub term_name: String,
    pub week: i64,
    pub out_of: f64,
    /// Weeks that already have at least one score, for the week picker.
    pub weeks_recorded: Vec<i64>,
    pub rows: Vec<WeeklyRow>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyRow {
    pub student_id: String,
    pub reg_number: String,
    pub full_name: String,
    pub stream_name: Option<String>,
    pub score: Option<f64>,
    pub remark: Option<String>,
}

fn class_of_subject(conn: &Connection, class_subject_id: &str) -> AppResult<(String, String, String)> {
    conn.query_row(
        "SELECT cs.class_id, c.name, COALESCE(cs.display_name, s.name)
         FROM class_subjects cs
         JOIN classes c ON c.id = cs.class_id
         JOIN subjects s ON s.id = cs.subject_id
         WHERE cs.id = ?1",
        params![class_subject_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("That subject"))
}

/// A Subject Teacher writes only their own subject; a class's own teachers
/// and admins write any subject in it — the same rule as marks (FR-C1).
fn require_subject_writer(conn: &Connection, session: &Session, class_id: &str, class_subject_id: &str) -> AppResult<()> {
    session.require_view_class(class_id)?;
    if session.is_admin() || session.may_manage_class(class_id) {
        return Ok(());
    }
    let assigned: i64 = conn.query_row(
        "SELECT COUNT(*) FROM teacher_assignments ta
         JOIN class_subjects cs ON cs.subject_id = ta.subject_id AND cs.class_id = ta.class_id
         WHERE ta.user_id = ?1 AND cs.id = ?2 AND ta.status = 'active'",
        params![session.user_id, class_subject_id],
        |row| row.get(0),
    )?;
    if assigned == 0 {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

#[tauri::command]
pub fn load_weekly_sheet(
    state: State<'_, AppState>,
    class_subject_id: String,
    term_id: String,
    week: i64,
) -> AppResult<WeeklySheet> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    require_feature(&conn, "weekly_assignments", "Weekly assignments")?;
    let (class_id, class_name, subject_name) = class_of_subject(&conn, &class_subject_id)?;
    session.require_view_class(&class_id)?;
    if !(1..=MAX_WEEKS).contains(&week) {
        return Err(AppError::validation("Pick a week from 1 to 20."));
    }

    let year_id = repo::current_academic_year_id(&conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;
    let term_name: String = conn
        .query_row("SELECT name FROM terms WHERE id = ?1", params![term_id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That term"))?;

    let weeks_recorded: Vec<i64> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT week FROM weekly_scores
             WHERE class_subject_id = ?1 AND term_id = ?2 AND score IS NOT NULL
             ORDER BY week ASC",
        )?;
        let collected = stmt
            .query_map(params![class_subject_id, term_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        collected
    };

    let out_of: f64 = conn
        .query_row(
            "SELECT out_of FROM weekly_scores
             WHERE class_subject_id = ?1 AND term_id = ?2 AND week = ?3 LIMIT 1",
            params![class_subject_id, term_id, week],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(10.0);

    let mut stmt = conn.prepare(
        "SELECT s.id, s.reg_number, s.full_name, st.name, w.score, w.remark
         FROM enrollments e
         JOIN students s ON s.id = e.student_id
         LEFT JOIN streams st ON st.id = e.stream_id
         LEFT JOIN weekly_scores w
                ON w.student_id = s.id AND w.class_subject_id = ?2 AND w.term_id = ?3 AND w.week = ?4
         WHERE e.class_id = ?1 AND e.academic_year_id = ?5 AND e.status = 'active'
         ORDER BY s.full_name COLLATE NOCASE ASC",
    )?;
    let rows = stmt
        .query_map(params![class_id, class_subject_id, term_id, week, year_id], |row| {
            Ok(WeeklyRow {
                student_id: row.get(0)?,
                reg_number: row.get(1)?,
                full_name: row.get(2)?,
                stream_name: row.get(3)?,
                score: row.get(4)?,
                remark: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(WeeklySheet {
        class_subject_id,
        class_name,
        subject_name,
        term_name,
        week,
        out_of,
        weeks_recorded,
        rows,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyEntry {
    pub student_id: String,
    pub score: Option<f64>,
    #[serde(default)]
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWeeklyRequest {
    pub class_subject_id: String,
    pub term_id: String,
    pub week: i64,
    pub out_of: f64,
    pub entries: Vec<WeeklyEntry>,
}

#[tauri::command]
pub fn save_weekly_scores(state: State<'_, AppState>, request: SaveWeeklyRequest) -> AppResult<usize> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    require_feature(&conn, "weekly_assignments", "Weekly assignments")?;
    let (class_id, class_name, subject_name) = class_of_subject(&conn, &request.class_subject_id)?;
    require_subject_writer(&conn, &session, &class_id, &request.class_subject_id)?;

    if !(1..=MAX_WEEKS).contains(&request.week) {
        return Err(AppError::validation("Pick a week from 1 to 20."));
    }
    if !(1.0..=100.0).contains(&request.out_of) {
        return Err(AppError::validation("\"Out of\" must be between 1 and 100."));
    }
    for entry in &request.entries {
        if let Some(score) = entry.score {
            if !(0.0..=request.out_of).contains(&score) {
                return Err(AppError::validation(format!(
                    "A score of {score} is outside 0 to {}.",
                    request.out_of
                )));
            }
        }
    }

    let year_id = repo::current_academic_year_id(&conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;
    let roster: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare(
            "SELECT student_id FROM enrollments
             WHERE class_id = ?1 AND academic_year_id = ?2 AND status = 'active'",
        )?;
        let collected = stmt
            .query_map(params![class_id, year_id], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        collected
    };

    let now = Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    let mut saved = 0usize;
    for entry in &request.entries {
        if !roster.contains(&entry.student_id) {
            continue;
        }
        let remark = entry.remark.as_deref().map(str::trim).filter(|r| !r.is_empty());
        if entry.score.is_none() && remark.is_none() {
            // Cleared: a blank is "not done", which is the absence of a row.
            tx.execute(
                "DELETE FROM weekly_scores
                 WHERE student_id = ?1 AND class_subject_id = ?2 AND term_id = ?3 AND week = ?4",
                params![entry.student_id, request.class_subject_id, request.term_id, request.week],
            )?;
            continue;
        }
        tx.execute(
            "INSERT INTO weekly_scores
                (id, student_id, class_subject_id, term_id, week, score, out_of, remark, entered_by, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT (student_id, class_subject_id, term_id, week) DO UPDATE SET
                score = excluded.score, out_of = excluded.out_of, remark = excluded.remark,
                entered_by = excluded.entered_by, updated_at = excluded.updated_at",
            params![
                new_id("wks"),
                entry.student_id,
                request.class_subject_id,
                request.term_id,
                request.week,
                entry.score,
                request.out_of,
                remark,
                session.user_id,
                now,
            ],
        )?;
        saved += 1;
    }
    // The whole week shares one "out of".
    tx.execute(
        "UPDATE weekly_scores SET out_of = ?1
         WHERE class_subject_id = ?2 AND term_id = ?3 AND week = ?4",
        params![request.out_of, request.class_subject_id, request.term_id, request.week],
    )?;
    audit::record(
        &tx,
        Some(&session),
        "weekly.save",
        "class_subject",
        &request.class_subject_id,
        format!(
            "Saved week {} assignment scores for {class_name} {subject_name} ({saved} learners)",
            request.week
        ),
    )?;
    tx.commit()?;
    Ok(saved)
}

// ---------------------------------------------------------------------------
// The end-of-term summary, for the back of the report card
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklySummary {
    pub subject: String,
    /// Percentage for each week 1..=weeks_set; None where not done.
    pub scores: Vec<Option<f64>>,
    pub weeks_done: i64,
    pub weeks_set: i64,
    pub mean_percentage: Option<f64>,
}

/// Every learner's weekly summary for a class and term, keyed by learner.
/// `subjects` is (class_subject_id, name) in report-card order.
pub fn weekly_summaries(
    conn: &Connection,
    term_id: &str,
    subjects: &[(String, String)],
) -> AppResult<(i64, HashMap<String, Vec<WeeklySummary>>)> {
    // (student, class_subject) -> week -> percentage
    let mut scores: HashMap<(String, String), HashMap<i64, f64>> = HashMap::new();
    // class_subject -> weeks that were set (anyone has a score)
    let mut set_weeks: HashMap<String, i64> = HashMap::new();
    let mut students: std::collections::BTreeSet<String> = Default::default();
    {
        let mut stmt = conn.prepare(
            "SELECT student_id, class_subject_id, week, score, out_of FROM weekly_scores
             WHERE term_id = ?1 AND score IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![term_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
            ))
        })?;
        for row in rows {
            let (student, subject, week, score, out_of) = row?;
            let highest = set_weeks.entry(subject.clone()).or_insert(0);
            *highest = (*highest).max(week);
            students.insert(student.clone());
            scores
                .entry((student, subject))
                .or_default()
                .insert(week, (score / out_of * 1000.0).round() / 10.0);
        }
    }

    let max_weeks = subjects
        .iter()
        .filter_map(|(id, _)| set_weeks.get(id))
        .copied()
        .max()
        .unwrap_or(0);

    let mut by_student: HashMap<String, Vec<WeeklySummary>> = HashMap::new();
    for student in students {
        let mut summaries = Vec::new();
        for (subject_id, name) in subjects {
            let Some(&weeks_set) = set_weeks.get(subject_id) else {
                continue;
            };
            let mine = scores.get(&(student.clone(), subject_id.clone()));
            let per_week: Vec<Option<f64>> = (1..=max_weeks)
                .map(|week| mine.and_then(|m| m.get(&week).copied()))
                .collect();
            let done: Vec<f64> = per_week.iter().flatten().copied().collect();
            summaries.push(WeeklySummary {
                subject: name.clone(),
                weeks_done: done.len() as i64,
                weeks_set,
                mean_percentage: (!done.is_empty())
                    .then(|| (done.iter().sum::<f64>() / done.len() as f64 * 10.0).round() / 10.0),
                scores: per_week,
            });
        }
        by_student.insert(student, summaries);
    }
    Ok((max_weeks, by_student))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn weekly_summary_averages_percentages_and_counts_weeks_done() {
        let db = Database::open_in_memory().expect("open");
        let conn = db.lock();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").expect("fk off");
        let insert = |student: &str, week: i64, score: f64, out_of: f64| {
            conn.execute(
                "INSERT INTO weekly_scores (id, student_id, class_subject_id, term_id, week, score, out_of, updated_at)
                 VALUES (?1, ?2, 'cs_eng', 'trm_1', ?3, ?4, ?5, 'now')",
                params![format!("{student}-{week}"), student, week, score, out_of],
            )
            .expect("insert");
        };
        // Week 1 out of 10, week 2 out of 20: the percentages must stay right.
        insert("stu_a", 1, 8.0, 10.0);
        insert("stu_a", 2, 10.0, 20.0);
        // stu_b missed week 1.
        insert("stu_b", 2, 20.0, 20.0);

        let subjects = vec![("cs_eng".to_string(), "English".to_string())];
        let (weeks, by_student) = weekly_summaries(&conn, "trm_1", &subjects).expect("summary");
        assert_eq!(weeks, 2);

        let a = &by_student["stu_a"][0];
        assert_eq!(a.scores, vec![Some(80.0), Some(50.0)]);
        assert_eq!(a.weeks_done, 2);
        assert_eq!(a.weeks_set, 2);
        assert_eq!(a.mean_percentage, Some(65.0));

        let b = &by_student["stu_b"][0];
        assert_eq!(b.scores, vec![None, Some(100.0)]);
        assert_eq!(b.weeks_done, 1);
        assert_eq!(b.mean_percentage, Some(100.0));
    }
}
