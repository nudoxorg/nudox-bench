//! Nudox as it exists on `canonical`, driven through `interface-cli`.
//!
//! # Why a second Nudox adapter
//!
//! [`crate::adapter::nudox`] speaks streamable-HTTP MCP to an endpoint built
//! from `workspace/nudox-engine`. That crate no longer exists: canonical
//! replaced it with a `compiler/ interface/ server/ heart/` layout whose
//! index plane lives in `server/index/*` and whose surface is one
//! `interface.application` tool over framed JSON-RPC, or the same application
//! protocol over argv. Neither the transport nor the tool names survive, so
//! pointing the old adapter at canonical would not fail honestly — it would
//! fail at connect and look like an outage.
//!
//! This adapter speaks what canonical actually speaks, so that the number it
//! reports is a fact about canonical rather than about a missing endpoint.
//!
//! # What it reports today
//!
//! On canonical at the time of writing, every retrieval action answers:
//!
//! ```text
//! {"body":{"kind":"dependency_unavailable","capability":"index"},
//!  "terminal":{"kind":"degraded","emitted":0}}
//! ```
//!
//! `server-index-retrieval` has no consumers, `interface-core` depends on no
//! index crate, and `ApplicationService` is generic only over a compiler — it
//! has no slot to hold an index at all. So this is not a configuration that
//! can be switched on; the capability is absent by construction.
//!
//! Every attempt therefore comes back [`Attempt::unsupported`], never a zero
//! score. The distinction is the whole point: "this system cannot be asked
//! this question yet" and "this system answered badly" are different findings,
//! and a report that averages them together would publish a damning number
//! that means nothing. When the plane is wired, the same adapter starts
//! returning real attempts with no change here.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::process::Command;

use crate::adapter::{Adapter, Attempt, AttemptBuilder};
use crate::scenario::Scenario;
use crate::tokens::TokenCount;

