# Canonical, 2026-09-03 — the index plane is not reachable

Backend `canonical` @ `2f82d74b`, driven through `interface-cli` by the
`nudox-canonical` adapter.

| adapter | pass | attempted | unsupported | tok/scenario |
|---|---:|---:|---:|---:|
| nudox-canonical | 0/0 | **0** | **10** | 0 |
| ripgrep | 5/6 (83%) | 6 | 2 | 9,686 |

## Read the middle columns, not the first

`0/0` is not a score of zero. Every scenario came back **unsupported** — the
adapter could not ask the question at all, and the harness keeps that separate
from a wrong answer on purpose. Averaging the two would publish "Nudox 0%",
which would be both damning and false.

## What canonical says about itself

Its own health roll-call, verbatim:

```
compiler_registry  local_ready
compiler_output    unavailable
index              unavailable
graph              unavailable
vector             unavailable
local_analyzer     unavailable
```

and every retrieval action:

```json
{"body":{"kind":"dependency_unavailable","capability":"index"},
 "terminal":{"kind":"degraded","emitted":0}}
```

## Why it is absent rather than switched off

Three facts, each checked on `2f82d74b`:

- `ApplicationService<Compiler = UnavailableCompiler>` is generic over **only**
  a compiler. There is no slot an index could occupy.
- `interface-core` declares **no dependency** on any `server-index-*` crate,
  and the built `interface-mcp` binary links none of them.
- `server-index-retrieval` has **zero consumers**; `RetrievalResult` returns one
  variant per call, so nothing fuses families.

The plane under `server/index/*` is real and well built. It is simply not
connected yet. No configuration, feature flag, or environment variable changes
this.

## What this run is good for

The ripgrep column is a live baseline taken today, so the moment the plane is
wired the comparison has a same-day floor to be read against. The adapter needs
no change then: it returns real attempts as soon as `index` reports ready.

Reproduce:

```sh
cd ../Backend && cargo build -p interface-cli
export NUDOX_CANONICAL_CLI=$PWD/target/debug/interface-cli
cd ../nudox-bench && cargo run -- run --config configs/canonical.toml
```
