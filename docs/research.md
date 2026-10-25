# Research: benchmark-result formats and comparison method

Date: 2026-10-23. Confidence labels: **VERIFIED** = I opened the spec or ran the tool and
looked at its output; **INFERRED** = reasoned from the above, not checked directly.

## 1. What our repos do today

| Repo | Harness | Output | Compare / report today |
|---|---|---|---|
| tymux | Go program `bench/` (tymux vs tmux), Rust renderer `crates/tymux-bench` | custom `docs/benchmarks/results.json` (per-backend `p50_ms`/`p95_ms`/`n`, plus `env`) | renders SVG charts; CI only runs `render --check` and a ratio `gate`, never diffs two runs |
| stapler-squad | `go test -bench ... -benchmem -count=8` (Tier 1 and Tier 2), Playwright terminal-throughput spec | Go benchmark text; `web-app/benchmark-results.json` (custom) | `benchstat` on a cache-restored baseline, then a Node gate script scraping benchstat text, then a hand-written PR comment (`.github/workflows/benchmark.yml`) |
| stelekit | JVM load benchmark (`:kmp:jvmTestProfile`), Android logcat benchmark, `kotlinx-benchmark` 0.4.13 (`jvmBenchmark`, `kmp/build.gradle.kts`) | custom JSON in `benchmark-baselines/*.json` (`metrics: {phase1TtiMs: ...}`), logcat lines, kotlinx-benchmark JSON | inline `actions/github-script` that builds a Markdown table and edits a comment found by heading text |

Three repos, three bespoke diff-and-comment scripts, and only stapler-squad uses a statistical test.
(Surveyed by reading the workflows and build files above.)

## 2. Formats compared

