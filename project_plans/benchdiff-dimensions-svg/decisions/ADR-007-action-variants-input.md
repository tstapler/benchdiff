# ADR-007: Action N-way input is a separate `variants` input (`label=path` lines), not an overload of `head`

**Status**: Accepted
**Date**: 2026-10-31
**Project**: benchdiff-dimensions-svg

## Context
The plan first said `head` "accepts `label=path` lines". `expand()` in `index.js` splits on comma/newline and globs each entry; `stage()` renames files to `head-<i>-<basename>`, losing labels; and `=` is legal in existing paths (`results/size=100/bench.txt`), so a plain existing `head` value would be silently misread as a label, breaking current users. The N >= 3 success metric was verified only through the CLI.

## Decision
1. A new opt-in input **`variants`**: newline-separated `label=path-or-glob` lines. `head` and `base` keep exactly today's parsing (including `=` in paths) and are untouched.
2. Grammar: split each line at the **first** `=`; label must match `^[A-Za-z0-9._-]{1,40}$` (no `/`, so a path can never be mistaken for a label); validated **before** glob expansion. Commas are not separators (globs like `{a,b}` survive). A line without a valid label is an error naming the line.
3. One label = exactly one file. A glob matching **more than one** file, or a duplicate label, is a static error (the job fails). A glob matching **zero** files is a data-dependent problem and degrades with a warning (see item 4). No merging within a label.
4. `variants` combined with `head` or `base` is an error. `reference` defaults to the first line (`fail-on-regression` with `variants` needs an explicit `reference`, ADR-005 item 7). A variant file that is missing, empty or yields 0 metrics (an upstream benchmark job crashed) is a data-dependent problem (round 3): by default the variant is dropped with a notice and the rest is compared; if the reference is lost, or fewer than two variants remain, a notice-only comment is posted; `dimensions-strict` fails the step instead. Ambiguous globs, duplicate labels and bad labels stay static errors.
5. `stage()` writes `variant-<index>-<basename with [^A-Za-z0-9._-] replaced by _>`; labels travel to the binary as `--variant label=<staged path>` (the CLI uses the same first-`=` rule), never in file names.
6. Output semantics follow ADR-006 (companion `counts`/`failed`); a regressed cell over `fail-on-regression` fails the job while the comment is still posted.
7. Tested end to end (plan task 4.2.3b) with the real wasm and `testdata/nway`.

## Alternatives rejected
- Overload `head` with `label=path`: ambiguous with `=` in paths and breaks existing users.
- Encode labels in staged file names: lossy and charset-fragile.
- Merge several files under one label: reintroduces the duplicate-metric collision the labelled design avoids (ADR-006 item 4).
