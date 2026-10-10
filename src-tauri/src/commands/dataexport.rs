//! FR-G10: the school's data in one Excel workbook, to keep, to share with
//! an accountant or the proprietor, or to take elsewhere. The school owns its
//! data; this is the proof.
//!
//! Each sheet is included only for someone allowed to read it: marks for
//! whoever reads every class, money for whoever reads the finances.

use chrono::Local;
use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::State;

use crate::audit;
use crate::commands::exports::{self, staged, Cell, Sheet, StagedFile};
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::{Permission, Session};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSection {
    pub id: String,
    pub label: String,
    pub allowed: bool,
}

fn allowed(session: &Session, id: &str) -> bool {
    match id {
        "learners" | "attendance" => session.has(Permission::ViewAllLearners),
        "staff" => session.has(Permission::ManageStaff),
        "marks" => session.has(Permission::ViewAllClasses),
        "fees" | "payments" | "cash" => session.has(Permission::ViewFinance),
        _ => false,
    }
}

const SECTIONS: &[(&str, &str)] = &[
    ("learners", "Learners and parents"),
    ("staff", "Staff"),
    ("marks", "Marks this term"),
    ("attendance", "Attendance this term"),
    ("fees", "Fee balances this term"),
    ("payments", "Receipts this year"),
    ("cash", "Spending and other income this year"),
];

#[tauri::command]
pub fn export_sections(state: State<'_, AppState>) -> AppResult<Vec<ExportSection>> {
    let session = state.sessions.require()?;
    Ok(SECTIONS.iter().map(|(id, label)| ExportSection { id: id.to_string(), label: label.to_string(), allowed: allowed(&session, id) }).collect())
}

fn rows<F>(conn: &Connection, sql: &str, args: &[&dyn rusqlite::ToSql], mut f: F) -> AppResult<Vec<Vec<Cell>>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<Vec<Cell>>,
{
    let mut stmt = conn.prepare(sql)?;
    let out = stmt.query_map(args, |r| f(r))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out)
}

fn text(r: &rusqlite::Row<'_>, i: usize) -> rusqlite::Result<Cell> {
    Ok(Cell::from(r.get::<_, Option<String>>(i)?))
}

fn num(r: &rusqlite::Row<'_>, i: usize) -> rusqlite::Result<Cell> {
    Ok(r.get::<_, Option<f64>>(i)?.map(Cell::Number).unwrap_or(Cell::Empty))
}

