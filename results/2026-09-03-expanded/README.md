# Canonical, 2026-09-03 — 14 scenarios

Backend `canonical` @ `2f82d74b`, through `interface-cli`. Same conclusion as
`../2026-09-03-canonical/`, on a suite half again as large.

| adapter | pass | attempted | unsupported | strict | matched | distractors | tok/scenario |
|---|---:|---:|---:|---:|---:|---:|---:|
| nudox-canonical | 0/0 | **0** | **14** | — | — | — | — |
| ripgrep | 7/10 (70%) | 10 | 2 | 0.97 | 0.63 | 5 | 10,314 |

## By family

| family | ripgrep | strict | tok/scenario |
|---|---:|---:|---:|
| efficiency | 1/1 (100%) | 1.00 | 664 |
| polyglot | 3/4 (75%) | 1.00 | 4,119 |
| retrieval | 3/5 (60%) | 0.93 | 17,200 |
| implementation | — (2 unscored) | — | — |

## What the four new scenarios added

Two of them cover task kinds that had **no scenario at all** — `implementors`
and `outbound` — so those adapter paths had never been exercised.

- `py/requests/toplevel-exceptions` — precision, not recall. requests re-exports
  10 of the 25 classes `exceptions.py` defines. A tool that answers by printing
  the module scores full recall and trips every distractor. **This one found a
  real adapter bug**: ripgrep's overview search grepped `pub |export |public `,
  which matches nothing in Python. It had been scoring 0.00 on 146 tokens for a
  question it never actually asked.
- `go/uuid/exported-types` — whether a tool models Go's own export rule (capital
  first letter) rather than pattern-matching `type` lines. ripgrep passes, 1,406
  tokens, no distractors.
- `rust/memchr/implementors-frequency-rank` — code versus documentation. The
  trait's doc comment contains a compiling example `impl ... for Binary`, so the
  string is in the source and is not an implementation. ripgrep returns all
  three impl lines: full recall, one distractor, 347 tokens, fail.
- `rust/memchr/outbound-memmem-find` — addressing, not comprehension. A six-line
  body, but `find` is one of the commonest names in any Rust crate. ripgrep
  passes at **38,051 tokens**, matched recall 0.33 against strict 1.00.

Note the polyglot and retrieval rows: strict recall stays near 1.00 while the
pass rate does not. Every remaining failure is precision or cost, not finding.

## Reproducibility

`nbench verify --config configs/canonical.toml --repeat 3` over this suite:
**28 cells, 100% reproduced, no cost moved more than 5%.**
