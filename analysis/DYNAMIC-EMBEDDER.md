# DYNAMIC EMBEDDER — paying for semantic search only where it pays back

Continues `analysis/MEASUREMENT.md`. That document ended with three candidate
fixes for the ranking defect and a falsifiable prediction. Two of the three
were built and measured; both are null, and the reason they are null is the
useful part. This document records what replaced them.

**Status:** the engine change described in §5 is Backend commit
`6c63b655` plus the follow-up in §6, on branch `exact-match-tier-and-doc-cap`.
Measurements are against the benchmark corpus (`cargo:memchr 2.8.3`,
`npm:commander`), exact `cl100k_base` counts.

---

## 0. Three corrections to the method, before any result

All three changed numbers that were already written down, so they come first.

**The endpoint reports ready before it is ready.** It prints
`NUDOX_MCP_URL=` — and `semantic search: on` — as soon as the packages are
indexed, but the embeddings land later. In between, a search returns
structural rows only, and nothing in the response says so. A benchmark started
on that banner measures a *different system*: 1.6 GB resident instead of 5.6,
23 rows instead of 50, and results that reproduce the embedder-off numbers
exactly while the log claims the embedder is on.

Two of the first three arms run for this document were invalidated that way,
and were caught only because two arms with different code produced
byte-identical output. Every run since gates on the index itself — query
`memchr` at limit 50 until the row count crosses 30 — not on the banner.
Script: `$CLAUDE_JOB_DIR/tmp/arms/probe_sem.py`.

**The pass column divides by each adapter's own denominator.** `report.md`
scores an adapter over the scenarios *it* attempted, so `nudox-full 4/5` and
`ripgrep 6/7` are fractions of different sets, and an adapter can raise its
rate by declining what it would have failed. Every competitive claim below is
computed over the intersection — the scenarios every adapter both attempted
and could be scored on. Script: `head2head.py`.

**`${NODE_MODULES}` must be exported.** Unset, `ts/commander/public-surface`
declines for ripgrep, silently dropping the one TypeScript scenario — which is
also the only scenario where the embedder earns its keep (§2). The run that
`MEASUREMENT.md` reports was made with it unset.

---

## 1. Where the benchmark actually stands

Five scenarios, every adapter scored on all five, embedder on unless noted:

| adapter | pass | strict | tok/scenario | tok/fact |
|---|---:|---:|---:|---:|
| **nudox-full** (R1+R2) | **5/5** | **1.00** | 5,782 | 781 |
| nudox-graph-first | 5/5 | 1.00 | 6,030 | 815 |
| **ripgrep** | 4/5 | 0.93 | 9,599 | 1,371 |
| nudox-search-only | 3/5 | 0.72 | 1,703 | 341 |
| nudox-oneshot | 2/5 | 0.43 | 121 | 76 |
| *nudox-full, embedder off* | *5/5* | *1.00* | *4,446* | *601* |
| *rust-analyzer* (full corpus) | *1/4* | *0.35* | *301* | *172* |

Nudox already beats the ripgrep floor on correctness **and** cost: same
scenarios, one more pass, a higher strict recall, and 40% fewer tokens. That
is a stronger position than any earlier run showed, and most of it is R1 and
R2 landing.

What is left is the gap between the last two Nudox rows: **the embedder costs
1,336 tok/scenario and buys nothing on this adapter.** Both are 5/5 at strict
1.00.

---

## 2. When is the embedder actually useful?

`embedder_utility.py` compares two runs of the same scenarios fact by fact,
because the pass column cannot answer this: a scenario can gain recall and
still fail, or pass on text the listing happened to carry. What matters is
which *expected facts* move tier, and what the move cost.

Over 34 adapter×scenario pairs scored both ways:

| adapter | scenarios | facts gained | facts lost | Δ tokens |
|---|---:|---:|---:|---:|
| nudox-search-only | 7 | **+9** | 0 | +6,562 |
| nudox-full | 5 | 0 | 0 | +6,678 |
| nudox-graph-first | 5 | 0 | 0 | +6,727 |
| nudox-keys | 5 | 0 | 0 | +6,678 |
| nudox-oneshot | 5 | 0 | 0 | −147 |
| ripgrep | 7 | 0 | 0 | 0 |

