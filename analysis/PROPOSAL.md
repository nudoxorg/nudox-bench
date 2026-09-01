# Improvement proposal — why enabling the embedder made Nudox worse, and what to do

Synthesised 2026-08-31 from `analysis/EVIDENCE.md` and
`analysis/findings/{E1,E2,E3,E4,LEAD-crux}.md`. Every citation below was
re-extracted verbatim from the cited file and line range before being repeated
here.

**Two corrections to the brief are recorded in §0.** One of them removes an
option the brief treated as available.

> **Superseded in part — read `analysis/MEASUREMENT.md` first.** R1 and R2 were
> implemented and measured on 2026-09-01
> ([`results/2026-09-01-exact-match-tier`](../results/2026-09-01-exact-match-tier)).
> **H3, H4, H7 and H8 are resolved and E4 is now measured**, so the §4
> hypotheses and the §2 token estimates should be read through that document,
> not from here. **R3, R5 and R8 are withdrawn** — R1 fixed the defect on the
> default path, so there is nothing left to route around. §1 and §3's reasoning
> stands; one defect it did not reach (prefix name matches displaced by semantic
> rows) is measured in `MEASUREMENT.md` §4.

---

## 0. Corrections to the established block

**0.1 — There is no structural-only entry point reachable from MCP.** The brief
says "a structural-only entry point already exists (`section != SECTION_SEMANTIC`,
`mcp/tools/mod.rs:313`)", and E1/E2 both record it as an escape hatch an adapter
could call. The guard exists; the *tool* does not. `mcp/server.rs` binds exactly
nine `#[tool]` methods:

```
mcp/server.rs:301  name = "search"          → do_unified_search
mcp/server.rs:324  name = "read"
mcp/server.rs:343  name = "refs"
mcp/server.rs:361  name = "packages"
mcp/server.rs:378  name = "select_version"
mcp/server.rs:396  name = "diff"
mcp/server.rs:415  name = "index"
mcp/server.rs:449  name = "graph"
mcp/server.rs:469  name = "schema"
```

`grep -rn 'do_search\b'` over the crate returns `mcp/tools/mod.rs:272` (the
definition), three doc-comment mentions, and **fourteen call sites, all under
`tests/`**. `mcp/tools/mod.rs:9-10` states the design intent directly:

> the consolidated nine of docs/MCP-SURFACE-PLAN.md §5.1 (`search`, `read`,
> `refs`, `packages`, `select_version`, `diff`, `index`, `graph`, `schema`)

So `do_search` is a Rust-internal test surface. E2's out-of-scope note — "an
adapter calling `search_symbols` gets the structural behaviour back with no
other change" — is **wrong**: no MCP client can call it. The cheapest imaginable
harness fix does not exist. *(Independently confirmed by the session lead after
this section was written.)*

Three consequences:

1. Any recommendation assuming a harness can "just call the structural tool" is
   struck. None below does.
2. **The genuinely harness-side levers are exactly the five `search` arguments**
   — `query`, `kinds`, `packages`, `limit`, `cursor` (`mcp/tools/args.rs:26-62`,
   READ). No threshold, no weighting, no section selection. R5 and R8 below are
   the only harness rows, and both live inside those five (R8 needs a sixth,
   which is why it is paired with a Backend row).
3. It **raises** a Backend option the brief could not have weighed: *exposing*
   the structural path. That is R7, and §2's excluded list carries the
   arithmetic for why it should be a flag on `search` rather than a tenth tool.

**0.2 — The visibility of `search_slice_with_raw` was inferred from a rendered
signature, not read from the recorded enum — now closed.** The brief called
`Visibility::Crate` "confirmed from Nudox's own rendered output". What the
structural run actually showed is the rendered Rust *text* `pub(crate) unsafe fn`;
the `Visibility` the Rust producer *records* is a separate value, and the whole
arithmetic hinges on which of 0.6 and 0.4 it is (§1.3's crossover table: at
`Crate` a semantic row needs cosine > 0.2; at `Private` it needs cosine > −0.2,
i.e. always).

The gap is now closed **statically, from source** — no endpoint experiment was
needed. `compiler/languages/src/rust/ra/ctx.rs:384-387` (LEAD-crux CL.8, READ):

```rust
pub(crate) fn visibility(&self, def: impl HasVisibility) -> IrVisibility {
    match def.visibility(self.db) {
        Visibility::Public => IrVisibility::Public,
        Visibility::PubCrate(_) => IrVisibility::Crate,
```

with the full correspondence documented immediately above it at `ctx.rs:376-383`:

| rust-analyzer HIR | recorded `IrVisibility` | `visibility_weight` |
|---|---|---:|
| `Public` | `Public` | 1.0 |
| `PubCrate(_)` | `Crate` | 0.6 |
| `Module(_, Implicit)` | `Private` | **0.4** |
| `Module(root, Explicit)` | `Crate` | 0.6 |
| `Module(other, Explicit)` | `Package` | 0.6 |

The payload rendered literal `pub(crate)`, which is `Visibility::PubCrate` ⇒
`IrVisibility::Crate` ⇒ weight **0.6** ⇒ score **0.60** ⇒ crossover
**cos > 0.2**, not the `cos > −0.2` the `Private` branch would have given. So
the arithmetic in §1.3 **stands as read, not assumed**, and R4's expected effect
is the `Crate` case rather than the degenerate one.

