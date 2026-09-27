//! Learners: search, bio-data, add and drop.
//!
//! FR-C7 (search by name or registration number), FR-C8 (bio-data, audited),
//! FR-C10 (add and drop — never delete).

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::domain::ids::{new_id, prefix};
use crate::domain::models::StudentRow;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const MAX_PHOTO_BYTES: usize = 2 * 1024 * 1024;

/// The roster for one class in the current academic year.
#[tauri::command]
pub fn list_class_roster(
    state: State<'_, AppState>,
    class_id: String,
    include_dropped: Option<bool>,
) -> AppResult<Vec<StudentRow>> {
    let session = state.sessions.require()?;
    session.require_view_class(&class_id)?;

    let conn = state.db.lock();
    let year_id = repo::current_academic_year_id(&conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;

    let include_dropped = include_dropped.unwrap_or(false);

    let mut stmt = conn.prepare(
        "SELECT s.id, s.reg_number, s.full_name, s.gender, s.date_of_birth, s.lin,
                LENGTH(s.photo_png) AS photo_len,
                s.guardian_name, s.guardian_phone, s.guardian_relationship, s.status,
                e.class_id, c.name AS class_name,
                e.stream_id, st.name AS stream_name,
                e.status AS enrollment_status
         FROM enrollments e
         JOIN students s ON s.id = e.student_id
         JOIN classes c ON c.id = e.class_id
         LEFT JOIN streams st ON st.id = e.stream_id
         WHERE e.class_id = ?1
           AND e.academic_year_id = ?2
           AND (?3 = 1 OR e.status = 'active')
         ORDER BY s.full_name COLLATE NOCASE ASC",
    )?;

    let rows = stmt.query_map(
        params![class_id, year_id, if include_dropped { 1 } else { 0 }],
        StudentRow::from_row,
    )?;

    let collected = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(collected)
}

/// FR-C7: find a learner by name or by registration number.
#[tauri::command]
pub fn search_students(state: State<'_, AppState>, query: String) -> AppResult<Vec<StudentRow>> {
    let session = state.sessions.require()?;
    let trimmed = query.trim();
    if trimmed.len() < 2 {
        return Ok(Vec::new());
    }

    let conn = state.db.lock();
    let year_id = repo::current_academic_year_id(&conn)?;
    let pattern = format!("%{trimmed}%");

    let mut stmt = conn.prepare(
        "SELECT s.id, s.reg_number, s.full_name, s.gender, s.date_of_birth, s.lin,
                LENGTH(s.photo_png) AS photo_len,
                s.guardian_name, s.guardian_phone, s.guardian_relationship, s.status,
                e.class_id, c.name AS class_name,
                e.stream_id, st.name AS stream_name,
                e.status AS enrollment_status
         FROM students s
         LEFT JOIN enrollments e
                ON e.student_id = s.id AND (?2 IS NULL OR e.academic_year_id = ?2)
         LEFT JOIN classes c ON c.id = e.class_id
         LEFT JOIN streams st ON st.id = e.stream_id
         WHERE s.full_name LIKE ?1 COLLATE NOCASE
            OR s.reg_number LIKE ?1 COLLATE NOCASE
         ORDER BY s.full_name COLLATE NOCASE ASC
         LIMIT 100",
    )?;

    let rows = stmt.query_map(params![pattern, year_id], StudentRow::from_row)?;

    let mut out = Vec::new();
    for row in rows {
        let student = row?;
        // A teacher searching institution-wide still only sees their own
        // classes' learners (FR-C1). An unenrolled learner is admin-only.
        let visible = match student.class_id.as_deref() {
            Some(class_id) => session.may_view_class(class_id),
            None => session.is_admin(),
        };
        if visible {
            out.push(student);
        }
    }

    Ok(out)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStudentRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub full_name: String,
    #[serde(default)]
    pub reg_number: Option<String>,
    #[serde(default)]
    pub gender: Option<String>,
    #[serde(default)]
    pub date_of_birth: Option<String>,
    #[serde(default)]
    pub lin: Option<String>,
    #[serde(default)]
    pub guardian_name: Option<String>,
    #[serde(default)]
    pub guardian_phone: Option<String>,
    #[serde(default)]
    pub guardian_relationship: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    /// Only used when creating. Existing learners move class through
    /// `transfer_student`, which keeps the history straight.
    #[serde(default)]
    pub class_id: Option<String>,
    #[serde(default)]
    pub stream_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStudentResult {
    pub id: String,
    pub reg_number: String,
}

/// FR-C10 (add) and FR-C8 (edit bio-data). Every change is audited with the
/// before-and-after, so a Class Teacher's edit can be told from their
/// Assistant's months later.
#[tauri::command]
pub fn save_student(
    state: State<'_, AppState>,
    request: SaveStudentRequest,
) -> AppResult<SaveStudentResult> {
    let session = state.sessions.require()?;

    if request.full_name.trim().is_empty() {
        return Err(AppError::validation("A learner needs a name."));
    }
    if let Some(gender) = request.gender.as_deref() {
        if !gender.is_empty() && !matches!(gender, "M" | "F") {
            return Err(AppError::validation("Gender must be M or F."));
        }
    }

    let mut conn = state.db.lock();
    let now = Utc::now().to_rfc3339();

    match request.id {
        // -------- Edit ------------------------------------------------------
        Some(id) => {
            let tx = conn.transaction()?;

            let before: (String, Option<String>, Option<String>, Option<String>) = tx
                .query_row(
                    "SELECT full_name, gender, lin, guardian_phone FROM students WHERE id = ?1",
                    params![id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?
                .ok_or_else(|| AppError::not_found("That learner"))?;

            // A teacher may only edit a learner in a class they manage.
            let class_id: Option<String> = tx
                .query_row(
                    "SELECT class_id FROM enrollments
                     WHERE student_id = ?1 AND status = 'active'
                     ORDER BY joined_at DESC LIMIT 1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?;

            match class_id.as_deref() {
                Some(class) => session.require_manage_class(class)?,
                None => session.require_admin()?,
            }

            tx.execute(
                "UPDATE students
                 SET full_name = ?1, gender = ?2, date_of_birth = ?3, lin = ?4,
                     guardian_name = ?5, guardian_phone = ?6, guardian_relationship = ?7,
                     address = ?8, updated_at = ?9
                 WHERE id = ?10",
                params![
                    request.full_name.trim(),
                    blank_to_none(request.gender.as_deref()),
                    blank_to_none(request.date_of_birth.as_deref()),
                    blank_to_none(request.lin.as_deref()),
                    blank_to_none(request.guardian_name.as_deref()),
                    blank_to_none(request.guardian_phone.as_deref()),
                    blank_to_none(request.guardian_relationship.as_deref()),
                    blank_to_none(request.address.as_deref()),
                    now,
                    id,
                ],
            )?;

            let mut changes = Vec::new();
            if before.0 != request.full_name.trim() {
                changes.push(format!("name '{}' → '{}'", before.0, request.full_name.trim()));
            }
            if before.1.as_deref() != blank_to_none(request.gender.as_deref()) {
                changes.push("gender".to_string());
            }
            if before.2.as_deref() != blank_to_none(request.lin.as_deref()) {
                changes.push("LIN".to_string());
            }
            if before.3.as_deref() != blank_to_none(request.guardian_phone.as_deref()) {
                changes.push("guardian phone".to_string());
            }

            let summary = if changes.is_empty() {
                format!("Saved {} with no changes", request.full_name.trim())
            } else {
                format!(
                    "Edited {}: {}",
                    request.full_name.trim(),
                    changes.join(", ")
                )
            };

            audit::record(&tx, Some(&session), "student.update", "student", &id, summary)?;

            let reg_number: String = tx.query_row(
                "SELECT reg_number FROM students WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )?;

            tx.commit()?;
            Ok(SaveStudentResult { id, reg_number })
        }

        // -------- Add -------------------------------------------------------
        None => {
            let class_id = request
                .class_id
                .clone()
                .ok_or_else(|| AppError::validation("Choose a class for the new learner."))?;
            session.require_manage_class(&class_id)?;

            let tx = conn.transaction()?;

            let year_id = repo::current_academic_year_id(&tx)?
                .ok_or_else(|| AppError::validation("No academic year is active yet."))?;

            // A supplied number is honoured (a school migrating its own
            // numbering); otherwise RM allocates the next one (FR-B5).
            let reg_number = match blank_to_none(request.reg_number.as_deref()) {
                Some(supplied) => {
                    let taken: i64 = tx.query_row(
                        "SELECT COUNT(*) FROM students WHERE reg_number = ?1 COLLATE NOCASE",
                        params![supplied],
                        |row| row.get(0),
                    )?;
                    if taken > 0 {
                        return Err(AppError::conflict(format!(
                            "Registration number {supplied} is already in use."
                        )));
                    }
                    supplied.to_string()
                }
                None => repo::next_registration_number(&tx)?,
            };

            let id = new_id(prefix::STUDENT);
            tx.execute(
                "INSERT INTO students
                    (id, reg_number, full_name, gender, date_of_birth, lin,
                     guardian_name, guardian_phone, guardian_relationship, address,
                     status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'active', ?11, ?11)",
                params![
                    id,
                    reg_number,
                    request.full_name.trim(),
                    blank_to_none(request.gender.as_deref()),
                    blank_to_none(request.date_of_birth.as_deref()),
                    blank_to_none(request.lin.as_deref()),
                    blank_to_none(request.guardian_name.as_deref()),
                    blank_to_none(request.guardian_phone.as_deref()),
                    blank_to_none(request.guardian_relationship.as_deref()),
                    blank_to_none(request.address.as_deref()),
                    now,
                ],
            )?;

            tx.execute(
                "INSERT INTO enrollments
                    (id, student_id, class_id, stream_id, academic_year_id,
                     status, joined_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6, ?6)",
                params![
                    new_id(prefix::ENROLLMENT),
                    id,
                    class_id,
                    request.stream_id,
                    year_id,
                    now,
                ],
            )?;

            let class_name: String = tx.query_row(
                "SELECT name FROM classes WHERE id = ?1",
                params![class_id],
                |row| row.get(0),
            )?;

            audit::record(
                &tx,
                Some(&session),
                "student.add",
                "student",
                &id,
                format!(
                    "Added {} ({reg_number}) to {class_name}",
                    request.full_name.trim()
                ),
            )?;

            tx.commit()?;
            Ok(SaveStudentResult { id, reg_number })
        }
    }
}

/// FR-C10: DROP, never DELETE. The learner, their marks and their history all
/// stay; only the enrollment closes.
#[tauri::command]
pub fn drop_student(
    state: State<'_, AppState>,
    student_id: String,
    reason: String,
) -> AppResult<()> {
    let session = state.sessions.require()?;

    if reason.trim().len() < 3 {
        return Err(AppError::validation(
            "Give a reason for dropping this learner. It is kept on the record.",
        ));
    }

    let mut conn = state.db.lock();
    let tx = conn.transaction()?;
    let now = Utc::now().to_rfc3339();

    let (enrollment_id, class_id, full_name, reg_number): (String, String, String, String) = tx
        .query_row(
            "SELECT e.id, e.class_id, s.full_name, s.reg_number
             FROM enrollments e JOIN students s ON s.id = e.student_id
             WHERE e.student_id = ?1 AND e.status = 'active'
             ORDER BY e.joined_at DESC LIMIT 1",
            params![student_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("An active enrollment for that learner"))?;

    session.require_manage_class(&class_id)?;

    tx.execute(
        "UPDATE enrollments SET status = 'dropped', left_at = ?1, drop_reason = ?2, updated_at = ?1
         WHERE id = ?3",
        params![now, reason.trim(), enrollment_id],
    )?;
    tx.execute(
        "UPDATE students SET status = 'dropped', updated_at = ?1 WHERE id = ?2",
        params![now, student_id],
    )?;

    audit::record(
        &tx,
        Some(&session),
        "student.drop",
        "student",
        &student_id,
        format!("Dropped {full_name} ({reg_number}): {}", reason.trim()),
    )?;

    tx.commit()?;
    Ok(())
}

/// Puts a dropped learner back on a roster. The original enrollment keeps its
/// dropped status, so the add/drop history in FR-E1 stays truthful.
#[tauri::command]
pub fn readmit_student(
    state: State<'_, AppState>,
    student_id: String,
    class_id: String,
    stream_id: Option<String>,
) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_manage_class(&class_id)?;

    let mut conn = state.db.lock();
    let tx = conn.transaction()?;
    let now = Utc::now().to_rfc3339();

    let year_id = repo::current_academic_year_id(&tx)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;

    let full_name: String = tx.query_row(
        "SELECT full_name FROM students WHERE id = ?1",
        params![student_id],
        |row| row.get(0),
    )?;

    let already: i64 = tx.query_row(
        "SELECT COUNT(*) FROM enrollments
         WHERE student_id = ?1 AND academic_year_id = ?2 AND status = 'active'",
        params![student_id, year_id],
        |row| row.get(0),
    )?;
    if already > 0 {
        return Err(AppError::conflict(format!(
            "{full_name} is already on a roster this year."
        )));
    }

    // One enrollment row per learner per year, so readmission in the same year
    // reopens the existing row rather than inserting a second one.
    let existing: Option<String> = tx
        .query_row(
            "SELECT id FROM enrollments WHERE student_id = ?1 AND academic_year_id = ?2",
            params![student_id, year_id],
            |row| row.get(0),
        )
        .optional()?;

    match existing {
        Some(enrollment_id) => {
            tx.execute(
                "UPDATE enrollments
                 SET status = 'active', class_id = ?1, stream_id = ?2,
                     left_at = NULL, drop_reason = NULL, updated_at = ?3
                 WHERE id = ?4",
                params![class_id, stream_id, now, enrollment_id],
            )?;
        }
        None => {
            tx.execute(
                "INSERT INTO enrollments
                    (id, student_id, class_id, stream_id, academic_year_id,
                     status, joined_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6, ?6)",
                params![
                    new_id(prefix::ENROLLMENT),
                    student_id,
                    class_id,
                    stream_id,
                    year_id,
                    now
                ],
            )?;
        }
    }

    tx.execute(
        "UPDATE students SET status = 'active', updated_at = ?1 WHERE id = ?2",
        params![now, student_id],
    )?;

    let class_name: String = tx.query_row(
        "SELECT name FROM classes WHERE id = ?1",
        params![class_id],
        |row| row.get(0),
    )?;

    audit::record(
        &tx,
        Some(&session),
        "student.readmit",
        "student",
        &student_id,
        format!("Readmitted {full_name} to {class_name}"),
    )?;

    tx.commit()?;
    Ok(())
}

/// Moves a learner between classes or streams within the same year.
#[tauri::command]
pub fn transfer_student(
    state: State<'_, AppState>,
    student_id: String,
    class_id: String,
    stream_id: Option<String>,
) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let conn = state.db.lock();
    let now = Utc::now().to_rfc3339();
    let year_id = repo::current_academic_year_id(&conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;

    let (full_name, from_class): (String, String) = conn.query_row(
        "SELECT s.full_name, c.name
         FROM enrollments e
         JOIN students s ON s.id = e.student_id
         JOIN classes c ON c.id = e.class_id
         WHERE e.student_id = ?1 AND e.academic_year_id = ?2 AND e.status = 'active'",
        params![student_id, year_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let to_class: String = conn.query_row(
        "SELECT name FROM classes WHERE id = ?1",
        params![class_id],
        |row| row.get(0),
    )?;

    conn.execute(
        "UPDATE enrollments SET class_id = ?1, stream_id = ?2, updated_at = ?3
         WHERE student_id = ?4 AND academic_year_id = ?5 AND status = 'active'",
        params![class_id, stream_id, now, student_id, year_id],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "student.transfer",
        "student",
        &student_id,
        format!("Moved {full_name} from {from_class} to {to_class}"),
    )?;

    Ok(())
}

/// FR-C8: the learner's photo, active only when the Photos toggle is on.
#[tauri::command]
pub fn set_student_photo(
    state: State<'_, AppState>,
    student_id: String,
    png: Vec<u8>,
) -> AppResult<()> {
    let session = state.sessions.require()?;

    if png.len() > MAX_PHOTO_BYTES {
        return Err(AppError::validation(
            "That photo is larger than 2 MB. Please use a smaller one.",
        ));
    }
    // Refuse anything that is not actually a PNG rather than storing whatever
    // bytes arrived and discovering it at print time.
    if !png.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Err(AppError::validation("That file is not a PNG image."));
    }

    let conn = state.db.lock();
    if !repo::feature_enabled(&conn, "student_photos")? {
        return Err(AppError::conflict(
            "Student Photos is switched off. Turn it on in the Optional Features panel first.",
        ));
    }

    let class_id: Option<String> = conn
        .query_row(
            "SELECT class_id FROM enrollments WHERE student_id = ?1 AND status = 'active' LIMIT 1",
            params![student_id],
            |row| row.get(0),
        )
        .optional()?;
    match class_id.as_deref() {
        Some(class) => session.require_manage_class(class)?,
        None => session.require_admin()?,
    }

    conn.execute(
        "UPDATE students SET photo_png = ?1, updated_at = ?2 WHERE id = ?3",
        params![png, Utc::now().to_rfc3339(), student_id],
    )?;

    audit::record(
        &conn,
        Some(&session),
        "student.photo",
        "student",
        &student_id,
        "Updated the learner's photo",
    )?;

    Ok(())
}

#[tauri::command]
pub fn get_student_photo(
    state: State<'_, AppState>,
    student_id: String,
) -> AppResult<Option<Vec<u8>>> {
    state.sessions.require()?;
    let conn = state.db.lock();
    Ok(conn
        .query_row(
            "SELECT photo_png FROM students WHERE id = ?1",
            params![student_id],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .optional()?
        .flatten())
}

fn blank_to_none(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}
