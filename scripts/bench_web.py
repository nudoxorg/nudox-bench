#!/usr/bin/env python3
"""A local results browser: the database, rendered, with charts drawn on demand.

    python3 scripts/bench_web.py            # ingest, then serve on :8765
    python3 scripts/bench_web.py --no-ingest --port 9000

Pick a run and you get its results table, its scenario matrix, and four charts.
Click a cell and you get the tool calls that produced it — the request, what it
cost, and every fact verdict. Pick a second run and you get the diff.

Deliberately small: `http.server` and `sqlite3` from the standard library,
matplotlib for the charts (already a dependency of `plot_results.py`), and one
vendored copy of htmx. No framework, no build step, no network.

The charts are rendered per request rather than written to disk. A run is
seventy rows; drawing is faster than deciding whether a cached file is stale.
"""

from __future__ import annotations

import argparse
import html
import io
import sqlite3
import sys
import threading
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

sys.path.insert(0, str(Path(__file__).resolve().parent))
import bench_db  # noqa: E402

import matplotlib  # noqa: E402
matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

STATIC = Path(__file__).resolve().parent / "static"

# The validated categorical palette, same slots and same fixed order as
# `plot_results.py` — a chart here and a chart there must not disagree about
# which colour an adapter is.
SERIES = {
    "nudox-full": "#2a78d6",
    "ripgrep": "#eb6834",
    "rust-analyzer": "#1baf7a",
    "nudox-oneshot": "#eda100",
    "nudox-search-only": "#e87ba4",
    "nudox-graph-first": "#4a3aa7",
    "nudox-keys": "#e34948",
}
FALLBACK = "#8a8983"


def colour(name: str) -> str:
    return SERIES.get(name, FALLBACK)


def esc(value) -> str:
    return html.escape("" if value is None else str(value))


# --- page shell -------------------------------------------------------------

