//! Does this benchmark actually reproduce?
//!
//! # Why this is not covered by the other numbers
//!
//! Every claim this repo makes rests on one sentence in `docs/DESIGN.md`:
//! the scoring is deterministic, so anyone can re-run these and get the same
//! result. Nothing measured that. The reports say what happened *once*, and a
//! verdict that quietly alternates between runs would look exactly like a
//! verdict that holds — it would just be whichever value the published run
//! happened to land on.
//!
//! So this compares repeated runs of one config and reports what moved. The
//! two kinds of movement are not equally serious and are never merged:
//!
//! - a **verdict** that changes is an alarm. It means a published pass rate
//!   is partly luck, and no amount of averaging fixes that.
//! - a **cost** that changes is ordinary for anything crossing a process or a
//!   network, and is reported as a spread rather than a fault.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::report::RunReport;

/// What one scenario did on one adapter, reduced to the facts that must not
/// drift between runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Verdict {
    Pass,
    Fail,
    /// The adapter declined the scenario.
    Unsupported,
    /// Ran, but nothing could score it — a judged scenario with no judge.
    Unscored,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Unsupported => "unsupported",
            Self::Unscored => "unscored",
        }
    }
}

/// One (adapter, scenario) cell observed across every repeat.
#[derive(Clone, Debug)]
pub struct Cell {
    pub adapter: String,
    pub scenario: String,
    pub verdicts: Vec<Verdict>,
    /// Strict recall, rounded to the precision the report prints. Comparing
    /// raw floats would flag differences no reader could ever see.
    pub strict: Vec<i64>,
    pub tokens: Vec<usize>,
}

impl Cell {
    /// Every run agreed on what happened.
    pub fn verdict_stable(&self) -> bool {
        self.verdicts.windows(2).all(|pair| pair[0] == pair[1])
    }

    /// Every run agreed on the recall that produced it.
    pub fn score_stable(&self) -> bool {
        self.strict.windows(2).all(|pair| pair[0] == pair[1])
    }

    /// Spread of the token cost, as (low, high).
    pub fn token_span(&self) -> (usize, usize) {
        let low = self.tokens.iter().copied().min().unwrap_or(0);
        let high = self.tokens.iter().copied().max().unwrap_or(0);
        (low, high)
    }

    /// How far the cost moved, as a fraction of the smallest observation.
    ///
    /// Relative rather than absolute so a 200-token wobble on a 20,000-token
    /// answer does not read like the same event as one on a 300-token answer.
    pub fn token_drift(&self) -> f64 {
        let (low, high) = self.token_span();
        if low == 0 {
            return if high == 0 { 0.0 } else { 1.0 };
        }
        (high - low) as f64 / low as f64
    }
}

/// What repeated runs of one config showed.
#[derive(Clone, Debug)]
pub struct Stability {
    pub repeats: usize,
    pub cells: Vec<Cell>,
}

impl Stability {
    /// Compare runs of the same config, in the order they were taken.
    ///
    /// A cell missing from some run is kept with the observations it has;
    /// dropping it would hide the most interesting failure of all, which is a
    /// scenario that only sometimes appears.
    pub fn compare(runs: &[RunReport]) -> Self {
        let mut cells: BTreeMap<(String, String), Cell> = BTreeMap::new();
        for run in runs {
            for outcome in &run.outcomes {
                let key = (outcome.adapter.clone(), outcome.scenario_id.clone());
                let cell = cells.entry(key).or_insert_with(|| Cell {
                    adapter: outcome.adapter.clone(),
                    scenario: outcome.scenario_id.clone(),
                    verdicts: Vec::new(),
                    strict: Vec::new(),
                    tokens: Vec::new(),
                });
                let verdict = if outcome.attempt.unsupported.is_some() {
                    Verdict::Unsupported
                } else {
                    match outcome.passed() {
                        Some(true) => Verdict::Pass,
                        Some(false) => Verdict::Fail,
                        None => Verdict::Unscored,
                    }
                };
                cell.verdicts.push(verdict);
                // Two decimals: what the report prints, so a difference
                // flagged here is one a reader could actually see.
                cell.strict.push((outcome.score.strict_recall * 100.0).round() as i64);
                // Marginal, not total: the fixed surface cost is constant by
                // construction and would mask drift in what a question cost.
                cell.tokens.push(outcome.attempt.cost.marginal().conservative());
            }
        }
        Self {
            repeats: runs.len(),
            cells: cells.into_values().collect(),
        }
    }

