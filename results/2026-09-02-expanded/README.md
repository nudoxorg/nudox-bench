# The expanded corpus — nine comparable scenarios, and why two of them fail

The corpus had five scenarios every adapter could be scored on, which is too
few to say anything: one flip moved a pass column twenty points. Four scenarios
were added (`analysis/DYNAMIC-EMBEDDER.md`, and the commit that introduced
them), chosen to separate capabilities the old set conflated. Nine are now
comparable.

Both arms below are the same Backend — `exact-match-tier-and-doc-cap` at
`6c63b655`, R1 + R2 — and the same harness. The only variable is whether an
embedding model is configured.

## 1. Where Nudox stands, on nine scenarios

Embedder off, scored over the nine scenarios every adapter answered:

| adapter | pass | strict | tok/scenario | tok/fact | calls |
|---|---:|---:|---:|---:|---:|
| **nudox-full** | **7/9** | **0.89** | **6,076** | **882** | 2.6 |
| nudox-keys | 7/9 | 0.89 | 6,076 | 882 | 2.6 |
| nudox-graph-first | 7/9 | 0.89 | 6,208 | 901 | 2.6 |
| ripgrep | 6/9 | 0.85 | 9,551 | 1,653 | 1.0 |
| nudox-search-only | 3/9 | 0.42 | 378 | 148 | 1.0 |

Nudox leads the ripgrep floor on every axis at once — one more pass, a higher
strict recall, 1.6× cheaper per scenario and 1.9× cheaper per fact recovered.
That statement now rests on nine scenarios rather than five.

## 2. The two `nudox-full` failures have nothing in common

This is what the four new scenarios were for: the old corpus could not
distinguish these causes.

### 2a. It fails by answering too much — `reverse-search-family`

The question is "which functions search a haystack from the end backwards?",
and the scenario's `none_of` names the three *forward* iterators so that
dumping the export list does not count as answering.

| adapter | steps | distractors | verdict |
|---|---|---:|---|
| nudox-search-only | `search` 716 tok | 0 | **pass** |
| nudox-full | `packages` 42, `search` 716, **`graph` 12,004** | **3** | fail |
| ripgrep | one grep, 18,159 tok | 3 | fail |

`nudox-full`'s search step is *exactly right*: 716 tokens, strict recall 1.00,
zero distractors. It then issues
`graph {package: memchr, public: true, limit: 500}` — a twelve-thousand-token
enumeration of the entire public surface — and that dump contains
`memchr_iter`, `memchr2_iter` and `memchr3_iter`.

**`nudox-full` loses to `nudox-search-only` here purely because it does more.**
Under the union scoring rule everything an adapter surfaces is part of its
answer, so a broad enumeration issued *after* a precise answer is a liability,
not extra credit. A smaller `search_limit` would not have helped; the search
step was never the problem.

Worth stating carefully, because the obvious fix is a trap: making the `full`
policy stop when the search step looks confident would win this scenario back,
and would also be tuning the client to the scorer. That is the H10 overfitting
objection from `analysis/PROPOSAL.md`, and it applies with full force. The
defensible version of the claim is narrower: *an agent asked a precise question
should not follow a precise answer with an unfiltered surface dump*, and that is
true whether or not a benchmark is watching.

### 2b. It fails on a capability it does not have — `error-types`

"What error class does commander's `InvalidArgumentError` extend?" Nudox scores
**strict 0.00**; ripgrep passes.

Nudox finds the right declaration — it reports
`typings/index.d.ts:33:8`, which is exactly where
`export class InvalidArgumentError extends CommanderError` is written — and
renders it as:

```
npm:commander::index.InvalidArgumentError[record]
  pub struct InvalidArgumentError
```

The `extends CommanderError` is gone. Two things are visible in that one line:
the inheritance edge is not in the IR the TypeScript producer builds, so no
"what does X extend / implement" question is answerable for TypeScript at all;
and a TS class is being rendered in Rust vocabulary (`pub struct`), which is
misleading on a polyglot surface.

