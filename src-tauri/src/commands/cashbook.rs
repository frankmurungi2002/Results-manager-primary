//! Spending, other income, and cash taken to the bank: the rest of the
//! school's money beside fees.
//!
//! Every entry is a numbered voucher (PV- for spending, OI- for other income,
//! BK- for banking), kept like a receipt: never edited, never deleted,
//! cancelled only with a reason and by someone other than whoever wrote it.
//! From these and the fee receipts Phantom works out what came in, what went
//! out, the surplus, and the cash that should be in the drawer right now,
//! so the head can count it.

use std::collections::BTreeMap;

use chrono::{Local, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::commands::reports::{envelope, DocumentEnvelope};
use crate::domain::ids::new_id;
use crate::error::{AppError, AppResult};
use crate::fees::ugx;
use crate::security::{Permission, Session};
use crate::state::AppState;

/// Spending heads a Ugandan primary school's books use.
pub const EXPENSE_CATEGORIES: &[&str] = &[
    "Salaries and wages",
    "Food and kitchen",
    "Stationery and printing",
    "Teaching materials",
    "Power and water",
    "Repairs and maintenance",
    "Transport and fuel",
    "Exams and UNEB",
    "Sports and co-curricular",
    "Medical",
    "Rent and land",
    "Communication and airtime",
    "Office and administration",
    "Other",
];

pub const INCOME_CATEGORIES: &[&str] = &[
    "Uniforms and items sold",
    "Grants and donations",
    "Hire of school property",
    "Farm and produce",
    "Other",
];

fn prefix(kind: &str) -> &'static str {
    match kind {
        "income" => "OI",
        "banking" => "BK",
        _ => "PV",
    }
}

pub fn voucher_label(kind: &str, n: i64) -> String {
    format!("{}-{n:05}", prefix(kind))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CashEntry {
    pub id: String,
    pub kind: String,
    pub voucher: String,
    pub on_date: String,
    pub category: String,
    pub description: String,
    pub party: Option<String>,
    pub amount: i64,
    pub method: String,
    pub reference: Option<String>,
    pub status: String,
    pub recorded_by: Option<String>,
    pub void_reason: Option<String>,
    /// "Two hundred thousand shillings only", for the voucher.
    pub amount_words: String,
}

const SELECT: &str = "SELECT e.id, e.kind, e.voucher_no, e.on_date, e.category, e.description, e.party, e.amount, e.method,
                             e.reference, e.status, u.full_name, e.void_reason
                      FROM cash_entries e LEFT JOIN users u ON u.id = e.recorded_by";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CashEntry> {
    let kind: String = r.get(1)?;
    let n: i64 = r.get(2)?;
    let amount: i64 = r.get(7)?;
    Ok(CashEntry {
        id: r.get(0)?,
        voucher: voucher_label(&kind, n),
        kind,
        on_date: r.get(3)?,
        category: r.get(4)?,
        description: r.get(5)?,
        party: r.get(6)?,
        amount,
        amount_words: crate::fees::amount_in_words(amount),
        method: r.get(8)?,
        reference: r.get(9)?,
        status: r.get(10)?,
        recorded_by: r.get(11)?,
        void_reason: r.get(12)?,
    })
}

