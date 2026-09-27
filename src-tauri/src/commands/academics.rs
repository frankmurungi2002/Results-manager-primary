//! Classes, streams, the subject catalogue, per-class subjects, teacher
//! assignment, and the academic calendar.
//!
//! FR-B3, FR-B5, FR-B6, FR-B7, FR-C13.

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::domain::grading::GradingSystem;
use crate::domain::ids::{new_id, prefix};
use crate::domain::models::{
    AcademicYearRow, ClassRow, ClassSubjectRow, ExamRow, SubjectRow, TermRow,
};
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Classes
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_classes(state: State<'_, AppState>) -> AppResult<Vec<ClassRow>> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let year_id = repo::current_academic_year_id(&conn)?;

    let mut stmt = conn.prepare(
        "SELECT c.id, c.code, c.name, c.level_kind, c.ladder_position,
                c.default_grading_system_id, gs.name AS grading_name, c.status,
                (SELECT COUNT(*) FROM enrollments e
                  WHERE e.class_id = c.id AND e.status = 'active'
                    AND (?1 IS NULL OR e.academic_year_id = ?1))       AS learner_count,
                (SELECT COUNT(*) FROM class_subjects cs
                  WHERE cs.class_id = c.id AND cs.status = 'active')   AS subject_count,
                (SELECT u.full_name FROM teacher_assignments ta
                  JOIN users u ON u.id = ta.user_id
                  WHERE ta.class_id = c.id AND ta.role = 'class_teacher'
                    AND ta.status = 'active' AND ta.stream_id IS NULL
                  LIMIT 1)                                             AS class_teacher_name
         FROM classes c
         LEFT JOIN grading_systems gs ON gs.id = c.default_grading_system_id
         WHERE c.status = 'active'
         ORDER BY c.ladder_position ASC",
    )?;

    let rows = stmt.query_map(params![year_id], |row| {
        Ok(ClassRow {
            id: row.get(0)?,
            code: row.get(1)?,
            name: row.get(2)?,
            level_kind: row.get(3)?,
            ladder_position: row.get(4)?,
            default_grading_system_id: row.get(5)?,
            default_grading_system_name: row.get(6)?,
            status: row.get(7)?,
            learner_count: row.get(8)?,
            subject_count: row.get(9)?,
            class_teacher_name: row.get(10)?,
        })
    })?;

    let mut classes = Vec::new();
    for row in rows {
        let class = row?;
        // FR-C1: a teacher only ever sees their own classes in any list.
        if session.may_view_class(&class.id) {
            classes.push(class);
        }
    }

    Ok(classes)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveClassRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub code: String,
    pub name: String,
    pub level_kind: String,
    pub ladder_position: i64,
    #[serde(default)]
    pub default_grading_system_id: Option<String>,
}

#[tauri::command]
pub fn save_class(state: State<'_, AppState>, request: SaveClassRequest) -> AppResult<String> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    if request.name.trim().is_empty() {
        return Err(AppError::validation("A class needs a name."));
    }
    if !matches!(request.level_kind.as_str(), "nursery" | "primary") {
        return Err(AppError::validation("A class must be nursery or primary."));
    }

    let conn = state.db.lock();
    let now = Utc::now().to_rfc3339();

    match request.id {
        Some(id) => {
            conn.execute(
                "UPDATE classes
                 SET code = ?1, name = ?2, level_kind = ?3, ladder_position = ?4,
                     default_grading_system_id = ?5, updated_at = ?6
                 WHERE id = ?7",
                params![
                    request.code.trim(),
                    request.name.trim(),
                    request.level_kind,
                    request.ladder_position,
                    request.default_grading_system_id,
                    now,
                    id,
                ],
            )?;
            audit::record(
                &conn,
                Some(&session),
                "class.update",
                "class",
                &id,
                format!("Updated class {}", request.name.trim()),
            )?;
            Ok(id)
        }
        None => {
            let id = new_id(prefix::CLASS);
            conn.execute(
                "INSERT INTO classes
                    (id, code, name, level_kind, ladder_position, default_grading_system_id,
                     status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?7)",
                params![
                    id,
                    request.code.trim(),
                    request.name.trim(),
                    request.level_kind,
                    request.ladder_position,
                    request.default_grading_system_id,
                    now,
                ],
            )?;
            audit::record(
                &conn,
                Some(&session),
                "class.create",
                "class",
                &id,
                format!("Created class {}", request.name.trim()),
            )?;
            Ok(id)
        }
    }
}

