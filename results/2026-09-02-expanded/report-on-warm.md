# nudox-bench run `run-2026-09-02-160158`

2026-09-02T16:02:30.556419880-04:00

## ⚠ Degraded capabilities during this run

A system reported part of itself unavailable while answering. Every number below was measured in that state — read them as a result about the configuration that ran, not about the system at full capability.

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
| nudox-full | 7/9 (78%) | 9 | 4 | 1 | 1.00 | 0.89 | 1.00 | 0.00 | 5→3 | 11353 | 14596 | 2.6 |
| nudox-oneshot | 2/9 (22%) | 9 | 4 | 1 | 0.35 | 0.35 | 0.37 | 0.02 | 2 | 1062 | 4780 | 1.0 |
| nudox-search-only | 4/11 (36%) | 11 | 1 | 2 | 0.65 | 0.65 | 0.67 | 0.02 | 5 | 4625 | 12719 | 1.0 |
| nudox-graph-first | 7/9 (78%) | 9 | 4 | 1 | 1.00 | 0.89 | 1.00 | 0.00 | 5→3 | 11151 | 14338 | 2.6 |
| nudox-keys | 7/9 (78%) | 9 | 4 | 1 | 1.00 | 0.89 | 1.00 | 0.00 | 5→3 | 11353 | 14596 | 2.6 |
| ripgrep | 8/11 (73%) | 11 | 1 | 2 | 0.88 | 0.64 | 0.91 | 0.03 | 3 | 8923 | 12269 | 1.0 |

## Per scenario

| scenario | family | nudox-full | nudox-oneshot | nudox-search-only | nudox-graph-first | nudox-keys | ripgrep |
|---|---|---|---|---|---|---|---|
| `go/uuid/callers-parse` | polyglot | n/a | n/a | fail 0% (46) | n/a | n/a | pass (2031) |
| `go/uuid/implement-namespaced-ids` | implementation | n/a | n/a | unscored | n/a | n/a | unscored |
| `py/requests/callers-api-request` | polyglot | n/a | n/a | fail 0% (43) | n/a | n/a | pass (7326) |
| `rust/hashbrown/diff-0.16.1-to-0.17.1` | retrieval | n/a | n/a | n/a | n/a | n/a | n/a |
| `rust/memchr/callers-search-slice-with-raw` | retrieval | pass (4498) | fail 0% (3060) | fail 0% (3060) | pass (2793) | pass (4498) | fail 67% (27599) |
| `rust/memchr/implement-three-byte-scan` | implementation | unscored | unscored | unscored | unscored | unscored | unscored |
| `rust/memchr/iter-return-type` | efficiency | fail 100% (2745) | fail 100% (2566) | fail 100% (2566) | fail 100% (2745) | fail 100% (2745) | pass (656) |
| `rust/memchr/memmem-api` | retrieval | pass (20273) | fail 0% (50) | fail 57% (8131) | pass (20273) | pass (20273) | pass (18159) |
| `rust/memchr/public-surface` | retrieval | pass (21276) | fail 13% (50) | fail 80% (9134) | pass (21276) | pass (21276) | pass (18159) |
| `rust/memchr/reverse-search-family` | retrieval | fail 100% (22447) | fail 0% (50) | fail 83% (10305) | fail 100% (22447) | fail 100% (22447) | fail 100% (18159) |
| `rust/memchr/signature-memchr3` | retrieval | pass (2131) | pass (1997) | pass (1997) | pass (2131) | pass (2131) | pass (627) |
| `ts/commander/error-types` | polyglot | pass (1689) | pass (1586) | pass (1586) | pass (1689) | pass (1689) | pass (689) |
| `ts/commander/option-configuration` | polyglot | pass (13083) | fail 0% (49) | pass (6580) | pass (13083) | pass (13083) | fail 0% (956) |
| `ts/commander/public-surface` | polyglot | pass (13530) | fail 0% (49) | pass (7027) | pass (13530) | pass (13530) | pass (956) |

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
