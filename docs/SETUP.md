# Setup, end to end

How to get from a fresh clone to a full Nudox comparison run, including the
parts that are easy to get silently wrong.

[docs/RUNNING.md](RUNNING.md) explains *why* each step is shaped the way it is.
This file is the checklist.

---

## The three tiers

You do not need all of this at once. Each tier is useful on its own, and each
one adds work in a straight line.

| tier | what you get | what it needs |
|---|---|---|
| **1 — baselines** | ripgrep and rust-analyzer over the same scenarios | a Rust toolchain, `rg`, `rust-analyzer`. No network, no key, no endpoint. |
| **2 — Nudox, structural** | the five Nudox policies against those baselines | tier 1, plus a Backend checkout and a ~20–40 min endpoint build |
| **3 — Nudox, complete** | semantic search on, Go and Python indexable, implementation scenarios judged | tier 2, plus an embedding model, the Go oracle, and an OpenRouter key |

**Every published number so far is tier 2.** The 2026-08-30 run reports
`~sem:unavailable(NoModelConfigured)` on every Nudox row, holds 3 of 5
packages, and leaves the two implementation scenarios `unscored`. Tier 3 is
what makes the next run decisive.

---

## 0. Prerequisites

```bash
rustup toolchain install stable            # the harness
rustup component add rust-analyzer         # a baseline adapter, see the trap below
```

| thing | why | check |
|---|---|---|
| Rust **stable** | the harness (`nbench`) pins stable in `rust-toolchain.toml` | `cargo --version` |
| Rust **nightly** | Backend uses `#![feature(...)]`; `endpoint/` pins nightly for its own directory only | `rustup toolchain list` |
| `ripgrep` | a baseline adapter | `rg --version` |
| `rust-analyzer` | a baseline adapter — the **component**, not the rustup shim | `rust-analyzer --version` |
| Python 3 + matplotlib | only for `scripts/plot_results.py` | optional |

> **The rust-analyzer trap.** `rustup` puts a shim named `rust-analyzer` on
> `PATH` whether or not the component is installed. `which` succeeds and
> spawning fails with `Unknown binary`. This is why `nbench doctor` *runs* the
> binary rather than looking for it. If your rust-analyzer lives elsewhere:
> ```bash
> export RUST_ANALYZER_BIN=~/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/bin/rust-analyzer
> ```

### The one layout constraint

`endpoint/Cargo.toml` depends on `nudox-engine` by **relative path**:

```
nudox-engine = { path = "../../Backend/workspace/nudox-engine" }
```

So the Backend checkout must sit **beside** this repo:

```
your-dev-dir/
├── Backend/          ← the Nudox backend checkout
└── nudox-bench/      ← this repo
```

Tier 1 does not care. Tiers 2 and 3 will not build without it. If your Backend
lives somewhere else, edit that one path — do not move the repo into Backend's
workspace, for the resolution reason in RUNNING.md.

Two consequences worth knowing before you hit them:

* A **git worktree** of this repo (`.claude/worktrees/<name>/`) sits three
  directories deeper, so `../../Backend` resolves to nothing there. Build the
  endpoint from the main checkout.
* `endpoint/` is deliberately **not** a workspace member and carries its own
  `Cargo.lock`. That is what keeps cargo from resolving Backend's whole
  workspace, which fails on a private SSH git dependency that `nudox-engine`
  itself does not use.

---

## 1. Tier 1 — the baselines

Clone, then:

```bash
cargo run --bin nbench -- doctor        # what is and is not set up
cargo run --bin nbench -- list          # validates every scenario
cargo run --bin nbench -- run --config configs/baselines.toml
```

`doctor` exits non-zero if a run would be degraded, so it works in CI. It
reports every scenario's package root, whether `rg` and rust-analyzer actually
run, whether a Nudox endpoint answers, and whether a judge key is present.

### Package roots

Scenarios point at packages already on your machine, written with variables so
the committed files stay portable:

