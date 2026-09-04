//! Results, aggregation, and the two renderings of them.
//!
//! # The aggregate this refuses to compute
//!
//! There is no single "score" column. Four things are measured — did it find
//! the fact, how *well-resolved* was the evidence, what did it cost, and
//! could it answer at all — and they trade against each other. Collapsing
//! them into one number would let a wide, expensive, unresolved answer
//! outrank a narrow correct one, which is the exact failure mode
//! `truth.none_of` and the strict/loose split exist to expose. So the
//! summary is a table, and the reader does the weighing.
//!
//! `unsupported` is likewise never folded into the pass rate. "This tool
//! cannot express this question" and "this tool got it wrong" are different
//! findings; averaging them together would make rust-analyzer look bad at Go
//! rather than inapplicable to it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::adapter::Attempt;
use crate::judge::JudgeVerdict;
use crate::scenario::{Family, Scenario};
use crate::tokens::TokenCount;
use crate::truth::DeterministicScore;

/// One adapter's answer to one scenario, scored.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Outcome {
    pub scenario_id: String,
    pub family: Family,
    pub adapter: String,
    pub score: DeterministicScore,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge: Option<JudgeVerdict>,
    pub attempt: Attempt,
}

impl Outcome {
    /// Whether this counts as a pass. Judged scenarios defer to the judge;
    /// matched ones to the deterministic scorer.
    pub fn passed(&self) -> Option<bool> {
        if self.attempt.unsupported.is_some() {
            return None;
        }
        match &self.judge {
            Some(verdict) => Some(verdict.passed),
            None if self.score.judged => None,
            None => Some(self.score.passed),
        }
    }
}

/// What an adapter is, recorded once per run so a report is readable months
/// later without the config that produced it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdapterInfo {
    pub name: String,
    pub description: String,
    pub resident: TokenCount,
}

/// A whole run.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunReport {
    pub run_id: String,
    pub timestamp: String,
    pub adapters: Vec<AdapterInfo>,
    pub outcomes: Vec<Outcome>,
}

/// Per-adapter aggregate.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AdapterSummary {
    pub adapter: String,
    pub attempted: usize,
    pub passed: usize,
    pub unsupported: usize,
    /// Scenarios that ran but cannot be scored — a judged scenario on a run
    /// with no judge. Kept out of `attempted` so an unjudged implementation
    /// scenario does not silently read as a failure.
    pub unscored: usize,
    pub errored: usize,
    pub mean_strict_recall: f64,
    /// Strict recall counting only evidence the adapter matched, not text it
    /// printed around a match. Equal to `mean_strict_recall` for adapters
    /// with no incidental output, which is why the column is never blank.
    pub mean_primary_strict_recall: f64,
    pub mean_loose_recall: f64,
    /// Mean of `loose - strict`: how much of the apparent success is
    /// unresolved text rather than resolved symbols.
    pub mean_resolution_gap: f64,
    pub total_distractors: usize,
    /// Of `total_distractors`, how many were in an adapter's *answer* rather
    /// than in text it surfaced while finding one. Equal to
    /// `total_distractors` for an adapter that cannot tell the two apart.
    pub answer_distractors: usize,
    /// Response + request tokens, summed over attempted scenarios.
    pub marginal_tokens: TokenCount,
    /// The fixed surface cost, paid once per session.
    pub resident_tokens: TokenCount,
    pub mean_tool_calls: f64,
    pub total_elapsed_ms: u128,
}

impl AdapterSummary {
    /// Pass rate over scenarios the adapter could actually attempt.
    pub fn pass_rate(&self) -> f64 {
        if self.attempted == 0 {
            0.0
        } else {
            self.passed as f64 / self.attempted as f64
        }
    }

    /// Marginal tokens per scenario attempted — the number to compare
    /// adapters on, since the resident cost is paid once per session however
    /// many questions follow.
    pub fn tokens_per_scenario(&self) -> f64 {
        if self.attempted == 0 {
            0.0
        } else {
            self.marginal_tokens.conservative() as f64 / self.attempted as f64
        }
    }

