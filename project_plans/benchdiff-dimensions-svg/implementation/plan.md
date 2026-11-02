# Implementation Plan: benchdiff-dimensions-svg

**Feature**: Generalize benchdiff metric identity to dimensions, add N-way comparison against a reference, render deterministic SVG charts, add a JSON config file, and deliver images to PR comments and job summaries via a swappable publisher. All new behaviour is opt-in; default output stays byte-identical.
**Date**: 2026-10-31
**Status**: Ready for implementation except gated items (see Unresolved Questions: 0.3.1b falsifier baseline, 0.2.1b, 3.1.2 width-table source, 5.1.1 wording). Revised after architecture and adversarial review, rounds 1 and 2, and after the phase-4 consistency check, pre-mortem and validation (round 3); all blockers resolved, see Review Resolutions at the end. Triad review round 1 (product / UX / engineering, NEEDS WORK) is resolved in the last table, "Review Resolutions (triad round 1)": N-way regression is now defined (plan Story 2.2.1/2.2.2, ADR-005 item 7), task 3.3.1a is split into 3.3.1a/b/c, every task body has a Files line, and 2.2.3a has its own wave. Triad review round 2 (product READY, UX READY, engineering NEEDS WORK) is resolved in the final table, "Review Resolutions (triad round 2)": method-note plumbing (task 1.2.1b) and every shared type now have a defining task before first use, the W12 call-site race is gone (additive `render_matrix_markdown_with`), tasks 1.1.2c, 2.2.1a, 2.2.2a, 4.2.1a and 4.2.3a are split into serial tasks, and the falsifier baseline (task 0.3.1a, gate G0) runs before the large build.
**ADRs**: [ADR-001 config JSON](../decisions/ADR-001-config-format-json.md), [ADR-002 Holm](../decisions/ADR-002-multiple-comparison-holm.md), [ADR-003 SVG hosting (Proposed, final after task 0.2.1)](../decisions/ADR-003-svg-hosting-orphan-branch.md), [ADR-004 dimension parse rules](../decisions/ADR-004-dimension-parse-rules.md), [ADR-005 reference semantics](../decisions/ADR-005-reference-semantics.md), [ADR-006 additive schema + matrix sibling (amended)](../decisions/ADR-006-additive-schema-and-matrix-sibling.md), [ADR-007 Action `variants` input](../decisions/ADR-007-action-variants-input.md)

Inputs: `../requirements.md`, `../research/{architecture,stack,features,pitfalls,ux,build-vs-buy}.md`. UX design doc: `../design/ux.md`.

---

## Step 0.5 Alternatives explored (CREATIVE pass)

| Approach | Strength | Weakness |
|---|---|---|
| A. Generalize `Report`/`Row`/`compare()` to N sides | One code path, "two-way is N=2" | Puts golden bytes and JSON schema 1 at risk; large diff in the hotspot files |
| B. **Sibling `MatrixReport` reusing the pair kernel via `compare_pair`; new modules for dims, config, svg** | Two-way path physically untouched, so byte-identity holds by construction; each piece testable alone | Some duplication of table rendering; two report shapes to maintain |
| C. Shell out: run benchdiff N-1 times and concatenate (what users do today) | Zero core change | No ranking, no geomean, noisy comments; rejected in requirements |

Chosen: **B** (ADR-006). Rejected A and C are recorded in Pattern Decisions.

## Technology validation

| Choice | Concern | Disposition |
|---|---|---|
| `regex-lite` 0.1.9 (new dep) | +55 KB wasm; ASCII-only classes | Accepted, ADR-004; budgeted in CI (task 0.1.2) |
| `serde_json` for config | no comments | Accepted, ADR-001 |
| Hand-written SVG writer | escaping and number-format ownership | Accepted; closed template, central escape, corpus tests (3.2.5) |
| Git Data REST API from zero-dep `index.js` | needs `contents: write`; ref races | Accepted with retry and fallback, ADR-003 |
| `toml`, `regex`, `svg`, `plotters`, `charts-rs`, YAML crates | size, fonts, wasm-bindgen, deprecation | Rejected (research/stack.md, build-vs-buy.md) |

## Success metric 3 as a support matrix

Requirements metric 3 ("SVG renders in comment, job summary and standalone file") cannot hold for every repository class: the hosted-image mechanism needs an anonymous raw URL and a write token (ADR-003). It is restated here per repo class; task 5.1.3a verifies each row (or records an explicit waiver with the reason). Every class gets the standalone files (`svg-dir`, `svg-paths`) and a text rendering of the chart in the Markdown body, so no class loses the information; "hosted inline image" is the only column that varies. Expected outputs are the design target; the INFERRED GitHub behaviours are settled by spike 0.2.1.

| Repo class / event | Comment | Job summary | Standalone files | Verified by |
|---|---|---|---|---|
| Public, same-repo PR, `contents: write` | hosted SVG inline + collapsed `Chart data (text)` | hosted SVG inline | yes | 5.1.3a run A (the G1 observation) |
| Public, same-repo PR, token without `contents: write` | table + in-body notice (`permission`) + open `Chart data (text)` | same | yes | 5.1.3a run B (`permissions: contents: read`) |
| Public, fork PR (read-only token) | usually no comment can be written by a fork token (INFERRED); if written, table + in-body notice (`read-only token on a fork PR`) + open text chart | same notice and open text chart (the summary is the main surface) | yes | 5.1.3a run C if a fork is available, else waived and recorded |
| Private repo | in-body notice (`private`) + open text chart; inline image only if spike 0.2.1a shows a rung that works with `GITHUB_TOKEN` | same | yes | 5.1.3a run D on a private repo (owner) |
| GHES | in-body notice (`unsupported-host`) + open text chart | same | yes | unit test only (`publishImages` with a GHES server URL); live run waived, no GHES available |
| Public, non-PR event (`push`, `workflow_dispatch`, `release`) | no comment (as today) | hosted SVG inline | yes | 5.1.3a run E (`workflow_dispatch`) |

The in-body notice is part of the Markdown (sticky comment and summary), not only a log line. The ladder behind this matrix is in ADR-003.
---

## Domain Glossary