**The methodological point is worth stating, because it is easy to repeat.**
A rendered signature is *produced by* the IR; it is not the IR. `pub(crate)`
in a response payload is the renderer's rendering of a value, and the chain from
Rust source → rust-analyzer HIR → `IrVisibility` → `visibility_weight` has three
places it could have mapped differently. Reading the number off the payload
looks like a READ and is an inference. Anyone re-deriving §1.3 from a captured
response will reach for the same shortcut.

Two things the mapping table makes visible that the single case did not:

* **The observed failure is the milder one.** A bare `fn helper()` — no `pub` at
  all, the ordinary case for most of any crate's internals — is
  `Module(_, Implicit)` ⇒ `Private` ⇒ **0.4** ⇒ crossover `cos > −0.2`, which
  *every* semantic row with a non-negative cosine satisfies. `search_slice_with_raw`
  at `pub(crate)` needed fifty rows above cosine 0.2 to be displaced; a
  private-by-default symbol needs none. The defect is worse than the measurement
  shows.
* Four of the five HIR cases land on 0.6 or 0.4, so **only `pub` reaches 1.0**.
  The exact-match-is-1.0 wire contract (`search/mod.rs:53-58`) holds for public
  symbols and for no one else.

This firms up one leg of R1 and R4. Both remain INFERRED overall, because their
*sufficiency* still rests on CL.6's composition — that fifty public rows
actually clear cosine 0.2 for this query — which is a prediction about an
unmeasured distribution, not a reading (H1).

Everything else in the brief's established block checked out against source,
including the nuance about `hits.rs:261-264`: the exact match is the shortest
key in its own prefix range and so is the first row the BTreeMap range yields,
which means the pre-scoring truncation cannot drop it. That remains a latent
issue for longer prefix matches only (H6), and it is not presented below as a
cause.

---

## 1. Mechanism

### 1.1 What `search` does

The MCP tool named `search` is `do_unified_search` (`mcp/server.rs:301-321`).
It runs one engine search, which emits up to three independent sections — name,
kind-facet, semantic — and **appends every section's rows into one flat vector**
(`mcp/tools/mod.rs:430-441`, READ). Nothing replaces or clears anything: the
engine's own guarantee at `search/mod.rs:15-19` ("a slow section only adds to
what is already visible") holds exactly as written.

The displacement happens one layer up. After a dedup that keeps the *first*
occurrence of each key, the merged vector is re-sorted:

```rust
deduped.sort_by(|(ka, a), (kb, b)| b.score.total_cmp(&a.score).then_with(|| ka.cmp(kb)));
```
— `mcp/tools/mod.rs:466` (READ)

There is no section term in that comparator. The sorted vector is then cut to
`limit` by position alone (`paginate_rows`, `mcp/tools/mod.rs:1963-1974`, READ),
and `limit` defaults to 50 (`DEFAULT_LIMIT`, `mcp/tools/mod.rs:137`, READ).
Whatever falls past row 50 never reaches the response.

So: **the semantic section merges with and outranks the name section under a
shared post-merge limit.** It does not replace it. This closes the first open
item of EVIDENCE §F.

### 1.2 The two relevance scales are not the same quantity

Both sections score through one function:

```rust
pub(crate) fn score_of(relevance: f32, visibility: Visibility, kind: KindDiscriminant) -> f32 {
    relevance * visibility_weight(visibility) * kind_weight(kind)
}
```
— `search/hits.rs:184-186` (READ)

Only the `relevance` term differs, and the two differ *in kind*:

* **Name relevance is a text-match fraction.** `text_relevance`
  (`search/hits.rs:111-117`, READ) returns `1.0` for a case-folded exact match
  and otherwise `prefix.len() / key.len()` — "how much of the matched name the
  query covers". Its zero point is "no match at all", and a row only exists if
  the prefix matched.
* **Semantic relevance is a rescaled cosine.**
  `cosine_to_relevance(c) = f32::midpoint(c.clamp(-1,1), 1.0).max(EPSILON)`
  (`search/mod.rs:573-576`, READ) — that is `(c + 1) / 2`. Its zero point is
  cosine −1. An **orthogonal, wholly unrelated symbol scores 0.5**, not 0
  (INFERRED, one arithmetic step from the quoted body).

The comparator at `mcp/tools/mod.rs:466` compares these two numbers with one
`total_cmp`, as if they were commensurable. They are not. A semantic row's
relevance is bounded below by 0.5 for any non-negative cosine; a name row's
relevance is only 1.0 when it is a perfect match, and is then **discounted by
visibility**.

### 1.3 The arithmetic

`visibility_weight` (`search/hits.rs:137-148`, READ): Public 1.0, Protected
0.75, Internal/Package/Crate 0.6, Private 0.4. `kind_weight`
(`search/hits.rs:163-180`, READ): 1.0 for Function, Impl, Record, Trait,
Module, Alias, Enum, Const, Static, Reexport; 0.85 Field/Variant; 0.7 Param.

An **exact name match** therefore scores `1.0 × visibility_weight × 1.0`. A
**public semantic row** scores `((cos+1)/2) × 1.0 × 1.0`. Setting the second
above the first gives `cos > 2w − 1`:

| exact match's visibility | weight `w` | its score | a public semantic row outranks it when |
|---|---:|---:|---|
| Public | 1.00 | **1.00** | cos > 1.0 — impossible; it can only tie |
| Protected | 0.75 | **0.75** | cos > 0.50 |
| Internal / Package / **Crate** | 0.60 | **0.60** | **cos > 0.20** |
| Private | 0.40 | **0.40** | cos > −0.20 — i.e. essentially always |

