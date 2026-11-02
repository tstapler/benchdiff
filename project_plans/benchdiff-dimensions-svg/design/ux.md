# UX design: benchdiff dimensions, N-way comparison, SVG charts

Inputs: `requirements.md`, `research/ux.md`. Decisions here are UX-level; config format (ADR-001, JSON) and Holm policy (ADR-002) are decided; hosting (ADR-003) is final only after the real-PR check.

## Surfaces

| # | Surface | Kind | Treatment |
|---|---|---|---|
| S1 | PR sticky comment / job summary report | Output read by humans | Wireframe, reading flow, state table |
| S2 | SVG charts (delta, ranking, distribution) | Output read by humans | Wireframe, spec, state table |
| S3 | CLI (`benchdiff`) | Typed | Flow, output samples, error table |
| S4 | Action inputs (`action.yml`) | Typed (YAML) | Condensed + error table |
| S5 | Config file (`benchdiff.config.json`) | Typed | Condensed + error table |

Shared vocabulary: one name per concept across S3/S4/S5 (`charts`, `compare-by`, `group-by`, `dimensions`, `dimensions-strict`, `reference`, `correction`, `cell-format`, `chart-rows`, `chart-width`, `chart-theme`, `svg-dir`, `svg-embed`, `publish-images`, `config`, `threshold`, `min-abs`, `sort`, `max-rows`, `only-changed`, `fail-on-regression`, `title`, `tag`, `format`, `base-format`; config-only: `version`, `$schema`, `thresholds`; CLI-only diagnostics `--print-config`, `--print-dimensions`).

**Naming, stated once.** *Variant* is the canonical term for one column of an N-way comparison, in the UI text, flags, errors and docs ("implementation" appears only inside examples where the axis happens to be `impl`). The CLI flag `--variant label=path` is singular and repeatable, one variant per occurrence. The Action input `variants` is plural because a GitHub input is a single string: it holds one `label=path-or-glob` per line and expands to repeated `--variant` flags. There is no config key for either, because paths in a config file are relative to nothing.

**N-way "regression", defined.** In an N-way report the *reference* is the baseline. A cell is slower than the reference (`🔴`, enum `Regressed`) only when the pair test, the threshold and the multiple-comparison correction all agree it is significantly worse than the reference; faster is `🟢` (`Improved`); there is no comparison between non-reference variants and nothing is judged over time. The report says "slower/faster than reference", never "regression". `fail-on-regression` is the gate: in an N-way run it needs an explicit `reference` (the baseline to protect) and fails on a post-correction cell over the threshold, or any invalid cell; a defaulted (first-listed) reference is rejected with exit 2, and with no `fail-on-regression` an N-way run never fails on cell contents. The Action outputs `regressed`, `improved`, ... keep their names and in N-way runs count cells versus the reference. Source labels in the Method note and `--print-config` are exactly `default`, `config`, `flag`; an Action input reaches the binary as a flag, so inputs and flags share the label `flag`. Defaults leave every existing output byte-identical (`charts` defaults to `none`).

Principle (from research sec. 4): data problems degrade with a visible notice and never fail the job; config problems fail fast (exit 2) naming the key, a sample value, and the fix.

---

## S1. Report (PR comment and job summary)

### Wireframe (base vs head, `charts: delta`)

```
<!-- benchdiff:sticky -->                                  (existing marker)
### Benchmark comparison: head vs base                     (existing title)
**3 regressed · 1 improved · 41 unchanged**                 (existing summary line, first)
⚠️ 2 metrics skipped: ...                                  (existing)
⚠️ chart notices (only when present, see states)

![Benchmark change, head vs base: 3 regressed, 1 improved](https://.../delta.svg)

| Benchmark | Base | Head | Change | Evidence |             (existing table, unchanged)
...
_…and 17 more changed rows not shown (`max-rows: 30`); the JSON report has them all._   (existing wording, unchanged)
<details><summary>Unchanged (41)</summary> ... </details>   (existing)
<details><summary>Chart data (text)</summary> text table equal to the chart's rows </details>
<details><summary>Method</summary> env, test, effective config sources </details>
```

N-way (`compare-by: impl`, reference `std`):

```
### Benchmark comparison: 4 variants (reference: std)
**radix** has the best geomean (0.61x vs reference std, over 15/15 common ns/op rows; variants are not tested against each other); slower than reference in 0 of 15 rows; bubble slower in 3 rows
reference: std (first listed; set 'reference' to change)
gate: fail-on-regression 10% vs std: passed                 (only when fail-on-regression is set)
⚠️ ... up to 5 notice lines, fixed order; "⚠️ N more notices (see Method)" beyond that
![Ratio vs std: radix has the best geomean (0.61x); largest gap: bubble 40.5x slower on Sort/size=100](https://.../ranking.svg)

| Benchmark | std (ref) | quick | radix | bubble | Rank |
|---|---|---|---|---|---|
| Sort/size=100  | 1210 ns | **900 ns** (0.74x) 🟢 | 1000 ns (0.83x) ~ | 49,000 ns (40.5x slower) 🔴 | quick |
| geomean (n=15/15) | 1.00x | 0.74x | 0.61x | 41.2x | |
_🟢 faster than reference · 🔴 slower than reference · ~ no significant difference · — not run · **bold** and Rank order the values in a row; only each variant against the reference is tested, not variants against each other_
```

