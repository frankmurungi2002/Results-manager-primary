//! Registrations the school makes with someone else: UNEB (PLE candidates),
//! the Ministry's EMIS census, and UPSSA (primary schools' sports).
//!
//! All three fail the same way. A name spelt two ways, an initial instead of
//! a name, a missing photo or LIN, or a wrong year of birth goes onto an
//! outside system, and correcting it later costs money, a letter on headed
//! paper, or a learner's place. So one checker reads every learner the same
//! way, each screen adds its own rules (the exam's age range, a
//! championship's cut-off), and the fixes are made here, where the problem
//! is shown, before anything is typed into a portal.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Local, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::commands::exports::{self, staged, Cell, Sheet, StagedFile};
use crate::commands::reports::{base64_encode, envelope, DocumentEnvelope};
use crate::domain::ids::new_id;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::{Permission, Session};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// The learner checker
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// `name`, `gender`, `dob`, `lin`, `photo`, `age`, `team`.
    pub field: String,
    /// Must be fixed before registering; otherwise worth a second look.
    pub serious: bool,
    pub message: String,
}

fn issue(field: &str, serious: bool, message: impl Into<String>) -> Issue {
    Issue { field: field.into(), serious, message: message.into() }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnerCheck {
    pub student_id: String,
    pub reg_number: String,
    pub full_name: String,
    pub gender: Option<String>,
    pub date_of_birth: Option<String>,
    pub lin: Option<String>,
    pub special_needs: Option<String>,
    pub guardian_phone: Option<String>,
    pub class_id: String,
    pub class_name: String,
    pub has_photo: bool,
    /// Whole years on the date the screen measures age at.
    pub age: Option<i64>,
    pub issues: Vec<Issue>,
    /// Only on printed documents.
    pub photo_data_url: Option<String>,
    /// A sports team's event, e.g. "100 m".
    pub discipline: Option<String>,
}

impl LearnerCheck {
    pub fn ready(&self) -> bool {
        !self.issues.iter().any(|i| i.serious)
    }
}

struct Raw {
    id: String,
    reg_number: String,
    full_name: String,
    gender: Option<String>,
    date_of_birth: Option<String>,
    lin: Option<String>,
    special_needs: Option<String>,
    guardian_phone: Option<String>,
    class_id: String,
    class_name: String,
    ladder: i64,
    photo: Option<Vec<u8>>,
}

/// Active learners this year, optionally one class or a set of learners.
fn load(conn: &Connection, class_id: Option<&str>, ids: Option<&[String]>) -> AppResult<Vec<Raw>> {
    let year = repo::current_academic_year_id(conn)?.ok_or_else(|| AppError::validation("No academic year is active yet."))?;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.reg_number, s.full_name, s.gender, s.date_of_birth, s.lin, s.special_needs, s.guardian_phone,
                c.id, c.name, c.ladder_position, s.photo_png
         FROM enrollments e JOIN students s ON s.id = e.student_id JOIN classes c ON c.id = e.class_id
         WHERE e.academic_year_id = ?1 AND e.status = 'active' AND s.status = 'active'
           AND (?2 IS NULL OR e.class_id = ?2)
         ORDER BY c.ladder_position, s.full_name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map(params![year, class_id], |r| {
            Ok(Raw {
                id: r.get(0)?,
                reg_number: r.get(1)?,
                full_name: r.get(2)?,
                gender: r.get(3)?,
                date_of_birth: r.get(4)?,
                lin: r.get(5)?,
                special_needs: r.get(6)?,
                guardian_phone: r.get(7)?,
                class_id: r.get(8)?,
                class_name: r.get(9)?,
                ladder: r.get(10)?,
                photo: r.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(match ids {
        Some(ids) => rows.into_iter().filter(|r| ids.contains(&r.id)).collect(),
        None => rows,
    })
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(s, "%d/%m/%Y"))
        .or_else(|_| NaiveDate::parse_from_str(s, "%d-%m-%Y"))
        .ok()
}

pub fn age_on(dob: NaiveDate, on: NaiveDate) -> i64 {
    let mut age = (on.year() - dob.year()) as i64;
    if (on.month(), on.day()) < (dob.month(), dob.day()) {
        age -= 1;
    }
    age
}

/// What one screen expects of a learner.
#[derive(Debug, Clone)]
pub struct Rules {
    /// Age is measured on this day.
    pub age_on: NaiveDate,
    /// Ages outside this range are worth a second look.
    pub usual_ages: Option<(i64, i64)>,
    pub need_lin: bool,
    pub need_photo: bool,
    /// Too old if born before this day (a championship's cut-off).
    pub born_on_or_after: Option<NaiveDate>,
    /// Only boys or only girls.
    pub sex: Option<String>,
}

fn photo_size(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?.into_dimensions().ok()
}

fn check(raw: &Raw, rules: &Rules, lin_owner: &HashMap<String, Vec<String>>, usual_lin_len: Option<usize>, same_names: &HashSet<String>) -> Vec<Issue> {
    let mut out = Vec::new();

    // Names, as UNEB and UPSSA will print them.
    let parts: Vec<&str> = raw.full_name.split_whitespace().collect();
    if parts.len() < 2 {
        out.push(issue("name", true, "Give at least two names, as on the birth certificate."));
    }
    if parts.iter().any(|p| p.trim_end_matches('.').chars().count() <= 1) {
        out.push(issue("name", true, "Write every name in full: an initial is not a name."));
    }
    if raw.full_name.chars().any(|c| !(c.is_alphabetic() || c == ' ' || c == '-' || c == '\'' || c == '.')) {
        out.push(issue("name", true, "Names may only have letters, hyphens and apostrophes."));
    }
    if same_names.contains(&raw.full_name.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")) {
        out.push(issue("name", false, "Another learner has exactly this name: check both are spelt as on their birth certificates."));
    }

    if raw.gender.is_none() {
        out.push(issue("gender", true, "Sex is not recorded."));
    } else if let Some(sex) = &rules.sex {
        if raw.gender.as_deref() != Some(sex.as_str()) {
            out.push(issue("team", true, if sex == "M" { "This championship is for boys." } else { "This championship is for girls." }));
        }
    }

    match raw.date_of_birth.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => out.push(issue("dob", true, "No date of birth.")),
        Some(text) => match parse_date(text) {
            None => out.push(issue("dob", true, format!("\"{text}\" is not a date. Write the day, month and year."))),
            Some(dob) => {
                let age = age_on(dob, rules.age_on);
                if dob > Local::now().date_naive() {
                    out.push(issue("dob", true, "The date of birth is in the future."));
                } else if !(3..=25).contains(&age) {
                    out.push(issue("dob", true, format!("That makes the learner {age}: check the year of birth.")));
                } else if let Some((lo, hi)) = rules.usual_ages {
                    if age < lo || age > hi {
                        out.push(issue("age", false, format!("{age} years old: check the year of birth is right.")));
                    }
                }
                if let Some(cut) = rules.born_on_or_after {
                    if dob < cut {
                        out.push(issue("team", true, format!("Born {}, before the cut-off of {}: too old for this championship.", dob.format("%-d %b %Y"), cut.format("%-d %b %Y"))));
                    }
                }
            }
        },
    }

    match raw.lin.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None if rules.need_lin => out.push(issue("lin", true, "No LIN (the EMIS Learner Identification Number).")),
        None => {}
        Some(lin) => {
            if !lin.chars().all(|c| c.is_ascii_alphanumeric()) {
                out.push(issue("lin", true, "A LIN has only letters and digits: no spaces or dashes."));
            } else if let Some(others) = lin_owner.get(&lin.to_uppercase()) {
                let others: Vec<&String> = others.iter().filter(|n| **n != raw.full_name).collect();
                if !others.is_empty() {
                    out.push(issue("lin", true, format!("Same LIN as {}. Every learner has their own.", others[0])));
                }
            }
            if let Some(n) = usual_lin_len {
                if lin.chars().count() != n && lin.chars().all(|c| c.is_ascii_alphanumeric()) {
                    out.push(issue("lin", false, format!("{} characters, where the school's other LINs have {n}: check it was copied in full.", lin.chars().count())));
                }
            }
        }
    }

    match &raw.photo {
        None if rules.need_photo => out.push(issue("photo", true, "No photo.")),
        None => {}
        Some(bytes) => {
            if let Some((w, h)) = photo_size(bytes) {
                if w.min(h) < 200 {
                    out.push(issue("photo", false, format!("The photo is small ({w} x {h}); retake it so it prints clearly.")));
                }
            }
        }
    }
    out
}

fn normal_name(s: &str) -> String {
    s.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Checks learners against the rules, comparing LINs across the whole school.
fn check_all(conn: &Connection, raws: &[Raw], rules: &Rules, with_photos: bool) -> AppResult<Vec<LearnerCheck>> {
    // LINs and names across every active learner, not just this list: a
    // duplicate in another class is still a duplicate.
    let everyone = load(conn, None, None)?;
    let mut lin_owner: HashMap<String, Vec<String>> = HashMap::new();
    let mut lens: HashMap<usize, usize> = HashMap::new();
    for r in &everyone {
        if let Some(lin) = r.lin.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            lin_owner.entry(lin.to_uppercase()).or_default().push(r.full_name.clone());
            *lens.entry(lin.chars().count()).or_default() += 1;
        }
    }
    let with_lin: usize = lens.values().sum();
    // The usual length, only when most LINs agree on one.
    let usual_lin_len = lens.iter().max_by_key(|(_, n)| **n).filter(|(_, n)| with_lin >= 5 && **n * 10 >= with_lin * 7).map(|(len, _)| *len);
    let mut seen: HashMap<String, usize> = HashMap::new();
    for r in raws {
        *seen.entry(normal_name(&r.full_name)).or_default() += 1;
    }
    let same_names: HashSet<String> = seen.into_iter().filter(|(_, n)| *n > 1).map(|(k, _)| k).collect();

    Ok(raws
        .iter()
        .map(|r| LearnerCheck {
            student_id: r.id.clone(),
            reg_number: r.reg_number.clone(),
            full_name: r.full_name.clone(),
            gender: r.gender.clone(),
            date_of_birth: r.date_of_birth.clone(),
            lin: r.lin.clone(),
            special_needs: r.special_needs.clone(),
            guardian_phone: r.guardian_phone.clone(),
            class_id: r.class_id.clone(),
            class_name: r.class_name.clone(),
            has_photo: r.photo.is_some(),
            age: r.date_of_birth.as_deref().and_then(parse_date).map(|d| age_on(d, rules.age_on)),
            issues: check(r, rules, &lin_owner, usual_lin_len, &same_names),
            photo_data_url: if with_photos { r.photo.as_ref().map(|b| format!("data:image/png;base64,{}", base64_encode(b))) } else { None },
            discipline: None,
        })
        .collect())
}

/// Fixing what the checker found, where it was found.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnerFix {
    pub full_name: String,
    pub gender: Option<String>,
    pub date_of_birth: Option<String>,
    pub lin: Option<String>,
    pub special_needs: Option<String>,
}

fn blank(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

pub fn fix_learner_in(conn: &Connection, session: &Session, student_id: &str, fix: &LearnerFix) -> AppResult<()> {
    session.require(Permission::ManageLearners)?;
    let name = fix.full_name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(AppError::validation("The learner needs a name."));
    }
    let gender = blank(fix.gender.as_deref());
    if gender.as_deref().is_some_and(|g| g != "M" && g != "F") {
        return Err(AppError::validation("Sex is M or F."));
    }
    let dob = match blank(fix.date_of_birth.as_deref()) {
        None => None,
        Some(text) => Some(
            parse_date(&text)
                .ok_or_else(|| AppError::validation("Write the date of birth as a date."))?
                .format("%Y-%m-%d")
                .to_string(),
        ),
    };
    let lin = blank(fix.lin.as_deref()).map(|l| l.to_uppercase());
    if let Some(l) = &lin {
        if !l.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(AppError::validation("A LIN has only letters and digits."));
        }
        let taken: Option<String> = conn
            .query_row("SELECT full_name FROM students WHERE upper(trim(lin)) = ?1 AND id <> ?2 AND status = 'active'", params![l, student_id], |r| r.get(0))
            .optional()?;
        if let Some(other) = taken {
            return Err(AppError::conflict(format!("{other} already has that LIN.")));
        }
    }
    let before: (String, Option<String>, Option<String>, Option<String>) = conn
        .query_row("SELECT full_name, gender, date_of_birth, lin FROM students WHERE id = ?1", params![student_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .optional()?
        .ok_or_else(|| AppError::not_found("That learner"))?;
    conn.execute(
        "UPDATE students SET full_name = ?1, gender = ?2, date_of_birth = ?3, lin = ?4, special_needs = ?5, updated_at = ?6 WHERE id = ?7",
        params![name, gender, dob, lin, blank(fix.special_needs.as_deref()), Utc::now().to_rfc3339(), student_id],
    )?;
    let mut changed = Vec::new();
    if before.0 != name {
        changed.push(format!("name \"{}\" → \"{name}\"", before.0));
    }
    if before.1 != gender {
        changed.push("sex".to_string());
    }
    if before.2 != dob {
        changed.push(format!("date of birth {} → {}", before.2.as_deref().unwrap_or("none"), dob.as_deref().unwrap_or("none")));
    }
    if before.3 != lin {
        changed.push("LIN".to_string());
    }
    if !changed.is_empty() {
        audit::record(conn, Some(session), "student.fix_details", "student", student_id, format!("Corrected {name}'s {}", changed.join(", ")))?;
    }
    Ok(())
}

#[tauri::command]
pub fn fix_learner_details(state: State<'_, AppState>, student_id: String, fix: LearnerFix) -> AppResult<()> {
    let session = state.sessions.require()?;
    fix_learner_in(&state.db.lock(), &session, &student_id, &fix)
}

// ---------------------------------------------------------------------------
// PLE candidates (UNEB)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PleCheck {
    pub class_id: String,
    pub class_name: String,
    pub exam_year: i32,
    pub candidates: Vec<LearnerCheck>,
    pub ready: usize,
    /// Optional: the date parents return their slips by.
    pub return_by: Option<String>,
}

/// The top primary class: P7 on the standard ladder.
fn candidate_class(conn: &Connection) -> AppResult<String> {
    conn.query_row(
        "SELECT id FROM classes WHERE status = 'active' AND level_kind = 'primary' ORDER BY ladder_position DESC LIMIT 1",
        [],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::validation("Add the primary classes first."))
}

fn ple_rules(year: i32) -> Rules {
    Rules {
        // PLE sits in early November.
        age_on: NaiveDate::from_ymd_opt(year, 11, 1).unwrap(),
        usual_ages: Some((11, 16)),
        need_lin: true,
        need_photo: true,
        born_on_or_after: None,
        sex: None,
    }
}

pub fn ple_check_in(conn: &Connection, class_id: Option<String>, with_photos: bool) -> AppResult<PleCheck> {
    let class_id = match class_id {
        Some(c) => c,
        None => candidate_class(conn)?,
    };
    let class_name: String = conn.query_row("SELECT name FROM classes WHERE id = ?1", params![class_id], |r| r.get(0))?;
    let exam_year = Local::now().year();
    let raws = load(conn, Some(&class_id), None)?;
    let candidates = check_all(conn, &raws, &ple_rules(exam_year), with_photos)?;
    let ready = candidates.iter().filter(|c| c.ready()).count();
    Ok(PleCheck { class_id, class_name, exam_year, candidates, ready, return_by: None })
}

#[tauri::command]
pub fn ple_check(state: State<'_, AppState>, class_id: Option<String>) -> AppResult<PleCheck> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let check = ple_check_in(&conn, class_id, false)?;
    session.require_view_learners_of(&check.class_id)?;
    Ok(check)
}

/// `slips` (one per candidate for parents to check and sign) or `register`
/// (the list for the noticeboard).
#[tauri::command]
pub fn build_ple_document(
    state: State<'_, AppState>,
    class_id: Option<String>,
    ids: Vec<String>,
    kind: String,
    return_by: Option<String>,
) -> AppResult<DocumentEnvelope<PleCheck>> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let mut check = ple_check_in(&conn, class_id, true)?;
    session.require_view_learners_of(&check.class_id)?;
    if !ids.is_empty() {
        check.candidates.retain(|c| ids.contains(&c.student_id));
    }
    check.return_by = blank(return_by.as_deref());
    let kind = if kind == "register" { "ple_register" } else { "ple_slips" };
    envelope(&conn, kind, check)
}