**The semantic section changes the answer on exactly one pair out of 34** —
`ts/commander/public-surface` under `nudox-search-only`, which goes fail→PASS
on nine facts at 729 tok/fact. That is *cheaper per fact than ripgrep manages
anywhere* (1,371–1,882), so the section is not merely defensible there; it is
the best instrument on the board for that query.

On the other 33 it moves nothing and charges ~6,700 tokens.

So the embedder is not bad. It is **misapplied**: it earns its place exactly
where the structural path comes up short — a TypeScript package whose exports
a `starts_with` prefix scan cannot enumerate — and is pure overhead where the
corpus already answers.

That is a condition, which means it can be gated.

---

## 3. Why the two scoring fixes were null

`MEASUREMENT.md` §5 proposed N1 (put the two relevance scales in comparable
units) and N2 (a middle tier for strong prefix matches). Both were built and
measured as separate arms against a fresh-process control.

| arm | pass (nudox-full) | tok/scenario | facts moved vs control |
|---|---|---:|---:|
| control — R1+R2 | 5/5 | 5,782 | — |
| **N1** — `cosine.clamp(0.0, 1.0)` | 5/5 | 5,778 | **0** |
| **N2** — `TIER_NAME_PREFIX` at 0.5 | 5/5 | 5,778 | **0** |

Both are no-ops, and the reason resolves **H1**, which had been open since the
proposal and which nobody had plotted.

N1 replaces `(cos + 1) / 2` with the identity, so a semantic row must clear a
prefix row's own relevance — 0.545 for `memchr_iter` under query `memchr` — to
outrank it. 43 semantic rows made the page under *both* maps. Therefore all 43
have cosine ≥ 0.545. Against an in-domain query, in a package where every
symbol scans bytes for a byte, **the cosine distribution is concentrated high**
— so no monotone rescaling of cosine can reorder the section against prefix
matches. A similarity floor is the same instrument and fails the same way.

The second reason is more basic, and it is what §5 is built on: **the sections
are not sized comparably.** The semantic arm returns `limit` rows *by
construction*; the name arm returns however many symbols happen to share the
query's prefix. One is sized by the request, the other by the corpus. After the
merge, the request-sized one wins on volume no matter how the scores compare —
which is why raising `limit` from 50 to 200 pushes structural rows *down*, from
7 to 3, instead of recovering them. No comparator can fix a section-size
mismatch, because by the time the comparator runs, the ratio is already set.

---

## 4. What R1 already fixed, contrary to §4 of MEASUREMENT.md

Worth stating plainly because it retires an open defect. On the warmed,
`NODE_MODULES`-exported control run, `rust/memchr/public-surface` under
`nudox-search-only` scores `memchr_iter`, `memchr2_iter` and `memchr3_iter` as
**strict** — the three facts `MEASUREMENT.md` §4 reported as displaced. They
are no longer displaced.

The three still absent are `memrchr_iter`, `memrchr2_iter`, `memrchr3_iter`,
and those are a different problem: `memrchr` does not start with `memchr`, so
`NameIndex::prefix` structurally cannot reach them. They are exactly the rows
the semantic section exists to find — and it is not finding them, because it
spends its budget on rows the name index already had.

Which is the same defect, seen from the other side: the section that could
answer is crowded out by its own redundant output.

---

## 5. The change: the semantic section fills the page, it does not compete for it

One rule, at the merge in `mcp/tools/mod.rs`:

```rust
let structural = deduped.iter().filter(|(k, _)| !semantic_keys.contains(k)).count();
let semantic_budget = fetch_limit.saturating_sub(structural);
let mut semantic_kept = 0usize;
deduped.retain(|(key, _)| {
    if !semantic_keys.contains(key) { return true; }
    semantic_kept += 1;
    semantic_kept <= semantic_budget
});
```

Applied after the tier/score sort, so the semantic rows that survive are the
best-scoring ones.

Why this and not a better comparator:

* **Every structural row keeps its place.** A symbol whose *name* matched what
  the caller typed is the one row a search has no business dropping, and this
  makes that structural rather than a matter of which scale won.
