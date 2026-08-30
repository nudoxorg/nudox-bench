#!/usr/bin/env python3
"""Render the charts for a nudox-bench run.

Usage:  python3 scripts/plot_results.py runs/run-YYYY-MM-DD-HHMMSS [outdir]

Every number plotted comes from that run's report.json. Nothing is smoothed,
interpolated or hand-entered: if a bar looks wrong, the run was wrong.

Palette: slots 1-3 of the validated categorical default (blue/orange/aqua),
which clears the all-pairs CVD and normal-vision floors in both modes. Aqua
sits below 3:1 on the light surface, so every mark is directly labelled —
identity is never carried by colour alone.
"""
import json
import sys
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.ticker import FuncFormatter

# --- design tokens ---------------------------------------------------------
SURFACE = "#fcfcfb"
INK = "#0b0b0b"
INK_2 = "#52514e"
INK_MUTED = "#8a8983"
GRID = "#e6e5e1"

SERIES = {  # adapter -> categorical slot, assigned in fixed order, never cycled
    "nudox-full": "#2a78d6",
    "ripgrep": "#eb6834",
    "rust-analyzer": "#1baf7a",
    "nudox-oneshot": "#eda100",
    "nudox-search-only": "#e87ba4",
    "nudox-graph-first": "#4a3aa7",
    "nudox-keys": "#e34948",
}
FALLBACK = "#8a8983"


def colour(name):
    return SERIES.get(name, FALLBACK)


def style_axes(ax, xgrid=True):
    """Recessive grid and axes; the data is the thing on the page."""
    ax.set_facecolor(SURFACE)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
    for side in ("left", "bottom"):
        ax.spines[side].set_color(GRID)
        ax.spines[side].set_linewidth(1)
    ax.tick_params(colors=INK_2, labelsize=9, length=0)
    if xgrid:
        ax.grid(axis="x", color=GRID, linewidth=1, zorder=0)
        ax.set_axisbelow(True)


def titles(fig, title, subtitle, top=0.86):
    """Figure-level title block.

    Drawn on the figure rather than the axes because an axes title and a
    hand-placed subtitle collide as soon as either wraps — reserving the space
    once, above everything, is the only version that stays correct when the
    figure is resized.
    """
    fig.subplots_adjust(top=top)
    fig.text(0.012, 0.975, title, fontsize=13.5, color=INK,
             weight="bold", va="top", ha="left")
    fig.text(0.012, 0.905, subtitle, fontsize=9.5, color=INK_2,
             va="top", ha="left")


def thousands(value, _pos):
    if value >= 1000:
        return f"{value/1000:g}k"
    return f"{value:g}"


def load(run_dir):
    with open(Path(run_dir) / "report.json") as handle:
        return json.load(handle)


def summarise(report):
    """Recompute the per-adapter aggregate the Rust reporter produces.

    Kept deliberately in step with `AdapterSummary::summarize`: `unsupported`
    and `unscored` outcomes are excluded from `attempted`, so a tool that
    cannot express a question is never charged for getting it wrong.
    """
    rows = {}
    for info in report["adapters"]:
        rows[info["name"]] = {
            "adapter": info["name"],
            "resident": info["resident"]["cl100k"],
            "attempted": 0,
            "passed": 0,
            "unsupported": 0,
            "unscored": 0,
            "tokens": 0,
            "strict": 0.0,
            "loose": 0.0,
        }
    for outcome in report["outcomes"]:
        row = rows.setdefault(outcome["adapter"], dict(
            adapter=outcome["adapter"], resident=0, attempted=0, passed=0,
            unsupported=0, unscored=0, tokens=0, strict=0.0, loose=0.0))
        attempt = outcome["attempt"]
        if attempt.get("unsupported"):
            row["unsupported"] += 1
            continue
        judged = outcome["score"].get("judged") and not outcome.get("judge")
        if judged:
            row["unscored"] += 1
            continue
        row["attempted"] += 1
        cost = attempt["cost"]
        row["tokens"] += cost["responses"]["cl100k"] + cost["requests"]["cl100k"]
        row["strict"] += outcome["score"]["strict_recall"]
        row["loose"] += outcome["score"]["loose_recall"]
        if outcome["score"]["passed"] or (outcome.get("judge") or {}).get("passed"):
            row["passed"] += 1
    for row in rows.values():
        n = max(row["attempted"], 1)
        row["pass_rate"] = row["passed"] / n
        row["tok_per_scenario"] = row["tokens"] / n
        row["strict"] /= n
        row["loose"] /= n
        row["gap"] = max(row["loose"] - row["strict"], 0.0)
    return [r for r in rows.values() if r["attempted"] or r["unsupported"]]


