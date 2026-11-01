# Research: Features (benchdiff-dimensions-svg)

Confidence labels: VERIFIED = fetched this session (source linked); PRIOR = from prior knowledge, not re-fetched this session (re-check before citing in an ADR).

## 1. Industry survey

| Tool | Multi-impl comparison | Dimension pivoting | Relative-to-reference | Geomean | Charts |
|---|---|---|---|---|---|
| benchstat | One column per input file; "vs base" is the change from the first file. VERIFIED | `-col`, `-row`, `-table`, `-ignore`, `-filter` over keys: `.name`, `.fullname`, `/param`, `{file-key}`, `.config`, `.file` (file label), `.unit`; sort `key@alpha`, `key@num`, `key@(a b c)`. VERIFIED | First file is reference; shows `-17.20% (p=0.000 n=10)`, `~` when not significant at alpha 0.05. VERIFIED | Last row, per column. VERIFIED | None |
| hyperfine | N commands; first is the reference. VERIFIED | `-P` / `-L` parameter scans produce one "command" per value | Markdown export has Change and Factor ("1.6x faster") columns. VERIFIED | No | `scripts/plot_histogram`, `plot_whisker` (external Python, JSON in). VERIFIED |
| Bencher | Report = one branch x testbed; comparison is against history, not peers | Fixed dims: branch, testbed, measure, benchmark; thresholds bound to one combination. VERIFIED ([docs](https://bencher.dev/docs/explanation/thresholds)) | Baseline from main history; t-test for under 30 points, z-score otherwise. VERIFIED | No | Hosted plots (service) |
| github-action-benchmark | One series per benchmark over commits | None (name only) | Alert when ratio vs previous exceeds threshold (default 200%) | No | Hosted JS chart on gh-pages (PRIOR) |
| Criterion.rs | Compares to the previous run of the same benchmark; benchmark groups give multi-impl charts | `BenchmarkGroup` + `BenchmarkId` params; parameter on x-axis (line chart) | Change % with p-value and CI | No | PDF, violin (per group), line chart vs parameter, slope; SVG via gnuplot/plotters. PRIOR |
| asv | Per-commit history; parameterized benchmarks (`params`) | Param axes become table/plot dims | `asv compare` base vs head with ratio, `+`/`-`/`x` markers; "BENCHMARKS CHANGED" gate | No | Hosted HTML time series, param pickers. PRIOR |
| pytest-benchmark | `--benchmark-compare`; `group_by` (group, name, func, fullname, param) | `group_by=param`, `param:<name>` | Column "(1.0)" for best, others as multiples ("1.2 (1.20)") per group | No | `--benchmark-histogram` writes SVG, one per group, with min/max/stddev. PRIOR |
| JMH + visualizer | `@Param` creates one result per combination | Visualizer picks the x-axis param, and series | Raw scores with error; no built-in relative | No | jmh.morethan.io (JS bar/line charts, client-side). PRIOR |

### Patterns worth copying
1. benchstat's key vocabulary (`.name`, `/param`, `.file`, `.unit`, `.config`) is the closest model for "pick any dimension as column or row". A `row=`/`col=` pair beats a single "comparison axis" because it also expresses the pivot (rows = benchmark, cols = implementation).
2. First input as reference, relative column per other input (benchstat, hyperfine). Default "first" is conventional. The requirements' "first/fastest" default needs an explicit choice. Fastest differs per row, so a reference column cannot be "fastest"; use "first" for the column reference and offer "best per row" only as a ranking marker.
3. pytest-benchmark's "(1.0)" best-in-group multiplier: a cheap, readable Markdown matrix cell format (`value (x1.20)`).
4. benchstat `~` for non-significant. benchdiff already has `Unchanged` with a method note.
5. Geomean as the last row/summary line, per column vs reference.
6. Criterion's line chart over a numeric parameter (e.g. `size=1000`) is the right chart for "param as axis". Grouped bars suit categorical dims (implementation).
7. hyperfine "N times faster" phrasing is more readable than % for multi-way. Show both x factor and %.

## 2. Gaps in the current code

| Gap | Where | Consequence |
|---|---|---|
| Identity is the fixed triple `(group, name, measure)` | [model.rs:69-71](../../../crates/benchdiff-core/src/model.rs) `Metric::key` | No dims; implementation must be baked into `name` today, so `BenchmarkSort/impl=quick` and `/impl=heap` are different rows and never line up |
| Duplicate detection uses the same key | `MetricSet::validate`, model.rs ~L212 | Two labelled files with the same metric fail `merge` ("duplicate metric"). N-way via `label=path` cannot reuse `merge`; it needs per-label sets |
| `compare()` is strictly `base: Option<&MetricSet>`, `head` | [compare.rs:177](../../../crates/benchdiff-core/src/compare.rs) | Two-way only. `Row` has `base`/`head`, `n_base`/`n_head`, `head_range` hard-coded. N-way needs a new `Report` shape rather than widening `Row` (golden/JSON schema 1 must stay identical) |
| Unit handling per pair | `changed_row` uses `units::factor(base.unit, head.unit)`, returns `Incomparable` row when no factor | Reusable per (ref, other) pair. N-way must scale each to the reference unit |
| Direction taken from head only (`head.better`) | compare.rs ~L332 | Mixed better across implementations of the same metric is silently resolved by the non-reference side; N-way must take it from the reference and flag disagreement |
| Zero base: `delta_pct` is `None` when `base_value == 0.0` | compare.rs ~L322 | Ratios/geomean undefined for zero values; must be excluded and counted, not turned into inf/NaN |
| Significance is pairwise, no multiplicity handling | `evidence()` | N-1 tests per row, hundreds of rows: false-positive inflation |
| No summary statistic | markdown.rs has no geomean, only per-verdict counts (`summary_line`) | Geomean is new |
| Only visual is `bar()` | markdown.rs L244 | Single-series |
| Sorting keyed on triple | `sort_rows` | New sort modes needed (by geomean, by rank, by dim value, natural numeric for `size=`) |
| Skipped metrics tracked by triple | `SkippedKey`, `Skipped` | Dropped-in-one-implementation must be attributed to its label |

## 3. Edge cases and failure modes the design must handle

1. **Missing implementation in some benchmarks.** Render `-`/"n/a" cell, never zero; exclude from geomean for that column, and compute geomean only over the benchmarks present in both the reference and that column (benchstat-style intersection). State "geomean over k of n" so intersection differences are visible. Existing `New`/`Removed` verdicts have no N-way analogue; define them.
2. **Missing reference in a row.** No deltas possible; show values and a marker. Optionally fall back to a per-row reference (best) only if the user opts in.
3. **Different units across implementations.** Reuse `units::factor`; if no factor, cell is `Incomparable` and shown raw with its unit, and excluded from chart and geomean. Charts need a single axis unit per chart: group by `(measure, unit family)`, never plot ns/op and B/op on the same axis (benchstat does the same by splitting tables per `.unit`).
4. **Mixed higher/lower-is-better.** Geomean must invert for `Higher` (or use the ratio oriented as "improvement"), or one geomean mixes meanings. Charts: encode good/bad direction by colour plus position, not by bar length alone. Best-per-row ranking must respect direction.
5. **Single sample / no samples.** `MIN_SAMPLES` fall-back chain Mann-Whitney, interval, threshold already exists; the N-way matrix must show which method each cell used (benchstat shows `n=`). Violin/distribution chart needs a floor (for example 5+ samples) and falls back to a point with interval.
6. **N-way multiple comparisons.** Options: (a) compare only to the reference (N-1 tests per row; still inflated across hundreds of rows), (b) Holm-Bonferroni per row, (c) Benjamini-Hochberg across the whole report. Unstated need: users read this as "which impl is better", and an uncorrected `p<0.05` flag among 300 rows will mislead. Recommend: reference-only comparisons plus a documented, opt-in FDR correction; report the raw p-value in all cases. This needs an ADR (requirements list it as an open question).
7. **Huge N.** Cap columns (chart legend and matrix width: about 6-8 series is the readable limit for categorical colour; requirements say ~10) with an explicit "N more not shown"; cap rows the same way. Wide Markdown tables wrap badly in comments: past a column limit, transpose or split into one table per measure. GitHub comment limit is 65,536 chars, so the matrix needs a byte budget; today `max_rows` is the only guard.
8. **Name-collision when parsing dims.** Go names: `BenchmarkX/size=10/impl=a-8` (trailing `-8` is GOMAXPROCS, which must be stripped, not read as a dim). Hazards: params with `=` and without (`/quick`), names containing `/` or `[]`, dims whose parse yields identical keys for distinct metrics (for example `a/b=1` vs `a-b=1`), the same dim key appearing with different value types (`size=1000` vs `size=big`), and a parsed dim colliding with an explicit file label dim. Rule: parsing is opt-in via an explicit pattern (the requirements' "not guessed" position); after dim extraction run a collision check on the new identity (full dims map + residual name) and fail with a message naming both originals, as `validate()` does today. Unmatched names keep the whole name as residual and empty dims.
9. **Empty data.** Zero metrics, zero rows after filter/cap, a label with zero metrics, or a dimension that yields one value (nothing to compare). Output: a clear one-line Markdown notice, no empty `<svg>` or division by zero in axis scaling, and exit-code semantics defined (does an empty implementation fail the Action?).
10. **Non-finite / zero / negative values** in ratio and log scale: zero and negatives break geomean and log axes. Exclude and count them; keep the existing finite-number validation.
11. **Axis scaling.** Relative deltas in a ranking chart may span 0.01x to 100x; use a log or symmetric-clamped scale with a visible clip marker, or one outlier flattens every other bar.
12. **Determinism.** Stable ordering with ties (existing sorts are stable), fixed float formatting (use `fmt_num`-style rounding, no locale), no HashMap iteration order leaking into output (`compare()` builds `HashMap`/`HashSet`, so N-way must sort before emitting).
13. **Security.** Every dim key/value and label goes into SVG text and attributes: XML-escape and strip control chars; limit label length (extends the existing `flatten`/`code` helpers in markdown.rs).

## 4. Users' unstated needs

- **A verdict, not a table.** "Which implementation wins overall, and is it robust?": a one-line headline (winner by geomean, count of rows where it loses) above the matrix.
- **Chart must stand alone** in a release note or README: title, unit, reference, N of benchmarks, and the commit/tag in the SVG itself, not only in the surrounding Markdown.
- **Stable, diff-friendly output.** Committed SVGs in READMEs should only change when the data does (determinism, fixed precision); users will `git diff` them.
- **Regression gating still works** in N-way: `fail_on_regression_pct` today triggers on any `Regressed` row. Define which pair(s) it applies to (vs reference only) so CI behaviour is predictable.
- **Colour-blind safe, dark/light safe**; direction must not rely on red/green alone (current table uses emoji plus sign, which already helps).
- **Same config in CLI and Action**, and a way to preview locally (`--svg-dir`) without a PR.
- **Reproducible slice:** the report states which dimension was the axis, the reference, and the parse rule used, so readers can interpret the picture without the workflow file.
- **Graceful degradation:** if the image URL cannot be produced (fork PR, private repo), the Markdown still carries the full table; matches the "fallback with a warning" requirement.
- **Plain-text alternative:** `<title>`/`<desc>` plus Markdown alt text that contains the headline numbers, for accessibility and for email notifications that never load images.

## 5. Implications for the plan

- Keep schema-1 `Metric`/`Row` untouched; add dims as an optional, skip-if-empty map (additive) and build N-way as a separate report type, so goldens are byte-identical by construction.
- Model identity as `(group, name, measure, dims)` with a residual-name rule; pivot = choose which dim is `col`, the rest compose the row key (benchstat `-col`/`-row`).
- Ship geomean (oriented by `better`, intersection-only, zero/neg excluded and counted).
- v1 charts: delta dumbbell/bar for two-way, grouped horizontal bar or ranking for N-way, line chart when the column dim is numeric (Criterion style). Distribution/violin only when samples allow, otherwise omitted with a stated reason.
- Needs ADRs: multiple-comparison policy; dim-parse rule syntax; reference semantics ("first" vs per-row best).

## Sources
- benchstat: https://pkg.go.dev/golang.org/x/perf/cmd/benchstat (VERIFIED)
- hyperfine: https://github.com/sharkdp/hyperfine (VERIFIED)
- Bencher: https://bencher.dev/docs/explanation/thresholds , https://bencher.dev/docs/explanation/branches/ (VERIFIED via search summary)
- github-action-benchmark, Criterion.rs, asv, pytest-benchmark, JMH visualizer: PRIOR knowledge, not re-fetched.
- Code: crates/benchdiff-core/src/{model,compare,markdown}.rs (read this session; line numbers approximate for model.rs validate).