* **The semantic arm is offered exactly the space the corpus did not fill.**
  On a query the name index answers completely, the budget is zero and the page
  costs what a structural search costs. On a query it cannot answer — the
  TypeScript surface in §2 — the budget is the whole page and nothing changes.
* **It gates the embedder by demand, with no threshold.** N2 needed a
  magic number nobody had a distribution for. This needs none: the corpus
  states the demand by how much of the page it could fill.
* **It targets the cost where the cost is.** `attach_addresses` fetches
  documentation for semantic rows *only*, so a semantic row is materially more
  expensive to render than a structural one. Capping semantic rows caps the
  `doc:` lines, which §2 of `MEASUREMENT.md` measured at 56% of the search
  surface before R2.

The page is the same size either way. What changes is who is displaced when
the two sections disagree.

---

## 6. Measured — and what is not yet measured

Four arms were run on 2026-09-02 against a fresh endpoint each: a control on
the R1/R2 binary, then N1, N2 and N4. **All four are contaminated and none of
them measures the warm system**, for the reason in §0: 23 of 30 search steps in
every arm ran with `~sem:building(0/1)`.

They are still worth reporting, because they are contaminated *identically* and
so remain comparable to one another:

| arm | pass (nudox-full) | tok/scenario | facts moved vs control |
|---|---|---:|---:|
| control — R1+R2 | 5/5 | 5,782 | — |
| N1 — `cosine.clamp(0.0, 1.0)` | 5/5 | 5,778 | 0 |
| N2 — `TIER_NAME_PREFIX` at 0.5 | 5/5 | 5,778 | 0 |
| N4 — semantic budget | 5/5 | 5,782 | 0 |

N1 and N2 are null for the reasons in §3, and those reasons do not depend on
warmth — the cosine argument comes from the *warm* 09-01 payloads. N4's null is
different and says nothing yet: with the semantic section mostly absent there
was no budget for it to apply.

The mechanism *was* verified directly against a live endpoint. On a scope whose
index was ready, query `memchr` at limit 50 returned 42 rows and **zero
documentation lines** — documentation attaches only to semantic rows, so the
semantic section had been squeezed out entirely, the exact inverse of the
43-semantic/7-structural split that motivated the change.

**One caveat that the design should carry forward.** The benchmark's own search
call is `{"limit":200,"packages":["cargo:memchr"],"query":"memchr"}`, and the
structural section supplies 21 rows against it. `budget = 200 − 21 = 179`, so on
*this* call the rule barely trims anything. The budget bites when a caller asks
for a page the corpus can fill, and the adapters here ask for pages far larger
than that. Whether the rule is worth its complexity therefore depends on a
question nobody has asked yet: **what `limit` should an adapter request?** A
page of 200 rows is itself the cost driver, and the union scoring rule makes
every extra row a potential distractor — `iter-return-type` already fails at
100% recall on exactly that.

### Validity, going forward

`head2head.py` now prints the semantic state of every search step and marks a
run whose steps disagree as `** MIXED — not one system **`. Re-checked against
the published record, which holds up: `2026-09-01` and `2026-08-31-embedder-ab`
are ready on all 30 steps, `2026-08-31-graph-fixed` is
`unavailable(NoModelConfigured)` on all 30.

Warm-up is slow and uneven, which is why the naive gate missed it: measured
2026-09-02, `npm:commander` reached ready in under two minutes while
`cargo:memchr` was still `building(0/1)` past six. Any A/B on this corpus must
budget for that, and must poll the scope it intends to measure.

---

## 7. What this does not settle

* **Two packages, five scenarios.** One scenario flipping moves a pass column
  by 20 points. These are point observations about known scenarios.
* **The `iter-return-type` distractor failure** is the union scoring rule, not
  a ranking defect, and no engine change addresses it.
* **`rust-analyzer` declines most of the corpus** (5 of 9 scenarios), so its
  row is a floor for the Rust scenarios only and not a competitive result.
* **Whether the budget rule holds on a corpus the name index answers badly.**
  Every package here is one a prefix scan does well on. A package with terse or
  abbreviated identifiers would leave a larger budget on every query, and the
  rule has not been tested there.
