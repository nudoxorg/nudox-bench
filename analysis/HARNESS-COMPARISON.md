# What this session's harness changes did to the numbers

Method: build the pre-session harness at `e3cd3d7` in a worktree, and run both
binaries against **the same scenario files** — the new binary pointed at the old
tree's `scenarios/` — with the same environment and the same ripgrep-only
config. Code is then the only variable.

## On the ten pre-existing scenarios: nothing changed

| | pass | attempted | strict | matched | loose | gap | distr. | tok/scenario | tok/pass | calls |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **before** `e3cd3d7` | 6/7 (86%) | 7 | 0.95 | 0.57 | 1.00 | 0.05 | 0 | 8,447 | 9,855 | 1.0 |
| **after** HEAD | 6/7 (86%) | 7 | 0.95 | 0.57 | 1.00 | 0.05 | 0 | 8,447 | 9,855 | 1.0 |

Checked cell by cell as well as in aggregate: all ten (scenario, adapter) cells
are identical on verdict, strict recall, loose recall, distractors, token cost
and tool calls. The only difference in the report is the new `errored` column.

**That is the correct result.** Crash fixes, a reporting column, grouped
declines and integrity checks are not supposed to move a score. If they had,
something would be wrong.

The ten scenario files are byte-identical between the two revisions, so they are
not a variable either.

## The one change that does move a number

`py/requests/toplevel-exceptions`, the same scenario file, run by each adapter:

| adapter | strict | loose | distractors | tokens | verdict |
|---|---:|---:|---:|---:|---|
| before | 0.00 | 0.00 | 0 | 146 | fail |
| after | **1.00** | **1.00** | **4** | 5,279 | fail |

Both fail, and the pass rate is unchanged — but for opposite reasons. Before, the
overview search was `pub |export |public `, which matches nothing whatsoever in
Python: ripgrep was scored 0.00 for a question the harness never actually asked
it. After, it finds all ten re-exported names and trips all four distractors,
because it answers by printing the module.

So this is **not an improvement to ripgrep's score.** It costs ripgrep 5,133 more
tokens and still fails. What changed is that the number is now true, and the
failure is now the real one: precision, not blindness.

## Honest summary

Nothing here makes a tool look better. Sorted by what each change actually does:

| change | effect on measured results |
|---|---|
| four panic fixes | none — they stop runs dying, which no passing run exercises |
| `errored` column, grouped declines | none — presentation of results already computed |
| `doctor --config`, version-skew check | none — they refuse bad *setups*, before a run |
| judge tie fix | none recorded — the judge has never run in any stored result |
| Python overview fix | one scenario's recall 0.00 → 1.00, cost 146 → 5,279, verdict unchanged |
| five new scenarios | new coverage; does not alter any pre-existing number |
| `$NODE_MODULES` resolved | one scenario stops declining and passes at 956 tokens |

## Reproducing this

```sh
git worktree add --detach /tmp/old-harness e3cd3d7
cd /tmp/old-harness && cargo build
printf 'name = "rg-only"\n[[adapter]]\nkind = "ripgrep"\n' > configs/rg-only.toml
./target/debug/nbench run --config configs/rg-only.toml --scenarios ./scenarios --out /tmp/before
cd - && ./target/debug/nbench run --config /tmp/old-harness/configs/rg-only.toml \
    --scenarios /tmp/old-harness/scenarios --out /tmp/after
```

One caution learned the hard way: the first attempt at this ran the old binary
twice, because a `cd` persisted across the two commands. The tell was that the
"after" table had twelve columns where the new report has thirteen. Use absolute
paths to each binary.