    /// Tokens spent per scenario actually passed. The efficiency figure that
    /// cannot be gamed by answering cheaply and wrongly: an adapter that
    /// passes nothing has no finite cost-per-answer at all.
    pub fn tokens_per_pass(&self) -> Option<f64> {
        (self.passed > 0)
            .then(|| self.marginal_tokens.conservative() as f64 / self.passed as f64)
    }
}

impl RunReport {
    /// Aggregate per adapter, in the order the adapters were declared.
    pub fn summarize(&self) -> Vec<AdapterSummary> {
        let mut by_adapter: BTreeMap<&str, AdapterSummary> = BTreeMap::new();
        for info in &self.adapters {
            by_adapter.insert(
                info.name.as_str(),
                AdapterSummary {
                    adapter: info.name.clone(),
                    resident_tokens: info.resident,
                    ..Default::default()
                },
            );
        }

        for outcome in &self.outcomes {
            let entry = by_adapter
                .entry(outcome.adapter.as_str())
                .or_insert_with(|| AdapterSummary {
                    adapter: outcome.adapter.clone(),
                    ..Default::default()
                });

            if outcome.attempt.unsupported.is_some() {
                entry.unsupported += 1;
                continue;
            }
            if outcome.passed().is_none() {
                // Judged, with no verdict: the run had no judge. Its recall
                // numbers are structurally zero and would poison the means.
                entry.unscored += 1;
                continue;
            }
            entry.attempted += 1;
            if outcome.attempt.error.is_some() {
                entry.errored += 1;
            }
            if outcome.passed() == Some(true) {
                entry.passed += 1;
            }
            entry.mean_strict_recall += outcome.score.strict_recall;
            // An adapter that never reported a primary subset has no
            // incidental output, so all of its strict recall is primary.
            entry.mean_primary_strict_recall += outcome
                .score
                .primary_strict_recall
                .unwrap_or(outcome.score.strict_recall);
            entry.mean_loose_recall += outcome.score.loose_recall;
            entry.mean_resolution_gap += outcome.score.resolution_gap();
            entry.total_distractors += outcome.score.distractors;
            entry.answer_distractors += outcome
                .score
                .primary_distractors
                .unwrap_or(outcome.score.distractors);
            entry.marginal_tokens = entry.marginal_tokens + outcome.attempt.cost.marginal();
            entry.mean_tool_calls += outcome.attempt.tool_calls as f64;
            entry.total_elapsed_ms += outcome.attempt.elapsed_ms;
        }

        let mut summaries: Vec<AdapterSummary> = by_adapter.into_values().collect();
        for summary in &mut summaries {
            let n = summary.attempted.max(1) as f64;
            summary.mean_strict_recall /= n;
            summary.mean_primary_strict_recall /= n;
            summary.mean_loose_recall /= n;
            summary.mean_resolution_gap /= n;
            summary.mean_tool_calls /= n;
        }
        // Report order follows declaration order, not the map's.
        let declared: Vec<&str> = self.adapters.iter().map(|a| a.name.as_str()).collect();
        summaries.sort_by_key(|s| {
            declared
                .iter()
                .position(|d| *d == s.adapter)
                .unwrap_or(usize::MAX)
        });
        summaries
    }

    /// The Markdown report.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# nudox-bench run `{}`\n\n", self.run_id));
        out.push_str(&format!("{}\n\n", self.timestamp));

        let mut degraded: Vec<(String, String)> = Vec::new();
        for outcome in &self.outcomes {
            for note in &outcome.attempt.degraded {
                let entry = (outcome.adapter.clone(), note.clone());
                if !degraded.contains(&entry) {
                    degraded.push(entry);
                }
            }
        }
        if !degraded.is_empty() {
            out.push_str("## ⚠ Degraded capabilities during this run\n\n");
            out.push_str(
                "A system reported part of itself unavailable while answering. \
                 Every number below was measured in that state — read them as a \
                 result about the configuration that ran, not about the system at \
                 full capability.\n\n",
            );
            for (adapter, note) in &degraded {
                out.push_str(&format!("- **{adapter}** — `{note}`\n"));
            }
            out.push('\n');
        }