In Rust, only `pub` reaches the 1.0 row: `pub(crate)` records as `Crate` and a
bare, unmarked item records as `Private` (`rust/ra/ctx.rs:376-387`, READ — full
table in §0.2). So the bottom two rows are the common case, and the *observed*
failure is the milder of them.

`search_slice_with_raw` renders as `pub(crate) unsafe fn`, and the Rust producer
maps `PubCrate` to `IrVisibility::Crate`
(`compiler/languages/src/rust/ra/ctx.rs:384-387`, READ), so `w = 0.6` and its
score is **0.60**. In a 2,129-symbol crate, fifty public symbols clearing cosine
0.2 against a query drawn from the same crate's vocabulary is the ordinary case,
not the edge case — and the measured output confirms it: the top five rows were
all public `find_raw` variants and 22 of the distinct names were `impl` blocks,
which carry `kind_weight` 1.0 like everything else.

The exact match was fetched, scored, merged, sorted to somewhere past row 50,
and cut. The string `search_slice_with_raw` occurs zero times in the response
not because it was filtered but because it was **outranked out of the page**.

### 1.4 Why the page always fills, and why that costs 9,065 tokens

There is **no similarity floor anywhere on this path** (E2's verdict, READ, at
three independent levels): `SemanticIndex::nearest` is a pure top-k
`truncate(limit)` whose only rejection is a vector-width mismatch
(`semantic.rs:495-515`); `collect_semantic_hits` over-fetches `limit*4` and
fills until it holds `limit` rows, rejecting only on package scope, missing IR
entry, and kind (`search/mod.rs:517-534`); and `cosine_to_relevance`'s
`.max(f32::EPSILON)` guarantees every row scores strictly positive, so nothing
can be dropped by scoring to zero.

Cosine is defined for every symbol. The name section has a natural cutoff — a
key either starts with the prefix or it does not — and the semantic section has
none. So the semantic section emits `limit + 1` rows whenever that many embedded
in-scope symbols exist, and the page fills to exactly `limit`. This is the
measured `1→50, 1→50, 23→200, 5→50, 3→200` pattern: the `200`s are the scenarios
where the adapter passes `limit: 200`.

Then each of those rows carries documentation. `attach_addresses` attaches the
symbol's **complete** `documentation` field, cloned whole, **only** to hits
semantic ranking contributed to:

```rust
let semantic = semantic_keys.contains(&hit_key_string(&hit));
let documentation = semantic
    .then(|| {
        pkg.as_deref()
            .and_then(|pkg| pkg.view().entry(hit.key.intro))
            .map(|entry| entry.sym().documentation.clone())
            .filter(|documentation| !documentation.is_empty())
    })
    .flatten();
```
— `mcp/tools/mod.rs:508-516` (READ)

and the renderer emits it with no cap:

```rust
let marker = if hit.semantic { " [semantic]" } else { "" };
let _ = writeln!(out, "{identity}\n  {declaration}{marker}");
if let Some(documentation) = &hit.documentation {
    let _ = writeln!(out, "  doc: {}", inline(documentation));
}
```
— `mcp/result_format.rs:87-91` (READ)

A 1,200-character clip for exactly this text **exists in this codebase** —
`result_format.rs:773-780`, `DocumentationEvidence::from_documentation(documentation, 1_200)`
— but it sits on the pre-merge `semantic_search` renderer, which no `#[tool]`
binds (§0.1). The tool-reachable path is the one without the clip.

E4's decomposition (INFERRED) puts ~135–155 of the ~181 tokens per row in that
`doc:` line. That is the 9,065-token search response.

### 1.5 Two problems, not one

The mechanism above contains **two independent defects that happen to share a
trigger**:

* a **ranking** defect — one comparator over two incommensurable scales, with
  a visibility discount applied to one side and not the other (§1.2, §1.3);
* a **cost** defect — an unbounded result set (no floor, page always fills)
  rendered with unbounded per-row documentation (§1.4).

They need different fixes and they move different columns. Fixing the ranking
does not save a token: the page still returns 50 rows. Fixing the cost does not
restore the exact match: capping documentation, or shrinking `limit`, or adding
a similarity floor at any value below the crossover cosine, all leave the
outranking untouched. **Conflating them is the main way this proposal could go
wrong**, and it is why R1/R4 and R2/R5 are separate rows below.

### 1.6 The embedder was not uniformly harmful, and that constrains the fix

Nothing above says semantic ranking is bad, and a proposal read that way would
be wrong. On the same A/B, `nudox-search-only` went **2/7 → 3/7** and strict
**0.39 → 0.57** — the one clear correctness *gain* in the whole experiment.

That is not noise-shaped. `nudox-search-only` is the policy with no `graph` and
no other route to a symbol whose name it does not already know, and it is the
policy that ran into the prefix-only recall ceiling hardest: `NameIndex::prefix`
is a `starts_with` range scan (`store/index/name.rs:123-131`, READ), which is
why the query `memchr` structurally cannot reach `memrchr`, and why
`rust/memchr/public-surface` sat at a 60% ceiling for every Nudox policy before
the harness moved enumeration to `graph`. Semantic similarity is the only
mechanism in the engine that crosses that gap, and for the policy that had
nothing else, it measurably did.

So the regression and the gain sit on opposite sides of one question — **does
the caller already know the exact name?** Where it does, §1.3 shows the ranking
is wrong and the exact match is displaced. Where it does not, semantic recall is
the only thing that works.