    /// Cells whose verdict moved. These are the alarming ones.
    pub fn unstable_verdicts(&self) -> Vec<&Cell> {
        self.cells
            .iter()
            .filter(|cell| !cell.verdict_stable())
            .collect()
    }

    /// Cells whose verdict held but whose recall moved.
    pub fn unstable_scores(&self) -> Vec<&Cell> {
        self.cells
            .iter()
            .filter(|cell| cell.verdict_stable() && !cell.score_stable())
            .collect()
    }

    /// Cells whose cost moved by more than `threshold` of its own floor.
    pub fn drifting_costs(&self, threshold: f64) -> Vec<&Cell> {
        let mut found: Vec<&Cell> = self
            .cells
            .iter()
            .filter(|cell| cell.token_drift() > threshold)
            .collect();
        found.sort_by(|a, b| b.token_drift().total_cmp(&a.token_drift()));
        found
    }

    /// The share of cells that reproduced exactly, verdict and recall alike.
    pub fn reproducibility(&self) -> f64 {
        if self.cells.is_empty() {
            return 1.0;
        }
        let solid = self
            .cells
            .iter()
            .filter(|cell| cell.verdict_stable() && cell.score_stable())
            .count();
        solid as f64 / self.cells.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(verdicts: &[Verdict], strict: &[i64], tokens: &[usize]) -> Cell {
        Cell {
            adapter: "a".into(),
            scenario: "s".into(),
            verdicts: verdicts.to_vec(),
            strict: strict.to_vec(),
            tokens: tokens.to_vec(),
        }
    }

    #[test]
    fn a_verdict_that_holds_is_stable() {
        let c = cell(&[Verdict::Pass, Verdict::Pass], &[100, 100], &[10, 10]);
        assert!(c.verdict_stable() && c.score_stable());
    }

    #[test]
    fn a_verdict_that_alternates_is_caught() {
        // The case the whole command exists for: a published pass that is
        // really a coin flip.
        let c = cell(&[Verdict::Pass, Verdict::Fail, Verdict::Pass], &[100, 50, 100], &[10, 10, 10]);
        assert!(!c.verdict_stable());
    }

    #[test]
    fn cost_drift_is_relative_to_the_smaller_reading() {
        let small = cell(&[Verdict::Pass], &[100], &[300, 500]);
        let large = cell(&[Verdict::Pass], &[100], &[20_000, 20_200]);
        assert!(small.token_drift() > large.token_drift());
        assert!((small.token_drift() - 0.666).abs() < 0.01);
    }

    #[test]
    fn a_single_run_is_trivially_reproducible_and_says_so() {
        // One run cannot disagree with itself. The number must not be read as
        // evidence of anything, which is why the command insists on >= 2.
        let s = Stability { repeats: 1, cells: vec![cell(&[Verdict::Pass], &[100], &[10])] };
        assert!((s.reproducibility() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn reproducibility_counts_recall_drift_as_a_failure_to_reproduce() {
        // Same verdict, different recall: the verdict survived by luck of
        // where the threshold sits, and that is not reproduction.
        let s = Stability {
            repeats: 2,
            cells: vec![
                cell(&[Verdict::Pass, Verdict::Pass], &[100, 100], &[10, 10]),
                cell(&[Verdict::Pass, Verdict::Pass], &[100, 67], &[10, 10]),
            ],
        };
        assert!((s.reproducibility() - 0.5).abs() < f64::EPSILON);
        assert_eq!(s.unstable_scores().len(), 1);
        assert!(s.unstable_verdicts().is_empty());
    }

    #[test]
    fn an_empty_comparison_does_not_divide_by_zero() {
        let s = Stability { repeats: 3, cells: Vec::new() };
        assert!((s.reproducibility() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_cell_absent_from_one_run_keeps_the_observations_it_has() {
        let mut c = cell(&[Verdict::Pass, Verdict::Pass], &[100, 100], &[10, 10]);
        c.verdicts.pop();
        assert!(c.verdict_stable(), "one observation cannot disagree");
        assert_eq!(c.verdicts.len(), 1);
    }
}
