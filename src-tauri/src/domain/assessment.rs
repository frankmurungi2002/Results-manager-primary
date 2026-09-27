//! How a term's examinations combine into one mark, and what it would take to
//! move a learner up a grade.
//!
//! The report card and the PLE projector must never disagree about a learner's
//! term mark — a headteacher shown "Division 2, needs +3 in Science" and then
//! handed a report card computed differently would stop trusting both. So the
//! arithmetic lives here, once, and both call it.

use serde::Serialize;

use crate::domain::grading::GradingSystem;

/// One examination's place in the term.
#[derive(Debug, Clone, Copy)]
pub struct ExamSlot {
    /// Share of the term mark, 0..1. Zero means "recorded but not counted".
    pub weight: f64,
    /// The set that produces the end-of-term report card.
    pub is_final: bool,
}

/// Combines a learner's scores for one subject into a single term mark.
///
/// `scores` is parallel to `slots`; `None` means the mark was never entered, or
/// the learner was absent. Either way it is **excluded from the weighting**
/// rather than counted as a zero — the divisor shrinks with it. A learner who
/// missed the Beginning of Term paper is judged on the papers they sat.
pub fn term_score(slots: &[ExamSlot], scores: &[Option<f64>]) -> Option<f64> {
    let mut weighted_total = 0.0;
    let mut weight_used = 0.0;
    let mut final_score = None;
    let mut first_available = None;

    for (slot, score) in slots.iter().zip(scores.iter()) {
        let Some(value) = score else { continue };

        if first_available.is_none() {
            first_available = Some(*value);
        }
        if slot.is_final {
            final_score = Some(*value);
        }
        if slot.weight > 0.0 {
            weighted_total += value * slot.weight;
            weight_used += slot.weight;
        }
    }

    if weight_used > 0.0 {
        Some(weighted_total / weight_used)
    } else if final_score.is_some() {
        final_score
    } else {
        first_available
    }
}

/// What it would take to lift one subject to the next grade up.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BandGap {
    /// The grade the learner would reach.
    pub next_label: String,
    /// Extra raw marks needed, on this subject's own scale.
    pub marks_needed: f64,
    /// How much the aggregate would fall. Lower is better in UNEB grading.
    pub points_gained: f64,
}

