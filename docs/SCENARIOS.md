# Writing scenarios

A scenario is a question about a real package plus enough machine-checkable
truth to score an answer.

## The rule that matters

**Derive the truth by reading the package. Never from memory.**

Every scenario in `scenarios/` opens with a comment naming the file and lines
its truth came from:

```toml
# Ground truth derived by reading memchr-2.8.3 directly, not from memory:
#   rg -n 'search_slice_with_raw' src/
# The six call sites in src/memchr.rs are at lines 31, 63, 96, 129, 167, 205,
# inside memchr, memrchr, memchr2, memrchr2, memchr3 and memrchr3.
```

Keep that habit. It is what makes a failing run a finding rather than an
argument about whether the scenario was right.

## Shape

```toml
id     = "rust/memchr/callers-search-slice-with-raw"   # stable; it is a report key
family = "retrieval"        # retrieval | implementation | efficiency | polyglot
task   = "callers"          # what shape of question this is
question = "..."            # as a developer would ask it

[target]
ecosystem = "crates.io"     # corpus tag; `crates.io` maps to `cargo` in an address
package   = "memchr"
version   = "2.8.3"         # required — a caller set is a fact about a version
root      = "/path/to/checkout"   # what the file-based adapters read

[probe]                     # the subject, in every spelling that is known
name   = "search_slice_with_raw"
symbol = "memchr::arch::generic::memchr::search_slice_with_raw"
query  = "search_slice_with_raw"

[truth]
mode   = "symbol_set"       # symbol_set | literal | judged
all_of = ["memchr::memchr", "..."]   # every one needed for full recall
any_of = []                          # one gate, not N facts
none_of = ["memchr::memrchr3"]       # a clean answer contains none of these

[budget]
max_tool_calls = 4
token_budget   = 4000
```

## What the loader rejects

`cargo run --bin nbench -- list` validates everything and refuses:

* a `symbol_set` or `literal` truth with **no** `all_of` and no `any_of` — it
  can never fail, so it measures nothing;
* a symbol-scoped task whose `probe` names no symbol;
* a `diff` task with no `target.to_version`;
* an `implementation` family whose truth is not `judged`;
* a `judged` truth with no oracle path.

## Choosing a good one

**A good scenario has an answer that is short, exact, and awkward for a text
search.** The `py/requests/callers-api-request` scenario is the cleanest in
the repo: seven one-line delegations to `request()`, no `cfg` gates, no
ambiguity — and `request` is a substring of `requests`, of `Request`, and of
every one of those helpers' own bodies.

**Put genuinely ambiguous facts in neither `all_of` nor `none_of`.** The
memchr callers scenario omits the architecture-specific backends, because each
is `cfg`-gated to one target and whether they belong in the answer depends on
what the index was built for. Scoring a tool on a defensible design choice
makes the number mean less, not more.

**Distractors should be real near-misses.** `ParseBytes` earns its place in
`go/uuid/callers-parse` because it is exported, contains `Parse`, and
genuinely does not call it — verified at `uuid.go:120`. A distractor nobody
would ever return teaches nothing.

**Say what a scenario is sensitive to in `notes`.** Those notes are the first
thing to read when a run disagrees with expectation.

## Families

| family | scored by | for |
|---|---|---|
| `retrieval` | ground truth | the core: who calls this, what is this, what changed |
| `polyglot` | ground truth | the same, outside Rust — where rust-analyzer cannot follow |
| `efficiency` | ground truth + a tight `token_budget` | short answers; the question is what they cost |
| `implementation` | 3-model judge | "using package X, build Y" — context-bench's shape |

## Corpus

`Backend/nix/corpus.nix` declares **220 packages across 7 ecosystems**
(crates.io 56, maven 62, go 21, nuget 21, pypi 20, npm 20, cpp 20), several
with multiple versions — which is what makes `diff` and `select_version`
scenarios possible. The corpus is Nix-owned and not materialized in a
checkout.

The scenarios here point `target.root` at packages already on this machine
(the cargo registry cache, `~/go/pkg/mod`, `node_modules`, site-packages) so
the benchmark runs without materializing the corpus. Use `--ensure-indexed` so
Nudox answers about those same roots.
