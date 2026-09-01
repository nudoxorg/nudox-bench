# MEASUREMENT — what the proposal's Backend fixes actually did

Continues `analysis/PROPOSAL.md`. Where the proposal reasoned from source to a
predicted effect, this document reports the measured one, and closes four of the
open hypotheses. The run is
[`results/2026-09-01-exact-match-tier`](../results/2026-09-01-exact-match-tier);
its README carries the tables. This file carries the method, the code review and
what to do next.

**Status of the code under measurement:** Backend branch
`exact-match-tier-and-doc-cap`, **uncommitted working-tree edits** to six files.
The running endpoint was built from that tree. Nothing here is a statement about
a merged Backend.

---

## 0. Method — two things that made this cheap

Both matter for the next session, because both remove a constraint the
`WORKFLOW.md` §0 rules assumed was binding.

**A `cl100k_base` tokenizer, exact, with no network and no build.**
`tiktoken-rs` vendors its BPE ranks as plain files in the cargo registry
(`~/.cargo/registry/src/*/tiktoken-rs-0.6.0/assets/cl100k_base.tiktoken`), so
the harness's own token metering can be reproduced in Python. Transcribing
cl100k's pretokenizer pattern for stdlib `re` needs two substitutions —
`\p{L}` → `[^\W\d_]`, `\p{N}` → `\d` — and the easy mistake is inverting the
first branch's optional prefix, which is a *non*-letter (` find` is one piece,
not two); getting it wrong overcounts by ~40% on prose.

Validated before use: it reproduces the harness's recorded `cl100k` figure on
**all 63 captured steps, exactly**. Every token number in this document and in
the results README is therefore a measurement, not an estimate. Script:
`$CLAUDE_JOB_DIR/tmp/cl100k.py`.

**Response bodies are already on disk.** `runs/*/report.json` written with
`--keep-responses` carries every response verbatim. Questions the proposal
framed as needing an experiment — E4's per-field decomposition, H7's duplication
count, H8's re-scoring — are offline queries against files that already existed.

**And the endpoint was already up.** WORKFLOW §0 forbade *starting* one, because
the embedder costs ~5.6 GB on a 16 GB machine with no swap. A process that is
already resident costs nothing to query, and querying it settled in one call
what the proposal could only infer. Worth checking `ps` before assuming an
experiment is unaffordable.

---

## 1. The hypothesis ledger, updated

| id | question | verdict |
|---|---|---|
| **H3** | Does R1 restore the buried exact match? | **RESOLVED — yes.** `callers-search-slice-with-raw` fail 17% → **pass 100%**, for `nudox-full` and `nudox-keys`. Rank 1 at every page depth. |
| **H4** | Does `search-only`'s semantic gain survive a ranking fix? | **RESOLVED — the gain was not what it looked like.** 3/7 → 2/7, but the lost pass was carried by documentation prose, not by ranking (§3). Honest score is 2/7 either way. |
| **H7** | Is doc text duplicated across rows? | **RESOLVED — yes, 14.4% of the search surface; 54% on the worst single payload.** R2 absorbed most of it; 4.7% remains. |
| **H8** | Does R2's clip cost a pass? | **RESOLVED — yes, one, and it was a false pass.** Same class the provenance column exists to catch. |
| **E4** | Per-row render cost | **Promoted from INFERRED to measured.** 98.6 tok/row before, 48.7 after; documentation was 56.1% of the search surface and is now 11.2%. |
| **H1** | Cosine distribution / is a floor worth adding? | **Still open**, but less urgent — R1 made it unnecessary for the exact-match case. §5 gives it a sharper target: the crossover that matters now is **0.09**, not 0.2. |
| **H5/H6** | Name-section budget and pre-scoring truncation | **Still open and now known not to be the cause** of §4 — the rows are collected, then outranked after scoring. |
| **H10** | Can "identity-shaped" be decided without hindsight? | **Moot for R3/R5/R8.** R1 fixed the defect on the default path, so no harness routing is needed to pass the scenario. |

