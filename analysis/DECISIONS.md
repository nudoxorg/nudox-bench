# Decision log

Why each change was made, and whether the reasoning still holds. Newest last.
Status: **HOLDS** (still true), **INVALID** (superseded by facts), **OPEN** (unresolved).

---

## D1 — Score deterministically, never by judgement · HOLDS
Every question carries `truth.all_of` (required), `any_of` (gate), `none_of` (distractors).
Pass = full strict recall + gate + zero distractors. No model grades any run.
**Why:** a benchmark whose scoring is itself a model is unfalsifiable. Anyone can re-run
these and get the same numbers, which is the only reason the results are arguable at all.

## D2 — Strict vs loose recall are reported separately · HOLDS
Strict wants a qualified path; loose accepts a bare identifier.
**Why:** loose recall silently rewards a tool for emitting a common word. The gap between
the two columns is the honest measure of whether a tool found the *symbol* or just the *string*.

## D3 — Retracted the `ParseBytes` claim · HOLDS
DESIGN.md said ripgrep matched `ParseBytes` via `\bParse\s*\(`. It cannot — `ParseBytes(`
has `Bytes` between. It was a `-B 12` context line from `uuid_test.go`.
**Why:** it was already published. A wrong mechanism in a document arguing for rigour
discredits every other claim in it. Corrected the artifact and the doc.

## D4 — ripgrep gets exclusion globs, but never at the root · HOLDS
`node_modules` etc. are excluded; `exclude_args(root)` drops an exclusion when the root is
*inside* that directory.
**Why:** the blanket glob swallowed commander's own root — ripgrep scored 0% on 149 tokens
and would have looked absurdly bad for a reason that was my bug, not its behaviour.
Guarded by `an_exclusion_never_swallows_the_root`.

## D5 — Fixed the graph adapter rather than reporting its zeros · HOLDS
It never executed: wrong args key, wrong root, a non-existent edge.
**Why:** publishing a 0% for a policy that never ran is fabrication. Learned in passing that
`occurrencesOf` is forward, `usages` lands on Symbol with no `owner` hop, and the engine
rejects unused args.

## D6 — `pick_hit` matches case-sensitively first · HOLDS
Tier order: exact case, then folded, then suffix, first-wins.
**Why:** `Memchr3` (struct) was beating `memchr3` (function), so `read` returned the wrong
symbol and primary-strict read 0.00 against strict 1.00. Also switched off `max_by_key`,
which returns the *last* maximum and so followed an arbitrary trailing hit on a tie.

## D7 — Added `matched` / evidence provenance · HOLDS
`Attempt::primary_context`, `primary_strict_recall`, `primary_distractors`, `Evidence::{Answer,Navigation}`.
**Why:** without it, a tool that dumps a large context gets credit for recall it never
directed the reader to. Separating the answer from the looking-around is what makes the
token-cost comparison mean anything.

## D8 — Results into SQLite + a shipped htmx viewer · HOLDS
`scripts/bench_db.py`, `scripts/bench_web.py`, vendored `htmx.min.js`.
**Why:** JSON runs in directories cannot be compared across dates. Vendored, not CDN, so
the viewer works offline and the artifact stays reproducible.

## D9 — Root cause found by structured multi-agent exploration · HOLDS
Plan agent → 4 explore agents → gates G0–G4 → synthesis. Artifacts in `analysis/`.
**Mechanism:** an exact name match on a `pub(crate)` symbol scores `1.0 × 0.6 = 0.60`; a
public semantic row scores `(cos+1)/2`; the merge sorted on **score alone**, so any public
symbol with cos > 0.2 outranked the exact match. Private-by-default (0.4) puts the crossover
at cos > −0.2, i.e. always.
**Why gated:** explore agents left alone produce plausible narrative. Each claim had to cite
a file and line before synthesis could use it.

## D10 — Implemented R1 (exact-match tier) and R2 (doc cap) · HOLDS as a finding
R1: `tier` on `HitRow`, comparator `tier → score → key`. R2: `doc_summary` caps previews.
**Measured:** memchr/callers 17% → pass, strict 1.00; cost 12,955 → 8,891 tokens.
**Also measured, and against my own prediction:** search-only fell 3/7 → 2/7, because R2
removed doc prose that had been supplying *spurious* recall. Recorded rather than buried —
it means one earlier pass was never real.

## D11 — The patch is INVALID; the finding is not · INVALID (patch) / HOLDS (finding)
The `origin/canonical` cached on this machine was **276 commits stale** (`70f1cbb6` vs the
real `2f82d74b`), and had never once been fetched. On real canonical `workspace/` is gone;
`HitRow`, `collect_name_hits` and `nudox-engine` return **zero matches**. The search plane
was rewritten onto Qdrant.
**Consequence:** every line of R1/R2 patches files that no longer exist. Pushed to
`exact-match-tier-and-doc-cap` for the record; it must not be merged.
**What carries over:** the *ranking policy* defect. A vector-backed rewrite reproduces it by
default unless its merge step deliberately ranks exact name matches above vector hits.
**Lesson:** I verified nine claims against `origin/canonical` and reported them as holding on
canonical. They held against a ref I never confirmed was current. Fetch before trusting a
remote-tracking ref, and treat an empty reflog as the warning it is.