PAGE = """<!doctype html>
<html lang="en"><head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>nudox-bench results</title>
<script src="/static/htmx.min.js"></script>
<style>
:root {
  color-scheme: light dark;
  --ground: #f4f4f1; --plate: #fcfcfb; --sunk: #eceae4;
  --ink: #0b0b0b; --ink-2: #52514e; --ink-3: #898781;
  --rule: #e1e0d9; --rule-2: #c8c6bd; --accent: #1c5cab;
  --good: #006300; --good-bg: #e6f2e6;
  --bad: #b02f2f;  --bad-bg: #f8e6e6;
  --warn: #8a5a00; --warn-bg: #f8efd9;
}
@media (prefers-color-scheme: dark) {
  :root {
    --ground: #0d0d0d; --plate: #1a1a19; --sunk: #131312;
    --ink: #f7f7f5; --ink-2: #c3c2b7; --ink-3: #898781;
    --rule: #2c2c2a; --rule-2: #43423e; --accent: #6da7ec;
    --good: #0ca30c; --good-bg: #142614;
    --bad: #e06767;  --bad-bg: #2b1616;
    --warn: #fab219; --warn-bg: #2a2211;
  }
}
* { box-sizing: border-box; }
body {
  margin: 0; padding: 0 1.4rem 4rem; background: var(--ground); color: var(--ink);
  font: 15px/1.55 ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
}
.wrap { max-width: 78rem; margin: 0 auto; }
header { padding: 2rem 0 1rem; border-bottom: 2px solid var(--ink); margin-bottom: 1.5rem; }
h1 { font-size: 1.5rem; margin: 0 0 .3rem; letter-spacing: -0.02em; }
h2 { font-size: 1.05rem; margin: 2rem 0 .7rem; letter-spacing: -0.01em; }
p.sub { margin: 0; color: var(--ink-2); font-size: .9rem; }
code, .mono, td.num, th.num { font-family: ui-monospace, "IBM Plex Mono", SFMono-Regular, Menlo, monospace; }
table { border-collapse: collapse; width: 100%; font-size: .84rem; font-variant-numeric: tabular-nums; }
th { text-align: right; font-size: .72rem; text-transform: uppercase; letter-spacing: .05em;
     color: var(--ink-3); padding: .5rem .6rem; border-bottom: 1px solid var(--rule-2); white-space: nowrap; }
td { text-align: right; padding: .42rem .6rem; border-bottom: 1px solid var(--rule); white-space: nowrap; }
th:first-child, td:first-child { text-align: left; }
tbody tr:hover { background: var(--sunk); }
.tablewrap { overflow-x: auto; border: 1px solid var(--rule); border-radius: 3px; background: var(--plate); }
.v { display: inline-block; font-size: .72rem; font-weight: 600; border-radius: 2px;
     padding: .08rem .34rem; border: 1px solid transparent; font-family: ui-monospace, monospace; }
.v-pass { color: var(--good); background: var(--good-bg); border-color: color-mix(in srgb, var(--good) 30%, transparent); }
.v-fail { color: var(--bad);  background: var(--bad-bg);  border-color: color-mix(in srgb, var(--bad) 30%, transparent); }
.v-dec  { color: var(--ink-3); background: var(--sunk); border-color: var(--rule); }
.v-uns  { color: var(--warn); background: var(--warn-bg); border-color: color-mix(in srgb, var(--warn) 35%, transparent); }
.dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: .4rem; vertical-align: 1px; }
.cell { cursor: pointer; }
.cell:hover .v { outline: 2px solid var(--accent); outline-offset: 1px; }
select, button {
  font: inherit; padding: .35rem .5rem; background: var(--plate); color: var(--ink);
  border: 1px solid var(--rule-2); border-radius: 3px;
}
.controls { display: flex; gap: .8rem; align-items: center; flex-wrap: wrap; margin-bottom: 1rem; }
.charts { display: grid; grid-template-columns: repeat(auto-fit, minmax(22rem, 1fr)); gap: 1rem; }
.chart { background: var(--plate); border: 1px solid var(--rule); border-radius: 3px; padding: .6rem; }
.chart img { width: 100%; height: auto; display: block; }
#detail:empty { display: none; }
#detail { margin-top: 1rem; background: var(--plate); border: 1px solid var(--rule-2);
          border-left: 3px solid var(--accent); border-radius: 3px; padding: 1rem 1.1rem; }
.muted { color: var(--ink-3); }
pre { margin: .3rem 0 0; padding: .5rem .6rem; background: var(--sunk); border: 1px solid var(--rule);
      border-radius: 3px; overflow-x: auto; font-size: .74rem; }
.htmx-request { opacity: .55; transition: opacity .1s; }
</style>
</head><body><div class="wrap">
<header>
  <h1>nudox-bench results</h1>
  <p class="sub">{{nruns}} run(s) · {{noutcomes}} attempts · <code>{{db}}</code></p>
</header>

<div class="controls">
  <label>Run
    <select name="run" hx-get="/run" hx-target="#run-view" hx-trigger="change, load"
            hx-include="[name='against']">{{options}}</select>
  </label>
  <label>Compare against
    <select name="against" hx-get="/run" hx-target="#run-view" hx-trigger="change"
            hx-include="[name='run']">
      <option value="">— nothing —</option>{{options}}
    </select>
  </label>
</div>

<div id="run-view"></div>
<div id="detail"></div>
</div></body></html>
"""


def verdict_html(row) -> str:
    if row["unsupported"]:
        return '<span class="v v-dec">declined</span>'
    if row["passed"] is None:
        return '<span class="v v-uns">unscored</span>'
    if row["passed"]:
        return '<span class="v v-pass">pass</span>'
    pct = "" if row["strict"] is None else " %.0f%%" % (row["strict"] * 100)
    return '<span class="v v-fail">fail%s</span>' % esc(pct)


def render_summary(conn, run_id: str) -> str:
    rows = bench_db.summary(conn, run_id)
    out = ['<h2>Results</h2><div class="tablewrap"><table><thead><tr>'
           '<th>adapter</th><th>pass</th><th>declined</th><th>unscored</th>'
           '<th>strict</th><th>matched</th><th>loose</th><th>gap</th>'
           '<th>distractors</th><th>tok/scenario</th><th>calls</th>'
           '</tr></thead><tbody>']
    for r in rows:
        rate = (r["passed"] / r["attempted"] * 100) if r["attempted"] else 0
        # `matched` below `strict` is the interesting case, so mark it.
        gap_note = ""
        if r["strict"] and r["matched"] is not None and r["strict"] - r["matched"] > 0.01:
            gap_note = ' title="%.0f%% of this recall came from lines printed around a match"' % (
                (r["strict"] - r["matched"]) / r["strict"] * 100)
        out.append(
            '<tr><td><span class="dot" style="background:%s"></span>%s</td>'
            '<td class="num">%d/%d · %.0f%%</td><td class="num">%d</td><td class="num">%d</td>'
            '<td class="num">%.2f</td><td class="num"%s>%.2f</td><td class="num">%.2f</td>'
            '<td class="num">%.2f</td><td class="num">%d</td><td class="num">%.0f</td>'
            '<td class="num">%.1f</td></tr>' % (
                colour(r["adapter"]), esc(r["adapter"]), r["passed"], r["attempted"], rate,
                r["declined"], r["unscored"], r["strict"] or 0, gap_note, r["matched"] or 0,
                r["loose"] or 0, r["gap"] or 0, r["distractors"] or 0,
                r["tok_per_scenario"] or 0, r["calls"] or 0))
    out.append("</tbody></table></div>")
    return "".join(out)