        out.push_str("## Surface cost before any question\n\n");
        out.push_str(
            "What each system puts in an agent's context before it is asked anything: \
             tool descriptions plus every argument-schema description. Paid once per \
             session.\n\n",
        );
        out.push_str("| adapter | cl100k | o200k | bytes |\n|---|---:|---:|---:|\n");
        for info in &self.adapters {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                info.name, info.resident.cl100k, info.resident.o200k, info.resident.bytes
            ));
        }

        let summaries = self.summarize();
        out.push_str("\n## Results\n\n");
        out.push_str(
            "`strict` counts a fact found as a fully-qualified path; `loose` counts it \
             found as a bare identifier anywhere in the returned text. The gap between \
             them is how much of an adapter's apparent success is unresolved text.\n\n\
             `matched` counts only facts on a line the adapter actually matched, not \
             ones it printed in the window around a match. Where it falls below \
             `strict`, that much of the recall was surrounded rather than found. It \
             equals `strict` for adapters that return only what they were asked for.\n\n\
             `errored` counts attempts where the tool itself failed — a crash, a \
             dropped connection, a timeout. They stay inside `attempted` because a \
             tool that cannot answer did not answer, but they are shown separately: \
             a column of errors and a column of wrong answers look identical in a \
             pass rate and mean entirely different things.\n\n",
        );
        out.push_str(
            "| adapter | pass | attempted | unsup. | unscored | errored | strict | matched | loose | gap | \
             distractors | tok/scenario | tok/pass | calls |\n\
             |---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
        );
        for s in &summaries {
            out.push_str(&format!(
                "| {} | {}/{} ({:.0}%) | {} | {} | {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {} | {:.0} | {} | {:.1} |\n",
                s.adapter,
                s.passed,
                s.attempted,
                s.pass_rate() * 100.0,
                s.attempted,
                s.unsupported,
                s.unscored,
                s.errored,
                s.mean_strict_recall,
                s.mean_primary_strict_recall,
                s.mean_loose_recall,
                s.mean_resolution_gap,
                if s.answer_distractors == s.total_distractors {
                    format!("{}", s.total_distractors)
                } else {
                    // A distractor the answer never contained is a different
                    // finding from one it did, and collapsing them hides it.
                    format!("{}→{}", s.total_distractors, s.answer_distractors)
                },
                s.tokens_per_scenario(),
                s.tokens_per_pass()
                    .map(|v| format!("{v:.0}"))
                    .unwrap_or_else(|| "—".into()),
                s.mean_tool_calls,
            ));
        }

        out.push_str("\n## Per scenario\n\n");
        let mut scenarios: Vec<&str> = self
            .outcomes
            .iter()
            .map(|o| o.scenario_id.as_str())
            .collect();
        scenarios.sort_unstable();
        scenarios.dedup();
        let adapters: Vec<&str> = self.adapters.iter().map(|a| a.name.as_str()).collect();

        out.push_str("| scenario | family |");
        for adapter in &adapters {
            out.push_str(&format!(" {adapter} |"));
        }
        out.push_str("\n|---|---|");
        for _ in &adapters {
            out.push_str("---|");
        }
        out.push('\n');

        for scenario in scenarios {
            let family = self
                .outcomes
                .iter()
                .find(|o| o.scenario_id == scenario)
                .map(|o| o.family.label())
                .unwrap_or("");
            out.push_str(&format!("| `{scenario}` | {family} |"));
            for adapter in &adapters {
                let cell = self
                    .outcomes
                    .iter()
                    .find(|o| o.scenario_id == scenario && o.adapter == *adapter)
                    .map(|o| match o.passed() {
                        None if o.attempt.unsupported.is_some() => "n/a".to_owned(),
                        None => "unscored".to_owned(),
                        Some(true) => format!("pass ({})", o.attempt.cost.marginal().cl100k),
                        Some(false) => format!(
                            "fail {:.0}% ({})",
                            o.score.strict_recall * 100.0,
                            o.attempt.cost.marginal().cl100k
                        ),
                    })
                    .unwrap_or_else(|| "—".into());
                out.push_str(&format!(" {cell} |"));
            }
            out.push('\n');
        }

        let unsupported: Vec<&Outcome> = self
            .outcomes
            .iter()
            .filter(|o| o.attempt.unsupported.is_some())
            .collect();
        if !unsupported.is_empty() {
            out.push_str("\n## Declined\n\n");
            out.push_str(
                "Scenarios an adapter could not express. Not failures — these are the \
                 shape of each tool's reach.\n\n",
            );
            for outcome in unsupported {
                out.push_str(&format!(
                    "- `{}` / **{}** — {}\n",
                    outcome.scenario_id,
                    outcome.adapter,
                    outcome.attempt.unsupported.as_deref().unwrap_or("")
                ));
            }
        }

        out
    }
}

