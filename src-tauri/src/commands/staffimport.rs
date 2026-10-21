//! FR-G13 for staff: a school's staff list, read from the spreadsheet it
//! already keeps, instead of typed in one by one.
//!
//! Like the learner import, two passes: the first reads and checks every row
//! and writes nothing; the second creates the accounts the office kept. Each
//! account is made by `teachers::create_staff_in`, the same checks as "Add
//! staff". Nobody is given a password: each new member of staff gets an
//! invite code (by SMS when the school is online and they have a phone, and
//! always on a printable slip), so they set their own.
//!
//! Usernames are made from the name ("joseph", then "josepho" if taken) unless
//! the sheet has a username column. Offices (bursar, secretary, director of
//! studies, headteacher) are read from a "position" or "role" column when one
//! exists; anything else makes an ordinary teacher.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::commands::importer::read_rows;
use crate::commands::invites::{invite_in, Invite};
use crate::commands::reports::{envelope, DocumentEnvelope};
use crate::commands::teachers::{create_staff_in, CreateStaffRequest};
use crate::error::{AppError, AppResult};
use crate::security::{Office, Permission, Role, Session};
use crate::sms;
use crate::state::AppState;

const MAX_ROWS: usize = 500;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StaffColumns {
    pub full_name: Option<usize>,
    pub phone: Option<usize>,
    pub email: Option<usize>,
    pub gender: Option<usize>,
    pub position: Option<usize>,
    pub username: Option<usize>,
    pub nin: Option<usize>,
    pub tmis: Option<usize>,
}

const ALIASES: &[(&str, &[&str])] = &[
    ("full_name", &["name", "names", "full name", "fullname", "staff name", "teacher name", "teacher", "employee", "employee name"]),
    ("phone", &["phone", "phone number", "telephone", "tel", "mobile", "contact", "contact number", "cell"]),
    ("email", &["email", "e-mail", "email address", "mail"]),
    ("gender", &["sex", "gender", "m/f"]),
    ("position", &["position", "role", "designation", "title", "office", "post", "responsibility", "job"]),
    ("username", &["username", "user name", "login"]),
    ("nin", &["nin", "national id", "national id number", "id number", "nid"]),
    ("tmis", &["tmis", "tmis number", "tmis no", "teacher number", "ipps", "file number"]),
];