| variable | resolved from | used by |
|---|---|---|
| `${CARGO_SRC}` | `~/.cargo/registry/src/<index-hash>/` | memchr, hashbrown |
| `${GO_MOD}` | `~/go/pkg/mod` | google/uuid |
| `${PY_SITE}` | `/usr/lib/python3.X/site-packages` | requests |
| `${NODE_MODULES}` | **not guessable — set it yourself** | commander |

```bash
export NODE_MODULES=/path/to/some/node_modules
```

An unset variable survives into the path verbatim, so `doctor` prints
`MISSING ${NODE_MODULES}/commander` rather than a confusing `/commander`.

To fetch what is missing:

```bash
cargo fetch                                   # populates ~/.cargo/registry/src
go mod download github.com/google/uuid        # populates ~/go/pkg/mod
npm install commander                         # then point $NODE_MODULES at ./node_modules
```

---

## 2. Tier 2 — a real Nudox endpoint

Nudox's MCP server is **not a standalone binary**. It is started inside the
`lindsey` GUI process by `McpHost::start`, streamable-HTTP only, loopback only,
on an ephemeral port, behind a bearer token minted fresh each launch. A
benchmark cannot drive a GUI, so this repo ships the missing piece in
[`endpoint/`](../endpoint).

### 2.1 Build it — once

```bash
cd endpoint          # NOT from the repo root
cargo build --release
```

Both details matter:

* **Run it from inside `endpoint/`.** `rustup` resolves `rust-toolchain.toml`
  from the *working directory*, not from `--manifest-path`. Building with
  `--manifest-path endpoint/Cargo.toml` from the repo root picks up the root's
  stable pin and fails with
  `` `#![feature]` may not be used on the stable release channel ``.
* **Budget 20–40 minutes** the first time — it compiles `nudox-engine` and its
  graph (`ra_ap_*`, pyrefly, the ONNX runtime). Use `--release`: a debug build
  indexes several times slower, and indexing is what you wait on at every
  start.

It needs neither the nix devshell nor `clang`.

### 2.2 Start it

```bash
cd endpoint
./target/release/nudox-endpoint \
  --package "$HOME/.cargo/registry/src"/*/memchr-2.8.3 \
  --package "$HOME/.cargo/registry/src"/*/hashbrown-0.17.1 \
  --package "$HOME/.cargo/registry/src"/*/hashbrown-0.16.1 \
  --package "$NODE_MODULES/commander"
```

| flag | meaning |
|---|---|
| `--package PATH[=NAME@VERSION]` | a package root to index at start-up, repeatable. Name, version and language are read from the manifest, so a bare path is enough; override when the directory name lies. |
| `--export` | print `export NUDOX_MCP_URL=… / NUDOX_MCP_TOKEN=…` and nothing else, so the output can be `eval`'d |
| `--token` | use a fixed launch token instead of a generated one — handy for a dev loop, never for anything reachable |
| `--load-timeout-secs` | how long to wait for every package to load (default 300) |

It prints the two values a client needs and stays up until Ctrl-C:

```
NUDOX_MCP_URL=http://127.0.0.1:41337/mcp
NUDOX_MCP_TOKEN=…
```

The **port and token change on every launch** — that is the product's design,
not a rough edge, which is why no config file can carry them.

A package that fails to lower is a warning, not a fatal error: the endpoint
serves the ones that loaded and says which did not. That is deliberate — one
producer missing an optional oracle is a result to record, not a reason to
refuse to serve.

Language is decided by the manifest present in the directory: `Cargo.toml`,
`go.mod`, `package.json`, `pyproject.toml`/`setup.py`, `pom.xml`/`build.gradle`,
`*.csproj`, `CMakeLists.txt`/`compile_commands.json`. **A directory with none of
these cannot be indexed** — see the Python note in tier 3.

### 2.3 Point the benchmark at it

In another shell:

```bash
export NUDOX_MCP_URL=http://127.0.0.1:41337/mcp
export NUDOX_MCP_TOKEN=…

cargo run --bin nbench -- surface --per-tool           # cheapest useful check
cargo run --bin nbench -- run --config configs/default.toml
cargo run --bin nbench -- run --config configs/ablations.toml
cargo run --bin nbench -- run --config configs/everything.toml
```

