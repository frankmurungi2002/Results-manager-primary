//! The visitors' book at the gate (FR-G2) and the conduct log (FR-G14).
//!
//! Visitors: signed in with a numbered slip they carry while on the
//! compound, and signed out when they leave, so the office can always say
//! who is inside. Conduct: commendations as well as incidents, the action
//! taken, and whether the parent was told; an entry is withdrawn with a
//! reason, never deleted. Corporal punishment is banned in Ugandan schools,
//! and the actions offered here follow that.

use chrono::{DateTime, Local, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::commands::reports::{envelope, DocumentEnvelope};
use crate::domain::ids::new_id;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::{Permission, Session};
use crate::sms;
use crate::state::AppState;

fn clean(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

// ---------------------------------------------------------------------------
// Visitors
// ---------------------------------------------------------------------------

fn may_keep_gate(session: &Session) -> AppResult<()> {
    if session.has(Permission::IssuePassOuts) || session.has(Permission::ManageLearners) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Visitor {
    pub id: String,
    pub slip: String,
    pub full_name: String,
    pub phone: Option<String>,
    pub id_number: Option<String>,
    pub organisation: Option<String>,
    pub purpose: String,
    pub to_see: Option<String>,
    pub student_id: Option<String>,
    pub student_name: Option<String>,
    pub class_name: Option<String>,
    pub vehicle: Option<String>,
    pub note: Option<String>,
    pub time_in: String,
    pub time_out: Option<String>,
    pub signed_in_by: Option<String>,
}

const VISITOR_SELECT: &str = "SELECT v.id, v.slip_no, v.full_name, v.phone, v.id_number, v.organisation, v.purpose, v.to_see,
        v.student_id, s.full_name,
        (SELECT c.name FROM enrollments e JOIN classes c ON c.id = e.class_id WHERE e.student_id = v.student_id AND e.status = 'active' LIMIT 1),
        v.vehicle, v.note, v.time_in, v.time_out, u.full_name
    FROM visitors v LEFT JOIN students s ON s.id = v.student_id LEFT JOIN users u ON u.id = v.signed_in_by";

fn visitor(r: &rusqlite::Row<'_>) -> rusqlite::Result<Visitor> {
    let n: i64 = r.get(1)?;
    Ok(Visitor {
        id: r.get(0)?,
        slip: format!("VS-{n:05}"),
        full_name: r.get(2)?,
        phone: r.get(3)?,
        id_number: r.get(4)?,
        organisation: r.get(5)?,
        purpose: r.get(6)?,
        to_see: r.get(7)?,
        student_id: r.get(8)?,
        student_name: r.get(9)?,
        class_name: r.get(10)?,
        vehicle: r.get(11)?,
        note: r.get(12)?,
        time_in: r.get(13)?,
        time_out: r.get(14)?,
        signed_in_by: r.get(15)?,
    })
}

fn load_visitor(conn: &Connection, id: &str) -> AppResult<Visitor> {
    conn.query_row(&format!("{VISITOR_SELECT} WHERE v.id = ?1"), params![id], visitor)
        .optional()?
        .ok_or_else(|| AppError::not_found("That visitor"))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisitorInput {
    pub full_name: String,
    pub phone: Option<String>,
    pub id_number: Option<String>,
    pub organisation: Option<String>,
    pub purpose: String,
    pub to_see: Option<String>,
    pub student_id: Option<String>,
    pub vehicle: Option<String>,
    pub note: Option<String>,
}

pub fn sign_in_visitor_in(conn: &Connection, session: &Session, input: &VisitorInput, now: DateTime<Utc>) -> AppResult<Visitor> {
    may_keep_gate(session)?;
    let name = input.full_name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.len() < 3 {
        return Err(AppError::validation("Write the visitor's name."));
    }
    if !["parent", "official", "delivery", "meeting", "job", "other"].contains(&input.purpose.as_str()) {
        return Err(AppError::validation("Choose why they have come."));
    }
    if input.purpose == "parent" && input.student_id.is_none() {
        return Err(AppError::validation("Choose the learner the parent has come to see."));
    }
    let n: i64 = conn.query_row("SELECT COALESCE(MAX(slip_no), 0) + 1 FROM visitors", [], |r| r.get(0))?;
    let id = new_id("vis");
    conn.execute(
        "INSERT INTO visitors (id, slip_no, full_name, phone, id_number, organisation, purpose, to_see, student_id, vehicle, note,
                               time_in, signed_in_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?12)",
        params![
            id,
            n,
            name,
            clean(input.phone.as_deref()),
            clean(input.id_number.as_deref()),
            clean(input.organisation.as_deref()),
            input.purpose,
            clean(input.to_see.as_deref()),
            input.student_id,
            clean(input.vehicle.as_deref()).map(|v| v.to_uppercase()),
            clean(input.note.as_deref()),
            now.to_rfc3339(),
            session.user_id,
        ],
    )?;
    audit::record(conn, Some(session), "visitor.in", "visitor", &id, format!("Signed in visitor {name} (VS-{n:05})"))?;
    load_visitor(conn, &id)
}

#[tauri::command]
pub fn sign_in_visitor(state: State<'_, AppState>, input: VisitorInput) -> AppResult<DocumentEnvelope<Visitor>> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let v = sign_in_visitor_in(&conn, &session, &input, Utc::now())?;
    envelope(&conn, "visitor_slip", v)
}

pub fn sign_out_visitor_in(conn: &Connection, session: &Session, id: &str, now: DateTime<Utc>) -> AppResult<Visitor> {
    may_keep_gate(session)?;
    let changed = conn.execute(
        "UPDATE visitors SET time_out = ?1, signed_out_by = ?2 WHERE id = ?3 AND time_out IS NULL",
        params![now.to_rfc3339(), session.user_id, id],
    )?;
    if changed == 0 {
        return Err(AppError::conflict("That visitor has already left."));
    }
    let v = load_visitor(conn, id)?;
    audit::record(conn, Some(session), "visitor.out", "visitor", id, format!("Signed out visitor {} ({})", v.full_name, v.slip))?;
    Ok(v)
}

#[tauri::command]
pub fn sign_out_visitor(state: State<'_, AppState>, id: String) -> AppResult<Visitor> {
    let session = state.sessions.require()?;
    sign_out_visitor_in(&state.db.lock(), &session, &id, Utc::now())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisitorBook {
    pub from: String,
    pub to: String,
    /// Everyone still on the compound, whatever day they came.
    pub inside: Vec<Visitor>,
    pub visitors: Vec<Visitor>,
}

fn local_day_bounds(day: NaiveDate) -> String {
    day.format("%Y-%m-%d").to_string()
}

pub fn book_in(conn: &Connection, from: NaiveDate, to: NaiveDate) -> AppResult<VisitorBook> {
    let mut stmt = conn.prepare(&format!("{VISITOR_SELECT} WHERE v.time_out IS NULL ORDER BY v.time_in"))?;
    let inside = stmt.query_map([], visitor)?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut stmt = conn.prepare(&format!(
        "{VISITOR_SELECT} WHERE date(v.time_in, 'localtime') BETWEEN ?1 AND ?2 ORDER BY v.time_in DESC"
    ))?;
    let visitors = stmt.query_map(params![local_day_bounds(from), local_day_bounds(to)], visitor)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(VisitorBook { from: local_day_bounds(from), to: local_day_bounds(to), inside, visitors })
}

fn parse(d: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| AppError::validation("Choose the dates."))
}

#[tauri::command]
pub fn visitor_book(state: State<'_, AppState>, from: String, to: String) -> AppResult<VisitorBook> {
    let session = state.sessions.require()?;
    if !(session.has(Permission::IssuePassOuts) || session.has(Permission::ManageLearners) || session.has(Permission::ViewAudit)) {
        return Err(AppError::Forbidden);
    }
    book_in(&state.db.lock(), parse(&from)?, parse(&to)?)
}

#[tauri::command]
pub fn build_visitor_book(state: State<'_, AppState>, from: String, to: String) -> AppResult<DocumentEnvelope<VisitorBook>> {
    let session = state.sessions.require()?;
    if !(session.has(Permission::IssuePassOuts) || session.has(Permission::ManageLearners) || session.has(Permission::ViewAudit)) {
        return Err(AppError::Forbidden);
    }
    let conn = state.db.lock();
    let book = book_in(&conn, parse(&from)?, parse(&to)?)?;
    envelope(&conn, "visitor_book", book)
}

#[tauri::command]
pub fn build_visitor_slip(state: State<'_, AppState>, id: String) -> AppResult<DocumentEnvelope<Visitor>> {
    let session = state.sessions.require()?;
    may_keep_gate(&session)?;
    let conn = state.db.lock();
    let v = load_visitor(&conn, &id)?;
    envelope(&conn, "visitor_slip", v)
}

// ---------------------------------------------------------------------------
// Conduct
// ---------------------------------------------------------------------------

pub const INCIDENTS: &[&str] = &[
    "Late coming",
    "Absent without permission",
    "Fighting",
    "Bullying",
    "Disrespect",
    "Dishonesty or theft",
    "Damage to property",
    "Uniform or grooming",
    "Bad language",
    "Escaping from school",
    "Other",
];

pub const COMMENDATIONS: &[&str] = &["Good conduct", "Helping others", "Leadership", "Cleanliness", "Honesty", "Improvement", "Other"];

/// Corrective, not corporal: the Ministry bans corporal punishment.
pub const ACTIONS: &[&str] = &[
    "Talked to and warned",
    "Counselling",
    "Apology",
    "Manual work",
    "Parent called to school",
    "Referred to the headteacher",
    "Suspension",
    "Praised in assembly",
    "Certificate or reward",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConductEntry {
    pub id: String,
    pub student_id: String,
    pub student_name: String,
    pub class_id: String,
    pub class_name: String,
    pub on_date: String,
    pub kind: String,
    pub category: String,
    pub description: String,
    pub action_taken: Option<String>,
    pub parent_informed: bool,
    pub status: String,
    pub withdrawn_reason: Option<String>,
    pub recorded_by: Option<String>,
}

const CONDUCT_SELECT: &str = "SELECT k.id, k.student_id, s.full_name, k.class_id, c.name, k.on_date, k.kind, k.category, k.description,
        k.action_taken, k.parent_informed, k.status, k.withdrawn_reason, u.full_name
    FROM conduct_entries k JOIN students s ON s.id = k.student_id JOIN classes c ON c.id = k.class_id
    LEFT JOIN users u ON u.id = k.recorded_by";

fn conduct(r: &rusqlite::Row<'_>) -> rusqlite::Result<ConductEntry> {
    Ok(ConductEntry {
        id: r.get(0)?,
        student_id: r.get(1)?,
        student_name: r.get(2)?,
        class_id: r.get(3)?,
        class_name: r.get(4)?,
        on_date: r.get(5)?,
        kind: r.get(6)?,
        category: r.get(7)?,
        description: r.get(8)?,
        action_taken: r.get(9)?,
        parent_informed: r.get::<_, i64>(10)? != 0,
        status: r.get(11)?,
        withdrawn_reason: r.get(12)?,
        recorded_by: r.get(13)?,
    })
}

fn may_handle_conduct(session: &Session, class_id: &str) -> AppResult<()> {
    if session.may_manage_class(class_id) || session.has(Permission::ManageAllClasses) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn current_class(conn: &Connection, student_id: &str) -> AppResult<(String, String, Option<String>)> {
    let year = repo::current_academic_year_id(conn)?.ok_or_else(|| AppError::validation("No academic year is active yet."))?;
    conn.query_row(
        "SELECT e.class_id, s.full_name, s.guardian_phone FROM enrollments e JOIN students s ON s.id = e.student_id
         WHERE e.student_id = ?1 AND e.academic_year_id = ?2 AND e.status = 'active'",
        params![student_id, year],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()?
    .ok_or_else(|| AppError::validation("That learner is not in a class this year."))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConductInput {
    pub student_id: String,
    pub on_date: Option<String>,
    pub kind: String,
    pub category: String,
    pub description: String,
    pub action_taken: Option<String>,
    pub parent_informed: bool,
    /// Queue an SMS telling the parent (only when the school is online).
    pub text_parent: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConductSaved {
    pub entry: ConductEntry,
    pub texted: bool,
}

pub fn record_conduct_in(conn: &Connection, session: &Session, input: &ConductInput) -> AppResult<ConductSaved> {
    let (class_id, name, phone) = current_class(conn, &input.student_id)?;
    may_handle_conduct(session, &class_id)?;
    let list = match input.kind.as_str() {
        "incident" => INCIDENTS,
        "commendation" => COMMENDATIONS,
        _ => return Err(AppError::validation("Choose a commendation or an incident.")),
    };
    if !list.contains(&input.category.as_str()) {
        return Err(AppError::validation("Choose what it was about."));
    }
    let description = input.description.trim();
    if description.len() < 5 {
        return Err(AppError::validation("Say what happened, in a sentence."));
    }
    let action = clean(input.action_taken.as_deref());
    if let Some(a) = &action {
        if !ACTIONS.contains(&a.as_str()) {
            return Err(AppError::validation("Choose the action taken from the list."));
        }
    }
    let today = Local::now().date_naive();
    let on_date = match clean(input.on_date.as_deref()) {
        None => today,
        Some(t) => parse(&t)?,
    };
    if on_date > today {
        return Err(AppError::validation("Record it on the day, or after."));
    }
    let id = new_id("cnd");
    let now = Utc::now().to_rfc3339();
    let mut texted = false;
    if input.text_parent {
        let school: String = conn.query_row("SELECT name FROM institution LIMIT 1", [], |r| r.get(0)).optional()?.unwrap_or_default();
        let first = name.split_whitespace().last().unwrap_or(&name);
        let body = if input.kind == "commendation" {
            format!("{school}: good news. {first} was commended today for {}: {description}", input.category.to_lowercase())
        } else {
            format!("{school}: please contact the school about {first} ({}). {description}", input.category.to_lowercase())
        };
        texted = sms::queue(conn, phone.as_deref(), &body, "conduct", &id, Some(&session.user_id))?.is_some();
    }
    conn.execute(
        "INSERT INTO conduct_entries (id, student_id, class_id, on_date, kind, category, description, action_taken, parent_informed,
                                      recorded_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            id,
            input.student_id,
            class_id,
            on_date.format("%Y-%m-%d").to_string(),
            input.kind,
            input.category,
            description,
            action,
            (input.parent_informed || texted) as i64,
            session.user_id,
            now
        ],
    )?;
    audit::record(conn, Some(session), "conduct.record", "student", &input.student_id, format!("Recorded {} for {name}: {}", if input.kind == "incident" { "an incident" } else { "a commendation" }, input.category))?;
    if texted {
        sms::wake();
    }
    let entry = conn.query_row(&format!("{CONDUCT_SELECT} WHERE k.id = ?1"), params![id], conduct)?;
    Ok(ConductSaved { entry, texted })
}

#[tauri::command]
pub fn record_conduct(state: State<'_, AppState>, input: ConductInput) -> AppResult<ConductSaved> {
    let session = state.sessions.require()?;
    record_conduct_in(&state.db.lock(), &session, &input)
}

#[tauri::command]
pub fn withdraw_conduct(state: State<'_, AppState>, id: String, reason: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let entry = conn.query_row(&format!("{CONDUCT_SELECT} WHERE k.id = ?1"), params![id], conduct).optional()?.ok_or_else(|| AppError::not_found("That entry"))?;
    may_handle_conduct(&session, &entry.class_id)?;
    if reason.trim().len() < 5 {
        return Err(AppError::validation("Say why it is withdrawn."));
    }
    conn.execute(
        "UPDATE conduct_entries SET status = 'withdrawn', withdrawn_reason = ?1 WHERE id = ?2 AND status = 'active'",
        params![reason.trim(), id],
    )?;
    audit::record(&conn, Some(&session), "conduct.withdraw", "student", &entry.student_id, format!("Withdrew a conduct entry for {}: {}", entry.student_name, reason.trim()))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConductLog {
    pub entries: Vec<ConductEntry>,
    pub incidents: Vec<String>,
    pub commendations: Vec<String>,
    pub actions: Vec<String>,
}

/// One learner's record, one class, or (for the school-wide offices) everyone.
pub fn log_in(conn: &Connection, session: &Session, class_id: Option<&str>, student_id: Option<&str>) -> AppResult<ConductLog> {
    if let Some(c) = class_id {
        may_handle_conduct(session, c)?;
    } else if let Some(s) = student_id {
        may_handle_conduct(session, &current_class(conn, s)?.0)?;
    } else if !session.has(Permission::ManageAllClasses) {
        return Err(AppError::Forbidden);
    }
    let mut stmt = conn.prepare(&format!(
        "{CONDUCT_SELECT} WHERE (?1 IS NULL OR k.class_id = ?1) AND (?2 IS NULL OR k.student_id = ?2)
         ORDER BY k.on_date DESC, k.created_at DESC LIMIT 500"
    ))?;
    let entries = stmt.query_map(params![class_id, student_id], conduct)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ConductLog {
        entries,
        incidents: INCIDENTS.iter().map(|s| s.to_string()).collect(),
        commendations: COMMENDATIONS.iter().map(|s| s.to_string()).collect(),
        actions: ACTIONS.iter().map(|s| s.to_string()).collect(),
    })
}

#[tauri::command]
pub fn conduct_log(state: State<'_, AppState>, class_id: Option<String>, student_id: Option<String>) -> AppResult<ConductLog> {
    let session = state.sessions.require()?;
    log_in(&state.db.lock(), &session, class_id.as_deref(), student_id.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fees::tests::fee_school;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    #[test]
    fn visitors_are_numbered_and_stay_inside_until_signed_out() {
        let db = fee_school();
        let conn = db.lock();
        let sec = test_session("bursar", Role::Teacher, Some(Office::Secretary), &[]);
        let now = Utc::now();
        let input = |purpose: &str, student: Option<&str>| VisitorInput {
            full_name: "  Okello   Peter ".into(),
            phone: Some("0772 111222".into()),
            id_number: None,
            organisation: None,
            purpose: purpose.into(),
            to_see: Some("Headteacher".into()),
            student_id: student.map(String::from),
            vehicle: Some("uab 123x".into()),
            note: None,
        };
        assert!(sign_in_visitor_in(&conn, &sec, &input("parent", None), now).is_err(), "a parent names the child");
        let a = sign_in_visitor_in(&conn, &sec, &input("parent", Some("amina")), now).unwrap();
        let b = sign_in_visitor_in(&conn, &sec, &input("delivery", None), now).unwrap();
        assert_eq!((a.slip.as_str(), b.slip.as_str()), ("VS-00001", "VS-00002"));
        assert_eq!(a.full_name, "Okello Peter");
        assert_eq!(a.vehicle.as_deref(), Some("UAB 123X"));
        assert_eq!(a.class_name.as_deref(), Some("P5"));
        let today = Local::now().date_naive();
        assert_eq!(book_in(&conn, today, today).unwrap().inside.len(), 2);
        sign_out_visitor_in(&conn, &sec, &a.id, now).unwrap();
        assert!(sign_out_visitor_in(&conn, &sec, &a.id, now).is_err(), "already left");
        let book = book_in(&conn, today, today).unwrap();
        assert_eq!(book.inside.len(), 1);
        assert_eq!(book.visitors.len(), 2);
        let bursar = test_session("bursar", Role::Teacher, Some(Office::Bursar), &[]);
        assert!(matches!(sign_in_visitor_in(&conn, &bursar, &input("other", None), now), Err(AppError::Forbidden)));
    }

    #[test]
    fn conduct_is_kept_by_the_class_teacher_and_withdrawn_not_deleted() {
        let db = fee_school();
        let conn = db.lock();
        let class_teacher = test_session("bursar", Role::Teacher, None, &["p5"]);
        let entry = |kind: &str, category: &str, action: Option<&str>| ConductInput {
            student_id: "musa".into(),
            on_date: None,
            kind: kind.into(),
            category: category.into(),
            description: "Fought with a classmate at break".into(),
            action_taken: action.map(String::from),
            parent_informed: false,
            text_parent: true,
        };
        assert!(record_conduct_in(&conn, &class_teacher, &entry("incident", "Fighting", Some("Caning"))).is_err(), "not an allowed action");
        let saved = record_conduct_in(&conn, &class_teacher, &entry("incident", "Fighting", Some("Counselling"))).unwrap();
        assert!(saved.texted && saved.entry.parent_informed, "the parent was texted");
        record_conduct_in(&conn, &class_teacher, &entry("commendation", "Improvement", Some("Praised in assembly"))).unwrap();

        let subject_teacher = {
            let mut s = test_session("bursar", Role::Teacher, None, &["p5"]);
            s.class_teacher_of.clear();
            s
        };
        assert!(matches!(record_conduct_in(&conn, &subject_teacher, &entry("incident", "Fighting", None)), Err(AppError::Forbidden)));
        assert!(matches!(log_in(&conn, &subject_teacher, Some("p5"), None), Err(AppError::Forbidden)), "not for every subject teacher");
        let bursar = test_session("bursar", Role::Teacher, Some(Office::Bursar), &[]);
        assert!(log_in(&conn, &bursar, None, None).is_err(), "not for the bursar");

        let head = test_session("head", Role::Teacher, Some(Office::HeadTeacher), &[]);
        let log = log_in(&conn, &head, None, Some("musa")).unwrap();
        assert_eq!(log.entries.len(), 2);
        assert!(conn.execute("DELETE FROM conduct_entries", []).is_err());
    }
}
