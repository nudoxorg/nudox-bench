# nudox-bench

A benchmark for the [Nudox](../Backend) code-intelligence surface.

It asks real questions about real packages — *who calls this, what is this
signature, what changed between these versions* — and measures, for each
system that can answer them, **whether the answer is right, how well resolved
the evidence is, and what it cost an agent's context**.

It takes its shape from [`context-bench`](../context-bench), which measured
how well MCP servers supply documentation context to coding agents. It differs
in three ways that follow from Nudox being a code-intelligence index rather
than a documentation search:

| | context-bench | nudox-bench |
|---|---|---|
| scoring | 3-model LLM judge on every scenario | recorded ground truth, matched mechanically; a judge only for implementation scenarios |
| comparison set | four MCP servers | Nudox against itself (ablations), plus the tools an agent already has — `ripgrep` and rust-analyzer |
| cost model | response tokens | resident surface cost *and* per-question cost, both via a real BPE tokenizer |

The practical consequence: **most of this benchmark runs offline, for free,
with no API key, and gives the same answer every time.**

---

## Quick start

Nothing below needs a Nudox endpoint. This is the floor every Nudox number is
read against, and it is the fastest way to check the harness works:

```bash
cargo run --bin nbench -- run --config configs/baselines.toml
```

To see what a live Nudox surface costs before it is asked anything:

```bash
NUDOX_MCP_URL=http://127.0.0.1:PORT/mcp NUDOX_MCP_TOKEN=… \
  cargo run --bin nbench -- surface --per-tool
```

To run the full comparison, and then the ablation sweep:

```bash
cargo run --bin nbench -- run --config configs/default.toml   --ensure-indexed
cargo run --bin nbench -- run --config configs/ablations.toml --ensure-indexed
```

See [docs/RUNNING.md](docs/RUNNING.md) for where the URL and token come from.

---

## What gets measured

Every run reports these per adapter. There is deliberately **no single score**
— see [docs/DESIGN.md](docs/DESIGN.md#no-headline-number).

| column | what it means |
|---|---|
| `pass` | full strict recall, `any_of` satisfied, and no distractors |
| `attempted` / `unsup.` | scenarios the adapter could express, and those it declined |
| `unscored` | ran, but judged with no judge available |
| `strict` | recall counting only fully-qualified matches |
| `loose` | recall counting bare identifiers anywhere in the output |
| `gap` | `loose − strict`: how much apparent success is unresolved text |
| `distractors` | near-miss symbols that leaked into the answer |
| `tok/scenario` | per-question cost, real BPE, larger of cl100k/o200k |
| `tok/pass` | tokens per scenario actually passed — `—` if none passed |

Plus a table of **resident cost**: what each surface puts in an agent's
context before it is asked anything.

### Measured baseline numbers

From `configs/baselines.toml` over the ten scenarios in this repo — real
output, not an illustration:

| adapter | pass | attempted | unsup. | unscored | strict | tok/scenario | tok/pass |
|---|---:|---:|---:|---:|---:|---:|---:|
| ripgrep | 5/7 (71%) | 7 | 1 | 2 | 0.95 | 9,658 | 13,522 |
| rust-analyzer | 1/4 (25%) | 4 | 5 | 1 | 0.35 | 306 | 1,226 |

`unscored` is the two implementation scenarios, which need a judge; this run
had no `OPENROUTER_API_KEY`. They are held out of the pass rate rather than
counted as failures.

Read that as the bar, not as a result about Nudox: **ripgrep answers most of
these questions**, and the interesting column is what it spends doing so.
rust-analyzer is an order of magnitude cheaper but declines four of eight
scenarios outright — every non-Rust package, and every cross-version question.
Those two facts are the shape of the opportunity.

One number worth singling out: on `go/uuid/callers-parse`, ripgrep reaches
100% recall and still **fails**, because `ParseBytes` — an adjacent exported
function that never calls `Parse` — leaked into the answer. That is the
distractor mechanism working as designed.

---

## Layout

```
scenarios/{rust,go,ts,py}/*.toml   questions + recorded ground truth
configs/*.toml                     which adapters take part in a run
src/scenario.rs                    the scenario model
src/truth.rs                       deterministic scoring (strict vs loose)
src/tokens.rs                      BPE metering, cl100k + o200k
src/mcp/                           a minimal MCP client (HTTP + stdio)
src/adapter/                       nudox (with ablations), ripgrep, rust-analyzer
src/judge.rs                       3-model panel, implementation scenarios only
src/report.rs                      aggregation and the Markdown report
```

## Adding a scenario

Scenarios are TOML, and the ground truth must be **derived by reading the
package**, not recalled. Every scenario in `scenarios/` carries a comment
naming the file and lines its truth came from; keep that habit — it is what
makes a failing run a finding rather than an argument.

`cargo run --bin nbench -- list` validates every scenario and refuses ones
that cannot fail, which is the most common way to write a worthless one.

See [docs/SCENARIOS.md](docs/SCENARIOS.md).

## Status

Working and producing numbers for the `ripgrep` and rust-analyzer adapters.
The Nudox adapter is written and compiles but **has not yet been run against a
live endpoint** — see [docs/RUNNING.md](docs/RUNNING.md#status) for what that
needs and what is currently in the way.
