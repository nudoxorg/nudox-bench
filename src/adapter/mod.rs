//! The thing under test.
//!
//! An adapter answers a [`Scenario`] using one system — Nudox's MCP surface
//! under some policy, a text search, an LSP server, or later a third-party
//! MCP server. It returns the *context it produced* plus what that context
//! cost. It does not score itself.
//!
//! # Why the adapter owns the tool mapping
//!
//! context-bench keeps a `MCP_MAPPINGS` table in the harness: for each MCP
//! server, which tool to call and how to build its parameters. That works
//! when every entrant is an MCP server with one documentation-search tool.
//! It stops working the moment the comparison set includes `ripgrep` and an
//! LSP server, which have no tools at all and reach an answer in several
//! steps. So the mapping moves into the adapter: the harness hands over a
//! question, and each system answers it the way that system is actually used.

pub mod nudox;
pub mod ripgrep;
pub mod rust_analyzer;

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::scenario::Scenario;
use crate::tokens::{AttemptCost, TokenCount};

/// One observable action an adapter took.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Step {
    /// Tool name, or the command that ran.
    pub label: String,
    /// The arguments, rendered for the transcript. Counted as request cost.
    pub request: String,
    pub response_cost: TokenCount,
    pub elapsed_ms: u128,
    pub is_error: bool,
    /// Present only when the run was asked to keep full transcripts; these
    /// get large fast.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
}

/// What an adapter produced for one scenario.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attempt {
    pub adapter: String,
    pub scenario_id: String,
    pub steps: Vec<Step>,
    /// Everything the adapter surfaced, concatenated in order. This is what
    /// the scorer reads and what a judge would be shown — the agent's-eye
    /// view of what the system said.
    pub context: String,
    pub tool_calls: u32,
    pub cost: AttemptCost,
    pub elapsed_ms: u128,
    /// Set when the adapter could not attempt the scenario at all. Distinct
    /// from a zero score: "cannot express this question" and "answered
    /// wrongly" are different findings and must not be averaged together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsupported: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Capabilities the system reported as unavailable while answering.
    ///
    /// # Why this is not just a log line
    ///
    /// A benchmark result is only meaningful alongside what was switched on
    /// when it was taken. Nudox's search combines structural and semantic
    /// ranking, but the semantic half needs an embedding model, and without
    /// one the response says so inline (`~sem:unavailable(NoModelConfigured)`)
    /// and quietly returns structural hits only. Nothing errors. A reader
    /// handed "nudox scored 75%" would have no way to know half the ranking
    /// was off — and would draw exactly the wrong conclusion about what the
    /// remaining half is worth. So degradations are collected per attempt and
    /// reprinted at the top of the report.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degraded: Vec<String>,
}

impl Attempt {
    /// An attempt the adapter declined, with the reason.
    pub fn unsupported(adapter: &str, scenario: &Scenario, why: impl Into<String>) -> Self {
        Self {
            adapter: adapter.to_owned(),
            scenario_id: scenario.id.clone(),
            steps: Vec::new(),
            context: String::new(),
            tool_calls: 0,
            cost: AttemptCost::default(),
            elapsed_ms: 0,
            unsupported: Some(why.into()),
            error: None,
            degraded: Vec::new(),
        }
    }
}

/// Recognise a capability the response says was unavailable.
///
/// Nudox marks these with a `~` signal prefix — a coverage note rather than a
/// row of data. The two that change what a score means are semantic ranking
/// (`~sem:`) and unrecorded references (`~refs:not_recorded`).
pub fn degradations_in(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let is_degraded = line.starts_with("~sem:unavailable")
            || line.starts_with("~sem:building")
            || line.starts_with("~refs:not_recorded");
        if is_degraded {
            // Keep the parenthesised reason, drop any trailing remedy prose:
            // the report wants a stable label, not a paragraph, and the
            // remedy is already in the transcript.
            let label = line.split(" — ").next().unwrap_or(line).to_owned();
            if !found.contains(&label) {
                found.push(label);
            }
        }
    }
    found
}

/// Accumulates steps and their costs so every adapter reports cost the same
/// way. An adapter that hand-rolled its own accounting would be free to
/// forget the request half, which is exactly the sort of asymmetry that makes
/// a benchmark's headline number meaningless.
pub struct AttemptBuilder {
    adapter: String,
    scenario_id: String,
    steps: Vec<Step>,
    context: String,
    cost: AttemptCost,
    started: std::time::Instant,
    keep_responses: bool,
    degraded: Vec<String>,
}