This one is not a ranking defect and no client-side change reaches it. It is a
producer gap, and it is the more valuable of the two findings.

## 3. What the new scenarios buy

| scenario | what it isolates | who passes (embedder off) |
|---|---|---|
| `rust/memchr/reverse-search-family` | conceptual retrieval a `starts_with` scan cannot do | search-only only |
| `rust/memchr/memmem-api` | submodule members whose names share no prefix with the module | nudox-full, ripgrep |
| `ts/commander/option-configuration` | class members in the weaker ecosystem | nudox-full only |
| `ts/commander/error-types` | an inheritance edge, TypeScript | ripgrep only |

Each is discriminating — no two adapters have the same profile across them —
which is what the previous five could not do.

## 4. With the semantic section on, warmed

Same corpus, same Backend, `ready×47` on every search step (the off arm is
`unavailable(NoModelConfigured)×47`) — so each arm is one system, checked
rather than assumed.

| adapter | embedder off | embedder on, warm |
|---|---|---|
| nudox-full | 7/9 · 0.89 · 6,076 | 7/9 · **1.00** · 11,297 |
| nudox-graph-first | 7/9 · 0.89 · 6,208 | 7/9 · **1.00** · 11,107 |
| nudox-keys | 7/9 · 0.89 · 6,076 | 7/9 · 1.00 · 11,297 |
| ripgrep | 6/9 · 0.85 · 9,551 | 6/9 · 0.85 · 9,551 |
| **nudox-search-only** | 3/9 · 0.42 · 378 | **4/9 · 0.80 · 5,598** |
| nudox-oneshot | 2/9 · 0.38 · 141 | 2/9 · 0.35 · 1,051 |

On the five-scenario corpus the embedder *cost* `nudox-full` a pass. On nine it
does not — the added scenarios include one only semantic retrieval can answer —
and it lifts strict recall from 0.89 to a clean 1.00. It still nearly doubles
the token cost.

### The marginal price of a fact

Fact by fact (`scripts/embedder_utility.py`), off → warm on:

| adapter | facts + | facts − | net | Δ tokens | tok per fact gained |
|---|---:|---:|---:|---:|---:|
| **nudox-search-only** | +28 | −4 | **+24** | +46,871 | **1,953** |
| nudox-full | +1 | 0 | +1 | +46,987 | **46,987** |
| nudox-keys | +1 | 0 | +1 | +46,987 | 46,987 |
| nudox-graph-first | +1 | 0 | +1 | +44,091 | 44,091 |
| nudox-oneshot | +1 | 0 | +1 | +8,420 | 8,420 |
| ripgrep | 0 | 0 | 0 | 0 | — |

**A 24× difference in the marginal price of a fact**, between the policy that
has no second tool and the policies that do. For `search-only`, 1,953 tokens
per fact is competitive with ripgrep's 1,653 *and* buys capabilities no
structural index has — enumerating a TypeScript entry point, and finding the
`memrchr*` family from a description rather than a name. For the graph-equipped
policies, the same section costs roughly 46,000 tokens per fact, because those
policies already reach the same facts through a graph edge.

**So: semantic on for `search-only`, off for `full`, `keys` and
`graph-first`.** This is the one intervention in the whole investigation that
improves the benchmark — and it is a configuration choice, not a code change.
Three engine changes were built and measured (N1 comparable relevance units, N2
a prefix tier, N4 a demand-gated page budget); all three were null.

## 5. Caveats

* Nine scenarios and two packages. Better than five, still small.
* `go/uuid/*` and `py/requests/*` decline for every Nudox adapter because
  those packages are not in the loaded corpus; the implementation-family
  scenarios stay unscored without a judge.
* `ripgrep`'s numbers depend on `${NODE_MODULES}` being exported. It was.