/// The cheapest single step up from where a learner currently sits.
///
/// Returns `None` when the learner is already in the top band, when the system
/// has no grade points (so there is no aggregate to improve), or when the mark
/// is missing.
pub fn gap_to_next_band(
    system: &GradingSystem,
    score: f64,
    max_score: f64,
) -> Option<BandGap> {
    if max_score <= 0.0 {
        return None;
    }

    let percentage = (score / max_score * 100.0).clamp(0.0, 100.0);
    let current = system.band_for(percentage)?;
    let current_points = current.points?;

    // The next band up is the one whose lower bound sits just above this band's
    // upper bound. Bands are validated to be contiguous, so "just above" is
    // exact rather than a search for the nearest.
    let next = system
        .bands
        .iter()
        .filter(|band| band.lower_bound > current.upper_bound)
        .min_by(|a, b| {
            a.lower_bound
                .partial_cmp(&b.lower_bound)
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;

    let next_points = next.points?;
    if next_points >= current_points {
        return None; // Not actually an improvement in this system.
    }

    // Marks are whole numbers on a report card, so round the requirement up —
    // telling a teacher "+2.3 marks" is not actionable.
    let target_score = next.lower_bound / 100.0 * max_score;
    let marks_needed = (target_score - score).max(0.0).ceil();

    Some(BandGap {
        next_label: next.label.clone(),
        marks_needed,
        points_gained: current_points - next_points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::grading::{Band, GradingKind};

    fn slots() -> Vec<ExamSlot> {
        vec![
            ExamSlot { weight: 0.2, is_final: false },
            ExamSlot { weight: 0.3, is_final: false },
            ExamSlot { weight: 0.5, is_final: true },
        ]
    }

    fn uneb() -> GradingSystem {
        let rows = [
            ("D1", 80.0, 100.0, 1.0),
            ("D2", 75.0, 79.0, 2.0),
            ("C3", 70.0, 74.0, 3.0),
            ("C4", 65.0, 69.0, 4.0),
            ("C5", 60.0, 64.0, 5.0),
            ("C6", 55.0, 59.0, 6.0),
            ("P7", 50.0, 54.0, 7.0),
            ("P8", 45.0, 49.0, 8.0),
            ("F9", 0.0, 44.0, 9.0),
        ];
        GradingSystem {
            id: "gs_uneb_primary".into(),
            code: "UNEB_PRIMARY".into(),
            name: "UNEB Primary".into(),
            description: None,
            kind: GradingKind::Numeric,
            is_builtin: true,
            is_editable: false,
            bands: rows
                .iter()
                .enumerate()
                .map(|(index, (label, lo, hi, pts))| Band {
                    id: format!("b_{label}"),
                    label: (*label).into(),
                    lower_bound: *lo,
                    upper_bound: *hi,
                    points: Some(*pts),
                    remark: None,
                    position: index as i64 + 1,
                })
                .collect(),
        }
    }

    #[test]
    fn weights_combine_the_way_a_school_expects() {
        let got = term_score(&slots(), &[Some(85.0), Some(88.0), Some(90.0)]);
        // 0.2*85 + 0.3*88 + 0.5*90 = 17 + 26.4 + 45
        assert_eq!(got, Some(88.4));
    }

    #[test]
    fn a_missed_paper_shrinks_the_divisor_instead_of_scoring_zero() {
        // Absent for the first paper, which carried 20% of the term.
        let got = term_score(&slots(), &[None, Some(35.0), Some(40.0)]).unwrap();
        // (0.3*35 + 0.5*40) / 0.8 = 30.5 / 0.8
        assert!((got - 38.125).abs() < 1e-9);

        // Had the absence been treated as a zero it would have been 30.5 —
        // nearly eight marks lower, for a paper the learner never sat.
        assert!(got > 30.5);
    }

    #[test]
    fn unweighted_terms_fall_back_to_the_end_of_term_paper() {
        let flat = vec![
            ExamSlot { weight: 0.0, is_final: false },
            ExamSlot { weight: 0.0, is_final: true },
        ];
        assert_eq!(term_score(&flat, &[Some(40.0), Some(70.0)]), Some(70.0));
    }

    #[test]
    fn a_single_partial_mark_still_reports() {
        let flat = vec![
            ExamSlot { weight: 0.0, is_final: false },
            ExamSlot { weight: 0.0, is_final: true },
        ];
        assert_eq!(term_score(&flat, &[Some(42.0), None]), Some(42.0));
    }

    #[test]
    fn nothing_entered_means_no_mark_not_a_zero() {
        assert_eq!(term_score(&slots(), &[None, None, None]), None);
    }

    #[test]
    fn the_gap_to_the_next_grade_is_reported_in_whole_marks() {
        let system = uneb();
        // 72/100 is C3. D2 starts at 75, so three more marks.
        let gap = gap_to_next_band(&system, 72.0, 100.0).unwrap();
        assert_eq!(gap.next_label, "D2");
        assert_eq!(gap.marks_needed, 3.0);
        assert_eq!(gap.points_gained, 1.0);
    }

    #[test]
    fn the_gap_scales_with_the_subject_maximum() {
        let system = uneb();
        // Out of 40: 28/40 is 70% (C3). D2 needs 75% = 30 marks, so +2.
        let gap = gap_to_next_band(&system, 28.0, 40.0).unwrap();
        assert_eq!(gap.next_label, "D2");
        assert_eq!(gap.marks_needed, 2.0);
    }

    #[test]
    fn a_big_jump_is_reported_honestly() {
        let system = uneb();
        // 44/100 is F9; the next band up is P8 at 45. One mark, five points.
        let gap = gap_to_next_band(&system, 44.0, 100.0).unwrap();
        assert_eq!(gap.next_label, "P8");
        assert_eq!(gap.marks_needed, 1.0);
        assert_eq!(gap.points_gained, 1.0);
    }

    #[test]
    fn the_top_band_has_nowhere_to_go() {
        assert!(gap_to_next_band(&uneb(), 95.0, 100.0).is_none());
    }
}
