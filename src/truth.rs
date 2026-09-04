//! Deterministic scoring: did the returned context actually contain the fact?
//!
//! # Two recalls, not one
//!
//! Matching a symbol inside a blob of returned Markdown is where a
//! retrieval benchmark quietly becomes dishonest. `memchr::memchr3_iter`
//! can appear as the full path, as a Nudox address
//! (`cargo:memchr@2.8.3::memchr::memchr3_iter[function]`), or as the bare
//! identifier `memchr3_iter` in a line of source that ripgrep happened to
//! print. Those are not the same evidence, and collapsing them into one
//! boolean flatters exactly the adapters that return the most undifferentiated
//! text.
//!
//! So every fact is matched twice and both results are reported:
//!
//! * **strict** — the fully-qualified path appears as written (module
//!   separators normalized across `::`, `.` and `/`, because Nudox's address
//!   grammar treats all three as segment boundaries and Go and Python spell
//!   them differently).
//! * **loose** — the final identifier appears on a word boundary anywhere.
//!
//! A tool that resolves symbols scores near-identically on both. A text
//! search scores far higher on loose than on strict, and the gap between the
//! two columns *is* the measurement of how much resolution is worth.

use serde::{Deserialize, Serialize};

use crate::scenario::{Scenario, Truth, TruthMode};

/// How a single expected fact was found, if it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchTier {
    /// Fully-qualified path present.
    Strict,
    /// Only the final identifier present, on a word boundary.
    Loose,
    /// Not present at all.
    Absent,
}

/// One expected fact and its verdict.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactVerdict {
    pub fact: String,
    pub tier: MatchTier,
}

/// The deterministic score for one attempt.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeterministicScore {
    /// Fraction of `all_of` found at strict tier, plus the `any_of` gate.
    pub strict_recall: f64,
    /// Same, allowing loose matches.
    pub loose_recall: f64,
    /// How many `none_of` distractors leaked into the answer.
    pub distractors: usize,
    /// Whether the `any_of` gate was satisfied (vacuously true if empty).
    pub any_of_satisfied: bool,
    /// The headline boolean: full strict recall, gate satisfied, no
    /// distractors. Deliberately strict — a partial answer to "who calls
    /// this" is a wrong answer to "who calls this".
    pub passed: bool,
    pub facts: Vec<FactVerdict>,
    /// Set when the scenario is judged rather than matched.
    pub judged: bool,
    /// Strict recall counting only evidence the adapter actually matched,
    /// rather than text it printed around a match.
    ///
    /// `None` for an adapter with no incidental output — everything it
    /// returned, it chose to return, so this would equal `strict_recall`.
    /// See [`crate::adapter::Attempt::primary_context`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_strict_recall: Option<f64>,
    /// Distractors present in the adapter's **answer**, as opposed to anywhere
    /// in what it surfaced.
    ///
    /// The two differ whenever a near-miss symbol appears only in text the
    /// adapter used to *find* its answer: a grep context window, or a `search`
    /// listing that a later `refs` call answered from. `distractors` stays the
    /// number that decides `passed`; this one says how much of it the answer
    /// is actually responsible for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_distractors: Option<usize>,
}

impl DeterministicScore {
    /// The gap between loose and strict recall: how much of this adapter's
    /// apparent success is unresolved text rather than resolved symbols.
    pub fn resolution_gap(&self) -> f64 {
        (self.loose_recall - self.strict_recall).max(0.0)
    }
}

/// Normalize module separators so one fact spelling matches every ecosystem's
/// rendering of it. Nudox's own address grammar already treats `::`, `.` and
/// `/` as interchangeable segment boundaries, so this follows that rule
/// rather than inventing a second one.
fn normalize_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ':' if chars.peek() == Some(&':') => {
                chars.next();
                out.push('\u{1}');
            }
            '.' | '/' => out.push('\u{1}'),
            other => out.extend(other.to_lowercase()),
        }
    }
    out
}

