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
