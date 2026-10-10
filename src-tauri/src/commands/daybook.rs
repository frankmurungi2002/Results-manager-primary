//! The Day Book: what happened at school each day, kept for good.
//!
//! An owner or headteacher opens it on the office computer and sees any day,
//! week or month: money received, who was in school, who left at the gate,
//! texts sent, and the head's own note for the day. Every figure is worked
//! out from the records (receipts, registers, pass-outs, the outbox), so the
//! book can never disagree with them, and nothing needs "closing" each
//! evening for it to be right.
//!
//! An owner who wants it on their phone can switch on an evening SMS. It is
//! off by default: a text a day is a cost a school chooses, not one it finds.

use chrono::{Local, NaiveDate, NaiveTime};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::fees::ugx;
use crate::security::{Permission, Session};
use crate::sms;
use crate::state::AppState;

fn may_read(session: &Session) -> AppResult<bool> {
    let money = session.has(Permission::ViewFinance);
    if money || session.has(Permission::ViewAllClasses) {
        Ok(money)
    } else {
        Err(AppError::Forbidden)
    }
}

fn parse_date(text: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|_| AppError::validation("Choose a date."))
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayRow {
    pub date: String,
    /// `None` when the reader may not see money.
    pub fees_collected: Option<i64>,
    pub fees_cash: Option<i64>,
    pub receipts: Option<i64>,
    pub receipts_cancelled: Option<i64>,
    /// Spending vouchers written that day (money readers only).
    pub spent: Option<i64>,
    /// Classes whose register was marked that day.
    pub registers: i64,
    pub present: i64,
    pub absent: i64,
    pub late: i64,
    /// Present and late, out of everyone marked; `None` when nobody was marked.
    pub attendance_percent: Option<f64>,
    pub pass_outs: i64,
    pub texts_sent: i64,
    pub texts_failed: i64,
    pub learners_admitted: i64,
    pub learners_left: i64,
    pub staff_signed_in: i64,
    pub staff_late: i64,
    /// How many things were recorded in the activity log.
    pub activity: i64,
    pub note: Option<String>,
    pub note_by: Option<String>,
}

impl DayRow {
    /// A day on which nothing at all was recorded (a weekend, a holiday).
    pub fn is_quiet(&self) -> bool {
        self.fees_collected.unwrap_or(0) == 0
            && self.spent.unwrap_or(0) == 0
            && self.registers == 0
            && self.pass_outs == 0
            && self.texts_sent == 0
            && self.learners_admitted == 0
            && self.learners_left == 0
            && self.staff_signed_in == 0
            && self.note.is_none()
    }
}

type DayMap = std::collections::BTreeMap<String, DayRow>;

/// Runs one grouped-by-day query over the range and applies each row to its day.
fn each(
    conn: &Connection,
    from: &str,
    to: &str,
    days: &mut DayMap,
    sql: &str,
    apply: &mut dyn FnMut(&mut DayRow, &rusqlite::Row<'_>) -> rusqlite::Result<()>,
) -> AppResult<()> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params![from, to])?;
    while let Some(row) = rows.next()? {
        let date: String = row.get(0)?;
        if let Some(day) = days.get_mut(&date) {
            apply(day, row)?;
        }
    }
    Ok(())
}

