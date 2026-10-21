//! Marks deadlines (FR-C6), and the texts that chase them.
//!
//! The head or Director of Studies sets one date by which an examination's
//! marks must be in, for the whole school (a single class can still have its
//! own date). Teachers see it on Marks entry. When "Marks reminders" is
//! switched on, each teacher with marks still missing gets one text a day or
//! two before the date and one on the morning itself, naming exactly what is
//! left: "P5 Mathematics (12 of 40)". A teacher who has finished gets nothing.
//!
//! The texts go to the teacher's own phone on their staff record, through
//! the ordinary outbox, so offline only and every other SMS rule apply.

use std::collections::BTreeMap;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::audit;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::{Permission, Session};
use crate::sms;
use crate::state::AppState;

fn exam_key(exam_id: &str) -> String {
    format!("deadline.exam.{exam_id}")
}

/// A stored deadline, whatever shape it was saved in: a date, a local
/// date-time, or RFC 3339 (converted to the school's local date).
pub fn parse_deadline(text: &str) -> Option<NaiveDate> {
    let t = text.trim();
    if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        return Some(d);
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(t) {
        return Some(dt.with_timezone(&Local).date_naive());
    }
    NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M").ok().map(|dt| dt.date())
}

/// The date marks are due for one class: its own date if it has one,
/// otherwise the examination's.
pub fn deadline_for(conn: &Connection, class_id: &str, exam_id: &str) -> AppResult<Option<NaiveDate>> {
    if let Some(own) = repo::get_setting(conn, &format!("deadline.{class_id}.{exam_id}"))?.as_deref().and_then(parse_deadline) {
        return Ok(Some(own));
    }
    Ok(repo::get_setting(conn, &exam_key(exam_id))?.as_deref().and_then(parse_deadline))
}

