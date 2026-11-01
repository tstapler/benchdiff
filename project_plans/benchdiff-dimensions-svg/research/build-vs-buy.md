# Build vs Buy: benchdiff dimensions + SVG

Date: 2026-10-31. Crate data (versions, dates, licenses, deps, downloads) pulled from the crates.io API on this date: VERIFIED. SaaS pricing comes from search snippets of third-party pages: INFERRED, re-check before relying on it. WASM-size impact was not measured (no build run): INFERRED from crate size and dependency lists. Baseline: `dist/benchdiff.wasm` is 330,650 bytes; core deps today are only `serde` + `serde_json`; release profile is `opt-level="s"`, LTO, `panic="abort"`.

## 1. OSS libraries

### SVG / chart crates

Key requirement: pure Rust, no fonts, no I/O, deterministic bytes, `wasm32-wasip1`, small, custom layout (dumbbell, grouped bars, light/dark CSS, `<title>`/`<desc>`).

| Crate | Version / last release | License | Normal deps | Verdict |
|---|---|---|---|---|
| `svg` | 0.18.0, 2024-09-27, 8.8M downloads | Apache-2.0 OR MIT | none | Viable (optional) |
| `plotters` | 0.3.7, 2024-09-08, 262M dl | MIT | `num-traits`, `plotters-backend`, `wasm-bindgen`, `web-sys` (non-optional) + many optional | Not recommended |
| `charts-rs` | 2.3.0, 2026-10-17, 141k dl | Apache-2.0 | `arc-swap`, `serde`, `serde_json`, `ttf-parser` (+ optional `image`, `resvg`) | Not recommended |
| `poloto` | 19.1.2, 2023-07-09, 1.07M dl | MIT | `tagu` | Not recommended |
| `tagu` | 0.1.6, 2023-07-11, 692k dl | MIT | none | Not recommended |
| `xmlwriter` | 0.1.0, 2019-08-12, 33.7M dl | MIT | none | Viable (but stale) |

- **`svg`**
  - Pros: zero deps, tiny (19 KB crate), dual MIT/Apache, widely used, builds a node tree and escapes attribute/text values. Mostly a typed builder, so layout stays ours.
  - Cons: gives no chart help, so the hard parts (scales, ticks, label truncation, theming) stay ours anyway. Escaping and serialization are the only part it saves, and that is about 40 lines. Serialization order/number formatting is not under our control (float formatting via `Display` is deterministic, but we would still want to round coordinates ourselves). Last release 13 months ago (slow but stable crate).
  - Verdict: Viable. Acceptable if a reviewer prefers not owning escaping. Not necessary.