def render_matrix(conn, run_id: str) -> str:
    adapters, cells = bench_db.matrix(conn, run_id)
    out = ['<h2>Every scenario <span class="muted" style="font-weight:400;font-size:.8rem">'
           '— click a cell for the tool calls behind it</span></h2>'
           '<div class="tablewrap"><table><thead><tr><th>scenario</th><th>family</th>']
    for a in adapters:
        out.append('<th><span class="dot" style="background:%s"></span>%s</th>' % (colour(a), esc(a)))
    out.append("</tr></thead><tbody>")
    for (scenario, family), by_adapter in sorted(cells.items()):
        out.append("<tr><td>%s</td><td class='muted'>%s</td>" % (esc(scenario), esc(family)))
        for a in adapters:
            row = by_adapter.get(a)
            if row is None:
                out.append('<td class="muted">·</td>')
                continue
            tok = "" if row["unsupported"] else ' <span class="muted">%d</span>' % (row["marginal"] or 0)
            out.append('<td class="cell" hx-get="/outcome?id=%d" hx-target="#detail" '
                       'hx-swap="innerHTML scroll:#detail:top">%s%s</td>'
                       % (row["id"], verdict_html(row), tok))
        out.append("</tr>")
    out.append("</tbody></table></div>")
    return "".join(out)


def render_charts(run_id: str) -> str:
    kinds = [
        ("cost", "Cost against correctness"),
        ("matched", "Found, or merely surrounded"),
        ("scenario", "What each answer cost"),
        ("resident", "Surface cost before any question"),
    ]
    out = ['<h2>Charts</h2><div class="charts">']
    for kind, label in kinds:
        out.append('<div class="chart"><img alt="%s" src="/chart.png?run=%s&kind=%s"></div>'
                   % (esc(label), esc(run_id), kind))
    out.append("</div>")
    return "".join(out)


def render_compare(conn, run_a: str, run_b: str) -> str:
    rows = bench_db.compare(conn, run_a, run_b)
    if not rows:
        return ('<h2>Compared with <code>%s</code></h2><p class="muted">'
                "Nothing moved: same verdicts, same recall, same tokens.</p>" % esc(run_b))
    out = ['<h2>Compared with <code>%s</code> <span class="muted" '
           'style="font-weight:400;font-size:.8rem">— %d cell(s) moved</span></h2>'
           '<div class="tablewrap"><table><thead><tr><th>scenario</th><th>adapter</th>'
           '<th>verdict</th><th>strict</th><th>tokens</th></tr></thead><tbody>'
           % (esc(run_b), len(rows))]

    def badge(passed, unsup):
        if unsup:
            return '<span class="v v-dec">declined</span>'
        if passed is None:
            return '<span class="v v-uns">unscored</span>'
        return ('<span class="v v-pass">pass</span>' if passed
                else '<span class="v v-fail">fail</span>')

    for r in rows:
        d_tok = (r["tok_b"] or 0) - (r["tok_a"] or 0)
        arrow = "→"
        out.append(
            '<tr><td>%s</td><td><span class="dot" style="background:%s"></span>%s</td>'
            '<td>%s %s %s</td><td class="num">%.2f %s %.2f</td>'
            '<td class="num">%d %s %d <span class="muted">(%+d)</span></td></tr>' % (
                esc(r["scenario_id"]), colour(r["adapter"]), esc(r["adapter"]),
                badge(r["passed_a"], r["unsup_a"]), arrow, badge(r["passed_b"], r["unsup_b"]),
                r["strict_a"] or 0, arrow, r["strict_b"] or 0,
                r["tok_a"] or 0, arrow, r["tok_b"] or 0, d_tok))
    out.append("</tbody></table></div>")
    return "".join(out)


