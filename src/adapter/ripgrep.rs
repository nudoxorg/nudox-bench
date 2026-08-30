//! The floor: text search over the source tree.
//!
//! # Why this adapter is the one that matters most
//!
//! Every code-intelligence tool is sold against an agent that has `Read` and
//! `Grep` and nothing else. That agent is not a strawman — it is what most
//! coding agents actually do today, it needs no index, no server and no
//! warm-up, and on a small package it is *fast*. If a resolved-symbol index
//! cannot beat it, the index is not paying for itself.
//!
//! Two properties of this adapter keep that comparison honest:
//!
//! * It is given the same source tree the index was built from, so it is not
//!   handicapped by missing data.
//! * Its output is capped ([`MAX_OUTPUT_BYTES`]) and the cap is *reported*.
//!   An uncapped grep can score perfect recall on any scenario by printing
//!   the package, which would be a measurement artefact rather than a
//!   result. The cap is what makes "found it" mean "found it in a budget a
//!   real agent would spend", and the `truth.none_of` distractor sets are
//!   what stop a wide net from scoring as a clean answer.

use std::process::Stdio;
use std::time::Instant;

use super::{Adapter, Attempt, AttemptBuilder};
use crate::scenario::{Scenario, TaskKind};

/// How much grep output a run is willing to put in an agent's context.
/// Roughly 25k tokens of source — already far past what a careful agent
/// would paste, and deliberately generous so the cap cannot be blamed for a
/// poor score.
pub const MAX_OUTPUT_BYTES: usize = 100_000;

/// Text search over a package checkout.
pub struct RipgrepAdapter {
    keep_responses: bool,
}

impl RipgrepAdapter {
    pub fn new(keep_responses: bool) -> Self {
        Self { keep_responses }
    }

    /// The patterns this task would send a developer to type, in order.
    ///
    /// Multiple patterns are allowed because that is what a developer does:
    /// the first grep for a name is rarely the last. Each one is a separate
    /// step and is charged for separately.
    fn patterns(scenario: &Scenario) -> Vec<(String, Vec<String>)> {
        let Some(literal) = scenario.probe.literal() else {
            return Vec::new();
        };
        let word = regex_escape(literal);
        match scenario.task {
            TaskKind::Definition | TaskKind::Signature => vec![(
                format!("declaration of {literal}"),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    // A signature spans lines: `pub fn memchr3(` puts every
                    // parameter and the return type on the lines that follow.
                    // Grepping the one matching line would score this adapter
                    // on a formatting convention rather than on what it found.
                    "-A".into(),
                    "8".into(),
                    "-e".into(),
                    format!(
                        r"(fn|struct|trait|enum|type|const|static|class|def|func|interface)\s+{word}\b"
                    ),
                ],
            )],
            TaskKind::Callers => vec![(
                format!("call sites of {literal}"),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    // The line holding a call names the callee, never the
                    // caller. Answering "who calls this" from grep output
                    // means reading back up to the enclosing declaration, so
                    // the leading context is part of the honest baseline —
                    // and the tokens it costs are part of the honest price.
                    "-B".into(),
                    "12".into(),
                    "-e".into(),
                    format!(r"\b{word}\s*\("),
                ],
            )],
            TaskKind::Outbound => vec![(
                format!("body of {literal}"),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    "-A".into(),
                    "40".into(),
                    "-e".into(),
                    format!(r"(fn|func|def)\s+{word}\b"),
                ],
            )],
            TaskKind::Implementors => vec![(
                format!("impls of {literal}"),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    "-e".into(),
                    format!(r"impl(<[^>]*>)?\s+.*\b{word}\b.*\s+for\s+"),
                ],
            )],
            TaskKind::Members => vec![(
                format!("body of {literal}"),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    "-A".into(),
                    "60".into(),
                    "-e".into(),
                    format!(r"(struct|enum|trait|class|interface)\s+{word}\b"),
                ],
            )],
            TaskKind::Overview => vec![(
                "exported surface".into(),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    "-e".into(),
                    r"^\s*(pub |export |public )".into(),
                ],
            )],
            TaskKind::Search | TaskKind::Implement => vec![(
                format!("mentions of {literal}"),
                vec![
                    "-n".into(),
                    "--no-heading".into(),
                    "-e".into(),
                    word.clone(),
                ],
            )],
            // Comparing two versions needs two checkouts; a scenario carries
            // one root, so this is declined rather than faked.
            TaskKind::Diff => Vec::new(),
        }
    }
}

