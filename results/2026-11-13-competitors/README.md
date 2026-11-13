# CodeGraph against ripgrep, 2026-11-13 — 30 scenarios

The first run of a third-party MCP server through this harness, end to end:
`@colbymchenry/codegraph` 1.6.2 (`codegraph serve --mcp`, stdio), spawned and
driven by the `mcp-stdio` adapter under its `codegraph` profile, each package
root copied to a scratch directory and indexed there with `codegraph init -y`.
Telemetry was turned off (`codegraph telemetry off`) before the run.
`--keep-responses` was on, so every tool response is in `report.json`.

Two CodeGraph columns: `codegraph` is the server as shipped, which lists only
`codegraph_explore`; `codegraph-full` sets
`CODEGRAPH_MCP_TOOLS=explore,node,search,callers,callees,impact,files,status`,
so callers/callees/node/search are used where the task has one.

| adapter | pass | attempted | unsupported | strict | matched | distractors | tok/scenario | resident (cl100k) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| codegraph | 5/20 (25%) | 20 | 9 | 0.89 | 0.89 | 14 | 4,031 | 302 |
| codegraph-full | 6/20 (30%) | 20 | 9 | 0.78 | 0.78 | 10 | 1,762 | 1,787 |
| ripgrep | 11/20 (55%) | 20 | 9 | 0.88 | 0.50 | 10→9 | 6,452 | 0 |

Split by scenario set (pass / scored, and how many scored attempts carried a
distractor):

| adapter | 15 earlier scenarios | 15 added 2026-11-13 |
|---|---|---|
| codegraph | 3/10, 2 with distractors | 2/10, 7 with distractors |
| codegraph-full | 5/10, 1 | 1/10, 5 |
| ripgrep | 7/10, 2 | 4/10, 4 |

## What it shows

* **The harness can benchmark a stdio MCP tool end to end**: spawn, handshake,
  live `tools/list` for resident cost, per-root indexing in preflight, a
  profile mapping tasks onto tools, `unsupported` where the tool cannot express
  the question (all six diffs: one index holds one checkout).
* **The new scenarios do what they were built for**: every adapter passes
  fewer of them, and almost all of the drop is precision, not recall —
  strict recall stays high while distractors fire. Distractors are
  same-named declarations on other types (`RawTable::reserve` versus
  `HashMap::reserve`, the five `insert`s, the `HashMap` versus `HashTable`
  `get_disjoint_mut`), caller/callee direction, and a re-export line.
* **Shipped default versus full surface**: `explore` returns a capped bundle
  of source (~4k tokens every time) with high recall and many distractors;
  the purpose-built tools are 2-3× cheaper per question but lose recall —
  `codegraph_callers` found no callers of memchr's generic
  `search_slice_with_raw` (0%), missed `Command.addOption`'s call to
  `_concatValue` made from inside an arrow function, and
  `codegraph_callees` for `find` returned 42 same-named definitions.

Read these as properties of CodeGraph 1.6.2 *as this profile drives it*
(bare symbol names, no `file` disambiguation). An agent that follows the
tool's "narrow with `file`" hints would do better on the ambiguous names;
that is the agent-driven track in `docs/PENDING.md`.

## What was not run

* **Nudox.** Backend v2 did not build on this machine — see
  `docs/RUN-V2-LOCALLY.md`.
* **Go**: `go/uuid/*` (3 scenarios) declined, `target.root does not exist`;
  the Go module proxy was unreachable from this session.
* **Diffs** (6): declined by both CodeGraph columns and by ripgrep.
* **Judge**: no API key; `rust/memchr/implement-three-byte-scan` is unscored.

Roots: `$CARGO_SRC` (cargo registry), `PY_SITE=/home/claude/pkgs/py`
(`pip install --target`, requests 2.34.2), `NODE_MODULES=…/node_modules`
(commander 12.1.0).

## Reproducibility

`nbench verify --config configs/competitors.toml --repeat 2` (each run copies
and re-indexes every root from scratch): **90 cells, 100% reproduced exactly,
no cost moved more than 5%.**
