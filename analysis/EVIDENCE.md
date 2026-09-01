# Evidence base — measured, 2026-08-30/31

Input artifact for the improvement proposal. **Every claim here was measured or
read out of source in this session, and each carries its provenance.** Anything
not in this file has not been established and must not be assumed.

Agents consuming this: do not re-derive these. Do challenge them — if a claim
contradicts what you read in the source, say so and cite the file and line.

---

## A. The headline problem

Turning the semantic embedder on made Nudox **score worse and cost 3–27× more**
on an otherwise identical A/B (same two packages, same code, same scenarios;
`results/2026-08-31-graph-fixed` vs `results/2026-08-31-embedder-ab`).

| adapter | pass | strict | tok/scenario |
|---|---|---|---|
| nudox-full | 5/5 → **3/5** | 1.00 → 0.83 | 4,464 → **12,955** |
| nudox-graph-first | 5/5 → 4/5 | 1.00 → **1.00** | 4,692 → 11,529 |
| nudox-search-only | 2/7 → **3/7** | 0.39 → **0.57** | 292 → 6,544 |
| nudox-oneshot | 2/5 → 1/5 | 0.43 → 0.43 | 151 → 4,114 |
| ripgrep (unchanged) | 6/7 | 0.95 | 8,447 |

At 12,955 tokens `nudox-full` is now **more expensive than ripgrep** (8,447),
reversing the benchmark's most confident prior claim.

---

## A1. Finding: semantic search returns a full page regardless of relevance

**Measured.** Identical query `search_slice_with_raw`, same package, same
build:

| | structural | semantic |
|---|---|---|
| response header | `## Search · 1` | `## Search · 50` |
| response cost | 351 tok (mean over 5) | 9,065 tok (mean over 5) |

Per-scenario hit counts, embedder off → on:
`1→50`, `1→50`, `23→200`, `5→50`, `3→200`. The `200`s are overview scenarios
where the adapter passes `limit: 200`; the `50`s are the tool's default limit.

**Token decomposition, `nudox-full`, 5 scored scenarios** (response tokens):

| step | embedder OFF | embedder ON |
|---|---:|---:|
| search | 1,758 (351/call) | **45,328 (9,065/call)** |
| graph | 18,370 | 18,370 |
| refs | 1,411 | 69 |
| read | 275 | 244 |
| packages | 83 | 83 |

**100% of the cost increase is the `search` response.** Every other tool call
is unchanged. `refs` *fell* because it followed a wrong symbol (see A2).

Interpretation (not yet verified in source): structural ranking has a natural
cutoff — a name either matches by exact/prefix or it does not — whereas cosine
similarity is defined for every symbol, so the page always fills.

## A2. Finding: the embedder displaced the exact-name match entirely

**Measured.** Query = `search_slice_with_raw`, a literal symbol name.

* Structural: `## Search · 1` — the exact function,
  `cargo:memchr::arch::generic::memchr::search_slice_with_raw[function]`.
* Semantic: `## Search · 50`, and the string `search_slice_with_raw` occurs
  **zero times in the whole response**. Top five hits are all `find_raw`
  variants; 22 of the distinct names returned are `impl` blocks
  (`'impl Debug for Finder'`, `'impl Copy for Prefilter'`, …).

Downstream: the adapter followed a wrong identity and `refs` returned
`(none)`, so `memchr/callers-search-slice-with-raw` went from **pass (1,602
tok)** to **fail 17% (9,882 tok)**.

This is the single most important open question for the proposal: **why does
enabling the embedder remove an exact name match from the result set?**

## A3. Finding: `search` ranks a name section by exact-and-prefix match

**Read in source.** `nudox-engine/src/search/mod.rs:25`:

> `| SECTION_NAME (0) | Exact and prefix name matches from NameIndex |`

Consequences, both measured:
* `memchr` as a query reaches `memchr2`, `memchr_iter`, never `memrchr` —
  which is exactly the 60% ceiling `rust/memchr/public-surface` hit for every
  Nudox policy in every run before the harness stopped using search to
  enumerate.
* An empty query yields nothing: `search/mod.rs:739` asserts *"empty query
  must produce no name hits"*.

## A4. Finding: enumeration is a graph question, and works

`Packages { name @filter … members { isPublic @filter … } }` enumerates a
package's public surface. With it, `memchr/public-surface` and
`ts/commander/public-surface` pass — the former for the first time in the
repo's history.

---

## B. The graph tool had never executed

**Measured.** Every `graph` call the benchmark ever made failed, in every run,
with `Field Package received an invalid value for argument name: $package`.
Three independent causes, all in the harness:

1. Arguments were sent under `variables`; `GraphQueryArgs` names the field
   **`args`** (`nudox-engine/src/mcp/tools/args.rs:205-225`), a
   `BTreeMap<String, String>` — **all values are strings**, coerced by the
   engine. A JSON `true` is a type error.
2. The root was `Package(name: $package)`. The schema card's worked queries
   enter at **`Symbols`** or **`Packages`**, and field arguments do not accept
   variables — only `@filter(op:, value: ["$var"])` does.
3. The callers query walked `referencedBy`, which is not one of `Symbol`'s
   thirteen edges.

