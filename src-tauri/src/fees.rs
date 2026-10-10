//! FR-G5 — what every learner owes, worked out the same way everywhere.
//!
//! A bill is never stored. It is worked out from the fee structure (what each
//! class pays for each item each term), the optional items a learner takes,
//! and anything that changes one learner's bill (a bursary, a sibling
//! discount, an extra charge). Change a rate and every balance, invoice and
//! report card agrees at once; there is no second copy to fall out of step.
//!
//! What *is* stored, and can never be changed, is every payment: one numbered
//! receipt each (see migration 014).
//!
//! The account runs across terms. A balance left at the end of Term 1 is
//! brought forward into Term 2, and money paid early is a credit, so the
//! figure on a receipt is always what the parent actually owes the school.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::AppResult;

/// One term the school has opened (planning terms are not billed yet).
#[derive(Debug, Clone)]
pub struct TermRef {
    pub id: String,
    pub academic_year_id: String,
    pub name: String,
    pub year_label: String,
    pub start_date: Option<String>,
}

#[derive(Debug, Clone)]
struct Rate {
    item_id: String,
    item_name: String,
    optional: bool,
    position: i64,
    amount: i64,
}

#[derive(Debug, Clone)]
struct Enrolment {
    academic_year_id: String,
    class_id: String,
    class_name: String,
    status: String,
    left_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Adjustment {
    pub id: String,
    pub term_id: String,
    pub fee_item_id: Option<String>,
    pub kind: String,
    pub amount: Option<i64>,
    pub percent: Option<f64>,
    pub reason: String,
}

impl Adjustment {
    /// Bursaries, discounts and waivers lower what a term costs.
    pub fn is_reduction(&self) -> bool {
        matches!(self.kind.as_str(), "bursary" | "discount" | "waiver")
    }
}

pub fn adjustment_label(kind: &str) -> &'static str {
    match kind {
        "bursary" => "Bursary",
        "discount" => "Discount",
        "waiver" => "Waiver",
        "charge" => "Extra charge",
        "balance_forward" => "Balance brought forward",
        "prepayment" => "Paid before Phantom",
        _ => "Adjustment",
    }
}

pub fn method_label(method: &str) -> &'static str {
    match method {
        "cash" => "Cash",
        "mobile_money" => "Mobile Money",
        "bank" => "Bank deposit",
        "schoolpay" => "SchoolPay",
        "cheque" => "Cheque",
        _ => "Other",
    }
}

/// One line of a learner's bill for one term. Positive adds to what is owed,
/// negative takes away.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BillLine {
    pub label: String,
    pub amount: i64,
    /// `item`, or the adjustment's kind.
    pub kind: String,
}

/// A learner's position in one term.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TermAccount {
    pub term_id: String,
    pub term_name: String,
    pub year_label: String,
    pub class_name: Option<String>,
    /// Owed from earlier terms; negative is a credit carried in.
    pub brought_forward: i64,
    pub lines: Vec<BillLine>,
    /// This term's bill after bursaries and charges.
    pub billed: i64,
    pub paid: i64,
    /// What is owed now; negative means the school owes the parent a credit.
    pub balance: i64,
}

/// Everything needed to work out balances, read in a handful of queries so
/// 800 learners take milliseconds, not 800 round trips.
pub struct Ledger {
    terms: Vec<TermRef>,
    rates: HashMap<(String, String), Vec<Rate>>,
    enrolments: HashMap<String, Vec<Enrolment>>,
    optins: HashSet<(String, String, String)>,
    adjustments: HashMap<String, Vec<Adjustment>>,
    paid: HashMap<(String, String), i64>,
}

