# Running it

Three things to know before the commands: what needs no setup, what the
endpoint is, and why the endpoint binary exists.

---

## 1. The part that needs nothing

The baselines run offline, with no endpoint, no API key and no network:

```bash
cargo run --bin nbench -- doctor      # check the machine is ready
cargo run --bin nbench -- run --config configs/baselines.toml
```

`doctor` is the fastest way to see what is and is not set up. It reports every
scenario's package root, whether `rg` and rust-analyzer actually *run* (not
just whether they exist on `PATH` — see below), whether the Nudox endpoint
answers, and whether a judge key is present. It exits non-zero if a run would
be degraded, so it works in CI.

### Package roots

Scenarios point at packages already on your machine, written with variables so
the committed files are portable:

| variable | resolved automatically from | used by |
|---|---|---|
| `${CARGO_SRC}` | `~/.cargo/registry/src/<index-hash>/` | memchr, hashbrown |
| `${GO_MOD}` | `~/go/pkg/mod` | google/uuid |
| `${PY_SITE}` | `/usr/lib/python3.X/site-packages` | requests |
| `${NODE_MODULES}` | **not guessable — set it yourself** | commander |

```bash
export NODE_MODULES=/path/to/some/node_modules
```

An unset variable is left in the path verbatim, so `doctor` shows you
`MISSING ${NODE_MODULES}/commander` rather than a confusing `/commander`.

### rust-analyzer

`rustup` puts a **shim** named `rust-analyzer` on `PATH` whether or not the
component is installed for the active toolchain. The shim exits with
`Unknown binary` — so `which` succeeds and spawning fails, which is why
`doctor` runs it rather than looking for it.

```bash
rustup component add rust-analyzer
# or point at one you have:
export RUST_ANALYZER_BIN=~/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/bin/rust-analyzer
```

---

## 2. Getting a Nudox endpoint

Nudox's MCP server is **not a standalone binary**. It is started inside the
`lindsey` GUI process by `McpHost::start`, and it is streamable-HTTP only
(the GUI does not own its stdin/stdout), loopback only, on an ephemeral port,
gated by a bearer token minted fresh each launch.

A benchmark cannot drive a GUI, so this repo ships the missing piece:
`endpoint/` — a small standalone binary that builds an engine over the package
roots you name, starts the same `McpEndpoint` the GUI starts, and prints its
URL and token.

### Build it once

```bash
cd endpoint          # NOT from the repo root — see the note below
cargo build --release
```

Two things about that build:

* **It must be run from inside `endpoint/`.** `rustup` resolves
  `rust-toolchain.toml` from the *working directory*, not from
  `--manifest-path`. Backend uses `#![feature(...)]` and needs nightly;
  the harness itself is stable and stays that way. Running
  `cargo build --manifest-path endpoint/Cargo.toml` from the repo root picks
  up the root's stable pin and fails with
  `` `#![feature]` may not be used on the stable release channel ``.
* **It is slow the first time** — it compiles `nudox-engine` and its
  dependency graph (`ra_ap_*`, pyrefly, ONNX runtime). Budget 20–40 minutes.
  Use `--release`: a debug build indexes packages several times slower, and
  indexing is what you wait on at every start.

It does **not** need the nix devshell, and it does not need `clang`. See
"Why this builds outside nix" below.

### Start it

```bash
cd endpoint
./target/release/nudox-endpoint \
  --package "$HOME/.cargo/registry/src"/*/memchr-2.8.3 \
  --package "$HOME/.cargo/registry/src"/*/hashbrown-0.17.1
```

Name, version and language are read from the manifest in each directory, so a
bare path is enough — the same rule the `index` tool follows for a path
target. Override with `PATH=name@version` if the directory name lies.

It prints:

```
NUDOX_MCP_URL=http://127.0.0.1:41337/mcp
NUDOX_MCP_TOKEN=…
```

and stays up until Ctrl-C. Indexing runs a real compiler front end, so
start-up is tens of seconds per package.

### Point the benchmark at it

In another shell:

```bash
export NUDOX_MCP_URL=http://127.0.0.1:41337/mcp
export NUDOX_MCP_TOKEN=…

cargo run --bin nbench -- surface --per-tool          # sanity check + cost table
cargo run --bin nbench -- run --config configs/default.toml
cargo run --bin nbench -- run --config configs/ablations.toml
```

Or in one step, with `--export`:

```bash
eval "$(cd endpoint && ./target/release/nudox-endpoint --package … --export)"
```

---

## 3. `surface`: the cheapest useful command

```bash
cargo run --bin nbench -- surface --per-tool
```

Handshakes, lists the tools, and prints what the surface costs an agent
*before it asks anything* — every tool description plus every argument-schema
description, under both `cl100k_base` and `o200k_base`.

This is a direct regression test for `Backend/docs/MCP-SURFACE-PLAN.md` §1.1,
which measured the pre-consolidation 13-tool surface at **3,875 cl100k tokens**
resident. The surface has since consolidated to nine tools (`search`, `read`,
`refs`, `packages`, `select_version`, `diff`, `index`, `graph`, `schema`);
this command says what it costs now, per tool, largest first.

---

## 4. `verify`: does any of this reproduce?

Every claim in `docs/DESIGN.md` rests on the scoring being deterministic —
anyone can re-run these and get the same numbers. A single report cannot
demonstrate that. It shows whichever value that run happened to land on, and a
verdict that quietly alternates looks exactly like one that holds.

```sh
cargo run -- verify --config configs/baselines.toml --repeat 3
```

It runs the config N times and separates two kinds of movement, because they
are not equally serious:

