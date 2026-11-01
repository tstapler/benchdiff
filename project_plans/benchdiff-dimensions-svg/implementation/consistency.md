# Cross-artifact consistency: benchdiff-dimensions-svg

Artifacts checked: `requirements.md`, `implementation/plan.md`, `design/ux.md`, `decisions/ADR-001..007`.
Summary: 24 findings (3 blockers, 11 concerns, 10 nitpicks). Highest: the missing-cell glyph is `-` in the plan but `—` in ux.md and in the shipping `markdown.rs`.

## Verified consistent (commands run)

- Key and flag names `dimensions-strict`, `variants` / `--variant`, `chart-theme`, `svg-dir`, `svg-embed`, `chart-rows`, `chart-width`, `compare-by`, `group-by`, `reference`, `correction`, `config` match across plan, ux.md and ADR-001/004/007. `dimensions-strict` has no bare `strict` alias anywhere.
- Config format is JSON everywhere (ADR-001, plan 1.2.2, ux S5, requirements decision block). Conventional name `benchdiff.config.json` in ux, ADR-001 and plan, except for the drift in C6.
- Plan task count 51: `grep -c '^##### Task'` = 51, and the estimate table sums to 51 tasks, 3,760k raw, 9,865k CU (awk over the table). Story count 42 equals the 42 estimate rows. Glossary rows 39 equals the stated count. Every one of the 51 tasks appears in exactly one wave (diff of task ids against the wave table; 1.1.2c and 4.1.2c matched by hand in W5/W10).
- ux.md AC list is 39 items and S1 has 11 states, S2 has 7 (counts match its coverage table, except S3, see N8).
- Chart defaults agree: `charts` none, rows 30, width 760, opaque light card `#fcfcfb`, clamp +/-100%, alt text <= 125 chars, series cap 8, column cap 8.
- Holm policy wording agrees (ADR-002, ux open items, plan 2.2.1).
- Overrun checkpoint arithmetic: 9,865 x 1.25 = 12,331 (plan says about 12.3M). Correct.

## BLOCKERS

**B1. Missing-cell glyph: `-` vs `—`** (plan vs ux.md; also vs existing code)
- plan 2.1.3 ("rendered `-`"), 2.2.1 AC ("never `-`"), 2.2.1a ("`-` for Missing"), but plan 3.2.2 chart uses `—`.
- ux.md S1 rules ("`—` for missing cell"), AC-S1-7, S2 states ("`—` and hollow marker"). The shipping `crates/benchdiff-core/src/markdown.rs` uses `"—"` for absent values (lines 46, 170, 211, 260, 306).
- Fix: change the three plan sites to `—` (U+2014). Add a snapshot assertion that a Missing cell renders `—` and an Invalid cell renders `⚠️ invalid`.

**B2. `charts: delta` alone vs required `svg-embed` / undefined `publish-images`** (ux.md vs plan, ADR-003)
- ux.md AC-S5-4: "enable a useful chart with `charts: delta` alone"; S1 "Chart hosted OK" shows the image with no other setting; `svg-embed` and `publish-images` appear nowhere in ux.md.
- plan 3.3.1 / 4.2.1: the placeholder appears only with `--svg-embed`; the 4.2.1 AC needs `svg-embed: true`; ADR-003 item 1 runs publishing only when charts are requested and `svg-embed` is on. `publish-images` is listed as an Action input in 4.2.1 with no AC and no defined semantics.
- Fix: for the Action, make `charts` non-empty imply embed and publish unless `svg-embed: false` / `publish-images: false`, and state the defaults in 4.2.1 and action.yml. Keep the CLI default off. Otherwise reword AC-S5-4 and S1 to require `svg-embed: true`.

**B3. Requirement coverage gaps in the config scope** (requirements.md Scope vs plan)
- Requirements say config covers "thresholds per group/metric" and "output formats". The plan's `thresholds` keys are exact `group/name` only (glossary `Thresholds`; 1.2.5 lookup order exact > default), so there is no per-group threshold. The 1.2.2 key table has no `format` / `base-format`.
- Fix: either add group-level lookup (`group/name` > `group` > default, with a test in 1.2.5a), or record an explicit scope cut for per-group thresholds in requirements.md. Do the same for `format`: add it to the key table or cut it explicitly.

## CONCERNS

