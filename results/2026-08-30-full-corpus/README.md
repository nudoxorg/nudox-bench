# Full-corpus run — `run-2026-08-30-225507`

The first run with **nothing switched off**: semantic search on, all five
packages loaded across four languages, and every adapter fix from the payload
audit in place.

Compare against [`../2026-08-30/`](../2026-08-30), which was structural-only
ranking over three of five packages.

## What changed since the previous run

| | 2026-08-30 (degraded) | this run |
|---|---|---|
| semantic search | `~sem:unavailable(NoModelConfigured)` on every row | **on** |
| packages loaded | 3 of 5 | **5 of 5** |
| scenarios nudox-full attempted | 6 (3 declined) | **8 (0 declined)** |
| ripgrep searched tests | yes | no |
| diff followed its cursor | no | **yes** |

## Results

| adapter | pass | declined | strict | matched | gap | distr. | tok/scenario | calls |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| nudox-full | 4/8 (50%) | 0 | 0.73 | 0.73 | 0.00 | 1 | 2,471 | 2.1 |
| nudox-keys | 4/8 (50%) | 0 | 0.73 | 0.73 | 0.00 | 1 | 2,471 | 2.1 |
| nudox-graph-first | 3/8 (38%) | 0 | 0.56 | 0.56 | 0.00 | 0 | 2,634 | 2.2 |
| nudox-search-only | 3/7 (43%) | 1 | 0.51 | 0.51 | 0.02 | 1 | 1,254 | 1.0 |
| nudox-oneshot | 2/8 (25%) | 0 | 0.27 | 0.27 | 0.02 | 1 | 574 | 1.0 |
| ripgrep | 6/7 (86%) | 1 | 0.95 | **0.57** | 0.05 | 0 | 8,447 | 1.0 |
| rust-analyzer | 1/4 (25%) | 5 | 0.35 | 0.35 | 0.04 | 0 | 306 | 1.5 |

## What this run establishes

**Nudox now out-reaches both baselines.** It attempts 8 of 10 scenarios with
zero declines. ripgrep cannot compare two versions; rust-analyzer declines
every non-Rust package and every cross-version question. That is the axis where
an index has a structural answer, and it is now measured rather than asserted.

**ripgrep is still ahead on correctness, and it is not close.** 6/7 against
4/8. Anyone claiming an index beats grep on these questions has to reckon with
that number first.

**But two fifths of ripgrep's recall is text it printed, not text it found.**
`strict` 0.95 against `matched` 0.57. Nudox's two figures are identical, as are
rust-analyzer's: everything they return, they were asked for. That column did
not exist before this session, and it is the clearest statement of what a
resolved index buys that a pass/fail rate cannot show.

**The embedder is worth a scenario, and it is not free.** On
`ts/commander/public-surface` the *identical* three calls with the *identical*
query scored 10% and returned 136 tokens without an embedder, and **passed**
returning 7,010 tokens with one. All of the gain and all of the 26× cost sit in
one `search` response.

**Following the cursor turned a zero into a 75%.** `diff` renders a `next:`
line; the adapter now asks for page two. `rust/hashbrown/diff-0.16.1-to-0.17.1`
went from 0% (3,745 tokens, one page of `alloc` internals) to 75% (7,904
tokens, two pages) — `into_entry`, `into_map` and `replace_key` all found.
`insert_entry_with_key` is presumably on page three, which
`max_tool_calls = 2` does not buy.

## This run predates two harness fixes

Two adapter bugs were found *after* this run, by reading the payload of the
scenario it scored zero on. Both are committed and unit-tested, and both were
re-measured in isolation against a single-package endpoint — but **this run's
table does not include them**, because the machine cannot hold the full corpus
and a rebuild at once (see below).

| | this run | re-measured with the fixes |
|---|---|---|
| `py/requests/callers-api-request`, nudox-full | 0% on 54 tokens | **29% on 2,398 tokens** |

1. **The query was a path, and `search` matches names.** The adapter sent
   `requests.api.request` verbatim; no declaration is *named* that, so the
   response was `## Search · 0 … (no matches)`. The Rust scenarios escaped this
   only because their queries were already bare identifiers.
2. **A bare name is ambiguous.** `request` returns 50 hits, the first being
   `'requests.sessions'.Session.request` — the method, not the module-level
   function the scenario names. The adapter followed it and `refs` returned
   `(none)`. It now picks the hit sharing the most trailing path segments with
   the scenario's `probe.symbol`.

**What is left after both fixes is a real finding, not a harness bug.**
`refs(in)` on the correct key — `pypi:requests::'requests.api'.request[function]`
— returns `(none)`. Nudox records no inbound references for that function, so
the 29% is what the search listing alone carries. Whether that is a Python
producer gap or a scoping default (`~scope:default` excludes `Field`, `Variant`
and `Param`) is the next thing to establish.

## Known defects in this run
* **The distractor moved to Nudox.** `ParseBytes` now leaks into *Nudox's*
  answer on `go/uuid/callers-parse` (50% recall, one distractor) while ripgrep
  passes that scenario. Unlike ripgrep's, this one is a genuine index
  attribution rather than a grep context line, so it is worth chasing: either
  the index attributes a reference `ParseBytes` does not make, or the scenario's
  ground truth is wrong. Neither has been checked yet.
* **`rust/memchr/public-surface` still fails at 60% for a harness reason.**
  Every `memchr*` fact matched strictly; every `memrchr*` fact is absent. The
  adapter searches the package name, which prefix-matches `memchr2` and
  `memchr_iter` but never `memrchr`. That is the query strategy, not the index.
* **Ten scenarios is still not a sample.** Nothing here is statistically
  significant.

## Why there is no re-run with the fixes

The machine ran out of memory. With the embedder loaded over five packages the
endpoint holds **~5.4 GB resident**, and the kernel killed it twice — once
mid-session while a `cargo build --release` ran beside it, and once before a
run could start, with 5 GB available against that 5.4 GB working set:

```
Out of memory: Killed process 1385885 (nudox-endpoint)
  total-vm:11401620kB, anon-rss:5395604kB
```

The two fixes above are therefore verified in isolation, on a single-package
endpoint, rather than in a full matrix. Reproducing the whole table with them
needs a machine with more headroom, or a corpus trimmed to the scenarios under
test. That is the first thing to do next, and it is a hardware constraint
rather than an open question.