**Schema facts** (`nudox-engine/src/mcp/schema_card.rs`):
* Root: `Packages: [Package!]!`, `Symbols: [Symbol!]!`.
* `Package`: `lineage`, `name`, `ecosystem`, `members: [Symbol!]`.
* `Symbol` scalars (13): key, keyTier, keyIsContentDerived, name, signature,
  path, kind, isPublic, documentation, isDeprecated, visibility,
  deprecationNote, deprecationSince.
* `Symbol` edges (13): package, members, parent, usages, mentions,
  implementedBy, subtypes, returnedBy, acceptedBy, heldBy, signatureTypes,
  occurrencesOf, location.
* `visibility` has exactly six values; `isPublic` only separates Public from
  the other five.

**Two edge semantics established by experiment, not documentation:**
* `occurrencesOf` is **forward** — "references I contain". Querying it for
  callers returned exactly one row: the declaration itself.
* `usages` is the **reverse** edge and lands on `Symbol` directly (no `owner`
  hop; that belongs to `Occurrence`). It returned 24 real caller rows.
* Outputs must include `path`, or rows come back as a bare `name` beside an
  opaque `key` and nothing matches at strict tier.
* The engine **rejects an `args` map containing a key the query does not
  use**: *"One or more of the provided arguments are not used in this query"*.

`graph` also takes `limit` (default 50, cap 500) and `cursor`.

---

## C. Producers and capabilities

* **Python is gated.** The producer registers only under `nudox-engine`'s
  `pyrefly` feature. `endpoint/` built with `default-features = false` and no
  opt-in, so every pypi package failed with `toolchain missing for python`.
  The blocker text (`packages/acquire/mod.rs:1301-1322`) names it. Note the
  engine's own comment calls the feature "off by default there" while
  `nudox-languages` has `default = ["pyrefly"]` — the *registration* is what
  is gated, and the crates compile either way, which makes this easy to miss.
* **Go needs `go` on PATH at run time**, not just `NUDOX_GO_ORACLE_BIN`: the
  oracle drives `go/packages`, which shells out. Failure text:
  `oracle: loading packages under …: go command required, not found`.
* **`refs(in)` on `pypi:requests::'requests.api'.request[function]` returns
  `(none)`** — with the correct key, semantic on, package indexed. Nudox
  records no inbound references for that function. Whether this is a Python
  producer gap or a `~scope:default` effect (the response notes it excludes
  `Field`, `Variant`, `Param`) is **unresolved and worth investigating**.

---

## D. Cost and operational limits

* **Embedder memory**: loading the model is a fixed ~4 GB step; building the
  index adds more and **scales with symbols embedded**. Two packages settle
  ~5.6 GB. Five packages reached **8.25 GB after 36 minutes, still climbing**,
  and does not fit in 16 GB.
* **Embedding time**: ~40 minutes for memchr + commander at four cores pinned.
* `MAX_BATCH = 32` is a compile-time constant in
  `nudox-engine/src/embed/onnx.rs`, mirroring a private constant in
  `registry`. Not an environment knob.
* Model: fp32 `jinaai/jina-embeddings-v2-base-code`, 641,517,466 bytes, pinned
  in nix, sha256-verified before it reaches ort.
* **Resident surface**: 3,761 cl100k tokens for nine tools, of which `index`
  alone is 946 (25%) — and `index` is the tool a reading session never calls.

---

## E. Harness bugs found (fixed) — relevant because they shaped earlier numbers

Listed so the proposal does not attribute these to Nudox:

1. ripgrep searched test files; excluding them took it 5/7 → 6/7 and
   `go/uuid/callers-parse` from fail-at-100%-recall to pass at a fifth of the
   tokens.
2. Scoring reads the union of everything an adapter surfaced, so a `search`
   listing used to *find* an answer is scored as the answer. Provenance is now
   reported (`matched`, `distractors` as `total→answer`) but `passed` still
   follows the union.
3. `pick_hit` was case-insensitive: `Memchr3` (struct) beat `memchr3`
   (function) once semantic reordered them, so `read` returned
   `pub struct Memchr3<'h>` for a signature question. The scenario still
   passed — on text the search listing carried — with
   `primary_strict 0.00` against `strict 1.00`.
4. `pick_hit` used `max_by_key`, which returns the *last* maximum, so a total
   tie followed an arbitrary trailing hit.
5. The Nudox adapter sent dotted paths to a name-matching search
   (`requests.api.request` → `## Search · 0`).

**Scoring-rule counterfactual**, computed over the A/B: union versus
answer-only changes nothing for any Nudox policy (two scenarios flip in
opposite directions and cancel) but costs **ripgrep** two passes, 6/7 → 4/7.

---

## F. What is NOT established

* Whether semantic ranking *replaces* or merely *outranks* the name section —
  A2 is consistent with either, and the difference decides the fix.
* Whether there is a similarity floor, and if so where it is configured.
* Why `refs(in)` is empty for a Python symbol.
* Whether `search` exposes any relevance threshold, hybrid weighting, or
  section-size control through its MCP arguments.
* Anything about ecosystems other than Rust, TypeScript, Go, Python.
* Anything at scale: the corpus here is 2–5 packages and 10 scenarios. No
  claim in this document is statistically significant.
