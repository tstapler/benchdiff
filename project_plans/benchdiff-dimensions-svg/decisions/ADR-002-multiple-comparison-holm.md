# ADR-002: N-way significance uses Holm correction per benchmark row, scoped to Mann-Whitney-decided pairs

**Status**: Accepted
**Date**: 2026-10-31
**Project**: benchdiff-dimensions-svg

## Context
N variants compared to one reference gives N-1 tests per benchmark row. Research disagreed: architecture said no correction in v1 (report raw p, Holm opt-in later); pitfalls and build-vs-buy said Holm against the reference. Existing precedence is Mann-Whitney, then interval, then threshold; only Mann-Whitney yields a p-value (`research/pitfalls.md` section 5). Applying a correction to one rule of three could make verdicts inconsistent.

## Decision
1. Each variant is compared **only to the reference** (N-1 pairs per row, never all pairs).
2. **Holm step-down** is applied **per benchmark row** as a **downgrade-only post-pass** over the cells for which the Mann-Whitney test ran (`method == MannWhitney`, i.e. the cell has a p-value). The **family size m** is the count of such cells in the row, whether or not the threshold/min-abs floor later suppressed their verdict (the test was still run, so it counts). Cells decided by the interval rule have no p-value and are neither counted nor altered. The pass never re-runs the kernel and never upgrades: a Regressed/Improved cell whose adjusted p fails alpha becomes Unchanged; the human method text `mann-whitney, Holm-adjusted p=0.07 (m=3)` is derived at render time from the stored `p_adjusted` and the row's m (no new `Method` variant). Counts, geomean and `matrix_failed` are computed after the pass.
3. With two columns (one pair, m=1) Holm is the identity, so two-way behaviour and every matrix with a single variant are unchanged. The two-way path (`compare()`) is not touched at all.
4. Both the raw `p_value` and `p_adjusted` are kept on the cell and in JSON. Every cell states its deciding rule.
5. Config key `correction`: `holm` (default for matrix) or `none`. Implemented in `stats.rs` as `holm_adjust(&[f64]) -> Vec<f64>` (about 15 lines, no dependency).
6. The method note always names the policy ("Holm-adjusted across N-1 comparisons per benchmark; reference-only").
7. Rank bars in charts are never styled as significant unless the pair verdict is non-Unchanged; ties are shown as ties.

Not corrected: the family of rows across the whole report (hundreds of benchmarks). **Holm is per-row only**: it controls the error rate inside one benchmark's N-1 comparisons, not across rows. That is documented in the README and the Method note, with the recommendation to pair N-way `fail-on-regression` with a `threshold` and `min-abs` (the threshold floor is what keeps hundreds of identically distributed rows from tripping the gate; a seeded 300-row test pins it, plan 2.1.4). A Benjamini-Hochberg option is a follow-up.

## Alternatives rejected
- **None in v1** (architecture): an uncorrected p<0.05 among 10 variants x 300 rows misleads; reading raw p cannot be left to users.
- **Holm across all verdict rules**: interval/threshold rules have no p-values; mixing yields inconsistent verdicts.
- **BH/FDR across the report**: larger behaviour change, needs a second pass over all rows; deferred.
- **Kruskal-Wallis omnibus**: more machinery; deferred.

## Verification
Unit tests against Holm textbook vectors; property test that m=1 is the identity; regression test that two-way golden output is byte-identical.

## Amendment (post-review)
Unit vector pinning m: raw p `[0.01, 0.04, 0.03]` where the 0.03 cell was already floor-Unchanged still yields m=3 and adjusted `[0.03, 0.06, 0.06]` (plan task 2.1.4).
