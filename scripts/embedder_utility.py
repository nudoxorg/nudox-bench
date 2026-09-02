"""When is the embedder actually worth its tokens?

Takes two runs of the same scenarios — one with the semantic index off, one
with it on — and reports, per adapter and scenario, which *expected facts*
changed tier and what the change cost.

The pass column cannot answer this on its own. A scenario can gain recall and
still fail, lose recall and still pass, or pass on text the listing happened to
carry; and the token price of the semantic section is paid on every query
whether or not it contributed. Facts moved per thousand tokens spent is the
ratio that says whether the section earned its place on a given query.

Usage: embedder_utility.py <off.json> <on.json>
"""
import json
import sys
from collections import defaultdict

RANK = {"absent": 0, "loose": 1, "strict": 2}


def load(path):
    d = json.load(open(path))
    out = {}
    for o in d["outcomes"]:
        out[(o["adapter"], o["scenario_id"])] = o
    return d["run_id"], out


def cost(o):
    c = o["attempt"]["cost"]
    return c["requests"]["cl100k"] + c["responses"]["cl100k"]


def scored(o):
    if o["attempt"].get("unsupported") is not None:
        return False
    return not (o["score"].get("judged") and o.get("judge") is None)


off_id, off = load(sys.argv[1])
on_id, on = load(sys.argv[2])

print(f"embedder OFF : {off_id}")
print(f"embedder ON  : {on_id}")

per_adapter = defaultdict(lambda: [0, 0, 0, 0])   # gained, lost, dtok, n
verdicts = []

for key in sorted(off.keys() & on.keys()):
    adapter, scenario = key
    a, b = off[key], on[key]
    if not (scored(a) and scored(b)):
        continue

    fa = {f["fact"]: f["tier"] for f in a["score"]["facts"]}
    fb = {f["fact"]: f["tier"] for f in b["score"]["facts"]}
    gained = [f for f in fa if RANK[fb.get(f, "absent")] > RANK[fa[f]]]
    lost = [f for f in fa if RANK[fb.get(f, "absent")] < RANK[fa[f]]]
    dtok = cost(b) - cost(a)

    agg = per_adapter[adapter]
    agg[0] += len(gained)
    agg[1] += len(lost)
    agg[2] += dtok
    agg[3] += 1

    if gained or lost:
        verdicts.append((adapter, scenario, gained, lost, dtok,
                         a["score"]["passed"], b["score"]["passed"]))

print("\n--- scenarios where the semantic section changed the answer ---")
if not verdicts:
    print("  none: every fact landed in the same tier either way")
for adapter, scenario, gained, lost, dtok, pa, pb in verdicts:
    flip = ("pass→pass", "pass→FAIL", "fail→PASS", "fail→fail")[
        (0 if pa else 2) + (0 if pb else 1)]
    print(f"\n  {scenario}  ·  {adapter}   [{flip}]   {dtok:+,} tok")
    for f in gained:
        print(f"      + {f}")
    for f in lost:
        print(f"      - {f}")

print(f"\n--- per adapter, over the scenarios scored both ways ---")
print(f"{'adapter':<22}{'scen':>6}{'facts+':>8}{'facts-':>8}"
      f"{'net':>6}{'Δtok':>10}{'tok/fact gained':>18}")
for adapter in sorted(per_adapter):
    gained, lost, dtok, n = per_adapter[adapter]
    net = gained - lost
    rate = f"{dtok/net:,.0f}" if net > 0 else ("—" if net == 0 else "worse & dearer")
    print(f"{adapter:<22}{n:>6}{gained:>8}{lost:>8}{net:>6}{dtok:>+10,}{rate:>18}")

print("\nA positive net at a low token rate is the semantic section earning its")
print("place. A zero net with a large Δtok is the case for gating it.")
