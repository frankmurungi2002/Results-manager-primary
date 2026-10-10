//! Who is signed in, and what they are allowed to touch.
//!
//! The session lives in Rust process memory and is never handed to the
//! frontend as a token. The webview holds no credential it could leak; it asks
//! "who am I?" and the backend answers from its own state. Every command that
//! touches school data resolves the actor here rather than trusting an
//! argument, so a compromised webview cannot claim to be a School Admin.

use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// How long a session survives with no activity before Phantom School Manager locks itself.
/// A school PC sits on a desk in a shared office; an unattended, signed-in
/// session is a real exposure.
const IDLE_TIMEOUT_MINUTES: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    SchoolAdmin,
    Teacher,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::SchoolAdmin => "school_admin",
            Role::Teacher => "teacher",
        }
    }

    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "school_admin" => Ok(Role::SchoolAdmin),
            "teacher" => Ok(Role::Teacher),
            other => Err(AppError::internal(format!("unknown role '{other}'"))),
        }
    }
}

/// An office some staff hold on top of their account: the people who run
/// the school rather than a class. A headteacher who also teaches P7 is a
/// `Teacher` with assignments *and* the `HeadTeacher` office, which is why
/// office is not a third kind of account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Office {
    HeadTeacher,
    DirectorOfStudies,
    Secretary,
    Bursar,
}

impl Office {
    pub fn as_str(self) -> &'static str {
        match self {
            Office::HeadTeacher => "head_teacher",
            Office::DirectorOfStudies => "director_of_studies",
            Office::Secretary => "secretary",
            Office::Bursar => "bursar",
        }
    }

    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "head_teacher" => Ok(Office::HeadTeacher),
            "director_of_studies" => Ok(Office::DirectorOfStudies),
            "secretary" => Ok(Office::Secretary),
            "bursar" => Ok(Office::Bursar),
            other => Err(AppError::validation(format!("Unknown office \"{other}\"."))),
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Office::HeadTeacher => "Headteacher",
            Office::DirectorOfStudies => "Director of Studies",
            Office::Secretary => "Secretary",
            Office::Bursar => "Bursar",
        }
    }
}

/// Something a person may do across the whole school. Class Teachers and
/// Subject Teachers get their powers from their assignments instead, scoped
/// to their own classes, and need none of these.
///
/// Every protected command names the permission it needs, so who may do
/// what is read from one table ([`permissions_for`]) rather than inferred
/// from eighty `if admin` checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// The school's identity, logo, optional features, grading systems, SMS
    /// and report settings, the stamp.
    ManageSchool,
    /// Backups and restoring them.
    ManageBackups,
    /// Staff accounts, passwords and teaching assignments.
    ManageStaff,
    /// The activity log.
    ViewAudit,
    /// Opening and closing terms.
    ManageCalendar,
    /// Classes, subjects, streams and timetables.
    ManageAcademics,
    /// Every learner's record and every class list, but not marks.
    ViewAllLearners,
    /// Every class's marks, attendance and report cards, to read and print.
    ViewAllClasses,
    /// A Class Teacher's powers in every class: register, roster, comments.
    ManageAllClasses,
    /// Entering or correcting marks in any class.
    EditAllMarks,
    /// Admitting, editing, transferring and importing learners.
    ManageLearners,
    /// Pass-outs for a learner in any class.
    IssuePassOuts,
    /// The fee structure, bursaries, recording payments and printing
    /// receipts and invoices, and the fees block.
    ManageFees,
    /// Fee balances, collections and the Day Book's money figures.
    ViewFinance,
    /// Cancelling a receipt. Kept from whoever takes the money, so the
    /// person holding the cash cannot also make a receipt disappear.
    VoidReceipts,
    /// Bulk SMS to any part of the school, and learner groups.
    SendMessages,
    /// Writing the headteacher's comment on report cards.
    HeadTeacherComment,
    /// The PLE projection and subject heat map for the whole school.
    ViewInsights,
}

