# Running against a live Nudox endpoint

## Where the URL and token come from

Nudox's MCP server is **not a standalone binary**. It is started inside the
`lindsey` GUI process by `McpHost::start`
(`Backend/workspace/nudox-engine/src/mcp/host.rs`), and it is:

* **streamable HTTP, never stdio** — the host is a GUI process and does not
  own its stdin/stdout, so a stdio transport would corrupt both
  (`endpoint.rs`, "Why streamable HTTP and not stdio");
* **loopback only** — `LOOPBACK_BIND` is a constant and every branch of
  `preferred_bind` hard-codes `127.0.0.1`;
* **on an ephemeral port** — so two windows on one machine cannot collide, or
  worse, silently attach an agent to the other window's corpus. A remembered
  port is reused when it is free;
* **token-gated** — a per-launch token, enforced in a layer *in front of* the
  MCP router, so an unauthenticated request is rejected before a session is
  allocated.

Both values are therefore inputs to this harness rather than something it can
discover. The running host exposes them:

* `McpEndpoint::url()` → `http://127.0.0.1:<port>/mcp`
* `McpEndpoint::client_config_snippet()` → the JSON shown in the GUI under
  **Settings → Connection**, which carries the `Authorization: Bearer …`
  header value.

```bash
export NUDOX_MCP_URL='http://127.0.0.1:PORT/mcp'
export NUDOX_MCP_TOKEN='<launch token>'
```

Because the token is minted fresh per launch, it is a credential and is read
from the environment — configs name the variable, never the value.

## Check the connection first

```bash
cargo run --bin nbench -- surface --per-tool
```

This performs the MCP handshake, lists the tools, and prints what the surface
costs an agent before any question — the number
`Backend/docs/MCP-SURFACE-PLAN.md` §1.1 tracks. If the token is wrong you get
an explicit 401 message rather than a decode failure.

It is also the cheapest regression test for the surface work: §1.1 measured
the pre-consolidation 13-tool surface at **3,875 cl100k tokens** resident.
The surface has since consolidated to nine tools (`search`, `read`, `refs`,
`packages`, `select_version`, `diff`, `index`, `graph`, `schema`); this command
says what it costs now.

## Version skew, and `--ensure-indexed`

The file-based adapters read `target.root` — a checkout on this machine.
Nudox answers from its own corpus. If those hold different versions of the
same package, the run measures the skew rather than the tools.

`--ensure-indexed` closes that: Nudox's `index` tool accepts a **filesystem
path** as a target, so the harness indexes the very roots `ripgrep` and
rust-analyzer read.

```bash
cargo run --bin nbench -- run --config configs/default.toml --ensure-indexed
```

It is off by default because indexing runs a real compiler front end — seconds
to minutes per package. `index` also returns `running` rather than failing
when its deadline expires, so the preflight logs the whole response instead of
inventing a verdict the tool did not give.

## rust-analyzer

`rustup` puts a **shim** named `rust-analyzer` on `PATH` whether or not the
component is installed for the active toolchain. The shim exits with
`Unknown binary` — so `which rust-analyzer` succeeds and spawning it fails.
Either install the component, or name the real binary:

```bash
rustup component add rust-analyzer
# or
export RUST_ANALYZER_BIN=~/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/bin/rust-analyzer
```

## Status

The Nudox adapter compiles and is wired end to end, but **has not been run
against a live endpoint yet**. Two things are in the way, neither of them in
this repo:

1. **No standalone server.** Bringing up an endpoint means running the GUI
   host, or writing a small binary that calls `McpEndpoint::start` and prints
   its URL and token. The latter is a few dozen lines and would make this
   benchmark runnable in CI; it belongs in `Backend`, not here.
2. **The Backend nix devshell currently fails to build.** `nix develop
   Backend` dies in the `smolvm` derivation:
   `install: cannot stat 'target/release/smolvm': No such file or directory`,
   after a 13-minute release build. Until that is fixed, `nudox-engine` cannot
   be built through the flake that supplies its `clang` linker.

This is exactly why the harness talks to the endpoint **over the wire** rather
than linking `nudox-engine`: the benchmark builds in about ten seconds and
stays runnable while the Backend build is broken. When an endpoint is
available, `configs/default.toml` and `configs/ablations.toml` run unchanged.
