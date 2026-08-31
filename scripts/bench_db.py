#!/usr/bin/env python3
"""The results database: every run this repo has ever produced, in one file.

A `report.json` answers "how did that run go". It cannot answer "did this get
better", "which scenario is flaky", or "what did turning the embedder on
actually buy" — those are questions across runs, and they need the runs in one
place.

So this ingests every `report.json` into SQLite, keyed by run id, and keeps
enough of each attempt (facts, steps, degradations) that a number on a chart
can be chased back to the exact tool call that produced it.

    python3 scripts/bench_db.py ingest          # everything under runs/ and results/
    python3 scripts/bench_db.py ingest path/…   # or named directories
    python3 scripts/bench_db.py runs            # what is in the database
    python3 scripts/bench_db.py summary <run>   # the results table for one run
    python3 scripts/bench_db.py compare A B     # what changed between two runs

Ingest is idempotent: re-ingesting a run replaces it, so a report that was
regenerated does not accumulate duplicates.

Standard library only. The database is disposable — delete it and re-ingest.
"""

from __future__ import annotations

import argparse
import json
import sqlite3
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DEFAULT_DB = REPO / "results.db"

SCHEMA = """
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS runs (
    run_id      TEXT PRIMARY KEY,
    timestamp   TEXT,
    source      TEXT,            -- where the report.json was read from
    ingested_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS adapters (
    run_id          TEXT NOT NULL REFERENCES runs(run_id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    description     TEXT,
    resident_bytes  INTEGER,
    resident_cl100k INTEGER,
    resident_o200k  INTEGER,
    PRIMARY KEY (run_id, name)
);

-- One adapter's answer to one scenario.
--
-- `passed` is nullable on purpose and the distinction is the whole point:
-- NULL means the attempt was never scored (declined, or judged with no judge
-- available), which must never be averaged in as a failure.
CREATE TABLE IF NOT EXISTS outcomes (
    id            INTEGER PRIMARY KEY,
    run_id        TEXT NOT NULL REFERENCES runs(run_id) ON DELETE CASCADE,
    scenario_id   TEXT NOT NULL,
    family        TEXT,
    adapter       TEXT NOT NULL,
    passed        INTEGER,        -- 1 / 0 / NULL (unscored or declined)
    strict        REAL,
    loose         REAL,
    matched       REAL,           -- strict recall over matched evidence only
    gap           REAL,           -- loose - strict
    distractors   INTEGER,
    any_of_ok     INTEGER,
    judged        INTEGER,
    unsupported   TEXT,           -- the decline reason, verbatim
    tool_calls    INTEGER,
    elapsed_ms    INTEGER,
    req_cl100k    INTEGER,
    resp_cl100k   INTEGER,
    req_o200k     INTEGER,
    resp_o200k    INTEGER,
    marginal      INTEGER,        -- the conservative per-question cost
    context_chars INTEGER,
    UNIQUE (run_id, scenario_id, adapter)
);

CREATE TABLE IF NOT EXISTS facts (
    outcome_id INTEGER NOT NULL REFERENCES outcomes(id) ON DELETE CASCADE,
    fact       TEXT NOT NULL,
    tier       TEXT NOT NULL      -- strict | loose | absent
);

CREATE TABLE IF NOT EXISTS steps (
    outcome_id  INTEGER NOT NULL REFERENCES outcomes(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    label       TEXT,
    request     TEXT,
    resp_cl100k INTEGER,
    resp_o200k  INTEGER,
    elapsed_ms  INTEGER,
    is_error    INTEGER
);

CREATE TABLE IF NOT EXISTS degradations (
    outcome_id INTEGER NOT NULL REFERENCES outcomes(id) ON DELETE CASCADE,
    note       TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS outcomes_run      ON outcomes(run_id);
CREATE INDEX IF NOT EXISTS outcomes_adapter  ON outcomes(adapter);
CREATE INDEX IF NOT EXISTS outcomes_scenario ON outcomes(scenario_id);
CREATE INDEX IF NOT EXISTS facts_outcome     ON facts(outcome_id);
CREATE INDEX IF NOT EXISTS steps_outcome     ON steps(outcome_id);

-- The report's own results table, as a view, so the web UI and the CLI cannot
-- disagree about what a pass rate means.
--
-- `attempted` counts scenarios the adapter could express AND that could be
-- scored; declined and unscored are reported beside it, never inside it.
CREATE VIEW IF NOT EXISTS adapter_summary AS
SELECT
    o.run_id,
    o.adapter,
    COUNT(*)                                              AS total,
    SUM(o.passed IS NOT NULL)                             AS attempted,
    SUM(COALESCE(o.passed, 0))                            AS passed,
    SUM(o.unsupported IS NOT NULL)                        AS declined,
    SUM(o.unsupported IS NULL AND o.passed IS NULL)       AS unscored,
    AVG(CASE WHEN o.passed IS NOT NULL THEN o.strict  END) AS strict,
    AVG(CASE WHEN o.passed IS NOT NULL THEN o.matched END) AS matched,
    AVG(CASE WHEN o.passed IS NOT NULL THEN o.loose   END) AS loose,
    AVG(CASE WHEN o.passed IS NOT NULL THEN o.gap     END) AS gap,
    SUM(COALESCE(o.distractors, 0))                       AS distractors,
    AVG(CASE WHEN o.passed IS NOT NULL THEN o.marginal END) AS tok_per_scenario,
    AVG(CASE WHEN o.passed IS NOT NULL THEN o.tool_calls END) AS calls
FROM outcomes o
GROUP BY o.run_id, o.adapter;
"""


