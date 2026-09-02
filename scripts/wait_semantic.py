"""Readiness gate: has the semantic index finished building *for the scope the
benchmark actually queries*?

The endpoint prints `NUDOX_MCP_URL=` — and `semantic search: on` — as soon as
the packages are indexed, but the embeddings are built lazily and per package.
Between the two a search returns structural rows only.

Row counts are the wrong signal for this: an unscoped query can look warm off
one package's embeddings while the package the benchmark asks about is still
building. The engine states it exactly, in the response — `~sem:building(0/1)`
while it works, and *nothing at all* when ready, because `SemanticStatus::Ready`
is deliberately omitted so a plain structural search does not pay a token for a
status that is always the same value.

So: query the benchmark's own scope and wait for the marker to disappear.
Exits 0 when ready, 1 if it never becomes ready.
"""
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import mcp_client as mcp

SEM = re.compile(r"~sem:(\S+)")
SCOPE = os.environ.get("PROBE_PACKAGE", "cargo:memchr")
deadline = time.time() + float(os.environ.get("PROBE_SECS", "1800"))
started = time.time()
last = None

while time.time() < deadline:
    try:
        if mcp._session["id"] is None:
            mcp.init()
        text, _ = mcp.call("search", {"query": "memchr", "packages": [SCOPE],
                                      "limit": 200})
    except Exception as exc:
        print(f"  t+{time.time()-started:5.0f}s  not answering ({type(exc).__name__})",
              flush=True)
        time.sleep(5)
        continue

    m = SEM.search(text)
    status = m.group(1) if m else "ready"
    rows = sum(1 for l in text.split("\n")
               if l and not l.startswith(" ") and l.rstrip().endswith("]"))
    if status != last:
        print(f"  t+{time.time()-started:5.0f}s  {SCOPE}  sem={status}  rows={rows}",
              flush=True)
        last = status
    if status == "ready":
        print(f"  ready after {time.time()-started:.0f}s", flush=True)
        time.sleep(5)
        sys.exit(0)
    time.sleep(10)

print(f"  NOT ready after {time.time()-started:.0f}s (last: {last})", flush=True)
sys.exit(1)
