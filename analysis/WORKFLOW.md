# WORKFLOW — producing a validated Nudox improvement proposal

An execution plan for a multi-agent run. It is written to be followed literally.
Phases are sequential; agents *within* a phase run in parallel.

**Input artifact (mandatory reading for every agent in every phase):**
`/home/leaf/Dev/nudox/nudox-bench/.claude/worktrees/bench-improvement-proposal/analysis/EVIDENCE.md`

**Output artifacts, in order of production:**

| phase | artifact |
|---|---|
| 1 | `analysis/findings/E1.md` … `E5.md` |
| 2 | `analysis/gates/CITATIONS.md`, `analysis/gates/CONTRADICTIONS.md`, `analysis/gates/COVERAGE.md` |
| 3 | `analysis/PROPOSAL.md` |
| 4 | `analysis/gates/VALIDATION.md` |

**The question the whole workflow serves:** enabling the semantic embedder made
Nudox score worse and cost 3–27× more; a query that *is* a symbol's literal name
returned 50 semantically-ranked hits with the exact match absent. Why, and what
should change.

---

## 0. Hard constraints — restate these verbatim in every agent prompt

The machine has **16 GB RAM and no swap**. A running Nudox endpoint with the
embedder resident costs ~5.6 GB for two packages (EVIDENCE §D). Therefore:

* **Exploration agents read code. They do not build and do not run anything.**
* Forbidden, without exception: `cargo build`, `cargo check`, `cargo test`,
  `cargo run`, `cargo clippy`, `nix build`, `nix develop`, starting an endpoint,
  running `nudox-bench`, invoking any MCP server, downloading a model.
* Allowed: `ls`, `cat`, `sed -n`, `head`, `rg`/`grep`, `wc`, `git log`, `git show`.
* No agent writes to `/home/leaf/Dev/nudox/Backend`. Backend is **read-only** for
  this entire workflow, including the synthesis phase. The proposal describes
  changes; it does not make them.
* Scratch files go in `$CLAUDE_JOB_DIR/tmp`. Deliverables go in `analysis/`.

Backend is a large Rust workspace. **No agent may attempt to read it
exhaustively.** Each exploration agent gets a start-file list, a two-hop
follow rule (§1.2), and a read budget of roughly 2,500 lines. An agent that
runs out of budget reports what it did not reach — it does not guess.

**Depth over breadth.** The search/ranking question (E1, E2, E3) is the point of
this exercise. E4 and E5 are support. If effort has to be cut, cut E5 first,
then E4 — never E1 or E2.

---

## 1. Phase 1 — exploration

Five agents, launched in parallel, each with exactly one question. All five are
read-only. Every path below was confirmed to exist on disk while writing this
plan.

### Shared path map (Backend)

```
/home/leaf/Dev/nudox/Backend/workspace/nudox-engine/src/
  search/mod.rs            1329 ln  — section fan-out, SECTION_NAME/TYPE/SEMANTIC
  search/hits.rs            627 ln  — HitRow, score = text × visibility × kind
  semantic.rs               792 ln  — Embedder trait (:156), SemanticIndex (:372)
  embed/mod.rs              151 ln  — engine-side embedder wiring
  embed/onnx.rs             274 ln  — ort runtime, MAX_BATCH
  runtime/semantic.rs               — background embed task
  store/index/name.rs               — NameIndex (:61)
  store/index/alias.rs
  mcp/tools/mod.rs         2286 ln  — tool dispatch; SearchEvent consumer
  mcp/tools/args.rs         412 ln  — every tool's argument struct
  mcp/tools/results.rs      898 ln
  mcp/result_format.rs     1508 ln  — the "## Search · N" renderer
  mcp/semantic_format.rs    703 ln
  mcp/occurrence_format.rs
  mcp/schema_card.rs        141 ln  — graph schema (already mined, EVIDENCE §B)
/home/leaf/Dev/nudox/Backend/workspace/registry/vector/{core,embed,local,remote}/
/home/leaf/Dev/nudox/Backend/workspace/index/search/{ranking,pipeline.rs,tantivy.rs}
```