R3, R5 and R8 — the harness rows that routed around the defect — are **withdrawn
as unnecessary**. The proposal's own §3 said they must never substitute for R1;
R1 shipped and the scenario passes, so the reason to reach for them is gone.

---

## 2. Review of what landed

Both changes are careful, and in one respect better than the proposal asked for.

**R1 — the tier.** `HitRow` gains `tier: u8` (`wire/mod.rs:854-885`), `#[serde(skip)]`
and `#[schemars(skip)]` so no wire format or tool schema changes; `TIER_EXACT_NAME`
and `TIER_ORDINARY` at `:889-892`. `collect_name_hits` sets the exact tier on
`entry.key == prefix_lower` — the same condition `text_relevance` uses for its
`1.0`, rather than a float comparison against the resulting score, which is the
right call and the comment says why. The comparator moved into a named
`compare_merged_rows` doing `tier, then score, then key`. Every other producer
of a `HitRow` (`collect_signature_hits`, `collect_kind_facet_hits`,
`collect_semantic_hits`) sets `TIER_ORDINARY` explicitly with a one-line reason,
so the exhaustive-struct-literal check makes a new section declare its intent
rather than inherit a default.

**R2 — the cap.** `doc_summary` (`mcp/tools/mod.rs:2041-2069`) takes the first
paragraph, bounds it at 200 characters on a `char_indices` boundary, and prefers
the last sentence end and then the last word boundary. The proposal asked for
"first blank line or ~200 characters"; this is that, plus the codepoint-safety
and the word-boundary trim.

Two notes, neither a defect:

* When the 200-character clip lands after a sentence end, `head.rfind(". ")`
  returns the index *of* the period and `+ 1` keeps it, so the result is
  `"…sentence.…"` — an ellipsis directly after a full stop. Cosmetic only.
* The paragraph split is on `"\n\n"`, so a doc comment with `\r\n\r\n`
  paragraph breaks would be treated as one paragraph and fall to the 200-char
  bound instead of the blank line. It still gets capped; it just cuts less
  tidily. Only reachable if some producer emits unnormalised newlines.

**What the measurement adds to the review:** the two changes are cleanly
separable in the data. R1 moved correctness and R2 moved cost, and neither
touched the other's column — the result sets before and after contain the *same
547 rows and same 161 doc lines*, so R2 is provably a rendering change and not a
ranking one.

---

## 3. The one regression, and why it is not one

`nudox-search-only` on `rust/memchr/public-surface`: pass 100% → fail 80%,
losing `memchr_iter`, `memchr2_iter`, `memchr3_iter`.

Those three names are absent from every address line in **both** payloads. Before
the fix they appeared only inside a neighbouring symbol's documentation prose;
under the union rule (EVIDENCE §E.2) that counted as recall, so the scenario
passed on text the listing happened to carry rather than on symbols the tool
surfaced. This is the same failure mode as the `signature-memchr3` case the
`pick_hit` fix caught, arriving from the other direction.

The consequence for the evidence base is worth stating plainly, because it
revises a published claim: the embedder A/B page's claim 1 — *"it helps exactly
where search is the answer, 2/7 → 3/7"* — rested on this single scenario, and
the extra pass was doc prose. **The embedder's honest contribution to the pass
column on this corpus is zero, on every adapter.**

What survives, and should not be lost in the correction: semantic ranking raised
recall on that scenario from **60% to 80%**, by reaching the six `memrchr*`
symbols that a `starts_with` prefix scan structurally cannot. That is a real
capability gain that no amount of structural ranking provides. It is held back
from a real pass by §4.

---

## 4. The defect that is left

Same arithmetic as §1.3 of the proposal, one notch down the relevance scale.

`text_relevance` for a prefix match is a length ratio, so `memchr_iter` under
query `memchr` scores `6/11 = 0.545`, times public 1.0, times function 1.0 —
**0.545**. A semantic row's relevance is `(cos + 1) / 2`, so any public symbol
whose cosine clears **0.09** outranks it. In a package where nearly every symbol
is about scanning bytes for a byte, that is nearly all of them.

Measured, query `memchr` on `cargo:memchr`:

| limit | rows | semantic | structural | name matches present |
|---:|---:|---:|---:|---|
| embedder off | 23 | 0 | 23 | 6/6 |
| 50 | 50 | 43 | 7 | 3/6 |
| 200 | 200 | 197 | 3 | 3/6 |
| 500 | 500 | 497 | 3 | 3/6 |

**A deeper page makes it worse, not better.** The semantic section is sized by
`limit`; the name section is sized by how many prefix matches exist. Raising
`limit` from 50 to 200 raises the semantic share and pushes structural rows from
7 down to 3, and 500 recovers nothing. There is no page depth at which a caller
gets the name index back.

R1 scoped the exact tier narrowly on purpose — its doc comment says a weak
prefix match "has no such claim", and for a one- or two-character query that is
plainly right. The measurement shows the scoping is too tight at the other end:
a query that matches 6 of 11 characters of a symbol's name is not a weak claim,
and it loses to a cosine of 0.09.

---

## 5. What to do next, ranked

**N1 — put the two relevance scales on comparable units.** This is the
proposal's R4, and §4 strengthens the case for it considerably. Replace
`cosine_to_relevance`'s midpoint map (`search/mod.rs:577-580`) with
`cosine.max(0.0).max(f32::EPSILON)`. A public prefix row at 0.545 would then be
displaced only by a cosine above 0.545 — genuinely similar symbols — instead of
above 0.09. It needs no new concept, and R1 already protects the exact-match
case independently, which removes the double-correction worry the proposal
raised about doing R1 and R4 together.
*Risk, unchanged and now the main open question:* it demotes semantic rows
globally. The `memrchr*` discovery in §3 is exactly the kind of row it could
demote, and whether those clear 0.545 against query `memchr` is **unmeasured** —
this is H1, now with a specific number to check.
*Cost:* 3 lines plus rewriting a doc comment, then a Backend rebuild.

**N2 — a middle tier for strong prefix matches.** `TIER_EXACT_NAME = 0`,
`TIER_NAME_PREFIX = 1` for a name row whose `text_relevance` clears a stated
threshold, `TIER_ORDINARY = 2`. Bounded and predictable, and it cannot demote a
semantic row that nothing structurally matched. Needs a threshold nobody has
data for — 0.5 would admit all three lost rows and exclude a one-character
query — so it trades N1's global reordering for a magic number.

**N3 — reserve page slots for the name section.** Take the top *k* structural
rows before the score merge. Most predictable of the three, least principled;
`k` is another magic number and it can push a semantic row off a page where
nothing structural was worth showing.

**Recommendation: N1, measured against N2 as separate arms.** Both are cheap to
implement; the expensive part is one Backend rebuild each, plus a benchmark run
that is now ~90 seconds against a resident endpoint.

**The test is already written and its answer is predicted.** The three facts
`nudox-search-only` is missing on `rust/memchr/public-surface` are exactly the
three displaced rows, so the arm that works takes that scenario to 15/15 and a
pass that is *not* carried by doc prose — the first one. If a change moves the
pass column without moving those three facts from `absent` to `strict`, it
worked for some other reason and should be distrusted.

**Do not re-measure the ripgrep comparison without setting `${NODE_MODULES}`.**
It silently drops `ts/commander/public-surface` from ripgrep's scenario set and
makes its column incomparable with every earlier run.

---

## 6. Considered and dropped

* **A similarity floor as the ranking fix** (proposal H1). §4 shows the
  threshold that matters for prefix rows is 0.09, which is so close to
  "everything" that a floor set there removes nothing, and a floor set high
  enough to matter is doing N1's job with a worse instrument. A floor remains a
  legitimate *cost* lever and nothing more, which is what PROPOSAL §1.5 said.
* **R3 / R5 / R8** — the harness routing rows. Withdrawn: R1 fixed the default
  path, so there is no longer a defect to route around, and H10's overfitting
  objection never had to be answered.
* **Deduplicating documentation** (H7). Was worth 14.4% before R2 and is worth
  4.7% now. Real, but below the noise of a five-scenario corpus.
* **Raising `limit` to recover the missing rows.** Measured to make it worse
  (§4).