pub fn build(conn: &Connection, session: &Session, sections: &[String]) -> AppResult<(Vec<u8>, Vec<String>)> {
    let year = repo::current_academic_year_id(conn)?.ok_or_else(|| AppError::validation("No academic year is active yet."))?;
    let term = crate::fees::payment_term(conn)?;
    let school: String = conn.query_row("SELECT name FROM institution LIMIT 1", [], |r| r.get(0)).unwrap_or_default();
    let stamp = Local::now().format("%-d %b %Y %H:%M").to_string();
    let title = |what: &str| format!("{school}: {what} (exported {stamp})");
    let mut sheets = Vec::new();
    let mut included = Vec::new();

    for id in sections {
        if !allowed(session, id) {
            continue;
        }
        let sheet = match id.as_str() {
            "learners" => {
                let mut s = Sheet::new("Learners", &["Reg no.", "Name", "Sex", "Date of birth", "LIN", "Class", "Stream", "Parent / guardian", "Relationship", "Phone", "Address", "Special needs"]).titled(title("learners"));
                s.rows = rows(
                    conn,
                    "SELECT s.reg_number, s.full_name, s.gender, s.date_of_birth, s.lin, c.name, st.name, s.guardian_name,
                            s.guardian_relationship, s.guardian_phone, s.address, s.special_needs
                     FROM enrollments e JOIN students s ON s.id = e.student_id JOIN classes c ON c.id = e.class_id
                     LEFT JOIN streams st ON st.id = e.stream_id
                     WHERE e.academic_year_id = ?1 AND e.status = 'active'
                     ORDER BY c.ladder_position, s.full_name COLLATE NOCASE",
                    &[&year],
                    |r| (0..12).map(|i| text(r, i)).collect(),
                )?;
                s
            }
            "staff" => {
                let mut s = Sheet::new("Staff", &["Name", "Username", "Office", "Sex", "Phone", "Email", "NIN", "TMIS number", "Last signed in"]).titled(title("staff"));
                s.rows = rows(
                    conn,
                    "SELECT full_name, username, COALESCE(office, CASE role WHEN 'school_admin' THEN 'School Admin' ELSE 'Teacher' END),
                            gender, phone, email, nin, tmis_number, last_login_at
                     FROM users WHERE status = 'active' ORDER BY full_name COLLATE NOCASE",
                    &[],
                    |r| (0..9).map(|i| text(r, i)).collect(),
                )?;
                s
            }
            "marks" => {
                let Some(term) = &term else { continue };
                let mut s = Sheet::new("Marks", &["Class", "Learner", "Reg no.", "Subject", "Exam", "Score", "Out of", "Percent"]).titled(title("marks this term"));
                s.rows = rows(
                    conn,
                    "SELECT c.name, s.full_name, s.reg_number, COALESCE(cs.display_name, sub.name), x.name,
                            CASE WHEN m.is_absent = 1 THEN NULL ELSE m.score END,
                            COALESCE(cs.max_score, sub.max_score),
                            CASE WHEN m.is_absent = 1 THEN NULL ELSE ROUND(m.score * 100.0 / COALESCE(cs.max_score, sub.max_score), 1) END,
                            m.is_absent
                     FROM marks m JOIN exams x ON x.id = m.exam_id JOIN class_subjects cs ON cs.id = m.class_subject_id
                     JOIN subjects sub ON sub.id = cs.subject_id JOIN classes c ON c.id = cs.class_id
                     JOIN students s ON s.id = m.student_id
                     WHERE x.term_id = ?1
                     ORDER BY c.ladder_position, s.full_name COLLATE NOCASE, cs.position, x.seq",
                    &[term],
                    |r| {
                        let absent: i64 = r.get(8)?;
                        Ok(vec![
                            text(r, 0)?,
                            text(r, 1)?,
                            text(r, 2)?,
                            text(r, 3)?,
                            text(r, 4)?,
                            if absent == 1 { Cell::from("Absent") } else { num(r, 5)? },
                            num(r, 6)?,
                            num(r, 7)?,
                        ])
                    },
                )?;
                s
            }
            "attendance" => {
                let Some(term) = &term else { continue };
                let mut s = Sheet::new("Attendance", &["Class", "Learner", "Reg no.", "Present", "Late", "Absent", "Excused", "Days marked", "Attendance %"]).titled(title("attendance this term"));
                s.rows = rows(
                    conn,
                    "SELECT c.name, s.full_name, s.reg_number,
                            SUM(a.state = 'present'), SUM(a.state = 'late'), SUM(a.state = 'absent'), SUM(a.state = 'excused'), COUNT(a.id),
                            ROUND(SUM(a.state IN ('present', 'late')) * 100.0 / NULLIF(SUM(a.state <> 'excused'), 0), 1)
                     FROM enrollments e JOIN students s ON s.id = e.student_id JOIN classes c ON c.id = e.class_id
                     LEFT JOIN attendance a ON a.student_id = s.id AND a.term_id = ?2
                     WHERE e.academic_year_id = ?1 AND e.status = 'active'
                     GROUP BY s.id ORDER BY c.ladder_position, s.full_name COLLATE NOCASE",
                    &[&year, term],
                    |r| Ok(vec![text(r, 0)?, text(r, 1)?, text(r, 2)?, num(r, 3)?, num(r, 4)?, num(r, 5)?, num(r, 6)?, num(r, 7)?, num(r, 8)?]),
                )?;
                s
            }
            "fees" => {
                let Some(term) = &term else { continue };
                let list = crate::commands::fees::fee_accounts(conn, term, None)?;
                let mut s = Sheet::new("Fee balances", &["Class", "Learner", "Reg no.", "Parent phone", "Brought forward", "Billed this term", "Paid this term", "Balance"]).titled(title(&format!("fee balances, {}", list.term_name)));
                for r in list.rows {
                    s.push(vec![
                        r.class_name.into(),
                        r.full_name.into(),
                        r.reg_number.into(),
                        r.guardian_phone.into(),
                        r.brought_forward.into(),
                        r.billed.into(),
                        r.paid.into(),
                        r.balance.into(),
                    ]);
                }
                s
            }
            "payments" => {
                let mut s = Sheet::new("Receipts", &["Receipt", "Date", "Learner", "Amount", "Method", "Reference", "Paid by", "Received by", "Status", "Cancelled because"]).titled(title("receipts this year"));
                s.rows = rows(
                    conn,
                    "SELECT printf('RCT-%06d', p.receipt_no), p.paid_on, s.full_name, p.amount, p.method, p.reference, p.paid_by,
                            u.full_name, p.status, p.void_reason
                     FROM fee_payments p JOIN students s ON s.id = p.student_id LEFT JOIN users u ON u.id = p.received_by
                     JOIN terms t ON t.id = p.term_id WHERE t.academic_year_id = ?1
                     ORDER BY p.receipt_no",
                    &[&year],
                    |r| Ok(vec![text(r, 0)?, text(r, 1)?, text(r, 2)?, num(r, 3)?, text(r, 4)?, text(r, 5)?, text(r, 6)?, text(r, 7)?, text(r, 8)?, text(r, 9)?]),
                )?;
                s
            }
            "cash" => {
                let mut s = Sheet::new("Spending and income", &["Voucher", "Date", "Kind", "Category", "Description", "Paid to / from", "Amount", "Method", "Reference", "Status"]).titled(title("spending and other income this year"));
                let since: Option<String> = conn
                    .query_row("SELECT MIN(start_date) FROM terms WHERE academic_year_id = ?1", params![year], |r| r.get(0))
                    .unwrap_or(None);
                s.rows = rows(
                    conn,
                    "SELECT CASE kind WHEN 'income' THEN 'OI' WHEN 'banking' THEN 'BK' ELSE 'PV' END || printf('-%05d', voucher_no),
                            on_date, kind, category, description, party, amount, method, reference, status
                     FROM cash_entries WHERE on_date >= COALESCE(?1, '0000') ORDER BY on_date, created_at",
                    &[&since],
                    |r| Ok(vec![text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?, num(r, 6)?, text(r, 7)?, text(r, 8)?, text(r, 9)?]),
                )?;
                s
            }
            _ => continue,
        };
        included.push(sheet.name.clone());
        sheets.push(sheet);
    }
    if sheets.is_empty() {
        return Err(AppError::validation("Choose at least one thing to export that you are allowed to read."));
    }
    Ok((exports::workbook(&sheets)?, included))
}