**`workspace/registry/` and `workspace/index/` are out of scope by default for
every agent.** They are the registry/service-side stack; the engine's own
embedder lives in `nudox-engine/src/embed/`. Enter them only if an assigned
start file leads there by a direct `use`, and only one file deep, and say so.

### 1.1 The five agents

---

#### E1 — Replace or outrank?

**Question (the only one):** On the code path that produces the MCP `search`
response, does the semantic section *replace* the name section's rows, *merge
with and outrank* them, or *displace* them through a shared limit? Resolve
EVIDENCE §F bullet 1.

**Start from:**
* `nudox-engine/src/mcp/tools/mod.rs` — the `SearchEvent` consumer. Confirmed
  anchors: `:313` (`section != SECTION_SEMANTIC`), `:420`, `:437`, `:1043`.
* `nudox-engine/src/search/mod.rs:200-420` — the documented event order
  (`:221-237`) and the semantic arm (`:384-398`).
* `nudox-engine/src/mcp/result_format.rs` — where sections become one list.
* `nudox-engine/src/mcp/semantic_format.rs`.

**Must produce:** the exact expression that decides which rows reach the
rendered response when both sections are non-empty, quoted verbatim, and a
statement of what happens to a `SECTION_NAME` row that is not in the semantic
top-k.

**Must NOT wander into:** `embed/`, `registry/`, `index/`, producers,
`packages/`, the `graph` tool, the account/gate subtree, or the bench repo.

---

#### E2 — Where does the page get filled?

**Question (the only one):** What sizes the semantic result set — is there a
similarity floor, a top-k, a per-section cap, or a dedup against the name
section — and where is each configured? Resolve EVIDENCE §F bullets 2 and (as
far as source allows) the A1 interpretation that "cosine similarity is defined
for every symbol, so the page always fills".

**Start from:**
* `nudox-engine/src/semantic.rs:300-560` — `SemanticIndex`, its nearest-neighbour
  routine, and anything named `k`, `top`, `min`, `score`, `cutoff`.
* `nudox-engine/src/search/mod.rs:360-410` — how the semantic arm is called and
  with what limit.
* `nudox-engine/src/mcp/tools/args.rs` — the `search` args struct and its
  `limit: Option<u32>` (confirmed at `:55`, `:106`, `:126`, `:149`, `:198`,
  `:219`, `:315`; identify which one is `search`).
* `nudox-engine/src/mcp/tools/mod.rs` — where that limit is defaulted and applied.

**Must produce:** a verdict `FLOOR EXISTS AT <file:line>` / `NO FLOOR` with the
quote that establishes it, and the default and cap for the `search` limit as
literals from source.

**Must NOT wander into:** the ONNX runtime internals, model weights,
`registry/vector/remote/`, rendering, the graph tool, the bench repo.

---

#### E3 — What can a caller ask for?

**Question (the only one):** What does the name/type path do with a query that
is a literal symbol name, and what — if anything — can an MCP caller do through
`search`'s arguments to bias, threshold, weight, or select sections? Resolve
EVIDENCE §F bullet 4.

**Start from:**
* `nudox-engine/src/store/index/name.rs` (`NameIndex` at `:61`) and
  `store/index/alias.rs` — exact/prefix semantics, case handling, tokenisation.
* `nudox-engine/src/search/hits.rs` — `text_relevance`, `visibility_weight`,
  `kind_weight`, the total comparator.
* `nudox-engine/src/mcp/tools/args.rs` — the full `search` argument surface.
* The `search` tool's description/schema string in `nudox-engine/src/mcp/tools/mod.rs`
  (this is also the text that costs resident tokens — EVIDENCE §D).

**Must produce:** the complete list of `search` MCP arguments with types and
defaults, each cited; and a yes/no on each of: relevance threshold, hybrid
weighting, section selection, section size.