impl AttemptBuilder {
    pub fn new(adapter: &str, scenario: &Scenario, resident: TokenCount, keep_responses: bool) -> Self {
        Self {
            adapter: adapter.to_owned(),
            scenario_id: scenario.id.clone(),
            steps: Vec::new(),
            context: String::new(),
            cost: AttemptCost {
                resident,
                ..Default::default()
            },
            started: std::time::Instant::now(),
            keep_responses,
            degraded: Vec::new(),
        }
    }

    /// Record one action and fold its text into the context under test.
    pub fn step(
        &mut self,
        label: impl Into<String>,
        request: impl Into<String>,
        response: &str,
        elapsed: Duration,
        is_error: bool,
    ) {
        let request = request.into();
        let response_cost = TokenCount::measure(response);
        self.cost.requests = self.cost.requests + TokenCount::measure(&request);
        self.cost.responses = self.cost.responses + response_cost;
        if !self.context.is_empty() {
            self.context.push_str("\n\n");
        }
        self.context.push_str(response);
        for degradation in degradations_in(response) {
            if !self.degraded.contains(&degradation) {
                self.degraded.push(degradation);
            }
        }
        self.steps.push(Step {
            label: label.into(),
            request,
            response_cost,
            elapsed_ms: elapsed.as_millis(),
            is_error,
            response: self.keep_responses.then(|| response.to_owned()),
        });
    }

    /// Record a degradation an adapter detected itself, rather than one read
    /// out of a response body.
    pub fn note_degraded(&mut self, note: impl Into<String>) {
        let note = note.into();
        if !self.degraded.contains(&note) {
            self.degraded.push(note);
        }
    }

    pub fn tool_calls(&self) -> u32 {
        self.steps.len() as u32
    }

    /// The context accumulated so far, so an adapter can decide its next step
    /// from what it already got back.
    pub fn context(&self) -> &str {
        &self.context
    }

    pub fn finish(self, error: Option<String>) -> Attempt {
        Attempt {
            adapter: self.adapter,
            scenario_id: self.scenario_id,
            tool_calls: self.steps.len() as u32,
            steps: self.steps,
            context: self.context,
            cost: self.cost,
            elapsed_ms: self.started.elapsed().as_millis(),
            unsupported: None,
            error,
            degraded: self.degraded,
        }
    }
}

/// A system that can answer scenarios.
#[async_trait::async_trait]
pub trait Adapter: Send {
    /// Stable name; becomes a report column, so it must not drift.
    fn name(&self) -> &str;

    /// One line for the report header describing what this adapter is.
    fn description(&self) -> String;

    /// The fixed context cost of this system's surface, before any question
    /// is asked. Zero for adapters that are not tool surfaces.
    fn resident_cost(&self) -> TokenCount {
        TokenCount::default()
    }

    /// Prepare for a run, given every scenario it will be asked.
    ///
    /// # Why this hook exists
    ///
    /// The file-based adapters read `target.root` — a checkout on this
    /// machine. Nudox answers from its own corpus. If those two hold
    /// *different versions of the same package*, the comparison is void: the
    /// adapters are being asked the same question about different code, and
    /// any difference in their answers measures the skew rather than the
    /// tools.
    ///
    /// Nudox's `index` tool accepts a filesystem path as a target, so the
    /// skew is closable rather than merely documentable: the Nudox adapter
    /// indexes the very roots the other adapters read. Returning `Ok` without
    /// doing anything is correct for adapters that have no corpus.
    async fn preflight(&mut self, _scenarios: &[Scenario]) -> anyhow::Result<()> {
        Ok(())
    }

    /// Answer one scenario.
    async fn attempt(&mut self, scenario: &Scenario) -> Attempt;

    /// Release anything long-lived (child processes, sessions).
    async fn shutdown(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unavailable_embedder_is_caught() {
        let response = "## Search · 0\n\
            ~sem:unavailable(NoModelConfigured) — the embedding runtime is always \
            compiled in, so no rebuild is possible or needed — set NUDOX_EMBED_MODEL_DIR\n\
            (no matches)";
        let found = degradations_in(response);
        assert_eq!(found, vec!["~sem:unavailable(NoModelConfigured)"]);
    }

    #[test]
    fn a_healthy_response_reports_nothing() {
        // `~sem:ready` and `~scope:default` are normal operation, not
        // degradation — flagging them would make the warning meaningless.
        let response = "## Search · 3\n~sem:ready\n~scope:default (excluded Field)\nx";
        assert!(degradations_in(response).is_empty());
    }

    #[test]
    fn a_partially_built_index_counts() {
        assert_eq!(
            degradations_in("~sem:building(12/2129)"),
            vec!["~sem:building(12/2129)"]
        );
    }
}
