//! Benchmark importers, comparison and report rendering.
//!
//! Pure functions over serde types: no clock, no network, no filesystem, so the same code
//! runs natively and as a `wasm32-wasip1` module inside the GitHub Action.

mod compare;
pub mod config;
pub mod dimensioned;
pub mod dimensions;
pub mod import;
mod markdown;
pub mod markdown_matrix;
pub mod matrix;
mod model;
pub mod notice;
pub mod options;
mod stats;
pub mod svg;
mod units;

pub use compare::{compare, Method, Options, Report, Row, Sort, Summary, Verdict};
pub use import::{detect, parse, Format};
pub use markdown::{marker, render_markdown};
pub use matrix::{
    compare_matrix, matrix_failed, matrix_failures, Cell, Comparison, FailedCell, FailureKind,
    MatrixOptions, MatrixReport, MatrixRow, MatrixSummary, Variant, VariantLabel,
};
pub use model::{Better, Metric, MetricSet};

/// The machine-readable report: every row, the settings, and the verdict counts.
pub fn render_json(report: &Report) -> String {
    let count = |v| report.count(v);
    let value = serde_json::json!({
        "counts": {
            "regressed": count(Verdict::Regressed),
            "improved": count(Verdict::Improved),
            "unchanged": count(Verdict::Unchanged),
            "new": count(Verdict::New),
            "removed": count(Verdict::Removed),
            "incomparable": count(Verdict::Incomparable),
            "invalid": count(Verdict::Invalid),
            "skipped": report.summary.skipped_head_total,
        },
        "failed": report.exceeds_failure_threshold(),
        "has_base": report.has_base,
        "summary": report.summary,
        "rows": report.rows,
    });
    serde_json::to_string_pretty(&value).expect("report serializes")
}

/// Verdict-count keys of the companion `counts`, matching [`render_json`].
#[derive(Default)]
struct CellCounts {
    regressed: usize,
    improved: usize,
    unchanged: usize,
    new: usize,
    removed: usize,
    incomparable: usize,
    invalid: usize,
}

impl CellCounts {
    fn add(&mut self, cell: &Cell, reference_present: bool) {
        match cell {
            Cell::Missing if reference_present => self.removed += 1,
            Cell::Missing => {}
            Cell::Present(p) => match &p.vs_reference {
                Comparison::Compared { verdict, .. } => match verdict {
                    Verdict::Regressed => self.regressed += 1,
                    Verdict::Improved => self.improved += 1,
                    _ => self.unchanged += 1,
                },
                Comparison::Incomparable(_) | Comparison::DirectionMismatch => {
                    self.incomparable += 1
                }
                Comparison::Invalid(_) => self.invalid += 1,
                Comparison::NoReference => self.new += 1,
                Comparison::Reference => {}
            },
        }
    }

    fn to_json(&self, skipped: usize) -> serde_json::Value {
        serde_json::json!({
            "regressed": self.regressed,
            "improved": self.improved,
            "unchanged": self.unchanged,
            "new": self.new,
            "removed": self.removed,
            "incomparable": self.incomparable,
            "invalid": self.invalid,
            "skipped": skipped,
        })
    }
}

