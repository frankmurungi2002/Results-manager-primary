//! Files that leave Termbook: Excel workbooks, the EMIS census, a sports
//! team's photos.
//!
//! The webview cannot touch the disk (see docs/SECURITY.md). A command builds
//! the file into the app's own `exports` folder and returns its name; the
//! interface asks where to keep it with the system's Save dialog, and
//! `save_export` copies it there. The webview only ever names a file that a
//! command made, and only the extension the file already has can be written.

use std::path::{Path, PathBuf};

use rust_xlsxwriter::{Format, Workbook};
use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// One cell of an exported sheet.
#[derive(Debug, Clone)]
pub enum Cell {
    Text(String),
    Number(f64),
    Empty,
}

impl From<&str> for Cell {
    fn from(s: &str) -> Self {
        Cell::Text(s.to_string())
    }
}
impl From<String> for Cell {
    fn from(s: String) -> Self {
        Cell::Text(s)
    }
}
impl From<Option<String>> for Cell {
    fn from(s: Option<String>) -> Self {
        s.map(Cell::Text).unwrap_or(Cell::Empty)
    }
}
impl From<i64> for Cell {
    fn from(n: i64) -> Self {
        Cell::Number(n as f64)
    }
}
impl From<f64> for Cell {
    fn from(n: f64) -> Self {
        Cell::Number(n)
    }
}
impl From<Option<f64>> for Cell {
    fn from(n: Option<f64>) -> Self {
        n.map(Cell::Number).unwrap_or(Cell::Empty)
    }
}

/// A worksheet: a title row above the headings, then the rows.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    pub name: String,
    pub title: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

impl Sheet {
    pub fn new(name: &str, headers: &[&str]) -> Self {
        Sheet { name: name.into(), title: None, headers: headers.iter().map(|h| h.to_string()).collect(), rows: Vec::new() }
    }
    pub fn titled(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
    pub fn push(&mut self, row: Vec<Cell>) {
        self.rows.push(row);
    }
}

/// Excel sheet names: at most 31 characters, none of `[]:*?/\`, unique.
fn sheet_name(name: &str, taken: &mut Vec<String>) -> String {
    let mut clean: String = name.chars().filter(|c| !"[]:*?/\\".contains(*c)).collect();
    clean = clean.trim().chars().take(28).collect();
    if clean.is_empty() {
        clean = "Sheet".into();
    }
    let mut candidate = clean.clone();
    let mut n = 2;
    while taken.iter().any(|t| t.eq_ignore_ascii_case(&candidate)) {
        candidate = format!("{clean} {n}");
        n += 1;
    }
    taken.push(candidate.clone());
    candidate
}

pub fn workbook(sheets: &[Sheet]) -> AppResult<Vec<u8>> {
    let mut book = Workbook::new();
    let heading = Format::new().set_bold().set_background_color("#E8EEF4").set_border_bottom(rust_xlsxwriter::FormatBorder::Thin);
    let title = Format::new().set_bold().set_font_size(13);
    let mut taken = Vec::new();
    let x = |e: rust_xlsxwriter::XlsxError| AppError::internal(e.to_string());
    for sheet in sheets {
        let ws = book.add_worksheet();
        ws.set_name(sheet_name(&sheet.name, &mut taken)).map_err(x)?;
        let mut r: u32 = 0;
        if let Some(t) = &sheet.title {
            ws.write_string_with_format(0, 0, t, &title).map_err(x)?;
            r = 2;
        }
        for (c, h) in sheet.headers.iter().enumerate() {
            ws.write_string_with_format(r, c as u16, h, &heading).map_err(x)?;
        }
        ws.set_freeze_panes(r + 1, 0).map_err(x)?;
        for row in &sheet.rows {
            r += 1;
            for (c, cell) in row.iter().enumerate() {
                match cell {
                    Cell::Text(s) => {
                        ws.write_string(r, c as u16, s).map_err(x)?;
                    }
                    Cell::Number(n) => {
                        ws.write_number(r, c as u16, *n).map_err(x)?;
                    }
                    Cell::Empty => {}
                }
            }
        }
        ws.autofit();
    }
    book.save_to_buffer().map_err(x)
}

/// A file name safe on Windows: no separators or reserved characters.
pub fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { ' ' } else { c })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_matches('.').to_string();
    if cleaned.is_empty() { "export".into() } else { cleaned.chars().take(120).collect() }
}