This constrains every fix below. R1 and R4 both demote semantic rows relative to
name rows, and neither knows which case it is in; they put the `nudox-search-only`
gain at risk (H4), and the right fix is one that reorders *only when there is an
exact name match to protect* — which is why R1 is a tier on the exact-match
predicate rather than a global rescale, and why R4, which is a global rescale,
ranks below it despite being the more elegant repair of §1.2's asymmetry.

---

## 2. Recommendations

Evidence status of a row is the **weakest** of its citations. Token estimates
marked *(est.)* descend from E4's INFERRED decomposition (~181 tok/row, of which
~135–155 is the `doc:` line, so ~36 tok/row of address + signature + marker) and
from EVIDENCE §A1's per-step split (`nudox-full`: 9,066 tok/scenario of search
plus 3,753 of everything else).

| # | target | change | evidence | expected effect | risk | cost |
|---|---|---|---|---|---|---|
| **R7** | **BACKEND** | Expose the structural path that already exists. Add a sixth field to `SearchSymbolsArgs` (`mcp/tools/args.rs:26-62`) — `mode: SearchMode` with variants `Hybrid` (**default, unchanged**) and `Structural` — and route `Structural` to the existing `do_search` (`mcp/tools/mod.rs:272`), which already carries the `section != SECTION_SEMANTIC` guard at `:313`, calls `attach_addresses(page, &HashSet::new())` so no hit is `semantic` and therefore **no hit gets documentation at all** (the `.then()` gate at `:508`), sets `semantic: None`, and breaks as soon as both structural sections arrive rather than waiting on the embedder (`:320-327`). Pairs with **R8**; inert without it. See §2's excluded list for why a flag beats binding a tenth tool. | `mcp/tools/mod.rs:272-384` READ (whole function re-read), `mcp/tools/mod.rs:313` READ, `mcp/tools/mod.rs:508` READ, `mcp/tools/args.rs:26-62` READ — **READ** | **Zero on its own** — a default-`Hybrid` flag nobody passes changes no cell. Enables R8; the pair's effect is stated there. | Adds a way for callers to opt out of the feature rather than fixing it, so the §1.3 defect keeps shipping on the default path. Widens the wire surface of the tool the whole design consolidated *down* to nine. | ~30 lines: one enum, one field, one branch, schema test. |
| **R8** | **HARNESS** ⚠ | Pass `mode: "structural"` on identity-shaped `search` calls — the ones where the scenario supplies a literal symbol name. Requires **R7**; impossible today (§0.1). Leave enumeration and open-ended scenarios on the default `hybrid`. | The pair's mechanism is READ (R7's citations); the magnitude is **measured** — `results/2026-08-31-graph-fixed` *is* the structural-response column, at `## Search · 1` and 351 tok/call | `nudox-full` **pass 3/5 → 5/5**, **strict 0.83 → 1.00**, **tok/scenario 12,955 → ~4,500** (bounded below by the measured embedder-off 4,464). `nudox-graph-first` **4/5 → 5/5**, **11,529 → ~4,800**. Search response on those calls **9,065 → 351 tok**, measured, not estimated. | ⚠ **Applied to *all* search calls this reproduces the embedder-off column exactly** — which is measured to cost `nudox-search-only` **3/7 → 2/7** and **strict 0.57 → 0.39**. That regression is the price, and it is a measurement, not a worry. Same benchmark-integrity flag as R3, harder: if the headline numbers come from `mode: structural`, the benchmark is reporting a path that ignores the embedder entirely — 40 minutes and 5.6 GB of index that no scored number depends on. Keep `nudox-search-only` on `hybrid` as the control. | One adapter argument. |
| **R1** | **BACKEND** | Give the merge comparator a rank tier so an exact name match cannot be outranked by a semantic row. Carry the `text_relevance == 1.0` fact out of `collect_name_hits` (`search/hits.rs:286-292`, where `score_of` is called) as a `tier: u8` on `HitRow` (`wire/mod.rs:839-854`), and change `mcp/tools/mod.rs:466` to `a_tier.cmp(&b_tier).then_with(|| b.score.total_cmp(&a.score)).then_with(|| ka.cmp(kb))`. Tier 0 rows keep their existing relative score order, so visibility still orders exact matches *among themselves*. | `hits.rs:184-186`, `hits.rs:111-117`, `hits.rs:137-148`, `mcp/tools/mod.rs:466`, `mcp/tools/mod.rs:430-441`, `rust/ra/ctx.rs:384-387` — all READ; sufficiency rests on CL.6/C1.12 (**INFERRED**) | `nudox-full` **pass 3/5 → 5/5**, **strict 0.83 → 1.00** (restores `memchr/callers-search-slice-with-raw`, the one scenario that regressed on identity). `nudox-graph-first` **pass 4/5 → 5/5**. **tok/scenario unchanged** at 12,955 / 11,529 — this row buys no tokens. | Reintroduces the exact case `visibility_weight` was written to suppress: its doc comment (`hits.rs:129-135`) names `memchr` explicitly — "memchr declares a private `mod memchr` once per architecture backend, all of them exact-matching a search for `memchr`". Those all become tier 0 and now sit above every semantic row. Keeping score order within the tier keeps the public one first but does not stop the private ones crowding rows 2..n. A common-word query (`new`, `len`) exact-matches hundreds of symbols and will now fill the page with them. | ~20 lines: one field, one setter, one comparator, plus tests. |
| **R2** | **BACKEND** | Cap `documentation` on the `search` path. Truncate in `attach_addresses` (`mcp/tools/mod.rs:508-516`) at the first blank line or **~200 characters**, whichever comes first. **Do not** reuse the existing `1_200` constant from `result_format.rs:773-780`: at ~4 chars/token that clip is ~300 tokens, well above the current ~135–155 token mean, so it would move nothing. | `mcp/tools/mod.rs:508-516` READ, `result_format.rs:87-91` READ, `result_format.rs:773-780` READ; magnitude rests on C4.12 (**INFERRED**) | search response **9,065 → ~4,300 tok/call** *(est.: 50 × (36 + ~50) + 30)*. `nudox-full` tok/scenario **12,955 → ~8,100**, back under ripgrep's 8,447. `nudox-graph-first` **11,529 → ~6,700**. `nudox-search-only` **6,544 → ~3,100**. **No pass/strict change.** | The `doc:` line is the only evidence a reader has for *why* a semantically-found hit was returned; one sentence may be too little to judge it, costing recall where the model picks a hit from doc text. EVIDENCE §E.2 — the harness scores the union of everything an adapter surfaced — means a shorter listing can lose a pass that was riding on search text; §E.3 documents exactly such a pass. Test H8 before shipping. | ~10 lines. |
| **R3** | **HARNESS** ⚠ | Stop using `search` for exact-name lookup. Where the scenario supplies a literal symbol name, issue a `graph` call instead: `{ Symbols { name @filter(op: "=", value: ["$name"]) key @output path @output signature @output } }` with `args: {"name": "search_slice_with_raw"}`. Pattern is a parse-tested worked query at `mcp/schema_card.rs:109-140`; `args` is a `BTreeMap<String,String>` and every value must be a string (EVIDENCE §B). Cross-references **R1**: this routes around the §1.3 defect, it does not fix it. | EVIDENCE §B (measured — `graph` executes and `usages` returns real rows), `mcp/schema_card.rs:104-140` READ; magnitude **INFERRED** | `nudox-full` **pass 3/5 → 5/5**, **strict 0.83 → 1.00**, **tok/scenario 12,955 → ~4,000** *(est.: a 1–2 row graph response replaces a 9,065-tok search call)*. `nudox-graph-first` **4/5 → 5/5**, **11,529 → ~4,700**. **Cannot move `nudox-search-only`** — that policy has no `graph` by definition. | ⚠ **This improves Nudox's numbers partly by weakening what the benchmark measures.** If every identity lookup leaves `search`, the harness stops exercising the surface whose regression it just found, and the §1.3 defect goes green while still shipping to every real MCP client. **Stated plainly: R3 must not be adopted *instead of* R1/R2 — only alongside them.** A green benchmark obtained by routing around a defect is a worse outcome than a red one, because it removes the signal that would have got the defect fixed. The condition for adopting it at all is that the harness's job is understood as *measuring the surface a client actually reaches*; under that reading R3 is legitimate only because a real agent holding an exact name plausibly would use a structural query, and it stops being legitimate the moment it is reached for to move a number. Keep `nudox-search-only` unrouted as the control. Secondary risk: `Symbols` name filtering is exact, so a scenario with a slightly wrong name now returns nothing instead of a near miss. | One adapter step + a scenario-level flag for which lookups are identity-shaped. |
| **R5** | **HARNESS** | Pass an explicit small `limit` (5) on identity-shaped `search` calls instead of relying on the default 50; keep 200 for the enumeration scenarios. Never pass `0` — `clamp_limit` maps `Some(0)` to `MAX_LIMIT = 500`, not to "unlimited" (`mcp/tools/mod.rs:2018-2026`). | `mcp/tools/mod.rs:137-144` READ, `mcp/tools/mod.rs:2018-2026` READ, `mcp/tools/mod.rs:396-406` READ — **READ**; magnitude est. | search response **9,065 → ~930 tok/call** on those scenarios *(est.: 5 × 181 + 30)*. `nudox-full` tok/scenario **12,955 → ~4,700**. **Explicitly no pass/strict change**: measured, the exact match is absent from the *entire* 50-row page, so a 5-row page contains it no more often. Pure cost. | Cuts recall wherever the answer sat in rows 6–50; under §E.2's union scoring the harness will feel that directly. Also shrinks the per-section fetch depth (`fetch_limit = offset + limit + 1`, `mcp/tools/mod.rs:400`), so the name section fetches 6 candidates instead of 51 — which activates the latent H5/H6 truncations at a much lower threshold. | One adapter constant. |
| **R4** | **BACKEND** | Put the two relevance scales on comparable units. Replace `cosine_to_relevance`'s midpoint map (`search/mod.rs:573-576`) with `cosine.max(0.0).max(f32::EPSILON)`, so relevance means "fraction of a match" on both sides. The `.max(EPSILON)` preserves the existing intent — the doc comment at `search/mod.rs:565-572` wants strictly-positive scores so `score_of`'s product cannot zero out — while dropping the affine lift that gives an orthogonal symbol 0.5. Alternative to R1, **not a companion**. | `search/mod.rs:573-576` READ, `hits.rs:184-186` READ, `hits.rs:137-148` READ, `rust/ra/ctx.rs:376-387` READ — **the crossover shift below is now arithmetic over four READ citations**, not an assumption; what stays **INFERRED** is only whether that shift suffices, which rests on an unmeasured cosine distribution (H1) | Raises the cosine a public semantic row needs to outrank an exact name match **from 0.20 to 0.60** at `Crate`/`Package` (0.6) — the measured case, now read rather than assumed (§0.2) — and **from −0.20 to 0.40** at `Private` (0.4), which is where it matters most, since today every non-negative cosine outranks a private-by-default exact match. *May* restore `nudox-full` **3/5 → 5/5** and **strict 0.83 → 1.00**, but only if fewer than 50 public rows clear cosine 0.60 — unmeasured (H1). tok/scenario unchanged. | A global reordering of every semantic result in every tool, not a targeted fix. It demotes semantic rows everywhere, which is exactly the effect that took `nudox-search-only` 2/7 → 3/7 and 0.39 → 0.57; that gain is at risk (H4). It also collapses all negative-cosine rows into one `EPSILON` tie, which the current doc comment argues against — an acceptable trade, but a deliberate reversal of a documented decision. Doing R1 **and** R4 double-corrects; measure them separately. | 3 lines plus rewriting the doc comment that argues for the current map. |
| **R6** | **BACKEND** | Shorten `~scope:default (excluded Field, Variant, Param; pass \`kinds\` to include them)` to a signal rather than a sentence, at `result_format.rs:60-71` / `excluded_kinds_signal`. | `result_format.rs:60-71` READ; ~22 tok figure is **INFERRED** (E4) | `nudox-search-only` tok/scenario **292 → ~270** with the embedder off (~7%). Negligible with it on. | Removes a hint a first-time caller uses to discover the `kinds` argument. | ~5 lines. |

