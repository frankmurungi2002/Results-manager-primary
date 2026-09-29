//! What a Ugandan nursery-and-primary school gets pre-filled at setup.
//!
//! FR-B3 and FR-B5 both say "sensible Ugandan defaults pre-fill, adjustable if
//! a school's calendar differs". These are those defaults. Nothing here is
//! enforced — a school can rename, reorder, add and retire freely; the ladder
//! position is what actually drives promotion (FR-E3), so a renamed class still
//! promotes correctly.

/// The promotion ladder: Baby → Middle → Top → P1 → … → P7 → PLE candidate.
pub struct DefaultClass {
    pub code: &'static str,
    pub name: &'static str,
    pub level_kind: &'static str,
    pub ladder_position: i64,
    pub grading_system_id: &'static str,
}

pub const DEFAULT_CLASSES: &[DefaultClass] = &[
    DefaultClass { code: "BABY",   name: "Baby Class",   level_kind: "nursery", ladder_position: 1,  grading_system_id: "gs_nursery_desc" },
    DefaultClass { code: "MIDDLE", name: "Middle Class", level_kind: "nursery", ladder_position: 2,  grading_system_id: "gs_nursery_desc" },
    DefaultClass { code: "TOP",    name: "Top Class",    level_kind: "nursery", ladder_position: 3,  grading_system_id: "gs_nursery_desc" },
    DefaultClass { code: "P1",     name: "P1",           level_kind: "primary", ladder_position: 4,  grading_system_id: "gs_uneb_primary" },
    DefaultClass { code: "P2",     name: "P2",           level_kind: "primary", ladder_position: 5,  grading_system_id: "gs_uneb_primary" },
    DefaultClass { code: "P3",     name: "P3",           level_kind: "primary", ladder_position: 6,  grading_system_id: "gs_uneb_primary" },
    DefaultClass { code: "P4",     name: "P4",           level_kind: "primary", ladder_position: 7,  grading_system_id: "gs_uneb_primary" },
    DefaultClass { code: "P5",     name: "P5",           level_kind: "primary", ladder_position: 8,  grading_system_id: "gs_uneb_primary" },
    DefaultClass { code: "P6",     name: "P6",           level_kind: "primary", ladder_position: 9,  grading_system_id: "gs_uneb_primary" },
    DefaultClass { code: "P7",     name: "P7",           level_kind: "primary", ladder_position: 10, grading_system_id: "gs_uneb_primary" },
];

/// The last rung. A P7 leaver is archived as a graduate (FR-E3).
pub const PLE_CANDIDATE_POSITION: i64 = 11;

pub struct DefaultSubject {
    pub code: &'static str,
    pub name: &'static str,
    /// Whether it counts toward the UNEB division aggregate.
    pub is_core: bool,
    /// Which level kinds get it by default.
    pub levels: &'static [&'static str],
}

pub const DEFAULT_SUBJECTS: &[DefaultSubject] = &[
    // The four PLE core subjects.
    DefaultSubject { code: "ENG",  name: "English",           is_core: true,  levels: &["primary"] },
    DefaultSubject { code: "MTC",  name: "Mathematics",       is_core: true,  levels: &["primary"] },
    DefaultSubject { code: "SCI",  name: "Science",           is_core: true,  levels: &["primary"] },
    DefaultSubject { code: "SST",  name: "Social Studies",    is_core: true,  levels: &["primary"] },
    // Commonly taught, reported but not aggregated.
    DefaultSubject { code: "RE",   name: "Religious Education", is_core: false, levels: &["primary"] },
    DefaultSubject { code: "KIS",  name: "Kiswahili",         is_core: false, levels: &["primary"] },
    DefaultSubject { code: "COMP", name: "Computer Studies",  is_core: false, levels: &["primary"] },
    DefaultSubject { code: "PE",   name: "Physical Education", is_core: false, levels: &["primary"] },
    // Nursery: the five learning areas of the national ECCE framework.
    DefaultSubject { code: "LA1", name: "Learning Area 1", is_core: true, levels: &["nursery"] },
    DefaultSubject { code: "LA2", name: "Learning Area 2", is_core: true, levels: &["nursery"] },
    DefaultSubject { code: "LA3", name: "Learning Area 3", is_core: true, levels: &["nursery"] },
    DefaultSubject { code: "LA4", name: "Learning Area 4", is_core: true, levels: &["nursery"] },
    DefaultSubject { code: "LA5", name: "Learning Area 5", is_core: true, levels: &["nursery"] },
];

/// FR-B3: three terms a year, with BOT / MID / EOT each term.
pub struct DefaultExam {
    pub code: &'static str,
    pub name: &'static str,
    pub weight: f64,
    pub is_final: bool,
}

pub const DEFAULT_EXAMS: &[DefaultExam] = &[
    DefaultExam { code: "BOT", name: "Beginning of Term", weight: 0.20, is_final: false },
    DefaultExam { code: "MID", name: "Mid Term",          weight: 0.30, is_final: false },
    DefaultExam { code: "EOT", name: "End of Term",       weight: 0.50, is_final: true },
];

pub const DEFAULT_TERM_NAMES: &[&str] = &["Term 1", "Term 2", "Term 3"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ladder_has_no_gaps_and_no_duplicates() {
        let mut positions: Vec<i64> = DEFAULT_CLASSES.iter().map(|c| c.ladder_position).collect();
        positions.sort();
        assert_eq!(positions, (1..=10).collect::<Vec<_>>());
    }

    #[test]
    fn exactly_four_primary_subjects_are_core() {
        let core = DEFAULT_SUBJECTS
            .iter()
            .filter(|s| s.is_core && s.levels.contains(&"primary"))
            .count();
        assert_eq!(core, 4, "PLE division is defined for four core subjects");
    }

    #[test]
    fn exam_weights_sum_to_one() {
        let total: f64 = DEFAULT_EXAMS.iter().map(|e| e.weight).sum();
        assert!((total - 1.0).abs() < 1e-9);
        assert_eq!(DEFAULT_EXAMS.iter().filter(|e| e.is_final).count(), 1);
    }

    #[test]
    fn subject_codes_are_unique() {
        let mut codes: Vec<&str> = DEFAULT_SUBJECTS.iter().map(|s| s.code).collect();
        let before = codes.len();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), before);
    }
}