Or in one step:

```bash
eval "$(cd endpoint && ./target/release/nudox-endpoint --package … --export)"
```

`surface --per-tool` handshakes, lists the tools, and prints what the surface
costs before any question is asked, per tool, largest first. If that works,
everything else will.

### 2.4 Keeping the corpus and the files in agreement

The file-based adapters read `target.root` off the scenario. Nudox answers from
its corpus. If those hold different versions of the same package, the run
measures the skew rather than the tools.

Two ways to close it — pick one:

* start the endpoint with `--package` pointing at **those same roots** (what
  the commands above do), or
* run with `--ensure-indexed`, which indexes each scenario's `target.root`
  through Nudox's `index` tool before running:

```bash
cargo run --bin nbench -- run --config configs/default.toml --ensure-indexed
```

`--ensure-indexed` is off by default because it costs seconds to minutes per
package.

---

## 3. Tier 3 — the complete Nudox setup

Four independent switches. Each one turns a currently-unmeasured axis into a
measured one.

### 3.1 Semantic search

Every Nudox row in the published run carries
`~sem:unavailable(NoModelConfigured)` — no embedding model was configured, so
half of what `search` ranks with was disabled, and the scenarios it should help
most (prose questions, concept lookups) are exactly the ones Nudox lost.

```bash
export NUDOX_EMBED_MODEL_DIR=/path/to/jina-embeddings-v2-base-code
cd endpoint && ./target/release/nudox-endpoint --package …
```

**Backend already pins this model in nix**, so on a machine that has built
Backend it is on disk and needs no download:

```bash
ls -d /nix/store/*-nudox-semantic-model-jina-code-v2
export NUDOX_EMBED_MODEL_DIR=$(ls -d /nix/store/*-nudox-semantic-model-jina-code-v2 | head -1)
```

Otherwise the directory must contain the fp32
`jinaai/jina-embeddings-v2-base-code` artifact and its four tokenizer
sidecars:

```
model.onnx                 # 641,517,466 bytes; sha256 pinned and verified before it reaches ort
tokenizer.json
config.json
special_tokens_map.json
tokenizer_config.json
```

The ONNX runtime is compiled into every build, so this needs **no rebuild** —
the variable is read at start-up. The endpoint now prints which state it is in:

```
semantic search: on (model from NUDOX_EMBED_MODEL_DIR)
semantic search: OFF — structural only. Set NUDOX_EMBED_MODEL_DIR to enable.
```

> Before 2026-08-30 the endpoint built `EngineConfig::default()`, which leaves
> `embedder: None`. Setting the variable alone did nothing, because the engine
> takes an embedder from its host and does not go looking for one — the same
> line `lindsey`'s `main.rs` has. That is fixed; a model directory is now all
> it takes.

A model that is *present but broken* reports `ModelFailed` rather than
`NoModelConfigured` — a different failure pointing at a missing file, a bad
hash, or an out-of-memory runtime.

### What the embedder costs, measured

**Budget 8 GB free for a five-package corpus.** Two costs, and the second is
the one that surprises:

```
t=15s  RSS=0.58 GB    producers running, no model yet
t=25s  RSS=4.50 GB    the model loads — a fixed ~4 GB step
t=30s  RSS=4.87 GB
…
        RSS=7.43 GB   building the vector index over five packages
```

The first step is fixed: loading the ONNX model costs ~4 GB whatever you are
indexing. The second is **not** — building the index adds roughly 2.5 GB more
over a five-package corpus, and it grows with the number of symbols embedded,
not the number of packages.

An earlier version of this file claimed the whole thing was flat at ~4.9 GB.
It is not; that measurement was taken from a probe the OOM killer stopped
before the index finished building, which is a good illustration of why a
number should come from a run that completed.

Memory does plateau — it held at 7.43 GB for several minutes while four cores
stayed busy — so it is bounded by corpus size rather than unbounded. But
plateauing at 7.43 GB on a 16 GB machine leaves very little room.

Three things follow, all learned by being killed:

* **Check `MemAvailable`, not total RAM.** On a 16 GB machine with an editor
  open, two `rust-analyzer` processes held 2.3 GB and left 5.08 GB available —
  enough to *start* the endpoint and not enough to keep it. Closing them took
  available to 8.77 GB, which is what made a full-corpus embedded run possible
  at all:
  ```bash
  awk '/MemAvailable/{printf "%.2f GB\n", $2/1048576}' /proc/meminfo
  pkill -f rust-analyzer   # your editor respawns them
  ```
* **Have some swap.** A machine with `SwapTotal: 0` gives a transient spike
  nowhere to go, so it becomes an instant kill rather than a slowdown.
* **Do not build and serve at once.** A `cargo build --release` beside a
  running endpoint got the endpoint OOM-killed mid-session.

There is no knob for this: `MAX_BATCH` is a compile-time constant in
`nudox-engine`'s `embed/onnx.rs`, mirroring a private constant in `registry`,
not an environment variable.

Two more operational facts:

* **It costs memory** — see above.
* **Embeddings are built after a package loads, not before.** A question asked
  too early gets `~sem:building(0/1)` and can score zero on a package that
  would otherwise answer. `py/requests/callers-api-request` did exactly that —
  54 tokens, no matches, purely because the run started seconds after the
  endpoint came up. Give it time to settle, and check a response for
  `~sem:building` before trusting a low score.

### 3.2 Go

The Go producer shells out to a compiled oracle binary that is not built by
`cargo`:

```bash
cd ../Backend/workspace/compiler/languages/oracle/go
go build -o nudox-go-oracle ./...
export NUDOX_GO_ORACLE_BIN=$PWD/nudox-go-oracle
```

The binary is resolved from `NUDOX_GO_ORACLE_BIN` if set, else the bare name
`nudox-go-oracle` on `PATH`. Its contract is
`nudox-go-oracle <module-root-dir>` — oracle JSON on stdout, diagnostics on
stderr, non-zero only on a fatal error.

> **`go` must also be on the endpoint's `PATH` at run time.** Pointing at the
> oracle binary is necessary but not sufficient: the oracle drives
> `go/packages`, which shells out to the `go` command itself. With the binary
> built and `NUDOX_GO_ORACLE_BIN` set but no `go` on `PATH`, the package fails
> to load with
> `oracle: loading packages under …: err: go command required, not found`.
> On a machine with nix and no system Go:
> ```bash
> export PATH=$(dirname $(nix shell nixpkgs#go --command which go)):$PATH
> ```

Without it, `go:github.com/google/uuid` never loads and the two Go scenarios
are recorded as **declined**, not failed.

### 3.3 Python

Python needs **two** things, and the second one is a rebuild.

**1. The `pyrefly` feature must be enabled.** `endpoint/Cargo.toml` builds
`nudox-engine` with `default-features = false`, and the Python producer is
registered only under `pyrefly`. Without it every pypi package fails to load
with `toolchain missing for python (package pypi:requests)` — which is
deliberate upstream: an unregistered producer is honest, whereas a registered
one returning an empty oracle would make *"we cannot read this"*
indistinguishable from *"this package has no public API"*.

This is **not** implied by `nudox-languages` enabling `pyrefly` by default. The
pyrefly crates compile either way — they are in the endpoint's lockfile — but
registration is gated separately. `endpoint/Cargo.toml` now asks for the
feature explicitly; a build from before 2026-08-30 needs
`cargo build --release` again to pick it up.

**2. The package needs a manifest.** An installed `requests` in
`site-packages` has no `pyproject.toml` and no `setup.py`, so `language_for()`
cannot classify it. Cloning upstream would introduce version skew against the
tree ripgrep reads, so the cheaper and more honest option is to wrap the
installed source itself:

```bash
mkdir -p /tmp/requests-2.34.2
cp -r "$PY_SITE/requests" /tmp/requests-2.34.2/requests
printf '[project]\nname = "requests"\nversion = "2.34.2"\n' > /tmp/requests-2.34.2/pyproject.toml
./endpoint/target/release/nudox-endpoint --package /tmp/requests-2.34.2=requests@2.34.2 …
```

