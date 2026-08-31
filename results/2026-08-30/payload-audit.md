# Payload audit — `run-2026-08-30-165221`

Every attempt in that run that stored its returned text (51 of 70) was
re-examined to answer two questions the report cannot:

1. **Where did each credited fact actually come from?** `rg -n --no-heading`
   writes a matched line as `path:line:text` and a context line as
   `path-line-text`. That one character says whether the tool *found* the fact
   or whether a `-B`/`-A` window happened to drag it in.
2. **How much of the payload is test-file noise?**

Run it yourself with `scripts/audit_payloads.py`.

---

## What prompted it

`docs/DESIGN.md` claimed ripgrep's distractor on `go/uuid/callers-parse`
arrived by matching `\bParse\s*\(`. It cannot: `ParseBytes(` has `Bytes` where
that pattern requires `\s*\(`. That one wrong sentence was worth a sweep of
everything else.

## The correction that matters most — to the audit's own framing

The first reading of these results was "the context window manufactures
recall". That is **wrong as a criticism**, and the adapter says so in its own
doc comment: for a callers query, *"the line holding a call names the callee,
never the caller. Answering 'who calls this' from grep output means reading
back up to the enclosing declaration, so the leading context is part of the
honest baseline — and the tokens it costs are part of the honest price."* The
same reasoning covers `-A 8` on a signature, which spans lines.

So context-line credit is **intended evidence**, not an artifact. What the
audit legitimately found is narrower and still worth acting on:

* one **real defect** — test files were being searched, and
* one **transparency gap** — nothing in the report tells a reader how much of a
  score came from matched lines versus the surrounding window.

## Findings

| adapter / scenario | payload | test-file share | facts credited from | verdict |
|---|---|---|---|---|
| ripgrep / `go/uuid/callers-parse` | 343 lines (31 matched, 293 context) | **82%** | context only | fail (distractor) |
| ripgrep / `rust/memchr/callers-search-slice-with-raw` | 517 lines (37 matched, 444 context) | 0% | 1 matched, 5 context | fail (0.67 strict) |
| ripgrep / `rust/memchr/signature-memchr3` | 19 lines (2 matched, 16 context) | 50%¹ | context only | **pass** |
| ripgrep / `py/requests/callers-api-request` | 302 lines (23 matched, 263 context) | 0% | all 7 matched | pass |
| ripgrep / `rust/memchr/public-surface` | 298 lines (298 matched, 0 context) | 0% | all 15 matched | pass |
| ripgrep / `ts/commander/public-surface` | 26 lines (25 matched) | 0% | all 10 matched | pass |
| rust-analyzer / `rust/memchr/callers-search-slice-with-raw` | 27 lines | 0% | 1 matched, 5 absent | fail |

¹ memchr keeps test-support code at `src/tests/`, which the share counts as
test files. The expected facts come from `src/memchr.rs`, so the exclusion
below does not cost this scenario its pass.

### The one defect: test files were in scope

On `go/uuid/callers-parse`, 267 of 343 returned lines came from `uuid_test.go`,
`json_test.go` and `sql_test.go`. That is where the 10,719-token cost went, and
it is where the run's only distractor came from — `ParseBytes` appears
in a `-B 12` window opened by a call site in `uuid_test.go`, five times, never
once in library source.

Both expected facts live in library source (`MustParse` in `uuid.go`, `Scan` in
`sql.go`), so excluding tests removes noise and a false positive while keeping
every fact. `src/adapter/ripgrep.rs` now excludes tests, testdata, vendor and
`node_modules` from every search.

**Measured effect** (`run-2026-08-30-223659` and the run after it):

| | before | after |
|---|---|---|
| `go/uuid/callers-parse` | fail at 100% recall, 10,719 tok | **pass, 2,031 tok** |
| `ts/commander/public-surface` | pass, 910 tok | pass, 956 tok |
| ripgrep overall | 5/7 (71%), 9,658 tok/scenario | **6/7 (86%), 8,447 tok/scenario** |
| ripgrep tok/pass | 13,522 | **9,855** |

The fix makes the floor **stronger** — which is the right direction for a
baseline everything else is measured against.

It also produced a regression worth recording, because it is the kind that
looks like a result. The first run with these exclusions turned
`ts/commander/public-surface` from a pass into **0% recall on 149 tokens**:
commander lives at `…/node_modules/commander`, so `!**/node_modules/**`
excluded the entire search root rather than a nested dependency. A directory
exclusion is now dropped whenever the root already sits inside a directory of
that name, and `an_exclusion_never_swallows_the_root` pins it.

### The transparency gap

Three of nine ripgrep verdicts — including one pass — rest on facts that
appear only in context lines. That is legitimate evidence, but it is not the
same evidence as a matched line, and the report currently cannot say which is
which. This is the same species of distinction as strict-versus-loose recall,
and it is not yet measured.

Worth adding, in the benchmark's own idiom: report both, and let the reader
weigh them. **Now added** — `Attempt::primary_context` carries the matched
subset for adapters that can tell the difference, and the results table gained
a `matched` column beside `strict`. The first measurement:

| adapter | strict | matched |
|---|---:|---:|
| ripgrep | 0.95 | **0.57** |
| rust-analyzer | 0.35 | 0.35 |

**Two-fifths of ripgrep's strict recall comes from lines it printed around a
match rather than lines it matched.** rust-analyzer's two figures are equal
because everything it returns, it was asked for — which is the shape the
column exists to show, and the shape Nudox should have when the endpoint is
next run against it.

### A measurement limit this exposes

Where a scenario's expected facts are bare identifiers — every Go and Python
scenario, because those ecosystems spell a symbol unqualified in its own
source — strict and loose recall **coincide by construction**, so the
resolution gap cannot detect anything there at all. Qualifying those facts
(`uuid.MustParse`) would not fix it: that spelling appears in neither Go source
nor a Nudox address, so it would simply make strict recall unreachable for
every adapter. The honest statement is that the resolution-gap column is
meaningful for Rust-style paths and vacuous elsewhere, and the
matched-versus-context split above is what would carry the same weight for the
rest.
