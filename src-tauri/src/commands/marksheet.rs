//! FR-C3, the third way in: marks typed in Excel.
//!
//! Many teachers mark at home, on a laptop, in Excel. Termbook gives them a
//! sheet already filled with their learners (and any marks already entered),
//! they type the marks, and the office uploads the file. Two layouts:
//!
//! * **one subject**: a Subject Teacher's class for one examination;
//! * **the whole class**: every subject as a column, for a Class Teacher.
//!
//! The upload never has its own rules. Every mark goes through
//! `marks::save_marks_in`, the same validator and save path as the grid and
//! the one-by-one form (SRS 14.7). Before anything is written the file is
//! checked and the problems are shown row by row; an empty cell means "leave
//! what Termbook has", never "delete".
//!
//! The sheet carries a hidden page naming the class and examination, and a
//! hidden column with each learner's id, so a sheet sorted, re-saved or sent
//! by WhatsApp still lands on the right learners. A file Termbook did not make
//! is refused with a plain message rather than guessed at.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use rust_xlsxwriter::{DataValidation, Format, Formula, Workbook};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit;
use crate::commands::exports::{self, staged, StagedFile};
use crate::commands::marks::{may_write_subject, save_marks_in, MarkWarning, RejectedMark};
use crate::domain::models::MarkEntry;
use crate::domain::repo;
use crate::error::{AppError, AppResult};
use crate::security::Session;
use crate::state::AppState;

const META_SHEET: &str = "termbook";
const META_TAG: &str = "termbook-marks";
const ID_HEADER: &str = "Learner ID (do not change)";
const REG_HEADER: &str = "Reg no";
const NAME_HEADER: &str = "Name";
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// One subject column of a sheet.
#[derive(Debug, Clone)]
struct Column {
    class_subject_id: String,
    subject_name: String,
    max_score: f64,
}

impl Column {
    fn header(&self) -> String {
        format!("{} (out of {})", self.subject_name, trim_number(self.max_score))
    }
}

fn trim_number(n: f64) -> String {
    if n.fract().abs() < 1e-9 { format!("{}", n as i64) } else { format!("{n}") }
}

struct ExamInfo {
    name: String,
    code: String,
    term: String,
    year: String,
    open: bool,
}

