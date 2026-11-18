# Lines against symbols in 20 Backend PRs, 2026-11-18

Tests a pitch line before anyone quotes it: that reviewing a change as symbols
instead of lines turns "a 1,000-line review" into "tens of symbols, under 100".
The last 20 PRs merged into Backend's `canonical` branch, #670 to #716
(21 Oct to 15 Nov 2026), each diffed from its merge commit's first parent.
`report.md` has the table, `results.json` every changed symbol, `reach.json`
the reach figures.

| | |
|---|---|
| PRs with code Nudox indexes | 15 of 20 |
| median changed code lines per symbol | **11.1** (range 2 to 24; pooled 15.8) |
| same, without modules and impl blocks | 11.6 |
| half the PRs with code changed | 120 code lines or fewer, 12 symbols or fewer |
| 1,000 to 2,000 lines | #692: 1,180 lines, 106 symbols · #684: 1,892 lines, 170 symbols |
| the two big PRs | #670: 10,063 lines, 475 symbols · #716: 20,614 lines, 1,374 symbols (494 tests) |

## What it shows

* **Below about 1,000 lines the claim holds**: about one symbol per 11 changed
  lines. Between 1,000 and 2,000 lines it is just over 100.
* **The big PRs break it.** 475 and 1,374 symbols are not a review; changes
  that size need grouping by module or cluster and one path at a time.
* **Reach can exceed the change.** At least 1,132 existing declarations
  mention something #670 modified (545 outside tests), against the 475 symbols
  it changed; for #716 at least 974 (469 outside tests), from only the 137 of
  its 334 modified symbols that could be measured. Most of it runs through two
  widely used types, `SurfaceCommand` and `SurfaceReply`, each mentioned in
  about 500 declarations. The small PRs reached 0 to 11.
* **A quarter of the PRs had nothing to map**: #700, #690, #683, #682 and
  #677 changed only files Nudox does not index (Nix, Nushell, a JSON fixture,
  a text list, docs).

The line to quote instead: *about one symbol for every 11 changed lines; a
typical PR touches about a dozen.*

## How it was counted

* **Lines** — added plus removed, leaving out vendored code (`vendor/`,
  `third_party/`; 16,265 lines in #670) and lockfiles. "Code lines" are in
  languages Nudox has frontends for; every changed code line here was Rust,
  Python, C# or TypeScript.
* **Symbols** — the innermost declaration holding each changed line, parsed
  with tree-sitter (`scripts/pr_symbols.py` has the exact rules). Changes
  outside any declaration count as one module-level symbol per file; blank
  lines, and comments between members, count for nothing. Checked by hand
  against the #673 diff and the new `crates/account/src/key.rs` in #684.
* **Reach** — `scripts/pr_reach.py`: for modified or removed symbols with a
  distinctive, uniquely declared name, the other declarations in the Backend
  tree at `a01c2b0` (11 Nov) that mention the name as a whole word. A floor:
  common names (`new`, `parse`) cannot be measured this way.
* **Not Nudox itself.** Indexing Backend's 1.9 million lines of Rust with the
  debug build was not practical on the 2-core, 7 GB machine this ran on.
  Running the same PRs through Nudox's own symbols and references would
  replace both stand-ins, and test Nudox on its own repository.

## Rerun

```sh
pip install -r scripts/requirements-pr-symbols.txt
git clone --filter=blob:none --no-checkout --single-branch -b canonical \
    --shallow-since=2026-08-15 https://github.com/nudoxorg/backend /tmp/backend-history
python3 scripts/pr_symbols.py --repo /tmp/backend-history --out results/<date>-pr-symbols \
    --prs results/2026-11-18-pr-symbols/prs.txt     # or --count 20 for the latest
python3 scripts/pr_reach.py --results results/<date>-pr-symbols --tree /path/to/Backend
```

The blobless clone stays small (16 MB here); git fetches the blobs each diff
needs. The two scripts together take under a minute on this repository.
