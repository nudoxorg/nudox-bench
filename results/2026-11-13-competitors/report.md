# nudox-bench run `run-2026-11-13-122712`

2026-11-13T12:28:10.195702406-05:00

## Surface cost before any question

What each system puts in an agent's context before it is asked anything: tool descriptions plus every argument-schema description. Paid once per session.

| adapter | cl100k | o200k | bytes |
|---|---:|---:|---:|
| codegraph | 302 | 302 | 1343 |
| codegraph-full | 1787 | 1788 | 7594 |
| ripgrep | 0 | 0 | 0 |

## Results

`strict` counts a fact found as a fully-qualified path; `loose` counts it found as a bare identifier anywhere in the returned text. The gap between them is how much of an adapter's apparent success is unresolved text.

`matched` counts only facts on a line the adapter actually matched, not ones it printed in the window around a match. Where it falls below `strict`, that much of the recall was surrounded rather than found. It equals `strict` for adapters that return only what they were asked for.

`errored` counts attempts where the tool itself failed — a crash, a dropped connection, a timeout. They stay inside `attempted` because a tool that cannot answer did not answer, but they are shown separately: a column of errors and a column of wrong answers look identical in a pass rate and mean entirely different things.

| adapter | pass | attempted | unsup. | unscored | errored | strict | matched | loose | gap | distractors | tok/scenario | tok/pass | calls |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| codegraph | 5/20 (25%) | 20 | 9 | 1 | 0 | 0.89 | 0.89 | 0.93 | 0.04 | 14 | 4031 | 16125 | 1.0 |
| codegraph-full | 6/20 (30%) | 20 | 9 | 1 | 0 | 0.78 | 0.78 | 0.81 | 0.03 | 10 | 1762 | 5873 | 1.0 |
| ripgrep | 11/20 (55%) | 20 | 9 | 1 | 0 | 0.88 | 0.50 | 0.93 | 0.04 | 10→9 | 6452 | 11732 | 1.0 |

## By family

The four families ask structurally different things. A single pass rate hides which one an adapter is good at: answering every retrieval question and no polyglot one reports the same headline as being mediocre at both, and the two call for different decisions. A family an adapter never attempted shows `—`, not 0%.

| adapter | family | pass | unsup. | unscored | strict | tok/scenario |
|---|---|---:|---:|---:|---:|---:|
| codegraph | efficiency | 1/2 (50%) | 0 | 0 | 1.00 | 4024 |
| codegraph | implementation | — | 1 | 1 | 0.00 | 0 |
| codegraph | polyglot | 1/6 (17%) | 2 | 0 | 0.92 | 4027 |
| codegraph | retrieval | 3/12 (25%) | 6 | 0 | 0.85 | 4034 |
| codegraph-full | efficiency | 1/2 (50%) | 0 | 0 | 1.00 | 1932 |
| codegraph-full | implementation | — | 1 | 1 | 0.00 | 0 |
| codegraph-full | polyglot | 1/6 (17%) | 2 | 0 | 0.80 | 2122 |
| codegraph-full | retrieval | 4/12 (33%) | 6 | 0 | 0.74 | 1553 |
| ripgrep | efficiency | 1/2 (50%) | 0 | 0 | 1.00 | 4102 |
| ripgrep | implementation | — | 1 | 1 | 0.00 | 0 |
| ripgrep | polyglot | 2/6 (33%) | 2 | 0 | 0.67 | 3098 |
| ripgrep | retrieval | 8/12 (67%) | 6 | 0 | 0.97 | 8521 |

## Per scenario

