# What the embedder actually buys

`run-2026-08-31-193200` against
[`../2026-08-31-graph-fixed`](../2026-08-31-graph-fixed) — **same two
packages, same code, same scenarios, one variable**: semantic search on.

This is the first controlled measurement of the embedder in this repo. Every
previous Nudox number was structural-only ranking, because the model was never
installed (before 2026-08-30) or the machine could not hold it.

## The result

| adapter | pass | strict | matched | distractors | tok/scenario |
|---|---|---|---|---|---|
| nudox-full | 5/5 → **3/5** | 1.00 → 0.83 | 1.00 → **0.63** | 0 → **2→0** | 4,464 → **12,955** |
| nudox-graph-first | 5/5 → **4/5** | 1.00 → **1.00** | 1.00 → 0.80 | 0 → 2→0 | 4,692 → 11,529 |
| nudox-search-only | 2/7 → **3/7** | 0.39 → **0.57** | 0.39 → 0.57 | 0 → 2 | 292 → 6,544 |
| nudox-oneshot | 2/5 → **1/5** | 0.43 → 0.43 | 0.43 → 0.43 | 0 → 2 | 151 → **4,114** |
| ripgrep | 6/7 | 0.95 | 0.57 | 0 | 8,447 |

**Turning the embedder on made Nudox score worse on this scenario set, and cost
three to twenty-seven times more tokens.** That is not the result anyone
expected, and it is worth being precise about why, because two of the three
causes are not the model's fault.

## 1. It helps exactly where search is the answer

`search-only` is the one policy that improved: 2/7 → 3/7, strict 0.39 → 0.57.
That is the honest positive result. Where the search listing *is* the
deliverable, semantic ranking finds things prefix matching cannot.

`ts/commander/public-surface` and `rust/memchr/public-surface` both still pass,
and `memchr/public-surface` passes for `search-only` now (18,049 tokens) where
structurally it reached only 60%.

## 2. It costs three to twenty-seven times more

| policy | structural | semantic | factor |
|---|---:|---:|---:|
| nudox-oneshot | 151 | 4,114 | **27×** |
| nudox-search-only | 292 | 6,544 | 22× |
| nudox-full | 4,464 | 12,955 | 2.9× |

A single `search` response went from 266 to 7,283 tokens on
`memchr/iter-return-type`. Semantic ranking returns neighbours, and neighbours
are not free. At 12,955 tokens per scenario, `nudox-full` is now **more
expensive than ripgrep** (8,447) — reversing the one claim this benchmark had
most confidently established.

## 3. It trips distractor sets, and every distractor is in the navigation

This is the finding that matters.

`rust/memchr/iter-return-type` scores **100% recall and fails**, for every
Nudox policy, because `Memchr2` and `Memchr3` appear in the output. Both are
real, related symbols; semantic search returned them because returning related
symbols is its entire job.

The provenance column says where they came from: **`2→0`** — two distractors in
what the adapter surfaced, **zero in its answer**. The `read` that answered the
question contained neither. They came from the `search` listing used to *find*
the symbol.

So under the current rule — score the union of everything surfaced — **the
scoring is actively hostile to the feature under test**. A semantic index is
penalised precisely for doing what it is for. `docs/DESIGN.md` left the
question of whether `passed` should follow the union or the answer deliberately
open; this run turns it from a design nicety into the thing that decides the
headline number.

Nothing here settles it. Both readings remain defensible — an agent that reads
a distractor can be misled by it — and changing `passed` would invalidate
comparison with every run already recorded. But the cost of leaving it open is
now visible: `nudox-full` reads 3/5 on a union rule and 5/5 on an answer rule,
from identical bytes.

## 4. Semantic ranking can bury an exact name match

`rust/memchr/callers-search-slice-with-raw` passes structurally (1,602 tokens)
and fails here (17%, 9,882). Searching for `search_slice_with_raw` returned
fifty hits ranked by similarity, topped by `rabinkarp::find_raw` — and **the
exact match was not among them**.

That is a Nudox behaviour worth reporting on its own: a query that *is* a
symbol's name should surface that symbol. It also exposed a harness bug, now
fixed — with no hit matching, every candidate tied and `max_by_key` returned
the *last*, so the adapter followed an unrelated `impl` block whose `refs` were
empty. It now prefers an exact name match, then longest shared path, then the
server's own ranking. The scenario still fails, because no tie-break can find a
symbol the results do not contain.

`graph-first` is the exception that proves the point: **strict 1.00, unchanged**,
because its answer comes from `graph`, which does not depend on search ranking
at all. It is the only policy semantic ranking did not disturb.

## Addendum — how much of this was the harness's fault

`run-2026-08-31-202901`, same endpoint configuration, after the
case-sensitivity fix to `pick_hit`.

| | before the fix | after |
|---|---:|---:|
| `nudox-full` pass | 3/5 | 3/5 |
| `nudox-full` strict | 0.83 | 0.83 |
| `nudox-full` **matched** | **0.63** | **0.83** |

**The pass rate did not move and that is the point.** What moved is `matched`
— strict recall counting only the adapter's *answer* rather than everything it
surfaced. On `rust/memchr/signature-memchr3`:

| | before | after |
|---|---|---|
| `read` request | `cargo:memchr::Memchr3[record]` | `cargo:memchr::memchr3[function]` |
| `read` returned | `pub struct Memchr3<'h>` | `pub fn memchr3(needle1: u8, …) -> Option<usize>` |
| strict | 1.00 | 1.00 |
| **primary_strict** | **0.00** | **1.00** |

Before the fix the scenario passed on text the *search listing* happened to
carry, while the answer it actually fetched was the wrong declaration — a
same-named struct instead of the function. `strict` could not see that;
`matched` could, and it is the only column that did. This is the first time the
provenance split has caught a pass for the wrong reason, which is what it was
added for.

So of the regression this page reports: **one scenario was the harness's fault
and is now fixed**, and it never showed up in the headline number either way.
The remaining two failures are not the harness's:

* `memchr/callers-search-slice-with-raw` — the exact match is absent from all
  fifty semantic rows. No harness change can pick a symbol the results do not
  contain. See `analysis/PROPOSAL.md` §1.3.
* `memchr/iter-return-type` — 100% recall, failed on two distractors that are
  in the search listing and **not** in the answer (`2→0`). That is the scoring
  rule, still deliberately unchanged.

## Scope, and what it cost to get here

* Two packages (memchr, commander), five scenarios attempted, four declined.
  Nothing here is statistically significant.
* **A five-package embedded corpus does not fit in 16 GB.** It reached 8.25 GB
  after 36 minutes, still climbing, with 0.76 GB free, and was abandoned before
  the OOM killer got it. Embedding memory scales with symbols embedded and does
  not plateau.
* Embedding two packages took ~40 minutes at four cores pinned. Budget for it.
* Freeing the editor's `rust-analyzer` processes (2.3 GB) is what made any of
  this possible; the machine has no swap.
