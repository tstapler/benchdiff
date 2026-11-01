# UX research: dimensions, N-way comparison, SVG charts

Scope: PR comment, job summary, README/release notes, CLI, Action/config inputs. Baseline read from `README.md` and `crates/benchdiff-core/src/markdown.rs` (lines 55-136: marker, title, summary line, skipped warning, changed rows, `max-rows` cut notice, collapsed `<details>` for unchanged, env and method notes) and `action.yml`.
Confidence labels: VERIFIED = opened or run in this session; KNOWN = established prior knowledge of the named tool, not re-fetched this session (no web calls were made); INFERRED = my judgement.

## 0. Baseline (VERIFIED)

- Report shape: sticky marker, `###` title, one bold summary line (`N regressed · N improved · ...`), optional `⚠️` skipped line, a table per group (`Benchmark | Base | Head | Change | Evidence`), unchanged rows folded in `<details>`, env and method notes at the end.
- What already works and must be kept: verdict-first summary line; worst-first sorting; evidence column that says which test judged; explicit "N more not shown (`max-rows`)" and "skipped metrics" lines. Loud degradation is the house style. New cap and failure states (sec. 4) should copy it.
- Gaps the feature fills: no cross-implementation ranking; the `▰▰▰▱▱` bar is one-series and quantised to 5 cells; no distribution view.

## 1. Comparable patterns: what works

| Tool | Works | Doesn't / avoid |
|---|---|---|
| benchstat (KNOWN) | Dense old/new/delta table; `~` for "no significant change" (the eye skips noise); `p=0.003 n=10` inline; geomean row at the bottom; one table per unit. | Text-only; no N-way beyond column-per-file; `~` is cryptic to newcomers (needs a legend). |
| Bencher PR comments (KNOWN) | Per-benchmark pass/fail against thresholds, link out to a hosted plot; consistent emoji status. | Needs a service; comment becomes a wall of links as benchmarks grow. |
| Criterion (KNOWN) | Violin/PDF plus "change" summary with CI; text verdict wording ("Performance has regressed") is instantly read. | HTML-report-per-benchmark; no cross-benchmark overview. |
| hyperfine (KNOWN) | Summary ranks N commands relative to the fastest: "A ran 1.32 ± 0.05 times faster than B". Best-in-class N-way mental model. | Single metric, CLI only; "times faster" breaks down for ratios near 1. |

Take-aways:
1. Lead with a one-line verdict, then the chart, then the table (chart is a scan aid; table is the source of truth with numbers and p-values).
2. Mark noise explicitly and quietly (benchstat `~`): benchdiff already folds unchanged rows; the chart must do the same by greying, not hiding.
3. Always print the number next to a mark. A chart without labelled values is unusable for review decisions.
4. Cap and say so (existing pattern).

## 2. Mental models for N implementations

Jobs ranked by how often reviewers ask them (INFERRED):
- "Is my variant faster than the one we ship?" -> relative to a chosen reference.
- "Which of these N is fastest here?" -> rank.
- "Overall which is best across the suite?" -> geomean.

Recommendations:
- **Default view: relative-to-reference.** Reference = explicit `reference` setting, else the first label in input order (stable, predictable, matches the README "first/fastest" ambiguity; prefer *first*, because "fastest" changes the reference between runs and makes diffs between PR comments incomparable). Rank is shown as a secondary column, not the sort key of the matrix.
- **Phrasing.** Use signed percent for base-vs-head (existing `+30.0%`). For N-way use a ratio with the reference as 1.00x: `0.74x` (lower is better) next to a word verdict. Use "x faster/slower" only when |change| >= 2x (`2.3x faster`), because "1.04x faster" reads as hype and mixes with direction confusion for higher-is-better metrics. Always say `slower`/`faster`, never `higher`/`lower`, in prose (direction-agnostic across `better=higher` metrics like MB/s).
- **Geomean.** Add one summary row per implementation (geomean of ratios over benchmarks present in all impls; state N used; exclude insignificant rows? No: include all, label "all benchmarks"; benchstat precedent). Show only when >= 3 benchmarks. Missing cells reduce N and the row says `n=12/15`.
- **Significance.** Mark each non-reference cell with the existing verdict glyph/`~`; the multiple-comparison policy (reference-only comparisons, so N-1 tests; correction optional) belongs in the stats research, but the UX needs one line in the method note naming it.
- **Matrix.** Rows = benchmark (the row-key dimension), columns = implementations, reference first and bold. Cell = `value (ratio)` e.g. `1210 ns (0.74x) 🟢`. Fastest per row gets a trophy-less marker: bold the value. Wide matrices (> ~6 implementations) wrap badly in GitHub comments, so cap columns at 8 and push the rest to JSON with a visible notice.

