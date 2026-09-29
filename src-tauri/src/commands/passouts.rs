//! FR-G22 — a learner leaving school during the day.
//!
//! The Class Teacher or School Admin records who is leaving, why, where to,
//! with whom and for how long. RM prints a slip for the gate and texts the
//! guardian (FR-G8) so a parent always knows where their child is. When the
//! learner comes back they are marked returned, and the guardian gets a second
//! text. A learner still out after their expected time shows up on the
//! dashboard instead of going unnoticed.
//!
//! Also here: the SMS settings and outbox screens.

use chrono::{DateTime, Local, SecondsFormat, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::audit;
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};
use crate::sms::{self, FlushResult, SmsSettings};
use crate::state::AppState;

const ID_PREFIX: &str = "pas";

fn now_utc() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn local_time(rfc3339: &str) -> String {
    DateTime::parse_from_rfc3339(rfc3339)
        .map(|t| t.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// "45 min", "2 hrs", "1 hr 30 min".
pub fn duration_words(minutes: i64) -> String {
    let (hours, mins) = (minutes / 60, minutes % 60);
    let hours_text = match hours {
        0 => String::new(),
        1 => "1 hr".into(),
        n => format!("{n} hrs"),
    };
    match (hours, mins) {
        (0, m) => format!("{m} min"),
        (_, 0) => hours_text,
        (_, m) => format!("{hours_text} {m} min"),
    }
}

pub fn reason_label(kind: &str) -> &'static str {
    match kind {
        "sick" => "Sick",
        "appointment" => "Appointment",
        "family" => "Family matter",
        "permission" => "Permission",
        _ => "Other",
    }
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassOutRow {
    pub id: String,
    pub number: i64,
    pub student_id: String,
    pub student_name: String,
    pub reg_number: String,
    pub class_name: String,
    pub reason_kind: String,
    pub reason: Option<String>,
    pub destination: Option<String>,
    pub picked_up_by: Option<String>,
    pub picked_up_relationship: Option<String>,
    pub picked_up_phone: Option<String>,
    pub time_out: String,
    pub expected_back: Option<String>,
    pub returned_at: Option<String>,
    pub status: String,
    pub guardian_phone: Option<String>,
    /// queued / sent / failed for the "has left" text; none when not texted.
    pub sms_status: Option<String>,
    pub sms_error: Option<String>,
    pub return_sms_status: Option<String>,
    pub issued_by_name: Option<String>,
    /// Still out after the time they were expected back.
    pub overdue: bool,
}

const ROW_SELECT: &str = "SELECT p.id, p.number, p.student_id, s.full_name, s.reg_number, c.name,
        p.reason_kind, p.reason, p.destination, p.picked_up_by, p.picked_up_relationship,
        p.picked_up_phone, p.time_out, p.expected_back, p.returned_at, p.status,
        p.guardian_phone, o1.status, o1.last_error, o2.status, u.full_name
 FROM pass_outs p
 JOIN students s ON s.id = p.student_id
 JOIN classes c ON c.id = p.class_id
 LEFT JOIN sms_outbox o1 ON o1.id = p.sms_out_id
 LEFT JOIN sms_outbox o2 ON o2.id = p.sms_return_id
 LEFT JOIN users u ON u.id = p.issued_by";

fn row(r: &rusqlite::Row<'_>, now: &str) -> rusqlite::Result<PassOutRow> {
    let status: String = r.get(15)?;
    let expected_back: Option<String> = r.get(13)?;
    Ok(PassOutRow {
        id: r.get(0)?,
        number: r.get(1)?,
        student_id: r.get(2)?,
        student_name: r.get(3)?,
        reg_number: r.get(4)?,
        class_name: r.get(5)?,
        reason_kind: r.get(6)?,
        reason: r.get(7)?,
        destination: r.get(8)?,
        picked_up_by: r.get(9)?,
        picked_up_relationship: r.get(10)?,
        picked_up_phone: r.get(11)?,
        time_out: r.get(12)?,
        overdue: status == "out" && expected_back.as_deref().is_some_and(|t| t < now),
        expected_back,
        returned_at: r.get(14)?,
        status,
        guardian_phone: r.get(16)?,
        sms_status: r.get(17)?,
        sms_error: r.get(18)?,
        return_sms_status: r.get(19)?,
        issued_by_name: r.get(20)?,
    })
}

fn load_row(conn: &Connection, id: &str) -> AppResult<PassOutRow> {
    let now = now_utc();
    conn.query_row(&format!("{ROW_SELECT} WHERE p.id = ?1"), params![id], |r| row(r, &now))
        .optional()?
        .ok_or_else(|| AppError::not_found("That pass-out"))
}

/// Start of today, local time, in the stored UTC form.
fn start_of_today() -> String {
    let midnight = Local::now().date_naive().and_hms_opt(0, 0, 0).unwrap_or_default();
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|t| t.with_timezone(&Utc).to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_else(now_utc)
}

/// Everyone out right now, plus everything issued today. Teachers see their
/// own classes only.
#[tauri::command]
pub fn list_pass_outs(state: State<'_, AppState>) -> AppResult<Vec<PassOutRow>> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let now = now_utc();

    let mut stmt = conn.prepare(&format!(
        "{ROW_SELECT}
         WHERE p.status = 'out' OR p.time_out >= ?1
         ORDER BY (p.status = 'out') DESC, p.time_out DESC
         LIMIT 200"
    ))?;
    let rows: Vec<PassOutRow> = stmt
        .query_map(params![start_of_today()], |r| row(r, &now))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    if session.is_admin() {
        return Ok(rows);
    }
    // A teacher sees the classes they manage.
    let mut visible = Vec::new();
    for entry in rows {
        let class_id: String = conn.query_row(
            "SELECT class_id FROM pass_outs WHERE id = ?1",
            params![entry.id],
            |r| r.get(0),
        )?;
        if session.require_manage_class(&class_id).is_ok() {
            visible.push(entry);
        }
    }
    Ok(visible)
}

// ---------------------------------------------------------------------------
// Issue a pass-out
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePassOutRequest {
    pub student_id: String,
    pub reason_kind: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub picked_up_by: Option<String>,
    #[serde(default)]
    pub picked_up_relationship: Option<String>,
    #[serde(default)]
    pub picked_up_phone: Option<String>,
    /// How long they will be away. `None` means not returning today.
    #[serde(default)]
    pub away_minutes: Option<i64>,
    /// Text the guardian now, and again when the learner returns.
    pub notify_guardian: bool,
    /// The number to text. Blank uses the guardian phone on file.
    #[serde(default)]
    pub guardian_phone: Option<String>,
}

fn clean(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(String::from)
}

/// The "has left school" text.
pub fn leaving_message(
    signature: &str,
    name: &str,
    time_out: &str,
    request: &CreatePassOutRequest,
    expected_back: Option<&str>,
) -> String {
    let mut text = format!("{signature}: {name} left school at {}", local_time(time_out));
    if let Some(by) = clean(&request.picked_up_by) {
        text.push_str(&format!(" with {by}"));
        if let Some(rel) = clean(&request.picked_up_relationship) {
            text.push_str(&format!(" ({rel})"));
        }
    }
    text.push_str(&format!(". Reason: {}", reason_label(&request.reason_kind)));
    if let Some(reason) = clean(&request.reason) {
        text.push_str(&format!(" - {reason}"));
    }
    text.push('.');
    if let Some(place) = clean(&request.destination) {
        text.push_str(&format!(" Going to: {place}."));
    }
    match (expected_back, request.away_minutes) {
        (Some(back), Some(minutes)) => text.push_str(&format!(
            " Away for about {}, expected back at {}.",
            duration_words(minutes),
            local_time(back)
        )),
        _ => text.push_str(" Not returning today."),
    }
    text
}

#[tauri::command]
pub fn create_pass_out(
    app: AppHandle,
    state: State<'_, AppState>,
    request: CreatePassOutRequest,
) -> AppResult<PassOutRow> {
    let session = state.sessions.require()?;

    if !matches!(
        request.reason_kind.as_str(),
        "sick" | "appointment" | "family" | "permission" | "other"
    ) {
        return Err(AppError::validation("Pick a reason."));
    }
    if let Some(minutes) = request.away_minutes {
        if !(5..=24 * 60).contains(&minutes) {
            return Err(AppError::validation(
                "The time away must be between 5 minutes and a day.",
            ));
        }
    }

    let conn = state.db.lock();

    let (full_name, class_id, guardian_on_file): (String, String, Option<String>) = conn
        .query_row(
            "SELECT s.full_name, e.class_id, s.guardian_phone
             FROM students s
             JOIN enrollments e ON e.student_id = s.id AND e.status = 'active'
             WHERE s.id = ?1
             LIMIT 1",
            params![request.student_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::validation("That learner is not enrolled in a class."))?;
    session.require_manage_class(&class_id)?;

    let already_out: Option<i64> = conn
        .query_row(
            "SELECT number FROM pass_outs WHERE student_id = ?1 AND status = 'out'",
            params![request.student_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(number) = already_out {
        return Err(AppError::conflict(format!(
            "{full_name} is already out on pass-out PO-{number:04}. Mark them returned first."
        )));
    }

    let guardian_phone = clean(&request.guardian_phone).or(guardian_on_file);
    if request.notify_guardian {
        match guardian_phone.as_deref() {
            None => {
                return Err(AppError::validation(
                    "There is no guardian phone number to text. Add one, or untick the text message.",
                ))
            }
            Some(phone) if sms::normalize_phone(phone).is_none() => {
                return Err(AppError::validation(format!(
                    "\"{phone}\" is not a mobile number that can receive an SMS."
                )))
            }
            _ => {}
        }
    }

    let now = Utc::now();
    let time_out = now.to_rfc3339_opts(SecondsFormat::Secs, true);
    let expected_back = request.away_minutes.map(|m| {
        (now + chrono::Duration::minutes(m)).to_rfc3339_opts(SecondsFormat::Secs, true)
    });

    let id = new_id(ID_PREFIX);
    let tx = conn.unchecked_transaction()?;
    let number: i64 = tx.query_row(
        "SELECT COALESCE(MAX(number), 0) + 1 FROM pass_outs",
        [],
        |r| r.get(0),
    )?;

    let sms_id = if request.notify_guardian {
        let body = leaving_message(
            &sms::signature(&tx)?,
            &full_name,
            &time_out,
            &request,
            expected_back.as_deref(),
        );
        sms::queue(
            &tx,
            guardian_phone.as_deref(),
            &body,
            "pass_out",
            &id,
            Some(&session.user_id),
        )?
    } else {
        None
    };

    tx.execute(
        "INSERT INTO pass_outs
            (id, number, student_id, class_id, reason_kind, reason, destination,
             picked_up_by, picked_up_relationship, picked_up_phone, time_out, expected_back,
             status, guardian_phone, sms_out_id, issued_by, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'out', ?13, ?14, ?15, ?11, ?11)",
        params![
            id,
            number,
            request.student_id,
            class_id,
            request.reason_kind,
            clean(&request.reason),
            clean(&request.destination),
            clean(&request.picked_up_by),
            clean(&request.picked_up_relationship),
            clean(&request.picked_up_phone),
            time_out,
            expected_back,
            guardian_phone,
            sms_id,
            session.user_id,
        ],
    )?;

    audit::record(
        &tx,
        Some(&session),
        "pass_out.issue",
        "student",
        &request.student_id,
        format!(
            "Issued pass-out PO-{number:04} to {full_name} ({}){}",
            reason_label(&request.reason_kind),
            if sms_id.is_some() { ", guardian texted" } else { "" }
        ),
    )?;
    tx.commit()?;

    let created = load_row(&conn, &id)?;
    drop(conn);

    if sms_id.is_some() {
        sms::flush_in_background(app);
    }
    Ok(created)
}

// ---------------------------------------------------------------------------
// Back at school
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn mark_pass_out_returned(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    notify_guardian: bool,
) -> AppResult<PassOutRow> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();

    let (class_id, status, student_name, number, guardian_phone, texted_out): (
        String,
        String,
        String,
        i64,
        Option<String>,
        bool,
    ) = conn
        .query_row(
            "SELECT p.class_id, p.status, s.full_name, p.number, p.guardian_phone,
                    p.sms_out_id IS NOT NULL
             FROM pass_outs p JOIN students s ON s.id = p.student_id
             WHERE p.id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That pass-out"))?;
    session.require_manage_class(&class_id)?;

    if status != "out" {
        return Err(AppError::conflict(format!(
            "{student_name} is already marked as returned."
        )));
    }

    let now = now_utc();
    let tx = conn.unchecked_transaction()?;
    // Only text a guardian who was told the learner left.
    let sms_id = if notify_guardian && texted_out {
        let body = format!(
            "{}: {student_name} is back at school. Returned at {}.",
            sms::signature(&tx)?,
            local_time(&now)
        );
        sms::queue(
            &tx,
            guardian_phone.as_deref(),
            &body,
            "pass_out_return",
            &id,
            Some(&session.user_id),
        )?
    } else {
        None
    };

    tx.execute(
        "UPDATE pass_outs
         SET status = 'returned', returned_at = ?1, returned_by = ?2, sms_return_id = ?3, updated_at = ?1
         WHERE id = ?4",
        params![now, session.user_id, sms_id, id],
    )?;
    audit::record(
        &tx,
        Some(&session),
        "pass_out.return",
        "pass_out",
        &id,
        format!("{student_name} returned (PO-{number:04})"),
    )?;
    tx.commit()?;

    let updated = load_row(&conn, &id)?;
    drop(conn);

    if sms_id.is_some() {
        sms::flush_in_background(app);
    }
    Ok(updated)
}

// ---------------------------------------------------------------------------
// Dashboard flag
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverduePassOut {
    pub id: String,
    pub student_name: String,
    pub class_name: String,
    pub expected_back: String,
}

/// Learners still out after their expected return time.
pub fn overdue(conn: &Connection) -> AppResult<Vec<OverduePassOut>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, s.full_name, c.name, p.expected_back
         FROM pass_outs p
         JOIN students s ON s.id = p.student_id
         JOIN classes c ON c.id = p.class_id
         WHERE p.status = 'out' AND p.expected_back IS NOT NULL AND p.expected_back < ?1
         ORDER BY p.expected_back ASC",
    )?;
    let rows = stmt
        .query_map(params![now_utc()], |r| {
            Ok(OverduePassOut {
                id: r.get(0)?,
                student_name: r.get(1)?,
                class_name: r.get(2)?,
                expected_back: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// SMS settings and outbox (FR-G8)
// ---------------------------------------------------------------------------

const MASK: &str = "••••••••";

#[tauri::command]
pub fn get_sms_settings(state: State<'_, AppState>) -> AppResult<SmsSettings> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    let conn = state.db.lock();
    let mut settings = sms::load_settings(&conn)?;
    // The key never travels back to the screen.
    if !settings.api_key.is_empty() {
        settings.api_key = MASK.into();
    }
    Ok(settings)
}

#[tauri::command]
pub fn save_sms_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: SmsSettings,
) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    let conn = state.db.lock();

    let mut settings = settings;
    if settings.api_key == MASK {
        settings.api_key = sms::load_settings(&conn)?.api_key;
    }
    sms::save_settings(&conn, &settings)?;
    audit::record(
        &conn,
        Some(&session),
        "sms.settings",
        "setting",
        "sms",
        format!("Set the SMS provider to {}", settings.provider),
    )?;
    drop(conn);

    // Anything waiting for SMS to be set up can go now.
    sms::flush_in_background(app);
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmsOutboxRow {
    pub id: String,
    pub to_phone: String,
    pub body: String,
    pub kind: String,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub created_at: String,
    pub sent_at: Option<String>,
}

#[tauri::command]
pub fn list_sms_outbox(state: State<'_, AppState>) -> AppResult<Vec<SmsOutboxRow>> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    let conn = state.db.lock();
    let mut stmt = conn.prepare(
        "SELECT id, to_phone, body, kind, status, attempts, last_error, created_at, sent_at
         FROM sms_outbox ORDER BY created_at DESC LIMIT 100",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SmsOutboxRow {
                id: r.get(0)?,
                to_phone: r.get(1)?,
                body: r.get(2)?,
                kind: r.get(3)?,
                status: r.get(4)?,
                attempts: r.get(5)?,
                last_error: r.get(6)?,
                created_at: r.get(7)?,
                sent_at: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Sends everything waiting now. Async so the network wait never freezes
/// the window.
#[tauri::command]
pub async fn send_queued_sms(state: State<'_, AppState>) -> AppResult<FlushResult> {
    let session = state.sessions.require()?;
    session.require_admin()?;
    {
        // Give failed messages another chance when someone asks explicitly.
        let conn = state.db.lock();
        conn.execute(
            "UPDATE sms_outbox SET status = 'queued', attempts = 0 WHERE status = 'failed'",
            [],
        )?;
    }
    sms::flush(&state)
}

/// Sends one test message straight away and says exactly what happened.
#[tauri::command]
pub async fn send_test_sms(state: State<'_, AppState>, phone: String) -> AppResult<String> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let to = sms::normalize_phone(&phone).ok_or_else(|| {
        AppError::validation(format!("\"{phone}\" is not a mobile number that can receive an SMS."))
    })?;
    let (settings, body) = {
        let conn = state.db.lock();
        let settings = sms::load_settings(&conn)?;
        let body = format!(
            "{}: this is a test message from Phantom School Manager. SMS is working.",
            sms::signature(&conn)?
        );
        (settings, body)
    };
    if settings.provider == "off" {
        return Err(AppError::validation("Pick an SMS provider and save first."));
    }

    let outcome = sms::send(&settings, &to, &body);
    let conn = state.db.lock();
    let now = Utc::now().to_rfc3339();
    let (status, error) = match &outcome {
        Ok(_) => ("sent", None),
        Err(sms::SendError::Offline(e)) | Err(sms::SendError::Rejected(e)) => ("failed", Some(e.clone())),
    };
    conn.execute(
        "INSERT INTO sms_outbox (id, to_phone, body, kind, status, attempts, last_error, created_by, created_at, sent_at)
         VALUES (?1, ?2, ?3, 'test', ?4, 1, ?5, ?6, ?7, ?8)",
        params![
            new_id(sms::ID_PREFIX),
            to,
            body,
            status,
            error,
            session.user_id,
            now,
            (status == "sent").then_some(now.clone()),
        ],
    )?;

    match outcome {
        Ok(_) => Ok(format!("Test message sent to {to}.")),
        Err(sms::SendError::Offline(e)) | Err(sms::SendError::Rejected(e)) => Err(AppError::validation(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration_words(45), "45 min");
        assert_eq!(duration_words(60), "1 hr");
        assert_eq!(duration_words(90), "1 hr 30 min");
        assert_eq!(duration_words(180), "3 hrs");
    }

    #[test]
    fn the_leaving_text_says_where_and_for_how_long() {
        let request = CreatePassOutRequest {
            student_id: "stu_1".into(),
            reason_kind: "sick".into(),
            reason: Some("Fever".into()),
            destination: Some("Mulago Hospital".into()),
            picked_up_by: Some("John Okello".into()),
            picked_up_relationship: Some("Father".into()),
            picked_up_phone: None,
            away_minutes: Some(150),
            notify_guardian: true,
            guardian_phone: None,
        };
        let text = leaving_message(
            "RAINBOW N&P",
            "Brian Okello",
            "2026-09-29T07:30:00Z",
            &request,
            Some("2026-09-29T10:00:00Z"),
        );
        assert!(text.starts_with("RAINBOW N&P: Brian Okello left school at "));
        assert!(text.contains("with John Okello (Father)"));
        assert!(text.contains("Reason: Sick - Fever."));
        assert!(text.contains("Going to: Mulago Hospital."));
        assert!(text.contains("Away for about 2 hrs 30 min, expected back at "));
    }

    #[test]
    fn not_returning_today_is_said_plainly() {
        let request = CreatePassOutRequest {
            student_id: "stu_1".into(),
            reason_kind: "family".into(),
            reason: None,
            destination: None,
            picked_up_by: None,
            picked_up_relationship: None,
            picked_up_phone: None,
            away_minutes: None,
            notify_guardian: true,
            guardian_phone: None,
        };
        let text = leaving_message("RNPS", "Ruth", "2026-09-29T07:30:00Z", &request, None);
        assert!(text.ends_with("Reason: Family matter. Not returning today."));
    }
}