- **C1. Collapsed text table missing from plan** (ux AC-S1-3; wireframe `<details>Chart data (text)`). plan Story 3.3.2 is titled "Alt text and collapsed table" but its AC covers only alt text/`<desc>`. Add an AC and task scope to emit the `<details>` block with the chart's rows, in both `markdown.rs` and `markdown_matrix.rs`. (Rubric rates UX gaps NITPICK; raised because the story title promises it.)
- **C2. Stale success metric** (requirements.md line 21 vs line 100). The metric lists "measure" as a selectable comparison axis; the recorded decision cuts `measure`. Edit the metric to "implementation, a param, or group".
- **C3. N-way headline wording differs** (ux wireframe `**quick 0.74x · radix 0.61x · bubble 41.2x slower (geomean vs std, 15/15 benchmarks)**` vs plan 2.2.1 `**merge** is fastest overall (0.81x vs quick, geomean over 15/15 ns/op); loses in 2 rows`). Pick one and align the other, including the AC-S1-2 "verdict first" check.
- **C4. Reference error text** (ux S3 `reference "slow" is not one of: std, quick, radix` vs plan 2.2.2 `unknown reference 'nope'; valid: quick, heap, merge`). AC-S3-4 and tests will assert one string; choose one.
- **C5. `--print-config` shape** (ux S3: text lines like `threshold=3  (benchdiff.config.json)`; plan 1.2.3: JSON with a `"sources"` map such as `{"threshold":"flag"}`). Decide the format; update the other.
- **C6. Config source and file-name labels** drift: plan 1.2.3a method note `threshold=3 (benchdiff.json)`, plan 4.2.3b `(config)`, 1.2.3 sources `flag`/`config`, ux `(benchdiff.config.json)`. Plan Story 1.2.2 title ("Parse `benchdiff.json`") and the 1.2.4 AC use `benchdiff.json`, the very name the no-auto-discovery decision says is a results file. The Action stages the file as `/work/config.json`, so the original name is lost to the binary. Use `benchdiff.config.json` in all examples and a single source label set (`default|config|flag`).
- **C7. `publish-images` breaks the one-vocabulary rule** (ux AC-S4-1: input names equal CLI flags and config keys). It is an Action input only; 4.2.1 does not list it as a flag or key. Either add `--publish-images`/key, or document it as Action-only like `variants`, and add it to ux "Shared vocabulary".
- **C8. `Dimensioned` shape stale in ADR-004 and the plan's Review Resolutions.** ADR-004 item 5 and the "Residual home" row say `Dimensioned { set, residuals }`; the glossary and 1.1.2c now define construct-only `Vec<(Metric, Residual)>`. Update the ADR (an amendment line) and the table row.
- **C9. Scope drift (no requirement):** `--print-config` (ux S3, plan 1.2.3), the wasm size gate (plan 0.1.2, justified only indirectly by the size constraint), and `columns_cap` (MatrixOptions) have no requirement line. Add one-line justifications to requirements.md or tag them as supporting work.
- **C10. Config keys omitted from ADR-001's key list:** `version`, `only-changed`, `fail-on-regression`, `title`, `tag` are in plan 1.2.2 but not in the ADR list. (Also see N6.)
- **C11. Hosting reason enum:** ADR-003 item 5 lists `'private'|'fork'|'permission'|'push-failed'|'unsupported-host'|'disabled'`; plan 4.1.2 also returns `'no-event'`. Add it to the ADR (terminology, not behavioural).

## NITPICKS

- N1. ux states with no plan story/AC: `· 3 incomplete` summary suffix and Method-note naming of the missing implementation; the matrix `Rank` column; the box plot at n >= 20; the "all unchanged" gray chart with `no significant changes`; the exact `chart skipped: no comparable rows` text for zero rows (plan uses `nothing to compare` / `Empty`); "metrics excluded from charts" in the skipped-metrics line; a private-repo row in the hosting-failed state table (ux lists permissions, rate limit, upload only).
- N2. AC-S2-8 (legible at 375 px, or docs say the table is primary on phones): add to the 5.1.1 README AC.
- N3. AC-S3-8 (unwritable `--svg-dir`: print report, then error, exit 2): plan 3.3.1a only says `create_dir_all`; add the error path.
- N4. plan Review Resolutions row "Reference direction" still says "matrix normalises `better` on the cloned input", which round 2 replaced with the short-circuit and no clone.
- N5. ux "Shared vocabulary" omits `dimensions-strict`, `correction`, `svg-embed`, `publish-images`, `min-abs`, `max-rows`, `fail-on-regression`.
- N6. ADR-001 states only some keys are tied to plan tasks; add 1.2.5a as the owner of `thresholds` application (already mentioned) and the missing core keys (see C10).
- N7. ux flow coverage table says S3 is "Covered (10 conditions)"; the S3 error table has 17 rows.
- N8. Six marker shapes (ux S2, plan 3.1.3a) for up to 8 series: shapes repeat for series 7 and 8 (colour still differs). State this, or add two shapes.
- N9. requirements NFR says "~10 implementations"; both ux and plan cap columns and series at 8 with a visible notice. This is consistent with "degrade by capping", but note the cap in the requirement.
- N10. ux Hosting-failed copy `read-only token on a fork PR` vs plan 4.1.1 example `fork PR`: align the reason strings.

## Requirement coverage matrix (In Scope)

| Requirement | Plan stories | Gap |
|---|---|---|
| Dimensions from names and file labels, axis and row key | 1.1.x, 2.1.2 | none |
| N-way vs reference, ranking, deltas, pair significance, Markdown matrix | 2.1.3-2.1.5, 2.2.1, ADR-002/005 | requirements say "first/fastest"; ADR-005 picks first only (documented) |
| SVG: delta, ranking, distribution, light/dark, a11y, bounded | 3.1.x, 3.2.x | none |
| Delivery: files, hosted embed, standalone, fallback | 3.3.x, 4.x, 5.1.4 | B2 (defaults); HTML wrapper cut is recorded |
| Config: dimensions, reference, thresholds per group/metric, charts, sizes, sort, caps, output formats | 1.2.x, 1.2.5 | B3 (per-group thresholds, output formats) |
| Docs, tests, rebuilt WASM | 0.1.x, 5.1.x | none |
