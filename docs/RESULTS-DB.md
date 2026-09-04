# The results database

A `report.json` answers *how did that run go*. It cannot answer the questions
that actually decide whether the work is going anywhere:

* did this get **better** than last week?
* which scenario is **flaky**, and which has never once passed?
* what did turning the embedder on **actually buy**, in passes and in tokens?

Those are questions *across* runs. So every report goes into one SQLite file,
and two small scripts read it.

```bash
python3 scripts/bench_db.py ingest      # every report.json under runs/ and results/
python3 scripts/bench_web.py --open     # ingest, then browse at http://127.0.0.1:8765
```

Standard library plus matplotlib, which `plot_results.py` already needed. No
framework, no build step, no network — one vendored copy of htmx does the
interactivity.

---

## `bench_db.py` — the database

```bash
python3 scripts/bench_db.py ingest              # runs/ and results/
python3 scripts/bench_db.py ingest path/to/dir  # or somewhere else
python3 scripts/bench_db.py runs                # what is in there
python3 scripts/bench_db.py summary <run-id>    # the results table
python3 scripts/bench_db.py compare <a> <b>     # what moved between two runs
```

Ingest is **idempotent**: re-ingesting a run replaces it, so regenerating a
report never accumulates duplicates. `results.db` is gitignored and
disposable — delete it and ingest again.

### What is stored

| table | one row per | why it is kept |
|---|---|---|
| `runs` | run | id, timestamp, and where the report was read from |
| `adapters` | adapter × run | including the resident surface cost |
| `outcomes` | attempt | the scored result and what it cost |
| `facts` | expected fact | `strict` / `loose` / `absent`, per attempt |
| `steps` | tool call | the request, its tokens, its latency |
| `degradations` | warning | `~sem:unavailable`, `~ablation:not-exercised` |

Keeping `steps` and `facts` is the point: a number on a chart can be chased
back to the exact request that produced it. That is how the `diff` pagination
fix was verified — click the hashbrown cell and both `diff` calls are there,
the second carrying `{"cursor":"50"}`.

### The one thing to know about the schema

`outcomes.passed` is **nullable**, and the distinction is load-bearing:

* `1` / `0` — scored, passed or failed;
* `NULL` — never scored, because the adapter **declined** the scenario or
  because it needs a judge that did not run.

Averaging a NULL in as a failure would report rust-analyzer as *bad at Go*
rather than *inapplicable to it*, which is the distinction the whole benchmark
is built around. The `adapter_summary` view keeps `declined` and `unscored`
beside the pass rate, never inside it.

That view exists so the CLI and the web UI cannot disagree about what a pass
rate means. It reproduces the Rust reporter's table independently — if the two
ever differ, one of them has a bug worth finding.

---

## `bench_web.py` — the browser

```bash
python3 scripts/bench_web.py                  # ingest, serve on 127.0.0.1:8765
python3 scripts/bench_web.py --no-ingest      # serve what is already stored
python3 scripts/bench_web.py --port 9000 --open
```

Pick a run and you get:

* the **results table**, with `matched` beside `strict` — hover a `matched`
  cell that sits below its `strict` and it tells you how much of that recall
  was text printed around a match rather than found;
* four **charts**, drawn per request rather than cached (a run is seventy rows;
  drawing beats deciding whether a file is stale);
* the **scenario matrix** — click any cell for the tool calls behind it, their
  requests, their costs, and every fact verdict;
* a **comparison** against any second run, listing only the cells that moved.

### The charts

| chart | form | why that form |
|---|---|---|
| Cost against correctness | scatter, log x | both measures at once — either alone is gameable |
| Found, or merely surrounded | dumbbell | strict against matched; one dot means identical |
| What each answer cost | dot plot, log x | **not** bars: on a log axis a bar's length stops being proportional to its value |
| Surface cost | bar, linear | a genuine part-to-whole on a linear scale |

Colours come from the same fixed categorical slots as `plot_results.py`, in
the same order, so a chart here and a chart in a published report never
disagree about which colour an adapter is.

---

## Adding a query

`bench_db.py` exposes `connect`, `list_runs`, `summary`, `matrix` and
`compare`; `bench_web.py` imports them. A new panel is a query plus a
`render_*` function returning an HTML fragment, plus a route — htmx swaps
fragments, so there is no client state to keep in step.

For a one-off, the file is just SQLite:

```bash
sqlite3 results.db "
  SELECT scenario_id, COUNT(*) attempts, SUM(passed) passes
  FROM outcomes WHERE passed IS NOT NULL
  GROUP BY scenario_id ORDER BY passes ASC;"
```

A scenario that nothing has ever passed is either very hard or wrong, and it
is worth knowing which.

## The other two scripts

Neither of these is part of the run loop, and both were undocumented until they
were found by the simple expedient of running `--help` on everything shipped.

### `audit_payloads.py` — where recall actually came from

```sh
python3 scripts/audit_payloads.py results/2026-09-03-expanded
```

`rg -n --no-heading` writes a matched line as `path:line:text` and a context
line as `path-line-text`. That one character is the whole signal, and this
script reads it: for every scored fact it says whether the fact was credited
from a line the tool **matched** or only from one the `-B`/`-A` window dragged
in. A fact credited only by context is recall the tool did not really produce.

It is where the `matched` column came from. Auditing `run-2026-08-30-165221`
found three ripgrep verdicts — including a pass — resting entirely on context
lines.

Its report path used to be hardcoded to one run, so it could only ever audit
that one. It now takes any run directory.

### `build_presentation.py` — rebuild the illustrated results page

```sh
python3 scripts/plot_results.py results/2026-08-30      # charts first
python3 scripts/build_presentation.py results/2026-08-30
```

Writes `<run-dir>/presentation.html`, embedding the charts as data URIs. The
prose is written about that specific run, so this regenerates one document
rather than being a general report builder.

It used to read a scratch file in `/tmp` that nothing here produces, so the
published page could be read but not rebuilt. Regenerating now yields a file
byte-identical to the committed `presentation.html`.

**It does not write `protocol.html`** — that is a different, shorter document
published from the same directory.
