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
}
