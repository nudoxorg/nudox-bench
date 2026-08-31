# nudox-bench run `run-2026-08-31-145059`

2026-08-31T14:51:10.972805930-04:00

## ⚠ Degraded capabilities during this run

A system reported part of itself unavailable while answering. Every number below was measured in that state — read them as a result about the configuration that ran, not about the system at full capability.

- **nudox-full** — `~sem:unavailable(NoModelConfigured)`
- **nudox-oneshot** — `~sem:unavailable(NoModelConfigured)`
- **nudox-search-only** — `~sem:unavailable(NoModelConfigured)`
- **nudox-graph-first** — `~sem:unavailable(NoModelConfigured)`
- **nudox-keys** — `~sem:unavailable(NoModelConfigured)`
- **nudox-keys** — `~ablation:not-exercised(address_mode=Key; search returned no 64-hex key to send back)`

## Surface cost before any question

What each system puts in an agent's context before it is asked anything: tool descriptions plus every argument-schema description. Paid once per session.

| adapter | cl100k | o200k | bytes |
|---|---:|---:|---:|
| nudox-full | 3761 | 3742 | 15745 |
| nudox-oneshot | 3761 | 3742 | 15745 |
| nudox-search-only | 3761 | 3742 | 15745 |
| nudox-graph-first | 3761 | 3742 | 15745 |
| nudox-keys | 3761 | 3742 | 15745 |
| ripgrep | 0 | 0 | 0 |

## Results

`strict` counts a fact found as a fully-qualified path; `loose` counts it found as a bare identifier anywhere in the returned text. The gap between them is how much of an adapter's apparent success is unresolved text.

`matched` counts only facts on a line the adapter actually matched, not ones it printed in the window around a match. Where it falls below `strict`, that much of the recall was surrounded rather than found. It equals `strict` for adapters that return only what they were asked for.

| adapter | pass | attempted | unsup. | unscored | strict | matched | loose | gap | distractors | tok/scenario | tok/pass | calls |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nudox-full | 5/5 (100%) | 5 | 4 | 1 | 1.00 | 1.00 | 1.00 | 0.00 | 0 | 4464 | 4464 | 2.4 |
| nudox-oneshot | 2/5 (40%) | 5 | 4 | 1 | 0.43 | 0.43 | 0.46 | 0.03 | 0 | 151 | 377 | 1.0 |
| nudox-search-only | 2/7 (29%) | 7 | 1 | 2 | 0.39 | 0.39 | 0.41 | 0.02 | 0 | 292 | 1022 | 1.0 |
| nudox-graph-first | 5/5 (100%) | 5 | 4 | 1 | 1.00 | 1.00 | 1.00 | 0.00 | 0 | 4692 | 4692 | 2.4 |
| nudox-keys | 5/5 (100%) | 5 | 4 | 1 | 1.00 | 1.00 | 1.00 | 0.00 | 0 | 4464 | 4464 | 2.4 |
| ripgrep | 6/7 (86%) | 7 | 1 | 2 | 0.95 | 0.57 | 1.00 | 0.05 | 0 | 8447 | 9855 | 1.0 |

## Per scenario

| scenario | family | nudox-full | nudox-oneshot | nudox-search-only | nudox-graph-first | nudox-keys | ripgrep |
|---|---|---|---|---|---|---|---|
| `go/uuid/callers-parse` | polyglot | n/a | n/a | fail 0% (104) | n/a | n/a | pass (2031) |
| `go/uuid/implement-namespaced-ids` | implementation | n/a | n/a | unscored | n/a | n/a | unscored |
| `py/requests/callers-api-request` | polyglot | n/a | n/a | fail 0% (101) | n/a | n/a | pass (7326) |
| `rust/hashbrown/diff-0.16.1-to-0.17.1` | retrieval | n/a | n/a | n/a | n/a | n/a | n/a |
| `rust/memchr/callers-search-slice-with-raw` | retrieval | pass (1602) | fail 0% (164) | fail 0% (164) | pass (2793) | pass (1602) | fail 67% (27599) |
| `rust/memchr/implement-three-byte-scan` | implementation | unscored | unscored | unscored | unscored | unscored | unscored |
| `rust/memchr/iter-return-type` | efficiency | pass (315) | pass (136) | pass (136) | pass (315) | pass (315) | pass (656) |
| `rust/memchr/public-surface` | retrieval | pass (13172) | fail 13% (50) | fail 60% (1030) | pass (13172) | pass (13172) | pass (18159) |
| `rust/memchr/signature-memchr3` | retrieval | pass (486) | pass (352) | pass (352) | pass (486) | pass (486) | pass (627) |
| `ts/commander/public-surface` | polyglot | pass (6656) | fail 0% (49) | fail 10% (153) | pass (6656) | pass (6656) | pass (956) |

## Declined

Scenarios an adapter could not express. Not failures — these are the shape of each tool's reach.

- `go/uuid/callers-parse` / **nudox-full** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `go/uuid/implement-namespaced-ids` / **nudox-full** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `py/requests/callers-api-request` / **nudox-full** — `pypi:requests` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **nudox-full** — `cargo:hashbrown` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `go/uuid/callers-parse` / **nudox-oneshot** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `go/uuid/implement-namespaced-ids` / **nudox-oneshot** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `py/requests/callers-api-request` / **nudox-oneshot** — `pypi:requests` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **nudox-oneshot** — `cargo:hashbrown` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **nudox-search-only** — policy forbids `diff`
- `go/uuid/callers-parse` / **nudox-graph-first** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `go/uuid/implement-namespaced-ids` / **nudox-graph-first** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `py/requests/callers-api-request` / **nudox-graph-first** — `pypi:requests` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **nudox-graph-first** — `cargo:hashbrown` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `go/uuid/callers-parse` / **nudox-keys** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `go/uuid/implement-namespaced-ids` / **nudox-keys** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `py/requests/callers-api-request` / **nudox-keys** — `pypi:requests` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **nudox-keys** — `cargo:hashbrown` is not loaded in this corpus (loaded: cargo:memchr, npm:commander)
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **ripgrep** — comparing two versions needs two checkouts; the scenario carries one
