//! The judge, for the scenarios that cannot be matched.
//!
//! # Scope, deliberately narrow
//!
//! Only [`Family::Implementation`] reaches a judge. Everything else has a
//! recorded answer and is scored mechanically, which is the property that
//! makes most of this benchmark re-runnable at no cost, with no API key, and
//! with no run-to-run variance to explain away.
//!
//! # The protocol, and why it is context-bench's
//!
//! Three models vote independently on two booleans — is the returned context
//! *complete* enough to implement the oracle, and is it *relevant* to the
//! task — and a scenario passes only if both carry 2-of-3. That is the
//! protocol `context-bench` used to produce the numbers this harness is meant
//! to be readable against, so it is reproduced rather than improved on:
//! changing the judging rule and the system under test at the same time would
//! make the two sets of numbers incomparable, which is the whole reason for
//! keeping it.
//!
//! What is judged is the *context*, not an implementation. No code is
//! generated or run. The question is whether a competent developer handed
//! this context could write the oracle — which is what a documentation-
//! retrieval tool is actually responsible for.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::scenario::Scenario;

/// The three voters. Kept as a constant rather than a config knob: a judge
/// panel that changes between runs makes the runs incomparable, which is the
/// one thing a benchmark may not do quietly.
pub const PANEL: [&str; 3] = [
    "openai/gpt-5",
    "x-ai/grok-4",
    "deepseek/deepseek-v3.2-exp",
];

/// One model's vote.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vote {
    pub model: String,
    pub complete: bool,
    pub relevant: bool,
    /// 1–5 quality score, recorded but not part of the pass rule.
    #[serde(default)]
    pub score: u8,
    #[serde(default)]
    pub reasoning: String,
}

/// The panel's verdict.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JudgeVerdict {
    pub votes: Vec<Vote>,
    pub complete: bool,
    pub relevant: bool,
    pub passed: bool,
}

impl JudgeVerdict {
    /// A strict majority of the votes cast, on both axes.
    ///
    /// # Why not `div_ceil`
    ///
    /// `div_ceil(2)` is a majority only for an odd panel. For two votes it
    /// returns one, so a split panel — one model says yes, the other no —
    /// resolved to a pass. Two-model panels are not hypothetical: a model
    /// that fails to answer is dropped and the remaining two decide.
    ///
    /// Resolving a tie in favour of passing is the one place this repo would
    /// have flattered a result. Everything else here refuses to: declines are
    /// never averaged with failures, and token counts take the larger of two
    /// vocabularies so a compactness claim cannot be flattered by tokenizer
    /// choice. `len / 2 + 1` is a strict majority at every panel size — 2 of
    /// 3, 2 of 2, 3 of 4 — which is what the comment on the panel loop and
    /// the name of the test below both already claimed.
    pub fn tally(votes: Vec<Vote>) -> Self {
        let needed = votes.len() / 2 + 1;
        let complete = votes.iter().filter(|v| v.complete).count() >= needed;
        let relevant = votes.iter().filter(|v| v.relevant).count() >= needed;
        Self {
            complete,
            relevant,
            passed: complete && relevant,
            votes,
        }
    }
}

/// An OpenRouter-backed judge.
pub struct Judge {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl Judge {
    /// Build a judge from `OPENROUTER_API_KEY`, or `None` if it is unset.
    ///
    /// Absence is not an error: a run with no key scores every matched
    /// scenario as usual and reports the judged ones as not-run, which is a
    /// perfectly useful run.
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("OPENROUTER_API_KEY").ok()?;
        if api_key.trim().is_empty() {
            return None;
        }
        Some(Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: std::env::var("OPENROUTER_BASE_URL")
                .unwrap_or_else(|_| "https://openrouter.ai/api/v1".into()),
        })
    }

    /// Judge one attempt's context against the scenario's oracle.
    pub async fn judge(&self, scenario: &Scenario, context: &str) -> Result<JudgeVerdict> {
        let Some(oracle_path) = scenario.truth.oracle.as_ref() else {
            bail!("{}: judged scenario has no oracle", scenario.id);
        };
        let oracle = std::fs::read_to_string(oracle_path)
            .with_context(|| format!("reading oracle {}", oracle_path.display()))?;
        let prompt = build_prompt(scenario, &oracle, context);

        let mut votes = Vec::new();
        for model in PANEL {
            match self.ask(model, &prompt).await {
                Ok(vote) => votes.push(vote),
                // One unreachable model must not void the panel; a 2-of-2 is
                // still a majority and is recorded as such.
                Err(e) => tracing::warn!("judge {model} failed: {e}"),
            }
        }
        if votes.is_empty() {
            bail!("every judge model failed");
        }
        Ok(JudgeVerdict::tally(votes))
    }

    async fn ask(&self, model: &str, prompt: &str) -> Result<Vote> {
        let body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": prompt }
            ],
            "response_format": { "type": "json_object" },
            "temperature": 0
        });

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            bail!("{model}: HTTP {status}: {text}");
        }
        let value: Value = serde_json::from_str(&text)?;
        let content = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        parse_vote(model, content)
    }
}

const SYSTEM_PROMPT: &str = "\
You evaluate whether a block of retrieved context is sufficient for a developer to \
implement a described task. You never write the implementation. Answer only with a \
JSON object of the form \
{\"complete\": bool, \"relevant\": bool, \"score\": 1-5, \"reasoning\": \"one or two sentences\"}.";

