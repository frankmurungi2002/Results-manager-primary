//! The grading engine.
//!
//! One rule holds everywhere: a raw score is converted to a **percentage of
//! that subject's maximum** before any band is consulted. Bands are therefore
//! stored as percentages and a subject scored out of 40 grades identically to
//! one scored out of 100 — which is what makes a per-subject `max_score`
//! override (FR-C13) safe.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradingKind {
    Numeric,
    Descriptive,
    Letter,
    Percentage,
}

impl GradingKind {
    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "numeric" => Ok(GradingKind::Numeric),
            "descriptive" => Ok(GradingKind::Descriptive),
            "letter" => Ok(GradingKind::Letter),
            "percentage" => Ok(GradingKind::Percentage),
            other => Err(AppError::internal(format!("unknown grading kind '{other}'"))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            GradingKind::Numeric => "numeric",
            GradingKind::Descriptive => "descriptive",
            GradingKind::Letter => "letter",
            GradingKind::Percentage => "percentage",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Band {
    pub id: String,
    pub label: String,
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub points: Option<f64>,
    pub remark: Option<String>,
    pub position: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradingSystem {
    pub id: String,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub kind: GradingKind,
    pub is_builtin: bool,
    pub is_editable: bool,
    /// Ordered by `position`, best band first.
    pub bands: Vec<Band>,
}

/// One graded mark, ready to print.
#[derive(Debug, Clone, Serialize)]
pub struct GradedMark {
    pub score: Option<f64>,
    pub max_score: f64,
    pub percentage: Option<f64>,
    pub grade_label: String,
    pub points: Option<f64>,
    pub remark: Option<String>,
    pub is_absent: bool,
    pub passed: Option<bool>,
}

impl GradingSystem {
    /// Finds the band a percentage falls into.
    ///
    /// Bounds are inclusive at both ends, which is how a school writes them
    /// ("75–79 is D2"). `validate` guarantees no overlap, so inclusivity at
    /// both ends is unambiguous.
    pub fn band_for(&self, percentage: f64) -> Option<&Band> {
        let clamped = percentage.clamp(0.0, 100.0);
        self.bands
            .iter()
            .find(|band| clamped >= band.lower_bound && clamped <= band.upper_bound)
    }

    /// Grades one mark against this system.
    pub fn grade(
        &self,
        score: Option<f64>,
        is_absent: bool,
        max_score: f64,
        pass_mark: Option<f64>,
    ) -> GradedMark {
        if is_absent || score.is_none() {
            return GradedMark {
                score: None,
                max_score,
                percentage: None,
                grade_label: if is_absent { "ABS".into() } else { "—".into() },
                points: None,
                remark: None,
                is_absent,
                passed: None,
            };
        }

        let raw = score.unwrap_or(0.0);
        let percentage = if max_score > 0.0 {
            (raw / max_score) * 100.0
        } else {
            0.0
        };

        // Percentage-only systems report the number itself rather than a band.
        if self.kind == GradingKind::Percentage {
            return GradedMark {
                score: Some(raw),
                max_score,
                percentage: Some(round2(percentage)),
                grade_label: format!("{:.0}%", percentage.round()),
                points: None,
                remark: None,
                is_absent: false,
                passed: pass_mark.map(|mark| raw >= mark),
            };
        }

        match self.band_for(percentage) {
            Some(band) => GradedMark {
                score: Some(raw),
                max_score,
                percentage: Some(round2(percentage)),
                grade_label: band.label.clone(),
                points: band.points,
                remark: band.remark.clone(),
                is_absent: false,
                passed: pass_mark.map(|mark| raw >= mark),
            },
            None => GradedMark {
                score: Some(raw),
                max_score,
                percentage: Some(round2(percentage)),
                grade_label: "—".into(),
                points: None,
                remark: None,
                is_absent: false,
                passed: pass_mark.map(|mark| raw >= mark),
            },
        }
    }

    /// Rejects a band set that would grade a mark ambiguously or not at all.
    ///
    /// This runs before the Custom Band Builder (FR-B5) saves, because a gap or
    /// an overlap here becomes a wrong report card months later, which is
    /// exactly the failure mode that destroys trust in the product.
    pub fn validate(bands: &[Band]) -> AppResult<()> {
        if bands.is_empty() {
            return Err(AppError::validation(
                "A grading system needs at least one band.",
            ));
        }

        for band in bands {
            if band.label.trim().is_empty() {
                return Err(AppError::validation("Every band needs a label."));
            }
            if band.lower_bound > band.upper_bound {
                return Err(AppError::validation(format!(
                    "Band '{}' has its lower bound above its upper bound.",
                    band.label
                )));
            }
            if band.lower_bound < 0.0 || band.upper_bound > 100.0 {
                return Err(AppError::validation(format!(
                    "Band '{}' must sit between 0 and 100.",
                    band.label
                )));
            }
        }

        let mut sorted: Vec<&Band> = bands.iter().collect();
        sorted.sort_by(|a, b| {
            a.lower_bound
                .partial_cmp(&b.lower_bound)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Must start at 0 and finish at 100, with no gap and no overlap.
        if sorted[0].lower_bound > 0.0 {
            return Err(AppError::validation(format!(
                "Nothing covers 0 to {}. Every mark must land in a band.",
                sorted[0].lower_bound
            )));
        }

        for pair in sorted.windows(2) {
            let (left, right) = (pair[0], pair[1]);
            if right.lower_bound <= left.upper_bound {
                return Err(AppError::validation(format!(
                    "Bands '{}' and '{}' overlap.",
                    left.label, right.label
                )));
            }
            // Bounds are inclusive, so the next band must start at the previous
            // upper bound plus one step. Anything wider leaves a hole.
            if right.lower_bound - left.upper_bound > 1.000_001 {
                return Err(AppError::validation(format!(
                    "Nothing covers the marks between '{}' and '{}'.",
                    left.label, right.label
                )));
            }
        }

        let top = sorted.last().expect("non-empty");
        if top.upper_bound < 100.0 {
            return Err(AppError::validation(format!(
                "Nothing covers {} to 100. Every mark must land in a band.",
                top.upper_bound
            )));
        }

        Ok(())
    }
}

/// The end-of-term summary line on a report card (FR-D1).
#[derive(Debug, Clone, Serialize)]
pub struct Aggregate {
    /// Sum of grade points across the core subjects, where the grading system
    /// has points at all.
    pub total_points: Option<f64>,
    /// Count of core subjects that actually contributed.
    pub core_subjects_counted: usize,
    /// Mean percentage across every graded subject, core or not.
    pub mean_percentage: Option<f64>,
    /// UNEB division, when the aggregate was built from exactly four core
    /// subjects graded 1–9.
    pub division: Option<String>,
}

/// Computes the aggregate for one learner from their graded marks.
///
/// `core` marks the subjects that count toward the division; in Ugandan primary
/// that is English, Mathematics, Science and Social Studies.
pub fn aggregate(marks: &[(bool, &GradedMark)]) -> Aggregate {
    let mut total_points = 0.0;
    let mut core_counted = 0usize;
    let mut percentage_sum = 0.0;
    let mut percentage_counted = 0usize;
    let mut any_points = false;

    for (is_core, mark) in marks {
        if let Some(pct) = mark.percentage {
            percentage_sum += pct;
            percentage_counted += 1;
        }
        if *is_core {
            if let Some(points) = mark.points {
                total_points += points;
                core_counted += 1;
                any_points = true;
            }
        }
    }

    Aggregate {
        total_points: any_points.then_some(total_points),
        core_subjects_counted: core_counted,
        mean_percentage: (percentage_counted > 0)
            .then(|| round2(percentage_sum / percentage_counted as f64)),
        division: if any_points {
            division_for(total_points, core_counted)
        } else {
            None
        },
    }
}

/// The published UNEB primary division boundaries, for an aggregate of the four
/// core subjects each graded 1–9. `(label, lowest aggregate, highest aggregate)`,
/// best division first.
///
/// Verify against the current UNEB circular before a school goes live; boards
/// revise boundaries, and RM will print whatever is here without complaint.
pub const PLE_DIVISIONS: &[(&str, i64, i64)] = &[
    ("Division 1", 4, 12),
    ("Division 2", 13, 23),
    ("Division 3", 24, 29),
    ("Division 4", 30, 34),
    ("Ungraded (U)", 35, 36),
];

/// The number of core subjects the division boundaries are defined for.
pub const PLE_CORE_SUBJECTS: usize = 4;

/// UNEB primary division from a four-subject aggregate.
///
/// Only computed for exactly four core subjects, because the published
/// boundaries are defined for that and nothing else. With a different number of
/// core subjects RM shows the aggregate and stays quiet about the division
/// rather than inventing a boundary.
pub fn division_for(total_points: f64, core_count: usize) -> Option<String> {
    if core_count != PLE_CORE_SUBJECTS {
        return None;
    }

    let aggregate = total_points.round() as i64;
    PLE_DIVISIONS
        .iter()
        .find(|(_, low, high)| aggregate >= *low && aggregate <= *high)
        .map(|(label, _, _)| (*label).to_string())
}

/// The aggregate a learner would have to reach to sit one division higher.
///
/// Returns `None` for a learner already in Division 1, or an aggregate outside
/// the published range.
pub fn next_division_target(total_points: f64) -> Option<(&'static str, i64)> {
    let aggregate = total_points.round() as i64;
    let index = PLE_DIVISIONS
        .iter()
        .position(|(_, low, high)| aggregate >= *low && aggregate <= *high)?;
    if index == 0 {
        return None; // Already in Division 1.
    }
    let (label, _, high) = PLE_DIVISIONS[index - 1];
    Some((label, high))
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band(label: &str, lower: f64, upper: f64, points: Option<f64>, position: i64) -> Band {
        Band {
            id: format!("b_{label}"),
            label: label.into(),
            lower_bound: lower,
            upper_bound: upper,
            points,
            remark: None,
            position,
        }
    }

    fn uneb() -> GradingSystem {
        GradingSystem {
            id: "gs_uneb_primary".into(),
            code: "UNEB_PRIMARY".into(),
            name: "UNEB Primary".into(),
            description: None,
            kind: GradingKind::Numeric,
            is_builtin: true,
            is_editable: false,
            bands: vec![
                band("D1", 80.0, 100.0, Some(1.0), 1),
                band("D2", 75.0, 79.0, Some(2.0), 2),
                band("C3", 70.0, 74.0, Some(3.0), 3),
                band("C4", 65.0, 69.0, Some(4.0), 4),
                band("C5", 60.0, 64.0, Some(5.0), 5),
                band("C6", 55.0, 59.0, Some(6.0), 6),
                band("P7", 50.0, 54.0, Some(7.0), 7),
                band("P8", 45.0, 49.0, Some(8.0), 8),
                band("F9", 0.0, 44.0, Some(9.0), 9),
            ],
        }
    }

    #[test]
    fn boundaries_grade_the_way_a_school_reads_them() {
        let system = uneb();
        assert_eq!(system.band_for(80.0).unwrap().label, "D1");
        assert_eq!(system.band_for(79.0).unwrap().label, "D2");
        assert_eq!(system.band_for(100.0).unwrap().label, "D1");
        assert_eq!(system.band_for(0.0).unwrap().label, "F9");
        assert_eq!(system.band_for(44.0).unwrap().label, "F9");
        assert_eq!(system.band_for(45.0).unwrap().label, "P8");
    }

    #[test]
    fn a_subject_out_of_forty_grades_like_one_out_of_a_hundred() {
        let system = uneb();
        // 32/40 and 80/100 are both 80%.
        let small = system.grade(Some(32.0), false, 40.0, Some(20.0));
        let large = system.grade(Some(80.0), false, 100.0, Some(50.0));
        assert_eq!(small.grade_label, "D1");
        assert_eq!(large.grade_label, "D1");
        assert_eq!(small.percentage, large.percentage);
    }

    #[test]
    fn absence_is_not_a_zero() {
        let system = uneb();
        let absent = system.grade(None, true, 100.0, Some(50.0));
        assert_eq!(absent.grade_label, "ABS");
        assert!(absent.points.is_none());
        assert!(absent.percentage.is_none());

        // A genuine zero still grades as F9 — the two must not be confused.
        let zero = system.grade(Some(0.0), false, 100.0, Some(50.0));
        assert_eq!(zero.grade_label, "F9");
        assert_eq!(zero.points, Some(9.0));
    }

    #[test]
    fn division_needs_exactly_four_core_subjects() {
        let system = uneb();
        let d1 = system.grade(Some(85.0), false, 100.0, None);
        let marks_four: Vec<(bool, &GradedMark)> =
            vec![(true, &d1), (true, &d1), (true, &d1), (true, &d1)];
        let result = aggregate(&marks_four);
        assert_eq!(result.total_points, Some(4.0));
        assert_eq!(result.division.as_deref(), Some("Division 1"));

        let marks_three: Vec<(bool, &GradedMark)> = vec![(true, &d1), (true, &d1), (true, &d1)];
        assert!(aggregate(&marks_three).division.is_none());
    }

    #[test]
    fn non_core_subjects_stay_out_of_the_aggregate() {
        let system = uneb();
        let d1 = system.grade(Some(85.0), false, 100.0, None);
        let f9 = system.grade(Some(10.0), false, 100.0, None);
        let marks: Vec<(bool, &GradedMark)> = vec![
            (true, &d1),
            (true, &d1),
            (true, &d1),
            (true, &d1),
            (false, &f9),
        ];
        let result = aggregate(&marks);
        assert_eq!(result.total_points, Some(4.0));
        assert_eq!(result.core_subjects_counted, 4);
        // …but it still moves the mean.
        assert!(result.mean_percentage.unwrap() < 85.0);
    }

    #[test]
    fn validation_catches_gaps_and_overlaps() {
        // A gap between 50 and 60.
        let gap = vec![
            band("A", 60.0, 100.0, None, 1),
            band("B", 0.0, 50.0, None, 2),
        ];
        assert!(GradingSystem::validate(&gap).is_err());

        // An overlap at 60.
        let overlap = vec![
            band("A", 60.0, 100.0, None, 1),
            band("B", 0.0, 60.0, None, 2),
        ];
        assert!(GradingSystem::validate(&overlap).is_err());

        // Does not reach 100.
        let short = vec![band("A", 0.0, 90.0, None, 1)];
        assert!(GradingSystem::validate(&short).is_err());

        assert!(GradingSystem::validate(&uneb().bands).is_ok());
    }
}