use Permission::*;

const ALL: &[Permission] = &[
    ManageSchool, ManageBackups, ManageStaff, ViewAudit, ManageCalendar, ManageAcademics,
    ViewAllLearners, ViewAllClasses, ManageAllClasses, EditAllMarks, ManageLearners,
    IssuePassOuts, ManageFees, ViewFinance, VoidReceipts, SendMessages, HeadTeacherComment,
    ViewInsights,
];

/// The whole policy. Changing who may do what is a change to this table.
pub fn permissions_for(role: Role, office: Option<Office>) -> HashSet<Permission> {
    let account: &[Permission] = match role {
        // The School Admin runs the system itself, including backups.
        Role::SchoolAdmin => ALL,
        Role::Teacher => &[],
    };
    let office: &[Permission] = match office {
        // Runs the school, but neither the computer (backups, settings) nor
        // anyone's marks: a headteacher who changes a mark is still asked to
        // do it through the teacher or the Director of Studies.
        Some(Office::HeadTeacher) => &[
            ManageStaff, ViewAudit, ManageCalendar, ManageAcademics, ViewAllLearners,
            ViewAllClasses, ManageAllClasses, ManageLearners, IssuePassOuts, ManageFees,
            ViewFinance, VoidReceipts, SendMessages, HeadTeacherComment, ViewInsights,
        ],
        // Academics end to end, including correcting marks; no money, no gate.
        Some(Office::DirectorOfStudies) => &[
            ManageCalendar, ManageAcademics, ViewAllLearners, ViewAllClasses,
            ManageAllClasses, EditAllMarks, ViewInsights,
        ],
        // The front office: admissions, the gate, messages, and printing
        // report cards. Reads marks to print them; never edits one.
        Some(Office::Secretary) => &[
            ViewAllLearners, ViewAllClasses, ManageLearners, IssuePassOuts, SendMessages,
        ],
        // Money only, and the reminders that go with it. Takes payments but
        // cannot void a receipt: that is the head's or the School Admin's.
        Some(Office::Bursar) => &[ViewAllLearners, ManageFees, ViewFinance, SendMessages],
        None => &[],
    };
    account.iter().chain(office).copied().collect()
}

impl Permission {
    /// What it allows, in the words of docs/ROLES.md.
    pub fn describe(self) -> &'static str {
        match self {
            ManageSchool => "School identity, logo, optional features, grading systems, SMS and report settings, the stamp",
            ManageBackups => "Backups, and restoring one",
            ManageStaff => "Staff accounts, passwords and teaching assignments",
            ViewAudit => "The activity log",
            ManageCalendar => "Opening and closing terms",
            ManageAcademics => "Classes, subjects, streams, timetables and exam timetables",
            ViewAllLearners => "Every learner's record and class list (no marks)",
            ViewAllClasses => "Every class's marks, attendance and report cards, to read and print",
            ManageAllClasses => "A Class Teacher's powers in every class",
            EditAllMarks => "Entering or correcting marks in any class",
            ManageLearners => "Admitting, editing, transferring and importing learners",
            IssuePassOuts => "Pass-outs for a learner in any class",
            ManageFees => "Fee structure, bursaries, taking payments, receipts and invoices, the fees block",
            ViewFinance => "Fee balances, collections and the Day Book's money figures",
            VoidReceipts => "Cancelling a receipt someone else issued",
            SendMessages => "Bulk SMS to any part of the school, and learner groups",
            HeadTeacherComment => "The headteacher's comment on report cards",
            ViewInsights => "The PLE projection and subject heat map",
        }
    }
}