**Must NOT wander into:** the semantic ranking path (E1/E2 own it), `embed/`,
`graph`, producers, the bench repo.

---

#### E4 — Where do 9,065 tokens come from?

**Question (the only one):** What does one rendered semantic hit row cost, and
which parts of the `search` response are structurally optional? EVIDENCE §A1
measures 351 tok for `## Search · 1` and 9,065 tok for `## Search · 50` — account
for the shape of that, from the renderer.

**Start from:**
* `nudox-engine/src/mcp/result_format.rs` — the `## Search · N` header and row
  rendering.
* `nudox-engine/src/mcp/semantic_format.rs`.
* `nudox-engine/src/mcp/tools/results.rs`.
* `nudox-engine/src/mcp/occurrence_format.rs` (only if rows share its helpers).

**Must produce:** the per-row field list as rendered, with the code that emits
each field, and any existing verbosity/compactness switch. Arithmetic against
the measured 9,065 is welcome but must be marked `INFERRED`.

**Must NOT wander into:** ranking, `semantic.rs`, `embed/`, `graph`, producers,
the bench repo.

---

#### E5 — What does the harness itself decide? *(lowest priority; cut first)*

**Question (the only one):** In the benchmark repo, what does the Nudox adapter
send to `search`, and what does the scorer count, such that a fuller page costs
more and scores worse? Identify harness-only changes that would move the
measured numbers without any Backend change.

**Start from** (bench worktree, `/home/leaf/Dev/nudox/nudox-bench/.claude/worktrees/bench-improvement-proposal/`):
* `src/adapter/nudox.rs` (1301 ln) — the calls, their limits, `pick_hit`.
* `src/judge.rs` (272 ln), `src/truth.rs` (377 ln) — pass/strict scoring, the
  union rule of EVIDENCE §E2.
* `src/tokens.rs` (170 ln) — how tokens are counted.
* `configs/*.toml`, `scenarios/`.

**Must NOT wander into:** the Backend repo at all. E5 exists precisely to keep
harness changes separable from Nudox changes (EVIDENCE §E is the precedent for
why that separation matters).

---

### 1.2 Rules binding on all exploration agents

1. **One question.** If you find something fascinating outside your question,
   write one line under `## Out of scope, noticed` and move on. Do not chase it.
2. **Two-hop rule.** You may follow a symbol out of your start files at most two
   files deep. Every file you open is listed in your header block, with line
   ranges. A third hop means the question is bigger than the agent — say so and
   stop.
3. **Read budget ≈ 2,500 lines.** Prefer `rg -n` and `sed -n 'A,Bp'` over reading
   whole files.
4. **Challenge EVIDENCE.md.** If source contradicts it, that is a first-class
   finding — file it as `CONTRADICTS-EVIDENCE` with the citation. Do not
   silently defer to it, and do not silently discard it.
5. **Never assert what §F says is unknown** unless you resolve it *in source*
   with a citation. "Probably", "presumably", "it seems" are not statuses.
6. No builds, no endpoints, no writes to Backend (§0).

---

## 2. The findings contract

Every exploration agent writes exactly one file, `analysis/findings/E<n>.md`, in
exactly this format. Gate G1 parses it mechanically; a file that does not parse
fails the gate as a whole.

````markdown
# E<n> — <the one question>

## Header
- agent: E<n>
- question: <verbatim from the prompt>
- files opened: <abs path>:<line-range>, ... (every one, including hops)
- files deliberately not opened: <abs path> — <why it was out of scope>
- read budget used: <approx lines>
- verdict on my question: RESOLVED | PARTIAL | UNRESOLVED

## Claims

### C<n>.1 — <the claim, one declarative sentence>
- status: READ | INFERRED | CONTRADICTS-EVIDENCE | UNRESOLVED
- evidence: /abs/path/file.rs:120-128
- quote:
  ```rust
  <verbatim, copied from the file, ≤ 12 lines, unmodified>
  ```