/// Retires a class. Never deletes: historical marks, enrollments and report
/// cards keep pointing at it (SRS 14.5).
#[tauri::command]
pub fn retire_class(state: State<'_, AppState>, class_id: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let conn = state.db.lock();

    let active_learners: i64 = conn.query_row(
        "SELECT COUNT(*) FROM enrollments WHERE class_id = ?1 AND status = 'active'",
        params![class_id],
        |row| row.get(0),
    )?;

    if active_learners > 0 {
        return Err(AppError::conflict(format!(
            "{active_learners} learners are still in this class. Move or drop them first."
        )));
    }

    let name: String = conn.query_row(
        "SELECT name FROM classes WHERE id = ?1",
        params![class_id],
        |row| row.get(0),
    )?;

    conn.execute(
        "UPDATE classes SET status = 'retired', updated_at = ?1 WHERE id = ?2",
        params![Utc::now().to_rfc3339(), class_id],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "class.retire",
        "class",
        &class_id,
        format!("Retired class {name}"),
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Subject catalogue (FR-B5)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_subjects(state: State<'_, AppState>) -> AppResult<Vec<SubjectRow>> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let conn = state.db.lock();
    let mut stmt = conn.prepare(
        "SELECT s.id, s.code, s.name, s.max_score, s.pass_mark, s.status,
                (SELECT COUNT(*) FROM class_subjects cs
                  WHERE cs.subject_id = s.id AND cs.status = 'active') AS used_by
         FROM subjects s
         WHERE s.status = 'active'
         ORDER BY s.name ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(SubjectRow {
            id: row.get(0)?,
            code: row.get(1)?,
            name: row.get(2)?,
            max_score: row.get(3)?,
            pass_mark: row.get(4)?,
            status: row.get(5)?,
            used_by_classes: row.get(6)?,
        })
    })?;

    let collected = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(collected)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSubjectRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub code: String,
    pub name: String,
    #[serde(default = "hundred")]
    pub max_score: f64,
    #[serde(default = "fifty")]
    pub pass_mark: f64,
}

fn hundred() -> f64 {
    100.0
}
fn fifty() -> f64 {
    50.0
}

#[tauri::command]
pub fn save_subject(state: State<'_, AppState>, request: SaveSubjectRequest) -> AppResult<String> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    if request.name.trim().is_empty() {
        return Err(AppError::validation("A subject needs a name."));
    }
    if request.max_score <= 0.0 {
        return Err(AppError::validation(
            "A subject's maximum score must be above zero.",
        ));
    }
    if request.pass_mark < 0.0 || request.pass_mark > request.max_score {
        return Err(AppError::validation(
            "The pass mark must sit between zero and the maximum score.",
        ));
    }

    let conn = state.db.lock();
    let now = Utc::now().to_rfc3339();

    match request.id {
        // Renaming a catalogue subject keeps its id, which is precisely why
        // multi-year history survives a rename (FR-C13).
        Some(id) => {
            conn.execute(
                "UPDATE subjects SET code = ?1, name = ?2, max_score = ?3, pass_mark = ?4, updated_at = ?5
                 WHERE id = ?6",
                params![
                    request.code.trim().to_uppercase(),
                    request.name.trim(),
                    request.max_score,
                    request.pass_mark,
                    now,
                    id
                ],
            )?;
            audit::record(
                &conn,
                Some(&session),
                "subject.update",
                "subject",
                &id,
                format!("Updated subject {}", request.name.trim()),
            )?;
            Ok(id)
        }
        None => {
            let id = new_id(prefix::SUBJECT);
            conn.execute(
                "INSERT INTO subjects (id, code, name, max_score, pass_mark, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6)",
                params![
                    id,
                    request.code.trim().to_uppercase(),
                    request.name.trim(),
                    request.max_score,
                    request.pass_mark,
                    now
                ],
            )?;
            audit::record(
                &conn,
                Some(&session),
                "subject.create",
                "subject",
                &id,
                format!("Added subject {} to the catalogue", request.name.trim()),
            )?;
            Ok(id)
        }
    }
}

