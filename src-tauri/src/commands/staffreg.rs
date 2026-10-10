//! The staff sign-in register: who came to school, and when.
//!
//! Teachers sign in at the office computer with their name and a short PIN,
//! on the sign-in screen, without signing in to Phantom. The sign-in station
//! shows names and nothing else, so it can be left open on the desk in the
//! morning. A second tap signs out. The headteacher sees who is in, who came
//! late and, at the end of the month, each person's record.
//!
//! The commands marked "station" need no session: that is the point of the
//! station. They are switched off until the school turns the register on,
//! and a PIN guessed wrongly five times locks that person's PIN for fifteen
//! minutes.

use chrono::{Duration, Local, NaiveDate, NaiveTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::domain::ids::new_id;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::password::{self, Secret};
use crate::security::{Office, Permission, Role, Session};
use crate::state::AppState;

const MAX_PIN_TRIES: i64 = 5;
const PIN_LOCK_MINUTES: i64 = 15;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffRegisterSettings {
    pub enabled: bool,
    /// "07:30": signing in after this is late.
    pub start_time: String,
}

pub fn load_settings(conn: &Connection) -> AppResult<StaffRegisterSettings> {
    let start = repo::get_setting(conn, "staff_register.start_time")?.unwrap_or_default();
    Ok(StaffRegisterSettings {
        enabled: repo::get_setting(conn, "staff_register.enabled")?.as_deref() == Some("1"),
        start_time: if start.is_empty() { "07:30".into() } else { start },
    })
}

#[tauri::command]
pub fn get_staff_register_settings(state: State<'_, AppState>) -> AppResult<StaffRegisterSettings> {
    state.sessions.require()?;
    load_settings(&state.db.lock())
}

#[tauri::command]
pub fn save_staff_register_settings(state: State<'_, AppState>, settings: StaffRegisterSettings) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageStaff)?;
    NaiveTime::parse_from_str(&settings.start_time, "%H:%M").map_err(|_| AppError::validation("Choose the time."))?;
    let conn = state.db.lock();
    repo::set_setting(&conn, "staff_register.enabled", if settings.enabled { "1" } else { "0" })?;
    repo::set_setting(&conn, "staff_register.start_time", &settings.start_time)?;
    audit::record(
        &conn,
        Some(&session),
        "staff_register.settings",
        "setting",
        "staff_register",
        if settings.enabled {
            format!("Staff sign-in register on; late after {}", settings.start_time)
        } else {
            "Staff sign-in register off".to_string()
        },
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// PINs
// ---------------------------------------------------------------------------

fn valid_pin(pin: &str) -> AppResult<()> {
    if !(4..=6).contains(&pin.len()) || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::validation("A PIN is 4 to 6 digits."));
    }
    let first = pin.chars().next().unwrap_or('0');
    if pin.chars().all(|c| c == first) || "0123456789".contains(pin) || "9876543210".contains(pin) {
        return Err(AppError::validation("Choose a PIN that is harder to guess than that."));
    }
    Ok(())
}

pub fn set_pin_in(conn: &Connection, user_id: &str, pin: &str) -> AppResult<()> {
    valid_pin(pin)?;
    let hash = password::hash(&Secret::new(pin))?;
    conn.execute(
        "UPDATE users SET pin_hash = ?1, pin_failed_attempts = 0, pin_locked_until = NULL, updated_at = ?2 WHERE id = ?3",
        params![hash, Utc::now().to_rfc3339(), user_id],
    )?;
    Ok(())
}

/// Each person sets their own PIN from My account.
#[tauri::command]
pub fn set_my_sign_in_pin(state: State<'_, AppState>, pin: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    set_pin_in(&conn, &session.user_id, &pin)?;
    audit::record(&conn, Some(&session), "staff_register.pin", "user", &session.user_id, "Set their sign-in PIN")?;
    Ok(())
}

/// The office clears a forgotten PIN; the person sets a new one when they
/// next sign in to Phantom.
#[tauri::command]
pub fn clear_staff_pin(state: State<'_, AppState>, user_id: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let (name, role, office): (String, String, Option<String>) = conn
        .query_row("SELECT full_name, role, office FROM users WHERE id = ?1", params![user_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()?
        .ok_or_else(|| AppError::not_found("That person"))?;
    session.require_act_on(Role::parse(&role)?, office.as_deref().map(Office::parse).transpose()?)?;
    conn.execute(
        "UPDATE users SET pin_hash = NULL, pin_failed_attempts = 0, pin_locked_until = NULL WHERE id = ?1",
        params![user_id],
    )?;
    audit::record(&conn, Some(&session), "staff_register.pin_clear", "user", &user_id, format!("Cleared {name}'s sign-in PIN"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The sign-in station (no session)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationPerson {
    pub user_id: String,
    pub full_name: String,
    pub has_pin: bool,
    /// HH:MM, or none when not yet in today.
    pub signed_in_at: Option<String>,
    pub signed_out_at: Option<String>,
}

/// Names and today's state only: what the station shows to anyone at the desk.
pub fn station_people(conn: &Connection, today: &str) -> AppResult<Vec<StationPerson>> {
    if !load_settings(conn)?.enabled {
        return Err(AppError::validation("The staff sign-in register is switched off."));
    }
    let mut stmt = conn.prepare(
        "SELECT u.id, u.full_name, u.pin_hash IS NOT NULL, a.signed_in_at, a.signed_out_at
         FROM users u
         LEFT JOIN staff_attendance a ON a.user_id = u.id AND a.on_date = ?1
         WHERE u.status = 'active'
         ORDER BY u.full_name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map(params![today], |r| {
            Ok(StationPerson {
                user_id: r.get(0)?,
                full_name: r.get(1)?,
                has_pin: r.get(2)?,
                signed_in_at: r.get(3)?,
                signed_out_at: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[tauri::command]
pub fn station_staff(state: State<'_, AppState>) -> AppResult<Vec<StationPerson>> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    station_people(&state.db.lock(), &today)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationResult {
    pub full_name: String,
    /// `in` or `out`.
    pub action: String,
    pub time: String,
    pub late: bool,
}

pub fn station_sign_in(conn: &Connection, user_id: &str, pin: &str, now: chrono::NaiveDateTime) -> AppResult<StationResult> {
    let settings = load_settings(conn)?;
    if !settings.enabled {
        return Err(AppError::validation("The staff sign-in register is switched off."));
    }
    let (name, hash, tries, locked_until): (String, Option<String>, i64, Option<String>) = conn
        .query_row(
            "SELECT full_name, pin_hash, pin_failed_attempts, pin_locked_until FROM users
             WHERE id = ?1 AND status = 'active'",
            params![user_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That person"))?;
    let Some(hash) = hash else {
        return Err(AppError::validation(format!(
            "{name} has no PIN yet. Sign in to Phantom once and set it under My account, or ask the office to record you."
        )));
    };
    let utc_now = Utc::now();
    if let Some(until) = locked_until.as_deref().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()) {
        if until > utc_now {
            return Err(AppError::validation("Too many wrong PINs. Try again in a few minutes, or ask the office."));
        }
    }
    if !password::verify(&Secret::new(pin), &hash) {
        let tries = tries + 1;
        let lock = (tries >= MAX_PIN_TRIES).then(|| (utc_now + Duration::minutes(PIN_LOCK_MINUTES)).to_rfc3339());
        conn.execute(
            "UPDATE users SET pin_failed_attempts = ?1, pin_locked_until = ?2 WHERE id = ?3",
            params![if lock.is_some() { 0 } else { tries }, lock, user_id],
        )?;
        if lock.is_some() {
            audit::record_system(conn, "staff_register.pin_locked", "user", user_id, format!("{name}'s sign-in PIN locked after {MAX_PIN_TRIES} wrong tries"))?;
        }
        return Err(AppError::validation("That PIN is not right."));
    }
    conn.execute("UPDATE users SET pin_failed_attempts = 0, pin_locked_until = NULL WHERE id = ?1", params![user_id])?;

    let today = now.date().format("%Y-%m-%d").to_string();
    let time = now.time().format("%H:%M").to_string();
    let start = NaiveTime::parse_from_str(&settings.start_time, "%H:%M").unwrap_or(NaiveTime::from_hms_opt(7, 30, 0).expect("valid"));
    let existing: Option<(String, Option<String>, bool)> = conn
        .query_row(
            "SELECT id, signed_out_at, late FROM staff_attendance WHERE user_id = ?1 AND on_date = ?2",
            params![user_id, today],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let stamp = utc_now.to_rfc3339();
    let (action, late) = match existing {
        None => {
            let late = now.time() > start;
            conn.execute(
                "INSERT INTO staff_attendance (id, user_id, on_date, signed_in_at, late, method, recorded_by, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'pin', ?2, ?6, ?6)",
                params![new_id("sat"), user_id, today, time, late, stamp],
            )?;
            ("in", late)
        }
        Some((id, None, late)) => {
            conn.execute(
                "UPDATE staff_attendance SET signed_out_at = ?1, updated_at = ?2 WHERE id = ?3",
                params![time, stamp, id],
            )?;
            ("out", late)
        }
        Some((_, Some(out), _)) => {
            return Err(AppError::conflict(format!("{name} already signed out today at {out}.")));
        }
    };
    audit::record_system(
        conn,
        &format!("staff_register.sign_{action}"),
        "user",
        user_id,
        format!("{name} signed {action} at {time}{}", if action == "in" && late { " (late)" } else { "" }),
    )?;
    Ok(StationResult { full_name: name, action: action.into(), time, late })
}

#[tauri::command]
pub fn station_sign(state: State<'_, AppState>, user_id: String, pin: String) -> AppResult<StationResult> {
    let conn = state.db.lock();
    station_sign_in(&conn, &user_id, &pin, Local::now().naive_local())
}

// ---------------------------------------------------------------------------
// The office: today, entering for someone, the month
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRow {
    pub user_id: String,
    pub full_name: String,
    pub office: Option<String>,
    pub signed_in_at: Option<String>,
    pub signed_out_at: Option<String>,
    pub late: bool,
    pub method: Option<String>,
    pub note: Option<String>,
}

#[tauri::command]
pub fn staff_register_day(state: State<'_, AppState>, on_date: String) -> AppResult<Vec<RegisterRow>> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageStaff)?;
    NaiveDate::parse_from_str(&on_date, "%Y-%m-%d").map_err(|_| AppError::validation("Choose a date."))?;
    let conn = state.db.lock();
    let mut stmt = conn.prepare(
        "SELECT u.id, u.full_name, u.office, a.signed_in_at, a.signed_out_at, COALESCE(a.late, 0), a.method, a.note
         FROM users u LEFT JOIN staff_attendance a ON a.user_id = u.id AND a.on_date = ?1
         WHERE u.status = 'active'
         ORDER BY a.signed_in_at IS NULL, a.signed_in_at, u.full_name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map(params![on_date], |r| {
            Ok(RegisterRow {
                user_id: r.get(0)?,
                full_name: r.get(1)?,
                office: r.get(2)?,
                signed_in_at: r.get(3)?,
                signed_out_at: r.get(4)?,
                late: r.get(5)?,
                method: r.get(6)?,
                note: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeEntry {
    pub user_id: String,
    pub on_date: String,
    pub signed_in_at: String,
    pub signed_out_at: Option<String>,
    /// Why the office entered it: required, it is on the record.
    pub note: String,
}

pub fn office_entry_in(conn: &Connection, session: &Session, entry: &OfficeEntry) -> AppResult<()> {
    session.require(Permission::ManageStaff)?;
    let date = NaiveDate::parse_from_str(&entry.on_date, "%Y-%m-%d").map_err(|_| AppError::validation("Choose a date."))?;
    if date > Local::now().date_naive() {
        return Err(AppError::validation("Nobody can be signed in for a day that has not come."));
    }
    let time_in = NaiveTime::parse_from_str(&entry.signed_in_at, "%H:%M").map_err(|_| AppError::validation("Choose the time in."))?;
    if let Some(out) = &entry.signed_out_at {
        let out = NaiveTime::parse_from_str(out, "%H:%M").map_err(|_| AppError::validation("Choose the time out."))?;
        if out <= time_in {
            return Err(AppError::validation("The time out must be after the time in."));
        }
    }
    let note = entry.note.trim();
    if note.is_empty() {
        return Err(AppError::validation("Say why the office is entering this (e.g. forgot PIN)."));
    }
    let start = NaiveTime::parse_from_str(&load_settings(conn)?.start_time, "%H:%M").unwrap_or(NaiveTime::from_hms_opt(7, 30, 0).expect("valid"));
    let name: String = conn
        .query_row("SELECT full_name FROM users WHERE id = ?1", params![entry.user_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That person"))?;
    let stamp = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO staff_attendance (id, user_id, on_date, signed_in_at, signed_out_at, late, method, note, recorded_by, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'office', ?7, ?8, ?9, ?9)
         ON CONFLICT (user_id, on_date) DO UPDATE SET
             signed_in_at = excluded.signed_in_at, signed_out_at = excluded.signed_out_at, late = excluded.late,
             method = 'office', note = excluded.note, recorded_by = excluded.recorded_by, updated_at = excluded.updated_at",
        params![
            new_id("sat"),
            entry.user_id,
            entry.on_date,
            entry.signed_in_at,
            entry.signed_out_at,
            time_in > start,
            note,
            session.user_id,
            stamp
        ],
    )?;
    audit::record(
        conn,
        Some(session),
        "staff_register.office",
        "user",
        &entry.user_id,
        format!("Entered {name} in the staff register for {}: in {}{} ({note})", entry.on_date, entry.signed_in_at,
                entry.signed_out_at.as_ref().map(|o| format!(", out {o}")).unwrap_or_default()),
    )?;
    Ok(())
}

#[tauri::command]
pub fn record_staff_attendance(state: State<'_, AppState>, entry: OfficeEntry) -> AppResult<()> {
    let session = state.sessions.require()?;
    office_entry_in(&state.db.lock(), &session, &entry)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthRow {
    pub user_id: String,
    pub full_name: String,
    pub office: Option<String>,
    pub days_in: i64,
    pub days_late: i64,
    /// Mean time in, "07:42".
    pub average_in: Option<String>,
    /// One entry per school day of the month: "07:25", "L 08:10" (late), or "".
    pub marks: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffMonth {
    pub month: String,
    pub label: String,
    /// The days anybody signed in, as day-of-month numbers.
    pub days: Vec<u32>,
    pub start_time: String,
    pub rows: Vec<MonthRow>,
}

pub fn staff_month(conn: &Connection, month: &str) -> AppResult<StaffMonth> {
    let first = NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").map_err(|_| AppError::validation("Choose a month."))?;
    let label = first.format("%B %Y").to_string();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT on_date FROM staff_attendance WHERE substr(on_date, 1, 7) = ?1 ORDER BY on_date",
    )?;
    let dates: Vec<String> = stmt.query_map(params![month], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    let days: Vec<u32> = dates.iter().filter_map(|d| d[8..10].parse().ok()).collect();

    let mut stmt = conn.prepare(
        "SELECT u.id, u.full_name, u.office FROM users u
         WHERE u.status = 'active' OR EXISTS (SELECT 1 FROM staff_attendance a WHERE a.user_id = u.id AND substr(a.on_date, 1, 7) = ?1)
         ORDER BY u.full_name COLLATE NOCASE",
    )?;
    let people: Vec<(String, String, Option<String>)> =
        stmt.query_map(params![month], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;

    let mut stmt = conn.prepare(
        "SELECT user_id, on_date, signed_in_at, late FROM staff_attendance WHERE substr(on_date, 1, 7) = ?1",
    )?;
    let entries: Vec<(String, String, String, bool)> =
        stmt.query_map(params![month], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;

    let rows = people
        .into_iter()
        .map(|(id, name, office)| {
            let mine: Vec<&(String, String, String, bool)> = entries.iter().filter(|e| e.0 == id).collect();
            let minutes: Vec<i64> = mine
                .iter()
                .filter_map(|e| NaiveTime::parse_from_str(&e.2, "%H:%M").ok())
                .map(|t| (t - NaiveTime::from_hms_opt(0, 0, 0).expect("midnight")).num_minutes())
                .collect();
            let average_in = (!minutes.is_empty()).then(|| {
                let m = minutes.iter().sum::<i64>() / minutes.len() as i64;
                format!("{:02}:{:02}", m / 60, m % 60)
            });
            let marks = dates
                .iter()
                .map(|d| match mine.iter().find(|e| &e.1 == d) {
                    Some(e) if e.3 => format!("L {}", e.2),
                    Some(e) => e.2.clone(),
                    None => String::new(),
                })
                .collect();
            MonthRow {
                user_id: id,
                full_name: name,
                office,
                days_in: mine.len() as i64,
                days_late: mine.iter().filter(|e| e.3).count() as i64,
                average_in,
                marks,
            }
        })
        .collect();
    Ok(StaffMonth { month: month.into(), label, days, start_time: load_settings(conn)?.start_time, rows })
}

#[tauri::command]
pub fn staff_register_month(state: State<'_, AppState>, month: String) -> AppResult<StaffMonth> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageStaff)?;
    staff_month(&state.db.lock(), &month)
}

#[tauri::command]
pub fn build_staff_register(
    state: State<'_, AppState>,
    month: String,
) -> AppResult<crate::commands::reports::DocumentEnvelope<StaffMonth>> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageStaff)?;
    let conn = state.db.lock();
    let body = staff_month(&conn, &month)?;
    crate::commands::reports::envelope(&conn, "staff_register", body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fees::tests::fee_school;
    use crate::security::session::test_session;

    fn at(s: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn on(conn: &Connection) {
        repo::set_setting(conn, "staff_register.enabled", "1").unwrap();
        repo::set_setting(conn, "staff_register.start_time", "07:30").unwrap();
    }

    #[test]
    fn the_station_is_shut_until_the_school_opens_it() {
        let db = fee_school();
        let conn = db.lock();
        assert!(station_people(&conn, "2026-10-12").is_err());
        assert!(station_sign_in(&conn, "bursar", "4826", at("2026-10-12 07:10")).is_err());
    }

    #[test]
    fn in_then_out_late_is_marked_and_a_third_tap_is_refused() {
        let db = fee_school();
        let conn = db.lock();
        on(&conn);
        set_pin_in(&conn, "bursar", "4826").unwrap();

        let first = station_sign_in(&conn, "bursar", "4826", at("2026-10-12 07:41")).unwrap();
        assert_eq!((first.action.as_str(), first.time.as_str(), first.late), ("in", "07:41", true));
        let out = station_sign_in(&conn, "bursar", "4826", at("2026-10-12 17:05")).unwrap();
        assert_eq!(out.action, "out");
        assert!(station_sign_in(&conn, "bursar", "4826", at("2026-10-12 17:06")).is_err());

        let people = station_people(&conn, "2026-10-12").unwrap();
        let bursar = people.iter().find(|p| p.user_id == "bursar").unwrap();
        assert_eq!((bursar.signed_in_at.as_deref(), bursar.signed_out_at.as_deref()), (Some("07:41"), Some("17:05")));

        let month = staff_month(&conn, "2026-10").unwrap();
        let row = month.rows.iter().find(|r| r.user_id == "bursar").unwrap();
        assert_eq!((row.days_in, row.days_late, row.average_in.as_deref()), (1, 1, Some("07:41")));
        assert_eq!(row.marks, vec!["L 07:41".to_string()]);
    }

    #[test]
    fn five_wrong_pins_lock_the_pin_and_easy_pins_are_refused() {
        let db = fee_school();
        let conn = db.lock();
        on(&conn);
        assert!(set_pin_in(&conn, "head", "1234").is_err());
        assert!(set_pin_in(&conn, "head", "0000").is_err());
        assert!(set_pin_in(&conn, "head", "12a4").is_err());
        set_pin_in(&conn, "head", "5917").unwrap();
        for _ in 0..MAX_PIN_TRIES {
            let wrong = station_sign_in(&conn, "head", "1111", at("2026-10-12 07:00")).unwrap_err();
            assert!(wrong.user_message().contains("not right"));
        }
        let locked = station_sign_in(&conn, "head", "5917", at("2026-10-12 07:00")).unwrap_err();
        assert!(locked.user_message().contains("Too many"), "{}", locked.user_message());
    }

    #[test]
    fn the_office_can_enter_someone_with_a_reason_and_only_with_staff_rights() {
        let db = fee_school();
        let conn = db.lock();
        on(&conn);
        let head = test_session("head", Role::Teacher, Some(Office::HeadTeacher), &[]);
        let bursar = test_session("bursar", Role::Teacher, Some(Office::Bursar), &[]);
        let entry = OfficeEntry {
            user_id: "bursar".into(),
            on_date: "2026-10-09".into(),
            signed_in_at: "07:15".into(),
            signed_out_at: Some("16:30".into()),
            note: "Forgot PIN".into(),
        };
        assert!(matches!(office_entry_in(&conn, &bursar, &entry), Err(AppError::Forbidden)));
        office_entry_in(&conn, &head, &entry).unwrap();
        let blank = OfficeEntry { note: " ".into(), ..entry.clone() };
        assert!(office_entry_in(&conn, &head, &blank).is_err());
        let (method, late): (String, bool) = conn
            .query_row("SELECT method, late FROM staff_attendance WHERE user_id = 'bursar'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((method.as_str(), late), ("office", false));
    }
}