#[async_trait::async_trait]
impl Adapter for RipgrepAdapter {
    fn name(&self) -> &str {
        "ripgrep"
    }

    fn description(&self) -> String {
        format!(
            "ripgrep over the package checkout, output capped at {} bytes",
            MAX_OUTPUT_BYTES
        )
    }

    async fn attempt(&mut self, scenario: &Scenario) -> Attempt {
        let Some(root) = scenario.target.root.as_ref() else {
            return Attempt::unsupported(
                self.name(),
                scenario,
                "scenario has no target.root; a text search needs the source on disk",
            );
        };
        if !root.exists() {
            return Attempt::unsupported(
                self.name(),
                scenario,
                format!("target.root does not exist: {}", root.display()),
            );
        }
        if scenario.task == TaskKind::Diff {
            return Attempt::unsupported(
                self.name(),
                scenario,
                "comparing two versions needs two checkouts; the scenario carries one",
            );
        }

        let patterns = Self::patterns(scenario);
        if patterns.is_empty() {
            return Attempt::unsupported(self.name(), scenario, "no probe literal to search for");
        }

        let mut builder =
            AttemptBuilder::new(self.name(), scenario, Default::default(), self.keep_responses);
        let mut error = None;

        for (label, args) in patterns {
            let started = Instant::now();
            let rendered = format!("rg {} {}", args.join(" "), root.display());
            let output = tokio::process::Command::new("rg")
                .args(&args)
                .arg(root)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await;

            let (text, is_error) = match output {
                Ok(out) => {
                    let mut stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                    let truncated = stdout.len() > MAX_OUTPUT_BYTES;
                    if truncated {
                        // Cut on a line boundary so the transcript stays readable.
                        let cut = stdout[..MAX_OUTPUT_BYTES]
                            .rfind('\n')
                            .unwrap_or(MAX_OUTPUT_BYTES);
                        stdout.truncate(cut);
                        stdout.push_str(&format!(
                            "\n… output truncated at {MAX_OUTPUT_BYTES} bytes; \
                             the remaining matches were not read\n"
                        ));
                    }
                    // rg exits 1 for "no matches", which is an answer, not a failure.
                    let failed = !out.status.success() && out.status.code() != Some(1);
                    if failed {
                        let err = String::from_utf8_lossy(&out.stderr).into_owned();
                        (format!("ripgrep failed: {err}"), true)
                    } else if stdout.trim().is_empty() {
                        ("(no matches)".to_owned(), false)
                    } else {
                        (stdout, false)
                    }
                }
                Err(e) => (format!("could not run ripgrep: {e}"), true),
            };

            if is_error {
                error.get_or_insert_with(|| text.clone());
            }
            builder.step(label, rendered, &text, started.elapsed(), is_error);
        }

        builder.finish(error)
    }
}

/// Escape a literal for use inside a regex.
fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regex_metacharacters_are_escaped() {
        assert_eq!(regex_escape("Vec<T>"), "Vec<T>");
        assert_eq!(regex_escape("a.b"), r"a\.b");
        assert_eq!(regex_escape("f(x)"), r"f\(x\)");
    }

    #[test]
    fn diff_is_declined_rather_than_faked() {
        let scenario = crate::testing::diff_scenario();
        assert!(RipgrepAdapter::patterns(&scenario).is_empty());
    }
}