### Chart selection

| Job | Chart | Why |
|---|---|---|
| Base vs head, many benchmarks | **Delta bars** (diverging, centred on 0%, sorted by delta): one row per benchmark, bar length = % change, label at the bar end. Dumbbell (base dot -> head dot on a shared log axis) as the alternative when absolute values matter. | Magnitude and direction in one glance; scales to 20-30 rows; works for mixed units because it plots % not absolutes. Dumbbells fail across units (each row needs its own scale). **Recommend delta bars as v1 default.** |
| N implementations, per benchmark | **Grouped horizontal bars of ratio vs reference** (ref = 1.0 gridline), one small-multiple panel per benchmark or one cluster per benchmark, bars in implementation order and fixed colour. | Direct labels fit; same encoding as delta bars so the reader learns one chart. |
| Many benchmarks x many impls | **Heatmap-like matrix of ratios** is tempting but needs colour alone; skip for v1, the Markdown matrix covers it. | Avoids color-only encoding. |
| Distribution (samples exist) | **Strip plot with median tick** (all points, jittered deterministically by index), one lane per implementation/side; box plot only when n >= 20. | Samples per run are typically 8-20: a box hides that; a strip shows overlap, which is exactly what the Mann-Whitney verdict is about. |
| Overall ranking | Single sorted bar of geomean ratio. | Optional v1.1. |

Log vs linear: ratios plotted as log2 scale around 1.0 (so 2x slower and 2x faster are symmetric); delta-percent bars are linear but clamp at +/-100% with an arrow/"clipped" marker (INFERRED, avoids one outlier flattening the rest).

### Visual specs (concrete)

- **Width**: 760 px viewBox (renders at GitHub comment column width ~ 700-780 px; scales down on mobile because `width` is not fixed in CSS). Row height 28 px (bar 16 px, 4 px rounded data-end, 2 px gap between grouped bars). Header 44 px (title, subtitle with reference and N). Footer 24 px (legend and "N more not shown").
- **Row cap**: 30 rows per chart (about 900 px tall max); beyond cap -> "+ N more not shown" line inside the SVG *and* in the Markdown. Max 8 series.
- **Label column**: 220 px left gutter, monospace-free sans stack (`system-ui, -apple-system, "Segoe UI", Roboto, sans-serif`), 12 px. Text width cannot be measured in WASM: estimate 0.56 em average per char for the sans stack (about 6.7 px at 12 px), uppercase and digits get 0.62. Truncate with an ellipsis in the middle for names (keep the suffix: `BenchmarkSortLa…/impl=quick`), because the distinguishing part of benchmark names is usually at the end. Fit = floor((220-8)/6.7) = ~31 chars; put the full name in a `<title>` child of the row so hover shows it. Add 10% safety margin so a wrong estimate truncates rather than clips.
- **Value labels**: 11 px, placed at the bar end outside the bar; flip inside (white/ink text) when within 48 px of the plot edge.
- **Palette** (reference palette from the dataviz skill, validated by `scripts/validate_palette.js`, VERIFIED run this session): categorical light `#2a78d6 #eb6834 #1baf7a #eda100 #e87ba4 #6250d6`, dark `#3987e5 #d95926 #199e70 #c98500 #d55181 #9085e9`. All checks PASS in both modes; the light mode contrast check WARNs (<3:1 for `#1baf7a #eda100 #e87ba4`), which obligates visible labels, so every series gets a direct label and value label is mandatory. CVD worst adjacent pair deltaE 9.1 light / 8.4 dark (above the 8 target... on the floor band in dark; add shape redundancy anyway). Assign colour by implementation order, never by rank, so colour is stable across runs.
- **Verdict colours for delta bars**: use diverging blue (improved) / red-orange (regressed) plus gray for unchanged, not the categorical set; pair with shapes (see sec. 3). Status palette: good `#0ca30c`, critical `#d03b3b`, serious `#ec835a`.
- **Reference**: draw as a 1 px full-height line at 0% / 1.0x with a text label.
- **Light/dark**: GitHub's theme can differ from the OS setting, and an SVG loaded via `<img>` only sees the OS `prefers-color-scheme` (KNOWN behaviour of SVG-in-img; not re-tested). Two options: (a) emit two files (`*-light.svg`, `*-dark.svg`) and reference them with `<picture><source media="(prefers-color-scheme: dark)">` in Markdown (GitHub honours this in README and comments; KNOWN); (b) emit one SVG with an opaque rounded-rect background card in its own light neutral (`#fcfcfb`) and ink, readable on any page theme. Recommend **(b) as the default, (a) opt-in** (`theme: auto|light|dark|both`): one URL per chart means half the hosting uploads and no `<picture>` failure mode in job summaries. Needs to be verified in a real PR (requirements already list this).
- **Determinism**: coordinates formatted with fixed 1-2 decimals; no timestamps, no random ids (derive ids from index).

