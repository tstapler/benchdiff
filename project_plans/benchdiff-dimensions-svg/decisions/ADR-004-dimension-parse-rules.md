# ADR-004: Dimension parsing is opt-in, hand-coded defaults plus `regex-lite` user rules, `name` is never rewritten

**Status**: Accepted
**Date**: 2026-10-31
**Project**: benchdiff-dimensions-svg

## Context
Name conventions collide: Go `BenchmarkSort/impl=quick-8` (trailing `-8` is GOMAXPROCS, not a dimension), Go `size=1000`, JMH `@Param` (arrives as JSON params), Criterion `group/function/param`, Google Benchmark `name/arg:1/real_time`. Requirements say "explicit, user-overridable rule, not guessing". Measured: `regex` +876 KB, `regex-lite` +55 KB, zero-dep matcher +0 (`research/stack.md`).

## Decision
1. **Model**: `Metric` gains `dims: BTreeMap<String,String>` (additive, `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]`, schema stays 1). `Metric::key()` and `name` are never changed; `dims` is derived annotation. `Metric::new` keeps its signature; `with_dims` is added.
2. **Opt-in**: extraction runs only when `compare-by`, `group-by` or `dimensions` is set (or a file label supplies a dim). With none set, no extraction happens and output is byte-identical.
3. **Built-in default extractor (hand-coded, no regex)**, used when axes are requested but no rule matches: split the benchmark name on `/`; each segment of form `key=value` becomes a dim; `[k=v]` and `(k = v)` bracket groups likewise; **No `-N` handling (revised after review):** the Go importer already strips the GOMAXPROCS suffix at import (`import/go.rs`), so `procs` never appears for real Go input and is not a dim in v1; names that keep a dash (`size=10-8`, `quick-sort`) are left whole. Importer-provided params (JMH, Criterion ids, Go `procs`) are copied into dims by the importers in a later additive step, not v1.
4. **User rules** (`dimensions` input / config array): `"<dim>=<regex>"` where the regex uses `regex-lite` with named capture groups; named groups become dims, the matched span is removed from the residual name. Rules run in order; first match per dim wins; ASCII semantics (`\w`, `\d`) documented. Pattern length cap 256 chars, compile size limit set; linear time (no ReDoS).
5. **Residual name rule**: `residual = name with every extracted span removed, separators collapsed`. Row identity for pivoting is `(group, residual, measure, non-axis dims)`; `group-by` chooses which non-axis dims participate (default all), the row label is `residual` plus those dims (`Sort [size=1000]`), and dropping a dim that causes a collision is an error telling the user to add it to `group-by`. The residual is stored beside the metric inside a construct-only `Dimensioned` holding `Vec<(Metric, Residual)>` (amended in round 3: the earlier `Dimensioned { set, residuals }` shape allowed a desynced filter or reorder), not on `Metric`; `group` is also accepted as a built-in pseudo-dimension, `measure` is not an axis (units differ). Rules are newline-delimited and their `<dim>` prefix must equal a named group.
6. **Strictness** (amended in round 3 to split data-dependent problems from static config errors): a rule that matches zero names is a **data-dependent problem**: by default a visible notice naming the key, count, one example and the fix (`research/ux.md` section 4) and the rule is skipped; under `dimensions-strict` it is a **config error, exit 2** with the same text. An invalid pattern, a rule without a named group, or a prefix that is not a group stays a static exit-2 error either way. `dimensions-strict` (Action input, CLI flag and config key, same kebab-case name; there is no bare `strict` key) also turns the other data-dependent problems (collision, absent axis, lost variant, absent reference; plan Risk Control) into errors and additionally errors when any name fails to match. Default mode lists unmatched names in the report diagnostics (first 10).
7. **Collision check** after extraction on `(group, residual, measure, dims)`: duplicates name both original names (same style as `MetricSet::validate`); fatal under `dimensions-strict`, otherwise the first is kept, the second dropped and the notice names both. Dim from file label colliding with a parsed dim of the same key is an error.
8. Unmatched names keep the whole name as residual and empty dims.
9. **Discoverability (added after triad review round 1).** The zero-config rules in item 3 are stated verbatim in `--help`, the README and `action.yml`. Every error about an absent or unknown axis ends with `available dimensions: ...` built from `Dimensioned::dimension_summary` (with nothing detected: `available dimensions: group only (default rules read /key=value segments and [k=v] groups; add a 'dimensions' rule or use labelled variants)`), and the CLI-only `--print-dimensions` prints the same summary as JSON (names, first-seen values, benchmark counts, rule source, unmatched count) and exits 0 without rendering a report.

## Alternatives rejected
- **Heuristic-only guessing**: mis-splits (`/` in values, `-8`).
- **Template-only matcher** (`{bench}/{impl}`): safer but cannot express arbitrary patterns; may be added later if regex proves hard for users.
- **Full `regex`**: +876 KB.
- **Widening `Metric::key()` to include dims**: old readers would merge or mismatch metrics.

## Consequences
- wasm grows about 55 KB (budgeted in task 0.1.2).
- A fuzz-ish table test covers Go, JMH, Criterion, GBench name shapes and the `-8` collisions.
- Budget note (2026-11-01): budget raised 480,000 -> 820,000 bytes: the measured wasm at HEAD (3.3.1b) is 799,198; matrix wiring (+194 KB, 2.2.2b) and SVG wiring (+116 KB, 3.3.1b) exceeded the 450-480 KB estimate, which counted unreachable code only (regex-lite alone is about 41 KB reachable); opt-level=z (-12%) and wasm-opt -Oz (about 610 KB) were measured and not adopted to keep the committed wasm reproducible; revisit if headroom drops under 20,000. The "about 55 KB" figure above is the superseded estimate for regex-lite.
