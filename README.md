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
- uses: tstapler/benchdiff@v0
  with:
    head: bench-head.txt          # files or globs, newline/comma separated
    base: bench-base.txt          # optional; omit for a head-only report
    format: auto                  # or go | criterion | cargo-bench | jmh | hyperfine | googlebench | gab-smaller | gab-bigger | bmf | benchdiff
```

`@v0` follows the latest `0.x` release. For a supply-chain-safe pin, use the full commit SHA of a
release instead (`tstapler/benchdiff@<40-hex-sha> # v0.1.0`). The Action is a committed WebAssembly
build plus a dependency-free `index.js`: nothing is downloaded or compiled at run time.

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

Outputs: `regressed`, `improved`, `unchanged`, `new`, `removed`, `incomparable`, `invalid`, `failed`, `report-path`,
`json-path`, `head-json-path` (the head results as canonical JSON, the file to upload as the release asset).

Unchanged rows are collapsed in `<details>`; the environment of both runs (CPU, Go version,
commit, whatever the importer or your JSON provides) is in a second `<details>`.

### More inputs

Every input below has a CLI flag and a config key of the same kebab-case name, except `head`/`base`/`variants` (flags `--head`, `--base`, `--variant`) and the GitHub-only `comment` and `github-token`. Precedence: flags > inputs > config file > defaults (the binary sees inputs as flags). `--print-config` shows the effective settings and the layer each came from (`default`, `config` or `flag`).