/// Writes a finished file into the exports folder and returns its name.
pub fn stage(dir: &Path, name: &str, bytes: &[u8]) -> AppResult<String> {
    std::fs::create_dir_all(dir)?;
    let name = safe_name(name);
    std::fs::write(dir.join(&name), bytes)?;
    Ok(name)
}

/// Writes a folder of files (a team's photos) and returns the folder's name.
pub fn stage_folder(dir: &Path, folder: &str, files: &[(String, Vec<u8>)]) -> AppResult<String> {
    let folder = safe_name(folder);
    let target = dir.join(&folder);
    if target.exists() {
        std::fs::remove_dir_all(&target)?;
    }
    std::fs::create_dir_all(&target)?;
    for (name, bytes) in files {
        std::fs::write(target.join(safe_name(name)), bytes)?;
    }
    Ok(folder)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedFile {
    /// The name in the exports folder, passed back to `save_export`.
    pub file: String,
    /// What the Save dialog offers.
    pub suggested_name: String,
    /// `xlsx`, `csv` or `folder`.
    pub kind: String,
    pub detail: String,
}

pub fn staged(file: String, kind: &str, detail: impl Into<String>) -> StagedFile {
    StagedFile { suggested_name: file.clone(), file, kind: kind.into(), detail: detail.into() }
}

/// Copies a staged export to where the user chose. A folder export is copied
/// into the chosen folder under its own name.
pub fn save_in(export_dir: &Path, file: &str, destination: &str) -> AppResult<String> {
    if file.is_empty() || file != safe_name(file) || file.contains("..") {
        return Err(AppError::validation("That export no longer exists. Make it again."));
    }
    let source = export_dir.join(file);
    if !source.exists() {
        return Err(AppError::validation("That export no longer exists. Make it again."));
    }
    let destination = PathBuf::from(destination);
    if source.is_dir() {
        let target = destination.join(file);
        std::fs::create_dir_all(&target)?;
        for entry in std::fs::read_dir(&source)? {
            let entry = entry?;
            std::fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
        return Ok(target.display().to_string());
    }
    let want = source.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut destination = destination;
    let got = destination.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if got != want {
        // The dialog lets people type any name; the file keeps its own type.
        let mut name = destination.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(format!(".{want}"));
        destination.set_file_name(name);
    }
    std::fs::copy(&source, &destination)?;
    Ok(destination.display().to_string())
}

#[tauri::command]
pub fn save_export(state: State<'_, AppState>, file: String, destination: String) -> AppResult<String> {
    state.sessions.require()?;
    save_in(&state.paths.export_dir, &file, &destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_workbook_opens_with_its_sheets_and_rows() {
        let mut s = Sheet::new("Learners: P5/Blue", &["Name", "Age"]).titled("Rainbow");
        s.push(vec!["Amina".into(), 11i64.into()]);
        s.push(vec!["Musa".into(), Cell::Empty]);
        let again = Sheet::new("Learners: P5/Blue", &["x"]);
        let bytes = workbook(&[s, again]).unwrap();
        let dir = std::env::temp_dir().join(format!("psm-x-{}", std::process::id()));
        let name = stage(&dir, "Learners: P5?.xlsx", &bytes).unwrap();
        assert_eq!(name, "Learners P5 .xlsx");
        use calamine::Reader;
        let mut book: calamine::Xlsx<_> = calamine::open_workbook(dir.join(&name)).unwrap();
        let names = book.sheet_names().to_vec();
        assert_eq!(names, vec!["Learners P5Blue".to_string(), "Learners P5Blue 2".to_string()]);
        let range = book.worksheet_range(&names[0]).unwrap();
        assert_eq!(range.get_value((3, 0)).unwrap().to_string(), "Amina");

        let out = dir.join("copy");
        let saved = save_in(&dir, &name, out.to_str().unwrap()).unwrap();
        assert!(saved.ends_with("copy.xlsx"), "keeps its type: {saved}");
        assert!(save_in(&dir, "../school.rmdb", "x").is_err());

        let folder = stage_folder(&dir, "Netball photos", &[("01 AMINA N.jpg".into(), vec![1, 2]), ("02 MUSA/K.jpg".into(), vec![3])]).unwrap();
        let chosen = dir.join("usb");
        std::fs::create_dir_all(&chosen).unwrap();
        let saved = save_in(&dir, &folder, chosen.to_str().unwrap()).unwrap();
        assert!(std::path::Path::new(&saved).join("02 MUSA K.jpg").exists(), "a slash in a name never makes a folder");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
