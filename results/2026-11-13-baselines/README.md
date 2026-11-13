# Baselines, 2026-11-13 — 30 scenarios (15 earlier + 15 new)

`configs/baselines.toml`: ripgrep and rust-analyzer. Same machine and roots as
`../2026-11-13-competitors/`.

| adapter | pass | attempted | unsupported | errored | strict | matched | distractors | tok/scenario |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| ripgrep | 11/20 (55%) | 20 | 9 | 0 | 0.88 | 0.50 | 10→9 | 6,452 |
| rust-analyzer | 0/14 | 14 | 15 | **14** | — | — | — | — |

**rust-analyzer was not available.** `rust-analyzer` on this machine is the
rustup proxy without the component, and `rustup component add` could not
download (static.rust-lang.org unreachable). The adapter reports each Rust
attempt as `errored`, not as a wrong answer, so its row is an absence of data.

**ripgrep on the earlier 15 scenarios is unchanged from 2026-09-03** in shape
(strict recall high, failures are precision or cost). On the 15 new ones it
passes 4 of 10 scored, against 7 of 10 on the earlier ones; of the six new
failures, four carry a distractor (`reserve`, `insert` and `get_disjoint_mut`
on the wrong type; commander's `Argument` side, which also cost it the
`addOption` caller made from inside a closure), one is a Python implementors
question its pattern cannot express (0%), and one found `Session.request`
only as the bare word `request` (strict 50%).

## What was not run

* Nudox (no `backend-mcp` build; see `docs/RUN-V2-LOCALLY.md`).
* Go scenarios (no Go module proxy access), the six diffs (ripgrep declines
  them), the judge.
