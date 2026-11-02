# Architecture research: benchdiff-dimensions-svg

Scope: how to add dimensions, N-way comparison, SVG rendering, config layering and image delivery without breaking the two-way path. Labels: VERIFIED = read in source this session; INFERRED = reasoning, not run.

## 0. Current shape (VERIFIED)

- `benchdiff-core` is pure (no fs/clock/net); deps are only `serde` and `serde_json` (`crates/benchdiff-core/Cargo.toml`). `lib.rs` exports `compare`, `render_markdown`, and builds the JSON report inline with `serde_json::json!` (`lib.rs:19-38`).
- Identity is `Metric::key() -> (group, name, measure)`, crate-private (`model.rs:69`). The same triple drives `MetricSet::validate` duplicate detection (`model.rs:217`), `compare()` joins (`compare.rs:178-195`), `Skipped::contains` and `sort_rows`.
- `Row` is public and serialized straight into the JSON report (`compare.rs:97`, `lib.rs:35`). It has `base`/`head` fields, so it is inherently two-sided.
- Pair logic is already isolated in two private fns: `changed_row(base, head, opts)` and `evidence(...)` (`compare.rs:266,297`). They take `&Metric`, not sets, so they are reusable for any pair.
- Size reality: `compare.rs` is 364 lines of production code plus 393 of tests (`#[cfg(test)]` at line 365); `markdown.rs` is 375 production plus 326 tests (line 376). The "758/702" figures are mostly tests.
- CLI (`main.rs`) is a hand-rolled arg parser mapping onto `Options`; all file IO is `std::fs` on paths given as args.
- `index.js` runs the wasm under `node:wasi` with exactly one preopen, `'/work' -> workDir` (a fresh temp dir) and `env: {}` (`index.js:43-51`). It copies every input into that dir (`stage()`), so the wasm never sees the repo, the workspace, or the environment. It always passes `--threshold`, `--min-abs`, `--sort` with JS-side defaults, and `action.yml` also declares defaults.
- `test/golden.test.js` asserts wasm output equals native output, plus a few content checks. It does NOT compare to committed expected files (only `testdata/golden/{base,head}.go.txt` inputs exist). So "byte-identical to today" is currently unguarded across commits.

## 1. Dimensions: `dims` map, not name parsing at identity level

Recommendation: add an optional, additive `dims: BTreeMap<String,String>` to `Metric`, serialized with `skip_serializing_if = "BTreeMap::is_empty"`, `#[serde(default)]`.

- Schema 1 compatibility (VERIFIED policy): the parser's own comment says "v1 only gains optional fields; unknown fields are ignored on read" (`model.rs:156`), and `Metric` has no `deny_unknown_fields`. An old reader therefore ignores `dims`; a new reader defaults it empty. No schema bump, no migration. Files without dims serialize byte-identically (so `head.json` round-trip and the `head_json_output_is_redetected...` test stay unchanged).
- Why not name parsing as the model: names are format-specific and ambiguous (Go `BenchmarkSort/impl=quick-8`, JMH `@Param` already arrive as JSON params, Criterion `group/function/param`). Parsing inside identity makes every join depend on a heuristic. Keep names as-is and treat dims as derived metadata.
- Where dims come from, in precedence order: (a) importer-provided (JMH params, Criterion function/value ids, Google Benchmark args: small additive change per importer, can be later); (b) a pure extraction pass `dimensions.rs`: `extract(&mut MetricSet, &Rules)` applying user rules to `name`/`group`; (c) the per-file label (section 2), set as the `impl` (or configured axis) dim by the loader. Extraction rules should be a small explicit grammar, not a regex dependency: `key=value` segments split on `/`, `,`, `[ ]`, plus a positional template such as `name_template = "{bench}/{impl}"`. Splitting on delimiter segments keeps the dependency list at serde only. (INFERRED: a `regex` crate would compile to wasm but adds size and an unneeded surface.)
- Identity generalization without touching `key()`: do NOT widen `key()`. Instead add a projection step that, given `axis` (which dim varies across columns) and `row_key` dims, builds `Vec<Series>` where the row identity is `(group, name-with-axis-segment-removed, measure, other-dims)` and the column is the axis value. Two-way mode never calls it. `validate()`'s duplicate check stays on the raw key, which is correct for a single file; labelled files are NOT merged (section 2), so they cannot collide.
- Skipped metrics: `SkippedKey` stays three strings; N-way adds nothing there in v1.

## 2. N-way: new sibling type, reuse the pair kernel

Recommendation: leave `compare()`, `Report`, `Row`, `render_markdown`, `render_json` untouched. Add:

- `compare.rs`: promote `changed_row` to `pub(crate) fn compare_pair(base: &Metric, head: &Metric, opts) -> Row` (rename only, or a thin wrapper; no logic change). This is the "seam".
- New `matrix.rs`:
  ```
  pub struct Variant { pub label: String, pub set: MetricSet }
  pub struct MatrixReport { summary, reference: String, columns: Vec<String>, rows: Vec<MatrixRow> }
  pub struct MatrixRow { group, name, measure, unit, better, cells: Vec<Cell> /* per column */, ranking: Vec<usize> }
  pub struct Cell { value: Option<f64>, vs_reference: Option<Row> /* reuses Row: delta_pct, verdict, method, p_value */ }
  pub fn compare_matrix(variants: &[Variant], reference: Reference, opts: &Options) -> MatrixReport
  ```
  Each cell versus the reference is a `compare_pair` call (reference as "base", the variant as "head"), so Mann-Whitney / interval / threshold precedence, unit conversion and `Incomparable` handling are inherited exactly. Two-way base/head is then the N=2 special case conceptually, but the code path is not rerouted, which is what keeps the golden output byte-identical.
- Statistics policy: compare each variant only to the reference (N-1 tests per benchmark, not all pairs). No multiple-comparison correction in v1; state it in the report's method note. Holm can be an opt-in later because `p_value` is already on every cell. (INFERRED recommendation; a stats-research track may refine.)
- Two ways to build `variants`, both through the same `compare_matrix`: (1) labelled files: CLI `--variant LABEL=PATH` (repeatable), each loaded as its own `MetricSet`, never merged; join across variants on `(group, name, measure)`; (2) one file with an `impl` dim: split the `MetricSet` by the axis dim via the projection in section 1 into one `Variant` per axis value.
- JSON: `render_json` already assembles via `json!`; add a top-level `"matrix"` key only when a `MatrixReport` is supplied. Absent otherwise, so default JSON is byte-identical. `failed`/`counts` keep their two-way meaning; add `matrix.counts` separately and decide separately whether `fail-on-regression` applies to the matrix (open question for the plan; default: it applies to cells vs. the reference).
- Markdown: new `markdown_matrix.rs` (or `render_matrix` in a new file) exposing `render_matrix_markdown(&MatrixReport) -> String`. Reuse `code`, `flatten`, `value`, `fmt_pct`, `fmt_num` by making them `pub(crate)` (they are currently private except `fmt_num`). Do not modify `render_groups`.

## 3. SVG renderer: new module in core, pure function

- Location: `crates/benchdiff-core/src/svg/` with `mod.rs` (public API), `text.rs` (width estimation, truncation), `style.rs` (CSS/palette), `delta.rs` (base-vs-head dumbbell/delta bars), `ranking.rs` (grouped bars for N-way). Public API:
  `pub fn render_svgs(input: SvgInput<'_>, opts: &SvgOptions) -> Vec<SvgFile>` where `SvgFile { name: String, svg: String, alt: String, skipped: Option<String> }`, with `SvgInput::Pair(&Report) | SvgInput::Matrix(&MatrixReport)`. Pure, no deps, no IO: a string builder. Fits the lib.rs promise ("no clock, no network, no filesystem").