- **`plotters`**
  - Pros: most mature, huge adoption, SVG backend exists (`plotters-svg`).
  - Cons: unconditional `wasm-bindgen`/`web-sys` deps are wrong for `wasm32-wasip1` (the Action's target) and bloat the lockfile; its own text layout needs fonts (`ab_glyph`/`font-kit`/`ttf-parser`) which are unavailable in the sandbox; chart look is canned and hard to make dark/light safe via CSS; last release 2024-09.
  - Verdict: Not recommended.
- **`charts-rs`**
  - Pros: actively maintained (released 2026-10-17), ships SVG output, themes, bar charts.
  - Cons: Apache-2.0 only, `ttf-parser` + embedded font assets (crate is 493 KB) for text measurement, `arc-swap`, serde-driven JSON options, pulls `resvg` only if PNG enabled. Would grow the WASM noticeably (INFERRED, unmeasured), and its output determinism/theme model (hard-coded colors, no CSS `prefers-color-scheme`) is not ours to control. Small user base (141k downloads).
  - Verdict: Not recommended.
- **`poloto` / `tagu`**
  - Pros: small, pure Rust, CSS-styleable output, no fonts (poloto emits CSS classes and a stylesheet); closest philosophical fit.
  - Cons: line/scatter/histogram plotting library with no grouped-bar or dumbbell primitive; unmaintained since July 2023 (API churn: 19 major versions); single maintainer. Would still need heavy custom code.
  - Verdict: Not recommended (poloto); `tagu` Not recommended (stale, tiny value).
- **`xmlwriter`**: no deps, 6 KB, writes escaped XML. Stale (2019) but "finished". Viable as an escaping layer, but again, the hand-rolled escape is trivial.

### Config format crates

| Option | Version / date | License | Fit | Verdict |
|---|---|---|---|---|
| `serde_json` (already a dep) | 1.0.154, 2026-10-11 | MIT OR Apache-2.0 | zero added weight; no comments | Recommended as the baseline |
| `toml` | 1.1.8, 2026-10-09, 1.08B dl | MIT OR Apache-2.0 | deps `serde_spanned`, `toml_datetime`, `winnow` (via `toml_parser`); pure Rust, wasm-clean; adds parser weight (not measured; expect tens of KB, INFERRED) | Recommended (if comments/human-edited config wanted) |
| `json5` | 1.3.7, 2026-10-31 | MIT-style (not verified) | pest-based, heavier, niche | Not recommended |
| YAML (`serde_yaml` archived; forks fragmented) | n/a | n/a | ecosystem unsettled | Not recommended |

- JSON: Pros: no new dependency, no WASM growth, existing serde derive types reused. Cons: no comments, awkward for hand-written config.
- TOML: Pros: the Rust-ecosystem norm, comments, per-group threshold tables map naturally to `[thresholds."group/name"]`; actively maintained by the cargo team's ecosystem (`toml-rs`). Cons: new transitive deps (`winnow`, `serde_spanned`, `toml_datetime`) and wasm size growth.
- Open design point: the WASM core is I/O-free, so the config file must be read by the host (CLI `main.rs` / `index.js`) and passed in as a string; the parser choice is independent of that.

### Regex crates (dimension extraction from names)

- `regex` 1.13.1 (2026-07-15, MIT OR Apache-2.0): full Unicode tables and DFA engines; builds for wasm32-wasip1 but adds hundreds of KB (INFERRED from known regex footprint; unmeasured). Not recommended for this use.
- `regex-lite` 0.1.9 (2026-02-03, MIT OR Apache-2.0, zero deps, 96 KB crate): same author (rust-lang), no Unicode tables, much smaller binary, linear-time, no catastrophic backtracking, same syntax subset. Recommended if user-supplied regex is a requirement (it is: requirements say "explicit, user-overridable rule (regex/template)").
- Hand-written template matcher (`{impl}/{size}`-style placeholders, split on delimiters): zero deps, covers Go `/impl=quick`, `[size=1000]`, JMH `param`, Criterion `group/function/param` patterns. Viable and arguably safer: no ReDoS surface on untrusted benchmark names, easier errors. Cannot express arbitrary regexes.
- Recommendation: default extractors per format hand-coded; user override as a `regex-lite` pattern with named capture groups (names become dimension names). Add the template form only if regex proves too hard for users.

### Stats crates and the existing `stats.rs`

`crates/benchdiff-core/src/stats.rs` (median + two-sided Mann-Whitney U) reviewed line by line:

- Rank-sum with average ranks over ties, `U = R_a - n(n+1)/2`, `U_min = min(U, nm-U)`: correct.
- Exact path (no ties, n,m <= 50): `u_distribution` builds the Gaussian-binomial coefficients `[n+m choose m]_q` by the product `(1-q^(n+i))/(1-q^i)`: correct identity; two-sided `p = min(1, 2*P(U <= u_min))`. Test `fully_separated_5_vs_5_has_the_textbook_exact_p` and the 3v3 minimum p = 0.1 (2/C(6,3)) match known tables. Soundness VERIFIED by reading, plus the existing unit tests (not rerun here). Possible float precision: counts up to C(100,50) ~ 1e29 held in `f64`, relative error ~1e-16 per op with subtractive steps in the polynomial (`poly[u] -= ...`); cancellation could in principle lose precision at the top end of n=m=50. INFERRED risk, low practical impact since p there is tiny or the decision is far from alpha; a property test comparing against the normal approximation at n=m=50 (already partly covered by `normal_approximation_tracks_the_exact_value_for_moderate_n`) would close it.
- Normal approximation: variance `nm/12 * ((N+1) - sum(t^3-t)/(N(N-1)))` with tie correction and 0.5 continuity correction: standard. `erfc` uses Numerical Recipes 6.2 (fractional error < 1.2e-7): adequate for decisions at alpha around 0.05; not for reporting tiny p-values (1e-10) with relative accuracy.
- Gaps relevant to N-way: no multiple-comparison correction (Holm/Bonferroni would be ~15 lines on top of `mann_whitney_p`); no effect-size/confidence interval (Hodges-Lehmann); no Kruskal-Wallis omnibus. All small to hand-roll.
- `statrs` 0.19.1 (2026-08-11, MIT, 225 KB, deps `approx`, `num-traits`, `thiserror`): provides distributions (normal CDF, etc.) but not a Mann-Whitney test. Replacing a 20-line `erfc` with it adds three deps for no functional gain. Not recommended. No other mainstream crate offers a ready, wasm-lean Mann-Whitney. Verdict: keep `stats.rs`; extend it by hand (Holm correction, maybe Kruskal-Wallis later).

## 2. SaaS / hosted

| Option | Cost | Data residency / lock-in | Fit | Verdict |
|---|---|---|---|---|
| Bencher (bencher.dev) | Not confirmed by search; free tier for public projects, paid for private (UNVERIFIED, check pricing page) | Results uploaded to Bencher Cloud (or self-host); account + API token secret | History/trend service; explicitly Out of Scope; requires a secret beyond `GITHUB_TOKEN` | Not recommended as a replacement; fine as a documented complement |
| github-action-benchmark | Free (license not confirmed by search; check repo LICENSE) | Stores history in `gh-pages` branch in the user's repo; needs `contents: write` | Trend dashboards with Chart.js; alert comments; JS/Action based, not an SVG-in-comment tool | Not recommended (overlaps out-of-scope history; no N-way) |
| CodSpeed | Free for public OSS; seat/minute priced for private per third-party listing (INFERRED) | Runs benchmarks in their instrumented environment; uploads to CodSpeed | Different model (it measures, benchdiff only compares) | Not recommended |
| QuickChart (quickchart.io) | Free tier plus paid plans (details from third-party pages; verify) | Chart config goes to a third-party URL on every render; private repo benchmark names/values leak to a third party; availability dependency; camo-proxied | Easy URL charts via Chart.js config, but PNG-oriented, no dumbbell/CSS theming, breaks "no data leaves the runner" requirement | Not recommended |

Rationale: the requirements demand "no data leaves the runner except via the user's own PR/release" and "no secrets beyond `GITHUB_TOKEN`". Every SaaS above violates one of those or targets history storage, which is out of scope.

## 3. Hand-written SVG builder vs library

Correctness risks and how each is handled in a small in-house builder:

- Escaping: escape `& < > " '` for both text nodes and attributes; strip/replace characters illegal in XML 1.0 (control chars other than tab/LF/CR, U+FFFE/FFFF, lone surrogates cannot occur in Rust `str`). A single `esc()` function with exhaustive unit tests plus a fuzz/property test ("output always parses as well-formed XML"; check with `xmllint` or a dev-dependency parser in tests only) removes the risk. A library only provides this if you use its text-node API consistently; it does not eliminate the review burden.
- Text metrics: no library helps in the sandbox (no fonts). `plotters`/`charts-rs` would embed font data; the others do not measure at all. Use a per-character width table (e.g. Helvetica/Arial-like advance widths for ASCII, default width for others, wide for CJK) scaled by font size, plus conservative truncation with an ellipsis and `<title>` on truncated labels holding the full name. Custom regardless of choice.
- Determinism: own the number formatter (fixed decimals, trim trailing zeros, avoid `-0`), iterate in sorted order (never `HashMap` iteration), no timestamps or random ids. Easier to guarantee with a builder we control than via a third-party float-formatting path.
- Theming/accessibility: `<style>` with CSS variables and `@media (prefers-color-scheme: dark)`, `role="img"`, `<title>`/`<desc>`: library-neutral; `svg` crate would require raw string escape hatches for `<style>`.
- Dependency weight: custom costs maybe 300 to 600 lines of Rust in `benchdiff-core` and adds no deps, no WASM bloat beyond its own code.
- When custom is justified: when no library supplies the required chart types (dumbbell, grouped ranking bars), when the target is `wasm32-wasip1`, and when output must be byte-deterministic. All three hold here.
- LLM-generated risk: the main failure modes are silent (a mis-escaped name only breaks with hostile input; clipped text only with long labels). Mitigate with snapshot tests of golden SVGs, an XML well-formedness check over a corpus of hostile names (`<script>`, `&amp;`, quotes, emoji, 500-char names), and viewing the rendering.

Verdict: custom string builder with a tiny internal `SvgWriter` (escape, number format, element push). Recommended. Adopting `svg` crate is Viable if a typed node API is preferred.

## 4. Fork / adapt

- `poloto` (MIT, stale since 2023): could be vendored for its tick/axis logic (the "nice number" tick generation), but its architecture is plot-iterator based and it lacks bar/dumbbell. Cost of adapting exceeds writing ~50 lines of tick computation. Not recommended.
- `charts-rs` (Apache-2.0): bar chart with themes is the closest feature match; forking means carrying font data and `ttf-parser`, and dropping serde/arc-swap. Not recommended; reading its bar-layout code for reference is fine (license permits, attribute if copied).
- `benchstat` (Go, BSD): reference for the stats/table semantics, not code to fork into Rust. benchdiff already follows its Mann-Whitney approach.
- No existing project renders N-way benchmark comparison SVGs from Go/JMH/Criterion input that is small enough to fork. Nothing close enough to fork.

## Recommendation

1. SVG: hand-written deterministic builder in `benchdiff-core` (new `svg` module with an internal writer), zero new dependencies. Fixed char-width table for text estimation, truncation plus `<title>` for clipped labels, CSS-variable light/dark theme, `role="img"` + `<title>`/`<desc>`. Tests: golden SVG snapshots, XML well-formedness over hostile names, determinism check (render twice, compare bytes).
2. Config: JSON via existing `serde_json` as the guaranteed baseline; add `toml` only if the owner wants comments and accepts the wasm growth (measure `dist/benchdiff.wasm` before/after; budget decision is in the plan/ADR). Host reads the file and passes the string into the I/O-free core. Schema versioned and additive.
3. Dimension extraction: built-in per-format extractors plus a user override via `regex-lite` with named groups (small, no Unicode tables, linear-time). Skip `regex`.
4. Stats: keep hand-rolled `stats.rs` (sound by inspection; tests exist). Add Holm correction, or reference-only comparison with documented policy, in-house. Do not add `statrs`.
5. SaaS: none in the core path; document Bencher/github-action-benchmark as complements for history. Hosting of images for comments is a separate research item (release asset, artifact, orphan branch); QuickChart is rejected on data-leak grounds.

Sources: crates.io API (`https://crates.io/api/v1/crates/<name>`) for all crate figures; [poloto docs.rs](https://docs.rs/crate/poloto/latest/source/Cargo.lock); [CodSpeed marketplace](https://github.imc.re/marketplace/codspeed-continuous-performance-analysis); [QuickChart pricing](https://www.quickchart.io/pricing/); [github-action-benchmark](https://www.Github.com/benchmark-action/github-action-benchmark).