/// The two-way JSON shape for an N-way run, plus an additive `matrix` key (schema stays 1).
///
/// Top-level `counts` are cells versus the reference after the Holm pass; `failed` is
/// [`matrix_failed`] for `gate` (`fail-on-regression`); `rows` is empty. A Missing cell in a
/// row the reference lacks has no verdict and is left out of the companion counts.
pub fn render_json_with_matrix(report: &MatrixReport, gate: Option<f64>) -> String {
    let ref_col = report
        .summary
        .reference
        .as_ref()
        .and_then(|r| report.columns.iter().position(|c| c.as_str() == r.label))
        .unwrap_or(0);
    let mut total = CellCounts::default();
    let mut per_variant: Vec<CellCounts> = report
        .columns
        .iter()
        .map(|_| CellCounts::default())
        .collect();
    for row in &report.rows {
        let reference_present = matches!(row.cells.get(ref_col), Some(Cell::Present(_)));
        for (col, cell) in row.cells.iter().enumerate().filter(|(c, _)| *c != ref_col) {
            total.add(cell, reference_present);
            per_variant[col].add(cell, reference_present);
        }
    }
    let skipped: usize = report.summary.skipped.iter().map(|s| s.total).sum();
    let breakdown: Vec<serde_json::Value> = report
        .columns
        .iter()
        .enumerate()
        .filter(|(c, _)| *c != ref_col)
        .map(|(c, label)| {
            let mut v = per_variant[c].to_json(0);
            v["label"] = label.as_str().into();
            v.as_object_mut().expect("object").remove("skipped");
            v
        })
        .collect();
    let value = serde_json::json!({
        "schema": 1,
        "counts": total.to_json(skipped),
        "failed": matrix_failed(report, gate),
        "has_base": true,
        "rows": [],
        "matrix": {
            "summary": report.summary,
            "reference": report.summary.reference,
            "columns": report.columns,
            "rows": report.rows,
            "reference_absent": report.reference_absent,
            "geomean": report.geomean,
            "counts": breakdown,
        },
    });
    serde_json::to_string_pretty(&value).expect("matrix report serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_json_should_expose_counts_and_the_failed_flag() {
        let set = MetricSet::parse(r#"{"metrics":[{"name":"x","value":1}]}"#).unwrap();
        let worse = MetricSet::parse(r#"{"metrics":[{"name":"x","value":2}]}"#).unwrap();
        let opts = Options {
            fail_on_regression_pct: Some(0.0),
            ..Options::default()
        };
        let v: serde_json::Value =
            serde_json::from_str(&render_json(&compare(Some(&set), &worse, &opts))).unwrap();
        assert_eq!(v["counts"]["regressed"], 1);
        assert_eq!(v["failed"], true);
        assert_eq!(v["rows"][0]["verdict"], "regressed");
    }

    const GO_BASE: &str = "BenchmarkA-8 100 5 ns/op\nBenchmarkB-8 100 6 ns/op\n";
    const GO_HEAD_NAN: &str = "BenchmarkA-8 100 5 ns/op\nBenchmarkB-8 100 NaN ns/op\n";

    fn go_report(fail: Option<f64>) -> Report {
        let base = parse(Format::Go, GO_BASE).unwrap();
        let head = parse(Format::Go, GO_HEAD_NAN).unwrap();
        let opts = Options {
            fail_on_regression_pct: fail,
            ..Options::default()
        };
        compare(Some(&base), &head, &opts)
    }

    #[test]
    fn nan_in_head_is_invalid_not_removed_and_trips_the_gate() {
        let r = go_report(Some(0.0));
        assert_eq!(r.count(Verdict::Invalid), 1);
        assert_eq!(r.count(Verdict::Removed), 0);
        assert!(r.exceeds_failure_threshold());
        assert!(!go_report(None).exceeds_failure_threshold());

        let md = render_markdown(&r);
        assert!(
            md.contains("1 metric(s) in head had no usable value and were ignored: `B [ns/op]`"),
            "{md}"
        );
        assert!(
            md.contains("⛔ no valid value") && md.contains("1 invalid"),
            "{md}"
        );

        let v: serde_json::Value = serde_json::from_str(&render_json(&r)).unwrap();
        assert_eq!(v["counts"]["invalid"], 1);
        assert_eq!(v["counts"]["skipped"], 1);
        assert_eq!(v["rows"][0]["verdict"], "invalid");
    }

    #[test]
    fn skipped_metric_missing_from_base_only_warns() {
        let base = parse(Format::Go, "BenchmarkA-8 100 5 ns/op\n").unwrap();
        let r = compare(
            Some(&base),
            &parse(Format::Go, GO_HEAD_NAN).unwrap(),
            &Options::default(),
        );
        assert_eq!(r.count(Verdict::Invalid), 0);
        assert!(!r.exceeds_failure_threshold());
        assert!(render_markdown(&r).contains("were ignored"));
    }
}
