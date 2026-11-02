# Test data provenance

| File | Origin |
|---|---|
| `go/bench.txt`, `go/bench-count8.txt` | Real `go test -bench . -benchmem -count=3` / `-count=8` output (go1.26.4, AMD Ryzen 9 7900X) from a throwaway package |
| `criterion/criterion.jsonl` | Real `cargo criterion --message-format=json` (cargo-criterion 1.1.0, criterion 0.5); the machine-specific `report_directory` field was removed |
| `cargo-bench/bench.txt` | Real libtest output (`RUSTC_BOOTSTRAP=1 cargo bench`) |
| `hyperfine/hyperfine.json` | Real `hyperfine -N --warmup 2 --runs 10 --export-json` (hyperfine 2.0.0, new schema) |
| `hyperfine/hyperfine-v1.json` | **Hand-written** in the 1.x shape (`results[].times`, `median`); not produced by a real 1.x binary |
| `jmh/jmh.json` | **Hand-written** from the field list in JMH's `JSONResultFormat.java`; not produced by a real JMH or kotlinx-benchmark run |
| `googlebench/googlebench.json` | **Hand-written** from the Google Benchmark user guide |
| `gab/custom.json`, `bmf/bmf.json` | **Hand-written** from the github-action-benchmark README and the Bencher BMF reference (the BMF file is the reference's own example) |
| `benchdiff/native.json` | **Hand-written** |
| `golden/base.go.txt` | Copy of `go/bench-count8.txt` |
| `golden/head.go.txt` | `go/bench-count8.txt` with `Sort` `ns/op` multiplied by 1.3 and `Join` `ns/op` by 0.8, to get a known regression and improvement |

Replace the hand-written files with real captures when a repo that produces them is wired in.

## Golden expected outputs

`golden/expected/<case>.{md,json}` freeze the output of the CLI (`--markdown` and `--json`) for 15 cases, run from the repo root with `cargo build --release -p benchdiff`:

- Option cases on `golden/head.go.txt` vs `golden/base.go.txt`: `default`, `json` (adds `--format go`), `only-changed` (`--only-changed`), `max-rows-2` (`--max-rows 2`), `sort-name` (`--sort name`), `threshold-1-min-abs-5` (`--threshold 1 --min-abs 5`).
- `native-base`: `benchdiff/native.json` as both head and base with `--format benchdiff`.
- `importer-<format>` for `bmf`, `cargo-bench`, `criterion`, `gab-smaller`, `gab-bigger`, `googlebench`, `hyperfine`, `jmh`: the fixture under `testdata/` as both base and head with `--format <format>`.

Regenerate with `UPDATE_GOLDEN=1` once the asserting tests (`test/golden.test.js`, `crates/benchdiff/tests/golden_expected.rs`) land; a diff in `golden/expected/` must be reviewed as an intentional behaviour change.

## Golden SVG review rule

The golden SVGs in `crates/benchdiff-core/tests/snapshots/svg/` are checked byte for byte by `crates/benchdiff-core/tests/svg_snapshots.rs` (rewrite with `UPDATE_SNAPSHOTS=1`). A byte match proves sameness, not that the chart is legible, so a pull request that updates any golden SVG must attach the PNGs from `scripts/svg-to-png.sh` and the reviewer opens them before approving:

```sh
scripts/svg-to-png.sh                 # all goldens -> target/svg-png/<name>-760.png and <name>-320.png
scripts/svg-to-png.sh -o out some.svg # specific files or directories
```

The script needs `resvg`, `rsvg-convert` or `inkscape` and prints a clear message when none is installed. It is dev-only: never a crate dependency and never part of the wasm.

## `svg-worst/` worst-case chart inputs

Inputs for the geometry tests in `crates/benchdiff-core/tests/svg_geometry.rs`, one name or label per line:

| File | Content |
|---|---|
| `names-long.txt` | one 500-character benchmark name |
| `names-cjk.txt` | CJK and Hangul names, including 120-character runs |
| `names-rtl.txt` | Arabic and Hebrew names, a bidi-override name and a mixed-direction name |
| `variants-8.txt`, `variants-10.txt` | 8 and 10 maximum-length (40-character) variant labels |

The tests build 30 rows from these names (two-way, and 8 and 10 variants x 30 rows for the N-way charts) and render every chart at widths 480 and 1600 in both themes. They assert every coordinate lies inside the viewBox, every `<text>` (advance-table width plus the 10% margin) lies inside the card, and no two text boxes overlap. A deliberately overflowing fixture in the same file proves the assertions can fail. The rendered worst-case charts are reviewed as PNG in task 5.1.3a.

## `nway/` N-way fixture

| File | Content |
|---|---|
| `quick.txt`, `heap.txt`, `merge.txt` | Go-format results for three sort implementations (66 lines each); `quick` is the reference |
| `impl.txt` | The same 180 benchmark lines in one file, derived from the three files by inserting `/impl=<label>` after the benchmark's base name (`BenchmarkSort/impl=heap/n=1000-8`), so values are identical; `--compare-by impl` splits it back into the three variants |
| `baseline/` | Pairwise outputs and the falsifier questions Q1-Q3 (`baseline/QUESTIONS.md`) |
| `expected/matrix.{md,json}` | Frozen N-way output with the Action's defaults (title `Benchmark comparison`, tag `default`) |
| `expected/notice-only.md` | Frozen notice-only report |
| `expected/svg/*.svg` | Frozen charts, see below |

`expected/matrix.{md,json}` come from, run in `testdata/nway` after `cargo build --release -p benchdiff`:

```sh
../../target/release/benchdiff --variant quick=quick.txt --variant heap=heap.txt --variant merge=merge.txt \
  --reference quick --markdown expected/matrix.md --json expected/matrix.json
```

`--head impl.txt --compare-by impl --reference quick` renders byte-identical Markdown and JSON (no documented differences). `crates/benchdiff-core/tests/nway_fixture.rs` checks both against the expected files through the library and asserts Q1-Q3 in the JSON (heap significantly faster on 4 `ns/op` rows, merge significantly slower on `Parse`, heap on `Parse` not significant); `test/golden.test.js` checks wasm and native against them (`UPDATE_GOLDEN=1` rewrites).

### `nway/expected/svg/`

| File | Command (run in `testdata/nway`) |
|---|---|
| `matrix-{ranking,distribution}-{light,dark}.svg` | the three `--variant` files, `--reference quick --charts ranking,distribution --chart-theme both --svg-dir <dir>` |
| `pairwise-heap-{delta,distribution}.svg` | `--base quick.txt --head heap.txt --charts delta,distribution --svg-dir <dir>` |
| `pairwise-merge-{delta,distribution}.svg` | `--base quick.txt --head merge.txt --charts delta,distribution --svg-dir <dir>` |

The CLI writes `ranking-light.svg`, `delta.svg` and so on; the golden test prefixes the case name. `test/golden.test.js` runs each case natively (temp `--svg-dir`) and under wasm (`--svg-dir /work/svg`, a WASI preopen) with `--svg-embed true`, and asserts the SVG bytes and the `<picture>` Markdown are identical between the two and the SVGs equal these files (`UPDATE_GOLDEN=1` rewrites). These goldens follow the PNG review rule above: `scripts/svg-to-png.sh -o <dir> testdata/nway/expected/svg` and open the PNGs before approving a change.