def connect(db_path: Path = DEFAULT_DB) -> sqlite3.Connection:
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    conn.executescript(SCHEMA)
    return conn


def _tok(cost: dict, half: str, vocab: str) -> int:
    return int((cost.get(half) or {}).get(vocab, 0) or 0)


def _marginal(cost: dict) -> int:
    """Requests plus responses, under whichever vocabulary is larger.

    Matches `TokenCount::conservative` in the harness: a compactness claim
    must never be flattered by the choice of tokenizer.
    """
    cl = _tok(cost, "requests", "cl100k") + _tok(cost, "responses", "cl100k")
    o2 = _tok(cost, "requests", "o200k") + _tok(cost, "responses", "o200k")
    return max(cl, o2)


def ingest_report(conn: sqlite3.Connection, path: Path) -> tuple[str, int]:
    """Load one report.json. Replaces any previous copy of the same run."""
    report = json.loads(path.read_text())
    run_id = report["run_id"]

    with conn:
        # ON DELETE CASCADE clears adapters/outcomes/facts/steps for this run.
        conn.execute("DELETE FROM runs WHERE run_id = ?", (run_id,))
        conn.execute(
            "INSERT INTO runs (run_id, timestamp, source) VALUES (?, ?, ?)",
            (run_id, report.get("timestamp"), str(path)),
        )

        for adapter in report.get("adapters", []):
            resident = adapter.get("resident") or {}
            conn.execute(
                "INSERT INTO adapters (run_id, name, description, resident_bytes,"
                " resident_cl100k, resident_o200k) VALUES (?, ?, ?, ?, ?, ?)",
                (
                    run_id,
                    adapter["name"],
                    adapter.get("description"),
                    resident.get("bytes"),
                    resident.get("cl100k"),
                    resident.get("o200k"),
                ),
            )

        count = 0
        for outcome in report.get("outcomes", []):
            attempt = outcome.get("attempt") or {}
            score = outcome.get("score") or {}
            judge = outcome.get("judge")
            cost = attempt.get("cost") or {}
            unsupported = attempt.get("unsupported")

            # Mirror `Outcome::passed`: a judged scenario defers to the judge,
            # a declined one has no verdict at all, and an unjudged judged one
            # stays NULL rather than counting as a failure.
            if unsupported is not None:
                passed = None
            elif judge is not None:
                passed = 1 if judge.get("passed") else 0
            elif score.get("judged"):
                passed = None
            else:
                passed = 1 if score.get("passed") else 0

            strict = score.get("strict_recall")
            loose = score.get("loose_recall")
            matched = score.get("primary_strict_recall")
            if matched is None:
                # No incidental output: everything returned was deliberate.
                matched = strict

            cur = conn.execute(
                "INSERT INTO outcomes (run_id, scenario_id, family, adapter, passed,"
                " strict, loose, matched, gap, distractors, any_of_ok, judged,"
                " unsupported, tool_calls, elapsed_ms, req_cl100k, resp_cl100k,"
                " req_o200k, resp_o200k, marginal, context_chars)"
                " VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                (
                    run_id,
                    outcome["scenario_id"],
                    outcome.get("family"),
                    outcome["adapter"],
                    passed,
                    strict,
                    loose,
                    matched,
                    None if strict is None or loose is None else max(0.0, loose - strict),
                    score.get("distractors"),
                    1 if score.get("any_of_satisfied") else 0,
                    1 if score.get("judged") else 0,
                    unsupported,
                    attempt.get("tool_calls"),
                    attempt.get("elapsed_ms"),
                    _tok(cost, "requests", "cl100k"),
                    _tok(cost, "responses", "cl100k"),
                    _tok(cost, "requests", "o200k"),
                    _tok(cost, "responses", "o200k"),
                    _marginal(cost),
                    len(attempt.get("context") or ""),
                ),
            )
            oid = cur.lastrowid
            count += 1

            conn.executemany(
                "INSERT INTO facts (outcome_id, fact, tier) VALUES (?, ?, ?)",
                [(oid, f["fact"], f["tier"]) for f in score.get("facts", [])],
            )
            conn.executemany(
                "INSERT INTO steps (outcome_id, idx, label, request, resp_cl100k,"
                " resp_o200k, elapsed_ms, is_error) VALUES (?,?,?,?,?,?,?,?)",
                [
                    (
                        oid,
                        i,
                        s.get("label"),
                        s.get("request"),
                        (s.get("response_cost") or {}).get("cl100k"),
                        (s.get("response_cost") or {}).get("o200k"),
                        s.get("elapsed_ms"),
                        1 if s.get("is_error") else 0,
                    )
                    for i, s in enumerate(attempt.get("steps") or [])
                ],
            )
            conn.executemany(
                "INSERT INTO degradations (outcome_id, note) VALUES (?, ?)",
                [(oid, n) for n in attempt.get("degraded", [])],
            )

    return run_id, count