#[tauri::command]
pub fn export_school_data(state: State<'_, AppState>, sections: Vec<String>) -> AppResult<StagedFile> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let (bytes, included) = build(&conn, &session, &sections)?;
    let school: String = conn.query_row("SELECT name FROM institution LIMIT 1", [], |r| r.get(0)).unwrap_or_default();
    let name = format!("{school} data {}.xlsx", Local::now().format("%Y-%m-%d"));
    let file = exports::stage(&state.paths.export_dir, &name, &bytes)?;
    audit::record(&conn, Some(&session), "data.export", "school", "export", format!("Exported to Excel: {}", included.join(", ")))?;
    Ok(staged(file, "xlsx", included.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fees::tests::fee_school;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    #[test]
    fn each_reader_gets_only_the_sheets_they_may_read() {
        let db = fee_school();
        let conn = db.lock();
        let all: Vec<String> = SECTIONS.iter().map(|(id, _)| id.to_string()).collect();
        let admin = test_session("head", Role::SchoolAdmin, None, &[]);
        let (bytes, sheets) = build(&conn, &admin, &all).unwrap();
        assert_eq!(sheets, vec!["Learners", "Staff", "Marks", "Attendance", "Fee balances", "Receipts", "Spending and income"]);
        assert!(bytes.len() > 2000);

        let bursar = test_session("bursar", Role::Teacher, Some(Office::Bursar), &[]);
        let (_, sheets) = build(&conn, &bursar, &all).unwrap();
        assert!(!sheets.contains(&"Marks".to_string()), "the bursar does not read marks");
        assert!(sheets.contains(&"Fee balances".to_string()));

        let dos = test_session("head", Role::Teacher, Some(Office::DirectorOfStudies), &[]);
        let (_, sheets) = build(&conn, &dos, &all).unwrap();
        assert!(!sheets.iter().any(|s| s == "Receipts" || s == "Fee balances"), "no money for the Director of Studies");

        let teacher = test_session("bursar", Role::Teacher, None, &["p5"]);
        assert!(build(&conn, &teacher, &all).is_err());
    }
}