Wording of the verdict (triad round 2): only a variant against the reference is tested, so the headline claims an ordering, not a head-to-head win: `has the best geomean` (best in the reference's `better` direction), never "fastest overall" or "loses". `slower than reference in N of M rows` counts the common rows where that variant's cell is 🔴 (so `0 of 15` is a defined reading). Other variants (neither the headline variant nor the reference) with at least one 🔴 cell are named after it, `; merge slower in 1 row, tail slower in 3 rows`, from the same post-Holm verdicts the table shows; at most 3 are named, then `and N more variants`. The clause also follows `not comparable` and the reference-is-best line, so a slower variant is never hidden behind the headline. When the reference itself has the best geomean the line reads `**std** (the reference) has the best geomean; no variant is better overall (...)`. **Bold** (best value in a row) and the Rank column are the same kind of ordering marker, and the legend says so.

Rules: verdict line first (a notice-only report has no verdict and starts with its first notice; when the benchmarks every variant ran are fewer than 3 or under half the rows it reads `**not comparable**: bubble covers 4/15 benchmarks (no overall winner)`; the geomean is over the common row set, with each variant's `n=used/total` in the geomean table row), then chart, then table (table is the source of truth). Table and text-fallback `<details>` always ship, so the chart is never the sole carrier. Best value per row bold (an ordering marker, not a test); reference column first and labelled `(ref)`; `—` for missing cell; `~` for no significant difference, with the one-line legend directly under the table (above). `cell-format: ratio` shortens cells to `0.74x 🟢` for phones (the reference column keeps its absolute value; absolute values stay in the JSON). Ratio phrasing: `0.74x` always; "Nx faster/slower" words only when |change| >= 2x; prose says faster/slower, never higher/lower. Columns capped at 8; extra variants go to JSON with a visible notice. Row-ordering and variant-order defaults: variants keep input order (for variants split from benchmark names, the order they are first encountered in the input), and the default reference is the first one, so set `reference` explicitly whenever the benchmark order is not under your control. Geomean row only when >= 3 benchmarks.

### Reading flow

1. Reviewer reads the bold verdict line (<= 1 s): knows whether anything regressed.
2. Scans the chart for magnitude and which rows; hovers a row for the full name (on hosts that support it).
3. Reads the table for numbers and the Evidence column.
4. Exit paths: nothing regressed, stop at step 1; a warning line points to the exact fix (see below); phone or hosting failure, the table alone is complete.

### States (all in Markdown; none fail the job)

| State | What the reader sees | Exit path / action offered |
|---|---|---|
| Charts off (default) | Today's report, byte-identical | Set `charts` to enable |
| Chart hosted OK | Image under the verdict line, alt text = `<title>` sentence (<= 125 chars) | None needed |
| Hosting failed (permissions, rate limit, upload error, private repo) | `⚠️ charts could not be hosted: <reason>; SVGs are in <svg-dir>; to keep them, upload that directory with actions/upload-artifact` **in the Markdown body** (comment and summary). The location is a path on the runner, never an artifact name, because `index.js` cannot upload artifacts; the `actions/upload-artifact` hint is the way to keep the files. The `Chart data (text)` block is opened so the chart's rows are visible without a click, and the Unicode `▰` Change column stays. `core.warning` annotation. | Reason names the fix (e.g. "grant `contents: write`"); SVG files still downloadable |
| Fork PR, read-only token | Same line, reason: `read-only token on a fork PR`. Job summary still gets table. | Same; no stack trace |
| First run of `charts: delta` alone on a read-only default token (often; INFERRED: newer repositories and organizations default to read-only workflow permissions) | `⚠️ charts could not be hosted: token lacks contents: write (grant it to host images); add the permissions block from README "Hosting images"; SVGs are in <svg-dir>; to keep them, upload that directory with actions/upload-artifact`. The opened `Chart data (text)` block shows the rows; the step log prints the snippet once. The README and the `charts` description in `action.yml` say a first run can show this and give the snippet: `permissions:` with `contents: write` (host the images) and `pull-requests: write` (the sticky comment) | Paste the permissions block; the next run hosts the image |
| Only one variant / no base | `chart skipped: nothing to compare` one line; head-only table as today | Add a second run or label |
| Variant missing for some benchmarks | Cell `—`; summary adds `· 3 incomplete`; Method note names the variant and count; chart slot empty with hollow marker, never a zero bar | Note names the missing variant so the user can fix the input |
| Reference missing for a benchmark | `⚠️ reference "X" absent from 2 benchmarks: A, B ...` (first 10, then "and N more"); row dropped from the ratio chart, kept in table with `—` ratios | Choose another `reference` |
| Row/series cap exceeded | Chart footer `+ 17 more not shown`; Markdown `_...and 17 more not shown (chart-rows: 30)_` (or `(max-rows: 20)` when that is the limiting key) and pointer to JSON. Relationship: effective chart cap = `min(chart-rows, max-rows)` when `max-rows` is set, the chart draws only rows the table displays, and `N more` = population minus shown in the footer, the notice and the text block, so equal caps give equal counts. Order: chart rows are the top N by \|delta\| (largest change first) while the table follows `sort`; every chart row label starts with its chart position (`1.`, `2.`) and the `Chart data (text)` block repeats it in a `#` column with the identical label, so a bar can be matched to its text row, and to the table by benchmark name | Raise the named key or read JSON |
| No samples | `distribution chart skipped: no samples in the input (use -count=10 or Criterion)`; delta charts unaffected | Message names how to get samples |
| Comment too long | Existing truncation behaviour; chart and verdict stay above the fold | Existing |
| Skipped metrics (existing) | Existing `⚠️` line, now also lists metrics excluded from charts | Existing |
| Several notices at once | Fixed order after the verdict, reference and gate lines: degradation, skipped metrics, reference-absent, invalid cells, chart notices. At most 5 inline; the rest collapse into `⚠️ N more notices (see Method)`, and the Method note lists every notice. A notice-only report has no verdict and starts with its first notice | Fix them top-down; the first line is the most consequential |
| N-way gate (`fail-on-regression` set) | `gate: fail-on-regression 10% vs quick: passed` or `FAILED (1 cell: heap on Sort/size=100)` after the reference line; job fails only on FAILED or an invalid cell. Many failures: `FAILED (7 cells: heap on Sort/size=100, heap on Sort/size=10, merge on Parse and 4 more; full list in Method)`: the first 3 cells in table order, each `variant on row` cut to 40 characters, the count always shown, the line at most 200 characters; the Method note lists every failing cell (first 50, then the JSON pointer) | Raise the threshold, fix the cell, or drop the flag |
| Data problem degraded (rule matched nothing, collision, axis absent, variant file missing/empty, reference lost) | One `⚠️` notice line naming count and example; the report shows what is left (matrix of remaining variants, two-way fallback, or a notice-only report with the sticky marker); job passes | Fix the named input, or set `dimensions-strict` to fail instead |
| Images not published (`publish-images: false`, or `svg-embed: false`) | `charts not embedded: publish-images is false; SVGs written to <svg-dir>` (only for `publish-images: false`) and the opened `Chart data (text)` block; with `svg-embed: false` no image and no block | Remove the opt-out |

**What the `Chart data (text)` block contains** (the fallback when no image is shown, and the accessible alternative on GitHub). Two-way: the chart's rows in chart order with columns `# | Benchmark | Change | ▰ bar`, plus `+ N more not shown`. N-way ranking chart: `# | Benchmark | <ref> (ref) | <variant> | ...` with each cell the ratio vs the reference in the table's cell style (`0.74x 🟢`, `1.05x ~`, `—` for Missing, `⚠️ invalid`), at most 8 variant columns, reference-absent rows left out and counted in one line. Distribution chart: `median (min-max, n)` per variant. Same rows, same cap, same order and same `#` as the chart (one `select_rows` function feeds both), it starts with `Rows are in chart order (largest change first); the table above follows the sort setting.`, and with several charts there is one block per chart titled `Chart data (text): <kind>`. It is opened (`<details open>`) whenever the image is not shown.

Mobile: the table is the primary representation on phones; docs state this (chart text scales to about 7 px at 375 px width). For a wide N-way table set `cell-format: ratio` (`0.74x 🟢` per cell) to cut each column to a few characters; the reference column keeps its absolute value, so absolute = ratio x reference, and the JSON has the rest.

---

## S2. SVG charts

### Spec (representative: delta bars, base vs head)

```
+--------------------------------------------------------------+  760 w (640 if mobile-tuned, see AC-S2-8)
| Benchmark change, head vs base           (title 13px, ink)    |
| 3 regressed · 1 improved · ref: base      (subtitle 12px)     |
|--------------------------------------------------------------|
| 1. BenchmarkSortLa…/impl=quick |      |=========▲ +30.0%     |   row 28px, bar 16px; label = chart position + name
| 2. BenchmarkParse/size=1000  | ▼ -12.4%=====|                |
| 3. BenchmarkHash             |      o  +0.3%  (gray, hollow)  |
|                              |<-- 0% line, 1px, labelled      |
|--------------------------------------------------------------|
| ▲ slower  ▼ faster  o no sig. change   largest change first · + 17 more not shown |   footer 24px
+--------------------------------------------------------------+
```

- Opaque rounded card, background `#fcfcfb`, ink `#0b0b0b`, secondary `#52514e` (readable on any page theme; `chart-theme: both` opt-in emits `-light`/`-dark` pairs via `<picture>`).
- Non-text contrast (WCAG 1.4.11, computed by script against `#fcfcfb`): `#d03b3b` 4.68:1 and `#0ca30c` 3.27:1 pass 3:1, so the delta palette stands; unchanged gray is `#8a8a86` (3.38:1); categorical `#2a78d6` 4.30, `#eb6834` 3.12, `#6250d6` 5.62 pass, `#1baf7a` 2.74, `#eda100` 2.11, `#e87ba4` 2.62 fail as fills, so series 3, 4 and 5 bars and markers carry a 1.5 px outline in secondary ink `#52514e` (7.73:1). A unit test recomputes this for both palettes; if the green ever falls under 3:1 it is darkened (`#0a7d0a`, 5.18:1), not waived.
- Delta bars: diverging, linear, clamped at +/-100% with a clip arrow. Colours: regressed critical `#d03b3b`, improved `#0ca30c`, unchanged gray `#8a8a86`; each paired with a shape (▲ regressed, ▼ improved, hollow circle unchanged). Value label always printed at the bar end (flips inside within 48 px of the edge).
- Ranking chart (N-way): ratio vs reference on log2 axis, 1.0x gridline labelled; series colour by variant order (categorical palette from research sec. 2), marker shape per series (circle, square, diamond, triangle, cross, star; shapes repeat for series 7 and 8, colour still differs), direct value labels mandatory, max 8 series.
- Distribution (when samples exist): strip plot with median tick, deterministic jitter by index, one lane per variant/side at every sample count (a box plot at n >= 20 is cut from v1).
- Label gutter 220 px, 12 px sans stack; width estimated 0.56 em/char (0.62 for uppercase/digits) with 10% margin; mid-ellipsis keeping the suffix; full name in row `<title>`.
- Determinism: fixed 1-2 decimal coordinates, no timestamps, ids derived from index. Names and labels XML-escaped.
- Accessibility: root `<svg role="img" aria-labelledby="t d">`, `<title id="t">` short sentence, `<desc id="d">` top-5 changes with values, reference and cap notice; per-row `<title>`. These serve the standalone file. **On GitHub the chart is an opaque `<img>`: its `role`, `<title>` and `<desc>` are not exposed there, so the Markdown alt text (<= 125 characters) and the `Chart data (text)` table are the accessible path, and the docs say so.**

### Flow
Chart is a static image: no interaction beyond hover tooltips (where the host serves it as a standalone document) and "open image in new tab". Reader exit: the table below carries every number. Standalone use (README/release notes): file is self-contained (no external refs), opaque background, `<title>`/`<desc>` present.

### States

| State | Chart behaviour | Exit path |
|---|---|---|
| Empty slot (missing impl) | `—` and hollow marker, never a zero bar | Table cell says `—` |
| Cap exceeded | Top N by abs(delta), stable tie-break by input order; footer `+ N more not shown` | Markdown notice + JSON |
| Label too long | Mid-ellipsis, full name on hover and in table | Table |
| Outlier > 100% | Bar clipped with arrow and true value label | Label shows real value |
| All unchanged | Chart still drawn in gray with verdict `no significant changes` (not blank) | None needed |
| Zero data rows | No file written; Markdown states `chart skipped: no comparable rows` | Fix inputs |
| Unsupported viewer strips SVG | Alt text and text-table `<details>` remain | Table |
| Forced colours or print | An SVG shown through `<img>` is not recoloured by forced-colors mode, and the card is an SVG `rect`, not a CSS background, so the chart prints and displays as drawn (opaque card, shapes, printed values). The Light card has no `<style>`, `@media` or system-colour keyword (every colour is a literal hex) | The text table is the accessible path; README says so |

---

## S3. CLI

### Samples

```
$ benchdiff --variant quick=a.txt --variant radix=b.txt --variant bubble=c.txt \
    --reference quick --charts ranking,delta,distribution --svg-dir out/
<markdown report on stdout, as today>
wrote out/ranking.svg
notice: delta chart skipped: delta is two-way; use ranking
notice: distribution chart skipped: no samples in the input (use -count=10 or Criterion)
$ echo $?
0
```

`benchdiff --help` lists every flag with default and allowed values, and under `--compare-by`/`--dimensions` states the zero-config extraction rules (`/key=value` path segments and `[k=v]` / `(k = v)` groups; nothing else). `benchdiff --print-dimensions` (same inputs as a run) prints one JSON object of the dimensions it detected, their values and benchmark counts, then exits 0 with no report, so a user can see what `--compare-by` can use before choosing it. `benchdiff --print-config` prints one JSON object (`{"config-file": ..., "config": {...}, "sources": {"threshold": "config"}}`) with the effective merged config and, in `sources`, the layer (`config` or `flag`) of every non-default value, then exits 0; the Method note uses the same labels (`threshold=3 (config)`). `--variant label=path` is repeatable (split at the first `=`; label `[A-Za-z0-9._-]`, max 40); `--base`/`--head` keep today's behaviour and cannot be combined with `--variant`. Single-file N-way instead of labels: `--compare-by impl` on sub-benchmark names. Charts and notices go to stderr, report to stdout (pipe-safe, existing contract).

### Flow

1. User runs with zero new flags: output unchanged (exit codes unchanged).
2. User adds `--charts delta --svg-dir out/`: report unchanged on stdout, files written, paths printed to stderr.
3. User replaces base/head with `--variant label=path` lines (or adds `--compare-by impl` to one file): matrix report.
4. Exit paths: every error prints the message and exits 2 (config) or the existing code for data/threshold failures; `--help` and `--print-config` always exit 0 and make no other writes.

### Errors

| Condition | Message (shape) | Exit |
|---|---|---|
| Invalid enum | `charts: "bars" is not valid; use none, delta, ranking, distribution (comma-separated)` | 2 |
| Unknown flag/key | `unknown key 'chart-row'; did you mean 'chart-rows'?` | 2 |
| Dimension rule matches nothing (data-dependent) | `dimension "impl": pattern "/impl=(\w+)" matched 0 of 48 benchmark names (example: "BenchmarkSort/size=100"). Check 'dimensions' or use labels (label=path).` Without `dimensions-strict` the rule is skipped and this is a notice in the report; with it, an error. | 0 (notice) / 2 (`dimensions-strict`) |
| Rule matches partially | Warning with count and 1 example; unmatched rows listed in report as incomplete | 0 |
| Names collide after pivot | `benchmark "X" maps to key K from both "A" and "B"` (same style as existing duplicate-in-two-files error). Without `dimensions-strict`: first kept, second dropped, notice names both. | 0 (notice) / 2 (`dimensions-strict`) |
| `--reference` not a known `--variant` label | `unknown reference 'slow'; valid: std, quick, radix` | 2 |
| Duplicate label | `label "quick" given twice (a.txt, b.txt)` | 2 |
| Label without path or path missing | `--variant quick=: path is empty` / existing file-not-found message with the label | 2 |
| Path without a valid label | `--variant: label 'results/size' is invalid (use [A-Za-z0-9._-], max 40) in 'results/size=100/a.txt'; expected label=path` | 2 |
| `--variant` combined with `--base`/`--head` | `--variant cannot be combined with --base/--head` | 2 |
| `compare-by` axis absent from some benchmarks | `1 benchmark has no 'impl' dimension (first 10: Setup)` (diagnostic; those benchmarks are excluded from the matrix) / `compare-by 'impl': no benchmark carries this dimension; available dimensions: size (2 values), group` if none (notice and fallback to the two-way or a notice-only report; exit 2 with `dimensions-strict`). With nothing detected the suffix reads `available dimensions: group only (default rules read /key=value segments and [k=v] groups; add a 'dimensions' rule or use labelled variants)`. `group-by` naming an absent dimension gets the same message | 0 / 0 (2 with `dimensions-strict`) |
| `compare-by measure` | `compare-by 'measure' is not an axis (units differ per measure); available dimensions: impl (3 values), size (2 values), group` | 2 |
| `fail-on-regression` in an N-way run with a defaulted reference | `fail-on-regression in an N-way run needs an explicit 'reference' (the baseline to protect); set 'reference'` | 2 |
| `reference` label absent from the axis values (`compare-by` mode) | `unknown reference 'slow'; valid: quick, heap` as a notice (notice-only report; a `fail-on-regression` gate is skipped, nothing was compared) / error under `dimensions-strict` | 0 / 2 |
| Action `variants` file missing, empty or zero metrics | `variant 'heap' produced no data (out/heap.txt); comparing quick and merge`; reference lost: `reference 'quick' produced no data; no comparison made` (notice-only report) | 0 (2 with `dimensions-strict`) |
| `--sort rank` on a two-way run | `--sort rank needs an N-way run (--variant or --compare-by)` | 2 |
| Chart kind does not fit the report kind | `notice: ranking chart skipped: ranking needs an N-way run` (stderr, report unaffected) | 0 |
| Glob under a label matches more than one file (Action) | `variants: label 'heap' matched 3 files; a label is one file` | job fails (config error) |
| Glob under a label matches zero files (Action) | `label 'heap': no file matches 'out/*heap*.txt'`; the variant is dropped with the notice `variant 'heap' produced no data (out/*heap*.txt); comparing quick and merge` (a missing, empty or zero-metric file behaves the same) | job passes with a warning (2 with `dimensions-strict`) |
| Config file lacks `version` (e.g. a results file passed by mistake) | `config: missing required key 'version'` | 2 |
| `--svg-dir` unwritable | `cannot write out/: permission denied; report printed without charts` | 2 on the CLI if the user explicitly asked for charts (opt-in contract), report still printed first; a warning only inside the Action |
| Non-finite numbers, bad numeric flags | Existing validation messages | 2 |

---

## S4. Action inputs

Sample:

```yaml
- uses: tstapler/benchdiff@v0
  with:
    variants: |
      quick=out/quick.txt
      radix=out/radix.txt
    reference: quick
    charts: ranking,distribution
    chart-rows: 30
```

Embedding defaults: a non-empty `charts` implies image embedding and hosting (`svg-embed` and `publish-images` both default on for the Action); set `svg-embed: false` to write files only, or `publish-images: false` to keep the chart as text with the files written. The CLI never embeds unless `--svg-embed` is given.

**Hosting side effects (disclosed, not hidden).** Embedding a chart in the comment and summary publishes the SVGs: (1) it needs `permissions: contents: write` on the job; (2) it creates or updates the branch `benchdiff-assets` in the same repository, one commit per run that has new bytes; (3) those commits are kept (benchdiff never prunes, and deleting the branch breaks the images in old comments, so to roll back unset `charts`); (4) the images are served from a public raw URL, so anyone who has the link can read them, which for a public repository is the same audience as the comment itself; (5) private repositories are never hosted: the report says `private repository (no anonymous image URL)` and nothing is written to the repository. The step log prints one line before the first write (`images: publishing to branch benchdiff-assets (needs contents: write; commits are kept; see README "Hosting images")`), the `action.yml` descriptions of `charts`, `svg-embed` and `publish-images` carry the same facts, and `publish-images: false` or `svg-embed: false` opts out.

**First run of `charts: delta` alone.** The zero-configuration path often hits the hosting-permission warning on its first run, because the default `GITHUB_TOKEN` of a newer repository or organization is read-only (INFERRED; older repositories may default to read/write). The report then carries the `permission` line (S1) instead of an image, and the job passes. The README ("Hosting images") and the `charts` description in `action.yml` say this up front and give the fix:

```yaml
permissions:
  contents: write        # host chart images on the benchdiff-assets branch
  pull-requests: write   # sticky comment
```

After adding it, the next run hosts the image. AC-S5-4 therefore reads "a useful chart with `charts: delta` alone, plus the permissions block when the token is read-only".

`variants` is one `label=path-or-glob` per line (newlines only; first `=` splits; one file per label; not combinable with `head`/`base`; ADR-007). For `variants` runs `head-json-path` is empty (no single head set to cache) and `base-format` is rejected; `format` applies to every file. Outputs added: `svg-dir`, `svg-paths` (newline list). Existing outputs, `report-path`, `json-path`, sticky marker unchanged.

Acceptance bullets:
- Every input name equals the CLI flag name without `--`; `action.yml` descriptions state allowed values and defaults.
- With no new inputs set, behaviour and outputs are identical to today.
- Config errors fail the step with `core.setFailed` using the same message text as the CLI, and the message names the offending input.
- Data/hosting problems emit `core.warning`, never fail the step (opt-in `fail-on-regression` semantics unchanged).
- The step log lists which SVGs were produced and which skipped with the reason (mirrors skipped-metrics surfacing).

Errors: same table as S3; plus `charts requested but token lacks permission to host images`: warning, fallback per S1 (exit path = documented permissions block in README).

---

## S5. Config file (`benchdiff.config.json`, JSON per ADR-001)

Sample (the tested starter example is `examples/benchdiff.config.json`; the schema is `benchdiff.config.schema.json`, generated from the key table):

```json
{
  "$schema": "https://raw.githubusercontent.com/tstapler/benchdiff/v0/benchdiff.config.schema.json",
  "version": 1,
  "reference": "quick",
  "compare-by": "impl",
  "charts": "delta,ranking",
  "chart-rows": 30,
  "threshold": 3,
  "thresholds": { "default": 3, "net/Parse": 10 },
  "dimensions": ["impl=/impl=(?P<impl>\\w+)"]
}
```

Acceptance bullets:
- Never auto-discovered: the file is read only when passed through the `config` input / `--config` (a committed results file named `benchdiff.json` must not change behaviour). A missing explicit path is an error.
- Precedence flags > inputs > file > defaults (inputs reach the binary as flags, so the binary distinguishes only `flag`, `config` and `default`); the report's Method note and `--print-config` show the source of each non-default value using those three labels.
- No comments (JSON); unknown keys are errors with nearest-name suggestion; enum errors list valid values; `version` is required. The one exception to "unknown keys are errors" is `$schema`: it is accepted (a string, ignored) so editors can validate the file against the published schema. Regex backslashes are written doubled in JSON (`"impl=/impl=(?P<impl>\\w+)"`); a single backslash is a JSON parse error that names the line and column, and the README says so next to the example.
- Table-shaped settings (per-metric `thresholds`, `dimensions` rules) live only here; inputs stay flat (`dimensions` input takes one rule per line).
- Parse errors report file and the serde line/column and the expected shape. The Action stages the file under its own base name, so the CLI and the Action print the same file name.

Errors: `benchdiff.config.json: line 7 column 12: invalid type: string "high", expected f64` (exit 2); unknown key; missing `version`; `config version 2 is newer than supported (1)`; regex invalid (`dimensions: invalid pattern: <regex error>`). If the WASM sandbox cannot read the file, `index.js` stages it as `/work/config/<base name>`; error text is identical either way. Table-shaped `thresholds` accept `group/name` and bare `group` keys (lookup `group/name` > `group` > default).

---

---

## UX acceptance criteria

Report (S1)
1. AC-S1-1: With no new options set, the report output is byte-identical to the previous release on every golden case.
2. AC-S1-2: In any report that has a verdict, a reviewer can state whether anything is worse from the bold verdict line alone, before any chart or table (the verdict is the first content line after the title; reference, gate and notice lines come after it). A notice-only report has no verdict and starts with its first notice.
3. AC-S1-3: Every chart image has alt text of <= 125 characters that names the verdict AND the top change (`Benchmark change, head vs base: 1 regressed, 1 improved; top: BenchmarkSort +30.0%`; a long name is middle-truncated before the verdict or the number is cut), and a collapsed text table with the same rows, order and `#` numbers follows it; for an N-way chart that table holds ratios vs the reference (see "What the `Chart data (text)` block contains").
4. AC-S1-4: Hosting failure shows a `⚠️` line stating the reason, the directory the SVGs are in (a path, not an artifact name) and the `actions/upload-artifact` hint (plus the permissions pointer for the `permission` reason), falls back to the Unicode bar, emits a warning annotation, and the job still passes.
5. AC-S1-5: Fork PR with a read-only token completes with table output and a warning naming the read-only token; no stack trace appears.
6. AC-S1-6: Missing variant, missing reference, capped rows and absent samples each produce one explicit line in the report (not silently omitted), naming counts and the next action.
7. AC-S1-7: In the N-way matrix the reference column is first and labelled `(ref)`, missing cells show `—`, and the best value per row is bold.
8. AC-S1-8: Ratios are shown as `0.74x`; "faster/slower" words appear only when |change| >= 2x; prose never uses higher/lower, and N-way prose never uses "regress" outside the gate line.
9. AC-S1-9: Matrix shows at most 8 variant columns; overflow produces a visible notice pointing to the JSON.
10. AC-S1-10: The `~` marker (and the other glyphs in use) has a one-line legend directly under the table, and the legend says that bold and Rank order the values in a row while only each variant against the reference is tested; the headline says `has the best geomean` and never "fastest overall" or "loses".
11. AC-S1-11: Notices appear after the verdict, reference and gate lines in the fixed order degradation, skipped metrics, reference-absent, invalid cells, chart; at most 5 render inline, the rest collapse into `⚠️ N more notices (see Method)` and the Method note lists all of them.
12. AC-S1-12: N-way regression is defined: a cell is slower/faster than the reference only on a significant, corrected result against the reference column; `fail-on-regression` in an N-way run requires an explicit `reference` (exit 2 otherwise), prints a gate line (with at most 3 cells named and the count always shown when many fail; the Method note lists them all), and without the flag the run cannot fail on cell contents.
13. AC-S1-13: `cell-format: ratio` renders compact `0.74x 🟢` cells (reference column keeps its absolute value) and the default `full` form is unchanged.
14. AC-S1-14: A first-time reader reads an N-way ratio cell and a two-way percent cell correctly (manual comprehension check MAN-6; a ratio misread by two or more of at least 3 readers reopens the ratio-vs-percent decision).

Charts (S2)
15. AC-S2-1: Same input produces byte-identical SVG on repeated runs and across native and WASM builds.
16. AC-S2-2: Chart is readable and unclipped in GitHub light and dark themes (verified by viewing a real PR comment and job summary).
17. AC-S2-3: Text contrast against the card is >= 4.5:1; series colour is never used for text.
18. AC-S2-4: Meaning is never colour-only: every bar has a shape marker and a printed value label; no region is distinguishable by colour alone.
19. AC-S2-5: The standalone SVG root has `role="img"`, `<title>` and `<desc>`, and each row has a `<title>` with the full benchmark name; because GitHub exposes none of that for an `<img>`, the Markdown alt text and the `Chart data (text)` table are documented as the accessible path on GitHub.
20. AC-S2-6: A benchmark name of 80 characters plus markup characters (`<`, `&`, `"`) renders truncated with an ellipsis, escaped, and without clipping.
21. AC-S2-7: With 100 changed benchmarks the chart shows <= 30 rows (default) and states `+ 70 more not shown` inside the SVG and in the Markdown, naming the limiting key (`chart-rows` or `max-rows`); the chart never shows a row the table omits when `max-rows` is set; chart rows are the top N by |delta| while the table follows `sort`, and chart labels and the text block share `#` numbers as the correlation aid.
22. AC-S2-8: Chart is legible at 375 px viewport width, or docs state the table is the primary view on phones; this is checked by viewing on a phone-width screenshot.
23. AC-S2-9: Missing data is drawn as an empty slot with `—`, never as a zero-length bar.
24. AC-S2-10: A standalone `.svg` opened in a browser and in a README shows an opaque background, title and description with no external references.

CLI (S3)
25. AC-S3-1: Running with no new flags produces the same stdout and exit code as the previous release.
26. AC-S3-2: A user can go from base-vs-head to a charted N-way report by replacing base/head with `--variant label=path` lines plus `--charts` and optionally `--svg-dir` (<= 3 distinct flags beyond the variants).
27. AC-S3-3: `--help` lists every new flag with default and allowed values; `--print-config` prints the effective config as JSON with the source (`config` or `flag`) of each non-default value and exits 0.
28. AC-S3-4: Every static configuration error exits 2 and names the offending key, a sample value, and the fix (valid values, nearest-name suggestion, or example name); data-dependent problems degrade with a notice naming the same, and exit 2 only under `dimensions-strict`.
29. AC-S3-5: The unmatched-dimension message includes matched/total counts and one example benchmark name.
30. AC-S3-6: Name-collision and duplicate-label errors name both conflicting sources.
31. AC-S3-7: Written SVG paths are printed on stderr; stdout remains the report only (pipe-safe).
32. AC-S3-8: An unwritable `--svg-dir` prints the report, then an error naming the directory and cause, exit 2 (CLI only; the Action never fails the job for this, AC-X-2).
33. AC-S3-9: Dimensions are discoverable: an absent `compare-by` or `group-by` dimension error ends with `available dimensions: ...`, the zero-config extraction rules are stated in `--help` and the README, and `--print-dimensions` lists what was detected.

Action and config (S4, S5)
34. AC-S4-1: Input names are identical to CLI flag names without `--` and to config keys (one vocabulary, with the documented exceptions `variants`/`--variant`, `$schema`, `thresholds`, `version`, `--print-config`, `--print-dimensions`); `action.yml` states allowed values and defaults.
35. AC-S4-2: Precedence flags > inputs > file > defaults is demonstrable: setting the same key in file, input and flag yields the higher layer's value, and the Method note names the source with the labels `default`, `config` or `flag` (an input is a flag to the binary, so the two share `flag`).
36. AC-S4-3: A config error fails the step with the same text as the CLI; a data or hosting problem only warns.
37. AC-S4-4: The step log lists SVGs produced and skipped (with reason), and the `svg-paths` output lists produced files.
38. AC-S4-5: Hosting side effects are disclosed before the user depends on them: `action.yml` descriptions, the README section "Hosting images" and one step-log line name `contents: write`, the `benchdiff-assets` branch and its per-run commit, retention, public-URL visibility and the private-repository rule, and give the `permissions:` snippet with the note that a first run on a read-only token shows the permission warning.
39. AC-S5-1: Unknown keys fail with a nearest-name suggestion; invalid enum values list the valid ones.
40. AC-S5-2: Parse errors give file, line and column; the file is strict JSON (no comments) with a required `version`; `$schema` is accepted.
41. AC-S5-3: Config is never auto-discovered (a results file named `benchdiff.json` changes nothing); a missing explicit `config` path is an error.
42. AC-S5-4: A user can enable a useful chart with `charts: delta` alone (zero other configuration): in the Action the chart is embedded and hosted by default; set `svg-embed: false` or `publish-images: false` to opt out. When the token is read-only the first run shows the `permission` line with a pointer to the `permissions:` snippet (README "Hosting images", `action.yml` description), keeps the opened text block, and passes.
43. AC-S5-5: A published JSON schema and a tested starter example exist; the example parses, runs and validates against the schema, and its doubled regex backslashes are explained.

Cross-cutting
44. AC-X-1: No dead ends: every warning or error in this document names its exit path (fix key, valid values, file location, or fallback view).
45. AC-X-2: In the Action, chart and hosting failures never fail the job and config errors always do. (CLI scope is different and listed in AC-S3-8: an explicitly requested but unwritable `--svg-dir` exits 2 after printing the report.)
46. AC-X-3: Adding `charts` and removing it again restores the original comment (sticky marker preserved, no stale image reference).

Charts (S2), added in triad round 2 (appended so the numbering above stays stable)
47. AC-S2-11: Bars, markers and lines needed to read a chart have at least 3:1 non-text contrast against the card, by fill or by an outline: the delta colours pass as computed (`#d03b3b` 4.68:1, `#0ca30c` 3.27:1, gray `#8a8a86` 3.38:1), series that fail as fills (`#1baf7a`, `#eda100`, `#e87ba4`) carry a `#52514e` outline, and a unit test recomputes both palettes; a failing green is darkened, not waived.
48. AC-S2-12: Forced-colors and print are a documented limit: the chart is an `<img>` that is not recoloured, the card is an SVG `rect`, the Light card has no `<style>`, `@media` or system-colour keyword, and the README names the text table as the accessible path.

## Flow coverage check (exit paths and error states)

| Flow | Exit path | Error states |
|---|---|---|
| Read report (S1) | Verdict line or table alone suffices | Covered (16 states) |
| View chart (S2) | Table below carries every number | Covered (8 states) |
| Run CLI (S3) | stdout report; exit 0/2; help, print-config and print-dimensions exit 0 | Covered (22 error-table rows) |
| Configure Action (S4) | Unset inputs to revert | Covered (shared with S3 plus hosting) |
| Author config (S5) | Delete file or omit the explicit `config` input | Covered (parse, type, key, version, regex) |

## Open items affecting copy (not blockers for UX)

- Multiple-comparison policy (Holm vs reference-only): changes only the Method-note sentence and whether `~` cells show adjusted p; pick in plan, UX needs one line either way.
- Config format is decided: JSON (ADR-001). Holm policy is decided (ADR-002): the Method note reads `Holm-adjusted across N-1 comparisons per benchmark; reference-only`.
- Real-PR verification of camo, dark mode and mobile legibility (AC-S2-2, AC-S2-8) is a post-implementation wall-clock blocker.
- The ratio-vs-delta unit mixing (N-way uses `0.74x`, two-way keeps `+30.0%`) is deliberate; it is now tested rather than assumed: manual comprehension check MAN-6 (AC-S1-14) in plan task 5.1.3a; a ratio misread by two or more of at least 3 readers reopens the decision (candidate fix: print `-26.0%` beside the ratio in `full` cells).