| scenario | family | codegraph | codegraph-full | ripgrep |
|---|---|---|---|---|
| `go/uuid/callers-parse` | polyglot | n/a | n/a | n/a |
| `go/uuid/exported-types` | polyglot | n/a | n/a | n/a |
| `go/uuid/implement-namespaced-ids` | implementation | n/a | n/a | n/a |
| `py/requests/callers-api-request` | polyglot | fail 71% (4072) | pass (257) | pass (7326) |
| `py/requests/callers-session-send` | polyglot | pass (3957) | fail 50% (147) | fail 50% (3115) |
| `py/requests/implementors-authbase` | polyglot | fail 100% (4472) | fail 100% (4473) | fail 0% (156) |
| `py/requests/toplevel-exceptions` | polyglot | fail 100% (4246) | fail 100% (4247) | fail 100% (4915) |
| `rust/bit-set/diff-0.5.3-to-0.8.0` | retrieval | n/a | n/a | n/a |
| `rust/bit-set/outbound-other-op` | retrieval | fail 100% (2949) | fail 33% (87) | pass (1917) |
| `rust/bit-vec/diff-0.6.3-to-0.8.0` | retrieval | n/a | n/a | n/a |
| `rust/hashbrown/callers-hashmap-reserve` | retrieval | pass (4295) | fail 100% (312) | fail 100% (11003) |
| `rust/hashbrown/definition-tryreserveerror` | retrieval | pass (3875) | pass (292) | pass (580) |
| `rust/hashbrown/diff-0.15.5-to-0.16.1` | retrieval | n/a | n/a | n/a |
| `rust/hashbrown/diff-0.16.1-to-0.17.1` | retrieval | n/a | n/a | n/a |
| `rust/hashbrown/signature-hashset-insert` | efficiency | fail 100% (4132) | fail 100% (3638) | fail 100% (7526) |
| `rust/hashbrown/signature-hashtable-get-disjoint-mut` | retrieval | fail 100% (4210) | fail 100% (850) | fail 100% (1459) |
| `rust/itertools/definition-interleave-reexport` | retrieval | fail 100% (3793) | fail 50% (215) | pass (606) |
| `rust/itertools/diff-0.14.0-to-0.15.0` | retrieval | n/a | n/a | n/a |
| `rust/itertools/implementors-peeking-next` | retrieval | fail 50% (5212) | fail 50% (5213) | pass (907) |
| `rust/lru/diff-0.16.4-to-0.18.4` | retrieval | n/a | n/a | n/a |
| `rust/lru/signature-get-or-insert-mut-ref` | retrieval | fail 100% (3060) | pass (466) | pass (626) |
| `rust/memchr/callers-search-slice-with-raw` | retrieval | fail 17% (4895) | fail 0% (63) | fail 67% (26635) |
| `rust/memchr/implement-three-byte-scan` | implementation | unscored | unscored | unscored |
| `rust/memchr/implementors-frequency-rank` | retrieval | pass (3863) | pass (3864) | fail 100% (335) |
| `rust/memchr/iter-return-type` | efficiency | pass (3802) | pass (225) | pass (636) |
| `rust/memchr/outbound-memmem-find` | retrieval | fail 67% (4158) | fail 67% (2352) | pass (37676) |
| `rust/memchr/public-surface` | retrieval | fail 87% (4165) | fail 87% (4166) | pass (17561) |
| `rust/memchr/signature-memchr3` | retrieval | fail 100% (3439) | pass (478) | pass (607) |
| `ts/commander/callers-option-concat-value` | polyglot | fail 100% (3895) | fail 50% (128) | fail 50% (1769) |
| `ts/commander/public-surface` | polyglot | fail 80% (3440) | fail 80% (3441) | pass (875) |

## Declined

Scenarios an adapter could not express. Not failures — these are the shape of each tool's reach.

- **codegraph**, 6 scenarios — one CodeGraph index holds one checkout; no tool compares two versions
  - `rust/bit-set/diff-0.5.3-to-0.8.0`
  - `rust/bit-vec/diff-0.6.3-to-0.8.0`
  - `rust/hashbrown/diff-0.15.5-to-0.16.1`
  - `rust/hashbrown/diff-0.16.1-to-0.17.1`
  - `rust/itertools/diff-0.14.0-to-0.15.0`
  - `rust/lru/diff-0.16.4-to-0.18.4`
- **codegraph**, 3 scenarios — target.root does not exist: /root/go/pkg/mod/github.com/google/uuid@v1.6.0
  - `go/uuid/callers-parse`
  - `go/uuid/exported-types`
  - `go/uuid/implement-namespaced-ids`
- **codegraph-full**, 6 scenarios — one CodeGraph index holds one checkout; no tool compares two versions
  - `rust/bit-set/diff-0.5.3-to-0.8.0`
  - `rust/bit-vec/diff-0.6.3-to-0.8.0`
  - `rust/hashbrown/diff-0.15.5-to-0.16.1`
  - `rust/hashbrown/diff-0.16.1-to-0.17.1`
  - `rust/itertools/diff-0.14.0-to-0.15.0`
  - `rust/lru/diff-0.16.4-to-0.18.4`
- **codegraph-full**, 3 scenarios — target.root does not exist: /root/go/pkg/mod/github.com/google/uuid@v1.6.0
  - `go/uuid/callers-parse`
  - `go/uuid/exported-types`
  - `go/uuid/implement-namespaced-ids`
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