/// Every day from `from` to `to`, newest first.
pub fn day_rows(conn: &Connection, from: NaiveDate, to: NaiveDate, money: bool) -> AppResult<Vec<DayRow>> {
    if to < from {
        return Err(AppError::validation("The end date is before the start."));
    }
    if (to - from).num_days() > 400 {
        return Err(AppError::validation("Choose at most about a year at a time."));
    }
    let (a, b) = (from.format("%Y-%m-%d").to_string(), to.format("%Y-%m-%d").to_string());
    let mut days: DayMap = DayMap::new();
    let mut d = from;
    while d <= to {
        let key = d.format("%Y-%m-%d").to_string();
        days.insert(key.clone(), DayRow { date: key, ..Default::default() });
        d = d.succ_opt().expect("a date after this one");
    }

    if money {
        each(conn, &a, &b, &mut days,
            "SELECT paid_on, SUM(CASE WHEN status = 'valid' THEN amount ELSE 0 END),
                    SUM(CASE WHEN status = 'valid' AND method = 'cash' THEN amount ELSE 0 END),
                    SUM(status = 'valid'), SUM(status = 'voided')
             FROM fee_payments WHERE paid_on BETWEEN ?1 AND ?2 GROUP BY paid_on",
            &mut |day, r| {
                day.fees_collected = Some(r.get(1)?);
                day.fees_cash = Some(r.get(2)?);
                day.receipts = Some(r.get(3)?);
                day.receipts_cancelled = Some(r.get(4)?);
                Ok(())
            },
        )?;
        each(conn, &a, &b, &mut days,
            "SELECT on_date, SUM(amount) FROM cash_entries
             WHERE kind = 'expense' AND status = 'valid' AND on_date BETWEEN ?1 AND ?2 GROUP BY on_date",
            &mut |day, r| {
                day.spent = Some(r.get(1)?);
                Ok(())
            },
        )?;
        for day in days.values_mut() {
            day.spent.get_or_insert(0);
            day.fees_collected.get_or_insert(0);
            day.fees_cash.get_or_insert(0);
            day.receipts.get_or_insert(0);
            day.receipts_cancelled.get_or_insert(0);
        }
    }
    each(conn, &a, &b, &mut days,
        "SELECT on_date, COUNT(DISTINCT class_id), SUM(state = 'present'), SUM(state = 'absent'), SUM(state = 'late'),
                COUNT(*)
         FROM attendance WHERE on_date BETWEEN ?1 AND ?2 GROUP BY on_date",
        &mut |day, r| {
            day.registers = r.get(1)?;
            day.present = r.get(2)?;
            day.absent = r.get(3)?;
            day.late = r.get(4)?;
            let marked: i64 = r.get(5)?;
            day.attendance_percent =
                (marked > 0).then(|| ((day.present + day.late) as f64 * 1000.0 / marked as f64).round() / 10.0);
            Ok(())
        },
    )?;
    each(conn, &a, &b, &mut days,
        "SELECT date(time_out, 'localtime') AS d, COUNT(*) FROM pass_outs
         WHERE date(time_out, 'localtime') BETWEEN ?1 AND ?2 GROUP BY d",
        &mut |day, r| {
            day.pass_outs = r.get(1)?;
            Ok(())
        },
    )?;
    each(conn, &a, &b, &mut days,
        "SELECT date(COALESCE(sent_at, last_attempt_at, created_at), 'localtime') AS d,
                SUM(status = 'sent'), SUM(status = 'failed')
         FROM sms_outbox
         WHERE date(COALESCE(sent_at, last_attempt_at, created_at), 'localtime') BETWEEN ?1 AND ?2 GROUP BY d",
        &mut |day, r| {
            day.texts_sent = r.get(1)?;
            day.texts_failed = r.get(2)?;
            Ok(())
        },
    )?;
    each(conn, &a, &b, &mut days,
        "SELECT date(created_at, 'localtime') AS d, COUNT(*) FROM students
         WHERE date(created_at, 'localtime') BETWEEN ?1 AND ?2 GROUP BY d",
        &mut |day, r| {
            day.learners_admitted = r.get(1)?;
            Ok(())
        },
    )?;
    each(conn, &a, &b, &mut days,
        "SELECT substr(left_at, 1, 10) AS d, COUNT(*) FROM enrollments
         WHERE status = 'dropped' AND substr(left_at, 1, 10) BETWEEN ?1 AND ?2 GROUP BY d",
        &mut |day, r| {
            day.learners_left = r.get(1)?;
            Ok(())
        },
    )?;
    each(conn, &a, &b, &mut days,
        "SELECT on_date, COUNT(*), SUM(late) FROM staff_attendance
         WHERE on_date BETWEEN ?1 AND ?2 GROUP BY on_date",
        &mut |day, r| {
            day.staff_signed_in = r.get(1)?;
            day.staff_late = r.get(2)?;
            Ok(())
        },
    )?;
    each(conn, &a, &b, &mut days,
        "SELECT date(at, 'localtime') AS d, COUNT(*) FROM audit_log
         WHERE date(at, 'localtime') BETWEEN ?1 AND ?2 GROUP BY d",
        &mut |day, r| {
            day.activity = r.get(1)?;
            Ok(())
        },
    )?;
    each(conn, &a, &b, &mut days,
        "SELECT n.on_date, n.note, u.full_name FROM day_notes n LEFT JOIN users u ON u.id = n.updated_by
         WHERE n.on_date BETWEEN ?1 AND ?2 AND n.note != ''",
        &mut |day, r| {
            day.note = Some(r.get(1)?);
            day.note_by = r.get(2)?;
            Ok(())
        },
    )?;

    Ok(days.into_values().rev().collect())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayBook {
    pub from: String,
    pub to: String,
    /// Whether money figures are included for this reader.
    pub money: bool,
    pub days: Vec<DayRow>,
    pub totals: DayRow,
}

pub fn day_book(conn: &Connection, from: NaiveDate, to: NaiveDate, money: bool) -> AppResult<DayBook> {
    let days = day_rows(conn, from, to, money)?;
    let mut totals = DayRow { date: "Total".into(), ..Default::default() };
    let (mut present, mut late, mut marked) = (0, 0, 0);
    for day in &days {
        if money {
            *totals.fees_collected.get_or_insert(0) += day.fees_collected.unwrap_or(0);
            *totals.fees_cash.get_or_insert(0) += day.fees_cash.unwrap_or(0);
            *totals.receipts.get_or_insert(0) += day.receipts.unwrap_or(0);
            *totals.receipts_cancelled.get_or_insert(0) += day.receipts_cancelled.unwrap_or(0);
            *totals.spent.get_or_insert(0) += day.spent.unwrap_or(0);
        }
        totals.registers += day.registers;
        totals.absent += day.absent;
        totals.pass_outs += day.pass_outs;
        totals.texts_sent += day.texts_sent;
        totals.texts_failed += day.texts_failed;
        totals.learners_admitted += day.learners_admitted;
        totals.learners_left += day.learners_left;
        totals.staff_signed_in += day.staff_signed_in;
        totals.staff_late += day.staff_late;
        totals.activity += day.activity;
        present += day.present;
        late += day.late;
        marked += day.present + day.late + day.absent;
    }
    totals.present = present;
    totals.late = late;
    totals.attendance_percent = (marked > 0).then(|| ((present + late) as f64 * 1000.0 / marked as f64).round() / 10.0);
    Ok(DayBook { from: from.to_string(), to: to.to_string(), money, days, totals })
}

#[tauri::command]
pub fn get_day_book(state: State<'_, AppState>, from: String, to: String) -> AppResult<DayBook> {
    let session = state.sessions.require()?;
    let money = may_read(&session)?;
    day_book(&state.db.lock(), parse_date(&from)?, parse_date(&to)?, money)
}

#[tauri::command]
pub fn build_day_book(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> AppResult<crate::commands::reports::DocumentEnvelope<DayBook>> {
    let session = state.sessions.require()?;
    let money = may_read(&session)?;
    let conn = state.db.lock();
    let mut book = day_book(&conn, parse_date(&from)?, parse_date(&to)?, money)?;
    book.days.retain(|d| !d.is_quiet());
    crate::commands::reports::envelope(&conn, "day_book", book)
}

/// The headteacher's note for a day. Anyone who may read the book may write
/// it; every change is in the activity log with who wrote it.
#[tauri::command]
pub fn save_day_note(state: State<'_, AppState>, on_date: String, note: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    may_read(&session)?;
    let date = parse_date(&on_date)?;
    if date > Local::now().date_naive() {
        return Err(AppError::validation("Write the note on the day, or after it."));
    }
    let note = note.trim();
    if note.len() > 2000 {
        return Err(AppError::validation("Keep the note under 2,000 characters."));
    }
    let conn = state.db.lock();
    let stamp = chrono::Utc::now().to_rfc3339();
    if note.is_empty() {
        conn.execute("UPDATE day_notes SET note = '', updated_by = ?1, updated_at = ?2 WHERE on_date = ?3",
                     params![session.user_id, stamp, on_date])?;
    } else {
        conn.execute(
            "INSERT INTO day_notes (on_date, note, updated_by, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (on_date) DO UPDATE SET note = excluded.note, updated_by = excluded.updated_by,
                                                 updated_at = excluded.updated_at",
            params![on_date, note, session.user_id, stamp],
        )?;
    }
    audit::record(&conn, Some(&session), "daybook.note", "day", &on_date, format!("Wrote the Day Book note for {on_date}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The owner's evening SMS (off unless the school switches it on)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerSmsSettings {
    pub enabled: bool,
    /// Up to three numbers: the director, the head.
    pub phones: Vec<String>,
    /// "18:30", the school's local time.
    pub send_at: String,
    /// Include fees figures (the owner may not want the bursar's numbers on a
    /// shared phone).
    pub include_money: bool,
}

pub fn load_owner_sms(conn: &Connection) -> AppResult<OwnerSmsSettings> {
    let get = |k: &str| -> AppResult<String> { Ok(repo::get_setting(conn, k)?.unwrap_or_default()) };
    let send_at = get("daybook.sms_at")?;
    Ok(OwnerSmsSettings {
        enabled: get("daybook.sms_enabled")? == "1",
        phones: get("daybook.sms_phones")?.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect(),
        send_at: if send_at.is_empty() { "18:30".into() } else { send_at },
        include_money: get("daybook.sms_money")? != "0",
    })
}

#[tauri::command]
pub fn get_owner_sms(state: State<'_, AppState>) -> AppResult<OwnerSmsSettings> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageSchool)?;
    load_owner_sms(&state.db.lock())
}

#[tauri::command]
pub fn save_owner_sms(state: State<'_, AppState>, settings: OwnerSmsSettings) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageSchool)?;
    if settings.phones.len() > 3 {
        return Err(AppError::validation("At most three numbers: each one costs a text every day."));
    }
    let mut phones = Vec::new();
    for phone in &settings.phones {
        phones.push(sms::normalize_phone(phone).ok_or_else(|| {
            AppError::validation(format!("\"{phone}\" is not a mobile number that can receive an SMS."))
        })?);
    }
    if settings.enabled && phones.is_empty() {
        return Err(AppError::validation("Add the number to text."));
    }
    NaiveTime::parse_from_str(&settings.send_at, "%H:%M").map_err(|_| AppError::validation("Choose the time."))?;
    let conn = state.db.lock();
    if settings.enabled {
        crate::network::require_online_allowed(&conn)?;
    }
    repo::set_setting(&conn, "daybook.sms_enabled", if settings.enabled { "1" } else { "0" })?;
    repo::set_setting(&conn, "daybook.sms_phones", &phones.join(","))?;
    repo::set_setting(&conn, "daybook.sms_at", &settings.send_at)?;
    repo::set_setting(&conn, "daybook.sms_money", if settings.include_money { "1" } else { "0" })?;
    audit::record(
        &conn,
        Some(&session),
        "daybook.sms",
        "setting",
        "daybook",
        if settings.enabled {
            format!("The day's summary will be texted to {} at {}", phones.join(", "), settings.send_at)
        } else {
            "Stopped texting the day's summary".to_string()
        },
    )?;
    Ok(())
}

/// "Rainbow N&P, Thu 8 Oct: Fees UGX 1,450,000 (12 receipts; cash 600,000).
/// Attendance 94%, 21 absent. 3 pass-outs. 41 texts."
pub fn summary_text(signature: &str, day: &DayRow, money: bool) -> String {
    let date = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d")
        .map(|d| d.format("%a %-d %b").to_string())
        .unwrap_or_else(|_| day.date.clone());
    let mut parts = Vec::new();
    if money {
        let receipts = day.receipts.unwrap_or(0);
        parts.push(if receipts == 0 {
            "No fees received".to_string()
        } else {
            format!(
                "Fees {} ({receipts} receipt{}; cash {})",
                ugx(day.fees_collected.unwrap_or(0)),
                if receipts == 1 { "" } else { "s" },
                crate::fees::thousands(day.fees_cash.unwrap_or(0).unsigned_abs())
            )
        });
        if day.receipts_cancelled.unwrap_or(0) > 0 {
            parts.push(format!("{} receipt(s) CANCELLED", day.receipts_cancelled.unwrap_or(0)));
        }
    }
    match day.attendance_percent {
        Some(p) => parts.push(format!("Attendance {p:.0}%, {} absent", day.absent)),
        None => parts.push("No registers marked".into()),
    }
    if day.staff_signed_in > 0 {
        parts.push(format!("{} staff in, {} late", day.staff_signed_in, day.staff_late));
    }
    if day.pass_outs > 0 {
        parts.push(format!("{} pass-out{}", day.pass_outs, if day.pass_outs == 1 { "" } else { "s" }));
    }
    sms::plain_text(&format!("{signature}, {date}: {}.", parts.join(". ")))
}

/// Run by the background jobs every few minutes: queues the day's summary
/// once, after the chosen time, on days something happened. Returns how
/// many texts were queued.
pub fn owner_summary_job(conn: &Connection, now: chrono::NaiveDateTime) -> AppResult<usize> {
    let settings = load_owner_sms(conn)?;
    if !settings.enabled || settings.phones.is_empty() || crate::network::offline_only(conn)? {
        return Ok(0);
    }
    let at = NaiveTime::parse_from_str(&settings.send_at, "%H:%M").unwrap_or(NaiveTime::from_hms_opt(18, 30, 0).expect("valid"));
    let today = now.date();
    if now.time() < at {
        return Ok(0);
    }
    let key = today.format("%Y-%m-%d").to_string();
    if repo::get_setting(conn, "daybook.sms_last")?.as_deref() == Some(key.as_str()) {
        return Ok(0);
    }
    repo::set_setting(conn, "daybook.sms_last", &key)?;
    let Some(day) = day_rows(conn, today, today, settings.include_money)?.pop() else { return Ok(0) };
    // Nothing happened (a Sunday, a holiday): no text, no cost.
    if day.is_quiet() {
        return Ok(0);
    }
    let body = summary_text(&sms::signature(conn)?, &day, settings.include_money);
    let mut queued = 0;
    for phone in &settings.phones {
        if sms::queue(conn, Some(phone), &body, "daybook", &key, None)?.is_some() {
            queued += 1;
        }
    }
    Ok(queued)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fees::tests::fee_school;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn busy_day(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO attendance (id, student_id, class_id, term_id, on_date, state, marked_at) VALUES
                 ('a1', 'amina', 'p5', 't262', '2026-05-26', 'present', 'now'),
                 ('a2', 'musa',  'p5', 't262', '2026-05-26', 'absent',  'now'),
                 ('a3', 'sarah', 'p5', 't262', '2026-05-26', 'late',    'now');
             INSERT INTO fee_payments (id, receipt_no, student_id, term_id, amount, method, reference, paid_on, created_at)
                 VALUES ('p9', 9, 'amina', 't262', 50000, 'mobile_money', 'MM9', '2026-05-26', '2026-05-26T09:00:00Z');
             INSERT INTO day_notes (on_date, note, updated_by, updated_at) VALUES
                 ('2026-05-26', 'District inspector visited P5.', 'head', 'now');",
        )
        .unwrap();
    }

    #[test]
    fn a_day_adds_up_receipts_registers_and_the_heads_note() {
        let db = fee_school();
        let conn = db.lock();
        busy_day(&conn);
        let book = day_book(&conn, date("2026-05-25"), date("2026-05-27"), true).unwrap();
        assert_eq!(book.days.len(), 3);
        assert_eq!(book.days[0].date, "2026-05-27", "newest first");
        let day = &book.days[1];
        assert_eq!(day.fees_collected, Some(150_000), "the fixture's cash 100,000 and this 50,000");
        assert_eq!(day.fees_cash, Some(100_000));
        assert_eq!(day.receipts, Some(2));
        assert_eq!((day.present, day.absent, day.late, day.registers), (1, 1, 1, 1));
        assert_eq!(day.attendance_percent, Some(66.7));
        assert_eq!(day.note.as_deref(), Some("District inspector visited P5."));
        assert_eq!(day.note_by.as_deref(), Some("Mr Head"));
        assert!(book.days[0].is_quiet());
        assert_eq!(book.totals.fees_collected, Some(150_000));
    }

    #[test]
    fn readers_without_finance_see_no_money() {
        let db = fee_school();
        let conn = db.lock();
        busy_day(&conn);
        let book = day_book(&conn, date("2026-05-26"), date("2026-05-26"), false).unwrap();
        assert_eq!(book.days[0].fees_collected, None);
        assert_eq!(book.totals.fees_collected, None);
        assert_eq!(book.days[0].absent, 1);
    }

    #[test]
    fn the_summary_text_fits_one_sms() {
        let db = fee_school();
        let conn = db.lock();
        busy_day(&conn);
        let day = day_rows(&conn, date("2026-05-26"), date("2026-05-26"), true).unwrap().pop().unwrap();
        let text = summary_text("Rainbow N&P", &day, true);
        assert_eq!(text, "Rainbow N&P, Tue 26 May: Fees UGX 150,000 (2 receipts; cash 100,000). Attendance 67%, 1 absent.");
        assert_eq!(sms::segments(&text), 1);
        let without = summary_text("Rainbow N&P", &day, false);
        assert!(!without.contains("UGX"));
    }

    #[test]
    fn the_owner_is_texted_once_after_the_time_and_never_on_a_quiet_day() {
        let db = fee_school();
        let conn = db.lock();
        busy_day(&conn);
        repo::set_setting(&conn, "daybook.sms_enabled", "1").unwrap();
        repo::set_setting(&conn, "daybook.sms_phones", "+256772999999").unwrap();
        repo::set_setting(&conn, "daybook.sms_at", "18:00").unwrap();
        let at = |s: &str| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();

        assert_eq!(owner_summary_job(&conn, at("2026-05-26 17:59")).unwrap(), 0, "too early");
        assert_eq!(owner_summary_job(&conn, at("2026-05-26 18:05")).unwrap(), 1);
        assert_eq!(owner_summary_job(&conn, at("2026-05-26 18:10")).unwrap(), 0, "once a day");
        assert_eq!(owner_summary_job(&conn, at("2026-05-27 18:10")).unwrap(), 0, "nothing happened");

        crate::network::set_offline_only(&conn, true).unwrap();
        repo::set_setting(&conn, "daybook.sms_last", "").unwrap();
        assert_eq!(owner_summary_job(&conn, at("2026-05-26 19:00")).unwrap(), 0, "offline only");
    }
}
