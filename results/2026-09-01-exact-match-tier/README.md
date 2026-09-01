# The ranking fix, measured

`run-2026-09-01-111915` against
[`../2026-08-31-embedder-ab`](../2026-08-31-embedder-ab) (`run-2026-08-31-202901`)
— **same endpoint process, same two packages, same scenarios, same harness
binary, one variable**: the Backend working tree now carries the exact-match
tier and the documentation cap.

This is the first measurement of `analysis/PROPOSAL.md`'s two Backend
recommendations, R1 and R2, as implemented on Backend branch
`exact-match-tier-and-doc-cap`. It settles hypotheses **H3**, **H4**, **H7** and
**H8**, and promotes E4's inferred token decomposition to a measurement.

## The result

| adapter | pass | strict | tok/scenario |
|---|---|---|---|
| nudox-full | 3/5 → **4/5** | 0.83 → **1.00** | 12,961 → **8,891** (−31%) |
| nudox-keys | 3/5 → **4/5** | 0.83 → **1.00** | 12,961 → **8,891** (−31%) |
| nudox-graph-first | 4/5 → 4/5 | 1.00 → 1.00 | 11,536 → **8,529** (−26%) |
| nudox-search-only | 3/7 → **2/7** | 0.57 → 0.54 | 6,544 → **3,438** (−47%) |
| nudox-oneshot | 1/5 → 1/5 | 0.43 → 0.43 | 4,114 → **1,564** (−62%) |
| ripgrep | 6/7 | 0.95 | 8,447 — *not comparable, see below* |

Structural reference (embedder off, `../2026-08-31-graph-fixed`):
`nudox-full` 5/5, strict 1.00, 4,464 tok/scenario.

**⚠ The ripgrep row is not comparable across these two runs.** `${NODE_MODULES}`
was unset in the shell that launched this run, so `ts/commander/public-surface`
declined for ripgrep and it attempted 6 scenarios instead of 7. The Nudox
adapters are unaffected — they answer from the endpoint's loaded corpus, not
from a path — so every Nudox row above *is* comparable. Use the recorded 8,447
when comparing against ripgrep. The `nudox-keys` "degraded capability" notice
appears in both runs and is not new.

## 1. The buried exact match came back — H3 resolved

`rust/memchr/callers-search-slice-with-raw`, the scenario the whole proposal was
written around:

| | before | after |
|---|---|---|
| nudox-full | fail, 17% recall, 9,836 tok | **pass, 100%, 4,456 tok** |
| nudox-keys | fail, 17% recall, 9,836 tok | **pass, 100%, 4,456 tok** |

Queried directly against the live endpoint, `search_slice_with_raw` now returns
at **rank 1** at every page depth tested (50, 100, 200, 500). Before the fix it
was absent from the whole fifty-row page.

So R1's mechanism is confirmed end to end: the name row *was* being fetched and
merged all along and was purely outranked, exactly as PROPOSAL §1.3 predicted.
Tiering the comparator was sufficient — no similarity floor was needed.

## 2. The page got half as expensive for identical rows — H7, and E4 promoted

Across all nine distinct `search` payloads in each run, both runs returned the
**same 547 rows and the same 161 documentation lines**:

| | before | after |
|---|---:|---:|
| address lines | 10,656 tok (19.8%) | 10,631 tok (39.9%) |
| signature lines | 9,965 tok (18.5%) | 9,973 tok (37.5%) |
| **documentation** | **30,249 tok (56.1%)** | **2,977 tok (11.2%)** |
| envelope | 3,049 tok | 3,048 tok |
| **total** | **53,919 tok** | **26,629 tok** |
| mean per row | 98.6 tok | **48.7 tok** |

Documentation fell **90%** and the whole search surface fell **51%**, with the
result set byte-for-byte unchanged in which symbols it names. Per-call, on the
three worst scenarios: −69%, −67%, −64%.

These counts are exact, not estimated. They come from a local `cl100k_base`
tokenizer built on the ranks `tiktoken-rs` vendors, validated to reproduce the
harness's own recorded `cl100k` figure on **all 63 captured steps, exactly**.
E4's inferred "~181 tok/row, of which ~135–155 is the `doc:` line" was in the
right place for the worst payload but too high as a corpus mean; the measured
mean was 98.6 tok/row before and 48.7 after.