| Term | Definition | Notes |
|---|---|---|
| `Metric` | One measured quantity: `(group, name, measure)` + unit, `better`, samples | Existing; gains optional `dims`; `name` never rewritten |
| `MetricSet` | Parsed collection of `Metric`s from one input | Existing; schema 1 |
| `Dims` | `BTreeMap<String,String>` of dimension name -> value on a `Metric` | Additive, `skip_serializing_if` empty (ADR-006) |
| `DimensionRule` | User rule `"<dim>=<regex-lite pattern>"` with named capture groups | ADR-004; length-capped |
| `Residual` | Newtype: benchmark name after extracted dimension spans are removed | Lives inside `Dimensioned` paired with its metric, not on `Metric`; projection clones a metric with `name = residual`, the source set keeps original names |
| `Dimensioned` | Construct-only (private fields in `dimensioned.rs`, task 1.1.2c; built solely by `dimensions::apply` (1.1.2d) or `Dimensioned::identity` (1.1.2c) through a crate-private `from_parts`) holding `Vec<(Metric, Residual)>` plus per-variant `skipped` keys; accessor `entries() -> impl Iterator<Item=(&Metric,&Residual)>`; input of `project_by_axis` | Pairing makes a desynced filter/reorder unrepresentable. Labelled-file variants get identity residuals (`residual = name`) when no rules are given |
| `Axis` | The dimension (or `run`, or the built-in pseudo-dimension `group`) that varies across columns (`compare-by`) | `run` = today's base/head; `measure` is deliberately NOT an axis (requirements.md scope decision) |
| `RowKey` | Row identity: `(group, residual, measure, non-axis dims)`; `group-by` selects which non-axis dims participate (default: all) | Same as ADR-004 item 5. Row label = `residual` + ` [k=v, ...]` for the participating non-axis dims in key order |
| `Variant` | `{ label, set: MetricSet }`: one column of an N-way comparison | **Canonical term** in docs, flags, messages and this plan; "implementation" appears only in examples where the axis is `impl`. From `label=path` files or by splitting on the axis dim. Order = input order: `--variant` occurrence order, `variants` line order, or, when split from names, first encounter walking `MetricSet.metrics` (files in the order given, glob matches sorted by path); a tie cannot occur within one pass and would break by label, byte-wise |
| `VariantLabel` | Validated label `^[A-Za-z0-9._-]{1,40}$` (no `/`, no `=`) | Newtype, not raw string; same grammar in CLI `--variant`, Action `variants`, config |
| `Reference` | The fixed column all cells are compared against; default first variant | ADR-005; enum `Reference { First, Label(VariantLabel) }`; `Label` is an *explicit* reference (the declared baseline, required by the regression gate), `First` is the defaulted one |
| `Cell` | Sum `Missing` / `Present(PresentCell)`; `PresentCell { value, samples (serde-skip), vs_reference: Comparison }` | Flag combinations (direction mismatch, absent reference, Invalid) are `Comparison` variants, not booleans. `Invalid` source: the reference has the metric, the variant lacks it but the variant's `skipped` list names it (see 2.1.3); beyond the importer's `MAX_SKIPPED` cap the cell is `Missing` |
| `Comparison` | Sum `Reference` / `Compared{verdict, delta, method, p_value, p_adjusted}` / `Incomparable(Reason)` / `DirectionMismatch` / `Invalid(Reason)` | Holm rewrites `Compared` in place (downgrade only). The human method text `mann-whitney, Holm-adjusted p=0.06 (m=2)` is derived at render time from `method`, `p_adjusted` and the row's `m`; the `Method` enum gains no Holm variant |
| `Samples` | `#[serde(skip)]` side channel `{ base: Vec<f64>, head: Vec<f64> }` on `Row` and `PresentCell`, filled only when `Options.keep_samples` | `base` is stored already multiplied by the unit `scale` (`units::factor`, the same `base_scale` `evidence` uses), i.e. in the head unit, so both lanes share one scale. Default JSON and default memory unchanged; only chart requests set it |
| `Thresholds` | `Options.threshold_pct` (default) plus `Options.threshold_overrides: BTreeMap<String,f64>` keyed `group/name` or bare `group`; `Options::threshold_for(&Metric)` | Lookup: exact `group/name` (or `name` when group empty) > `group` (non-empty group only) > default. In matrix runs the metric handed to the kernel is the projected clone, so the key is `group/residual`, with `group` = the reference variant's group under `compare-by=group` |
| `MatrixRow` | One benchmark (row key) across all variants: cells + `ranking` | Sibling of `Row` |
| `MatrixReport` | Summary (incl. per-variant skipped/invalid), reference, columns, rows, geomeans (per measure), counts | Emitted under JSON key `matrix` only when requested |
| `Report` / `Row` | Existing two-way report and row | Untouched |
| `Verdict` | Existing enum Regressed/Improved/Unchanged/New/Removed/Incomparable/Invalid | Reused for cells. In a matrix cell `Regressed`/`Improved` mean *significantly worse/better than the reference column* (rendered "slower/faster than reference"), never a change over time; the enum and the Action output names stay for compatibility (ADR-005 item 7) |
| `Correction` | Enum `Holm` / `None` | ADR-002. **Defined by 2.1.3a** in `matrix.rs` (so `MatrixOptions` compiles in its own wave); the Holm logic is 2.1.4a |
| `PAdjusted` | Holm-adjusted p-value; raw `p_value` retained | Newtype over f64 in [0,1]. **Type and `Compared.p_adjusted` field defined by 2.1.3a**; filled by 2.1.4a |
| `Geomean` | Orientation-aware geometric mean of ratios vs reference, per `(measure, unit)` family. Two views: per-variant (over that variant's rows shared with the reference, `n_used/n_total`) and `headline` (over the common row set: rows where the reference and every compared variant are Present and not Invalid) | Zero/neg/non-finite excluded and counted; families never mixed; the headline sentence uses only the common-set view, and says `not comparable` when the common set is under 3 rows or under half the rows |
| `Ranking` | Order of present cells in a row by direction-aware value | Separate from sort and verdict |
| `ChartKind` | Enum `Delta`, `Ranking`, `Distribution` | `charts` input; default none |
| `ChartTheme` | Enum `Light`, `Dark`, `Both` | `chart-theme` |
| `SvgOptions` | `{ kinds, rows: ChartRows, width: ChartWidth, theme, max_series, embed: EmbedMode, publish_images: bool }` | `ChartRows` (1..=200) / `ChartWidth` (480..=1600) are range-checked newtypes; defaults 30 rows, width 760 (research/ux.md). **Effective chart cap** = `min(chart-rows, max-rows)` when `max-rows` is set, else `chart-rows`; when `max-rows` is set the chart draws only rows the table displays (see 3.2.4). `EmbedMode` is `On`/`Off`, resolved from `svg-embed` `true|false|auto`; `auto` is a default-layer value (never an override): embed iff `charts` is non-empty and no layer set a concrete value. CLI and a bare config default to `Off`; the Action passes `auto` when its input is unset. `publish_images` default `true`, consulted only when embedding |
| `SvgInput` | `Pair(&Report)` or `Matrix(&MatrixReport)` | Pure input to renderer; samples travel on the report structs via `Samples`. **Defined by 3.2.4a** (W9, the first task that needs one input type for both report kinds; it also defines `select_rows`, the single row-selection function the charts and the text table share). Chart functions in W7-W8 take `&Report` / `&MatrixReport` directly |
| `SvgFile` | `{ name, svg, alt, skipped: Option<SkipReason> }` | Output of `render_svgs`. **Defined by 3.3.1a** (W6) with `SkipReason` and `TextRow`; every chart function returns `Result<SvgFile, SkipReason>` (`Err` = no file) |
| `SkipReason` | Enum: `NothingToCompare`, `NoSamples`, `CapExceeded{shown,total}`, `Empty`, `NotApplicable{kind}` | Surfaced in report and logs; `NotApplicable` = chart kind does not fit the report kind (3.2.4 table, dispatched by 3.3.1b). **Defined by 3.3.1a** |
| `TextRow` | `{ index, label, cells: Vec<String>, bar: Option<String> }`: one row of the `Chart data (text)` table, `index` = the `#` shown in the chart label and the table | **Defined by 3.3.1a**; filled by `chart_text_table` (3.3.2a) |
| `MatrixSummary` | The matrix analogue of `Summary`: `title, tag, threshold_pct, min_abs, alpha, use_stats, only_changed, max_rows, fail_on_regression_pct, correction, columns_cap, cell_format, sort, reference {label, explicit}`, per-variant skipped lists and the method facts | Copied from `Options` + `MatrixOptions` by `compare_matrix` (2.1.3a), so `render_matrix_markdown(&MatrixReport)` and `render_svgs(SvgInput::Matrix(..))` read the report only and need no second options argument |
| `MethodFacts` | Two optional fields on the public `Summary` (`threshold_overrides: BTreeMap<String,f64>`, `config_sources: Vec<ConfigSource>`, `ConfigSource { key, value, source }`), both `#[serde(skip_serializing_if = "...is_empty")]`, and the `render_method` sentences that print them only when non-empty | **Defined and rendered by 1.2.1b** (W2); filled by 1.2.5a (overrides, W3) and 1.2.3a (sources, W5); consumed by `markdown_matrix.rs`. Default JSON and Markdown bytes are unchanged |
| `SvgWriter` | Internal string builder: escape, number format, element push | Closed template: no script/foreignObject/links |
| `TextMetrics` | Static per-character advance-width table + truncation | No font access in WASM |
| `ImagePlaceholder` | Token `benchdiff-svg:NAME.svg` (constant `PLACEHOLDER_SCHEME`, mirrored in JS and asserted equal in a test) inside `![alt](...)` or a `<picture>` block | Rewritten or stripped by `index.js` |
| `RenderExtras` | `{ placeholders: Vec<..>, ... }` passed to `render_markdown_with` / `render_matrix_markdown_with` | `render_markdown(&Report)` stays as a thin wrapper over `render_markdown_with(report, &RenderExtras::default())`; likewise `render_matrix_markdown(&MatrixReport)` (2.2.1a, signature frozen) wraps `render_matrix_markdown_with(report, &RenderExtras::default())` (3.3.1c). Additive: no existing call site changes in the wave that adds the `_with` form |
| `PublishResult` | `{ urls }` or `{ error, reason }` from `publishImages` | Seam, ADR-003 |
| `HostedImageUrl` | Immutable `raw.githubusercontent.com/<o>/<r>/<commit-sha>/<path>` | Content-hash path |
| `Config` | All-`Option` file model (kebab-case keys, required `version`, optional ignored `$schema` string) | `deny_unknown_fields`; loaded only via explicit `config` input / `--config` (never auto-discovered) |
| `OptionsOverlay` | All-`Option` partial of `Options`; `apply(&mut Options)` | file overlay then flag overlay; `MatrixOptions` and `SvgOptions` get their own overlays so `Options` (pair kernel) does not grow |
| `MatrixOptions` | `{ compare_by, group_by, dimensions, strict, reference, correction, columns_cap, cell_format, sort: MatrixSort }` | Separate from `Options`; lives in `matrix.rs`, defined by 2.1.3a together with `CellFormat`, `Correction` and `MatrixSort`. `compare_matrix(variants, &Options, &MatrixOptions) -> MatrixReport`: the shared settings (`threshold_pct`, `min_abs`, `alpha`, `use_stats`, `threshold_overrides`, `keep_samples`, `title`, `tag`, `only_changed`, `max_rows`, `fail_on_regression_pct`) come from `Options`, the matrix-specific ones from `MatrixOptions`, and both are copied into `MatrixSummary`. `cell_format` (`CellFormat`, key/flag/input `cell-format`) selects the table cell shape. `strict` (`dimensions-strict`) turns data-dependent problems from a visible notice into an exit-2 error (Risk Control). `columns_cap` is the 8-column cap (AC-S1-9) |
| `VariantsInput` | Action input: newline-separated `label=path-or-glob` lines | ADR-007; one label = exactly one file. Plural because a GitHub input is one string holding a list; each line becomes one occurrence of the singular, repeatable CLI flag `--variant` (as repeated `--dimensions`). Not a config key |
| `RegressionGate` | `fail-on-regression` in a matrix run: trips when a non-reference cell is `Regressed` over the threshold vs an **explicit** reference (post-Holm), or any cell is `Invalid` | Defined once in ADR-005 item 7. Without `fail-on-regression` `failed` is false. A defaulted (first-listed) reference is rejected with a static exit-2 error, so a gate never protects an arbitrary column |
| `Notice` | One `⚠️` line: `{ class: NoticeClass, text }` with `NoticeClass` in the fixed order degradation, skipped, reference-absent, invalid, chart | **Types defined by 1.1.1c** (`notice.rs`, derives `Ord` in class order). Produced by 1.1.2d (degradation), 2.1.2a, 2.2.2b and 3.3.1b/c (chart); rendered by `render_notices` (2.2.1b): fixed order, first 5 inline, overflow line, full list in the Method note; always after the verdict line (ux.md S1) |
| `CellFormat` | Enum `Full` (default: `1210 ns (0.74x) 🟢`) / `Ratio` (`0.74x 🟢`) | Compact matrix cells for phones; the reference column keeps its absolute value in both. **Defined by 2.1.3a** (with `MatrixOptions`); flag/key 2.2.2a; rendered by 2.2.1a |
| `DimensionSummary` | `{ name, values (first-seen, first 20), benchmarks }` per detected dimension, plus the pseudo-dimension `group` | Built by `Dimensioned::dimension_summary` (1.1.2c); feeds the `available dimensions: ...` suffix and `--print-dimensions` |

Glossary term count: 46.

---

## Pattern Decisions

| Component | Pattern Chosen | Source | Alternative Rejected | Reason |
|---|---|---|---|---|
| N-way compare | Sibling type + reuse pair kernel (`compare_pair`) | Fowler: Transaction Script (pure fn pipeline) | Generalize `Report`/`Row` to N sides (approach A) | Puts golden bytes and schema 1 at risk |
| N-way vs "run N-1 times" | Single matrix report | n/a | Concatenated pair runs (approach C) | No ranking/geomean; noisy |
| Pair reuse | Seam = `pub(crate) fn compare_pair` | Feathers seam | Duplicating pair logic | Inherits Mann-Whitney/interval/threshold precedence exactly |
| Dimension extraction | Strategy: default extractor, user-rule extractor, label extractor behind `fn extract(&mut MetricSet, &Rules)` | GoF Strategy | One heuristic | Ambiguity across Go/JMH/Criterion (ADR-004) |
| Config layering | Overlay objects applied in order (Builder-like) | Fowler: Special Case/Overlay | Flags mutate `Options` directly | "Flag equals default" ambiguity (ADR-001) |
| Image publishing | Adapter behind `publishImages()` | GoF Adapter | Inline `fetch` calls in `main()` | Mechanism unverified; must be swappable (ADR-003) |
| Image URL injection | Placeholder + post-render rewrite | Fowler: Template View | Core knows URLs | Keeps core I/O-free; marker-first invariant intact |
| SVG generation | Closed template builder (`SvgWriter`), no user text in attribute names/ids/style | Secure-by-construction | `svg` crate / plotters / charts-rs | No chart help, fonts, wasm-bindgen; escaping surface must be single |
| `VariantLabel`, `PAdjusted`, `Reference` | Newtypes / sum types | type-driven-design | Raw `String`/`f64` | Prevent mixing label with name; clamp p in [0,1] |
| `Cell` | Sum type `Present/Missing/Incomparable` | type-driven-design | `Option<f64>` + flags | Missing must never render as 0 |
| `ChartKind`, `ChartTheme`, `Correction` | Enums with exhaustive `match` | type-driven-design | strings | Compiler-enforced; enum errors list valid values |
| Config unknown keys | Fail-fast with nearest-name suggestion | n/a | Silent ignore | UX requirement (research/ux.md section 6) |
| Per-metric thresholds | `Options::threshold_for(&Metric)` consulted by the kernel; default path identical | Feathers: preserve behaviour, then extend | Silently parsed-but-unused config key | Fail-fast principle; golden-protected (task 1.2.5a) |
| Reference direction | Matrix compares `better` of reference and variant first; a mismatch short-circuits to `DirectionMismatch` before the kernel is reached. On a match the two values are equal, so the kernel runs on the unmodified variant metric (no clone, no overwrite); ranking/geomean orientation read the reference's `better` | n/a | Changing the kernel to read the reference's `better`; cloning with `better` overwritten (a no-op after the check) | Kernel change would alter two-way bytes; the equality check makes the overwrite redundant, and `compare_pair` stays a pure rename |
| Holm | Post-pass over a row's cells: downgrade-only; method text derived at render time; counts, geomean and `matrix_failed` computed after the pass | n/a | Re-running the kernel with adjusted p | Verdict is final inside the kernel; downgrade needs no re-run and changes failure status, so aggregates must follow it (ADR-002) |
| Chart samples | `#[serde(skip)]` `Samples` side channel on `Row`/`PresentCell`, filled only when `keep_samples` | n/a | Passing source `MetricSet`s to `render_svgs`; serializing samples | Keeps `SvgInput` a pure function of the report and default JSON byte-identical |
| Report API growth | `render_markdown_with(&Report, &RenderExtras)`; `render_markdown` stays | Open/closed | Adding a parameter to `render_markdown` | SemVer break; byte-identity by construction |
| Option structs | `Options` stays in `compare.rs` for the pair kernel only; `MatrixOptions`, `SvgOptions` separate, each with an overlay | SRP | One growing `Options` in the hotspot file | Avoids moving `Options` mid-flight (file-conflict hazard) and kernel signature growth |
| N-way regression | The reference is the declared baseline; `Regressed` = significantly worse than the reference; the gate (`fail-on-regression`) needs an explicit `reference` | ADR-005 item 7 | Applying `fail-on-regression` to a defaulted first-listed column; a separate new gate flag | A defaulted column is arbitrary, so failing a job on it is a hidden policy; reusing the existing flag keeps one vocabulary |
| Domain model complexity | Transaction Script style pure functions | PoEAA | Domain Model / Repository | No persistence; stateless pipeline |

---

## Tech Debt Disposition

| Area | Existing Issue | Disposition | Justification |
|---|---|---|---|
| `crates/benchdiff-core/src/compare.rs` (364 prod lines) | Single global `threshold_pct` read inside `changed_row`; no sample access for charts | **Refactor-first** (tasks 1.2.5a, 1.2.5b, before the seam) | Per-metric thresholds and `Samples` need the kernel; both are golden-protected and default-neutral (`threshold_overrides` empty, `keep_samples` false). The method note lists active overrides only when non-empty, so default bytes are unchanged |
| `crates/benchdiff-core/src/compare.rs` pair kernel | Two-sided `Row`; pair kernel private | **Isolate via seam** (task 2.1.1a, rename + visibility only) | Kernel is already a pure fn of two `Metric`s; new code lands in new files |
| `crates/benchdiff-core/src/markdown.rs` (375 prod lines) | Helpers private; would grow if matrix added in place | **Isolate via seam** (task 2.1.1a `pub(crate)` helpers; matrix in `markdown_matrix.rs`; placeholder via `render_markdown_with`, 3.3.1c) | `render_groups` untouched; `render_markdown` signature unchanged |
| `crates/benchdiff/src/main.rs` `parse_args` | Flags mutate `Options` directly; hand-rolled parser | **Refactor-first** (story 1.2.1, before any new flag) | Adding ~10 flags to this pattern would worsen it and breaks config layering |
| `index.js` `buildArgs` + `action.yml` defaults | Defaults make inputs un-unsettable | **Refactor-first** (task 1.2.4) | Required for file < inputs layering |
| `test/golden.test.js` | Proves only wasm == native | **Refactor-first** (story 0.1.1) | Byte-identity currently unguarded |
| `index.js` growth | Zero-dep single file | **Isolate via seam**: new `lib/images.js`; `module.exports` re-exports retained | Keeps `main: index.js`, tests' imports working |

---

## Migration Plan

No database. Data-format change only:
- **Schema**: stays 1; `dims` additive and omitted when empty (ADR-006). No migration file.
- **Reversibility**: fully reversible; older readers ignore `dims` and `matrix`.
- **Artifact**: `dist/benchdiff.wasm` is rebuilt and committed ONLY by the integration tasks (a wasm-committing task never shares a wave with a task that edits Rust source): 2.2.3a (wave 14, alone, after the matrix/Markdown work and 3.3.1b/3.3.3a of W13), 4.2.2a (wave 19, charts; it shares the wave only with tasks that change no Rust source), and 5.1.2a (release, W20). Every other task runs native tests and may build a local, uncommitted wasm; nobody else commits `dist/benchdiff.wasm`, so it cannot conflict. Caveat verified in the `Makefile`: `make golden` depends on `make wasm`, which copies the fresh build over `dist/benchdiff.wasm` in the working tree, so any task that runs `make golden` ends with a modified `dist/benchdiff.wasm`; non-integration tasks must `git checkout -- dist/benchdiff.wasm` before committing (each task's PR check lists only its own `Files`). A CI job `wasm-fresh` (task 0.1.2a) rebuilds the wasm from source and fails if it differs from the committed file; it runs on every PR that touches `dist/` and on release tags, so each of the three integration tasks must pass it before it commits (between integration waves the committed wasm legitimately lags the source, so the job is path-gated, not run on every PR).
- **Rollback**: pin previous release tag; `@v0` floating tag moves only after task 5.1.3.

## Observability Plan
- **Logs**: `index.js` logs one line per decision: `charts: requested=delta,ranking produced=2 skipped=1 (reason)`, `images: published 2 files to benchdiff-assets@<sha>` or `images: not hosted (fork PR); falling back to Markdown`. Errors log reason + context, never tokens.
- **Report surfacing** (mirrors "skipped metrics"): the Markdown names SVGs produced and skipped with reason (`NothingToCompare`, `NoSamples`, `CapExceeded`, `NotApplicable`, hosting failure), dimension diagnostics (unmatched names, first 10), data-dependent notices (zero-match rule, collision, absent axis, dropped variant; see Risk Control), reference-absent rows, effective non-default config sources (labels `default|config|flag`).
- **Metrics**: none (batch CLI). Size guard instead: CI step fails if `dist/benchdiff.wasm` exceeds the budget (task 0.1.2); per-SVG size warning above 100 KB.
- **Alerts**: no new alerts required.

## Risk Control
- **Feature flag**: not a flag but opt-in defaults: `charts: none`, no `compare-by`/`group-by`/`dimensions`/`variants`, no config file is ever auto-discovered (decision in 1.2.4: config is read only from an explicit `config` input / `--config`, because a committed canonical results file named `benchdiff.json` would otherwise hit `deny_unknown_fields` and fail a workflow that changed nothing). Golden tests guard the default path.
- **Rollback**: unset new inputs (unsetting `charts` restores the original comment, AC-X-3), or pin the previous release tag. Do NOT delete or force-push branch `benchdiff-assets` to roll back: hosted URLs are commit-pinned and historical comments depend on those commits staying reachable (INFERRED: unreachable commits are eventually garbage-collected), so deleting it breaks every image in old comments. Retention rule in Story 4.1.2.
- **Staged rollout**: land on a `v0.x` minor tag, verify on this repo's own PR (task 5.1.3), then move `@v0`.
- **Failure policy** (two classes; the old wording "config problems fail fast" lumped data-dependent problems in with static ones, so one renamed benchmark or crashed upstream job turned every PR red with no comment):
  - **Static config errors, always exit 2** (the Action fails the step): unknown key, bad enum, bad number, invalid regex, rule without a named group, bad label grammar, duplicate label, `version` problems, missing explicit `config`/`--variant` path on the CLI, `--sort rank` on a two-way run, `--head-json` with `--variant`, `--variant` with `--base/--head`, `variants` with `head/base`, `--reference` not among explicitly given `--variant` labels, `compare-by measure`, and `fail-on-regression` in an N-way run without an explicit `reference` (`fail-on-regression in an N-way run needs an explicit 'reference' (the baseline to protect); set 'reference'`).
  - **Data-dependent problems, default: visible notice, never a failed job**: a rule that matches 0 names, a collision after residual extraction, a `compare-by` axis no benchmark carries, a `variants` file that is missing, empty or yields 0 metrics (job crashed), a `reference` label absent from the axis values, a reference variant that produced no data. The report degrades in a fixed order: drop the offending rule or variant and render what is left (a matrix needs >= 2 variants); fall back to the two-way report if `base`/`head` were given; otherwise a notice-only report (sticky marker, title and the notice) is still posted. `dimensions-strict` (Action input, CLI flag, config key) opts in to fail: each of these becomes the exit-2 error text it had before this split.
  - Hosting failure never fails the job (ADR-003). In the Action, chart and hosting failures never fail the job and config errors always do (AC-X-2, scoped to the Action and hosting). The CLI is different by design: an explicitly requested `--charts` with an unwritable `--svg-dir` prints the report, then exits 2 (AC-S3-8, Story 3.3.1b).
- A matrix run where no comparison could be made (notice-only report) skips the regression gate and passes; `dimensions-strict` turns that into a failure.

## Unresolved Questions
- [ ] Do SHA-pinned raw URLs render in a PR comment AND a step summary (Camo `Content-Type`/`nosniff`, dark mode, mobile), and which of the other embed variants (`data:` URI in the summary, repo-tracked path, `github.com/.../raw/`) work for private-repo and fork-PR runs? The widened spike matrix is in 0.2.1a. — blocks Stories 4.1.2, 4.2.1 and ADR-003 finalization — owner: Tyler (task 0.2.1b real PR runs; scheduled as the explicit human gate G1 in the wave table)
  - G1 result: SHA-pinned raw URL renders in a public same-repo PR comment (light and dark, `image/svg+xml`, `nosniff`; observed on one run, https://github.com/tstapler/benchdiff/pull/3#issuecomment-6567806842). Still open: step summary (UNVERIFIED), private-repo and fork-PR runs (untested). See ADR-003.
- [ ] Does `prefers-color-scheme` inside `<img>` SVG follow OS or GitHub theme? Decides default `chart-theme` (`light` card vs `auto`) — blocks Story 3.1.3 palette finalization (default stays opaque light card until answered) — owner: Tyler (0.2.1b)
  - G1 result: it followed the GitHub comment theme in the owner's observation (one run, public PR); a transparent media-query-adaptive SVG is viable as the default `chart-theme`. See ADR-003.
- [ ] Confirm-only: `node:wasi` `create_dir_all` under the preopen. Already mitigated (JS creates `svg/` before invoking wasm, 4.2.1a); not a blocker — owner: implementer
- [ ] Is the 65,536 comment limit characters or bytes with non-ASCII names? Plan stays bytes-conservative (60,000 existing) — blocks nothing; test in 5.1.3 — owner: Tyler
- [ ] Per-character width table source (public Arial/Helvetica AFM advances) — blocks Story 3.1.2; implementer must cite the source — owner: implementer
- [ ] Is `contents: write` acceptable to document as an extra permission for hosted images? — blocks Story 5.1.1 wording — owner: Tyler
- [ ] Private-repo inline images: no verified anonymous path with `GITHUB_TOKEN` only; v1 falls back to the open text chart (ADR-003 ladder). The `github.com/<o>/<r>/raw/...` and `data:` URI variants are now part of the 0.2.1a matrix (run in a private repo), so a working rung can be adopted without a redesign — non-blocking — owner: Tyler
- [x] `dist/benchdiff.wasm` reproducibility: resolved in W1 by task 0.1.2a (build twice in clean checkouts and compare; if not byte-identical, the `wasm-fresh` CI job compares exported functions plus size and the cause is recorded in ADR-004's note). No longer deferred — owner: implementer
- [ ] Does importer-level dim extraction (JMH `@Param`, Criterion ids, Go `procs`) belong in v1? Plan: NOT in v1 (follow-up); the Go importer already strips `-N`, so `procs` is not a dim in v1; user confirmation welcome — non-blocking — owner: Tyler
- [ ] Do reviewers read ratios (`0.74x`) in N-way tables and percent (`-26.0%`) in two-way tables correctly? Manual comprehension check MAN-6 in validation.md, run in 5.1.3a with at least 3 people (Tyler plus two others, or waived and recorded) — blocks nothing; a failure triggers the revisit noted in ux.md — owner: Tyler
- [ ] Demand: is anyone besides the owner's own repos asking for N-way plus charts? Unknown today (requirements.md "Demand evidence"); the plan de-risks by running the falsifier baseline first (gate G0, task 0.3.1b), shipping the N-way table before any hosting work (waves W10-W14), gate G1 before any hosting work, and a no-telemetry adoption proxy after release (requirements.md "Adoption proxy": count of the owner's repos whose workflows set `variants` or `charts` at release+30 and +90 days, manual, requirement OM-2) — owner: Tyler
- [ ] Falsifier baseline (gate G0): does today's pairwise output on `testdata/nway` already answer Q1-Q3 correctly in under 60 seconds for at least 2 of 3 readers? If yes, the problem is smaller than assumed and the plan stops after Phase 1 (dimensions and config); decided by Tyler before W6 — blocks 2.1.2a and every later N-way, chart and hosting task — owner: Tyler (task 0.3.1b)
- [ ] Optional: run `code-hotspot-analysis` (churn-weighted) before Phase 5 to confirm `main.rs`/`index.js` dispositions; current dispositions rest on line counts — non-blocking — owner: implementer
- [ ] Does GitHub honour `#gh-light-mode-only` / `#gh-dark-mode-only` fragments in step summaries (alternative to `<picture>`)? Part of the seven-variant matrix in 0.2.1a — blocks Story 3.3.3 default only — owner: Tyler (0.2.1b)

## Dependency Visualization

```
Phase 0                  Phase 1                                   Phase 2                              Phase 3                        Phase 4                       Phase 5
0.3.1a baseline fixture (W1) ─> 0.3.1b = GATE G0 (human, before W6) ─────────────────────────────────────────┐ (blocks 2.1.2a, 3.3.1a and everything after them; 1.2.4a, Phase 1, is G0-exempt)
1.1.1b module stubs (W1, lib.rs frozen) ─┬─> 1.1.1a dims model ─> 1.1.2a ─> 1.1.2b ─> 1.1.2d apply (W5)
                                         ├─> 1.1.1c Notice types (W2) ─────────────────────────> 1.1.2d, 2.1.2a, 2.2.2b, 3.3.1c
                                         └─> 1.1.2c Dimensioned + summary (W4, dimensioned.rs) ─> 1.1.2d
0.1.1a ─> 0.1.1b (W2; only needs 0.1.1a files, not a prerequisite of 1.x)  │
0.1.2a size gate ─┤                                                                                         │
0.1.3a buildArgs ─┤                                                                       │
                  ├─> 1.2.1a overlay ─> 1.2.5a thresholds ─> 1.2.5b Samples ──┐           │
                  ├─> 1.2.1b method-note plumbing (W2: Summary fields + render_method) ─> 1.2.5a (W3), 1.2.3a (W5)
                  │         └──────────> 1.2.2a config.rs ─> 1.2.3a CLI ─> 1.2.4a js/action.yml
0.2.1a ─> 0.2.1b = GATE G1 (human) │                                                           │           │
 (human)          │                          2.1.1a seam (after 1.2.5b) ─> 2.1.2a project ─> 2.1.3a matrix (defines CellFormat, Correction, MatrixSort) ─> 2.1.4a Holm ─> 2.1.5a geomean ─> 2.1.6a JSON
                  │                                      2.2.2a CLI flags/keys/validation (W9, needs 2.1.3a) ─┐              └─> 2.2.1a table (W10) ─> 2.2.1b notices+gate (W11) ─> 3.3.1c extras (W12) ┐
                  │                                                                                                       └─> 2.2.2b wiring + ladder (W12) ─> 3.3.1b render_svgs + svg-dir (W13) ─> 2.2.3a fixtures + wasm (W14, alone)
                  ├─> 3.1.1a writer ─> 3.1.2a text ─> 3.1.3a style ─> 3.3.1a SvgOptions/SvgFile/SkipReason/TextRow/flags (W6) ─> 3.2.1a delta (W7) ─┬─> 3.2.4a caps + SvgInput + select_rows (W9) ─> 3.2.5a security ─> 3.3.1b (needs 3.2.4a, 2.2.2b, 3.3.1c; owns chart dispatch) ─> 3.3.3a picture (W13, needs 3.3.1c) ─> 3.2.6a perf (W15, needs the W14 wasm)
                  │                          (3.2.2a ranking, 3.2.3a dist need 2.1.3a + 1.2.5b) ─┘          3.3.2a alt text
                  │
                  └─> 4.1.1a images.js (after 1.2.4a) ─> [G1] 4.1.2a ─> 4.1.2b ─> 4.1.2c ─> 4.1.3a clip ─> 4.2.1a inputs/buildArgs (W15) ─> 4.2.1b main() pipeline (W16) ─> 4.2.3a variants parse/stage (W17) ─> 4.2.3b data-error handling (W18) ─> 4.2.3c e2e (W19) ─┐
                                                                                         (4.2.1a needs 3.3.1b, 4.1.3a)              4.2.2a golden SVG + wasm (W19)
                                                                                                                            5.1.1a README, 5.1.4a release wf ─> 5.1.2a wasm (W20) ─> 5.1.3a e2e + release (W21)
```

Gate G1 (task 0.2.1b, human) is scheduled between W1 and W8: waves W2-W7 and W9+ work that does not touch the publisher proceed without it; only 4.1.2a-c (W8-W10) and the default-embedding form in 3.3.3a (W13) wait on it, and 3.1.3a ships the opaque light card regardless. The width-table source (3.1.2a) is cited by the implementer in the code comment and does not wait on G1.

Gate G0 (task 0.3.1b, human) is the product falsifier: task 0.3.1a (W1) commits the `testdata/nway` inputs and today's pairwise output; Tyler runs the baseline readers (Q1-Q3, requirements.md "Outcome metric") any time before W6. Waves W2-W5 do not wait: they are the safety net, dimensions, config and kernel prerequisites (Phase 1 ships in every outcome) plus the pure, dependency-free SVG foundation (3.1.1a-3.1.3a, three S tasks). W6 onward (2.1.2a matrix core, 3.3.1a chart options, and everything after them) waits for G0. If G0 says the baseline is answered correctly in under 60 seconds by at least 2 of 3 readers, the problem is smaller than assumed and the plan stops after Phase 1.

Parallel lanes after Phase 0: (L1) dims -> config -> thresholds, (L2) matrix core, (L3) SVG foundation (3.1.x has no dependency on L1/L2), (L4) JS images seam. Chart tasks 3.2.2a/3.2.3a need `MatrixReport` (2.1.3a) and `Samples` (1.2.5b); 3.2.1a needs the existing `Report` only, plus `SvgOptions` (3.3.1a, W6), which is why 3.2.1a moved from W5 to W7.

**Delivery order de-risks the product bet** (requirements.md "Risky assumption"): the N-way table, `variants` input and CLI are complete and useful by W14 (2.2.3a) with no dependence on image hosting; hosting (4.1.2a-c, W8-W10) sits behind gate G1, and if G1 fails the ladder in ADR-003 still delivers SVG files plus the text chart, so the N-way table is never blocked by the SVG bet.

---

## Effort estimate

Round 3 raised five stories one class each (0.1.1, 0.1.2, 1.1.2, 3.2.5, 4.1.2: importer and Action goldens, freshness and purity gates, data-error policy, geometry and rasterized review, retention); no task was added. Triad round 1 split task 3.3.1a into three (S, S, M) and added task 5.1.5a (config JSON schema and starter example, S); the other triad additions (regression gate, notices, `--print-dimensions`, `cell-format`, legend) were absorbed inside existing tasks, which was the main estimate risk. Triad round 2 turned that risk into tasks: nine tasks were added (0.3.1a/b falsifier baseline, 1.1.1c Notice types, 1.1.2d apply split from 1.1.2c, 1.2.1b method-note plumbing, 2.2.1b notices and gate, 2.2.2b wiring and ladder, 4.2.1b main() pipeline, 4.2.3b data-error handling), 4.2.3b became 4.2.3c, and 1.1.2c, 2.2.1a, 2.2.2a, 4.2.1a and 4.2.3a were narrowed. The remaining estimate risk is 2.1.3a (now also defines the shared matrix enums and `MatrixSummary`; watch it, INFERRED). Basis: classes per `ESTIMATION.md` (XS 20k, S 55k, M 130k raw tokens midpoints); assumed output share 12%; CU factor 1.5 (no cache discount, conservative); verification multiplier x1.75 per story. Bands and weights INFERRED; recalibrate after the first implementer run.

| Story | Tasks | Class mix | Raw tokens (k) | Output share | CU (k) | Wall-clock blocker |
|---|---|---|---|---|---|---|
| 0.1.1 Freeze goldens | 2 | M,S | 185 | 12% | 486 | |
| 0.1.2 Size, freshness and purity gates | 1 | S | 55 | 12% | 144 | |
| 0.1.3 buildArgs characterization | 1 | XS | 20 | 12% | 53 | |
| 0.2.1 Hosting verification | 2 | S,XS | 75 | 8% | 197 | Real PR CI run + owner viewing (Tyler) |
| 0.3.1 Falsifier baseline (gate G0) | 2 | S,XS | 75 | 8% | 197 | Baseline readers, Q1-Q3 on today's pairwise output (Tyler plus two others, or a recorded waiver) |
| 1.1.1 Dims model + module stubs + Notice types | 3 | S,XS,XS | 95 | 12% | 249 | |
| 1.1.2 Extractors (default, rules, Dimensioned + summary, apply) | 4 | M,M,S,M | 445 | 12% | 1,168 | |
| 1.2.1 Options overlay refactor + method-note plumbing | 2 | M,S | 185 | 12% | 486 | |
| 1.2.2 Config parse | 1 | M | 130 | 12% | 341 | |
| 1.2.3 CLI config flags | 1 | S | 55 | 12% | 144 | |
| 1.2.4 JS + action.yml layering | 1 | M | 130 | 12% | 341 | |
| 1.2.5 Kernel prerequisites (thresholds, Samples) | 2 | S,S | 110 | 12% | 289 | |
| 2.1.1 Seam | 1 | XS | 20 | 12% | 53 | |
| 2.1.2 Projection | 1 | M | 130 | 12% | 341 | |
| 2.1.3 compare_matrix | 1 | M | 130 | 12% | 341 | |
| 2.1.4 Holm | 1 | S | 55 | 12% | 144 | |
| 2.1.5 Geomean + ranking | 1 | S | 55 | 12% | 144 | |
| 2.1.6 JSON matrix | 1 | S | 55 | 12% | 144 | |
| 2.2.1 Matrix Markdown (table; notices and gate) | 2 | M,S | 185 | 12% | 486 | |
| 2.2.2 CLI N-way flags (flags and validation; wiring and ladder) | 2 | M,M | 260 | 12% | 682 | |
| 2.2.3 N-way fixture | 1 | S | 55 | 12% | 144 | |
| 3.1.1 SvgWriter | 1 | S | 55 | 12% | 144 | |
| 3.1.2 Text metrics | 1 | S | 55 | 12% | 144 | |
| 3.1.3 Style/theme | 1 | S | 55 | 12% | 144 | Theme default pending 0.2.1b |
| 3.2.1 Delta chart | 1 | M | 130 | 12% | 341 | |
| 3.2.2 Ranking chart | 1 | M | 130 | 12% | 341 | |
| 3.2.3 Distribution | 1 | S | 55 | 12% | 144 | |
| 3.2.4 Caps/empty | 1 | S | 55 | 12% | 144 | |
| 3.2.5 Security corpus and geometry | 1 | M | 130 | 12% | 341 | |
| 3.2.6 Performance budget | 1 | S | 55 | 12% | 144 | |
| 3.3.1 SvgOptions + flags, render_svgs + svg-dir, placeholders | 3 | S,S,M | 240 | 12% | 630 | |
| 3.3.2 Alt text + details | 1 | S | 55 | 12% | 144 | |
| 3.3.3 Theme-pair embedding | 1 | S | 55 | 12% | 144 | Default pending 0.2.1b |
| 4.1.1 images.js seam | 1 | M | 130 | 12% | 341 | |
| 4.1.2 Orphan-branch publisher | 3 | M,S,M | 315 | 12% | 827 | 0.2.1b verdict |
| 4.1.3 clip with images | 1 | XS | 20 | 12% | 53 | |
| 4.2.1 Action wiring (inputs and buildArgs; main() pipeline) | 2 | M,M | 260 | 12% | 682 | |
| 4.2.2 Golden SVG shim | 1 | S | 55 | 12% | 144 | |
| 4.2.3 Action `variants` input (parse and stage; data-error handling; e2e) | 3 | M,S,S | 240 | 12% | 630 | |
| 5.1.1 README | 1 | S | 55 | 12% | 144 | |
| 5.1.2 Rebuild wasm | 1 | XS | 20 | 12% | 53 | |
| 5.1.3 Real-PR e2e + release | 1 | S | 55 | 8% | 144 | CI run, owner sign-off, tag move (Tyler) |
| 5.1.4 Release workflow charts | 1 | S | 55 | 12% | 144 | |
| 5.1.5 Config JSON schema + starter example | 1 | S | 55 | 12% | 144 | |
| **Total** | **63** | | **4,835** | | **~12,685** | |

Verification multiplier x1.75 already included in CU. Size band: Large (3-15M CU), informational. Planning/validation overhead (Phases 3-4 of SDD) is additional.

**Critical path (agent waves)**. No two tasks in the same wave edit the same file; a file's edits are serialized across waves in the order below.

| Wave | Tasks (parallel) | 
|---|---|
| W1 | 0.1.1a, 0.1.2a, 0.1.3a, 0.2.1a, 0.3.1a (falsifier fixture and baseline), 1.1.1b (module stubs; freezes the `lib.rs` module list and `svg/mod.rs` submodule list) |
| G0 | 0.3.1b: explicit human gate (Tyler), baseline readers on today's pairwise output; runs any time after 0.3.1a and before W6; W2-W5 do not wait for it; W6 waits for it except 1.2.4a (G0-exempt, Phase 1), and W7 onward does |
| G1 | 0.2.1b: explicit human gate (Tyler), runs any time after 0.2.1a's PR run and before W8; W2-W7 do not wait for it |
| W2 | 0.1.1b, 1.1.1a, 1.1.1c (`Notice`/`NoticeClass`), 1.2.1a, 1.2.1b (method-note plumbing: `Summary` fields + `render_method`; owns `compare.rs` and `markdown.rs` in this wave), 3.1.1a |
| W3 | 1.1.2a, 1.2.5a, 3.1.2a |
| W4 | 1.1.2b, 1.1.2c (`dimensioned.rs`), 1.2.2a, 1.2.5b, 3.1.3a |
| W5 | 1.1.2d, 1.2.3a, 2.1.1a |
| W6 (2.1.2a and 3.3.1a need G0; 1.2.4a is G0-exempt: Phase 1 config/action layering ships in every G0 outcome) | 1.2.4a, 2.1.2a, 3.3.1a (`SvgOptions`, `SvgFile`, `SkipReason`, `TextRow`, overlay, chart config keys, chart flags; scheduled here so every chart task, 3.2.1a onward, consumes one options type) |
| W7 | 2.1.3a (also defines `CellFormat`, `Correction`, `MatrixSort`, `MatrixSummary`), 3.2.1a, 4.1.1a |
| W8 (needs G1 for 4.1.2a) | 2.1.4a, 3.2.2a, 3.2.3a, 4.1.2a |
| W9 | 2.1.5a, 2.2.2a (flags, keys, overlays, static validation, `--help`; no wiring), 3.2.4a (caps, `SvgInput`, `select_rows`), 4.1.2b |
| W10 | 2.1.6a, 2.2.1a (matrix table), 3.2.5a, 4.1.2c, 4.1.3a |
| W11 | 2.2.1b (notices, gate line, notice-only report), 3.3.2a |
| W12 | 2.2.2b (matrix wiring in `main.rs`, degradation ladder, `--print-dimensions`; calls the frozen `render_matrix_markdown`), 3.3.1c (`RenderExtras`, `render_markdown_with`, additive `render_matrix_markdown_with`, placeholders; disjoint files from 2.2.2b) |
| W13 | 3.3.1b (`render_svgs` and chart dispatch, `--svg-dir` write; switches `main.rs` to the `_with` forms), 3.3.3a (`<picture>` block; disjoint files from 3.3.1b) |
| W14 | 2.2.3a (integration task, alone in its wave: rebuilds and commits `dist/benchdiff.wasm` from the merged tree; no `then` sequencing needed because nothing else runs in the wave) |
| W15 | 4.2.1a, 3.2.6a (needs the W14 wasm), 5.1.5a |
| W16 | 4.2.1b |
| W17 | 4.2.3a |
| W18 | 4.2.3b |
| W19 | 4.2.3c, 5.1.1a, 5.1.4a, 4.2.2a (integration task: rebuilds and commits the wasm; the other tasks of the wave change no Rust source, so there is nothing to order against) |
| W20 | 5.1.2a |
| W21 | 5.1.3a |

**File ownership (serialization)**. The wave column is the order in which tasks touch the file; one owner per wave. Each task body carries a `Files` line that must agree with this table.

| File | Owners in order |
|---|---|
| `crates/benchdiff-core/src/lib.rs` | 1.1.1b (all `pub mod` lines, so later tasks use module paths), then 2.1.6a (W10, `render_json_with_matrix` and the matrix re-exports), 3.3.1c (W12, `render_markdown_with`, `RenderExtras`, `render_matrix_markdown_with`), 3.3.1b (W13, `render_svgs` and the `SvgOptions` family). These three are the only crate-root re-exports; every other task (3.3.1a included) uses module paths such as `benchdiff_core::svg::SvgOptions`. No other task or story Files list names `lib.rs` |
| `svg/mod.rs` | 1.1.1b (stub: declares submodules `writer`, `text`, `style`, `delta`, `ranking`, `distribution`, so later tasks only fill files), 3.1.1a (W2), 3.3.1a (W6, `SvgOptions`, the chart enums, `SvgFile`, `SkipReason`, `TextRow`), 3.2.4a (W9, `SvgInput`, `select_rows`, caps), 3.3.2a (W11, `chart_text_table`, alt text), 3.3.1b (W13, `render_svgs` and chart dispatch) |
| `svg/{writer,text,style,delta,ranking,distribution}.rs` | one owner each: 3.1.1a, 3.1.2a, 3.1.3a, 3.2.1a, 3.2.2a, 3.2.3a (3.2.4a also edits `delta.rs`, `ranking.rs` and `distribution.rs` in W9, after their W7/W8 owners) |
| `Cargo.toml`, `Cargo.lock` | 1.1.2b only (`regex-lite`); no dev-dependencies are added (3.1.1a uses a hand-written tag-balance checker) |
| `.github/workflows/ci.yml`, `Makefile` | 0.1.2a (W1, size gate and `wasm-fresh` job), 3.2.6a (W15, `make perf` target running `cargo test --release`), 5.1.3a (W21) |
| `compare.rs` (incl. `Options`, `Summary` and `ConfigSource`, which stay here) | 1.2.1b (W2, `Summary.threshold_overrides` / `config_sources` and the `pub struct ConfigSource`, consumed by `main.rs` in 1.2.3a via `benchdiff_core::compare::ConfigSource`), 1.2.5a (W3), 1.2.5b (W4), 2.1.1a (W5) |
| `options.rs` (overlays only: `OptionsOverlay`, `MatrixOptionsOverlay`, `SvgOptionsOverlay`) | 1.2.1a (W2), 1.2.5a (W3), 3.3.1a (W6), 2.2.2a (W9) |
| `config.rs` | 1.2.2a (core keys and `$schema`), 3.3.1a (W6, chart keys), 2.2.2a (W9, matrix keys incl. `cell-format`); `5.1.5a` only reads the public key table |
| `matrix.rs` | 2.1.2a (W6), 2.1.3a (W7: `Variant` consumers, `MatrixReport`, `MatrixOptions`, `MatrixSummary`, `CellFormat`, `Correction`, `MatrixSort`), 2.1.4a (W8), 2.1.5a (W9), 2.1.6a (W10) |
| `stats.rs` | 2.1.4a, 2.1.5a |
| `dimensions.rs` | 1.1.2a (W3), 1.1.2b (W4), 1.1.2d (W5, `apply`, collisions, diagnostics), 2.1.2a (W6, axis helpers only) |
| `dimensioned.rs` | 1.1.2c only (W4: `Dimensioned`, `Residual`, `identity`, accessors, `dimension_summary`, `available_dimensions_text`); stub from 1.1.1b |
| `notice.rs` | 1.1.1c only (W2: `Notice`, `NoticeClass`); stub from 1.1.1b |
| `model.rs` | 1.1.1a only (`dims` field and `with_dims`; `Metric::new` is the sole `Metric` literal, so importers are untouched) |
| `import/*.rs` | no task edits them (verified: `grep 'Metric {' crates` finds only `model.rs` inside `Metric::new` and a test helper that calls `Metric::new`); 1.1.2d only reads their fixtures |
| `markdown.rs` | 1.2.1b (W2, `render_method` sentences for `MethodFacts`), 2.1.1a (W5, visibility), 3.3.1c (W12, `render_markdown_with`, notices, text block), 3.3.3a (W13, picture builder) |
| `markdown_matrix.rs` | 2.2.1a (W10, table, headline, legend, `cell-format`, column cap, Method note), 2.2.1b (W11, `render_notices`, notice list in Method, gate line, `render_notice_report`), 3.3.1c (W12, `render_matrix_markdown_with`, text block, chart notices), 3.3.3a (W13) |
| `crates/benchdiff/src/main.rs` | 1.2.1a (W2), 1.2.3a (W5), 3.3.1a (W6, chart flags), 2.2.2a (W9, matrix flag parsing and static validation), 2.2.2b (W12, matrix wiring, degradation ladder, `--print-dimensions`), 3.3.1b (W13, `--svg-dir` write, `keep_samples`, `_with` call sites) |
| `crates/benchdiff/tests/{config_cli,matrix_cli}.rs` | `config_cli.rs`: 1.2.3a (W5), 1.2.4a (W6), 2.2.2a (W9), 2.2.2b (W12); `matrix_cli.rs`: 2.2.2a (W9), 2.2.2b (W12) |
| `crates/benchdiff/tests/svg_cli.rs` | 3.3.1a (help/flags), 3.3.1b (write, embed, stdout/stderr) |
| `crates/benchdiff/tests/vocabulary.rs` | 4.2.1a only |
| `crates/benchdiff/tests/golden_expected.rs`, `testdata/golden/expected/*` | `golden_expected.rs`: 0.1.1b only; `expected/*`: 0.1.1a only |
| `index.js`, `action.yml` | 1.2.4a (W6), 4.1.1a (W7, `index.js` only), 4.1.3a (W10, `index.js` only), 4.2.1a (W15), 4.2.1b (W16), 4.2.3a (W17), 4.2.3b (W18) |
| `test/action.test.js` | 0.1.3a (test only), 1.2.4a, 4.1.3a, 4.2.1a, 4.2.1b, 4.2.3a, 4.2.3b, 4.2.3c (test only) |
| `test/ux_messages.test.js` | 4.2.1b only |
| `scripts/check-wasm-size.sh`, `scripts/check-wasm-fresh.sh`, `test/wasm_size.test.js` | 0.1.2a only (`scripts/` does not exist yet; 0.1.2a creates it) |
| `.github/workflows/release-benchmarks.yml` | 5.1.4a only |
| `crates/benchdiff-core/tests/perf.rs`, `crates/benchdiff-core/tests/nway_scale.rs`, `test/perf.test.js` | 3.2.6a only |
| `crates/benchdiff-core/tests/nway_fixture.rs` | 2.2.3a only |
| `testdata/nway/*` | `{quick,heap,merge}.txt` and `baseline/*`: 0.3.1a only; `impl.txt` and `expected/{matrix.md,matrix.json}`: 2.2.3a; `expected/notice-only.md`: 2.2.1b; `expected/svg/*`: 4.2.2a |
| `requirements.md` | 0.3.1b only (appends "Falsifier baseline result"); this review round also edited it, outside the task graph |
| `crates/benchdiff-core/tests/json_compat.rs` | 1.2.5b (W4, default-bytes and samples-never-serialized tests), 2.1.6a (W10, matrix key, companion counts, `migration_should_be_reversible`) |
| `crates/benchdiff-core/tests/dimensions_import.rs` | 1.1.1a (W2, importers leave `dims` empty), 1.1.2d (W5, apply over every importer fixture) |
| `crates/benchdiff-core/tests/snapshots/svg/*` | one owner per file name: `delta*` 3.2.1a, `ranking*` 3.2.2a, `distribution*` 3.2.3a; `svg_snapshots.rs` 3.2.5a |
| `lib/images.js` | 4.1.1a, 4.1.2a, 4.1.2b, 4.1.2c, 3.3.3a |
| `test/images.test.js` | 4.1.1a, 4.1.2a, 4.1.2b, 4.1.2c, 3.3.3a (same order as `lib/images.js`) |
| `test/golden.test.js` | 0.1.1b, 1.2.4a, 2.2.3a, 4.2.2a |
| `dist/benchdiff.wasm` | 2.2.3a (W14), 4.2.2a (W19), 5.1.2a (W20) only (see Migration Plan). By design the committed wasm lags the Rust source from W15 to W19 (rebuilt in W19 by 4.2.2a for goldens and finally in W20/5.1.2a); no W15-W18 task may assume it is current |
| `README.md`, `test/docs.test.js`, `docs/config.md` | 5.1.1a only (`docs/config.md` only if README exceeds scope) |
| `.github/workflows/spike-svg-hosting.yml`, `fixtures/spike/*.svg` | 0.2.1a (W1, created), 0.2.1b (G1, deleted) |
| `project_plans/benchdiff-dimensions-svg/decisions/ADR-003-svg-hosting-orphan-branch.md` | 0.2.1b (G1, matrix and status), 5.1.3a (W21, finalize) |
| `benchdiff.config.schema.json`, `examples/benchdiff.config.json`, `crates/benchdiff/tests/config_schema.rs` | 5.1.5a only |
| `testdata/README.md` | 0.1.1a (W1), 3.2.5a (W10, PNG-review rule for golden SVGs), 2.2.3a (W14), 4.2.2a (W19) |
| `crates/benchdiff-core/tests/purity.rs` | 0.1.2a only |
| `crates/benchdiff-core/tests/{svg_security,svg_geometry,svg_snapshots}.rs`, `scripts/svg-to-png.sh`, `testdata/svg-worst/*` | 3.2.5a only |

**Overrun checkpoint**: if cumulative spend exceeds 25% over the table total for completed stories (about 15.9M CU overall (12,685 x 1.25 = 15,856k); check at each phase boundary against that phase's sum), pause and report delta and cause.

**Wall-clock blockers**: (0) gate G0, the falsifier baseline readers (Q1-Q3 on today's pairwise output) - Tyler plus two others, or a recorded waiver, before W6; (1) real-PR rendering verification, CI run and visual check - Tyler; (1b) the MAN-6 ratio-vs-percent comprehension check and the MAN-7 time-to-signal walkthrough need two other readers (or an explicit waiver) - Tyler; (2) ADR-003 finalization after (1) - Tyler; (3) `contents: write` documentation wording - Tyler; (4) release tag and `@v0` move after e2e - Tyler; (5) the adoption proxy count at release+30 and +90 days (requirements.md "Adoption proxy", OM-2) - Tyler.

---

# PHASE 0: Safety net and verification spikes

## Epic 0.1: Freeze current behaviour
**Goal**: "Byte-identical when new options off" becomes testable.

### Story 0.1.1: Committed expected golden files
**As a** maintainer, **I want** today's Markdown/JSON outputs committed as expected files, **so that** any accidental change fails a test (today's golden test only proves wasm == native).
**Acceptance Criteria**:
- Expected files exist for each default-path case, for each importer format and for the Action as a whole, and are asserted by both native and wasm runs (the Action golden by `main()` through the real wasm).
  - *Given* `testdata/golden/base.go.txt` and `head.go.txt` with default options, *When* the CLI renders Markdown, *Then* stdout equals `testdata/golden/expected/default.md` byte for byte.
**Files**: `testdata/golden/expected/*`, `testdata/README.md`, `test/golden.test.js`, `crates/benchdiff/tests/golden_expected.rs`

##### Task 0.1.1a: Generate and commit expected outputs from the current build (M)
- Using the current unmodified build, run the CLI for cases: default md; `--format json`; `--only-changed`; `--max-rows 2`; `--sort name`; `--threshold 1 --min-abs 5`; native JSON re-read as base (`testdata/benchdiff/native.json`) (7 option cases). Because `Metric` gains a field (1.1.1a) and every importer's output later feeds dimension extraction (1.1.2), the default path must stay byte-identical for every importer, so also one golden per importer format value not already covered by those cases, using the existing fixtures under `testdata/<format>/` as both base and head with `--format <value>`: `bmf`, `cargo-bench`, `criterion`, `gab-smaller`, `gab-bigger`, `googlebench`, `hyperfine`, `jmh` (8 importer cases; `go` and `benchdiff` are covered by cases 1 and 7). Write results to `testdata/golden/expected/<case>.{md,json}`. Also commit the Action-level golden (`expected/action-default.{md,json}`: full sticky-comment body, `report-path` and `json-path` contents and the output counts for the golden fixtures with no tunable inputs; produced by 0.1.1b's harness from the unmodified build). Document regeneration command in `testdata/README.md` (`UPDATE_GOLDEN=1`).
- Files: `testdata/golden/expected/*` (new), `testdata/README.md`
- AC: 15 CLI cases (7 option + 8 importer) plus the Action-level golden committed.
  - *Given* the unmodified `main` build, *When* each case command runs twice, *Then* both runs produce identical bytes and the files are written once.
- Test approach: run each command twice and `cmp`; review diff is additions only.

##### Task 0.1.1b: Assert expected files in node and cargo tests (S)
- Extend `test/golden.test.js` to compare wasm AND native output to the expected files per case (all 15); add `crates/benchdiff/tests/golden_expected.rs` running the same cases natively. Add the Action-level case: `main()` with the real wasm, a mocked `fetch` and no tunable inputs, asserting the comment body, outputs and `failed` equal `expected/action-default.*` (this is the guard for the `action.yml` default removal in 1.2.4a). `UPDATE_GOLDEN=1` rewrites.
- Files: `test/golden.test.js`, `crates/benchdiff/tests/golden_expected.rs`
- AC: mutation fails the test.
  - *Given* the committed `default.md`, *When* one character of `markdown.rs` header text is changed, *Then* both test suites fail naming `default`.
- Test approach: manual mutation check, then revert; `make golden` green.

### Story 0.1.2: wasm size, freshness and purity gates
**As a** maintainer, **I want** CI to fail when the wasm exceeds a budget or differs from its source, and when core gains I/O or the Action gains a dependency, **so that** growth and drift are conscious decisions.
**Acceptance Criteria**:
- Budget 480,000 bytes, since raised to 820,000 (see `scripts/check-wasm-size.sh`, ADR-004 'Budget note') (baseline 330,650; stack projection 450-480 KB, INFERRED).
  - *Given* `dist/benchdiff.wasm` of 330,650 bytes, *When* `scripts/check-wasm-size.sh` runs, *Then* it prints `330650 / 820000 bytes` (originally `330650 / 480000 bytes`; budget raised to 820,000, ADR-004 'Budget note') and exits 0; *Given* a 500,000-byte file, *Then* it exits 1.
- Freshness (the Action runs the committed wasm, so a stale binary ships old behaviour under a new tag).
  - *Given* a PR that touches `dist/benchdiff.wasm` or a release tag, *When* CI job `wasm-fresh` rebuilds the wasm from source with the pinned toolchain, *Then* it fails if the rebuilt file differs from the committed one (byte comparison; if two clean builds are not byte-identical, it compares the export list plus size and the cause is recorded in ADR-004's note). Resolves the reproducibility question in W1.
- Purity (requirement C-1).
  - *Given* `crates/benchdiff-core/src/**`, *When* `purity.rs` scans it, *Then* it finds no use of `std::fs`, `std::net`, `std::process` or `std::env` outside `#[cfg(test)]` items (I/O stays in the binary and `index.js`); *Given* a core source with `std::fs` in production code, *Then* the test fails naming the file.
  - The scan must exclude `#[cfg(test)]` modules, because the current tree already uses `std::fs` in tests (verified: `crates/benchdiff-core/src/import/mod.rs:137` and `crates/benchdiff-core/src/import/json_formats.rs:425` read fixtures inside `#[cfg(test)]` modules). *Given* those two files as they are, *Then* the scan passes; *Given* the same `std::fs` call moved out of the `#[cfg(test)]` block, *Then* it fails naming the file and line. Method: the scanner removes each `#[cfg(test)]` item by brace matching before searching (strings and comments are not parsed; a brace inside a string literal in a test module is a known limit, noted in the test); `crates/benchdiff-core/tests/**` is not scanned.
**Files**: `scripts/check-wasm-size.sh`, `scripts/check-wasm-fresh.sh`, `.github/workflows/ci.yml`, `Makefile`, `crates/benchdiff-core/tests/purity.rs`, `test/wasm_size.test.js`

##### Task 0.1.2a: Size check script, `wasm-fresh` job, purity test (S)
- Files: `scripts/check-wasm-size.sh`, `scripts/check-wasm-fresh.sh`, `.github/workflows/ci.yml`, `Makefile`, `crates/benchdiff-core/tests/purity.rs`, `test/wasm_size.test.js`
- Add script `scripts/check-wasm-size.sh [path]` (POSIX `stat`/`wc -c`; default path `dist/benchdiff.wasm`), `make wasm-size`, CI step run against the freshly built wasm (and 5.1.2a runs it against the committed file); the path-gated `wasm-fresh` job and the `purity.rs` test above. Budget in one variable. **Headroom risk**: the baseline is 330,650 bytes, `regex-lite` adds about 55 KB (INFERRED, research/stack.md), and the matrix, config and SVG code come on top, so the stack projection of 450-480 KB leaves little or no margin under 480,000. Re-measure at four points, each noted in that task's PR with the number and the remaining headroom: after 1.1.2b (`regex-lite`), after 3.3.1b (all of core is in), after 2.2.3a (the first committed integration wasm, W14) and after 4.2.2a (W19); the budget policy is fixed at the first point where headroom drops under 20,000 bytes, not at release. The profile is already size-tuned (`opt-level = "s"`, `lto`, `strip`, `panic = "abort"` in `Cargo.toml`), so the remaining levers are `opt-level = "z"`, `wasm-opt -Oz` and trimming; raising the budget requires a one-line note in ADR-004. The check script and the `Makefile` live in this task, so a budget breach in a later wave surfaces as a failing CI step, not as a surprise at 5.1.2a. **Budget moved to 820,000 bytes on 2026-11-01** (measured 799,198 at 3.3.1b); see the Budget note in ADR-004. Update (2026-11-01): after Phase 6 the wasm hit 826,099 bytes under `opt-level = "s"`, so the workspace `[profile.release]` now uses `opt-level = "z"` (727,365 bytes; see ADR-004 Budget note).
- AC as above. Test approach: run with a padded temp copy.

### Story 0.1.3: Characterize `buildArgs`
**As a** maintainer, **I want** current argument-building behaviour pinned, **so that** removing action.yml defaults (1.2.4) is a visible, intended diff.
**Acceptance Criteria**:
- Pin current output.
  - *Given* inputs `{head:['h.txt'], base:['b.txt'], format:'auto', threshold:'5', minAbs:'0', sort:'delta', title:'Benchmark comparison'}`, *When* `buildArgs` runs, *Then* it returns the exact current argv (snapshot in test).
**Files**: `test/action.test.js`

##### Task 0.1.3a: Snapshot tests for `buildArgs` (XS)
- Files: `test/action.test.js` (tests only; no code change)
- Add tests for all-defaults, all-set, `onlyChanged`, `failOnRegression` forms. Add the zero-dependency guard (requirement C-1). Verified: there is no `package.json` in the repository (the Action is `main: index.js` in `action.yml` with `using: node24`), so absence is treated as "no dependencies" and this task does not create one. *Given* no `package.json`, *Then* the guard passes; *Given* a `package.json` that has a non-empty `dependencies` object, *Then* it fails naming the first dependency (a temp-dir fixture proves the failing branch); and in both cases `index.js` and `lib/*.js` (the latter is absent until 4.1.1a, and absent files are skipped) require only `node:*` built-ins and local relative files (a test greps their `require`/`import` lines). No code change.
- Test approach: `node --test test/action.test.js`.

## Epic 0.2: Hosting verification spike
**Goal**: Replace INFERRED rendering claims with observation before building the publisher.

### Story 0.2.1: Verify SVG rendering on GitHub
**As a** maintainer, **I want** to see a hosted SVG in a real comment and step summary, **so that** ADR-003 is decided on evidence.
**Acceptance Criteria**:
- Observation matrix recorded in ADR-003 (widened in round 3 so the fallback ladder is chosen on evidence for every repo class in the support matrix).
  - *Given* fixture `fixtures/spike/card.svg` (with `<style>` using `prefers-color-scheme`, `<title>`, `<desc>`) committed to branch `benchdiff-assets` at commit `<sha>`, *When* a throwaway workflow on a same-repo public PR posts a comment and writes `$GITHUB_STEP_SUMMARY` embedding `https://raw.githubusercontent.com/tstapler/benchdiff/<sha>/spike/card.svg`, *Then* the owner records for comment and summary: renders yes/no, GitHub light/dark theme, OS light/dark, desktop/mobile, and whether `<picture>` works.
  - *Given* the rendered comment HTML (fetched with `Accept: application/vnd.github.html+json`), *When* the Camo-proxied `img src` is requested with `curl -I`, *Then* the owner records its `Content-Type` and `X-Content-Type-Options` (a `nosniff` plus non-`image/svg+xml` type would break rendering even where the raw URL itself serves `image/svg+xml`).
  - *Given* the same workflow run in a **private repo** and from a **fork PR**, *When* it posts, *Then* the owner records which of the seven embed variants (below) render in each class (comment and summary), so ADR-003 picks the rungs of the ladder that still show a chart there.
**Files**: `.github/workflows/spike-svg-hosting.yml` (deleted after the spike), `project_plans/benchdiff-dimensions-svg/decisions/ADR-003-svg-hosting-orphan-branch.md`

##### Task 0.2.1a: Fixture SVGs + throwaway workflow (S)
- Hand-write two small SVGs (one with CSS media query, one opaque card), a workflow (`workflow_dispatch` + `pull_request`) that commits them to an orphan branch via the Git Data API (this prototypes 4.1.2's calls), then posts a comment and a summary with seven embed variants: (1) plain `![]()` to the raw URL, (2) `<picture>`, (3) `#gh-light-mode-only`/`#gh-dark-mode-only` fragments, (4) an artifact `archive:false` link, (5) a `github.com/.../raw/...` link, (6) a base64 `data:image/svg+xml` URI (summary especially; the comment sanitizer is expected to strip it), (7) a repo-tracked path (`docs/spike/card.svg` on the default branch, via the raw URL and via a relative path). It also prints the Camo `Content-Type` check from the AC. It must be runnable in a public same-repo PR, a private repo and a fork PR (three runs).
- Files: `.github/workflows/spike-svg-hosting.yml`, `fixtures/spike/*.svg`
- AC: workflow green.
  - *Given* a PR from a same-repo branch, *When* the workflow runs, *Then* a comment and summary exist containing all seven embed variants.
- Test approach: run on a draft PR; repeat in a private repo and from a fork PR (run C may be waived if no fork is available, recorded in ADR-003).

##### Task 0.2.1b: Owner observation and ADR update (XS, wall-clock blocker, gate G1)
- Files: `project_plans/benchdiff-dimensions-svg/decisions/ADR-003-svg-hosting-orphan-branch.md`, `.github/workflows/spike-svg-hosting.yml` (deleted), `fixtures/spike/*.svg` (deleted)
- Owner views results (public same-repo, private repo, and a fork PR run if available), records the matrix and the Camo header check in ADR-003, flips status to Final or selects ladder rungs per repo class (ADR-003 ladder); removes the spike workflow, deletes the throwaway `benchdiff-assets` branch and `fixtures/spike/` (unless the branch is kept as the real hosting branch, which is stated in the ADR), (the width-table licence is recorded by 3.1.2a itself, not here).
- AC: ADR-003 has a filled results table.
  - *Given* the observation results, *When* ADR-003 is edited, *Then* its Status line reads `Final` or names the chosen fallback rung.
- Scheduling: gate G1 in the wave table. It blocks 4.1.2a-c and the default embed form in 3.3.3a only; nothing else waits on it.
- Test approach: n/a (human observation).

## Epic 0.3: Falsifier baseline
**Goal**: Test the claim the feature rests on (requirements.md "Primary problem (falsifiable)") before the large build, not at the end of it.

### Story 0.3.1: Baseline half of the falsifier
**As the** owner, **I want** today's pairwise output on the N-way fixture put in front of readers before the matrix and chart work starts, **so that** a falsified problem statement stops the large build early and the treatment half (MAN-7 in 5.1.3a) has a baseline to beat.
**Acceptance Criteria**:
- Fixture and baseline come from the unmodified build.
  - *Given* `testdata/nway/{quick,heap,merge}.txt` (Go benchmark output, 10 samples each, 6 benchmarks, built so Q1-Q3 have defined answers: one variant significantly better than the reference on at least 3 rows, one variant significantly slower than the reference on at least 1 row, and at least 1 row whose gap is not significant), *When* the unmodified CLI runs `--base quick.txt --head heap.txt` and `--base quick.txt --head merge.txt`, *Then* the two Markdown outputs are committed as `testdata/nway/baseline/pairwise-heap.md` and `pairwise-merge.md`, byte-identical on a second run, with the questions and an answer key in `testdata/nway/baseline/QUESTIONS.md`.
- The questions are the ones of the treatment half (requirements.md "Outcome metric"): Q1 which variant has the best overall result and against which reference, Q2 whether the gap on one named benchmark is significant, Q3 whether any variant is slower than the reference.
- Baseline reading (human, gate G0).
  - *Given* only the two pairwise comments (no artifact opened), *When* at least 3 readers (Tyler plus two others, or an explicit waiver with the reason) answer Q1-Q3 against a stopwatch, *Then* answers and seconds are recorded in requirements.md under "Falsifier baseline result".
  - Decision rule, one wording with requirements.md: if at least 2 of 3 readers answer all three correctly in under 60 seconds from the pairwise output, the problem claim is falsified, the plan stops after Phase 1 (tasks in flight finish; W6 onward does not start) and Tyler decides what is next; otherwise the build proceeds and 5.1.3a (MAN-7) asks the same questions of the N-way comment.
**Files**: `testdata/nway/{quick,heap,merge}.txt`, `testdata/nway/baseline/*`, `requirements.md`

##### Task 0.3.1a: Fixture inputs, pairwise baseline outputs and question sheet (S)
- Files: `testdata/nway/quick.txt`, `testdata/nway/heap.txt`, `testdata/nway/merge.txt`, `testdata/nway/baseline/pairwise-heap.md`, `testdata/nway/baseline/pairwise-merge.md`, `testdata/nway/baseline/QUESTIONS.md`
- Runs in W1 on the unmodified build, so the baseline is today's output by construction. The three input files are the same ones task 2.2.3a later renders as the N-way matrix (2.2.3a adds only `impl.txt` and the expected files), so the properties that make Q1-Q3 answerable must also hold in the N-way output (2.2.3a asserts them). `QUESTIONS.md` holds the three questions, the answer key and the regeneration commands (it does not touch `testdata/README.md`, which 0.1.1a owns in this wave).
- AC: as the first bullet above. Test approach: run each command twice and `cmp`; review the diff is additions only.

##### Task 0.3.1b: Baseline readers and decision (XS, wall-clock blocker, gate G0)
- Files: `requirements.md` (appends the section "Falsifier baseline result")
- Tyler runs the readers any time after 0.3.1a and before W6, records answers, seconds and the decision. W2-W5 do not wait (Phase 1 ships in every outcome); W6 onward does.
- AC: requirements.md names the readers (or the waiver reason), the seconds, the answers and the decision (`proceed` or `stop after Phase 1`). Test approach: n/a (human observation).

---

# PHASE 1: Dimensions and configuration

## Epic 1.1: Dimensions
**Goal**: Metrics carry derived dimensions without changing identity or schema (ADR-004, ADR-006).

### Story 1.1.1: `dims` on `Metric`
**As a** library consumer, **I want** an optional `dims` map, **so that** pivots can use dimensions while old files and readers keep working.
**Acceptance Criteria**:
- Additive, skipped when empty.
  - *Given* a `MetricSet` JSON without `dims`, *When* parsed and re-serialized, *Then* output bytes equal the input (existing round-trip tests untouched).
- Present dims round-trip; old reader ignores.
  - *Given* a metric with `dims {"impl":"quick","size":"1000"}`, *When* serialized, *Then* JSON contains `"dims":{"impl":"quick","size":"1000"}` with keys sorted, and parsing it with `dims` stripped yields the same identity key.
**Files**: `crates/benchdiff-core/src/model.rs`, `crates/benchdiff-core/tests/dimensions_import.rs`, `crates/benchdiff-core/src/notice.rs` (1.1.1c), `crates/benchdiff-core/src/lib.rs` and stub files (task 1.1.1b only)

##### Task 1.1.1a: Add `dims` + `with_dims` (S)
- Add field with `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]`; keep `Metric::new` signature; add `with_dims(self, Dims) -> Self`; `Metric::new` sets `dims: Default::default()`. `key()` unchanged. No importer edit is needed: verified that `Metric { .. }` is written as a literal only inside `Metric::new` (`model.rs:56`; `grep -rn 'Metric {' crates` finds, outside `model.rs`, only the return type of the `compare.rs` test helper `fn metric(..) -> Metric {`, which itself calls `Metric::new`), and every importer (`import/go.rs`, `gab.rs`, `json_formats.rs`) goes through `Metric::new`. `MetricSet { .. }` literals are not touched by this task (`MetricSet` gains no field).
- Files: `crates/benchdiff-core/src/model.rs` (with its `#[cfg(test)]` tests), `crates/benchdiff-core/tests/dimensions_import.rs` (created here with `importers_should_leave_metric_dims_empty_when_no_extraction_requested`)
- Test approach: unit tests above; `cargo test --workspace`; `make golden` passes with 0.1.1 expected files untouched.

##### Task 1.1.1b: Module stubs and `lib.rs` freeze (XS)
- Runs in W1 so no later same-wave task needs `lib.rs` or a stub file. Create empty stub modules `dimensions.rs`, `dimensioned.rs`, `notice.rs`, `config.rs`, `options.rs`, `matrix.rs`, `markdown_matrix.rs`, `svg/mod.rs` (declaring submodules `writer`, `text`, `style`, `delta`, `ranking`, `distribution`, each an empty file) and declare the top-level ones as `pub mod` in `lib.rs` once; later tasks fill the files and use module paths (`benchdiff_core::matrix::compare_matrix`). **Crate-root re-exports** are added only by 2.1.6a (matrix and `render_json_with_matrix`), 3.3.1c (`render_markdown_with`, `RenderExtras`, `render_matrix_markdown_with`) and 3.3.1b (`render_svgs` and the `SvgOptions` family) in `lib.rs`; no other task (3.3.1a included) edits `lib.rs`, they use module paths.
- Files: `crates/benchdiff-core/src/lib.rs`, `crates/benchdiff-core/src/{dimensions,dimensioned,notice,config,options,matrix,markdown_matrix}.rs`, `crates/benchdiff-core/src/svg/mod.rs`, `crates/benchdiff-core/src/svg/{writer,text,style,delta,ranking,distribution}.rs` (all empty stubs)
- AC: *Given* the stubs, *When* `cargo build --workspace` and `make golden` run, *Then* both succeed and default bytes are unchanged.
- Test approach: build + golden.

##### Task 1.1.1c: `Notice` and `NoticeClass` types (XS)
- Runs in W2, before the first producer (1.1.2d, W5; 2.1.2a, W6). Fills `notice.rs` with `Notice { class: NoticeClass, text: String }` and `NoticeClass` (`Degradation`, `Skipped`, `ReferenceAbsent`, `Invalid`, `Chart`, derived `Ord` in exactly that order, plus `NoticeClass::ALL`). Types only: `render_notices` (cap, overflow line) is 2.2.1b and nothing renders in this task.
- Files: `crates/benchdiff-core/src/notice.rs`
- AC: *Given* notices of mixed classes, *When* sorted with the derived `Ord`, *Then* the order is degradation, skipped, reference-absent, invalid, chart, stable within a class. Test approach: unit test.

### Story 1.1.2: Extraction rules
**As a** user, **I want** dimensions extracted from benchmark names by documented rules, **so that** I can pivot without editing benchmarks.
**Acceptance Criteria**:
- Default extractor handles `/k=v` and bracket groups. It does NOT touch a trailing `-N`: the Go importer already strips the GOMAXPROCS suffix at import (`import/go.rs`), so `procs` is never a dim in v1 (ADR-004 item 3, revised).
  - *Given* a `Metric` named `BenchmarkSort/impl=quick/size=1000` (as the Go importer produces it), *When* `extract` runs with defaults, *Then* `dims == {impl: quick, size: 1000}` and `residual == "BenchmarkSort"`.
  - *Given* the committed Go fixture `testdata/golden/head.go.txt` imported and extracted, *Then* no dim named `procs` exists.
  - *Given* `BenchmarkQuick-sort` or `BenchmarkX/size=10-8` (a name that keeps its dash, e.g. from a non-Go importer), *When* extracted, *Then* `-sort` is untouched and `-8` stays inside the value (`size` = `10-8`).
- User rule with named groups.
  - *Given* rule `impl=(?P<impl>[A-Za-z]+)Sort$` and name `BenchmarkQuickSort` (rule matched against the name), *When* applied, *Then* dims `{impl: BenchmarkQuick}` and an empty residual (the whole matched span is removed, here the entire name: ADR-004 items 4 and 5); a rule with no named group is rejected with `rule 'impl=...' has no named capture group`; the `<dim>` prefix must equal one of the pattern's named groups, else `rule prefix 'foo' is not a named group of its pattern (groups: impl)`. Rules are newline-delimited everywhere (repeatable CLI `--dimensions`, multiline Action `dimensions`, JSON array) because regexes may contain commas (`{1,3}`).
- Zero-match rule is a data-dependent problem (Risk Control split): notice by default, error under `dimensions-strict`.
  - *Given* rule `impl=impl=(?P<impl>\w+)` and 48 names none containing `impl=`, *When* extraction runs with `dimensions-strict`, *Then* error text is `dimension "impl": pattern ... matched 0 of 48 benchmark names (example: "BenchmarkSort/size=100")` and CLI exits 2.
  - *Given* the same input without `dimensions-strict`, *When* extraction runs, *Then* the rule is skipped, a diagnostic carries the same text plus `; rule skipped (set dimensions-strict to fail)`, and the run continues exit 0 (the report shows the notice in its body; downstream fallbacks are in 2.1.2 and 2.2.2). A rule that is an invalid pattern, has no named group, or names a prefix that is not a group stays a static exit-2 error either way.
**Files**: `crates/benchdiff-core/src/dimensions.rs` (1.1.2a, b, d), `crates/benchdiff-core/src/dimensioned.rs` (1.1.2c), `crates/benchdiff-core/Cargo.toml`, `Cargo.lock`, `crates/benchdiff-core/tests/dimensions_import.rs` (1.1.2d) (modules declared by 1.1.1b)
- Zero-config default extraction rules are stated once, here and in ADR-004 item 3, and repeated verbatim in `--help` for `--compare-by`/`--dimensions`, the README and `action.yml` descriptions.

##### Task 1.1.2a: Default extractor (M)
- Files: `crates/benchdiff-core/src/dimensions.rs`
- New `dimensions.rs`: `pub fn extract_defaults(name:&str)->Extracted{dims,residual}` per ADR-004 point 3 (hand-coded, no regex, no `-N` handling). Table-driven tests covering Go importer output (`quick-sort`; no `-8`), non-Go names with a dash (`size=10-8` kept whole), JMH-style `(size = 1000)`, Criterion `group/function/param` (no `=` so untouched), Google Benchmark `name/arg:1/real_time`.
- AC: *Given* `BenchmarkX/size=10/impl=a`, *When* extracted, *Then* dims `{size:10, impl:a}`, residual `BenchmarkX`; *Given* `BenchmarkX/size=10/impl=a-8`, *Then* `impl` = `a-8`.
- Test approach: ~15-row table test; property test that `residual` never contains an extracted span.

##### Task 1.1.2b: User rules via `regex-lite` (M)
- Files: `crates/benchdiff-core/src/dimensions.rs`, `crates/benchdiff-core/Cargo.toml`, `Cargo.lock`
- Add `regex-lite = "0.1.9"` (default-features off if possible). `Rule::parse("dim=pattern")`, caps: pattern <= 256 chars, size limit; named groups become dims; matched span removed from residual; first match per dim wins; invalid pattern -> error with rule text. Re-measure wasm via 0.1.2 script.
- AC: *Given* rule `impl=/impl=(?P<impl>\w+)` and `BenchmarkSort/impl=quick`, *When* applied, *Then* dims `{impl: quick}` residual `BenchmarkSort`.
  - *Given* a 300-char pattern, *When* parsed, *Then* error `pattern too long (300 > 256)`.
- Test approach: unit tests; size script prints new size (expect +~55 KB).

##### Task 1.1.2c: `Dimensioned`, `Residual`, `identity`, accessors, `dimension_summary` (S)
- Files: `crates/benchdiff-core/src/dimensioned.rs` (stub from 1.1.1b)
- Split from the old 1.1.2c (round 2 size review: one task held the data model, the apply pipeline, collision and strict logic and the discoverability helpers in one file). It runs in W4 in its own file, so it does not collide with 1.1.2b in `dimensions.rs`. `Dimensioned` is construct-only: private fields holding `Vec<(Metric, Residual)>` (the metric and its residual live in one tuple, so a filter or reorder cannot desync them) plus the set's `skipped` entries; the only constructors are `Dimensioned::identity(set)` (residual = name, empty dims: what a `label=path` file gets when no rules are given) and the crate-private `from_parts`, called solely by `dimensions::apply` (1.1.2d). Public accessors `entries() -> impl Iterator<Item=(&Metric,&Residual)>` and `skipped_entries()` likewise. `Residual` is a newtype; `Metric` gets no residual field and `name` is never rewritten. `project_by_axis` (2.1.2a) consumes `Dimensioned`, so nothing re-extracts. Also `dimension_summary() -> Vec<DimensionSummary>` and `available_dimensions_text()` (below).
- AC: *Given* a `Dimensioned` built outside the crate, *Then* the only available constructor is `identity` (compile-fail test) and `entries()` yields each metric with its own residual, including after the source set was deduplicated by the caller.
  - Detected dimensions are discoverable (zero-config default rules: `key=value` path segments such as `a/impl=quick`, and bracket groups `[k=v, k2=v2]` / `(k = v)`; nothing else, in particular not Criterion `group/function/param` ids, which carry no `=`). *Given* a `Dimensioned` whose metrics carry dims from names `Sort/impl=quick/size=10`, `Sort/impl=heap/size=10`, `Setup` (built in the test through `dimensions::apply` once 1.1.2d exists, and through the test-only `from_parts` before that), *When* `dimension_summary()` runs, *Then* it returns `impl` (values `quick, heap` in first-seen order, 2 benchmarks) and `size` (values `10`, 2 benchmarks), plus the pseudo-dimension `group`; *Given* names with no `=`, *Then* it returns only `group`. `available_dimensions_text()` renders it as `available dimensions: impl (2 values), size (1 value), group` or `available dimensions: group only (default rules read /key=value segments and [k=v] groups; add a 'dimensions' rule or use labelled variants)`; this one string is the suffix used by every axis error below. The count of metrics with empty dims is `unmatched` (used by `--print-dimensions`, 2.2.2b).
- Test approach: unit tests; the summary tests construct `Dimensioned` through `from_parts` under `#[cfg(test)]`, which keeps this task independent of 1.1.2d (same-wave ordering is not needed, 1.1.2d runs one wave later).

##### Task 1.1.2d: `apply`: residual identity, collision check, strict mode, diagnostics (M)
- Files: `crates/benchdiff-core/src/dimensions.rs`, `crates/benchdiff-core/tests/dimensions_import.rs`
- `pub fn apply(set:MetricSet, rules:&Rules, strict:bool)->Result<(Dimensioned,Diagnostics),DimError>`, built on 1.1.2a/b extraction and the 1.1.2c type. `apply` sets `dims` on each metric and runs the set's `skipped` keys through the same extraction so a skipped `Sort/impl=merge` is attributed to variant `merge` and row `Sort` (surfaced by `skipped_entries()`). When rules are given, `apply` runs per labelled file, and mixed use of `--variant` with `--dimensions` is defined as exactly that (rules apply to every file; a label supplying the same dim as a rule is the conflict error in 2.1.2). Collision check on `(group, residual, measure, dims)` naming both raw names; strict errors on any unmatched; non-strict collects first 10 unmatched; zero-match-rule text per above. `apply` returns the data-dependent problems (zero-match rule, collision) as `Diagnostics` entries (each convertible to a `Notice` of class `Degradation`, type from 1.1.1c) when `strict` is false and as `DimError` when `strict` is true; in the non-strict collision case it keeps the first metric in input order, drops the later one and records `kept A, dropped B`.
- AC: *Given* a `Dimensioned` built by `apply`, *Then* it exposes no public field or constructor other than `apply`/`identity`, and `entries()` yields each metric with its own residual.
  - *Given* names `a/impl=1` and `a-impl=1` both reducing to the same identity, *When* applied with `strict`, *Then* error names both originals; *When* applied without `strict`, *Then* no error, the first is kept, the second is dropped, and the diagnostic names both (`kept 'a/impl=1', dropped 'a-impl=1'`).
  - *Given* skipped key `Sort/impl=merge`, *When* applied with the default extractor, *Then* `skipped_entries()` yields (dims `{impl: merge}`, residual `Sort`).
  - *Given* strict and an unmatched name `Foo`, *When* applied, *Then* error lists `Foo`.
  - *Given* every importer fixture under `testdata/<format>/` and the default rules, *When* applied, *Then* no dim named `procs` exists and no importer-produced name is rewritten (`dimensions_import.rs`, `apply_should_extract_dims_from_every_importer_fixture_without_procs_when_defaults`).
- Test approach: unit tests.

## Epic 1.2: Configuration
**Goal**: One vocabulary across Action inputs, CLI flags and file keys; precedence `defaults < file < flags` in the binary (ADR-001).

### Story 1.2.1: `OptionsOverlay` refactor
**As a** maintainer, **I want** flags parsed into an overlay, **so that** a config-file layer can slot in without "flag equals default" ambiguity.
**Acceptance Criteria**:
- Behaviour unchanged.
  - *Given* any existing CLI invocation, *When* run after the refactor, *Then* all 0.1.1 expected goldens still match.
- Method-note plumbing has one owner (round 2 engineering blocker). Two later tasks change `render_method` (`markdown.rs`) and need new data on the public serialized `Summary` (`compare.rs`): 1.2.5a (W3, active threshold overrides) and 1.2.3a (W5, config/flag sources). Task 1.2.1b owns both edits in W2, before either; 1.2.5a and 1.2.3a only fill the fields from their own files.
  - *Given* default options and no config, *When* a report renders, *Then* `render_markdown` and `render_json` bytes equal `expected/default.*` (the new `Summary` fields are `Default`, `skip_serializing_if` empty, and `render_method` prints nothing for them).
  - *Given* a `Summary` with `threshold_overrides = {"net/Parse": 20}`, *When* `render_method` runs, *Then* the Method note gains the sentence `Per-metric thresholds: net/Parse 20%.` (keys in sorted order); *Given* `config_sources = [{key:"threshold", value:"3", source:"config"}, {key:"sort", value:"name", source:"flag"}]`, *Then* it gains `Settings not at their defaults: threshold=3 (config), sort=name (flag).` (the caller supplies the rendered value, because `Summary` does not carry every setting, e.g. `sort`); with both empty the note is byte-identical to today.
**Files**: `crates/benchdiff/src/main.rs`, `crates/benchdiff-core/src/options.rs` (overlays only; `Options` itself does NOT move out of `compare.rs`, decided here so 1.2.5a/2.1.1a never race a move), `crates/benchdiff-core/src/compare.rs` and `crates/benchdiff-core/src/markdown.rs` (task 1.2.1b only)

##### Task 1.2.1a: Introduce `OptionsOverlay` (M)
- Files: `crates/benchdiff/src/main.rs`, `crates/benchdiff-core/src/options.rs`
- `OptionsOverlay { threshold, min_abs, sort, max_rows, only_changed, ... : Option<_> }`, `apply(&self,&mut Options)`; `parse_args` fills an overlay instead of mutating; `Options::default()` unchanged. `Options` stays the pair-kernel struct: matrix settings go in `MatrixOptions` (`matrix.rs`) and chart settings in `SvgOptions` (`svg/mod.rs`), each with its own overlay type in `options.rs` added by 2.2.2a / 3.3.1a, so the kernel's `Options` does not grow.
- AC: *Given* flags `--threshold 3`, *When* overlay applied to defaults, *Then* `threshold == 3.0` and all other fields equal `Options::default()`.
- Test approach: unit tests on overlay; `make golden`.

##### Task 1.2.1b: Method-note plumbing: `Summary` fields and `render_method` (S, Refactor-first)
- Files: `crates/benchdiff-core/src/compare.rs`, `crates/benchdiff-core/src/markdown.rs`
- Runs in W2, before 1.2.5a (W3) and 1.2.3a (W5), and is the only task that edits `render_method` (`markdown.rs:329`) and the `Summary` struct (`compare.rs:130`). Add to the public `Summary`: `threshold_overrides: BTreeMap<String,f64>` and `config_sources: Vec<ConfigSource>` (`ConfigSource { key, value, source }`, all `String`, defined `pub` in `compare.rs` next to `Summary` and named from `main.rs` as `benchdiff_core::compare::ConfigSource`; no crate-root re-export, 1.2.1b is the only task that defines it), each `#[serde(default, skip_serializing_if = "...::is_empty")]` so default JSON stays byte-identical; the one `Summary { .. }` literal in `compare()` (`compare.rs:217`) gets `..Default::default()` for them. `render_method` appends the two sentences from the AC above only when the field is non-empty, using the same `fmt_num` helper as the existing sentence. `markdown_matrix.rs` (2.2.1a) reads the same fields from `MatrixSummary`, which copies them from `Summary` data. Fills nothing: 1.2.5a sets `threshold_overrides` from `Options` in `compare()`; 1.2.3a sets `config_sources` from `main.rs` after layering.
- Test approach: unit tests per AC; `cmp` of both outputs against `testdata/golden/expected/*` (from 0.1.1a, W1; the node/cargo suites that assert them are 0.1.1b, same wave); `make golden` unchanged.

### Story 1.2.2: Parse `benchdiff.config.json`
**As a** user, **I want** a validated config file, **so that** typos are caught.
**Acceptance Criteria** (core keys only: the keys that exist at this slot are `version`, `$schema`, `threshold`, `thresholds`, `min-abs`, `sort`, `max-rows`, `only-changed`, `fail-on-regression`, `title`, `tag`, `format`, `base-format`; matrix keys are added and tested by 2.2.2a, chart keys by 3.3.1a, each extending the same key list, so this task never needs a later wave's types):
- Unknown key suggestion.
  - *Given* `{"version":1,"treshold":10}`, *When* parsed, *Then* error `unknown key 'treshold'; did you mean 'threshold'?`.
- Enum errors list valid values.
  - *Given* `{"version":1,"sort":"size"}`, *When* parsed, *Then* error `sort: "size" is not valid; use input, name, delta`.
- Input-format keys (requirements: "output formats" is read as the importer selection `format`/`base-format`, which are config-governed today; the output paths `--markdown`/`--json` are CLI/Action outputs and deliberately not config keys, recorded in requirements.md).
  - *Given* `{"version":1,"format":"go","base-format":"benchdiff"}`, *When* parsed, *Then* the overlay has format Go and base format Benchdiff; *Given* `{"version":1,"format":"golang"}`, *Then* error `format: "golang" is not valid; use auto, benchdiff, go, criterion, cargo-bench, jmh, hyperfine, googlebench, gab-smaller, gab-bigger, bmf`.
- Parse errors name file, line and column (AC-S5-2).
  - *Given* a file `benchdiff.config.json` containing `{"version":1,\n  // note\n  "threshold":3}`, *When* parsed through `parse_config(text, name, Json)`, *Then* the error reads `benchdiff.config.json: line 2 column 3: <serde_json message>; the file is strict JSON (no comments)` and the exit code is 2; a type error reads `benchdiff.config.json: line 7 column 12: invalid type: string "high", expected f64`. The name is the file's own base name (the Action stages it under that name, 1.2.4), so the CLI and the Action print the same text.
- `$schema` is accepted and ignored (editors validate against the published schema, 5.1.5a).
  - *Given* `{"$schema":"https://raw.githubusercontent.com/tstapler/benchdiff/v0/benchdiff.config.schema.json","version":1,"threshold":3}`, *When* parsed, *Then* no error and the overlay is the same as without `$schema`; *Given* `"$schema": 3`, *Then* `$schema: expected a string`. `--print-config` does not echo `$schema`. `$schema` is the one config key with no flag or input (allowlisted in the vocabulary test, 4.2.1).
- Valid file maps to overlay.
  - *Given* `{"version":1,"threshold":3,"sort":"name","thresholds":{"default":4,"net/Parse":20}}`, *When* parsed, *Then* the overlay has `threshold` 4.0 (the `thresholds.default` entry wins over the plain key only if the plain key is absent; both present is an error `threshold and thresholds.default both set`), sort Name and one override `net/Parse` = 20.
**Files**: `crates/benchdiff-core/src/config.rs`

##### Task 1.2.2a: `config.rs` (M)
- Files: `crates/benchdiff-core/src/config.rs`
- `Config` (all `Option`, kebab-case via serde rename, `deny_unknown_fields`), `parse_config(text, ConfigFormat::Json)`, nearest-name suggestion (edit distance <= 2), `version` must be 1, per-metric thresholds `{"thresholds":{"default":5,"net":8,"net/Parse":3}}` (key `default` -> `threshold_pct`; other keys -> `threshold_overrides`, a key is `group/name` or a bare `group`, validated as finite and >= 0; applied by the kernel via task 1.2.5a), `parse_config(text, file_name, ConfigFormat)` (the file name is only used in error text), `Config::into_overlay`. A missing `version` is an error (`config: missing required key 'version'`), so a canonical results file passed by mistake (it has `schema`, not `version`) is rejected with a clear message rather than parsed.
- AC as above; also `version: 2` -> `config version 2 is newer than supported (1)`. Key list is a single table in `config.rs` that 2.2.2a and 3.3.1a extend (the suggestion search reads it).
- Test approach: unit tests incl. proptest-free table.

### Story 1.2.3: CLI `--config` and `--print-config`
**As a** CLI user, **I want** to load and inspect config, **so that** I can debug precedence.
**Acceptance Criteria**:
- Layering.
  - *Given* config `{"version":1,"threshold":3}` and flags `--config c.json`, *When* run, *Then* effective threshold is 3; *Given* also `--threshold 7`, *Then* 7.
- `--print-config` (decided format: JSON, one object; ux.md S3 is aligned to it). Source labels are exactly `default`, `config`, `flag`; an Action input is a flag to the binary, so inputs and flags share the label `flag` (decision on validation gap G-6: ADR-001 item 2 already collapses inputs into flags, so the binary cannot tell them apart, and the Method note says `flag`).
  - *Given* the above, *When* `--print-config` runs, *Then* stdout is JSON `{"config-file":"c.json","config":{...every effective key...},"sources":{"threshold":"flag"}}` where `sources` lists only keys whose source is not `default` (`config` or `flag`), containing `"threshold": 7` and `{"threshold":"flag"}`; exit 0, no report rendered, no other writes.
- Missing explicit path.
  - *Given* `--config nope.json`, *When* run, *Then* exit 2 with `config file not found: nope.json`.
**Files**: `crates/benchdiff/src/main.rs`, `crates/benchdiff/tests/config_cli.rs`

##### Task 1.2.3a: Wire config into the binary (S)
- Files: `crates/benchdiff/src/main.rs`, `crates/benchdiff/tests/config_cli.rs`
- Apply defaults -> file overlay -> flag overlay; record sources; implement `--print-config`; fill `Summary.config_sources` (field and `render_method` sentence are owned by 1.2.1b; this task edits neither `compare.rs` nor `markdown.rs`) so the Method note lists non-default sources in the same vocabulary (`threshold=3 (config)`, `sort=name (flag)`) only when a config was supplied (default output unchanged).
- Test approach: integration tests spawning the binary; `make golden`.

### Story 1.2.4: Action layering (stage config; unset-able inputs)
**As a** Action user, **I want** the file to apply unless I set an input, **so that** precedence works.
**Acceptance Criteria**:
- Defaults dropped.
  - *Given* `action.yml`, *When* inspected, *Then* `threshold`, `min-abs`, `sort` have no `default:` and their descriptions state `default 5`, `default 0`, `default delta`.
- No tunable flags when unset.
  - *Given* all-empty tunable inputs, *When* `buildArgs` runs, *Then* argv contains no `--threshold`, `--min-abs`, `--sort`.
- Config staged under its own base name (so the binary's error text and `--print-config` name the file the user wrote).
  - *Given* input `config: benchdiff.config.json` existing in the workspace, *When* `main` stages files, *Then* argv contains `--config /work/config/benchdiff.config.json` and that file has identical bytes (basename characters outside `[A-Za-z0-9._-]` become `_`).
- Built-in defaults equal the removed `action.yml` defaults (guards the default path; pre-mortem failure 5).
  - *Given* the pre-change `action.yml` defaults as an explicit table (`format` auto, `threshold` 5, `min-abs` 0, `sort` delta, `title` Benchmark comparison, `tag` default, `only-changed` false, `fail-on-regression` off, `max-rows` as then documented), *When* a Rust test builds `Options::default()` and the CLI's parsed defaults, *Then* every row of the table equals the binary default; a row that cannot match (for example a CLI default title that differs from the Action's) stays as an `action.yml` `default:` and the test records it as an explicit exception.
- Default behaviour identical, even with a results file lying around.
  - *Given* no `config` input, no tunable inputs and a workspace containing a canonical results file named `benchdiff.json`, *When* the Action runs on the golden fixtures, *Then* output equals the 0.1.1 expected files and `buildArgs` emits no `--config`.
**Files**: `action.yml`, `index.js`, `test/action.test.js`, `test/golden.test.js`
- Decision (ADR-001 amended): NO auto-discovery. The config is read only from an explicit `config` input (CLI `--config`). Rationale: `benchdiff.json` is also a natural name for a committed canonical results file, which `deny_unknown_fields` would reject (exit 2) in a workflow that changed nothing, violating "opt-in / byte-identical". Docs use the conventional name `benchdiff.config.json` everywhere (this plan, ux.md and ADR-001 included). A missing explicitly named file is an error.

##### Task 1.2.4a: Implement (M)
- Files: `action.yml`, `index.js`, `test/action.test.js`, `test/golden.test.js`, `crates/benchdiff/tests/config_cli.rs`
- Edit `buildArgs` to emit flags only when raw input non-empty; remove `input(..., default)` fallbacks for config-governed names; stage config; update 0.1.3 snapshots intentionally (diff is the dropped default flags only).
- Test approach: updated unit tests; `make golden` with expected files untouched.

### Story 1.2.5: Kernel prerequisites (Refactor-first, before the seam)
**As a** maintainer, **I want** per-metric thresholds and a chart-sample side channel inside the pair kernel while the default path stays byte-identical, **so that** the `thresholds` config key does something (fail-fast: no validated-but-ignored key) and charts have data (ARCH-1, ADV-3).
**Acceptance Criteria**:
- Default unchanged.
  - *Given* `Options::default()` (empty `threshold_overrides`, `keep_samples` false), *When* every 0.1.1 golden case runs, *Then* outputs equal the expected files and the existing `compare.rs` tests pass untouched.
- Per-metric lookup.
  - *Given* `threshold_pct = 5` and `threshold_overrides = {"net/Parse": 20}`, *When* metric `(group "net", name "Parse")` changes by +12% and `(group "net", name "Dial")` changes by +12%, *Then* `Parse` is Unchanged and `Dial` is Regressed. Lookup order: exact `group/name` (just `name` when the group is empty) > `group` (only for a non-empty group) > default. Measure is not part of the key.
  - *Given* `threshold_pct = 5` and `threshold_overrides = {"net": 8, "net/Parse": 20}`, *When* the four metrics below each change by the stated amount, *Then* the verdicts are: `(net, Parse, +12%)` Unchanged (exact key 20 wins over group key 8); `(net, Dial, +6%)` Unchanged (group key 8 applies; it would be Regressed under the default 5); `(net, Dial, +12%)` Regressed (group key 8 exceeded); `(db, Query, +6%)` Regressed (no key, default 5). Test `threshold_for_should_prefer_exact_over_group_over_default` asserts the three-level order directly, and `compare_should_apply_group_threshold_when_only_group_key_matches` asserts the table.
- Config end-to-end (asserted by the integration test added in 1.2.3a, since the binary wiring lands there).
  - *Given* `{"version":1,"thresholds":{"default":3,"net/Parse":20}}` passed via `--config`, *When* run, *Then* `threshold_pct == 3` and the `Parse` override is in effect; `--print-config` lists both with source `config`.
- Samples side channel, JSON unchanged.
  - *Given* `keep_samples = true`, *When* `compare()` runs, *Then* each changed `Row.samples` holds the base (pre-scaled to the head unit) and head sample vectors; *Given* default options, *Then* `Row.samples` is empty. In both cases `render_json` bytes equal `expected/default.json` (the field is `#[serde(skip)]` and `Default`; `Row` derives no `PartialEq`, so nothing else changes).
  - *Given* base samples in `us` and head samples in `ns` for one metric (`scale` 1000 from `units::factor`) and `keep_samples = true`, *When* `compare()` runs, *Then* `Row.samples.base` equals the base samples multiplied by that same scale (stored in the head unit, identical to what `evidence` compares) and `Row.samples.head` is unscaled, so distribution lanes share one axis.
  - *Given* a New or Removed row (built by `one_sided_row`, `compare.rs:238`), *When* `keep_samples = true`, *Then* `Samples` stays empty for both sides (there is no pair to plot; charts skip such rows).
**Files**: `crates/benchdiff-core/src/compare.rs`, `crates/benchdiff-core/src/options.rs`, `crates/benchdiff-core/tests/json_compat.rs`

##### Task 1.2.5a: Per-metric thresholds in the kernel (S, Refactor-first)
- Files: `crates/benchdiff-core/src/compare.rs`, `crates/benchdiff-core/src/options.rs`
- Add `Options.threshold_overrides: BTreeMap<String,f64>` and `Options::threshold_for(&Metric)->f64`; `changed_row` replaces its direct `opts.threshold_pct` read (`compare.rs:329`) with `opts.threshold_for(head)` (lookup `group/name` > `group` > default); add `threshold_overrides` to `OptionsOverlay`. Struct-literal sites use `..Default::default()`. `compare()` copies `opts.threshold_overrides` into `Summary.threshold_overrides` (the field and the Method-note sentence were added by 1.2.1b in W2; this task edits only `compare.rs` and `options.rs`). AC: *Given* overrides `{"net/Parse": 20}`, *When* `compare()` runs and the report renders, *Then* the Method note carries `Per-metric thresholds: net/Parse 20%.`; with none, the bytes equal `expected/default.md`.
- Test approach: unit tests above; `make golden` unchanged (0.1.1 expected files untouched).

##### Task 1.2.5b: `Samples` side channel (S, Refactor-first)
- Files: `crates/benchdiff-core/src/compare.rs`, `crates/benchdiff-core/tests/json_compat.rs` (created here: default-bytes and samples-never-serialized tests)
- Add `Options.keep_samples: bool` (internal; not a config key; chart requests set it) and `#[serde(skip)] pub samples: Samples` on `Row`; `changed_row` fills it only when `keep_samples`, with `base` already multiplied by the `base_scale` that `evidence` uses (head unit); `one_sided_row` leaves it empty. Runs after 1.2.5a (same file).
- Test approach: JSON-bytes-equal test with `keep_samples` on and off; `make golden`.

---

# PHASE 2: N-way comparison

## Epic 2.1: Matrix core
**Goal**: Reference-based N-way report built from the pair kernel (ADR-002, 005, 006).

### Story 2.1.1: Pair-kernel seam
**As a** maintainer, **I want** `changed_row` reusable, **so that** the matrix inherits the verdict precedence exactly.
**Acceptance Criteria**:
- Rename-only, no logic change (kernel semantics stay exactly as today: direction `better` comes from the HEAD-side metric, threshold from `Options::threshold_for(head)`).
  - *Given* the existing `compare.rs` tests, *When* run after the rename to `compare_pair` (`pub(crate)`) and helper visibility changes, *Then* all pass and 0.1.1 goldens are unchanged.
- Reference direction is the matrix's job, not the kernel's (ADR-005 amended; asserted in 2.1.3a).
  - *Given* a matrix cell whose reference and variant `better` agree, *When* the kernel is called, *Then* `compare_pair(reference, variant)` runs on the unmodified variant metric: after the equality check `head.better == reference.better`, so the kernel's read of `head.better` already orients by the reference and nothing is cloned or overwritten. Mismatches never reach the kernel (asserted in 2.1.3).
**Files**: `crates/benchdiff-core/src/compare.rs`, `crates/benchdiff-core/src/markdown.rs`

##### Task 2.1.1a: Promote seam (XS)
- Files: `crates/benchdiff-core/src/compare.rs`, `crates/benchdiff-core/src/markdown.rs`
- `changed_row` -> `pub(crate) fn compare_pair(base,head,opts)->Row`; `code`, `flatten`, `value`, `fmt_pct`, `fmt_num` -> `pub(crate)`. No logic change (the two refactors that do change the kernel, 1.2.5a/b, are already merged and golden-protected before this task).
- Test approach: `cargo test`; `make golden`.

### Story 2.1.2: Build variants
**As a** user, **I want** variants from labelled files or from an axis dimension, **so that** both input styles work.
**Acceptance Criteria**:
- Split by axis.
  - *Given* one `MetricSet` with metrics `Sort/impl=quick`, `Sort/impl=heap`, `Sort/impl=merge` (dims set) and `compare-by=impl`, *When* projected, *Then* 3 `Variant`s labelled `quick`,`heap`,`merge` (first-seen order) each containing metric residual `Sort`.
  - Deterministic default reference when variants come from names. *Given* the same metrics listed in the order `heap`, `quick`, `merge` and no explicit `reference`, *Then* the variants are `heap, quick, merge` and the reference is `heap` (first encountered walking `MetricSet.metrics`; with several `head` files, files in the order given and glob matches sorted by path; a tie cannot occur within one pass and would break by label, byte-wise). *Given* the identical input projected twice, or the files passed in the same order, *Then* the order and the reference are identical. The report states the choice (`reference: heap (first listed; set 'reference' to change)`), and the README advises setting `reference` whenever benchmark order is not under the user's control.
- Labelled files never merged.
  - *Given* files `quick=a.txt` and `heap=b.txt` both containing `BenchmarkSort`, *When* loaded, *Then* two variants, no duplicate-metric error.
- Label/dim conflict.
  - *Given* label `quick=a.txt` and parsed dim `impl=quick` on `compare-by=impl`, *When* both supply the axis, *Then* error `axis 'impl' set by both file label and name rule`.
- Row identity with two dimensions.
  - *Given* metrics `Sort/impl=quick/size=10`, `Sort/impl=quick/size=1000`, `Sort/impl=heap/size=10`, `Sort/impl=heap/size=1000` and `compare-by=impl`, *When* projected, *Then* 2 rows labelled `Sort [size=10]` and `Sort [size=1000]` (residual plus non-axis dims in key order), each with 2 cells, and no collision error.
  - *Given* the same input with `group-by=` listing a dim that does not include `size`, *When* projected, *Then* error `rows collide after dropping non-axis dimension 'size'; add it to group-by` naming both original names.
  - *Given* `compare-by=measure`, *Then* static exit-2 error `compare-by 'measure' is not an axis (units differ per measure); available dimensions: impl (3 values), size (2 values), group`. *Given* `group-by=foo` where no benchmark carries `foo`, *Then* the same data-dependent class and suffix as an absent `compare-by` axis (next bullets).
- Built-in pseudo-dimension `group`.
  - *Given* metrics in groups `pkgA` and `pkgB` with the same names and `compare-by=group`, *When* projected, *Then* two variants `pkgA`, `pkgB`. (`measure` is not an axis: units differ by construction; recorded in requirements.md.)
- Labelled files and rules.
  - *Given* `--variant quick=a.txt` with no rules, *Then* the file's metrics get identity residuals (`residual = name`) and no dims; *Given* the same plus `--dimensions`, *Then* the rules are applied to each file via `dimensions::apply` (1.1.2d); a label-supplied axis that a rule also supplies is the `axis ... set by both` error above.
- Metrics lacking the axis dimension.
  - *Given* `compare-by=impl` and a metric `Setup` with no `impl` dim, *When* projected, *Then* it is excluded from every variant (no default variant is invented) and listed in the diagnostics as `1 benchmark has no 'impl' dimension (first 10: Setup)`; the run still succeeds; if no metric has the axis dim (a data-dependent problem) the text is `compare-by 'impl': no benchmark carries this dimension; available dimensions: size (2 values), group` (the suffix is `available_dimensions_text()` from 1.1.2c; with nothing detected it explains the zero-config rules and names the two ways out, a `dimensions` rule or labelled variants): exit 2 under `dimensions-strict`; otherwise it is a notice and the run degrades (two-way report if `base`/`head` were given, else a notice-only report, exit 0).
- Reference label absent from the axis values (data-dependent; label sets from `--variant` are static and stay exit 2).
  - *Given* `compare-by=impl` with `reference=slow` and axis values `quick, heap`, *When* projected without `dimensions-strict`, *Then* no matrix is built, a notice `reference 'slow' is not one of: quick, heap; no comparison made` is returned and the report is notice-only (exit 0); with `dimensions-strict` the same text is an exit-2 error (message wording is the single string used everywhere: `unknown reference 'slow'; valid: quick, heap`, ux.md aligned).
- Projection never rewrites source names.
  - *Given* a `Dimensioned` set, *When* projected, *Then* variant metrics are clones with `name = residual`; the source set's `Metric.name` values are unchanged.
**Files**: `crates/benchdiff-core/src/matrix.rs`, `crates/benchdiff-core/src/dimensions.rs` (axis helpers only). Data-dependent axis problems are returned as `Notice`s of class `Degradation` (type from 1.1.1c).

##### Task 2.1.2a: `Variant`, `VariantLabel`, `project_by_axis` (M)
- Files: `crates/benchdiff-core/src/matrix.rs`, `crates/benchdiff-core/src/dimensions.rs`
- Newtype `VariantLabel` (`^[A-Za-z0-9._-]{1,40}$`); pure function from `Dimensioned` + axis + `group-by` dims to `Vec<Variant>`; deterministic order; row identity `(group, residual, measure, non-axis dims)` per ADR-004 item 5 (`group-by` selects which non-axis dims participate; default all); `group` accepted as a pseudo-dimension. Consumes `Dimensioned::entries()` and `skipped_entries()` (so each `Variant` carries its `skipped` keys for 2.1.3's Invalid cells); never re-extracts. Lookup of per-metric thresholds in the matrix is keyed `group/residual` (see 2.1.3).
- Test approach: unit tests incl. missing-in-some-benchmarks cases.

### Story 2.1.3: `compare_matrix`
**As a** reviewer, **I want** N variants compared to the reference, **so that** one report shows all of them.
**Acceptance Criteria**:
- Cells vs reference.
  - *Given* variants `quick`(ref), `heap`, `merge` with `Sort` median 100/130/80 ns and 10 samples each (separated), *When* `compare_matrix` runs, *Then* `heap` cell is Regressed `+30.0%`, `merge` Improved `-20.0%`, decided by `mann-whitney`.
- Regression is defined for N-way (ADR-005 item 7; round-1 triad blocker).
  - *Given* any matrix cell, *Then* `Regressed` means exactly: the pair kernel (threshold, `min-abs`, Mann-Whitney / interval precedence, then Holm) judges the variant significantly worse than the **reference column**, oriented by the reference's `better`; `Improved` is the mirror; nothing is ever compared across non-reference variants and nothing is judged over time. The rendered words are "slower/faster than reference" (never "regression" in prose); the enum and the Action output names keep `regressed`/`improved` for compatibility.
  - *Given* `matrix_failed(&report, None)` (no `fail-on-regression`), *Then* it is false whatever the cells are; *Given* `Some(10.0)` and a post-Holm `Regressed` cell over 10%, or any `Invalid` cell, *Then* true; *Given* `Some(10.0)` and only cells `Regressed` at +7%, *Then* false. The explicit-reference requirement is enforced where options are validated (2.2.2a), not here, so this function stays a pure function of the report.
- Missing cell.
  - *Given* `merge` lacks `Search`, *When* compared, *Then* its `Search` cell is `Missing` (rendered `—`, U+2014, the glyph already used in `markdown.rs`), never 0, and `ranking` omits it.
- Missing reference.
  - *Given* the reference lacks `Insert`, *When* compared, *Then* that row has no deltas and is listed in `matrix.reference_absent`.
- Direction from reference.
  - *Given* reference `better=lower` and variant `better=higher` for the same metric, *When* compared, *Then* the cell is `Comparison::DirectionMismatch` (shown raw) and excluded from counts and geomean, and `compare_pair` is not reached for it (asserted with a test seam: a counter or a panicking kernel stub in the test build).
  - *Given* the same three variants `quick`, `heap`, `merge` passed in the order `[quick, heap, merge]` and again in each permutation, with `reference: quick` set explicitly, *When* compared, *Then* every variant's per-cell delta, verdict, p-value and rank are identical across permutations (only the column order differs); without an explicit reference the first variant is the reference and the headline names it (2.2.1).
  - *Given* matching directions (`lower`), reference 100 and variants 130 and 80, *When* compared, *Then* `heap` (130) is Regressed and `merge` (80) Improved, and `MatrixRow.ranking` is `[merge, quick, heap]`; *Given* the same values with `better=higher` on both, *Then* verdicts flip and ranking is `[heap, quick, merge]`. Ranking and `Row.better`-style orientation always equal the reference's.
- Incomparable units.
  - *Given* ns vs B/op for one metric, *When* compared, *Then* cell `Comparison::Incomparable`, shown raw, excluded from geomean.
- Per-metric thresholds apply in the matrix.
  - *Given* `threshold_overrides = {"Sort": 20}` (key = `group/residual`, bare `residual` when group is empty), `threshold_pct = 5`, and every `impl` variant of `Sort` +12% against the reference, *When* compared, *Then* all `Sort` cells are Unchanged while other benchmarks at +12% are Regressed.
  - *Given* a group-level override `{"pkgA": 20}` and variants whose metrics sit in group `pkgA` (reference variant's group), *When* compared at +12%, *Then* those cells are Unchanged and cells in group `pkgB` at +12% are Regressed (lookup `group/residual` > `group` > default, the same function as the pair kernel). A key written against the raw name (`Sort/impl=quick`) matches nothing and is reported by the config validation as `threshold key 'Sort/impl=quick' matches no benchmark (matrix keys are group/residual or group)`. Under `compare-by=group` the lookup `group` is the reference variant's group.
- Invalid and skipped metrics are surfaced, not dropped.
  - Source of `Invalid`: the kernel never returns it. A cell is `Invalid(reason)` when the reference has the metric, the variant has no such metric in its `metrics`, and the variant's `skipped` list (per labelled file; for axis splitting, the file's skipped names extracted through `Dimensioned::skipped_entries()` and attributed to their variant/row) contains it. The importer caps `skipped` at `MAX_SKIPPED` (50), so beyond the cap the cell is `Missing`, not `Invalid`. A skipped reference metric makes its row `reference_absent`.
  - *Given* variant `merge` has a `Sort` series whose samples are all NaN (the importer drops NaN lines and registers the series as skipped only when no usable sample remains, `import/go.rs`), *When* compared, *Then* the cell is `Comparison::Invalid(reason)` (rendered `⚠️ invalid`, never `—`), the matrix summary lists `merge: 1 invalid`, and `matrix_failed(&report, Some(pct))` returns true (same as two-way `exceeds_failure_threshold` tripping on Invalid).
- Samples for charts.
  - *Given* `keep_samples`, *When* compared, *Then* each `PresentCell.samples` holds the reference samples as `base` and that variant's samples as `head`, with `base` pre-scaled to the variant's unit exactly as in 1.2.5b (`#[serde(skip)]`); default JSON bytes unchanged (see 2.1.6).
- Presentation settings reach the report (round 2: how `max-rows`, `only-changed`, `title` and `tag` get to the matrix renderer and to the chart cap).
  - Signature: `compare_matrix(variants: &[Variant], opts: &Options, mopts: &MatrixOptions) -> MatrixReport`. The pair kernel's settings and the presentation settings come from `Options` (the CLI/Action/config layering already fills it: `threshold_pct`, `min_abs`, `alpha`, `use_stats`, `threshold_overrides`, `keep_samples`, `title`, `tag`, `only_changed`, `max_rows`, `fail_on_regression_pct`); the matrix-specific ones come from `MatrixOptions` (`reference`, `correction`, `columns_cap`, `cell_format`, `sort: MatrixSort`). All of them are copied into `MatrixReport.summary: MatrixSummary` (glossary), exactly as `compare()` copies them into `Summary`.
  - *Given* `Options { title: "T", tag: "x", max_rows: Some(20), only_changed: true, fail_on_regression_pct: Some(10.0), .. }` and `MatrixOptions { cell_format: Ratio, columns_cap: 8, .. }`, *When* `compare_matrix` runs, *Then* `report.summary` carries `title "T"`, `tag "x"`, `max_rows Some(20)`, `only_changed true`, `fail_on_regression_pct Some(10.0)`, `cell_format Ratio`, `columns_cap 8`, and `render_matrix_markdown(&report)` (2.2.1a) and `render_svgs(SvgInput::Matrix(&report), &svg_opts)` (3.3.1b) take no second options argument: the chart cap `min(chart-rows, max-rows)` is computed from `report.summary.max_rows`.
  - `MatrixSort { Input, Name, Delta, Rank }`, `CellFormat { Full, Ratio }` and `Correction { Holm, None }` are defined here (they appear in `MatrixOptions`, so they must exist in this wave); 2.1.4a adds the Holm logic, 2.2.1a renders `CellFormat`, 2.2.2a parses all three from flags and keys.
**Files**: `crates/benchdiff-core/src/matrix.rs`

##### Task 2.1.3a: Types + `compare_matrix` (M)
- Files: `crates/benchdiff-core/src/matrix.rs`
- Types per the glossary (including `MatrixOptions`, `MatrixSummary`, `CellFormat`, `Correction`, `MatrixSort`, defined here): `Cell::{Missing, Present(PresentCell)}`, `PresentCell { value, samples, vs_reference: Comparison }`, `Comparison::{Reference, Compared{..}, Incomparable, DirectionMismatch, Invalid}`; no boolean flags. `MatrixReport/MatrixRow`, `Reference` resolution (ADR-005), `MatrixOptions` (separate from `Options`), BTree iteration only, sort modes `delta|name|rank`. This task builds `MatrixRow.ranking` (per-row ordering of reference and variants by value, oriented by `better`, ties stable by input order) and implements `sort=rank` over it; 2.1.5a covers only geomean. It defines the `PAdjusted` newtype (f64 clamped to [0,1]) and the `p_adjusted: Option<PAdjusted>` field on `Compared`, always `None` here; 2.1.4a fills it. Checks `better` equality first (mismatch short-circuits to `DirectionMismatch`), then calls `compare_pair(reference, variant)` on the unmodified variant metric; calls `Options::threshold_for` with the projected metric (key `group/residual`); resolves `Invalid` from `Variant.skipped` as above. Matrix `sort` is its own `MatrixSort { Input, Name, Delta, Rank }` enum (the pair `Sort` enum is unchanged); the CLI/config layer rejects `rank` on a two-way run (2.2.2).
- Test approach: unit tests per AC; determinism test (run twice, equal serialized output).

### Story 2.1.4: Holm correction
**As a** reviewer, **I want** multiplicity handled, **so that** verdicts across N-1 comparisons are trustworthy (ADR-002).
**Acceptance Criteria**:
- Textbook vector.
  - *Given* raw p `[0.01, 0.04, 0.03]`, *When* `holm_adjust` runs, *Then* result `[0.03, 0.06, 0.06]` (in input order).
- Downgrade-only post-pass.
  - *Given* a row where `heap` p=0.04 and `merge` p=0.03 decided by Mann-Whitney with `correction=holm`, *When* verdicts are finalized, *Then* both cells become Unchanged and their `Compared` carries `method == MannWhitney`, `p_adjusted = 0.06`; the rendered method text `mann-whitney, Holm-adjusted p=0.06 (m=2)` is derived at render time from `p_adjusted` and the row's `m` (the `Method` enum gains no variant).
  - *Given* any cell, *Then* Holm never turns Unchanged into Regressed/Improved and never re-runs the kernel; it only rewrites `Compared{ verdict, method, p_adjusted }` for cells with `method == MannWhitney` and `p_adjusted >= alpha` (alpha as the kernel's significance level).
- Family definition (m).
  - *Given* a row with variants whose Mann-Whitney p-values are `[0.01, 0.04, 0.03]` where the 0.03 cell was already made Unchanged by the threshold/min-abs floor, *When* Holm runs, *Then* m = 3 (every cell for which the test ran, i.e. has a p-value, counts, whether or not the floor then suppressed it) and the adjusted values equal the textbook vector `[0.03, 0.06, 0.06]`.
  - *Given* a cell decided by the interval rule (no p-value), *Then* it is not in m and not altered.
- Ordering of aggregates.
  - *Given* a row whose Regressed cell Holm downgrades to Unchanged, *When* `counts`, per-variant counts, geomean and `matrix_failed` are computed, *Then* they reflect the post-Holm verdicts (the pass runs before any aggregate is computed), so `matrix_failed` is false for a cell Holm cleared.
- Identity.
  - *Given* a single variant (m=1), *When* Holm runs, *Then* verdict and p are unchanged.
- Holm is per-row only (limitation, ADR-002): it controls the error rate within one benchmark's N-1 comparisons, not across the report's hundreds of rows.
  - *Given* a seeded generator producing 300 rows x 3 variants of identically distributed samples (fixed seed, coefficient of variation <= 2% so no row's median shifts by the default 5% threshold), *When* `compare_matrix` runs at the default threshold with `correction=holm`, *Then* `matrix_failed` is false and no cell is Regressed (test `matrix_should_not_fail_when_300_identical_distribution_rows`). The test pins the threshold floor, not Holm, as the across-rows guard; the README documents that without a threshold floor the across-rows false-positive rate is not controlled.
  - *Given* the README and the Method note, *Then* both state that Holm is per-row only and recommend pairing N-way `fail-on-regression` with a `threshold` and `min-abs` (5.1.1).
**Files**: `crates/benchdiff-core/src/stats.rs`, `crates/benchdiff-core/src/matrix.rs`

##### Task 2.1.4a: `holm_adjust` + integration (S)
- Files: `crates/benchdiff-core/src/stats.rs`, `crates/benchdiff-core/src/matrix.rs`
- Pure fn; monotone enforcement; `Correction::{Holm,None}` is consumed here (the enum is defined by 2.1.3a); fills `Compared.p_adjusted` (the `PAdjusted` type and field are defined by 2.1.3a); the post-pass described above, run inside `compare_matrix` before counts, geomean (2.1.5a) and `matrix_failed` (2.1.6a) are computed.
- Test approach: vectors above + property test (adjusted >= raw, <= 1, monotone).

### Story 2.1.5: Geomean and ranking
**As a** maintainer choosing an implementation, **I want** an overall figure, **so that** I can see a winner.
**Acceptance Criteria**:
- Orientation-aware geomean (per-variant view).
  - *Given* ratios vs reference `[0.5, 2.0]` for `better=lower` metrics, *When* geomean runs, *Then* result `1.00x` with `n=2/2`.
  - *Given* a `better=higher` metric with ratio 2.0, *When* oriented, *Then* it contributes 0.5.
- Exclusions counted.
  - *Given* a zero base value in 1 of 15 rows, *When* geomean runs, *Then* it reports `n=14/15` and no NaN/inf appears.
- Measures and units are never mixed.
  - *Given* rows with measures `ns/op` (10 rows) and `B/op` (5 rows), *When* geomean runs, *Then* two geomean families are produced (`ns/op n=10/10`, `B/op n=5/5`), each shown only if it has >= 3 rows; no value combines them. The headline sentence uses the primary family only: the first family whose unit is a time unit (ns, us, ms, s), else the largest family.
- Headline geomean over the common row set (round 3: a variant that skipped its slow benchmarks must not win by omission).
  - *Given* 15 rows where `merge` lacks its 6 slowest benchmarks and `heap` has all 15, *When* geomeans are computed, *Then* the per-variant view reports `merge n=9/15` and `heap n=15/15`, while the headline view uses only the 9 common rows for every variant and reports `common 9/15`.
  - *Given* a common set under 3 rows or under half the rows (here 9 of 15 is above half; 4 of 15 is below), *Then* the headline is `not comparable: merge covers 4/15 benchmarks` (naming the variant with the lowest coverage) and no "best geomean" sentence is produced.
- Ranking respects direction.
  - *Given* values 100 (quick), 130 (heap), 80 (merge) with `better=lower`, *When* ranked, *Then* `ranking == [merge, quick, heap]`; with `better=higher` it reverses.
**Files**: `crates/benchdiff-core/src/matrix.rs`, `crates/benchdiff-core/src/stats.rs`

##### Task 2.1.5a: Implement (S)
- Files: `crates/benchdiff-core/src/matrix.rs`, `crates/benchdiff-core/src/stats.rs`
- Geomean per `(measure, unit)` family, using the reference's `better` for orientation, in two views: per-variant over that variant's intersection with the reference (`n_used/n_total`), and `headline` over the common row set (reference and every compared variant Present and not Invalid) with `common_n/total` and a `comparable: bool` (false when the common set is under 3 rows or under half the rows); ties stable by input order; a family is shown only when >= 3 benchmarks (field `Option`).

### Story 2.1.6: Matrix JSON
**As a** consumer of canonical JSON, **I want** the matrix under a new key only when requested, **so that** existing consumers see no change.
**Acceptance Criteria**:
- Absent by default.
  - *Given* a two-way run, *When* JSON renders, *Then* bytes equal `expected/default.json`.
- Present when requested.
  - *Given* a matrix run, *When* JSON renders, *Then* the top level has `"matrix"` with `reference`, `columns`, `rows[].cells[]`, `geomean`, `counts`, and schema stays 1; `samples` never appear (`#[serde(skip)]`).
- Companion fields keep Action semantics (ADR-006 amended).
  - *Given* a matrix run (there is no two-way `Report`), *When* JSON renders, *Then* top-level `counts` carries every key the two-way `render_json` emits (`regressed`, `improved`, `unchanged`, `new`, `removed`, `incomparable`, `invalid`, `skipped`; `index.js` reads `regressed`, `improved`, `unchanged`, `new`, `removed`), with `has_base: true`, `failed` equal to `matrix_failed(&MatrixReport, gate)` (`gate: Option<f64>` is the `fail-on-regression` value; `None` gives `false`), and `rows` `[]`. In a matrix run these counts are cells versus the reference (`regressed` = significantly slower than the reference, ADR-005 item 7); `matrix.reference` is `{"label": ..., "explicit": bool}`. Mapping over all non-reference cell slots, so nothing is uncounted: `regressed`/`improved`/`unchanged` from `Compared` verdicts; `new` = Present cells in `reference_absent` rows; `removed` = `Missing` cells in rows where the reference is present; `incomparable` = `Incomparable` + `DirectionMismatch` cells; `invalid` = `Invalid` cells; `skipped` = importer-skipped total across variants. Invariant (tested): the sum of the seven verdict keys equals (variants - 1) x rows. All values are computed after the Holm pass. `matrix.counts` keeps the per-variant breakdown.
**Files**: `crates/benchdiff-core/src/lib.rs`, `crates/benchdiff-core/src/matrix.rs`, `crates/benchdiff-core/tests/json_compat.rs` (extends the file 1.2.5b created)

##### Task 2.1.6a: Add `render_json_with_matrix` (S)
- Files: `crates/benchdiff-core/src/lib.rs`, `crates/benchdiff-core/src/matrix.rs`, `crates/benchdiff-core/tests/json_compat.rs`
- Keep `render_json(&Report)` unchanged; new `render_json_with_matrix(&MatrixReport, gate: Option<f64>)` emits `schema`, `counts`, `failed`, `rows: []` (companion fields above) plus the `matrix` key via `serde_json::json!`; `matrix_failed(&MatrixReport, gate: Option<f64>)` is `false` for `None` and otherwise trips on a (post-Holm) Regressed cell over the threshold or any Invalid cell; it is defined as `!matrix_failures(report, gate).is_empty()`, where `matrix_failures(&MatrixReport, Option<f64>) -> Vec<FailedCell>` lists the failing cells (variant, row label, kind `Regressed{pct}` or `Invalid`) in table order for the gate line (2.2.1b). Adds the crate-root re-exports for the matrix items (`lib.rs` owner for this wave).
- Test approach: golden compare + key-presence test.

## Epic 2.2: N-way surfaces
**Goal**: Markdown matrix and CLI for N-way.

### Story 2.2.1: Markdown matrix
**As a** reviewer, **I want** a readable matrix, **so that** I can compare variants in a comment.
Split by owner (round 2 size review): **(a)** task 2.2.1a (W10) is the table: heading and marker, headline, reference line, cells, `cell-format`, column cap, row caps, legend, Method note; **(b)** task 2.2.1b (W11) is `render_notices`, the notice list in the Method note, the gate line and `render_notice_report`. Each AC is tagged with the task that makes it pass; both tasks edit only `markdown_matrix.rs`, serially.
**Acceptance Criteria**:
- (a) Heading, marker and presentation settings (round 2: how `title`, `tag`, `max-rows` and `only-changed` reach the renderer; all are read from `MatrixReport.summary`, 2.1.3a).
  - *Given* `summary.tag "x"` and `summary.title "T"`, *When* rendered, *Then* the first line is the same marker the two-way report writes for tag `x` (the `marker()` helper, `markdown.rs`) and the heading is `### T: 4 variants (reference: std)`.
  - *Given* `summary.max_rows = Some(20)` and 47 rows, *Then* the table shows the first 20 rows after `sort` followed by the single line `_…and 27 more rows not shown (`max-rows: 20`); the JSON report has them all._` (the shape of the two-way line, `markdown.rs`, which says "changed rows" there because the two-way cap counts only changed rows; the matrix table lists all rows, so the word is dropped). *Given* `max_rows` unset, no such line.
  - *Given* `summary.only_changed`, *Then* rows with no Regressed, Improved, Invalid or DirectionMismatch cell are left out and one line `_N rows with no significant difference hidden (`only-changed`)._` follows the table; without `only_changed` they are listed.
- (a) Cell format.
  - *Given* reference `quick`=1630 ns and `merge`=1210 ns Improved, *When* rendered, *Then* the `merge` cell reads `1210 ns (0.74x) 🟢` and the best value in the row is bold (an ordering marker, not a test; the legend says so).
- (a) Headline (single wording used by plan, ux.md wireframe and AC-S1-2; reworded in triad round 2: only variant-versus-reference is tested, so the sentence must not claim a head-to-head "fastest" or "loses").
  - *Given* headline geomeans quick 1.00x, heap 1.28x, merge 0.81x over the common set of 15/15 rows and `merge` Regressed (slower than the reference) in 2 rows, *When* rendered, *Then* the first line after the title reads `**merge** has the best geomean (0.81x vs reference quick, over 15/15 common ns/op rows; variants are not tested against each other); slower than reference in 2 of 15 rows`. "Best" means best in the reference's `better` direction. The clause `slower than reference in N of M rows` is defined as the number of common rows where the named variant's cell is `Regressed` (so `0 of 15` is a valid reading and replaces the earlier undefined phrase "loses in 0 rows"). Primary time-unit family only, named in the sentence; other measures get their own `geomean (B/op)` table row, never merged into the headline. *Given* the reference itself has the best geomean, *Then* the line reads `**quick** (the reference) has the best geomean; no variant is better overall (over 15/15 common ns/op rows)`. The next line states the reference choice: `reference: quick (first listed; set 'reference' to change)` when defaulted, `reference: quick (set by 'reference')` when explicit.
  - (a) *Given* `merge` covering 4 of 15 benchmarks, *When* rendered, *Then* the headline reads `**not comparable**: merge covers 4/15 benchmarks (no overall winner)` and each variant's `n_used/n_total` is shown in the geomean table row (`merge 0.9x (n=4/15)`).
  - (a) *Given* a Present cell missing in some variants, *When* rendered, *Then* the summary line carries `· 3 incomplete` and the Method note names the impl and count (`merge is missing 3 benchmarks`).
- Invalid and skipped (glyphs fixed in round 3: Missing is `—` U+2014 everywhere, Invalid is `⚠️ invalid`).
  - (a) *Given* a `merge` cell that is `Comparison::Invalid` and another that is `Missing`, *When* rendered, *Then* the Invalid cell shows `⚠️ invalid` (never `—`), the Missing cell shows `—` (never `-`, never `0`), and a snapshot asserts both glyphs; (b) the `⚠️ N invalid cells (merge: 1)` line follows the headline as a notice of class `Invalid`.
- One explicit line per degradation, each naming a count and a next action (AC-S1-6).
  - *Given* four fixtures, *Then* the report contains exactly one line each: (a) `· 3 incomplete` plus a Method-note sentence naming the impl; (b) `⚠️ reference "X" absent from 2 benchmarks: A, B ... (first 10, and N more); set 'reference' to another variant`; and the two chart lines, produced by 3.3.1c and asserted there: `_...and 17 more not shown (chart-rows: 30)_` and `distribution chart skipped: no samples in the input (use -count=10 or Criterion)`. Reference-absent rows show `—` ratios and are excluded from the ratio chart and the geomean.
- (a) Prose wording (AC-S1-8): *Given* any rendered matrix, *Then* a regex scan finds no `higher` or `lower` in report prose (cell ratios `0.74x`; words `faster`/`slower` only at >= 2x). The scan covers the headline, legend, Method note and every notice, and (via 3.3.1c) the chart alt text.
- (a) Column cap.
  - *Given* 11 variants, *When* rendered with default cap 8, *Then* 8 columns plus `_...and 3 more variants not shown (see JSON)_`.
- (a) Method note names Holm policy and reference, and lists the `MethodFacts` sentences from `MatrixSummary` (1.2.1b fields: per-metric thresholds, non-default sources) when non-empty.
- (b) Regression wording and the gate line (ADR-005 item 7).
  - *Given* any rendered matrix, *Then* a regex scan for `regress` (case-insensitive) finds nothing except the flag name inside the gate line; cells and prose say "slower/faster than reference" (`🔴`/`🟢`).
  - *Given* `fail-on-regression 10` with explicit reference `quick` and one post-Holm cell `heap` on `Sort/size=100` at +30%, *When* rendered, *Then* the line after the reference line reads `gate: fail-on-regression 10% vs quick: FAILED (1 cell: heap on Sort/size=100)`; with no such cell it reads `gate: fail-on-regression 10% vs quick: passed`; with no `fail-on-regression` there is no gate line.
  - Truncation rule when many cells fail (round 2). *Given* 7 failing cells (6 `Regressed` over 10% and 1 `Invalid`), *Then* the line reads `gate: fail-on-regression 10% vs quick: FAILED (7 cells: heap on Sort/size=100, heap on Sort/size=10, merge on Parse and 4 more; full list in Method)`: the first 3 failing cells in table order are named, each `variant on row` middle-truncated to 40 characters, the count is always printed, the whole line is at most 200 characters (names are trimmed before the count is ever dropped), and the Method note lists every failing cell (first 50, then `and N more; the JSON report has them all`). An Invalid cell reads `merge on Sort invalid`.
- (a) Legend directly under the table (AC-S1-10, moved from the Method note; reworded in triad round 2 so that bold, Rank and the headline's "best" are explicitly untested orderings).
  - *Given* any rendered matrix, *Then* the line immediately after the table is `_🟢 faster than reference · 🔴 slower than reference · ~ no significant difference · — not run · **bold** and Rank order the values in a row; only each variant against the reference is tested, not variants against each other_` (the `~` clause is included only when the table has a `~` cell; `—` only when it has a missing cell; the last clause always; a snapshot asserts the full line).
- (a) Compact ratio-only cells for phones (`cell-format`).
  - *Given* `cell-format=ratio`, *When* rendered, *Then* the `merge` cell reads `0.74x 🟢`, the reference column still shows its absolute value (`1630 ns`), the bold-best, `~`, `—` and `⚠️ invalid` forms are unchanged, and the Method note adds `cells show ratio vs reference; absolute values are in the JSON`; with `full` (the default) the cell is the `1210 ns (0.74x) 🟢` form.
- (b) Notices: fixed order, capped, never above the verdict (AC-S1-11; keeps AC-S1-2 true). The `Notice`/`NoticeClass` types come from 1.1.1c (W2).
  - Order after the verdict, reference and gate lines: (1) degradation notices (dropped rule/variant, collision, absent axis, lost reference), (2) skipped metrics (the existing line), (3) reference-absent, (4) invalid cells, (5) chart notices (skipped, capped, not hosted, not embedded). At most 5 notice lines render inline; any further notice is replaced by one line `⚠️ N more notices (see Method)`, and the Method note's `<details>` always lists every notice in the same order. Each notice is one line naming a count and the next action; example lists inside a notice keep their own first-10 cap. The cap applies to matrix reports and to chart notices of two-way reports; the two-way report's existing skipped-metrics line is neither capped nor reordered (default bytes unchanged).
  - *Given* a matrix with 7 notices of mixed classes, *When* rendered, *Then* lines 1-5 inline are the first five in class order, the sixth line is `⚠️ 2 more notices (see Method)`, and the verdict headline is still the first content line after the title. *Given* a notice-only report (nothing to compare), *Then* it has no verdict line and its first content line is the first notice (AC-S1-2 is scoped to reports that have a verdict).
**Files**: `crates/benchdiff-core/src/markdown_matrix.rs`

##### Task 2.2.1a: `render_matrix_markdown`: table, headline, legend, cell formats (M)
- Files: `crates/benchdiff-core/src/markdown_matrix.rs`
- `pub fn render_matrix_markdown(report: &MatrixReport) -> String`; **this signature is frozen**: 2.2.2b (W12) calls it, and 3.3.1c (W12) adds the separate `render_matrix_markdown_with(report, &RenderExtras)` and turns this function into a thin wrapper over it, so neither task edits the other's call site. Uses `pub(crate)` helpers (2.1.1a); marker/heading from `summary.tag`/`summary.title`; the (a)-tagged ACs above (headline, reference line, rows, `cell-format`, column cap, row caps, `only-changed`, legend, Method note with `MethodFacts`); `x faster/slower` phrasing only when >= 2x; `—` (U+2014) for Missing. Emits no notice lines, no gate line and no placeholder: those are 2.2.1b and 3.3.1c.
- Test approach: snapshot tests + hostile-name escape test (reuse `untrusted_fields...` pattern).

##### Task 2.2.1b: Notices, gate line and notice-only report (S)
- Files: `crates/benchdiff-core/src/markdown_matrix.rs`, `testdata/nway/expected/notice-only.md` (the golden the Rust snapshot test and the JS mirror in 4.2.3b both assert)
- Runs in W11 after 2.2.1a (W10, same file) and after 2.1.6a (W10, `matrix_failed`). Adds `pub(crate) render_notices(&[Notice]) -> String` (fixed class order, 5-line cap, overflow line; also used by `render_markdown_with` in 3.3.1c) and the Method-note list of every notice, the invalid-cells notice, the reference-absent notice, the gate line with its truncation rule (the failing cells come from `matrix_failures`, added by 2.1.6a), and `render_notice_report(&Options, &[Notice]) -> String` (sticky marker, title, one line per notice; used when a data-dependent problem leaves nothing to compare, 2.2.2b). These are inserted into `render_matrix_markdown` after the reference line (a small, same-file edit) in the order verdict, reference, gate, notices.
- Test approach: snapshot tests for the 7-notice case, the 7-failing-cell gate case and the notice-only report.

### Story 2.2.2: CLI flags for N-way
**As a** CLI user, **I want** flags for variants and axes, **so that** I can run N-way locally.
Split by owner (round 2 size review): **(a)** task 2.2.2a (W9) parses and validates: flags, config keys, overlays, the `label=path` grammar, every static exit-2 error, `--help`; it neither loads a file nor renders; **(b)** task 2.2.2b (W12) wires `compare_matrix` and the Markdown/JSON renderers into `main.rs`, owns the degradation ladder (notices, two-way fallback, notice-only report), the regression gate's exit code and `--print-dimensions`. Each AC below is tagged with the task that makes it pass. Both edit `main.rs`, `config_cli.rs` and `matrix_cli.rs` serially, so no signature race exists: 2.2.2b calls the frozen `render_matrix_markdown` (2.2.1a) and 3.3.1b (W13) later swaps in the `_with` forms.
**Acceptance Criteria**:
- `--variant label=path` repeatable (singular flag, one variant per occurrence; the Action's plural `variants` input is a newline list that expands to these, glossary `VariantsInput`); `--compare-by`, `--group-by`, `--dimensions` (repeatable, one rule per occurrence), `--dimensions-strict` (fail instead of notice on data-dependent problems), `--reference`, `--correction`, `--cell-format full|ratio` accepted; all except `--variant` mirrored as config keys (`variant` is not a config key: paths in a config file would be relative to nothing). `--print-dimensions` is a CLI-only diagnostic like `--print-config` (no key, no input).
  - (b) *Given* `--variant quick=a.txt --variant heap=b.txt --variant merge=c.txt --reference quick`, *When* run, *Then* stdout is the matrix report with `quick` first/reference.
  - (a) *Given* `--reference nope`, *When* run, *Then* exit 2 `unknown reference 'nope'; valid: quick, heap, merge` (explicit `--variant` labels are static; this is the one wording everywhere, ux.md aligned).
  - (b) *Given* `--compare-by impl` on a file where no name carries `impl=` but some carry `size=` (no `--dimensions-strict`), *When* run with `--base`/`--head`, *Then* the two-way report is printed with the notice `compare-by 'impl': no benchmark carries this dimension; available dimensions: size (2 values), group; showing the two-way report` and exit 0; *Given* the same with only `--variant` files whose rules match nothing, *Then* the notice-only report (`render_notice_report`) is printed, exit 0; *Given* `--dimensions-strict`, *Then* exit 2 with `compare-by 'impl': no benchmark carries this dimension; available dimensions: size (2 values), group`.
  - (b) *Given* `--dimensions 'impl=/impl=(?P<impl>\w+)'` that matches 0 of 48 names without `--dimensions-strict`, *Then* exit 0 and the report carries the zero-match notice (1.1.2); with `--dimensions-strict` exit 2.
  - (b) The regression gate (ADR-005 item 7). *Given* `--fail-on-regression 10 --reference quick` and a post-Holm cell Regressed `+30%` vs `quick`, *When* run, *Then* exit 1 and the report's gate line says FAILED; *Given* an Invalid cell, *Then* exit 1; *Given* only cells Regressed at +7%, *Then* exit 0 and the gate line says passed; *Given* no `--fail-on-regression`, *Then* exit 0 whatever the cells are (no gate, no gate line).
  - (a) *Given* `--fail-on-regression 10` in an N-way run with the reference defaulted (no `--reference`/`reference` key), *When* run, *Then* static exit 2 `fail-on-regression in an N-way run needs an explicit 'reference' (the baseline to protect); set 'reference'`; the same text in the Action (the step fails). (b) *Given* an explicit reference that is later lost from the data (`compare-by` mode, absent label), *Then* the run degrades to the notice-only report, the gate is skipped and the job passes unless `--dimensions-strict` (Risk Control).
  - (b) Dimension discovery. *Given* `--print-dimensions --head a.txt` (or `--variant` files, same inputs as a run), *When* run, *Then* stdout is one JSON object `{"dimensions":[{"name":"impl","values":["quick","heap"],"benchmarks":18}],"pseudo":["group"],"rules":"default","unmatched":2}` (`rules` is `default` or `custom (N)`; `values` capped at 20 with `"values-truncated":true`), exit 0, no report and no other writes; *Given* names with no `=` and no rules, *Then* `dimensions` is `[]` and a `hint` string repeats the zero-config rules.
  - Compact cells. (b) *Given* `--cell-format ratio`, *Then* the matrix renders the `0.74x 🟢` cell form (2.2.1); (a) *Given* `--cell-format wide`, *Then* exit 2 `cell-format: "wide" is not valid; use full, ratio`.
  - (a) *Given* no new flags, *When* run on golden fixtures, *Then* output equals expected files.
  - (a) *Given* `--sort rank` on a two-way run, *Then* exit 2 `--sort rank needs an N-way run (--variant or --compare-by)`; on an N-way run `rank` is accepted (matrix sort enum `MatrixSort`, 2.1.3a); the config key `sort: rank` follows the same rule.
  - (a) *Given* `--head-json` together with `--variant` (or a matrix run via `--compare-by`), *Then* exit 2 `--head-json is two-way only`.
  - (a) *Given* `--help`, *Then* it lists every flag added by this task (`--variant`, `--compare-by`, `--group-by`, `--dimensions`, `--dimensions-strict`, `--reference`, `--correction`, `--cell-format`, `--config`, `--print-config`, `--print-dimensions`) and, under `--compare-by`/`--dimensions`, the zero-config default extraction rules with its default and allowed values, generated from the one key table in `config.rs` (so the text cannot drift); a test asserts each flag name and the word `default` appear.
- (a) Config keys added by this task (extends the 1.2.2a key list; ACs here so they run at the wave that owns the types).
  - *Given* `{"version":1,"threshold":3,"sort":"name","reference":"quick","dimensions":["impl=/impl=(?P<impl>\\w+)"]}`, *When* parsed, *Then* the overlay has threshold 3.0, sort Name, reference label `quick` and one rule.
  - *Given* `{"version":1,"cell-format":"ratio"}`, *Then* the overlay has `CellFormat::Ratio`.
  - *Given* `{"version":1,"correction":"bonf"}`, *Then* `correction: "bonf" is not valid; use holm, none`; *Given* `{"version":1,"compare-bi":"impl"}`, *Then* `unknown key 'compare-bi'; did you mean 'compare-by'?`; `dimensions-strict` (not `strict`) is the key name everywhere. Config parse errors for these keys carry file, line and column (1.2.2).
- (a) `label=path` grammar (unambiguous; shared with ADR-007).
  - *Given* `--variant quick=results/size=100/a.txt`, *When* parsed, *Then* the split is at the FIRST `=`: label `quick`, path `results/size=100/a.txt`.
  - *Given* `--variant results/size=100/a.txt` (no label), *When* parsed, *Then* exit 2 `--variant: label 'results/size' is invalid (use [A-Za-z0-9._-], max 40) in 'results/size=100/a.txt'; expected label=path`.
  - *Given* `--variant quick=a.txt --variant quick=b.txt`, *Then* exit 2 `label "quick" given twice (a.txt, b.txt)`; `--variant quick=` -> `path is empty`.
  - *Given* `--variant` together with `--base`/`--head`, *Then* exit 2 `--variant cannot be combined with --base/--head`.
**Files**: `crates/benchdiff/src/main.rs`, `crates/benchdiff/tests/matrix_cli.rs`, `crates/benchdiff/tests/config_cli.rs`, `crates/benchdiff-core/src/config.rs` (2.2.2a), `crates/benchdiff-core/src/options.rs` (2.2.2a)

##### Task 2.2.2a: Matrix flags, config keys, overlays and static validation (M)
- Files: `crates/benchdiff/src/main.rs`, `crates/benchdiff-core/src/config.rs`, `crates/benchdiff-core/src/options.rs`, `crates/benchdiff/tests/matrix_cli.rs`, `crates/benchdiff/tests/config_cli.rs`
- Runs in W9 (needs `CellFormat`, `Correction`, `MatrixSort`, `MatrixOptions` from 2.1.3a, W7). Adds `MatrixOptionsOverlay` and the matrix config keys (`compare-by`, `group-by`, `dimensions`, `dimensions-strict`, `reference`, `correction`, `cell-format`) to the 1.2.2a key table, `rank` in the sort enum, the flags `--variant`, `--compare-by`, `--group-by`, `--dimensions`, `--dimensions-strict`, `--reference`, `--correction`, `--cell-format`; the `label=path` grammar and every static exit-2 error (including the explicit-reference check for the regression gate, `--sort rank` on a two-way run and `--head-json` with `--variant`); `--help` text generated from the key table. Parses into `MatrixOptions` and stops: it does not load variant files, call `compare_matrix` or render (that is 2.2.2b), so it needs nothing from W10-W12.

##### Task 2.2.2b: Matrix wiring, degradation ladder and `--print-dimensions` (M)
- Files: `crates/benchdiff/src/main.rs`, `crates/benchdiff/tests/matrix_cli.rs`, `crates/benchdiff/tests/config_cli.rs`
- Runs in W12 after 2.2.1b (`render_notice_report`) and 2.1.6a (`render_json_with_matrix`, `matrix_failed`). Loads variant files (`dimensions::apply` per file), builds variants (`project_by_axis`), calls `compare_matrix` with `Options` + `MatrixOptions`, renders with the frozen `render_matrix_markdown(&MatrixReport)` and `render_json_with_matrix`, sets the exit code from `matrix_failed`; implements the warn-by-default degradation ladder from Risk Control (drop rule/variant, two-way fallback, notice-only report via `render_notice_report`) with `dimensions-strict` as the fail switch; `--print-dimensions` via `Dimensioned::dimension_summary`. `RenderExtras` is not used here: 3.3.1b (W13) switches the call sites to the `_with` forms, so this task and 3.3.1c (same wave, different files) cannot race on a signature.
- Size watch: this is the likeliest task to overflow M into L; if it exceeds 5 files, split it further (ladder vs. `--print-dimensions`) in the same wave order without changing the task count elsewhere without updating every count.
- Test approach: integration tests spawning the binary, one per (b)-tagged AC.

### Story 2.2.3: N-way fixtures
**As a** maintainer, **I want** a >= 3 implementation fixture with expected output, **so that** the success metric is verified.
**Acceptance Criteria**:
- *Given* `testdata/nway/{quick,heap,merge}.txt` (Go bench output, 10 samples each, 6 benchmarks; committed earlier by 0.3.1a) plus a single-file variant `testdata/nway/impl.txt` using `impl=` sub-benchmarks, *When* both forms render, *Then* matrices are identical and equal `testdata/nway/expected/matrix.md` and `matrix.json`.
- The properties 0.3.1a built into the inputs hold in the matrix: *Given* the rendered matrix, *Then* at least 3 rows have a variant significantly better than `quick`, at least 1 cell is slower than the reference, and at least 1 row has a `~` cell, so Q1-Q3 (MAN-7) have defined answers on both the baseline and the treatment.
**Files**: `testdata/nway/impl.txt`, `testdata/nway/expected/{matrix.md,matrix.json}`, `test/golden.test.js`, `testdata/README.md`, `crates/benchdiff-core/tests/nway_fixture.rs`, `dist/benchdiff.wasm`

##### Task 2.2.3a: Add `impl.txt` and the expected matrix goldens; wave-14 wasm integration (S)
- Files: `testdata/nway/impl.txt`, `testdata/nway/expected/matrix.md`, `testdata/nway/expected/matrix.json`, `test/golden.test.js`, `testdata/README.md`, `crates/benchdiff-core/tests/nway_fixture.rs`, `dist/benchdiff.wasm`
- `expected/matrix.md` and `matrix.json` are generated with the Action's defaults, not the CLI's: title `Benchmark comparison`, tag `default` (the values `action.yml` supplies), so 4.2.3c's comment-body comparison differs only by nothing. The generator command is recorded in `testdata/README.md`.
- Integration task: runs alone in wave 14 (after 3.3.1b and 3.3.3a of W13, so no `then` ordering is needed), rebuilds and commits `dist/benchdiff.wasm` from the merged tree (needed for wasm-vs-native matrix cases; later tasks use it for 4.2.3c), and is the only committer of the wasm in that wave.

---

# PHASE 3: SVG rendering

## Epic 3.1: Foundation
**Goal**: A closed, deterministic, escaping SVG toolkit with no dependencies.

### Story 3.1.1: `SvgWriter`
**As a** maintainer, **I want** one place for escaping and number formatting, **so that** hostile names cannot inject markup and bytes are stable.
**Acceptance Criteria**:
- Escaping.
  - *Given* text `"><script>alert(1)</script> & ]]>`, *When* written as a text node, *Then* output contains `&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt; &amp; ]]&gt;` and no `<script`.
- Control chars.
  - *Given* text containing U+0000 and U+0008, *When* written, *Then* they are replaced with `?`-class placeholder (U+FFFD) and the document is well-formed XML 1.0.
- Numbers.
  - *Given* `-0.0`, `1.0`, `0.30000000000000004`, `NaN`, *When* formatted, *Then* `0`, `1`, `0.3`, and `NaN` returns an error (never emitted).
**Files**: `crates/benchdiff-core/src/svg/mod.rs`, `crates/benchdiff-core/src/svg/writer.rs`

##### Task 3.1.1a: Writer, `xml_escape`, `fmt_coord` (S)
- Files: `crates/benchdiff-core/src/svg/mod.rs`, `crates/benchdiff-core/src/svg/writer.rs`
- API: `rect/line/text/title/desc/group`; user text only through `text`/`title`/`desc`; attributes are numbers or closed-enum class names; ids from counters; `\n` newlines.
- Test approach: unit tests + well-formedness check using a dev-dependency-free tiny XML tag-balance checker in tests (or `quick-xml` as `dev-dependency` only).

### Story 3.1.2: Text metrics
**As a** reader, **I want** labels that never clip, **so that** names remain legible without font access.
**Acceptance Criteria**:
- *Given* the name `BenchmarkSortLargeArrays/impl=quick` at 12 px and a 212 px column, *When* fitted, *Then* it is middle-truncated keeping the suffix (`BenchmarkSortLa…/impl=quick`), the estimate includes a +10% margin, and a `<title>` carries the full name.
- *Given* CJK text, *When* measured, *Then* each char counts 1.0 em.
**Files**: `crates/benchdiff-core/src/svg/text.rs`

##### Task 3.1.2a: Width table + truncate (S)
- Files: `crates/benchdiff-core/src/svg/text.rs`
- ~95-entry advance table from a cited public Arial/Helvetica AFM (cite in a comment); non-ASCII = 1.0 em; fixed-width tests.

### Story 3.1.3: Style and theme
**As a** reader on light or dark pages, **I want** a legible chart, **so that** it works anywhere.
**Acceptance Criteria**:
- Opaque card.
  - *Given* theme `Light`, *When* rendered, *Then* the root has a background `rect` `#fcfcfb`, ink `#0b0b0b`, series colours `#2a78d6 #eb6834 #1baf7a #eda100 #e87ba4 #6250d6` (+2 more) assigned by variant order.
- `Both` emits two files (embedding is Story 3.3.3).
  - *Given* theme `Both`, *When* rendered, *Then* `NAME-light.svg` and `NAME-dark.svg` exist and the dark file uses the dark palette from research/ux.md.
- Accessibility.
  - *Given* any chart, *When* rendered, *Then* root has `role="img"` and `aria-labelledby` referencing `<title>` and `<desc>` ids.
- Contrast is computed, not asserted by hand (AC-S2-3).
  - *Given* the palette constants, *When* the unit test computes the WCAG relative-luminance contrast ratio, *Then* ink `#0b0b0b` on `#fcfcfb` is >= 4.5 (computed in this review as 19.17:1), secondary `#52514e` on `#fcfcfb` is >= 4.5 (7.73:1), and the dark palette's ink and secondary on the dark card are >= 4.5; every `<text>` fill in every golden SVG is ink or secondary (never a series colour). The light series colours `#1baf7a #eda100 #e87ba4` are below 3:1 on the card (research/ux.md), which is why they are never text, every series carries a shape and a printed value, and (next bullet) their bars and markers get an outline.
- Non-text contrast, WCAG 1.4.11 (AC-S2-11, triad round 2). Computed against the card `#fcfcfb` by script in this round: regressed `#d03b3b` 4.68:1 and improved `#0ca30c` 3.27:1 both pass 3:1, so the delta palette needs no change; categorical `#2a78d6` 4.30, `#eb6834` 3.12 and `#6250d6` 5.62 pass; `#1baf7a` 2.74, `#eda100` 2.11 and `#e87ba4` 2.62 fail. The unchanged-bar gray is fixed at `#8a8a86` (3.38:1).
  - *Given* any bar, marker, gridline or zero line needed to read a chart, *When* the unit test computes the contrast of its fill and of its stroke against the card, *Then* at least one of the two is >= 3:1 (the dark palette is computed by the test the same way). Series colours that fail as fills (series 3, 4 and 5 on the light card) therefore carry a 1.5 px stroke in secondary ink `#52514e` (7.73:1) on every bar and marker, so the object boundary passes. If a later palette change drops the improved green under 3:1 the test fails and the green is darkened (for example `#0a7d0a`, 5.18:1) instead of the rule being waived.
- Forced colours and print (AC-S2-12, a documented limit with one automated check). An SVG shown through `<img>` is not recoloured by forced-colors mode, and the card is an SVG `rect` (not a CSS background), so the chart keeps its own opaque card, shapes and printed values when the page is forced to a high-contrast palette or printed without background graphics. *Given* the Light card, *Then* the SVG contains no `<style>`, no `@media` and no `currentColor`/system-colour keyword (every colour is a literal hex), asserted by a unit test; README says forced-colors and print show the chart as drawn and that the text table is the accessible path; the print case is spot-checked in MAN-4.
**Files**: `crates/benchdiff-core/src/svg/style.rs`
- Dependency: default `chart-theme` is `light` until 0.2.1b answers the `prefers-color-scheme` question; if answered favourably, add `auto` as a single-file CSS variable mode.

##### Task 3.1.3a: Palette, shapes (circle, square, diamond, triangle, cross, star), card, text and non-text contrast tests (S)
- Files: `crates/benchdiff-core/src/svg/style.rs`
- Adds the non-text contrast test and the secondary-ink outline for series that fail 3:1 as fills (AC-S2-11), the unchanged gray `#8a8a86`, and the no-`<style>`/no-`@media` check for the Light card (AC-S2-12).
- Six shapes serve up to 8 series: shapes repeat for series 7 and 8 while colour still differs (documented in the chart footer legend and README).

## Epic 3.2: Charts
**Goal**: Delta, ranking and distribution charts, bounded and honest.

### Story 3.2.1: Delta bar chart (two-way)
**As a** PR author, **I want** a diverging bar chart, **so that** I see magnitude and direction at a glance.
**Acceptance Criteria**:
- Layout.
  - *Given* a `Report` with rows `A +30.0%` Regressed, `B -12.5%` Improved, `C +0.4%` Unchanged, *When* rendered at width 760, *Then* viewBox `0 0 760 H`, row height 28, bars centred on a 0% line labelled, regressed right with a filled triangle cap, improved left with a down cap, unchanged grey with a hollow circle, value labels at bar ends.
- Clamp.
  - *Given* a delta of +450%, *When* rendered, *Then* the bar clips at +100% with a visible clip arrow and the label still reads `+450.0%`.
- Sorted worst-first with stable ties.
**Files**: `crates/benchdiff-core/src/svg/delta.rs`

##### Task 3.2.1a: Implement (M)
- Files: `crates/benchdiff-core/src/svg/delta.rs`, `crates/benchdiff-core/tests/snapshots/svg/delta*`
- Snapshot test files `crates/benchdiff-core/tests/snapshots/svg/delta*` (the `ranking*` and `distribution*` files belong to 3.2.2a and 3.2.3a, `svg_snapshots.rs` to 3.2.5a). The chart's own cap, `1.`-style labels and `largest change first` footer arrive with 3.2.4a (W9); this task draws all rows it is given.

### Story 3.2.2: Ranking chart (N-way)
**As a** reviewer, **I want** grouped bars of ratio vs reference, **so that** I can see all variants per benchmark.
**Acceptance Criteria**:
- *Given* a `MatrixReport` with 3 variants and 6 rows, *When* rendered, *Then* one cluster per benchmark, bars in variant order with fixed colour+shape, a 1.0x reference gridline labelled `quick = 1.00x`, log2 scale so 2x slower and 2x faster are symmetric, direct value labels on every bar, Missing cells drawn as an empty slot with `—`.
- Max 8 series; 9th+ listed in footer `+ N more not shown`.
**Files**: `crates/benchdiff-core/src/svg/ranking.rs`

##### Task 3.2.2a: Implement (M)
- Files: `crates/benchdiff-core/src/svg/ranking.rs`, `crates/benchdiff-core/tests/snapshots/svg/ranking*`
- Takes `&MatrixReport` directly (the `SvgInput` wrapper arrives with 3.2.4a). Series 3, 4 and 5 on the light card get the secondary-ink outline of 3.1.3 (AC-S2-11).

### Story 3.2.3: Distribution strip
**As a** reviewer, **I want** to see sample overlap, **so that** significance is visible.
**Data source (ADV-3)**: samples reach the renderer through the `#[serde(skip)]` `Samples` side channel on `Row` (1.2.5b) and `PresentCell` (2.1.3a), filled only when `Options.keep_samples` is set (chart requests set it). `SvgInput` stays `Pair(&Report) | Matrix(&MatrixReport)`; `render_svgs` needs no extra argument and default JSON/memory are unchanged.
**Acceptance Criteria**:
- Two-way: *Given* a `Report` built with `keep_samples` and base and head each with 10 samples for `Sort`, *When* rendered, *Then* two lanes show every point with deterministic index-based jitter and a median tick.
- N-way: *Given* a `MatrixReport` built with `keep_samples`, *When* rendered, *Then* one lane per variant per benchmark for the top `chart-rows` benchmarks by max |delta|, one file `distribution.svg`.
- *Given* a metric with 0 samples (or a report built without `keep_samples`), *When* requested, *Then* the chart is skipped with `SkipReason::NoSamples` and delta charts are unaffected.
- *Given* default options, *Then* `render_json` bytes are unchanged.
**Files**: `crates/benchdiff-core/src/svg/distribution.rs`

##### Task 3.2.3a: Implement (S)
- Files: `crates/benchdiff-core/src/svg/distribution.rs`, `crates/benchdiff-core/tests/snapshots/svg/distribution*`
- Takes `&Report` / `&MatrixReport` directly; its top-N by max |delta| is provisional and is replaced by `select_rows` in 3.2.4a.

### Story 3.2.4: Caps, empty and skip states
**As a** maintainer, **I want** bounded output and loud degradation.
**Acceptance Criteria**:
- Row cap.
  - *Given* 47 changed rows and `chart-rows 30`, *When* rendered, *Then* 30 rows (by |delta| desc, input order tiebreak) plus footer text `+ 17 more not shown` inside the SVG and `SvgFile.skipped == CapExceeded{shown:30,total:47}` surfaced in Markdown.
- `chart-rows` and `max-rows` have one defined relationship, and both "N more" counts agree (triad round 1).
  - Rule: the chart's *population* is the table's population (two-way: the changed rows, or all compared rows when none changed, drawn grey; N-way: all matrix rows). The effective cap is `min(chart-rows, max-rows)` when `max-rows` is set, else `chart-rows`; `max-rows` is read from `report.summary.max_rows` (`Summary` for the pair report, `MatrixSummary` for the matrix, 2.1.3a), so no extra argument is needed. When `max-rows` is set the chart draws only rows the table displays (the table's rows after its `sort`, then ordered by |delta| for drawing). `N more` is always `population - shown`, in the SVG footer, in the Markdown notice and in the text block, and each notice names the limiting key: `_...and 17 more not shown (chart-rows: 30)_` or `(max-rows: 20)`.
  - Existing wording, verified in `markdown.rs` (the two-way cap line): `_…and 27 more changed rows not shown (`max-rows: 20`); the JSON report has them all._`. It stays byte-for-byte as is (default bytes); the chart's own footer and notice use the chart wording above, so the two lines differ in form but agree in the number and the key.
  - *Given* 47 changed rows, `max-rows 20` and `chart-rows 30`, *Then* the table says `_…and 27 more changed rows not shown (`max-rows: 20`); the JSON report has them all._`, the chart shows the same 20 rows and its footer and notice say `+ 27 more not shown` / `(max-rows: 20)` (equal counts, equal rows). *Given* the same rows with `max-rows` unset and `chart-rows 30`, *Then* the chart shows the top 30 by |delta| with `+ 17 more`. *Given* `max-rows 40` and `chart-rows 30`, *Then* the chart shows 30 and says `+ 17 more not shown (chart-rows: 30)` while the table says `…and 7 more changed rows not shown (`max-rows: 40`)…`; the two counts differ by exactly the two caps, and each line names its own key.
- Chart order versus table order, and a correlation aid (triad round 2). The chart draws its rows top-N by |delta| (stable ties by input order); the table follows `--sort` (default `delta`, worst first), so the two orders can differ.
  - *Given* 47 rows and `sort=name`, *Then* the chart's first label is the row with the largest |delta| while the table starts alphabetically; every chart row label starts with its chart position (`1.`, `2.`, ...), the `Chart data (text)` table repeats that number in a `#` column with the identical label text (same escaping and truncation), and the chart footer says `largest change first`. The main table is unchanged (default bytes); a reader matches a bar to the table by benchmark name and to the text block by `#`. A test asserts that the `#` column equals the chart label numbers and that both orders are as stated.
- Size.
  - *Given* 10 variants x 30 rows, *When* rendered, *Then* the SVG is < 100 KB.
- The chart-dispatch ACs that used to sit here (nothing to compare; chart kind versus report kind, `NotApplicable`) moved to Story 3.3.1, tagged (b): the dispatcher `render_svgs` is created by 3.3.1b (W13), so 3.2.4a (W9) could not make them pass. 3.2.4a owns per-chart row selection, caps, footers and `CapExceeded`.
**Files**: `crates/benchdiff-core/src/svg/mod.rs`, `crates/benchdiff-core/src/svg/delta.rs`, `crates/benchdiff-core/src/svg/ranking.rs`, `crates/benchdiff-core/src/svg/distribution.rs`

##### Task 3.2.4a: Row selection, caps and `CapExceeded` plumbing (S)
- Files: `crates/benchdiff-core/src/svg/mod.rs`, `crates/benchdiff-core/src/svg/delta.rs`, `crates/benchdiff-core/src/svg/ranking.rs`, `crates/benchdiff-core/src/svg/distribution.rs`
- Defines `SvgInput` (`Pair(&Report)` | `Matrix(&MatrixReport)`, needs 2.1.3a) and `select_rows(&SvgInput, &SvgOptions) -> Selected { rows, population, limiting_key }`, the one function the charts and `chart_text_table` (3.3.2a) use, so the text block and the chart cannot diverge; applies `min(chart-rows, max-rows)`, the |delta| ordering and the `1.`-style labels; edits `delta.rs`, `ranking.rs` and `distribution.rs` (their W7/W8 owners are done; `distribution.rs` swaps its own provisional top-N for `select_rows`, which selects the same rows by max |delta|) to call it, draw the `+ N more not shown` footer and `largest change first`, and return `SvgFile.skipped = Some(CapExceeded{shown,total})`. It defines no dispatcher (that is 3.3.1b).

### Story 3.2.5: Security and determinism corpus
**As a** maintainer, **I want** proof that hostile names stay inert, bytes are stable, and layouts are actually legible, **so that** a committed golden cannot lock in a clipped or overlapping chart (byte snapshots only prove sameness, not correctness).
**Acceptance Criteria**:
- *Given* a corpus of names (`"><script>`, `]]>`, `&#x0;`, emoji, 500-char, RTL), *When* every chart renders, *Then* every output is well-formed XML, contains none of `<script`, `<foreignObject`, `href`, `onload=`, `url(`.
- Geometry (round 3): *Given* the worst-case set `testdata/svg-worst/` (a 500-char name, CJK names, RTL names, 8 variants x 30 rows, 10 variants x 30 rows), *When* every chart renders, *Then* for every `<text>` the estimated width (advance table plus the 10% margin) plus its x offset lies inside its container (label gutter, value-label slot, or the card), and no two label boxes overlap (axis-aligned rectangle intersection over `<text>` boxes within one chart); a deliberately overflowing fixture proves the assertion can fail.
- Rasterized review: *Given* `scripts/svg-to-png.sh` (dev-only; invokes `resvg` or another installed rasteriser, never a crate dependency and never part of the wasm), *When* it renders every golden SVG and the worst-case set to PNG at 760 px and at a 320 px-wide display, *Then* the PNGs are produced for human review. Rule recorded in `testdata/README.md`: a pull request that updates any golden SVG must attach the PNGs and the reviewer opens them; the worst-case set is reviewed in 5.1.3a.
- *Given* the same input rendered twice natively, *When* compared, *Then* bytes are identical. (Native-vs-wasm byte identity is asserted in 4.2.2a, the wave-19 integration task that owns `test/golden.test.js` and the wasm.)
**Files**: `crates/benchdiff-core/tests/svg_security.rs`, `crates/benchdiff-core/tests/svg_geometry.rs`, `crates/benchdiff-core/tests/svg_snapshots.rs`, `scripts/svg-to-png.sh`, `testdata/svg-worst/*`, `testdata/README.md`

##### Task 3.2.5a: Corpus, determinism, geometry, snapshot tests and rasterize script (M)
- Files: `crates/benchdiff-core/tests/svg_security.rs`, `crates/benchdiff-core/tests/svg_geometry.rs`, `crates/benchdiff-core/tests/svg_snapshots.rs`, `scripts/svg-to-png.sh`, `testdata/svg-worst/*`, `testdata/README.md`
- `testdata/README.md` gets the PNG-review rule (a PR that updates a golden SVG attaches the PNGs from `scripts/svg-to-png.sh`) and the worst-case set description; `svg_snapshots.rs` compares each chart kind and theme with the committed `tests/snapshots/svg/*` files that 3.2.1a/3.2.2a/3.2.3a created. `scripts/` is created by 0.1.2a (W1), so this task only adds one file there.

### Story 3.2.6: Performance budget
**As a** maintainer, **I want** the < 1 s / 10k metrics SLO (requirements.md) measured, **so that** regex extraction, N-1 Mann-Whitney per row and SVG generation cannot regress silently.
**Acceptance Criteria**:
- Two-way scale.
  - *Given* a generated set of 10,000 metrics (10 samples each) in base and head, *When* `compare` + `render_markdown` run in a test executed with `cargo test --release` (`make perf`; the test is `#[ignore]`d in the default debug `cargo test` and CI runs `make perf`, so the budget is actually enforced), *Then* wall time < 1 s.
- N-way scale.
  - *Given* 10 variants x 300 benchmarks (3,000 metrics, 10 samples each) with two user dimension rules, *When* extraction, `compare_matrix` (Holm on), `render_matrix_markdown` and `render_svgs(delta/ranking/distribution)` run, *Then* total wall time < 1 s native.
- WASM.
  - *Given* the same N-way input run through `dist/benchdiff.wasm` in `test/perf.test.js`, *Then* wall time < 1 s (generous machine-variance guard: fail only above 2 s, warn between 1 s and 2 s, so CI noise does not flake; the number is logged).
**Files**: `crates/benchdiff-core/tests/perf.rs`, `crates/benchdiff-core/tests/nway_scale.rs`, `test/perf.test.js`

##### Task 3.2.6a: Perf tests (S)
- Files: `crates/benchdiff-core/tests/perf.rs`, `crates/benchdiff-core/tests/nway_scale.rs`, `test/perf.test.js`, `.github/workflows/ci.yml`, `Makefile`
- `nway_scale.rs` holds `nway_should_complete_and_report_caps_when_ten_variants_hundreds_of_rows` (no timing assertion; the caps and notices are what is checked).
- Deterministic generators (seeded), no network. Adds the `make perf` target (`cargo test --release -- --ignored perf`) and its CI step (`ci.yml`/`Makefile` owner in W15). Runs after 3.3.1b/3.3.3a (needs all charts) and after the wave-14 wasm.

## Epic 3.3: Integration into the pipeline

### Story 3.3.1: Chart options, `--svg-dir` and placeholders
**As a** CLI user, **I want** SVG files written locally, **so that** I can preview without a PR.
Split by owner (round 1 triad; file ownership and waves in the tables above): **(a)** `SvgOptions`, overlay, chart config keys and CLI flags (W6); **(c)** `RenderExtras`, `render_markdown_with`, placeholders and the text block in Markdown (W12); **(b)** `render_svgs` and the `--svg-dir` write (W13). Each AC below is tagged with the task that makes it pass.
**Acceptance Criteria**:
- (b) *Given* `--charts delta --svg-dir out/`, *When* run, *Then* `out/delta.svg` exists and stderr lists `wrote out/delta.svg` (stdout stays the report only).
- (b) *Given* additionally `--svg-embed`, *When* run, *Then* the Markdown contains `![Benchmark change, head vs base: 1 regressed, 1 improved; top: BenchmarkSort +30.0%](benchdiff-svg:delta.svg)` (alt text = verdict and top change, 3.3.2) directly under the summary line, followed by the collapsed `<details><summary>Chart data (text)</summary>` block (3.3.2); without `--svg-embed` no placeholder and no details block appear. The placeholder is an Action integration; the CLI default is off and the README documents that CLI stdout with `--svg-embed` carries dangling `benchdiff-svg:` links. (The Markdown insertion is made by (c); the CLI-level observation is asserted by (b), the first task where flags, renderer and Markdown meet.)
- Embedding and publishing vocabulary (one name per concept in CLI flag, config key and Action input; defaults by surface). `svg-embed` accepts `true|false|auto`: the CLI and a config file default to `false`; `auto` is a default-layer value, never an override of a concrete value from any layer: it embeds iff `charts` is non-empty. The Action passes `--svg-embed auto` when its input is unset, so for the Action a non-empty `charts` implies embed unless `svg-embed: false`. `publish-images` accepts `true|false`, default `true` on every surface, and is consulted only when embedding is effective; it has no effect on a bare CLI run (the CLI never publishes).
  - (a) *Given* `{"version":1,"svg-embed":false}`, *Then* the resolved `EmbedMode` is `Off`, and a later `auto` default layer does not override it (unit test on `SvgOptionsOverlay`); *Given* `--charts delta --svg-embed auto` with `charts` non-empty, *Then* it resolves `On`; *Given* `--svg-embed auto` with no `charts`, *Then* `Off`.
  - (b) *Given* `--charts delta --svg-embed auto` (the Action's argv), *Then* the placeholder and details block appear; *Given* `--charts delta --svg-embed auto` with config `{"version":1,"svg-embed":false}`, *Then* none appear (a concrete config value beats `auto`); *Given* `--charts delta` alone, *Then* none appear (CLI default off); *Given* `--svg-embed auto` with no `charts`, *Then* none appear.
  - (c) *Given* `charts not embedded` extras (`RenderExtras { embed: false, publish_images: false, svg_dir }`), *When* `render_markdown_with` runs, *Then* no placeholder is emitted, the Markdown carries the single line `charts not embedded: publish-images is false; SVGs written to <svg-dir>` followed by the `Chart data (text)` block opened.
  - (b) *Given* `--charts delta --svg-embed --publish-images=false`, *When* run, *Then* that same output appears via the CLI and files are still written.
  - (a) *Given* `{"version":1,"publish-images":"yes"}`, *Then* error `publish-images: "yes" is not valid; use true, false`.
- (b) Unwritable `--svg-dir` (AC-S3-8, opt-in contract): *Given* `--charts delta --svg-dir /nonexistent-ro/out`, *When* `create_dir_all` or a write fails, *Then* the report is printed on stdout first, then stderr `cannot write /nonexistent-ro/out: <cause>; report printed without charts`, exit 2 (charts were explicitly requested, so the opt-in contract fails loudly); the same failure inside the Action (where JS creates the dir) is a warning, never a failed job (AC-X-2, scoped to the Action and hosting).
- (b) *Given* no `--charts`, *When* run, *Then* output equals expected goldens (charts default `none`).
- (b) Chart dispatch (moved here from Story 3.2.4 in round 2, because `render_svgs` is created by this task). Nothing to compare: *Given* a head-only run, *When* charts are requested, *Then* no file, `SkipReason::NothingToCompare`, Markdown line `chart skipped: nothing to compare`.
- (b) Chart kind versus report kind (one `SkipReason::NotApplicable{kind}` per mismatch; never an error; the rest still render):

  | Requested | Two-way `Report` | N-way `MatrixReport` |
  |---|---|---|
  | `delta` | renders `delta.svg` | skipped `NotApplicable`: `delta is two-way; use ranking` |
  | `ranking` | skipped `NotApplicable`: `ranking needs an N-way run (compare-by or variants)` | renders `ranking.svg` |
  | `distribution` | renders `distribution.svg` (needs samples) | renders `distribution.svg` (needs samples) |

  - (b) *Given* `charts=delta,ranking` on a two-way run, *When* rendered, *Then* `delta.svg` is produced, `ranking` is reported as skipped with the reason above in the Markdown and the log line (`produced=1 skipped=1`).
- (c) API compatibility: *Given* the existing public `render_markdown(&Report)`, *Then* its signature is unchanged and it is a thin wrapper over the new `render_markdown_with(&Report, &RenderExtras)` (`RenderExtras::default()` leaves bytes identical); all existing tests that call `render_markdown` compile untouched. The same holds for the matrix: `render_matrix_markdown(&MatrixReport)` (2.2.1a, signature frozen, already called by 2.2.2b's `main.rs` code) becomes a thin wrapper over the new `render_matrix_markdown_with(&MatrixReport, &RenderExtras)`; no call site changes in this wave, and 3.3.1b (W13) is the task that switches `main.rs` to the `_with` forms.
- (a) Chart config keys (added to the 1.2.2a key table here, at the wave that owns `ChartKind`):
  - *Given* `{"version":1,"charts":"bars"}`, *Then* error `charts: "bars" is not valid; use none, delta, ranking, distribution (comma-separated)`.
  - *Given* `{"version":1,"chart-row":10}`, *Then* `unknown key 'chart-row'; did you mean 'chart-rows'?`.
  - *Given* `chart-rows` 0 or `chart-width` 100, *Then* the newtype constructors reject with `chart-rows: 0 is not valid; use 1 to 200` and `chart-width: 100 is not valid; use 480 to 1600` (bounds stated in `--help`).
- (a) `--help` lists `--charts`, `--chart-rows`, `--chart-width`, `--chart-theme`, `--svg-dir`, `--svg-embed`, `--publish-images` with default and allowed values, generated from the same key table (G-5); `--chart-rows` states that the effective cap is `min(chart-rows, max-rows)` when `max-rows` is set.
- (c) Notices in two-way reports: chart notices (skipped, capped, not embedded, not hosted) are `Notice`s of class `chart`, rendered by `render_notices` (2.2.1b) after the existing skipped-metrics line, with the same 5-line cap and overflow line.
- (c) The placeholder scheme is one constant, `PLACEHOLDER_SCHEME = "benchdiff-svg:"`, in core; `lib/images.js` mirrors it and a test asserts the two strings are equal.
**Files**: (a) `crates/benchdiff-core/src/svg/mod.rs` (`SvgOptions`, `ChartKind`, `ChartTheme`, `ChartRows`, `ChartWidth`, `EmbedMode`), `crates/benchdiff-core/src/options.rs`, `crates/benchdiff-core/src/config.rs`, `crates/benchdiff/src/main.rs`, `crates/benchdiff/tests/svg_cli.rs`; (b) `crates/benchdiff-core/src/svg/mod.rs` (`render_svgs`), `crates/benchdiff-core/src/lib.rs`, `crates/benchdiff/src/main.rs`, `crates/benchdiff/tests/svg_cli.rs`; (c) `crates/benchdiff-core/src/markdown.rs` (`render_markdown_with` + wrapper), `crates/benchdiff-core/src/markdown_matrix.rs`, `crates/benchdiff-core/src/lib.rs`

##### Task 3.3.1a: `SvgOptions`, `SvgOptionsOverlay`, chart config keys and CLI flags (S)
- Files: `crates/benchdiff-core/src/svg/mod.rs`, `crates/benchdiff-core/src/options.rs`, `crates/benchdiff-core/src/config.rs`, `crates/benchdiff/src/main.rs`, `crates/benchdiff/tests/svg_cli.rs`
- Add the types and enums (`SvgOptions`, `ChartKind`, `ChartTheme`, `ChartRows`, `ChartWidth`, `EmbedMode`) and the output types every later chart and Markdown task uses: `SvgFile { name, svg, alt, skipped }`, `SkipReason` (`NothingToCompare`, `NoSamples`, `CapExceeded{shown,total}`, `Empty`, `NotApplicable{kind}`) and `TextRow` (glossary), with the convention that each chart function returns `Result<SvgFile, SkipReason>` (`Err` = no file). `SvgInput` is not defined here (it needs `MatrixReport`; it comes with 3.2.4a). Also `SvgOptionsOverlay` (separate from `Options`/`MatrixOptions`), the config keys `charts`, `chart-rows`, `chart-width`, `chart-theme`, `svg-dir`, `svg-embed`, `publish-images` in the `config.rs` key table, and the matching flags with `--help` text. No rendering, no file writes. Scheduled at W6 so that every chart task (3.2.1a onward) takes the one options type.
- AC: the (a)-tagged ACs above. Test approach: unit tests on overlay/enum parsing; `svg_cli.rs` help test; `make golden` unchanged.

##### Task 3.3.1b: `render_svgs` and the `--svg-dir` write (S)
- Files: `crates/benchdiff-core/src/svg/mod.rs`, `crates/benchdiff-core/src/lib.rs`, `crates/benchdiff/src/main.rs`, `crates/benchdiff/tests/svg_cli.rs`
- `render_svgs(SvgInput, &SvgOptions) -> Vec<SvgFile>` public API: the dispatcher over the chart kinds finished by 3.2.1a-3.2.4a, owning the chart-dispatch ACs above (`NothingToCompare`, the kind-versus-report-kind table, `NotApplicable`), crate-root re-export (with the `SvgOptions` family), `create_dir_all` and file writes natively with the error path above (JS creates the dir for wasm), the stderr `wrote ...` lines, setting `Options.keep_samples` when `distribution` is requested, and switching the `main.rs` call sites from `render_markdown` / `render_matrix_markdown` to `render_markdown_with` / `render_matrix_markdown_with` with the `RenderExtras` built from the results. Re-measure wasm size (0.1.2a note).
- AC: the (b)-tagged ACs above. Test approach: `svg_cli.rs` integration tests spawning the binary.

##### Task 3.3.1c: `RenderExtras`, `render_markdown_with`, placeholders and the text block (M)
- Files: `crates/benchdiff-core/src/markdown.rs`, `crates/benchdiff-core/src/markdown_matrix.rs`, `crates/benchdiff-core/src/lib.rs`
- `RenderExtras` (placeholders, skipped charts, embed/publish flags, svg-dir text, notices), `render_markdown_with` with `render_markdown` as a thin wrapper, and, additively, `render_matrix_markdown_with(&MatrixReport, &RenderExtras)` with the frozen `render_matrix_markdown(&MatrixReport)` (2.2.1a) reduced to a wrapper over it (the old function keeps its signature, so 2.2.2b's `main.rs` call site in the same wave stays valid), placeholder insertion directly under the summary/verdict block and the `Chart data (text)` block (built by 3.3.2a's `chart_text_table`) in both Markdown modules, chart `Notice`s through `render_notices`. Needs only chart names and kinds, not rendered SVG bytes, which is why it can run in the same wave as 2.2.2b.
- AC: the (c)-tagged ACs above. Test approach: unit/snapshot tests with hand-built `RenderExtras`; the existing `render_markdown` tests compile untouched; `make golden`.

### Story 3.3.2: Alt text and collapsed table
**As a** screen-reader user or email reader, **I want** text alternatives.
**Acceptance Criteria**:
- Alt text names the verdict AND the top change (triad round 2; AC-S1-3 always said both, the examples carried only the verdict).
  - *Given* a delta chart with 1 regressed, 1 improved and the largest |change| `BenchmarkSort` +30.0%, *When* alt text is built, *Then* it reads `Benchmark change, head vs base: 1 regressed, 1 improved; top: BenchmarkSort +30.0%`, is <= 125 characters and equals the SVG `<title>`; full `<desc>` contains the top 5 changes with values, reference and cap notice. *Given* all rows unchanged, *Then* the verdict part reads `no significant changes` and `top:` still names the largest |change|.
  - *Given* a ranking chart where `merge` has the best geomean 0.81x vs reference `quick` and the largest gap is `heap` 1.30x on `Sort/size=100`, *Then* the alt text reads `Ratio vs quick: merge has the best geomean (0.81x); largest gap: heap 1.30x on Sort/size=100`; it never uses `higher`, `lower` or `regress` (the prose scan of 2.2.1 covers it).
  - *Given* a 200-character benchmark name, *Then* the name is middle-truncated first so that the verdict and the number are never cut, the whole alt text is still <= 125 characters, and it equals `<title>`; if verdict plus number alone exceed 125, the `top:` clause is dropped before the verdict is shortened.
- Text block for N-way charts (the fallback content, triad round 2: what the opened `Chart data (text)` block shows when there is no image).
  - *Given* an embedded or fallen-back ranking chart, *Then* the block is a table with the columns `# | Benchmark | <reference> (ref) | <variant> | ...` whose rows are exactly `select_rows` of the chart (same cap, same order, same `#` numbers, same labels); cells hold the ratio vs the reference in the matrix cell style (`0.74x 🟢`, `1.05x ~`, `1.00x` for the reference column, `—` for Missing, `⚠️ invalid` for Invalid), up to 8 variant columns like the main table; reference-absent rows are left out and counted in the line `N rows have no reference value and are not charted`. The block ends with `+ N more not shown (chart-rows: 30)` when capped, and starts with the one-line sentence `Rows are in chart order (largest change first); the table above follows the sort setting.` *Given* a distribution chart, *Then* the cells read `median (min-max, n)` per variant instead of ratios. When several charts are embedded there is one block per chart, titled `Chart data (text): ranking`, `Chart data (text): distribution`; with one chart the title stays `Chart data (text)`.
- - GitHub shows the chart as an opaque `<img>`: the SVG's `role`, `<title>` and `<desc>` are not exposed to assistive technology there, so the Markdown alt text and the `Chart data (text)` table are the accessible path on GitHub (README and ux.md S2 say so); the in-SVG semantics serve the standalone file.
- Hosting failure leaves the full table (already present).
- Collapsed text table (AC-S1-3; the story title always promised it, round 3 adds the AC).
  - *Given* a chart that is embedded, *When* Markdown renders (two-way via `render_markdown_with`, N-way via `render_matrix_markdown`), *Then* directly after the image comes `<details><summary>Chart data (text)</summary>` followed by a text table with exactly the rows the chart shows (same cap, same order, same values, `—` for Missing, plus the `▰` bar column on two-way) and `</details>`; the block lists the `+ N more not shown` line when the chart is capped; all names pass through the existing Markdown escaping (hostile-name test).
  - *Given* images are not hosted (publish failed, `publish-images` false), *Then* the block is emitted as `<details open>` so the chart content is visible without a click (the rewrite/strip step in 4.1.1 does this on fallback).
  - *Given* no chart was produced (skipped, or embed off), *Then* no details block is emitted.
**Files**: `crates/benchdiff-core/src/svg/mod.rs` (`chart_text_table`; the Markdown insertion is owned by 3.3.1c)

##### Task 3.3.2a: Implement alt text and `chart_text_table` (S)
- Files: `crates/benchdiff-core/src/svg/mod.rs`
- `chart_text_table(&SvgInput, &SvgOptions) -> Vec<TextRow>` reuses `select_rows` (3.2.4a, W9; `SvgInput` is defined there) so the table and the chart cannot diverge; 3.3.1c (W12) renders it into the Markdown. Takes the `SvgOptions` and `TextRow` that 3.3.1a (W6) defined. Also builds the alt text / `<title>` sentences above (verdict plus top change, truncation order) for all three chart kinds.

### Story 3.3.3: Theme-pair embedding (`chart-theme: both`)
**As a** reader on a dark GitHub theme, **I want** the matching chart variant, **so that** a theme pair works in comments and summaries.
**Acceptance Criteria**:
- Placeholder form for a pair (core emits it; ADR-003 item 6 amended): `<picture><source media="(prefers-color-scheme: dark)" srcset="benchdiff-svg:NAME-dark.svg"><img alt="ALT" src="benchdiff-svg:NAME-light.svg"></picture>` where `ALT` is HTML-escaped. Single-file themes keep `![alt](benchdiff-svg:NAME.svg)`. (If 0.2.1b shows `<picture>` fails and the `#gh-light-mode-only` fragment variant works, only this task's emitted form changes.)
  - *Given* `chart-theme=both` and `svg-embed`, *When* Markdown renders, *Then* the block above appears with escaped alt text; with `light` or `dark` only, the `![]()` form appears.
- Rewrite: *Given* that block and `urls` for both files, *When* `rewritePlaceholders` runs, *Then* both `benchdiff-svg:` tokens (found by one regex over `benchdiff-svg:[A-Za-z0-9._-]+\.svg`, in `srcset=`, `src=` or `(...)`) become hosted URLs.
- Strip on fallback: *Given* a failed publish, *When* applied, *Then* the whole `<picture>...</picture>` block is removed (not left as an empty element) and the same one-line warning as the `![]()` case is added once, not twice.
- `clip()` (4.1.3a) treats `<picture>...</picture>` as an unbreakable unit like image markup.
**Files**: `crates/benchdiff-core/src/markdown.rs` (placeholder builder), `crates/benchdiff-core/src/markdown_matrix.rs`, `lib/images.js`, `test/images.test.js`

##### Task 3.3.3a: Emit and rewrite/strip the `<picture>` block (S)
- Files: `crates/benchdiff-core/src/markdown.rs`, `crates/benchdiff-core/src/markdown_matrix.rs`, `lib/images.js`, `test/images.test.js`

---

# PHASE 4: Delivery to GitHub

## Epic 4.1: Image publishing

### Story 4.1.1: `publishImages()` seam
**As a** maintainer, **I want** one swappable function, **so that** the hosting mechanism can change without touching `main()`.
**Acceptance Criteria**:
- Rewrite.
  - *Given* Markdown with `![a](benchdiff-svg:delta.svg)` and `urls {'delta.svg':'https://raw.githubusercontent.com/o/r/abc/x/delta.svg'}`, *When* `rewritePlaceholders` runs, *Then* the placeholder becomes the URL and the sticky marker remains the first line.
- Strip + warn.
  - *Given* `{error:'read-only token on a fork PR', reason:'fork'}`, *When* applied, *Then* placeholder lines are removed, the `Chart data (text)` block is opened (`<details>` becomes `<details open>`), and the warning line `⚠️ charts could not be hosted: read-only token on a fork PR; SVGs are in <svg-dir>; to keep them, upload that directory with actions/upload-artifact` is added **in the Markdown body** (so it reaches the sticky comment and the job summary, not only the log); a `core.warning` is logged as well. Round 2: the line names the directory (`<svg-dir>` is a path on the runner, never an artifact name, because `index.js` cannot upload artifacts: it has no `@actions/artifact`, see ADR-003 item 2) and hints `actions/upload-artifact` as the way to keep the files. For `reason: permission` the line reads `⚠️ charts could not be hosted: token lacks contents: write (grant it to host images); add the permissions block from README "Hosting images"; SVGs are in <svg-dir>; to keep them, upload that directory with actions/upload-artifact`, so the first-run case (a read-only default `GITHUB_TOKEN`, which `charts: delta` alone often meets on the first run; INFERRED: newer repositories and organizations default to read-only workflow permissions, older ones may not; see Story 4.2.1) has its exit path in the line itself. The error strings per reason are fixed here (single source for ux.md): `private` -> `private repository (no anonymous image URL)`, `fork` -> `read-only token on a fork PR`, `permission` -> `token lacks contents: write (grant it to host images)`, `push-failed` -> `could not update benchdiff-assets after 5 attempts`, `unsupported-host` -> `GitHub Enterprise host not supported for hosted images`, `no-event` -> `event payload unreadable`, `disabled` -> `publish-images is false`.
- Exports retained.
  - *Given* `require('./index.js')`, *When* imported by existing tests, *Then* `runWasm, stage, expand, upsertComment, clip` still resolve.
**Files**: `lib/images.js`, `index.js`, `test/images.test.js`

##### Task 4.1.1a: `lib/images.js` with no-op publisher + rewrite/strip (M)
- Files: `lib/images.js`, `index.js`, `test/images.test.js`

### Story 4.1.2: Orphan-branch publisher
**As a** user of a public repo, **I want** images embedded inline.
**Acceptance Criteria**:
- Happy path.
  - *Given* a public repo, same-repo PR, token with `contents: write`, and file `delta.svg` with sha256 prefix `3fa9c1d2`, *When* `publishImages` runs against a mocked `fetch`, *Then* it makes blob/tree/commit/ref calls creating or updating `benchdiff-assets` in one commit and returns `<rawBase>/o/r/<commit-sha>/t-<hash of tag>/3fa9c1d2/delta.svg` (default `rawBase` `https://raw.githubusercontent.com`).
- Ref race with backoff.
  - *Given* the ref update returns 422 twice then succeeds, *When* run, *Then* it re-reads the ref, re-parents each time, waits with exponential backoff plus jitter (injectable `sleep`/`random` in tests; base 250 ms, up to 5 attempts) and succeeds on the third attempt; *Given* 5 consecutive 422s, *Then* `{reason:'push-failed'}`, never a thrown error.
- Create race.
  - *Given* the branch does not exist, *When* `POST /git/refs` returns 422 `Reference already exists` (another job created it first), *Then* the publisher switches to the update path (re-read, re-parent, PATCH) instead of failing.
- Hostile `tag`.
  - *Given* `tag` values `..`, `a/../b`, `/abs`, `x:y` (all pass `validateTag` because of the HTML-comment marker), *When* the tree path is built, *Then* the path component is `t-<first 12 hex of sha256(tag)>`, never the raw tag, so the final path is `<t-hash>/<content-hash>/<name>.svg` and cannot contain `..`, a leading `/`, or `:`.
- Host-aware URLs.
  - *Given* `GITHUB_SERVER_URL=https://github.com` (default), *When* URLs are built, *Then* base is `https://raw.githubusercontent.com`; *Given* any other server URL (GHES), *Then* `{reason:'unsupported-host'}` with no network writes (the raw host is derived from the server URL only for github.com; the base is a `ctx.rawBase` parameter so a future mechanism swaps it without touching `main()`). API calls honour `GITHUB_API_URL` as `upsertComment` does.
- Gating.
  - Source of both signals: the JSON event payload at `GITHUB_EVENT_PATH` (`repository.private`; `pull_request.head.repo.full_name` vs `pull_request.base.repo.full_name`), read with a guarded `JSON.parse` (unreadable or missing payload -> `{reason:'no-event'}`, no writes). *Given* `repository.private == true`, *Then* result `{reason:'private'}` with no network writes; *Given* `GITHUB_EVENT_NAME=pull_request` with head repo != base repo, *Then* `{reason:'fork'}`; *Given* a non-PR event (`push`, `workflow_dispatch`, `release`) on a public repo, *Then* publishing proceeds (no fork check applies; the comment path is skipped by `main()` as today, but job-summary images are still hosted); *Given* a 403 on blob create, *Then* `{reason:'permission'}`.
- Idempotent and growth-bounded (retention rule, pre-mortem notes).
  - *Given* the same bytes published twice, *Then* the path is identical (content hash) and the second run creates **no new commit** (every content-hash path already exists at the branch tip, so the ref is not updated and the existing tip SHA is used in the URLs).
  - *Given* any run, *Then* the publisher never deletes the branch, never force-pushes and never rewrites history, so every commit SHA already embedded in a historical comment stays reachable; a new commit's tree always contains the previous tip's entries (assert that a URL built from the previous tip SHA is still a path in the new tree's parent).
  - Documented in the README and ADR-003: each publishing run adds at most one small commit (the SVGs are capped at 100 KB each), so growth is bounded by (runs x chart bytes); a maintainer who squashes or deletes the branch to reclaim space breaks the images in every old comment (INFERRED: unreachable commits are garbage-collected), which is why rollback advice is to unset `charts`, never to delete the branch. Pruning by squash is out of scope for v1.
- Only GitHub is contacted (NF-4 / G-14).
  - *Given* any publish path with a recording `fetch`, *Then* every requested URL's host equals the API host derived from `GITHUB_API_URL` (default `api.github.com`), and zero `fetch` calls happen for the private, fork, no-event, unsupported-host and disabled reasons.
**Files**: `lib/images.js`, `test/images.test.js`

##### Task 4.1.2a: Git Data API client with retry (M)
- Files: `lib/images.js`, `test/images.test.js`
##### Task 4.1.2b: Gating (private/fork/permission/unsupported-host) and reason mapping (S)
- Files: `lib/images.js`, `test/images.test.js`
##### Task 4.1.2c: Robustness and retention: backoff, create-race, tag path hashing, host-aware base URL, no-op on identical content, history preservation (M)
- Files: `lib/images.js`, `test/images.test.js`
- 4.1.2a-c depend on the 0.2.1b outcome; if the fallback ladder changes the mechanism, only this story changes.

### Story 4.1.3: Truncation with images
**As a** maintainer, **I want** `clip()` never to cut inside image markup.
**Acceptance Criteria**:
- *Given* a 62,000-char body whose cut point falls inside `![alt](https://raw.githubusercontent.com/...)`, *When* `clip` runs, *Then* the cut moves before the `![`, the truncation notice and the marker are intact; the same holds for a cut inside a `<picture>...</picture>` block. This task runs in W10, before 3.3.3a (W13) emits the block, so the test uses a literal `<picture>` string, not core output; an end-to-end re-check with real core output is part of 4.2.2a's `chart-theme=both` case.
**Files**: `index.js`, `test/action.test.js`

##### Task 4.1.3a: Extend closers logic (XS)
- Files: `index.js`, `test/action.test.js`

## Epic 4.2: Action wiring

### Story 4.2.1: Inputs, outputs, `main()`
**As an** Action user, **I want** charts through inputs.
Split by owner (round 2 size review; the old single task touched four files and about a dozen ACs): **(a)** task 4.2.1a (W15) is the declarative surface: `action.yml` inputs, outputs and descriptions, `buildArgs`, the vocabulary test; **(b)** task 4.2.1b (W16) is `main()`: the chart pipeline (create `svg/`, call `publishImages`, rewrite or strip, summary and comment, logs, outputs) and its tests. Each AC is tagged with its owner; both edit `index.js`, `action.yml` and `test/action.test.js` serially.
**Acceptance Criteria**:
- (a) New inputs (all default empty, kebab-case, each equal to a CLI flag and config key): `config`, `compare-by`, `group-by`, `dimensions`, `reference`, `charts`, `chart-rows`, `chart-width`, `chart-theme`, `svg-dir`, `svg-embed`, `publish-images`, `correction`, `dimensions-strict`, `cell-format` (N-way label input `variants` is Story 4.2.3; `head` and `base` keep exactly today's meaning, including `=` in paths). New outputs `svg-dir`, `svg-paths`.
- (a) Defaults for the Action (decided in round 3, closes the `charts: delta` alone ambiguity): a non-empty `charts` implies embed and publish unless the user opts out. `buildArgs` passes `--svg-embed auto` when the `svg-embed` input is empty (3.3.1), `--svg-embed <v>` when set; `publish-images` is passed through only when set and defaults to `true`. The CLI default stays off (`svg-embed` false).
  - (b) *Given* `charts: delta` alone (nothing else), a mocked publisher returning URLs, *When* `main` runs, *Then* the job summary and the comment body contain the hosted URL image (AC-S5-4) and `svg-paths` lists `.../delta.svg`.
  - (b) *Given* `charts: delta`, `svg-embed: false`, *Then* `delta.svg` is written and listed in `svg-paths`, the Markdown has no image and no details block, and the recording `fetch` saw zero publish calls.
  - (b) *Given* `charts: delta`, `publish-images: false`, *Then* the Markdown has the line `charts not embedded: publish-images is false; SVGs written to <svg-dir>` and the opened `Chart data (text)` block, zero publish calls, job succeeds.
  - (b) *Given* `svg-embed: true` with `charts` empty, *Then* nothing is embedded and no error is raised.
  - (b) *Given* `charts` set only inside the config file and `svg-embed` input unset, *Then* embedding still applies (the `auto` default is resolved in the binary after layering).
  - (b) *Given* the sticky comment already contains a run with `charts: delta` and the next run has `charts` unset (AC-X-3), *When* `upsertComment` updates it, *Then* the new body equals the original golden body (marker preserved, no `raw.githubusercontent.com`, no `benchdiff-svg:` token).
  - (a) *Given* `action.yml`, the CLI `--help` text and the config key table, *When* the vocabulary test (`crates/benchdiff/tests/vocabulary.rs` reading `action.yml` as text) runs, *Then* the three name sets are compared with allowlists that match the real files (round 2 corrected the earlier list: `setup`, `bench-command` and `token` are not inputs of `action.yml`; `bench-command` belongs to the reusable `release-benchmarks.yml`, README). Verified today: the inputs are `head, base, format, base-format, threshold, min-abs, only-changed, sort, max-rows, title, tag, fail-on-regression, comment, github-token`; the CLI flags are `--head --base --format --base-format --threshold --min-abs --alpha --no-stats --only-changed --sort --max-rows --title --tag --fail-on-regression --markdown --json --head-json --help`. The rules:
    - every config key is also a CLI flag and an Action input, except the config-only names `version`, `$schema`, `thresholds`; the converse is not required for `config` (a flag and an input that is not a key, since a file cannot name itself);
    - every CLI flag without an Action input is classified in the test: `--variant` (Action: `variants`), the diagnostics `--print-config`, `--print-dimensions`, `--help`, and the pre-existing CLI-only flags `--alpha` and `--no-stats` (statistics tuning, not exposed to the Action) and `--markdown`, `--json`, `--head-json` (the Action passes these itself and exposes the paths as the outputs `report-path`, `json-path`, `head-json-path`);
    - every Action input without a CLI flag is classified: `comment`, `github-token` (the comment upsert is done by `index.js`, not the binary) and `variants`;
    - the lists live in the test, a new name must be classified, and an allowlisted name that no longer exists fails the test, so the lists cannot rot.
  - (a) Hosting side effects are disclosed where the user opts in (triad round 1). *Given* `action.yml`, *Then* the descriptions of `charts`, `svg-embed` and `publish-images` state: hosting needs `permissions: contents: write`; it creates or updates the branch `benchdiff-assets` with at most one commit per run that has new bytes; commits are kept and never pruned (deleting the branch breaks images in old comments); hosted images are readable by anyone who has the URL, which for a public repo is the same audience as the comment; private repositories are never hosted (reason `private`), so nothing is written there. (b) *Given* a run that publishes, *Then* the step log carries one line before the first write: `images: publishing to branch benchdiff-assets (needs contents: write; commits are kept; see README "Hosting images")`. The same text is the README section (5.1.1).
  - First run with `charts: delta` alone (triad round 2). With a read-only default `GITHUB_TOKEN` the first run often reports `permission` (INFERRED: newer repositories and organizations default to read-only workflow permissions). (a) The `charts` description in `action.yml` and the README section "Hosting images" carry the permissions snippet, `permissions:` with `contents: write` (host the images) and `pull-requests: write` (the sticky comment, README), and say that the first run on a read-only token shows the permission warning. (b) *Given* `charts: delta` alone and a publisher returning `{reason:'permission'}`, *Then* the comment body carries the permission line of 4.1.1 (which points to the snippet), the opened `Chart data (text)` block, `svg-paths` still lists the file, the log prints the snippet once, and the job passes.
  - (b) *Given* a matrix run through `main()`, *Then* the Action outputs `regressed`/`improved`/`unchanged`/`new`/`removed` are the companion counts (cells versus the reference, ADR-005 item 7) and `failed` is the regression gate result (false when `fail-on-regression` is unset); `action.yml` descriptions of those outputs say "N-way runs: cells vs the reference".
  - (a)+(b) *Given* no new inputs, *When* `buildArgs` (a) and `main` (b) run, *Then* argv, outputs and bodies equal today's goldens.
  - (b) *Given* `svg/` must exist, *Then* JS creates `<workDir>/svg` before invoking wasm and copies produced files to the `svg-dir` workspace path afterwards.
  - (b) Log line `charts: requested=delta produced=1 skipped=0` emitted (and `skipped=1 (NotApplicable: ...)` for kind mismatches, 3.3.1).
  - (a) Documented: `only-changed` and `fail-on-regression` set in a config file cannot be switched back off by an input (no negative flag).
  - (b) Every warning and error string `index.js` emits for charts and hosting is a named constant collected in `test/ux_messages.test.js`, which asserts each names an exit path (AC-X-1).
**Files**: `action.yml`, `index.js`, `test/action.test.js`, `test/ux_messages.test.js`, `crates/benchdiff/tests/vocabulary.rs`

##### Task 4.2.1a: Inputs, outputs, descriptions, `buildArgs`, vocabulary test (M)
- Files: `action.yml`, `index.js` (`buildArgs` and input parsing only), `test/action.test.js`, `crates/benchdiff/tests/vocabulary.rs`
- The (a)-tagged ACs: the new inputs and outputs declared in `action.yml` with allowed values, defaults and the hosting disclosure and permissions snippet in the descriptions; `buildArgs` passes `--svg-embed auto` when unset and the flags for every new input; the vocabulary test with the corrected allowlists.

##### Task 4.2.1b: `main()` chart pipeline, outputs, logs and messages (M)
- Files: `action.yml` (output wiring only if 4.2.1a left a description to adjust), `index.js` (`main()` only), `test/action.test.js`, `test/ux_messages.test.js`
- The (b)-tagged ACs: create `svg/`, run the binary, call `publishImages` (4.1.1a seam) only when embedding is effective, rewrite or strip, write comment and step summary, set `svg-dir` / `svg-paths` and the matrix-run outputs, log lines, the first-run permission case, and the AC-X-3 two-run comment test. Needs 3.3.1b (CLI writes SVGs), 4.1.3a (`clip` with images) and 4.2.1a (W15).

### Story 4.2.2: Golden shim for SVG
**As a** maintainer, **I want** native-vs-wasm SVG determinism and expected files.
**Acceptance Criteria**:
- *Given* `testdata/nway/*` and `--charts delta,ranking --svg-dir`, *When* run natively and under wasm, *Then* both directories contain byte-identical files equal to `testdata/nway/expected/svg/*.svg`.
**Files**: `test/golden.test.js`, `testdata/nway/expected/svg/*`, `dist/benchdiff.wasm`

##### Task 4.2.2a: Add cases; wave-19 wasm integration (S)
- Files: `test/golden.test.js`, `testdata/nway/expected/svg/*.svg`, `testdata/README.md`, `dist/benchdiff.wasm`
- Integration task: runs in wave 19 with tasks that change no Rust source (4.2.3c, 5.1.1a, 5.1.4a), rebuilds and commits `dist/benchdiff.wasm` from the merged tree. Also asserts `chart-theme=both` file pairs and the `<picture>` Markdown are byte-identical native vs wasm.

### Story 4.2.3: Action `variants` input (N-way from labelled files)
**As an** Action user, **I want** to compare labelled files side by side from a workflow, **so that** the N >= 3 success metric is delivered by the Action, not only the CLI (ADV-2, ADR-007).
Split by owner (round 2 size review): **(a)** task 4.2.3a (W17) is the pure side: parsing, glob resolution, staging, `buildArgs`, the `main()` guard; **(b)** task 4.2.3b (W18) is the runtime degradation (drop-and-notice, notice-only report without a binary run, strict mode, outputs); **(c)** task 4.2.3c (W19, the old 4.2.3b) is the end-to-end test. Each AC is tagged with its owner; all three edit `index.js`/`action.yml`/`test/action.test.js` serially (4.2.3c only the test).
**Acceptance Criteria**:
- (a) Grammar: input `variants` is newline-separated `label=path-or-glob` lines (commas are NOT separators, so globs like `{a,b}` survive). Each line splits at the FIRST `=`; label must match `^[A-Za-z0-9._-]{1,40}$` (so it can never contain `/`), applied BEFORE glob expansion.
  - *Given* lines `quick=out/quick.txt` and `heap=out/*heap*.txt`, *When* `buildArgs` runs, *Then* argv contains `--variant quick=/work/variant-0-quick.txt --variant heap=/work/variant-1-<basename>` in input order, and `/work/...` copies have identical bytes.
  - *Given* a path containing `=` (`merge=results/size=100/m.txt`), *Then* label `merge`, path `results/size=100/m.txt`.
  - *Given* a line without a valid label (`results/size=100/m.txt`), *Then* `core.setFailed`-style error `variants: line 3 'results/size=100/m.txt' has no valid label; expected label=path (label: A-Za-z0-9._-, max 40)`.
- (a) Staging: `stage()` names variant files `variant-<index>-<sanitized basename>` (basename characters outside `[A-Za-z0-9._-]` become `_`) so staged paths never carry `=`; labels are passed to the binary via `--variant`, never encoded in file names.
- Glob and uniqueness (one rule, stated once): (a) a glob that matches **more than one** file is a static error `variants: label 'heap' matched 3 files; a label is one file` (ambiguous, a user error: the job fails); (b) a glob that matches **zero** files is data-dependent and degrades with a warning (next bullet). (a) Duplicate labels -> `label "quick" given twice` (static).
- (b) Missing, empty or zero-metric variant files are data-dependent (a benchmark job crashed): zero glob matches (`label 'heap': no file matches 'out/*heap*.txt'`), a missing path, an empty file, or a file that yields 0 metrics.
  - *Given* three labelled lines where `heap`'s file is empty and `dimensions-strict` is unset, *When* `main()` runs, *Then* `heap` is dropped with the notice `variant 'heap' produced no data (out/heap.txt); comparing quick and merge`, a matrix of the remaining two variants is posted, the job passes (exit 0).
  - *Given* the dropped variant is the reference (explicit or first line), *Then* no binary run happens, `render_notice_report` text is posted (`reference 'quick' produced no data; no comparison made`) with the sticky marker, and the job passes.
  - *Given* fewer than two usable variants remain, *Then* the same notice-only comment is posted.
  - *Given* `dimensions-strict: true`, *Then* each of the above fails the step with the same text and no comment is required.
- (a) Interaction: `variants` together with `head` or `base` -> error `variants cannot be combined with head/base`. `reference` defaults to the first line; an unknown `reference` surfaces the binary's exit-2 text; `fail-on-regression` together with `variants` and no `reference` input surfaces the binary's static error `fail-on-regression in an N-way run needs an explicit 'reference' ...` (step fails, no comment required).
- (a) `main()` guard: today `main()` throws `no head files match` when `head` expands to nothing. *Given* `variants` non-empty and `head`/`base` empty, *Then* the guard is skipped; *Given* `head` empty and `variants` empty, *Then* the existing error is unchanged.
- (a) `--head-json` / `head-json-path`: `buildArgs` passes `--head-json /work/head.json` only for two-way runs (the binary rejects it with `--variant`, 2.2.2). For `variants` runs the output `head-json-path` is the empty string (there is no single head set to cache as the next base); `json-path` is the matrix JSON. `action.yml` descriptions and README say so.
- (a) Format inputs: `format` applies to every variant file (`--format`, auto-detected per file); `base-format` together with `variants` is an error `base-format cannot be combined with variants`.
- (a) Globs: a new single-pattern resolver `resolveOne(pattern, cwd)` reuses the matcher `expand()` uses per entry but does NOT split on `[\n,]`; `expand()` stays as is for `head`/`base`. *Given* `merge=out/{a,b}.txt` matching two files, *Then* the error `label 'merge' matched 2 files`; *Given* it matches exactly one, *Then* it resolves.
- (b) Outputs for matrix runs: `regressed`/`improved`/`unchanged`/`new`/`removed` and `failed` come from the companion `counts`/`failed` fields (2.1.6a, all keys populated, Missing cells counted as `removed`/`new` per its mapping); `report-path` and `json-path` point to the matrix Markdown and JSON.
- (c) End-to-end (task 4.2.3c, run again against the final wasm in W20 by 5.1.2a; title and tag are the Action defaults used to generate the expected file, 2.2.3a): *Given* the committed `testdata/nway/{quick,heap,merge}.txt` and `variants` of three labelled lines, *When* `main()` runs with the real `dist/benchdiff.wasm` and a mocked `fetch`, *Then* the comment body equals `testdata/nway/expected/matrix.md` (marker first line), and with `reference: quick`, `fail-on-regression: 10` and a cell Regressed over 10% vs `quick` the job is marked failed (`core.setFailed`-equivalent) while the comment is still posted.
**Files**: `action.yml`, `index.js`, `test/action.test.js`

##### Task 4.2.3a: `variants` parsing, staging, `buildArgs`, guard (M)
- Files: `action.yml` (the `variants` input description), `index.js`, `test/action.test.js`
- The (a)-tagged ACs: pure functions `parseVariants(text)->[{label,path}]` and `resolveOne`, the relaxed `main()` guard, the `buildArgs` changes (no `--head-json`, `--variant` lines, format handling) and extended `stage`; unit tests for each (a) bullet (including `=` in paths, `{a,b}` globs, and the existing `expand()` behaviour for `head`/`base` untouched).

##### Task 4.2.3b: `variants` data-error handling and matrix outputs (S)
- Files: `action.yml` (output descriptions for N-way runs), `index.js` (`main()`), `test/action.test.js`
- The (b)-tagged ACs: drop-and-notice handling of missing/empty/zero-metric/zero-glob variants (the remaining variants still go to the binary), the no-binary notice-only path when the reference is lost or fewer than two variants remain (warn-by-default, `dimensions-strict` fails), and the matrix-run outputs. The notice-only body is built in JS (the core function cannot run without the binary): `index.js` mirrors `render_notice_report` (marker, `### <title>`, one line per notice), and a test asserts the JS output equals the committed `testdata/nway/expected/notice-only.md`, the same file the Rust snapshot test of 2.2.1b asserts against, so the two cannot drift.

##### Task 4.2.3c: Action end-to-end test with matrix report and failure semantics (S)
- Files: `test/action.test.js` (test only)
- Uses the wave-14 wasm (re-run against the final wasm by 5.1.2a); asserts outputs, comment body, and exit/failure behaviour as above. Also: one data-error case (*Given* the `heap` variant file empty, *Then* the comment is still posted with the drop notice and the job passes, per 4.2.3b), and one `config`-input case through the real wasm: *Given* a workspace `benchdiff.config.json` `{"version":1,"threshold":3}` passed via the `config` input, *When* `main()` runs, *Then* the staged `/work/config/benchdiff.config.json` is read by the wasm binary through the WASI preopen and the effective threshold is 3 (asserted via `--print-config` run through the same wasm, or the report's method note listing `threshold=3 (config)`); *Given* additionally `threshold: 7` as an input, *Then* the effective value is 7 (flags over file). This is the first test of the `--config` path under WASI, since 1.2.4a ran against the committed pre-config wasm. The wave-14 wasm already contains the `--config` code (it is built from the merged tree after W13).

---

# PHASE 5: Docs and release

### Story 5.1.1: README and docs
**Acceptance Criteria**:
- *Given* README, *When* read, *Then* it has one table mapping each concept to input / flag / config key (identical kebab-case names), the precedence rule `flags > inputs > file > defaults` (inputs-as-flags collapse in the binary), the `variants` grammar (first `=` splits, label charset, one file per label, newline-only, not combinable with `head`/`base`), the conventional config name `benchdiff.config.json` and that it is never auto-discovered, the note that `only-changed`/`fail-on-regression` from a file cannot be turned off by an input, the CLI `--svg-embed` dangling-link caveat, the embed/publish defaults (Action: non-empty `charts` implies embed and publish unless `svg-embed: false`/`publish-images: false`; CLI: off), the vocabulary label set (`default|config|flag`) used by the Method note and `--print-config`, the support matrix (Success metric 3 section) and the fallback ladder, that Holm is per-row only with the recommendation to pair N-way `fail-on-regression` with `threshold` and `min-abs`, the reference choice and the common-row-set headline rule, the warn-by-default data-error behaviour and `dimensions-strict`, that the table is the primary view on phones (AC-S2-8: chart text scales to about 7 px at 375 px width), that rollback is unsetting `charts` and never deleting `benchdiff-assets`, the standalone-SVG story (files are the standalone artifact; no HTML wrapper in v1), the `contents: write` requirement for hosted images, the private/fork fallback, the artifact `archive:false` snippet, and the Holm and reference policies.
- Triad round 1 additions, each asserted by `test/docs.test.js`: (1) the N-way regression definition and gate (a cell is `Regressed` only versus the reference; `fail-on-regression` in an N-way run needs an explicit `reference`; Action outputs `regressed`/`improved`/... mean cells vs the reference there; no gate means `failed` is false); (2) a section "Hosting images" with the side-effect disclosure: `contents: write`, the `benchdiff-assets` branch and one commit per publishing run with new bytes, retention (never pruned; deleting the branch breaks old comments), public-URL visibility, and the private-repository rule (never hosted, nothing written, reason `private`); (3) vocabulary: *variant* is the canonical term, `--variant` is the repeatable singular flag and `variants` the multi-line Action input, explained once in a "Naming" note and used consistently; (4) zero-config default dimension rules (`/key=value` segments and `[k=v]`/`(k = v)` groups) with `--print-dimensions` as the way to see what was detected, and the `available dimensions: ...` error suffix; (5) `cell-format: ratio` for phones; (6) the ratio-vs-percent explanation (N-way cells are ratios vs the reference, `0.74x` = 26% lower value; two-way keeps percent); (7) GitHub shows charts as an opaque `<img>`, so alt text and the `Chart data (text)` table are the accessible path; (8) the `chart-rows`/`max-rows` relationship; (9) the notice order and 5-line cap; (10) the config `$schema` line, the published schema and the tested starter example with the note that regex backslashes are written doubled in JSON (`\\w`); (11) the default reference when variants come from benchmark names (first encountered) and the advice to set `reference` explicitly.
- Triad round 2 additions, each asserted by `test/docs.test.js`: (12) bold, Rank and the headline's "best geomean" are untested orderings, only each variant against the reference is tested, and there is no head-to-head claim between variants; (13) the hosting-failed line names the directory (not an artifact) and hints `actions/upload-artifact`, and the first-run note: with a read-only default `GITHUB_TOKEN`, `charts: delta` alone often shows the permission warning, fixed by the `permissions:` snippet (`contents: write`, `pull-requests: write`) in "Hosting images"; (14) chart rows are the top N by |delta| and the table follows `sort`, matched by name and by the `#` column of the text block; (15) forced-colors and print show the chart as drawn (opaque card, shapes, printed values) and the text table is the accessible path, and the 3:1 non-text-contrast rule with the outline for light series 3-5; (16) the gate line truncation rule and where the full list is; (17) the adoption proxy is a manual count (no telemetry), see requirements.md.
**Files**: `README.md`, `test/docs.test.js`, `docs/config.md` (only if README exceeds scope)

##### Task 5.1.1a: Write docs (S)
- Files: `README.md`, `test/docs.test.js`, `docs/config.md` (only if README exceeds scope)

### Story 5.1.2: Rebuild committed wasm
**Acceptance Criteria**:
- *Given* the final Rust tree, *When* `make wasm` runs, *Then* `dist/benchdiff.wasm` is updated, size within the budget in `scripts/check-wasm-size.sh` (now 820,000 bytes, ADR-004 'Budget note'; originally 480,000 bytes) (script passes), `make golden` is green, the `wasm-fresh` job (0.1.2a) passes, and the 4.2.3c end-to-end test is re-run against it.
**Files**: `dist/benchdiff.wasm`

##### Task 5.1.2a: `make wasm`, size gate, commit (XS)
- Files: `dist/benchdiff.wasm`

### Story 5.1.3: Real-PR end-to-end and release
**Acceptance Criteria**:
- *Given* a same-repo PR on this repository using `charts: delta,ranking` (no other chart setting; the Action default embeds), *When* the workflow completes, *Then* the owner confirms images render in the sticky comment and job summary, ADR-003 is `Final` with observed results, the 8-item checklist from research/pitfalls.md section 9 is ticked or explicitly waived, and a minor version tag is cut before `@v0` moves.
- Support matrix (see "Success metric 3 as a support matrix"): *Given* the six rows of that table, *When* the owner runs the listed workflow variants (A same-repo public, B without `contents: write`, C fork PR, D private repo, E `workflow_dispatch`; GHES by unit test), *Then* each row's observed output is recorded in ADR-003 next to the expected output, and any row not run is marked `waived: <reason>`; the in-body notice is seen in the comment/summary for B-D.
- Comprehension and time-to-signal checks (manual, validation.md MAN-6 and MAN-7): *Given* the `testdata/nway/expected/matrix.md` comment and one two-way comment shown to at least 3 readers (Tyler plus two others, or an explicit waiver recorded in ADR-003), *When* each is asked "which variant is slower than the reference and by how much?" and the three outcome-metric questions of requirements.md (which variant has the best geomean and against which reference, is the gap on one named row significant, is any variant slower than the reference), *Then* the answers and the seconds taken are recorded in ADR-003 next to the baseline recorded at gate G0 (task 0.3.1b asked the same questions of today's pairwise output, so the comparison needs no new baseline run); a ratio misread by two or more readers reopens the ratio-vs-percent decision in ux.md.
- Layout review: *Given* the PNGs from `scripts/svg-to-png.sh` for the worst-case set at 760 px and 320 px, *Then* the owner records pass/fail per chart kind in ADR-003; also record the 375 px phone view (AC-S2-8).
**Files**: `.github/workflows/ci.yml`, `decisions/ADR-003-svg-hosting-orphan-branch.md`

##### Task 5.1.3a: E2E run, ADR finalize, tag (S, wall-clock blocker: Tyler)
- Files: `.github/workflows/ci.yml`, `project_plans/benchdiff-dimensions-svg/decisions/ADR-003-svg-hosting-orphan-branch.md`

### Story 5.1.4: Release-benchmarks workflow coverage
**As a** maintainer using the reusable release workflow, **I want** charts attached to the release, **so that** release notes and READMEs can link standalone SVGs (requirements: standalone artifacts, `release-benchmarks.yml`).
**Acceptance Criteria**:
- New optional inputs `charts` (default empty = unchanged behaviour) and `config` on `.github/workflows/release-benchmarks.yml`, passed to the Diff step.
  - *Given* neither input set, *When* the workflow's step list is compared with today's, *Then* the Diff step's `with:` block is unchanged apart from the two empty pass-throughs (workflow lint/`actionlint` green).
  - *Given* `charts: delta`, *When* it runs, *Then* the `Attach to the release` step uploads each path in the Diff step's `svg-paths` output with `gh release upload --clobber` next to `benchmarks.json`, and the release workflow never sets `publish-images` (release notes link the asset URLs; hosting is for PR comments).
**Files**: `.github/workflows/release-benchmarks.yml`

##### Task 5.1.4a: Wire `charts`/`config` and attach SVGs (S)
- Files: `.github/workflows/release-benchmarks.yml`
- Depends on 4.2.1b (the `svg-paths` output is set by `main()`). Verified with `actionlint` and a dry-run review; live proof happens on the next real release (not a blocker for 5.1.3).

### Story 5.1.5: Config JSON schema and tested starter example
**As a** new user, **I want** editor validation and a copy-pasteable config that is known to work, **so that** the first config I write does not fail on a typo or a mis-escaped regex.
**Acceptance Criteria**:
- Schema generated from the one key table, not hand-maintained.
  - *Given* `config::key_table()` (public, 1.2.2a/2.2.2a/3.3.1a), *When* `config_schema.rs` builds a JSON Schema (draft-07) from it, *Then* the result equals the committed `benchdiff.config.schema.json` byte for byte (`UPDATE_GOLDEN=1` rewrites it), every key of the table appears with its enum values and type, `additionalProperties` is `false`, and `$schema` and `version` are present (`version` required).
  - *Given* a key added to the table without regenerating the file, *Then* the test fails naming the key.
- The starter example works.
  - *Given* `examples/benchdiff.config.json` (starts with `$schema` pointing at the schema URL, then `version`, `reference`, `compare-by`, `dimensions`, `charts`, `thresholds`; the `dimensions` rule is written with doubled backslashes, `"impl=/impl=(?P<impl>\\w+)"`, and a `"//"`-style comment is deliberately absent because JSON has none), *When* `config_schema.rs` parses it with `parse_config` and then runs the binary with `--config examples/benchdiff.config.json --svg-dir <tmp>` on `testdata/nway/impl.txt` (the single-file `impl=` form from 2.2.3), *Then* the exit code is 0, the report is the matrix, and `--print-config` lists every example key with source `config`.
  - *Given* the same example with the regex written with a single backslash (`\w` inside the JSON string, an invalid JSON escape), *Then* `parse_config` fails with file, line and column, proving the doubled-backslash note in the README is load-bearing.
- The example validates against the schema (a minimal in-test validator over `type`, `enum`, `required`, `additionalProperties`; no new dependency).
**Files**: `benchdiff.config.schema.json`, `examples/benchdiff.config.json`, `crates/benchdiff/tests/config_schema.rs`

##### Task 5.1.5a: Generate and commit the schema; add and test the starter example (S)
- Files: `benchdiff.config.schema.json`, `examples/benchdiff.config.json`, `crates/benchdiff/tests/config_schema.rs`
- Depends on the complete key table (3.3.1a, 2.2.2a) and the fixture (2.2.3a). The README (5.1.1a, W19) links both files; the schema URL pins the `v0` tag.
- Test approach: `cargo test -p benchdiff --test config_schema`.

## Review Resolutions (architecture + adversarial review, 2026-10-31)

| Finding | Resolution | Where |
|---|---|---|
| ARCH-1 per-metric thresholds unused | Wired in: Refactor-first Story 1.2.5 (`Thresholds`, `threshold_for`), golden-protected, before the seam | 1.2.5a, Tech Debt, glossary |
| ADV-1 config auto-detect collision | Auto-detect dropped; explicit `config` only; `version` required; conventional name `benchdiff.config.json` | 1.2.2a, 1.2.4, Risk Control, ADR-001 |
| ADV-2 Action N-way input | Separate `variants` input; ADR-007; ACs and e2e test | 4.2.3, ADR-007 |
| ADV-3 distribution data source | `#[serde(skip)]` `Samples` side channel, `keep_samples` | 1.2.5b, 2.1.3a, 3.2.3, ADR-006 |
| Reference direction vs `changed_row` | Kernel unchanged; mismatch short-circuits to `DirectionMismatch` before the kernel; no clone or overwrite (round-1 wording "normalises on the cloned input" corrected in round 3); explicit ACs | 2.1.1, 2.1.3, ADR-005 |
| Holm shape and family m | Downgrade-only post-pass; m = cells with a Mann-Whitney p-value | 2.1.4, ADR-002 |
| Residual home | Construct-only `Dimensioned` holding `Vec<(Metric, Residual)>` (superseded `Dimensioned { set, residuals }`, corrected in round 3 per the glossary) | 1.1.2c, glossary, ADR-004 |
| Go `procs` never present | `procs` dropped from the default extractor; `-N` untouched | 1.1.2a, ADR-004 |
| `render_markdown` API break | `render_markdown_with` + wrapper | 3.3.1 |
| Cell model flags | `Cell`/`Comparison` sums | glossary, 2.1.3 |
| Row identity | `(group, residual, measure, non-axis dims)`; labels `Sort [size=10]`; two-dim AC | glossary, 2.1.2, ADR-004 |
| Matrix Invalid/skipped | `Comparison::Invalid`, rendered, trips `matrix_failed` | 2.1.3, 2.1.6, 2.2.1 |
| Companion Report / `failed` | Top-level `counts`/`failed`/`rows: []` defined for matrix runs | 2.1.6, ADR-006 |
| Geomean mixing measures | Per `(measure, unit)` family; headline uses primary time family | 2.1.5, 2.2.1, ADR-005 |
| Chart kind vs report kind | Compatibility table, `NotApplicable` | 3.2.4 |
| `chart-theme: both` embed | `<picture>` placeholder, rewrite, strip | 3.3.3 |
| Standalone HTML | Explicit scope cut in requirements.md; SVG files are the standalone artifact | requirements.md |
| `release-benchmarks.yml` | Story 5.1.4 | 5.1.4 |
| `group`/`measure` axis | `group` added as pseudo-dimension; `measure` cut (units) in requirements.md | 2.1.2, requirements.md |
| Performance verification | Story 3.2.6 | 3.2.6 |
| Wave/file conflicts, wasm owner | Wave table + file ownership table; `Options` stays in `compare.rs`; wasm only by integration tasks | Critical path, Migration Plan |
| Publisher robustness | Backoff, create-race, hashed tag path, host-aware base URL | 4.1.2, ADR-003 |
| `label=path` ambiguity | First-`=` split, label charset, before glob expansion | 2.2.2, 4.2.3, ADR-007 |
| Minors: glossary count, `Verdict::Invalid`, projection naming, rule delimiting, size gate path, CLI placeholders, spike cleanup, status line | Fixed inline | glossary, 1.1.2b, 0.1.2, 3.3.1, 0.2.1b, header |

Left open deliberately: churn-weighted hotspot analysis (optional, listed in Unresolved Questions); importer-level dims and Go `procs` (follow-up); BH/FDR across rows (ADR-002 limitation).

## Review Resolutions (round 2)

| Finding | Resolution | Where |
|---|---|---|
| ARCH `Samples` unit scale / `one_sided_row` | `Samples.base` stored pre-scaled to the head unit; unit-converted-pair AC; `one_sided_row` leaves `Samples` empty | glossary, 1.2.5, 1.2.5b, 2.1.3 |
| ARCH threshold key vs projection | Matrix lookup keyed `group/residual` (reference's group under `compare-by=group`); matrix AC; bad-key diagnostic | glossary, 2.1.3 |
| ARCH vacuous reference-direction AC | Mismatch short-circuits before the kernel (asserted); clone/overwrite dropped; ranking orientation AC | Pattern Decisions, 2.1.1, 2.1.3, ADR-005 |
| ARCH `Dimensioned` invariant / labelled path | Construct-only `Vec<(Metric, Residual)>`; identity residuals for labelled files; per-file `apply` when rules given | glossary, 1.1.2c, 2.1.2 |
| ARCH Holm text and ordering | Method text derived at render time; counts/geomean/`matrix_failed` after the pass | glossary, 2.1.4, 2.1.6 |
| ARCH nitpicks | `PartialEq` clause removed; 0.1.1b arrow fixed; overrides in method note only when non-empty | 1.2.5, diagram, Tech Debt |
| ADV C1 W2 file conflicts | New XS task 1.1.1b (W1) owns stubs and `lib.rs`; ownership table completed (`svg/*`, `test/images.test.js`, Cargo, `ci.yml`/`Makefile`); `lib.rs` removed from story Files lists | Waves, ownership, 1.1.x/1.2.2/2.1.3 |
| ADV C2 Story 1.2.2 later-wave ACs | 1.2.2 limited to core keys; matrix-key ACs moved to 2.2.2a, chart-key ACs to 3.3.1a | 1.2.2, 2.2.2, 3.3.1 |
| ADV C3 `Comparison::Invalid` source | Defined from variant `skipped` lists (axis names extracted via `Dimensioned`); beyond cap = `Missing`; AC fixed to all-NaN series | glossary, 1.1.2c, 2.1.3 |
| ADV C4 `variants` in `main()`/`buildArgs` | Empty-head guard, no `--head-json`/empty `head-json-path`, `resolveOne`, format rules, full companion `counts` mapping | 4.2.3, 2.1.6, 2.2.2 |
| ADV C5 task 0.2.1b unscheduled | Gate G1 row in wave table; which waves wait documented | Waves, 0.2.1b |
| ADV C6 cross-reference drift | ADR-005 default reference, ADR-004 `dimensions-strict`, ADR-001 key list fixed | ADR-001, 004, 005 |
| ADV C7 no real-wasm `config` test | `config` input case (file 3, input 7) added to 4.2.3b | 4.2.3b |
| ADV minors | 0.1.1b diagram; axis-less metrics excluded and diagnosed; `sort=rank` error on two-way and `MatrixSort`; `cargo test --release` via `make perf`; Action-default title/tag for expected file; literal `<picture>` in 4.1.3a; `GITHUB_EVENT_PATH` source and non-PR behaviour | 2.1.2, 2.2.2, 2.1.3a, 3.2.6, 2.2.3a, 4.1.3, 4.1.2 |

## Review Resolutions (round 3 / phase 4)

Sources: `implementation/consistency.md` (B = blocker, C = concern, N = nitpick), `implementation/pre-mortem.md` (PM-n), `implementation/validation.md` gaps (G-n). The three stale-source documents are left as the record of what was found; this table is the record of what changed.

| Finding | Resolution | Where |
|---|---|---|
| B1 missing-cell glyph `-` vs `—` | `—` (U+2014) in 2.1.3, 2.2.1, 2.2.1a; snapshot AC asserts Missing `—` and Invalid `⚠️ invalid` | 2.1.3, 2.2.1, 2.2.1a |
| B2 `charts: delta` alone vs `svg-embed`/`publish-images` | Action: non-empty `charts` implies embed and publish unless `svg-embed: false` / `publish-images: false`; `svg-embed` accepts `true|false|auto` (`auto` = default-layer value, Action passes it when unset); `publish-images` default `true`, consulted only when embedding; CLI default stays off; `publish-images` is a flag, a config key and an input (one vocabulary); ACs incl. AC-S5-4 | glossary `SvgOptions`, 3.3.1, 4.2.1, ADR-003 item 1, ux.md S4 |
| B3 per-group thresholds / `format` keys | Group-level lookup `group/name` > `group` > default in the kernel and the matrix, with `threshold_for_should_prefer_exact_over_group_over_default` and a table test; `format` and `base-format` added to the core key table; output paths (`--markdown`, `--json`) are an explicit scope cut in requirements.md | glossary `Thresholds`, 1.2.2a, 1.2.5, 2.1.3, requirements.md |
| C1 / G-1 collapsed "Chart data (text)" block | AC in 3.3.2 (content, cap line, open on fallback, absent when no chart); built by `chart_text_table` (3.3.2a), inserted by 3.3.1c in both Markdown modules | 3.3.1, 3.3.2, 4.1.1 |
| C2 stale metric listing `measure` | requirements.md metric 2 now says "implementation, a param, or group" | requirements.md |
| C3 headline wording | One wording (plan 2.2.1), ux.md wireframe and AC-S1-2 aligned; reference stated on the next line | 2.2.1, ux.md S1 |
| C4 reference error text | `unknown reference 'nope'; valid: quick, heap, merge` everywhere | 2.1.2, 2.2.2, ux.md S3 |
| C5 / G-6 `--print-config` and source labels | JSON object with `config-file`, `config`, `sources`; labels exactly `default|config|flag`; Action inputs are flags to the binary, so inputs and flags share `flag` (ADR-001 item 2); ux.md AC-S4-2 and S3 aligned | 1.2.3, 1.2.3a, ux.md S3/S5, AC-S4-2 |
| C6 config filename and staging | `benchdiff.config.json` in all examples; Story 1.2.2 retitled; the Action stages it as `/work/config/<its own base name>` so error text names the real file | 1.2.2, 1.2.4, 4.2.3b, ADR-001 |
| C7 `publish-images` vocabulary | Flag + key + input (see B2); added to ux.md shared vocabulary | 3.3.1, ux.md |
| C8 `Dimensioned` shape | ADR-004 item 5 amended; Review Resolutions row corrected | ADR-004, this file |
| C9 scope drift | `--print-config`, wasm size gate and `columns_cap` get one-line justifications as supporting work | requirements.md |
| C10 ADR-001 key list | Missing keys added | ADR-001 |
| C11 hosting reason enum | `no-event` (and the fixed error strings per reason) added | ADR-003 item 5, 4.1.1 |
| N2 / G-9 phone legibility | README sentence in the 5.1.1 AC | 5.1.1 |
| N3 / G-2 unwritable `--svg-dir` | Error path AC (report first, then error, exit 2; warning only inside the Action) | 3.3.1 |
| N4 stale "Reference direction" row | Corrected | this file |
| N5 ux vocabulary list | Completed | ux.md |
| N7 ux S3 coverage count | Set to the recounted 19 rows | ux.md |
| N8 six shapes for eight series | Stated in 3.1.3a and ux.md | 3.1.3a, ux.md S2 |
| N10 / G-11 fork wording | Fixed error string per reason (`read-only token on a fork PR`) | 4.1.1 |
| PM-1a spike matrix | 0.2.1a now seven embed variants incl. `data:` URI and repo-path, Camo `Content-Type`/`nosniff` check, and three runs (public, private, fork) | 0.2.1, 0.2.1a, 0.2.1b |
| PM-1b/c fallback ladder, support matrix | Ladder (with a rung that still shows the chart for private/fork) in ADR-003; success metric 3 restated as a per-repo-class support matrix verified in 5.1.3a | Support-matrix section, ADR-003, 5.1.3a |
| PM-1d notice in the body | In-Markdown warning line is part of the strip step and the AC | 4.1.1, ADR-003 item 4 |
| PM-1e / notes retention and rollback | No delete/force-push, no-op on identical content, history preserved, documented growth bound; rollback advice fixed | 4.1.2, 4.1.2c, Risk Control |
| PM-2a misleading N-way aggregates | Headline geomean over the common row set, per-variant `n_used/n_total`, `not comparable` rule, fixture with a variant missing its slowest rows | glossary `Geomean`, 2.1.5, 2.2.1, ADR-005 item 5 |
| PM-2b reference order | Reference stated in the headline; permutation AC with explicit `reference` | 2.1.3, 2.2.1 |
| PM-2c Holm per-row | AC, README/Method-note statement, threshold/min-abs recommendation, seeded 300-identical-row test (guards via the threshold floor, stated honestly) | 2.1.4, 5.1.1, ADR-002 |
| PM-3 data errors vs config errors | Two-class failure policy; warn by default, `dimensions-strict` fails; degradation ladder; ACs in 1.1.2, 1.1.2c, 2.1.2, 2.2.2, 4.2.3, e2e case in 4.2.3b | Risk Control, ADR-004 item 6, ADR-007 |
| PM-4 layout correctness | Geometric no-overflow/no-overlap assertions, rasterize script, PNGs required with golden updates, worst-case set reviewed in 5.1.3a | 3.2.5, 5.1.3 |
| PM-5 default/artifact drift | Golden per importer format (8) plus an Action-level golden; binary-defaults == old `action.yml` defaults table test; `wasm-fresh` CI job; reproducibility resolved in W1; 4.2.3b re-run after the final wasm; size re-measured after 1.1.2b and 3.3.1b | 0.1.1, 0.1.2, 1.2.4, 5.1.2 |
| G-3 AC-X-3 | Two-run sticky-comment AC | 4.2.1 |
| G-4 parse error location | File, line, column AC; `parse_config` takes the file name | 1.2.2 |
| G-5 `--help` | AC in 2.2.2 and 3.3.1, generated from the key table | 2.2.2, 3.3.1 |
| G-7 contrast | Computed contrast test (ink 19.17:1, secondary 7.73:1 on the light card, verified this round) | 3.1.3 |
| G-8 vocabulary sameness | `vocabulary.rs` test with explicit allowlists | 4.2.1 |
| G-10, G-12 report lines, prose scan | ACs for `· N incomplete`, reference-absent, cap, no-samples lines and the higher/lower scan | 2.2.1 |
| G-13 core I/O-free, zero-dep Action | `purity.rs` (0.1.2a) and the `package.json`/`require` guard (0.1.3a) | 0.1.2, 0.1.3 |
| G-14 egress | Host-assertion AC | 4.1.2 |
| G-16 box plot | Cut from ux.md (distribution is a strip plot at every n), recorded there | ux.md S2 |
| G-15 manual rendering evidence | Unchanged: SM-3 stays manual behind gate G1 and the 5.1.3a matrix | validation.md |

Left open deliberately: the actual GitHub rendering results (gate G1), `contents: write` wording (Tyler), the optional hotspot analysis, importer-level dimensions (follow-up), BH/FDR across rows (ADR-002 limitation), pruning by squash for `benchdiff-assets` (documented as breaking old comments, out of scope).

## Review Resolutions (triad round 1)

Source: product / UX / engineering triad review (verdict NEEDS WORK, 2026-10-31). Option chosen for the blocker: define N-way regression explicitly (not reject the flag), because the matrix already compares every cell with the reference and the Action's `regressed`/`failed` outputs need a meaning for matrix runs.

| Finding | Resolution | Where |
|---|---|---|
| UX BLOCKER: "regression" undefined for N-way | Defined: the reference is the declared baseline; `Regressed` = significantly worse than the reference (same kernel and Holm); wording "slower/faster than reference". `fail-on-regression` is the gate and requires an **explicit** `reference` (static exit 2 otherwise); no flag means `failed` false; gate line in the report; Action outputs count cells vs the reference. Aligned in AC-S1-2/S1-8/S1-12, S4, plan, ADR-005 item 7, ADR-006 item 3, ADR-007, validation.md and requirements.md | glossary (`Verdict`, `RegressionGate`, `Reference`), Pattern Decisions, Risk Control, 2.1.3, 2.1.6, 2.2.1, 2.2.2, 4.2.1, 4.2.3, 5.1.1, ADR-005/006/007, ux.md, validation.md |
| UX: notice stacking/ordering/cap | Fixed class order, 5 inline, `⚠️ N more notices (see Method)`, full list in Method; always after the verdict; notice-only report has no verdict (AC-S1-2 scoped); existing skipped line exempt | 2.2.1, 2.2.1a, 3.3.1 (c), ux.md S1, AC-S1-11, glossary `Notice` |
| UX: one-line legend under the table | Legend directly under the table (moved from the Method note) | 2.2.1, ux.md wireframe, AC-S1-10 |
| UX: unknown-axis errors list dimensions; zero-config rules; way to see them | `available dimensions: ...` suffix on every axis error; rules stated in `--help`, README, `action.yml`, ADR-004 item 9; `--print-dimensions`; `Dimensioned::dimension_summary` | 1.1.2c, 2.1.2, 2.2.2, 2.2.2a, 5.1.1, ADR-004, ux.md S3, AC-S3-9, glossary `DimensionSummary` |
| UX: compact ratio-only option | `cell-format` (`full` or `ratio`) as flag, key and input | 2.2.1, 2.2.2, 2.2.2a, 4.2.1, glossary `CellFormat`/`MatrixOptions`, ADR-001, ux.md, AC-S1-13 |
| UX: hosting side-effect disclosure | `action.yml` descriptions, README "Hosting images", one step-log line: `contents: write`, `benchdiff-assets` branch and per-run commit, retention, public URL, private repos never hosted | 4.2.1, 5.1.1, ADR-003, ux.md S4, AC-S4-5 |
| UX: glob matching 0 vs >1 files | One rule: 0 matches degrades with a warning (variant dropped), >1 fails; ux.md S3 row split; ADR-007 item 3 corrected | 4.2.3, ux.md S3, ADR-007 |
| UX: scope AC-X-2 vs AC-S3-8 | AC-X-2 limited to the Action and hosting; AC-S3-8 is the CLI exception | ux.md, Risk Control, 3.3.1 (b), validation.md |
| UX: `$schema`, JSON schema task, tested starter example | `$schema` accepted and ignored; new Story 5.1.5 / task 5.1.5a generates the schema from the key table and tests the example, with the doubled-backslash note | 1.2.2, 5.1.5, 5.1.5a, ADR-001, ux.md S5, AC-S5-5 |
| UX: vocabulary | *variant* canonical; singular repeatable `--variant` vs plural multi-line `variants` explained once | glossary (`Variant`, `VariantsInput`), 2.2.2, ux.md "Naming", 5.1.1 |
| UX: `chart-rows` vs `max-rows` and consistent "N more" | Effective cap `min(chart-rows, max-rows)`; chart rows subset of table rows; `N more` = population - shown; notices name the limiting key | glossary `SvgOptions`, 3.2.4, ux.md S1/S2, AC-S2-7 |
| UX: `<img>` exposes no role/title/desc on GitHub | Stated: alt text and the `Chart data (text)` table are the accessible path; AC-S2-5 reworded | 3.3.2, 5.1.1, ux.md S2, AC-S2-5 |
| UX: "annotation points at the input" | Reworded to "the message names the input" | ux.md S4 |
| UX: default reference when variants come from names | Deterministic: first encountered walking `MetricSet.metrics` (files in given order, globs sorted by path), ties by label; AC with permuted input | 2.1.2, glossary `Variant`, ADR-005 item 1 |
| UX: comprehension check for ratio vs percent | MAN-6 (>= 3 readers or waiver) in 5.1.3a; AC-S1-14; revisit trigger recorded | 5.1.3, validation.md, ux.md open items, Unresolved Questions |
| ENG: split task 3.3.1a | 3.3.1a (`SvgOptions`, overlay, config keys, flags; W6), 3.3.1c (`RenderExtras`, `render_markdown_with`, placeholders; W12), 3.3.1b (`render_svgs`, `--svg-dir` write; W13); ACs tagged by owner; ownership and waves updated; 3.2.1a moved to W7 so every chart task takes the one options type | 3.3.1, waves, file ownership, diagram |
| ENG: Files line on every task | Added to the 44 task bodies that lacked one (54 of 54 now have one), consistent with the ownership table (cross-checked by script) | all task bodies |
| ENG: 2.2.3a in a `then` sequence | Own slot W14, alone; 4.2.2a likewise shares its wave only with tasks that change no Rust source; later waves renumbered to W19 | waves, Migration Plan, 2.2.3a, 4.2.2a |
| PRODUCT: falsifiable problem, persona, demand evidence, outcome metric, roadmap fit, risky assumption | Added to requirements.md with VERIFIED / INFERRED / not-established labels; outcome metric verified by MAN-7; N-way ships before SVG hosting dependence and gate G1 precedes hosting | requirements.md, Dependency Visualization, Unresolved Questions, validation.md OM-1 |

Recounted after this round (script over this file): 54 tasks, 43 stories, 43 glossary terms, 54 effort-table tasks, 4,260k raw tokens, about 11,176k CU, 19 wave rows plus gate G1. Validation: 185 mapping rows + 46 UX rows = 231 tests, 32 of 32 requirements, 46 of 46 UX criteria.

Left open deliberately: MAN-6 and MAN-7 results (need readers), whether anyone beyond the owner's repos wants this (demand), `contents: write` wording sign-off (Tyler), real hosting results (gate G1), the importer-level dimensions follow-up, and BH/FDR across rows.

## Review Resolutions (triad round 2)

Source: product / UX / engineering triad round 2 (product READY, UX READY, engineering NEEDS WORK, 2026-10-31). The two earlier tables above record task IDs, wave numbers and counts as they were at the time (for example the old 1.1.2c, 4.2.3b, 54 tasks and 19 waves); this table and the sections above are current. Claims about the code were checked against it: `render_method` is `markdown.rs:329`; `Summary` is `compare.rs:130` with its single literal at `compare.rs:217`; `Metric { .. }` is a literal only in `Metric::new` (`model.rs:56`); `std::fs` appears in `#[cfg(test)]` modules at `import/mod.rs:137` and `import/json_formats.rs:425`; there is no `package.json`; `action.yml` inputs and `main.rs` flags are the ones listed in 4.2.1; `make golden` depends on `make wasm`, which overwrites `dist/benchdiff.wasm`; the two-way cap line in `markdown.rs` reads `_…and N more changed rows not shown (`max-rows: N`); the JSON report has them all._` (the old plan text quoted a different string).

| Finding | Resolution | Where |
|---|---|---|
| ENG blocker 1: method-note plumbing had no owner (1.2.5a W3 and 1.2.3a W5 both change `render_method` and need new `Summary` fields) | New task 1.2.1b (W2) owns the `Summary` fields (`threshold_overrides`, `config_sources`, both `skip_serializing_if` empty so default JSON is byte-identical) and the `render_method` sentences, with `compare.rs` and `markdown.rs` on its Files line; 1.2.5a and 1.2.3a only fill the fields; glossary `MethodFacts`; ownership rows for `compare.rs` and `markdown.rs` | 1.2.1, 1.2.1b, 1.2.3a, 1.2.5a, glossary, ownership, ADR-006 5e |
| ENG blocker 2: shared types without defining tasks | `Notice`/`NoticeClass`: new task 1.1.1c (W2, `notice.rs`); `CellFormat`, `Correction`, `MatrixSort`, `MatrixSummary`: 2.1.3a (W7, before every consumer); `SvgFile`, `SkipReason`, `TextRow`: 3.3.1a (W6); `SvgInput` and `select_rows`: 3.2.4a (W9, first need); glossary names the definer of each | glossary, 1.1.1c, 2.1.3a, 3.2.4a, 3.3.1a, ownership |
| ENG blocker 3: W12 signature race (3.3.1c changes `render_matrix_markdown`, 2.2.2a writes its call site) | Additive: `render_matrix_markdown(&MatrixReport)` is frozen by 2.2.1a; 3.3.1c adds `render_matrix_markdown_with` and makes the old function a wrapper; 2.2.2b (W12) calls the old one; 3.3.1b (W13) switches `main.rs` to the `_with` forms; waves re-checked by script for file conflicts (none) | 2.2.1a, 2.2.2b, 3.3.1b, 3.3.1c, waves |
| ENG: split 2.2.2a and 2.2.1a; re-examine 1.1.2c, 4.2.1a, 4.2.3a | 2.2.1a (table, W10) + 2.2.1b (notices and gate, W11); 2.2.2a (flags, keys, validation, W9) + 2.2.2b (wiring and ladder, W12); 1.1.2c (`Dimensioned` and summary in its own file `dimensioned.rs`, W4) + 1.1.2d (`apply`, W5); 4.2.1a (inputs, `buildArgs`, vocabulary, W15) + 4.2.1b (`main()` pipeline, W16); 4.2.3a (parse, stage, `buildArgs`, W17) + 4.2.3b (data-error handling, W18) + 4.2.3c (the old 4.2.3b e2e, W19). Others kept: each is at most 5 files and one concern | tasks above, waves |
| ENG: drop the `import/*.rs` edit in 1.1.1a | Dropped (verified: `Metric` is built only by `Metric::new`); 1.1.1a Files is `model.rs` and `dimensions_import.rs` | 1.1.1a |
| ENG: complete the ownership table | Added `model.rs`, `import/*`, `scripts/*.sh`, `release-benchmarks.yml`, `test/wasm_size.test.js`, `test/perf.test.js`, `perf.rs`, `nway_scale.rs`, `nway_fixture.rs`, `testdata/nway/*` (by file), `testdata/README.md` (now incl. 3.2.5a), `json_compat.rs`, `dimensions_import.rs`, `snapshots/svg/*`, `golden_expected.rs`, `ux_messages.test.js`, `dimensioned.rs`, `notice.rs`, `requirements.md` | ownership |
| ENG: `lib.rs` re-export contradiction | One rule: crate-root re-exports only by 2.1.6a, 3.3.1c and 3.3.1b; 3.3.1a and everything else use module paths; 1.1.1b text fixed | 1.1.1b, ownership |
| ENG: 3.2.4a chart-dispatch ACs | Moved to Story 3.3.1 (b) (the dispatcher `render_svgs` is created by 3.3.1b); 3.2.4a keeps row selection, caps, footers, `CapExceeded` | 3.2.4, 3.3.1 |
| ENG: how `max-rows`, `only-changed`, `title`, `tag` reach the matrix | `compare_matrix(variants, &Options, &MatrixOptions)` copies them into `MatrixSummary`; renderers and `render_svgs` read the report only; chart cap `min(chart-rows, summary.max_rows)`; matrix ACs for `max-rows`, `only-changed`, heading and marker | 2.1.3, 2.2.1 (a), 3.2.4 |
| ENG: 0.1.3a zero-dependency AC | No `package.json` exists: absence passes, a temp-dir fixture proves the failing branch, `require` scan of `index.js` and `lib/*.js` | 0.1.3a |
| ENG: 0.1.2a purity scan vs `#[cfg(test)]` | Scan removes `#[cfg(test)]` items by brace matching; ACs for the two real files and a planted production use; `tests/**` not scanned | 0.1.2a |
| ENG: 4.2.1a vocabulary allowlist | Rebuilt from the real files (`comment`, `github-token`, `variants` action-only; `--alpha`, `--no-stats`, `--markdown`, `--json`, `--head-json`, `--variant`, diagnostics CLI-only; `version`, `$schema`, `thresholds` config-only); stale `setup`, `bench-command`, `token` removed; test fails on a stale allowlisted name | 4.2.1 (a) |
| ENG: wasm headroom and re-measure points | Four re-measure points (after 1.1.2b, 3.3.1b, 2.2.3a, 4.2.2a), a 20,000-byte headroom trigger, named levers (`opt-level = "z"`, `wasm-opt -Oz`); plus the `make golden` overwrite caveat in the Migration Plan | 0.1.2a, Migration Plan |
| UX: hosting-failed text | Names `<svg-dir>` (a path, never an artifact name) and hints `actions/upload-artifact`; `permission` line also points to the README snippet | 4.1.1, ux.md S1, ADR-003 item 4 |
| UX: first-run `charts: delta` hits the permission warning | Documented in `action.yml`, README and ux.md S4 with the `permissions:` snippet (`contents: write`, `pull-requests: write`); labelled INFERRED for the default-token claim; AC and test | 4.2.1, 5.1.1 (13), ux.md S4, AC-S5-4 |
| UX: N-way `Chart data (text)` content | Defined: `# | Benchmark | ref | variants` ratio table from `select_rows`, `—`/`⚠️ invalid` cells, cap line, reference-absent count; distribution as `median (min-max, n)`; one block per chart | 3.3.2, ux.md S1 |
| UX: legend and "fastest overall" | Legend says bold and Rank are untested orderings (only variant vs reference is tested); headline now `has the best geomean ...; slower than reference in N of M rows` (defined; "loses in 0 rows" dropped); alt text without `lower`; requirements and ADR-005 aligned | 2.2.1 (a), ux.md S1, ADR-005, requirements.md |
| UX: alt text names verdict AND top change | `...: 1 regressed, 1 improved; top: BenchmarkSort +30.0%`; name truncated first; N-way form; tests | 3.3.2, 3.3.1, ux.md AC-S1-3 |
| UX: chart rows (top-N by \|delta\|) vs table order (`sort`) | Correlation aid: `1.`-style chart labels, `#` column in the text block, `largest change first` footer, shared `select_rows`; main table unchanged | 3.2.4, ux.md S1/S2, AC-S2-7 |
| UX: 3:1 non-text contrast; forced-colors/print | Computed: red 4.68, green 3.27 (passes, no palette change), gray fixed `#8a8a86` 3.38; light series 3-5 (2.74, 2.11, 2.62) get a `#52514e` outline; AC-S2-11 and AC-S2-12 (no `<style>`/`@media` in the Light card) | 3.1.3, ux.md S2, AC-S2-11/12 |
| UX: gate FAILED line truncation | First 3 cells, names cut to 40 characters, count always shown, line at most 200 characters, full list in Method | 2.2.1 (b), ux.md S1 |
| PRODUCT: falsifier wording | One threshold: at least 2 of 3 readers under 60 s, in requirements.md for baseline and treatment | requirements.md |
| PRODUCT: baseline half scheduled late | Task 0.3.1a (W1: fixture inputs, today's pairwise output, question sheet) and gate G0 (task 0.3.1b: readers, decision) before W6; W2-W5 do not wait | Epic 0.3, waves, diagram |
| PRODUCT: adoption proxy | Manual, telemetry-free count at release+30 and +90 days (OM-2, MAN-8); wall-clock blocker 5 | requirements.md, validation.md, Unresolved Questions |
| PRODUCT: requirements success metric 1 / scope text | "Chosen reference (the first input unless `reference` names another; fastest is only a ranking marker)", matching ADR-005; Q1 reworded to "best geomean" | requirements.md |

Recounted after this round (script over this file and validation.md): 63 tasks (every task body has a Files line; no two tasks of one wave share a file, checked by script), 44 stories, 46 glossary terms, 44 effort-table rows covering 63 tasks, 4,835k raw tokens, about 12,685k CU, 21 wave rows plus gates G0 and G1. Validation: 205 mapping rows + 48 UX rows = 253 tests, 34 of 34 requirements, 48 of 48 UX criteria.

Left open deliberately: gate G0 (baseline readers) and gate G1 (real hosting results), MAN-6, MAN-7, MAN-8 and MAN-9 results, `contents: write` wording sign-off (Tyler), the width-table source (3.1.2a), wasm-size headroom (the risk is named and measured at four points, not removed), the `only-changed` matrix wording `N rows with no significant difference hidden` (a new line proposed here for UX sign-off), whether the read-only default-token claim holds for the owner's repos (INFERRED), the importer-level dimensions follow-up and BH/FDR across rows.