def render_outcome(conn, outcome_id: int) -> str:
    row = conn.execute("SELECT * FROM outcomes WHERE id = ?", (outcome_id,)).fetchone()
    if row is None:
        return '<p class="muted">no such outcome</p>'
    out = ["<h2 style='margin-top:0'>%s <span class='muted'>·</span> %s</h2>"
           % (esc(row["scenario_id"]), esc(row["adapter"]))]

    if row["unsupported"]:
        out.append('<p><span class="v v-dec">declined</span> %s</p>' % esc(row["unsupported"]))
        return "".join(out)

    out.append('<p>%s <span class="muted">·</span> strict %.2f, matched %.2f, loose %.2f '
               '<span class="muted">·</span> %d tokens over %d call(s)</p>' % (
                   verdict_html(row), row["strict"] or 0, row["matched"] or 0,
                   row["loose"] or 0, row["marginal"] or 0, row["tool_calls"] or 0))

    degraded = conn.execute("SELECT note FROM degradations WHERE outcome_id = ?",
                            (outcome_id,)).fetchall()
    if degraded:
        out.append('<p><span class="v v-uns">degraded</span> %s</p>'
                   % ", ".join("<code>%s</code>" % esc(d["note"]) for d in degraded))

    steps = conn.execute("SELECT * FROM steps WHERE outcome_id = ? ORDER BY idx",
                         (outcome_id,)).fetchall()
    if steps:
        out.append("<h2>Tool calls</h2>")
        for s in steps:
            out.append("<div><strong>%s</strong> <span class='muted'>· %s tokens · %s ms%s</span>"
                       "<pre>%s</pre></div>" % (
                           esc(s["label"]), s["resp_cl100k"], s["elapsed_ms"],
                           " · ERROR" if s["is_error"] else "", esc(s["request"])))

    facts = conn.execute("SELECT fact, tier FROM facts WHERE outcome_id = ?",
                         (outcome_id,)).fetchall()
    if facts:
        out.append('<h2>Expected facts</h2><div class="tablewrap"><table><thead><tr>'
                   "<th>fact</th><th>found as</th></tr></thead><tbody>")
        tone = {"strict": "v-pass", "loose": "v-uns", "absent": "v-fail"}
        for f in facts:
            out.append('<tr><td class="mono">%s</td><td><span class="v %s">%s</span></td></tr>'
                       % (esc(f["fact"]), tone.get(f["tier"], "v-dec"), esc(f["tier"])))
        out.append("</tbody></table></div>")
    return "".join(out)


# --- charts -----------------------------------------------------------------

SURFACE = "#fcfcfb"
INK, INK_2, GRID = "#0b0b0b", "#52514e", "#e6e5e1"


def _fig(w=6.2, h=3.6):
    fig, ax = plt.subplots(figsize=(w, h), dpi=140)
    fig.patch.set_facecolor(SURFACE)
    ax.set_facecolor(SURFACE)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
    for side in ("left", "bottom"):
        ax.spines[side].set_color(GRID)
    ax.tick_params(colors=INK_2, labelsize=8, length=0)
    ax.set_axisbelow(True)
    return fig, ax


def _png(fig) -> bytes:
    buf = io.BytesIO()
    fig.savefig(buf, format="png", facecolor=SURFACE, bbox_inches="tight")
    plt.close(fig)
    return buf.getvalue()


