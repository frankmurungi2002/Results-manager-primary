//! Texts to parents that Phantom can send by itself, each switched on by the
//! school or not at all.
//!
//! | Switch          | When                                                | Off by default |
//! |-----------------|-----------------------------------------------------|----------------|
//! | Absence         | A learner is newly marked absent on the register    | yes            |
//! | Results         | The office presses "Text results" for a class       | yes            |
//! | Fee reminders   | Once a week, at the day and time the school chooses | yes            |
//! | Promises        | The morning a parent's promise to pay falls due     | yes            |
//! | Not reported    | Weekday mornings in a term's first two weeks        | yes            |
//!
//! A parent with several children gets one text naming them all. Nothing is
//! sent twice: each kind remembers whom it has texted (in the outbox itself).
//! Everything goes through the outbox, so offline only, priorities and the
//! one-sender rule apply as they do to every other text.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::fees::{ugx, Ledger};
use crate::security::{Permission, Session};
use crate::sms;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoSmsSettings {
    pub absence: bool,
    pub results: bool,
    pub fee_reminders: bool,
    /// 1 = Monday … 7 = Sunday.
    pub fee_reminder_day: u32,
    pub fee_reminder_time: String,
    /// Only parents owing more than this are reminded.
    pub fee_reminder_above: i64,
    pub promise_reminders: bool,
    pub not_reported: bool,
}

fn flag(conn: &Connection, key: &str) -> AppResult<bool> {
    Ok(repo::get_setting(conn, key)?.as_deref() == Some("1"))
}

pub fn load(conn: &Connection) -> AppResult<AutoSmsSettings> {
    let get = |k: &str| -> AppResult<String> { Ok(repo::get_setting(conn, k)?.unwrap_or_default()) };
    let time = get("auto_sms.fee_reminder_time")?;
    Ok(AutoSmsSettings {
        absence: flag(conn, "auto_sms.absence")?,
        results: flag(conn, "auto_sms.results")?,
        fee_reminders: flag(conn, "auto_sms.fee_reminders")?,
        fee_reminder_day: get("auto_sms.fee_reminder_day")?.parse().unwrap_or(1),
        fee_reminder_time: if time.is_empty() { "10:00".into() } else { time },
        fee_reminder_above: get("auto_sms.fee_reminder_above")?.parse().unwrap_or(0),
        promise_reminders: flag(conn, "auto_sms.promise_reminders")?,
        not_reported: flag(conn, "auto_sms.not_reported")?,
    })
}

pub fn absence_on(conn: &Connection) -> AppResult<bool> {
    flag(conn, "auto_sms.absence")
}

#[tauri::command]
pub fn get_auto_sms(state: State<'_, AppState>) -> AppResult<AutoSmsSettings> {
    state.sessions.require()?;
    load(&state.db.lock())
}