fn exam_info(conn: &Connection, exam_id: &str) -> AppResult<ExamInfo> {
    conn.query_row(
        "SELECT e.name, e.code, t.name, y.label, e.status
         FROM exams e JOIN terms t ON t.id = e.term_id JOIN academic_years y ON y.id = t.academic_year_id
         WHERE e.id = ?1",
        params![exam_id],
        |r| {
            Ok(ExamInfo {
                name: r.get(0)?,
                code: r.get(1)?,
                term: r.get(2)?,
                year: r.get(3)?,
                open: r.get::<_, String>(4)? == "open",
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("That examination"))
}

fn class_name(conn: &Connection, class_id: &str) -> AppResult<String> {
    conn.query_row("SELECT name FROM classes WHERE id = ?1", params![class_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("That class"))
}

/// The class's active subjects, in report-card order.
fn class_columns(conn: &Connection, class_id: &str) -> AppResult<Vec<Column>> {
    let mut stmt = conn.prepare(
        "SELECT cs.id, COALESCE(cs.display_name, s.name), COALESCE(cs.max_score, s.max_score)
         FROM class_subjects cs JOIN subjects s ON s.id = cs.subject_id
         WHERE cs.class_id = ?1 AND cs.status = 'active'
         ORDER BY cs.is_core DESC, cs.position ASC, s.name",
    )?;
    let rows = stmt.query_map(params![class_id], |r| {
        Ok(Column { class_subject_id: r.get(0)?, subject_name: r.get(1)?, max_score: r.get(2)? })
    })?;
    let collected = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(collected)
}

struct Learner {
    id: String,
    reg: String,
    name: String,
}

fn roster(conn: &Connection, class_id: &str) -> AppResult<Vec<Learner>> {
    let year_id = repo::current_academic_year_id(conn)?
        .ok_or_else(|| AppError::validation("No academic year is active yet."))?;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.reg_number, s.full_name FROM enrollments e JOIN students s ON s.id = e.student_id
         WHERE e.class_id = ?1 AND e.academic_year_id = ?2 AND e.status = 'active'
         ORDER BY s.full_name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map(params![class_id, year_id], |r| {
        Ok(Learner { id: r.get(0)?, reg: r.get(1)?, name: r.get(2)? })
    })?;
    let collected = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(collected)
}

/// What is already entered, keyed by (student, class subject).
fn existing_marks(conn: &Connection, exam_id: &str, columns: &[Column]) -> AppResult<HashMap<(String, String), (Option<f64>, bool)>> {
    let mut out = HashMap::new();
    let mut stmt = conn.prepare("SELECT student_id, score, is_absent FROM marks WHERE exam_id = ?1 AND class_subject_id = ?2")?;
    for column in columns {
        let rows = stmt.query_map(params![exam_id, column.class_subject_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<f64>>(1)?, r.get::<_, i64>(2)? != 0))
        })?;
        for row in rows {
            let (student, score, absent) = row?;
            out.insert((student, column.class_subject_id.clone()), (score, absent));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// The sheet to fill in
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRequest {
    pub class_id: String,
    pub exam_id: String,
    /// One subject; `None` is the whole class (every subject this person may write).
    #[serde(default)]
    pub class_subject_id: Option<String>,
}

#[derive(Debug)]
pub struct Template {
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub learners: usize,
    pub subjects: usize,
}

pub fn template_in(conn: &Connection, session: &Session, request: &TemplateRequest) -> AppResult<Template> {
    session.require_view_class(&request.class_id)?;
    let exam = exam_info(conn, &request.exam_id)?;
    if !exam.open {
        return Err(AppError::validation(format!(
            "{} is not open for marks, so there is nothing to fill in. A School Admin opens the term in Terms & exams.",
            exam.name
        )));
    }
    let class = class_name(conn, &request.class_id)?;
    let mut columns = Vec::new();
    for column in class_columns(conn, &request.class_id)? {
        if let Some(only) = &request.class_subject_id {
            if &column.class_subject_id != only {
                continue;
            }
        }
        if may_write_subject(conn, session, &request.class_id, &column.class_subject_id)? {
            columns.push(column);
        }
    }
    if columns.is_empty() {
        return Err(match &request.class_subject_id {
            Some(_) => AppError::Forbidden,
            None => AppError::validation(format!("You do not enter marks for any subject in {class}.")),
        });
    }
    let learners = roster(conn, &request.class_id)?;
    if learners.is_empty() {
        return Err(AppError::validation(format!("{class} has no learners yet.")));
    }
    let existing = existing_marks(conn, &request.exam_id, &columns)?;
    let school: String = conn
        .query_row("SELECT name FROM institution WHERE id = 1", [], |r| r.get(0))
        .optional()?
        .unwrap_or_else(|| "School".into());

    let x = |e: rust_xlsxwriter::XlsxError| AppError::internal(e.to_string());
    let mut book = Workbook::new();
    let title = Format::new().set_bold().set_font_size(13);
    let note = Format::new().set_italic().set_font_color("#555555");
    let heading = Format::new()
        .set_bold()
        .set_background_color("#E8EEF4")
        .set_border_bottom(rust_xlsxwriter::FormatBorder::Thin)
        .set_text_wrap();
    let entry = Format::new().set_background_color("#FFFBEA").set_border(rust_xlsxwriter::FormatBorder::Hair);

    let what = if columns.len() == 1 { columns[0].subject_name.clone() } else { "All subjects".to_string() };
    {
        let ws = book.add_worksheet();
        let sheet_title: String = format!("{class} {}", exam.code).chars().filter(|c| !"[]:*?/\\".contains(*c)).take(28).collect();
        ws.set_name(sheet_title.trim()).map_err(x)?;
        ws.write_string_with_format(0, 1, format!("{school}: {class}, {what}, {} ({} {})", exam.name, exam.term, exam.year), &title).map_err(x)?;
        ws.write_string_with_format(
            1,
            1,
            "Type each mark. Type A for a learner who was absent. Leave a box empty to keep what Termbook already has.",
            &note,
        )
        .map_err(x)?;
        let header_row: u32 = 3;
        ws.write_string_with_format(header_row, 0, ID_HEADER, &heading).map_err(x)?;
        ws.write_string_with_format(header_row, 1, REG_HEADER, &heading).map_err(x)?;
        ws.write_string_with_format(header_row, 2, NAME_HEADER, &heading).map_err(x)?;
        for (i, column) in columns.iter().enumerate() {
            ws.write_string_with_format(header_row, 3 + i as u16, column.header(), &heading).map_err(x)?;
        }
        for (r, learner) in learners.iter().enumerate() {
            let row = header_row + 1 + r as u32;
            ws.write_string(row, 0, &learner.id).map_err(x)?;
            ws.write_string(row, 1, &learner.reg).map_err(x)?;
            ws.write_string(row, 2, &learner.name).map_err(x)?;
            for (i, column) in columns.iter().enumerate() {
                let col = 3 + i as u16;
                match existing.get(&(learner.id.clone(), column.class_subject_id.clone())) {
                    Some((_, true)) => {
                        ws.write_string_with_format(row, col, "A", &entry).map_err(x)?;
                    }
                    Some((Some(score), false)) => {
                        ws.write_number_with_format(row, col, *score, &entry).map_err(x)?;
                    }
                    _ => {
                        ws.write_blank(row, col, &entry).map_err(x)?;
                    }
                }
            }
        }
        // Excel itself refuses a mark that cannot be right, while the teacher
        // is typing, so fewer problems ever reach the upload.
        let last_row = header_row + learners.len() as u32;
        for (i, column) in columns.iter().enumerate() {
            let col = 3 + i as u16;
            let cell = format!("{}{}", column_letter(col), header_row + 2);
            let max = trim_number(column.max_score);
            let rule = format!(
                "=OR(UPPER(TRIM({cell}))=\"A\",UPPER(TRIM({cell}))=\"ABS\",UPPER(TRIM({cell}))=\"ABSENT\",AND(ISNUMBER({cell}),{cell}>=0,{cell}<={max}))"
            );
            let validation = DataValidation::new()
                .allow_custom(Formula::new(rule))
                .set_error_title("Not a mark")
                .map_err(x)?
                .set_error_message(format!("Type a mark from 0 to {max}, or A if the learner was absent."))
                .map_err(x)?;
            ws.add_data_validation(header_row + 1, col, last_row, col, &validation).map_err(x)?;
            ws.set_column_width(col, 14).map_err(x)?;
        }
        ws.set_column_hidden(0).map_err(x)?;
        ws.set_column_width(1, 14).map_err(x)?;
        ws.set_column_width(2, 30).map_err(x)?;
        ws.set_freeze_panes(header_row + 1, 3).map_err(x)?;
    }
    {
        let ws = book.add_worksheet();
        ws.set_name(META_SHEET).map_err(x)?;
        ws.write_string(0, 0, META_TAG).map_err(x)?;
        ws.write_string(0, 1, "1").map_err(x)?;
        ws.write_string(1, 0, "exam").map_err(x)?;
        ws.write_string(1, 1, &request.exam_id).map_err(x)?;
        ws.write_string(2, 0, "class").map_err(x)?;
        ws.write_string(2, 1, &request.class_id).map_err(x)?;
        for (i, column) in columns.iter().enumerate() {
            let row = 4 + i as u32;
            ws.write_string(row, 0, column.header()).map_err(x)?;
            ws.write_string(row, 1, &column.class_subject_id).map_err(x)?;
        }
        ws.set_hidden(true);
    }
    let bytes = book.save_to_buffer().map_err(x)?;
    Ok(Template {
        file_name: format!("{class} {what} {} {} {}.xlsx", exam.code, exam.term, exam.year),
        bytes,
        learners: learners.len(),
        subjects: columns.len(),
    })
}

fn column_letter(mut col: u16) -> String {
    let mut out = String::new();
    col += 1;
    while col > 0 {
        let rem = (col - 1) % 26;
        out.insert(0, (b'A' + rem as u8) as char);
        col = (col - 1) / 26;
    }
    out
}

#[tauri::command]
pub fn download_marks_template(state: State<'_, AppState>, request: TemplateRequest) -> AppResult<StagedFile> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    let template = template_in(&conn, &session, &request)?;
    let file = exports::stage(&state.paths.export_dir, &template.file_name, &template.bytes)?;
    Ok(staged(
        file,
        "xlsx",
        format!("{} learners, {} subject{}", template.learners, template.subjects, if template.subjects == 1 { "" } else { "s" }),
    ))
}

// ---------------------------------------------------------------------------
// Reading a filled sheet back
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadSubject {
    pub class_subject_id: String,
    pub subject_name: String,
    pub max_score: f64,
    /// Marks (and absences) in the file that will be saved.
    pub marks: usize,
    pub absent: usize,
    /// Of those, how many differ from what Termbook has now: only these are written.
    pub changed: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadProblem {
    /// The row number as Excel shows it.
    pub row: Option<usize>,
    pub learner: Option<String>,
    pub subject: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarksUpload {
    pub file_name: String,
    pub class_id: String,
    pub class_name: String,
    pub exam_id: String,
    pub exam_name: String,
    pub subjects: Vec<UploadSubject>,
    pub problems: Vec<UploadProblem>,
    /// Marks read from the file, across subjects.
    pub ready: usize,
    /// Of those, the ones that will be written.
    pub changed: usize,
}

struct Parsed {
    preview: MarksUpload,
    entries: Vec<(String, Vec<MarkEntry>)>,
}

enum Value {
    Blank,
    Score(f64),
    Absent,
    Bad(String),
}

fn cell_value(cell: &calamine::Data) -> Value {
    use calamine::Data;
    match cell {
        Data::Empty => Value::Blank,
        Data::Int(n) => Value::Score(*n as f64),
        Data::Float(n) => Value::Score(*n),
        Data::String(text) => {
            let t = text.trim();
            if t.is_empty() || t == "-" {
                return Value::Blank;
            }
            match t.to_uppercase().as_str() {
                "A" | "AB" | "ABS" | "ABSENT" => Value::Absent,
                _ => match t.replace(',', ".").parse::<f64>() {
                    Ok(n) => Value::Score(n),
                    Err(_) => Value::Bad(t.to_string()),
                },
            }
        }
        Data::Error(_) => Value::Bad("an Excel error".into()),
        other => Value::Bad(other.to_string()),
    }
}

fn cell_text(cell: Option<&calamine::Data>) -> String {
    match cell {
        None | Some(calamine::Data::Empty) => String::new(),
        Some(calamine::Data::Float(n)) if n.fract().abs() < 1e-9 => format!("{}", *n as i64),
        Some(other) => other.to_string().trim().to_string(),
    }
}

fn not_ours() -> AppError {
    AppError::validation(
        "This is not a marks sheet from Termbook. In Marks entry, press \"Download Excel sheet\", fill that file in, then upload it.",
    )
}

fn parse_in(conn: &Connection, session: &Session, path: &Path) -> AppResult<Parsed> {
    use calamine::{open_workbook_auto, Reader};

    if !path.is_file() {
        return Err(AppError::not_found("That file"));
    }
    if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
        return Err(not_ours());
    }
    let mut book = open_workbook_auto(path).map_err(|_| not_ours())?;
    let names = book.sheet_names().to_owned();
    if !names.iter().any(|n| n == META_SHEET) {
        return Err(not_ours());
    }
    let meta = book.worksheet_range(META_SHEET).map_err(|_| not_ours())?;
    let at = |r: u32, c: u32| cell_text(meta.get_value((r, c)));
    if at(0, 0) != META_TAG {
        return Err(not_ours());
    }
    let exam_id = at(1, 1);
    let class_id = at(2, 1);
    let mut wanted: Vec<(String, String)> = Vec::new(); // (header, class_subject_id)
    let mut r = 4;
    loop {
        let header = at(r, 0);
        if header.is_empty() {
            break;
        }
        wanted.push((header, at(r, 1)));
        r += 1;
    }

    session.require_view_class(&class_id)?;
    let exam = exam_info(conn, &exam_id).map_err(|_| AppError::validation("The examination this sheet was made for no longer exists."))?;
    let class = class_name(conn, &class_id).map_err(|_| AppError::validation("The class this sheet was made for no longer exists."))?;
    if !exam.open {
        return Err(AppError::conflict(format!("{} is closed, so its marks cannot be changed. A School Admin must reopen it first.", exam.name)));
    }
    let columns: HashMap<String, Column> = class_columns(conn, &class_id)?.into_iter().map(|c| (c.class_subject_id.clone(), c)).collect();

    let main = names.iter().find(|n| n.as_str() != META_SHEET).ok_or_else(not_ours)?.clone();
    let range = book.worksheet_range(&main).map_err(|_| not_ours())?;
    let rows: Vec<&[calamine::Data]> = range.rows().collect();
    let (start_row, _) = range.start().unwrap_or((0, 0));
    let header_index = rows
        .iter()
        .position(|row| row.iter().any(|c| cell_text(Some(c)) == REG_HEADER))
        .ok_or_else(not_ours)?;
    let headers: Vec<String> = rows[header_index].iter().map(|c| cell_text(Some(c))).collect();
    let find = |name: &str| headers.iter().position(|h| h == name);
    let id_col = find(ID_HEADER);
    let reg_col = find(REG_HEADER).ok_or_else(not_ours)?;
    let name_col = find(NAME_HEADER);

    let mut problems = Vec::new();
    // Subject columns, found by their heading so a moved column still works.
    let mut subject_cols: Vec<(usize, Column)> = Vec::new();
    for (header, cs_id) in &wanted {
        let Some(column) = columns.get(cs_id) else {
            problems.push(UploadProblem { row: None, learner: None, subject: Some(header.clone()), message: "This subject is no longer taught in this class, so its column is ignored.".into() });
            continue;
        };
        let Some(index) = headers.iter().position(|h| h == header) else {
            problems.push(UploadProblem { row: None, learner: None, subject: Some(column.subject_name.clone()), message: "Its column was not found in the sheet, so nothing was read for it.".into() });
            continue;
        };
        if !may_write_subject(conn, session, &class_id, cs_id)? {
            problems.push(UploadProblem { row: None, learner: None, subject: Some(column.subject_name.clone()), message: "You do not enter marks for this subject, so its column is ignored.".into() });
            continue;
        }
        subject_cols.push((index, column.clone()));
    }

    let learners = roster(conn, &class_id)?;
    let by_id: HashMap<&str, &Learner> = learners.iter().map(|l| (l.id.as_str(), l)).collect();
    let by_reg: HashMap<String, &Learner> = learners.iter().map(|l| (l.reg.trim().to_uppercase(), l)).collect();
    let all_columns: Vec<Column> = subject_cols.iter().map(|(_, c)| c.clone()).collect();
    let existing = existing_marks(conn, &exam_id, &all_columns)?;

    let mut entries: HashMap<String, Vec<MarkEntry>> = HashMap::new();
    let mut stats: HashMap<String, (usize, usize, usize)> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();

    for (offset, row) in rows.iter().enumerate().skip(header_index + 1) {
        let excel_row = start_row as usize + offset + 1;
        let id = id_col.map(|c| cell_text(row.get(c))).unwrap_or_default();
        let reg = cell_text(row.get(reg_col));
        let name = name_col.map(|c| cell_text(row.get(c))).unwrap_or_default();
        let has_marks = subject_cols.iter().any(|(c, _)| !matches!(row.get(*c).map(cell_value), None | Some(Value::Blank)));
        if id.is_empty() && reg.is_empty() && name.is_empty() && !has_marks {
            continue;
        }
        let learner = by_id.get(id.as_str()).copied().or_else(|| by_reg.get(&reg.trim().to_uppercase()).copied());
        let Some(learner) = learner else {
            if has_marks {
                let who = if name.is_empty() { reg.clone() } else { name.clone() };
                problems.push(UploadProblem {
                    row: Some(excel_row),
                    learner: Some(who.clone()),
                    subject: None,
                    message: format!("{} is not in {class} in Termbook, so this row was not read.", if who.is_empty() { "This learner".to_string() } else { who }),
                });
            }
            continue;
        };
        if !seen.insert(learner.id.clone()) {
            problems.push(UploadProblem { row: Some(excel_row), learner: Some(learner.name.clone()), subject: None, message: "This learner appears twice in the sheet. Only the first row was read.".into() });
            continue;
        }
        for (c, column) in &subject_cols {
            let value = row.get(*c).map(cell_value).unwrap_or(Value::Blank);
            let (score, absent) = match value {
                Value::Blank => continue,
                Value::Absent => (None, true),
                Value::Score(n) if n < 0.0 => {
                    problems.push(UploadProblem { row: Some(excel_row), learner: Some(learner.name.clone()), subject: Some(column.subject_name.clone()), message: format!("{} is below zero.", trim_number(n)) });
                    continue;
                }
                Value::Score(n) if n > column.max_score => {
                    problems.push(UploadProblem {
                        row: Some(excel_row),
                        learner: Some(learner.name.clone()),
                        subject: Some(column.subject_name.clone()),
                        message: format!("{} is above the maximum of {}.", trim_number(n), trim_number(column.max_score)),
                    });
                    continue;
                }
                Value::Score(n) => (Some(n), false),
                Value::Bad(text) => {
                    problems.push(UploadProblem {
                        row: Some(excel_row),
                        learner: Some(learner.name.clone()),
                        subject: Some(column.subject_name.clone()),
                        message: format!("\"{text}\" is not a mark. Type a number, or A for absent."),
                    });
                    continue;
                }
            };
            let before = existing.get(&(learner.id.clone(), column.class_subject_id.clone()));
            let changed = match before {
                None => true,
                Some((old, was_absent)) => *was_absent != absent || old.map(|o| (o - score.unwrap_or(-1.0)).abs() > 1e-9).unwrap_or(score.is_some()),
            };
            let stat = stats.entry(column.class_subject_id.clone()).or_default();
            stat.0 += 1;
            if absent {
                stat.1 += 1;
            }
            if changed {
                stat.2 += 1;
            }
            // Only what differs is written, so "entered by" stays true for marks
            // the upload did not change.
            if changed {
                entries.entry(column.class_subject_id.clone()).or_default().push(MarkEntry { student_id: learner.id.clone(), score, is_absent: absent });
            }
        }
    }

    let subjects: Vec<UploadSubject> = subject_cols
        .iter()
        .map(|(_, c)| {
            let (marks, absent, changed) = stats.get(&c.class_subject_id).copied().unwrap_or_default();
            UploadSubject { class_subject_id: c.class_subject_id.clone(), subject_name: c.subject_name.clone(), max_score: c.max_score, marks, absent, changed }
        })
        .collect();
    let ready = subjects.iter().map(|s| s.marks).sum();
    let changed = subjects.iter().map(|s| s.changed).sum();
    let ordered: Vec<(String, Vec<MarkEntry>)> = subject_cols
        .iter()
        .filter_map(|(_, c)| entries.remove(&c.class_subject_id).map(|e| (c.class_subject_id.clone(), e)))
        .collect();

    Ok(Parsed {
        preview: MarksUpload {
            file_name: path.file_name().and_then(|n| n.to_str()).unwrap_or("marks.xlsx").to_string(),
            class_id,
            class_name: class,
            exam_id,
            exam_name: exam.name,
            subjects,
            problems,
            ready,
            changed,
        },
        entries: ordered,
    })
}

pub fn check_in(conn: &Connection, session: &Session, path: &Path) -> AppResult<MarksUpload> {
    Ok(parse_in(conn, session, path)?.preview)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarksUploadResult {
    pub saved: usize,
    pub subjects: usize,
    pub skipped: Vec<RejectedMark>,
    pub warnings: Vec<MarkWarning>,
}

pub fn apply_in(conn: &mut Connection, session: &Session, path: &Path) -> AppResult<MarksUploadResult> {
    let parsed = parse_in(conn, session, path)?;
    let mut result = MarksUploadResult { saved: 0, subjects: 0, skipped: Vec::new(), warnings: Vec::new() };
    for (class_subject_id, entries) in &parsed.entries {
        if entries.is_empty() {
            continue;
        }
        let saved = save_marks_in(conn, session, class_subject_id, &parsed.preview.exam_id, entries)?;
        result.saved += saved.saved;
        result.subjects += 1;
        result.skipped.extend(saved.skipped);
        result.warnings.extend(saved.warnings);
    }
    audit::record(
        conn,
        Some(session),
        "marks.upload",
        "class",
        &parsed.preview.class_id,
        format!(
            "Uploaded {} marks for {}, {} from \"{}\"",
            result.saved, parsed.preview.class_name, parsed.preview.exam_name, parsed.preview.file_name
        ),
    )?;
    Ok(result)
}

#[tauri::command]
pub fn check_marks_upload(state: State<'_, AppState>, path: String) -> AppResult<MarksUpload> {
    let session = state.sessions.require()?;
    let conn = state.db.lock();
    check_in(&conn, &session, Path::new(&path))
}

#[tauri::command]
pub fn apply_marks_upload(state: State<'_, AppState>, path: String) -> AppResult<MarksUploadResult> {
    let session = state.sessions.require()?;
    let mut conn = state.db.lock();
    apply_in(&mut conn, &session, Path::new(&path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::session::test_session;
    use crate::security::{Office, Role};

    fn school() -> crate::db::Database {
        let db = crate::fees::tests::fee_school();
        db.lock()
            .execute_batch(
                "INSERT INTO subjects (id, code, name, max_score, pass_mark, created_at, updated_at) VALUES
                     ('mtc', 'MTC', 'Mathematics', 100, 50, 'now', 'now'),
                     ('eng', 'ENG', 'English', 100, 50, 'now', 'now');
                 INSERT INTO class_subjects (id, class_id, subject_id, max_score, position, created_at, updated_at) VALUES
                     ('p5mtc', 'p5', 'mtc', NULL, 1, 'now', 'now'),
                     ('p5eng', 'p5', 'eng', 40, 2, 'now', 'now');
                 INSERT INTO exams (id, term_id, seq, code, name, weight, status, created_at, updated_at) VALUES
                     ('mid', 't262', 2, 'MID', 'Mid Term', 0.4, 'open', 'now', 'now'),
                     ('old', 't261', 3, 'EOT', 'End of Term', 0.6, 'closed', 'now', 'now');
                 INSERT INTO users (id, username, full_name, role, password_hash, created_at, updated_at) VALUES
                     ('tmtc', 'tmtc', 'Maths Teacher', 'teacher', 'x', 'now', 'now');
                 INSERT INTO teacher_assignments (id, user_id, class_id, subject_id, role, created_at, updated_at) VALUES
                     ('ta1', 'tmtc', 'p5', 'mtc', 'subject_teacher', 'now', 'now');
                 INSERT INTO marks (id, student_id, class_subject_id, exam_id, score, is_absent, entered_by, entered_at, updated_by, updated_at)
                     VALUES ('m1', 'musa', 'p5mtc', 'mid', 70, 0, 'head', 'now', 'head', 'now');",
            )
            .unwrap();
        db
    }

    fn class_teacher() -> Session {
        test_session("head", Role::Teacher, Some(Office::HeadTeacher), &["p5"])
    }

    fn subject_teacher() -> Session {
        let mut s = test_session("tmtc", Role::Teacher, None, &[]);
        s.mode = Some(crate::security::TeacherMode::SubjectTeacher);
        s.class_ids = ["p5".to_string()].into_iter().collect();
        s
    }

    /// Fills cells of a downloaded template the way a teacher would in Excel,
    /// by rewriting the workbook with calamine's values plus the edits.
    fn fill(template: &Template, edits: &[(&str, usize, &str)]) -> std::path::PathBuf {
        use calamine::{Reader, Xlsx};
        let dir = std::env::temp_dir().join(format!("tbk-marks-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("template.xlsx");
        std::fs::write(&source, &template.bytes).unwrap();
        let mut book: Xlsx<_> = calamine::open_workbook(&source).unwrap();
        let names = book.sheet_names().to_vec();
        let mut out = Workbook::new();
        for name in &names {
            let range = book.worksheet_range(name).unwrap();
            let ws = out.add_worksheet();
            ws.set_name(name).unwrap();
            let (r0, c0) = range.start().unwrap_or((0, 0));
            for (r, row) in range.rows().enumerate() {
                for (c, cell) in row.iter().enumerate() {
                    let (rr, cc) = (r0 + r as u32, (c0 as usize + c) as u16);
                    match cell {
                        calamine::Data::String(s) => {
                            ws.write_string(rr, cc, s).unwrap();
                        }
                        calamine::Data::Float(n) => {
                            ws.write_number(rr, cc, *n).unwrap();
                        }
                        _ => {}
                    }
                }
            }
            if name != META_SHEET {
                for (learner_name, col, value) in edits {
                    let row = range.rows().position(|row| row.iter().any(|c| c.to_string() == *learner_name)).unwrap() as u32 + r0;
                    match value.parse::<f64>() {
                        Ok(n) => ws.write_number(row, *col as u16, n).unwrap(),
                        Err(_) if value.is_empty() => ws.write_blank(row, *col as u16, &Format::new()).unwrap(),
                        Err(_) => ws.write_string(row, *col as u16, *value).unwrap(),
                    };
                }
            }
        }
        let path = dir.join("filled.xlsx");
        out.save(&path).unwrap();
        path
    }

    fn rand_suffix() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
    }

    #[test]
    fn a_class_sheet_round_trips_through_the_one_save_path() {
        let db = school();
        let head = class_teacher();
        let request = TemplateRequest { class_id: "p5".into(), exam_id: "mid".into(), class_subject_id: None };
        let template = template_in(&db.lock(), &head, &request).unwrap();
        assert_eq!((template.learners, template.subjects), (2, 2), "only active learners; every subject");
        assert!(template.file_name.contains("All subjects"));

        // Columns: D Mathematics, E English (out of 40).
        let path = fill(&template, &[("Amina N", 3, "81"), ("Amina N", 4, "A"), ("Musa K", 4, "45"), ("Musa K", 3, "")]);
        let preview = check_in(&db.lock(), &head, &path).unwrap();
        assert_eq!(preview.class_name, "P5");
        assert_eq!(preview.problems.len(), 1, "{:?}", preview.problems);
        assert!(preview.problems[0].message.contains("above the maximum of 40"));
        let mtc = preview.subjects.iter().find(|s| s.class_subject_id == "p5mtc").unwrap();
        assert_eq!((mtc.marks, mtc.changed), (1, 1), "Musa's blank keeps his 70 rather than deleting it");
        let eng = preview.subjects.iter().find(|s| s.class_subject_id == "p5eng").unwrap();
        assert_eq!((eng.marks, eng.absent), (1, 1));

        let result = apply_in(&mut db.lock(), &head, &path).unwrap();
        assert_eq!(result.saved, 2);
        let conn = db.lock();
        let score = |s: &str, cs: &str| -> (Option<f64>, i64) {
            conn.query_row("SELECT score, is_absent FROM marks WHERE student_id = ?1 AND class_subject_id = ?2 AND exam_id = 'mid'", params![s, cs], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
        };
        assert_eq!(score("amina", "p5mtc"), (Some(81.0), 0));
        assert_eq!(score("amina", "p5eng"), (None, 1));
        assert_eq!(score("musa", "p5mtc"), (Some(70.0), 0), "untouched");
        let audited: i64 = conn.query_row("SELECT COUNT(*) FROM audit_log WHERE action = 'marks.upload'", [], |r| r.get(0)).unwrap();
        assert_eq!(audited, 1);
    }

    #[test]
    fn a_subject_teacher_gets_only_their_subject_and_cannot_upload_another() {
        let db = school();
        let teacher = subject_teacher();
        let all = TemplateRequest { class_id: "p5".into(), exam_id: "mid".into(), class_subject_id: None };
        let template = template_in(&db.lock(), &teacher, &all).unwrap();
        assert_eq!(template.subjects, 1, "Mathematics only");
        let english = TemplateRequest { class_subject_id: Some("p5eng".into()), ..all.clone() };
        assert!(matches!(template_in(&db.lock(), &teacher, &english), Err(AppError::Forbidden)));

        // The class teacher's whole-class sheet, uploaded by the maths teacher:
        // English is reported and ignored, Mathematics is saved.
        let head_sheet = template_in(&db.lock(), &class_teacher(), &all).unwrap();
        let path = fill(&head_sheet, &[("Amina N", 3, "60"), ("Amina N", 4, "30")]);
        let preview = check_in(&db.lock(), &teacher, &path).unwrap();
        assert!(preview.problems.iter().any(|p| p.subject.as_deref() == Some("English") && p.message.contains("do not enter marks")));
        let result = apply_in(&mut db.lock(), &teacher, &path).unwrap();
        assert_eq!(result.saved, 1);
        let english_marks: i64 = db.lock().query_row("SELECT COUNT(*) FROM marks WHERE class_subject_id = 'p5eng'", [], |r| r.get(0)).unwrap();
        assert_eq!(english_marks, 0);
    }

    #[test]
    fn closed_exams_strangers_and_other_files_are_refused_plainly() {
        let db = school();
        let head = class_teacher();
        let closed = TemplateRequest { class_id: "p5".into(), exam_id: "old".into(), class_subject_id: None };
        assert!(template_in(&db.lock(), &head, &closed).unwrap_err().to_string().contains("not open"));

        let template = template_in(&db.lock(), &head, &TemplateRequest { exam_id: "mid".into(), ..closed.clone() }).unwrap();
        let path = fill(&template, &[("Musa K", 3, "abc")]);
        let preview = check_in(&db.lock(), &head, &path).unwrap();
        assert!(preview.problems.iter().any(|p| p.message.contains("\"abc\" is not a mark") && p.row.is_some()));

        // A plain spreadsheet someone made themselves.
        let mut plain = Workbook::new();
        plain.add_worksheet().write_string(0, 0, "Name").unwrap();
        let other = path.with_file_name("plain.xlsx");
        plain.save(&other).unwrap();
        assert!(check_in(&db.lock(), &head, &other).unwrap_err().to_string().contains("not a marks sheet from Termbook"));

        // The exam closes between download and upload.
        db.lock().execute("UPDATE exams SET status = 'closed' WHERE id = 'mid'", []).unwrap();
        assert!(check_in(&db.lock(), &head, &path).unwrap_err().to_string().contains("closed"));
    }

    #[test]
    fn column_letters() {
        assert_eq!(column_letter(0), "A");
        assert_eq!(column_letter(3), "D");
        assert_eq!(column_letter(25), "Z");
        assert_eq!(column_letter(26), "AA");
    }
}