def _display(path: Path) -> str:
    """Repo-relative where possible; a run directory outside the checkout —
    another worktree's `runs/`, say — keeps its absolute path."""
    try:
        return str(path.relative_to(REPO))
    except ValueError:
        return str(path)


def find_reports(roots: list[Path]) -> list[Path]:
    found: list[Path] = []
    for root in roots:
        if root.is_file() and root.name.endswith(".json"):
            found.append(root)
        elif root.is_dir():
            found.extend(sorted(root.rglob("report.json")))
    # Stable order, no duplicates.
    seen, unique = set(), []
    for path in found:
        resolved = path.resolve()
        if resolved not in seen:
            seen.add(resolved)
            unique.append(path)
    return unique


# --- queries the CLI and the web UI share -----------------------------------

def list_runs(conn) -> list[sqlite3.Row]:
    return conn.execute(
        "SELECT r.run_id, r.timestamp, r.source,"
        "  (SELECT COUNT(DISTINCT adapter) FROM outcomes o WHERE o.run_id = r.run_id) AS adapters,"
        "  (SELECT COUNT(DISTINCT scenario_id) FROM outcomes o WHERE o.run_id = r.run_id) AS scenarios,"
        "  (SELECT COUNT(*) FROM outcomes o WHERE o.run_id = r.run_id AND o.passed = 1) AS passed,"
        "  (SELECT COUNT(*) FROM outcomes o WHERE o.run_id = r.run_id AND o.passed IS NOT NULL) AS attempted,"
        "  (SELECT COUNT(*) FROM degradations d JOIN outcomes o ON o.id = d.outcome_id"
        "     WHERE o.run_id = r.run_id) AS degraded"
        " FROM runs r ORDER BY r.timestamp DESC, r.run_id DESC"
    ).fetchall()


def summary(conn, run_id: str) -> list[sqlite3.Row]:
    return conn.execute(
        "SELECT * FROM adapter_summary WHERE run_id = ? ORDER BY passed*1.0/NULLIF(attempted,0) DESC,"
        " tok_per_scenario ASC",
        (run_id,),
    ).fetchall()


def matrix(conn, run_id: str) -> tuple[list[str], dict]:
    adapters = [r["adapter"] for r in conn.execute(
        "SELECT DISTINCT adapter FROM outcomes WHERE run_id = ? ORDER BY adapter", (run_id,))]
    cells = {}
    for row in conn.execute(
        "SELECT scenario_id, family, adapter, passed, strict, unsupported, marginal, id"
        " FROM outcomes WHERE run_id = ? ORDER BY scenario_id", (run_id,)
    ):
        cells.setdefault((row["scenario_id"], row["family"]), {})[row["adapter"]] = row
    return adapters, cells


def compare(conn, run_a: str, run_b: str) -> list[sqlite3.Row]:
    """Per scenario+adapter, what changed between two runs."""
    return conn.execute(
        "SELECT a.scenario_id, a.adapter,"
        "       a.passed AS passed_a, b.passed AS passed_b,"
        "       a.strict AS strict_a, b.strict AS strict_b,"
        "       a.marginal AS tok_a, b.marginal AS tok_b,"
        "       a.unsupported AS unsup_a, b.unsupported AS unsup_b"
        " FROM outcomes a JOIN outcomes b"
        "   ON a.scenario_id = b.scenario_id AND a.adapter = b.adapter"
        " WHERE a.run_id = ? AND b.run_id = ?"
        "   AND (IFNULL(a.passed,-1) != IFNULL(b.passed,-1)"
        "        OR ABS(IFNULL(a.strict,0) - IFNULL(b.strict,0)) > 0.001"
        "        OR ABS(IFNULL(a.marginal,0) - IFNULL(b.marginal,0)) > 0)"
        " ORDER BY a.scenario_id, a.adapter",
        (run_a, run_b),
    ).fetchall()


