# nudox-bench run `run-2026-11-13-122938`

2026-11-13T12:29:54.234694778-05:00

## Surface cost before any question

What each system puts in an agent's context before it is asked anything: tool descriptions plus every argument-schema description. Paid once per session.

| adapter | cl100k | o200k | bytes |
|---|---:|---:|---:|
| ripgrep | 0 | 0 | 0 |
| rust-analyzer | 0 | 0 | 0 |

## Results

`strict` counts a fact found as a fully-qualified path; `loose` counts it found as a bare identifier anywhere in the returned text. The gap between them is how much of an adapter's apparent success is unresolved text.

`matched` counts only facts on a line the adapter actually matched, not ones it printed in the window around a match. Where it falls below `strict`, that much of the recall was surrounded rather than found. It equals `strict` for adapters that return only what they were asked for.

`errored` counts attempts where the tool itself failed — a crash, a dropped connection, a timeout. They stay inside `attempted` because a tool that cannot answer did not answer, but they are shown separately: a column of errors and a column of wrong answers look identical in a pass rate and mean entirely different things.

| adapter | pass | attempted | unsup. | unscored | errored | strict | matched | loose | gap | distractors | tok/scenario | tok/pass | calls |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ripgrep | 11/20 (55%) | 20 | 9 | 1 | 0 | 0.88 | 0.50 | 0.93 | 0.04 | 10→9 | 6452 | 11732 | 1.0 |
| rust-analyzer | 0/14 (0%) | 14 | 15 | 1 | 14 | 0.00 | 0.00 | 0.00 | 0.00 | 0 | 50 | — | 1.0 |

## By family

The four families ask structurally different things. A single pass rate hides which one an adapter is good at: answering every retrieval question and no polyglot one reports the same headline as being mediocre at both, and the two call for different decisions. A family an adapter never attempted shows `—`, not 0%.

| adapter | family | pass | unsup. | unscored | strict | tok/scenario |
|---|---|---:|---:|---:|---:|---:|
| ripgrep | efficiency | 1/2 (50%) | 0 | 0 | 1.00 | 4102 |
| ripgrep | implementation | — | 1 | 1 | 0.00 | 0 |
| ripgrep | polyglot | 2/6 (33%) | 2 | 0 | 0.67 | 3098 |
| ripgrep | retrieval | 8/12 (67%) | 6 | 0 | 0.97 | 8521 |
| rust-analyzer | efficiency | 0/2 (0%) | 0 | 0 | 0.00 | 50 |
| rust-analyzer | implementation | — | 1 | 1 | 0.00 | 0 |
| rust-analyzer | polyglot | — | 8 | 0 | 0.00 | 0 |
| rust-analyzer | retrieval | 0/12 (0%) | 6 | 0 | 0.00 | 50 |

## Per scenario

