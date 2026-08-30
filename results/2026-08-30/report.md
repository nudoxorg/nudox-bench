# nudox-bench run `run-2026-08-30-165221`

2026-08-30T16:55:31.733555398-04:00

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
| rust-analyzer | 0 | 0 | 0 |

## Results

`strict` counts a fact found as a fully-qualified path; `loose` counts it found as a bare identifier anywhere in the returned text. The gap between them is how much of an adapter's apparent success is unresolved text.

| adapter | pass | attempted | unsup. | unscored | strict | loose | gap | distractors | tok/scenario | tok/pass | calls |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nudox-full | 3/6 (50%) | 6 | 3 | 1 | 0.62 | 0.62 | 0.00 | 0 | 1277 | 2555 | 2.2 |
| nudox-oneshot | 2/6 (33%) | 6 | 3 | 1 | 0.36 | 0.38 | 0.03 | 0 | 749 | 2248 | 1.0 |
| nudox-search-only | 2/7 (29%) | 7 | 1 | 2 | 0.39 | 0.41 | 0.02 | 0 | 292 | 1023 | 1.0 |
| nudox-graph-first | 2/6 (33%) | 6 | 3 | 1 | 0.45 | 0.45 | 0.00 | 0 | 1230 | 3688 | 2.2 |
| nudox-keys | 3/6 (50%) | 6 | 3 | 1 | 0.62 | 0.62 | 0.00 | 0 | 1277 | 2555 | 2.2 |
| ripgrep | 5/7 (71%) | 7 | 1 | 2 | 0.95 | 1.00 | 0.05 | 1 | 9658 | 13522 | 1.0 |
| rust-analyzer | 1/4 (25%) | 4 | 5 | 1 | 0.35 | 0.39 | 0.04 | 0 | 306 | 1226 | 1.5 |

## Per scenario

| scenario | family | nudox-full | nudox-oneshot | nudox-search-only | nudox-graph-first | nudox-keys | ripgrep | rust-analyzer |
|---|---|---|---|---|---|---|---|---|
| `go/uuid/callers-parse` | polyglot | n/a | n/a | fail 0% (104) | n/a | n/a | fail 100% (10612) | n/a |
| `go/uuid/implement-namespaced-ids` | implementation | n/a | n/a | unscored | n/a | n/a | unscored | n/a |
| `py/requests/callers-api-request` | polyglot | n/a | n/a | fail 0% (103) | n/a | n/a | pass (7214) | n/a |
| `rust/hashbrown/diff-0.16.1-to-0.17.1` | retrieval | fail 0% (3745) | fail 0% (3745) | n/a | fail 0% (3745) | fail 0% (3745) | n/a | n/a |
| `rust/memchr/callers-search-slice-with-raw` | retrieval | pass (1602) | fail 0% (164) | fail 0% (164) | fail 0% (1367) | pass (1602) | fail 67% (27487) | fail 0% (824) |
| `rust/memchr/implement-three-byte-scan` | implementation | unscored | unscored | unscored | unscored | unscored | unscored | unscored |
| `rust/memchr/iter-return-type` | efficiency | pass (315) | pass (136) | pass (136) | pass (315) | pass (315) | pass (544) | pass (223) |
| `rust/memchr/public-surface` | retrieval | fail 60% (1205) | fail 13% (50) | fail 60% (1030) | fail 60% (1205) | fail 60% (1205) | pass (18047) | fail 40% (119) |
| `rust/memchr/signature-memchr3` | retrieval | pass (455) | pass (352) | pass (352) | pass (455) | pass (455) | pass (987) | fail 0% (39) |
| `ts/commander/public-surface` | polyglot | fail 10% (279) | fail 0% (49) | fail 10% (153) | fail 10% (279) | fail 10% (279) | pass (852) | n/a |

## Declined

Scenarios an adapter could not express. Not failures — these are the shape of each tool's reach.

- `go/uuid/callers-parse` / **nudox-full** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:hashbrown, cargo:memchr, npm:commander)
- `go/uuid/implement-namespaced-ids` / **nudox-full** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:hashbrown, cargo:memchr, npm:commander)
- `py/requests/callers-api-request` / **nudox-full** — `pypi:requests` is not loaded in this corpus (loaded: cargo:hashbrown, cargo:memchr, npm:commander)
- `go/uuid/callers-parse` / **nudox-oneshot** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:hashbrown, cargo:memchr, npm:commander)
- `go/uuid/implement-namespaced-ids` / **nudox-oneshot** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:hashbrown, cargo:memchr, npm:commander)
- `py/requests/callers-api-request` / **nudox-oneshot** — `pypi:requests` is not loaded in this corpus (loaded: cargo:hashbrown, cargo:memchr, npm:commander)
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **nudox-search-only** — policy forbids `diff`
- `go/uuid/callers-parse` / **nudox-graph-first** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:hashbrown, cargo:memchr, npm:commander)
- `go/uuid/implement-namespaced-ids` / **nudox-graph-first** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:hashbrown, cargo:memchr, npm:commander)
- `py/requests/callers-api-request` / **nudox-graph-first** — `pypi:requests` is not loaded in this corpus (loaded: cargo:hashbrown, cargo:memchr, npm:commander)
- `go/uuid/callers-parse` / **nudox-keys** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:hashbrown, cargo:memchr, npm:commander)
- `go/uuid/implement-namespaced-ids` / **nudox-keys** — `go:github.com/google/uuid` is not loaded in this corpus (loaded: cargo:hashbrown, cargo:memchr, npm:commander)
- `py/requests/callers-api-request` / **nudox-keys** — `pypi:requests` is not loaded in this corpus (loaded: cargo:hashbrown, cargo:memchr, npm:commander)
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **ripgrep** — comparing two versions needs two checkouts; the scenario carries one
- `go/uuid/callers-parse` / **rust-analyzer** — rust-analyzer does not speak go; this is a real limit of the comparison, not a harness gap
- `go/uuid/implement-namespaced-ids` / **rust-analyzer** — rust-analyzer does not speak go; this is a real limit of the comparison, not a harness gap
- `py/requests/callers-api-request` / **rust-analyzer** — rust-analyzer does not speak pypi; this is a real limit of the comparison, not a harness gap
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **rust-analyzer** — an LSP server answers about the checkout it has, so it cannot compare two versions
- `ts/commander/public-surface` / **rust-analyzer** — rust-analyzer does not speak npm; this is a real limit of the comparison, not a harness gap
