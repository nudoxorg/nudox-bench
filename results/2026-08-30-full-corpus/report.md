# nudox-bench run `run-2026-08-30-225507`

2026-08-30T22:58:53.077767013-04:00

## ⚠ Degraded capabilities during this run

A system reported part of itself unavailable while answering. Every number below was measured in that state — read them as a result about the configuration that ran, not about the system at full capability.

- **nudox-full** — `~sem:building(0/1)`
- **nudox-oneshot** — `~sem:building(0/1)`
- **nudox-search-only** — `~sem:building(0/1)`
- **nudox-graph-first** — `~sem:building(0/1)`
- **nudox-keys** — `~sem:building(0/1)`
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

`matched` counts only facts on a line the adapter actually matched, not ones it printed in the window around a match. Where it falls below `strict`, that much of the recall was surrounded rather than found. It equals `strict` for adapters that return only what they were asked for.

| adapter | pass | attempted | unsup. | unscored | strict | matched | loose | gap | distractors | tok/scenario | tok/pass | calls |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nudox-full | 4/8 (50%) | 8 | 0 | 2 | 0.73 | 0.73 | 0.73 | 0.00 | 1 | 2471 | 4942 | 2.1 |
| nudox-oneshot | 2/8 (25%) | 8 | 0 | 2 | 0.27 | 0.27 | 0.29 | 0.02 | 1 | 574 | 2298 | 1.0 |
| nudox-search-only | 3/7 (43%) | 7 | 1 | 2 | 0.51 | 0.51 | 0.54 | 0.02 | 1 | 1254 | 2927 | 1.0 |
| nudox-graph-first | 3/8 (38%) | 8 | 0 | 2 | 0.56 | 0.56 | 0.56 | 0.00 | 0 | 2634 | 7023 | 2.2 |
| nudox-keys | 4/8 (50%) | 8 | 0 | 2 | 0.73 | 0.73 | 0.73 | 0.00 | 1 | 2471 | 4942 | 2.1 |
| ripgrep | 6/7 (86%) | 7 | 1 | 2 | 0.95 | 0.57 | 1.00 | 0.05 | 0 | 8447 | 9855 | 1.0 |
| rust-analyzer | 1/4 (25%) | 4 | 5 | 1 | 0.35 | 0.35 | 0.39 | 0.04 | 0 | 306 | 1226 | 1.5 |

## Per scenario

| scenario | family | nudox-full | nudox-oneshot | nudox-search-only | nudox-graph-first | nudox-keys | ripgrep | rust-analyzer |
|---|---|---|---|---|---|---|---|---|
| `go/uuid/callers-parse` | polyglot | fail 50% (1141) | fail 0% (186) | fail 0% (186) | fail 0% (1367) | fail 50% (1141) | pass (2031) | n/a |
| `go/uuid/implement-namespaced-ids` | implementation | unscored | unscored | unscored | unscored | unscored | unscored | n/a |
| `py/requests/callers-api-request` | polyglot | fail 0% (54) | fail 0% (54) | fail 0% (54) | fail 14% (1363) | fail 0% (54) | pass (7326) | n/a |
| `rust/hashbrown/diff-0.16.1-to-0.17.1` | retrieval | fail 75% (7904) | fail 0% (3745) | n/a | fail 75% (7904) | fail 75% (7904) | n/a | n/a |
| `rust/memchr/callers-search-slice-with-raw` | retrieval | pass (1553) | fail 0% (115) | fail 0% (115) | fail 0% (1367) | pass (1553) | fail 67% (27599) | fail 0% (824) |
| `rust/memchr/implement-three-byte-scan` | implementation | unscored | unscored | unscored | unscored | unscored | unscored | unscored |
| `rust/memchr/iter-return-type` | efficiency | pass (266) | pass (87) | pass (87) | pass (266) | pass (266) | pass (656) | pass (223) |
| `rust/memchr/public-surface` | retrieval | fail 60% (1156) | fail 13% (50) | fail 60% (981) | fail 60% (1156) | fail 60% (1156) | pass (18159) | fail 40% (119) |
| `rust/memchr/signature-memchr3` | retrieval | pass (406) | pass (303) | pass (303) | pass (406) | pass (406) | pass (627) | fail 0% (39) |
| `ts/commander/public-surface` | polyglot | pass (7153) | fail 0% (49) | pass (7027) | pass (7153) | pass (7153) | pass (956) | n/a |

## Declined

Scenarios an adapter could not express. Not failures — these are the shape of each tool's reach.

- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **nudox-search-only** — policy forbids `diff`
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **ripgrep** — comparing two versions needs two checkouts; the scenario carries one
- `go/uuid/callers-parse` / **rust-analyzer** — rust-analyzer does not speak go; this is a real limit of the comparison, not a harness gap
- `go/uuid/implement-namespaced-ids` / **rust-analyzer** — rust-analyzer does not speak go; this is a real limit of the comparison, not a harness gap
- `py/requests/callers-api-request` / **rust-analyzer** — rust-analyzer does not speak pypi; this is a real limit of the comparison, not a harness gap
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **rust-analyzer** — an LSP server answers about the checkout it has, so it cannot compare two versions
- `ts/commander/public-surface` / **rust-analyzer** — rust-analyzer does not speak npm; this is a real limit of the comparison, not a harness gap
