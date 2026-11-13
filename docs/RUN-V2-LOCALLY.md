# Running Nudox Backend v2 through the benchmark, locally

The `nudox-v2` adapter (`src/adapter/nudox_v2.rs`) drives `backend-mcp` over
stdio. It was written against the Backend source at commit `a01c2b0` and its
unit tests, but **it has not yet been run against a live server**: Backend did
not build on the machine the adapter was written on (see the end of this
page). These are the steps to run it on a machine where it does build, and
what to send back if the adapter misreads what the server says.

## 1. Build Backend

In the Backend checkout. Only two binaries are needed; `backend-mcp` finds
`backend-locald` beside itself.

```sh
cd /path/to/Backend
cargo build --locked -p backend-locald -p backend-mcp
ls target/debug/backend-mcp target/debug/backend-locald
```

The workspace pins `rust-version = 1.97.1`. If your toolchain is older and
cannot be updated, `--ignore-rust-version` lets cargo try anyway.

## 2. Point the harness at it

```sh
cd /path/to/nudox-bench
export NUDOX_MCP_BIN=/path/to/Backend/target/debug/backend-mcp
# only if locald is not next to backend-mcp:
# export BACKEND_LOCALD_BIN=/path/to/backend-locald

# Package roots for the non-Rust scenarios (optional; they decline without):
#   pip install --target ~/pkgs/py requests==2.34.2  && export PY_SITE=~/pkgs/py
#   npm install --prefix ~/pkgs/node commander@12.1.0 && export NODE_MODULES=~/pkgs/node/node_modules
# The Rust roots come from ~/.cargo/registry/src; `cargo fetch` in any project
# depending on the crates, or download them, puts them there.
```

## 3. Check, then run

```sh
cargo run --bin nbench -- doctor --config configs/v2.toml
```

The `nudox-v2` line should say `ok`, with a tool count and resident cost, and
should not print `note: <tool> is not listed` for `backend.index`,
`backend.resolve`, `backend.document`, `backend.references`,
`backend.graph` or `backend.outline`.

Start with a few Rust scenarios — indexing is done once per package root
before any scenario runs, and can take minutes per root:

```sh
RUST_LOG=info cargo run --bin nbench -- run --config configs/v2.toml \
    --filter rust/memchr --keep-responses --out runs
```

then everything:

```sh
RUST_LOG=info cargo run --bin nbench -- run --config configs/v2.toml --keep-responses
```

`--keep-responses` stores every tool response in `report.json`; keep it on
for the first runs, it is what makes a mis-parse diagnosable.

## 4. If the adapter misreads the live server

The places where it can go wrong, and the symptom of each:

| symptom in the report | likely cause |
|---|---|
| every nudox-v2 row `unsupported: index not ready: ...` | the index receipt is shaped differently from `index_operation` / `index_poll` as read in `index_verdict` |
| rows that score 0% with a `~v2:no-coordinate-for(name)` degradation | `backend.resolve` rows are not under `structuredContent.records[].identity`, or no row carries the probe's name |
| `backend.call→backend.diff` steps erroring | diff needs both versions acquired first, or rejects the purls (`pkg:cargo/hashbrown@0.16.1`) |
| `errored` on many rows | a tool rejects the argument names (`coordinate`, `query`, `limit`, `path`, `from`, `to`) |

Send back, for one failing scenario of each kind:

1. the `RUST_LOG=info` console output of the run (it logs each root's index
   outcome);
2. the run's `report.json` (with `--keep-responses`);
3. the output of `cargo run --bin nbench -- doctor --config configs/v2.toml`;
4. one raw exchange from the server, captured outside the harness:

```sh
printf '%s\n' \
 '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"probe","version":"0"}}}' \
 '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
 '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"backend.index","arguments":{"path":"'"$ROOT"'"}}}' \
 | $NUDOX_MCP_BIN --project "$ROOT" > index-reply.jsonl
```

with `ROOT` set to a package root (e.g. `~/.cargo/registry/src/*/memchr-2.8.3`),
and the same with `backend.resolve` `{"query":"memchr3"}` once indexing has
finished. Those two replies are enough to correct the parsers.

## Why it did not run where it was written

Recorded so the attempt is not repeated blind:

* `rustup toolchain install 1.97.1` could not download (static.rust-lang.org
  unreachable from that machine); the build used 1.97.0 with
  `--ignore-rust-version`.
* With 2 cores and a 7 GB memory limit, the build compiled several hundred
  crates and was then killed by the kernel's OOM killer compiling the vendored
  TypeScript checker `tsz-checker` (`third_party/tsz/crates/tsz-checker`,
  built at `opt-level = 2` even in the dev profile): rustc reached ~3.6 GB
  resident, at `-j 2` and again at `-j 1`. The outcome of a third attempt
  with that crate's `opt-level` lowered is recorded in the run notes.