/// The permission table as docs/ROLES.md prints it. A test keeps the two the
/// same, so the document can never describe a policy the code does not have.
pub fn roles_table() -> String {
    let columns: [(&str, Role, Option<Office>); 6] = [
        ("School Admin", Role::SchoolAdmin, None),
        ("Headteacher", Role::Teacher, Some(Office::HeadTeacher)),
        ("Director of Studies", Role::Teacher, Some(Office::DirectorOfStudies)),
        ("Secretary", Role::Teacher, Some(Office::Secretary)),
        ("Bursar", Role::Teacher, Some(Office::Bursar)),
        ("Teacher", Role::Teacher, None),
    ];
    let mut out = String::from("| Permission | ");
    out.push_str(&columns.iter().map(|c| c.0).collect::<Vec<_>>().join(" | "));
    out.push_str(" |\n|---|");
    out.push_str(&"---|".repeat(columns.len()));
    out.push('\n');
    for permission in ALL {
        out.push_str(&format!("| {} |", permission.describe()));
        for (_, role, office) in &columns {
            let mark = if permissions_for(*role, *office).contains(permission) { " ✓ |" } else { " |" };
            out.push_str(mark);
        }
        out.push('\n');
    }
    out
}

/// FR-C2: the same credentials open either view. Which one is in use changes
/// what the dashboard shows, never what the backend permits — permission comes
/// from the assignment table, not from the mode the user picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeacherMode {
    SubjectTeacher,
    ClassTeacher,
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub user_id: String,
    pub username: String,
    pub full_name: String,
    pub role: Role,
    pub office: Option<Office>,
    /// Derived from role and office at sign-in; see [`permissions_for`].
    pub permissions: HashSet<Permission>,
    pub must_change_password: bool,
    /// Only meaningful for `Role::Teacher`.
    pub mode: Option<TeacherMode>,
    /// Classes this teacher may act on at all (FR-C1).
    pub class_ids: HashSet<String>,
    /// Classes where this teacher is the Class Teacher or the Assistant.
    pub class_teacher_of: HashSet<String>,
    pub signed_in_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
}

impl Session {
    pub fn has(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }

