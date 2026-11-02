# Architecture Review: benchdiff-dimensions-svg
**Date**: 2026-10-31 (re-review of the repaired plan; supersedes the BLOCKED review)
**Verdict**: CONCERNS

Method: re-read plan.md (978 lines, all of it), ADR-001..007, the prior review, and the review prompt. Checked against `crates/benchdiff-core/src/compare.rs` (Options at 33-51, Row at 98-116, `evidence` at 266-295, `changed_row` at 297-338), `model.rs` (Metric fields, `metrics: Vec<Metric>`), and a grep of all `Row {` and `Options {` literal sites. kibitzer is not configured (no `.claude/inspect.json`), so this is a code-read review, not tool-grounded. No `docs/adr/ADR-000-architecture-constitution.md`, so no constitution violations. Scope: only the previously BLOCKED item, the listed concerns, and the new tasks they introduced.

## Constitution Violations
None.

## Blockers
None. The prior blocker (per-metric `thresholds` parsed by 1.2.2a but consumed by nothing) is resolved:
- Story 1.2.5 / Task 1.2.5a adds `Options.threshold_overrides` and `Options::threshold_for(&Metric)`, replacing the single read at `compare.rs:329`. It is sequenced Refactor-first (W3), before the rename-only seam 2.1.1a (W5), and is listed in Tech Debt Disposition, glossary, Pattern Decisions and the file-ownership table.
- The default path is provably neutral: empty overrides give `threshold_pct`, and the AC requires the 0.1.1 goldens and the existing `compare.rs` tests to pass untouched.
- Wiring is end to end: 1.2.2a (W4) maps config keys to the overlay, and 1.2.3a asserts `--config` plus `--print-config`.
- Wave order is valid: 1.2.1a (W2, `OptionsOverlay`) -> 1.2.5a (W3) -> 1.2.2a (W4); `compare.rs` is serialized 1.2.5a, 1.2.5b, 2.1.1a, one owner per wave.
- `Options` derives only `Debug, Clone`, so adding a `BTreeMap` field is safe. All `Options { .. }` literals found (lib.rs:48,65; compare.rs tests; markdown.rs tests) need `..Default::default()`, which the task states.

## Concerns (5 open; 7 of the 9 prior concerns resolved)

New or residual, from the repaired tasks:
- [ ] Task 1.2.5b / Story 3.2.3 (`Samples` unit scale) — `evidence` scales base samples into the head unit on the fly (`compare.rs:276`, `base_scale`) and does not store them. If `changed_row` copies `base.samples` raw into `Row.samples`, the distribution chart plots base and head lanes on different scales whenever the units differ (for example ns vs us), the exact case `units::factor` exists for. — State that `Samples.base` is stored already multiplied by `scale`, add an AC with a unit-converted pair, and say whether `one_sided_row` (`compare.rs:238`) leaves `Samples` empty or fills only the present side.
- [ ] Task 1.2.5a vs 2.1.2a/2.1.3a (threshold key vs projection) — `threshold_overrides` are keyed `group/name`, and `threshold_for` is called on the metric handed to the kernel. In the matrix that metric is the projected clone with `name = residual` (and a different `group` under `compare-by=group`), so a user key written against the original name (`net/Sort/impl=quick`) never matches, which is the silent no-op the fix was meant to eliminate. — Define matrix lookups as keyed on `group/residual`, document it beside the config key, and add one matrix AC (override on `Sort` applies to every `impl` cell). Decide the `group` used for lookup under `compare-by=group` (reference's group).
- [ ] Story 2.1.1 / 2.1.3 / ADR-005 point 2 (reference-direction AC is vacuous) — The "clone with `better` overwritten from the reference after the mismatch check" design is sound for the kernel (`compare.rs:303,332` read `head.better`, so a pure rename stays byte-identical). But once the check passes, the two `better` values are equal, so the overwrite is a no-op, and the AC "a variant whose declared `better` differs only in an ignored field cannot flip the verdict orientation" has no testable scenario (a differing `better` is `DirectionMismatch`, never reaches the kernel). — Rewrite as: mismatch short-circuits before the kernel (assert `compare_pair` is not reached, cell is `DirectionMismatch`); keep the overwrite only as a documented defensive normalisation, or drop it and the clone cost; add an AC that `Row.better`/ranking orientation equals the reference's.
- [ ] Tasks 1.1.2c / 2.1.2a (`Dimensioned` invariant and labelled-file path) — (a) `residuals` is index-parallel to `set.metrics` with public fields; any filter or reorder desyncs identity silently. (b) The `label=path` variant path never says whether each file goes through `dimensions::apply`; without it, `Variant` for a labelled file has no residuals, and mixed use (`--variant` plus `--dimensions`) is undefined. — Make `Dimensioned` construct-only (private fields, accessor yielding `(&Metric, &Residual)` pairs) or store `Vec<(Metric, Residual)>`, and state that labelled files get identity residuals (`residual = name`) when no rules are given, and are otherwise applied per file.
- [ ] Task 2.1.4a (Holm representation) — Now correctly a downgrade-only post-pass over `method == MannWhitney`. Two loose ends: the human text `mann-whitney, Holm-adjusted p=0.06 (m=2)` is not representable in the existing `Method` enum (`compare.rs:89`), so say it is derived at render time from `p_adjusted` and `m`; and state that counts, geomean and `matrix_failed` are computed after the pass (ordering, since a downgrade changes failure status).

Carried over, still open (non-blocking):
- [ ] Hotspot evidence — line counts only; the optional `code-hotspot-analysis` run before Phase 5 is still listed as an unresolved question. Dispositions for `main.rs`/`index.js` look right.

Resolved since the last review (verified in plan text): Holm shape and family m (ADR-002, 2.1.4); residual home (`Dimensioned`); Go `-N`/`procs` (dropped from the extractor, matches `import/go.rs`); `render_markdown` API (`render_markdown_with` wrapper); `Cell`/`Comparison` sums; requirements gaps (scope cuts recorded at requirements.md:99-101); `Options` growth (`MatrixOptions`/`SvgOptions` separate); `label=path` grammar (first `=` split, charset, before glob).

## Nitpicks
- Glossary count 39 is correct (recounted: 39 term rows).
- 1.2.5 AC says `Samples` is "excluded from `PartialEq` in goldens"; `Row` does not derive `PartialEq` (`compare.rs:97`), so drop the clause.
- Dependency diagram draws 0.1.1b -> 1.1.1a, but both are W2; 1.1.1a only needs the 0.1.1a expected files (W1). Fix the arrow.
- `Dimensioned.residuals` newtype `Residual` and `PAdjusted` are good; `ChartRows`/`ChartWidth` clamped newtypes are now in the glossary.
- Per-metric overrides do not appear in `Summary` (`compare.rs:130`), so the method note should list active overrides only when non-empty to keep default bytes unchanged.

## What holds up (verified)
- Kernel seam: `changed_row` is a pure function of two `Metric`s and `Options`; rename to `compare_pair` plus the two golden-protected refactors keeps default bytes (`Row` is constructed at only two sites, `compare.rs:241,298`, so adding a `#[serde(skip)]` `Samples` field is cheap).
- `Samples` as a serde-skipped side channel keeps `SvgInput` a pure function of the report and default JSON byte-identical.
- Wave and file-ownership tables are consistent for the new tasks (1.2.5a W3, 1.2.5b W4, 2.1.1a W5; `options.rs` and `compare.rs` owners serialized).
- Core stays I/O-free; sibling `MatrixReport` leaves `compare()`/`render_json` untouched.