/// The reply envelope every `interface-cli` action prints on stdout.
#[derive(Debug, Deserialize)]
struct Reply {
    body: Body,
    #[serde(default)]
    terminal: Option<Terminal>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Body {
    /// The capability needed to answer is not present in this build.
    DependencyUnavailable { capability: String },
    /// A health roll-call of every capability and its state.
    Health { facts: Vec<Fact> },
    /// The request was refused before it reached a capability.
    Rejected,
    /// Anything else: kept whole so an unrecognised success is not silently
    /// read as an absence.
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct Fact {
    capability: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct Terminal {
    #[serde(default)]
    emitted: Option<u32>,
}

/// Nudox on canonical, over the `interface-cli` application protocol.
pub struct CanonicalAdapter {
    binary: PathBuf,
    snapshot: String,
    limit: u32,
    keep_responses: bool,
    /// Capability roll-call from preflight, reprinted into every attempt so a
    /// reader never sees an `unsupported` without its cause.
    unavailable: Vec<String>,
}

impl CanonicalAdapter {
    pub fn new(binary: PathBuf, snapshot: String, limit: u32, keep_responses: bool) -> Self {
        Self {
            binary,
            snapshot,
            limit,
            keep_responses,
            unavailable: Vec::new(),
        }
    }

    /// Run one action and return `(stdout, elapsed, is_error)`.
    async fn call(&self, args: &[String]) -> (String, Duration, bool) {
        let started = Instant::now();
        let output = Command::new(&self.binary)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .await;
        let elapsed = started.elapsed();
        match output {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                if text.is_empty() {
                    let err = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                    (err, elapsed, true)
                } else {
                    (text, elapsed, !output.status.success())
                }
            }
            Err(error) => (format!("{error}"), elapsed, true),
        }
    }
}

#[async_trait::async_trait]
impl Adapter for CanonicalAdapter {
    fn name(&self) -> &str {
        "nudox-canonical"
    }

    fn description(&self) -> String {
        format!(
            "Nudox on canonical via {} (application protocol over argv)",
            self.binary.display()
        )
    }

    fn resident_cost(&self) -> TokenCount {
        // One tool, `interface.application`, dispatched by an `action`
        // string. There is no tool catalogue to carry, so the resident cost
        // of the surface is not comparable to the old MCP tool list and is
        // reported as zero rather than guessed at.
        TokenCount::default()
    }

    async fn preflight(&mut self, _scenarios: &[Scenario]) -> anyhow::Result<()> {
        let args = vec!["health".to_owned(), "1".to_owned()];
        let (text, _, _) = self.call(&args).await;
        let reply: Reply = serde_json::from_str(&text).map_err(|error| {
            anyhow::anyhow!("canonical health reply was not the expected envelope: {error}\n{text}")
        })?;
        if let Body::Health { facts } = reply.body {
            self.unavailable = facts
                .into_iter()
                .filter(|fact| fact.state != "local_ready")
                .map(|fact| format!("{}:{}", fact.capability, fact.state))
                .collect();
        }
        Ok(())
    }

    async fn attempt(&mut self, scenario: &Scenario) -> Attempt {
        let Some(query) = scenario
            .probe
            .name
            .clone()
            .or_else(|| scenario.probe.query.clone())
            .or_else(|| scenario.probe.symbol.clone())
        else {
            return Attempt::unsupported(
                self.name(),
                scenario,
                "scenario names no probe this surface can carry",
            );
        };

        let args = vec![
            "search".to_owned(),
            "1".to_owned(),
            self.snapshot.clone(),
            query.clone(),
            self.limit.to_string(),
        ];
        let rendered = args.join(" ");
        let (text, elapsed, is_error) = self.call(&args).await;

        // A reply this adapter cannot parse is an error, not an absence: the
        // surface may have moved again, and reporting that as "no results"
        // would quietly turn a broken adapter into a bad score.
        let reply: Result<Reply, _> = serde_json::from_str(&text);
        let unavailable = match &reply {
            Ok(reply) => match &reply.body {
                Body::DependencyUnavailable { capability } => Some(capability.clone()),
                Body::Health { .. } | Body::Rejected | Body::Other => None,
            },
            Err(_) => None,
        };

        if let Some(capability) = unavailable {
            let emitted = reply
                .ok()
                .and_then(|reply| reply.terminal)
                .and_then(|terminal| terminal.emitted)
                .unwrap_or(0);
            let mut why = format!(
                "canonical reports capability `{capability}` unavailable \
                 (emitted {emitted}); the index plane is built but not wired \
                 to interface-core"
            );
            if !self.unavailable.is_empty() {
                why.push_str(&format!(" — health: {}", self.unavailable.join(", ")));
            }
            return Attempt::unsupported(self.name(), scenario, why);
        }

        let mut builder =
            AttemptBuilder::new(self.name(), scenario, self.resident_cost(), self.keep_responses);
        builder.step("search", rendered, &text, elapsed, is_error);
        for note in &self.unavailable {
            builder.note_degraded(note.clone());
        }
        builder.finish(is_error.then(|| text.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unavailable_index_is_read_as_unsupported_not_empty() {
        let text = r#"{"correlation":5,"body":{"kind":"dependency_unavailable",
            "capability":"index"},"terminal":{"kind":"degraded","emitted":0},
            "diagnostic":null}"#;
        let reply: Reply = serde_json::from_str(text).expect("envelope parses");
        match reply.body {
            Body::DependencyUnavailable { capability } => assert_eq!(capability, "index"),
            other => panic!("expected an unavailable dependency, got {other:?}"),
        }
        assert_eq!(reply.terminal.and_then(|t| t.emitted), Some(0));
    }

    #[test]
    fn health_names_every_capability_that_is_not_ready() {
        let text = r#"{"correlation":1,"body":{"kind":"health","facts":[
            {"capability":"compiler_registry","state":"local_ready"},
            {"capability":"index","state":"unavailable"},
            {"capability":"graph","state":"unavailable"}]},
            "terminal":{"kind":"partial","emitted":1},"diagnostic":null}"#;
        let reply: Reply = serde_json::from_str(text).expect("envelope parses");
        let Body::Health { facts } = reply.body else {
            panic!("expected health");
        };
        let down: Vec<_> = facts
            .into_iter()
            .filter(|fact| fact.state != "local_ready")
            .map(|fact| fact.capability)
            .collect();
        assert_eq!(down, vec!["index", "graph"]);
    }

    #[test]
    fn a_successful_reply_is_not_mistaken_for_an_absence() {
        // The point of `Body::Other` — an unrecognised success must not fall
        // into the unavailable branch and be reported as a missing capability.
        let text = r#"{"correlation":5,"body":{"kind":"search","rows":[]},
            "terminal":{"kind":"complete","emitted":0},"diagnostic":null}"#;
        let reply: Reply = serde_json::from_str(text).expect("envelope parses");
        assert!(matches!(reply.body, Body::Other));
    }
}