    pub fn require(&self, permission: Permission) -> AppResult<()> {
        if self.has(permission) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    /// Whether this is the account that runs the system itself. Used only
    /// for FR-B4's count of School Admins, never to grant anything.
    pub fn is_school_admin(&self) -> bool {
        self.role == Role::SchoolAdmin
    }

    /// The school-wide offices see every class; a teacher sees only what
    /// FR-B6 assigned them.
    pub fn may_view_class(&self, class_id: &str) -> bool {
        self.has(ViewAllClasses) || self.class_ids.contains(class_id)
    }

    /// Roster edits, deadlines and report cards belong to the Class Teacher
    /// and the Assistant, not to every Subject Teacher in the room.
    pub fn may_manage_class(&self, class_id: &str) -> bool {
        self.has(ManageAllClasses) || self.class_teacher_of.contains(class_id)
    }

    /// Whether this person may reset the password of, retire, or change the
    /// office of someone with `target`'s role and office. Only when the
    /// target can do nothing this person cannot: otherwise a headteacher
    /// could reset the School Admin's password and sign in as them, gaining
    /// backups and settings they were never given.
    pub fn may_act_on(&self, role: Role, office: Option<Office>) -> bool {
        self.has(ManageStaff) && permissions_for(role, office).is_subset(&self.permissions)
    }

    pub fn require_act_on(&self, role: Role, office: Option<Office>) -> AppResult<()> {
        if self.may_act_on(role, office) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    /// Whether this person may write any subject's marks in a class: the
    /// class's own Class Teacher (by assignment, never by office), or someone
    /// trusted with every class's marks. A headteacher manages every class but
    /// does not edit marks, so office powers alone are not enough here.
    pub fn may_write_all_marks_in(&self, class_id: &str) -> bool {
        self.has(EditAllMarks) || self.class_teacher_of.contains(class_id)
    }

    /// A class list and learners' records, without their marks: the bursar
    /// and the secretary need these for every class.
    pub fn may_view_learners_of(&self, class_id: &str) -> bool {
        self.has(ViewAllLearners) || self.may_view_class(class_id)
    }

    pub fn require_view_learners_of(&self, class_id: &str) -> AppResult<()> {
        if self.may_view_learners_of(class_id) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    pub fn require_view_class(&self, class_id: &str) -> AppResult<()> {
        if self.may_view_class(class_id) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    pub fn require_manage_class(&self, class_id: &str) -> AppResult<()> {
        if self.may_manage_class(class_id) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }
}

/// What the frontend is allowed to know about the current session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub user_id: String,
    pub username: String,
    pub full_name: String,
    pub role: Role,
    pub office: Option<Office>,
    /// What the interface may offer. The backend checks every command again;
    /// this only decides which screens and buttons are shown.
    pub permissions: Vec<Permission>,
    pub mode: Option<TeacherMode>,
    pub must_change_password: bool,
    /// The account that runs the system itself (FR-B4).
    pub is_admin: bool,
    pub can_switch_to_class_teacher: bool,
    pub class_ids: Vec<String>,
    pub class_teacher_of: Vec<String>,
    pub signed_in_at: String,
}

impl From<&Session> for SessionView {
    fn from(session: &Session) -> Self {
        let mut class_ids: Vec<String> = session.class_ids.iter().cloned().collect();
        let mut class_teacher_of: Vec<String> = session.class_teacher_of.iter().cloned().collect();
        class_ids.sort();
        class_teacher_of.sort();

        let mut permissions: Vec<Permission> = session.permissions.iter().copied().collect();
        permissions.sort_by_key(|p| ALL.iter().position(|q| q == p));

        SessionView {
            user_id: session.user_id.clone(),
            username: session.username.clone(),
            full_name: session.full_name.clone(),
            role: session.role,
            office: session.office,
            permissions,
            mode: session.mode,
            must_change_password: session.must_change_password,
            is_admin: session.is_school_admin(),
            can_switch_to_class_teacher: !session.class_teacher_of.is_empty(),
            class_ids,
            class_teacher_of,
            signed_in_at: session.signed_in_at.to_rfc3339(),
        }
    }
}

#[derive(Default)]
pub struct SessionStore {
    current: RwLock<Option<Session>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sign_in(&self, session: Session) {
        *self.current.write() = Some(session);
    }

    pub fn sign_out(&self) {
        *self.current.write() = None;
    }

    /// Returns the signed-in session and refreshes its idle clock.
    ///
    /// This is the single choke point every protected command goes through, so
    /// the idle timeout is enforced once, here, rather than remembered in
    /// eighty places.
    pub fn require(&self) -> AppResult<Session> {
        let mut guard = self.current.write();
        let Some(session) = guard.as_mut() else {
            return Err(AppError::Unauthenticated);
        };

        let idle = Utc::now() - session.last_activity;
        if idle > Duration::minutes(IDLE_TIMEOUT_MINUTES) {
            *guard = None;
            return Err(AppError::Unauthenticated);
        }

        session.last_activity = Utc::now();
        Ok(session.clone())
    }

    /// Reads the session without extending it — for the frontend's
    /// "am I still signed in?" poll, which should not itself keep a session
    /// alive on an unattended machine.
    pub fn peek(&self) -> Option<Session> {
        let guard = self.current.read();
        let session = guard.as_ref()?;
        if Utc::now() - session.last_activity > Duration::minutes(IDLE_TIMEOUT_MINUTES) {
            return None;
        }
        Some(session.clone())
    }

    pub fn set_mode(&self, mode: TeacherMode) -> AppResult<Session> {
        let mut guard = self.current.write();
        let Some(session) = guard.as_mut() else {
            return Err(AppError::Unauthenticated);
        };

        if mode == TeacherMode::ClassTeacher && session.class_teacher_of.is_empty() {
            return Err(AppError::Forbidden);
        }

        session.mode = Some(mode);
        session.last_activity = Utc::now();
        Ok(session.clone())
    }

    pub fn clear_password_change_flag(&self) {
        if let Some(session) = self.current.write().as_mut() {
            session.must_change_password = false;
        }
    }
}

/// A signed-in session for tests in any module: `class_teacher_of` are the
/// classes this person is Class Teacher of (and so may also view).
#[cfg(test)]
pub fn test_session(id: &str, role: Role, office: Option<Office>, class_teacher_of: &[&str]) -> Session {
    let classes: HashSet<String> = class_teacher_of.iter().map(|c| c.to_string()).collect();
    Session {
        user_id: id.into(),
        username: id.into(),
        full_name: id.into(),
        role,
        office,
        permissions: permissions_for(role, office),
        must_change_password: false,
        mode: None,
        class_ids: classes.clone(),
        class_teacher_of: classes,
        signed_in_at: Utc::now(),
        last_activity: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn teacher(class_teacher_of: &[&str], class_ids: &[&str]) -> Session {
        let mut session = test_session("u1", Role::Teacher, None, class_teacher_of);
        session.mode = Some(TeacherMode::SubjectTeacher);
        session.class_ids = class_ids.iter().map(|s| s.to_string()).collect();
        session
    }

    #[test]
    fn teacher_scope_is_enforced() {
        let session = teacher(&["p5"], &["p5", "p6"]);
        assert!(session.may_view_class("p6"));
        assert!(!session.may_manage_class("p6"));
        assert!(session.may_manage_class("p5"));
        assert!(!session.may_view_class("p7"));
        assert!(session.require(ManageSchool).is_err());
        assert!(session.permissions.is_empty(), "a teacher's powers come from assignments only");
    }

    #[test]
    fn admin_sees_every_class() {
        let session = test_session("a", Role::SchoolAdmin, None, &[]);
        assert!(session.may_view_class("anything"));
        assert!(session.may_manage_class("anything"));
        assert!(ALL.iter().all(|p| session.has(*p)), "the School Admin can do everything");
    }

    #[test]
    fn a_headteacher_runs_the_school_but_not_the_computer_or_the_marks() {
        let head = test_session("h", Role::Teacher, Some(Office::HeadTeacher), &[]);
        assert!(head.may_view_class("p5") && head.may_manage_class("p5"));
        for allowed in [HeadTeacherComment, ManageStaff, ViewAudit, ManageFees, SendMessages] {
            assert!(head.has(allowed), "{allowed:?}");
        }
        for denied in [ManageBackups, ManageSchool, EditAllMarks] {
            assert!(!head.has(denied), "{denied:?}");
        }
    }

    #[test]
    fn a_secretary_prints_report_cards_but_never_edits_marks() {
        let sec = test_session("s", Role::Teacher, Some(Office::Secretary), &[]);
        assert!(sec.may_view_class("p5"), "reads marks to print report cards");
        assert!(!sec.may_manage_class("p5"));
        assert!(!sec.has(EditAllMarks) && !sec.has(ManageFees) && !sec.has(HeadTeacherComment));
        assert!(sec.has(IssuePassOuts) && sec.has(ManageLearners));
    }

    #[test]
    fn a_bursar_sees_learners_and_fees_but_no_marks() {
        let bursar = test_session("b", Role::Teacher, Some(Office::Bursar), &[]);
        assert!(bursar.may_view_learners_of("p5"));
        assert!(!bursar.may_view_class("p5"), "marks are not the bursar's business");
        assert!(bursar.has(ManageFees) && bursar.has(ViewFinance));
        assert!(!bursar.has(VoidReceipts), "whoever holds the cash cannot cancel its receipt");
    }

    #[test]
    fn the_director_of_studies_owns_academics_not_money_or_the_gate() {
        let dos = test_session("d", Role::Teacher, Some(Office::DirectorOfStudies), &[]);
        assert!(dos.has(EditAllMarks) && dos.has(ManageAcademics) && dos.has(ViewInsights));
        assert!(!dos.has(ManageFees) && !dos.has(IssuePassOuts) && !dos.has(ManageStaff));
    }

    #[test]
    fn an_office_adds_to_a_teachers_own_classes() {
        // The headteacher who also teaches P7 keeps P7 as their own class.
        let head_who_teaches = test_session("h", Role::Teacher, Some(Office::HeadTeacher), &["p7"]);
        assert!(head_who_teaches.class_teacher_of.contains("p7"));
        assert!(head_who_teaches.has(HeadTeacherComment));
    }

    #[test]
    fn nobody_can_take_over_an_account_more_powerful_than_their_own() {
        let head = test_session("h", Role::Teacher, Some(Office::HeadTeacher), &[]);
        let admin = test_session("a", Role::SchoolAdmin, None, &[]);
        assert!(!head.may_act_on(Role::SchoolAdmin, None), "the head cannot reset the School Admin");
        assert!(head.may_act_on(Role::Teacher, None));
        assert!(head.may_act_on(Role::Teacher, Some(Office::Bursar)));
        assert!(!head.may_act_on(Role::Teacher, Some(Office::DirectorOfStudies)), "the DOS can edit marks; the head cannot");
        assert!(admin.may_act_on(Role::SchoolAdmin, None));
        let sec = test_session("s", Role::Teacher, Some(Office::Secretary), &[]);
        assert!(!sec.may_act_on(Role::Teacher, None), "managing staff is not the secretary's job");
    }

    #[test]
    fn managing_every_class_does_not_mean_editing_its_marks() {
        let head = test_session("h", Role::Teacher, Some(Office::HeadTeacher), &[]);
        assert!(head.may_manage_class("p5"));
        assert!(!head.may_write_all_marks_in("p5"));
        let dos = test_session("d", Role::Teacher, Some(Office::DirectorOfStudies), &[]);
        assert!(dos.may_write_all_marks_in("p5"));
        let class_teacher = test_session("t", Role::Teacher, None, &["p5"]);
        assert!(class_teacher.may_write_all_marks_in("p5") && !class_teacher.may_write_all_marks_in("p6"));
    }

    #[test]
    fn docs_roles_md_matches_the_policy() {
        let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/ROLES.md"))
            .expect("docs/ROLES.md");
        let table = roles_table();
        assert!(
            doc.contains(&table),
            "docs/ROLES.md is out of date. Replace its table with:\n\n{table}"
        );
    }

    #[test]
    fn offices_round_trip_and_unknown_ones_are_refused() {
        for office in [Office::HeadTeacher, Office::DirectorOfStudies, Office::Secretary, Office::Bursar] {
            assert_eq!(Office::parse(office.as_str()).unwrap(), office);
        }
        assert!(Office::parse("janitor").is_err());
    }

    #[test]
    fn idle_sessions_expire() {
        let store = SessionStore::new();
        let mut session = teacher(&["p5"], &["p5"]);
        session.last_activity = Utc::now() - Duration::minutes(IDLE_TIMEOUT_MINUTES + 1);
        store.sign_in(session);

        assert!(store.peek().is_none());
        assert!(matches!(store.require(), Err(AppError::Unauthenticated)));
        // The expired session is cleared, not merely rejected.
        assert!(store.peek().is_none());
    }

    #[test]
    fn class_teacher_mode_needs_a_class() {
        let store = SessionStore::new();
        store.sign_in(teacher(&[], &["p5"]));
        assert!(store.set_mode(TeacherMode::ClassTeacher).is_err());
        assert!(store.set_mode(TeacherMode::SubjectTeacher).is_ok());

        store.sign_in(teacher(&["p5"], &["p5"]));
        assert!(store.set_mode(TeacherMode::ClassTeacher).is_ok());
    }
}
