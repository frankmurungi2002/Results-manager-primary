//! FR-G13 — onboarding import.
//!
//! A school arriving with 400 learners already in a spreadsheet will not retype
//! them, and if Phantom School Manager asks them to, Phantom School Manager does not get used. So Phantom School Manager reads whatever
//! they have.
//!
//! The file is parsed **in Rust**, never in the webview: a spreadsheet from a
//! school is untrusted input, and it is handled in a memory-safe sandbox with
//! no filesystem or network reach.
//!
//! Two passes, always. The first validates and returns every row with its
//! problems; nothing is written. The School Admin fixes or skips what is
//! flagged, and only then does the second pass commit. A partial import that
//! half-succeeded would be worse than no import at all.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::domain::ids::{new_id, prefix};
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Beyond this a spreadsheet is not a class list, and reading it would hang the
/// interface rather than help anyone.
const MAX_ROWS: usize = 5_000;
const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Reading the file
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadsheetPreview {
    pub file_name: String,
    pub sheets: Vec<String>,
    pub sheet: String,
    pub headers: Vec<String>,
    /// Every data row, as text. Capped at `MAX_ROWS`.
    pub rows: Vec<Vec<String>>,
    pub total_rows: usize,
    pub truncated: bool,
    /// Phantom School Manager's guess at which column is which, by field name.
    pub suggested_mapping: HashMap<String, usize>,
}

/// Column headings Phantom School Manager recognises, in the words schools actually use.
const ALIASES: &[(&str, &[&str])] = &[
    ("fullName", &[
        "name", "names", "full name", "fullname", "student name", "learner name",
        "pupil name", "student", "learner", "pupil", "child name", "candidate name",
    ]),
    ("regNumber", &[
        "reg", "reg no", "reg number", "reg. no", "registration", "registration number",
        "regno", "admission", "admission number", "adm", "adm no", "admno",
        "student id", "student no", "index", "index number", "serial",
    ]),
    ("gender", &["sex", "gender", "m/f", "boy/girl"]),
    ("className", &["class", "class name", "grade", "level", "stream/class", "current class"]),
    ("dateOfBirth", &[
        "dob", "d.o.b", "date of birth", "birth date", "birthdate", "birthday", "born",
    ]),
    ("lin", &["lin", "l.i.n", "learner identification number", "uneb lin"]),
    ("guardianName", &[
        "guardian", "guardian name", "parent", "parent name", "parents name",
        "father", "mother", "next of kin", "nok", "contact person", "parent/guardian",
    ]),
    ("guardianPhone", &[
        "phone", "phone number", "telephone", "tel", "mobile", "contact", "contact number",
        "guardian phone", "parent phone", "parents contact", "guardian contact", "cell",
    ]),
    ("guardianRelationship", &["relationship", "relation", "guardian relationship"]),
];