# --- chart 1: cost against correctness -------------------------------------
def chart_efficiency(rows, out):
    """Two measures that trade against each other, on one plane.

    A log x-axis because the spread is more than an order of magnitude — on a
    linear axis every adapter but the most expensive collapses onto the y-axis
    and the chart says nothing.
    """
    fig, ax = plt.subplots(figsize=(8, 5), facecolor=SURFACE)
    style_axes(ax)
    ax.grid(axis="y", color=GRID, linewidth=1)

    plotted = [r for r in rows if r["attempted"]]
    for row in plotted:
        ax.scatter(row["tok_per_scenario"], row["pass_rate"] * 100,
                   s=190, color=colour(row["adapter"]), zorder=3,
                   edgecolors=SURFACE, linewidths=2)
        ax.annotate(row["adapter"],
                    (row["tok_per_scenario"], row["pass_rate"] * 100),
                    textcoords="offset points", xytext=(0, 15),
                    ha="center", fontsize=9.5, color=INK, weight="normal")
        ax.annotate(f"{row['tok_per_scenario']:,.0f} tok · {row['pass_rate']*100:.0f}%",
                    (row["tok_per_scenario"], row["pass_rate"] * 100),
                    textcoords="offset points", xytext=(0, -24),
                    ha="center", fontsize=8.5, color=INK_MUTED)

    ax.set_xscale("log")
    ax.xaxis.set_major_formatter(FuncFormatter(thousands))
    ax.set_xlabel("tokens per scenario  (cl100k, log scale)", fontsize=9.5, color=INK_2)
    ax.set_ylabel("scenarios passed", fontsize=9.5, color=INK_2)
    ax.yaxis.set_major_formatter(FuncFormatter(lambda v, _: f"{v:g}%"))
    ax.set_ylim(-8, 112)
    ax.margins(x=0.22)
    titles(fig, "Cheaper and further up is better",
           "Cost against correctness — every point is one adapter over the same scenarios",
           top=0.82)
    fig.savefig(out, dpi=200, facecolor=SURFACE)
    plt.close(fig)


# --- chart 2: per-scenario cost --------------------------------------------
# The three-way comparison the per-scenario chart shows. Seven series on one
# grouped bar chart is unreadable and past the point where categorical hues
# stay separable; the ablations get their own chart instead.
HEADLINE = ["nudox-full", "ripgrep", "rust-analyzer"]


