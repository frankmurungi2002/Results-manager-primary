//! Marks entry (FR-C3) and subject analytics (FR-C5).
//!
//! SRS 14.7 insists all three entry methods be equally reliable, so they all
//! converge on one function — `save_marks` — and share one validator. The grid,
//! the guided form and the Excel upload differ only in how the rows are
//! gathered, never in how they are checked or stored.

use std::collections::HashMap;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::domain::grading::GradedMark;
use crate::domain::ids::{new_id, prefix};
use crate::domain::models::MarkEntry;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// The marks grid
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarksSheet {
    pub class_id: String,
    pub class_name: String,
    pub class_subject_id: String,
    pub subject_name: String,
    pub exam_id: String,
    pub exam_name: String,
    pub term_name: String,
    pub max_score: f64,
    pub pass_mark: f64,
    pub grading_system_id: String,
    pub grading_system_name: String,
    pub is_open: bool,
    pub rows: Vec<MarksRow>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarksRow {
    pub student_id: String,
    pub reg_number: String,
    pub full_name: String,
    pub score: Option<f64>,
    pub is_absent: bool,
    pub grade_label: Option<String>,
    pub entered_by: Option<String>,
    pub updated_at: Option<String>,
}

/// Everything the marks screen needs for one (class, subject, exam), in one
/// round trip — an 800-learner school cannot afford a query per row (SRS 14.6).
#[tauri::command]
pub fn load_marks_sheet(
    state: State<'_, AppState>,
    class_subject_id: String,
    exam_id: String,
) -> AppResult<MarksSheet> {
    load_marks_sheet_inner(state.inner(), &class_subject_id, &exam_id)
}

fn load_marks_sheet_inner(
    state: &AppState,
    class_subject_id: &str,
    exam_id: &str,
) -> AppResult<MarksSheet> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();

    type Header = (String, String, String, f64, f64, String, String, String, String);
    let header: Header = conn
        .query_row(
            "SELECT cs.class_id,
                    c.name  AS class_name,
                    COALESCE(cs.display_name, s.name) AS subject_name,
                    COALESCE(cs.max_score, s.max_score) AS max_score,
                    COALESCE(cs.pass_mark, s.pass_mark) AS pass_mark,
                    COALESCE(cs.grading_system_id, c.default_grading_system_id, 'gs_percentage') AS gs_id,
                    e.name  AS exam_name,
                    e.status AS exam_status,
                    t.name  AS term_name
             FROM class_subjects cs
             JOIN classes c ON c.id = cs.class_id
             JOIN subjects s ON s.id = cs.subject_id
             JOIN exams e ON e.id = ?2
             JOIN terms t ON t.id = e.term_id
             WHERE cs.id = ?1",
            params![class_subject_id, exam_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That subject or examination"))?;

    let (class_id, class_name, subject_name, max_score, pass_mark, gs_id, exam_name, exam_status, term_name) =
        header;

    session.require_view_class(&class_id)?;

    let system = repo::load_grading_system(&conn, &gs_id)?;
    let year_id = repo::current_academic_year_id(&conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;

    let mut stmt = conn.prepare(
        "SELECT s.id, s.reg_number, s.full_name,
                m.score, m.is_absent, u.full_name AS entered_by, m.updated_at
         FROM enrollments e
         JOIN students s ON s.id = e.student_id
         LEFT JOIN marks m
                ON m.student_id = s.id AND m.class_subject_id = ?1 AND m.exam_id = ?2
         LEFT JOIN users u ON u.id = COALESCE(m.updated_by, m.entered_by)
         WHERE e.class_id = ?3 AND e.academic_year_id = ?4 AND e.status = 'active'
         ORDER BY s.full_name COLLATE NOCASE ASC",
    )?;

    let rows = stmt.query_map(
        params![class_subject_id, exam_id, class_id, year_id],
        |row| {
            let score: Option<f64> = row.get(3)?;
            let is_absent: bool = row.get::<_, Option<i64>>(4)?.unwrap_or(0) != 0;
            Ok(MarksRow {
                student_id: row.get(0)?,
                reg_number: row.get(1)?,
                full_name: row.get(2)?,
                score,
                is_absent,
                grade_label: None,
                entered_by: row.get(5)?,
                updated_at: row.get(6)?,
            })
        },
    )?;

    let mut out = Vec::new();
    for row in rows {
        let mut row = row?;
        // Live grade preview, computed on the same engine the report card uses
        // so what a teacher sees at entry is what gets printed (FR-C3).
        if row.score.is_some() || row.is_absent {
            let graded = system.grade(row.score, row.is_absent, max_score, Some(pass_mark));
            row.grade_label = Some(graded.grade_label);
        }
        out.push(row);
    }

    Ok(MarksSheet {
        class_id,
        class_name,
        class_subject_id: class_subject_id.to_string(),
        subject_name,
        exam_id: exam_id.to_string(),
        exam_name,
        term_name,
        max_score,
        pass_mark,
        grading_system_id: system.id.clone(),
        grading_system_name: system.name.clone(),
        is_open: exam_status == "open",
        rows: out,
    })
}

// ---------------------------------------------------------------------------
// Saving
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMarksResult {
    pub saved: usize,
    pub skipped: Vec<RejectedMark>,
    /// Marks that saved but look wrong. See `detect_anomalies`.
    pub warnings: Vec<MarkWarning>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedMark {
    pub student_id: String,
    pub student_name: String,
    pub reason: String,
}

/// A mark that is legal but does not look like this learner.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkWarning {
    pub student_id: String,
    pub student_name: String,
    pub score: f64,
    /// This learner's average across their other subjects in the same
    /// examination, as a percentage.
    pub usual_percentage: f64,
    /// The number of other subjects that average is drawn from.
    pub compared_against: usize,
    /// A likelier value, where one is obvious — a dropped digit, usually.
    pub suggested: Option<f64>,
    pub reason: String,
}

/// How far a mark must sit from a learner's own average before RM says
/// anything. Deliberately wide: a warning system that cries wolf is a warning
/// system teachers learn to click past, and then it protects nobody.
const ANOMALY_THRESHOLD_PCT: f64 = 35.0;

/// The smallest number of other subjects that makes an average worth comparing
/// against. Two marks is not a pattern.
const MIN_COMPARISON_SUBJECTS: usize = 3;

/// Flags marks that do not look like the learner who supposedly earned them.
///
/// This exists for one specific accident: a teacher types `8` instead of `80`.
/// It is legal, it validates, it saves, and it prints — and the first anyone
/// hears of it is a parent at the gate holding a report card that says their
/// child failed Mathematics. RM has every other mark that learner scored this
/// examination, so it can simply notice.
///
/// Nothing is blocked. The marks are already saved; this only tells the teacher
/// where to look.
fn detect_anomalies(
    conn: &rusqlite::Connection,
    class_id: &str,
    class_subject_id: &str,
    exam_id: &str,
    max_score: f64,
    entries: &[MarkEntry],
    roster: &HashMap<String, String>,
) -> AppResult<Vec<MarkWarning>> {
    if max_score <= 0.0 {
        return Ok(Vec::new());
    }

    // Each learner's average across their *other* subjects in this same
    // examination, normalised to a percentage so subjects marked out of 40 and
    // out of 100 compare honestly.
    let mut baseline: HashMap<String, (f64, usize)> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT m.student_id,
                    AVG(m.score / COALESCE(cs.max_score, s.max_score) * 100.0) AS mean_pct,
                    COUNT(*) AS n
             FROM marks m
             JOIN class_subjects cs ON cs.id = m.class_subject_id
             JOIN subjects s ON s.id = cs.subject_id
             WHERE cs.class_id = ?1
               AND m.exam_id = ?2
               AND m.class_subject_id <> ?3
               AND m.is_absent = 0
               AND m.score IS NOT NULL
               AND COALESCE(cs.max_score, s.max_score) > 0
             GROUP BY m.student_id",
        )?;
        let rows = stmt.query_map(params![class_id, exam_id, class_subject_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, i64>(2)? as usize,
            ))
        })?;
        for row in rows {
            let (student_id, mean, count) = row?;
            baseline.insert(student_id, (mean, count));
        }
    }

    let mut warnings = Vec::new();

    for entry in entries {
        if entry.is_absent {
            continue;
        }
        let Some(score) = entry.score else { continue };
        let Some((usual, count)) = baseline.get(&entry.student_id).copied() else {
            continue;
        };
        if count < MIN_COMPARISON_SUBJECTS {
            continue;
        }

        let percentage = score / max_score * 100.0;
        let distance = (percentage - usual).abs();
        if distance <= ANOMALY_THRESHOLD_PCT {
            continue;
        }

        let name = roster
            .get(&entry.student_id)
            .cloned()
            .unwrap_or_else(|| "This learner".to_string());

        // A dropped trailing zero is far and away the most common way this
        // happens, so check for it explicitly and offer the fix.
        let shifted = score * 10.0;
        let suggested = (shifted <= max_score
            && (shifted / max_score * 100.0 - usual).abs() < ANOMALY_THRESHOLD_PCT / 2.0)
            .then_some(shifted);

        let reason = match suggested {
            Some(value) => format!(
                "{name} averages {usual:.0}% in their other subjects this examination. \
                 Did you mean {value:.0}?"
            ),
            None if percentage < usual => format!(
                "Well below {name}'s usual {usual:.0}% across their other {count} subjects."
            ),
            None => format!(
                "Well above {name}'s usual {usual:.0}% across their other {count} subjects."
            ),
        };

        warnings.push(MarkWarning {
            student_id: entry.student_id.clone(),
            student_name: name,
            score,
            usual_percentage: (usual * 10.0).round() / 10.0,
            compared_against: count,
            suggested,
            reason,
        });
    }

    Ok(warnings)
}

