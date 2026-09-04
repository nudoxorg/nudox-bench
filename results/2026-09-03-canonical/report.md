# nudox-bench run `run-2026-09-03-220632`

2026-09-03T22:06:35.884261762-04:00

## Surface cost before any question

What each system puts in an agent's context before it is asked anything: tool descriptions plus every argument-schema description. Paid once per session.

| adapter | cl100k | o200k | bytes |
|---|---:|---:|---:|
| nudox-canonical | 0 | 0 | 0 |
| ripgrep | 0 | 0 | 0 |

## Results

`strict` counts a fact found as a fully-qualified path; `loose` counts it found as a bare identifier anywhere in the returned text. The gap between them is how much of an adapter's apparent success is unresolved text.

`matched` counts only facts on a line the adapter actually matched, not ones it printed in the window around a match. Where it falls below `strict`, that much of the recall was surrounded rather than found. It equals `strict` for adapters that return only what they were asked for.

| adapter | pass | attempted | unsup. | unscored | strict | matched | loose | gap | distractors | tok/scenario | tok/pass | calls |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nudox-canonical | 0/0 (0%) | 0 | 10 | 0 | 0.00 | 0.00 | 0.00 | 0.00 | 0 | 0 | — | 0.0 |
| ripgrep | 5/6 (83%) | 6 | 2 | 2 | 0.94 | 0.50 | 1.00 | 0.06 | 0 | 9686 | 11623 | 1.0 |

## Per scenario

| scenario | family | nudox-canonical | ripgrep |
|---|---|---|---|
| `go/uuid/callers-parse` | polyglot | n/a | pass (2031) |
| `go/uuid/implement-namespaced-ids` | implementation | n/a | unscored |
| `py/requests/callers-api-request` | polyglot | n/a | pass (7326) |
| `rust/hashbrown/diff-0.16.1-to-0.17.1` | retrieval | n/a | n/a |
| `rust/memchr/callers-search-slice-with-raw` | retrieval | n/a | fail 67% (27599) |
| `rust/memchr/implement-three-byte-scan` | implementation | n/a | unscored |
| `rust/memchr/iter-return-type` | efficiency | n/a | pass (656) |
| `rust/memchr/public-surface` | retrieval | n/a | pass (18159) |
| `rust/memchr/signature-memchr3` | retrieval | n/a | pass (627) |
| `ts/commander/public-surface` | polyglot | n/a | n/a |

## Declined

Scenarios an adapter could not express. Not failures — these are the shape of each tool's reach.

- `go/uuid/callers-parse` / **nudox-canonical** — canonical reports capability `index` unavailable (emitted 0); the index plane is built but not wired to interface-core — health: compiler_output:unavailable, index:unavailable, graph:unavailable, vector:unavailable, local_analyzer:unavailable
- `go/uuid/implement-namespaced-ids` / **nudox-canonical** — canonical reports capability `index` unavailable (emitted 0); the index plane is built but not wired to interface-core — health: compiler_output:unavailable, index:unavailable, graph:unavailable, vector:unavailable, local_analyzer:unavailable
- `py/requests/callers-api-request` / **nudox-canonical** — canonical reports capability `index` unavailable (emitted 0); the index plane is built but not wired to interface-core — health: compiler_output:unavailable, index:unavailable, graph:unavailable, vector:unavailable, local_analyzer:unavailable
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **nudox-canonical** — canonical reports capability `index` unavailable (emitted 0); the index plane is built but not wired to interface-core — health: compiler_output:unavailable, index:unavailable, graph:unavailable, vector:unavailable, local_analyzer:unavailable
- `rust/memchr/callers-search-slice-with-raw` / **nudox-canonical** — canonical reports capability `index` unavailable (emitted 0); the index plane is built but not wired to interface-core — health: compiler_output:unavailable, index:unavailable, graph:unavailable, vector:unavailable, local_analyzer:unavailable
- `rust/memchr/implement-three-byte-scan` / **nudox-canonical** — canonical reports capability `index` unavailable (emitted 0); the index plane is built but not wired to interface-core — health: compiler_output:unavailable, index:unavailable, graph:unavailable, vector:unavailable, local_analyzer:unavailable
- `rust/memchr/iter-return-type` / **nudox-canonical** — canonical reports capability `index` unavailable (emitted 0); the index plane is built but not wired to interface-core — health: compiler_output:unavailable, index:unavailable, graph:unavailable, vector:unavailable, local_analyzer:unavailable
- `rust/memchr/public-surface` / **nudox-canonical** — canonical reports capability `index` unavailable (emitted 0); the index plane is built but not wired to interface-core — health: compiler_output:unavailable, index:unavailable, graph:unavailable, vector:unavailable, local_analyzer:unavailable
- `rust/memchr/signature-memchr3` / **nudox-canonical** — canonical reports capability `index` unavailable (emitted 0); the index plane is built but not wired to interface-core — health: compiler_output:unavailable, index:unavailable, graph:unavailable, vector:unavailable, local_analyzer:unavailable
- `ts/commander/public-surface` / **nudox-canonical** — canonical reports capability `index` unavailable (emitted 0); the index plane is built but not wired to interface-core — health: compiler_output:unavailable, index:unavailable, graph:unavailable, vector:unavailable, local_analyzer:unavailable
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **ripgrep** — comparing two versions needs two checkouts; the scenario carries one
- `ts/commander/public-surface` / **ripgrep** — target.root does not exist: ${NODE_MODULES}/commander
