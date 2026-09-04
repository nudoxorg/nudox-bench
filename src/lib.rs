//! `nudox-bench` — a benchmark for the Nudox code-intelligence surface.
//!
//! # What this measures, and what it refuses to
//!
//! The question is not "does Nudox return something". It is: **for a real
//! question about a real package, does a system produce the answer, how well
//! resolved is the evidence, and what does it cost an agent's context** —
//! measured against systems that are not Nudox, including the ones an agent
//! already has.
//!
//! Three design commitments follow from that, and they are what distinguish
//! this from the documentation-retrieval benchmark it borrows its shape from:
//!
//! * **Most scenarios are scored without a model in the loop.** A question
//!   like "who calls `memchr3`" has a recorded answer, so it is matched, not
//!   judged. Only [`Family::Implementation`](scenario::Family::Implementation)
//!   reaches [`judge`]. Runs are therefore reproducible, free, and free of
//!   judge variance.
//! * **Cost is counted with a real BPE tokenizer, and the fixed surface cost
//!   is counted separately from the per-question cost.** See [`tokens`].
//! * **Declining is recorded, not scored.** An adapter that cannot express a
//!   question reports [`Attempt::unsupported`](adapter::Attempt::unsupported),
//!   which never enters a pass rate.

pub mod adapter;
pub mod config;
pub mod judge;
pub mod mcp;
pub mod report;
pub mod runner;
pub mod scenario;
pub mod text;
pub mod tokens;
pub mod truth;
pub mod verify;

#[cfg(test)]
pub mod testing;