#[tauri::command]
pub fn save_auto_sms(state: State<'_, AppState>, settings: AutoSmsSettings) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageSchool)?;
    if !(1..=7).contains(&settings.fee_reminder_day) {
        return Err(AppError::validation("Choose a day of the week."));
    }
    NaiveTime::parse_from_str(&settings.fee_reminder_time, "%H:%M").map_err(|_| AppError::validation("Choose the time."))?;
    if settings.fee_reminder_above < 0 {
        return Err(AppError::validation("The amount must be zero or more."));
    }
    let conn = state.db.lock();
    let any_on = settings.absence || settings.results || settings.fee_reminders || settings.promise_reminders || settings.not_reported;
    if any_on {
        crate::network::require_online_allowed(&conn)?;
    }
    let before = load(&conn)?;
    let set = |k: &str, v: bool| repo::set_setting(&conn, k, if v { "1" } else { "0" });
    set("auto_sms.absence", settings.absence)?;
    set("auto_sms.results", settings.results)?;
    set("auto_sms.fee_reminders", settings.fee_reminders)?;
    set("auto_sms.promise_reminders", settings.promise_reminders)?;
    set("auto_sms.not_reported", settings.not_reported)?;
    repo::set_setting(&conn, "auto_sms.fee_reminder_day", &settings.fee_reminder_day.to_string())?;
    repo::set_setting(&conn, "auto_sms.fee_reminder_time", &settings.fee_reminder_time)?;
    repo::set_setting(&conn, "auto_sms.fee_reminder_above", &settings.fee_reminder_above.to_string())?;
    let mut changes = Vec::new();
    for (name, was, now) in [
        ("absence texts", before.absence, settings.absence),
        ("results texts", before.results, settings.results),
        ("weekly fee reminders", before.fee_reminders, settings.fee_reminders),
        ("promise reminders", before.promise_reminders, settings.promise_reminders),
        ("not-reported texts", before.not_reported, settings.not_reported),
    ] {
        if was != now {
            changes.push(format!("{} {name}", if now { "turned on" } else { "turned off" }));
        }
    }
    audit::record(
        &conn,
        Some(&session),
        "auto_sms.settings",
        "setting",
        "auto_sms",
        if changes.is_empty() { "Updated the automatic texts".to_string() } else { format!("Automatic texts: {}", changes.join(", ")) },
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// One text per parent
// ---------------------------------------------------------------------------

/// Learners grouped by their parent's phone, so a family gets one text.
fn by_phone<T>(items: Vec<(Option<String>, T)>) -> (BTreeMap<String, Vec<T>>, usize) {
    let mut out: BTreeMap<String, Vec<T>> = BTreeMap::new();
    let mut without = 0;
    for (phone, item) in items {
        match phone.as_deref().and_then(sms::normalize_phone) {
            Some(p) => out.entry(p).or_default().push(item),
            None => without += 1,
        }
    }
    (out, without)
}

/// "Amina (P5)", "Amina (P5) and Musa (P3)", "A, B and C".
fn names(list: &[String]) -> String {
    match list.len() {
        0 => String::new(),
        1 => list[0].clone(),
        n => format!("{} and {}", list[..n - 1].join(", "), list[n - 1]),
    }
}

/// Whether a text of this kind already went to this phone, about this, since `since`.
fn already(conn: &Connection, kind: &str, phone: &str, related: &str, since: &str) -> AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sms_outbox WHERE kind = ?1 AND to_phone = ?2 AND related_id = ?3
                         AND created_at >= ?4 AND status != 'failed')",
        params![kind, phone, related, since],
        |r| r.get(0),
    )?)
}

/// Whether any text of this kind went to this phone after `since`.
fn recently(conn: &Connection, kind: &str, phone: &str, since: &str) -> AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sms_outbox WHERE kind = ?1 AND to_phone = ?2 AND created_at >= ?3 AND status != 'failed')",
        params![kind, phone, since],
        |r| r.get(0),
    )?)
}

// ---------------------------------------------------------------------------
// Fee reminders
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPlan {
    pub recipients: usize,
    pub learners: usize,
    pub without_phone: usize,
    pub sms_units: usize,
    pub sample: Option<String>,
    /// Already texted recently, so left out.
    pub skipped_recent: usize,
}

struct Planned {
    phone: String,
    body: String,
    related: String,
}

/// `related` names the round ("week:2026-W23" for the weekly job), so a family
/// is reminded once per round; `since` also skips anyone reminded after it.
fn plan_fee_reminders(conn: &Connection, above: i64, related: &str, since: &str) -> AppResult<(Vec<Planned>, TextPlan)> {
    let Some(year) = repo::current_academic_year_id(conn)? else {
        return Ok((Vec::new(), TextPlan { recipients: 0, learners: 0, without_phone: 0, sms_units: 0, sample: None, skipped_recent: 0 }));
    };
    let learners = crate::fees::learners_in(conn, &year, None)?;
    let ids: Vec<String> = learners.iter().map(|l| l.0.clone()).collect();
    let ledger = Ledger::load(conn, Some(&ids))?;
    let owing: Vec<(Option<String>, (String, String, i64))> = learners
        .into_iter()
        .filter_map(|(id, name, _, class, phone)| {
            let balance = ledger.position(&id, None).map_or(0, |p| p.balance);
            (balance > above.max(0)).then(|| (phone, (name, class, balance)))
        })
        .collect();
    let learner_count = owing.len();
    let (groups, without) = by_phone(owing);
    let signature = sms::signature(conn)?;
    let mut planned = Vec::new();
    let mut skipped = 0;
    for (phone, kids) in groups {
        if already(conn, "fee_reminder", &phone, related, "0")? || recently(conn, "fee_reminder", &phone, since)? {
            skipped += 1;
            continue;
        }
        let total: i64 = kids.iter().map(|k| k.2).sum();
        let parts: Vec<String> = kids.iter().map(|(n, c, b)| format!("{n} ({c}) {}", ugx(*b))).collect();
        let body = sms::plain_text(&format!(
            "{signature}: Dear parent, school fees balance: {}{}. Please pay or talk to the bursar. Thank you.",
            names(&parts),
            if kids.len() > 1 { format!(". Total {}", ugx(total)) } else { String::new() }
        ));
        planned.push(Planned { phone, body, related: related.to_string() });
    }
    let plan = TextPlan {
        recipients: planned.len(),
        learners: learner_count,
        without_phone: without,
        sms_units: planned.iter().map(|p| sms::segments(&p.body)).sum(),
        sample: planned.first().map(|p| p.body.clone()),
        skipped_recent: skipped,
    };
    Ok((planned, plan))
}