- **a verdict that changes is an alarm.** A published pass rate is then partly
  luck, and no amount of averaging repairs it. `verify` exits non-zero for
  this, so it works as a CI gate.
- **a cost that changes is ordinary** for anything crossing a process or a
  network. It is reported as a spread relative to its own floor, so a
  200-token wobble on a 20,000-token answer does not read like the same event
  as one on a 300-token answer. Tune with `--drift`.

Recall that moves while the verdict holds is counted as a *failure to
reproduce*, not a pass: the verdict survived on where the threshold happened
to sit, which is luck wearing the same clothes as a result.

`--repeat 1` is refused. One run cannot disagree with itself, and a number
that cannot fail is not evidence.

## 5. Nudox on `canonical`

`canonical` replaced `workspace/nudox-engine` with a
`compiler/ interface/ server/ heart/` layout. The MCP endpoint §2 describes
does not exist there, so the `nudox` adapter cannot reach it — pointing it at
canonical fails at connect and reads as an outage rather than as the truth.

Use the `nudox-canonical` adapter, which speaks the application protocol that
canonical actually ships:

```sh
cd ../Backend && cargo build -p interface-cli
export NUDOX_CANONICAL_CLI=$PWD/target/debug/interface-cli
cd ../nudox-bench && cargo run -- run --config configs/canonical.toml
```

Check it first — `doctor` reads the capability roll-call, not just whether the
binary starts:

```sh
cargo run -- doctor --config configs/canonical.toml
```

At the time of writing that reports five capabilities unavailable, `index`
among them, and every scenario comes back **unsupported** rather than scoring
zero. The index plane under `server/index/*` is built but not wired to
`interface-core`; there is no flag that turns it on. See
`results/2026-09-03-canonical/`.

## 6. Version skew, and `--ensure-indexed`

The file-based adapters read `target.root`. Nudox answers from its corpus. If
those hold different versions of the same package, the run measures the skew
rather than the tools.

`--ensure-indexed` closes it: Nudox's `index` tool accepts a **filesystem
path**, so the harness indexes the very roots `ripgrep` and rust-analyzer
read.

```bash
cargo run --bin nbench -- run --config configs/default.toml --ensure-indexed
```

Off by default because indexing costs seconds to minutes per package. If you
already started the endpoint with `--package` for those same roots, you do not
need it.

---

## Why this builds outside nix

Worth recording, because the obvious paths all fail and the reasons are not
what they look like.

* **`nix develop Backend` fails.** The `smolvm` derivation builds for ~13
  minutes and dies at `install: cannot stat 'target/release/smolvm'`. But
  smolvm is only the launcher for opt-in sandbox VM tests — nothing
  `nudox-engine` needs.
* **`cargo check -p nudox-engine` inside Backend also fails**, and not because
  of a linker. Backend's workspace includes `workspace/compiler/sandbox`,
  which depends on `smolvm` by git, whose `libkrun` submodule is fetched over
  SSH from a private repo. Cargo resolves the *whole workspace* before
  building any member, so resolution dies with "failed to authenticate when
  downloading repository". `nudox-engine` itself has **no git dependencies at
  all**.
* **There is no `.cargo/config.toml`** in Backend, so the often-repeated
  "Backend needs clang as its linker" is no longer true. `clang` is not
  installed on this machine and the endpoint builds anyway.

So `endpoint/` is a standalone package — its own `[workspace]`, its own
lockfile — which makes cargo resolve only what `nudox-engine` actually needs.
It is the same trick `lindsey` uses for the opposite reason (keeping gpui out
of the backend graph).

### Running `nudox-engine`'s own tests

`endpoint/` gets the engine *compiled*, which is enough to check a change
builds. It does not run the engine's tests: cargo does not run a dependency's
test targets, and `cargo test -p nudox-engine` inside Backend hits the same
resolution wall.

One line unblocks it. The dependency that cannot resolve is declared in
exactly one place:

```
workspace/compiler/sandbox/Cargo.toml:19
smolvm = { git = "https://github.com/smol-machines/smolvm", rev = "…" }
```

Comment it out and workspace resolution succeeds, because nothing
`nudox-engine` needs reaches `sandbox` — it is an *optional* dependency of
`index`, and `cargo test -p nudox-engine` never builds it either way.

```bash
cd ../Backend
cp workspace/compiler/sandbox/Cargo.toml /tmp/sandbox-Cargo.toml.bak
sed -i 's|^smolvm = { git = .*|# temporarily disabled to run engine tests|' \
    workspace/compiler/sandbox/Cargo.toml

cargo test -p nudox-engine        # now resolves

cp /tmp/sandbox-Cargo.toml.bak workspace/compiler/sandbox/Cargo.toml
git checkout Cargo.lock            # ← do not skip this
```

**Restore `Cargo.lock` as well as the manifest.** Resolving without `smolvm`
rewrites the lockfile and drops ~500 lines of its transitive tree; committing
that would be a much larger and more confusing change than whatever you were
actually working on. Check `git status` before you commit either way.

This is a workaround for a local credential gap, not a fix: on a machine with
SSH access to that private repo, none of it is necessary.

One more thing was needed: `nudox-engine`'s default `local-persistence`
feature gates `ir-vcs`, which uses `libpijul::nudox_f1` — a symbol that exists
only in the vendored fork supplied by Backend's root `[patch.crates-io]`, and
a standalone package inherits no patch table. That feature makes produced IR
survive a restart, which a benchmark that re-indexes at every start does not
need, so `endpoint/Cargo.toml` sets `default-features = false` rather than
carrying a copy of someone else's vendored build configuration.
