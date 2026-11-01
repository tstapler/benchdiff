# Research: Stack (benchdiff-dimensions-svg)

## Current state (VERIFIED)
- Workspace deps: only `serde 1` (derive) and `serde_json 1` (`Cargo.toml`); Cargo.lock has serde 1.0.229, serde_json 1.0.154, memchr 2.8.3. No regex, no TOML.
- Release profile: `opt-level="s"`, `lto`, `codegen-units=1`, `strip`, `panic="abort"`. Toolchain `stable`, target `wasm32-wasip1` (`rust-toolchain.toml`). `wasm-opt` not installed.
- `dist/benchdiff.wasm` = **330,650 bytes** (`ls -l`). Probe rebuild of unmodified tree = 330,778 (no path remap).
- Build quirk: Homebrew rustc on PATH lacks the wasip1 std; probes used `~/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin` (same as the Makefile).

## Size measurements (VERIFIED)
Method: copy of the workspace in the scratchpad, add one dep to the `benchdiff` bin, call it through `black_box` so LTO can't drop it, `cargo build --release --target wasm32-wasip1`. All variants compiled for wasm32-wasip1. Runtime on WASI was not exercised (all pure Rust, no I/O).

| Variant | wasm bytes | delta vs baseline |
|---|---|---|
| baseline | 330,778 | 0 |
| `regex 1.13.1` default features | 1,206,505 | **+876 KB** (3.6x) |
| `regex` no default features (`std`,`perf`, no unicode) | 898,305 | +568 KB |
| `regex-lite 0.1.9` | 385,680 | **+55 KB** |
| `svg 0.18.0` (trivial doc) | 350,299 | +19 KB |
| serde_json + derive struct (control for config structs) | 338,888 | +8 KB |
| `toml 1.1.8` (`parse`,`serde`, no defaults) | 443,310 | +104 KB over baseline (+96 KB over control) |
| `basic-toml 0.1.10` | 395,336 | +57 KB (+48 KB over control) |
| `serde_norway 0.9.42` (YAML) | 422,843 | +84 KB (+76 KB over control) |

## Crate status (crates.io API, 2026-10-31; VERIFIED)
| Crate | Latest | Updated | Notes |
|---|---|---|---|
| regex | 1.13.1 | 2026-07 | Healthy, huge usage; too big for this wasm. |
| regex-lite | 0.1.9 | 2026-02 | Same maintainers (rust-lang). No Unicode classes (`\w`,`\d` are ASCII-only), no lookaround; fine for dimension rules. |
| toml | 1.1.8+spec-1.1.0 | 2026-10 | Very active (toml-rs). Full TOML 1.1. |
| basic-toml | 0.1.10 | 2025-03 | dtolnay; deliberately minimal, TOML 0.5, slow release cadence but stable. |
| serde_yaml | 0.9.34+deprecated | 2024-03 | Archived/deprecated; avoid. |
| serde_norway | 0.9.42 | 2024-12 | Community fork, small maintainer base. |
| json5 | 1.3.7 | 2026-10 | Not measured. |
| svg | 0.18.0 | 2024-09 | Light node builder; ~1.9M recent dl; slow cadence. |
| plotters | 0.3.7 | 2024-09 | `plotters-svg` backend is optional; unconditional deps on `wasm-bindgen`/`web-sys` (normal deps, not optional). Heavy for a wasip1 CLI. |
| charts-rs | 2.3.0 | 2026-10 | Hard dep on `ttf-parser`+`arc-swap`; ships/uses embedded fonts (INFERRED from deps; font data not inspected). ~12.5k recent dl. Prescriptive chart types, not benchdiff's dumbbell/ranking shapes. |
| poloto | 19.1.2 | 2023-07 | Unmaintained since 2023. |

## Recommendations
1. **SVG generation: hand-written string builder in `benchdiff-core`, zero new deps.** Charts needed (delta bars/dumbbell, grouped bars) are rects, lines, and text. A tiny `SvgBuf` with `rect/line/text/title/desc` plus one `xml_escape` fn keeps output byte-deterministic (fixed float formatting, e.g. round to 2 dp, stable ordering) and the escaping surface in one place (satisfies the requirements' untrusted-input rule). `svg` crate saves little (+19 KB) and gives no escaping/number-format determinism guarantees beyond what we'd write anyway; plotters/charts-rs/poloto rejected (deps, fonts, wasm-bindgen, abandoned).
2. **Text width: ASCII-bucket estimator table.** Embed a ~95-entry per-character advance-width table (units/1000 em) for a generic sans stack (Helvetica/Arial/Verdana-ish metrics), fallback width for non-ASCII = 1.0 em (wide, to avoid clipping), scale by font-size. Use a conservative factor (~1.05-1.1x) and truncate with an ellipsis to a max width computed from the table. SVG `font-family="system-ui, -apple-system, Segoe UI, Helvetica, Arial, sans-serif"`; GitHub image proxy renders SVG as `<img>`, so no webfonts load. Add `textLength`/`lengthAdjust` only for fixed-width labels (not recommended generally: it distorts). Pure const table = no size cost beyond ~1 KB. (Table values not yet sourced; take them from a public Arial/Helvetica AFM and cite when implementing.)
3. **Dimension extraction regex: `regex-lite`** (+55 KB, 17% growth) over `regex` (+876 KB, ~3.6x). Rules are user-supplied patterns with named capture groups; ASCII semantics are acceptable (document it). Alternative with zero deps: a small `{key}` template/split-based matcher (Go `/k=v`, `[k=v]`, Criterion `/` segments) and keep regex-lite as an opt-in power feature. Recommend: built-in default rules implemented by hand (no regex), user override via `regex-lite` named groups. Needs an anchoring/size limit and error reporting on invalid patterns; regex-lite is linear-time (no ReDoS).
4. **Config format: TOML via `toml` crate or `basic-toml`, plus JSON for free.**
   - JSON via existing `serde_json`: +0 (needs no new dep); no comments, weak for hand-edited config.
   - `basic-toml`: +48 KB, TOML 0.5 only (no datetimes quirks that matter here), dtolnay-maintained, already serde-based. Recommended default choice for size.
   - `toml` 1.1.8: +96 KB, current spec, very active. Pick it if TOML 1.1 features or better error spans matter; ~48 KB more than basic-toml.
   - YAML: reject (serde_yaml deprecated; fork has a thin maintainer base; YAML parsing edge cases).
   - Suggested: accept `benchdiff.toml` (basic-toml) and `.json` by extension, same serde structs, `#[serde(deny_unknown_fields)]` + `version` field for schema evolution.
   - WASM FS note: config bytes should be read host-side in `index.js` (or via a preopened dir) and passed in; core stays pure (same pattern as the existing "pure, builds natively and to wasm32-wasip1" description in `crates/benchdiff-core/Cargo.toml`). Not verified against `index.js` here.
5. **Projected budget:** hand-written SVG (0) + basic-toml (+48 KB) + regex-lite (+55 KB) + new code => roughly 330 KB -> ~450-480 KB (ESTIMATE; sum of measured deltas, not a combined build). `wasm-opt` is not installed; `brew install binaryen` + `wasm-opt -Oz` could shave more but would change the Makefile reproducibility story.

## Open gaps
- No combined build measured; deltas may overlap (shared serde machinery).
- No WASI runtime execution of toml/regex-lite probes (compile-only evidence).
- Per-character width table source not chosen.
- Did not verify the `svg`/`charts-rs` output determinism or their exact font handling by reading source.