/// Build a `Outcome` from a scored attempt.
pub fn outcome(
    scenario: &Scenario,
    attempt: Attempt,
    score: DeterministicScore,
    judge: Option<JudgeVerdict>,
) -> Outcome {
    Outcome {
        scenario_id: scenario.id.clone(),
        family: scenario.family,
        adapter: attempt.adapter.clone(),
        score,
        judge,
        attempt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::Attempt;
    use crate::tokens::AttemptCost;

    fn attempt(adapter: &str, unsupported: bool, tokens: usize) -> Attempt {
        Attempt {
            adapter: adapter.into(),
            scenario_id: "s".into(),
            steps: vec![],
            context: String::new(),
            primary_context: None,
            tool_calls: 1,
            cost: AttemptCost {
                responses: TokenCount {
                    bytes: tokens * 4,
                    cl100k: tokens,
                    o200k: tokens,
                },
                ..Default::default()
            },
            elapsed_ms: 10,
            unsupported: unsupported.then(|| "nope".to_owned()),
            error: None,
            degraded: Vec::new(),
        }
    }

    fn outcome_for(adapter: &str, passed: bool, unsupported: bool, tokens: usize) -> Outcome {
        Outcome {
            scenario_id: "s".into(),
            family: Family::Retrieval,
            adapter: adapter.into(),
            score: DeterministicScore {
                passed,
                strict_recall: if passed { 1.0 } else { 0.0 },
                loose_recall: 1.0,
                ..Default::default()
            },
            judge: None,
            attempt: attempt(adapter, unsupported, tokens),
        }
    }

    /// A distractor the answer never contained still fails the scenario, but
    /// the table has to say so, or the reader cannot tell "the tool offered a
    /// near-miss" from "the tool found one while looking".
    #[test]
    fn distractor_provenance_reaches_the_table() {
        let mut only_while_looking = outcome_for("a", false, false, 100);
        only_while_looking.score.distractors = 1;
        only_while_looking.score.primary_distractors = Some(0);

        let mut in_the_answer = outcome_for("b", false, false, 100);
        in_the_answer.score.distractors = 1;
        in_the_answer.score.primary_distractors = Some(1);

        let report = RunReport {
            run_id: "r".into(),
            timestamp: "t".into(),
            adapters: vec![
                AdapterInfo { name: "a".into(), description: String::new(), resident: TokenCount::default() },
                AdapterInfo { name: "b".into(), description: String::new(), resident: TokenCount::default() },
            ],
            outcomes: vec![only_while_looking, in_the_answer],
        };

        let summaries = report.summarize();
        let a = summaries.iter().find(|s| s.adapter == "a").unwrap();
        assert_eq!((a.total_distractors, a.answer_distractors), (1, 0));
        let b = summaries.iter().find(|s| s.adapter == "b").unwrap();
        assert_eq!((b.total_distractors, b.answer_distractors), (1, 1));

        let markdown = report.to_markdown();
        assert!(markdown.contains("1→0"), "provenance must be visible: {markdown}");
        assert!(
            markdown.contains("| 1 |"),
            "and a distractor genuinely in the answer stays a plain count"
        );
    }

    #[test]
    fn unsupported_never_counts_against_the_pass_rate() {
        let report = RunReport {
            run_id: "r".into(),
            timestamp: "t".into(),
            adapters: vec![AdapterInfo {
                name: "a".into(),
                description: String::new(),
                resident: TokenCount::default(),
            }],
            outcomes: vec![
                outcome_for("a", true, false, 100),
                outcome_for("a", false, true, 0),
            ],
        };
        let summary = &report.summarize()[0];
        assert_eq!(summary.attempted, 1);
        assert_eq!(summary.unsupported, 1);
        assert_eq!(summary.pass_rate(), 1.0, "declining is not failing");
    }

    #[test]
    fn an_unjudged_scenario_is_not_a_failure() {
        let mut judged = outcome_for("a", false, false, 900);
        judged.family = Family::Implementation;
        judged.score.judged = true;
        judged.score.strict_recall = 0.0;
        let report = RunReport {
            run_id: "r".into(),
            timestamp: "t".into(),
            adapters: vec![AdapterInfo {
                name: "a".into(),
                description: String::new(),
                resident: TokenCount::default(),
            }],
            outcomes: vec![outcome_for("a", true, false, 100), judged],
        };
        let summary = &report.summarize()[0];
        assert_eq!(summary.attempted, 1, "a judged scenario with no judge is not attempted");
        assert_eq!(summary.unscored, 1);
        assert_eq!(summary.pass_rate(), 1.0);
        assert_eq!(
            summary.mean_strict_recall, 1.0,
            "a structurally-zero recall must not enter the mean"
        );
    }

    #[test]
    fn a_tool_that_broke_is_visible_next_to_one_that_was_wrong() {
        // Both of these are a failed scenario in the pass rate. Only one of
        // them says anything about the tool's ability to answer, and a report
        // that cannot tell them apart sends a reader chasing a capability gap
        // that was really a dropped connection.
        let mut broke = outcome_for("a", false, false, 100);
        broke.attempt.error = Some("connection reset".into());
        let report = RunReport {
            run_id: "r".into(),
            timestamp: "t".into(),
            adapters: vec![AdapterInfo {
                name: "a".into(),
                description: String::new(),
                resident: TokenCount::default(),
            }],
            outcomes: vec![outcome_for("a", false, false, 100), broke],
        };
        let summary = &report.summarize()[0];
        assert_eq!(summary.attempted, 2, "a broken attempt still tried");
        assert_eq!(summary.errored, 1);
        assert_eq!(summary.passed, 0);
        // And it has to reach the reader, not just the struct.
        let rendered = report.to_markdown();
        assert!(rendered.contains("errored"), "the column is missing from the table");
    }

    #[test]
    fn tokens_per_pass_is_undefined_when_nothing_passes() {
        let report = RunReport {
            run_id: "r".into(),
            timestamp: "t".into(),
            adapters: vec![AdapterInfo {
                name: "a".into(),
                description: String::new(),
                resident: TokenCount::default(),
            }],
            outcomes: vec![outcome_for("a", false, false, 5_000)],
        };
        let summary = &report.summarize()[0];
        assert_eq!(summary.tokens_per_pass(), None, "cheap and wrong is not efficient");
    }

    #[test]
    fn markdown_renders_every_declared_adapter() {
        let report = RunReport {
            run_id: "r".into(),
            timestamp: "t".into(),
            adapters: vec![
                AdapterInfo { name: "a".into(), description: String::new(), resident: TokenCount::default() },
                AdapterInfo { name: "b".into(), description: String::new(), resident: TokenCount::default() },
            ],
            outcomes: vec![outcome_for("a", true, false, 10)],
        };
        let markdown = report.to_markdown();
        assert!(markdown.contains("| a |"));
        assert!(markdown.contains("| b |"), "an adapter with no outcomes still gets a row");
    }
}