## 3. Accessibility

- Root: `<svg role="img" aria-labelledby="t d">` with `<title id="t">` (short: "Benchmark change, head vs base: 3 regressed, 1 improved") and `<desc id="d">` (full text summary: top 5 changes with values, reference, cap notice). Row elements get their own `<title>` for hover.
- Markdown alt text: same sentence as `<title>`, <= ~125 chars; the longer `<desc>` goes in a collapsed `<details>` text table beneath (the table already exists, so alt text is never the only access path). Failing hosting still leaves the table.
- Not colour-only: regressed = bar to the right with a filled triangle/diamond cap `▲`, improved = left with `▼`, unchanged = hollow circle; in N-way each implementation gets a marker shape (circle, square, diamond, triangle, cross, star) used in the legend and at the bar end, and always a direct text label with the value. Optional hatch texture in `theme: high-contrast`.
- Contrast: text ink against card >= 4.5:1 (use `#0b0b0b` on `#fcfcfb`, secondary `#52514e`); bar fills >= 3:1 against the card or a visible label (see WARN above). Text never takes the series colour.
- Minimum font 11 px at rendered scale; check at 50% (mobile) = about 5.5 px: so the viewBox should be 760 but charts must stay readable when scaled to ~375 px width. That fails for the 220 px label gutter plus bars. Mitigation: on mobile GitHub, images scale to column width; choose 640 px viewBox width and 12-13 px text (scaled ~7 px on a phone, marginal) and rely on the Markdown table for phones. Say so in docs. INFERRED.
- Keyboard and screen readers: SVG in `<img>` is a single image; the table is the accessible representation, which is why the text table always ships.

## 4. Error and edge states

State -> behaviour (all surfaced in the Markdown, mirroring the "skipped metrics" line; none fatal unless the user opted in):

| State | Chart | Markdown |
|---|---|---|
| Implementation missing for some benchmarks | Cell/bar absent, drawn as an empty slot with `—` and a hollow marker, never a zero bar | Cell `—`; summary line `· 3 incomplete`; method note names the missing impl and count |
| Reference missing for a benchmark | Row dropped from ratio chart, listed | `⚠️ reference "X" absent from 2 benchmarks: ...` (first 10 like skipped) |
| Only one implementation / no base | Skip chart (a chart of one is noise) | Existing head-only table; one line: "chart skipped: nothing to compare" |
| Cap exceeded (rows/series/columns) | Show top N by |delta| (stable tiebreak: input order) and footer text `+ 17 more not shown` inside the SVG | `_...and 17 more not shown (max-chart-rows: 30)_` plus pointer to the JSON |
| No samples | Distribution chart skipped, not blanked | `distribution chart skipped: no samples in the input (use -count=10 or Criterion)`; delta charts unaffected |
| Hosting failed (permissions, fork, rate limit, upload error) | SVG file still written to `svg-dir`/outputs | Fall back to today's Unicode `▰` rendering, one `⚠️ charts could not be hosted: <reason>; the SVGs are attached as <path/artifact>` line, `core.warning` annotation; job still succeeds |
| Fork PR read-only token | Same as hosting failed, but the reason names it | Job summary still gets the table; no scary stack trace |
| Invalid dimension spec / regex matches nothing | Fatal, exit 2 (config error, not data error) | Message: `dimension "impl": pattern "/impl=(\w+)" matched 0 of 48 benchmark names (example: "BenchmarkSort/size=100"). Check 'dimensions' or use labels (label=path).` Include count, one example, and the key to change |
| Names collide after pivot (two rows map to one key) | Fatal with both source names | Same style as the existing "benchmark present in two files" error |

Principle: data problems degrade with a visible notice; config problems fail fast with the offending key, a sample value, and the fix.

## 5. Jobs to be done