fn queue_all(conn: &Connection, planned: &[Planned], kind: &str, by: Option<&str>) -> AppResult<usize> {
    let mut n = 0;
    for p in planned {
        sms::insert(conn, &p.phone, &p.body, kind, Some(&p.related), by)?;
        n += 1;
    }
    Ok(n)
}

fn days_ago(now: NaiveDateTime, days: i64) -> String {
    (now - chrono::Duration::days(days)).and_utc().to_rfc3339()
}

/// Weekly, on the chosen day after the chosen time: parents who owe more than
/// the school's amount, if they were not reminded in the last six days.
pub fn fee_reminder_job(conn: &Connection, now: NaiveDateTime) -> AppResult<usize> {
    let settings = load(conn)?;
    if !settings.fee_reminders || crate::network::offline_only(conn)? {
        return Ok(0);
    }
    let at = NaiveTime::parse_from_str(&settings.fee_reminder_time, "%H:%M").unwrap_or(NaiveTime::from_hms_opt(10, 0, 0).expect("valid"));
    if now.weekday().number_from_monday() != settings.fee_reminder_day || now.time() < at {
        return Ok(0);
    }
    // The school must have a term open: nobody is reminded in the holidays.
    if repo::current_term_id(conn)?.is_none() {
        return Ok(0);
    }
    let week = now.date().iso_week();
    let round = format!("week:{}-W{:02}", week.year(), week.week());
    // Once per family per week; the round's name is the whole rule.
    let (planned, _) = plan_fee_reminders(conn, settings.fee_reminder_above, &round, "9999")?;
    queue_all(conn, &planned, "fee_reminder", None)
}

#[tauri::command]
pub fn preview_fee_reminders(state: State<'_, AppState>, above: i64) -> AppResult<TextPlan> {
    let session = state.sessions.require()?;
    require_fees_and_messages(&session)?;
    let conn = state.db.lock();
    Ok(plan_fee_reminders(&conn, above, "manual", &days_ago(chrono::Local::now().naive_local(), 2))?.1)
}

/// Sends the reminders now, from the Fees screen. Still skips anyone texted a
/// reminder in the last two days, so a double click costs nothing.
#[tauri::command]
pub fn send_fee_reminders(state: State<'_, AppState>, above: i64) -> AppResult<usize> {
    let session = state.sessions.require()?;
    require_fees_and_messages(&session)?;
    let conn = state.db.lock();
    crate::network::require_online_allowed(&conn)?;
    let tx = conn.unchecked_transaction()?;
    let (planned, plan) = plan_fee_reminders(&tx, above, "manual", &days_ago(chrono::Local::now().naive_local(), 2))?;
    let n = queue_all(&tx, &planned, "fee_reminder", Some(&session.user_id))?;
    audit::record(
        &tx,
        Some(&session),
        "auto_sms.fee_reminders",
        "sms",
        "fee_reminder",
        format!("Texted fee reminders to {n} parent(s) of {} learner(s) owing more than {}", plan.learners, ugx(above)),
    )?;
    tx.commit()?;
    drop(conn);
    sms::wake();
    Ok(n)
}