- Determinism: fixed-precision number formatting (reuse a helper, never `{}` of f64 in coordinates), sorted iteration, no HashMap iteration in output (note `render_groups` uses a HashMap but orders via a separate Vec, so the pattern is already known to work).
- Text width without fonts: a static per-character advance table for a generic sans (e.g. 10 width classes), conservative +10%, truncate with an ellipsis and a `<title>` carrying the full text. Pin with snapshot tests.
- Theming: a `<style>` block with CSS variables and `@media (prefers-color-scheme: dark)`; `<title>`/`<desc>` and `role="img"`, `aria-labelledby`. Whether GitHub's `<img>`-rendered SVG honors `prefers-color-scheme` is INFERRED from common practice, not verified here; the design-QA step must verify in a real PR. Fall back to colors that read on both light and dark backgrounds with an opaque neutral panel if it does not.
- Security: every label goes through one `xml_escape` (and the existing `flatten` for control characters); no attribute is built from raw input. Add a payload test parallel to `untrusted_fields_cannot_inject_markup...`.
- Caps: `max_rows`/`max_series` in `SvgOptions`; a "+N more not shown" text node, and `skipped` reason string surfaced into the report and logs (matches the requirements' observability section).

## 4. Config layering and the WASM sandbox

Facts (VERIFIED): the wasm sees only `/work`; env is `{}`; it cannot read `benchdiff.toml` from the repo, resolve globs, or see `INPUT_*`. Therefore the Node shim must be the file reader, as it already is for inputs.

Recommendation:
1. Config file path comes from an Action input `config` (default: `benchdiff.json` if present at `GITHUB_WORKSPACE`; auto-detect optional). `index.js` copies it into `/work/config.<ext>` via the existing `stage()` and passes `--config /work/config.<ext>`. CLI native uses a real path, same flag.
2. Parsing lives in core (`config.rs`), pure `parse_config(text, format) -> Result<Config>`; `Config` fields are all `Option<...>` and map onto `Options`/`SvgOptions`/dimension rules.
3. Layering is applied in exactly one place, the CLI: `defaults < --config file < Action-input-derived flags < explicit flags`. Because the Action turns inputs into flags, "inputs over file" and "flags over inputs" collapse into "flags over file" in the binary. 
4. Gotcha to fix: `index.js` currently always passes `--threshold 5 --min-abs 0 --sort delta` and `action.yml` has defaults for them, so an input can never be "unset" and would always beat the file. Required change: for config-governed inputs, drop the `default:` in `action.yml` (document the default in `description`) and make `buildArgs` emit a flag only when the raw input is non-empty. CLI defaults in `Options::default()` already equal the old defaults (threshold 5, min-abs 0, sort delta), so default behavior and output are unchanged. (The `input()` fallbacks for title, tag, format are fine to keep only if no config key exists for them; simplest is the same treatment.) Add a unit test that `buildArgs` with all-empty inputs emits no tunable flags.
5. Format: JSON is the zero-cost option (serde_json already linked). TOML would need the `toml` crate (pure Rust; builds for wasm32-wasip1 but adds a dependency and binary size; INFERRED, not measured). Recommend JSON in v1 with the `Config` struct format-agnostic, so TOML is an additive follow-up via a tiny `ConfigFormat` switch on file extension. This answers the open question for the architecture track only; the owner/ADR decides.
6. Mapping config precedence to `Options`: introduce `OptionsOverlay` (all `Option` fields) with `apply(self, &mut Options)`; file overlay then flag overlay applied in order. This removes the current pattern where flags mutate `Options` directly in `parse_args` and avoids "flag equals default" ambiguity.

## 5. index.js delivery of SVGs

- The CLI writes SVGs to a directory (`--svg-dir /work/svg`); the wasm's single preopen already permits writing there (`std::fs::write` is used for the report today; creating a subdirectory with `create_dir_all` under a preopen is INFERRED to work under node:wasi, so create `svg/` from JS before the run to avoid relying on it).
- Core never knows URLs. The Markdown embeds a placeholder image reference, e.g. `![alt text](benchdiff-svg:NAME.svg)`, only when `--svg-dir` and `--svg-embed` are set. `index.js` then runs `publishImages(files, ctx) -> Promise<{name: url} | {error}>` and rewrites placeholders to hosted URLs before the job summary write and `upsertComment`. If publishing fails or is disabled, it strips the placeholder lines and emits `warning(...)`; the existing Unicode bar columns remain as the fallback. Pure string replace keeps `clip()` and the marker-first invariant intact (the marker is the first line and never touched).
- The hosting mechanism (release asset vs artifact vs branch) is another research track; the seam above makes it swappable: `publishImages` is the only function that changes. Keep it in a new `lib/images.js` required by `index.js`, with `module.exports` re-exports retained so `test/action.test.js` / `golden.test.js` imports (`runWasm, stage, expand, upsertComment, clip`) keep working. Still zero dependencies; `main: index.js` unchanged.
- New outputs (additive): `svg-dir`, `svg-paths`. Existing outputs and the sticky marker are unchanged.
- Golden shim: `golden.test.js` stages inputs and builds args by hand, so new cases there must add `--svg-dir` pairs and compare directories; native-vs-wasm determinism of SVG bytes is the same property the Markdown already has.

## 6. Hotspot disposition

**Isolate via seam (not Refactor-first, not plain Extend).** Production code in `compare.rs` (364 lines) and `markdown.rs` (375 lines) is moderate and well-factored; the 758/702 totals are mostly tests. The pair kernel (`changed_row`/`evidence`) is already a pure function of two `Metric`s, so the new work lands in new files (`matrix.rs`, `dimensions.rs`, `config.rs`, `svg/`, `markdown_matrix.rs`) behind a one-line visibility change (`compare_pair`, helper `pub(crate)`), leaving the two-way code path physically untouched. Churn evidence was not computed (no git-history hotspot run); if the plan wants a numeric hotspot score, run `code-hotspot-analysis` first.

## 7. Risks and prerequisites surfaced

1. Freeze goldens first: today's tests only prove wasm == native. Before any change, commit expected `report.md`/`report.json` outputs for the existing cases (generated from the current build) and assert against them, otherwise "byte-identical to today" is not testable. Do this as task zero.
2. `Row` carries `base`/`head` and is public JSON; do not overload it for N-way. Reusing it as `Cell.vs_reference` is fine because it appears only under the new `matrix` key.
3. `Metric::new` has six positional args and many struct-literal construction sites (importers, tests); adding `dims` means every literal needs `dims: Default::default()` or a `..` fill. Mechanical, but touches every importer and test. Prefer keeping `Metric::new` and adding `with_dims`.
4. Action input defaults removal (section 4.4) is the one change that edits default-path code; it must be covered by `buildArgs` tests and the golden suite.
5. WASM size and compile cost: avoid regex/toml unless measured; current deps are serde-only.
6. Event-Command-Policy table: skipped (single-actor pipeline, no multi-actor domain).
