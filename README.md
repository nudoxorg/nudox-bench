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

Check the machine first — this tells you exactly what is and is not set up:

```bash
cargo run --bin nbench -- doctor
```

Then run the baselines. These need no Nudox endpoint, no API key and no
network, and they are the floor every Nudox number is read against:

```bash
cargo run --bin nbench -- run --config configs/baselines.toml
```

For the Nudox comparison you need a running endpoint. Nudox has no standalone
server — its MCP endpoint lives inside the `lindsey` GUI — so this repo ships
the missing piece in [`endpoint/`](endpoint):

```bash
cd endpoint && cargo build --release          # once; ~20-40 min, needs nightly
./target/release/nudox-endpoint --package "$HOME/.cargo/registry/src"/*/memchr-2.8.3
# prints NUDOX_MCP_URL and NUDOX_MCP_TOKEN, stays up until Ctrl-C
```

Then, in another shell:

```bash
export NUDOX_MCP_URL=…  NUDOX_MCP_TOKEN=…
cargo run --bin nbench -- surface --per-tool        # what the surface costs, per tool
cargo run --bin nbench -- run --config configs/default.toml
cargo run --bin nbench -- run --config configs/ablations.toml
```

**[docs/SETUP.md](docs/SETUP.md) is the end-to-end checklist** — prerequisites,
the three tiers of setup, every environment variable, and a troubleshooting
table. [docs/RUNNING.md](docs/RUNNING.md) explains *why* each step is shaped
the way it is, including the two non-obvious build traps (the toolchain is
resolved from your *working directory*, and Backend's workspace — not
`nudox-engine` — is what fails to resolve inside nix).

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

## Results

The first full run — all five Nudox policies against both baselines — is in
[`results/2026-08-30/`](results/2026-08-30), with `report.md` and `report.json`
beside the charts. [`protocol.html`](results/2026-08-30/protocol.html) is the
written result: what came back, how every number was recorded and scored, and
which claims the method supports.

The headline, stated carefully: on the five scenarios both systems attempted,
Nudox answered in **785 tokens per question against ripgrep's 9,853**, with a
resolution gap of zero — and passed 3 of 5 to ripgrep's 4 (on the four Rust
scenarios alone, both pass 3 of 4). *Same answers, a fraction of the context*
is supported by that run. *More correct answers* is not.

## Status

* **Baselines** — working, producing the numbers above.
* **`endpoint/`** — builds and starts a real MCP endpoint over real indexed
  packages, outside nix, without `clang`. It now also installs the semantic
  embedder from `NUDOX_EMBED_MODEL_DIR`, which it previously could not: the
  engine takes an embedder from its host, and this one never supplied one, so
  every published Nudox number so far is **structural-only ranking**.
* **The Nudox adapter** — wired end to end, with five client-side ablations,
  and exercised by the 2026-08-30 run.
* **Not yet measured** — semantic search (see above), Go and Python (the
  producers need an oracle and a source checkout, [SETUP.md §3](docs/SETUP.md)),
  the implementation scenarios (no judge key that run), and any third-party MCP
  server: `AdapterConfig::McpStdio` is declared in config but the harness still
  refuses it as not wired. **There is no measured comparison against Context7,
  Deepcon, Exa or NIA** — the only baselines are ripgrep and rust-analyzer.