1. As a PR author: "did I regress anything, and by how much?" -> summary line + delta chart + folded table (today plus chart).
2. As a reviewer choosing between implementations: "which is best and is the gap real?" -> N-way matrix, ratio chart, significance marks, geomean row.
3. As a maintainer cutting a release: "paste something into release notes/README" -> standalone SVG + table (self-contained, no hosted URL, opaque background, `<title>`/`<desc>`).
4. As a library author with `size=` params: "how does it scale?" -> pivot the param as the axis; line/bars per size (v1.1; delta bars per size cover v1 via grouping).
5. As CI owner: "fail on regressions, no flapping, no surprises" -> unchanged defaults, opt-in charts, chart failure never fails the job.
6. As a CLI user locally: "show me now" -> `--svg-dir out/` writes files and prints their paths; table on stdout as today.

## 6. Config ergonomics

Naming (kebab-case inputs, same names as CLI flags with `--`, same names as config keys; one vocabulary, all three surfaces; existing precedent: `max-rows`, `only-changed`, `base-format`):

| Concept | Input / flag / key | Default |
|---|---|---|
| Config file | `config` / `--config` | auto-discover `benchdiff.toml` (or chosen format) in repo root; error only if an explicit path is missing |
| Labelled inputs | `head: 'quick=a.txt'` style `label=path`; CLI `--head quick=a.txt` repeatable | unlabelled = current behaviour |
| Comparison axis | `compare-by` / `--compare-by` (values: `run` default, `impl`, or any dimension name) | `run` (base vs head, today) |
| Row key | `group-by` / `--group-by` | current `(group, name, measure)` |
| Dimension extraction | `dimensions` (e.g. `impl=/impl=(\w+)`; params auto-detected from `[k=v]` and `/k=v`) | auto for the documented conventions; explicit rule always wins |
| Reference | `reference` | first label |
| Charts | `charts` (`none`, `delta`, `ranking`, `distribution`, comma list) | `none` (opt-in, golden-safe) |
| Chart size and caps | `chart-rows` (cap, default 30), `chart-width` (default 760) | as stated in sec. 2 |
| Theme | `chart-theme` (`light` default, `dark`, `auto`) | `light` card |
| SVG dir | `svg-dir` / `--svg-dir`; Action outputs `svg-paths` (newline list) | unset |

Guidance:
- **One knob, one name across surfaces.** Avoid `svg-*` for chart choices (SVG is the format; `charts` is the intent). Avoid abbreviations (`dim`, `ref`).
- **Zero-config happy path**: `charts: delta` with existing inputs must produce a useful chart; every other key is optional. Defaults documented in one table (extend the README table, adding a "Surface" column or a note that names are identical on all three).
- **Precedence** flags > inputs > file > defaults, and the report's method note (collapsed) prints the effective config source for each non-default value (`threshold=3 (benchdiff.toml)`): this answers the "why did it behave this way" question that precedence rules always cause.
- **Discoverability**: `benchdiff --help` lists all flags with defaults; `benchdiff --print-config` dumps the effective merged config; unknown keys in the file are errors with a nearest-name suggestion (`unknown key 'chart-row'; did you mean 'chart-rows'?`) rather than silently ignored; action.yml descriptions state the allowed values (as today for `format`).
- **Enum errors list valid values**, as the existing numeric validation does. Example: `charts: "bars" is not valid; use none, delta, ranking, distribution (comma-separated)`.
- **Per-metric thresholds** as a table in the file (`[threshold]` default plus `[threshold."group/name"]`) not as Action inputs; inputs stay flat. Config file is the place for anything list- or table-shaped (INFERRED; Action inputs are strings).
- Config format: the choice is for the stack research; UX requirement is comments allowed and no significant whitespace traps, which favours TOML over JSON/YAML for hand-editing.

## Recommendations summary

1. v1 charts: delta bars (base-vs-head and, with a reference, N-way ratio) plus strip distribution when samples exist; skip heatmap.
2. Opaque self-contained light card by default; `both` themes opt-in via `<picture>`.
3. Always ship the full text table and alt text; the chart is never the sole carrier.
4. Every degradation is stated in Markdown in the existing `⚠️`/italic-notice style; hosting failure never fails the job and falls back to the Unicode bar.
5. Same vocabulary across inputs, flags, config keys; `charts` default `none` keeps goldens byte-identical.
6. Items to verify empirically before locking (not done here): how GitHub camo renders `prefers-color-scheme` in an `<img>` SVG, and mobile legibility at the chosen viewBox.
