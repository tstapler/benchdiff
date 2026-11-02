//! Performance budgets (requirements SLO: < 1 s per 10k metrics). `#[ignore]`d so the default
//! debug `cargo test` stays fast; `make perf` runs them with `cargo test --release -- --ignored perf`.
//! Generators are seeded and deterministic; no network, no filesystem.

use benchdiff_core::dimensions::{self, Rule};
use benchdiff_core::matrix::project_by_axis;
use benchdiff_core::{
    compare, compare_matrix, parse, render_markdown, render_matrix_markdown_with, render_svgs,
    ChartKind, Format, MatrixOptions, MetricSet, Options, RenderExtras, SvgInput, SvgOptions,
};
use std::fmt::Write;
use std::time::{Duration, Instant};

const BUDGET: Duration = Duration::from_secs(1);

/// xorshift64: deterministic, dependency-free.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Go-format text: `names` benchmarks, each with `samples` lines around `base * scale(name)`.
fn go_text(names: &[String], samples: usize, scale: f64, seed: u64) -> String {
    let mut rng = Rng(seed);
    let mut out = String::from("goos: linux\ngoarch: amd64\npkg: perf\n");
    for (i, name) in names.iter().enumerate() {
        let base = 1000.0 + (i % 97) as f64 * 50.0;
        for _ in 0..samples {
            let v = base * scale * (0.97 + 0.06 * rng.unit());
            let _ = writeln!(out, "{name}-8 \t 1000 \t {v:.2} ns/op");
        }
    }
    out
}

fn metric_set(text: &str) -> MetricSet {
    parse(Format::Go, text).expect("generated Go text parses")
}

#[test]
#[ignore = "perf budget: run via `make perf` (cargo test --release)"]
fn perf_two_way_compare_and_render_markdown_when_10k_metrics_should_finish_under_1s() {
    let names: Vec<String> = (0..10_000).map(|i| format!("BenchmarkB{i:05}")).collect();
    let base = metric_set(&go_text(&names, 10, 1.0, 1));
    let head = metric_set(&go_text(&names, 10, 1.08, 2));
    assert_eq!(head.metrics.len(), 10_000);

    let start = Instant::now();
    let report = compare(Some(&base), &head, &Options::default());
    let md = render_markdown(&report);
    let elapsed = start.elapsed();

    eprintln!("perf two-way 10k metrics: {elapsed:?} (budget {BUDGET:?})");
    assert!(md.contains("benchdiff"), "report rendered");
    assert!(report.rows.len() >= 10_000, "every metric compared");
    assert!(elapsed < BUDGET, "took {elapsed:?}, budget {BUDGET:?}");
}

#[test]
#[ignore = "perf budget: run via `make perf` (cargo test --release)"]
fn perf_nway_extract_compare_render_and_svgs_when_10_variants_300_benchmarks_should_finish_under_1s(
) {
    let rules: Vec<Rule> = ["algo=(?P<algo>a[0-9]+)", "impl=/impl=(?P<impl>v[0-9]+)"]
        .iter()
        .map(|r| Rule::parse(r).unwrap())
        .collect();
    let text = nway_text(10, 300);
    let set = metric_set(&text);
    assert_eq!(set.metrics.len(), 3_000);

    let start = Instant::now();
    let (dimensioned, _) = dimensions::apply(set, &rules, false).unwrap();
    let projection = project_by_axis(&dimensioned, "impl", None).unwrap();
    assert_eq!(projection.variants.len(), 10, "{:?}", projection.notices);
    let options = MatrixOptions {
        columns_cap: 10,
        ..MatrixOptions::default()
    };
    let report = compare_matrix(&projection.variants, &Options::default(), &options);
    let md = render_matrix_markdown_with(&report, &RenderExtras::default());
    let svgs = render_svgs(
        SvgInput::Matrix(&report),
        &SvgOptions {
            kinds: vec![
                ChartKind::Delta,
                ChartKind::Ranking,
                ChartKind::Distribution,
            ],
            ..SvgOptions::default()
        },
    );
    let elapsed = start.elapsed();

    eprintln!("perf n-way 10x300 + svgs: {elapsed:?} (budget {BUDGET:?})");
    assert_eq!(report.rows.len(), 300);
    assert!(!md.is_empty());
    // Delta is pair-only; the matrix input yields the ranking chart and skips the others with a reason.
    assert_eq!(svgs.files.len() + svgs.skipped.len(), 3);
    assert!(
        svgs.files.iter().any(|f| f.name.starts_with("ranking")),
        "skipped: {:?}",
        svgs.skipped
    );
    assert!(elapsed < BUDGET, "took {elapsed:?}, budget {BUDGET:?}");
}

/// `variants` x `benchmarks` names like `Benchmark_a12_s3/impl=v04`, 10 samples each.
fn nway_text(variants: usize, benchmarks: usize) -> String {
    let mut out = String::new();
    for v in 0..variants {
        let names: Vec<String> = (0..benchmarks)
            .map(|b| format!("Benchmark_a{}_s{}/impl=v{v:02}", b / 10, b % 10))
            .collect();
        out.push_str(&go_text(&names, 10, 1.0 + v as f64 * 0.02, 10 + v as u64));
    }
    out
}

#[test]
#[ignore = "perf budget: run via `make perf` (cargo test --release)"]
fn perf_two_way_svgs_when_500_rows_should_finish_under_1s() {
    let names: Vec<String> = (0..500).map(|i| format!("BenchmarkB{i:04}")).collect();
    let base = metric_set(&go_text(&names, 10, 1.0, 3));
    let head = metric_set(&go_text(&names, 10, 1.1, 4));

    let start = Instant::now();
    let report = compare(Some(&base), &head, &Options::default());
    let svgs = render_svgs(
        SvgInput::Pair(&report),
        &SvgOptions {
            kinds: vec![
                ChartKind::Delta,
                ChartKind::Ranking,
                ChartKind::Distribution,
            ],
            ..SvgOptions::default()
        },
    );
    let elapsed = start.elapsed();

    eprintln!("perf two-way 500 rows + svgs: {elapsed:?} (budget {BUDGET:?})");
    assert!(!svgs.files.is_empty(), "skipped: {:?}", svgs.skipped);
    assert!(elapsed < BUDGET, "took {elapsed:?}, budget {BUDGET:?}");
}
