//! Token metering.
//!
//! # Method, and why it is this method
//!
//! `Backend/docs/MCP-SURFACE-PLAN.md` establishes the measurement contract
//! this harness has to match if its numbers are to be comparable with the
//! ones already in that document: count with a **real BPE tokenizer**, and
//! report `cl100k_base` and `o200k_base` together. Neither is Anthropic's
//! tokenizer — that one is not public — but agreement between two
//! independent vocabularies is what makes a compactness claim defensible.
//!
//! The same document records why a cheaper estimator is not acceptable here.
//! `heart::cost::estimated_text_tokens` counts an alphanumeric run as one
//! token, so a 64-character hex key measures as **1** instead of ~50 — a 50×
//! error on the single largest item in a Nudox response payload. Any
//! character-count or word-count proxy has the same defect in the same place:
//! high-entropy hex defeats BPE merges almost completely (~1.28 chars/token
//! against ~4 for identifier text), so the exact thing this benchmark exists
//! to measure is the thing an estimator gets most wrong. There is therefore
//! no "fast path" here on purpose.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tiktoken_rs::CoreBPE;

fn cl100k() -> &'static CoreBPE {
    static BPE: OnceLock<CoreBPE> = OnceLock::new();
    BPE.get_or_init(|| tiktoken_rs::cl100k_base().expect("bundled cl100k_base ranks"))
}

fn o200k() -> &'static CoreBPE {
    static BPE: OnceLock<CoreBPE> = OnceLock::new();
    BPE.get_or_init(|| tiktoken_rs::o200k_base().expect("bundled o200k_base ranks"))
}

/// One text measured under both vocabularies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCount {
    pub bytes: usize,
    pub cl100k: usize,
    pub o200k: usize,
}

impl TokenCount {
    /// Measure a string under both tokenizers.
    pub fn measure(text: &str) -> Self {
        Self {
            bytes: text.len(),
            cl100k: cl100k().encode_ordinary(text).len(),
            o200k: o200k().encode_ordinary(text).len(),
        }
    }

    /// The figure to use when one number is needed — the larger of the two,
    /// so a budget claim is never flattered by tokenizer choice.
    pub fn conservative(&self) -> usize {
        self.cl100k.max(self.o200k)
    }

    /// Whether the two vocabularies disagree by more than 5%, which is the
    /// signal that a claim should quote both rather than one.
    pub fn vocabularies_disagree(&self) -> bool {
        let (lo, hi) = if self.cl100k < self.o200k {
            (self.cl100k, self.o200k)
        } else {
            (self.o200k, self.cl100k)
        };
        hi > 0 && (hi - lo) * 20 > hi
    }
}

impl std::ops::Add for TokenCount {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            bytes: self.bytes + rhs.bytes,
            cl100k: self.cl100k + rhs.cl100k,
            o200k: self.o200k + rhs.o200k,
        }
    }
}

impl std::iter::Sum for TokenCount {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), |a, b| a + b)
    }
}

/// The cost of a whole attempt, split so that fixed and variable cost can be
/// reported separately.
///
/// # Why `resident` is separate from `responses`
///
/// An MCP server bills the agent's context before the agent asks anything:
/// every tool description and argument-schema `description` is resident from
/// the first turn. MCP-SURFACE-PLAN §1.1 measures that at 3,875 cl100k
/// tokens for the 13-tool surface. A benchmark that only counted response
/// bodies would rank a server with a 15k-token schema dump equal to one
/// without it, so the fixed cost is measured once per adapter per run and
/// carried on every attempt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptCost {
    /// Tool descriptions plus argument-schema descriptions: what the surface
    /// costs before any call. Constant per adapter.
    pub resident: TokenCount,
    /// Everything the tools returned across this attempt.
    pub responses: TokenCount,
    /// What the harness sent — queries and arguments. Small, but not zero,
    /// and it is the half an agent pays to *ask* a question.
    pub requests: TokenCount,
}

impl AttemptCost {
    /// What the agent's context actually carries after this attempt.
    pub fn total(&self) -> TokenCount {
        self.resident + self.responses + self.requests
    }

    /// The marginal cost of this attempt, excluding the fixed surface. This
    /// is the honest number for "what did this question cost", once the
    /// session is warm.
    pub fn marginal(&self) -> TokenCount {
        self.responses + self.requests
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The claim MCP-SURFACE-PLAN §1.3 rests on: a 64-hex key is dramatically
    /// more expensive per character than identifier text. If this ever stops
    /// holding, the address work it motivates needs re-justifying.
    #[test]
    fn hex_keys_defeat_bpe_merges() {
        let hex = "cargo:serde#".to_owned() + &"a3f9".repeat(16);
        let addr = "cargo:serde@1.0.196::serde::de::Deserializer::deserialize_map[method]";
        let hex_count = TokenCount::measure(&hex);
        let addr_count = TokenCount::measure(addr);

        // Comparable byte lengths...
        assert!(hex.len() as f64 / addr.len() as f64 > 0.9);
        // ...wildly different token cost.
        assert!(
            hex_count.cl100k > addr_count.cl100k * 2,
            "hex {hex_count:?} vs address {addr_count:?}"
        );
    }

    #[test]
    fn character_proxies_are_wrong_where_it_matters() {
        let hex = "a3f9".repeat(16);
        let counted = TokenCount::measure(&hex);
        let naive_chars_over_four = hex.len() / 4;
        assert!(
            counted.cl100k > naive_chars_over_four,
            "chars/4 under-counts hex: {counted:?} vs {naive_chars_over_four}"
        );
    }

    #[test]
    fn costs_add() {
        let a = TokenCount::measure("hello world");
        let b = TokenCount::measure("goodbye world");
        let sum = a + b;
        assert_eq!(sum.cl100k, a.cl100k + b.cl100k);
        assert_eq!(sum.bytes, a.bytes + b.bytes);
    }
}
