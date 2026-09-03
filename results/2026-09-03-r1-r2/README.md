# R1 + R2, measured

`run-2026-09-03-143940` — the same two-package, embedder-on configuration as
[`../2026-08-31-embedder-ab`](../2026-08-31-embedder-ab), with the two Backend
changes from [`../../analysis/PROPOSAL.md`](../../analysis/PROPOSAL.md) applied
(`Backend` branch `exact-match-tier-and-doc-cap`).

Predictions were written down first, in
[`../../analysis/PREDICTIONS.md`](../../analysis/PREDICTIONS.md), and are scored
below including the two I got wrong.

## The numbers

| adapter | pass | strict | matched | tok/scenario |
|---|---|---|---|---|
| nudox-full | 3/5 → **4/5** | 0.83 → **1.00** | 0.83 → **1.00** | 12,955 → **8,891** |
| nudox-graph-first | 4/5 → 4/5 | 1.00 → 1.00 | 0.80 → **1.00** | 11,529 → **8,529** |
| nudox-keys | 3/5 → **4/5** | 0.83 → **1.00** | 0.83 → **1.00** | 12,955 → **8,891** |
| nudox-search-only | 3/7 → **2/7** | 0.57 → 0.54 | 0.57 → 0.54 | 6,544 → **3,438** |
| nudox-oneshot | 1/5 → 1/5 | 0.43 → 0.43 | 0.43 → 0.43 | 4,114 → **1,564** |
| ripgrep (control) | 6/7 | 0.95 | 0.57 | 8,447 — **unchanged** |

## Scorecard

| | prediction | outcome |
|---|---|---|
| **P1** | `memchr/callers` passes | ✅ fail 17% → **pass**, strict 1.00 |
| **P2** | `iter-return-type` still fails, so 4/5 not 5/5 | ✅ **the proposal's R1 row was wrong**, mine was right |
| **P3** | nudox-full 4/5, strict ~0.97 | ✅ 4/5, strict **1.00** |
| **P4** | tok/scenario 4,000–6,000 | ❌ **8,891** — I was wrong, see below |
| **P5** | search-only keeps 3/7 | ❌ **fell to 2/7** — and the reason matters more than the number |
| **P6** | ripgrep unchanged | ✅ exactly 6/7, 0.95, 8,447 — control held |

## R1 did exactly what it was designed to do

`rust/memchr/callers-search-slice-with-raw` was the scenario the tier was
written for: the exact match `pub(crate) fn search_slice_with_raw` scored 0.60
and lost to fifty public semantic rows, so it was absent from the page
entirely. With the tier it sorts first, `pick_hit` selects it, `refs` answers
about the right symbol, and the scenario **passes**. `nudox-full`'s strict
recall is now **1.00** across everything it attempts.

`nudox-graph-first`'s `matched` also went 0.80 → 1.00, which is the provenance
column confirming the same thing from the other side: its answers now come
entirely from what it deliberately returned.

## P4 — why the cost estimate was wrong

I predicted 4,000–6,000 tokens per scenario. The real figure is 8,891, still
just **above** ripgrep's 8,447 — so the proposal's "back under ripgrep" claim
does not hold either.

Both estimates assumed `search` dominates every scenario. It does not. After
R2, the biggest single response in the run is the **graph enumeration** on
`memchr/public-surface` at **21,276 tokens** — 500 rows of name, kind and path,
carrying no documentation at all, which R2 cannot touch. R2 worked exactly as
specified on the responses it governs (`iter-return-type`'s search response
went 7,283 → 2,745; search-only's whole scenario halved 6,544 → 3,438), but
docs are no longer the dominant cost.

**The next cost lever is the overview enumeration's page size, not doc text.**

## P5 — the falsified prediction, and why it is the most useful result here

`nudox-search-only` lost `memchr/public-surface`: strict 1.00 → 0.80, three
facts absent — `memchr_iter`, `memchr2_iter`, `memchr3_iter`.

I expected R1 not to disturb that policy, and I was right about R1. R2 did it.
But the interesting part is *how*. Each of those three names appeared in the
pre-R2 response **only inside another symbol's documentation prose**:

```
doc: ``An iterator over all occurrences of a single byte in a haystack.␊␊This
     iterator implements `DoubleEndedIterator`, which means it can also be…
```

Counting mentions in the pre-R2 payload: `memchr_iter` appeared in **1 doc line
and 0 identity lines**; `memchr2_iter` in 2 and 0; `memchr3_iter` in 1 and 0.
The symbols themselves were **never returned as hits** — in either run.

So `search-only` never actually found them. Its pass was riding on the names
being mentioned in unrelated prose that happened to be included because
documentation was uncapped. R2 removed the prose and the coincidence went with
it.

**That pass was spurious, and 0.80 is the more honest number.** This is the
same shape as every other finding in this repo: the measurement was counting
something other than what it claimed to. The `matched` column cannot catch this
one, because for `search-only` the search listing *is* the answer — provenance
within a response cannot distinguish "returned this symbol" from "mentioned its
name in passing".

That is a gap in the scoring worth naming: **strict recall credits a fact found
anywhere in the returned text, including inside another symbol's prose.**

## What this does not settle

* Two packages, five scenarios attempted, four declined. Not significant.
* `iter-return-type` still fails on two distractors that are in the search
  listing and not in the answer (`2→0`), which is the scoring-rule question
  `docs/DESIGN.md` leaves open — neither R1 nor R2 addresses it.
* The `nudox-full` figure is dominated by one 21,276-token graph response, so
  the per-scenario mean says more about the overview scenario than about the
  changes measured here.