fn normalise(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '.' || *c == '/')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn suggest_mapping(headers: &[String]) -> HashMap<String, usize> {
    let mut mapping = HashMap::new();
    let cleaned: Vec<String> = headers.iter().map(|h| normalise(h)).collect();

    // Exact matches first, for every field — "name" should not lose to
    // "guardian name".
    for (field, aliases) in ALIASES {
        let exact = cleaned
            .iter()
            .enumerate()
            .position(|(i, h)| aliases.contains(&h.as_str()) && !mapping.values().any(|t| *t == i));
        if let Some(index) = exact {
            mapping.insert((*field).to_string(), index);
        }
    }

    // Then partial matches, the most specific first: in "Parent Contact",
    // "contact" (a phone) is a longer, better match than "parent" (a name).
    // Ties go to the field listed first in ALIASES, then the leftmost column.
    let mut candidates: Vec<(usize, usize, usize)> = Vec::new(); // (alias length, field order, column)
    for (order, (field, aliases)) in ALIASES.iter().enumerate() {
        if mapping.contains_key(*field) {
            continue;
        }
        for (column, heading) in cleaned.iter().enumerate() {
            if heading.is_empty() {
                continue;
            }
            if let Some(len) = aliases.iter().filter(|a| heading.contains(*a)).map(|a| a.len()).max() {
                candidates.push((len, order, column));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

    for (_, order, column) in candidates {
        let field = ALIASES[order].0;
        if !mapping.contains_key(field) && !mapping.values().any(|t| *t == column) {
            mapping.insert(field.to_string(), column);
        }
    }

    mapping
}

/// Reads a spreadsheet or CSV into plain text rows.
fn read_rows(path: &Path, sheet: Option<&str>) -> AppResult<(Vec<String>, Vec<Vec<String>>, String)> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if extension == "csv" || extension == "tsv" || extension == "txt" {
        let text = std::fs::read_to_string(path)?;
        let delimiter = if extension == "tsv" { '\t' } else { ',' };
        let rows = parse_delimited(&text, delimiter);
        return Ok((vec!["Sheet 1".to_string()], rows, "Sheet 1".to_string()));
    }

    read_workbook(path, sheet)
}

/// The only calamine-facing code in Phantom School Manager.
fn read_workbook(
    path: &Path,
    sheet: Option<&str>,
) -> AppResult<(Vec<String>, Vec<Vec<String>>, String)> {
    use calamine::{open_workbook_auto, Data, Reader};

    let mut workbook = open_workbook_auto(path)
        .map_err(|e| AppError::Spreadsheet(format!("{e}")))?;

    let sheets: Vec<String> = workbook.sheet_names().to_owned();
    if sheets.is_empty() {
        return Err(AppError::Spreadsheet("the file has no sheets".into()));
    }

    let chosen = match sheet {
        Some(name) if sheets.iter().any(|s| s == name) => name.to_string(),
        _ => sheets[0].clone(),
    };

    let range = workbook
        .worksheet_range(&chosen)
        .map_err(|e| AppError::Spreadsheet(format!("{e}")))?;

    fn cell_text(cell: &Data) -> String {
        match cell {
            Data::Empty => String::new(),
            Data::String(value) => value.trim().to_string(),
            Data::Int(value) => value.to_string(),
            // A registration number stored as a number must not come back as
            // "2026.0", and an age must not come back as "11.000000000001".
            Data::Float(value) => {
                if (value.fract()).abs() < 1e-9 {
                    format!("{}", *value as i64)
                } else {
                    format!("{value}")
                }
            }
            other => other.to_string().trim().to_string(),
        }
    }

    let rows: Vec<Vec<String>> = range
        .rows()
        .take(MAX_ROWS + 50)
        .map(|row| row.iter().map(cell_text).collect())
        .collect();

    Ok((sheets, rows, chosen))
}

/// A small delimited-text reader that understands quoted fields, so a guardian
/// name containing a comma does not split a learner into two.
fn parse_delimited(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' {
            quoted = true;
        } else if c == delimiter {
            row.push(field.trim().to_string());
            field = String::new();
        } else if c == '\n' {
            row.push(field.trim().to_string());
            field = String::new();
            rows.push(std::mem::take(&mut row));
        } else if c != '\r' {
            field.push(c);
        }
    }

    if !field.is_empty() || !row.is_empty() {
        row.push(field.trim().to_string());
        rows.push(row);
    }

    rows.retain(|r| r.iter().any(|f| !f.is_empty()));
    rows.truncate(MAX_ROWS + 50);
    rows
}

#[tauri::command]
pub fn read_spreadsheet(
    state: State<'_, AppState>,
    path: String,
    sheet: Option<String>,
    header_row: Option<usize>,
) -> AppResult<SpreadsheetPreview> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let file = Path::new(&path);
    if !file.is_file() {
        return Err(AppError::not_found("That file"));
    }
    if std::fs::metadata(file).map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
        return Err(AppError::validation(
            "That file is larger than 25 MB. Please split it.",
        ));
    }

    let (sheets, all_rows, chosen) = read_rows(file, sheet.as_deref())?;

    // Skip leading blank or title rows so a school's decorated spreadsheet
    // ("MB PRIMARY SCHOOL", "P5 REGISTER 2026", then the real headings) works.
    let header_index = header_row.unwrap_or_else(|| {
        all_rows
            .iter()
            .position(|row| row.iter().filter(|c| !c.is_empty()).count() >= 2)
            .unwrap_or(0)
    });

    let headers = all_rows.get(header_index).cloned().unwrap_or_default();
    let mut rows: Vec<Vec<String>> = all_rows
        .into_iter()
        .skip(header_index + 1)
        .filter(|row| row.iter().any(|c| !c.is_empty()))
        .collect();

    let total = rows.len();
    let truncated = total > MAX_ROWS;
    rows.truncate(MAX_ROWS);

    Ok(SpreadsheetPreview {
        file_name: file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("spreadsheet")
            .to_string(),
        sheets,
        sheet: chosen,
        suggested_mapping: suggest_mapping(&headers),
        headers,
        rows,
        total_rows: total,
        truncated,
    })
}

