//! Questions a headteacher has never been able to ask.
//!
//! Nothing here is new data. Every figure comes from marks already entered —
//! but no Ugandan primary headteacher can currently answer "which of my P7s is
//! two marks away from Division 1", because nobody has ever been able to hold
//! the whole class's marks in one place and do the arithmetic.
//!
//! Two views:
//!
//! * **PLE projection** — where this class would land if PLE were sat today,
//!   and the cheapest route up for each learner near a boundary.
//! * **Subject heat map** — which class, in which subject, is dragging.

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::domain::assessment::{self, ExamSlot};
use crate::domain::grading::{self, GradingSystem};
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// PLE projection
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PleProjection {
    pub class_name: String,
    pub term_name: String,
    pub academic_year: String,
    /// Everyone on the roster.
    pub total_learners: usize,
    /// Learners with a mark in all four core subjects — the only ones a
    /// division can honestly be computed for.
    pub projected_learners: usize,
    pub core_subjects: Vec<String>,
    pub divisions: Vec<DivisionCount>,
    /// Learners who could reach the next division up, cheapest first.
    pub near_misses: Vec<NearMiss>,
    /// Learners left out of the projection, and what is missing.
    pub incomplete: Vec<IncompleteLearner>,
    /// True when the class has exactly four core subjects. When it does not,
    /// no division is computed at all and the caller must say so.
    pub divisions_computable: bool,
    pub best_aggregate: Option<f64>,
    pub mean_aggregate: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DivisionCount {
    pub label: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NearMiss {
    pub student_id: String,
    pub full_name: String,
    pub reg_number: String,
    pub aggregate: f64,
    pub current_division: String,
    pub target_division: String,
    /// Aggregate points that must be shed to cross the boundary.
    pub points_needed: f64,
    /// Total extra raw marks across the subjects below.
    pub total_marks_needed: f64,
    /// The cheapest route, subject by subject.
    pub steps: Vec<ImprovementStep>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImprovementStep {
    pub subject_name: String,
    pub current_score: f64,
    pub max_score: f64,
    pub current_grade: String,
    pub next_grade: String,
    pub marks_needed: f64,
    pub points_gained: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncompleteLearner {
    pub student_id: String,
    pub full_name: String,
    pub missing: Vec<String>,
}

/// Beyond this, "improvement" stops being advice and starts being fantasy.
/// A learner needing forty extra marks across four subjects does not belong on
/// a list a headteacher is meant to act on this term.
const MAX_PLAUSIBLE_MARKS: f64 = 20.0;

#[tauri::command]
pub fn ple_projection(
    state: State<'_, AppState>,
    class_id: String,
    term_id: String,
) -> AppResult<PleProjection> {
    let session = state.sessions.require()?;
    session.require_view_class(&class_id)?;

    let conn = state.db.lock();
    let year_id = repo::current_academic_year_id(&conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;

    let (class_name, term_name, academic_year): (String, String, String) = conn
        .query_row(
            "SELECT c.name, t.name, y.label
             FROM classes c
             CROSS JOIN terms t
             JOIN academic_years y ON y.id = t.academic_year_id
             WHERE c.id = ?1 AND t.id = ?2",
            params![class_id, term_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That class or term"))?;

    // --- Examination slots for the term -------------------------------------
    let exams: Vec<(String, f64, bool)> = {
        let mut stmt = conn.prepare(
            "SELECT id, weight, is_final FROM exams
             WHERE term_id = ?1 ORDER BY seq ASC",
        )?;
        let collected = stmt.query_map(params![term_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)? != 0))
        })?
        .collect::<rusqlite::Result<_>>()?;
        collected
    };

    if exams.is_empty() {
        return Err(AppError::validation(
            "That term has no examinations to project from.",
        ));
    }

    let slots: Vec<ExamSlot> = exams
        .iter()
        .map(|(_, weight, is_final)| ExamSlot {
            weight: *weight,
            is_final: *is_final,
        })
        .collect();

    // --- Core subjects ------------------------------------------------------
    struct Core {
        id: String,
        name: String,
        max_score: f64,
        grading_system_id: String,
    }

    let core: Vec<Core> = {
        let mut stmt = conn.prepare(
            "SELECT cs.id,
                    COALESCE(cs.display_name, s.name),
                    COALESCE(cs.max_score, s.max_score),
                    COALESCE(cs.grading_system_id, c.default_grading_system_id, 'gs_percentage')
             FROM class_subjects cs
             JOIN subjects s ON s.id = cs.subject_id
             JOIN classes c ON c.id = cs.class_id
             WHERE cs.class_id = ?1 AND cs.status = 'active' AND cs.is_core = 1
             ORDER BY cs.position ASC",
        )?;
        let collected = stmt.query_map(params![class_id], |row| {
            Ok(Core {
                id: row.get(0)?,
                name: row.get(1)?,
                max_score: row.get(2)?,
                grading_system_id: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
        collected
    };

    let divisions_computable = core.len() == grading::PLE_CORE_SUBJECTS;

    let systems: HashMap<String, GradingSystem> = repo::load_grading_systems_by_id(
        &conn,
        &core
            .iter()
            .map(|c| c.grading_system_id.clone())
            .collect::<Vec<_>>(),
    )?;

    // --- Learners and their marks -------------------------------------------
    let learners: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.full_name, s.reg_number
             FROM enrollments e JOIN students s ON s.id = e.student_id
             WHERE e.class_id = ?1 AND e.academic_year_id = ?2 AND e.status = 'active'
             ORDER BY s.full_name COLLATE NOCASE ASC",
        )?;
        let collected = stmt.query_map(params![class_id, year_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
        collected
    };

    let mut marks: HashMap<(String, String, String), f64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT m.student_id, m.class_subject_id, m.exam_id, m.score
             FROM marks m
             JOIN class_subjects cs ON cs.id = m.class_subject_id
             WHERE cs.class_id = ?1 AND m.is_absent = 0 AND m.score IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![class_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })?;
        for row in rows {
            let (student, subject, exam, score) = row?;
            marks.insert((student, subject, exam), score);
        }
    }

    // --- Project ------------------------------------------------------------
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    let mut near_misses = Vec::new();
    let mut incomplete = Vec::new();
    let mut aggregates: Vec<f64> = Vec::new();

    for (student_id, full_name, reg_number) in &learners {
        let mut aggregate = 0.0;
        let mut graded = Vec::new();
        let mut missing = Vec::new();

        for subject in &core {
            let scores: Vec<Option<f64>> = exams
                .iter()
                .map(|(exam_id, _, _)| {
                    marks
                        .get(&(student_id.clone(), subject.id.clone(), exam_id.clone()))
                        .copied()
                })
                .collect();

            let Some(score) = assessment::term_score(&slots, &scores) else {
                missing.push(subject.name.clone());
                continue;
            };

            let system = systems
                .get(&subject.grading_system_id)
                .ok_or_else(|| AppError::internal("grading system missing"))?;
            let mark = system.grade(Some(score), false, subject.max_score, None);

            match mark.points {
                Some(points) => {
                    aggregate += points;
                    graded.push((subject, system, score, mark.grade_label.clone()));
                }
                None => missing.push(subject.name.clone()),
            }
        }

        if !missing.is_empty() || graded.len() != core.len() {
            incomplete.push(IncompleteLearner {
                student_id: student_id.clone(),
                full_name: full_name.clone(),
                missing,
            });
            continue;
        }

        aggregates.push(aggregate);

        let Some(division) = grading::division_for(aggregate, graded.len()) else {
            continue;
        };
        let label: &'static str = grading::PLE_DIVISIONS
            .iter()
            .find(|(name, _, _)| *name == division)
            .map(|(name, _, _)| *name)
            .unwrap_or("Ungraded (U)");
        *counts.entry(label).or_insert(0) += 1;

        // --- The cheapest route to the next division up ---------------------
        let Some((target_label, target_aggregate)) = grading::next_division_target(aggregate)
        else {
            continue; // Already Division 1.
        };

        let points_needed = aggregate - target_aggregate as f64;
        if points_needed <= 0.0 {
            continue;
        }

        // Rank every available improvement by marks per aggregate point, then
        // take them in order until the boundary is crossed. Greedy is exactly
        // right here: each subject's gain is independent of the others.
        let mut options: Vec<ImprovementStep> = graded
            .iter()
            .filter_map(|(subject, system, score, grade)| {
                let gap = assessment::gap_to_next_band(system, *score, subject.max_score)?;
                Some(ImprovementStep {
                    subject_name: subject.name.clone(),
                    current_score: (score * 10.0).round() / 10.0,
                    max_score: subject.max_score,
                    current_grade: grade.clone(),
                    next_grade: gap.next_label,
                    marks_needed: gap.marks_needed,
                    points_gained: gap.points_gained,
                })
            })
            .collect();

        options.sort_by(|a, b| {
            let cost_a = a.marks_needed / a.points_gained.max(0.001);
            let cost_b = b.marks_needed / b.points_gained.max(0.001);
            cost_a
                .partial_cmp(&cost_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut gained = 0.0;
        let mut marks_needed = 0.0;
        let mut steps = Vec::new();
        for step in options {
            if gained >= points_needed {
                break;
            }
            gained += step.points_gained;
            marks_needed += step.marks_needed;
            steps.push(step);
        }

        if gained < points_needed || marks_needed > MAX_PLAUSIBLE_MARKS || steps.is_empty() {
            continue;
        }

        near_misses.push(NearMiss {
            student_id: student_id.clone(),
            full_name: full_name.clone(),
            reg_number: reg_number.clone(),
            aggregate,
            current_division: division,
            target_division: target_label.to_string(),
            points_needed,
            total_marks_needed: marks_needed,
            steps,
        });
    }

    near_misses.sort_by(|a, b| {
        a.total_marks_needed
            .partial_cmp(&b.total_marks_needed)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let divisions = grading::PLE_DIVISIONS
        .iter()
        .map(|(label, _, _)| DivisionCount {
            label: (*label).to_string(),
            count: counts.get(label).copied().unwrap_or(0),
        })
        .collect();

    let projected = aggregates.len();
    let best = aggregates
        .iter()
        .copied()
        .fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a| a.min(v))));
    let mean = (projected > 0)
        .then(|| (aggregates.iter().sum::<f64>() / projected as f64 * 10.0).round() / 10.0);

    Ok(PleProjection {
        class_name,
        term_name,
        academic_year,
        total_learners: learners.len(),
        projected_learners: projected,
        core_subjects: core.iter().map(|c| c.name.clone()).collect(),
        divisions,
        near_misses,
        incomplete,
        divisions_computable,
        best_aggregate: best,
        mean_aggregate: mean,
    })
}

// ---------------------------------------------------------------------------
// Subject heat map
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectHeatmap {
    pub term_name: String,
    pub classes: Vec<String>,
    pub subjects: Vec<String>,
    pub cells: Vec<HeatCell>,
    pub school_mean: Option<f64>,
    /// Ranked worst first — where a headteacher should look.
    pub weakest: Vec<HeatCell>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatCell {
    pub class_name: String,
    pub subject_name: String,
    pub mean_percentage: f64,
    /// Distance from the school-wide mean, in percentage points.
    pub delta: f64,
    pub learners: i64,
}

/// Every class against every subject, as a percentage of each subject's own
/// maximum so a subject marked out of 40 compares honestly with one out of 100.
///
/// This averages every mark recorded in the term rather than the weighted term
/// mark — it is a measure of the teaching, not of any one learner's report
/// card, and the caller labels it as such.
#[tauri::command]
pub fn subject_heatmap(
    state: State<'_, AppState>,
    term_id: String,
) -> AppResult<SubjectHeatmap> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let conn = state.db.lock();

    let term_name: String = conn
        .query_row(
            "SELECT name FROM terms WHERE id = ?1",
            params![term_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That term"))?;

    let mut stmt = conn.prepare(
        "SELECT c.name, c.ladder_position,
                COALESCE(cs.display_name, s.name) AS subject_name,
                AVG(m.score / COALESCE(cs.max_score, s.max_score) * 100.0) AS mean_pct,
                COUNT(DISTINCT m.student_id) AS learners
         FROM marks m
         JOIN class_subjects cs ON cs.id = m.class_subject_id
         JOIN subjects s ON s.id = cs.subject_id
         JOIN classes c ON c.id = cs.class_id
         JOIN exams e ON e.id = m.exam_id
         WHERE e.term_id = ?1
           AND m.is_absent = 0
           AND m.score IS NOT NULL
           AND COALESCE(cs.max_score, s.max_score) > 0
           AND c.status = 'active'
           AND cs.status = 'active'
         GROUP BY c.id, cs.id
         ORDER BY c.ladder_position ASC, subject_name ASC",
    )?;

    let raw: Vec<(String, i64, String, f64, i64)> = stmt
        .query_map(params![term_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    if raw.is_empty() {
        return Ok(SubjectHeatmap {
            term_name,
            classes: Vec::new(),
            subjects: Vec::new(),
            cells: Vec::new(),
            school_mean: None,
            weakest: Vec::new(),
        });
    }

    // Weight the school mean by learners rather than by cell, so a class of
    // four does not pull the whole school's average around.
    let total_learners: i64 = raw.iter().map(|(_, _, _, _, n)| n).sum();
    let school_mean = if total_learners > 0 {
        raw.iter()
            .map(|(_, _, _, mean, n)| mean * *n as f64)
            .sum::<f64>()
            / total_learners as f64
    } else {
        0.0
    };

    let mut classes: Vec<(i64, String)> = raw
        .iter()
        .map(|(name, ladder, _, _, _)| (*ladder, name.clone()))
        .collect();
    classes.sort();
    classes.dedup();

    let mut subjects: Vec<String> = raw.iter().map(|(_, _, s, _, _)| s.clone()).collect();
    subjects.sort();
    subjects.dedup();

    let cells: Vec<HeatCell> = raw
        .iter()
        .map(|(class_name, _, subject_name, mean, learners)| HeatCell {
            class_name: class_name.clone(),
            subject_name: subject_name.clone(),
            mean_percentage: (mean * 10.0).round() / 10.0,
            delta: ((mean - school_mean) * 10.0).round() / 10.0,
            learners: *learners,
        })
        .collect();

    let mut weakest: Vec<HeatCell> = cells
        .iter()
        .filter(|cell| cell.delta < 0.0)
        .cloned()
        .collect();
    weakest.sort_by(|a, b| {
        a.delta
            .partial_cmp(&b.delta)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    weakest.truncate(6);

    Ok(SubjectHeatmap {
        term_name,
        classes: classes.into_iter().map(|(_, name)| name).collect(),
        subjects,
        cells,
        school_mean: Some((school_mean * 10.0).round() / 10.0),
        weakest,
    })
}
