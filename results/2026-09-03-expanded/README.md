# Canonical, 2026-09-03 — 15 scenarios

Backend `canonical` @ `2f82d74b`, through `interface-cli`. Same conclusion as
`../2026-09-03-canonical/`, on a suite half again as large.

| adapter | pass | attempted | unsupported | strict | matched | distractors | tok/scenario |
|---|---:|---:|---:|---:|---:|---:|---:|
| nudox-canonical | 0/0 | **0** | **15** | — | — | — | — |
| ripgrep | 8/11 (73%) | 11 | 2 | 0.97 | 0.58 | 5 | 9,452 |

## By family

| family | ripgrep | strict | tok/scenario |
|---|---:|---:|---:|
| efficiency | 1/1 (100%) | 1.00 | 664 |
| polyglot | 3/4 (75%) | 1.00 | 4,118 |
| retrieval | 4/6 (67%) | 0.94 | 14,472 |
| implementation | — (2 unscored) | — | — |

**Strict recall 0.97 against a 73% pass rate.** Every remaining failure is
precision or cost, not finding — which is the shape the five new scenarios were
built to expose.

## The five scenarios added on this date

Two cover task kinds that had **no scenario at all** (`implementors`,
`outbound`), so those adapter paths had never been exercised.

| scenario | what it isolates | ripgrep |
|---|---|---|
| `py/requests/toplevel-exceptions` | precision: 10 of 25 classes are re-exported | strict 1.00, 4 distractors, **fail** |
| `go/uuid/exported-types` | Go's own export rule (capital initial) | pass, 1,406 tok |
| `rust/memchr/implementors-frequency-rank` | code vs. documentation | strict 1.00, 1 distractor, **fail**, 347 tok |
| `rust/memchr/outbound-memmem-find` | addressing, not comprehension | pass, **38,051 tok**, matched 0.33 |
| `rust/hashbrown/definition-tryreserveerror` | package diversity | pass, 601 tok, matched **0.00** |

The first one found a real adapter bug: ripgrep's overview search grepped
`pub |export |public `, which matches nothing in Python. It had been scoring
0.00 on 146 tokens for a question it never actually asked.

The last one produced the widest strict-versus-matched split in the corpus —
1.00 against 0.00. It greps the declaration and reads the variants out of the
context printed after it, so every fact arrived as surrounding text rather than
as something the tool matched.

## Reproducibility

`nbench verify --config configs/canonical.toml --repeat 3` over this suite:
**30 cells, 100% reproduced, no cost moved more than 5%.**