- bears on: A1 | A2 | A3 | B | C | D | F1 | F2 | F3 | F4 | (none)
- reasoning: <REQUIRED for INFERRED: the step from quote to claim>
- to promote to READ: <REQUIRED for INFERRED: which file/function would settle it>
- would falsify: <what a reader could find that makes this claim wrong>

### C<n>.2 — ...

## Unanswered
- <what my question still does not have an answer to, and where the answer lives>

## Out of scope, noticed
- <one line each, max five>
````

**Status definitions, enforced at G1/G2:**

* `READ` — the quoted lines, by themselves, establish the claim. No inference
  step. If a reader must reason from the quote, it is not READ.
* `INFERRED` — the quote supports the claim but does not establish it. Requires
  both `reasoning:` and `to promote to READ:`.
* `CONTRADICTS-EVIDENCE` — the quote is inconsistent with a named EVIDENCE.md
  claim. Must name the EVIDENCE section.
* `UNRESOLVED` — question asked, answer not found, with where it would be.

A claim with no `evidence:` line is not a claim. It is deleted at G1.

---

## 3. Phase 2 — gates

### G0 — Prompt scope gate *(lead, before launching Phase 1)*

For each agent prompt, check: exactly one question mark in the question field;
every start path exists on disk (`ls` it); a non-empty "must NOT wander into"
list; the §0 constraint block copied verbatim.
**Fails:** a prompt with two questions, or a start path that does not exist.
**Action on failure:** rewrite the prompt. Do not launch.

### G1 — Citation gate *(one dedicated verifier agent, mechanical, read-only)*

For every claim in every findings file:

1. The `evidence:` path exists and the line range is within the file.
2. The `quote:` block appears verbatim in that file within ±5 lines of the cited
   range. Whitespace-insensitive comparison is allowed; changed identifiers,
   elided lines without `...`, or paraphrase are not.
3. `status: INFERRED` has both `reasoning:` and `to promote to READ:`.
4. `status: READ` contains no hedging verb ("suggests", "appears", "likely") in
   the claim sentence — a hedged claim is re-labelled `INFERRED` by the verifier.

Suggested mechanical check per claim, run by the verifier:

```
sed -n '<start>,<end>p' <file>   # then compare to the quote block
```

**Output:** `analysis/gates/CITATIONS.md` — one row per claim id:
`PASS` / `FAIL <reason>` / `DEMOTED READ→INFERRED`.
**Fails a claim:** any of 1–3.
**Action:** failed claims go back to their originating agent for **one** repair
round (SendMessage to the same agent, which keeps its context). Claims still
failing after that round are struck and do not enter synthesis. The struck list
stays in `CITATIONS.md` so synthesis can see what was attempted and lost.

### G2 — Section F gate *(same verifier)*

EVIDENCE §F is the list of things not established. For every claim whose
`bears on:` names an F bullet, or whose text asserts one of §F's unknowns:

* If `status: READ` and G1 passed — the F bullet is resolved. Record it.
* If `status: INFERRED` — it is **not** a resolution. It is re-labelled
  `HYPOTHESIS` and may enter synthesis only with a stated test attached.
* Any claim asserting something about scale, statistical significance, or an
  ecosystem outside Rust/TypeScript/Go/Python (§F bullets 5 and 6) is **struck
  outright**. Ten scenarios and two packages cannot support it, and no amount of
  source reading changes that.

**Output:** appended to `CITATIONS.md` as an `F-LEDGER` table:
one row per §F bullet — `RESOLVED (claim id)` / `HYPOTHESIS (claim id, test)` /
`STILL UNKNOWN`.

### G3 — Contradiction gate *(lead)*

Every `CONTRADICTS-EVIDENCE` claim that survived G1 is escalated, not dropped.
The lead spawns **one fresh agent** per contradiction, given only: the cited
file:line, the quoted EVIDENCE.md sentence, and the question "which is right?"
— not the originating agent's reasoning, so the check is independent.

