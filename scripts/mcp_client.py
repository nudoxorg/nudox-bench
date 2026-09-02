"""Minimal MCP streamable-HTTP client, matching src/mcp/client.rs's conventions."""
import json
import os
import urllib.request

URL = os.environ.get("NUDOX_MCP_URL", "http://127.0.0.1:44921/mcp")
TOKEN = os.environ.get("NUDOX_MCP_TOKEN")

_session = {"id": None}
_next_id = {"n": 0}


def _post(body):
    data = json.dumps(body).encode()
    req = urllib.request.Request(URL, data=data, method="POST")
    req.add_header("Content-Type", "application/json")
    req.add_header("Accept", "application/json, text/event-stream")
    if TOKEN:
        req.add_header("Authorization", "Bearer " + TOKEN)
    if _session["id"]:
        req.add_header("Mcp-Session-Id", _session["id"])
    with urllib.request.urlopen(req, timeout=180) as resp:
        sid = resp.headers.get("Mcp-Session-Id")
        if sid:
            _session["id"] = sid
        ctype = resp.headers.get("Content-Type", "")
        text = resp.read().decode()
    if not text.strip():
        return None
    if "text/event-stream" in ctype:
        for line in text.splitlines():
            if not line.startswith("data:"):
                continue
            payload = line[5:].strip()
            if not payload:
                continue  # the server opens the stream with an empty keepalive
            try:
                msg = json.loads(payload)
            except json.JSONDecodeError:
                continue
            if "id" in msg and ("result" in msg or "error" in msg):
                return msg
        raise RuntimeError("no result frame in SSE: " + text[:400])
    return json.loads(text)


def rpc(method, params=None):
    _next_id["n"] += 1
    body = {"jsonrpc": "2.0", "id": _next_id["n"], "method": method}
    if params is not None:
        body["params"] = params
    msg = _post(body)
    if msg is None:
        return None
    if "error" in msg:
        raise RuntimeError(f"{method}: {msg['error']}")
    return msg.get("result")


def notify(method, params=None):
    body = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        body["params"] = params
    _post(body)


def init():
    r = rpc("initialize", {
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "nudox-bench-analysis", "version": "0"},
    })
    notify("notifications/initialized")
    return r


def call(tool, args):
    r = rpc("tools/call", {"name": tool, "arguments": args})
    parts = []
    for c in r.get("content", []):
        if c.get("type") == "text":
            parts.append(c["text"])
    return "\n".join(parts), r
