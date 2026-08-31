# The graph tool had never once run

`run-2026-08-31-145*` — memchr and commander only, **semantic search off**.

## What was wrong

Every `graph` call this benchmark has ever made failed. All of them, in every
run, since the adapter was written. The transcripts said so plainly and nobody
had read them:

```
graph_query_failed: Field Package received an invalid value for argument name: $package
```

Three independent mistakes, each enough on its own:

1. **The variables went under the wrong key.** `GraphQueryArgs` names the field
   `args`; the adapter sent `variables`, so `$package` was never bound.
2. **The root type was wrong.** The schema card's worked queries enter at
   `Symbols` or `Packages`. The adapter wrote `Package(name: $package)`, and
   field arguments do not take variables here — only `@filter` does.
3. **The edge did not exist.** The callers query walked `referencedBy`. The
   `Symbol` interface has thirteen edges and that is not one of them.

Two further wrong turns, recorded so they are not retaken: `occurrencesOf` is
the *forward* edge — "references I contain" — and returns only the declaration
itself; and `usages` lands on `Symbol`, so it has no `owner` hop.

The correct callers query is `Symbols { name @filter … usages { … } }`, and
outputs must include `path`, or every row comes back as a bare name beside an
opaque hash and nothing matches at strict tier.

## What it cost

`nudox-graph-first` — the ablation whose entire purpose is to price the escape
hatch — has been measuring an error message. Every number ever published for
it is the cost of a failed call plus whatever `search` happened to return.

This page therefore **retracts a published claim**. `results/2026-08-30/protocol.html`
said:

> `graph-first` costs the same as full and answers less. The schema card is
> cheap (134 tokens) but writing a correct Trustfall query against it is not
> free.

The second sentence is true in a way that indicts the harness, not the tool:
the query was never accepted, so the ablation never tested what it claimed to.

## After the fix

| adapter | pass | strict | matched | tok/scenario |
|---|---:|---:|---:|---:|
| nudox-full | **5/5** | 1.00 | 1.00 | 4,464 |
| nudox-graph-first | **5/5** | 1.00 | 1.00 | 4,692 |
| nudox-keys | **5/5** | 1.00 | 1.00 | 4,464 |
| ripgrep | 6/7 | 0.95 | **0.57** | 8,447 |
| nudox-oneshot | 2/5 | 0.43 | 0.43 | 151 |
| nudox-search-only | 2/7 | 0.39 | 0.39 | 292 |

**Read the scope before the numbers.** This endpoint held two packages, so
Nudox attempted five scenarios and declined four; ripgrep attempted seven. On
the five both could attempt, ripgrep passes four. The comparison is
`nudox-full` 5/5 against `ripgrep` 4/5, at roughly half the tokens — the first
like-for-like result in this repo's history where Nudox is ahead on both.
It is also five scenarios. Nothing here is significant.

The `matched` column is the durable part: Nudox's 1.00 and ripgrep's 0.57 mean
every fact Nudox returned was on a line it produced as its answer, while two
fifths of ripgrep's came from the window printed around a match.

## The overview scenario, which had never passed

`rust/memchr/public-surface` failed at exactly 60% for every Nudox policy in
every run: all nine `memchr*` facts found, all six `memrchr*` absent.

That shape is a query's, not an index's. `search` ranks a name section built
from *exact and prefix* matches, so a query can only reach names sharing its
prefix — `memchr` finds `memchr2` and `memchr_iter` and can never find
`memrchr`. Following it with `read` made it worse: an overview names no
`probe.symbol`, so the read landed on whichever hit ranked first, which in
`run-2026-08-30-225507` was `arch::x86_64::avx2::memchr[module]`.

Enumerating a surface is a graph question. `Packages { members { isPublic … } }`
answers it, and the scenario now passes — as does `ts/commander/public-surface`,
which previously needed the embedder to pass and now passes structurally.

## What is still not measured

* Only memchr and commander were loaded; hashbrown, uuid and requests were
  not, so the diff and polyglot scenarios are declined here.
* **Semantic search was off.** These are structural-only numbers again — this
  time because the machine cannot hold the embedder and the corpus at once,
  not because it was forgotten.
* `diff` now asks for `limit: 500` rather than the default 50, on the reasoning
  that a complete answer is worth a bigger page and that raising a scenario's
  call budget until it passes would be tuning to the answer. Unverified: the
  hashbrown package was not loaded in this run.