// ---------------------------------------------------------------------------
// Validating and importing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub path: String,
    #[serde(default)]
    pub sheet: Option<String>,
    #[serde(default)]
    pub header_row: Option<usize>,
    /// Field name to column index. Fields Phantom School Manager does not find are simply absent.
    pub mapping: HashMap<String, usize>,
    /// Used when the sheet has no class column, or a row's class is blank.
    #[serde(default)]
    pub default_class_id: Option<String>,
    /// False validates and reports; true writes. Always call false first.
    #[serde(default)]
    pub commit: bool,
    /// Row numbers (as reported by a dry run) to leave out.
    #[serde(default)]
    pub skip_rows: Vec<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub committed: bool,
    pub total: usize,
    pub ready: usize,
    pub blocked: usize,
    pub imported: usize,
    pub rows: Vec<ImportRow>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRow {
    /// 1-based, counting data rows only — what the admin sees in the preview.
    pub row_number: usize,
    pub full_name: String,
    pub reg_number: Option<String>,
    pub class_name: Option<String>,
    pub gender: Option<String>,
    pub guardian_name: Option<String>,
    pub guardian_phone: Option<String>,
    /// "ready" | "blocked" | "skipped" | "imported"
    pub status: String,
    /// Why it cannot be imported. Empty when ready.
    pub problems: Vec<String>,
    /// Things Phantom School Manager corrected or noticed but which do not block the row.
    pub notes: Vec<String>,
}