impl Ledger {
    /// Loads the ledger for some learners, or for everyone when `students`
    /// is `None`.
    pub fn load(conn: &Connection, students: Option<&[String]>) -> AppResult<Self> {
        let wanted: Option<HashSet<&str>> = students.map(|s| s.iter().map(String::as_str).collect());
        let keep = |id: &str| wanted.as_ref().map_or(true, |w| w.contains(id));

        let mut stmt = conn.prepare(
            "SELECT t.id, t.academic_year_id, t.name, y.label, t.start_date
             FROM terms t JOIN academic_years y ON y.id = t.academic_year_id
             WHERE t.status IN ('open', 'closed')
             ORDER BY y.label ASC, t.seq ASC",
        )?;
        let terms = stmt
            .query_map([], |r| {
                Ok(TermRef {
                    id: r.get(0)?,
                    academic_year_id: r.get(1)?,
                    name: r.get(2)?,
                    year_label: r.get(3)?,
                    start_date: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut rates: HashMap<(String, String), Vec<Rate>> = HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT r.term_id, r.class_id, r.fee_item_id, i.name, i.kind, i.position, r.amount
             FROM fee_rates r JOIN fee_items i ON i.id = r.fee_item_id
             WHERE r.amount > 0
             ORDER BY i.position ASC, i.name ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    Rate {
                        item_id: r.get(2)?,
                        item_name: r.get(3)?,
                        optional: r.get::<_, String>(4)? == "optional",
                        position: r.get(5)?,
                        amount: r.get(6)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (term, class, rate) in rows {
            rates.entry((term, class)).or_default().push(rate);
        }

        let mut enrolments: HashMap<String, Vec<Enrolment>> = HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT e.student_id, e.academic_year_id, e.class_id, c.name, e.status, e.left_at
             FROM enrollments e JOIN classes c ON c.id = e.class_id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    Enrolment {
                        academic_year_id: r.get(1)?,
                        class_id: r.get(2)?,
                        class_name: r.get(3)?,
                        status: r.get(4)?,
                        left_at: r.get(5)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (student, enrolment) in rows {
            if keep(&student) {
                enrolments.entry(student).or_default().push(enrolment);
            }
        }

        let mut stmt = conn.prepare(
            "SELECT term_id, fee_item_id, student_id FROM fee_optins WHERE status = 'active'",
        )?;
        let optins = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<HashSet<(String, String, String)>>>()?;

        let mut adjustments: HashMap<String, Vec<Adjustment>> = HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT id, student_id, term_id, fee_item_id, kind, amount, percent, reason
             FROM fee_adjustments WHERE status = 'active' ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(1)?,
                    Adjustment {
                        id: r.get(0)?,
                        term_id: r.get(2)?,
                        fee_item_id: r.get(3)?,
                        kind: r.get(4)?,
                        amount: r.get(5)?,
                        percent: r.get(6)?,
                        reason: r.get(7)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (student, adjustment) in rows {
            if keep(&student) {
                adjustments.entry(student).or_default().push(adjustment);
            }
        }

        let mut paid = HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT student_id, term_id, SUM(amount) FROM fee_payments
             WHERE status = 'valid' GROUP BY student_id, term_id",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (student, term, amount) in rows {
            if keep(&student) {
                paid.insert((student, term), amount);
            }
        }

        Ok(Self { terms, rates, enrolments, optins, adjustments, paid })
    }

    pub fn terms(&self) -> &[TermRef] {
        &self.terms
    }

    /// The class a learner was in during a term, if they were billed for it.
    fn enrolment_for(&self, student: &str, term: &TermRef) -> Option<&Enrolment> {
        let enrolment = self
            .enrolments
            .get(student)?
            .iter()
            .find(|e| e.academic_year_id == term.academic_year_id)?;
        // A learner who left before a term began is not charged for it.
        if enrolment.status == "dropped" {
            match (&enrolment.left_at, &term.start_date) {
                (Some(left), Some(start)) if left.as_str() >= start.as_str() => {}
                _ => return None,
            }
        }
        Some(enrolment)
    }

    /// This term's bill, line by line.
    pub fn bill(&self, student: &str, term: &TermRef) -> (Option<String>, Vec<BillLine>) {
        let mut lines = Vec::new();
        let mut item_amounts: HashMap<&str, i64> = HashMap::new();
        let mut class_name = None;

        if let Some(enrolment) = self.enrolment_for(student, term) {
            class_name = Some(enrolment.class_name.clone());
            if let Some(rates) = self.rates.get(&(term.id.clone(), enrolment.class_id.clone())) {
                let mut rates: Vec<&Rate> = rates.iter().collect();
                rates.sort_by(|a, b| a.position.cmp(&b.position).then(a.item_name.cmp(&b.item_name)));
                for rate in rates {
                    let taken = !rate.optional
                        || self.optins.contains(&(term.id.clone(), rate.item_id.clone(), student.to_string()));
                    if taken {
                        item_amounts.insert(rate.item_id.as_str(), rate.amount);
                        lines.push(BillLine { label: rate.item_name.clone(), amount: rate.amount, kind: "item".into() });
                    }
                }
            }
        }

        let items_total: i64 = item_amounts.values().sum();
        let mut reductions = 0i64;
        if let Some(adjustments) = self.adjustments.get(student) {
            for adjustment in adjustments.iter().filter(|a| a.term_id == term.id) {
                let base = match &adjustment.fee_item_id {
                    Some(item) => item_amounts.get(item.as_str()).copied().unwrap_or(0),
                    None => items_total,
                };
                let value = match (adjustment.amount, adjustment.percent) {
                    (Some(amount), _) => amount,
                    (None, Some(percent)) => ((base as f64) * percent / 100.0).round() as i64,
                    _ => 0,
                };
                let signed = if adjustment.is_reduction() || adjustment.kind == "prepayment" {
                    // A bursary or discount never takes a bill below nothing;
                    // money paid before Phantom is a credit and may.
                    let value = if adjustment.is_reduction() {
                        value.min((items_total - reductions).max(0))
                    } else {
                        value
                    };
                    if adjustment.is_reduction() {
                        reductions += value;
                    }
                    -value
                } else {
                    value
                };
                let mut label = adjustment_label(&adjustment.kind).to_string();
                if let Some(percent) = adjustment.percent {
                    label.push_str(&format!(" ({}%)", trim_percent(percent)));
                }
                if !adjustment.reason.trim().is_empty() {
                    label.push_str(&format!(": {}", adjustment.reason.trim()));
                }
                lines.push(BillLine { label, amount: signed, kind: adjustment.kind.clone() });
            }
        }
        (class_name, lines)
    }

    /// The learner's account term by term, oldest first, up to and including
    /// `upto` (every opened term when `None`).
    pub fn account(&self, student: &str, upto: Option<&str>) -> Vec<TermAccount> {
        let mut out = Vec::new();
        let mut carried = 0i64;
        for term in &self.terms {
            let (class_name, lines) = self.bill(student, term);
            let billed: i64 = lines.iter().map(|l| l.amount).sum();
            let paid = self.paid.get(&(student.to_string(), term.id.clone())).copied().unwrap_or(0);
            let balance = carried + billed - paid;
            out.push(TermAccount {
                term_id: term.id.clone(),
                term_name: term.name.clone(),
                year_label: term.year_label.clone(),
                class_name,
                brought_forward: carried,
                lines,
                billed,
                paid,
                balance,
            });
            carried = balance;
            if upto == Some(term.id.as_str()) {
                break;
            }
        }
        out
    }

    /// The account as it stands at the end of `term` (or of the latest term).
    pub fn position(&self, student: &str, term: Option<&str>) -> Option<TermAccount> {
        self.account(student, term).pop()
    }
}

fn trim_percent(percent: f64) -> String {
    if (percent - percent.round()).abs() < 1e-9 {
        format!("{}", percent as i64)
    } else {
        format!("{percent:.1}")
    }
}

/// The term payments are recorded against now: the open term, or the most
/// recent one the school has used.
pub fn payment_term(conn: &Connection) -> AppResult<Option<String>> {
    use rusqlite::OptionalExtension;
    if let Some(open) = crate::domain::repo::current_term_id(conn)? {
        return Ok(Some(open));
    }
    Ok(conn
        .query_row(
            "SELECT t.id FROM terms t JOIN academic_years y ON y.id = t.academic_year_id
             WHERE t.status IN ('open', 'closed')
             ORDER BY y.label DESC, t.seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?)
}

/// The receipt number the next payment gets.
pub fn next_receipt_no(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("SELECT COALESCE(MAX(receipt_no), 0) + 1 FROM fee_payments", [], |r| r.get(0))?)
}

pub fn receipt_label(number: i64) -> String {
    format!("RCT-{number:06}")
}

/// "UGX 1,250,000".
pub fn ugx(amount: i64) -> String {
    format!("{}UGX {}", if amount < 0 { "-" } else { "" }, thousands(amount.unsigned_abs()))
}

/// "1,250,000".
pub fn thousands(amount: u64) -> String {
    let digits = amount.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    grouped
}

/// An amount in words, the way a Ugandan receipt book writes it:
/// "Two hundred and fifty thousand shillings only".
pub fn amount_in_words(amount: i64) -> String {
    if amount == 0 {
        return "Zero shillings only".into();
    }
    let words = number_words(amount.unsigned_abs());
    let mut out = String::new();
    if amount < 0 {
        out.push_str("minus ");
    }
    out.push_str(&words);
    out.push_str(" shillings only");
    let mut chars = out.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => out,
    }
}

fn number_words(n: u64) -> String {
    const SCALES: [(u64, &str); 3] = [(1_000_000_000, "billion"), (1_000_000, "million"), (1_000, "thousand")];
    let mut n = n;
    let mut parts: Vec<String> = Vec::new();
    for (size, name) in SCALES {
        if n >= size {
            parts.push(format!("{} {name}", below_thousand(n / size)));
            n %= size;
        }
    }
    if n > 0 {
        // British usage: "one thousand and five", "two million and forty".
        if !parts.is_empty() && n < 100 {
            parts.push(format!("and {}", below_thousand(n)));
        } else {
            parts.push(below_thousand(n));
        }
    }
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.push_str(if part.starts_with("and ") { " " } else { ", " });
        }
        out.push_str(part);
    }
    out
}

fn below_thousand(n: u64) -> String {
    const ONES: [&str; 20] = [
        "", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
        "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
    ];
    const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
    let hundreds = n / 100;
    let rest = n % 100;
    let rest_words = if rest < 20 {
        ONES[rest as usize].to_string()
    } else if rest % 10 == 0 {
        TENS[(rest / 10) as usize].to_string()
    } else {
        format!("{}-{}", TENS[(rest / 10) as usize], ONES[(rest % 10) as usize])
    };
    match (hundreds, rest) {
        (0, _) => rest_words,
        (h, 0) => format!("{} hundred", ONES[h as usize]),
        (h, _) => format!("{} hundred and {rest_words}", ONES[h as usize]),
    }
}

/// How many learners in a list owe more than nothing, and how much in all.
pub fn owing_total(accounts: &[TermAccount]) -> (usize, i64) {
    accounts
        .iter()
        .filter(|a| a.balance > 0)
        .fold((0, 0), |(n, sum), a| (n + 1, sum + a.balance))
}

/// Learners in a class (or the whole school) this academic year, in name
/// order, for lists and invoices.
pub fn learners_in(
    conn: &Connection,
    academic_year_id: &str,
    class_id: Option<&str>,
) -> AppResult<Vec<(String, String, String, String, Option<String>)>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.full_name, s.reg_number, c.name, s.guardian_phone
         FROM enrollments e
         JOIN students s ON s.id = e.student_id
         JOIN classes c ON c.id = e.class_id
         WHERE e.academic_year_id = ?1 AND e.status = 'active' AND s.status = 'active'
           AND (?2 IS NULL OR e.class_id = ?2)
         ORDER BY c.ladder_position ASC, s.full_name COLLATE NOCASE ASC",
    )?;
    let rows = stmt
        .query_map(params![academic_year_id, class_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::db::Database;

    /// Two terms of 2026 and one of 2025. P5 pays tuition 300,000 and
    /// development 50,000; lunch (optional) is 120,000. Three learners:
    /// Amina takes lunch and has a 50% tuition bursary, Musa has a 20,000
    /// discount on the whole bill, Sarah left before Term 2.
    pub fn fee_school() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.lock()
            .execute_batch(
                "INSERT INTO institution (id, name, created_at, updated_at) VALUES (1, 'Rainbow N&P', 'now', 'now');
                 INSERT INTO academic_years (id, label, status, created_at, updated_at) VALUES
                     ('y25', '2025', 'sealed', 'now', 'now'),
                     ('y26', '2026', 'active', 'now', 'now');
                 INSERT INTO terms (id, academic_year_id, seq, name, start_date, status, created_at, updated_at) VALUES
                     ('t253', 'y25', 3, 'Term 3', '2025-09-15', 'closed', 'now', 'now'),
                     ('t261', 'y26', 1, 'Term 1', '2026-02-02', 'closed', 'now', 'now'),
                     ('t262', 'y26', 2, 'Term 2', '2026-05-25', 'open', 'now', 'now'),
                     ('t263', 'y26', 3, 'Term 3', '2026-09-14', 'planning', 'now', 'now');
                 INSERT INTO classes (id, code, name, level_kind, ladder_position, created_at, updated_at) VALUES
                     ('p4', 'P4', 'P4', 'primary', 7, 'now', 'now'),
                     ('p5', 'P5', 'P5', 'primary', 8, 'now', 'now');
                 INSERT INTO users (id, username, full_name, role, password_hash, office, created_at, updated_at) VALUES
                     ('bursar', 'bursar', 'Ms Bursar', 'teacher', 'x', 'bursar', 'now', 'now'),
                     ('head', 'head', 'Mr Head', 'teacher', 'x', 'head_teacher', 'now', 'now');
                 INSERT INTO students (id, reg_number, full_name, guardian_phone, created_at, updated_at) VALUES
                     ('amina', 'R1', 'Amina N', '0772000001', 'now', 'now'),
                     ('musa',  'R2', 'Musa K',  '0772000002', 'now', 'now'),
                     ('sarah', 'R3', 'Sarah T', '0772000003', 'now', 'now');
                 INSERT INTO enrollments (id, student_id, class_id, academic_year_id, status, joined_at, left_at, created_at, updated_at) VALUES
                     ('e0', 'amina', 'p4', 'y25', 'promoted', 'now', NULL, 'now', 'now'),
                     ('e1', 'amina', 'p5', 'y26', 'active',   'now', NULL, 'now', 'now'),
                     ('e2', 'musa',  'p5', 'y26', 'active',   'now', NULL, 'now', 'now'),
                     ('e3', 'sarah', 'p5', 'y26', 'dropped',  'now', '2026-04-20', 'now', 'now');
                 INSERT INTO fee_items (id, name, kind, position, created_at, updated_at) VALUES
                     ('tuition', 'Tuition', 'termly', 1, 'now', 'now'),
                     ('dev', 'Development', 'termly', 2, 'now', 'now'),
                     ('lunch', 'Lunch', 'optional', 3, 'now', 'now');
                 INSERT INTO fee_rates (term_id, fee_item_id, class_id, amount, updated_at) VALUES
                     ('t253', 'tuition', 'p4', 280000, 'now'),
                     ('t261', 'tuition', 'p5', 300000, 'now'),
                     ('t261', 'dev',     'p5',  50000, 'now'),
                     ('t262', 'tuition', 'p5', 300000, 'now'),
                     ('t262', 'dev',     'p5',  50000, 'now'),
                     ('t262', 'lunch',   'p5', 120000, 'now'),
                     ('t263', 'tuition', 'p5', 999999, 'now');
                 INSERT INTO fee_optins (term_id, fee_item_id, student_id, changed_at) VALUES
                     ('t262', 'lunch', 'amina', 'now');
                 INSERT INTO fee_adjustments (id, student_id, term_id, fee_item_id, kind, amount, percent, reason, created_at) VALUES
                     ('a1', 'amina', 't262', 'tuition', 'bursary', NULL, 50, 'Staff child', 'now'),
                     ('a2', 'musa',  't262', NULL, 'discount', 20000, NULL, 'Sibling', 'now');
                 INSERT INTO fee_payments (id, receipt_no, student_id, term_id, amount, method, reference, paid_on, created_at) VALUES
                     ('p1', 1, 'amina', 't253', 280000, 'cash', NULL, '2025-09-20', '2025-09-20T08:00:00Z'),
                     ('p2', 2, 'amina', 't261', 300000, 'mobile_money', 'MP1', '2026-02-05', '2026-02-05T08:00:00Z'),
                     ('p3', 3, 'musa',  't262', 100000, 'cash', NULL, '2026-05-26', '2026-05-26T08:00:00Z');",
            )
            .unwrap();
        db
    }

    #[test]
    fn a_bill_adds_items_takes_bursaries_and_skips_unchosen_options() {
        let db = fee_school();
        let ledger = Ledger::load(&db.lock(), None).unwrap();
        let term2 = ledger.terms().iter().find(|t| t.id == "t262").unwrap().clone();

        let (class, amina) = ledger.bill("amina", &term2);
        assert_eq!(class.as_deref(), Some("P5"));
        let total: i64 = amina.iter().map(|l| l.amount).sum();
        // 300,000 + 50,000 + 120,000 lunch - 150,000 (half tuition).
        assert_eq!(total, 320_000);
        assert!(amina.iter().any(|l| l.label == "Bursary (50%): Staff child" && l.amount == -150_000));

        let (_, musa) = ledger.bill("musa", &term2);
        let total: i64 = musa.iter().map(|l| l.amount).sum();
        assert_eq!(total, 330_000, "no lunch for Musa; 20,000 off the whole bill");
    }

    #[test]
    fn balances_carry_from_term_to_term_and_across_years() {
        let db = fee_school();
        let ledger = Ledger::load(&db.lock(), None).unwrap();
        let account = ledger.account("amina", None);
        // Term 3 of 2025 paid in full; Term 1 of 2026 underpaid by 50,000.
        assert_eq!(account.iter().map(|t| t.term_id.as_str()).collect::<Vec<_>>(), ["t253", "t261", "t262"],
            "a term still in planning is not billed");
        assert_eq!(account[0].balance, 0);
        assert_eq!(account[1].balance, 50_000);
        assert_eq!(account[2].brought_forward, 50_000);
        assert_eq!(account[2].balance, 50_000 + 320_000);
        assert_eq!(ledger.position("amina", Some("t261")).unwrap().balance, 50_000);
    }

    #[test]
    fn a_learner_who_left_is_not_charged_for_later_terms() {
        let db = fee_school();
        let ledger = Ledger::load(&db.lock(), None).unwrap();
        let sarah = ledger.account("sarah", None);
        assert_eq!(sarah[1].billed, 350_000, "she was there in Term 1");
        assert_eq!(sarah[2].billed, 0, "she left in April, before Term 2 began");
        assert_eq!(sarah[2].balance, 350_000, "but what she owed still stands");
    }

    #[test]
    fn a_bursary_can_never_make_the_school_owe_the_parent() {
        let db = fee_school();
        db.lock()
            .execute(
                "INSERT INTO fee_adjustments (id, student_id, term_id, kind, amount, reason, created_at)
                 VALUES ('big', 'musa', 't262', 'waiver', 5000000, 'Full waiver', 'now')",
                [],
            )
            .unwrap();
        let ledger = Ledger::load(&db.lock(), None).unwrap();
        let term2 = ledger.terms().iter().find(|t| t.id == "t262").unwrap().clone();
        let (_, lines) = ledger.bill("musa", &term2);
        assert_eq!(lines.iter().map(|l| l.amount).sum::<i64>(), 0);
    }

    #[test]
    fn cancelled_adjustments_and_voided_receipts_do_not_count() {
        let db = fee_school();
        {
            let conn = db.lock();
            conn.execute(
                "UPDATE fee_adjustments SET status = 'cancelled', cancel_reason = 'wrong learner' WHERE id = 'a2'",
                [],
            )
            .unwrap();
            conn.execute(
                "UPDATE fee_payments SET status = 'voided', voided_at = 'now', void_reason = 'typo' WHERE id = 'p3'",
                [],
            )
            .unwrap();
        }
        let ledger = Ledger::load(&db.lock(), None).unwrap();
        let musa = ledger.position("musa", None).unwrap();
        // Term 1's 350,000 was never paid either, so it comes forward.
        assert_eq!((musa.brought_forward, musa.billed, musa.paid, musa.balance), (350_000, 350_000, 0, 700_000));
    }

    #[test]
    fn loading_some_learners_gives_the_same_answer_as_loading_all() {
        let db = fee_school();
        let all = Ledger::load(&db.lock(), None).unwrap();
        let some = Ledger::load(&db.lock(), Some(&["amina".to_string()])).unwrap();
        assert_eq!(all.account("amina", None), some.account("amina", None));
    }

    #[test]
    fn amounts_read_like_a_receipt_book() {
        assert_eq!(ugx(1_250_000), "UGX 1,250,000");
        assert_eq!(ugx(500), "UGX 500");
        assert_eq!(ugx(-20_000), "-UGX 20,000");
        assert_eq!(amount_in_words(250_000), "Two hundred and fifty thousand shillings only");
        assert_eq!(amount_in_words(1_005), "One thousand and five shillings only");
        assert_eq!(
            amount_in_words(1_250_500),
            "One million, two hundred and fifty thousand, five hundred shillings only"
        );
        assert_eq!(amount_in_words(70_000), "Seventy thousand shillings only");
        assert_eq!(amount_in_words(45), "Forty-five shillings only");
        assert_eq!(receipt_label(42), "RCT-000042");
    }
}
