//! github-action-benchmark custom JSON (`customSmallerIsBetter` / `customBiggerIsBetter`):
//! <https://github.com/benchmark-action/github-action-benchmark>. The direction comes from
//! which tool name the user picked, so the caller passes it.

use crate::model::{Better, Metric, MetricSet, Skipped};
use serde::Deserialize;

#[derive(Deserialize)]
struct Entry {
    name: String,
    unit: String,
    value: f64,
    /// Free-form in the spec, usually `"± 3"` or `"3"`.
    #[serde(default)]
    range: Option<String>,
}

pub(super) fn parse(text: &str, better: Better) -> Result<MetricSet, String> {
    let entries: Vec<Entry> =
        serde_json::from_str(text).map_err(|e| format!("github-action-benchmark JSON: {e}"))?;
    let mut skipped = Skipped::default();
    let mut metrics: Vec<Metric> = Vec::new();
    for e in entries {
        if !e.value.is_finite() {
            skipped.push("", &e.name, "");
            continue;
        }
        let mut m = Metric::new("", &e.name, "", &e.unit, e.value, better);
        if let Some(r) = e.range.as_deref().and_then(parse_range) {
            m.lower = Some(e.value - r);
            m.upper = Some(e.value + r);
        }
        metrics.push(m);
    }
    if metrics.is_empty() {
        return Err("github-action-benchmark JSON contains no benchmarks".into());
    }
    Ok(MetricSet {
        env: Default::default(),
        metrics,
        skipped,
    })
}

/// `"3"`, `"± 3"`, `"+/- 3"` -> 3.0; anything else (including the Go tool's `"±12%"`) is ignored.
fn parse_range(range: &str) -> Option<f64> {
    let digits = range.trim().trim_start_matches(['±', '+', '/', '-', ' ']);
    digits
        .parse::<f64>()
        .ok()
        .filter(|r| r.is_finite() && *r >= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_should_apply_the_caller_chosen_direction() {
        let text = r#"[{"name":"a","unit":"ms","value":1}]"#;
        assert_eq!(
            parse(text, Better::Lower).unwrap().metrics[0].better,
            Better::Lower
        );
        assert_eq!(
            parse(text, Better::Higher).unwrap().metrics[0].better,
            Better::Higher
        );
    }

    #[test]
    fn range_should_become_a_symmetric_interval() {
        let set = parse(
            r#"[{"name":"a","unit":"ms","value":10,"range":"± 2"}]"#,
            Better::Lower,
        )
        .unwrap();
        assert_eq!(
            (set.metrics[0].lower, set.metrics[0].upper),
            (Some(8.0), Some(12.0))
        );
    }

    #[test]
    fn percentage_ranges_are_not_guessed_at() {
        let set = parse(
            r#"[{"name":"a","unit":"ms","value":10,"range":"±12%"}]"#,
            Better::Lower,
        )
        .unwrap();
        assert_eq!(set.metrics[0].lower, None);
    }

    #[test]
    fn entry_without_a_unit_is_an_error() {
        assert!(parse(r#"[{"name":"a","value":1}]"#, Better::Lower).is_err());
        assert!(parse("[]", Better::Lower).is_err());
    }

    #[test]
    fn fixture_should_parse() {
        let set = parse(
            include_str!("../../../../testdata/gab/custom.json"),
            Better::Lower,
        )
        .unwrap();
        assert_eq!(set.metrics.len(), 3);
    }
}