## D12 — Accepted a changed SSH host key · OPEN
`[dev.nudox.org]:2222` offered `SHA256:6Rk/Mvcbm8LXLQ0BUckwM7BOLVeWVZx5mSlEpGNut/s`,
not the recorded key. I refused to change it unprompted, surfaced it, and only acted once
told to push. Backup: `~/.ssh/known_hosts.bak.20260903-201639`.
**Open:** the fingerprint is still unverified by a human. Restore the backup if it is wrong.

## D13 — Restarted against real canonical; the benchmark cannot run there · HOLDS
Checked out `2f82d74b` fresh and traced the search path. On canonical,
`ApplicationInput::{Search,Graph,Vector}` every one returns **`unavailable`**, and `interface-core`
depends on no index crate. `server-index-retrieval` has **zero consumers**; `RetrievalResult`
returns one variant per call, so no code fuses families.
**Why it matters:** the index plane is built but unwired. nudox-bench cannot execute a single
question against canonical — not "needs a new adapter", there is no reachable search. Re-measurement
waits on the plane being connected.
**Rejected:** porting R1/R2. The comparator they fix does not exist; porting would mean inventing a
fusion site and then "fixing" it, which measures nothing.

## D14 — Shipped the finding as a law, not a patch · HOLDS
`analysis/CROSS-FAMILY-RANKING.md` here; a 9-line change to
`.opencode/skills/build-greenfield-index-plane/SKILL.md` on Backend branch
`ranking-on-new-index-plane` (one law, one do/don't, one proof gate).
**Why:** the skill states legacy "can supply counterexamples and candidate product ideas only" —
which is exactly the standing this evidence has. No cross-family ordering law exists anywhere in the
skills or plan (verified; every "tier" hit is cache/storage). `Score<Recipe>` protects *within* a
family; `RankedKey` carries no family, so the legacy category error stays expressible.
**The proof gate is the load-bearing part:** a two-family test where precedence and score agree is
satisfied by a comparator that ignores family. Legacy had passing ranking tests the whole time.
**Honest limit:** the evidence is from the deleted engine. It is a prior about a failure mode, not a
measurement of the new plane, and the document says so rather than borrowing authority it lacks.

## D15 — Ran the harness against canonical anyway, and shipped the adapter · HOLDS
Built `interface-cli` from `2f82d74b` and drove it. Health reports `index`, `graph`, `vector`
all `unavailable`; every retrieval action returns `dependency_unavailable` with `emitted: 0`.
Empirical, from a running binary — not the grep argument D13 rested on.
**Built:** `src/adapter/canonical.rs` + `configs/canonical.toml`. It speaks the application
protocol over argv, because canonical shares neither the transport nor the tool names of the old
MCP engine — pointing the `nudox` adapter at it would have failed at connect and read as an outage.
**Result:** `results/2026-09-03-canonical/` — nudox-canonical 0 attempted / 10 unsupported,
ripgrep 5/6 (83%). Recorded as **unsupported, never a zero score**; the harness already refuses to
average "cannot be asked" with "answered wrongly", and this is the case that vocabulary was for.
**Why ship an adapter that returns nothing:** it needs no change when the plane is wired, and the
ripgrep column is a same-day baseline for that comparison to be read against.

## D16 — Fixed three panics, and made tool errors visible · HOLDS
**Panics.** Every size limit here is a byte count applied to source text, which is full of
multi-byte characters. Three sites cut at a raw byte offset — `truth::contains_word` advancing one
byte after a non-word-bounded hit, the ripgrep adapter slicing *and* truncating at
`MAX_OUTPUT_BYTES`, and `mcp::truncate` — and all three panic mid-run when the cut splits a
character. Each was verified as an actual panic against the real code path before being fixed, not
argued from reading. `text::floor_char_boundary` is `str`'s own unstable method, spelled out.

**Errors.** `summarize()` had counted `errored` from the start and no table ever rendered it, so an
adapter whose endpoint dropped was indistinguishable from one that answered wrongly: same pass rate,
same recall. That is precisely the conflation the `unsupported` and `unscored` columns exist to
prevent, leaking back through the one case nobody displayed. Added the column, and an `error` column
to the results DB.

**What it found immediately:** `run-2026-08-30-141350` recorded four rust-analyzer outcomes as
`passed=0, strict=0.00` when the tool had failed at `LSP initialize` — it never started, and the run
reported it as answering wrongly. **Checked, and it is not one of the six published runs**, so no
published claim rests on it. Verified rather than assumed.

## D17 — `nbench verify` and a DB stability view · HOLDS
`verify` re-runs a config N times; a moved verdict exits non-zero, a moved cost is reported as a
spread relative to its own floor. Recall that moves while the verdict holds counts as a failure to
reproduce — the verdict survived on where the threshold sat.
`bench_db.py stability` answers the same question retroactively over runs already recorded, but
**always exits 0 and refuses to say why anything moved**: across repeats of one build it is
flakiness, across a code change it is a changelog, and the query cannot tell. The default set is the
last three runs, which usually spans changes, so asserting "flaky" would be wrong most of the time.
**First measurement:** `configs/canonical.toml`, 3 runs, 20 cells, 100% reproduced.
