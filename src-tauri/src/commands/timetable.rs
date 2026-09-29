//! FR-G6 — class timetables, and FR-G7 — the exam timetable.
//!
//! Class timetables: the school sets its day once (lessons, breaks, lunch,
//! assembly, games) and which weekdays it teaches. Each class — or each stream
//! — gets a grid of days × lessons, filled by hand or by the auto-fill. A
//! teacher can never be in two rooms at once: every save checks the whole
//! school, and the auto-fill works around every existing lesson.
//!
//! Exam timetable: each examination (BOT, MID, END…) gets papers — a subject
//! on a date and time, sat by one or more classes, with a venue and an
//! invigilator. No class sits two papers at once and no invigilator is in two
//! places. The first paper's date becomes the exam's date, so permits carry it.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate, NaiveTime, Utc, Weekday};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::commands::reports::{envelope, DocumentEnvelope};
use crate::domain::ids::new_id;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const DAY_NAMES: &[&str] = &["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const KINDS: &[&str] = &["lesson", "break", "lunch", "assembly", "games", "prep"];

fn check_time(text: &str) -> AppResult<String> {
    NaiveTime::parse_from_str(text.trim(), "%H:%M")
        .map(|t| t.format("%H:%M").to_string())
        .map_err(|_| AppError::validation(format!("\"{text}\" is not a time. Use 24-hour HH:MM, like 08:20.")))
}

// ---------------------------------------------------------------------------
// The school day
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodRow {
    pub id: String,
    pub seq: i64,
    pub label: String,
    pub start_time: String,
    pub end_time: String,
    pub kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimetableSetup {
    pub periods: Vec<PeriodRow>,
    /// 5 (Monday to Friday) or 6 (with Saturday).
    pub days: i64,
}

pub fn load_periods(conn: &Connection) -> AppResult<Vec<PeriodRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, seq, label, start_time, end_time, kind FROM timetable_periods
         WHERE status = 'active' ORDER BY seq ASC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PeriodRow {
                id: row.get(0)?,
                seq: row.get(1)?,
                label: row.get(2)?,
                start_time: row.get(3)?,
                end_time: row.get(4)?,
                kind: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn school_days(conn: &Connection) -> AppResult<i64> {
    Ok(match repo::get_setting(conn, "timetable.days")?.as_deref() {
        Some("6") => 6,
        _ => 5,
    })
}

#[tauri::command]
pub fn get_timetable_setup(state: State<'_, AppState>) -> AppResult<TimetableSetup> {
    state.sessions.require()?;
    let conn = state.db.lock();
    Ok(TimetableSetup {
        periods: load_periods(&conn)?,
        days: school_days(&conn)?,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodInput {
    #[serde(default)]
    pub id: Option<String>,
    pub label: String,
    pub start_time: String,
    pub end_time: String,
    pub kind: String,
}

/// Replaces the school day. A period left out is retired, not deleted, so a
/// lesson placed in it is kept in the records and simply stops showing.
#[tauri::command]
pub fn save_timetable_setup(
    state: State<'_, AppState>,
    periods: Vec<PeriodInput>,
    days: i64,
) -> AppResult<TimetableSetup> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    if !(days == 5 || days == 6) {
        return Err(AppError::validation("A school week is 5 or 6 days."));
    }
    if periods.is_empty() || periods.len() > 20 {
        return Err(AppError::validation("A school day has 1 to 20 periods."));
    }

    let mut cleaned = Vec::new();
    let mut previous_end: Option<String> = None;
    for (index, period) in periods.iter().enumerate() {
        let label = period.label.trim();
        if label.is_empty() {
            return Err(AppError::validation(format!("Period {} needs a name.", index + 1)));
        }
        if !KINDS.contains(&period.kind.as_str()) {
            return Err(AppError::validation("Unknown kind of period."));
        }
        let start = check_time(&period.start_time)?;
        let end = check_time(&period.end_time)?;
        if end <= start {
            return Err(AppError::validation(format!("{label} must end after it starts.")));
        }
        if let Some(prev) = &previous_end {
            if &start < prev {
                return Err(AppError::validation(format!(
                    "{label} starts at {start}, before the period above it ends ({prev}). Periods go in order."
                )));
            }
        }
        previous_end = Some(end.clone());
        cleaned.push((period.id.clone(), label.to_string(), start, end, period.kind.clone()));
    }

    let conn = state.db.lock();
    let now = Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    let mut kept: HashSet<String> = HashSet::new();
    for (seq, (id, label, start, end, kind)) in cleaned.into_iter().enumerate() {
        let existing = match &id {
            Some(id) => tx
                .query_row("SELECT id FROM timetable_periods WHERE id = ?1", params![id], |row| {
                    row.get::<_, String>(0)
                })
                .optional()?,
            None => None,
        };
        let period_id = existing.unwrap_or_else(|| new_id("tp"));
        tx.execute(
            "INSERT INTO timetable_periods (id, seq, label, start_time, end_time, kind, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?7)
             ON CONFLICT (id) DO UPDATE SET seq = excluded.seq, label = excluded.label,
                start_time = excluded.start_time, end_time = excluded.end_time, kind = excluded.kind,
                status = 'active', updated_at = excluded.updated_at",
            params![period_id, seq as i64 + 1, label, start, end, kind, now],
        )?;
        kept.insert(period_id);
    }
    let all: Vec<String> = {
        let mut stmt = tx.prepare("SELECT id FROM timetable_periods WHERE status = 'active'")?;
        let collected = stmt.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        collected
    };
    for id in all.into_iter().filter(|id| !kept.contains(id)) {
        tx.execute(
            "UPDATE timetable_periods SET status = 'retired', updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
    }
    repo::set_setting(&tx, "timetable.days", &days.to_string())?;
    audit::record(&tx, Some(&session), "timetable.setup", "setting", "timetable", "Updated the school day and week")?;
    tx.commit()?;

    Ok(TimetableSetup {
        periods: load_periods(&conn)?,
        days,
    })
}

// ---------------------------------------------------------------------------
// A class's (or stream's) timetable
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotRow {
    pub day: i64,
    pub period_id: String,
    pub subject_id: Option<String>,
    pub subject_name: Option<String>,
    pub teacher_id: Option<String>,
    pub teacher_name: Option<String>,
    pub room: Option<String>,
    /// Set when the teacher is booked somewhere else at the same time.
    pub clash: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectLoad {
    pub class_subject_id: String,
    pub subject_id: String,
    pub name: String,
    pub lessons_per_week: i64,
    pub placed: i64,
    pub teacher_id: Option<String>,
    pub teacher_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassTimetable {
    pub class_id: String,
    pub class_name: String,
    pub stream_id: Option<String>,
    pub stream_name: Option<String>,
    pub days: i64,
    pub periods: Vec<PeriodRow>,
    pub slots: Vec<SlotRow>,
    pub subjects: Vec<SubjectLoad>,
}

/// Every lesson in the school: (teacher, day, period) → where they are.
fn teacher_bookings(conn: &Connection) -> AppResult<HashMap<(String, i64, String), (String, Option<String>, String)>> {
    let mut stmt = conn.prepare(
        "SELECT ts.teacher_id, ts.day, ts.period_id, ts.class_id, ts.stream_id,
                c.name || COALESCE(' ' || st.name, '')
         FROM timetable_slots ts
         JOIN classes c ON c.id = ts.class_id
         LEFT JOIN streams st ON st.id = ts.stream_id
         WHERE ts.teacher_id IS NOT NULL",
    )?;
    let mut map = HashMap::new();
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    for row in rows {
        let (teacher, day, period, class_id, stream_id, label) = row?;
        map.insert((teacher, day, period), (class_id, stream_id, label));
    }
    Ok(map)
}

fn subject_teacher(conn: &Connection, class_id: &str, stream_id: Option<&str>, subject_id: &str) -> AppResult<Option<(String, String)>> {
    Ok(conn
        .query_row(
            "SELECT u.id, u.full_name FROM teacher_assignments ta
             JOIN users u ON u.id = ta.user_id
             WHERE ta.class_id = ?1 AND ta.subject_id = ?2 AND ta.role = 'subject_teacher'
               AND ta.status = 'active' AND (ta.stream_id IS NULL OR ta.stream_id IS ?3)
             ORDER BY ta.stream_id IS NULL ASC LIMIT 1",
            params![class_id, subject_id, stream_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

pub fn class_timetable(conn: &Connection, class_id: &str, stream_id: Option<&str>) -> AppResult<ClassTimetable> {
    let class_name: String = conn
        .query_row("SELECT name FROM classes WHERE id = ?1", params![class_id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That class"))?;
    let stream_name: Option<String> = match stream_id {
        Some(id) => conn
            .query_row("SELECT name FROM streams WHERE id = ?1", params![id], |row| row.get(0))
            .optional()?,
        None => None,
    };
    let periods = load_periods(conn)?;
    let days = school_days(conn)?;
    let bookings = teacher_bookings(conn)?;

    let mut stmt = conn.prepare(
        "SELECT ts.day, ts.period_id, ts.subject_id, s.name, ts.teacher_id, u.full_name, ts.room
         FROM timetable_slots ts
         JOIN timetable_periods tp ON tp.id = ts.period_id AND tp.status = 'active'
         LEFT JOIN subjects s ON s.id = ts.subject_id
         LEFT JOIN users u ON u.id = ts.teacher_id
         WHERE ts.class_id = ?1 AND ts.stream_id IS ?2 AND ts.day <= ?3",
    )?;
    let slots: Vec<SlotRow> = stmt
        .query_map(params![class_id, stream_id, days], |row| {
            Ok(SlotRow {
                day: row.get(0)?,
                period_id: row.get(1)?,
                subject_id: row.get(2)?,
                subject_name: row.get(3)?,
                teacher_id: row.get(4)?,
                teacher_name: row.get(5)?,
                room: row.get(6)?,
                clash: None,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    // A clash is a booking of the same teacher, same time, in another class.
    let slots = slots
        .into_iter()
        .map(|mut slot| {
            if let Some(teacher) = &slot.teacher_id {
                if let Some((other_class, other_stream, label)) =
                    bookings.get(&(teacher.clone(), slot.day, slot.period_id.clone()))
                {
                    if other_class != class_id || other_stream.as_deref() != stream_id {
                        slot.clash = Some(format!("Also teaching {label} at this time"));
                    }
                }
            }
            slot
        })
        .collect::<Vec<_>>();

    let mut placed: HashMap<String, i64> = HashMap::new();
    for slot in &slots {
        if let Some(subject) = &slot.subject_id {
            *placed.entry(subject.clone()).or_default() += 1;
        }
    }

    let mut stmt = conn.prepare(
        "SELECT cs.id, cs.subject_id, COALESCE(cs.display_name, s.name), cs.lessons_per_week
         FROM class_subjects cs JOIN subjects s ON s.id = cs.subject_id
         WHERE cs.class_id = ?1 AND cs.status = 'active'
         ORDER BY cs.is_core DESC, cs.position ASC",
    )?;
    let raw: Vec<(String, String, String, i64)> = stmt
        .query_map(params![class_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut subjects = Vec::new();
    for (cs_id, subject_id, name, lessons) in raw {
        let teacher = subject_teacher(conn, class_id, stream_id, &subject_id)?;
        subjects.push(SubjectLoad {
            placed: placed.get(&subject_id).copied().unwrap_or(0),
            class_subject_id: cs_id,
            subject_id,
            name,
            lessons_per_week: lessons,
            teacher_id: teacher.as_ref().map(|t| t.0.clone()),
            teacher_name: teacher.map(|t| t.1),
        });
    }

    Ok(ClassTimetable {
        class_id: class_id.to_string(),
        class_name,
        stream_id: stream_id.map(String::from),
        stream_name,
        days,
        periods,
        slots,
        subjects,
    })
}

#[tauri::command]
pub fn load_class_timetable(
    state: State<'_, AppState>,
    class_id: String,
    stream_id: Option<String>,
) -> AppResult<ClassTimetable> {
    let session = state.sessions.require()?;
    session.require_view_class(&class_id)?;
    let conn = state.db.lock();
    class_timetable(&conn, &class_id, stream_id.as_deref())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSlotRequest {
    pub class_id: String,
    #[serde(default)]
    pub stream_id: Option<String>,
    pub day: i64,
    pub period_id: String,
    /// None empties the cell.
    #[serde(default)]
    pub subject_id: Option<String>,
    #[serde(default)]
    pub teacher_id: Option<String>,
    #[serde(default)]
    pub room: Option<String>,
}

#[tauri::command]
pub fn save_timetable_slot(state: State<'_, AppState>, request: SaveSlotRequest) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    let conn = state.db.lock();

    if !(1..=6).contains(&request.day) {
        return Err(AppError::validation("Pick a day of the week."));
    }
    let kind: String = conn
        .query_row(
            "SELECT kind FROM timetable_periods WHERE id = ?1 AND status = 'active'",
            params![request.period_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That period"))?;
    if kind != "lesson" && kind != "prep" && request.subject_id.is_some() {
        return Err(AppError::validation("Only lesson and prep periods take a subject."));
    }

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "DELETE FROM timetable_slots
         WHERE class_id = ?1 AND stream_id IS ?2 AND day = ?3 AND period_id = ?4",
        params![request.class_id, request.stream_id, request.day, request.period_id],
    )?;
    let Some(subject_id) = request.subject_id.clone() else {
        return Ok(());
    };

    if let Some(teacher) = &request.teacher_id {
        let busy: Option<String> = conn
            .query_row(
                "SELECT c.name || COALESCE(' ' || st.name, '') FROM timetable_slots ts
                 JOIN classes c ON c.id = ts.class_id
                 LEFT JOIN streams st ON st.id = ts.stream_id
                 WHERE ts.teacher_id = ?1 AND ts.day = ?2 AND ts.period_id = ?3",
                params![teacher, request.day, request.period_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(label) = busy {
            let name: String =
                conn.query_row("SELECT full_name FROM users WHERE id = ?1", params![teacher], |row| row.get(0))?;
            return Err(AppError::conflict(format!(
                "{name} is already teaching {label} at that time on {}.",
                DAY_NAMES[(request.day - 1) as usize]
            )));
        }
    }

    conn.execute(
        "INSERT INTO timetable_slots (id, class_id, stream_id, day, period_id, subject_id, teacher_id, room, updated_by, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            new_id("tts"),
            request.class_id,
            request.stream_id,
            request.day,
            request.period_id,
            subject_id,
            request.teacher_id,
            request.room.as_deref().map(str::trim).filter(|r| !r.is_empty()),
            session.user_id,
            now,
        ],
    )?;
    Ok(())
}

#[tauri::command]
pub fn set_lessons_per_week(state: State<'_, AppState>, class_subject_id: String, lessons: i64) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    if !(0..=15).contains(&lessons) {
        return Err(AppError::validation("A subject gets 0 to 15 lessons a week."));
    }
    let conn = state.db.lock();
    conn.execute(
        "UPDATE class_subjects SET lessons_per_week = ?1, updated_at = ?2 WHERE id = ?3",
        params![lessons, Utc::now().to_rfc3339(), class_subject_id],
    )?;
    Ok(())
}

#[tauri::command]
pub fn clear_timetable(state: State<'_, AppState>, class_id: String, stream_id: Option<String>) -> AppResult<usize> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    let conn = state.db.lock();
    let removed = conn.execute(
        "DELETE FROM timetable_slots WHERE class_id = ?1 AND stream_id IS ?2",
        params![class_id, stream_id],
    )?;
    audit::record(&conn, Some(&session), "timetable.clear", "class", &class_id, format!("Cleared {removed} lessons"))?;
    Ok(removed)
}

// ---------------------------------------------------------------------------
// Auto-fill
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoFillRequest {
    /// (class, stream) pairs to fill. A missing stream is the whole class.
    pub targets: Vec<AutoFillTarget>,
    /// Empty the chosen timetables first, instead of filling around them.
    #[serde(default)]
    pub replace: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoFillTarget {
    pub class_id: String,
    #[serde(default)]
    pub stream_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoFillResult {
    pub placed: usize,
    /// "P5 Mathematics: 2 of 7 lessons could not be placed" and the like.
    pub unplaced: Vec<String>,
}

/// Places each subject's weekly lessons into free lesson periods.
///
/// Rules, in order: a teacher is never in two places; a subject is spread
/// across the week (one lesson a day before a second on any day); core
/// subjects get the morning. Lessons already on the grid are kept unless
/// `replace` is set.
#[tauri::command]
pub fn auto_fill_timetable(state: State<'_, AppState>, request: AutoFillRequest) -> AppResult<AutoFillResult> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    let conn = state.db.lock();
    let tx = conn.unchecked_transaction()?;

    if request.replace {
        for target in &request.targets {
            tx.execute(
                "DELETE FROM timetable_slots WHERE class_id = ?1 AND stream_id IS ?2",
                params![target.class_id, target.stream_id],
            )?;
        }
    }

    let periods: Vec<PeriodRow> = load_periods(&tx)?.into_iter().filter(|p| p.kind == "lesson").collect();
    let days = school_days(&tx)?;
    if periods.is_empty() {
        return Err(AppError::validation("The school day has no lesson periods. Add some under Timetables → School day."));
    }
    let mut busy: HashSet<(String, i64, String)> = teacher_bookings(&tx)?
        .into_keys()
        .collect();

    let now = Utc::now().to_rfc3339();
    let mut result = AutoFillResult {
        placed: 0,
        unplaced: Vec::new(),
    };

    for target in &request.targets {
        let table = class_timetable(&tx, &target.class_id, target.stream_id.as_deref())?;
        let label = match &table.stream_name {
            Some(stream) => format!("{} {stream}", table.class_name),
            None => table.class_name.clone(),
        };
        // Cells already taken in this timetable.
        let mut taken: HashSet<(i64, String)> =
            table.slots.iter().map(|slot| (slot.day, slot.period_id.clone())).collect();
        // Lessons of each subject on each day, to spread them out.
        let mut per_day: HashMap<(String, i64), i64> = HashMap::new();
        for slot in &table.slots {
            if let Some(subject) = &slot.subject_id {
                *per_day.entry((subject.clone(), slot.day)).or_default() += 1;
            }
        }
        let core: HashSet<String> = {
            let mut stmt = tx.prepare(
                "SELECT subject_id FROM class_subjects WHERE class_id = ?1 AND status = 'active' AND is_core = 1",
            )?;
            let collected = stmt
                .query_map(params![target.class_id], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            collected
        };

        // Most lessons first: they are the hardest to fit.
        let mut subjects: Vec<&SubjectLoad> = table.subjects.iter().filter(|s| s.lessons_per_week > s.placed).collect();
        subjects.sort_by(|a, b| (b.lessons_per_week - b.placed).cmp(&(a.lessons_per_week - a.placed)));

        for subject in subjects {
            let mut needed = subject.lessons_per_week - subject.placed;
            let is_core = core.contains(&subject.subject_id);
            // Allow a second lesson on a day only when a day each is not enough.
            for max_per_day in 1..=2i64 {
                if needed == 0 {
                    break;
                }
                let mut day_order: Vec<i64> = (1..=days).collect();
                day_order.sort_by_key(|day| per_day.get(&(subject.subject_id.clone(), *day)).copied().unwrap_or(0));
                for day in day_order {
                    if needed == 0 {
                        break;
                    }
                    if per_day.get(&(subject.subject_id.clone(), day)).copied().unwrap_or(0) >= max_per_day {
                        continue;
                    }
                    let mut order: Vec<&PeriodRow> = periods.iter().collect();
                    if !is_core {
                        order.reverse();
                    }
                    let free = order.into_iter().find(|period| {
                        !taken.contains(&(day, period.id.clone()))
                            && subject
                                .teacher_id
                                .as_ref()
                                .is_none_or(|t| !busy.contains(&(t.clone(), day, period.id.clone())))
                    });
                    let Some(period) = free else { continue };
                    tx.execute(
                        "INSERT INTO timetable_slots (id, class_id, stream_id, day, period_id, subject_id, teacher_id, updated_by, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        params![
                            new_id("tts"),
                            target.class_id,
                            target.stream_id,
                            day,
                            period.id,
                            subject.subject_id,
                            subject.teacher_id,
                            session.user_id,
                            now,
                        ],
                    )?;
                    taken.insert((day, period.id.clone()));
                    if let Some(teacher) = &subject.teacher_id {
                        busy.insert((teacher.clone(), day, period.id.clone()));
                    }
                    *per_day.entry((subject.subject_id.clone(), day)).or_default() += 1;
                    needed -= 1;
                    result.placed += 1;
                }
            }
            if needed > 0 {
                result.unplaced.push(format!(
                    "{label} {}: {needed} of {} lessons had no free period{}",
                    subject.name,
                    subject.lessons_per_week,
                    if subject.teacher_id.is_some() { " while the teacher was free" } else { "" }
                ));
            }
        }
    }

    audit::record(
        &tx,
        Some(&session),
        "timetable.autofill",
        "timetable",
        "school",
        format!("Auto-filled {} lessons across {} timetables", result.placed, request.targets.len()),
    )?;
    tx.commit()?;
    Ok(result)
}

// ---------------------------------------------------------------------------
// A teacher's week
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeacherSlot {
    pub day: i64,
    pub period_id: String,
    pub class_name: String,
    pub stream_name: Option<String>,
    pub subject_name: Option<String>,
    pub room: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeacherTimetable {
    pub user_id: String,
    pub teacher_name: String,
    pub days: i64,
    pub periods: Vec<PeriodRow>,
    pub slots: Vec<TeacherSlot>,
}

pub fn teacher_timetable_of(conn: &Connection, user_id: &str) -> AppResult<TeacherTimetable> {
    let teacher_name: String = conn
        .query_row("SELECT full_name FROM users WHERE id = ?1", params![user_id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That teacher"))?;
    let days = school_days(conn)?;
    let mut stmt = conn.prepare(
        "SELECT ts.day, ts.period_id, c.name, st.name, s.name, ts.room
         FROM timetable_slots ts
         JOIN timetable_periods tp ON tp.id = ts.period_id AND tp.status = 'active'
         JOIN classes c ON c.id = ts.class_id
         LEFT JOIN streams st ON st.id = ts.stream_id
         LEFT JOIN subjects s ON s.id = ts.subject_id
         WHERE ts.teacher_id = ?1 AND ts.day <= ?2",
    )?;
    let slots = stmt
        .query_map(params![user_id, days], |row| {
            Ok(TeacherSlot {
                day: row.get(0)?,
                period_id: row.get(1)?,
                class_name: row.get(2)?,
                stream_name: row.get(3)?,
                subject_name: row.get(4)?,
                room: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    Ok(TeacherTimetable {
        user_id: user_id.to_string(),
        teacher_name,
        days,
        periods: load_periods(conn)?,
        slots,
    })
}

#[tauri::command]
pub fn load_teacher_timetable(state: State<'_, AppState>, user_id: String) -> AppResult<TeacherTimetable> {
    let session = state.sessions.require()?;
    if session.user_id != user_id {
        session.require_admin()?;
    }
    let conn = state.db.lock();
    teacher_timetable_of(&conn, &user_id)
}

// ---------------------------------------------------------------------------
// Printed timetables
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimetableDocument {
    pub title: String,
    pub days: i64,
    pub periods: Vec<PeriodRow>,
    /// One grid per page: a class, a stream or a teacher.
    pub grids: Vec<TimetableGrid>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimetableGrid {
    pub heading: String,
    pub subheading: Option<String>,
    pub cells: Vec<TimetableCell>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimetableCell {
    pub day: i64,
    pub period_id: String,
    pub main: String,
    pub sub: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimetableDocumentRequest {
    /// "class", "teacher", "school" (every class) or "teachers" (every teacher).
    pub scope: String,
    #[serde(default)]
    pub class_id: Option<String>,
    #[serde(default)]
    pub stream_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
}

fn class_grid(conn: &Connection, class_id: &str, stream_id: Option<&str>) -> AppResult<TimetableGrid> {
    let table = class_timetable(conn, class_id, stream_id)?;
    let class_teacher: Option<String> = conn
        .query_row(
            "SELECT u.full_name FROM teacher_assignments ta JOIN users u ON u.id = ta.user_id
             WHERE ta.class_id = ?1 AND ta.role = 'class_teacher' AND ta.status = 'active'
               AND ta.stream_id IS ?2 LIMIT 1",
            params![class_id, stream_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(TimetableGrid {
        heading: match &table.stream_name {
            Some(stream) => format!("{} — {stream}", table.class_name),
            None => table.class_name.clone(),
        },
        subheading: class_teacher.map(|name| format!("Class Teacher: {name}")),
        cells: table
            .slots
            .into_iter()
            .map(|slot| TimetableCell {
                day: slot.day,
                period_id: slot.period_id,
                main: slot.subject_name.unwrap_or_default(),
                sub: match (slot.teacher_name, slot.room) {
                    (Some(t), Some(r)) => Some(format!("{t} • {r}")),
                    (Some(t), None) => Some(t),
                    (None, r) => r,
                },
            })
            .collect(),
    })
}

fn teacher_grid(conn: &Connection, user_id: &str) -> AppResult<TimetableGrid> {
    let table = teacher_timetable_of(conn, user_id)?;
    let count = table.slots.len();
    Ok(TimetableGrid {
        heading: table.teacher_name,
        subheading: Some(format!("{count} lesson{} a week", if count == 1 { "" } else { "s" })),
        cells: table
            .slots
            .into_iter()
            .map(|slot| TimetableCell {
                day: slot.day,
                period_id: slot.period_id,
                main: match slot.stream_name {
                    Some(stream) => format!("{} {stream}", slot.class_name),
                    None => slot.class_name,
                },
                sub: match (slot.subject_name, slot.room) {
                    (Some(s), Some(r)) => Some(format!("{s} • {r}")),
                    (s, r) => s.or(r),
                },
            })
            .collect(),
    })
}

#[tauri::command]
pub fn build_timetable_document(
    state: State<'_, AppState>,
    request: TimetableDocumentRequest,
) -> AppResult<DocumentEnvelope<TimetableDocument>> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();

    let (title, grids) = match request.scope.as_str() {
        "class" => {
            let class_id = request.class_id.as_deref().ok_or_else(|| AppError::validation("Pick a class."))?;
            session.require_view_class(class_id)?;
            ("Class Timetable".to_string(), vec![class_grid(&conn, class_id, request.stream_id.as_deref())?])
        }
        "teacher" => {
            let user_id = request.user_id.as_deref().ok_or_else(|| AppError::validation("Pick a teacher."))?;
            if session.user_id != user_id {
                session.require_admin()?;
            }
            ("Teacher's Timetable".to_string(), vec![teacher_grid(&conn, user_id)?])
        }
        "school" => {
            session.require_admin()?;
            let mut stmt = conn.prepare(
                "SELECT DISTINCT ts.class_id, ts.stream_id FROM timetable_slots ts
                 JOIN classes c ON c.id = ts.class_id
                 LEFT JOIN streams st ON st.id = ts.stream_id
                 ORDER BY c.ladder_position, st.name",
            )?;
            let targets = stmt
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(stmt);
            let mut grids = Vec::new();
            for (class_id, stream_id) in targets {
                grids.push(class_grid(&conn, &class_id, stream_id.as_deref())?);
            }
            ("Class Timetables".to_string(), grids)
        }
        "teachers" => {
            session.require_admin()?;
            let mut stmt = conn.prepare(
                "SELECT DISTINCT u.id FROM timetable_slots ts JOIN users u ON u.id = ts.teacher_id
                 ORDER BY u.full_name COLLATE NOCASE",
            )?;
            let ids = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(stmt);
            let mut grids = Vec::new();
            for id in ids {
                grids.push(teacher_grid(&conn, &id)?);
            }
            ("Teachers' Timetables".to_string(), grids)
        }
        _ => return Err(AppError::validation("Unknown kind of timetable.")),
    };
    if grids.is_empty() {
        return Err(AppError::validation("There is nothing on the timetable yet."));
    }

    let document = TimetableDocument {
        title,
        days: school_days(&conn)?,
        periods: load_periods(&conn)?,
        grids,
    };
    envelope(&conn, "timetable", document)
}

// ---------------------------------------------------------------------------
// FR-G7 — the exam timetable
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamPaperRow {
    pub id: String,
    pub exam_id: String,
    pub subject_id: String,
    pub subject_name: String,
    pub paper_label: Option<String>,
    pub on_date: String,
    pub start_time: String,
    pub end_time: String,
    pub venue: Option<String>,
    pub invigilator_id: Option<String>,
    pub invigilator_name: Option<String>,
    pub class_ids: Vec<String>,
    pub class_names: Vec<String>,
}

pub fn exam_papers(conn: &Connection, exam_id: &str) -> AppResult<Vec<ExamPaperRow>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.exam_id, p.subject_id, s.name, p.paper_label, p.on_date, p.start_time, p.end_time,
                p.venue, p.invigilator_id, u.full_name
         FROM exam_papers p
         JOIN subjects s ON s.id = p.subject_id
         LEFT JOIN users u ON u.id = p.invigilator_id
         WHERE p.exam_id = ?1
         ORDER BY p.on_date, p.start_time, s.name",
    )?;
    let mut rows = stmt
        .query_map(params![exam_id], |row| {
            Ok(ExamPaperRow {
                id: row.get(0)?,
                exam_id: row.get(1)?,
                subject_id: row.get(2)?,
                subject_name: row.get(3)?,
                paper_label: row.get(4)?,
                on_date: row.get(5)?,
                start_time: row.get(6)?,
                end_time: row.get(7)?,
                venue: row.get(8)?,
                invigilator_id: row.get(9)?,
                invigilator_name: row.get(10)?,
                class_ids: Vec::new(),
                class_names: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut stmt = conn.prepare(
        "SELECT pc.class_id, c.name FROM exam_paper_classes pc
         JOIN classes c ON c.id = pc.class_id
         WHERE pc.paper_id = ?1 ORDER BY c.ladder_position",
    )?;
    for row in &mut rows {
        let classes = stmt
            .query_map(params![row.id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, name) in classes {
            row.class_ids.push(id);
            row.class_names.push(name);
        }
    }
    Ok(rows)
}

#[tauri::command]
pub fn list_exam_papers(state: State<'_, AppState>, exam_id: String) -> AppResult<Vec<ExamPaperRow>> {
    state.sessions.require()?;
    let conn = state.db.lock();
    exam_papers(&conn, &exam_id)
}

/// The exam's date follows its first paper, so permits and the calendar agree.
fn sync_exam_date(conn: &Connection, exam_id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE exams SET scheduled_date = (SELECT MIN(on_date) FROM exam_papers WHERE exam_id = ?1),
                          updated_at = ?2
         WHERE id = ?1 AND EXISTS (SELECT 1 FROM exam_papers WHERE exam_id = ?1)",
        params![exam_id, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveExamPaperRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub exam_id: String,
    pub subject_id: String,
    #[serde(default)]
    pub paper_label: Option<String>,
    pub on_date: String,
    pub start_time: String,
    pub end_time: String,
    #[serde(default)]
    pub venue: Option<String>,
    #[serde(default)]
    pub invigilator_id: Option<String>,
    pub class_ids: Vec<String>,
}

fn overlaps(a_start: &str, a_end: &str, b_start: &str, b_end: &str) -> bool {
    a_start < b_end && b_start < a_end
}

/// Checks a paper against every other paper on the same day. Returns the
/// first clash in words.
fn paper_clash(
    conn: &Connection,
    id: Option<&str>,
    on_date: &str,
    start: &str,
    end: &str,
    class_ids: &[String],
    invigilator: Option<&str>,
) -> AppResult<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.start_time, p.end_time, s.name, p.invigilator_id
         FROM exam_papers p JOIN subjects s ON s.id = p.subject_id
         WHERE p.on_date = ?1",
    )?;
    let same_day = stmt
        .query_map(params![on_date], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    for (other_id, other_start, other_end, subject, other_invigilator) in same_day {
        if Some(other_id.as_str()) == id || !overlaps(start, end, &other_start, &other_end) {
            continue;
        }
        let shared: Option<String> = {
            let mut stmt = conn.prepare(
                "SELECT pc.class_id, c.name FROM exam_paper_classes pc
                 JOIN classes c ON c.id = pc.class_id
                 WHERE pc.paper_id = ?1",
            )?;
            let sitting = stmt
                .query_map(params![other_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            sitting
                .into_iter()
                .find(|(class_id, _)| class_ids.contains(class_id))
                .map(|(_, name)| name)
        };
        if let Some(class_name) = shared {
            return Ok(Some(format!(
                "{class_name} is already sitting {subject} from {other_start} to {other_end} that day."
            )));
        }
        if invigilator.is_some() && other_invigilator.as_deref() == invigilator {
            return Ok(Some(format!(
                "That invigilator is already supervising {subject} from {other_start} to {other_end} that day."
            )));
        }
    }
    Ok(None)
}

#[tauri::command]
pub fn save_exam_paper(state: State<'_, AppState>, request: SaveExamPaperRequest) -> AppResult<String> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    let date = NaiveDate::parse_from_str(request.on_date.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::validation("Pick the paper's date."))?;
    let on_date = date.format("%Y-%m-%d").to_string();
    let start = check_time(&request.start_time)?;
    let end = check_time(&request.end_time)?;
    if end <= start {
        return Err(AppError::validation("A paper must end after it starts."));
    }
    if request.class_ids.is_empty() {
        return Err(AppError::validation("Choose at least one class to sit this paper."));
    }

    let conn = state.db.lock();
    if let Some(clash) = paper_clash(
        &conn,
        request.id.as_deref(),
        &on_date,
        &start,
        &end,
        &request.class_ids,
        request.invigilator_id.as_deref(),
    )? {
        return Err(AppError::conflict(clash));
    }

    let now = Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    let id = request.id.clone().unwrap_or_else(|| new_id("epp"));
    tx.execute(
        "INSERT INTO exam_papers (id, exam_id, subject_id, paper_label, on_date, start_time, end_time, venue, invigilator_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
         ON CONFLICT (id) DO UPDATE SET subject_id = excluded.subject_id, paper_label = excluded.paper_label,
            on_date = excluded.on_date, start_time = excluded.start_time, end_time = excluded.end_time,
            venue = excluded.venue, invigilator_id = excluded.invigilator_id, updated_at = excluded.updated_at",
        params![
            id,
            request.exam_id,
            request.subject_id,
            request.paper_label.as_deref().map(str::trim).filter(|l| !l.is_empty()),
            on_date,
            start,
            end,
            request.venue.as_deref().map(str::trim).filter(|v| !v.is_empty()),
            request.invigilator_id,
            now,
        ],
    )?;
    tx.execute("DELETE FROM exam_paper_classes WHERE paper_id = ?1", params![id])?;
    for class_id in &request.class_ids {
        tx.execute(
            "INSERT INTO exam_paper_classes (paper_id, class_id) VALUES (?1, ?2)",
            params![id, class_id],
        )?;
    }
    sync_exam_date(&tx, &request.exam_id)?;
    audit::record(&tx, Some(&session), "exam_paper.save", "exam", &request.exam_id, format!("Scheduled a paper on {on_date} {start}-{end}"))?;
    tx.commit()?;
    Ok(id)
}

#[tauri::command]
pub fn delete_exam_paper(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    let conn = state.db.lock();
    let exam_id: String = conn
        .query_row("SELECT exam_id FROM exam_papers WHERE id = ?1", params![id], |row| row.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That paper"))?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM exam_paper_classes WHERE paper_id = ?1", params![id])?;
    tx.execute("DELETE FROM exam_papers WHERE id = ?1", params![id])?;
    sync_exam_date(&tx, &exam_id)?;
    audit::record(&tx, Some(&session), "exam_paper.delete", "exam", &exam_id, "Removed a paper from the exam timetable")?;
    tx.commit()?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamSession {
    pub start_time: String,
    pub end_time: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoExamRequest {
    pub exam_id: String,
    pub start_date: String,
    /// The day's sittings, e.g. 09:00–11:00 and 11:30–13:30.
    pub sessions: Vec<ExamSession>,
    #[serde(default = "yes")]
    pub skip_weekends: bool,
    /// Only these classes; empty means every class.
    #[serde(default)]
    pub class_ids: Vec<String>,
    /// Remove the exam's existing papers first.
    #[serde(default)]
    pub replace: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoExamResult {
    pub created: usize,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
}

/// One paper per subject, sat together by every class that takes it, placed
/// into the day's sittings from the start date on. Core subjects go first.
#[tauri::command]
pub fn auto_generate_exam_timetable(state: State<'_, AppState>, request: AutoExamRequest) -> AppResult<AutoExamResult> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    if request.sessions.is_empty() || request.sessions.len() > 4 {
        return Err(AppError::validation("Choose 1 to 4 sittings a day."));
    }
    let mut sittings = Vec::new();
    for sitting in &request.sessions {
        let start = check_time(&sitting.start_time)?;
        let end = check_time(&sitting.end_time)?;
        if end <= start {
            return Err(AppError::validation("Each sitting must end after it starts."));
        }
        sittings.push((start, end));
    }
    sittings.sort();
    let mut date = NaiveDate::parse_from_str(request.start_date.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::validation("Pick the first day of the exams."))?;

    let conn = state.db.lock();
    let tx = conn.unchecked_transaction()?;
    if request.replace {
        tx.execute(
            "DELETE FROM exam_paper_classes WHERE paper_id IN (SELECT id FROM exam_papers WHERE exam_id = ?1)",
            params![request.exam_id],
        )?;
        tx.execute("DELETE FROM exam_papers WHERE exam_id = ?1", params![request.exam_id])?;
    }

    // Each subject and the classes that take it, core subjects first.
    let mut subjects: Vec<(String, bool, String, Vec<String>)> = Vec::new();
    {
        let mut stmt = tx.prepare(
            "SELECT cs.subject_id, MAX(cs.is_core), s.name, GROUP_CONCAT(cs.class_id)
             FROM class_subjects cs
             JOIN subjects s ON s.id = cs.subject_id
             JOIN classes c ON c.id = cs.class_id AND c.status = 'active'
             WHERE cs.status = 'active'
               AND NOT EXISTS (SELECT 1 FROM exam_papers p WHERE p.exam_id = ?1 AND p.subject_id = cs.subject_id)
             GROUP BY cs.subject_id",
        )?;
        let rows = stmt.query_map(params![request.exam_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? != 0,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (subject_id, core, name, classes) = row?;
            let classes: Vec<String> = classes
                .split(',')
                .map(String::from)
                .filter(|c| request.class_ids.is_empty() || request.class_ids.contains(c))
                .collect();
            if !classes.is_empty() {
                subjects.push((subject_id, core, name, classes));
            }
        }
    }
    subjects.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));

    let now = Utc::now().to_rfc3339();
    let mut result = AutoExamResult {
        created: 0,
        first_date: None,
        last_date: None,
    };
    let mut sitting_index = 0usize;
    for (subject_id, _, _, classes) in subjects {
        loop {
            if request.skip_weekends {
                while matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
                    date = date.succ_opt().ok_or_else(|| AppError::internal("date overflow"))?;
                }
            }
            let (start, end) = &sittings[sitting_index];
            let day = date.format("%Y-%m-%d").to_string();
            let clash = paper_clash(&tx, None, &day, start, end, &classes, None)?;
            let placed_here = clash.is_none();
            if placed_here {
                let id = new_id("epp");
                tx.execute(
                    "INSERT INTO exam_papers (id, exam_id, subject_id, on_date, start_time, end_time, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                    params![id, request.exam_id, subject_id, day, start, end, now],
                )?;
                for class_id in &classes {
                    tx.execute(
                        "INSERT INTO exam_paper_classes (paper_id, class_id) VALUES (?1, ?2)",
                        params![id, class_id],
                    )?;
                }
                result.created += 1;
                if result.first_date.is_none() {
                    result.first_date = Some(day.clone());
                }
                result.last_date = Some(day);
            }
            sitting_index += 1;
            if sitting_index == sittings.len() {
                sitting_index = 0;
                date = date.succ_opt().ok_or_else(|| AppError::internal("date overflow"))?;
            }
            if placed_here {
                break;
            }
        }
    }

    sync_exam_date(&tx, &request.exam_id)?;
    audit::record(
        &tx,
        Some(&session),
        "exam_paper.generate",
        "exam",
        &request.exam_id,
        format!("Generated {} papers for the exam timetable", result.created),
    )?;
    tx.commit()?;
    Ok(result)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamTimetableDocument {
    pub exam_name: String,
    pub term_name: String,
    pub academic_year: String,
    /// Set when the timetable is for one class only.
    pub class_name: Option<String>,
    pub days: Vec<ExamDay>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamDay {
    pub date: String,
    /// "Monday 16 November"
    pub label: String,
    pub papers: Vec<ExamPaperRow>,
}

#[tauri::command]
pub fn build_exam_timetable(
    state: State<'_, AppState>,
    exam_id: String,
    class_id: Option<String>,
) -> AppResult<DocumentEnvelope<ExamTimetableDocument>> {
    let session = state.sessions.require()?;
    if let Some(class) = &class_id {
        session.require_view_class(class)?;
    }
    let conn = state.db.lock();
    let (exam_name, term_name, academic_year): (String, String, String) = conn
        .query_row(
            "SELECT e.name, t.name, y.label FROM exams e
             JOIN terms t ON t.id = e.term_id JOIN academic_years y ON y.id = t.academic_year_id
             WHERE e.id = ?1",
            params![exam_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That examination"))?;
    let class_name: Option<String> = match &class_id {
        Some(id) => conn.query_row("SELECT name FROM classes WHERE id = ?1", params![id], |row| row.get(0)).optional()?,
        None => None,
    };

    let papers: Vec<ExamPaperRow> = exam_papers(&conn, &exam_id)?
        .into_iter()
        .filter(|paper| class_id.as_ref().is_none_or(|c| paper.class_ids.contains(c)))
        .collect();
    if papers.is_empty() {
        return Err(AppError::validation("That examination has no papers on its timetable yet."));
    }

    let mut days: Vec<ExamDay> = Vec::new();
    for paper in papers {
        if days.last().is_none_or(|day| day.date != paper.on_date) {
            let label = NaiveDate::parse_from_str(&paper.on_date, "%Y-%m-%d")
                .map(|d| d.format("%A %-d %B").to_string())
                .unwrap_or_else(|_| paper.on_date.clone());
            days.push(ExamDay {
                date: paper.on_date.clone(),
                label,
                papers: Vec::new(),
            });
        }
        if let Some(day) = days.last_mut() {
            day.papers.push(paper);
        }
    }

    envelope(
        &conn,
        "exam_timetable",
        ExamTimetableDocument {
            exam_name,
            term_name,
            academic_year,
            class_name,
            days,
        },
    )
}

/// For exam permits: when each subject is sat by a class, e.g. "Mon 16 Nov, 09:00–11:00".
pub fn paper_times_for_class(conn: &Connection, exam_id: &str, class_id: &str) -> AppResult<HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT s.name, p.on_date, p.start_time, p.end_time
         FROM exam_papers p
         JOIN exam_paper_classes pc ON pc.paper_id = p.id AND pc.class_id = ?2
         JOIN subjects s ON s.id = p.subject_id
         WHERE p.exam_id = ?1
         ORDER BY p.on_date, p.start_time",
    )?;
    let rows = stmt.query_map(params![exam_id, class_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (subject, date, start, end) = row?;
        let day = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
            .map(|d| d.format("%a %-d %b").to_string())
            .unwrap_or(date);
        map.entry(subject).or_insert(format!("{day}, {start}–{end}"));
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_times_are_caught_and_back_to_back_is_fine() {
        assert!(overlaps("09:00", "11:00", "10:00", "12:00"));
        assert!(overlaps("09:00", "11:00", "09:00", "11:00"));
        assert!(!overlaps("09:00", "11:00", "11:00", "13:00"));
        assert!(!overlaps("11:30", "13:30", "09:00", "11:00"));
    }

    #[test]
    fn times_are_normalised_and_bad_ones_refused() {
        assert_eq!(check_time("8:05").unwrap(), "08:05");
        assert!(check_time("25:00").is_err());
        assert!(check_time("noon").is_err());
    }
}
