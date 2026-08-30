//! Oracle for `rust/memchr/implement-three-byte-scan`.
//!
//! A typical, working use of memchr's three-needle iterator API. Written
//! against the crate's own documentation for `memchr3_iter` and `Memchr3`
//! (src/memchr.rs:256 and :432 of 2.8.3) — not against any tool's output.

use memchr::{memchr3, memchr3_iter, Memchr3};

/// Every index in `haystack` holding one of the three needles.
pub fn all_positions(n1: u8, n2: u8, n3: u8, haystack: &[u8]) -> Vec<usize> {
    memchr3_iter(n1, n2, n3, haystack).collect()
}

/// The first such index, or `None`.
pub fn first_position(n1: u8, n2: u8, n3: u8, haystack: &[u8]) -> Option<usize> {
    memchr3(n1, n2, n3, haystack)
}

/// Split `haystack` on any of the three needles, yielding the spans between
/// them. Uses the iterator type by name so the signature is nameable.
pub fn split_on_any(n1: u8, n2: u8, n3: u8, haystack: &[u8]) -> Vec<&[u8]> {
    let iter: Memchr3<'_> = memchr3_iter(n1, n2, n3, haystack);
    let mut spans = Vec::new();
    let mut start = 0usize;
    for index in iter {
        spans.push(&haystack[start..index]);
        start = index + 1;
    }
    spans.push(&haystack[start..]);
    spans
}

/// The same scan in reverse, to show the double-ended property the docs
/// promise for the returned iterator.
pub fn last_position(n1: u8, n2: u8, n3: u8, haystack: &[u8]) -> Option<usize> {
    memchr3_iter(n1, n2, n3, haystack).next_back()
}