/// The single write path shared by all three entry methods (SRS 14.7).
///
/// Rows that fail validation are reported back rather than silently dropped or
/// allowed to abort the whole save — a teacher who typed one mark out of range
/// should not lose the other thirty-nine.
#[tauri::command]
pub fn save_marks(
    state: State<'_, AppState>,
    class_subject_id: String,
    exam_id: String,
    entries: Vec<MarkEntry>,
) -> AppResult<SaveMarksResult> {
    let session = state.sessions.require()?;

    let mut conn = state.db.lock();

    type Context = (String, f64, String, String, String);
    let (class_id, max_score, exam_status, subject_name, exam_name): Context = conn
        .query_row(
            "SELECT cs.class_id,
                    COALESCE(cs.max_score, s.max_score),
                    e.status,
                    COALESCE(cs.display_name, s.name),
                    e.name
             FROM class_subjects cs
             JOIN subjects s ON s.id = cs.subject_id
             JOIN exams e ON e.id = ?2
             WHERE cs.id = ?1",
            params![class_subject_id, exam_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That subject or examination"))?;

    session.require_view_class(&class_id)?;

    // FR-C1 again, one level finer: a Subject Teacher may only write marks for
    // the subject they were assigned. Being in the class is not enough.
    if !session.is_admin() && !session.may_manage_class(&class_id) {
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
    }

    // A closed term is closed. Reopening one cell is a separate, approved
    // workflow (FR-C14) rather than something this path quietly allows.
    if exam_status != "open" {
        return Err(AppError::conflict(
            "This examination is closed. A School Admin must reopen it to change marks.",
        ));
    }

    let year_id = repo::current_academic_year_id(&conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;

    // Who is actually on this roster, so a mark cannot be filed against a
    // learner who left or was never in this class.
    let roster: HashMap<String, String> = {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.full_name FROM enrollments e
             JOIN students s ON s.id = e.student_id
             WHERE e.class_id = ?1 AND e.academic_year_id = ?2 AND e.status = 'active'",
        )?;
        let rows = stmt.query_map(params![class_id, year_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let collected = rows.collect::<rusqlite::Result<_>>()?;
        collected
    };

    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction()?;

    let mut saved = 0usize;
    let mut skipped = Vec::new();

    for entry in &entries {
        let Some(student_name) = roster.get(&entry.student_id) else {
            skipped.push(RejectedMark {
                student_id: entry.student_id.clone(),
                student_name: "Unknown learner".into(),
                reason: "Not on this class's roster.".into(),
            });
            continue;
        };

        if let Some(score) = entry.score {
            if !score.is_finite() {
                skipped.push(RejectedMark {
                    student_id: entry.student_id.clone(),
                    student_name: student_name.clone(),
                    reason: "That is not a number.".into(),
                });
                continue;
            }
            if score < 0.0 {
                skipped.push(RejectedMark {
                    student_id: entry.student_id.clone(),
                    student_name: student_name.clone(),
                    reason: "A mark cannot be negative.".into(),
                });
                continue;
            }
            if score > max_score {
                skipped.push(RejectedMark {
                    student_id: entry.student_id.clone(),
                    student_name: student_name.clone(),
                    reason: format!("{score} is above the maximum of {max_score}."),
                });
                continue;
            }
        }

        // A blank cell clears the mark rather than storing a zero — the
        // difference between "not marked yet" and "scored nothing" has to
        // survive to the report card.
        if entry.score.is_none() && !entry.is_absent {
            tx.execute(
                "DELETE FROM marks WHERE student_id = ?1 AND class_subject_id = ?2 AND exam_id = ?3",
                params![entry.student_id, class_subject_id, exam_id],
            )?;
            continue;
        }

        tx.execute(
            "INSERT INTO marks
                (id, student_id, class_subject_id, exam_id, score, is_absent,
                 entered_by, entered_at, updated_by, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?7, ?8)
             ON CONFLICT (student_id, class_subject_id, exam_id) DO UPDATE SET
                score      = excluded.score,
                is_absent  = excluded.is_absent,
                updated_by = excluded.updated_by,
                updated_at = excluded.updated_at",
            params![
                new_id(prefix::MARK),
                entry.student_id,
                class_subject_id,
                exam_id,
                if entry.is_absent { None } else { entry.score },
                if entry.is_absent { 1 } else { 0 },
                session.user_id,
                now,
            ],
        )?;

        saved += 1;
    }

    // Run inside the transaction, so the baseline includes what was just
    // written and a whole sheet saved at once is checked against itself.
    let warnings = detect_anomalies(
        &tx,
        &class_id,
        &class_subject_id,
        &exam_id,
        max_score,
        &entries,
        &roster,
    )?;

    audit::record(
        &tx,
        Some(&session),
        "marks.save",
        "class_subject",
        &class_subject_id,
        format!(
            "Saved {saved} marks for {subject_name}, {exam_name}{}",
            if warnings.is_empty() {
                String::new()
            } else {
                format!(" ({} flagged for checking)", warnings.len())
            }
        ),
    )?;

    tx.commit()?;

    Ok(SaveMarksResult {
        saved,
        skipped,
        warnings,
    })
}

// ---------------------------------------------------------------------------
// Progress (FR-C6)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectProgress {
    pub class_subject_id: String,
    pub subject_name: String,
    pub teacher_name: Option<String>,
    pub expected: i64,
    pub entered: i64,
    pub percent: f64,
}

/// FR-C6: what fraction of each Subject Teacher's marks are in, live.
#[tauri::command]
pub fn marks_progress(
    state: State<'_, AppState>,
    class_id: String,
    exam_id: String,
) -> AppResult<Vec<SubjectProgress>> {
    let session = state.sessions.require()?;
    session.require_view_class(&class_id)?;

    let conn = state.db.lock();
    let year_id = repo::current_academic_year_id(&conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;

    let expected: i64 = conn.query_row(
        "SELECT COUNT(*) FROM enrollments
         WHERE class_id = ?1 AND academic_year_id = ?2 AND status = 'active'",
        params![class_id, year_id],
        |row| row.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT cs.id,
                COALESCE(cs.display_name, s.name) AS subject_name,
                (SELECT u.full_name FROM teacher_assignments ta
                  JOIN users u ON u.id = ta.user_id
                  WHERE ta.class_id = cs.class_id AND ta.subject_id = cs.subject_id
                    AND ta.role = 'subject_teacher' AND ta.status = 'active'
                  LIMIT 1) AS teacher_name,
                (SELECT COUNT(*) FROM marks m
                  JOIN enrollments e2 ON e2.student_id = m.student_id
                  WHERE m.class_subject_id = cs.id AND m.exam_id = ?2
                    AND e2.class_id = cs.class_id AND e2.academic_year_id = ?3
                    AND e2.status = 'active') AS entered
         FROM class_subjects cs
         JOIN subjects s ON s.id = cs.subject_id
         WHERE cs.class_id = ?1 AND cs.status = 'active'
         ORDER BY cs.is_core DESC, cs.position ASC",
    )?;

    let rows = stmt.query_map(params![class_id, exam_id, year_id], |row| {
        let entered: i64 = row.get(3)?;
        Ok(SubjectProgress {
            class_subject_id: row.get(0)?,
            subject_name: row.get(1)?,
            teacher_name: row.get(2)?,
            expected,
            entered,
            percent: if expected > 0 {
                (entered as f64 / expected as f64 * 100.0).round()
            } else {
                0.0
            },
        })
    })?;

    let collected = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(collected)
}

// ---------------------------------------------------------------------------
// Subject analytics (FR-C5)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectAnalytics {
    pub subject_name: String,
    pub exam_name: String,
    pub entered: usize,
    pub absent: usize,
    pub mean_percentage: Option<f64>,
    pub highest: Option<f64>,
    pub lowest: Option<f64>,
    pub pass_rate: Option<f64>,
    pub distribution: Vec<GradeCount>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GradeCount {
    pub label: String,
    pub count: usize,
}

#[tauri::command]
pub fn subject_analytics(
    state: State<'_, AppState>,
    class_subject_id: String,
    exam_id: String,
) -> AppResult<SubjectAnalytics> {
    let sheet = load_marks_sheet_inner(state.inner(), &class_subject_id, &exam_id)?;

    let system = {
        let conn = state.db.lock();
        repo::load_grading_system(&conn, &sheet.grading_system_id)?
    };

    let graded: Vec<GradedMark> = sheet
        .rows
        .iter()
        .filter(|row| row.score.is_some() || row.is_absent)
        .map(|row| {
            system.grade(
                row.score,
                row.is_absent,
                sheet.max_score,
                Some(sheet.pass_mark),
            )
        })
        .collect();

    let scored: Vec<&GradedMark> = graded.iter().filter(|g| !g.is_absent).collect();
    let absent = graded.len() - scored.len();

    let percentages: Vec<f64> = scored.iter().filter_map(|g| g.percentage).collect();
    let mean = (!percentages.is_empty())
        .then(|| (percentages.iter().sum::<f64>() / percentages.len() as f64 * 100.0).round() / 100.0);

    let passes = scored.iter().filter(|g| g.passed == Some(true)).count();
    let pass_rate = (!scored.is_empty())
        .then(|| (passes as f64 / scored.len() as f64 * 100.0).round());

    // Distribution follows the grading system's own band order, so an empty
    // band still shows as a zero rather than vanishing from the chart.
    let distribution = system
        .bands
        .iter()
        .map(|band| GradeCount {
            label: band.label.clone(),
            count: scored
                .iter()
                .filter(|g| g.grade_label == band.label)
                .count(),
        })
        .collect();

    Ok(SubjectAnalytics {
        subject_name: sheet.subject_name,
        exam_name: sheet.exam_name,
        entered: graded.len(),
        absent,
        mean_percentage: mean,
        highest: scored
            .iter()
            .filter_map(|g| g.score)
            .fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a| a.max(v)))),
        lowest: scored
            .iter()
            .filter_map(|g| g.score)
            .fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a| a.min(v)))),
        pass_rate,
        distribution,
    })
}