fn load(conn: &Connection, id: &str) -> AppResult<CashEntry> {
    conn.query_row(&format!("{SELECT} WHERE e.id = ?1"), params![id], row)
        .optional()?
        .ok_or_else(|| AppError::not_found("That voucher"))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CashEntryInput {
    /// `expense`, `income` or `banking`.
    pub kind: String,
    pub on_date: Option<String>,
    pub category: String,
    pub description: String,
    pub party: Option<String>,
    pub amount: i64,
    pub method: String,
    pub reference: Option<String>,
}

fn clean(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

pub fn record_in(conn: &Connection, session: &Session, input: &CashEntryInput) -> AppResult<CashEntry> {
    session.require(Permission::ManageFees)?;
    let kind = input.kind.as_str();
    let (category, description) = match kind {
        "expense" | "income" => {
            let list = if kind == "expense" { EXPENSE_CATEGORIES } else { INCOME_CATEGORIES };
            if !list.contains(&input.category.as_str()) {
                return Err(AppError::validation("Choose what kind of spending or income this is."));
            }
            let description = input.description.trim();
            if description.len() < 3 {
                return Err(AppError::validation("Say what it was for, e.g. \"20 bags of maize flour\"."));
            }
            (input.category.clone(), description.to_string())
        }
        "banking" => ("Cash banked".to_string(), clean(Some(&input.description)).unwrap_or_else(|| "Cash taken to the bank".into())),
        _ => return Err(AppError::validation("Choose spending, income or banking.")),
    };
    if input.amount <= 0 || input.amount > 500_000_000 {
        return Err(AppError::validation("Check the amount."));
    }
    let method = if kind == "banking" { "cash" } else { input.method.as_str() };
    if !matches!(method, "cash" | "mobile_money" | "bank" | "cheque") {
        return Err(AppError::validation("Choose how the money moved."));
    }
    let reference = clean(input.reference.as_deref());
    if kind == "banking" && reference.is_none() {
        return Err(AppError::validation("Enter the bank deposit slip number."));
    }
    if matches!(method, "mobile_money" | "bank" | "cheque") && reference.is_none() {
        return Err(AppError::validation("Enter the transaction ID, slip or cheque number."));
    }
    let today = Local::now().date_naive();
    let on_date = match clean(input.on_date.as_deref()) {
        None => today,
        Some(t) => NaiveDate::parse_from_str(&t, "%Y-%m-%d").map_err(|_| AppError::validation("Choose the date."))?,
    };
    if on_date > today {
        return Err(AppError::validation("Money cannot move in the future."));
    }
    if (today - on_date).num_days() > 62 {
        return Err(AppError::validation("That is more than two months ago. Record it on the day, or ask the School Admin."));
    }
    if kind != "income" && method == "cash" {
        let at_hand = cash_at_hand(conn, &on_date.format("%Y-%m-%d").to_string())?;
        if input.amount > at_hand {
            return Err(AppError::validation(format!(
                "That is more cash than the books say was at hand ({}). Record the money that came in first.",
                ugx(at_hand)
            )));
        }
    }
    let n: i64 = conn.query_row("SELECT COALESCE(MAX(voucher_no), 0) + 1 FROM cash_entries WHERE kind = ?1", params![kind], |r| r.get(0))?;
    let id = new_id("cash");
    conn.execute(
        "INSERT INTO cash_entries (id, kind, voucher_no, on_date, category, description, party, amount, method, reference, recorded_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id,
            kind,
            n,
            on_date.format("%Y-%m-%d").to_string(),
            category,
            description,
            clean(input.party.as_deref()),
            input.amount,
            method,
            reference,
            session.user_id,
            Utc::now().to_rfc3339()
        ],
    )?;
    let what = match kind {
        "expense" => "Spent",
        "income" => "Received",
        _ => "Banked",
    };
    audit::record(conn, Some(session), "cash.record", "cash_entry", &id, format!("{} {what} {} ({category}): {description}", voucher_label(kind, n), ugx(input.amount)))?;
    load(conn, &id)
}

#[tauri::command]
pub fn record_cash_entry(state: State<'_, AppState>, input: CashEntryInput) -> AppResult<CashEntry> {
    let session = state.sessions.require()?;
    record_in(&state.db.lock(), &session, &input)
}

pub fn void_in(conn: &Connection, session: &Session, id: &str, reason: &str) -> AppResult<CashEntry> {
    session.require(Permission::VoidReceipts)?;
    let reason = reason.trim();
    if reason.len() < 5 {
        return Err(AppError::validation("Say why this voucher is being cancelled. It is kept on record."));
    }
    let entry = load(conn, id)?;
    if entry.status == "voided" {
        return Err(AppError::conflict("That voucher is already cancelled."));
    }
    let by: Option<String> = conn.query_row("SELECT recorded_by FROM cash_entries WHERE id = ?1", params![id], |r| r.get(0))?;
    if by.as_deref() == Some(session.user_id.as_str()) && !session.is_school_admin() {
        return Err(AppError::validation("You wrote this voucher, so someone else must cancel it."));
    }
    conn.execute(
        "UPDATE cash_entries SET status = 'voided', voided_by = ?1, voided_at = ?2, void_reason = ?3 WHERE id = ?4",
        params![session.user_id, Utc::now().to_rfc3339(), reason, id],
    )?;
    audit::record(conn, Some(session), "cash.void", "cash_entry", id, format!("Cancelled {} ({}): {reason}", entry.voucher, ugx(entry.amount)))?;
    load(conn, id)
}

#[tauri::command]
pub fn void_cash_entry(state: State<'_, AppState>, id: String, reason: String) -> AppResult<CashEntry> {
    let session = state.sessions.require()?;
    void_in(&state.db.lock(), &session, &id, &reason)
}

/// Cash that should be in the drawer at the end of `date`: cash fees and
/// other cash income, less cash spent and cash banked.
pub fn cash_at_hand(conn: &Connection, date: &str) -> AppResult<i64> {
    let fees: i64 = conn.query_row(
        "SELECT COALESCE(SUM(amount), 0) FROM fee_payments WHERE status = 'valid' AND method = 'cash' AND paid_on <= ?1",
        params![date],
        |r| r.get(0),
    )?;
    let moved: i64 = conn.query_row(
        "SELECT COALESCE(SUM(CASE WHEN kind = 'income' THEN amount ELSE -amount END), 0)
         FROM cash_entries WHERE status = 'valid' AND method = 'cash' AND on_date <= ?1",
        params![date],
        |r| r.get(0),
    )?;
    Ok(fees + moved)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    pub category: String,
    pub amount: i64,
    pub entries: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CashSummary {
    pub from: String,
    pub to: String,
    pub fees: i64,
    pub other_income: i64,
    pub income: i64,
    pub spent: i64,
    /// Income less spending; banking is neither.
    pub surplus: i64,
    pub banked: i64,
    pub cash_at_start: i64,
    pub cash_at_end: i64,
    pub spending: Vec<CategoryTotal>,
    pub income_by: Vec<CategoryTotal>,
    pub entries: Vec<CashEntry>,
    pub expense_categories: Vec<String>,
    pub income_categories: Vec<String>,
}

pub fn summary(conn: &Connection, from: NaiveDate, to: NaiveDate) -> AppResult<CashSummary> {
    if to < from {
        return Err(AppError::validation("The end date is before the start."));
    }
    let (a, b) = (from.format("%Y-%m-%d").to_string(), to.format("%Y-%m-%d").to_string());
    let fees: i64 = conn.query_row(
        "SELECT COALESCE(SUM(amount), 0) FROM fee_payments WHERE status = 'valid' AND paid_on BETWEEN ?1 AND ?2",
        params![a, b],
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare(&format!("{SELECT} WHERE e.on_date BETWEEN ?1 AND ?2 ORDER BY e.on_date DESC, e.created_at DESC"))?;
    let entries = stmt.query_map(params![a, b], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut spending: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut income_by: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let (mut spent, mut other, mut banked) = (0, 0, 0);
    for e in entries.iter().filter(|e| e.status == "valid") {
        match e.kind.as_str() {
            "expense" => {
                spent += e.amount;
                let t = spending.entry(e.category.clone()).or_default();
                t.0 += e.amount;
                t.1 += 1;
            }
            "income" => {
                other += e.amount;
                let t = income_by.entry(e.category.clone()).or_default();
                t.0 += e.amount;
                t.1 += 1;
            }
            _ => banked += e.amount,
        }
    }
    let ranked = |m: BTreeMap<String, (i64, i64)>| {
        let mut v: Vec<CategoryTotal> = m.into_iter().map(|(category, (amount, entries))| CategoryTotal { category, amount, entries }).collect();
        v.sort_by(|x, y| y.amount.cmp(&x.amount));
        v
    };
    let day_before = from.pred_opt().unwrap_or(from).format("%Y-%m-%d").to_string();
    Ok(CashSummary {
        from: a,
        to: b.clone(),
        fees,
        other_income: other,
        income: fees + other,
        spent,
        surplus: fees + other - spent,
        banked,
        cash_at_start: cash_at_hand(conn, &day_before)?,
        cash_at_end: cash_at_hand(conn, &b)?,
        spending: ranked(spending),
        income_by: ranked(income_by),
        entries,
        expense_categories: EXPENSE_CATEGORIES.iter().map(|s| s.to_string()).collect(),
        income_categories: INCOME_CATEGORIES.iter().map(|s| s.to_string()).collect(),
    })
}

fn parse(d: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| AppError::validation("Choose the dates."))
}

fn may_read(session: &Session) -> AppResult<()> {
    if session.has(Permission::ViewFinance) || session.has(Permission::ManageFees) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

#[tauri::command]
pub fn cash_summary(state: State<'_, AppState>, from: String, to: String) -> AppResult<CashSummary> {
    let session = state.sessions.require()?;
    may_read(&session)?;
    summary(&state.db.lock(), parse(&from)?, parse(&to)?)
}

#[tauri::command]
pub fn build_cash_summary(state: State<'_, AppState>, from: String, to: String) -> AppResult<DocumentEnvelope<CashSummary>> {
    let session = state.sessions.require()?;
    may_read(&session)?;
    let conn = state.db.lock();
    let body = summary(&conn, parse(&from)?, parse(&to)?)?;
    envelope(&conn, "cash_summary", body)
}

/// A payment voucher to be signed by whoever took the money.
#[tauri::command]
pub fn build_voucher(state: State<'_, AppState>, id: String) -> AppResult<DocumentEnvelope<CashEntry>> {
    let session = state.sessions.require()?;
    may_read(&session)?;
    let conn = state.db.lock();
    let entry = load(&conn, &id)?;
    envelope(&conn, "voucher", entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fees::tests::fee_school;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    fn input(kind: &str, category: &str, amount: i64, method: &str, reference: Option<&str>) -> CashEntryInput {
        CashEntryInput {
            kind: kind.into(),
            on_date: None,
            category: category.into(),
            description: "Maize flour, 10 bags".into(),
            party: Some("Kalerwe market".into()),
            amount,
            method: method.into(),
            reference: reference.map(String::from),
        }
    }

    #[test]
    fn cash_at_hand_follows_fees_spending_and_banking() {
        let db = fee_school();
        let conn = db.lock();
        let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
        conn.execute(
            "INSERT INTO fee_payments (id, receipt_no, student_id, term_id, amount, method, paid_on, created_at)
             VALUES ('px', 900, 'amina', 't262', 300000, 'cash', ?1, 'now')",
            params![today],
        )
        .unwrap();
        let before = cash_at_hand(&conn, &today).unwrap();
        let bursar = test_session("bursar", Role::Teacher, Some(Office::Bursar), &[]);
        let food = record_in(&conn, &bursar, &input("expense", "Food and kitchen", 120_000, "cash", None)).unwrap();
        assert_eq!(food.voucher, "PV-00001");
        record_in(&conn, &bursar, &input("income", "Uniforms and items sold", 40_000, "cash", None)).unwrap();
        assert!(record_in(&conn, &bursar, &input("banking", "", 100_000, "cash", None)).is_err(), "a deposit slip is needed");
        let bank = record_in(&conn, &bursar, &input("banking", "", 100_000, "cash", Some("DS-55"))).unwrap();
        assert_eq!(bank.voucher, "BK-00001");
        assert_eq!(cash_at_hand(&conn, &today).unwrap(), before - 120_000 + 40_000 - 100_000);
        assert!(record_in(&conn, &bursar, &input("expense", "Medical", before, "cash", None)).is_err(), "more cash than is at hand");
        assert!(record_in(&conn, &bursar, &input("expense", "Snacks", 1_000, "cash", None)).is_err(), "unknown category");
        assert!(record_in(&conn, &bursar, &input("expense", "Power and water", 90_000, "mobile_money", None)).is_err(), "needs the transaction ID");

        let d = Local::now().date_naive();
        let s = summary(&conn, d, d).unwrap();
        assert_eq!(s.spent, 120_000);
        assert_eq!(s.other_income, 40_000);
        assert_eq!(s.banked, 100_000);
        assert_eq!(s.surplus, s.fees + 40_000 - 120_000);
        assert_eq!(s.cash_at_end, cash_at_hand(&conn, &today).unwrap());
        assert_eq!(s.spending[0].category, "Food and kitchen");

        // The person who wrote it cannot cancel it; the head can, and it stays on record.
        assert!(void_in(&conn, &bursar, &food.id, "wrong amount").is_err());
        let head = test_session("head", Role::Teacher, Some(Office::HeadTeacher), &[]);
        assert!(void_in(&conn, &head, &food.id, "no").is_err(), "a reason is needed");
        void_in(&conn, &head, &food.id, "Entered twice").unwrap();
        assert_eq!(summary(&conn, d, d).unwrap().spent, 0);
        assert!(conn.execute("UPDATE cash_entries SET amount = 1 WHERE id = ?1", params![food.id]).is_err());
        assert!(conn.execute("DELETE FROM cash_entries", []).is_err());
        let teacher = test_session("bursar", Role::Teacher, None, &[]);
        assert!(matches!(record_in(&conn, &teacher, &input("expense", "Medical", 1, "cash", None)), Err(AppError::Forbidden)));
    }
}
