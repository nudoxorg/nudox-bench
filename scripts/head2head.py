"""Head-to-head on the scenarios every adapter actually answered.

The report's pass column divides by each adapter's own scored count, so
`nudox-full 4/5` and `ripgrep 6/7` are fractions of different denominators —
an adapter can raise its rate by declining the scenarios it would have failed.
For a competitive claim the only honest denominator is the intersection: the
scenarios that every adapter both attempted and could be scored on.

Scored-ness mirrors `Outcome::passed` (and `bench_db.py`): a declined attempt
has no verdict, and a judged scenario with no judge stays unscored rather than
counting as a failure.

Usage: head2head.py <report.json> [report.json ...]
"""
import json
import re
import sys
from collections import Counter, defaultdict


def verdict(o):
    """(scored, passed) for one outcome."""
    if o["attempt"].get("unsupported") is not None:
        return False, False
    judge = o.get("judge")
    if judge is not None:
        return True, bool(judge.get("passed"))
    if o["score"].get("judged"):
        return False, False
    return True, bool(o["score"]["passed"])


def cost(o):
    """What the harness meters: requests plus responses, cl100k."""
    c = o["attempt"]["cost"]
    return c["requests"]["cl100k"] + c["responses"]["cl100k"]


SEM = re.compile(r"~sem:(\S+)")


def semantic_state(outcomes):
    """What the engine said about its own semantic index, per search step.

    The engine omits the status entirely when it is Ready, so an absent marker
    means ready and any present one means it was not. A run whose search steps
    disagree is not one system being measured — it is two, and comparing it
    against anything is a mistake. This is the check that would have caught
    four arms measured against an index that was still building while the
    launch log said `semantic search: on`.
    """
    seen = Counter()
    for o in outcomes:
        if not o["adapter"].startswith("nudox"):
            continue
        for s in o["attempt"]["steps"]:
            if s["label"] != "search":
                continue
            m = SEM.search(s.get("response") or "")
            seen[m.group(1) if m else "ready"] += 1
    return seen


def load(path):
    d = json.load(open(path))
    rows = defaultdict(dict)
    for o in d["outcomes"]:
        rows[o["adapter"]][o["scenario_id"]] = o
    return d["run_id"], rows, semantic_state(d["outcomes"])


def report(run_id, rows, sem):
    adapters = sorted(rows)
    scored = {a: {s for s, o in rows[a].items() if verdict(o)[0]} for a in adapters}
    common = sorted(set.intersection(*scored.values())) if scored else []

    allsc = sorted(set().union(*scored.values())) if scored else []
    print(f"\n=== {run_id} ===")
    states = ", ".join(f"{k}×{v}" for k, v in sorted(sem.items()))
    warn = "" if len(sem) <= 1 else "   ** MIXED — not one system **"
    print(f"semantic index during search steps: {states}{warn}")
    if "ready" not in sem and sem:
        print("  ! no search step ran against a ready semantic index")
    print(f"scenarios scored by at least one adapter: {len(allsc)}   "
          f"by every adapter: {len(common)}")
    for s in common:
        print(f"    · {s}")
    dropped = [s for s in allsc if s not in common]
    if dropped:
        print("  outside the common set (someone declined or could not be scored):")
        for s in dropped:
            who = [a for a in adapters if s not in scored[a]]
            print(f"    · {s:<46} {', '.join(who)}")
    if not common:
        print("    (no common subset — nothing comparable)")
        return

    print(f"\n{'adapter':<22}{'pass':>10}{'strict':>8}{'tok/scen':>10}"
          f"{'tok/fact':>9}{'calls':>7}")
    table = []
    for a in adapters:
        sub = [rows[a][s] for s in common]
        passed = sum(1 for o in sub if verdict(o)[1])
        strict = sum(o["score"]["strict_recall"] for o in sub) / len(sub)
        tot = sum(cost(o) for o in sub)
        facts = sum(1 for o in sub for f in o["score"]["facts"]
                    if f["tier"] == "strict")
        calls = sum(o["attempt"]["tool_calls"] for o in sub) / len(sub)
        table.append((passed, strict, tot / len(sub),
                      tot / facts if facts else float("inf"), calls, a))

    for passed, strict, tok, per_fact, calls, a in sorted(
            table, key=lambda r: (-r[0], -r[1], r[2])):
        pf = "—" if per_fact == float("inf") else f"{per_fact:,.0f}"
        print(f"{a:<22}{passed:>5}/{len(common):<4}{strict:>8.2f}"
              f"{tok:>10,.0f}{pf:>9}{calls:>7.1f}")


for p in sys.argv[1:]:
    report(*load(p))