fn require_fees_and_messages(session: &Session) -> AppResult<()> {
    if session.has(Permission::ManageFees) && session.has(Permission::SendMessages) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

// ---------------------------------------------------------------------------
// Promises to pay
// ---------------------------------------------------------------------------

/// The morning a promise falls due (after 08:00), one reminder.
pub fn promise_reminder_job(conn: &Connection, now: NaiveDateTime) -> AppResult<usize> {
    if !flag(conn, "auto_sms.promise_reminders")? || crate::network::offline_only(conn)? {
        return Ok(0);
    }
    if now.time() < NaiveTime::from_hms_opt(8, 0, 0).expect("valid") {
        return Ok(0);
    }
    let today = now.date().format("%Y-%m-%d").to_string();
    let mut stmt = conn.prepare(
        "SELECT p.id, p.amount, s.full_name, s.guardian_phone FROM fee_promises p
         JOIN students s ON s.id = p.student_id
         WHERE p.status = 'open' AND p.due_on = ?1",
    )?;
    let due: Vec<(String, i64, String, Option<String>)> =
        stmt.query_map(params![today], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
    let signature = sms::signature(conn)?;
    let mut n = 0;
    for (id, amount, name, phone) in due {
        let Some(phone) = phone.as_deref().and_then(sms::normalize_phone) else { continue };
        if already(conn, "promise_reminder", &phone, &id, "0")? {
            continue;
        }
        let body = sms::plain_text(&format!(
            "{signature}: Reminder: you agreed to pay {} for {} by today. Thank you for keeping your word.",
            ugx(amount),
            name
        ));
        sms::insert(conn, &phone, &body, "promise_reminder", Some(&id), None)?;
        n += 1;
    }
    Ok(n)
}

// ---------------------------------------------------------------------------
// Not yet reported at the start of term
// ---------------------------------------------------------------------------

/// Weekday mornings after 10:00 in the first fourteen days of a term:
/// learners on the roll who have not once been marked present or late this
/// term, in classes whose register has been marked at least twice. Each
/// family once per term.
pub fn not_reported_job(conn: &Connection, now: NaiveDateTime) -> AppResult<usize> {
    if !flag(conn, "auto_sms.not_reported")? || crate::network::offline_only(conn)? {
        return Ok(0);
    }
    if now.weekday().number_from_monday() > 5 || now.time() < NaiveTime::from_hms_opt(10, 0, 0).expect("valid") {
        return Ok(0);
    }
    let Some(term_id) = repo::current_term_id(conn)? else { return Ok(0) };
    let start: Option<String> =
        conn.query_row("SELECT start_date FROM terms WHERE id = ?1", params![term_id], |r| r.get(0)).optional()?.flatten();
    let Some(start) = start.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()) else { return Ok(0) };
    let day = (now.date() - start).num_days();
    if !(1..=14).contains(&day) {
        return Ok(0);
    }
    let Some(year) = repo::current_academic_year_id(conn)? else { return Ok(0) };
    let mut stmt = conn.prepare(
        "SELECT s.id, s.full_name, s.guardian_phone
         FROM enrollments e JOIN students s ON s.id = e.student_id
         WHERE e.academic_year_id = ?1 AND e.status = 'active' AND s.status = 'active'
           AND (SELECT COUNT(DISTINCT a.on_date) FROM attendance a WHERE a.class_id = e.class_id AND a.term_id = ?2) >= 2
           AND NOT EXISTS (SELECT 1 FROM attendance a WHERE a.student_id = s.id AND a.term_id = ?2
                           AND a.state IN ('present', 'late'))",
    )?;
    let missing: Vec<(Option<String>, String)> = stmt
        .query_map(params![year, term_id], |r| Ok((r.get::<_, Option<String>>(2)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let (groups, _) = by_phone(missing);
    let signature = sms::signature(conn)?;
    let mut n = 0;
    for (phone, kids) in groups {
        if already(conn, "not_reported", &phone, &term_id, "0")? {
            continue;
        }
        let list: Vec<String> = kids.clone();
        let body = sms::plain_text(&format!(
            "{signature}: Dear parent, {} {} not yet reported for this term. Lessons have started. Please bring {} to school or call us.",
            names(&list),
            if list.len() == 1 { "has" } else { "have" },
            if list.len() == 1 { "your child" } else { "them" }
        ));
        sms::insert(conn, &phone, &body, "not_reported", Some(&term_id), None)?;
        n += 1;
    }
    Ok(n)
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// Plans one text per family with each child's results for the term.
fn plan_results(conn: &Connection, class_id: &str, term_id: &str) -> AppResult<(Vec<Planned>, TextPlan)> {
    let cards = crate::commands::reports::cards_for_texting(conn, class_id, term_id)?;
    let learners = cards.len();
    let (groups, without) = by_phone(cards.into_iter().map(|c| (c.phone.clone(), c)).collect());
    let signature = sms::signature(conn)?;
    let mut planned = Vec::new();
    let mut skipped = 0;
    for (phone, kids) in groups {
        if already(conn, "results", &phone, term_id, "0")? {
            skipped += 1;
            continue;
        }
        let parts: Vec<String> = kids.iter().map(|k| k.summary.clone()).collect();
        let body = sms::plain_text(&format!("{signature}: {}. Report cards are at school.", parts.join(". ")));
        planned.push(Planned { phone, body, related: term_id.to_string() });
    }
    let plan = TextPlan {
        recipients: planned.len(),
        learners,
        without_phone: without,
        sms_units: planned.iter().map(|p| sms::segments(&p.body)).sum(),
        sample: planned.first().map(|p| p.body.clone()),
        skipped_recent: skipped,
    };
    Ok((planned, plan))
}

fn require_results(conn: &Connection, session: &Session, class_id: &str) -> AppResult<()> {
    if !flag(conn, "auto_sms.results")? {
        return Err(AppError::validation("Results texts are switched off (Settings → SMS → Automatic texts)."));
    }
    if session.may_manage_class(class_id) || (session.has(Permission::ViewAllClasses) && session.has(Permission::SendMessages)) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

#[tauri::command]
pub fn preview_results_sms(state: State<'_, AppState>, class_id: String, term_id: String) -> AppResult<TextPlan> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    require_results(&conn, &session, &class_id)?;
    Ok(plan_results(&conn, &class_id, &term_id)?.1)
}

#[tauri::command]
pub fn send_results_sms(state: State<'_, AppState>, class_id: String, term_id: String) -> AppResult<usize> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    require_results(&conn, &session, &class_id)?;
    crate::network::require_online_allowed(&conn)?;
    let tx = conn.unchecked_transaction()?;
    let (planned, plan) = plan_results(&tx, &class_id, &term_id)?;
    let n = queue_all(&tx, &planned, "results", Some(&session.user_id))?;
    audit::record(&tx, Some(&session), "auto_sms.results", "class", &class_id, format!("Texted results to {n} parent(s) of {} learner(s)", plan.learners))?;
    tx.commit()?;
    drop(conn);
    sms::wake();
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fees::tests::fee_school;

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn texts(conn: &Connection, kind: &str) -> Vec<(String, String)> {
        let mut stmt = conn.prepare("SELECT to_phone, body FROM sms_outbox WHERE kind = ?1 ORDER BY to_phone").unwrap();
        let rows = stmt.query_map([kind], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        rows
    }

    #[test]
    fn nothing_is_sent_until_switched_on() {
        let db = fee_school();
        let conn = db.lock();
        let monday = at("2026-06-01 10:30");
        assert_eq!(fee_reminder_job(&conn, monday).unwrap(), 0);
        assert_eq!(promise_reminder_job(&conn, monday).unwrap(), 0);
        assert_eq!(not_reported_job(&conn, monday).unwrap(), 0);
        assert!(texts(&conn, "fee_reminder").is_empty());
    }

    #[test]
    fn weekly_reminders_go_once_a_week_one_per_family() {
        let db = fee_school();
        let conn = db.lock();
        // Amina and Musa share a parent's phone.
        conn.execute("UPDATE students SET guardian_phone = '0772000001' WHERE id = 'musa'", []).unwrap();
        repo::set_setting(&conn, "auto_sms.fee_reminders", "1").unwrap();
        repo::set_setting(&conn, "auto_sms.fee_reminder_day", "1").unwrap();
        repo::set_setting(&conn, "auto_sms.fee_reminder_time", "10:00").unwrap();

        assert_eq!(fee_reminder_job(&conn, at("2026-06-01 09:59")).unwrap(), 0, "before the time");
        assert_eq!(fee_reminder_job(&conn, at("2026-06-02 10:30")).unwrap(), 0, "a Tuesday");
        assert_eq!(fee_reminder_job(&conn, at("2026-06-01 10:05")).unwrap(), 1, "one family");
        assert_eq!(fee_reminder_job(&conn, at("2026-06-01 11:05")).unwrap(), 0, "not twice the same week");
        let sent = texts(&conn, "fee_reminder");
        assert_eq!(sent.len(), 1);
        assert_eq!(
            sent[0].1,
            "Rainbow N&P: Dear parent, school fees balance: Amina N (P5) UGX 370,000 and Musa K (P5) UGX 580,000. Total UGX 950,000. Please pay or talk to the bursar. Thank you."
        );
        assert_eq!(fee_reminder_job(&conn, at("2026-06-08 10:05")).unwrap(), 1, "the next Monday");
    }

    #[test]
    fn only_balances_above_the_amount_are_reminded() {
        let db = fee_school();
        let conn = db.lock();
        let (_, plan) = plan_fee_reminders(&conn, 400_000, "manual", "9999").unwrap();
        assert_eq!((plan.recipients, plan.learners), (1, 1), "Musa owes 580,000; Amina 370,000");
    }

    #[test]
    fn a_promise_is_remembered_on_its_day_once() {
        let db = fee_school();
        let conn = db.lock();
        repo::set_setting(&conn, "auto_sms.promise_reminders", "1").unwrap();
        conn.execute(
            "INSERT INTO fee_promises (id, student_id, term_id, amount, due_on, created_at) VALUES ('pr1', 'musa', 't262', 200000, '2026-06-05', 'x')",
            [],
        )
        .unwrap();
        assert_eq!(promise_reminder_job(&conn, at("2026-06-04 09:00")).unwrap(), 0);
        assert_eq!(promise_reminder_job(&conn, at("2026-06-05 07:00")).unwrap(), 0, "not before 8");
        assert_eq!(promise_reminder_job(&conn, at("2026-06-05 08:30")).unwrap(), 1);
        assert_eq!(promise_reminder_job(&conn, at("2026-06-05 12:30")).unwrap(), 0);
        assert!(texts(&conn, "promise_reminder")[0].1.contains("you agreed to pay UGX 200,000 for Musa K by today"));
    }

    #[test]
    fn learners_who_never_came_are_noticed_in_the_first_two_weeks() {
        let db = fee_school();
        let conn = db.lock();
        repo::set_setting(&conn, "auto_sms.not_reported", "1").unwrap();
        conn.execute_batch(
            "INSERT INTO attendance (id, student_id, class_id, term_id, on_date, state, marked_at) VALUES
                ('a1', 'amina', 'p5', 't262', '2026-05-26', 'present', 'x'),
                ('a2', 'musa',  'p5', 't262', '2026-05-26', 'absent',  'x'),
                ('a3', 'amina', 'p5', 't262', '2026-05-27', 'present', 'x'),
                ('a4', 'musa',  'p5', 't262', '2026-05-27', 'absent',  'x');",
        )
        .unwrap();
        assert_eq!(not_reported_job(&conn, at("2026-05-30 10:30")).unwrap(), 0, "a Saturday");
        assert_eq!(not_reported_job(&conn, at("2026-05-28 10:30")).unwrap(), 1, "Musa never came");
        assert_eq!(not_reported_job(&conn, at("2026-05-29 10:30")).unwrap(), 0, "once a term");
        assert_eq!(not_reported_job(&conn, at("2026-06-15 10:30")).unwrap(), 0, "past the first two weeks");
        let sent = texts(&conn, "not_reported");
        assert_eq!(sent[0].1, "Rainbow N&P: Dear parent, Musa K has not yet reported for this term. Lessons have started. Please bring your child to school or call us.");
    }

    #[test]
    fn offline_only_stops_every_job() {
        let db = fee_school();
        let conn = db.lock();
        repo::set_setting(&conn, "auto_sms.fee_reminders", "1").unwrap();
        crate::network::set_offline_only(&conn, true).unwrap();
        assert_eq!(fee_reminder_job(&conn, at("2026-06-01 10:30")).unwrap(), 0);
    }

    #[test]
    fn names_join_like_a_sentence() {
        assert_eq!(names(&["A".into()]), "A");
        assert_eq!(names(&["A".into(), "B".into()]), "A and B");
        assert_eq!(names(&["A".into(), "B".into(), "C".into()]), "A, B and C");
    }
}
