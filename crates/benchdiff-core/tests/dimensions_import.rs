//! Dimension extraction over every importer fixture.

use benchdiff_core::dimensions::apply;
use benchdiff_core::{parse, Format};

const FIXTURES: [(Format, &str); 10] = [
    (Format::Go, "go/bench.txt"),
    (Format::Go, "go/bench-count8.txt"),
    (Format::Benchdiff, "benchdiff/native.json"),
    (Format::Bmf, "bmf/bmf.json"),
    (Format::CargoBench, "cargo-bench/bench.txt"),
    (Format::Criterion, "criterion/criterion.jsonl"),
    (Format::GabSmaller, "gab/custom.json"),
    (Format::GoogleBench, "googlebench/googlebench.json"),
    (Format::Hyperfine, "hyperfine/hyperfine.json"),
    (Format::Jmh, "jmh/jmh.json"),
];

#[test]
fn importers_should_leave_metric_dims_empty_when_no_extraction_requested() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/");
    for (format, file) in FIXTURES {
        let text = std::fs::read_to_string(format!("{root}{file}")).unwrap();
        let set = parse(format, &text).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert!(!set.metrics.is_empty(), "{file}: no metrics");
        for m in &set.metrics {
            assert!(
                m.dims.is_empty(),
                "{file}: {} has dims {:?}",
                m.name,
                m.dims
            );
        }
    }
}

#[test]
fn apply_should_extract_dims_from_every_importer_fixture_without_procs_when_defaults() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/");
    for (format, file) in FIXTURES {
        let text = std::fs::read_to_string(format!("{root}{file}")).unwrap();
        let set = parse(format, &text).unwrap();
        let names: Vec<String> = set.metrics.iter().map(|m| m.name.clone()).collect();
        let (dimensioned, _) = apply(set, &[], false).unwrap_or_else(|e| panic!("{file}: {e}"));
        let mut seen = 0;
        for (metric, _) in dimensioned.entries() {
            assert!(
                !metric.dims.contains_key("procs"),
                "{file}: {}",
                metric.name
            );
            assert!(
                names.contains(&metric.name),
                "{file}: name rewritten: {}",
                metric.name
            );
            seen += 1;
        }
        assert!(seen > 0, "{file}: no entries");
    }
}