def chart_cost_per_scenario(report, out, only=None):
    adapters = [a["name"] for a in report["adapters"]]
    if only:
        adapters = [a for a in adapters if a in only]
    scenarios = sorted({o["scenario_id"] for o in report["outcomes"]})
    index = {(o["adapter"], o["scenario_id"]): o for o in report["outcomes"]}

    # Drop scenarios nobody could score — an all-"n/a" row is empty ink.
    keep = []
    for scenario in scenarios:
        if any(not index[(a, scenario)]["attempt"].get("unsupported")
               and not (index[(a, scenario)]["score"].get("judged")
                        and not index[(a, scenario)].get("judge"))
               for a in adapters if (a, scenario) in index):
            keep.append(scenario)
    scenarios = keep

    height = max(3.4, 0.40 * len(scenarios) * len(adapters) + 1.9)
    fig, ax = plt.subplots(figsize=(9.5, height), facecolor=SURFACE)
    style_axes(ax)

    bar_h = 0.72 / max(len(adapters), 1)
    ticks, labels = [], []
    for row, scenario in enumerate(scenarios):
        centre = row * 1.0
        ticks.append(centre)
        labels.append(scenario.split("/", 1)[-1])
        for slot, adapter in enumerate(adapters):
            outcome = index.get((adapter, scenario))
            if not outcome:
                continue
            attempt = outcome["attempt"]
            offset = (slot - (len(adapters) - 1) / 2) * bar_h
            y = centre - offset
            if attempt.get("unsupported"):
                ax.text(1.2, y, "declined", va="center", fontsize=7.5,
                        color=INK_MUTED, style="italic")
                continue
            cost = attempt["cost"]
            value = cost["responses"]["cl100k"] + cost["requests"]["cl100k"]
            if value <= 0:
                continue
            passed = outcome["score"]["passed"] or (outcome.get("judge") or {}).get("passed")
            ax.barh(y, value, height=bar_h * 0.82, color=colour(adapter),
                    zorder=3, alpha=1.0 if passed else 0.42)
            ax.text(value * 1.08, y, f"{value:,}" + ("" if passed else "  ✗"),
                    va="center", fontsize=7.5,
                    color=INK_2 if passed else INK_MUTED)

    ax.set_yticks(ticks)
    ax.set_yticklabels(labels, fontsize=8.5, color=INK)
    ax.invert_yaxis()
    ax.set_xscale("log")
    ax.xaxis.set_major_formatter(FuncFormatter(thousands))
    ax.set_xlabel("tokens returned  (cl100k, log scale)", fontsize=9.5, color=INK_2)

    handles = [plt.Line2D([], [], marker="s", linestyle="none", markersize=8,
                          color=colour(a), label=a) for a in adapters]
    ax.legend(handles=handles, loc="lower right", frameon=False,
              fontsize=8.5, labelcolor=INK_2, ncols=min(len(adapters), 3))
    fig.tight_layout()
    titles(fig, "What each answer cost",
           "Faded bars marked ✗ are wrong answers — cheap and wrong is not efficient",
           top=1 - 1.5 / height)
    fig.savefig(out, dpi=200, facecolor=SURFACE)
    plt.close(fig)


# --- chart 3: the resolution gap -------------------------------------------
def chart_resolution(rows, out):
    """Strict recall against loose recall.

    The distance between the two dots is the measurement: how much of an
    adapter's apparent success is a bare identifier in a blob of text rather
    than a resolved symbol.
    """
    plotted = [r for r in rows if r["attempted"]]
    fig, ax = plt.subplots(figsize=(8, 0.9 * len(plotted) + 2.2), facecolor=SURFACE)
    style_axes(ax)

    for row_index, row in enumerate(plotted):
        y = row_index
        c = colour(row["adapter"])
        ax.plot([row["strict"] * 100, row["loose"] * 100], [y, y],
                color=c, linewidth=2, zorder=2, alpha=0.45,
                solid_capstyle="round")
        ax.scatter([row["loose"] * 100], [y], s=110, facecolor=SURFACE,
                   edgecolors=c, linewidths=2, zorder=3)
        ax.scatter([row["strict"] * 100], [y], s=130, color=c, zorder=4,
                   edgecolors=SURFACE, linewidths=2)
        gap = row["gap"] * 100
        note = "no gap — fully resolved" if gap < 0.5 else f"gap {gap:.0f} pts"
        ax.text(max(row["loose"], row["strict"]) * 100 + 3, y, note,
                va="center", fontsize=8.5,
                color=INK_2 if gap >= 0.5 else "#008300")

    ax.set_yticks(range(len(plotted)))
    ax.set_yticklabels([r["adapter"] for r in plotted], fontsize=9.5, color=INK)
    ax.invert_yaxis()
    ax.set_xlim(-2, 118)
    ax.xaxis.set_major_formatter(FuncFormatter(lambda v, _: f"{v:g}%"))
    ax.set_xlabel("mean recall", fontsize=9.5, color=INK_2)
    fig.tight_layout()
    titles(fig, "How much of the answer was actually resolved",
           "Filled = strict (qualified path)   ·   Hollow = loose (bare identifier anywhere)",
           top=1 - 1.35 / (0.9 * len(plotted) + 2.2))
    fig.savefig(out, dpi=200, facecolor=SURFACE)
    plt.close(fig)


