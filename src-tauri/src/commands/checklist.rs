//! "Getting started": the steps a new school takes before its first term on
//! Termbook, on the dashboard, ticked off as the school does them.
//!
//! Every tick is worked out from the school's own records (a logo is stored,
//! learners exist, fees are set for the open term...), never from a box
//! someone clicked, so the list cannot say a step is done when it is not.
//! Optional steps are marked so; the list hides itself once every required
//! step is done, or when the office closes it.

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::State;

use crate::domain::repo;
use crate::error::AppResult;
use crate::security::{Permission, Session};
use crate::state::AppState;

const DISMISSED: &str = "checklist.dismissed";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistStep {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub done: bool,
    pub optional: bool,
    /// The screen that does it, and its tab where one applies.
    pub screen: String,
    pub tab: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checklist {
    /// False when there is nothing to show this person.
    pub show: bool,
    pub done: usize,
    pub total: usize,
    pub steps: Vec<ChecklistStep>,
}

fn count(conn: &Connection, sql: &str) -> AppResult<i64> {
    Ok(conn.query_row(sql, [], |r| r.get(0))?)
}

fn step(id: &str, title: &str, detail: &str, done: bool, optional: bool, screen: &str, tab: Option<&str>) -> ChecklistStep {
    ChecklistStep {
        id: id.into(),
        title: title.into(),
        detail: detail.into(),
        done,
        optional,
        screen: screen.into(),
        tab: tab.map(String::from),
    }
}

pub fn checklist_in(conn: &Connection, session: &Session) -> AppResult<Checklist> {
    let term = repo::current_term_id(conn)?;
    let fees_set = match &term {
        Some(t) => conn.query_row("SELECT COUNT(*) FROM fee_rates WHERE term_id = ?1 AND amount > 0", params![t], |r| r.get::<_, i64>(0))? > 0,
        None => false,
    };
    let offline = crate::network::offline_only(conn)?;
    let sms_on = repo::get_setting(conn, "sms.provider")?.map_or(false, |p| !p.is_empty() && p != "off");
    let mirror = repo::get_setting(conn, "backup.mirror_path")?.map_or(false, |p| !p.trim().is_empty());

    let steps = vec![
        step(
            "logo",
            "Add the school badge",
            "Your badge, motto and colour print on every report card, receipt and letter.",
            count(conn, "SELECT COUNT(*) FROM institution WHERE logo_png IS NOT NULL")? > 0,
            false,
            "settings",
            Some("school"),
        ),
        step(
            "subjects",
            "Check classes and subjects",
            "The subjects each class takes, which ones count for the aggregate, and what each paper is out of.",
            count(conn, "SELECT COUNT(*) FROM class_subjects WHERE status = 'active'")? > 0,
            false,
            "classes",
            None,
        ),
        step(
            "grading",
            "Check the grading",
            "UNEB grading is ready. Change the marks for each grade, or make your own, before the first report cards.",
            count(conn, "SELECT COUNT(*) FROM audit_log WHERE action = 'grading_system.save'")? > 0,
            true,
            "grading",
            None,
        ),
        step(
            "learners",
            "Add your learners",
            "Import them from the Excel list you already have, or add them one by one.",
            count(conn, "SELECT COUNT(*) FROM students")? > 0,
            false,
            "learners",
            None,
        ),
        step(
            "staff",
            "Add your staff",
            "Import the staff list from Excel. Each person gets an invite code and sets their own password.",
            count(conn, "SELECT COUNT(*) FROM users WHERE role = 'teacher' AND status = 'active'")? > 0,
            false,
            "staff",
            None,
        ),
        step(
            "assign",
            "Give teachers their classes and subjects",
            "A class teacher for each class, and who teaches which subject, so everyone sees only their own.",
            count(conn, "SELECT COUNT(*) FROM teacher_assignments WHERE status = 'active'")? > 0,
            false,
            "staff",
            None,
        ),
        step(
            "term",
            "Open the term for marks",
            "Exactly one term is open at a time. Check how much each examination counts while you are there.",
            term.is_some(),
            false,
            "calendar",
            None,
        ),
        step(
            "fees",
            "Set this term's fees",
            "Tuition and the other items for each class, bursaries and old balances.",
            fees_set,
            false,
            "fees",
            None,
        ),
        step(
            "signatures",
            "Add the signatures and stamp",
            "Photograph them once and they print on every report card, instead of signing each by hand.",
            count(conn, "SELECT COUNT(*) FROM signature_images WHERE status = 'active' AND kind IN ('head_teacher', 'stamp')")? > 0,
            true,
            "settings",
            Some("signatures"),
        ),
        step(
            "backup",
            "Choose a second place for backups",
            "A flash disk, a second drive, or the Google Drive folder, so a broken computer loses nothing.",
            mirror,
            false,
            "settings",
            Some("backup"),
        ),
        step(
            "recovery",
            "Print your recovery sheet",
            "If you forget your password, this sheet in the school safe lets you back in without calling anyone.",
            conn.query_row("SELECT recovery_hash IS NOT NULL FROM users WHERE id = ?1", params![session.user_id], |r| r.get::<_, bool>(0)).unwrap_or(false),
            false,
            "settings",
            Some("account"),
        ),
        step(
            "sms",
            "Connect SMS",
            "Your SMS account, so receipts, results and reminders reach parents' phones.",
            sms_on,
            true,
            "settings",
            Some("sms"),
        ),
    ];
    // An offline-only school has nothing to connect.
    let steps: Vec<ChecklistStep> = steps.into_iter().filter(|s| !(offline && s.id == "sms")).collect();

    let allowed = session.has(Permission::ManageSchool);
    let dismissed = repo::get_setting(conn, DISMISSED)?.as_deref() == Some("1");
    let practice = crate::commands::practice::is_practice(conn)?;
    let required_left = steps.iter().any(|s| !s.done && !s.optional);
    let done = steps.iter().filter(|s| s.done).count();
    Ok(Checklist { show: allowed && !dismissed && !practice && required_left, done, total: steps.len(), steps })
}

#[tauri::command]
pub fn setup_checklist(state: State<'_, AppState>) -> AppResult<Checklist> {
    let session = state.sessions.require()?;
    checklist_in(&state.db.lock(), &session)
}

#[tauri::command]
pub fn dismiss_setup_checklist(state: State<'_, AppState>) -> AppResult<()> {
    let session = state.sessions.require()?;
    session.require(Permission::ManageSchool)?;
    repo::set_setting(&state.db.lock(), DISMISSED, "1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::session::test_session;
    use crate::security::Role;

    #[test]
    fn ticks_come_from_the_records_and_the_list_goes_when_done() {
        let db = crate::fees::tests::fee_school();
        let conn = db.lock();
        let admin = test_session("head", Role::SchoolAdmin, None, &[]);
        let list = checklist_in(&conn, &admin).unwrap();
        assert!(list.show);
        let done = |l: &Checklist, id: &str| l.steps.iter().find(|s| s.id == id).unwrap().done;
        assert!(done(&list, "learners") && done(&list, "term") && done(&list, "fees"), "the fixture has learners, an open term and its fees");
        assert!(!done(&list, "logo") && !done(&list, "backup") && !done(&list, "recovery"));

        let teacher = test_session("t", Role::Teacher, None, &[]);
        assert!(!checklist_in(&conn, &teacher).unwrap().show, "only the office sees it");

        conn.execute_batch(
            "UPDATE institution SET logo_png = x'89';
             INSERT INTO subjects (id, code, name, created_at, updated_at) VALUES ('mtc', 'MTC', 'Mathematics', 'now', 'now');
             INSERT INTO class_subjects (id, class_id, subject_id, created_at, updated_at) VALUES ('p5mtc', 'p5', 'mtc', 'now', 'now');
             INSERT INTO users (id, username, full_name, role, password_hash, created_at, updated_at) VALUES ('tm', 'tm', 'T M', 'teacher', 'x', 'now', 'now');
             INSERT INTO teacher_assignments (id, user_id, class_id, subject_id, role, created_at, updated_at) VALUES ('a1', 'tm', 'p5', 'mtc', 'subject_teacher', 'now', 'now');
             UPDATE users SET recovery_hash = 'h' WHERE id = 'head';",
        )
        .unwrap();
        repo::set_setting(&conn, "backup.mirror_path", "E:\\").unwrap();
        let list = checklist_in(&conn, &admin).unwrap();
        assert!(!list.show, "every required step is done; optional ones do not hold it open");
    }

    #[test]
    fn closing_it_hides_it_and_offline_schools_skip_sms() {
        let db = crate::fees::tests::fee_school();
        let conn = db.lock();
        let admin = test_session("head", Role::SchoolAdmin, None, &[]);
        crate::network::set_offline_only(&conn, true).unwrap();
        assert!(checklist_in(&conn, &admin).unwrap().steps.iter().all(|s| s.id != "sms"));
        repo::set_setting(&conn, DISMISSED, "1").unwrap();
        assert!(!checklist_in(&conn, &admin).unwrap().show);
    }
}
