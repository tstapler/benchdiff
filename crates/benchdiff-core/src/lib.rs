//! Benchmark importers, comparison and report rendering.
//!
//! Pure functions over serde types: no clock, no network, no filesystem, so the same code
//! runs natively and as a `wasm32-wasip1` module inside the GitHub Action.

mod compare;
pub mod import;
mod markdown;
mod model;
mod stats;
mod units;

pub use compare::{compare, Method, Options, Report, Row, Sort, Summary, Verdict};
pub use import::{detect, parse, Format};
pub use markdown::{marker, render_markdown};
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