# --- chart 4: the resident surface -----------------------------------------
def chart_surface(per_tool, out, total):
    """What the tool surface costs before the agent asks anything."""
    fig, ax = plt.subplots(figsize=(8, 0.42 * len(per_tool) + 2.4), facecolor=SURFACE)
    style_axes(ax)
    names = [t[0] for t in per_tool]
    values = [t[1] for t in per_tool]
    positions = range(len(names))
    ax.barh(list(positions), values, height=0.62, color="#2a78d6", zorder=3)
    for y, (name, value) in enumerate(per_tool):
        share = value / total * 100 if total else 0
        ax.text(value + max(values) * 0.015, y, f"{value:,}  ({share:.0f}%)",
                va="center", fontsize=8.5, color=INK_2)
    ax.set_yticks(list(positions))
    ax.set_yticklabels(names, fontsize=9.5, color=INK)
    ax.invert_yaxis()
    ax.set_xlim(0, max(values) * 1.28)
    ax.xaxis.set_major_formatter(FuncFormatter(thousands))
    ax.set_xlabel("cl100k tokens resident before any call", fontsize=9.5, color=INK_2)
    fig.tight_layout()
    titles(fig, f"The tool surface costs {total:,} tokens before a question is asked",
           "Tool description plus every argument-schema description, per tool",
           top=1 - 1.3 / (0.42 * len(per_tool) + 2.4))
    fig.savefig(out, dpi=200, facecolor=SURFACE)
    plt.close(fig)


def chart_ablations(rows, out):
    """Nudox against itself: what each restriction costs and buys.

    Paired bars would imply the two measures share a scale; they do not. So
    pass rate is the bar and cost is a labelled annotation — one axis, one
    encoding, the second measure written out.
    """
    ablations = [r for r in rows if r["adapter"].startswith("nudox") and r["attempted"]]
    ablations.sort(key=lambda r: (-r["pass_rate"], r["tok_per_scenario"]))
    if not ablations:
        return
    height = 0.62 * len(ablations) + 2.6
    fig, ax = plt.subplots(figsize=(8.6, height), facecolor=SURFACE)
    style_axes(ax)

    positions = range(len(ablations))
    ax.barh(list(positions), [r["pass_rate"] * 100 for r in ablations],
            height=0.6, color=[colour(r["adapter"]) for r in ablations], zorder=3)
    for y, row in enumerate(ablations):
        ax.text(row["pass_rate"] * 100 + 1.6, y,
                f"{row['passed']}/{row['attempted']}   ·   "
                f"{row['tok_per_scenario']:,.0f} tok/scenario",
                va="center", fontsize=8.5, color=INK_2)
    ax.set_yticks(list(positions))
    ax.set_yticklabels([r["adapter"] for r in ablations], fontsize=9.5, color=INK)
    ax.invert_yaxis()
    ax.set_xlim(0, 100)
    ax.xaxis.set_major_formatter(FuncFormatter(lambda v, _: f"{v:g}%"))
    ax.set_xlabel("scenarios passed", fontsize=9.5, color=INK_2)
    fig.tight_layout()
    titles(fig, "Nudox against itself",
           "Same server, five client-side restrictions — what each one costs and buys",
           top=1 - 1.5 / height)
    fig.savefig(out, dpi=200, facecolor=SURFACE)
    plt.close(fig)


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    run_dir = Path(sys.argv[1])
    out_dir = Path(sys.argv[2]) if len(sys.argv) > 2 else run_dir / "charts"
    out_dir.mkdir(parents=True, exist_ok=True)

    report = load(run_dir)
    rows = summarise(report)

    chart_efficiency(rows, out_dir / "efficiency.png")
    chart_cost_per_scenario(report, out_dir / "cost_per_scenario.png", only=HEADLINE)
    chart_ablations(rows, out_dir / "ablations.png")
    chart_resolution(rows, out_dir / "resolution_gap.png")

    surface_file = run_dir / "surface.json"
    if surface_file.exists():
        with open(surface_file) as handle:
            data = json.load(handle)
        per_tool = [(t["name"], t["cl100k"]) for t in data["tools"]]
        per_tool.sort(key=lambda t: -t[1])
        chart_surface(per_tool, out_dir / "surface_cost.png", data["total"])

    print(f"charts written to {out_dir}")
    for row in sorted(rows, key=lambda r: -r["pass_rate"]):
        print(f"  {row['adapter']:<20} {row['passed']}/{row['attempted']} "
              f"{row['tok_per_scenario']:>8,.0f} tok  gap {row['gap']*100:.0f}pts")


if __name__ == "__main__":
    main()
