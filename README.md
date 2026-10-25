# benchdiff

Compare two benchmark runs and get a Markdown report: a sticky PR comment, a job summary, a
release asset. One Rust core, compiled to WebAssembly and run by a zero-dependency GitHub Action
(`runs.using: node24`), so there is no toolchain to install in CI.

```
**1 regressed · 1 improved · 14 unchanged · 0 new · 0 removed**

| Benchmark      | Base        | Head        | Change              | Evidence         |
|----------------|------------:|------------:|--------------------:|------------------|
| `Sort [ns/op]` | 9534 ns/op  | 12394 ns/op | 🔴 +30.0% ▰▰▰▱▱     | p<0.001 (n=8/8)  |
| `Join [ns/op]` | 26.88 ns/op | 21.5 ns/op  | 🟢 -20.0% ▰▰▰▱▱     | p<0.001 (n=8/8)  |
```

Rows with samples (Go `-count=N`, Criterion, JMH raw data, Hyperfine runs) are judged with
the same Mann-Whitney U test as `benchstat`; point estimates fall back to a percentage/absolute
threshold. The report says which method judged how many rows. Precedence: samples (at least 4 per
side, Mann-Whitney) -> non-overlapping intervals -> threshold. Background and sources:
[docs/research.md](docs/research.md).

## Use it

```yaml
- uses: tstapler/benchdiff@<commit-sha>   # not published yet; pin a SHA once it is
  with:
    head: bench-head.txt          # files or globs, newline/comma separated
    base: bench-base.txt          # optional; omit for a head-only report
    format: auto                  # or go | criterion | cargo-bench | jmh | hyperfine | googlebench | gab-smaller | gab-bigger | bmf | benchdiff
```

Needs `permissions: pull-requests: write` for the sticky comment. The comment is found by its marker on a Bot-authored comment, so a personal access token as `github-token` creates a new comment each run; use the default `GITHUB_TOKEN` or an App token. On fork PRs the token is
read-only: the action logs a warning and the report still lands in the job summary.

| Input | Default | |
|---|---|---|
| `head` | required | result file(s) for this commit |
| `base` | | result file(s) to compare against |
| `format` | `auto` | see above; `auto` sniffs the content |
| `base-format` | same as `format` | format of the base files, e.g. `benchdiff` for a previous release's canonical JSON |
| `threshold` | `5` | percent; smaller changes are noise |
| `min-abs` | `0` | absolute noise floor in the metric's own unit |
| `only-changed` | `false` | hide unchanged rows |
| `sort` | `delta` | `delta` (worst regression first), `name`, `input` |
| `max-rows` | | cap on changed rows shown; the JSON report keeps all |
| `title` / `tag` | `Benchmark comparison` / `default` | heading; the sticky comment is keyed by `<!-- benchdiff:TAG -->` (tag: letters, digits, `- _ . : /`). `title` is rendered as Markdown, so treat it as trusted: never pass PR titles or other event data |
| `fail-on-regression` | `false` | `true` = any regression, a number = at least that many percent |
| `comment` | `true` | post/update the PR comment |

Several files passed to `head` or `base` are merged; a benchmark present in two files is an error
(repeated Go lines inside one file are samples of the same benchmark). `fail-on-regression` and the
numeric inputs must be finite and non-negative, and `tag` must not contain `-->` or control characters.
The sticky comment is matched only if it was posted by a bot and starts with the marker.

Outputs: `regressed`, `improved`, `unchanged`, `new`, `removed`, `failed`, `report-path`,
`json-path`, `head-json-path` (the head results as canonical JSON, the file to upload as the release asset).

Unchanged rows are collapsed in `<details>`; the environment of both runs (CPU, Go version,
commit, whatever the importer or your JSON provides) is in a second `<details>`.

## Baseline in a PR workflow

Keep the base from the default branch in the Actions cache (the pattern stapler-squad uses):

```yaml
- uses: actions/cache/restore@<sha>
  with: { path: base/, key: bench-main-${{ github.sha }}, restore-keys: bench-main- }
# ... run benchmarks into head/ ...
- uses: tstapler/benchdiff@<commit-sha>
  with: { head: head/*, base: base/* }
# on pushes to main only: cp head/* base/ and actions/cache/save
```

## Quickstarts

Each shows the benchmark command, then the `with:` inputs. Add the PR workflow above around it.

**Go** (`go test`, same input as `benchstat`; use `-count=10` or more so the significance test can run)
```yaml
- run: go test -bench . -benchmem -count=10 ./... | tee head.txt
- uses: tstapler/benchdiff@<sha>
  with: { head: head.txt, base: base.txt, format: go }
```
One row per benchmark and unit (`ns/op`, `B/op`, `allocs/op`, `MB/s`); `MB/s` is higher-is-better, `Unit x better=higher` lines are honoured.