**Output:** `analysis/gates/CONTRADICTIONS.md` — for each: the EVIDENCE claim,
the source claim, the adjudicator's verdict with its own citation, and which
one synthesis must use. An unresolved contradiction is carried into the proposal
as an explicit open question, never silently resolved in EVIDENCE's favour.

### G4 — Coverage gate *(lead)*

Before synthesis starts, each of §F bullets 1, 2 and 4 must have a verdict in
the F-LEDGER: `RESOLVED`, `HYPOTHESIS` with a test, or `STILL UNKNOWN` with the
file that would settle it. Bullet 1 (*replace vs outrank*) is the one that
decides the fix; if it is `STILL UNKNOWN`, the lead launches **one** follow-up
agent with a single sharpened question and one round only. Bullets 3 and 5–6 may
remain unknown; they are out of the search question's scope.

**Fails:** F1 unknown with no follow-up run, or any exploration agent reporting
`verdict: UNRESOLVED` with no `Unanswered` section explaining where the answer
lives.

---

## 4. Phase 3 — synthesis

**One agent.** Not two: the deliverable is a *ranking*, and a ranking produced
by a committee is not a ranking.

**It receives, and nothing else:**
* `analysis/EVIDENCE.md`
* every `analysis/findings/E<n>.md` that passed G1
* `analysis/gates/CITATIONS.md` (including the struck list and the F-LEDGER)
* `analysis/gates/CONTRADICTIONS.md`

It may open Backend files **to verify a citation it is about to repeat**. It may
not introduce a new claim that is not in a findings file; if it needs one, it
says so and the lead runs a targeted follow-up agent. Same §0 constraints: no
builds, no endpoints, no writes to Backend.

**It writes `analysis/PROPOSAL.md`, containing:**

1. **Mechanism, one page.** What actually happens when the embedder is on, built
   only from `READ` claims, each sentence carrying its claim id. If the
   mechanism cannot be told without a `HYPOTHESIS`, the hypothesis is named as
   one in the text.