/// Whether `needle` appears in `haystack` bounded by non-identifier
/// characters on both sides — so `memchr` does not match inside `memchr3`.
fn contains_word(haystack_lower: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let needle = needle.to_ascii_lowercase();
    let bytes = haystack_lower.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = haystack_lower[from..].find(&needle) {
        let start = from + rel;
        let end = start + needle.len();
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let after_ok = end >= bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        // Advance by one whole character, not one byte: `start + 1` lands
        // inside a multi-byte character whenever the needle begins with one,
        // and re-slicing there panics. A fact naming a non-ASCII identifier
        // would take the whole run down mid-score.
        from = start
            + haystack_lower[start..]
                .chars()
                .next()
                .map_or(1, char::len_utf8);
        if from >= haystack_lower.len() {
            break;
        }
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Match one expected fact against the returned text.
pub fn match_fact(fact: &str, text: &str, text_lower: &str, text_normalized: &str) -> MatchTier {
    // Strict: the qualified path, separator-normalized.
    let fact_norm = normalize_path(fact);
    if fact_norm.contains('\u{1}') && text_normalized.contains(&fact_norm) {
        return MatchTier::Strict;
    }
    // A fact with no separators can still match strictly: there is no
    // qualification to lose, so a word-bounded hit is the strongest evidence
    // the fact admits.
    if !fact_norm.contains('\u{1}') && contains_word(text_lower, fact) {
        return MatchTier::Strict;
    }
    // Loose: the final identifier only.
    let tail = fact.rsplit([':', '.', '/']).next().unwrap_or(fact);
    let tail = tail.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
    if !tail.is_empty() && contains_word(text_lower, tail) {
        let _ = text; // kept for symmetry with future span-aware matching
        return MatchTier::Loose;
    }
    MatchTier::Absent
}

/// Score returned context against a scenario's recorded truth.
pub fn score(scenario: &Scenario, returned_text: &str) -> DeterministicScore {
    let truth: &Truth = &scenario.truth;
    if truth.mode == TruthMode::Judged {
        return DeterministicScore {
            judged: true,
            ..Default::default()
        };
    }

    let text_lower = returned_text.to_ascii_lowercase();
    let text_normalized = normalize_path(returned_text);

    let mut facts = Vec::new();
    let mut strict_hits = 0usize;
    let mut loose_hits = 0usize;

    for fact in &truth.all_of {
        let tier = match_fact(fact, returned_text, &text_lower, &text_normalized);
        match tier {
            MatchTier::Strict => {
                strict_hits += 1;
                loose_hits += 1;
            }
            MatchTier::Loose => loose_hits += 1,
            MatchTier::Absent => {}
        }
        facts.push(FactVerdict {
            fact: fact.clone(),
            tier,
        });
    }

    // `any_of` is one gate, not N facts: satisfying it once is the whole
    // requirement, so it contributes a single unit to recall.
    let mut any_strict = false;
    let mut any_loose = false;
    for fact in &truth.any_of {
        let tier = match_fact(fact, returned_text, &text_lower, &text_normalized);
        any_strict |= tier == MatchTier::Strict;
        any_loose |= tier != MatchTier::Absent;
        facts.push(FactVerdict {
            fact: fact.clone(),
            tier,
        });
    }

    let mut distractors = 0usize;
    for fact in &truth.none_of {
        if match_fact(fact, returned_text, &text_lower, &text_normalized) != MatchTier::Absent {
            distractors += 1;
            facts.push(FactVerdict {
                fact: format!("!{fact}"),
                tier: MatchTier::Strict,
            });
        }
    }

    let denom = truth.required_count().max(1) as f64;
    let strict_recall =
        (strict_hits as f64 + if any_strict { 1.0 } else { 0.0}).min(denom) / denom;
    let loose_recall = (loose_hits as f64 + if any_loose { 1.0 } else { 0.0}).min(denom) / denom;
    let any_of_satisfied = truth.any_of.is_empty() || any_loose;

    DeterministicScore {
        passed: strict_recall >= 1.0 - f64::EPSILON && any_of_satisfied && distractors == 0,
        strict_recall,
        loose_recall,
        distractors,
        any_of_satisfied,
        facts,
        judged: false,
        // Filled in by the runner for adapters that can distinguish matched
        // evidence from surrounding text; this function scores whatever text
        // it is handed and does not know which kind it is.
        primary_strict_recall: None,
        primary_distractors: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiers(fact: &str, text: &str) -> MatchTier {
        match_fact(fact, text, &text.to_ascii_lowercase(), &normalize_path(text))
    }

    /// The whole point of splitting answer from navigation.
    ///
    /// A `search` listing that finds a symbol legitimately contains its
    /// near-names; the `refs` answer built from it does not. Scored against
    /// the union, the adapter is charged for a distractor its answer never
    /// contained — the `go/uuid/callers-parse` case in
    /// `run-2026-08-30-225507`.
    #[test]
    fn a_distractor_in_navigation_is_not_one_in_the_answer() {
        let scenario = crate::testing::scenario_with_truth(Truth {
            mode: TruthMode::SymbolSet,
            all_of: vec!["MustParse".into()],
            any_of: vec![],
            none_of: vec!["ParseBytes".into()],
            oracle: None,
            sources: vec![],
        });

        // What `search` returned while locating `Parse`.
        let navigation = "go:github.com/google/uuid::ParseBytes[function]\n\
                          go:github.com/google/uuid::Parse[function]";
        // What `refs` answered with.
        let answer = "go:github.com/google/uuid::MustParse[function]";
        let union = format!("{navigation}\n\n{answer}");

        let whole = score(&scenario, &union);
        assert_eq!(whole.distractors, 1, "the union does contain the near-name");
        assert!(!whole.passed, "and that is what decides the verdict today");

        let only_answer = score(&scenario, answer);
        assert_eq!(only_answer.distractors, 0, "the answer does not");
        assert_eq!(only_answer.strict_recall, 1.0);
    }

    /// Word boundaries do the work here, and it is worth pinning against a
    /// real payload: every `ParseBytes` in the `uuid` search response of
    /// 2026-08-31 was inside a longer identifier, so none of them is the
    /// distractor and the run correctly recorded none.
    #[test]
    fn a_longer_identifier_is_not_the_distractor() {
        for line in [
            "go:github.com/google/uuid::BenchmarkParseBytes[function]",
            "go:github.com/google/uuid::FuzzParseBytes[function]",
            "go:github.com/google/uuid::BenchmarkParseBytesUnsafe[function]",
            "  fn parseBytesCopy(b: [u8])",
        ] {
            assert_eq!(
                tiers("ParseBytes", line),
                MatchTier::Absent,
                "{line} must not be credited as ParseBytes"
            );
        }
        // The bare declaration, however, is exactly the distractor.
        assert_eq!(
            tiers("ParseBytes", "go:github.com/google/uuid::ParseBytes[function]"),
            MatchTier::Strict
        );
    }

    #[test]
    fn qualified_path_matches_strictly_across_separators() {
        assert_eq!(
            tiers("memchr::memchr3_iter", "cargo:memchr@2.8.3::memchr::memchr3_iter[function]"),
            MatchTier::Strict
        );
        // Go spells it with a dot; the fact may be written either way.
        assert_eq!(tiers("errors::Wrap", "errors.Wrap(err, \"x\")"), MatchTier::Strict);
    }

    #[test]
    fn bare_identifier_from_a_source_dump_is_only_loose() {
        assert_eq!(
            tiers("memchr::memchr3_iter", "    let it = memchr3_iter(a, b, c, hay);"),
            MatchTier::Loose
        );
    }

    #[test]
    fn word_boundaries_stop_prefix_collisions() {
        // The classic text-search false positive this benchmark must not reward.
        assert_eq!(tiers("memchr", "pub fn memchr3(...)"), MatchTier::Absent);
        assert_eq!(tiers("memchr", "pub fn memchr(...)"), MatchTier::Strict);
    }

    #[test]
    fn a_non_ascii_fact_does_not_panic_the_run() {
        // The first `é` is preceded by an identifier byte, so it is not a
        // word match and the search advances past it. Advancing by one byte
        // used to split the character and panic, taking the whole run down.
        assert!(contains_word("xé foo é", "é"));
        assert!(!contains_word("xé foo", "é"));
        // A multi-byte identifier still matches on its own boundaries.
        assert_eq!(tiers("café", "let café = 1;"), MatchTier::Strict);
    }

    #[test]
    fn absent_is_absent() {
        assert_eq!(tiers("serde::Deserializer", "nothing to see"), MatchTier::Absent);
    }

    #[test]
    fn distractors_fail_an_otherwise_perfect_answer() {
        let scenario = crate::testing::scenario_with_truth(Truth {
            mode: TruthMode::SymbolSet,
            all_of: vec!["memchr::memchr3".into()],
            any_of: vec![],
            none_of: vec!["memchr::memrchr3".into()],
            oracle: None,
            sources: vec![],
        });
        let clean = score(&scenario, "memchr::memchr3 is here");
        assert!(clean.passed);
        let noisy = score(&scenario, "memchr::memchr3 and memchr::memrchr3 both");
        assert!(!noisy.passed, "distractor must sink the answer");
        assert_eq!(noisy.distractors, 1);
    }

    #[test]
    fn resolution_gap_exposes_text_search() {
        let scenario = crate::testing::scenario_with_truth(Truth {
            mode: TruthMode::SymbolSet,
            all_of: vec!["memchr::memchr3_iter".into(), "memchr::memchr2_iter".into()],
            any_of: vec![],
            none_of: vec![],
            oracle: None,
            sources: vec![],
        });
        let resolved = score(&scenario, "memchr::memchr3_iter memchr::memchr2_iter");
        assert_eq!(resolved.resolution_gap(), 0.0);
        let grepped = score(&scenario, "memchr3_iter(...) memchr2_iter(...)");
        assert!(grepped.resolution_gap() > 0.9, "{grepped:?}");
        assert!(!grepped.passed);
    }
}