def chart(conn, run_id: str, kind: str) -> bytes:
    rows = bench_db.summary(conn, run_id)

    if kind == "cost":
        fig, ax = _fig()
        for r in rows:
            if not r["attempted"]:
                continue
            x = max(r["tok_per_scenario"] or 1, 1)
            y = r["passed"] / r["attempted"] * 100
            ax.scatter([x], [y], s=90, color=colour(r["adapter"]),
                       edgecolors=SURFACE, linewidths=2, zorder=3)
            ax.annotate(r["adapter"], (x, y), textcoords="offset points", xytext=(0, 11),
                        ha="center", fontsize=7.5, color=INK)
        ax.set_xscale("log")
        ax.set_xlabel("tokens per scenario (log)", fontsize=8, color=INK_2)
        ax.set_ylabel("scenarios passed (%)", fontsize=8, color=INK_2)
        ax.set_ylim(-8, 108)
        ax.grid(axis="y", color=GRID, linewidth=1)
        ax.set_title("Cheaper and further up is better", fontsize=10, color=INK, loc="left")
        return _png(fig)

    if kind == "matched":
        fig, ax = _fig()
        names = [r["adapter"] for r in rows][::-1]
        strict = [r["strict"] or 0 for r in rows][::-1]
        matched = [r["matched"] or 0 for r in rows][::-1]
        y = range(len(names))
        for i, (s, m, n) in enumerate(zip(strict, matched, names)):
            # Where the two coincide there is nothing to compare, and drawing
            # a hollow ring on top of a filled dot reads as "matched only" —
            # the opposite of the truth, which is that every fact was matched.
            # So a single filled dot means "no incidental evidence at all".
            if abs(s - m) < 0.005:
                ax.scatter([s], [i], s=70, color=colour(n), edgecolors=SURFACE,
                           linewidths=1.6, zorder=3)
                continue
            ax.plot([m, s], [i, i], color=GRID, linewidth=3, zorder=1, solid_capstyle="round")
            ax.scatter([s], [i], s=70, color=colour(n), edgecolors=SURFACE, linewidths=1.6, zorder=3)
            ax.scatter([m], [i], s=70, facecolors=SURFACE, edgecolors=colour(n),
                       linewidths=2, zorder=3)
        ax.set_yticks(list(y)); ax.set_yticklabels(names, fontsize=8)
        ax.set_xlim(-0.03, 1.05)
        ax.set_xlabel("recall — filled: strict · hollow: matched · one dot: identical",
                      fontsize=8, color=INK_2)
        ax.grid(axis="x", color=GRID, linewidth=1)
        ax.set_title("Found, or merely surrounded", fontsize=10, color=INK, loc="left")
        return _png(fig)

    if kind == "scenario":
        data = conn.execute(
            "SELECT scenario_id, adapter, marginal, passed FROM outcomes"
            " WHERE run_id = ? AND unsupported IS NULL AND marginal > 0"
            " ORDER BY scenario_id", (run_id,)).fetchall()
        scenarios = sorted({r["scenario_id"] for r in data})
        adapters = sorted({r["adapter"] for r in data})
        # Dots, not bars. The spread here is three orders of magnitude, so the
        # axis has to be logarithmic — and a bar on a log axis encodes nothing
        # honest, because its length stops being proportional to its value and
        # its left edge is wherever the axis happens to start. A dot has no
        # length to misread.
        fig, ax = _fig(6.2, 0.34 * max(len(scenarios), 1) + 1.6)
        for si, s in enumerate(scenarios):
            ax.axhline(si, color=GRID, linewidth=1, zorder=1)
        seen = set()
        for row in data:
            si = scenarios.index(row["scenario_id"])
            a = row["adapter"]
            ax.scatter([max(row["marginal"], 1)], [si], s=46, color=colour(a),
                       edgecolors=SURFACE, linewidths=1.2, zorder=3,
                       label=a if a not in seen else None)
            seen.add(a)
        ax.set_xscale("log")
        ax.set_yticks(range(len(scenarios)))
        ax.set_yticklabels([s.split("/", 1)[-1] for s in scenarios], fontsize=7)
        ax.set_ylim(-0.6, len(scenarios) - 0.4)
        ax.set_xlabel("tokens per answer (log)", fontsize=8, color=INK_2)
        ax.grid(axis="x", color=GRID, linewidth=1)
        ax.legend(fontsize=6.5, frameon=False, ncol=4, loc="upper center",
                  bbox_to_anchor=(0.5, -0.10 - 0.6 / max(len(scenarios), 1)))
        ax.set_title("What each answer cost", fontsize=10, color=INK, loc="left")
        return _png(fig)

    # resident
    data = conn.execute(
        "SELECT name, resident_cl100k FROM adapters WHERE run_id = ? AND resident_cl100k > 0"
        " ORDER BY resident_cl100k DESC", (run_id,)).fetchall()
    fig, ax = _fig(6.2, 2.4)
    if not data:
        ax.text(0.5, 0.5, "no resident surface in this run\n(baselines have no tool schema)",
                ha="center", va="center", fontsize=9, color=INK_2)
        ax.axis("off")
        return _png(fig)
    names = [r["name"] for r in data]
    vals = [r["resident_cl100k"] for r in data]
    ax.barh(range(len(names)), vals, color=[colour(n) for n in names], zorder=2)
    for i, v in enumerate(vals):
        ax.text(v, i, " %d" % v, va="center", fontsize=7.5, color=INK_2)
    ax.set_yticks(range(len(names))); ax.set_yticklabels(names, fontsize=8)
    ax.invert_yaxis()
    ax.set_xlabel("cl100k tokens resident before any question", fontsize=8, color=INK_2)
    ax.grid(axis="x", color=GRID, linewidth=1)
    ax.set_title("Surface cost before any question", fontsize=10, color=INK, loc="left")
    return _png(fig)