| Format | Shape | Direction known? | Units | Samples / interval | Stability | Source |
|---|---|---|---|---|---|---|
| Go benchmark format (benchfmt) | text lines `BenchmarkX-8  N  v unit [v unit]...`, `key: value` config lines, `Unit <u> better=higher\|lower assume=...` lines | yes, via `Unit` lines; defaults: `ns/op`,`B/op`,`allocs/op` lower, `MB/s` higher | per value | one line per run, so `-count=N` gives N samples | formal proposal, used by `x/perf` | [Go proposal 14313](https://go.googlesource.com/proposal/+/master/design/14313-benchmark-format.md) **VERIFIED**; I also ran `go test -bench -count=3` ([testdata/go/bench.txt](../testdata/go/bench.txt)) |
| benchstat | consumer of the above | n/a | n/a | median plus CI; Mann-Whitney U, alpha 0.05, `~` = not significant; docs say run each benchmark at least 10 times | n/a | [benchstat docs](https://pkg.go.dev/golang.org/x/perf/cmd/benchstat) **VERIFIED** |
| github-action-benchmark | tools `go`, `cargo`, `jmh`, `pytest`, `googlecpp`, `benchmarkjs`, ...; custom JSON array `[{name, unit, value, range?, extra?}]` | by tool name only (`customBiggerIsBetter` vs `customSmallerIsBetter`) | free-text | `range` is a string | documented, widely used | [README](https://github.com/benchmark-action/github-action-benchmark) **VERIFIED** |
| Bencher Metric Format (BMF) | `{benchmark: {measure: {value, lower_value?, upper_value?}}}`, JSON Schema draft-07 | **no** (a measure's direction lives server-side) | **no** (a measure's unit lives server-side) | bounds only | versioned schema `bencher.dev/v0/bmf.json` | [BMF reference](https://bencher.dev/docs/reference/bencher-metric-format/) **VERIFIED** |
| Criterion / cargo-criterion | `cargo criterion --message-format=json` prints JSON lines (`reason: benchmark-complete`, `typical`/`mean`/`median` each with `estimate`, `lower_bound`, `upper_bound`, plus per-sample `iteration_count` and `measured_values`) | implicitly lower (time) | `unit` field (`ns`) | **yes**, per-sample arrays | Criterion's own book calls `estimates.json`/`sample.json` "private implementation details" that "may change at any time" and points to cargo-criterion's `--message-format=json` for machine use ([book](https://bheisler.github.io/criterion.rs/book/user_guide/csv_output.html) **VERIFIED**) | I ran cargo-criterion 1.1.0 ([testdata/criterion/criterion.jsonl](../testdata/criterion/criterion.jsonl)) **VERIFIED** |
| Rust libtest (`cargo bench`) | text lines `test name ... bench:  1,234 ns/iter (+/- 56)`; the `+/-` is max minus min, not a confidence interval | lower | `ns/iter` | none (one estimate) | unstable harness (nightly, or `RUSTC_BOOTSTRAP=1`); github-action-benchmark parses it as its `cargo` tool | I ran it on rustc 1.98.1 ([testdata/cargo-bench/bench.txt](../testdata/cargo-bench/bench.txt)) **VERIFIED**; the meaning of `+/-` is from libtest's source as I remember it, **INFERRED** |
| Hyperfine | `--export-json`. v1.x: `results[].{command,mean,stddev,median,min,max,times}` (**INFERRED** from memory of 1.x). **v2.0.0 (what I installed) changed it**: `schema_version`, `primary_metric`, `results[].{command,measurements[],summary{metric:{unit,count,mean,stddev,median,min,max}}}` | lower | per metric (`second`, `byte`) | per-run `measurements[]` in v2, `times[]` in v1 | **schema broke between majors**, so the importer handles both | I ran hyperfine 2.0.0 ([testdata/hyperfine/hyperfine.json](../testdata/hyperfine/hyperfine.json)) **VERIFIED**; the hyperfine scripts README does not document the JSON fields ([README](https://raw.githubusercontent.com/sharkdp/hyperfine/master/scripts/README.md)) |
| JMH JSON | array of `{benchmark, mode, threads, forks, params?, primaryMetric{score, scoreError, scoreConfidence[lo,hi], scorePercentiles, scoreUnit, rawData[[...]]}, secondaryMetrics{}}`; non-finite as strings `"NaN"`, `"-INF"`, `"+INF"` | by `mode`: `thrpt` higher, `avgt`/`sample`/`ss` lower (**INFERRED** from JMH mode semantics) | `scoreUnit` (`ops/s`, `ns/op`) | `rawData` per fork per iteration | stable for years | [JSONResultFormat.java](https://github.com/openjdk/jmh/blob/master/jmh-core/src/main/java/org/openjdk/jmh/results/format/JSONResultFormat.java) **VERIFIED** |
| kotlinx-benchmark | JVM target emits "a JSON file with a format compatible with the format used by JMH" ([README](https://github.com/Kotlin/kotlinx-benchmark)) | as JMH | as JMH | as JMH | tracks JMH | **VERIFIED** claim; **INFERRED** that non-JVM targets differ only by omitting JMH-only fields, so the importer tolerates missing fields |
| Google Benchmark JSON | `{context{...,json_schema_version}, benchmarks[{name, run_name, run_type, iterations, real_time, cpu_time, time_unit, aggregate_name?, <counters inline>}]}`; docs tell consumers to ignore unknown fields | time lower; counters (`bytes_per_second`, `items_per_second`) higher | `time_unit` | repetitions appear as separate entries, aggregates flagged by `run_type: aggregate` | versioned | [user guide](https://google.github.io/benchmark/user_guide.html) **VERIFIED** (aggregate_name and repetitions keys are not documented on that page; the importer uses `run_type` and `aggregate_name` defensively) |
| Prometheus text / OpenMetrics | `name{labels} value [ts]`, `# TYPE`, `# HELP`, OpenMetrics adds `# UNIT` and `# EOF` | no | `# UNIT` (OpenMetrics only) | histograms via `_bucket` | standardized | [Prometheus exposition formats](https://prometheus.io/docs/instrumenting/exposition_formats/) **VERIFIED** |
| OpenTelemetry metrics | protobuf/JSON data model of instruments, temporality, resources | no | UCUM string | histograms | standardized | not opened; **excluded**, see below |

## 3. Decision

**Canonical model: a flat list of metrics, a superset of BMF's leaf.** No existing open format
carries all of: stable identity, unit, direction, point estimate, interval, raw samples.

* BMF has the right *shape* (name, measure, value, lower, upper) but no unit and no direction,
  and no samples, so a regression cannot be classified from a BMF file alone.
* github-action-benchmark's custom JSON has unit and (via tool name) direction but no groups,
  no samples, and `range` is a free string.
* The Go format has samples, units and `better=` metadata, but is text, per-benchmark-line.

So the internal model, also readable as a file (`--format benchdiff`), is:

```json
{"schema": 1,
 "env": {"sha": "abc123", "cpu": "..."},
 "metrics": [
  {"group": "api", "name": "create", "measure": "p50", "unit": "ms",
   "better": "lower", "value": 0.44, "lower": 0.43, "upper": 0.46,
   "samples": [0.44, 0.45, 0.43]}
 ]}
```

Identity is `(group, name, measure)`. `lower`/`upper`/`samples` are optional. This is the
seed's placeholder format plus `measure`, `lower`, `upper`, `samples`, so existing seed files
keep parsing.

**Importers** (all in `benchdiff-core`, pure `&str -> MetricSet`):
`go`, `criterion` (cargo-criterion JSON lines), `cargo-bench` (libtest), `jmh` (also kotlinx-benchmark JVM), `hyperfine`
(v1 and v2), `googlebench`, `gab-smaller`, `gab-bigger`, `bmf`, `benchdiff`; plus `auto` detection.
Not importing OpenMetrics or OpenTelemetry: they model *live service telemetry* (counters,
gauges, cumulative histograms), not a finished benchmark run with a before/after direction, so
mapping them would mean inventing direction and sample semantics. Anything that can emit
benchmark numbers can emit the benchdiff JSON or BMF; that is the escape hatch for tymux and
stelekit's custom JSON (small adapters live in those repos).

## 4. Comparison method

Follow benchstat ([docs](https://pkg.go.dev/golang.org/x/perf/cmd/benchstat) **VERIFIED**):

1. **Both sides have at least 4 samples** -> two-sided Mann-Whitney U test at alpha 0.05
   (exact distribution when there are no ties and both sides have at most 50 samples, otherwise
   normal approximation with tie correction). Four is the floor because the smallest attainable
   two-sided p-value with 3 vs 3 is 2/C(6,3) = 0.1, so n=3 can never reach 0.05; with 4 vs 4 it
   is 2/70 = 0.029 (**derived**, and asserted in a unit test). benchstat recommends 10+ runs.
2. **Otherwise, if both sides carry an interval** (BMF bounds, JMH `scoreConfidence`,
   Criterion CI): significant only if the intervals do not overlap. Conservative but standard.
3. **Otherwise (point estimates)**: threshold only.

A change is reported as regressed/improved only if it is statistically significant (when a test
ran) **and** passes the user's effect-size floor (`threshold` percent and `min-abs` units).
Setting `threshold: 0` gives pure benchstat behavior. The report states which method ran per row
and how many rows used each.

Deltas are computed on the reported `value` (median for sampled Go/Hyperfine data, the tool's own
estimate for Criterion/JMH), not re-derived from samples, so the table matches what the tool printed.
Time units (`ns`, `us`, `ms`, `s`, with or without `/op`) are converted when base and head differ;
other unit mismatches are flagged "incomparable" rather than guessed.

## 5. Fixture provenance

Which `testdata/` files are real tool output and which are hand-written from the specs above is
listed in [testdata/README.md](../testdata/README.md). In particular the JMH/kotlinx-benchmark
importer has only been run against a hand-written fixture; no real kotlinx-benchmark output was
available when this was written.
