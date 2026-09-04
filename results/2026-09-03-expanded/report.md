# nudox-bench run `run-2026-09-03-233027`

2026-09-03T23:30:32.408545666-04:00

## Surface cost before any question

What each system puts in an agent's context before it is asked anything: tool descriptions plus every argument-schema description. Paid once per session.

| adapter | cl100k | o200k | bytes |
|---|---:|---:|---:|
| nudox-canonical | 0 | 0 | 0 |
| ripgrep | 0 | 0 | 0 |

## Results

`strict` counts a fact found as a fully-qualified path; `loose` counts it found as a bare identifier anywhere in the returned text. The gap between them is how much of an adapter's apparent success is unresolved text.

`matched` counts only facts on a line the adapter actually matched, not ones it printed in the window around a match. Where it falls below `strict`, that much of the recall was surrounded rather than found. It equals `strict` for adapters that return only what they were asked for.

`errored` counts attempts where the tool itself failed — a crash, a dropped connection, a timeout. They stay inside `attempted` because a tool that cannot answer did not answer, but they are shown separately: a column of errors and a column of wrong answers look identical in a pass rate and mean entirely different things.

| adapter | pass | attempted | unsup. | unscored | errored | strict | matched | loose | gap | distractors | tok/scenario | tok/pass | calls |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nudox-canonical | 0/0 (0%) | 0 | 14 | 0 | 0 | 0.00 | 0.00 | 0.00 | 0.00 | 0 | 0 | — | 0.0 |
| ripgrep | 7/10 (70%) | 10 | 2 | 2 | 0 | 0.97 | 0.63 | 1.00 | 0.03 | 5 | 10314 | 14734 | 1.0 |

## By family

The four families ask structurally different things. A single pass rate hides which one an adapter is good at: answering every retrieval question and no polyglot one reports the same headline as being mediocre at both, and the two call for different decisions. A family an adapter never attempted shows `—`, not 0%.

| adapter | family | pass | unsup. | unscored | strict | tok/scenario |
|---|---|---:|---:|---:|---:|---:|
| nudox-canonical | efficiency | — | 1 | 0 | 0.00 | 0 |
| nudox-canonical | implementation | — | 2 | 0 | 0.00 | 0 |
| nudox-canonical | polyglot | — | 5 | 0 | 0.00 | 0 |
| nudox-canonical | retrieval | — | 6 | 0 | 0.00 | 0 |
| ripgrep | efficiency | 1/1 (100%) | 0 | 0 | 1.00 | 664 |
| ripgrep | implementation | — | 0 | 2 | 0.00 | 0 |
| ripgrep | polyglot | 3/4 (75%) | 1 | 0 | 1.00 | 4119 |
| ripgrep | retrieval | 3/5 (60%) | 1 | 0 | 0.93 | 17200 |

## Per scenario

| scenario | family | nudox-canonical | ripgrep |
|---|---|---|---|
| `go/uuid/callers-parse` | polyglot | n/a | pass (2031) |
| `go/uuid/exported-types` | polyglot | n/a | pass (1403) |
| `go/uuid/implement-namespaced-ids` | implementation | n/a | unscored |
| `py/requests/callers-api-request` | polyglot | n/a | pass (7326) |
| `py/requests/toplevel-exceptions` | polyglot | n/a | fail 100% (4915) |
| `rust/hashbrown/diff-0.16.1-to-0.17.1` | retrieval | n/a | n/a |
| `rust/memchr/callers-search-slice-with-raw` | retrieval | n/a | fail 67% (27599) |
| `rust/memchr/implement-three-byte-scan` | implementation | n/a | unscored |
| `rust/memchr/implementors-frequency-rank` | retrieval | n/a | fail 100% (343) |
| `rust/memchr/iter-return-type` | efficiency | n/a | pass (656) |
| `rust/memchr/outbound-memmem-find` | retrieval | n/a | pass (37117) |
| `rust/memchr/public-surface` | retrieval | n/a | pass (18159) |
| `rust/memchr/signature-memchr3` | retrieval | n/a | pass (627) |
| `ts/commander/public-surface` | polyglot | n/a | n/a |

## Declined

Scenarios an adapter could not express. Not failures — these are the shape of each tool's reach.

- **nudox-canonical**, 14 scenarios — canonical reports capability `index` unavailable (emitted 0); the index plane is built but not wired to interface-core — health: compiler_output:unavailable, index:unavailable, graph:unavailable, vector:unavailable, local_analyzer:unavailable
  - `go/uuid/callers-parse`
  - `go/uuid/exported-types`
  - `go/uuid/implement-namespaced-ids`
  - `py/requests/callers-api-request`
  - `py/requests/toplevel-exceptions`
  - `rust/hashbrown/diff-0.16.1-to-0.17.1`
  - `rust/memchr/callers-search-slice-with-raw`
  - `rust/memchr/implement-three-byte-scan`
  - `rust/memchr/implementors-frequency-rank`
  - `rust/memchr/iter-return-type`
  - `rust/memchr/outbound-memmem-find`
  - `rust/memchr/public-surface`
  - `rust/memchr/signature-memchr3`
  - `ts/commander/public-surface`
- `rust/hashbrown/diff-0.16.1-to-0.17.1` / **ripgrep** — comparing two versions needs two checkouts; the scenario carries one
- `ts/commander/public-surface` / **ripgrep** — target.root does not exist: ${NODE_MODULES}/commander