# --- server -----------------------------------------------------------------

class Handler(BaseHTTPRequestHandler):
    db_path: Path = bench_db.DEFAULT_DB
    _local = threading.local()

    @property
    def conn(self) -> sqlite3.Connection:
        # One connection per thread: sqlite3 objects are not shareable, and
        # ThreadingHTTPServer will happily serve four chart requests at once.
        conn = getattr(self._local, "conn", None)
        if conn is None:
            conn = sqlite3.connect(self.db_path)
            conn.row_factory = sqlite3.Row
            self._local.conn = conn
        return conn

    def log_message(self, fmt, *args):  # quieter than the default
        pass

    def _send(self, body: bytes, content_type="text/html; charset=utf-8", status=200):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        url = urlparse(self.path)
        query = parse_qs(url.query)

        try:
            if url.path == "/":
                return self._send(self.index().encode())
            if url.path == "/static/htmx.min.js":
                return self._send((STATIC / "htmx.min.js").read_bytes(),
                                  "application/javascript")
            if url.path == "/run":
                run_id = (query.get("run") or [""])[0]
                against = (query.get("against") or [""])[0]
                if not run_id:
                    return self._send(b"<p class='muted'>pick a run</p>")
                parts = [render_summary(self.conn, run_id), render_charts(run_id),
                         render_matrix(self.conn, run_id)]
                if against and against != run_id:
                    parts.insert(1, render_compare(self.conn, run_id, against))
                return self._send("".join(parts).encode())
            if url.path == "/outcome":
                oid = int((query.get("id") or ["0"])[0])
                return self._send(render_outcome(self.conn, oid).encode())
            if url.path == "/chart.png":
                run_id = (query.get("run") or [""])[0]
                kind = (query.get("kind") or ["cost"])[0]
                return self._send(chart(self.conn, run_id, kind), "image/png")
        except Exception as exc:  # a broken panel should not kill the page
            return self._send(("<p class='muted'>error: %s</p>" % esc(exc)).encode(),
                              status=500)

        self._send(b"<p class='muted'>not found</p>", status=404)

    def index(self) -> str:
        runs = bench_db.list_runs(self.conn)
        total = self.conn.execute("SELECT COUNT(*) FROM outcomes").fetchone()[0]
        options = "".join(
            '<option value="%s">%s — %s</option>' % (
                esc(r["run_id"]), esc(r["run_id"]), esc((r["timestamp"] or "")[:19]))
            for r in runs)
        # Plain substitution, not %-formatting: the stylesheet is full of
        # literal `%` (100%, color-mix(... 30%)) and every one would have to
        # be doubled, which is a trap the next edit falls into.
        page = PAGE
        for key, value in (("nruns", len(runs)), ("noutcomes", total),
                           ("db", esc(self.db_path)), ("options", options)):
            page = page.replace("{{%s}}" % key, str(value))
        return page


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--db", type=Path, default=bench_db.DEFAULT_DB)
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--no-ingest", action="store_true",
                        help="serve what is already in the database")
    parser.add_argument("--open", action="store_true", help="open a browser")
    args = parser.parse_args(argv)

    if not args.no_ingest:
        conn = bench_db.connect(args.db)
        reports = bench_db.find_reports([bench_db.REPO / "runs", bench_db.REPO / "results"])
        for path in reports:
            try:
                bench_db.ingest_report(conn, path)
            except Exception as exc:
                print("  SKIP %s — %s" % (path, exc))
        print("ingested %d report(s)" % len(reports))
        conn.close()

    Handler.db_path = args.db
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    url = "http://%s:%d/" % (args.host, args.port)
    print("nudox-bench results on %s  (ctrl-c to stop)" % url)
    if args.open:
        threading.Timer(0.5, lambda: webbrowser.open(url)).start()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nstopping")
    return 0


if __name__ == "__main__":
    sys.exit(main())