| Input | Default | |
|---|---|---|
| `variants` | | N-way input, one `label=path-or-glob` per line; see [N-way comparison](#n-way-comparison) |
| `compare-by` | | N-way: the dimension to pivot into columns (a dimension name, `group`, or `run`) |
| `group-by` | all but the axis | N-way: comma-separated dimensions that identify a row |
| `dimensions` | | extra `dim=regex` rules, one per line; see [Dimensions](#dimensions) |
| `dimensions-strict` | `false` | `true` turns data-dependent dimension notices into exit 2 |
| `reference` | first variant | the variant (or axis value) every other column is compared against |
| `correction` | `holm` | `holm` or `none`, across one row's cells |
| `cell-format` | `full` | `full` (`1210 ns (0.74x)`) or `ratio` (`0.74x`, shorter for phones) |
| `charts` | | `none`, or any of `delta`, `ranking`, `distribution`; see [Charts](#charts) |
| `chart-rows` | `30` | rows per chart, 1 to 200; the effective cap is `min(chart-rows, max-rows)` when `max-rows` is set |
| `chart-width` | `760` | pixels, 480 to 1600 |
| `chart-theme` | `light` | `light`, `dark` or `both` |
| `svg-dir` | | copy the SVG files to this workspace directory |
| `svg-embed` | `auto` | embed charts in the comment and summary; `auto` embeds iff `charts` is set |
| `publish-images` | `true` | host embedded charts as images; see [Hosting images](#hosting-images) |
| `config` | | path to a settings file; see [Config file](#config-file) |

Outputs added: `svg-dir`, `svg-paths` (newline-separated). In an N-way run, `regressed`, `improved`, `unchanged`, `new`, `removed`, `incomparable`, `invalid` and `failed` count cells against the reference (`incomparable` and `invalid` as in the report, after Holm; `invalid` also counts a variant series that was skipped or unusable), and `head-json-path` is empty.

**Naming.** A *variant* is one labelled input to an N-way comparison: `--variant` is the repeatable CLI flag, `variants` the multi-line Action input.

## N-way comparison

Compare more than two runs (implementations, libraries, versions) in one matrix. Two ways to supply them:

```yaml
- uses: tstapler/benchdiff@v0
  with:
    variants: |
      std=bench-std.txt
      radix=bench-radix.txt
      quick=bench-quick.txt
    reference: std
    cell-format: ratio
```

```sh
benchdiff --variant std=std.txt --variant radix=radix.txt --reference std
benchdiff --head all.txt --compare-by impl      # pivot a dimension of one result set
```

* `variants` grammar: the first `=` splits label from path; labels use `A-Za-z0-9._-` (at most 40 characters); one label is exactly one file (a glob that matches several is an error); entries are newline-separated only; it cannot be combined with `head`, `base` or `base-format`.
* Axis values that become column labels under `compare-by` (for example a Go package `net/http` under `compare-by: group`) are shown with disallowed characters replaced by `_`, cut to 40 characters and made unique with `-2`, `-3`; a notice lists the renamed values. Only explicit `variants` labels must already be valid.
* `compare-by: run` makes the two runs the columns: `--base B --head H --compare-by run` is the same as `--variant base=B --variant head=H` (reference `base` unless `reference: head`), and needs exactly one `--base` and one `--head`. The Action passes `head`/`base` and `compare-by` unchanged.
* `compare-by` pivots one [dimension](#dimensions) of the head results into columns; `group-by` names the dimensions that identify a row (default: all others).
* **Reference.** Every other column is compared against one reference: the variant or axis value named by `reference`. The default is the first variant, and for variants split from benchmark names the first one encountered in the input. Set `reference` explicitly whenever the input order is not under your control.
* **Ratio, not percent.** N-way cells are ratios against the reference: `0.74x` means a value 26% lower. Two-way reports keep percent. Prose says faster or slower, never higher or lower.
* **Holm correction.** The cells of one row are tested against the reference, and Holm's step-down procedure (`correction: holm`, the default) controls the family-wise error rate across that row only. It is not applied across rows, so pair an N-way `fail-on-regression` with a `threshold` and `min-abs`. `correction: none` skips it.
* **Geomean and the headline.** The geometric mean of a variant's ratios is taken over the rows it shares with the reference; the headline uses only the rows every variant ran (the common set) and reads `not comparable` when that set is under 3 rows or under half of all rows. Bold, the Rank column and "has the best geomean" are orderings: only each variant against the reference is tested, and there is no head-to-head claim between variants. The headline also names any other variant (not the best one, not the reference) with at least one cell slower than the reference, such as `; merge slower in 1 row`, so a slower variant is not hidden behind the best one.
* **Gate.** A cell is `Regressed` only against the reference. `fail-on-regression` in an N-way run requires an explicit `reference` (exit 2 otherwise), fails on any regressed cell over the limit (after Holm) or any invalid cell, and without it `failed` is `false`.
* `sort: rank` orders rows by variant ranking and needs an N-way run.
* A variant with unreadable or malformed data is skipped with a notice (warn by default); `dimensions-strict` turns data-dependent dimension problems into exit 2.
* The notice-only report built by the Action without running the binary uses the `title` and `tag` Action inputs (it is built when the reference or all but one variant has no data); values that exist only in a `--config` file are not known to that path, so set the inputs to match.
* For a malformed (unparseable) variant file the binary adds the parse error to the notice (`variant 'x' produced no data (path): <err>`); a missing or empty file produces the shorter text.

## Dimensions

Dimensions are the key/value facets of a benchmark name (`impl=quick`, `size=100`) that `compare-by` and `group-by` refer to. Zero-config extraction:

zero-config rules read only `key=value` path segments (a/impl=quick) and bracket groups [k=v, k2=v2] / (k = v); nothing else (Criterion group/function/param ids carry no '=')

* `dimensions` adds rules `dim=regex`, each with a `(?P<dim>...)` group, to those defaults (repeatable flag, or one per line in the Action input; YAML block scalars need no backslash doubling, JSON does: `\\d`).
* `group` is a pseudo-dimension: the benchmark's group, usable in `compare-by` and `group-by`.
* `benchdiff --head FILE --print-dimensions` prints the detected dimensions as JSON and exits (with `--variant`, it summarizes the union of all variant files). A wrong dimension name fails with an `available dimensions: ...` suffix.
* In an N-way run, a `thresholds` key matches `group/residual` (bare residual when the group is empty) before `group`; a key that matches no benchmark, such as one written against a raw name like `Sort/impl=quick`, produces the notice `threshold key '...' matches no benchmark (matrix keys are group/residual or group)` (exit 2 with `dimensions-strict`). `--base` given with `--compare-by` is ignored by the matrix, which is built from `--head`, and produces a notice; it only feeds the two-way fallback.
* `dimensions-strict` has no negative form: a `true` set in the config file cannot be switched off by an input or flag.

## Config file

Put shared settings in `benchdiff.config.json` and pass it explicitly with `config:` or `--config`. It is never auto-discovered. The [JSON Schema](benchdiff.config.schema.json) validates it and [`examples/benchdiff.config.json`](examples/benchdiff.config.json) is a tested starter (line one is the `$schema` reference). Regex backslashes are doubled in the JSON examples (`\\d`), because a JSON string needs it; in an Action input or a flag they are written as-is.

Layering is defaults < config file < flags (inputs act as flags). Boolean-like settings have negative flags so a flag can undo the file: `--no-only-changed` and `--no-fail-on-regression` (the Action inputs `only-changed: false` and `fail-on-regression: false` do the same). `dimensions-strict` has none. Run `--print-config` to see each effective value and its source.

### Concept map

Each concept has the same kebab-case name as an Action input, a CLI flag and a config key. Sources use the labels `default|config|flag` (`--print-config` `sources`, and the Method note as `threshold=3 (config)`).

| Concept | Action input | CLI flag | Config key |
|---|---|---|---|
| Noise threshold (percent) | `threshold` | `--threshold` | `threshold`, per-metric overrides in `thresholds` |
| Input formats | `format`, `base-format` | `--format`, `--base-format` | `format`, `base-format` |
| Heading and sticky-comment key | `title`, `tag` | `--title`, `--tag` | `title`, `tag` |
| Absolute noise floor | `min-abs` | `--min-abs` | `min-abs` |
| Hide unchanged rows | `only-changed` | `--only-changed` / `--no-only-changed` | `only-changed` |
| Row order | `sort` | `--sort` | `sort` |
| Row cap | `max-rows` | `--max-rows` | `max-rows` |
| Gate | `fail-on-regression` | `--fail-on-regression` / `--no-fail-on-regression` | `fail-on-regression` |
| N-way inputs | `variants` | `--variant` | (not a key) |
| Pivot axis | `compare-by` | `--compare-by` | `compare-by` |
| Row identity | `group-by` | `--group-by` | `group-by` |
| Dimension rules | `dimensions` | `--dimensions` | `dimensions` |
| Strict data problems | `dimensions-strict` | `--dimensions-strict` | `dimensions-strict` |
| Reference | `reference` | `--reference` | `reference` |
| Multiple-comparison correction | `correction` | `--correction` | `correction` |
| Cell style | `cell-format` | `--cell-format` | `cell-format` |
| Charts | `charts` | `--charts` | `charts` |
| Chart rows / width / theme | `chart-rows`, `chart-width`, `chart-theme` | `--chart-rows`, `--chart-width`, `--chart-theme` | same |
| SVG directory / embedding / hosting | `svg-dir`, `svg-embed`, `publish-images` | `--svg-dir`, `--svg-embed`, `--publish-images` | same |

### Notices and the gate line

* **Notices.** Data problems degrade with a notice instead of failing. After the verdict, reference and gate lines they appear in a fixed class order: degradation, skipped metrics, reference-absent, invalid cells, chart notices. The first 5 are inline; the rest collapse into `N more notices (see Method)`, and the Method note lists every notice.
* **Gate line.** With `fail-on-regression` set, an N-way report prints `gate: fail-on-regression 10% vs quick: passed` or `FAILED (...)`. When cells fail, the line names up to 3 of them in table order, each `variant on row` cut to 40 characters, always shows the count, and is capped at 200 characters in all; the Method note has the full list.

## Charts

`charts` renders SVG files: `delta` (change per benchmark), `ranking` (N-way ratio vs the reference) and `distribution` (samples). A non-empty `charts` embeds them in the comment and job summary and hosts them as images, unless you set `svg-embed: false` or `publish-images: false`. The CLI defaults are off: `--svg-embed` writes `benchdiff-svg:` placeholders into the Markdown, which are dangling links until something hosts the files.

* `chart-theme: both` embeds a light/dark pair with `<picture>`.
* Chart rows are the top `chart-rows` by absolute delta; the table follows `sort`. Match them by name or by the `#` column of the `Chart data (text)` block.
* Files in `svg-dir` (output `svg-paths`) are the standalone artifact; there is no HTML wrapper. For a download link, add `actions/upload-artifact` with `archive: false` yourself.
* GitHub shows the chart as an opaque `<img>`: the alt text and the `Chart data (text)` table are the accessible path. The table is the primary view on phones (chart text scales to about 7 px at 375 px width). Forced-colors and print show the chart as drawn.
* Contrast: text is at least 4.5:1 and chart marks at least 3:1 against the card in both themes; a fill under 3:1 gets an outline in the secondary ink.
* Roll back by unsetting `charts`. Never delete `benchdiff-assets`: old comments link to it.

## Hosting images

GitHub has no upload API for comment images, so benchdiff commits the SVGs to a branch and links the raw URL. This is an opt-in side effect of `charts`; every item here applies only when images are hosted.

```yaml
permissions:
  contents: write         # extra: only needed for hosted chart images
  pull-requests: write    # sticky comment
```

* Needs `permissions: contents: write`, on top of what the report already uses.
* Creates or updates the branch `benchdiff-assets` with at most one commit per run that has new bytes. Commits are kept and never pruned; deleting the branch breaks images in old comments.
* Anyone with the URL can read the images. For a public repository that is the same audience as the comment.
* Private repositories are never hosted: nothing is written there (reason `private`), and the report carries a text chart instead. GHES falls back the same way.
* Fork PRs have a read-only token: no write happens, the report shows a notice and a text chart, and often no comment can be posted at all.
* The step log prints one line before the first write: `images: publishing to branch benchdiff-assets (needs contents: write; commits are kept; see README "Hosting images")`.
* First run: with a read-only default `GITHUB_TOKEN`, `charts: delta` alone often shows the permission warning. Add the `permissions:` block above and rerun. When hosting fails, the SVG files are in the directory named in the notice (not an artifact); use `actions/upload-artifact` to keep them.

Support by repository class (every class gets the standalone files and a text chart in the report; only the hosted inline image varies):

| Repo class / event | Comment | Job summary |
|---|---|---|
| Public, same-repo PR, `contents: write` | hosted SVG inline, collapsed `Chart data (text)` | hosted SVG inline |
| Public, same-repo PR, no `contents: write` | table, `permission` notice, open text chart | same |
| Public, fork PR | usually no comment (INFERRED); otherwise notice and open text chart | notice and open text chart |
| Private repo | `private` notice and open text chart | same |
| GHES | `unsupported-host` notice and open text chart | same |
| Public, non-PR event (`push`, `workflow_dispatch`, `release`) | no comment | hosted SVG inline |

Status: VERIFIED for a public same-repo PR (a plain SHA-pinned raw URL renders in the comment). UNVERIFIED: rendering in the job summary. Not tested: private repositories and fork PRs, which is why they fall back to text.

## Adoption

benchdiff has no telemetry. Adoption is counted by hand: how many of the maintainer's repositories set `variants` or `charts` 30 and 90 days after release.

## wasm size

`dist/benchdiff.wasm` is committed and checked against a byte budget (currently 820,000 bytes, `scripts/check-wasm-size.sh`). The budget is a CI gate, not a runtime limit.

## Baseline in a PR workflow

Keep the base from the default branch in the Actions cache (the pattern stapler-squad uses):

```yaml
- uses: actions/cache/restore@<sha>
  with: { path: base/, key: bench-main-${{ github.sha }}, restore-keys: bench-main- }
# ... run benchmarks into head/ ...
- uses: tstapler/benchdiff@v0
  with: { head: head/*, base: base/* }
# on pushes to main only: cp head/* base/ and actions/cache/save
```

## Quickstarts

Each shows the benchmark command, then the `with:` inputs. Add the PR workflow above around it.

**Go** (`go test`, same input as `benchstat`; use `-count=10` or more so the significance test can run)
```yaml
- run: go test -bench . -benchmem -count=10 ./... | tee head.txt
- uses: tstapler/benchdiff@v0
  with: { head: head.txt, base: base.txt, format: go }
```
One row per benchmark and unit (`ns/op`, `B/op`, `allocs/op`, `MB/s`); `MB/s` is higher-is-better, `Unit x better=higher` lines are honoured.

**Rust, `cargo bench` (libtest)** (nightly harness, or `RUSTC_BOOTSTRAP=1` on stable)
```yaml
- run: cargo bench 2>&1 | tee head.txt
- uses: tstapler/benchdiff@v0
  with: { head: head.txt, base: base.txt, format: cargo-bench }
```
Single estimate per benchmark, so threshold-only (the `+/-` is max-min, not an interval, and is ignored).

**Rust, Criterion** (via [cargo-criterion](https://github.com/bheisler/cargo-criterion); gives per-sample data, so the Mann-Whitney test runs)
```yaml
- run: cargo install cargo-criterion --locked && cargo criterion --message-format=json > head.jsonl
- uses: tstapler/benchdiff@v0
  with: { head: head.jsonl, base: base.jsonl, format: criterion }
```

**Kotlin / JVM (kotlinx-benchmark, JMH)**
```yaml
- run: ./gradlew jvmBenchmark          # JSON lands under <module>/build/reports/benchmarks/ by default
- uses: tstapler/benchdiff@v0
  with: { head: 'kmp/build/reports/benchmarks/**/*.json', base: base.json, format: jmh }
```
`thrpt` is higher-is-better, other JMH modes lower. Parameterised benchmarks appear as `name[size=1000]`. Plain JMH works the same with `-rf json -rff head.json`.
Check the glob against your module's actual report directory.

**Hyperfine** (v1 and v2 JSON schemas)
```yaml
- run: hyperfine --warmup 3 --runs 20 --export-json head.json './mytool build'
- uses: tstapler/benchdiff@v0
  with: { head: head.json, base: base.json, format: hyperfine }
```

**Google Benchmark (C++)**
```yaml
- run: ./bench --benchmark_repetitions=10 --benchmark_format=json > head.json
- uses: tstapler/benchdiff@v0
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
    uses: tstapler/benchdiff/.github/workflows/release-benchmarks.yml@v0
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
Flags with no Action input: `--alpha P` (Mann-Whitney significance level, `0.05`), `--no-stats` (thresholds only), `--markdown FILE`, `--json FILE`, `--head-json FILE`, `--print-config`, `--print-dimensions`, `--no-only-changed`, `--no-fail-on-regression`, `--help`. All other flags mirror the inputs above (`--variant` for `variants`): `--base-format`, `--threshold`, `--min-abs`, `--only-changed`, `--sort`, `--max-rows`, `--title`, `--tag`, `--compare-by`, `--group-by`, `--dimensions`, `--dimensions-strict`, `--reference`, `--correction`, `--cell-format`, `--charts`, `--chart-rows`, `--chart-width`, `--chart-theme`, `--svg-dir`, `--svg-embed`, `--publish-images`. `--fail-on-regression` takes an optional `=PCT`.

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

## Releasing

1. `make ready` (rebuilds `dist/benchdiff.wasm` with build paths remapped, then runs every check).
2. Merge to `main`, tag `vX.Y.Z` on the merge commit, and create a GitHub release for it.
3. Move the floating major tag: `git tag -f v0 vX.Y.Z && git push -f origin v0`.
4. Optional: publish to the Marketplace from the release page (needs a one-time acceptance of the
   GitHub Marketplace Developer Agreement in the web UI).

## License

MIT, see [LICENSE](LICENSE).
