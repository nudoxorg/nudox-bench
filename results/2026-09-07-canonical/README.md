# Canonical, 2026-09-07 — the same conclusion, 465 commits later

Backend `canonical` @ `c6fff26b3`, through `interface-cli`, built fresh from a
detached worktree of that revision. Same 15 scenarios and same config as
[`../2026-09-03-expanded/`](../2026-09-03-expanded), against a canonical that has
advanced **465 commits** since — the bulk of them `feat(index)`.

| adapter | pass | attempted | unsupported | strict | matched | distractors | tok/scenario |
|---|---:|---:|---:|---:|---:|---:|---:|
| nudox-canonical | 0/0 | **0** | **15** | — | — | — | — |
| ripgrep | 9/12 (75%) | 12 | 1 | 0.97 | 0.61 | 5 | 8,740 |

## The point of this run: reproduction across revisions, not across repeats

`nbench verify` measures whether a stored configuration repeats. This run is a
stronger test that nothing in the repo had made before — a **freshly compiled
harness** and a **freshly built CLI from a 465-commit-newer Backend**, four days
later, on the same scenario files.

Every verdict, every recall figure, the distractor count and the
strict-versus-matched gap are **identical** to 2026-09-03. The only movement
anywhere in the report is cost:

| | 2026-09-03 | 2026-09-07 | Δ |
|---|---:|---:|---:|
| `rust/memchr/outbound-memmem-find` | 37,744 | 37,190 | **−1.5%** |
| retrieval family, tok/scenario | 14,534 | 14,453 | −0.56% |
| overall tok/scenario | 8,780 | 8,740 | −0.46% |
| overall tok/pass | 11,706 | 11,653 | −0.45% |

That is exactly the split the design calls for: a verdict that moves is an alarm,
a cost that moves is ordinary and reported as a spread against its own floor.

## Reproducibility

`nbench verify --config configs/canonical.toml --repeat 3`:
**30 cells over 3 runs — 100% reproduced exactly**, no cost moved more than 5%.

Cumulative ledger: **21 runs, 182 cells, 100%**.

## Why Nudox still reports 0/0

The architecture moved and the wiring did not. Re-checked against `c6fff26b3`:

| fact | at `2f82d74b` | at `c6fff26b3` |
|---|---|---|
| `ApplicationService` type params | compiler only | **`<Compiler = UnavailableCompiler, Retrieval = UnavailableRetrieval>`** |
| `interface-core` dependencies | no `server-index-*` | **declares `server-index-core`** |
| `server-index-retrieval` consumers | zero | **one — `interface/protocol`** |
| `RetrievalCapability` impls | — | **`UnavailableRetrieval` only** (the sole other impl is a test double in `interface/core/tests/retrieval.rs`) |
| `interface/cli/main.rs:33` | `ApplicationService::new()` | **unchanged** — routes through `with_compiler`, which hardcodes `retrieval: UnavailableRetrieval` |

So a slot an index could occupy now exists, the dependency boundary has opened,
and the retrieval crate is no longer orphaned — but nothing binds a real
implementation, and the CLI the benchmark drives still constructs the unavailable
default. Empirically, on a clean build of `c6fff26b3`:

```
$ interface-cli search 5 primary memchr3 4
{"correlation":5,"body":{"kind":"dependency_unavailable","capability":"index"},
 "terminal":{"kind":"degraded","emitted":0,"unavailable":"index"}}
```

One capability did come online. The health roll-call gained
`compiler_registry: local_ready`, which did not exist in the previous run at all;
`index`, `graph` and `vector` all still report `unavailable`.

## Setup

`doctor --config configs/canonical.toml` was clean before the run — all five
package roots resolved at the versions the scenarios describe, including
commander **12.1.0** (the machine also holds 8.1.0, which the skew check
refuses). ripgrep 15.2.0. No judge key, so the two `implementation` scenarios
report `unscored`.

```sh
git -C ../Backend worktree add --detach /tmp/canon origin/canonical
cargo build -p interface-cli --manifest-path /tmp/canon/Cargo.toml
export NUDOX_CANONICAL_CLI=/tmp/canon/target/debug/interface-cli
export NODE_MODULES=/path/to/node_modules   # must hold commander 12.1.0
nbench run --config configs/canonical.toml --out results/2026-09-07-canonical
```

A readable page over this run is `../../docs/METHOD-2026-09-07.html`.
