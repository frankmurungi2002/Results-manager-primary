//! FR-G9 — the daily attendance register.
//!
//! The Class Teacher marks each learner present, absent, late or excused for
//! a day. The register feeds "days present" on the report card, the school
//! overview, the printable monthly register, and — when the teacher asks —
//! an SMS to the guardian of every learner newly marked absent (FR-G8).

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate, Utc, Weekday};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::audit;
use crate::domain::ids::{new_id, prefix};
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::sms;
use crate::state::AppState;

pub const STATES: &[&str] = &["present", "absent", "late", "excused"];

pub fn parse_date(text: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::validation("Use a date like 2026-09-29."))
}

/// The term a date falls in: by its dates first, then the open term.
pub fn term_for_date(conn: &Connection, date: &str) -> AppResult<Option<(String, String)>> {
    let dated: Option<(String, String)> = conn
        .query_row(
            "SELECT id, name FROM terms
             WHERE start_date IS NOT NULL AND end_date IS NOT NULL
               AND ?1 BETWEEN start_date AND end_date
             ORDER BY start_date DESC LIMIT 1",
            params![date],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if dated.is_some() {
        return Ok(dated);
    }
    match repo::current_term_id(conn)? {
        Some(id) => Ok(conn
            .query_row("SELECT id, name FROM terms WHERE id = ?1", params![id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Marking the register
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Register {
    pub class_id: String,
    pub class_name: String,
    pub stream_id: Option<String>,
    pub on_date: String,
    pub weekday: String,
    pub is_weekend: bool,
    pub term_id: Option<String>,
    pub term_name: Option<String>,
    /// True once anyone has saved this register for this day.
    pub already_marked: bool,
    pub marked_by: Option<String>,
    pub rows: Vec<RegisterRow>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRow {
    pub student_id: String,
    pub reg_number: String,
    pub full_name: String,
    pub gender: Option<String>,
    pub stream_name: Option<String>,
    pub guardian_phone: Option<String>,
    /// present / absent / late / excused; None when not yet marked.
    pub state: Option<String>,
    pub note: Option<String>,
    /// Days absent so far this term, to spot a pattern while marking.
    pub absences_this_term: i64,
}

#[tauri::command]
pub fn load_register(
    state: State<'_, AppState>,
    class_id: String,
    stream_id: Option<String>,
    on_date: String,
) -> AppResult<Register> {
    let session = state.sessions.require()?;
    session.require_view_class(&class_id)?;
    let date = parse_date(&on_date)?;
    let on_date = date.format("%Y-%m-%d").to_string();

    let conn = state.db.lock();
    let year_id = repo::current_academic_year_id(&conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;
    let class_name: String = conn
        .query_row("SELECT name FROM classes WHERE id = ?1", params![class_id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That class"))?;
    let term = term_for_date(&conn, &on_date)?;

    let mut stmt = conn.prepare(
        "SELECT s.id, s.reg_number, s.full_name, s.gender, st.name, s.guardian_phone,
                a.state, a.note,
                (SELECT COUNT(*) FROM attendance x
                  WHERE x.student_id = s.id AND x.term_id = ?4 AND x.state = 'absent')
         FROM enrollments e
         JOIN students s ON s.id = e.student_id
         LEFT JOIN streams st ON st.id = e.stream_id
         LEFT JOIN attendance a ON a.student_id = s.id AND a.on_date = ?3
         WHERE e.class_id = ?1 AND e.academic_year_id = ?2 AND e.status = 'active'
           AND (?5 IS NULL OR e.stream_id = ?5)
         ORDER BY s.full_name COLLATE NOCASE ASC",
    )?;
    let term_id = term.as_ref().map(|(id, _)| id.clone());
    let rows = stmt
        .query_map(params![class_id, year_id, on_date, term_id, stream_id], |row| {
            Ok(RegisterRow {
                student_id: row.get(0)?,
                reg_number: row.get(1)?,
                full_name: row.get(2)?,
                gender: row.get(3)?,
                stream_name: row.get(4)?,
                guardian_phone: row.get(5)?,
                state: row.get(6)?,
                note: row.get(7)?,
                absences_this_term: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let marked_by: Option<String> = conn
        .query_row(
            "SELECT u.full_name FROM attendance a
             LEFT JOIN users u ON u.id = a.marked_by
             WHERE a.class_id = ?1 AND a.on_date = ?2 AND (?3 IS NULL OR a.stream_id = ?3)
             ORDER BY a.marked_at DESC LIMIT 1",
            params![class_id, on_date, stream_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();

    Ok(Register {
        already_marked: rows.iter().any(|row| row.state.is_some()),
        class_id,
        class_name,
        stream_id,
        weekday: date.format("%A").to_string(),
        is_weekend: matches!(date.weekday(), Weekday::Sat | Weekday::Sun),
        on_date,
        term_id: term.as_ref().map(|(id, _)| id.clone()),
        term_name: term.map(|(_, name)| name),
        marked_by,
        rows,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterEntry {
    pub student_id: String,
    pub state: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRegisterRequest {
    pub class_id: String,
    #[serde(default)]
    pub stream_id: Option<String>,
    pub on_date: String,
    pub entries: Vec<RegisterEntry>,
    /// Text the guardian of every learner newly marked absent.
    #[serde(default)]
    pub notify_absent: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRegisterResult {
    pub saved: usize,
    pub present: usize,
    pub absent: usize,
    pub late: usize,
    pub excused: usize,
    pub texts_queued: usize,
}

#[tauri::command]
pub fn save_register(
    app: AppHandle,
    state: State<'_, AppState>,
    request: SaveRegisterRequest,
) -> AppResult<SaveRegisterResult> {
    let session = state.sessions.require()?;
    session.require_manage_class(&request.class_id)?;
    let date = parse_date(&request.on_date)?;
    if date > Utc::now().date_naive() + chrono::Duration::days(1) {
        return Err(AppError::validation("A register cannot be marked for a day that has not come yet."));
    }
    let on_date = date.format("%Y-%m-%d").to_string();
    if let Some(bad) = request.entries.iter().find(|e| !STATES.contains(&e.state.as_str())) {
        return Err(AppError::validation(format!("\"{}\" is not an attendance mark.", bad.state)));
    }

    let conn = state.db.lock();
    let year_id = repo::current_academic_year_id(&conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;
    let (term_id, _) = term_for_date(&conn, &on_date)?
        .ok_or_else(|| AppError::validation("No term covers that date. Set the term dates under Terms & exams."))?;
    let class_name: String =
        conn.query_row("SELECT name FROM classes WHERE id = ?1", params![request.class_id], |row| row.get(0))?;

    // Who is on the roster, their stream, and what they were marked before.
    struct Learner {
        name: String,
        stream_id: Option<String>,
        phone: Option<String>,
        before: Option<String>,
    }
    let roster: HashMap<String, Learner> = {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.full_name, e.stream_id, s.guardian_phone, a.state
             FROM enrollments e
             JOIN students s ON s.id = e.student_id
             LEFT JOIN attendance a ON a.student_id = s.id AND a.on_date = ?3
             WHERE e.class_id = ?1 AND e.academic_year_id = ?2 AND e.status = 'active'",
        )?;
        let collected = stmt
            .query_map(params![request.class_id, year_id, on_date], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    Learner {
                        name: row.get(1)?,
                        stream_id: row.get(2)?,
                        phone: row.get(3)?,
                        before: row.get(4)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        collected
    };

    let signature = if request.notify_absent { Some(sms::signature(&conn)?) } else { None };
    let pretty_date = date.format("%A %-d %B").to_string();
    let now = Utc::now().to_rfc3339();

    let tx = conn.unchecked_transaction()?;
    let mut result = SaveRegisterResult {
        saved: 0,
        present: 0,
        absent: 0,
        late: 0,
        excused: 0,
        texts_queued: 0,
    };
    for entry in &request.entries {
        let Some(learner) = roster.get(&entry.student_id) else {
            continue;
        };
        let note = entry.note.as_deref().map(str::trim).filter(|n| !n.is_empty());
        tx.execute(
            "INSERT INTO attendance (id, student_id, class_id, stream_id, term_id, on_date, state, note, marked_by, marked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT (student_id, on_date) DO UPDATE SET
                class_id = excluded.class_id, stream_id = excluded.stream_id, term_id = excluded.term_id,
                state = excluded.state, note = excluded.note,
                marked_by = excluded.marked_by, marked_at = excluded.marked_at",
            params![
                new_id(prefix::ATTENDANCE),
                entry.student_id,
                request.class_id,
                learner.stream_id,
                term_id,
                on_date,
                entry.state,
                note,
                session.user_id,
                now,
            ],
        )?;
        result.saved += 1;
        match entry.state.as_str() {
            "present" => result.present += 1,
            "absent" => result.absent += 1,
            "late" => result.late += 1,
            _ => result.excused += 1,
        }

        // Only a learner newly marked absent gets a text: saving the same
        // register twice must not text a parent twice.
        if let Some(sig) = &signature {
            if entry.state == "absent" && learner.before.as_deref() != Some("absent") {
                let body = format!(
                    "{sig}: {} was marked absent from school today, {pretty_date}. Please contact the class teacher if you did not know.",
                    learner.name
                );
                if sms::queue(&tx, learner.phone.as_deref(), &body, "absence", &entry.student_id, Some(&session.user_id))?
                    .is_some()
                {
                    result.texts_queued += 1;
                }
            }
        }
    }

    audit::record(
        &tx,
        Some(&session),
        "attendance.mark",
        "class",
        &request.class_id,
        format!(
            "Marked the {class_name} register for {on_date}: {} present, {} absent, {} late, {} excused{}",
            result.present,
            result.absent,
            result.late,
            result.excused,
            if result.texts_queued > 0 {
                format!(", {} guardians texted", result.texts_queued)
            } else {
                String::new()
            }
        ),
    )?;
    tx.commit()?;
    drop(conn);

    if result.texts_queued > 0 {
        sms::flush_in_background(app);
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// The whole school for one day
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceOverviewRow {
    pub class_id: String,
    pub class_name: String,
    pub learners: i64,
    pub marked: i64,
    pub present: i64,
    pub absent: i64,
    pub late: i64,
    pub excused: i64,
    pub marked_by: Option<String>,
    pub class_teacher_name: Option<String>,
}

#[tauri::command]
pub fn attendance_overview(state: State<'_, AppState>, on_date: String) -> AppResult<Vec<AttendanceOverviewRow>> {
    let session = state.sessions.require()?;
    let on_date = parse_date(&on_date)?.format("%Y-%m-%d").to_string();
    let conn = state.db.lock();
    let year_id = repo::current_academic_year_id(&conn)?;

    let mut stmt = conn.prepare(
        "SELECT c.id, c.name,
                (SELECT COUNT(*) FROM enrollments e
                  WHERE e.class_id = c.id AND e.status = 'active' AND (?2 IS NULL OR e.academic_year_id = ?2)),
                COUNT(a.id),
                SUM(CASE WHEN a.state = 'present' THEN 1 ELSE 0 END),
                SUM(CASE WHEN a.state = 'absent' THEN 1 ELSE 0 END),
                SUM(CASE WHEN a.state = 'late' THEN 1 ELSE 0 END),
                SUM(CASE WHEN a.state = 'excused' THEN 1 ELSE 0 END),
                (SELECT u.full_name FROM attendance x JOIN users u ON u.id = x.marked_by
                  WHERE x.class_id = c.id AND x.on_date = ?1 ORDER BY x.marked_at DESC LIMIT 1),
                (SELECT u.full_name FROM teacher_assignments ta JOIN users u ON u.id = ta.user_id
                  WHERE ta.class_id = c.id AND ta.role = 'class_teacher' AND ta.status = 'active' LIMIT 1)
         FROM classes c
         LEFT JOIN attendance a ON a.class_id = c.id AND a.on_date = ?1
         WHERE c.status = 'active'
         GROUP BY c.id
         ORDER BY c.ladder_position ASC",
    )?;
    let rows = stmt
        .query_map(params![on_date, year_id], |row| {
            Ok(AttendanceOverviewRow {
                class_id: row.get(0)?,
                class_name: row.get(1)?,
                learners: row.get(2)?,
                marked: row.get(3)?,
                present: row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                absent: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
                late: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
                excused: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
                marked_by: row.get(8)?,
                class_teacher_name: row.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().filter(|row| session.may_view_class(&row.class_id)).collect())
}

// ---------------------------------------------------------------------------
// The monthly register (screen and print)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthRegister {
    pub class_name: String,
    pub stream_name: Option<String>,
    /// e.g. "September 2026"
    pub month_label: String,
    /// Every school day (Monday to Friday, plus any day someone marked).
    pub days: Vec<RegisterDay>,
    pub rows: Vec<MonthRow>,
    pub totals: MonthTotals,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDay {
    pub date: String,
    pub day: u32,
    /// "M", "T", "W", "T", "F", "S", "S"
    pub initial: String,
    /// How many learners were present (or late) that day; None if unmarked.
    pub present: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthRow {
    pub student_id: String,
    pub full_name: String,
    pub reg_number: String,
    pub gender: Option<String>,
    /// One mark per entry in `days`: "P", "A", "L", "E" or "" when unmarked.
    pub marks: Vec<String>,
    pub present: i64,
    pub absent: i64,
    pub late: i64,
    pub excused: i64,
    /// Present-or-late as a share of days marked, 0–100.
    pub rate: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthTotals {
    pub days_marked: i64,
    pub present: i64,
    pub absent: i64,
    pub late: i64,
    pub excused: i64,
    pub rate: Option<f64>,
}

pub fn month_register(
    conn: &Connection,
    class_id: &str,
    stream_id: Option<&str>,
    year: i32,
    month: u32,
) -> AppResult<MonthRegister> {
    let first = NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| AppError::validation("That is not a month."))?;
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .ok_or_else(|| AppError::validation("That is not a month."))?;
    let (from, to) = (first.format("%Y-%m-%d").to_string(), next.format("%Y-%m-%d").to_string());

    let year_id = repo::current_academic_year_id(conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;
    let class_name: String =
        conn.query_row("SELECT name FROM classes WHERE id = ?1", params![class_id], |row| row.get(0))?;
    let stream_name: Option<String> = match stream_id {
        Some(id) => conn
            .query_row("SELECT name FROM streams WHERE id = ?1", params![id], |row| row.get(0))
            .optional()?,
        None => None,
    };

    // Every mark this month for the class.
    let mut marks: HashMap<(String, String), String> = HashMap::new();
    let mut marked_days: std::collections::BTreeSet<String> = Default::default();
    {
        let mut stmt = conn.prepare(
            "SELECT student_id, on_date, state FROM attendance
             WHERE class_id = ?1 AND on_date >= ?2 AND on_date < ?3
               AND (?4 IS NULL OR stream_id = ?4)",
        )?;
        let rows = stmt.query_map(params![class_id, from, to, stream_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?;
        for row in rows {
            let (student, date, state) = row?;
            marked_days.insert(date.clone());
            marks.insert((student, date), state);
        }
    }

    // The days to show: every weekday, plus any weekend day that was marked.
    let mut days = Vec::new();
    let mut day = first;
    while day < next {
        let key = day.format("%Y-%m-%d").to_string();
        let weekend = matches!(day.weekday(), Weekday::Sat | Weekday::Sun);
        if !weekend || marked_days.contains(&key) {
            days.push(key);
        }
        day = day.succ_opt().unwrap_or(next);
    }

    let mut stmt = conn.prepare(
        "SELECT s.id, s.full_name, s.reg_number, s.gender
         FROM enrollments e JOIN students s ON s.id = e.student_id
         WHERE e.class_id = ?1 AND e.academic_year_id = ?2 AND e.status = 'active'
           AND (?3 IS NULL OR e.stream_id = ?3)
         ORDER BY s.full_name COLLATE NOCASE ASC",
    )?;
    let learners = stmt
        .query_map(params![class_id, year_id, stream_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let rate = |present: i64, late: i64, marked: i64| -> Option<f64> {
        (marked > 0).then(|| ((present + late) as f64 / marked as f64 * 1000.0).round() / 10.0)
    };

    let mut totals = MonthTotals {
        days_marked: marked_days.len() as i64,
        present: 0,
        absent: 0,
        late: 0,
        excused: 0,
        rate: None,
    };
    let mut rows = Vec::new();
    for (id, full_name, reg_number, gender) in learners {
        let mut row = MonthRow {
            student_id: id.clone(),
            full_name,
            reg_number,
            gender,
            marks: Vec::with_capacity(days.len()),
            present: 0,
            absent: 0,
            late: 0,
            excused: 0,
            rate: None,
        };
        for date in &days {
            let mark = match marks.get(&(id.clone(), date.clone())).map(String::as_str) {
                Some("present") => {
                    row.present += 1;
                    "P"
                }
                Some("absent") => {
                    row.absent += 1;
                    "A"
                }
                Some("late") => {
                    row.late += 1;
                    "L"
                }
                Some("excused") => {
                    row.excused += 1;
                    "E"
                }
                _ => "",
            };
            row.marks.push(mark.to_string());
        }
        row.rate = rate(row.present, row.late, row.present + row.absent + row.late + row.excused);
        totals.present += row.present;
        totals.absent += row.absent;
        totals.late += row.late;
        totals.excused += row.excused;
        rows.push(row);
    }
    totals.rate = rate(totals.present, totals.late, totals.present + totals.absent + totals.late + totals.excused);

    let register_days = days
        .iter()
        .map(|date| {
            let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap_or(first);
            let present = marked_days.contains(date).then(|| {
                rows.iter()
                    .filter(|row| {
                        let index = days.iter().position(|d| d == date).unwrap_or(0);
                        matches!(row.marks[index].as_str(), "P" | "L")
                    })
                    .count() as i64
            });
            RegisterDay {
                date: date.clone(),
                day: parsed.day(),
                initial: parsed.format("%a").to_string().chars().next().unwrap_or(' ').to_string(),
                present,
            }
        })
        .collect();

    Ok(MonthRegister {
        class_name,
        stream_name,
        month_label: first.format("%B %Y").to_string(),
        days: register_days,
        rows,
        totals,
    })
}

#[tauri::command]
pub fn attendance_month(
    state: State<'_, AppState>,
    class_id: String,
    stream_id: Option<String>,
    year: i32,
    month: u32,
) -> AppResult<MonthRegister> {
    let session = state.sessions.require()?;
    session.require_view_class(&class_id)?;
    let conn = state.db.lock();
    month_register(&conn, &class_id, stream_id.as_deref(), year, month)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn the_month_counts_marks_and_skips_weekends() {
        let db = Database::open_in_memory().expect("open");
        let conn = db.lock();
        conn.execute_batch(
            "PRAGMA foreign_keys = OFF;
             INSERT INTO academic_years (id, label, status, created_at, updated_at) VALUES ('yr1', '2026', 'active', 'now', 'now');
             INSERT INTO classes (id, code, name, level_kind, ladder_position, created_at, updated_at)
                 VALUES ('c1', 'P5', 'Primary Five', 'primary', 8, 'now', 'now');
             INSERT INTO students (id, reg_number, full_name, created_at, updated_at) VALUES ('s1', 'R1', 'Amina', 'now', 'now');
             INSERT INTO enrollments (id, student_id, class_id, academic_year_id, joined_at, created_at, updated_at)
                 VALUES ('e1', 's1', 'c1', 'yr1', 'now', 'now', 'now');
             INSERT INTO attendance (id, student_id, class_id, term_id, on_date, state, marked_at) VALUES
                 ('a1', 's1', 'c1', 't1', '2026-09-01', 'present', 'now'),
                 ('a2', 's1', 'c1', 't1', '2026-09-02', 'absent', 'now'),
                 ('a3', 's1', 'c1', 't1', '2026-09-03', 'late', 'now');",
        )
        .expect("seed");

        let register = month_register(&conn, "c1", None, 2026, 9).expect("register");
        // September 2026 has 22 weekdays.
        assert_eq!(register.days.len(), 22);
        let row = &register.rows[0];
        assert_eq!(&row.marks[..3], &["P".to_string(), "A".to_string(), "L".to_string()]);
        assert_eq!((row.present, row.absent, row.late), (1, 1, 1));
        assert_eq!(row.rate, Some(66.7));
        assert_eq!(register.totals.days_marked, 3);
    }
}
