//! First-run provisioning.
//!
//! One transaction creates the institution, the first School Admin, the
//! academic year with its terms and exams, the class ladder, and the subject
//! catalogue. Either the whole school exists afterwards or none of it does —
//! a half-configured school is worse than an unconfigured one.
//!
//! Per SRS 16.4 there is no payment gateway and no hardware fingerprint in this
//! phase: activation is manual, which is the recommendation for the pilot and
//! the first friendly schools.

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::domain::ids::{new_id, prefix};
use crate::domain::repo;
use crate::domain::setup_defaults::{
    DEFAULT_CLASSES, DEFAULT_EXAMS, DEFAULT_SUBJECTS, DEFAULT_TERM_NAMES,
};
use crate::error::{AppError, AppResult};
use crate::security::password::{self, Secret};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// What the wizard shows before anything is saved
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupDefaults {
    pub classes: Vec<DefaultClassView>,
    pub subjects: Vec<DefaultSubjectView>,
    pub terms: Vec<String>,
    pub exams: Vec<DefaultExamView>,
    pub suggested_year_label: String,
    pub reg_number_pattern: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultClassView {
    pub code: String,
    pub name: String,
    pub level_kind: String,
    pub ladder_position: i64,
    pub grading_system_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultSubjectView {
    pub code: String,
    pub name: String,
    pub is_core: bool,
    pub levels: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultExamView {
    pub code: String,
    pub name: String,
    pub weight: f64,
    pub is_final: bool,
}

#[tauri::command]
pub fn setup_defaults() -> AppResult<SetupDefaults> {
    Ok(SetupDefaults {
        classes: DEFAULT_CLASSES
            .iter()
            .map(|c| DefaultClassView {
                code: c.code.into(),
                name: c.name.into(),
                level_kind: c.level_kind.into(),
                ladder_position: c.ladder_position,
                grading_system_id: c.grading_system_id.into(),
            })
            .collect(),
        subjects: DEFAULT_SUBJECTS
            .iter()
            .map(|s| DefaultSubjectView {
                code: s.code.into(),
                name: s.name.into(),
                is_core: s.is_core,
                levels: s.levels.iter().map(|l| l.to_string()).collect(),
            })
            .collect(),
        terms: DEFAULT_TERM_NAMES.iter().map(|t| t.to_string()).collect(),
        exams: DEFAULT_EXAMS
            .iter()
            .map(|e| DefaultExamView {
                code: e.code.into(),
                name: e.name.into(),
                weight: e.weight,
                is_final: e.is_final,
            })
            .collect(),
        suggested_year_label: Utc::now().format("%Y").to_string(),
        reg_number_pattern: "{YEAR}/{SEQ:4}".into(),
    })
}

// ---------------------------------------------------------------------------
// Provisioning
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupRequest {
    pub institution_name: String,
    #[serde(default)]
    pub motto: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default = "default_accent")]
    pub accent_color: String,
    #[serde(default = "default_pattern")]
    pub reg_number_pattern: String,

    pub admin_full_name: String,
    pub admin_username: String,
    pub admin_password: Secret,
    #[serde(default)]
    pub admin_email: Option<String>,

    pub academic_year_label: String,
    /// Class codes from `setup_defaults`, in the order they should appear.
    pub class_codes: Vec<String>,
    pub terms: Vec<TermSpec>,
    pub exams: Vec<ExamSpec>,
}

fn default_accent() -> String {
    "#4F63D2".into()
}

fn default_pattern() -> String {
    "{YEAR}/{SEQ:4}".into()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermSpec {
    pub name: String,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamSpec {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub is_final: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupResult {
    pub institution_name: String,
    pub admin_username: String,
    pub classes_created: usize,
    pub subjects_created: usize,
    pub terms_created: usize,
}

#[tauri::command]
pub fn complete_setup(state: State<'_, AppState>, request: SetupRequest) -> AppResult<SetupResult> {
    let mut conn = state.db.lock();

    if repo::is_provisioned(&conn)? {
        return Err(AppError::conflict("This school is already set up."));
    }

    validate(&request)?;

    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction()?;

    // --- Institution (FR-B2) ------------------------------------------------
    tx.execute(
        "INSERT INTO institution
            (id, name, motto, accent_color, address, phone, email,
             reg_number_pattern, reg_number_next, setup_completed_at, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?8, ?8)",
        params![
            request.institution_name.trim(),
            request.motto.as_deref().map(str::trim),
            request.accent_color.trim(),
            request.address.as_deref().map(str::trim),
            request.phone.as_deref().map(str::trim),
            request.email.as_deref().map(str::trim),
            request.reg_number_pattern.trim(),
            now,
        ],
    )?;

    // --- First School Admin (FR-B4) ----------------------------------------
    let admin_id = new_id(prefix::USER);
    let admin_hash = password::hash(&request.admin_password)?;
    tx.execute(
        "INSERT INTO users
            (id, username, full_name, email, role, password_hash,
             must_change_password, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'school_admin', ?5, 0, 'active', ?6, ?6)",
        params![
            admin_id,
            request.admin_username.trim(),
            request.admin_full_name.trim(),
            request.admin_email.as_deref().map(str::trim),
            admin_hash,
            now,
        ],
    )?;

    // --- Classes (FR-B5) ----------------------------------------------------
    let mut classes_created = 0usize;
    let mut chosen_levels: Vec<&str> = Vec::new();

    for code in &request.class_codes {
        let Some(default) = DEFAULT_CLASSES.iter().find(|c| c.code == code) else {
            return Err(AppError::validation(format!("Unknown class '{code}'.")));
        };

        tx.execute(
            "INSERT INTO classes
                (id, code, name, level_kind, ladder_position, default_grading_system_id,
                 status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?7)",
            params![
                new_id(prefix::CLASS),
                default.code,
                default.name,
                default.level_kind,
                default.ladder_position,
                default.grading_system_id,
                now,
            ],
        )?;

        if !chosen_levels.contains(&default.level_kind) {
            chosen_levels.push(default.level_kind);
        }
        classes_created += 1;
    }

    // --- Subject catalogue (FR-B5) -----------------------------------------
    // Only the learning areas relevant to the levels this school actually
    // opened. A primary-only school has no business carrying nursery subjects.
    let mut subjects_created = 0usize;
    let mut subject_ids: Vec<(String, &str, bool, Vec<&str>)> = Vec::new();

    for subject in DEFAULT_SUBJECTS {
        if !subject.levels.iter().any(|l| chosen_levels.contains(l)) {
            continue;
        }
        let id = new_id(prefix::SUBJECT);
        tx.execute(
            "INSERT INTO subjects (id, code, name, max_score, pass_mark, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, 100, 50, 'active', ?4, ?4)",
            params![id, subject.code, subject.name, now],
        )?;
        subject_ids.push((
            id,
            subject.code,
            subject.is_core,
            subject.levels.to_vec(),
        ));
        subjects_created += 1;
    }

    // --- Attach subjects to each class -------------------------------------
    {
        let mut class_stmt =
            tx.prepare("SELECT id, code, level_kind FROM classes WHERE status = 'active'")?;
        let classes: Vec<(String, String, String)> = class_stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        drop(class_stmt);

        for (class_id, _class_code, level_kind) in classes {
            let mut position = 0i64;
            for (subject_id, _code, is_core, levels) in &subject_ids {
                if !levels.contains(&level_kind.as_str()) {
                    continue;
                }
                position += 1;
                tx.execute(
                    "INSERT INTO class_subjects
                        (id, class_id, subject_id, position, is_core, status, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6)",
                    params![
                        new_id(prefix::CLASS_SUBJECT),
                        class_id,
                        subject_id,
                        position,
                        if *is_core { 1 } else { 0 },
                        now,
                    ],
                )?;
            }
        }
    }

    // --- Academic calendar (FR-B3) -----------------------------------------
    let year_id = new_id(prefix::YEAR);
    tx.execute(
        "INSERT INTO academic_years (id, label, status, created_at, updated_at)
         VALUES (?1, ?2, 'active', ?3, ?3)",
        params![year_id, request.academic_year_label.trim(), now],
    )?;

    let mut terms_created = 0usize;
    for (index, term) in request.terms.iter().enumerate() {
        let term_id = new_id(prefix::TERM);
        let seq = index as i64 + 1;
        // The first term opens; the rest wait. Only one term is ever open, so
        // "where do I enter marks" has exactly one answer.
        let status = if index == 0 { "open" } else { "planning" };

        tx.execute(
            "INSERT INTO terms
                (id, academic_year_id, seq, name, start_date, end_date, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                term_id,
                year_id,
                seq,
                term.name.trim(),
                term.start_date.as_deref(),
                term.end_date.as_deref(),
                status,
                now,
            ],
        )?;

        for (exam_index, exam) in request.exams.iter().enumerate() {
            tx.execute(
                "INSERT INTO exams
                    (id, term_id, seq, code, name, weight, is_final, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                params![
                    new_id(prefix::EXAM),
                    term_id,
                    exam_index as i64 + 1,
                    exam.code.trim().to_uppercase(),
                    exam.name.trim(),
                    exam.weight,
                    if exam.is_final { 1 } else { 0 },
                    if index == 0 { "open" } else { "planning" },
                    now,
                ],
            )?;
        }

        terms_created += 1;
    }

    audit::record_system(
        &tx,
        "setup.complete",
        "institution",
        "1",
        format!(
            "{} was set up with {} classes, {} subjects and {} terms",
            request.institution_name.trim(),
            classes_created,
            subjects_created,
            terms_created
        ),
    )?;

    tx.commit()?;

    Ok(SetupResult {
        institution_name: request.institution_name.trim().to_string(),
        admin_username: request.admin_username.trim().to_string(),
        classes_created,
        subjects_created,
        terms_created,
    })
}

fn validate(request: &SetupRequest) -> AppResult<()> {
    if request.institution_name.trim().is_empty() {
        return Err(AppError::validation("The school's name is required."));
    }
    if request.admin_full_name.trim().is_empty() {
        return Err(AppError::validation("The School Admin's name is required."));
    }

    let username = request.admin_username.trim();
    if username.len() < 3 {
        return Err(AppError::validation(
            "The username must be at least 3 characters long.",
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err(AppError::validation(
            "A username may use letters, numbers, dots, dashes and underscores only.",
        ));
    }

    password::check_strength(&request.admin_password)?;

    if request.academic_year_label.trim().is_empty() {
        return Err(AppError::validation("The academic year is required."));
    }
    if request.class_codes.is_empty() {
        return Err(AppError::validation("Choose at least one class."));
    }
    if request.terms.is_empty() {
        return Err(AppError::validation("A year needs at least one term."));
    }
    if request.exams.is_empty() {
        return Err(AppError::validation(
            "A term needs at least one examination.",
        ));
    }

    let finals = request.exams.iter().filter(|e| e.is_final).count();
    if finals != 1 {
        return Err(AppError::validation(
            "Exactly one examination must be marked as the end-of-term set.",
        ));
    }

    let mut codes: Vec<String> = request
        .exams
        .iter()
        .map(|e| e.code.trim().to_uppercase())
        .collect();
    let before = codes.len();
    codes.sort();
    codes.dedup();
    if codes.len() != before {
        return Err(AppError::validation(
            "Two examinations share the same code.",
        ));
    }

    Ok(())
}
