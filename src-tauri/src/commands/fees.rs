//! FR-G5 — fees: the structure, payments and receipts, bursaries, promises
//! to pay, invoices and statements.
//!
//! Who may do what (see `security::session`):
//!
//! * the bursar, headteacher and School Admin set fees and take payments;
//! * the same three read balances and collections;
//! * only the headteacher and School Admin void a receipt, and never one they
//!   issued themselves unless they run the system: the person holding the
//!   cash cannot also make its receipt disappear.
//!
//! Every money figure is whole shillings (`i64`). Nothing here is deleted.

use std::collections::HashMap;

use chrono::{Local, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::commands::reports::{envelope, DocumentEnvelope};
use crate::domain::ids::new_id;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::fees::{self, amount_in_words, method_label, receipt_label, ugx, BillLine, Ledger, TermAccount};
use crate::security::{Permission, Session};
use crate::sms;
use crate::state::AppState;

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn require_finance(session: &Session) -> AppResult<()> {
    if session.has(Permission::ViewFinance) || session.has(Permission::ManageFees) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn clean(value: &Option<String>) -> Option<String> {
    value.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn term_name(conn: &Connection, term_id: &str) -> AppResult<String> {
    conn.query_row(
        "SELECT t.name || ' ' || y.label FROM terms t JOIN academic_years y ON y.id = t.academic_year_id
         WHERE t.id = ?1",
        params![term_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("That term"))
}

fn term_year(conn: &Connection, term_id: &str) -> AppResult<String> {
    conn.query_row("SELECT academic_year_id FROM terms WHERE id = ?1", params![term_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That term"))
}

/// The term a screen means: the one asked for, or the one payments go to now.
fn resolve_term(conn: &Connection, term_id: Option<String>) -> AppResult<String> {
    match term_id {
        Some(id) => Ok(id),
        None => fees::payment_term(conn)?
            .ok_or_else(|| AppError::validation("Open a term first (Calendar), then set its fees.")),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnerBrief {
    pub student_id: String,
    pub full_name: String,
    pub reg_number: String,
    pub class_name: Option<String>,
    pub guardian_name: Option<String>,
    pub guardian_phone: Option<String>,
}

fn learner_brief(conn: &Connection, student_id: &str) -> AppResult<LearnerBrief> {
    let year = repo::current_academic_year_id(conn)?;
    conn.query_row(
        "SELECT s.id, s.full_name, s.reg_number,
                (SELECT c.name FROM enrollments e JOIN classes c ON c.id = e.class_id
                  WHERE e.student_id = s.id ORDER BY (e.academic_year_id = ?2) DESC, e.joined_at DESC LIMIT 1),
                s.guardian_name, s.guardian_phone
         FROM students s WHERE s.id = ?1",
        params![student_id, year],
        |r| {
            Ok(LearnerBrief {
                student_id: r.get(0)?,
                full_name: r.get(1)?,
                reg_number: r.get(2)?,
                class_name: r.get(3)?,
                guardian_name: r.get(4)?,
                guardian_phone: r.get(5)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("That learner"))
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeSettings {
    /// Printed at the foot of every invoice: bank account, Mobile Money
    /// number, SchoolPay code.
    pub payment_instructions: String,
    /// Print the fees balance and next term's fees on report cards.
    pub show_on_report_card: bool,
    /// With the fees rule on, a balance above this holds back a report card
    /// or exam permit automatically. `None` leaves it to the manual flag.
    pub auto_block_above: Option<i64>,
    /// Text the guardian a receipt when a payment is recorded.
    pub text_receipts: bool,
}

pub fn load_fee_settings(conn: &Connection) -> AppResult<FeeSettings> {
    let get = |k: &str| -> AppResult<String> { Ok(repo::get_setting(conn, k)?.unwrap_or_default()) };
    Ok(FeeSettings {
        payment_instructions: get("fees.payment_instructions")?,
        show_on_report_card: get("fees.show_on_report_card")? == "1",
        auto_block_above: get("fees.auto_block_above")?.parse().ok(),
        text_receipts: get("fees.text_receipts")? == "1",
    })
}

#[tauri::command]
pub fn get_fee_settings(state: State<'_, AppState>) -> AppResult<FeeSettings> {
    let session = state.sessions.require()?;
    require_finance(&session)?;
    load_fee_settings(&state.db.lock())
}

#[tauri::command]
pub fn save_fee_settings(state: State<'_, AppState>, settings: FeeSettings) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    if settings.auto_block_above.is_some_and(|v| v < 0) {
        return Err(AppError::validation("The amount must be zero or more."));
    }
    let conn = state.db.lock();
    let tx = conn.unchecked_transaction()?;
    repo::set_setting(&tx, "fees.payment_instructions", settings.payment_instructions.trim())?;
    repo::set_setting(&tx, "fees.show_on_report_card", if settings.show_on_report_card { "1" } else { "0" })?;
    repo::set_setting(
        &tx,
        "fees.auto_block_above",
        &settings.auto_block_above.map(|v| v.to_string()).unwrap_or_default(),
    )?;
    repo::set_setting(&tx, "fees.text_receipts", if settings.text_receipts { "1" } else { "0" })?;
    audit::record(&tx, Some(&session), "fees.settings", "setting", "fees", "Updated the fees settings")?;
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The fee structure
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeItemRow {
    pub id: String,
    pub name: String,
    /// `termly` or `optional`.
    pub kind: String,
    pub position: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeRateRow {
    pub fee_item_id: String,
    pub class_id: String,
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeTermOption {
    pub id: String,
    pub label: String,
    pub status: String,
    /// Whether any fee has been set for this term yet.
    pub has_rates: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeClassOption {
    pub id: String,
    pub name: String,
    pub learners: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeSetup {
    pub term_id: String,
    pub terms: Vec<FeeTermOption>,
    pub items: Vec<FeeItemRow>,
    pub classes: Vec<FeeClassOption>,
    pub rates: Vec<FeeRateRow>,
    /// Learners taking each optional item this term.
    pub optin_counts: Vec<FeeRateRow>,
}

pub fn fee_setup(conn: &Connection, term_id: &str) -> AppResult<FeeSetup> {
    let year = term_year(conn, term_id)?;
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name || ' ' || y.label, t.status,
                EXISTS (SELECT 1 FROM fee_rates r WHERE r.term_id = t.id AND r.amount > 0)
         FROM terms t JOIN academic_years y ON y.id = t.academic_year_id
         ORDER BY y.label DESC, t.seq DESC LIMIT 12",
    )?;
    let terms = stmt
        .query_map([], |r| {
            Ok(FeeTermOption { id: r.get(0)?, label: r.get(1)?, status: r.get(2)?, has_rates: r.get(3)? })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stmt = conn.prepare(
        "SELECT id, name, kind, position FROM fee_items WHERE status = 'active'
         ORDER BY position ASC, name COLLATE NOCASE ASC",
    )?;
    let items = stmt
        .query_map([], |r| Ok(FeeItemRow { id: r.get(0)?, name: r.get(1)?, kind: r.get(2)?, position: r.get(3)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stmt = conn.prepare(
        "SELECT c.id, c.name,
                (SELECT COUNT(*) FROM enrollments e WHERE e.class_id = c.id
                   AND e.academic_year_id = ?1 AND e.status = 'active')
         FROM classes c WHERE c.status = 'active' ORDER BY c.ladder_position ASC",
    )?;
    let classes = stmt
        .query_map(params![year], |r| Ok(FeeClassOption { id: r.get(0)?, name: r.get(1)?, learners: r.get(2)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stmt = conn.prepare("SELECT fee_item_id, class_id, amount FROM fee_rates WHERE term_id = ?1")?;
    let rates = stmt
        .query_map(params![term_id], |r| Ok(FeeRateRow { fee_item_id: r.get(0)?, class_id: r.get(1)?, amount: r.get(2)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stmt = conn.prepare(
        "SELECT o.fee_item_id, e.class_id, COUNT(*)
         FROM fee_optins o
         JOIN enrollments e ON e.student_id = o.student_id AND e.academic_year_id = ?2 AND e.status = 'active'
         WHERE o.term_id = ?1 AND o.status = 'active'
         GROUP BY o.fee_item_id, e.class_id",
    )?;
    let optin_counts = stmt
        .query_map(params![term_id, year], |r| {
            Ok(FeeRateRow { fee_item_id: r.get(0)?, class_id: r.get(1)?, amount: r.get(2)? })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(FeeSetup { term_id: term_id.to_string(), terms, items, classes, rates, optin_counts })
}

#[tauri::command]
pub fn get_fee_setup(state: State<'_, AppState>, term_id: Option<String>) -> AppResult<FeeSetup> {
    let session = state.sessions.require()?;
    require_finance(&session)?;
    let conn = state.db.lock();
    let term_id = resolve_term(&conn, term_id)?;
    fee_setup(&conn, &term_id)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeItemInput {
    pub id: Option<String>,
    pub name: String,
    pub kind: String,
    pub position: Option<i64>,
}

#[tauri::command]
pub fn save_fee_item(state: State<'_, AppState>, item: FeeItemInput) -> AppResult<FeeItemRow> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    let conn = state.db.lock();
    save_fee_item_in(&conn, &session, item)
}

pub fn save_fee_item_in(conn: &Connection, session: &Session, item: FeeItemInput) -> AppResult<FeeItemRow> {
    let name = item.name.trim();
    if name.is_empty() || name.len() > 60 {
        return Err(AppError::validation("Give the fee a name, such as Tuition or Lunch."));
    }
    if !matches!(item.kind.as_str(), "termly" | "optional") {
        return Err(AppError::validation("Choose whether every learner pays it or only some."));
    }
    let clash: Option<String> = conn
        .query_row(
            "SELECT id FROM fee_items WHERE name = ?1 COLLATE NOCASE AND status = 'active' AND id != IFNULL(?2, '')",
            params![name, item.id],
            |r| r.get(0),
        )
        .optional()?;
    if clash.is_some() {
        return Err(AppError::conflict(format!("There is already a fee called \"{name}\".")));
    }
    let stamp = now();
    let id = match &item.id {
        Some(id) => {
            let changed = conn.execute(
                "UPDATE fee_items SET name = ?1, kind = ?2, position = COALESCE(?3, position), updated_at = ?4
                 WHERE id = ?5 AND status = 'active'",
                params![name, item.kind, item.position, stamp, id],
            )?;
            if changed == 0 {
                return Err(AppError::not_found("That fee"));
            }
            id.clone()
        }
        None => {
            let id = new_id("fee");
            let position: i64 = match item.position {
                Some(p) => p,
                None => conn.query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM fee_items", [], |r| r.get(0))?,
            };
            conn.execute(
                "INSERT INTO fee_items (id, name, kind, position, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![id, name, item.kind, position, stamp],
            )?;
            id
        }
    };
    audit::record(conn, Some(session), "fees.item", "fee_item", &id, format!("Saved the fee \"{name}\" ({})", item.kind))?;
    Ok(conn.query_row("SELECT id, name, kind, position FROM fee_items WHERE id = ?1", params![id], |r| {
        Ok(FeeItemRow { id: r.get(0)?, name: r.get(1)?, kind: r.get(2)?, position: r.get(3)? })
    })?)
}

#[tauri::command]
pub fn retire_fee_item(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    let conn = state.db.lock();
    let name: String = conn
        .query_row("SELECT name FROM fee_items WHERE id = ?1 AND status = 'active'", params![id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That fee"))?;
    conn.execute("UPDATE fee_items SET status = 'retired', updated_at = ?1 WHERE id = ?2", params![now(), id])?;
    audit::record(
        &conn,
        Some(&session),
        "fees.item_retire",
        "fee_item",
        &id,
        format!("Stopped charging \"{name}\" from now on (past bills keep it)"),
    )?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeRateInput {
    pub fee_item_id: String,
    pub class_id: String,
    pub amount: i64,
}

pub fn save_fee_rates_in(conn: &Connection, session: &Session, term_id: &str, rates: &[FeeRateInput]) -> AppResult<usize> {
    let label = term_name(conn, term_id)?;
    let mut changed = 0;
    let stamp = now();
    for rate in rates {
        if rate.amount < 0 || rate.amount > 100_000_000 {
            return Err(AppError::validation("A fee must be between 0 and 100,000,000 shillings."));
        }
        let before: Option<i64> = conn
            .query_row(
                "SELECT amount FROM fee_rates WHERE term_id = ?1 AND fee_item_id = ?2 AND class_id = ?3",
                params![term_id, rate.fee_item_id, rate.class_id],
                |r| r.get(0),
            )
            .optional()?;
        if before == Some(rate.amount) || (before.is_none() && rate.amount == 0) {
            continue;
        }
        conn.execute(
            "INSERT INTO fee_rates (term_id, fee_item_id, class_id, amount, updated_by, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (term_id, fee_item_id, class_id)
             DO UPDATE SET amount = excluded.amount, updated_by = excluded.updated_by, updated_at = excluded.updated_at",
            params![term_id, rate.fee_item_id, rate.class_id, rate.amount, session.user_id, stamp],
        )?;
        let (item, class): (String, String) = conn.query_row(
            "SELECT (SELECT name FROM fee_items WHERE id = ?1), (SELECT name FROM classes WHERE id = ?2)",
            params![rate.fee_item_id, rate.class_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        audit::record(
            conn,
            Some(session),
            "fees.rate",
            "fee_rate",
            &format!("{term_id}/{}/{}", rate.fee_item_id, rate.class_id),
            format!(
                "{label}: {item} for {class} {} {}",
                if before.is_some() { format!("changed from {} to", ugx(before.unwrap_or(0))) } else { "set to".into() },
                ugx(rate.amount)
            ),
        )?;
        changed += 1;
    }
    Ok(changed)
}

#[tauri::command]
pub fn save_fee_rates(state: State<'_, AppState>, term_id: String, rates: Vec<FeeRateInput>) -> AppResult<usize> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    let conn = state.db.lock();
    let tx = conn.unchecked_transaction()?;
    let changed = save_fee_rates_in(&tx, &session, &term_id, &rates)?;
    tx.commit()?;
    Ok(changed)
}

/// Copies one term's fees into another, so a school sets fees once a year
/// and only adjusts. Overwrites what the target term had.
#[tauri::command]
pub fn copy_fee_structure(state: State<'_, AppState>, from_term_id: String, to_term_id: String) -> AppResult<usize> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    if from_term_id == to_term_id {
        return Err(AppError::validation("Choose a different term to copy from."));
    }
    let conn = state.db.lock();
    let tx = conn.unchecked_transaction()?;
    let rates: Vec<FeeRateInput> = {
        let mut stmt = tx.prepare(
            "SELECT r.fee_item_id, r.class_id, r.amount FROM fee_rates r
             JOIN fee_items i ON i.id = r.fee_item_id AND i.status = 'active'
             WHERE r.term_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![from_term_id], |r| {
                Ok(FeeRateInput { fee_item_id: r.get(0)?, class_id: r.get(1)?, amount: r.get(2)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    if rates.is_empty() {
        return Err(AppError::validation("That term has no fees to copy."));
    }
    let changed = save_fee_rates_in(&tx, &session, &to_term_id, &rates)?;
    tx.commit()?;
    Ok(changed)
}

// ---------------------------------------------------------------------------
// Optional items: who takes lunch, transport...
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptinRow {
    pub student_id: String,
    pub full_name: String,
    pub class_name: String,
    pub taking: bool,
}

#[tauri::command]
pub fn list_fee_optins(
    state: State<'_, AppState>,
    term_id: String,
    fee_item_id: String,
    class_id: Option<String>,
) -> AppResult<Vec<OptinRow>> {
    let session = state.sessions.require()?;
    require_finance(&session)?;
    let conn = state.db.lock();
    let year = term_year(&conn, &term_id)?;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.full_name, c.name,
                EXISTS (SELECT 1 FROM fee_optins o WHERE o.term_id = ?1 AND o.fee_item_id = ?2
                        AND o.student_id = s.id AND o.status = 'active')
         FROM enrollments e
         JOIN students s ON s.id = e.student_id
         JOIN classes c ON c.id = e.class_id
         WHERE e.academic_year_id = ?3 AND e.status = 'active' AND (?4 IS NULL OR e.class_id = ?4)
         ORDER BY c.ladder_position, s.full_name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map(params![term_id, fee_item_id, year, class_id], |r| {
            Ok(OptinRow { student_id: r.get(0)?, full_name: r.get(1)?, class_name: r.get(2)?, taking: r.get(3)? })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn set_fee_optins_in(
    conn: &Connection,
    session: &Session,
    term_id: &str,
    fee_item_id: &str,
    student_ids: &[String],
    on: bool,
) -> AppResult<usize> {
    let (item_name, kind): (String, String) = conn
        .query_row("SELECT name, kind FROM fee_items WHERE id = ?1", params![fee_item_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| AppError::not_found("That fee"))?;
    if kind != "optional" {
        return Err(AppError::validation(format!("Every learner pays {item_name}; there is nobody to sign up.")));
    }
    let stamp = now();
    let status = if on { "active" } else { "removed" };
    let mut changed = 0;
    for student in student_ids {
        changed += conn.execute(
            "INSERT INTO fee_optins (term_id, fee_item_id, student_id, status, changed_by, changed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (term_id, fee_item_id, student_id)
             DO UPDATE SET status = excluded.status, changed_by = excluded.changed_by, changed_at = excluded.changed_at
             WHERE fee_optins.status != excluded.status",
            params![term_id, fee_item_id, student, status, session.user_id, stamp],
        )?;
    }
    if changed > 0 {
        audit::record(
            conn,
            Some(session),
            "fees.optin",
            "fee_item",
            fee_item_id,
            format!(
                "{} {changed} learner(s) {} {item_name} for {}",
                if on { "Signed up" } else { "Took" },
                if on { "for" } else { "off" },
                term_name(conn, term_id)?
            ),
        )?;
    }
    Ok(changed)
}

#[tauri::command]
pub fn set_fee_optins(
    state: State<'_, AppState>,
    term_id: String,
    fee_item_id: String,
    student_ids: Vec<String>,
    on: bool,
) -> AppResult<usize> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    let conn = state.db.lock();
    let tx = conn.unchecked_transaction()?;
    let changed = set_fee_optins_in(&tx, &session, &term_id, &fee_item_id, &student_ids, on)?;
    tx.commit()?;
    Ok(changed)
}

// ---------------------------------------------------------------------------
// Balances
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromiseRow {
    pub id: String,
    pub student_id: String,
    pub term_id: String,
    pub amount: i64,
    pub due_on: String,
    pub promised_by: Option<String>,
    pub note: Option<String>,
    pub status: String,
    pub created_at: String,
    /// Past its date and still open.
    pub overdue: bool,
}

fn promises_for(conn: &Connection, student_id: Option<&str>, open_only: bool) -> AppResult<Vec<PromiseRow>> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let mut stmt = conn.prepare(
        "SELECT id, student_id, term_id, amount, due_on, promised_by, note, status, created_at
         FROM fee_promises
         WHERE (?1 IS NULL OR student_id = ?1) AND (?2 = 0 OR status = 'open')
         ORDER BY due_on ASC",
    )?;
    let rows = stmt
        .query_map(params![student_id, open_only as i64], |r| {
            let due_on: String = r.get(4)?;
            let status: String = r.get(7)?;
            Ok(PromiseRow {
                id: r.get(0)?,
                student_id: r.get(1)?,
                term_id: r.get(2)?,
                amount: r.get(3)?,
                overdue: status == "open" && due_on < today,
                due_on,
                promised_by: r.get(5)?,
                note: r.get(6)?,
                status,
                created_at: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeAccountRow {
    pub student_id: String,
    pub full_name: String,
    pub reg_number: String,
    pub class_name: String,
    pub guardian_phone: Option<String>,
    pub brought_forward: i64,
    pub billed: i64,
    pub paid: i64,
    pub balance: i64,
    pub promise: Option<PromiseRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeTotals {
    pub learners: usize,
    pub expected: i64,
    pub collected: i64,
    pub outstanding: i64,
    pub owing: usize,
    /// Parents who have paid ahead.
    pub credit: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeAccountList {
    pub term_id: String,
    pub term_name: String,
    pub rows: Vec<FeeAccountRow>,
    pub totals: FeeTotals,
}

pub fn fee_accounts(conn: &Connection, term_id: &str, class_id: Option<&str>) -> AppResult<FeeAccountList> {
    let year = term_year(conn, term_id)?;
    let learners = fees::learners_in(conn, &year, class_id)?;
    let ids: Vec<String> = learners.iter().map(|l| l.0.clone()).collect();
    let ledger = Ledger::load(conn, Some(&ids))?;
    let mut promises: HashMap<String, PromiseRow> = HashMap::new();
    for promise in promises_for(conn, None, true)? {
        promises.entry(promise.student_id.clone()).or_insert(promise);
    }

    let mut rows = Vec::with_capacity(learners.len());
    let mut totals = FeeTotals { learners: learners.len(), expected: 0, collected: 0, outstanding: 0, owing: 0, credit: 0 };
    for (id, name, reg, class, phone) in learners {
        let position = ledger.position(&id, Some(term_id));
        let (bf, billed, paid, balance) = position
            .as_ref()
            .map(|p| (p.brought_forward, p.billed, p.paid, p.balance))
            .unwrap_or((0, 0, 0, 0));
        totals.expected += bf.max(0) + billed;
        totals.collected += paid;
        if balance > 0 {
            totals.outstanding += balance;
            totals.owing += 1;
        } else {
            totals.credit += -balance;
        }
        rows.push(FeeAccountRow {
            promise: promises.remove(&id),
            student_id: id,
            full_name: name,
            reg_number: reg,
            class_name: class,
            guardian_phone: phone,
            brought_forward: bf,
            billed,
            paid,
            balance,
        });
    }
    Ok(FeeAccountList { term_id: term_id.into(), term_name: term_name(conn, term_id)?, rows, totals })
}

#[tauri::command]
pub fn list_fee_accounts(
    state: State<'_, AppState>,
    term_id: Option<String>,
    class_id: Option<String>,
) -> AppResult<FeeAccountList> {
    let session = state.sessions.require()?;
    require_finance(&session)?;
    let conn = state.db.lock();
    let term_id = resolve_term(&conn, term_id)?;
    fee_accounts(&conn, &term_id, class_id.as_deref())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRow {
    pub id: String,
    pub receipt_no: i64,
    pub receipt_label: String,
    pub student_id: String,
    pub student_name: String,
    pub class_name: Option<String>,
    pub term_id: String,
    pub amount: i64,
    pub method: String,
    pub method_label: String,
    pub reference: Option<String>,
    pub paid_by: Option<String>,
    pub paid_on: String,
    pub note: Option<String>,
    pub source: String,
    pub balance_after: Option<i64>,
    pub status: String,
    pub received_by_name: Option<String>,
    pub created_at: String,
    pub void_reason: Option<String>,
    pub voided_by_name: Option<String>,
    pub voided_at: Option<String>,
}

const PAYMENT_SELECT: &str = "SELECT p.id, p.receipt_no, p.student_id, s.full_name,
        (SELECT c.name FROM enrollments e JOIN classes c ON c.id = e.class_id
          WHERE e.student_id = p.student_id
            AND e.academic_year_id = (SELECT academic_year_id FROM terms WHERE id = p.term_id)),
        p.term_id, p.amount, p.method, p.reference, p.paid_by, p.paid_on, p.note, p.source,
        p.balance_after, p.status, ur.full_name, p.created_at, p.void_reason, uv.full_name, p.voided_at
     FROM fee_payments p
     JOIN students s ON s.id = p.student_id
     LEFT JOIN users ur ON ur.id = p.received_by
     LEFT JOIN users uv ON uv.id = p.voided_by";

fn payment_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PaymentRow> {
    let receipt_no: i64 = r.get(1)?;
    let method: String = r.get(7)?;
    Ok(PaymentRow {
        id: r.get(0)?,
        receipt_no,
        receipt_label: receipt_label(receipt_no),
        student_id: r.get(2)?,
        student_name: r.get(3)?,
        class_name: r.get(4)?,
        term_id: r.get(5)?,
        amount: r.get(6)?,
        method_label: method_label(&method).into(),
        method,
        reference: r.get(8)?,
        paid_by: r.get(9)?,
        paid_on: r.get(10)?,
        note: r.get(11)?,
        source: r.get(12)?,
        balance_after: r.get(13)?,
        status: r.get(14)?,
        received_by_name: r.get(15)?,
        created_at: r.get(16)?,
        void_reason: r.get(17)?,
        voided_by_name: r.get(18)?,
        voided_at: r.get(19)?,
    })
}

pub fn load_payment(conn: &Connection, id: &str) -> AppResult<PaymentRow> {
    conn.query_row(&format!("{PAYMENT_SELECT} WHERE p.id = ?1"), params![id], payment_from_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("That receipt"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustmentRow {
    pub id: String,
    pub term_id: String,
    pub term_name: String,
    pub fee_item_name: Option<String>,
    pub kind: String,
    pub label: String,
    pub amount: Option<i64>,
    pub percent: Option<f64>,
    pub reason: String,
    pub status: String,
    pub created_by_name: Option<String>,
    pub created_at: String,
    pub cancel_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeAccountDetail {
    pub learner: LearnerBrief,
    pub terms: Vec<TermAccount>,
    pub balance: i64,
    pub payments: Vec<PaymentRow>,
    pub adjustments: Vec<AdjustmentRow>,
    pub promises: Vec<PromiseRow>,
}

pub fn fee_account_detail(conn: &Connection, student_id: &str) -> AppResult<FeeAccountDetail> {
    let learner = learner_brief(conn, student_id)?;
    let ledger = Ledger::load(conn, Some(&[student_id.to_string()]))?;
    let terms = ledger.account(student_id, None);
    let balance = terms.last().map_or(0, |t| t.balance);

    let mut stmt = conn.prepare(&format!("{PAYMENT_SELECT} WHERE p.student_id = ?1 ORDER BY p.receipt_no DESC"))?;
    let payments = stmt.query_map(params![student_id], payment_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stmt = conn.prepare(
        "SELECT a.id, a.term_id, t.name || ' ' || y.label, i.name, a.kind, a.amount, a.percent, a.reason,
                a.status, u.full_name, a.created_at, a.cancel_reason
         FROM fee_adjustments a
         JOIN terms t ON t.id = a.term_id JOIN academic_years y ON y.id = t.academic_year_id
         LEFT JOIN fee_items i ON i.id = a.fee_item_id
         LEFT JOIN users u ON u.id = a.created_by
         WHERE a.student_id = ?1 ORDER BY a.created_at DESC",
    )?;
    let adjustments = stmt
        .query_map(params![student_id], |r| {
            let kind: String = r.get(4)?;
            Ok(AdjustmentRow {
                id: r.get(0)?,
                term_id: r.get(1)?,
                term_name: r.get(2)?,
                fee_item_name: r.get(3)?,
                label: fees::adjustment_label(&kind).into(),
                kind,
                amount: r.get(5)?,
                percent: r.get(6)?,
                reason: r.get(7)?,
                status: r.get(8)?,
                created_by_name: r.get(9)?,
                created_at: r.get(10)?,
                cancel_reason: r.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(FeeAccountDetail { learner, terms, balance, payments, adjustments, promises: promises_for(conn, Some(student_id), false)? })
}

#[tauri::command]
pub fn get_fee_account(state: State<'_, AppState>, student_id: String) -> AppResult<FeeAccountDetail> {
    let session = state.sessions.require()?;
    require_finance(&session)?;
    fee_account_detail(&state.db.lock(), &student_id)
}

/// A learner's balance now, for receipts, report cards and reminders.
pub fn current_balance(conn: &Connection, student_id: &str) -> AppResult<i64> {
    let ledger = Ledger::load(conn, Some(&[student_id.to_string()]))?;
    Ok(ledger.position(student_id, None).map_or(0, |p| p.balance))
}

// ---------------------------------------------------------------------------
// Taking a payment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordPaymentRequest {
    pub student_id: String,
    pub amount: i64,
    pub method: String,
    pub reference: Option<String>,
    pub paid_by: Option<String>,
    pub payer_phone: Option<String>,
    /// YYYY-MM-DD; today when missing.
    pub paid_on: Option<String>,
    pub note: Option<String>,
    /// Text the guardian the receipt (only when the school allows it).
    pub text_guardian: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedPayment {
    pub payment: PaymentRow,
    pub guardian_texted: bool,
    /// Open promises this payment completed.
    pub promises_kept: usize,
}

pub fn record_payment_in(
    conn: &Connection,
    session: &Session,
    request: &RecordPaymentRequest,
    source: &str,
) -> AppResult<RecordedPayment> {
    if request.amount <= 0 {
        return Err(AppError::validation("Enter the amount received."));
    }
    if request.amount > 100_000_000 {
        return Err(AppError::validation("That amount is too large for one receipt. Check the figure."));
    }
    if !matches!(request.method.as_str(), "cash" | "mobile_money" | "bank" | "schoolpay" | "cheque" | "other") {
        return Err(AppError::validation("Choose how the money was paid."));
    }
    let reference = clean(&request.reference);
    if !matches!(request.method.as_str(), "cash" | "other") && reference.is_none() {
        return Err(AppError::validation(match request.method.as_str() {
            "mobile_money" => "Enter the Mobile Money transaction ID from the parent's message.",
            "bank" => "Enter the bank slip number.",
            "cheque" => "Enter the cheque number.",
            _ => "Enter the payment reference.",
        }));
    }
    let today = Local::now().date_naive();
    let paid_on = match clean(&request.paid_on) {
        Some(text) => {
            let date = NaiveDate::parse_from_str(&text, "%Y-%m-%d")
                .map_err(|_| AppError::validation("The payment date is not a date."))?;
            if date > today {
                return Err(AppError::validation("A payment cannot be dated in the future."));
            }
            text
        }
        None => today.format("%Y-%m-%d").to_string(),
    };
    if let Some(reference) = &reference {
        let earlier: Option<i64> = conn
            .query_row(
                "SELECT receipt_no FROM fee_payments
                 WHERE method = ?1 AND reference = ?2 COLLATE NOCASE AND status = 'valid'",
                params![request.method, reference],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(earlier) = earlier {
            return Err(AppError::conflict(format!(
                "{} {reference} was already receipted on {}. One payment gets one receipt.",
                method_label(&request.method),
                receipt_label(earlier)
            )));
        }
    }
    let learner = learner_brief(conn, &request.student_id)?;
    let term_id = fees::payment_term(conn)?
        .ok_or_else(|| AppError::validation("Open a term first (Calendar) so payments have a term to go to."))?;

    let balance_before = current_balance(conn, &request.student_id)?;
    let balance_after = balance_before - request.amount;
    let receipt_no = fees::next_receipt_no(conn)?;
    let id = new_id("pay");
    conn.execute(
        "INSERT INTO fee_payments (id, receipt_no, student_id, term_id, amount, method, reference, paid_by,
                                   payer_phone, paid_on, note, source, balance_after, received_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            id,
            receipt_no,
            request.student_id,
            term_id,
            request.amount,
            request.method,
            reference,
            clean(&request.paid_by),
            clean(&request.payer_phone),
            paid_on,
            clean(&request.note),
            source,
            balance_after,
            session.user_id,
            now(),
        ],
    )?;

    // A promise is kept once the money paid since it was made covers it.
    let mut promises_kept = 0;
    for promise in promises_for(conn, Some(&request.student_id), true)? {
        let since: i64 = conn.query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM fee_payments
             WHERE student_id = ?1 AND status = 'valid' AND created_at >= ?2",
            params![request.student_id, promise.created_at],
            |r| r.get(0),
        )?;
        if since >= promise.amount {
            conn.execute(
                "UPDATE fee_promises SET status = 'kept', closed_at = ?1 WHERE id = ?2",
                params![now(), promise.id],
            )?;
            promises_kept += 1;
        }
    }

    let settings = load_fee_settings(conn)?;
    let mut guardian_texted = false;
    if request.text_guardian && settings.text_receipts {
        let body = sms::plain_text(&format!(
            "{}: Received {} for {} on {}. Receipt {}. {}. Thank you.",
            sms::signature(conn)?,
            ugx(request.amount),
            learner.full_name,
            NaiveDate::parse_from_str(&paid_on, "%Y-%m-%d").map(|d| d.format("%d %b %Y").to_string()).unwrap_or(paid_on.clone()),
            receipt_label(receipt_no),
            if balance_after > 0 {
                format!("Balance {}", ugx(balance_after))
            } else if balance_after == 0 {
                "Fees fully paid".to_string()
            } else {
                format!("Credit {}", ugx(-balance_after))
            }
        ));
        let phone = clean(&request.payer_phone).or(learner.guardian_phone.clone());
        guardian_texted = sms::queue(conn, phone.as_deref(), &body, "fee_receipt", &id, Some(&session.user_id))?.is_some();
    }

    audit::record_with_details(
        conn,
        Some(session),
        "fees.payment",
        "student",
        &request.student_id,
        format!(
            "Received {} from {} for {} by {}{}: receipt {}",
            ugx(request.amount),
            clean(&request.paid_by).unwrap_or_else(|| "a parent".into()),
            learner.full_name,
            method_label(&request.method),
            reference.as_ref().map(|r| format!(" ({r})")).unwrap_or_default(),
            receipt_label(receipt_no)
        ),
        Some(serde_json::json!({ "receipt": receipt_no, "amount": request.amount, "balanceAfter": balance_after })),
    )?;

    Ok(RecordedPayment { payment: load_payment(conn, &id)?, guardian_texted, promises_kept })
}

#[tauri::command]
pub fn record_fee_payment(
    state: State<'_, AppState>,
    request: RecordPaymentRequest,
) -> AppResult<RecordedPayment> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    let conn = state.db.lock();
    let tx = conn.unchecked_transaction()?;
    let recorded = record_payment_in(&tx, &session, &request, "manual")?;
    tx.commit()?;
    drop(conn);
    if recorded.guardian_texted {
        sms::wake();
    }
    Ok(recorded)
}

pub fn void_payment_in(conn: &Connection, session: &Session, id: &str, reason: &str) -> AppResult<PaymentRow> {
    session.require(Permission::VoidReceipts)?;
    let reason = reason.trim();
    if reason.len() < 5 {
        return Err(AppError::validation("Say why this receipt is being cancelled. It is kept on record."));
    }
    let payment = load_payment(conn, id)?;
    if payment.status == "voided" {
        return Err(AppError::conflict("That receipt is already cancelled."));
    }
    let received_by: Option<String> =
        conn.query_row("SELECT received_by FROM fee_payments WHERE id = ?1", params![id], |r| r.get(0))?;
    if received_by.as_deref() == Some(session.user_id.as_str()) && !session.is_school_admin() {
        return Err(AppError::validation(
            "You issued this receipt, so someone else must cancel it: the School Admin or the headteacher.",
        ));
    }
    conn.execute(
        "UPDATE fee_payments SET status = 'voided', voided_by = ?1, voided_at = ?2, void_reason = ?3 WHERE id = ?4",
        params![session.user_id, now(), reason, id],
    )?;
    audit::record(
        conn,
        Some(session),
        "fees.void",
        "student",
        &payment.student_id,
        format!(
            "Cancelled receipt {} ({} for {}): {reason}",
            payment.receipt_label,
            ugx(payment.amount),
            payment.student_name
        ),
    )?;
    load_payment(conn, id)
}

#[tauri::command]
pub fn void_fee_payment(state: State<'_, AppState>, id: String, reason: String) -> AppResult<PaymentRow> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let tx = conn.unchecked_transaction()?;
    let row = void_payment_in(&tx, &session, &id, &reason)?;
    tx.commit()?;
    Ok(row)
}

// ---------------------------------------------------------------------------
// Bursaries, discounts and charges
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustmentInput {
    pub student_ids: Vec<String>,
    pub term_id: Option<String>,
    pub fee_item_id: Option<String>,
    pub kind: String,
    pub amount: Option<i64>,
    pub percent: Option<f64>,
    pub reason: String,
}

pub fn add_adjustment_in(conn: &Connection, session: &Session, input: &AdjustmentInput) -> AppResult<usize> {
    if !matches!(
        input.kind.as_str(),
        "bursary" | "discount" | "waiver" | "charge" | "balance_forward" | "prepayment"
    ) {
        return Err(AppError::validation("Choose what kind of change this is."));
    }
    let reason = input.reason.trim();
    if reason.is_empty() {
        return Err(AppError::validation("Give a reason. It prints on the invoice and stays on record."));
    }
    match (input.amount, input.percent) {
        (Some(a), None) if a > 0 => {}
        (None, Some(p)) if p > 0.0 && p <= 100.0 => {
            if !matches!(input.kind.as_str(), "bursary" | "discount" | "waiver") {
                return Err(AppError::validation("Only a bursary, discount or waiver can be a percentage."));
            }
        }
        _ => return Err(AppError::validation("Enter an amount, or a percentage between 1 and 100.")),
    }
    if input.student_ids.is_empty() {
        return Err(AppError::validation("Choose at least one learner."));
    }
    let term_id = match &input.term_id {
        Some(t) => t.clone(),
        None => fees::payment_term(conn)?.ok_or_else(|| AppError::validation("Open a term first."))?,
    };
    let label = term_name(conn, &term_id)?;
    let stamp = now();
    for student in &input.student_ids {
        let learner = learner_brief(conn, student)?;
        let id = new_id("fad");
        conn.execute(
            "INSERT INTO fee_adjustments (id, student_id, term_id, fee_item_id, kind, amount, percent, reason,
                                          created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, student, term_id, input.fee_item_id, input.kind, input.amount, input.percent, reason, session.user_id, stamp],
        )?;
        audit::record(
            conn,
            Some(session),
            "fees.adjustment",
            "student",
            student,
            format!(
                "{} of {} for {} in {label}: {reason}",
                fees::adjustment_label(&input.kind),
                match (input.amount, input.percent) {
                    (Some(a), _) => ugx(a),
                    (_, Some(p)) => format!("{p}%"),
                    _ => String::new(),
                },
                learner.full_name
            ),
        )?;
    }
    Ok(input.student_ids.len())
}

#[tauri::command]
pub fn add_fee_adjustment(state: State<'_, AppState>, input: AdjustmentInput) -> AppResult<usize> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    let conn = state.db.lock();
    let tx = conn.unchecked_transaction()?;
    let n = add_adjustment_in(&tx, &session, &input)?;
    tx.commit()?;
    Ok(n)
}

#[tauri::command]
pub fn cancel_fee_adjustment(state: State<'_, AppState>, id: String, reason: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::validation("Say why it is being cancelled."));
    }
    let conn = state.db.lock();
    let (student, kind): (String, String) = conn
        .query_row(
            "SELECT student_id, kind FROM fee_adjustments WHERE id = ?1 AND status = 'active'",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("That bursary or charge"))?;
    conn.execute(
        "UPDATE fee_adjustments SET status = 'cancelled', cancelled_by = ?1, cancelled_at = ?2, cancel_reason = ?3
         WHERE id = ?4",
        params![session.user_id, now(), reason, id],
    )?;
    audit::record(
        &conn,
        Some(&session),
        "fees.adjustment_cancel",
        "student",
        &student,
        format!("Cancelled a {}: {reason}", fees::adjustment_label(&kind).to_lowercase()),
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Promises to pay
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromiseInput {
    pub student_id: String,
    pub amount: i64,
    pub due_on: String,
    pub promised_by: Option<String>,
    pub note: Option<String>,
}

#[tauri::command]
pub fn add_fee_promise(state: State<'_, AppState>, input: PromiseInput) -> AppResult<PromiseRow> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    if input.amount <= 0 {
        return Err(AppError::validation("Enter the amount the parent promised."));
    }
    NaiveDate::parse_from_str(&input.due_on, "%Y-%m-%d").map_err(|_| AppError::validation("Choose the date."))?;
    let conn = state.db.lock();
    let learner = learner_brief(&conn, &input.student_id)?;
    let term_id = fees::payment_term(&conn)?.ok_or_else(|| AppError::validation("Open a term first."))?;
    let id = new_id("fpr");
    conn.execute(
        "INSERT INTO fee_promises (id, student_id, term_id, amount, due_on, promised_by, note, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![id, input.student_id, term_id, input.amount, input.due_on, clean(&input.promised_by), clean(&input.note), session.user_id, now()],
    )?;
    audit::record(
        &conn,
        Some(&session),
        "fees.promise",
        "student",
        &input.student_id,
        format!("{} promised {} for {} by {}", clean(&input.promised_by).unwrap_or_else(|| "The parent".into()), ugx(input.amount), learner.full_name, input.due_on),
    )?;
    promises_for(&conn, Some(&input.student_id), false)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::internal("promise vanished"))
}

#[tauri::command]
pub fn close_fee_promise(state: State<'_, AppState>, id: String, status: String) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    if !matches!(status.as_str(), "kept" | "broken" | "cancelled") {
        return Err(AppError::validation("Unknown outcome."));
    }
    let conn = state.db.lock();
    let student: String = conn
        .query_row("SELECT student_id FROM fee_promises WHERE id = ?1 AND status = 'open'", params![id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That promise"))?;
    conn.execute("UPDATE fee_promises SET status = ?1, closed_at = ?2 WHERE id = ?3", params![status, now(), id])?;
    audit::record(&conn, Some(&session), "fees.promise_close", "student", &student, format!("Marked a promise to pay as {status}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Collections: what came in, day by day
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDay {
    pub date: String,
    pub cash: i64,
    pub mobile_money: i64,
    pub bank: i64,
    pub schoolpay: i64,
    pub other: i64,
    pub total: i64,
    pub receipts: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collections {
    pub from: String,
    pub to: String,
    pub days: Vec<CollectionDay>,
    pub totals: CollectionDay,
    pub payments: Vec<PaymentRow>,
    pub voided: Vec<PaymentRow>,
}

pub fn collections(conn: &Connection, from: &str, to: &str) -> AppResult<Collections> {
    let mut stmt = conn.prepare(&format!(
        "{PAYMENT_SELECT} WHERE p.paid_on BETWEEN ?1 AND ?2 ORDER BY p.paid_on ASC, p.receipt_no ASC"
    ))?;
    let all = stmt.query_map(params![from, to], payment_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    let (payments, voided): (Vec<_>, Vec<_>) = all.into_iter().partition(|p| p.status == "valid");

    let mut days: Vec<CollectionDay> = Vec::new();
    let mut totals = CollectionDay { date: format!("{from} to {to}"), ..Default::default() };
    for payment in &payments {
        if days.last().map_or(true, |d| d.date != payment.paid_on) {
            days.push(CollectionDay { date: payment.paid_on.clone(), ..Default::default() });
        }
        let day = days.last_mut().expect("just pushed");
        for target in [&mut *day, &mut totals] {
            match payment.method.as_str() {
                "cash" => target.cash += payment.amount,
                "mobile_money" => target.mobile_money += payment.amount,
                "bank" | "cheque" => target.bank += payment.amount,
                "schoolpay" => target.schoolpay += payment.amount,
                _ => target.other += payment.amount,
            }
            target.total += payment.amount;
            target.receipts += 1;
        }
    }
    Ok(Collections { from: from.into(), to: to.into(), days, totals, payments, voided })
}

#[tauri::command]
pub fn fee_collections(state: State<'_, AppState>, from: String, to: String) -> AppResult<Collections> {
    let session = state.sessions.require()?;
    require_finance(&session)?;
    for date in [&from, &to] {
        NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| AppError::validation("Choose the dates."))?;
    }
    collections(&state.db.lock(), &from, &to)
}

// ---------------------------------------------------------------------------
// Printed: receipt, invoices, statement, balances list, collections
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeReceipt {
    pub payment: PaymentRow,
    pub reg_number: String,
    pub term_name: String,
    pub amount_words: String,
    /// Typed into Phantom (or scanned) to prove the receipt is genuine.
    pub verification_code: String,
    pub verification_qr: String,
}

#[tauri::command]
pub fn build_fee_receipt(state: State<'_, AppState>, payment_id: String) -> AppResult<DocumentEnvelope<FeeReceipt>> {
    let session = state.sessions.require()?;
    require_finance(&session)?;
    let conn = state.db.lock();
    build_receipt_in(&conn, &payment_id)
}

pub fn build_receipt_in(conn: &Connection, payment_id: &str) -> AppResult<DocumentEnvelope<FeeReceipt>> {
    let payment = load_payment(conn, payment_id)?;
    let reg_number: String =
        conn.query_row("SELECT reg_number FROM students WHERE id = ?1", params![payment.student_id], |r| r.get(0))?;
    let verification_code = crate::verify::code_for_receipt(conn, &payment)?;
    let school: String = conn.query_row("SELECT name FROM institution WHERE id = 1", [], |r| r.get(0))?;
    let body = FeeReceipt {
        term_name: term_name(conn, &payment.term_id)?,
        amount_words: amount_in_words(payment.amount),
        verification_qr: crate::verify::qr_data_url(&crate::verify::receipt_qr(&school, &payment, &verification_code))?,
        verification_code,
        reg_number,
        payment,
    };
    envelope(conn, "fee_receipt", body)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeInvoice {
    pub learner: LearnerBrief,
    pub brought_forward: i64,
    pub lines: Vec<BillLine>,
    pub billed: i64,
    pub paid: i64,
    pub balance: i64,
    pub promise: Option<PromiseRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeInvoiceBatch {
    pub term_name: String,
    pub class_name: Option<String>,
    pub payment_instructions: String,
    pub invoices: Vec<FeeInvoice>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceRequest {
    pub term_id: Option<String>,
    pub class_id: Option<String>,
    pub student_ids: Option<Vec<String>>,
    /// Leave out learners who owe nothing.
    pub only_owing: bool,
}

pub fn build_invoices_in(conn: &Connection, request: &InvoiceRequest) -> AppResult<DocumentEnvelope<FeeInvoiceBatch>> {
    let term_id = resolve_term(conn, request.term_id.clone())?;
    let year = term_year(conn, &term_id)?;
    let mut learners = fees::learners_in(conn, &year, request.class_id.as_deref())?;
    if let Some(ids) = &request.student_ids {
        learners.retain(|l| ids.contains(&l.0));
    }
    let ids: Vec<String> = learners.iter().map(|l| l.0.clone()).collect();
    let ledger = Ledger::load(conn, Some(&ids))?;
    let mut promises: HashMap<String, PromiseRow> = HashMap::new();
    for promise in promises_for(conn, None, true)? {
        promises.entry(promise.student_id.clone()).or_insert(promise);
    }
    let mut invoices = Vec::new();
    for (id, ..) in &learners {
        let Some(position) = ledger.position(id, Some(&term_id)) else { continue };
        if request.only_owing && position.balance <= 0 {
            continue;
        }
        invoices.push(FeeInvoice {
            learner: learner_brief(conn, id)?,
            brought_forward: position.brought_forward,
            lines: position.lines,
            billed: position.billed,
            paid: position.paid,
            balance: position.balance,
            promise: promises.remove(id),
        });
    }
    let class_name = match &request.class_id {
        Some(c) => conn.query_row("SELECT name FROM classes WHERE id = ?1", params![c], |r| r.get(0)).optional()?,
        None => None,
    };
    let body = FeeInvoiceBatch {
        term_name: term_name(conn, &term_id)?,
        class_name,
        payment_instructions: load_fee_settings(conn)?.payment_instructions,
        invoices,
    };
    envelope(conn, "fee_invoices", body)
}

#[tauri::command]
pub fn build_fee_invoices(state: State<'_, AppState>, request: InvoiceRequest) -> AppResult<DocumentEnvelope<FeeInvoiceBatch>> {
    let session = state.sessions.require()?;
    require_finance(&session)?;
    build_invoices_in(&state.db.lock(), &request)
}

#[tauri::command]
pub fn build_fee_statement(state: State<'_, AppState>, student_id: String) -> AppResult<DocumentEnvelope<FeeAccountDetail>> {
    let session = state.sessions.require()?;
    require_finance(&session)?;
    let conn = state.db.lock();
    let detail = fee_account_detail(&conn, &student_id)?;
    envelope(&conn, "fee_statement", detail)
}

#[tauri::command]
pub fn build_fee_balances(
    state: State<'_, AppState>,
    term_id: Option<String>,
    class_id: Option<String>,
) -> AppResult<DocumentEnvelope<FeeAccountList>> {
    let session = state.sessions.require()?;
    require_finance(&session)?;
    let conn = state.db.lock();
    let term_id = resolve_term(&conn, term_id)?;
    let mut list = fee_accounts(&conn, &term_id, class_id.as_deref())?;
    list.rows.retain(|r| r.balance > 0);
    list.rows.sort_by(|a, b| b.balance.cmp(&a.balance));
    envelope(&conn, "fee_balances", list)
}

#[tauri::command]
pub fn build_fee_collections(state: State<'_, AppState>, from: String, to: String) -> AppResult<DocumentEnvelope<Collections>> {
    let session = state.sessions.require()?;
    require_finance(&session)?;
    let conn = state.db.lock();
    let body = collections(&conn, &from, &to)?;
    envelope(&conn, "fee_collections", body)
}

// ---------------------------------------------------------------------------
// Report cards and permits
// ---------------------------------------------------------------------------

/// What a report card prints about fees, when the school chose to show it.
pub struct ReportFees {
    pub show: bool,
    pub balances: HashMap<String, i64>,
    /// What next term costs this class (termly items only).
    pub next_term_fees: Option<i64>,
    /// With the fees rule on: a balance above this holds back the card.
    pub block_above: Option<i64>,
}

pub fn report_fees(conn: &Connection, term_id: &str, class_id: &str, students: &[String]) -> AppResult<ReportFees> {
    let settings = load_fee_settings(conn)?;
    let uses_ledger: bool = conn.query_row("SELECT EXISTS (SELECT 1 FROM fee_rates WHERE amount > 0)", [], |r| r.get(0))?;
    if !uses_ledger || (!settings.show_on_report_card && settings.auto_block_above.is_none()) {
        return Ok(ReportFees { show: false, balances: HashMap::new(), next_term_fees: None, block_above: None });
    }
    let ledger = Ledger::load(conn, Some(students))?;
    let balances = students
        .iter()
        .map(|s| (s.clone(), ledger.position(s, Some(term_id)).map_or(0, |p| p.balance)))
        .collect();
    // The next term in order, whatever its status: schools set next term's
    // fees before it opens precisely so the report card can say them.
    let next_term: Option<String> = conn
        .query_row(
            "WITH ordered AS (
                SELECT t.id, ROW_NUMBER() OVER (ORDER BY y.label, t.seq) AS n
                FROM terms t JOIN academic_years y ON y.id = t.academic_year_id)
             SELECT b.id FROM ordered a JOIN ordered b ON b.n = a.n + 1 WHERE a.id = ?1",
            params![term_id],
            |r| r.get(0),
        )
        .optional()?;
    let next_term_fees = match next_term {
        Some(next) => conn
            .query_row(
                "SELECT SUM(r.amount) FROM fee_rates r JOIN fee_items i ON i.id = r.fee_item_id
                 WHERE r.term_id = ?1 AND r.class_id = ?2 AND i.kind = 'termly' AND i.status = 'active'",
                params![next, class_id],
                |r| r.get::<_, Option<i64>>(0),
            )?
            .filter(|v| *v > 0),
        None => None,
    };
    Ok(ReportFees { show: settings.show_on_report_card, balances, next_term_fees, block_above: settings.auto_block_above })
}

/// For exam permits: who is over the school's fees limit, and by how much.
/// `None` when the school has not set a limit.
pub fn over_fees_limit(conn: &Connection, students: &[String]) -> AppResult<Option<HashMap<String, i64>>> {
    let Some(limit) = load_fee_settings(conn)?.auto_block_above else {
        return Ok(None);
    };
    let ledger = Ledger::load(conn, Some(students))?;
    Ok(Some(
        students
            .iter()
            .filter_map(|s| {
                let balance = ledger.position(s, None).map_or(0, |p| p.balance);
                (balance > limit).then(|| (s.clone(), balance))
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fees::tests::fee_school;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    fn bursar() -> Session {
        test_session("bursar", Role::Teacher, Some(Office::Bursar), &[])
    }
    fn head() -> Session {
        test_session("head", Role::Teacher, Some(Office::HeadTeacher), &[])
    }
    fn pay(student: &str, amount: i64, method: &str, reference: Option<&str>) -> RecordPaymentRequest {
        RecordPaymentRequest {
            student_id: student.into(),
            amount,
            method: method.into(),
            reference: reference.map(str::to_string),
            paid_by: Some("Mrs N".into()),
            payer_phone: None,
            paid_on: None,
            note: None,
            text_guardian: true,
        }
    }

    #[test]
    fn a_payment_gets_the_next_receipt_number_and_the_balance_after_it() {
        let db = fee_school();
        let conn = db.lock();
        let before = current_balance(&conn, "amina").unwrap();
        let recorded = record_payment_in(&conn, &bursar(), &pay("amina", 70_000, "cash", None), "manual").unwrap();
        assert_eq!(recorded.payment.receipt_no, 4);
        assert_eq!(recorded.payment.receipt_label, "RCT-000004");
        assert_eq!(recorded.payment.balance_after, Some(before - 70_000));
        assert_eq!(recorded.payment.term_id, "t262", "payments go to the open term");
        assert!(!recorded.guardian_texted, "receipt texts are off until the school turns them on");
        assert_eq!(current_balance(&conn, "amina").unwrap(), before - 70_000);
    }

    #[test]
    fn the_same_mobile_money_transaction_cannot_be_receipted_twice() {
        let db = fee_school();
        let conn = db.lock();
        record_payment_in(&conn, &bursar(), &pay("musa", 50_000, "mobile_money", Some("MP2610XYZ")), "manual").unwrap();
        let again = record_payment_in(&conn, &bursar(), &pay("amina", 50_000, "mobile_money", Some("mp2610xyz")), "manual")
            .unwrap_err();
        assert!(again.user_message().contains("already receipted on RCT-000004"), "{}", again.user_message());
        let missing = record_payment_in(&conn, &bursar(), &pay("musa", 50_000, "bank", None), "manual").unwrap_err();
        assert!(missing.user_message().contains("bank slip"));
    }

    #[test]
    fn the_bursar_cannot_void_and_nobody_voids_their_own_receipt() {
        let db = fee_school();
        let conn = db.lock();
        let head = head();
        let by_head = record_payment_in(&conn, &head, &pay("musa", 10_000, "cash", None), "manual").unwrap();
        let by_bursar = record_payment_in(&conn, &bursar(), &pay("musa", 20_000, "cash", None), "manual").unwrap();

        assert!(matches!(
            void_payment_in(&conn, &bursar(), &by_bursar.payment.id, "Entered twice by mistake"),
            Err(AppError::Forbidden)
        ));
        assert!(void_payment_in(&conn, &head, &by_head.payment.id, "Entered twice by mistake").is_err(), "own receipt");
        assert!(void_payment_in(&conn, &head, &by_bursar.payment.id, "too short").is_ok());
        assert!(void_payment_in(&conn, &head, &by_bursar.payment.id, "Entered twice").is_err(), "only once");

        let balance = current_balance(&conn, "musa").unwrap();
        // Term 1 350,000 + Term 2 330,000 - 100,000 - 10,000 (the voided 20,000 does not count).
        assert_eq!(balance, 570_000);
    }

    #[test]
    fn a_promise_is_kept_when_the_money_paid_since_covers_it() {
        let db = fee_school();
        let conn = db.lock();
        conn.execute(
            "INSERT INTO fee_promises (id, student_id, term_id, amount, due_on, created_at)
             VALUES ('pr1', 'musa', 't262', 100000, '2026-06-30', '2026-06-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let first = record_payment_in(&conn, &bursar(), &pay("musa", 60_000, "cash", None), "manual").unwrap();
        assert_eq!(first.promises_kept, 0);
        let second = record_payment_in(&conn, &bursar(), &pay("musa", 40_000, "cash", None), "manual").unwrap();
        assert_eq!(second.promises_kept, 1);
    }

    #[test]
    fn receipt_texts_go_only_when_the_school_allows_them() {
        let db = fee_school();
        let conn = db.lock();
        repo::set_setting(&conn, "fees.text_receipts", "1").unwrap();
        let recorded = record_payment_in(&conn, &bursar(), &pay("amina", 70_000, "cash", None), "manual").unwrap();
        assert!(recorded.guardian_texted);
        let body: String = conn.query_row("SELECT body FROM sms_outbox WHERE kind = 'fee_receipt'", [], |r| r.get(0)).unwrap();
        assert!(body.starts_with("Rainbow N&P: Received UGX 70,000 for Amina N on "), "{body}");
        assert!(body.contains("Receipt RCT-000004. Balance UGX 300,000. Thank you."), "{body}");
    }

    #[test]
    fn the_class_list_totals_what_is_expected_collected_and_owed() {
        let db = fee_school();
        let conn = db.lock();
        let list = fee_accounts(&conn, "t262", Some("p5")).unwrap();
        assert_eq!(list.rows.len(), 2, "Sarah left");
        let amina = list.rows.iter().find(|r| r.student_id == "amina").unwrap();
        assert_eq!((amina.brought_forward, amina.billed, amina.paid, amina.balance), (50_000, 320_000, 0, 370_000));
        assert_eq!(list.totals.collected, 100_000);
        assert_eq!(list.totals.outstanding, 370_000 + 580_000);
        assert_eq!(list.totals.owing, 2);
    }

    #[test]
    fn collections_are_split_by_day_and_method_and_voids_are_listed_apart() {
        let db = fee_school();
        let conn = db.lock();
        let c = collections(&conn, "2026-02-01", "2026-06-30").unwrap();
        assert_eq!(c.days.len(), 2);
        assert_eq!((c.totals.mobile_money, c.totals.cash, c.totals.total, c.totals.receipts), (300_000, 100_000, 400_000, 2));
        assert!(c.voided.is_empty());
    }

    #[test]
    fn invoices_list_lines_and_can_skip_those_who_owe_nothing() {
        let db = fee_school();
        let conn = db.lock();
        repo::set_setting(&conn, "fees.payment_instructions", "Stanbic 9030001234567").unwrap();
        let all = build_invoices_in(
            &conn,
            &InvoiceRequest { term_id: Some("t262".into()), class_id: Some("p5".into()), student_ids: None, only_owing: true },
        )
        .unwrap();
        assert_eq!(all.body.invoices.len(), 2);
        assert_eq!(all.body.payment_instructions, "Stanbic 9030001234567");
        let amina = &all.body.invoices[0];
        assert_eq!(amina.learner.full_name, "Amina N");
        assert_eq!(amina.lines.len(), 4);
    }

    #[test]
    fn fee_rates_change_is_audited_and_unchanged_rates_are_not() {
        let db = fee_school();
        let conn = db.lock();
        let rates = vec![
            FeeRateInput { fee_item_id: "tuition".into(), class_id: "p5".into(), amount: 320_000 },
            FeeRateInput { fee_item_id: "dev".into(), class_id: "p5".into(), amount: 50_000 },
        ];
        assert_eq!(save_fee_rates_in(&conn, &bursar(), "t262", &rates).unwrap(), 1);
        let summary: String = conn
            .query_row("SELECT summary FROM audit_log WHERE action = 'fees.rate'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(summary, "Term 2 2026: Tuition for P5 changed from UGX 300,000 to UGX 320,000");
    }

    #[test]
    fn only_optional_items_take_sign_ups() {
        let db = fee_school();
        let conn = db.lock();
        assert!(set_fee_optins_in(&conn, &bursar(), "t262", "tuition", &["musa".into()], true).is_err());
        assert_eq!(set_fee_optins_in(&conn, &bursar(), "t262", "lunch", &["musa".into()], true).unwrap(), 1);
        assert_eq!(set_fee_optins_in(&conn, &bursar(), "t262", "lunch", &["musa".into()], true).unwrap(), 0, "already on");
        assert_eq!(current_balance(&conn, "musa").unwrap(), 350_000 + 330_000 + 120_000 - 100_000);
    }

    #[test]
    fn the_fees_limit_names_only_those_above_it() {
        let db = fee_school();
        let conn = db.lock();
        let students = vec!["amina".to_string(), "musa".to_string()];
        assert!(over_fees_limit(&conn, &students).unwrap().is_none(), "no limit set");
        repo::set_setting(&conn, "fees.auto_block_above", "400000").unwrap();
        let over = over_fees_limit(&conn, &students).unwrap().unwrap();
        assert_eq!(over.len(), 1);
        assert_eq!(over["musa"], 580_000);
    }

    #[test]
    fn report_cards_show_balances_and_next_terms_fees_only_when_asked() {
        let db = fee_school();
        let conn = db.lock();
        let students = vec!["amina".to_string(), "musa".to_string()];
        assert!(!report_fees(&conn, "t262", "p5", &students).unwrap().show);
        repo::set_setting(&conn, "fees.show_on_report_card", "1").unwrap();
        let shown = report_fees(&conn, "t262", "p5", &students).unwrap();
        assert!(shown.show);
        assert_eq!(shown.balances["amina"], 370_000);
        assert_eq!(shown.next_term_fees, Some(999_999), "Term 3's tuition, set before it opens");
    }
}