fn clean(h: &str) -> String {
    h.trim().to_lowercase().replace(['.', ':', '_'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn guess(headers: &[String]) -> StaffColumns {
    let cleaned: Vec<String> = headers.iter().map(|h| clean(h)).collect();
    let mut taken: HashSet<usize> = HashSet::new();
    let mut found: HashMap<&str, usize> = HashMap::new();
    // Exact first, then "contains", so "Name" beats "Next of kin name".
    for exact in [true, false] {
        for (field, aliases) in ALIASES {
            if found.contains_key(field) {
                continue;
            }
            let hit = cleaned.iter().enumerate().find(|(i, h)| {
                !taken.contains(i) && !h.is_empty() && aliases.iter().any(|a| if exact { *h == a } else { h.contains(a) })
            });
            if let Some((i, _)) = hit {
                taken.insert(i);
                found.insert(field, i);
            }
        }
    }
    StaffColumns {
        full_name: found.get("full_name").copied(),
        phone: found.get("phone").copied(),
        email: found.get("email").copied(),
        gender: found.get("gender").copied(),
        position: found.get("position").copied(),
        username: found.get("username").copied(),
        nin: found.get("nin").copied(),
        tmis: found.get("tmis").copied(),
    }
}

/// "Bursar", "School secretary", "D.O.S", "Head teacher" → an office. Deputy
/// heads and everyone else are ordinary teachers.
fn office_from(position: &str) -> Option<Office> {
    let p = clean(position);
    if p.is_empty() || p.contains("deputy") || p.contains("assistant") {
        return None;
    }
    if p.contains("bursar") || p.contains("accountant") {
        Some(Office::Bursar)
    } else if p.contains("secretary") {
        Some(Office::Secretary)
    } else if p.contains("director of studies") || p == "dos" || p == "d o s" {
        Some(Office::DirectorOfStudies)
    } else if p.contains("head teacher") || p.contains("headteacher") || p == "head" || p.contains("headmaster") || p.contains("headmistress") {
        Some(Office::HeadTeacher)
    } else {
        None
    }
}

fn office_label(office: Option<Office>) -> &'static str {
    match office {
        Some(Office::HeadTeacher) => "Headteacher",
        Some(Office::DirectorOfStudies) => "Director of Studies",
        Some(Office::Secretary) => "Secretary",
        Some(Office::Bursar) => "Bursar",
        None => "Teacher",
    }
}

/// Titles people put before names in a staff list.
const TITLES: &[&str] = &["mr", "mrs", "ms", "miss", "madam", "dr", "rev", "fr", "sr", "tr", "teacher", "hon"];

fn tidy_name(raw: &str) -> String {
    let words: Vec<&str> = raw.split_whitespace().collect();
    let skip = words.iter().take_while(|w| TITLES.contains(&w.trim_end_matches('.').to_lowercase().as_str())).count();
    let kept = if skip < words.len() { &words[skip..] } else { &words[..] };
    kept.iter()
        .map(|w| {
            if w.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) && w.chars().count() > 1 {
                let lower = w.to_lowercase();
                let mut c = lower.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            } else {
                (*w).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn letters(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
}

fn make_username(name: &str, taken: &HashSet<String>) -> String {
    let words: Vec<String> = name.split_whitespace().map(letters).filter(|w| !w.is_empty()).collect();
    let first = words.first().cloned().unwrap_or_else(|| "staff".into());
    let last_initial = words.get(1).and_then(|w| w.chars().next()).map(String::from).unwrap_or_default();
    let mut candidates = vec![first.clone(), format!("{first}{last_initial}")];
    if let Some(last) = words.get(1) {
        candidates.push(format!("{first}{last}"));
    }
    for c in candidates {
        if c.len() >= 3 && !taken.contains(&c) {
            return c;
        }
    }
    let base = if first.len() >= 3 { first } else { format!("{first}staff") };
    (2..).map(|n| format!("{base}{n}")).find(|c| !taken.contains(c)).expect("some number is free")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffRowPlan {
    /// The row number as the spreadsheet shows it.
    pub row: usize,
    pub full_name: String,
    pub username: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub gender: Option<String>,
    pub office: Option<String>,
    pub office_label: String,
    pub nin: Option<String>,
    pub tmis: Option<String>,
    /// "ready", "exists" (already a member of staff) or "blocked".
    pub status: String,
    pub problems: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffImportPlan {
    pub file_name: String,
    pub headers: Vec<String>,
    pub columns: StaffColumns,
    pub rows: Vec<StaffRowPlan>,
    pub ready: usize,
}

pub fn plan_in(conn: &Connection, session: &Session, path: &Path, columns: Option<StaffColumns>) -> AppResult<StaffImportPlan> {
    session.require(Permission::ManageStaff)?;
    if !path.is_file() {
        return Err(AppError::not_found("That file"));
    }
    let (_, all_rows, _) = read_rows(path, None)?;
    let header_index = all_rows.iter().position(|row| row.iter().filter(|c| !c.is_empty()).count() >= 2).unwrap_or(0);
    let headers = all_rows.get(header_index).cloned().unwrap_or_default();
    let columns = columns.unwrap_or_else(|| guess(&headers));
    let Some(name_col) = columns.full_name else {
        return Err(AppError::validation("Choose which column has the staff names."));
    };

    let mut taken: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT LOWER(username) FROM users")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let collected = rows.collect::<rusqlite::Result<HashSet<_>>>()?;
        collected
    };
    let existing_names: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT LOWER(full_name) FROM users WHERE status = 'active'")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let collected = rows.collect::<rusqlite::Result<HashSet<_>>>()?;
        collected
    };
    let mut seen_names: HashSet<String> = HashSet::new();
    let get = |row: &[String], col: Option<usize>| col.and_then(|c| row.get(c)).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());

    let mut rows = Vec::new();
    for (offset, row) in all_rows.iter().enumerate().skip(header_index + 1).take(MAX_ROWS) {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let mut problems = Vec::new();
        let mut notes = Vec::new();
        let raw_name = get(row, Some(name_col)).unwrap_or_default();
        let full_name = tidy_name(&raw_name);
        if full_name.is_empty() {
            continue;
        }
        if full_name.split_whitespace().count() < 2 {
            problems.push("Give at least two names, so staff with the same first name are told apart.".to_string());
        }
        let key = full_name.to_lowercase();
        let exists = existing_names.contains(&key) || existing_names.contains(&raw_name.trim().to_lowercase());
        if !seen_names.insert(key) {
            problems.push("This name appears twice in the sheet.".to_string());
        }
        let phone = match get(row, columns.phone) {
            Some(p) => match sms::normalize_phone(&p) {
                Some(n) => Some(n),
                None => {
                    notes.push(format!("\"{p}\" is not a phone number Termbook can text, so it was left out."));
                    None
                }
            },
            None => None,
        };
        let email = match get(row, columns.email) {
            Some(e) if e.contains('@') && e.contains('.') && !e.contains(' ') => Some(e.to_lowercase()),
            Some(e) => {
                notes.push(format!("\"{e}\" is not an email address, so it was left out."));
                None
            }
            None => None,
        };
        let gender = get(row, columns.gender).and_then(|g| match g.to_uppercase().chars().next() {
            Some('M') => Some("M".to_string()),
            Some('F') => Some("F".to_string()),
            _ => None,
        });
        let office = get(row, columns.position).as_deref().and_then(office_from);
        if let Some(o) = office {
            if session.require_act_on(Role::Teacher, Some(o)).is_err() {
                problems.push(format!("Only someone with more powers can create a {}.", office_label(Some(o))));
            }
        }
        let nin = get(row, columns.nin).map(|n| n.to_uppercase().replace(' ', "")).and_then(|n| {
            if n.chars().count() == 14 && n.chars().all(|c| c.is_ascii_alphanumeric()) {
                Some(n)
            } else {
                notes.push("The NIN is not 14 letters and digits, so it was left out.".to_string());
                None
            }
        });
        let tmis = get(row, columns.tmis);
        let username = match get(row, columns.username).map(|u| letters(&u)) {
            Some(u) if u.len() >= 3 && !taken.contains(&u) => u,
            Some(u) if !u.is_empty() => {
                let made = make_username(&full_name, &taken);
                notes.push(format!("The username \"{u}\" is taken or too short, so it is {made}."));
                made
            }
            _ => make_username(&full_name, &taken),
        };
        let status = if exists {
            "exists"
        } else if problems.is_empty() {
            taken.insert(username.clone());
            "ready"
        } else {
            "blocked"
        };
        rows.push(StaffRowPlan {
            row: offset + 1,
            full_name,
            username,
            phone,
            email,
            gender,
            office: office.map(|o| o.as_str().to_string()),
            office_label: office_label(office).to_string(),
            nin,
            tmis,
            status: status.to_string(),
            problems,
            notes,
        });
    }
    let ready = rows.iter().filter(|r| r.status == "ready").count();
    Ok(StaffImportPlan {
        file_name: path.file_name().and_then(|n| n.to_str()).unwrap_or("staff").to_string(),
        headers,
        columns,
        rows,
        ready,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffImported {
    pub created: usize,
    pub sms_sent: usize,
    /// One invite per new account, to print as slips.
    pub invites: Vec<Invite>,
}

pub fn import_in(
    conn: &Connection,
    session: &Session,
    path: &Path,
    columns: StaffColumns,
    skip_rows: &[usize],
    send_sms: bool,
) -> AppResult<StaffImported> {
    let plan = plan_in(conn, session, path, Some(columns))?;
    let tx = conn.unchecked_transaction()?;
    let mut invites = Vec::new();
    let now = Utc::now();
    for row in plan.rows.iter().filter(|r| r.status == "ready" && !skip_rows.contains(&r.row)) {
        let made = create_staff_in(
            &tx,
            session,
            &CreateStaffRequest {
                full_name: row.full_name.clone(),
                username: row.username.clone(),
                role: "teacher".into(),
                office: row.office.clone(),
                email: row.email.clone(),
                phone: row.phone.clone(),
            },
        )?;
        tx.execute(
            "UPDATE users SET gender = ?2, nin = ?3, tmis_number = ?4 WHERE id = ?1",
            params![made.id, row.gender, row.nin, row.tmis],
        )?;
        invites.push(invite_in(&tx, session, &made.id, send_sms, now)?);
    }
    let sms_sent = invites.iter().filter(|i| i.sms_queued).count();
    audit::record(
        &tx,
        Some(session),
        "user.import",
        "user",
        "import",
        format!("Imported {} member(s) of staff from \"{}\"", invites.len(), plan.file_name),
    )?;
    tx.commit()?;
    Ok(StaffImported { created: invites.len(), sms_sent, invites })
}

#[tauri::command]
pub fn plan_staff_import(state: State<'_, AppState>, path: String, columns: Option<StaffColumns>) -> AppResult<StaffImportPlan> {
    let session = state.sessions.require()?;
    plan_in(&state.db.lock(), &session, Path::new(&path), columns)
}

#[tauri::command]
pub fn import_staff(
    state: State<'_, AppState>,
    path: String,
    columns: StaffColumns,
    skip_rows: Vec<usize>,
    send_sms: bool,
) -> AppResult<DocumentEnvelope<StaffImported>> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let done = import_in(&conn, &session, Path::new(&path), columns, &skip_rows, send_sms)?;
    envelope(&conn, "invite_slip", done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::session::test_session;

    fn sheet(rows: &[&[&str]]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tbk-staff-{}-{}", std::process::id(), rows.len()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut book = rust_xlsxwriter::Workbook::new();
        let ws = book.add_worksheet();
        ws.write_string(0, 0, "RAINBOW STAFF LIST 2026").unwrap();
        for (r, row) in rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                ws.write_string(r as u32 + 2, c as u16, *v).unwrap();
            }
        }
        let path = dir.join("staff.xlsx");
        book.save(&path).unwrap();
        path
    }

    fn admin() -> Session {
        test_session("head", Role::SchoolAdmin, None, &[])
    }

    #[test]
    fn a_staff_list_is_read_checked_and_made_into_invited_accounts() {
        let db = crate::fees::tests::fee_school();
        let path = sheet(&[
            &["S/N", "Name", "Sex", "Position", "Tel", "NIN"],
            &["1", "MR. JOSEPH OKELLO", "M", "Class teacher P5", "0772 111 222", "CM12345678ABCD"],
            &["2", "Grace Nambi", "Female", "Bursar", "", "123"],
            &["3", "Joseph Opio", "M", "Deputy head teacher", "0701999888", ""],
            &["4", "Mr Head", "M", "Head teacher", "", ""],
            &["5", "Kato", "M", "", "", ""],
            &["6", "Grace Nambi", "F", "", "", ""],
        ]);
        let plan = plan_in(&db.lock(), &admin(), &path, None).unwrap();
        assert_eq!(plan.columns.full_name, Some(1));
        assert_eq!(plan.columns.position, Some(3));
        assert_eq!(plan.columns.phone, Some(4));
        let by_row: HashMap<usize, &StaffRowPlan> = plan.rows.iter().map(|r| (r.row, r)).collect();
        let joseph = by_row[&4];
        assert_eq!((joseph.full_name.as_str(), joseph.username.as_str()), ("Joseph Okello", "joseph"));
        assert_eq!(joseph.phone.as_deref(), Some("+256772111222"));
        assert_eq!(joseph.office_label, "Teacher", "a class teacher is a teacher; classes are assigned later");
        let grace = by_row[&5];
        assert_eq!((grace.office.as_deref(), grace.gender.as_deref()), (Some("bursar"), Some("F")));
        assert!(grace.nin.is_none() && grace.notes.iter().any(|n| n.contains("NIN")));
        let opio = by_row[&6];
        assert_eq!((opio.username.as_str(), opio.office_label.as_str()), ("josepho", "Teacher"), "a deputy is not the head");
        assert_eq!(by_row[&7].status, "exists", "Mr Head is already on the staff");
        assert_eq!(by_row[&8].status, "blocked", "one name only");
        assert_eq!(by_row[&9].status, "blocked", "Grace twice");
        assert_eq!(plan.ready, 3);

        let done = import_in(&db.lock(), &admin(), &path, plan.columns.clone(), &[6], false).unwrap();
        assert_eq!(done.created, 2, "Opio was left out by the office");
        assert!(done.invites.iter().all(|i| i.code.starts_with("INV-")));
        let conn = db.lock();
        let (office, nin, must_change): (Option<String>, Option<String>, i64) = conn
            .query_row("SELECT office, nin, must_change_password FROM users WHERE username = 'joseph'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!((office, nin.as_deref(), must_change), (None, Some("CM12345678ABCD"), 1));
        let bursar: Option<String> = conn.query_row("SELECT office FROM users WHERE username = 'grace'", [], |r| r.get(0)).unwrap();
        assert_eq!(bursar.as_deref(), Some("bursar"));
        drop(conn);

        // Importing the same file again makes nobody twice.
        let again = plan_in(&db.lock(), &admin(), &path, None).unwrap();
        assert_eq!(again.ready, 1, "only Opio is new now");
    }

    #[test]
    fn a_teacher_cannot_import_staff() {
        let db = crate::fees::tests::fee_school();
        let path = sheet(&[&["Name", "Phone"], &["Joseph Okello", "0772111222"]]);
        let teacher = test_session("t", Role::Teacher, None, &[]);
        assert!(matches!(plan_in(&db.lock(), &teacher, &path, None), Err(AppError::Forbidden)));
    }

    #[test]
    fn usernames_and_names_tidy_up() {
        let taken: HashSet<String> = ["joseph", "josepho"].iter().map(|s| s.to_string()).collect();
        assert_eq!(make_username("Joseph Okello", &taken), "josephokello");
        assert_eq!(make_username("Al", &HashSet::new()), "alstaff2");
        assert_eq!(tidy_name("MRS.  NAKATO  Sarah"), "Nakato Sarah");
        assert_eq!(office_from("D.O.S"), Some(Office::DirectorOfStudies));
        assert_eq!(office_from("Headmistress"), Some(Office::HeadTeacher));
        assert_eq!(office_from("Assistant bursar"), None);
    }
}