fn build_prompt(scenario: &Scenario, oracle: &str, context: &str) -> String {
    format!(
        "# Task the developer was given\n\n{}\n\n\
         # Oracle implementation (what a correct answer looks like)\n\n```\n{}\n```\n\n\
         # Context that was retrieved for them\n\n```\n{}\n```\n\n\
         # What to decide\n\n\
         `complete`: could a competent developer write something functionally \
         equivalent to the oracle from the retrieved context alone? API names, \
         parameter types, return values and usage patterns must be inferable. \
         Exact textual match with the oracle is NOT required — functional \
         equivalence is enough.\n\
         `relevant`: does the retrieved context address this task, rather than a \
         neighbouring one?\n\
         `score`: overall quality of the context, 1 to 5.",
        scenario.question,
        oracle.trim(),
        context.trim()
    )
}

/// Parse a vote out of a model's reply, tolerating a fenced JSON block.
fn parse_vote(model: &str, content: &str) -> Result<Vote> {
    let trimmed = content.trim();
    // The braces are located independently, so a reply whose last `}` comes
    // *before* its first `{` — a model explaining itself in prose rather than
    // answering — inverts the range and panics on the slice. Measured:
    // "the object ends with } and starts with {" gives 39..22.
    //
    // Falling back to the whole reply is right: it fails to parse, `ask`
    // returns an error, that model drops out of the panel, and the run
    // continues on the models that did answer. One chatty judge must not take
    // down a benchmark run.
    let json_text = match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if end + 1 > start => &trimmed[start..end + 1],
        (Some(start), None) => &trimmed[start..],
        _ => trimmed,
    };
    let value: Value = serde_json::from_str(json_text)
        .with_context(|| format!("{model} did not answer with JSON: {trimmed}"))?;
    Ok(Vote {
        model: model.to_owned(),
        complete: value
            .get("complete")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        relevant: value
            .get("relevant")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        score: value.get("score").and_then(Value::as_u64).unwrap_or(0) as u8,
        reasoning: value
            .get("reasoning")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vote(complete: bool, relevant: bool) -> Vote {
        Vote {
            model: "m".into(),
            complete,
            relevant,
            score: 3,
            reasoning: String::new(),
        }
    }

    #[test]
    fn two_of_three_carries_both_axes() {
        let verdict = JudgeVerdict::tally(vec![
            vote(true, true),
            vote(true, false),
            vote(false, true),
        ]);
        assert!(verdict.complete);
        assert!(verdict.relevant);
        assert!(verdict.passed);
    }

    #[test]
    fn one_axis_failing_sinks_the_verdict() {
        let verdict = JudgeVerdict::tally(vec![
            vote(true, false),
            vote(true, false),
            vote(false, true),
        ]);
        assert!(verdict.complete);
        assert!(!verdict.relevant);
        assert!(!verdict.passed, "both axes must carry");
    }

    #[test]
    fn a_degraded_panel_still_needs_a_majority_of_those_that_voted() {
        // A split two-model panel is a tie, and a tie is not a pass.
        let verdict = JudgeVerdict::tally(vec![vote(true, true), vote(false, false)]);
        assert!(!verdict.passed, "1 of 2 is a tie, not a majority");
        // Both agreeing is.
        let verdict = JudgeVerdict::tally(vec![vote(true, true), vote(true, true)]);
        assert!(verdict.passed);
        let verdict = JudgeVerdict::tally(vec![vote(false, false), vote(false, false)]);
        assert!(!verdict.passed);
    }

    #[test]
    fn a_lone_surviving_model_still_decides() {
        // One vote is a majority of one. The alternative is voiding the
        // scenario, and `judge()` already bails when every model fails.
        assert!(JudgeVerdict::tally(vec![vote(true, true)]).passed);
        assert!(!JudgeVerdict::tally(vec![vote(true, false)]).passed);
    }

    #[test]
    fn an_even_panel_needs_more_than_half() {
        // 2 of 4 is half, not a majority.
        let split = vec![vote(true, true), vote(true, true), vote(false, false), vote(false, false)];
        assert!(!JudgeVerdict::tally(split).passed);
        let three = vec![vote(true, true), vote(true, true), vote(true, true), vote(false, false)];
        assert!(JudgeVerdict::tally(three).passed);
    }

    #[test]
    fn a_reply_with_reversed_braces_is_an_error_not_a_crash() {
        // A model that describes JSON instead of emitting it. Locating the
        // braces independently used to invert the slice range and panic,
        // taking down the whole run rather than just this vote.
        let err = parse_vote("m", "the object ends with } and starts with {")
            .expect_err("prose is not a vote");
        assert!(format!("{err}").contains("did not answer with JSON"));

        // And the degenerate cases nearby.
        assert!(parse_vote("m", "").is_err());
        assert!(parse_vote("m", "{").is_err());
        assert!(parse_vote("m", "}").is_err());
    }

    #[test]
    fn a_vote_is_read_out_of_surrounding_prose() {
        let vote = parse_vote(
            "m",
            "Sure! {\"complete\": true, \"relevant\": true, \"score\": 5} Hope that helps.",
        )
        .expect("parses");
        assert!(vote.complete && vote.relevant);
        assert_eq!(vote.score, 5);
    }

    #[test]
    fn fenced_json_still_parses() {
        let vote = parse_vote(
            "m",
            "```json\n{\"complete\": true, \"relevant\": true, \"score\": 4}\n```",
        )
        .expect("parses");
        assert!(vote.complete && vote.relevant);
        assert_eq!(vote.score, 4);
    }
}