**H7 (is doc text duplicated across rows?) — yes, and R2 mostly absorbed it.**
Duplicate documentation was 7,779 tok (14.4% of the search surface) before, and
is 1,263 tok (4.7%) after. On the single worst payload —
`callers-search-slice-with-raw` — 36 doc lines held only 14 distinct strings and
duplication alone was 54% of that response. Deduplicating is still worth ~5%,
but it is no longer the lever it was; R2 took the same tokens more cheaply.

## 3. `search-only` lost a pass, and the pass was not real — H4 and H8

`rust/memchr/public-surface` for `nudox-search-only` went from pass (100%,
18,032 tok) to fail (80%, 9,117 tok). Three facts were lost: `memchr_iter`,
`memchr2_iter`, `memchr3_iter`.

Those three were **never rows in the result**. In both the before and after
payloads they appear nowhere in any address line; before the fix they occurred
only inside a *neighbouring symbol's documentation prose*, and the cap removed
that prose. Under the union scoring rule (EVIDENCE §E.2) the prose counted as
recall.

So H8's warning was right that R2 would cost a pass, and the pass it cost was
the same false-positive class the provenance column was added to catch: recall
credited to text the listing happened to carry, not to a symbol the tool
surfaced. `nudox-search-only`'s honest score on this corpus is 2/7 both before
and after.

**That revises claim 1 of the embedder A/B page.** "It helps exactly where
search is the answer — 2/7 → 3/7" was carried by that one scenario, and the
extra pass was doc prose. The embedder's honest pass contribution on this corpus
is zero on every adapter.

The more careful version of the claim survives, though, and is worth keeping:
semantic ranking *did* raise recall on that scenario from **60% to 80%**, because
it reaches the six `memrchr*` symbols that prefix matching structurally cannot
(`NameIndex::prefix` is a `starts_with` scan, so `memchr` never reaches
`memrchr`). That is a real gain. What stops it reaching 100% is §4.

## 4. What is still broken: the semantic section crowds out prefix matches

The three lost facts are exactly three symbols whose names *start with the
query*. They are in the name index, they are collected, and they are then
outranked off the page.

Measured on the live endpoint, query `memchr`, package `cargo:memchr`:

| limit | rows | semantic | structural | of the 6 name matches, present |
|---:|---:|---:|---:|---|
| embedder off | 23 | 0 | 23 | **6/6** |
| 50 | 50 | 43 | 7 | 3/6 |
| 200 | 200 | 197 | 3 | 3/6 |
| 500 | 500 | 497 | 3 | 3/6 |

**Deepening the page does not help, and between 50 and 200 it actively hurts** —
structural rows fall from 7 to 3. The semantic section grows with `limit` while
the name section is bounded by however many prefix matches exist, so a larger
page is a larger majority of semantic rows.

The arithmetic is the same one R1 fixed, one notch down. `memchr_iter` is a
public function, so its score is `text_relevance × 1.0 × 1.0`, and
`text_relevance` for a prefix is a length ratio: `len("memchr") / len("memchr_iter")`
= **0.545**. Any public semantic row whose cosine clears **0.09** outranks it.
R1 deliberately scoped the exact tier to exact matches — "a weak prefix match
has no such claim" — which is the right call for a two-character query and the
wrong one here, where six of six name matches became three.

Narrowing `kinds` to `function` recovers one of the three (`memchr_iter`),
because the kind filter thins the semantic section too. That is a workaround,
not a fix.

**Testable prediction:** the three facts `nudox-search-only` is missing are
exactly the three displaced rows, so a change that keeps prefix name matches on
the page should take `rust/memchr/public-surface` to 15/15 facts and a
**legitimate** pass — the first pass on that scenario that is not carried by doc
prose. `analysis/MEASUREMENT.md` §5 states the candidate changes.

## What this run does not settle

* **H1 — the cosine distribution** is still unmeasured. R1 made it unnecessary
  for the exact-match case, and §4 gives a floor a *lower* bound of interest
  (0.09) rather than the 0.2/0.6 crossovers the proposal discussed, but nothing
  here plots the distribution.
* **`rust/memchr/iter-return-type`** still fails at 100% recall on two
  distractors that are in the listing and not in the answer (`2→0`). That is the
  union scoring rule, deliberately unchanged, and no Backend change addresses it.
* **Statistical weight.** Two packages, five to seven scenarios per adapter. One
  scenario flipping moves a pass column by 14–20 points. Everything above is a
  point observation about known scenarios, not an estimate of a population
  effect.
* The Backend changes are **uncommitted working-tree edits** on branch
  `exact-match-tier-and-doc-cap`. This run measures that tree, not a merged
  state.