pub fn set_exam_deadline_in(conn: &Connection, session: &Session, exam_id: &str, date: Option<&str>) -> AppResult<()> {
    session.require(Permission::ManageAcademics)?;
    let name: String = conn
        .query_row("SELECT name FROM exams WHERE id = ?1", params![exam_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That examination"))?;
    match date.map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => {
            let parsed = NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| AppError::validation("Choose the date."))?;
            repo::set_setting(conn, &exam_key(exam_id), &parsed.format("%Y-%m-%d").to_string())?;
            audit::record(conn, Some(session), "marks.deadline", "exam", exam_id, format!("Set the {name} marks deadline to {}", long_date(parsed)))?;
        }
        None => {
            conn.execute("DELETE FROM settings WHERE key = ?1", params![exam_key(exam_id)])?;
            audit::record(conn, Some(session), "marks.deadline", "exam", exam_id, format!("Removed the {name} marks deadline"))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn set_exam_deadline(state: State<'_, AppState>, exam_id: String, date: Option<String>) -> AppResult<()> {
    let session = state.sessions.require()?;
    set_exam_deadline_in(&state.db.lock(), &session, &exam_id, date.as_deref())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamDeadline {
    /// YYYY-MM-DD, or none.
    pub date: Option<String>,
    /// Days from today: negative once it has passed.
    pub days_left: Option<i64>,
    /// "Thursday 23 October".
    pub label: Option<String>,
}

/// The deadline a teacher sees on Marks entry, for one class.
#[tauri::command]
pub fn get_exam_deadline(state: State<'_, AppState>, class_id: String, exam_id: String) -> AppResult<ExamDeadline> {
    state.sessions.require()?;
    let conn = state.db.lock();
    let date = deadline_for(&conn, &class_id, &exam_id)?;
    let today = Local::now().date_naive();
    Ok(ExamDeadline {
        date: date.map(|d| d.format("%Y-%m-%d").to_string()),
        days_left: date.map(|d| (d - today).num_days()),
        label: date.map(long_date),
    })
}

pub fn long_date(d: NaiveDate) -> String {
    d.format("%A %-d %B").to_string()
}

// ---------------------------------------------------------------------------
// The reminders
// ---------------------------------------------------------------------------

struct Missing {
    teacher_id: String,
    item: String,
}

/// What is still missing in one class for one examination, by teacher. A
/// subject with no subject teacher falls to the Class Teacher.
fn missing_in(conn: &Connection, class_id: &str, class_name: &str, exam_id: &str, year_id: &str) -> AppResult<Vec<Missing>> {
    let expected: i64 = conn.query_row(
        "SELECT COUNT(*) FROM enrollments WHERE class_id = ?1 AND academic_year_id = ?2 AND status = 'active'",
        params![class_id, year_id],
        |r| r.get(0),
    )?;
    if expected == 0 {
        return Ok(Vec::new());
    }
    let class_teacher: Option<String> = conn
        .query_row(
            "SELECT user_id FROM teacher_assignments
             WHERE class_id = ?1 AND role = 'class_teacher' AND subject_id IS NULL AND status = 'active' LIMIT 1",
            params![class_id],
            |r| r.get(0),
        )
        .optional()?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(cs.display_name, s.name),
                (SELECT ta.user_id FROM teacher_assignments ta
                  WHERE ta.class_id = cs.class_id AND ta.subject_id = cs.subject_id
                    AND ta.role = 'subject_teacher' AND ta.status = 'active' LIMIT 1),
                (SELECT COUNT(*) FROM marks m JOIN enrollments e2 ON e2.student_id = m.student_id
                  WHERE m.class_subject_id = cs.id AND m.exam_id = ?2
                    AND e2.class_id = cs.class_id AND e2.academic_year_id = ?3 AND e2.status = 'active')
         FROM class_subjects cs JOIN subjects s ON s.id = cs.subject_id
         WHERE cs.class_id = ?1 AND cs.status = 'active'
         ORDER BY cs.is_core DESC, cs.position ASC",
    )?;
    let rows = stmt.query_map(params![class_id, exam_id, year_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, i64>(2)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (subject, teacher, entered) = row?;
        if entered >= expected {
            continue;
        }
        if let Some(teacher_id) = teacher.or_else(|| class_teacher.clone()) {
            out.push(Missing { teacher_id, item: format!("{class_name} {subject} ({entered} of {expected})") });
        }
    }
    Ok(out)
}

/// Mornings from 07:30: a text a day or two before each deadline, and one on
/// the day, to every teacher who still has marks to enter.
pub fn marks_reminder_job(conn: &Connection, now: NaiveDateTime) -> AppResult<usize> {
    if repo::get_setting(conn, "auto_sms.marks_reminders")?.as_deref() != Some("1") || crate::network::offline_only(conn)? {
        return Ok(0);
    }
    if now.time() < NaiveTime::from_hms_opt(7, 30, 0).expect("valid") {
        return Ok(0);
    }
    let Some(year_id) = repo::current_academic_year_id(conn)? else { return Ok(0) };
    let today = now.date();
    let exams: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT e.id, e.name FROM exams e JOIN terms t ON t.id = e.term_id
             WHERE e.status = 'open' AND t.academic_year_id = ?1",
        )?;
        let rows = stmt.query_map(params![year_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let collected = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        collected
    };
    let classes: Vec<(String, String)> = {
        let mut stmt = conn.prepare("SELECT id, name FROM classes WHERE status = 'active' ORDER BY ladder_position")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let collected = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        collected
    };
    let signature = sms::signature(conn)?;
    let mut queued = 0;
    for (exam_id, exam_name) in &exams {
        // teacher -> (due date, stage, what is missing)
        let mut by_teacher: BTreeMap<String, (NaiveDate, &'static str, Vec<String>)> = BTreeMap::new();
        for (class_id, class_name) in &classes {
            let Some(due) = deadline_for(conn, class_id, exam_id)? else { continue };
            let stage = match (due - today).num_days() {
                1 | 2 => "soon",
                0 => "today",
                _ => continue,
            };
            for missing in missing_in(conn, class_id, class_name, exam_id, &year_id)? {
                let entry = by_teacher.entry(missing.teacher_id).or_insert((due, stage, Vec::new()));
                // The earliest date wins when classes differ.
                if due < entry.0 {
                    entry.0 = due;
                    entry.1 = stage;
                }
                entry.2.push(missing.item);
            }
        }
        for (teacher_id, (due, stage, items)) in by_teacher {
            let teacher: Option<(String, Option<String>)> = conn
                .query_row("SELECT full_name, phone FROM users WHERE id = ?1 AND status = 'active'", params![teacher_id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?;
            let Some((name, phone)) = teacher else { continue };
            let Some(phone) = phone.as_deref().and_then(sms::normalize_phone) else { continue };
            let related = format!("{exam_id}:{stage}:{due}:{teacher_id}");
            let sent: bool = conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM sms_outbox WHERE kind = 'marks_reminder' AND related_id = ?1 AND status != 'failed')",
                params![related],
                |r| r.get(0),
            )?;
            if sent {
                continue;
            }
            let first = name.split_whitespace().next().unwrap_or(&name).to_string();
            let when = if stage == "today" { "today".to_string() } else { format!("by {}", long_date(due)) };
            let shown: Vec<String> = items.iter().take(6).cloned().collect();
            let more = if items.len() > shown.len() { format!(" and {} more", items.len() - shown.len()) } else { String::new() };
            let body = sms::plain_text(&format!(
                "{signature}: Dear {first}, {exam_name} marks are due {when}. Still to enter: {}{more}. Thank you.",
                shown.join(", ")
            ));
            sms::insert(conn, &phone, &body, "marks_reminder", Some(&related), None)?;
            queued += 1;
        }
    }
    Ok(queued)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    fn school() -> crate::db::Database {
        let db = crate::fees::tests::fee_school();
        db.lock()
            .execute_batch(
                "INSERT INTO subjects (id, code, name, created_at, updated_at) VALUES
                     ('mtc', 'MTC', 'Mathematics', 'now', 'now'), ('eng', 'ENG', 'English', 'now', 'now');
                 INSERT INTO class_subjects (id, class_id, subject_id, position, created_at, updated_at) VALUES
                     ('p5mtc', 'p5', 'mtc', 1, 'now', 'now'), ('p5eng', 'p5', 'eng', 2, 'now', 'now');
                 INSERT INTO exams (id, term_id, seq, code, name, status, created_at, updated_at) VALUES
                     ('mid', 't262', 2, 'MID', 'Mid Term', 'open', 'now', 'now');
                 INSERT INTO users (id, username, full_name, role, password_hash, phone, created_at, updated_at) VALUES
                     ('tm', 'tm', 'Joseph Okello', 'teacher', 'x', '0772111222', 'now', 'now'),
                     ('ct', 'ct', 'Grace Nambi', 'teacher', 'x', '0772333444', 'now', 'now');
                 INSERT INTO teacher_assignments (id, user_id, class_id, subject_id, role, created_at, updated_at) VALUES
                     ('a1', 'tm', 'p5', 'mtc', 'subject_teacher', 'now', 'now'),
                     ('a2', 'ct', 'p5', NULL, 'class_teacher', 'now', 'now');
                 INSERT INTO marks (id, student_id, class_subject_id, exam_id, score, is_absent, entered_by, entered_at, updated_by, updated_at)
                     VALUES ('m1', 'amina', 'p5mtc', 'mid', 60, 0, 'tm', 'now', 'tm', 'now');",
            )
            .unwrap();
        db
    }

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn texts(conn: &Connection) -> Vec<(String, String)> {
        let mut stmt = conn.prepare("SELECT to_phone, body FROM sms_outbox WHERE kind = 'marks_reminder' ORDER BY to_phone").unwrap();
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        let collected = rows.collect::<rusqlite::Result<Vec<_>>>().unwrap();
        collected
    }

    #[test]
    fn teachers_with_marks_missing_are_reminded_before_and_on_the_day() {
        let db = school();
        let conn = db.lock();
        let dos = test_session("dos", Role::Teacher, Some(Office::DirectorOfStudies), &[]);
        set_exam_deadline_in(&conn, &dos, "mid", Some("2026-06-12")).unwrap();
        assert_eq!(deadline_for(&conn, "p5", "mid").unwrap(), NaiveDate::from_ymd_opt(2026, 6, 12));
        let teacher = test_session("tm", Role::Teacher, None, &[]);
        assert!(matches!(set_exam_deadline_in(&conn, &teacher, "mid", Some("2026-06-01")), Err(AppError::Forbidden)));

        assert_eq!(marks_reminder_job(&conn, at("2026-06-10 09:00")).unwrap(), 0, "off until switched on");
        repo::set_setting(&conn, "auto_sms.marks_reminders", "1").unwrap();
        assert_eq!(marks_reminder_job(&conn, at("2026-06-09 09:00")).unwrap(), 0, "three days early");
        assert_eq!(marks_reminder_job(&conn, at("2026-06-10 07:00")).unwrap(), 0, "too early in the morning");
        assert_eq!(marks_reminder_job(&conn, at("2026-06-10 09:00")).unwrap(), 2);
        let sent = texts(&conn);
        assert!(sent[0].1.contains("Dear Joseph, Mid Term marks are due by Friday 12 June. Still to enter: P5 Mathematics (1 of 2)"), "{}", sent[0].1);
        assert!(sent[1].1.contains("Dear Grace") && sent[1].1.contains("P5 English (0 of 2)"), "English has no subject teacher, so the class teacher");
        assert_eq!(marks_reminder_job(&conn, at("2026-06-11 09:00")).unwrap(), 0, "one 'soon' text, not one a day");

        // Joseph finishes before the day; only Grace hears again.
        conn.execute(
            "INSERT INTO marks (id, student_id, class_subject_id, exam_id, score, is_absent, entered_by, entered_at, updated_by, updated_at)
             VALUES ('m2', 'musa', 'p5mtc', 'mid', NULL, 1, 'tm', 'now', 'tm', 'now')",
            [],
        )
        .unwrap();
        assert_eq!(marks_reminder_job(&conn, at("2026-06-12 08:00")).unwrap(), 1);
        assert!(texts(&conn).iter().any(|(_, b)| b.contains("Dear Grace, Mid Term marks are due today")));
        assert_eq!(marks_reminder_job(&conn, at("2026-06-13 08:00")).unwrap(), 0, "nothing after the date");
    }

    #[test]
    fn a_class_date_overrides_the_exam_and_offline_stops_it() {
        let db = school();
        let conn = db.lock();
        repo::set_setting(&conn, "deadline.exam.mid", "2026-06-12").unwrap();
        repo::set_setting(&conn, "deadline.p5.mid", "2026-06-20T14:00:00.000Z").unwrap();
        assert_eq!(deadline_for(&conn, "p5", "mid").unwrap(), NaiveDate::from_ymd_opt(2026, 6, 20));
        repo::set_setting(&conn, "auto_sms.marks_reminders", "1").unwrap();
        crate::network::set_offline_only(&conn, true).unwrap();
        assert_eq!(marks_reminder_job(&conn, at("2026-06-19 09:00")).unwrap(), 0);
    }

    #[test]
    fn dates_read_in_every_shape() {
        assert_eq!(parse_deadline("2026-10-23"), NaiveDate::from_ymd_opt(2026, 10, 23));
        assert_eq!(parse_deadline("2026-10-23T17:00"), NaiveDate::from_ymd_opt(2026, 10, 23));
        assert!(parse_deadline("soon").is_none());
        assert_eq!(long_date(NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()), "Friday 23 October");
    }
}