// ---------------------------------------------------------------------------
// Per-class subjects (FR-C13)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_class_subjects(
    state: State<'_, AppState>,
    class_id: String,
) -> AppResult<Vec<ClassSubjectRow>> {
    let session = state.sessions.require()?;
    session.require_view_class(&class_id)?;

    let conn = state.db.lock();

    let mut stmt = conn.prepare(
        "SELECT cs.id, cs.class_id, cs.subject_id,
                s.name AS catalogue_name,
                COALESCE(cs.display_name, s.name) AS display_name,
                s.code,
                COALESCE(cs.max_score, s.max_score) AS max_score,
                COALESCE(cs.pass_mark, s.pass_mark) AS pass_mark,
                cs.is_core, cs.position, cs.status,
                COALESCE(cs.grading_system_id, c.default_grading_system_id, 'gs_percentage') AS gs_id,
                cs.grading_system_id IS NOT NULL AS overridden,
                (SELECT u.full_name FROM teacher_assignments ta
                  JOIN users u ON u.id = ta.user_id
                  WHERE ta.class_id = cs.class_id AND ta.subject_id = cs.subject_id
                    AND ta.role = 'subject_teacher' AND ta.status = 'active'
                  LIMIT 1) AS teacher_name
         FROM class_subjects cs
         JOIN subjects s ON s.id = cs.subject_id
         JOIN classes c ON c.id = cs.class_id
         WHERE cs.class_id = ?1 AND cs.status = 'active'
         ORDER BY cs.is_core DESC, cs.position ASC, display_name ASC",
    )?;

    let raw: Vec<(ClassSubjectRow, String)> = stmt
        .query_map(params![class_id], |row| {
            let gs_id: String = row.get("gs_id")?;
            Ok((
                ClassSubjectRow {
                    id: row.get("id")?,
                    class_id: row.get("class_id")?,
                    subject_id: row.get("subject_id")?,
                    catalogue_name: row.get("catalogue_name")?,
                    display_name: row.get("display_name")?,
                    code: row.get("code")?,
                    max_score: row.get("max_score")?,
                    pass_mark: row.get("pass_mark")?,
                    is_core: row.get::<_, i64>("is_core")? != 0,
                    position: row.get("position")?,
                    status: row.get("status")?,
                    grading_system_id: gs_id.clone(),
                    grading_system_name: String::new(),
                    grading_overridden: row.get::<_, i64>("overridden")? != 0,
                    teacher_name: row.get("teacher_name")?,
                },
                gs_id,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    // Name the grading systems in one pass rather than a join per row.
    let ids: Vec<String> = raw.iter().map(|(_, gs)| gs.clone()).collect();
    let systems = repo::load_grading_systems_by_id(&conn, &ids)?;

    Ok(raw
        .into_iter()
        .map(|(mut row, gs_id)| {
            row.grading_system_name = systems
                .get(&gs_id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "Percentage Only".into());
            row
        })
        .collect())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddClassSubjectRequest {
    pub class_id: String,
    pub subject_id: String,
    #[serde(default)]
    pub is_core: bool,
}

#[tauri::command]
pub fn add_class_subject(
    state: State<'_, AppState>,
    request: AddClassSubjectRequest,
) -> AppResult<String> {
    let session = state.sessions.require()?;
    session.require_manage_class(&request.class_id)?;

    let conn = state.db.lock();
    let now = Utc::now().to_rfc3339();

    // A subject retired from this class earlier comes back rather than being
    // duplicated — its historical marks are still attached to the same row.
    let existing: Option<(String, String)> = conn
        .query_row(
            "SELECT id, status FROM class_subjects WHERE class_id = ?1 AND subject_id = ?2",
            params![request.class_id, request.subject_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    let subject_name: String = conn.query_row(
        "SELECT name FROM subjects WHERE id = ?1",
        params![request.subject_id],
        |row| row.get(0),
    )?;

    let id = match existing {
        Some((id, status)) => {
            if status == "active" {
                return Err(AppError::conflict(format!(
                    "{subject_name} is already taught in this class."
                )));
            }
            conn.execute(
                "UPDATE class_subjects SET status = 'active', is_core = ?1, updated_at = ?2 WHERE id = ?3",
                params![if request.is_core { 1 } else { 0 }, now, id],
            )?;
            id
        }
        None => {
            let next_position: i64 = conn.query_row(
                "SELECT COALESCE(MAX(position), 0) + 1 FROM class_subjects WHERE class_id = ?1",
                params![request.class_id],
                |row| row.get(0),
            )?;
            let id = new_id(prefix::CLASS_SUBJECT);
            conn.execute(
                "INSERT INTO class_subjects
                    (id, class_id, subject_id, position, is_core, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6)",
                params![
                    id,
                    request.class_id,
                    request.subject_id,
                    next_position,
                    if request.is_core { 1 } else { 0 },
                    now
                ],
            )?;
            id
        }
    };

    audit::record(
        &conn,
        Some(&session),
        "class_subject.add",
        "class_subject",
        &id,
        format!("Added {subject_name} to the class"),
    )?;

    Ok(id)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateClassSubjectRequest {
    pub id: String,
    /// `None` clears the rename and falls back to the catalogue name.
    #[serde(default)]
    pub display_name: Option<String>,
    /// `None` clears the override and falls back to the class default.
    #[serde(default)]
    pub grading_system_id: Option<String>,
    #[serde(default)]
    pub max_score: Option<f64>,
    #[serde(default)]
    pub pass_mark: Option<f64>,
    #[serde(default)]
    pub is_core: Option<bool>,
}

#[tauri::command]
pub fn update_class_subject(
    state: State<'_, AppState>,
    request: UpdateClassSubjectRequest,
) -> AppResult<()> {
    let session = state.sessions.require()?;

    let conn = state.db.lock();
    let class_id: String = conn.query_row(
        "SELECT class_id FROM class_subjects WHERE id = ?1",
        params![request.id],
        |row| row.get(0),
    )?;
    session.require_manage_class(&class_id)?;

    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);

    conn.execute(
        "UPDATE class_subjects
         SET display_name      = ?1,
             grading_system_id = ?2,
             max_score         = COALESCE(?3, max_score),
             pass_mark         = COALESCE(?4, pass_mark),
             is_core           = COALESCE(?5, is_core),
             updated_at        = ?6
         WHERE id = ?7",
        params![
            display_name,
            request.grading_system_id,
            request.max_score,
            request.pass_mark,
            request.is_core.map(|core| if core { 1 } else { 0 }),
            Utc::now().to_rfc3339(),
            request.id,
        ],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "class_subject.update",
        "class_subject",
        &request.id,
        "Changed a class subject's name, grading or maximum",
    )?;

    Ok(())
}

/// Removes a subject from a class going forward. Historical marks are
/// untouched — FR-C13 is explicit about this.
#[tauri::command]
pub fn retire_class_subject(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let session = state.sessions.require()?;

    let conn = state.db.lock();
    let (class_id, name): (String, String) = conn.query_row(
        "SELECT cs.class_id, COALESCE(cs.display_name, s.name)
         FROM class_subjects cs JOIN subjects s ON s.id = cs.subject_id
         WHERE cs.id = ?1",
        params![id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    session.require_manage_class(&class_id)?;

    conn.execute(
        "UPDATE class_subjects SET status = 'retired', updated_at = ?1 WHERE id = ?2",
        params![Utc::now().to_rfc3339(), id],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "class_subject.retire",
        "class_subject",
        &id,
        format!("Removed {name} from the class going forward; past marks kept"),
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Grading systems
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_grading_systems(state: State<'_, AppState>) -> AppResult<Vec<GradingSystem>> {
    state.sessions.require()?;
    let conn = state.db.lock();
    repo::load_all_grading_systems(&conn)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveGradingSystemRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub kind: String,
    pub bands: Vec<BandInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BandInput {
    pub label: String,
    pub lower_bound: f64,
    pub upper_bound: f64,
    #[serde(default)]
    pub points: Option<f64>,
    #[serde(default)]
    pub remark: Option<String>,
}

/// The Custom Band Builder (SRS 4.2). Validation runs before anything is
/// written, so a system with a gap or an overlap can never reach a report card.
#[tauri::command]
pub fn save_grading_system(
    state: State<'_, AppState>,
    request: SaveGradingSystemRequest,
) -> AppResult<String> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    if request.name.trim().is_empty() {
        return Err(AppError::validation("A grading system needs a name."));
    }

    let kind = crate::domain::grading::GradingKind::parse(&request.kind)?;

    let bands: Vec<crate::domain::grading::Band> = request
        .bands
        .iter()
        .enumerate()
        .map(|(index, band)| crate::domain::grading::Band {
            id: new_id(prefix::GRADING_BAND),
            label: band.label.trim().to_string(),
            lower_bound: band.lower_bound,
            upper_bound: band.upper_bound,
            points: band.points,
            remark: band.remark.clone(),
            position: index as i64 + 1,
        })
        .collect();

    GradingSystem::validate(&bands)?;

    let mut conn = state.db.lock();
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction()?;

    let id = match &request.id {
        Some(id) => {
            let editable: i64 = tx.query_row(
                "SELECT is_editable FROM grading_systems WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )?;
            if editable == 0 {
                return Err(AppError::conflict(
                    "A built-in grading system cannot be edited. Duplicate it instead.",
                ));
            }
            tx.execute(
                "UPDATE grading_systems SET name = ?1, description = ?2, kind = ?3, updated_at = ?4
                 WHERE id = ?5",
                params![request.name.trim(), request.description, kind.as_str(), now, id],
            )?;
            tx.execute(
                "DELETE FROM grading_bands WHERE grading_system_id = ?1",
                params![id],
            )?;
            id.clone()
        }
        None => {
            let id = new_id(prefix::GRADING_SYSTEM);
            let code = request
                .name
                .trim()
                .to_uppercase()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_");
            tx.execute(
                "INSERT INTO grading_systems
                    (id, code, name, description, kind, is_builtin, is_editable, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 1, 'active', ?6, ?6)",
                params![id, format!("{code}_{}", &id[3..9]), request.name.trim(), request.description, kind.as_str(), now],
            )?;
            id
        }
    };

    for band in &bands {
        tx.execute(
            "INSERT INTO grading_bands
                (id, grading_system_id, label, lower_bound, upper_bound, points, remark, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                band.id,
                id,
                band.label,
                band.lower_bound,
                band.upper_bound,
                band.points,
                band.remark,
                band.position
            ],
        )?;
    }

    audit::record(
        &tx,
        Some(&session),
        "grading_system.save",
        "grading_system",
        &id,
        format!(
            "Saved grading system {} with {} bands",
            request.name.trim(),
            bands.len()
        ),
    )?;

    tx.commit()?;
    Ok(id)
}

// ---------------------------------------------------------------------------
// Academic calendar (FR-B3)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_academic_years(state: State<'_, AppState>) -> AppResult<Vec<AcademicYearRow>> {
    state.sessions.require()?;
    let conn = state.db.lock();

    let mut year_stmt = conn.prepare(
        "SELECT id, label, start_date, end_date, status FROM academic_years ORDER BY label DESC",
    )?;
    let years: Vec<(String, String, Option<String>, Option<String>, String)> = year_stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(year_stmt);

    let mut out = Vec::new();
    for (year_id, label, start_date, end_date, status) in years {
        let mut term_stmt = conn.prepare(
            "SELECT id, seq, name, start_date, end_date, status
             FROM terms WHERE academic_year_id = ?1 ORDER BY seq ASC",
        )?;
        let terms: Vec<(String, i64, String, Option<String>, Option<String>, String)> = term_stmt
            .query_map(params![year_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        drop(term_stmt);

        let mut term_rows = Vec::new();
        for (term_id, seq, name, term_start, term_end, term_status) in terms {
            let mut exam_stmt = conn.prepare(
                "SELECT id, seq, code, name, weight, is_final, scheduled_date, status
                 FROM exams WHERE term_id = ?1 ORDER BY seq ASC",
            )?;
            let exams: Vec<ExamRow> = exam_stmt
                .query_map(params![term_id], |row| {
                    Ok(ExamRow {
                        id: row.get(0)?,
                        term_id: term_id.clone(),
                        seq: row.get(1)?,
                        code: row.get(2)?,
                        name: row.get(3)?,
                        weight: row.get(4)?,
                        is_final: row.get::<_, i64>(5)? != 0,
                        scheduled_date: row.get(6)?,
                        status: row.get(7)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            drop(exam_stmt);

            term_rows.push(TermRow {
                id: term_id,
                academic_year_id: year_id.clone(),
                seq,
                name,
                start_date: term_start,
                end_date: term_end,
                status: term_status,
                exams,
            });
        }

        out.push(AcademicYearRow {
            id: year_id,
            label,
            start_date,
            end_date,
            status,
            terms: term_rows,
        });
    }

    Ok(out)
}

/// Opens a term for marks entry, closing whichever one was open.
///
/// Exactly one term is open at a time, so "where do I enter marks?" always has
/// one answer and a mark can never be filed against last term by accident.
#[tauri::command]
pub fn open_term(state: State<'_, AppState>, term_id: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let mut conn = state.db.lock();
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction()?;

    let (name, year_id): (String, String) = tx.query_row(
        "SELECT name, academic_year_id FROM terms WHERE id = ?1",
        params![term_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    tx.execute(
        "UPDATE terms SET status = 'closed', updated_at = ?1 WHERE status = 'open' AND id != ?2",
        params![now, term_id],
    )?;
    tx.execute(
        "UPDATE terms SET status = 'open', updated_at = ?1 WHERE id = ?2",
        params![now, term_id],
    )?;
    tx.execute(
        "UPDATE academic_years SET status = 'active', updated_at = ?1 WHERE id = ?2",
        params![now, year_id],
    )?;
    tx.execute(
        "UPDATE exams SET status = 'open', updated_at = ?1 WHERE term_id = ?2 AND status = 'planning'",
        params![now, term_id],
    )?;

    audit::record(
        &tx,
        Some(&session),
        "term.open",
        "term",
        &term_id,
        format!("Opened {name} for marks entry"),
    )?;

    tx.commit()?;
    Ok(())
}

#[tauri::command]
pub fn close_term(state: State<'_, AppState>, term_id: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let conn = state.db.lock();
    let now = Utc::now().to_rfc3339();

    let name: String = conn.query_row(
        "SELECT name FROM terms WHERE id = ?1",
        params![term_id],
        |row| row.get(0),
    )?;

    conn.execute(
        "UPDATE terms SET status = 'closed', updated_at = ?1 WHERE id = ?2",
        params![now, term_id],
    )?;
    conn.execute(
        "UPDATE exams SET status = 'closed', updated_at = ?1 WHERE term_id = ?2",
        params![now, term_id],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "term.close",
        "term",
        &term_id,
        format!("Closed {name}"),
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Feature toggles (FR-B7)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureFlags {
    pub student_photos: bool,
    pub exam_permits: bool,
    pub streams: bool,
    pub weekly_assignments: bool,
}

#[tauri::command]
pub fn get_features(state: State<'_, AppState>) -> AppResult<FeatureFlags> {
    state.sessions.require()?;
    let conn = state.db.lock();
    Ok(FeatureFlags {
        student_photos: repo::feature_enabled(&conn, "student_photos")?,
        exam_permits: repo::feature_enabled(&conn, "exam_permits")?,
        streams: repo::feature_enabled(&conn, "streams")?,
        weekly_assignments: repo::feature_enabled(&conn, "weekly_assignments")?,
    })
}

#[tauri::command]
pub fn set_feature(state: State<'_, AppState>, key: String, enabled: bool) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    if !matches!(
        key.as_str(),
        "student_photos" | "exam_permits" | "streams" | "weekly_assignments"
    ) {
        return Err(AppError::validation("Unknown feature."));
    }

    let conn = state.db.lock();
    repo::set_feature(&conn, &key, enabled)?;

    audit::record(
        &conn,
        Some(&session),
        "feature.toggle",
        "setting",
        &key,
        format!(
            "Turned {} {}",
            key.replace('_', " "),
            if enabled { "on" } else { "off" }
        ),
    )?;

    Ok(())
}