2. **Recommendations table.** One row per change:

   | id | target | change | rests on | evidence | expected effect | risk | cheap test |
   |---|---|---|---|---|---|---|---|

   * `target` — **`BACKEND`** or **`HARNESS`**, never both. A change needing
     both is split into two rows that name each other.
   * `rests on` — claim ids. A row with no claim ids does not go in the table.
   * `evidence` — `READ` / `INFERRED` / `HYPOTHESIS`, inherited as the *weakest*
     of the claims it rests on.
   * `expected effect` — must name the row and column of the EVIDENCE §A or §A1
     table it is expected to move, with a direction and a rough magnitude
     (e.g. "`nudox-full` tok/scenario 12,955 → order 4–5k; restores the
     ripgrep comparison"). "Improves relevance" is not an expected effect.
   * `cheap test` — something that fits 16 GB, two packages, one A/B.

3. **Ranking.** Ordered by *evidence strength × expected effect on the measured
   numbers*. Show both factors per row so the ordering can be audited and
   disagreed with. State the scale used (e.g. READ=3, INFERRED=2, HYPOTHESIS=1;
   effect scored against the §A table). A high-effect hypothesis ranks below a
   moderate-effect READ — say so explicitly, because it is the whole point of
   the gates.

4. **Backend section and Harness section, separately.** EVIDENCE §E lists five
   harness bugs that had shaped earlier numbers; the proposal must not repeat
   that confusion in the other direction. A harness change is never presented as
   a Nudox improvement, and a Nudox change is never justified by a harness bug.

5. **Open questions.** Every §F bullet still unresolved, each with the cheapest
   experiment that would settle it under the 16 GB / no-swap / two-package
   constraint, and what it would cost in wall-clock (EVIDENCE §D: ~40 min to
   embed two packages).

6. **Considered and rejected.** Ideas the findings actively rule out, with the
   claim id that rules them out. This is what stops the next session from
   re-proposing them.

**Every number in the proposal** is either a citation to an EVIDENCE.md section,
or carries the literal marker `HYPOTHESIS` with a stated test. There is no third
category.

---

## 5. Phase 4 — validation

**One agent, and it must not be the synthesis agent** — a fresh context, given
`PROPOSAL.md`, `EVIDENCE.md`, the findings files, and `CITATIONS.md`. Read-only,
§0 constraints apply.

It checks, and writes `analysis/gates/VALIDATION.md` with a per-check verdict:

* **V1 — code references resolve.** Every `path:line` in the proposal exists,
  and the code at it still says what the proposal says it says. Re-check by
  opening the file, not by trusting the findings file. `FAIL` lists each bad
  reference.
* **V2 — numbers trace.** Every performance number traces to a named EVIDENCE.md
  section, or is marked `HYPOTHESIS` with a test. An untraced number is a `FAIL`,
  not a rounding note.
* **V3 — no laundering.** No recommendation rests solely on an `INFERRED` claim
  while presenting itself as established; no `HYPOTHESIS` appears in the
  mechanism section without being labelled.
* **V4 — labels correct.** Every `BACKEND` row names a file under
  `/home/leaf/Dev/nudox/Backend/`; every `HARNESS` row names a file under the
  bench worktree. A row that names files in both repos is a `FAIL` (it should
  have been split).
* **V5 — §F respected.** Nothing in the proposal asserts as fact anything §F
  lists as unknown, unless the F-LEDGER marks it `RESOLVED` and V1 confirms the
  citation.
* **V6 — the ranking is reproducible.** Recompute the ordering from the two
  stated factors. If the printed order does not follow the stated rule, `FAIL`.

**Ship condition:** V1–V6 all `PASS`. On any `FAIL`, the lead sends the specific
failures back to the synthesis agent (SendMessage, context intact) for one
repair round, then re-runs validation. Two failed validation rounds means the
evidence base is too thin for the claim being made — the correct response is to
weaken the claim, not to add a third round.

---

## 6. Sequencing summary

```
G0  lead checks prompts
 |
 +-- E1  replace-or-outrank      \
 +-- E2  page fill / floor        |  parallel, read-only,
 +-- E3  caller-visible knobs     |  no builds, no endpoints
 +-- E4  render cost             /
 +-- E5  harness (bench repo only, cut first if effort is short)
 |
G1  verifier: every quote resolves at its file:line   -> CITATIONS.md
G2  verifier: §F discipline, hypotheses labelled      -> F-LEDGER
G3  lead: contradictions independently adjudicated    -> CONTRADICTIONS.md
G4  lead: F1/F2/F4 have verdicts; F1 gets one follow-up if unknown
 |
S   synthesis (one agent)                             -> PROPOSAL.md
 |
V   validation (different agent, V1-V6)               -> VALIDATION.md
 |
    lead: one repair round if needed, then done
```

## 7. Prompt template for exploration agents

Copy this, fill the four bracketed fields from §1.1, and paste §0 verbatim.

> You are exploration agent **[E<n>]** in a gated multi-agent workflow. You have
> **one question** and you will be judged on evidence quality, not coverage.
>
> **Read first, in full:** `analysis/EVIDENCE.md` (path above). It is a measured
> evidence base. Do not re-derive it. Do challenge it: if source contradicts it,
> file that as a `CONTRADICTS-EVIDENCE` claim with a citation.
>
> **Your question:** [one question]
>
> **Start from:** [files with line anchors]
>
> **Do NOT wander into:** [exclusions]. If you notice something interesting
> there, write one line under `## Out of scope, noticed` and move on.
>
> **Constraints:** [paste §0 verbatim — 16 GB no swap, read code only, no
> builds, no endpoints, no writes to Backend, two-hop rule, ~2,500 line budget.]
>
> **Output:** write `analysis/findings/E<n>.md` in exactly the format in §2 of
> `analysis/WORKFLOW.md`. Every claim needs a `file:line` and a verbatim quote;
> a claim without one is deleted by the citation gate. Mark `READ` only when the
> quote alone establishes the claim. Return a short summary as your final
> message — the parent reads your text, not your file.
