//! JSON compatibility: the chart-sample side channel never reaches the serialized report (G-18).

use benchdiff_core::{compare, render_json, Better, Metric, MetricSet, Options};

fn set(unit: &str, values: &[f64]) -> MetricSet {
    let mut m = Metric::new("g", "Bench", "", unit, values[0], Better::Lower);
    m.samples = values.to_vec();
    MetricSet {
        env: Default::default(),
        skipped: Default::default(),
        metrics: vec![m],
    }
}

fn json(keep_samples: bool) -> String {
    let opts = Options {
        keep_samples,
        ..Options::default()
    };
    let base = set("us", &[1.0, 1.1, 1.2, 1.3, 1.4]);
    let head = set("ns", &[2000.0, 2100.0, 2200.0, 2300.0, 2400.0]);
    render_json(&compare(Some(&base), &head, &opts))
}

#[test]
fn json_should_be_byte_identical_when_keep_samples_toggles() {
    assert_eq!(json(false), json(true));
}

#[test]
fn json_should_exclude_samples_when_keep_samples_true() {
    let out = json(true);
    assert!(!out.contains("samples"), "{out}");
    assert!(!out.contains("2100"), "{out}");
}
