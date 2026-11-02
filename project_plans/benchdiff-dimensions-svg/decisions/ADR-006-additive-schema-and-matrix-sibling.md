# ADR-006: Canonical JSON stays schema 1 (additive `dims`, new top-level `matrix`); N-way is a sibling type

**Status**: Accepted
**Date**: 2026-10-31
**Project**: benchdiff-dimensions-svg

## Context
`MetricSet::parse` ignores unknown fields and rejects `schema > SCHEMA_VERSION`; its comment says v1 only gains optional fields (`model.rs:150-170`). A bump to schema 2 would make older pinned `@v0` readers fail on baselines produced by newer runs. `Row` is public JSON with base/head fields.

## Decision
1. No schema bump. `dims` is optional and skipped when empty, so files without dims serialize byte-identically.
2. `compare()`, `Report`, `render_markdown`, `render_json` stay unchanged; `Row` gains only a `#[serde(skip)]` `samples` field (see 5a). `render_markdown_with(&Report, &RenderExtras)` is added and `render_markdown` becomes a thin wrapper, so its public signature never changes. N-way adds `matrix.rs` (`compare_matrix`, `MatrixReport`, `MatrixRow`, `Cell`), `markdown_matrix.rs`, and a top-level `"matrix"` key in report JSON **only when a `MatrixReport` is supplied**. `changed_row` is promoted to `pub(crate) compare_pair` (rename, no logic change); `code`, `flatten`, `value`, `fmt_pct`, `fmt_num` become `pub(crate)`.
3. `fail-on-regression` is the regression gate for matrix cells versus an **explicit** reference (ADR-005 item 7): a post-Holm Regressed cell over the threshold, or any Invalid cell, fails, consistent with two-way; `matrix_failed(&MatrixReport, gate: Option<f64>)` is false for `None`. **Companion fields (amended):** a matrix run has no two-way `Report`, so `render_json_with_matrix(&MatrixReport, pct)` emits `schema`, top-level `counts` (all keys the two-way JSON emits, mapped over every non-reference cell slot so Missing cells are counted as `removed`/`new`; see plan 2.1.6), computed after the Holm pass, `failed` (`matrix_failed`) and `rows: []` beside `matrix`; `index.js` keeps reading `report.counts.*` / `report.failed` unchanged and `matrix.counts` holds the per-variant breakdown.
4. Labelled variants (`label=path`) are loaded as separate `MetricSet`s and never merged, so the duplicate-key check cannot collide across labels.
5. SVG output is derived data only: `SvgInput::Pair(&Report) | SvgInput::Matrix(&MatrixReport)`; no change to either report type.

## Alternatives rejected
- Schema 2 (one-way trap for pinned readers), widening `Row` (breaks goldens/JSON), rerouting two-way through the matrix (puts golden bytes at risk).

## Amendments (post-review)
5a. **Chart samples (ADV-3):** `Row` and the matrix `PresentCell` carry a `#[serde(skip)]`, `Default` `Samples` field, filled only when the internal `Options.keep_samples` is set (chart requests set it). `SvgInput` stays `Pair(&Report) | Matrix(&MatrixReport)` and `render_svgs` needs no extra argument; default JSON bytes and memory are unchanged. Added by Refactor-first task 1.2.5b, golden-protected.
5b. **Kernel prerequisites:** per-metric thresholds (`Options::threshold_for`, task 1.2.5a) change the kernel's threshold read but default to the old behaviour; they land before the rename-only seam (2.1.1a).
5c. **Option structs:** `Options` stays in `compare.rs` for the pair kernel only; matrix and chart settings live in `MatrixOptions` and `SvgOptions` with their own overlays.
5d. **`Cell` model:** `Cell::{Missing, Present(PresentCell)}` with `Comparison::{Reference, Compared, Incomparable, DirectionMismatch, Invalid}`; no boolean flags.
5e. **Method-note plumbing and shared types (triad round 2):** the public `Summary` gains `threshold_overrides` and `config_sources`, both `skip_serializing_if` empty (so default JSON stays byte-identical), and `render_method` prints them only when non-empty; one task (1.2.1b, W2) owns both edits and 1.2.5a/1.2.3a only fill the fields. `compare_matrix(variants, &Options, &MatrixOptions)` copies the shared settings into a `MatrixSummary`, so `render_matrix_markdown(&MatrixReport)` and `render_svgs` read the report alone. `render_matrix_markdown(&MatrixReport)` is a frozen signature; `render_matrix_markdown_with(&MatrixReport, &RenderExtras)` is added beside it (additive, the old function becomes a wrapper), mirroring `render_markdown_with`.
