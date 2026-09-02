# The warm A/B — what the semantic section costs when it is actually running

Two runs, `ready×30` on every search step, one variable: the demand-gated
semantic budget (N4). Read against
[`../2026-08-31-graph-fixed`](../2026-08-31-graph-fixed), which is the same
harness with no embedder configured (`unavailable(NoModelConfigured)` on all 30
steps).

Every figure is over the **five scenarios every adapter answered**, not each
adapter's own denominator — see `scripts/head2head.py` and
`analysis/DYNAMIC-EMBEDDER.md` §0.

## 1. The result

| adapter | embedder off | embedder on (warm) | N4 (warm) |
|---|---|---|---|
| **nudox-full** | **5/5 · 1.00 · 4,446 tok** | 4/5 · 1.00 · 8,836 | 4/5 · 1.00 · 8,854 |
| nudox-graph-first | 5/5 · 1.00 · 4,684 | 4/5 · 1.00 · 8,495 | 4/5 · 1.00 · 8,513 |
| nudox-keys | 5/5 · 1.00 · 4,446 | 4/5 · 1.00 · 8,836 | 4/5 · 1.00 · 8,854 |
| ripgrep | 4/5 · 0.93 · 9,599 | 4/5 · 0.93 · 9,599 | 4/5 · 0.93 · 9,599 |
| nudox-search-only | 2/5 · 0.54 · 367 | 2/5 · **0.76** · 4,757 | 2/5 · 0.76 · 4,775 |
| nudox-oneshot | 2/5 · 0.43 · 150 | 1/5 · 0.43 · 1,544 | 1/5 · 0.43 · 1,557 |

**Turning the semantic section on costs `nudox-full` a pass and doubles its
tokens.** N4 changes nothing — +18 tokens, no fact moves.

## 2. Why N4 is null

The rule reserves the page for structural rows and offers the semantic arm only
what is left: `budget = limit − structural`. The adapters ask for
`limit: 200`, and the structural section supplies 21 rows against
`rust/memchr/public-surface`. A budget of 179 trims nothing.

The rule is not wrong — verified directly against a ready endpoint, a scope
where structural fills the page returns **zero documentation lines**, meaning
the semantic section was squeezed out entirely. It simply never triggers at the
page sizes these adapters request.

Which relocates the question: **what `limit` should an adapter ask for?** A
200-row page is itself the cost driver, and under the union scoring rule every
extra row is a liability, not just an expense — see §3.

## 3. The semantic rows *are* the distractors

`nudox-full`, scenario by scenario, embedder off → warm on:

| scenario | off | warm on |
|---|---|---|
| `iter-return-type` | **pass**, 0 distractors, 315 tok | **fail**, **2 distractors**, 2,745 tok |
| `callers-search-slice-with-raw` | pass, 1,602 | pass, 4,498 |
| `public-surface` | pass, 13,172 | pass, 21,276 |
| `signature-memchr3` | pass, 486 | pass, 2,131 |
| `ts/commander/public-surface` | pass, 6,656 | pass, 13,530 |

The one lost pass is not a ranking failure — strict recall stays at 1.00. The
scenario's `none_of` names `Memchr2` and `Memchr3` precisely so that answering a
signature question by returning the whole iterator module is marked down, and
the semantic section returns exactly that. Every other scenario pays 2–4× the
tokens for a byte-identical verdict.

## 4. Where the section does pay

Measured fact by fact (`scripts/embedder_utility.py`), off → warm on:

| adapter | facts gained | facts lost | net | Δ tokens | tok per fact gained |
|---|---:|---:|---:|---:|---:|
| **nudox-search-only** | +15 | −3 | **+12** | +21,833 | 1,819 |
| nudox-full | 0 | 0 | 0 | +21,949 | — |
| nudox-keys | 0 | 0 | 0 | +21,949 | — |
| nudox-graph-first | 0 | 0 | 0 | +19,053 | — |
| nudox-oneshot | 0 | 0 | 0 | +6,971 | — |
| ripgrep | 0 | 0 | 0 | 0 | — |

Only `nudox-search-only` gains anything, and it gains real ground —
`ts/commander/public-surface` goes fail→PASS on nine exports a `starts_with`
prefix scan structurally cannot enumerate, and strict recall rises 0.54 → 0.76.
The three facts it *loses* are `memchr_iter`, `memchr2_iter` and
`memchr3_iter` — displacement, the defect the exact-match tier was scoped too
narrowly to catch.

So the section earns its place on exactly one shape of query: **the structural
index cannot answer, and the caller has no second tool to fall back on.** Every
policy that has a graph tool already answers those queries more cheaply.

## 5. What this supports

**On this corpus, the semantic section should be off for `full`, `keys` and
`graph-first`, and on for `search-only`.** That is a measured 5/5 at 4,446
tokens instead of 4/5 at 8,836 — better on both axes at once, and it beats the
ripgrep floor (4/5, 0.93, 9,599) on passes, strict recall and cost together.

It is a policy decision, not an engine change, and it is the one intervention in
this whole investigation that moves the pass column in the right direction.

## Caveats

* Two packages, five comparable scenarios. One scenario flipping moves a pass
  column 20 points.
* `cargo:memchr`'s semantic index takes **510 seconds** to build;
  `npm:commander`'s is ready in under two minutes. A run started before that is
  measuring a structural-only server that reports `semantic search: on`. Both
  runs here are `ready×30`.
* The Backend under test is `exact-match-tier-and-doc-cap` at `6c63b655`
  (R1 + R2). N4 was built and measured, found null, and not committed.