**Rust, `cargo bench` (libtest)** (nightly harness, or `RUSTC_BOOTSTRAP=1` on stable)
```yaml
- run: cargo bench 2>&1 | tee head.txt
- uses: tstapler/benchdiff@<sha>
  with: { head: head.txt, base: base.txt, format: cargo-bench }
```
Single estimate per benchmark, so threshold-only (the `+/-` is max-min, not an interval, and is ignored).

**Rust, Criterion** (via [cargo-criterion](https://github.com/bheisler/cargo-criterion); gives per-sample data, so the Mann-Whitney test runs)
```yaml
- run: cargo install cargo-criterion --locked && cargo criterion --message-format=json > head.jsonl
- uses: tstapler/benchdiff@<sha>
  with: { head: head.jsonl, base: base.jsonl, format: criterion }
```

**Kotlin / JVM (kotlinx-benchmark, JMH)**
```yaml
- run: ./gradlew jvmBenchmark          # JSON lands under <module>/build/reports/benchmarks/ by default
- uses: tstapler/benchdiff@<sha>
  with: { head: 'kmp/build/reports/benchmarks/**/*.json', base: base.json, format: jmh }
```
`thrpt` is higher-is-better, other JMH modes lower. Parameterised benchmarks appear as `name[size=1000]`. Plain JMH works the same with `-rf json -rff head.json`.
Check the glob against your module's actual report directory.

**Hyperfine** (v1 and v2 JSON schemas)
```yaml
- run: hyperfine --warmup 3 --runs 20 --export-json head.json './mytool build'
- uses: tstapler/benchdiff@<sha>
  with: { head: head.json, base: base.json, format: hyperfine }
```

**Google Benchmark (C++)**
```yaml
- run: ./bench --benchmark_repetitions=10 --benchmark_format=json > head.json
- uses: tstapler/benchdiff@<sha>
  with: { head: head.json, base: base.json, format: googlebench }
```

**Anything else** (a custom harness): emit [github-action-benchmark custom JSON](https://github.com/benchmark-action/github-action-benchmark) (`gab-smaller` / `gab-bigger`), [Bencher BMF](https://bencher.dev/docs/reference/bencher-metric-format/) (`bmf`), or benchdiff's own JSON, which carries groups, units, direction, intervals and samples:
```json
{"schema": 1, "env": {"sha": "abc123"},
 "metrics": [{"group": "api", "name": "create", "measure": "p50", "unit": "ms",
              "better": "lower", "value": 0.44, "samples": [0.44, 0.45, 0.43, 0.44]}]}
```
A metric is identified by `(group, name, measure)`. Readers accept `schema` 1; a newer schema is rejected.

## Release benchmarks

[`.github/workflows/release-benchmarks.yml`](.github/workflows/release-benchmarks.yml) is a reusable
workflow: it runs your benchmarks, uploads `benchmarks.json` and `benchmark-diff.md` as release
assets, and diffs against the previous release's `benchmarks.json`.

```yaml
on: { release: { types: [published] } }
jobs:
  bench:
    permissions: { contents: write }
    uses: tstapler/benchdiff/.github/workflows/release-benchmarks.yml@<sha>
    with:
      bench-command: go test -bench . -benchmem -count=10 ./... | tee bench.txt
      results: bench.txt
      format: go
      benchdiff-ref: <same commit SHA>   # required; pin it
```

The previous release's asset is canonical benchdiff JSON, so the workflow diffs it with
`base-format: benchdiff` whatever `format` you use for the new results. `setup` and `bench-command`
run verbatim: never build them from `github.event.*` data.

## CLI

```sh
cargo run -p benchdiff -- --base base.txt --head head.txt --format go
cargo run -p benchdiff -- --head new.json --head-json canonical.json   # convert to canonical JSON
```
The CLI rejects non-finite or negative numbers and flags that would swallow the next flag.
Exit codes: `0` ok, `1` the `--fail-on-regression` limit was exceeded, `2` bad input or usage.

## Development

```sh
make test      # cargo test --workspace
make golden    # rebuild dist/benchdiff.wasm and check it against the native binary via the JS shim
node --test "test/*.test.js"   # same checks against the committed wasm; set BENCHDIFF_NATIVE to use a native binary other than target/release/benchdiff
make ready     # fmt, clippy -D warnings, tests, golden
```

* `crates/benchdiff-core`: model, importers, comparison, Markdown/JSON rendering. No I/O.
* `crates/benchdiff`: the CLI (hand-rolled flags, `std::fs` only so it also runs under WASI).
* `index.js`, `action.yml`: the Action. `dist/benchdiff.wasm` is committed; rebuild with `make wasm` after any Rust change. The golden test compares the committed wasm with the native build on a fixed set of cases, so behavior drift on those cases is caught; a byte-identical rebuild is not enforced because toolchain output is not reproducible across hosts.
* `testdata/`: real tool output where I could produce it; see [testdata/README.md](testdata/README.md) for which files are hand-written.
