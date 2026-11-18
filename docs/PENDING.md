# Pending comparisons

Tracks that are designed for but not built, and what each one needs. Nothing
here is wired into a config yet.

## Context7

A documentation-retrieval MCP server — the comparison context-bench was built
around. Needs:

* an API key (`CONTEXT7_API_KEY`) and outbound network to its hosted service;
  neither was available in the session that built the `mcp-stdio` adapter;
* an `mcp-stdio` profile (`src/adapter/mcp_stdio.rs`) mapping the scenario's
  package and question onto its library-resolve then docs-fetch pair, and
  declining structural tasks (callers, implementors, outbound, diff) that a
  documentation index cannot express;
* the scenarios' `target.version` honoured: Context7 answers about whatever
  version it holds, so every attempt should record which version came back.

## Agent-driven track

Today every adapter maps a task onto a fixed tool sequence. An agent-driven
track would hand the scenario's `question` and a tool surface to a model and
let it choose the calls, scoring the transcript the same way. Needs:

* a model API key (the judge already reads `OPENROUTER_API_KEY`) and network;
* a driver loop with a hard call budget (`budget.max_tool_calls`) and the same
  `AttemptBuilder` accounting, so cost stays comparable to the scripted rows;
* repeated runs per cell: unlike the scripted adapters it is not
  deterministic, so `nbench verify` becomes a requirement rather than a check.

## Go scenarios on this machine

Not a missing adapter, a missing package: `go mod download` could not reach
`proxy.golang.org` from the session that added the newer scenarios, so the
three `go/uuid/*` scenarios declined there (`target.root does not exist`).