That root is byte-identical to what ripgrep reads (`diff -r` it), so the two
adapters answer about exactly the same code and there is no skew to argue
about.

### 3.4 The judge

Only the `implementation` family is judged; everything else is matched
mechanically. The panel is three models voting on two booleans (*complete*,
*relevant*), 2-of-3 required on both — context-bench's protocol, kept unchanged
so the two sets of numbers stay comparable.

```bash
export OPENROUTER_API_KEY=sk-or-…
cargo run --bin nbench -- run --config configs/everything.toml --judge
```

Without the key, those scenarios report `unscored`, which is held out of every
pass rate rather than counted as a failure. The panel is a constant in
`src/judge.rs`, not a config knob — a panel that drifts between runs would make
the runs incomparable.

### 3.5 The whole thing, in order

```bash
# once
cd endpoint && cargo build --release && cd ..
cd ../Backend/workspace/compiler/languages/oracle/go && go build -o nudox-go-oracle ./... && cd -

# every session
export NUDOX_EMBED_MODEL_DIR=$(ls -d /nix/store/*-nudox-semantic-model-jina-code-v2 | head -1)
export NUDOX_GO_ORACLE_BIN=/path/to/nudox-go-oracle
export OPENROUTER_API_KEY=sk-or-…
export NODE_MODULES=/path/to/node_modules

eval "$(cd endpoint && ./target/release/nudox-endpoint --export \
  --package "$HOME/.cargo/registry/src"/*/memchr-2.8.3 \
  --package "$HOME/.cargo/registry/src"/*/hashbrown-0.17.1 \
  --package "$HOME/.cargo/registry/src"/*/hashbrown-0.16.1 \
  --package "$HOME/go/pkg/mod/github.com/google/uuid@v1.6.0" \
  --package "$NODE_MODULES/commander" \
  --package /tmp/requests=requests@2.31.0 &)"

cargo run --bin nbench -- doctor
cargo run --bin nbench -- run --config configs/everything.toml --judge
```

---

## 4. Running and reading

### Commands

| command | what it does |
|---|---|
| `nbench doctor` | preflight; non-zero exit if a run would be degraded |
| `nbench list` | load and validate every scenario, then list them |
| `nbench surface --per-tool` | resident cost of a live MCP surface, per tool |
| `nbench run --config <f>` | the matrix: every adapter × every scenario |

Useful `run` flags:

| flag | effect |
|---|---|
| `--filter <substr>` | only scenarios whose id contains this — the fast loop |
| `--out <dir>` | where to write (default `runs/`) |
| `--judge` | judge the implementation scenarios (needs the key) |
| `--ensure-indexed` | index each scenario's root into Nudox first |
| `--keep-responses` | keep every response body in `report.json` — large, but the only way to debug a score |
| `--scenarios <dir>` | load scenarios from elsewhere (default `scenarios/`) |

### Configs

| config | adapters |
|---|---|
| `configs/baselines.toml` | ripgrep, rust-analyzer — no endpoint needed |
| `configs/default.toml` | nudox-full against both baselines |
| `configs/ablations.toml` | all five Nudox policies plus ripgrep |
| `configs/everything.toml` | all five policies plus both baselines — what the charts come from |

The five policies are client-side restrictions on the same running server, so
a whole sweep is one command and no rebuild: `full`, `oneshot` (stop after one
call), `search-only`, `graph-first` (structural questions through the escape
hatch), `keys` (echo 64-hex keys instead of addresses).

### Output

Each run writes a timestamped directory:

```
runs/run-YYYY-MM-DD-HHMMSS/
├── report.json     every attempt, fact verdict, cost, and decline reason
└── report.md       the same, as tables
```

`runs/` is gitignored. To publish a run, copy it under `results/<date>/` and
render the charts:

```bash
python3 scripts/plot_results.py runs/run-YYYY-MM-DD-HHMMSS results/<date>/charts
```

Every number plotted comes from that run's `report.json` — nothing is smoothed
or hand-entered.

### How to read the columns

