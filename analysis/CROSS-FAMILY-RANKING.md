# Cross-family ranking: a legacy counterexample

**Role.** `build-greenfield-index-plane` says legacy `workspace/index` "can supply counterexamples
and candidate product ideas only." This is a counterexample, with the measurement that makes it one.

**Claim.** The greenfield plane keeps exact, lexical, relation, usage, and vector as distinct typed
families, but names **no law for ordering results of different families against each other**. The
legacy engine had the same silence, and it produced a reproducible correctness failure — not a
relevance nicety. The plane will reproduce it the moment a single query touches two families,
because nothing in the current design makes that ordering inexpressible.

---

## What was measured (legacy engine)

Question: *where is `memchr3` called from?* The engine returned `pub struct Memchr3<'h>` — the
wrong entity, same name, different family path.

| | before | after tiering |
|---|---|---|
| strict recall | 0.00 | 1.00 |
| question outcome | fail (17%) | pass |
| tokens for the answer | 12,955 | 8,891 |

The fix was one comparator key. Nothing else changed.

## Mechanism

Exact and vector produced rows into one list ordered by a single `f32`:

```
exact name match on a pub(crate) symbol :  1.0 × 0.6 (visibility) = 0.60
public semantic row                     :  (cos + 1) / 2
```

The merge sorted on **score alone**. So any public symbol with `cos > 0.2` outranked a *literal
name match*. With private-visibility weighting at 0.4 the crossover moves to `cos > −0.2` — true
for every row a cosine index will ever return. The exact match could not win. Not "usually lost":
could not win.

The defect is not the weights. It is that **two families' scores were made comparable at all.**
`0.60` from the exact family and `0.60` from the vector family denote nothing in common — one is a
visibility discount on a certainty, the other is a rescaled angle. Placing them in one `sort_by` is
a category error that the type system was not asked to prevent.

## Why the greenfield design does not yet prevent it

Three current facts, each verified on canonical `2f82d74b`:

1. **The law list is silent.** `## Index-specific laws` governs snapshots, pruning, borrowing,
   partial terminals, placement — nothing about ordering across families.
2. **The prescribed key permits it.** The Do/don't already improves on `(f32, Uuid)`:
   ```
   DO: RankedKey { score: Score<Recipe>, document: DocumentKey }
   ```
   `Score<Recipe>` is phantom-tagged by recipe, which is real protection *within* a family. But a
   `RankedKey` carries no family, so two keys from different families still compare — and if
   `Recipe` differs, the fix is a cast, which is exactly what a tired implementer reaches for.
3. **The fusion site does not exist yet.** `server-index-retrieval` has **zero consumers**;
   `RetrievalResult` returns one variant per call; `ApplicationInput::{Search,Graph,Vector}` all
   return `unavailable`. Nothing has been written wrong. That is the entire reason to write this now.

## Proposed law

> Results of different families are never ordered by a shared scalar. A query that touches more
> than one family declares a **family precedence** and orders within each family only; a score
> from one family never compares against a score from another. Where precedence is
> query-dependent, it is a typed input to the query, never a constant in a comparator.

## Proposed Do / don't

```text
DON'T: candidates.sort_by(|a, b| b.score.total_cmp(&a.score))   // exact and vector in one list
DO:    FamilyRanked { family: Family, within: RankedKey }        // precedence, then within-family order
```

## Proposed proof gate

> For any query spanning two families, evidence includes the case where the lower-precedence family
> holds the **numerically higher** score. A test in which precedence and score agree proves nothing —
> it passes under a comparator that ignores family entirely.

That last sentence is the one that matters. The legacy engine had passing ranking tests throughout.
They agreed with score, so they were satisfied by the broken comparator.

## Falsifier

This is wrong if any of these hold, and each is cheap to check:

- a single query never touches two families — then the law is vacuous, and should be *stated* as
  vacuous so the assumption is visible when it stops being true;
- family precedence is genuinely score-derived, with a calibration that makes the two scales
  commensurable — then the law becomes "publish the calibration and its domain";
- `Score<Recipe>` is already uninstantiable across recipes at the fusion site — then the type
  system covers it and only the proof gate is needed.

## Provenance and limits

The measurement is from the **legacy** engine (`nudox-engine`, deleted in the restructure). It is a
prior about a failure mode, not a measurement of the new plane, and I cannot make it one: on
canonical, search returns `unavailable`, so nudox-bench cannot execute a single question against it.
Re-measurement is possible only once the plane is wired to `interface-core`.

Treating this as a measured property of the new plane would be exactly the error recorded as D11 in
[DECISIONS.md](DECISIONS.md) — checking claims against a ref I had not confirmed was current.
