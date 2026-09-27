//! Shapes that cross the IPC boundary.
//!
//! These are read models, not an ORM. Each one is what a particular screen
//! needs, which keeps the queries explicit and stops the frontend from
//! receiving columns it has no business seeing (password hashes, photo blobs
//! it did not ask for).

use rusqlite::Row;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Institution {
    pub name: String,
    pub motto: Option<String>,
    pub has_logo: bool,
    pub accent_color: String,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub reg_number_pattern: String,
    pub setup_completed: bool,
}

impl Institution {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        let setup_completed_at: Option<String> = row.get("setup_completed_at")?;
        let logo_len: Option<i64> = row.get("logo_len")?;
        Ok(Institution {
            name: row.get("name")?,
            motto: row.get("motto")?,
            has_logo: logo_len.unwrap_or(0) > 0,
            accent_color: row.get("accent_color")?,
            address: row.get("address")?,
            phone: row.get("phone")?,
            email: row.get("email")?,
            reg_number_pattern: row.get("reg_number_pattern")?,
            setup_completed: setup_completed_at.is_some(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSummary {
    pub id: String,
    pub username: String,
    pub full_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub role: String,
    pub status: String,
    pub must_change_password: bool,
    pub last_login_at: Option<String>,
}

impl UserSummary {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(UserSummary {
            id: row.get("id")?,
            username: row.get("username")?,
            full_name: row.get("full_name")?,
            email: row.get("email")?,
            phone: row.get("phone")?,
            role: row.get("role")?,
            status: row.get("status")?,
            must_change_password: row.get::<_, i64>("must_change_password")? != 0,
            last_login_at: row.get("last_login_at")?,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassRow {
    pub id: String,
    pub code: String,
    pub name: String,
    pub level_kind: String,
    pub ladder_position: i64,
    pub default_grading_system_id: Option<String>,
    pub default_grading_system_name: Option<String>,
    pub status: String,
    pub learner_count: i64,
    pub subject_count: i64,
    pub class_teacher_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectRow {
    pub id: String,
    pub code: String,
    pub name: String,
    pub max_score: f64,
    pub pass_mark: f64,
    pub status: String,
    pub used_by_classes: i64,
}

/// A subject as taught in one class, with every override already resolved.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassSubjectRow {
    pub id: String,
    pub class_id: String,
    pub subject_id: String,
    /// The catalogue name, for the School Admin's benefit.
    pub catalogue_name: String,
    /// What this class calls it — the display rename, if any (FR-C13).
    pub display_name: String,
    pub code: String,
    pub max_score: f64,
    pub pass_mark: f64,
    pub is_core: bool,
    pub position: i64,
    pub status: String,
    pub grading_system_id: String,
    pub grading_system_name: String,
    /// True when this class overrode the class default for this subject.
    pub grading_overridden: bool,
    pub teacher_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcademicYearRow {
    pub id: String,
    pub label: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub status: String,
    pub terms: Vec<TermRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermRow {
    pub id: String,
    pub academic_year_id: String,
    pub seq: i64,
    pub name: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub status: String,
    pub exams: Vec<ExamRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamRow {
    pub id: String,
    pub term_id: String,
    pub seq: i64,
    pub code: String,
    pub name: String,
    pub weight: f64,
    pub is_final: bool,
    pub scheduled_date: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentRow {
    pub id: String,
    pub reg_number: String,
    pub full_name: String,
    pub gender: Option<String>,
    pub date_of_birth: Option<String>,
    pub lin: Option<String>,
    pub has_photo: bool,
    pub guardian_name: Option<String>,
    pub guardian_phone: Option<String>,
    pub guardian_relationship: Option<String>,
    pub status: String,
    pub class_id: Option<String>,
    pub class_name: Option<String>,
    pub stream_id: Option<String>,
    pub stream_name: Option<String>,
    pub enrollment_status: Option<String>,
}

impl StudentRow {
    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        let photo_len: Option<i64> = row.get("photo_len")?;
        Ok(StudentRow {
            id: row.get("id")?,
            reg_number: row.get("reg_number")?,
            full_name: row.get("full_name")?,
            gender: row.get("gender")?,
            date_of_birth: row.get("date_of_birth")?,
            lin: row.get("lin")?,
            has_photo: photo_len.unwrap_or(0) > 0,
            guardian_name: row.get("guardian_name")?,
            guardian_phone: row.get("guardian_phone")?,
            guardian_relationship: row.get("guardian_relationship")?,
            status: row.get("status")?,
            class_id: row.get("class_id")?,
            class_name: row.get("class_name")?,
            stream_id: row.get("stream_id")?,
            stream_name: row.get("stream_name")?,
            enrollment_status: row.get("enrollment_status")?,
        })
    }
}

/// One cell of the marks grid, as sent to and received from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkEntry {
    pub student_id: String,
    pub score: Option<f64>,
    #[serde(default)]
    pub is_absent: bool,
}

/// Helper for the many queries that need "is this row still live?".
pub fn parse_bool(row: &Row<'_>, column: &str) -> AppResult<bool> {
    Ok(row.get::<_, i64>(column)? != 0)
}
