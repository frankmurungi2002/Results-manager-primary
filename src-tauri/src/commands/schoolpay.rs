//! A SchoolPay statement, turned into receipts without typing.
//!
//! Most Ugandan parents now pay through SchoolPay, which gives the school a
//! statement (Excel or CSV) of every payment. SchoolPay publishes no way for
//! other software to read it directly, so the bursar downloads the statement
//! from the SchoolPay portal and opens it here.
//!
//! Each line is matched to a learner by, in order: the learner's SchoolPay
//! payment code, their registration number, or (only as a suggestion the
//! bursar confirms) their name. A transaction already receipted is shown
//! and skipped, so importing the same statement twice records nothing twice:
//! the database itself refuses a second receipt for one SchoolPay reference.

use std::collections::HashMap;
use std::path::Path;

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::commands::fees::{record_payment_in, RecordPaymentRequest};
use crate::error::{AppError, AppResult};
use crate::security::{Permission, Session};
use crate::state::AppState;

/// Which column holds what. Indexes into each row.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatementColumns {
    pub reference: Option<usize>,
    pub amount: Option<usize>,
    pub date: Option<usize>,
    pub payment_code: Option<usize>,
    pub reg_number: Option<usize>,
    pub name: Option<usize>,
    pub payer: Option<usize>,
}

fn guess(headers: &[String]) -> StatementColumns {
    let find = |words: &[&str]| -> Option<usize> {
        headers.iter().position(|h| {
            let h = h.to_lowercase();
            words.iter().any(|w| h.contains(w))
        })
    };
    StatementColumns {
        reference: find(&["transaction id", "transaction", "receipt", "reference", "ref", "txn"]),
        amount: find(&["amount", "paid", "value"]),
        date: find(&["date", "time"]),
        payment_code: find(&["payment code", "student code", "schoolpay code", "pay code", "code"]),
        reg_number: find(&["reg", "admission", "student no", "student number", "student id"]),
        name: find(&["student name", "learner", "pupil", "name"]),
        payer: find(&["payer", "paid by", "depositor", "parent"]),
    }
}

/// "250,000", "UGX 250000", "250000.00" → 250000.
pub fn parse_amount(text: &str) -> Option<i64> {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let value: f64 = digits.parse().ok()?;
    (value > 0.0).then(|| value.round() as i64)
}

