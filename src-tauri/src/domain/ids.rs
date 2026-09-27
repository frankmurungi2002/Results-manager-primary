//! Record identifiers.
//!
//! Prefixed UUIDs rather than autoincrement integers: an id that carries its
//! own type is far easier to trace through an audit log, an Excel export or a
//! support conversation, and it lets records from two SSDs or two exports be
//! merged without collision.

use uuid::Uuid;

pub fn new_id(prefix: &str) -> String {
    let raw = Uuid::new_v4().simple().to_string();
    format!("{prefix}_{}", &raw[..20])
}

pub mod prefix {
    pub const USER: &str = "usr";
    pub const CLASS: &str = "cls";
    pub const STREAM: &str = "str";
    pub const SUBJECT: &str = "sub";
    pub const CLASS_SUBJECT: &str = "csb";
    pub const STUDENT: &str = "stu";
    pub const ENROLLMENT: &str = "enr";
    pub const MARK: &str = "mrk";
    pub const YEAR: &str = "yr";
    pub const TERM: &str = "trm";
    pub const EXAM: &str = "exm";
    pub const ASSIGNMENT: &str = "asg";
    pub const GRADING_SYSTEM: &str = "gs";
    pub const GRADING_BAND: &str = "gb";
    pub const ATTENDANCE: &str = "att";
    pub const COMMENT: &str = "cmt";
    pub const REPORT_COMMENT: &str = "rcm";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_prefixed_and_unique() {
        let a = new_id(prefix::STUDENT);
        let b = new_id(prefix::STUDENT);
        assert!(a.starts_with("stu_"));
        assert_ne!(a, b);
        assert_eq!(a.len(), 24);
    }
}