#[tauri::command]
pub fn import_learners(
    state: State<'_, AppState>,
    request: ImportRequest,
) -> AppResult<ImportResult> {
    let session = state.sessions.require()?;
    session.require_admin()?;

    let file = Path::new(&request.path);
    if !file.is_file() {
        return Err(AppError::not_found("That file"));
    }

    let (_, all_rows, _) = read_rows(file, request.sheet.as_deref())?;

    let header_index = request.header_row.unwrap_or_else(|| {
        all_rows
            .iter()
            .position(|row| row.iter().filter(|c| !c.is_empty()).count() >= 2)
            .unwrap_or(0)
    });

    let data_rows: Vec<Vec<String>> = all_rows
        .into_iter()
        .skip(header_index + 1)
        .filter(|row| row.iter().any(|c| !c.is_empty()))
        .take(MAX_ROWS)
        .collect();

    let mut conn = state.db.lock();

    let year_id = repo::current_academic_year_id(&conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;

    // --- What the school actually has, to validate against ------------------
    let classes: Vec<(String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT id, name, code FROM classes WHERE status = 'active' ORDER BY ladder_position",
        )?;
        let collected = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        collected
    };
    if classes.is_empty() {
        return Err(AppError::validation(
            "Create your classes before importing learners.",
        ));
    }

    let existing_regs: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT LOWER(reg_number) FROM students")?;
        let collected = stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<_>>()?;
        collected
    };

    let column = |field: &str| request.mapping.get(field).copied();
    let cell = |row: &[String], field: &str| -> Option<String> {
        column(field)
            .and_then(|index| row.get(index))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };

    let default_class = request
        .default_class_id
        .as_deref()
        .and_then(|id| classes.iter().find(|(class_id, _, _)| class_id == id));

    // --- Pass one: validate every row --------------------------------------
    struct Resolved {
        row_number: usize,
        full_name: String,
        reg_number: Option<String>,
        class_id: String,
        gender: Option<String>,
        date_of_birth: Option<String>,
        lin: Option<String>,
        guardian_name: Option<String>,
        guardian_phone: Option<String>,
        guardian_relationship: Option<String>,
    }

    let mut rows = Vec::new();
    let mut seen_regs: HashSet<String> = HashSet::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut resolved: Vec<Resolved> = Vec::new();

    for (index, row) in data_rows.iter().enumerate() {
        let row_number = index + 1;
        let mut problems = Vec::new();
        let mut notes = Vec::new();

        let full_name = cell(row, "fullName").unwrap_or_default();
        if full_name.len() < 2 {
            problems.push("No learner name in this row.".to_string());
        }

        // --- Class ---
        let raw_class = cell(row, "className");
        let matched_class = raw_class.as_ref().and_then(|wanted| {
            let want = wanted.trim().to_lowercase();
            classes
                .iter()
                .find(|(_, name, code)| {
                    name.to_lowercase() == want || code.to_lowercase() == want
                })
                .or_else(|| {
                    classes.iter().find(|(_, name, code)| {
                        let n = name.to_lowercase();
                        let c = code.to_lowercase();
                        want.contains(&n) || want.contains(&c) || n.contains(&want)
                    })
                })
        });

        let class_for_row = match (&matched_class, &default_class) {
            (Some(found), _) => Some((*found).clone()),
            (None, Some(fallback)) => {
                if raw_class.is_some() {
                    notes.push(format!(
                        "Class \"{}\" was not recognised; using {} instead.",
                        raw_class.as_deref().unwrap_or(""),
                        fallback.1
                    ));
                }
                Some((*fallback).clone())
            }
            (None, None) => {
                problems.push(match &raw_class {
                    Some(name) => format!(
                        "\"{name}\" is not one of your classes ({}).",
                        classes
                            .iter()
                            .map(|(_, n, _)| n.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    None => "No class for this learner. Choose a class to import into."
                        .to_string(),
                });
                None
            }
        };

        // --- Registration number ---
        let reg_number = cell(row, "regNumber");
        if let Some(reg) = &reg_number {
            let key = reg.to_lowercase();
            if existing_regs.contains(&key) {
                problems.push(format!("{reg} is already used by a learner in Phantom School Manager."));
            } else if !seen_regs.insert(key) {
                problems.push(format!("{reg} appears more than once in this file."));
            }
        } else {
            notes.push("Phantom School Manager will allocate a registration number.".to_string());
        }

        // --- Gender ---
        let gender = cell(row, "gender").and_then(|raw| {
            match raw.trim().to_lowercase().chars().next() {
                Some('m') => Some("M".to_string()),
                Some('f') => Some("F".to_string()),
                Some('b') => Some("M".to_string()), // "Boy"
                Some('g') => Some("F".to_string()), // "Girl"
                _ => {
                    notes.push(format!("Could not read \"{raw}\" as a sex; left blank."));
                    None
                }
            }
        });

        // Two learners with identical names is common and legitimate, but it is
        // also what a duplicated spreadsheet row looks like. Flag, never block.
        if !full_name.is_empty() && !seen_names.insert(full_name.to_lowercase()) {
            notes.push("Another row in this file has the same name. Check it is not a duplicate."
                .to_string());
        }

        let guardian_name = cell(row, "guardianName");
        let guardian_phone = cell(row, "guardianPhone");
        let dob = cell(row, "dateOfBirth");
        let lin = cell(row, "lin");
        let relationship = cell(row, "guardianRelationship");

        let skipped = request.skip_rows.contains(&row_number);
        let status = if skipped {
            "skipped"
        } else if problems.is_empty() {
            "ready"
        } else {
            "blocked"
        };

        if status == "ready" {
            if let Some((class_id, _, _)) = &class_for_row {
                resolved.push(Resolved {
                    row_number,
                    full_name: full_name.clone(),
                    reg_number: reg_number.clone(),
                    class_id: class_id.clone(),
                    gender: gender.clone(),
                    date_of_birth: dob,
                    lin,
                    guardian_name: guardian_name.clone(),
                    guardian_phone: guardian_phone.clone(),
                    guardian_relationship: relationship,
                });
            }
        }

        rows.push(ImportRow {
            row_number,
            full_name,
            reg_number,
            class_name: class_for_row.map(|(_, name, _)| name),
            gender,
            guardian_name,
            guardian_phone,
            status: status.to_string(),
            problems,
            notes,
        });
    }

    let ready = rows.iter().filter(|r| r.status == "ready").count();
    let blocked = rows.iter().filter(|r| r.status == "blocked").count();

    if !request.commit {
        return Ok(ImportResult {
            committed: false,
            total: rows.len(),
            ready,
            blocked,
            imported: 0,
            rows,
        });
    }

    // --- Pass two: write, all or nothing ------------------------------------
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction()?;
    let mut imported = 0usize;

    for entry in &resolved {
        let reg = match &entry.reg_number {
            Some(value) => value.clone(),
            None => repo::next_registration_number(&tx)?,
        };

        let student_id = new_id(prefix::STUDENT);
        tx.execute(
            "INSERT INTO students
                (id, reg_number, full_name, gender, date_of_birth, lin,
                 guardian_name, guardian_phone, guardian_relationship,
                 status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'active', ?10, ?10)",
            params![
                student_id,
                reg,
                entry.full_name,
                entry.gender,
                entry.date_of_birth,
                entry.lin,
                entry.guardian_name,
                entry.guardian_phone,
                entry.guardian_relationship,
                now
            ],
        )?;

        tx.execute(
            "INSERT INTO enrollments
                (id, student_id, class_id, academic_year_id, status, joined_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?5, ?5)",
            params![
                new_id(prefix::ENROLLMENT),
                student_id,
                entry.class_id,
                year_id,
                now
            ],
        )?;

        imported += 1;
        if let Some(row) = rows.iter_mut().find(|r| r.row_number == entry.row_number) {
            row.status = "imported".to_string();
            row.reg_number = Some(reg);
        }
    }

    audit::record(
        &tx,
        Some(&session),
        "import.learners",
        "institution",
        "1",
        format!(
            "Imported {imported} learners from a spreadsheet ({blocked} rows could not be read)"
        ),
    )?;

    tx.commit()?;

    Ok(ImportResult {
        committed: true,
        total: rows.len(),
        ready,
        blocked,
        imported,
        rows,
    })
}

// ---------------------------------------------------------------------------
// Reading an image the user picked
// ---------------------------------------------------------------------------

/// Hands the backend a PNG the School Admin chose in a file dialog.
///
/// The webview has no filesystem access by design (see docs/SECURITY.md), so
/// it passes the path and Rust does the reading — and checks that the bytes are
/// actually a PNG rather than trusting the extension.
#[tauri::command]
pub fn read_image_file(state: State<'_, AppState>, path: String) -> AppResult<Vec<u8>> {
    // Any signed-in user: a Class Teacher may set a learner's photo, and the
    // command only ever returns a PNG under 2 MB.
    state.sessions.require()?;

    let file = Path::new(&path);
    if !file.is_file() {
        return Err(AppError::not_found("That file"));
    }

    let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    if size > 2 * 1024 * 1024 {
        return Err(AppError::validation(
            "That image is larger than 2 MB. Please use a smaller one.",
        ));
    }

    let bytes = std::fs::read(file)?;
    if !bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Err(AppError::validation(
            "That file is not a PNG image. Save it as a PNG and try again.",
        ));
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_are_matched_in_the_words_schools_use() {
        let headers: Vec<String> = ["NO.", "Student Name", "Sex", "Class", "Parent Contact"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mapping = suggest_mapping(&headers);

        assert_eq!(mapping.get("fullName"), Some(&1));
        assert_eq!(mapping.get("gender"), Some(&2));
        assert_eq!(mapping.get("className"), Some(&3));
        assert_eq!(mapping.get("guardianPhone"), Some(&4));
    }

    #[test]
    fn a_learner_name_column_is_not_stolen_by_guardian_name() {
        let headers: Vec<String> = ["Name", "Guardian Name", "Phone"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mapping = suggest_mapping(&headers);
        assert_eq!(mapping.get("fullName"), Some(&0));
        assert_eq!(mapping.get("guardianName"), Some(&1));
    }

    #[test]
    fn a_partial_match_goes_to_the_most_specific_field() {
        let headers: Vec<String> = ["Pupil's Full Name", "Parent/Guardian Names", "Parents Contact No."]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mapping = suggest_mapping(&headers);
        assert_eq!(mapping.get("fullName"), Some(&0));
        assert_eq!(mapping.get("guardianName"), Some(&1));
        assert_eq!(mapping.get("guardianPhone"), Some(&2));
    }

    #[test]
    fn quoted_commas_do_not_split_a_row() {
        let rows = parse_delimited("Name,Guardian\n\"Okello, Brian\",\"Mother, Jane\"\n", ',');
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1], vec!["Okello, Brian", "Mother, Jane"]);
    }

    #[test]
    fn blank_lines_are_dropped() {
        let rows = parse_delimited("A,B\n\n\nC,D\n", ',');
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn doubled_quotes_become_one() {
        let rows = parse_delimited("x\n\"say \"\"hi\"\"\"\n", ',');
        assert_eq!(rows[1], vec!["say \"hi\""]);
    }
}