// ---------------------------------------------------------------------------
// EMIS: identity numbers and the census
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffCheck {
    pub user_id: String,
    pub full_name: String,
    pub office: Option<String>,
    pub gender: Option<String>,
    pub phone: Option<String>,
    pub nin: Option<String>,
    pub tmis_number: Option<String>,
    pub issues: Vec<Issue>,
}

/// A Ugandan National ID number: 14 letters and digits, e.g. CM…, CF….
pub fn nin_issues(nin: Option<&str>, gender: Option<&str>) -> Vec<Issue> {
    let mut out = Vec::new();
    match nin.map(str::trim).filter(|s| !s.is_empty()) {
        None => out.push(issue("nin", true, "No National ID number (NIN).")),
        Some(n) => {
            if n.chars().count() != 14 || !n.chars().all(|c| c.is_ascii_alphanumeric()) {
                out.push(issue("nin", true, "A NIN is 14 letters and digits, as printed on the National ID."));
            } else {
                let second = n.chars().nth(1).map(|c| c.to_ascii_uppercase());
                if let (Some(g), Some(s)) = (gender, second) {
                    if (s == 'M' || s == 'F') && s.to_string() != g {
                        out.push(issue("nin", false, "The NIN's second letter does not match the sex recorded: check both."));
                    }
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgeCount {
    pub age: i64,
    pub boys: i64,
    pub girls: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CensusClass {
    pub class_name: String,
    pub boys: i64,
    pub girls: i64,
    /// Sex not recorded: counted so totals still add up, and shown so it gets fixed.
    pub unknown_sex: i64,
    pub new_boys: i64,
    pub new_girls: i64,
    pub repeat_boys: i64,
    pub repeat_girls: i64,
    pub special_needs: i64,
    pub by_age: Vec<AgeCount>,
    pub unknown_age: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmisCheck {
    pub census_date: String,
    pub year_label: String,
    pub classes: Vec<CensusClass>,
    /// The age columns used by every class's row.
    pub ages: Vec<i64>,
    pub learners_total: usize,
    pub learners_ready: usize,
    /// Only the learners with something to fix.
    pub learners: Vec<LearnerCheck>,
    pub staff: Vec<StaffCheck>,
}

pub fn emis_check_in(conn: &Connection, today: NaiveDate) -> AppResult<EmisCheck> {
    let year = repo::current_academic_year_id(conn)?.ok_or_else(|| AppError::validation("No academic year is active yet."))?;
    let year_label: String = conn.query_row("SELECT label FROM academic_years WHERE id = ?1", params![year], |r| r.get(0))?;
    let raws = load(conn, None, None)?;
    let rules = Rules { age_on: today, usual_ages: None, need_lin: true, need_photo: false, born_on_or_after: None, sex: None };
    let checks = check_all(conn, &raws, &rules, false)?;

    // Last year's class position for each learner: same position = repeating,
    // no enrolment = new to the school.
    let previous: Option<String> = conn
        .query_row(
            "WITH y AS (SELECT id, COALESCE(start_date, (SELECT MIN(start_date) FROM terms t WHERE t.academic_year_id = academic_years.id), label) AS starts
                        FROM academic_years)
             SELECT p.id FROM y p, y c WHERE c.id = ?1 AND p.starts < c.starts ORDER BY p.starts DESC LIMIT 1",
            params![year],
            |r| r.get(0),
        )
        .optional()?;
    let mut last_year: HashMap<String, i64> = HashMap::new();
    if let Some(prev) = &previous {
        let mut stmt = conn.prepare("SELECT e.student_id, c.ladder_position FROM enrollments e JOIN classes c ON c.id = e.class_id WHERE e.academic_year_id = ?1")?;
        let rows: Vec<(String, i64)> = stmt.query_map(params![prev], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        last_year.extend(rows);
    }

    let mut classes: Vec<CensusClass> = Vec::new();
    let mut ages: Vec<i64> = Vec::new();
    for (raw, chk) in raws.iter().zip(&checks) {
        if classes.last().map(|c| c.class_name != raw.class_name).unwrap_or(true) {
            classes.push(CensusClass {
                class_name: raw.class_name.clone(),
                boys: 0, girls: 0, unknown_sex: 0, new_boys: 0, new_girls: 0, repeat_boys: 0, repeat_girls: 0,
                special_needs: 0, by_age: Vec::new(), unknown_age: 0,
            });
        }
        let c = classes.last_mut().unwrap();
        let (boy, girl) = match raw.gender.as_deref() {
            Some("M") => (1, 0),
            Some("F") => (0, 1),
            _ => (0, 0),
        };
        c.boys += boy;
        c.girls += girl;
        if boy + girl == 0 {
            c.unknown_sex += 1;
        }
        match last_year.get(&raw.id) {
            None if previous.is_some() => {
                c.new_boys += boy;
                c.new_girls += girl;
            }
            Some(pos) if *pos == raw.ladder => {
                c.repeat_boys += boy;
                c.repeat_girls += girl;
            }
            _ => {}
        }
        if raw.special_needs.as_deref().is_some_and(|s| !s.trim().is_empty()) {
            c.special_needs += 1;
        }
        match chk.age.filter(|a| (3..=25).contains(a)) {
            None => c.unknown_age += 1,
            Some(age) => {
                if !ages.contains(&age) {
                    ages.push(age);
                }
                match c.by_age.iter_mut().find(|a| a.age == age) {
                    Some(a) => {
                        a.boys += boy;
                        a.girls += girl;
                    }
                    None => c.by_age.push(AgeCount { age, boys: boy, girls: girl }),
                }
            }
        }
    }
    ages.sort();
    for c in &mut classes {
        c.by_age.sort_by_key(|a| a.age);
    }

    let mut stmt = conn.prepare(
        "SELECT id, full_name, office, gender, phone, nin, tmis_number FROM users WHERE status = 'active' ORDER BY full_name COLLATE NOCASE",
    )?;
    let staff_rows: Vec<StaffCheck> = stmt
        .query_map([], |r| {
            Ok(StaffCheck {
                user_id: r.get(0)?,
                full_name: r.get(1)?,
                office: r.get(2)?,
                gender: r.get(3)?,
                phone: r.get(4)?,
                nin: r.get(5)?,
                tmis_number: r.get(6)?,
                issues: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut nins: HashMap<String, usize> = HashMap::new();
    for s in &staff_rows {
        if let Some(n) = s.nin.as_deref().map(|n| n.trim().to_uppercase()).filter(|n| !n.is_empty()) {
            *nins.entry(n).or_default() += 1;
        }
    }
    let staff = staff_rows
        .into_iter()
        .map(|mut s| {
            if s.gender.is_none() {
                s.issues.push(issue("gender", true, "Sex is not recorded."));
            }
            s.issues.extend(nin_issues(s.nin.as_deref(), s.gender.as_deref()));
            if s.nin.as_deref().map(|n| n.trim().to_uppercase()).is_some_and(|n| nins.get(&n).copied().unwrap_or(0) > 1) {
                s.issues.push(issue("nin", true, "Another member of staff has the same NIN."));
            }
            s
        })
        .collect();

    let learners_total = checks.len();
    let learners_ready = checks.iter().filter(|c| c.issues.is_empty()).count();
    Ok(EmisCheck {
        census_date: today.format("%Y-%m-%d").to_string(),
        year_label,
        classes,
        ages,
        learners_total,
        learners_ready,
        learners: checks.into_iter().filter(|c| !c.issues.is_empty()).collect(),
        staff,
    })
}

#[tauri::command]
pub fn emis_check(state: State<'_, AppState>) -> AppResult<EmisCheck> {
    let session = state.sessions.require()?;
    session.require(Permission::ViewAllLearners)?;
    emis_check_in(&state.db.lock(), Local::now().date_naive())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffIds {
    pub gender: Option<String>,
    pub nin: Option<String>,
    pub tmis_number: Option<String>,
}

pub fn save_staff_ids_in(conn: &Connection, session: &Session, user_id: &str, ids: &StaffIds) -> AppResult<()> {
    if session.user_id != user_id {
        session.require(Permission::ManageStaff)?;
    }
    let gender = blank(ids.gender.as_deref());
    if gender.as_deref().is_some_and(|g| g != "M" && g != "F") {
        return Err(AppError::validation("Sex is M or F."));
    }
    let nin = blank(ids.nin.as_deref()).map(|n| n.to_uppercase().replace(' ', ""));
    if let Some(n) = &nin {
        if n.chars().count() != 14 || !n.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(AppError::validation("A NIN is 14 letters and digits, as printed on the National ID."));
        }
    }
    let name: String = conn
        .query_row("SELECT full_name FROM users WHERE id = ?1", params![user_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That member of staff"))?;
    conn.execute(
        "UPDATE users SET gender = ?1, nin = ?2, tmis_number = ?3, updated_at = ?4 WHERE id = ?5",
        params![gender, nin, blank(ids.tmis_number.as_deref()), Utc::now().to_rfc3339(), user_id],
    )?;
    audit::record(conn, Some(session), "staff.ids", "user", user_id, format!("Recorded {name}'s sex, NIN and TMIS number"))?;
    Ok(())
}

#[tauri::command]
pub fn save_staff_ids(state: State<'_, AppState>, user_id: String, ids: StaffIds) -> AppResult<()> {
    let session = state.sessions.require()?;
    save_staff_ids_in(&state.db.lock(), &session, &user_id, &ids)
}

fn office_name(office: Option<&str>) -> &'static str {
    match office {
        Some("head_teacher") => "Headteacher",
        Some("director_of_studies") => "Director of Studies",
        Some("secretary") => "Secretary",
        Some("bursar") => "Bursar",
        _ => "Teacher",
    }
}

pub fn emis_workbook(conn: &Connection, check: &EmisCheck) -> AppResult<Vec<u8>> {
    let school: String = conn.query_row("SELECT name FROM institution LIMIT 1", [], |r| r.get(0)).optional()?.unwrap_or_default();
    let title = format!("{school}: EMIS census {} (counted {})", check.year_label, check.census_date);

    let mut enrol = Sheet::new("Enrolment", &["Class", "Boys", "Girls", "Total", "Sex not recorded", "New boys", "New girls", "Repeating boys", "Repeating girls", "Special needs"]).titled(&title);
    let mut totals = [0i64; 9];
    for c in &check.classes {
        let row = [c.boys, c.girls, c.boys + c.girls + c.unknown_sex, c.unknown_sex, c.new_boys, c.new_girls, c.repeat_boys, c.repeat_girls, c.special_needs];
        for (t, v) in totals.iter_mut().zip(row) {
            *t += v;
        }
        let mut cells: Vec<Cell> = vec![c.class_name.clone().into()];
        cells.extend(row.iter().map(|v| Cell::from(*v)));
        enrol.push(cells);
    }
    let mut cells: Vec<Cell> = vec!["Total".into()];
    cells.extend(totals.iter().map(|v| Cell::from(*v)));
    enrol.push(cells);

    let mut headers: Vec<String> = vec!["Class".into()];
    for a in &check.ages {
        headers.push(format!("{a} boys"));
        headers.push(format!("{a} girls"));
    }
    headers.push("Age not known".into());
    let mut by_age = Sheet { name: "Age by class".into(), title: Some(title.clone()), headers, rows: Vec::new() };
    for c in &check.classes {
        let mut row: Vec<Cell> = vec![c.class_name.clone().into()];
        for a in &check.ages {
            let found = c.by_age.iter().find(|x| x.age == *a);
            row.push(found.map(|x| x.boys).unwrap_or(0).into());
            row.push(found.map(|x| x.girls).unwrap_or(0).into());
        }
        row.push(c.unknown_age.into());
        by_age.rows.push(row);
    }

    let everyone = load(conn, None, None)?;
    let mut learners = Sheet::new("Learners", &["Class", "Name", "Sex", "Date of birth", "LIN", "Special needs", "Registration no."]).titled(&title);
    for r in &everyone {
        learners.push(vec![
            r.class_name.clone().into(),
            r.full_name.clone().into(),
            r.gender.clone().into(),
            r.date_of_birth.clone().into(),
            r.lin.clone().into(),
            r.special_needs.clone().into(),
            r.reg_number.clone().into(),
        ]);
    }
    let mut staff = Sheet::new("Staff", &["Name", "Sex", "Role", "NIN", "TMIS number", "Phone"]).titled(&title);
    for s in &check.staff {
        staff.push(vec![
            s.full_name.clone().into(),
            s.gender.clone().into(),
            office_name(s.office.as_deref()).into(),
            s.nin.clone().into(),
            s.tmis_number.clone().into(),
            s.phone.clone().into(),
        ]);
    }
    let mut fix = Sheet::new("To fix", &["Who", "Class / role", "What to fix"]).titled(&title);
    for l in &check.learners {
        for i in l.issues.iter().filter(|i| i.serious) {
            fix.push(vec![l.full_name.clone().into(), l.class_name.clone().into(), i.message.clone().into()]);
        }
    }
    for s in &check.staff {
        for i in &s.issues {
            fix.push(vec![s.full_name.clone().into(), office_name(s.office.as_deref()).into(), i.message.clone().into()]);
        }
    }
    exports::workbook(&[enrol, by_age, learners, staff, fix])
}

#[tauri::command]
pub fn export_emis(state: State<'_, AppState>) -> AppResult<StagedFile> {
    let session = state.sessions.require()?;
    session.require(Permission::ViewAllLearners)?;
    let conn = state.db.lock();
    let today = Local::now().date_naive();
    let check = emis_check_in(&conn, today)?;
    let bytes = emis_workbook(&conn, &check)?;
    let file = exports::stage(&state.paths.export_dir, &format!("EMIS census {}.xlsx", check.census_date), &bytes)?;
    audit::record(&conn, Some(&session), "emis.export", "school", "emis", "Exported the EMIS census workbook")?;
    Ok(staged(file, "xlsx", format!("{} learners, {} staff", check.learners_total, check.staff.len())))
}

// ---------------------------------------------------------------------------
// Sports (UPSSA)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SportsEvent {
    pub id: String,
    pub name: String,
    pub sport: String,
    pub term_id: Option<String>,
    pub born_on_or_after: Option<String>,
    /// `M`, `F` or `any`.
    pub sex: String,
    pub max_athletes: Option<i64>,
    pub athletes: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SportsEventInput {
    pub id: Option<String>,
    pub name: String,
    pub sport: String,
    pub born_on_or_after: Option<String>,
    pub sex: String,
    pub max_athletes: Option<i64>,
}

fn may_run_sports(session: &Session) -> AppResult<()> {
    if session.has(Permission::ManageLearners) || session.has(Permission::ManageAcademics) || session.has(Permission::ManageAllClasses) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn events(conn: &Connection, id: Option<&str>) -> AppResult<Vec<SportsEvent>> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.name, e.sport, e.term_id, e.born_on_or_after, e.sex, e.max_athletes,
                (SELECT COUNT(*) FROM sports_athletes a WHERE a.event_id = e.id AND a.status = 'active')
         FROM sports_events e WHERE e.status = 'active' AND (?1 IS NULL OR e.id = ?1)
         ORDER BY e.created_at DESC",
    )?;
    let rows = stmt
        .query_map(params![id], |r| {
            Ok(SportsEvent {
                id: r.get(0)?,
                name: r.get(1)?,
                sport: r.get(2)?,
                term_id: r.get(3)?,
                born_on_or_after: r.get(4)?,
                sex: r.get(5)?,
                max_athletes: r.get(6)?,
                athletes: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[tauri::command]
pub fn list_sports_events(state: State<'_, AppState>) -> AppResult<Vec<SportsEvent>> {
    state.sessions.require()?;
    events(&state.db.lock(), None)
}

pub fn save_sports_event_in(conn: &Connection, session: &Session, input: &SportsEventInput) -> AppResult<String> {
    may_run_sports(session)?;
    let name = input.name.trim();
    let sport = input.sport.trim();
    if name.is_empty() || sport.is_empty() {
        return Err(AppError::validation("Name the championship and the sport."));
    }
    if !["M", "F", "any"].contains(&input.sex.as_str()) {
        return Err(AppError::validation("Choose boys, girls or both."));
    }
    let cut = match blank(input.born_on_or_after.as_deref()) {
        None => None,
        Some(t) => Some(parse_date(&t).ok_or_else(|| AppError::validation("Write the cut-off as a date."))?.format("%Y-%m-%d").to_string()),
    };
    let now = Utc::now().to_rfc3339();
    let id = match &input.id {
        Some(id) => {
            conn.execute(
                "UPDATE sports_events SET name = ?1, sport = ?2, born_on_or_after = ?3, sex = ?4, max_athletes = ?5, updated_at = ?6 WHERE id = ?7",
                params![name, sport, cut, input.sex, input.max_athletes, now, id],
            )?;
            id.clone()
        }
        None => {
            let id = new_id("spt");
            conn.execute(
                "INSERT INTO sports_events (id, name, sport, term_id, born_on_or_after, sex, max_athletes, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![id, name, sport, repo::current_term_id(conn)?, cut, input.sex, input.max_athletes, now],
            )?;
            id
        }
    };
    audit::record(conn, Some(session), "sports.event", "sports_event", &id, format!("Set up {name} ({sport})"))?;
    Ok(id)
}

#[tauri::command]
pub fn save_sports_event(state: State<'_, AppState>, input: SportsEventInput) -> AppResult<String> {
    let session = state.sessions.require()?;
    save_sports_event_in(&state.db.lock(), &session, &input)
}

#[tauri::command]
pub fn retire_sports_event(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    may_run_sports(&session)?;
    let conn = state.db.lock();
    conn.execute("UPDATE sports_events SET status = 'retired', updated_at = ?1 WHERE id = ?2", params![Utc::now().to_rfc3339(), id])?;
    audit::record(&conn, Some(&session), "sports.retire", "sports_event", &id, "Closed a championship")?;
    Ok(())
}

pub fn set_athletes_in(conn: &Connection, session: &Session, event_id: &str, student_ids: &[String], discipline: Option<&str>, on_team: bool) -> AppResult<usize> {
    may_run_sports(session)?;
    let now = Utc::now().to_rfc3339();
    let status = if on_team { "active" } else { "removed" };
    let mut n = 0;
    for id in student_ids {
        n += conn.execute(
            "INSERT INTO sports_athletes (event_id, student_id, discipline, status, added_by, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (event_id, student_id) DO UPDATE SET status = excluded.status,
                 discipline = COALESCE(excluded.discipline, sports_athletes.discipline), updated_at = excluded.updated_at",
            params![event_id, id, blank(discipline), status, session.user_id, now],
        )?;
    }
    let verb = if on_team { "Added" } else { "Removed" };
    audit::record(conn, Some(session), "sports.athletes", "sports_event", event_id, format!("{verb} {n} athlete(s)"))?;
    Ok(n)
}

#[tauri::command]
pub fn set_athletes(state: State<'_, AppState>, event_id: String, student_ids: Vec<String>, discipline: Option<String>, on_team: bool) -> AppResult<usize> {
    let session = state.sessions.require()?;
    set_athletes_in(&state.db.lock(), &session, &event_id, &student_ids, discipline.as_deref(), on_team)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SportsTeam {
    pub event: SportsEvent,
    pub athletes: Vec<LearnerCheck>,
    pub ready: usize,
    pub warnings: Vec<String>,
}

pub fn team_in(conn: &Connection, event_id: &str, with_photos: bool) -> AppResult<SportsTeam> {
    let event = events(conn, Some(event_id))?.into_iter().next().ok_or_else(|| AppError::not_found("That championship"))?;
    let mut stmt = conn.prepare("SELECT student_id, discipline FROM sports_athletes WHERE event_id = ?1 AND status = 'active'")?;
    let members: HashMap<String, Option<String>> = stmt.query_map(params![event_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    let ids: Vec<String> = members.keys().cloned().collect();
    let raws = load(conn, None, Some(&ids))?;
    let rules = Rules {
        // Ages are measured on the championship's own cut-off when it has
        // one; otherwise today.
        age_on: Local::now().date_naive(),
        usual_ages: None,
        need_lin: true,
        need_photo: true,
        born_on_or_after: event.born_on_or_after.as_deref().and_then(parse_date),
        sex: (event.sex != "any").then(|| event.sex.clone()),
    };
    let mut athletes = check_all(conn, &raws, &rules, with_photos)?;
    for a in &mut athletes {
        a.discipline = members.get(&a.student_id).cloned().flatten();
    }
    let mut warnings = Vec::new();
    if ids.len() > raws.len() {
        warnings.push(format!("{} on the team no longer study here this year and were left out.", ids.len() - raws.len()));
    }
    if let Some(max) = event.max_athletes {
        if athletes.len() as i64 > max {
            warnings.push(format!("{} athletes, but the championship takes {max}.", athletes.len()));
        }
    }
    let ready = athletes.iter().filter(|a| a.ready()).count();
    Ok(SportsTeam { event, athletes, ready, warnings })
}

#[tauri::command]
pub fn sports_team(state: State<'_, AppState>, event_id: String) -> AppResult<SportsTeam> {
    let session = state.sessions.require()?;
    session.require(Permission::ViewAllLearners).or_else(|_| may_run_sports(&session))?;
    team_in(&state.db.lock(), &event_id, false)
}

#[tauri::command]
pub fn build_sports_album(state: State<'_, AppState>, event_id: String) -> AppResult<DocumentEnvelope<SportsTeam>> {
    let session = state.sessions.require()?;
    session.require(Permission::ViewAllLearners).or_else(|_| may_run_sports(&session))?;
    let conn = state.db.lock();
    let team = team_in(&conn, &event_id, true)?;
    envelope(&conn, "sports_album", team)
}

fn jpeg(png: &[u8]) -> AppResult<Vec<u8>> {
    let img = image::load_from_memory(png).map_err(|e| AppError::internal(e.to_string()))?;
    let mut out = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90);
    img.to_rgb8().write_with_encoder(encoder).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(out)
}

/// The portal sheet (in the order the athletes are typed in) and a folder of
/// their photos as JPEG files named to match.
#[tauri::command]
pub fn export_sports_team(state: State<'_, AppState>, event_id: String) -> AppResult<Vec<StagedFile>> {
    let session = state.sessions.require()?;
    may_run_sports(&session)?;
    let conn = state.db.lock();
    let team = team_in(&conn, &event_id, false)?;
    let raws = load(&conn, None, Some(&team.athletes.iter().map(|a| a.student_id.clone()).collect::<Vec<_>>()))?;
    let photos: HashMap<&str, &Vec<u8>> = raws.iter().filter_map(|r| r.photo.as_ref().map(|p| (r.id.as_str(), p))).collect();

    let title = format!("{}: {} ({})", team.event.name, team.event.sport, team.event.born_on_or_after.as_deref().map(|d| format!("born on or after {d}")).unwrap_or_default());
    let mut sheet = Sheet::new("Athletes", &["No.", "Surname", "Other names", "Sex", "Date of birth", "Age", "Class", "LIN", "Event", "Photo file", "Ready"]).titled(title);
    let mut files = Vec::new();
    for (n, a) in team.athletes.iter().enumerate() {
        let mut parts = a.full_name.split_whitespace();
        let surname = parts.next().unwrap_or_default().to_string();
        let others = parts.collect::<Vec<_>>().join(" ");
        let photo_file = format!("{:02} {}.jpg", n + 1, a.full_name.to_uppercase());
        let photo_name = match photos.get(a.student_id.as_str()) {
            Some(png) => {
                files.push((photo_file.clone(), jpeg(png)?));
                exports::safe_name(&photo_file)
            }
            None => String::new(),
        };
        sheet.push(vec![
            ((n + 1) as i64).into(),
            surname.into(),
            others.into(),
            a.gender.clone().into(),
            a.date_of_birth.clone().into(),
            a.age.map(Cell::from).unwrap_or(Cell::Empty),
            a.class_name.clone().into(),
            a.lin.clone().into(),
            a.discipline.clone().into(),
            photo_name.into(),
            (if a.ready() { "Yes".to_string() } else { a.issues.iter().filter(|i| i.serious).map(|i| i.message.clone()).collect::<Vec<_>>().join(" ") }).into(),
        ]);
    }
    let base = format!("{} {}", team.event.name, team.event.sport);
    let book = exports::stage(&state.paths.export_dir, &format!("{base}.xlsx"), &exports::workbook(&[sheet])?)?;
    let mut out = vec![staged(book, "xlsx", format!("{} athletes", team.athletes.len()))];
    if !files.is_empty() {
        let folder = exports::stage_folder(&state.paths.export_dir, &format!("{base} photos"), &files)?;
        out.push(staged(folder, "folder", format!("{} photos", files.len())));
    }
    audit::record(&conn, Some(&session), "sports.export", "sports_event", &event_id, format!("Exported {} for the sports portal", team.event.name))?;
    Ok(out)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Correction {
    pub student_id: String,
    pub what: String,
    pub wrong: String,
    pub right: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LetterInput {
    pub event_id: String,
    pub addressee: String,
    pub corrections: Vec<Correction>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionLine {
    pub full_name: String,
    pub lin: Option<String>,
    pub class_name: String,
    pub what: String,
    pub wrong: String,
    pub right: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionLetter {
    pub date: String,
    pub addressee: Vec<String>,
    pub subject: String,
    pub event_name: String,
    pub sport: String,
    pub lines: Vec<CorrectionLine>,
    pub note: Option<String>,
    pub head_teacher_name: String,
}

/// The letter a correction needs: on the school's headed paper, signed by
/// the headteacher and stamped.
#[tauri::command]
pub fn build_correction_letter(state: State<'_, AppState>, input: LetterInput) -> AppResult<DocumentEnvelope<CorrectionLetter>> {
    let session = state.sessions.require()?;
    may_run_sports(&session)?;
    let conn = state.db.lock();
    let team = team_in(&conn, &input.event_id, false)?;
    if input.corrections.is_empty() {
        return Err(AppError::validation("Add at least one correction."));
    }
    let mut lines = Vec::new();
    for c in &input.corrections {
        let a = team.athletes.iter().find(|a| a.student_id == c.student_id).ok_or_else(|| AppError::validation("Every correction must be for someone on the team."))?;
        if c.right.trim().is_empty() {
            return Err(AppError::validation(format!("Write the correct {} for {}.", c.what.to_lowercase(), a.full_name)));
        }
        lines.push(CorrectionLine {
            full_name: a.full_name.clone(),
            lin: a.lin.clone(),
            class_name: a.class_name.clone(),
            what: c.what.trim().into(),
            wrong: c.wrong.trim().into(),
            right: c.right.trim().into(),
        });
    }
    let addressee: Vec<String> = input.addressee.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    let letter = CorrectionLetter {
        date: Local::now().format("%-d %B %Y").to_string(),
        addressee: if addressee.is_empty() { vec!["The General Secretary".into(), "Uganda Primary Schools Sports Association".into()] } else { addressee },
        subject: format!("Correction of athletes' details: {} ({})", team.event.name, team.event.sport),
        event_name: team.event.name.clone(),
        sport: team.event.sport.clone(),
        lines,
        note: blank(input.note.as_deref()),
        head_teacher_name: repo::get_setting(&conn, "report.head_teacher_name")?.unwrap_or_default(),
    };
    audit::record(&conn, Some(&session), "sports.letter", "sports_event", &input.event_id, format!("Printed a correction letter for {} athlete(s)", letter.lines.len()))?;
    envelope(&conn, "correction_letter", letter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fees::tests::fee_school;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    fn tiny_png(side: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(side, side, image::Rgb([200, 180, 160]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn the_checker_finds_what_an_outside_system_would_reject() {
        let db = fee_school();
        let conn = db.lock();
        conn.execute("UPDATE students SET full_name = 'Amina Nakato', gender = 'F', date_of_birth = '2014-03-02', lin = 'ABC123', photo_png = ?1 WHERE id = 'amina'", params![tiny_png(300)]).unwrap();
        // Musa: an initial, no sex, a LIN with a dash, a small photo.
        conn.execute("UPDATE students SET date_of_birth = '02/03/2021', lin = 'AB-12', photo_png = ?1 WHERE id = 'musa'", params![tiny_png(120)]).unwrap();

        let check = ple_check_in(&conn, Some("p5".into()), false).unwrap();
        assert_eq!(check.candidates.len(), 2, "the dropped learner is not a candidate");
        let amina = check.candidates.iter().find(|c| c.student_id == "amina").unwrap();
        assert!(amina.ready(), "{:?}", amina.issues);
        let musa = check.candidates.iter().find(|c| c.student_id == "musa").unwrap();
        let fields: Vec<(&str, bool)> = musa.issues.iter().map(|i| (i.field.as_str(), i.serious)).collect();
        assert!(fields.contains(&("name", true)), "Musa K: an initial");
        assert!(fields.contains(&("gender", true)));
        assert!(fields.contains(&("lin", true)));
        assert!(fields.contains(&("photo", false)), "small photo is a warning");
        assert!(fields.contains(&("age", false)), "5 at PLE: check the year");
        assert_eq!(check.ready, 1);

        // Fixed where it was found; a LIN cannot be shared.
        let sec = test_session("bursar", Role::Teacher, Some(Office::Secretary), &[]);
        let taken = fix_learner_in(&conn, &sec, "musa", &LearnerFix { full_name: "Musa  Kato".into(), gender: Some("M".into()), date_of_birth: Some("02/03/2013".into()), lin: Some("abc123".into()), special_needs: None });
        assert!(matches!(taken, Err(AppError::Conflict(_))));
        fix_learner_in(&conn, &sec, "musa", &LearnerFix { full_name: "Musa  Kato".into(), gender: Some("M".into()), date_of_birth: Some("02/03/2013".into()), lin: Some("xyz789".into()), special_needs: None }).unwrap();
        let (name, dob, lin): (String, String, String) = conn.query_row("SELECT full_name, date_of_birth, lin FROM students WHERE id = 'musa'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!((name.as_str(), dob.as_str(), lin.as_str()), ("Musa Kato", "2013-03-02", "XYZ789"));
        let bursar = test_session("bursar", Role::Teacher, Some(Office::Bursar), &[]);
        assert!(matches!(fix_learner_in(&conn, &bursar, "musa", &LearnerFix { full_name: "X Y".into(), gender: None, date_of_birth: None, lin: None, special_needs: None }), Err(AppError::Forbidden)));
    }

    #[test]
    fn the_census_counts_new_and_repeating_learners_by_sex_and_age() {
        let db = fee_school();
        let conn = db.lock();
        conn.execute_batch(
            "UPDATE students SET gender = 'F', date_of_birth = '2015-01-10' WHERE id = 'amina';
             UPDATE students SET gender = 'M', date_of_birth = '2014-06-01', special_needs = 'Low vision' WHERE id = 'musa';
             INSERT INTO enrollments (id, student_id, class_id, academic_year_id, status, joined_at, created_at, updated_at)
                 VALUES ('e9', 'musa', 'p5', 'y25', 'active', 'now', 'now', 'now');",
        )
        .unwrap();
        let check = emis_check_in(&conn, NaiveDate::from_ymd_opt(2026, 10, 10).unwrap()).unwrap();
        let p5 = check.classes.iter().find(|c| c.class_name == "P5").unwrap();
        assert_eq!((p5.boys, p5.girls), (1, 1));
        assert_eq!((p5.repeat_boys, p5.new_girls, p5.new_boys), (1, 0, 0), "Musa repeats P5; Amina came up from P4");
        assert_eq!(p5.special_needs, 1);
        assert_eq!(check.ages, vec![11, 12]);
        assert!(check.learners.iter().all(|l| l.issues.iter().any(|i| i.field == "lin")), "neither has a LIN");
        let head = check.staff.iter().find(|s| s.user_id == "head").unwrap();
        assert!(head.issues.iter().any(|i| i.field == "nin"));

        let boss = test_session("head", Role::Teacher, Some(Office::HeadTeacher), &[]);
        assert!(save_staff_ids_in(&conn, &boss, "bursar", &StaffIds { gender: Some("F".into()), nin: Some("CF12".into()), tmis_number: None }).is_err());
        save_staff_ids_in(&conn, &boss, "bursar", &StaffIds { gender: Some("F".into()), nin: Some("cm 1234567890ab".into()), tmis_number: Some("T-55".into()) }).unwrap();
        let again = emis_check_in(&conn, NaiveDate::from_ymd_opt(2026, 10, 10).unwrap()).unwrap();
        let bursar = again.staff.iter().find(|s| s.user_id == "bursar").unwrap();
        assert_eq!(bursar.nin.as_deref(), Some("CM1234567890AB"));
        assert_eq!(bursar.issues.len(), 1, "a CM number for a woman is worth a second look: {:?}", bursar.issues);
        assert!(!bursar.issues[0].serious);

        let bytes = emis_workbook(&conn, &again).unwrap();
        assert!(bytes.len() > 1000);
    }

    #[test]
    fn a_team_is_checked_against_the_championships_cut_off_and_sex() {
        let db = fee_school();
        let conn = db.lock();
        conn.execute_batch(
            "UPDATE students SET full_name = 'Amina Nakato', gender = 'F', date_of_birth = '2014-03-02', lin = 'L1' WHERE id = 'amina';
             UPDATE students SET full_name = 'Musa Kato', gender = 'M', date_of_birth = '2013-05-01', lin = 'L2' WHERE id = 'musa';",
        )
        .unwrap();
        conn.execute("UPDATE students SET photo_png = ?1", params![tiny_png(240)]).unwrap();
        let head = test_session("head", Role::Teacher, Some(Office::HeadTeacher), &[]);
        let event = save_sports_event_in(&conn, &head, &SportsEventInput { id: None, name: "Ball games".into(), sport: "Netball".into(), born_on_or_after: Some("01/01/2014".into()), sex: "F".into(), max_athletes: Some(1) }).unwrap();
        set_athletes_in(&conn, &head, &event, &["amina".into(), "musa".into()], Some("Goal shooter"), true).unwrap();
        let team = team_in(&conn, &event, false).unwrap();
        assert_eq!(team.ready, 1);
        let musa = team.athletes.iter().find(|a| a.student_id == "musa").unwrap();
        assert_eq!(musa.issues.iter().filter(|i| i.field == "team").count(), 2, "a boy, and born before the cut-off");
        assert_eq!(musa.discipline.as_deref(), Some("Goal shooter"));
        assert_eq!(team.warnings.len(), 1, "two athletes where the championship takes one");

        set_athletes_in(&conn, &head, &event, &["musa".into()], None, false).unwrap();
        let team = team_in(&conn, &event, false).unwrap();
        assert_eq!(team.athletes.len(), 1);
        assert_eq!(team.event.athletes, 1);
        assert!(jpeg(&tiny_png(50)).unwrap().starts_with(&[0xFF, 0xD8]));

        let teacher = test_session("bursar", Role::Teacher, None, &["p5"]);
        assert!(matches!(set_athletes_in(&conn, &teacher, &event, &["musa".into()], None, true), Err(AppError::Forbidden)));
    }
}