Rows considered and **excluded**:

* **Binding `do_search` as a tenth MCP tool**, rather than R7's flag. Same
  capability, ~10× the standing cost. EVIDENCE §D measures the nine-tool
  resident surface at **3,761 cl100k tokens**, of which `index` alone is 946;
  the other eight average **~352 tokens each**. A structural-search tool would
  carry a description of comparable length to `search`'s ~180-word one
  (`mcp/server.rs:301-315`) plus a derived schema, so **~300–400 tokens, ~+9%
  resident — paid on every turn of every session whether or not it is ever
  called**. R7's alternative is one enum field with a one-line doc comment on a
  schema that already ships: ~20–40 tokens, ~+1%. Beyond the tokens, a second
  search tool re-imposes exactly the choice the merged surface was built to
  remove — `search`'s own description sells it as "combining exact/prefix
  name-and-kind matches with semantic ranking … **no need to choose an index up
  front**" — and undoes the MCP-SURFACE-PLAN §5.1 consolidation that produced
  the nine. **Recommend the flag, not the tool.**

Rows excluded for failing the "name a measurable effect" bar:

* **A `format`/terseness switch on `search`**, mirroring `ReadArgs.format`
  (`mcp/tools/args.rs:284-291`, READ — the precedent exists). Dominated by R2:
  an opt-in switch leaves the default — what the benchmark measures and what a
  real agent gets — unchanged, so its expected effect on the table is zero
  until a paired harness row turns it on, and R2 gets the same tokens with no
  harness coordination.
