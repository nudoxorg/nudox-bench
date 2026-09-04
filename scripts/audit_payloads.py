"""Audit every stored payload in a nudox-bench report for scoring artifacts.

Two questions per scored fact:

  * Was it credited from a line ripgrep actually MATCHED, or only from a
    CONTEXT line that the -B/-A window happened to drag in? A fact credited
    only by context is recall the tool did not really produce.
  * How much of the returned payload is test/vendor noise?

rg is invoked with `-n --no-heading`, which writes matched lines as
`path:line:text` and context lines as `path-line-text`. That one character is
the whole signal.
"""
import json
import re
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

REPO = Path(__file__).resolve().parent.parent
# The run audited when none is named. It is the first published one, kept as
# the default because the artifacts it contains are the ones the written
# results discuss — but any run can be audited by naming it.
DEFAULT_REPORT = REPO / "results/2026-08-30/report.json"

EXT = r"(?:go|rs|py|ts|tsx|js|mjs|cjs|toml|md|txt|json|cfg|ini)"
LINE_RE = re.compile(r"^(?P<path>\S*?\.%s)(?P<sep>[:-])(?P<line>\d+)[:-]" % EXT)


def norm(s):
    return s.replace("::", "\x01").replace(".", "\x01").replace("/", "\x01").lower()


def word(hay_lower, needle):
    return re.search(
        r"(?<![A-Za-z0-9_])" + re.escape(needle.lower()) + r"(?![A-Za-z0-9_])",
        hay_lower,
    ) is not None


def tail(fact):
    return fact.replace("::", ".").replace("/", ".").split(".")[-1]


def load_scenarios():
    out = {}
    for p in sorted((REPO / "scenarios").rglob("*.toml")):
        d = tomllib.loads(p.read_text())
        t = d.get("truth", {})
        out[d["id"]] = {
            "all_of": t.get("all_of", []),
            "any_of": t.get("any_of", []),
            "none_of": t.get("none_of", []),
            "family": d.get("family"),
        }
    return out


def classify(ctx):
    """Split a ripgrep payload into (match_lines, context_lines, other)."""
    match, context, other = [], [], []
    for raw in ctx.splitlines():
        m = LINE_RE.match(raw)
        if not m:
            other.append(raw)
        elif m.group("sep") == ":":
            match.append(raw)
        else:
            context.append(raw)
    return match, context, other


def is_test(line):
    low = line.lower()
    return ("_test." in low or "/test/" in low or "/tests/" in low
            or "test_" in low.rsplit("/", 1)[-1] or ".test." in low)


def resolve_report(argv):
    """The report to audit: a run directory, a report.json, or the default.

    The path used to be hardcoded, so this script could only ever audit one
    run — and that run is now several behind. `--help` was swallowed too, and
    silently audited the default instead of answering.
    """
    args = [a for a in argv[1:] if a not in ("-h", "--help")]
    if len(args) != len(argv[1:]):
        print(__doc__)
        print("usage: audit_payloads.py [run-dir | report.json]")
        print("       defaults to %s" % DEFAULT_REPORT.relative_to(REPO))
        sys.exit(0)
    if not args:
        return DEFAULT_REPORT
    path = Path(args[0])
    if path.is_dir():
        path = path / "report.json"
    if not path.is_file():
        sys.exit("no report at %s" % path)
    return path


def main():
    scen = load_scenarios()
    report_path = resolve_report(sys.argv)
    rep = json.loads(report_path.read_text())

    print("# Payload audit — %s\n" % rep["run_id"])
    artifacts = []

    for o in rep["outcomes"]:
        att = o["attempt"]
        ctx = att.get("context") or ""
        if not ctx:
            continue
        sid, ad = o["scenario_id"], o["adapter"]
        truth = scen.get(sid, {})
        all_of = truth.get("all_of", [])
        none_of = truth.get("none_of", [])
        score = o["score"]

        match, context, other = classify(ctx)
        total = len(match) + len(context)
        rg_like = total > 0

        test_lines = sum(1 for l in match + context if is_test(l))
        test_share = (100.0 * test_lines / total) if total else 0.0

        rows = []
        for fact in all_of:
            n = norm(fact)
            t = tail(fact)
            in_match = any(n in norm(l) or word(l.lower(), t) for l in match)
            in_ctx = any(n in norm(l) or word(l.lower(), t) for l in context)
            if in_match:
                rows.append((fact, "match"))
            elif in_ctx:
                rows.append((fact, "CONTEXT-ONLY"))
            else:
                rows.append((fact, "absent"))

        distract = []
        for bad in none_of:
            t = tail(bad)
            in_match = any(word(l.lower(), t) for l in match)
            in_ctx = any(word(l.lower(), t) for l in context)
            if in_match:
                distract.append((bad, "match"))
            elif in_ctx:
                distract.append((bad, "CONTEXT-ONLY"))

        ctx_only = [f for f, k in rows if k == "CONTEXT-ONLY"]
        bad_ctx = [b for b, k in distract if k == "CONTEXT-ONLY"]

        if not rg_like and not ctx_only:
            continue

        flag = ""
        if ctx_only:
            flag = "  <-- recall from context lines"
        if bad_ctx:
            flag += "  <-- distractor from context lines"

        print("## %s / %s%s" % (ad, sid, flag))
        print("   payload %d lines: %d matched, %d context, %d other; %.0f%% from test files"
              % (len(ctx.splitlines()), len(match), len(context), len(other), test_share))
        print("   harness: strict %.2f loose %.2f distractors %d passed %s"
              % (score["strict_recall"], score["loose_recall"],
                 score["distractors"], score["passed"]))
        if rows:
            print("   facts: " + ", ".join("%s=%s" % (tail(f), k) for f, k in rows))
        if distract:
            print("   distractors: " + ", ".join("%s=%s" % (tail(b), k) for b, k in distract))
        print()

        if ctx_only or bad_ctx:
            artifacts.append((ad, sid, ctx_only, bad_ctx, test_share, score["passed"]))

    print("\n# Summary of artifacts\n")
    if not artifacts:
        print("none found")
    for ad, sid, co, bc, ts, passed in artifacts:
        print("* **%s / %s** — %s%s%s (%.0f%% test lines, harness verdict: %s)" % (
            ad, sid,
            ("recall credited only by context lines: %s" % ", ".join(tail(f) for f in co)) if co else "",
            "; " if co and bc else "",
            ("distractor arrived only via context lines: %s" % ", ".join(tail(b) for b in bc)) if bc else "",
            ts, "pass" if passed else "fail"))


main()
