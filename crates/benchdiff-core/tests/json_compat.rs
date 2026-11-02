//! JSON compatibility: the chart-sample side channel never reaches the serialized report (G-18).

use benchdiff_core::matrix::{MatrixOptions, Variant, VariantLabel};
use benchdiff_core::{
    compare, compare_matrix, matrix_failed, render_json, render_json_with_matrix, Better, Metric,
    MetricSet, Options,
};
use serde_json::Value;

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

fn variant(label: &str, items: &[(&str, f64)], samples: &[f64]) -> Variant {
    let metrics = items
        .iter()
        .map(|(n, v)| {
            let mut m = Metric::new("", n, "", "ns", *v, Better::Lower);
            m.samples = samples.to_vec();
            m
        })
        .collect();
    Variant {
        label: VariantLabel::parse(label).unwrap(),
        set: MetricSet {
            metrics,
            ..Default::default()
        },
    }
}

fn matrix_json(gate: Option<f64>) -> Value {
    let vs = vec![
        variant("quick", &[("Sort", 100.0), ("Only", 5.0)], &[]),
        variant("heap", &[("Sort", 130.0), ("Extra", 7.0)], &[]),
        variant(
            "merge",
            &[("Sort", 80.0), ("Only", 5.0), ("Extra", 7.0)],
            &[],
        ),
    ];
    let opts = Options {
        keep_samples: true,
        fail_on_regression_pct: gate,
        ..Options::default()
    };
    let r = compare_matrix(&vs, &opts, &MatrixOptions::default());
    assert_eq!(matrix_failed(&r, gate), gate.is_some());
    serde_json::from_str(&render_json_with_matrix(&r, gate)).unwrap()
}

#[test]
fn matrix_key_should_be_present_only_when_a_matrix_is_rendered() {
    let two_way: Value = serde_json::from_str(&json(false)).unwrap();
    assert!(two_way.get("matrix").is_none());
    assert!(two_way.get("schema").is_none(), "default bytes unchanged");
    let v = matrix_json(None);
    assert_eq!(v["schema"], 1);
    for k in ["reference", "columns", "rows", "geomean", "counts"] {
        assert!(v["matrix"].get(k).is_some(), "matrix.{k}");
    }
    assert_eq!(v["matrix"]["reference"]["label"], "quick");
    assert_eq!(v["matrix"]["reference"]["explicit"], false);
}

#[test]
fn companion_counts_should_cover_every_non_reference_cell_slot() {
    let v = matrix_json(Some(10.0));
    let counts = v["counts"].as_object().unwrap();
    let two_way: Value = serde_json::from_str(&json(false)).unwrap();
    let mut want: Vec<_> = two_way["counts"].as_object().unwrap().keys().collect();
    let mut got: Vec<_> = counts.keys().collect();
    want.sort();
    got.sort();
    assert_eq!(got, want, "same key set as the two-way report");
    assert_eq!(counts["regressed"], 1, "heap on Sort");
    assert_eq!(counts["improved"], 1, "merge on Sort");
    assert_eq!(counts["new"], 2, "Extra: reference lacks it");
    assert_eq!(counts["removed"], 1, "heap lacks Only");
    let sum: u64 = [
        "regressed",
        "improved",
        "unchanged",
        "new",
        "removed",
        "incomparable",
        "invalid",
    ]
    .iter()
    .map(|k| counts[*k].as_u64().unwrap())
    .sum();
    let rows = v["matrix"]["rows"].as_array().unwrap().len() as u64;
    assert_eq!(sum, 2 * rows, "(variants - 1) x rows");
    assert_eq!(v["failed"], true);
    assert_eq!(v["has_base"], true);
    assert_eq!(v["rows"], serde_json::json!([]));
    assert!(v["matrix"]["counts"].is_array(), "per-variant breakdown");
}

#[test]
fn companion_failed_should_be_false_without_a_gate() {
    assert_eq!(matrix_json(None)["failed"], false);
}

#[test]
fn matrix_json_should_carry_p_adjusted_and_geomean_but_never_samples() {
    let samples = [90.0, 100.0, 110.0, 100.0, 95.0, 105.0];
    let vs = vec![
        variant("a", &[("X", 100.0), ("Y", 100.0), ("Z", 100.0)], &samples),
        variant(
            "b",
            &[("X", 300.0), ("Y", 310.0), ("Z", 290.0)],
            &[300.0; 6],
        ),
    ];
    let opts = Options {
        keep_samples: true,
        ..Options::default()
    };
    let r = compare_matrix(&vs, &opts, &MatrixOptions::default());
    let out = render_json_with_matrix(&r, None);
    assert!(!out.contains("samples"), "{out}");
    assert!(out.contains("p_adjusted"), "{out}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(!v["matrix"]["geomean"].as_array().unwrap().is_empty());
}

#[test]
fn migration_should_be_reversible() {
    // A schema-1 reader ignores `matrix`; what is left carries the two-way companion keys.
    let mut v = matrix_json(Some(10.0));
    let full = v.clone();
    v.as_object_mut().unwrap().remove("matrix");
    let two_way: Value = serde_json::from_str(&json(false)).unwrap();
    for k in ["counts", "failed", "has_base", "rows"] {
        assert!(v.get(k).is_some(), "{k}");
        assert!(two_way.get(k).is_some(), "{k}");
    }
    assert_eq!(full["counts"], v["counts"]);
    // The default path is untouched by the new function.
    assert_eq!(json(false), json(true));
}