# --- CLI --------------------------------------------------------------------

def _verdict(row, prefix="") -> str:
    if row[prefix + "unsup" if prefix else "unsupported"]:
        return "declined"
    passed = row[prefix + "passed" if prefix else "passed"]
    if passed is None:
        return "unscored"
    return "pass" if passed else "fail"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--db", type=Path, default=DEFAULT_DB)
    sub = parser.add_subparsers(dest="command", required=True)

    p_ingest = sub.add_parser("ingest", help="load report.json files into the database")
    p_ingest.add_argument("paths", nargs="*", type=Path,
                          help="directories or report.json files (default: runs/ and results/)")

    sub.add_parser("runs", help="list every run in the database")

    p_summary = sub.add_parser("summary", help="the results table for one run")
    p_summary.add_argument("run_id")

    p_compare = sub.add_parser("compare", help="what changed between two runs")
    p_compare.add_argument("run_a")
    p_compare.add_argument("run_b")

    args = parser.parse_args(argv)
    conn = connect(args.db)

    if args.command == "ingest":
        roots = args.paths or [REPO / "runs", REPO / "results"]
        reports = find_reports([Path(r) for r in roots])
        if not reports:
            print("no report.json found under: %s" % ", ".join(str(r) for r in roots))
            return 1
        total = 0
        for path in reports:
            try:
                run_id, n = ingest_report(conn, path)
            except (KeyError, json.JSONDecodeError) as exc:
                print("  SKIP %s — %s" % (path, exc))
                continue
            total += n
            print("  %-28s %3d outcomes  (%s)" % (run_id, n, _display(path)))
        print("\n%d run(s), %d outcomes -> %s" % (len(reports), total, args.db))
        return 0

    if args.command == "runs":
        rows = list_runs(conn)
        if not rows:
            print("database is empty; run `ingest` first")
            return 1
        print("%-26s %-26s %8s %10s %9s %9s" %
              ("run", "timestamp", "adapters", "scenarios", "passed", "degraded"))
        for r in rows:
            print("%-26s %-26s %8d %10d %5d/%-3d %9d" % (
                r["run_id"], (r["timestamp"] or "")[:26], r["adapters"],
                r["scenarios"], r["passed"], r["attempted"], r["degraded"]))
        return 0

    if args.command == "summary":
        rows = summary(conn, args.run_id)
        if not rows:
            print("no such run: %s" % args.run_id)
            return 1
        print("%-20s %10s %9s %9s %8s %8s %8s %7s %12s %7s" % (
            "adapter", "pass", "declined", "unscored", "strict", "matched",
            "loose", "gap", "tok/scenario", "calls"))
        for r in rows:
            rate = (r["passed"] / r["attempted"] * 100) if r["attempted"] else 0
            print("%-20s %4d/%-3d %3.0f%% %9d %9d %8.2f %8.2f %8.2f %7.2f %12.0f %7.1f" % (
                r["adapter"], r["passed"], r["attempted"], rate, r["declined"],
                r["unscored"], r["strict"] or 0, r["matched"] or 0, r["loose"] or 0,
                r["gap"] or 0, r["tok_per_scenario"] or 0, r["calls"] or 0))
        return 0

    if args.command == "compare":
        rows = compare(conn, args.run_a, args.run_b)
        if not rows:
            print("no differences between %s and %s" % (args.run_a, args.run_b))
            return 0
        print("%-44s %-20s %-22s %s" % ("scenario", "adapter", "verdict", "tokens"))
        for r in rows:
            va = "declined" if r["unsup_a"] else ("unscored" if r["passed_a"] is None
                                                 else ("pass" if r["passed_a"] else "fail"))
            vb = "declined" if r["unsup_b"] else ("unscored" if r["passed_b"] is None
                                                 else ("pass" if r["passed_b"] else "fail"))
            print("%-44s %-20s %-10s -> %-10s %d -> %d" % (
                r["scenario_id"], r["adapter"], va, vb, r["tok_a"] or 0, r["tok_b"] or 0))
        return 0

    return 0


if __name__ == "__main__":
    sys.exit(main())