// ---------------------------------------------------------------------------
// Deadlines (FR-C6)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDeadlineRequest {
    pub class_id: String,
    pub exam_id: String,
    /// RFC3339. `None` clears the deadline.
    pub deadline: Option<String>,
}

#[tauri::command]
pub fn set_marks_deadline(
    state: State<'_, AppState>,
    request: SetDeadlineRequest,
) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_manage_class(&request.class_id)?;

    let conn = state.db.lock();
    let key = format!("deadline.{}.{}", request.class_id, request.exam_id);

    match &request.deadline {
        Some(value) => repo::set_setting(&conn, &key, value)?,
        None => {
            conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        }
    }

    audit::record(
        &conn,
        Some(&session),
        "marks.deadline",
        "exam",
        &request.exam_id,
        match &request.deadline {
            Some(value) => format!("Set the marks-entry deadline to {value}"),
            None => "Cleared the marks-entry deadline".to_string(),
        },
    )?;

    Ok(())
}

#[tauri::command]
pub fn get_marks_deadline(
    state: State<'_, AppState>,
    class_id: String,
    exam_id: String,
) -> AppResult<Option<String>> {
    state.sessions.require()?;
    let conn = state.db.lock();
    repo::get_setting(&conn, &format!("deadline.{class_id}.{exam_id}"))
}