* **Trimming the resident tool surface** (3,761 cl100k tokens for nine tools,
  of which `index` alone is 946 — EVIDENCE §D; the concrete lead is `IndexArgs`'
  ~60-line doc comment at `mcp/tools/args.rs:333-347`, which compiles into the
  MCP schema). Real, and roughly a 23% cut to a per-session fixed cost — but the
  tok/scenario column is *response* tokens (EVIDENCE §A1), so this moves no cell
  of the table. Worth doing; not a row here.
* **A similarity floor.** See H1 — it belongs in the table only once its
  magnitude is measured, and §1.5 explains why it is a cost lever, not the
  ranking fix it looks like.

---

## 3. Ranking

Score = evidence strength × expected effect, both printed. Evidence strength:
**READ = 1.0**, **INFERRED = 0.6** (the row's weakest citation). Expected effect
on a 1–5 scale: 5 = restores ≥2 passes *and* cuts a token column by more than
half; 4 = reverses the headline ripgrep comparison; 3 = restores 2 passes, or
cuts one adapter's tokens by ~60%; 1 = single-digit percent on one cell.

| rank | # | target | evidence strength | expected effect | product | tie-break |
|---:|---|---|---:|---:|---:|---|
| 1 | **R7 + R8** | BACKEND + HARNESS ⚠ | 1.0 (READ mechanism, **measured** magnitude) | 5 | **5.0** | scored as a pair — R7 alone is 0. See the caveat below: this is the metric's answer, not mine |
| 2 | **R1** | BACKEND | 0.6 (INFERRED) | 5 | **3.0** | fixes the defect on the default path rather than offering an opt-out |
| 3 | **R3** | HARNESS ⚠ | 0.6 (INFERRED) | 5 | **3.0** | largest single-row movement, but ⚠ routes around the defect — adopt only with R1 or R4 |
| 4 | **R5** | HARNESS | 1.0 (READ) | 3 | **3.0** | strongest evidence of the three-way tie, but cost-only |
| 5 | **R2** | BACKEND | 0.6 (INFERRED) | 4 | **2.4** | |
| 6 | **R4** | BACKEND | 0.6 (INFERRED) | 3 | **1.8** | alternative to R1, not a companion |
| 7 | **R6** | BACKEND | 0.6 (INFERRED) | 1 | **0.6** | below the noise of a 5-scenario corpus (§5) |

**The metric ranks R7+R8 first, and the metric is wrong about it.** I am printing
both factors so the order is auditable, and this is the case the audit is for.
R7+R8 scores 5.0 because it recovers a *measured* baseline — its magnitude needs
no estimate, since `results/2026-08-31-graph-fixed` already is the answer. That
is exactly why the score is misleading: `evidence strength × expected effect`
rewards a change for being well-measured, and the best-measured change available
is always "stop using the new thing". It fixes nothing on the default path. Every
MCP client that does not pass `mode: structural` — which is every client that
has not read this document — still gets the §1.3 defect. Treat 5.0 as a
statement about the confidence of the number, not about the merit of the change.

Reading of the order: **do R1 and R2.** One ranking fix, one cost fix, both in
Backend, both on the default path, together taking `nudox-full` to 5/5 at
~8,100 tok/scenario — better than the embedder-off baseline on correctness and
roughly level with ripgrep on cost, while keeping the feature that took 40
minutes and 5.6 GB to build. **Do R7 as well**, because a caller who knows a
symbol's exact name should be able to say so and an opt-out is worth having on
its own merits — but ship it *after* R1, so the flag is an optimisation rather
than the fix. Take R5 (honest, cheap, cost-only). Take R3 and R8 only in the
qualified forms their risk columns describe, and never as a substitute for R1.
R4 is the more elegant Backend fix and the only one that addresses §1.2's
asymmetry at its root, but it is a global reordering resting on an unmeasured
distribution — hold it until H1 and H4 report.

Ranks 2–4 are a three-way tie at 3.0. That is not a coincidence to be smoothed
over: with a 5–7 scenario corpus (§5) the effect estimate is the weak factor in
every product, and the ordering within the tie is a judgement about *where a
defect should be fixed*, not a measurement.

---

## 4. Hypotheses

Nothing in EVIDENCE §F is stated as fact above. Each item below is open, with
the specific experiment that would settle it.

**H1 — What is the cosine distribution for these queries, and is there a floor
worth adding?** A floor is the only single change that could move *both*
columns: it shortens the page (cost) *and*, if set above the §1.3 crossover,
removes rows that were outranking the exact match (ranking). But its magnitude
is unknown, which is why it is not in the table. **It must be applied to the raw
cosine**, in the `for (lineage, intro, cosine) in nearest` loop at
`search/mod.rs:517-527` where the cosine is still in hand — *not* to `score`,
because `cosine_to_relevance` floors relevance at 0.5 for an orthogonal symbol,
so any `score` threshold below `0.5 × visibility × kind` is unreachable and any
threshold above it silently doubles as a visibility filter.
*Experiment:* instrument `collect_semantic_hits` to log every candidate's cosine
for the five benchmark queries; plot the distribution; report page length at
floors 0.2 / 0.3 / 0.4 / 0.5 and where the knee is. Read-only against a patched
build; no scoring run needed.

*(H2, "what `Visibility` does the Rust producer record", was an open hypothesis
in an earlier draft. It is resolved from source and has been **promoted to an
established fact in §0.2** — `ctx.rs:384-387` plus the mapping table at
`ctx.rs:376-383`. It is not a hypothesis and needs no experiment.)*

**H3 — Does R1 alone, or R4 alone, actually restore the scenario?** §1.3 shows
the exact match is *displaced*; neither fix is proven *sufficient* until the row
is observed back on the page.
*Experiment:* patched build, re-run `results/2026-08-31-embedder-ab` unchanged,
diff the `nudox-full` and `nudox-graph-first` rows. Run R1 and R4 as separate
arms — they double-correct if combined and the result would be uninterpretable.

**H4 — Does `nudox-search-only`'s gain (2/7 → 3/7, 0.39 → 0.57) survive a
ranking fix?** That gain is the one thing the embedder unambiguously bought, and
both R1 and R4 demote semantic rows relative to name rows.
*Experiment:* the same arms as H3, reading the `nudox-search-only` row.

**H5 — The name section's row budget is shared across packages, not per
package.** In `collect_name_hits`, `candidates` is declared before the `for pkg
in packages` loop while the `if candidates.len() >= limit { break; }` guard sits
inside the inner scan (`search/hits.rs:250-264`). With two in-scope packages and
a common prefix, the first package can consume the entire per-section budget and
the second contributes nothing. Not a cause of the observed failure (memchr's
prefix range for this query is tiny) and not established as a real-world effect.
*Experiment:* `search` a name present in both `memchr` and `commander` with
`limit: 2` and check whether both packages appear.

**H6 — The pre-scoring lexicographic truncation is latent, not active.**
`search/hits.rs:261-264` truncates the prefix scan at `limit` in BTreeMap key
order *before* scoring, so an alphabetically late strong match can be discarded
for an early weak one. The brief is right that this did **not** cause the
observed failure: the exact match is the shortest key in its own prefix range
and so is the first row the range yields. R5 makes it bite sooner by shrinking
`limit` to 6.
*Experiment:* query `f` in memchr with `limit: 5`; check whether any `fz…` name
can appear.

**H7 — Is doc text duplicated across rows?** Neither `attach_addresses` nor the
render loop dedupes documentation, and 22 of the 50 returned rows were `impl`
blocks, which frequently share one doc string. If duplication is heavy, deduping
is a cheaper cost fix than R2's clip and loses no information.
*Experiment:* capture one `## Search · 50` body, split on `  doc: `, count
distinct strings against total.

**H8 — Does R2's clip cost a pass?** EVIDENCE §E.2: the harness scores the union
of everything an adapter surfaced, and §E.3 records a scenario that passed on
text the search listing carried.
*Experiment:* no re-run needed — re-score the existing captured transcripts
offline with every `doc:` line truncated to 200 characters, and diff the pass
and strict columns.

**H10 — Can "identity-shaped" be decided without hindsight?** R3, R5 and R8 all
assume the harness can tell, *before* the call, that a scenario supplies a
literal symbol name and so should route structurally / narrowly. If that
classification is made by looking at which scenarios failed, the improvement is
overfitting to five data points, not a policy a real agent could follow. The
same objection applies to a real client: a model does not reliably know whether
the string it holds is an exact symbol name.
*Experiment:* write the classifier as a rule over the scenario's *stated query
text alone* (e.g. "the query is a single identifier matching `[a-z_][a-z0-9_]*`
and contains no space"), freeze it, and only then apply it. Report how many
scenarios it catches and whether it catches any it should not. Cheap, offline,
and it should be done before R3/R5/R8 are credited with anything.

**H9 — Why is `refs(in)` empty for `pypi:requests::'requests.api'.request`?**
Carried forward from EVIDENCE §C unresolved; a Python producer gap and a
`~scope:default` effect are both consistent with it. Untouched by everything
above.

---

## 5. What this proposal does not address

**Statistical weight — read this before quoting any number above.** The corpus
is **2 packages** and **5–7 scenarios per adapter**. One scenario flipping moves
the pass column by 14–20 percentage points. Nothing in this document is
statistically significant, no confidence interval can be computed from it, and
every "expected effect" is a *point prediction about a specific known scenario*,
not an estimate of a population effect. The token estimates carry a second layer
of uncertainty: they descend from E4's INFERRED per-row decomposition, which has
never been checked against a captured response body (E4's own "to promote to
READ" is exactly that measurement).

Also out of scope:

* **Whether the embedder should be on at all.** Every recommendation above
  assumes the feature stays and should be made to work. The measured A/B does
  not obviously support that assumption: turning it on made **pass worse on
  three of four adapters** (5/5→3/5, 5/5→4/5, 2/5→1/5), better on one
  (`nudox-search-only`, 2/7→3/7), and **tokens worse on all four** (3–27×).
  R8 shows the good column is one argument away. Whether ~4 GB resident,
  ~40 minutes of indexing per two packages, and a 16 GB ceiling at five
  packages (EVIDENCE §D) are worth the one adapter it helped is a product
  decision resting on what users do outside this benchmark, and this proposal
  does not make it. It only establishes that the current *cost* of the feature
  is mostly an artefact of §1.4 and largely removable.
* **The default path, if only R7/R8 are taken.** R7's flag defaults to
  `Hybrid`. A client that does not pass `mode: structural` — which is every
  client that has not read this document — still gets everything §1 describes.
  Only R1, R2, R4 and R6 change what an unmodified caller receives.
* **Everything but the ranking, merging and rendering of `search` results.** No
  recommendation changes what gets embedded, how it is chunked, or which model
  is used. The *quality* of `jinaai/jina-embeddings-v2-base-code` on this corpus
  is unevaluated; §1 explains why bad results appear even if the embeddings are
  good, and does not establish that they are.
* **The operational cost of the embedder.** EVIDENCE §D: a fixed ~4 GB model
  load plus an index that scales with symbols embedded, ~5.6 GB at two packages,
  **8.25 GB at five and still climbing**, and ~40 minutes of embedding for two
  packages on four pinned cores. Nothing here reduces any of that, and it
  remains the binding constraint on growing the corpus past two packages in
  16 GB — which is the same constraint that makes the previous bullet's
  significance problem hard to fix.
* **Ecosystems other than Rust and TypeScript.** Python is gated behind
  `nudox-engine`'s `pyrefly` feature and `endpoint/` is built without it
  (EVIDENCE §C), and Go needs `go` on PATH at run time. The A/B ran on `memchr`
  and `commander` only. Nothing above is evidence about any other language.
* **The prefix-only recall ceiling.** `NameIndex::prefix` is a `starts_with`
  range scan (`store/index/name.rs:123-131`), which is why `memchr` structurally
  cannot reach `memrchr` and why `rust/memchr/public-surface` hit a 60% ceiling
  for every Nudox policy before the harness switched to `graph` for enumeration.
  Semantic search was the plausible fix for that, and for `nudox-search-only` it
  measurably was one. No recommendation above restores that; R1 and R4 both push
  in the opposite direction.
* **Aliases and dotted paths.** `AliasIndex` has no reader in `search/`
  (INFERRED, E3 C3.14), so a symbol cannot be reached through its public
  re-export path via `search`. This is the index-level reason harness bug §E.5
  existed. Untouched.
* **The harness's union scoring rule.** EVIDENCE §E.2 — a `search` listing used
  to *find* an answer is still scored as the answer. Provenance is now reported
  but `passed` still follows the union. Every "expected effect" above is stated
  under that rule, and R2/R5 both interact with it (H8).
* **The `search` tool description itself.** `mcp/server.rs:301-315` tells the
  caller a hit is "marked `semantic: true`", which reads like an input flag and
  is not one (E3 C3.5) — no such argument exists on `SearchSymbolsArgs`. A model
  that writes it is passing a field that does not exist. Not measured, so not a
  row.