`pass` requires all three of: full strict recall, the `any_of` gate satisfied,
and zero distractors. `strict` counts a fact found as a fully-qualified path;
`loose` counts it as a bare identifier anywhere in the returned text; the
**gap** between them is how much of an adapter's apparent success is
unresolved text. `declined` and `unscored` never enter a pass rate.

There is deliberately no single headline score — see
[DESIGN.md](DESIGN.md#no-headline-number).

---

## 5. Troubleshooting

| symptom | cause | fix |
|---|---|---|
| `` `#![feature]` may not be used on the stable release channel `` | built the endpoint from the repo root | `cd endpoint` first — the toolchain is resolved from the working directory |
| `failed to authenticate when downloading repository` | building `nudox-engine` from inside Backend's workspace | build via `endpoint/`, which stands outside that workspace |
| `could not compile pyrefly … (signal: 9, SIGKILL)` | the OOM killer — `pyrefly` and `ra_ap_*` are large, and `-C opt-level=3` codegen holds a lot per job | `cargo build --release -j 2`, and stop any running endpoint first (it holds an indexed corpus in memory) |
| the endpoint vanishes mid-run; `Connection refused` on the next call | the OOM killer — the embedder alone is **~4.9 GB resident**, flat, and the machine had less than that free | free memory before starting (`MemAvailable` ≥ ~5.5 GB), add swap, and never build beside a running endpoint. Indexing fewer packages barely helps: the cost is the model, not the corpus |
| `~sem:building(0/1)` in a response, and a scenario scores 0 | the semantic index was still being built when the question arrived — embeddings are computed after a package loads, not before | wait for it to settle after start-up before running, or the run measures a warm-up race rather than the tool |
| `no such file or directory: ../../Backend/...` | Backend is not a sibling, or you are in a git worktree | put the checkouts side by side; build from the main checkout |
| `MISSING ${NODE_MODULES}/commander` | the variable is unset and survived into the path verbatim | `export NODE_MODULES=…` |
| `MISSING rust-analyzer is not runnable` | the rustup shim exists but the component does not | `rustup component add rust-analyzer`, or set `RUST_ANALYZER_BIN` |
| `connecting to http://127.0.0.1:…: connection refused` | the endpoint stopped, or the port changed on relaunch | re-read `NUDOX_MCP_URL`/`NUDOX_MCP_TOKEN` from the running endpoint |
| `~sem:unavailable(NoModelConfigured)` on every row | no embedding model | set `NUDOX_EMBED_MODEL_DIR` (§3.1) |
| `~sem:unavailable(ModelFailed)` | model configured but unloadable | check the file, its sha256, and available memory |
| `is not loaded in this corpus (loaded: …)` | Nudox was never given that package | add it with `--package`, or run with `--ensure-indexed` |
| `WARN <pkg> failed to load` | a producer is missing its oracle | §3.2 for Go; §3.3 for Python |
| `oracle: loading packages under …: go command required, not found` | the Go oracle is installed but `go` itself is not on the endpoint's `PATH` — the oracle shells out to it | §3.2; put a Go toolchain on `PATH` before starting the endpoint |
| `toolchain missing for python (package pypi:…)` | the build has no Python producer registered | rebuild the endpoint: the `pyrefly` feature, §3.3 |
| `unscored` on the implementation scenarios | no judge key | `export OPENROUTER_API_KEY` and pass `--judge` |
| `~ablation:not-exercised` on `nudox-keys` | `search` rendered addresses, so no 64-hex key crossed the wire | expected today; it is reported rather than papered over |
| a run takes minutes before answering anything | indexing runs a real compiler front end | expected; keep one endpoint up across runs |

---

## 6. In CI

The baselines need no endpoint, no key and no network, so the whole tier-1 loop
is three commands:

```bash
cargo test
cargo run --bin nbench -- list        # refuses a scenario that cannot fail
cargo run --bin nbench -- doctor      # non-zero if anything a run needs is missing
cargo run --bin nbench -- run --config configs/baselines.toml
```

Deterministic scoring is what makes that safe: no temperature, no panel drift,
no model deprecation quietly invalidating last month's numbers. The same run
gives the same table.
