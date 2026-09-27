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

/// How long a session survives with no activity before RM locks itself.
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
    pub fn is_admin(&self) -> bool {
        self.role == Role::SchoolAdmin
    }

    /// A School Admin sees the whole institution; a teacher sees only what
    /// FR-B6 assigned them.
    pub fn may_view_class(&self, class_id: &str) -> bool {
        self.is_admin() || self.class_ids.contains(class_id)
    }

    /// Roster edits, deadlines and report cards belong to the Class Teacher
    /// and the Assistant, not to every Subject Teacher in the room.
    pub fn may_manage_class(&self, class_id: &str) -> bool {
        self.is_admin() || self.class_teacher_of.contains(class_id)
    }

    pub fn require_admin(&self) -> AppResult<()> {
        if self.is_admin() {
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
pub struct SessionView {
    pub user_id: String,
    pub username: String,
    pub full_name: String,
    pub role: Role,
    pub mode: Option<TeacherMode>,
    pub must_change_password: bool,
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

        SessionView {
            user_id: session.user_id.clone(),
            username: session.username.clone(),
            full_name: session.full_name.clone(),
            role: session.role,
            mode: session.mode,
            must_change_password: session.must_change_password,
            is_admin: session.is_admin(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn teacher(class_teacher_of: &[&str], class_ids: &[&str]) -> Session {
        Session {
            user_id: "u1".into(),
            username: "jane".into(),
            full_name: "Jane Nakato".into(),
            role: Role::Teacher,
            must_change_password: false,
            mode: Some(TeacherMode::SubjectTeacher),
            class_ids: class_ids.iter().map(|s| s.to_string()).collect(),
            class_teacher_of: class_teacher_of.iter().map(|s| s.to_string()).collect(),
            signed_in_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }

    #[test]
    fn teacher_scope_is_enforced() {
        let session = teacher(&["p5"], &["p5", "p6"]);
        assert!(session.may_view_class("p6"));
        assert!(!session.may_manage_class("p6"));
        assert!(session.may_manage_class("p5"));
        assert!(!session.may_view_class("p7"));
        assert!(session.require_admin().is_err());
    }

    #[test]
    fn admin_sees_every_class() {
        let mut session = teacher(&[], &[]);
        session.role = Role::SchoolAdmin;
        assert!(session.may_view_class("anything"));
        assert!(session.may_manage_class("anything"));
        assert!(session.require_admin().is_ok());
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
