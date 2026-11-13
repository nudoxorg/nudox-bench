# Nudox Backend v2 through `nudox-v2`, 2026-11-13 — 30 scenarios

The first Nudox number on the current product. Backend `a01c2b0`, built on
this machine (`cargo build --locked --ignore-rust-version -p backend-locald
-p backend-mcp`, rustc 1.97.0 against a pinned 1.97.1, `-j 1`, and
`--config 'profile.dev.package.tsz-checker.opt-level=0'` because that crate
was OOM-killed at its configured opt-level 2 under a 7 GB limit). Debug
build. `configs/v2.toml`: nudox-v2, ripgrep, rust-analyzer. Each root was
indexed once in preflight (not charged to any scenario); each diff got its
own workspace holding both versions.

| adapter | pass | attempted | unsupported | errored | strict | distractors | tok/scenario |
|---|---:|---:|---:|---:|---:|---:|---:|
| nudox-v2 | 9/23 (39%) | 23 | 6 | 5 | 0.49 | 2 | 2,113 |
| ripgrep | 11/20 (55%) | 20 | 9 | 0 | 0.88 | 10→9 | 6,452 |
| rust-analyzer | 0/14 | 14 | 15 | 14 | — | — | — |

## What it shows

* **Precision where ripgrep fails.** nudox-v2 passes the three new
  same-name scenarios ripgrep fails on distractors
  (`signature-hashset-insert` at 605 tokens vs 7,526,
  `signature-hashtable-get-disjoint-mut`, and `py/requests/implementors-authbase`,
  where ripgrep scores 0%).
* **Recall is the weak side.** Strict recall 0.49 against ripgrep's 0.88.
  `references` returned no records for memchr's `search_slice_with_raw`
  (callers made inside `unsafe` closures); `graph` for `memmem::find` lists
  callee names without their owning types; `outline` of memchr lists
  internal structs and fields first, so the public functions are not reached
  within the call budget; `Session.send`'s references hold `request` but not
  `resolve_redirects`.
* **Diffs reach the tool and are refused by it.** With both versions on one
  shelf, `backend.diff` answers `invalid-query: semantic diff exceeds the
  bounded result contract` for lru, bit-set, bit-vec and hashbrown
  0.16.1→0.17.1 (hashbrown 0.15.5→0.16.1 hit an owner restart). These are the
  5 errored rows. Without a shared workspace the same call answered "diff
  package is not indexed".
* **itertools does not index**: both versions are refused by the compiler
  lane (`benches/specializations.rs: authority/authority_binding; resolve
  authority fault`), so its 3 scenarios are `unsupported`.

## Caveats

* The adapter's mapping was corrected against this live server during the
  session (see the commit log); this is the third full run, not a blind one.
* Earlier runs on the same build saw intermittent owner restarts and
  connection resets when many sessions were open; the adapter now retries
  refusals the server marks as safe to repeat. Not re-verified with
  `nbench verify`.
* `rust/hashbrown/callers-hashmap-reserve` scored 0% without an error; the
  coordinate choice among the four `reserve`s was not investigated.

## What was not run

Go (`go/uuid/*`, no module proxy), rust-analyzer (not installable here), the
judge (no key).
