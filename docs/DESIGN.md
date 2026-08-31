# Design

## What is being measured

> For a real question about a real package, does a system produce the answer,
> how well resolved is the evidence, and what does it cost an agent's context?

Not "does Nudox return something". Not "does Nudox beat Context7 at
documentation retrieval" — that comparison is worth running, but only once
there are numbers against the tools an agent *already has*, because those are
what a code-intelligence index has to beat to be worth installing.

## Why most scenarios have no model in the loop

context-bench judges every scenario with a 3-model panel because its questions
("build a multi-agent system with termination conditions") have no single
right answer. Most questions here do. *Who calls `search_slice_with_raw`* has
an answer you can read out of the source, and once it is written down the
scenario can be scored by string matching.

That one change buys a lot:

* runs are **reproducible** — no temperature, no panel drift, no model
  deprecation invalidating last month's numbers;
* runs are **free**, so the benchmark can go in CI;
* a failure is a **finding**, not a judge's opinion to argue with.

`Family::Implementation` keeps the judge, and keeps context-bench's exact
protocol — 2-of-3 on both *complete* and *relevant* — because changing the
judging rule and the system under test at once would make the two sets of
numbers incomparable.

## Strict and loose recall

The subtle way a retrieval benchmark goes wrong is in how it decides a symbol
"appeared" in an answer. `memchr::memchr3_iter` can show up as:

* the fully-qualified path,
* a Nudox address — `cargo:memchr@2.8.3::memchr::memchr3_iter[function]`,
* the bare identifier `memchr3_iter`, in a line of source a grep printed.

Those are not the same evidence. Collapsing them into one boolean flatters
whichever adapter returns the most undifferentiated text. So every fact is
matched twice:

* **strict** — the qualified path, with `::`, `.` and `/` treated as the same
  separator (Nudox's address grammar already does this, and Go and Python
  spell it differently);
* **loose** — the final identifier, on a word boundary.

The **gap** between the two columns is the measurement of what resolution is
worth. A symbol index scores near-identically on both. A text search does not.

Word boundaries are load-bearing: `memchr` must not be credited by a line
containing only `memchr3`. `truth.rs` tests exactly that case.

## Distractors

A retrieval tool can score perfect recall by returning the entire package.
`truth.none_of` names symbols a *correct, narrow* answer would not contain —
near-misses, same-named symbols elsewhere, the classic text-search false
positives. Any distractor sinks the scenario.

This is not hypothetical. On `go/uuid/callers-parse`, ripgrep achieves 100%
strict recall and fails, because `ParseBytes` lands in the output.

**Corrected 2026-08-30, by auditing the stored payload.** This paragraph used
to say `ParseBytes` *matches* `\bParse\s*\(`. It does not, and cannot —
`ParseBytes(` has `Bytes` where that pattern requires `\s*\(`. All five of its
mentions in the returned context are **`-B 12` context lines** from
`uuid_test.go`. The recall came the same way: `MustParse` and `Scan` were both
credited from context lines carrying the enclosing `func` declaration, not from
a matched call site, and 267 of the 343 returned lines are from `_test.go`
files.

Two things follow, and both are worth more than the original claim:

* The scenario is real — a near-miss symbol did leak into an answer that should
  have been narrow, and the distractor rule caught it. But what it prices is the
  **ripgrep adapter's invocation** (twelve lines of leading context, no
  test-file exclusion), not a limit of text search. `--glob '!*_test.go'` would
  plausibly pass it.
* Where a scenario's facts are bare identifiers — every Go and Python scenario —
  strict and loose recall coincide by construction, so the resolution gap cannot
  detect recall manufactured by a context window. That is a gap in the
  measurement, not in the tool under test.

## <a id="no-headline-number"></a>No headline number

There is no weighted composite score, deliberately. Four things are measured
— found it, how well-resolved, what it cost, could it answer at all — and they
trade against each other. Any fixed weighting would encode an opinion about
that trade-off into what is supposed to be evidence about it. The summary is a
table; the reader does the weighing.

For the same reason `unsupported` never enters a pass rate. "This tool cannot
express this question" and "this tool got it wrong" are different findings.
Folding them together would report rust-analyzer as *bad at Go* rather than
*inapplicable to it*.

## Cost accounting

Two numbers, kept apart:

* **resident** — every tool description plus every argument-schema
  `description`: what the surface costs before the agent asks anything, paid
  once per session. A benchmark that counted only response bodies would rank a
  server with a 15k-token schema dump equal to one without.
* **marginal** — requests plus responses for this question. The number to
  compare adapters on once a session is warm.

Both are counted with a real BPE tokenizer, reporting `cl100k_base` and
`o200k_base` together, matching the contract in
`Backend/docs/MCP-SURFACE-PLAN.md`. There is no cheap estimator path, because
the thing this benchmark exists to measure is the thing an estimator gets most
wrong: a 64-hex key measures as ~50 tokens and as **1** under a
count-alphanumeric-runs proxy — a 50× error on the largest single item in a
Nudox payload. `tokens.rs` has a test pinning that.

## Why the adapters own their tool mapping

context-bench keeps a `MCP_MAPPINGS` table in the harness: per server, which
tool to call and how to build its parameters. That works when every entrant is
an MCP server with one documentation-search tool. It stops working when the
comparison set includes `ripgrep` and an LSP server, which have no tools and
reach an answer in several steps.

So a scenario names the *question* (`TaskKind`) and the *subject* (`Probe`),
and each adapter maps that pair onto its own surface. `Probe` carries the
subject in several spellings at once — bare identifier, qualified path, free
text — because ripgrep wants a literal, rust-analyzer wants a name plus a file
position, and Nudox wants an address.

## Ablations are client-side

Every Nudox ablation is a restriction applied by the client — which tools may
be called, whether to stop after one call, whether to echo keys or addresses.
None needs a rebuilt server, so a five-way sweep is one `cargo run`. They map
onto specific claims in MCP-SURFACE-PLAN; the table is on `Policy` in
`src/adapter/nudox.rs`.

## Why the MCP client is hand-rolled

Three reasons, all about measurement:

1. **The payload is the measurement.** A client that deserializes into typed
   structs discards the exact text that was on the wire, which is the only
   thing worth counting.
2. **The comparison set will grow** to third-party MCP servers, which ship as
   stdio processes, while Nudox is loopback HTTP only. One client speaking
   both keeps the adapters symmetric.
3. **Build independence.** Linking the server's own `rmcp` would couple this
   repo to the Backend workspace's build — which is behind a nix flake that is
   currently broken. The benchmark has to stay cheap to run, or it will not be
   run.