/// The date formats statements use, and Excel's day numbers.
pub fn parse_date(text: &str) -> Option<NaiveDate> {
    let t = text.trim();
    let first = t.split([' ', 'T']).next().unwrap_or(t);
    for format in ["%Y-%m-%d", "%d/%m/%Y", "%d-%m-%Y", "%d-%b-%Y", "%d %b %Y", "%Y/%m/%d", "%d.%m.%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(first, format) {
            return Some(d);
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(t, "%d %b %Y") {
        return Some(d);
    }
    // An Excel date cell read as its serial number.
    if let Ok(serial) = t.parse::<f64>() {
        if (20_000.0..80_000.0).contains(&serial) {
            return NaiveDate::from_ymd_opt(1899, 12, 30)?.checked_add_signed(chrono::Duration::days(serial.floor() as i64));
        }
    }
    None
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementLine {
    pub row: usize,
    pub reference: String,
    pub amount: Option<i64>,
    pub paid_on: Option<String>,
    pub name_on_statement: String,
    pub payment_code: String,
    /// `ready`, `suggested` (by name: confirm it), `duplicate`, `unmatched`, `invalid`.
    pub status: String,
    pub student_id: Option<String>,
    pub student_name: Option<String>,
    pub class_name: Option<String>,
    /// How the learner was found: `code`, `reg`, `name`.
    pub matched_by: Option<String>,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementPlan {
    pub file_name: String,
    pub headers: Vec<String>,
    pub columns: StatementColumns,
    pub lines: Vec<StatementLine>,
    pub ready: usize,
    pub ready_amount: i64,
}

struct Learner {
    id: String,
    name: String,
    class: Option<String>,
}

fn learners(conn: &Connection) -> AppResult<(HashMap<String, Learner>, HashMap<String, String>, HashMap<String, String>, Vec<(String, String)>)> {
    let year = crate::domain::repo::current_academic_year_id(conn)?;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.full_name, s.reg_number, s.schoolpay_code,
                (SELECT c.name FROM enrollments e JOIN classes c ON c.id = e.class_id
                  WHERE e.student_id = s.id AND e.academic_year_id = ?1)
         FROM students s",
    )?;
    let rows = stmt
        .query_map(params![year], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut by_id = HashMap::new();
    let mut by_reg = HashMap::new();
    let mut by_code = HashMap::new();
    let mut names = Vec::new();
    for (id, name, reg, code, class) in rows {
        by_reg.insert(reg.to_lowercase(), id.clone());
        if let Some(code) = code {
            by_code.insert(code.to_lowercase(), id.clone());
        }
        names.push((normal_name(&name), id.clone()));
        by_id.insert(id.clone(), Learner { id, name, class });
    }
    Ok((by_id, by_reg, by_code, names))
}

/// Order-free, case-free: "NAKATO Sarah" and "Sarah Nakato" are one name.
fn normal_name(name: &str) -> String {
    let mut words: Vec<String> = name.split_whitespace().map(|w| w.to_lowercase().replace(['.', ','], "")).collect();
    words.sort();
    words.join(" ")
}

pub fn plan(conn: &Connection, file_name: &str, headers: Vec<String>, rows: &[Vec<String>], columns: StatementColumns) -> AppResult<StatementPlan> {
    let (by_id, by_reg, by_code, names) = learners(conn)?;
    let cell = |row: &Vec<String>, col: Option<usize>| col.and_then(|c| row.get(c)).map(|s| s.trim().to_string()).unwrap_or_default();
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let reference = cell(row, columns.reference);
        let amount = parse_amount(&cell(row, columns.amount));
        let paid_on = parse_date(&cell(row, columns.date)).map(|d| d.format("%Y-%m-%d").to_string());
        let name = cell(row, columns.name);
        let code = cell(row, columns.payment_code);
        let reg = cell(row, columns.reg_number);
        let mut line = StatementLine {
            row: index,
            reference: reference.clone(),
            amount,
            paid_on: paid_on.clone(),
            name_on_statement: name.clone(),
            payment_code: code.clone(),
            status: "unmatched".into(),
            student_id: None,
            student_name: None,
            class_name: None,
            matched_by: None,
            problem: None,
        };
        if reference.is_empty() || amount.is_none() || paid_on.is_none() {
            line.status = "invalid".into();
            line.problem = Some(
                match (reference.is_empty(), amount.is_none(), paid_on.is_none()) {
                    (true, _, _) => "No transaction ID",
                    (_, true, _) => "No amount",
                    _ => "The date cannot be read",
                }
                .into(),
            );
            lines.push(line);
            continue;
        }
        let earlier: Option<i64> = conn
            .query_row(
                "SELECT receipt_no FROM fee_payments WHERE method = 'schoolpay' AND reference = ?1 COLLATE NOCASE AND status = 'valid'",
                params![reference],
                |r| r.get(0),
            )
            .optional()?;
        let found = (!code.is_empty())
            .then(|| by_code.get(&code.to_lowercase()).map(|id| (id.clone(), "code")))
            .flatten()
            .or_else(|| (!reg.is_empty()).then(|| by_reg.get(&reg.to_lowercase()).map(|id| (id.clone(), "reg"))).flatten())
            .or_else(|| {
                let wanted = normal_name(&name);
                let hits: Vec<&String> = names.iter().filter(|(n, _)| !wanted.is_empty() && *n == wanted).map(|(_, id)| id).collect();
                (hits.len() == 1).then(|| (hits[0].clone(), "name"))
            });
        if let Some((id, how)) = found {
            let learner = &by_id[&id];
            line.student_id = Some(learner.id.clone());
            line.student_name = Some(learner.name.clone());
            line.class_name = learner.class.clone();
            line.matched_by = Some(how.into());
            line.status = if how == "name" { "suggested".into() } else { "ready".into() };
        }
        if let Some(earlier) = earlier {
            line.status = "duplicate".into();
            line.problem = Some(format!("Already receipted on {}", crate::fees::receipt_label(earlier)));
        }
        lines.push(line);
    }
    let ready: Vec<&StatementLine> = lines.iter().filter(|l| l.status == "ready").collect();
    Ok(StatementPlan {
        file_name: file_name.into(),
        headers,
        columns,
        ready: ready.len(),
        ready_amount: ready.iter().filter_map(|l| l.amount).sum(),
        lines,
    })
}

fn read(path: &str) -> AppResult<(String, Vec<String>, Vec<Vec<String>>)> {
    let file = Path::new(path);
    if !file.is_file() {
        return Err(AppError::not_found("That file"));
    }
    let (_, rows, _) = crate::commands::importer::read_rows(file, None)?;
    // The headings are the first row with at least three filled cells.
    let header_index = rows.iter().position(|r| r.iter().filter(|c| !c.is_empty()).count() >= 3).unwrap_or(0);
    let headers = rows.get(header_index).cloned().unwrap_or_default();
    let data: Vec<Vec<String>> = rows.into_iter().skip(header_index + 1).filter(|r| r.iter().any(|c| !c.is_empty())).take(5_000).collect();
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("statement").to_string();
    Ok((name, headers, data))
}

#[tauri::command]
pub fn read_schoolpay_statement(state: State<'_, AppState>, path: String, columns: Option<StatementColumns>) -> AppResult<StatementPlan> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    let (name, headers, rows) = read(&path)?;
    let columns = columns.unwrap_or_else(|| guess(&headers));
    plan(&state.db.lock(), &name, headers, &rows, columns)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportChoice {
    pub row: usize,
    pub student_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementImported {
    pub receipts: usize,
    pub amount: i64,
    pub skipped: usize,
    pub codes_learned: usize,
}

pub fn import_in(conn: &Connection, session: &Session, plan: &StatementPlan, choices: &[ImportChoice]) -> AppResult<StatementImported> {
    let mut out = StatementImported { receipts: 0, amount: 0, skipped: 0, codes_learned: 0 };
    for choice in choices {
        let Some(line) = plan.lines.get(choice.row) else { continue };
        if !matches!(line.status.as_str(), "ready" | "suggested" | "unmatched") {
            out.skipped += 1;
            continue;
        }
        let (Some(amount), Some(paid_on)) = (line.amount, line.paid_on.clone()) else {
            out.skipped += 1;
            continue;
        };
        let request = RecordPaymentRequest {
            student_id: choice.student_id.clone(),
            amount,
            method: "schoolpay".into(),
            reference: Some(line.reference.clone()),
            paid_by: None,
            payer_phone: None,
            paid_on: Some(paid_on),
            note: Some(format!("From SchoolPay statement {}", plan.file_name)),
            text_guardian: false,
        };
        match record_payment_in(conn, session, &request, "schoolpay_import") {
            Ok(_) => {
                out.receipts += 1;
                out.amount += amount;
                // Learn the learner's SchoolPay code, so next time it matches by itself.
                if !line.payment_code.is_empty() {
                    out.codes_learned += conn.execute(
                        "UPDATE students SET schoolpay_code = ?1 WHERE id = ?2 AND schoolpay_code IS NULL
                           AND NOT EXISTS (SELECT 1 FROM students WHERE schoolpay_code = ?1)",
                        params![line.payment_code, choice.student_id],
                    )?;
                }
            }
            Err(AppError::Conflict(_)) => out.skipped += 1,
            Err(other) => return Err(other),
        }
    }
    audit::record(
        conn,
        Some(session),
        "fees.schoolpay_import",
        "fees",
        &plan.file_name,
        format!("Imported {} SchoolPay payment(s), {} in all, from {}", out.receipts, crate::fees::ugx(out.amount), plan.file_name),
    )?;
    Ok(out)
}

#[tauri::command]
pub fn import_schoolpay_statement(
    state: State<'_, AppState>,
    path: String,
    columns: StatementColumns,
    choices: Vec<ImportChoice>,
) -> AppResult<StatementImported> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageFees)?;
    let (name, headers, rows) = read(&path)?;
    let conn = state.db.lock();
    let tx = conn.unchecked_transaction()?;
    let plan = plan(&tx, &name, headers, &rows, columns)?;
    let result = import_in(&tx, &session, &plan, &choices)?;
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fees::tests::fee_school;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    fn statement() -> (Vec<String>, Vec<Vec<String>>) {
        let headers = ["Transaction ID", "Payment Date", "Student Code", "Student Name", "Amount", "Channel"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let rows = vec![
            vec!["SP1001", "05/10/2026", "100200300", "N AMINA", "150,000", "MTN"],
            vec!["SP1002", "2026-10-06", "", "Musa K", "UGX 80000", "Airtel"],
            vec!["SP1003", "45936", "999", "Nobody Here", "50000", "Bank"],
            vec!["", "06/10/2026", "", "Musa K", "1000", "MTN"],
        ]
        .into_iter()
        .map(|r| r.into_iter().map(str::to_string).collect())
        .collect();
        (headers, rows)
    }

    #[test]
    fn columns_are_guessed_from_the_headings() {
        let (headers, _) = statement();
        let c = guess(&headers);
        assert_eq!((c.reference, c.date, c.payment_code, c.name, c.amount), (Some(0), Some(1), Some(2), Some(3), Some(4)));
    }

    #[test]
    fn dates_and_amounts_in_every_shape() {
        assert_eq!(parse_date("05/10/2026").unwrap().to_string(), "2026-10-05");
        assert_eq!(parse_date("2026-10-06 14:22:01").unwrap().to_string(), "2026-10-06");
        assert_eq!(parse_date("6-Oct-2026").unwrap().to_string(), "2026-10-06");
        assert_eq!(parse_date("45936").unwrap().to_string(), "2025-10-06");
        assert_eq!(parse_amount("UGX 250,000.00"), Some(250_000));
        assert_eq!(parse_amount("-"), None);
    }

    #[test]
    fn a_statement_matches_by_name_suggests_and_never_imports_twice() {
        let db = fee_school();
        let conn = db.lock();
        let bursar = test_session("bursar", Role::Teacher, Some(Office::Bursar), &[]);
        let (headers, rows) = statement();
        let columns = guess(&headers);
        let first = plan(&conn, "oct.xlsx", headers.clone(), &rows, columns.clone()).unwrap();
        let status: Vec<&str> = first.lines.iter().map(|l| l.status.as_str()).collect();
        assert_eq!(status, ["suggested", "suggested", "unmatched", "invalid"]);
        assert_eq!(first.lines[0].student_id.as_deref(), Some("amina"), "names match in any order and case");

        let choices = vec![ImportChoice { row: 0, student_id: "amina".into() }, ImportChoice { row: 1, student_id: "musa".into() }];
        let done = import_in(&conn, &bursar, &first, &choices).unwrap();
        assert_eq!((done.receipts, done.amount, done.codes_learned), (2, 230_000, 1));

        // The same statement again: both already receipted, and Amina now
        // matches by her SchoolPay code.
        let again = plan(&conn, "oct.xlsx", headers, &rows, columns).unwrap();
        assert_eq!(again.lines[0].status, "duplicate");
        assert_eq!(again.lines[0].matched_by.as_deref(), Some("code"));
        let twice = import_in(&conn, &bursar, &again, &choices).unwrap();
        assert_eq!(twice.receipts, 0);
        let method: String = conn.query_row("SELECT method FROM fee_payments WHERE reference = 'SP1001'", [], |r| r.get(0)).unwrap();
        assert_eq!(method, "schoolpay");
    }
}
