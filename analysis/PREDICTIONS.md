# Predictions for the R1 + R2 run

Written **before** the run, so the result can falsify them. Both changes are in
`Backend` on branch `exact-match-tier-and-doc-cap`; the endpoint is the same
two-package, embedder-on configuration as
[`results/2026-08-31-embedder-ab`](../results/2026-08-31-embedder-ab).

Baseline to beat (embedder on, before R1/R2):

| adapter | pass | strict | matched | tok/scenario |
|---|---|---|---|---|
| nudox-full | 3/5 | 0.83 | 0.83 | 12,955 |
| nudox-graph-first | 4/5 | 1.00 | 0.80 | 11,529 |
| nudox-search-only | 3/7 | 0.57 | 0.57 | 6,544 |
| ripgrep (control) | 6/7 | 0.95 | 0.57 | 8,447 |

## What I expect, and why

**P1 — `memchr/callers-search-slice-with-raw` passes.** It fails today at 17%
because the exact match is absent from all fifty rows. R1 gives it
`TIER_EXACT_NAME`, so it sorts first, `pick_hit` selects it, and `refs` answers
about the right symbol. This is the scenario R1 was written for.

**P2 — `memchr/iter-return-type` still fails.** The proposal's R1 row predicts
`nudox-full` reaches 5/5; **I predict 4/5**, and I think the proposal is wrong
here. That scenario fails on two distractors, `Memchr2` and `Memchr3`, which
arrive in the *search listing*. R1 reorders that listing; it does not shorten
it. Both distractors are ordinary semantic rows well inside a 50-row page, so
they will still be present and the scenario will still fail. R2 shortens the
`doc:` text, not the identity lines the distractors match on.

Falsified if it passes — which would mean either the distractors were ranked
past position 50 by the reordering, or they were never in the listing for the
reason I think.

**P3 — `nudox-full` reaches 4/5, strict ~0.97.** From P1 + P2.

**P4 — tok/scenario falls to roughly 3,000–5,000.** E4 measured ~181 tok/row,
of which ~135–155 is the uncapped `doc:` line, leaving ~36 for address +
signature + marker. Capping at the first paragraph or 200 characters (~50
tokens) puts a row near 60–86 tokens, so a 50-row response lands near
3,000–4,300 against the measured 9,065. Across a scenario that is
**12,955 → ~4,000–6,000**, i.e. comfortably back under ripgrep's 8,447.

**P5 — `nudox-search-only` keeps its gain.** It is 3/7 today, up from 2/7
without the embedder. R1 only promotes exact name matches, and that policy's
gain came from queries where no exact match exists, so nothing should demote
those rows. If this drops to 2/7, R1 is broader than intended.

**P6 — `ripgrep` is unchanged at 6/7, 8,447.** It never calls Nudox. Any
movement here means the run differs from the baseline in some way I have not
accounted for, and invalidates the comparison.

## What would make me discard the whole run

* `ripgrep` moving (P6) — the control failed.
* `~sem:building` in any response — the index was not settled, as happened
  twice before; the numbers would then measure a warm-up race.
* `nudox-graph-first` losing strict 1.00 — its answers come from `graph`, which
  neither change touches.