| scenario | family | ripgrep | rust-analyzer |
|---|---|---|---|
| `go/uuid/callers-parse` | polyglot | n/a | n/a |
| `go/uuid/exported-types` | polyglot | n/a | n/a |
| `go/uuid/implement-namespaced-ids` | implementation | n/a | n/a |
| `py/requests/callers-api-request` | polyglot | pass (7326) | n/a |
| `py/requests/callers-session-send` | polyglot | fail 50% (3115) | n/a |
| `py/requests/implementors-authbase` | polyglot | fail 0% (156) | n/a |
| `py/requests/toplevel-exceptions` | polyglot | fail 100% (4915) | n/a |
| `rust/bit-set/diff-0.5.3-to-0.8.0` | retrieval | n/a | n/a |
| `rust/bit-set/outbound-other-op` | retrieval | pass (1917) | fail 0% (49) |
| `rust/bit-vec/diff-0.6.3-to-0.8.0` | retrieval | n/a | n/a |
| `rust/hashbrown/callers-hashmap-reserve` | retrieval | fail 100% (11003) | fail 0% (49) |
| `rust/hashbrown/definition-tryreserveerror` | retrieval | pass (580) | fail 0% (49) |
| `rust/hashbrown/diff-0.15.5-to-0.16.1` | retrieval | n/a | n/a |
| `rust/hashbrown/diff-0.16.1-to-0.17.1` | retrieval | n/a | n/a |
| `rust/hashbrown/signature-hashset-insert` | efficiency | fail 100% (7526) | fail 0% (49) |
| `rust/hashbrown/signature-hashtable-get-disjoint-mut` | retrieval | fail 100% (1459) | fail 0% (49) |
| `rust/itertools/definition-interleave-reexport` | retrieval | pass (606) | fail 0% (50) |
| `rust/itertools/diff-0.14.0-to-0.15.0` | retrieval | n/a | n/a |
| `rust/itertools/implementors-peeking-next` | retrieval | pass (907) | fail 0% (50) |
| `rust/lru/diff-0.16.4-to-0.18.4` | retrieval | n/a | n/a |
| `rust/lru/signature-get-or-insert-mut-ref` | retrieval | pass (626) | fail 0% (49) |
| `rust/memchr/callers-search-slice-with-raw` | retrieval | fail 67% (26635) | fail 0% (50) |
| `rust/memchr/implement-three-byte-scan` | implementation | unscored | unscored |
| `rust/memchr/implementors-frequency-rank` | retrieval | fail 100% (335) | fail 0% (50) |
| `rust/memchr/iter-return-type` | efficiency | pass (636) | fail 0% (50) |
| `rust/memchr/outbound-memmem-find` | retrieval | pass (37676) | fail 0% (50) |
| `rust/memchr/public-surface` | retrieval | pass (17561) | fail 0% (50) |
| `rust/memchr/signature-memchr3` | retrieval | pass (607) | fail 0% (50) |
| `ts/commander/callers-option-concat-value` | polyglot | fail 50% (1769) | n/a |
| `ts/commander/public-surface` | polyglot | pass (875) | n/a |

## Declined

Scenarios an adapter could not express. Not failures — these are the shape of each tool's reach.

- **ripgrep**, 6 scenarios — comparing two versions needs two checkouts; the scenario carries one
  - `rust/bit-set/diff-0.5.3-to-0.8.0`
  - `rust/bit-vec/diff-0.6.3-to-0.8.0`
  - `rust/hashbrown/diff-0.15.5-to-0.16.1`
  - `rust/hashbrown/diff-0.16.1-to-0.17.1`
  - `rust/itertools/diff-0.14.0-to-0.15.0`
  - `rust/lru/diff-0.16.4-to-0.18.4`
- **ripgrep**, 3 scenarios — target.root does not exist: /root/go/pkg/mod/github.com/google/uuid@v1.6.0
  - `go/uuid/callers-parse`
  - `go/uuid/exported-types`
  - `go/uuid/implement-namespaced-ids`
- **rust-analyzer**, 6 scenarios — an LSP server answers about the checkout it has, so it cannot compare two versions
  - `rust/bit-set/diff-0.5.3-to-0.8.0`
  - `rust/bit-vec/diff-0.6.3-to-0.8.0`
  - `rust/hashbrown/diff-0.15.5-to-0.16.1`
  - `rust/hashbrown/diff-0.16.1-to-0.17.1`
  - `rust/itertools/diff-0.14.0-to-0.15.0`
  - `rust/lru/diff-0.16.4-to-0.18.4`
- **rust-analyzer**, 3 scenarios — rust-analyzer does not speak go; this is a real limit of the comparison, not a harness gap
  - `go/uuid/callers-parse`
  - `go/uuid/exported-types`
  - `go/uuid/implement-namespaced-ids`
- **rust-analyzer**, 2 scenarios — rust-analyzer does not speak npm; this is a real limit of the comparison, not a harness gap
  - `ts/commander/callers-option-concat-value`
  - `ts/commander/public-surface`
- **rust-analyzer**, 4 scenarios — rust-analyzer does not speak pypi; this is a real limit of the comparison, not a harness gap
  - `py/requests/callers-api-request`
  - `py/requests/callers-session-send`
  - `py/requests/implementors-authbase`
  - `py/requests/toplevel-exceptions`
