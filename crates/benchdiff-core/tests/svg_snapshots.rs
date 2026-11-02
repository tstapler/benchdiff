//! Byte snapshots of every chart kind and theme against the committed files in
//! `tests/snapshots/svg/`. Run with `UPDATE_SNAPSHOTS=1` to rewrite them, then review the
//! result as PNG (`scripts/svg-to-png.sh`, see `testdata/README.md`) before committing.
//!
//! Snapshots prove sameness, not legibility: `svg_geometry.rs` covers the latter.

use benchdiff_core::matrix::{
    CellFormat, Correction, MatrixOptions, MatrixSort, PresentCell, ReferenceInfo, VariantLabel,
};
use benchdiff_core::svg::delta::delta_chart;
use benchdiff_core::svg::distribution::{distribution_chart, distribution_matrix_chart};
use benchdiff_core::svg::ranking::ranking_chart;
use benchdiff_core::svg::{ChartTheme, ChartWidth, SvgOptions};
use benchdiff_core::{
    compare_matrix, Better, Cell, Comparison, MatrixReport, MatrixRow, MatrixSummary, Method,
    Metric, MetricSet, Options, Report, Row, Summary, Variant, Verdict,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/svg")
}

fn opts(theme: ChartTheme) -> SvgOptions {
    SvgOptions {
        theme,
        width: ChartWidth::new(760).unwrap(),
        ..Default::default()
    }
}

// --- two-way fixtures -------------------------------------------------------------------

fn pair_row(name: &str, pct: f64, verdict: Verdict, base: Vec<f64>, head: Vec<f64>) -> Row {
    let mut row = Row {
        group: String::new(),
        name: name.into(),
        measure: String::new(),
        unit: "ns".into(),
        better: Better::Lower,
        base: Some(100.0),
        head: Some(100.0),
        delta_pct: Some(pct),
        verdict,
        method: Some(Method::Threshold),
        p_value: None,
        n_base: base.len().max(1),
        n_head: head.len().max(1),
        head_range: None,
        samples: Default::default(),
    };
    row.samples.base = base;
    row.samples.head = head;
    row
}

fn report(rows: Vec<Row>) -> Report {
    Report {
        summary: Summary::default(),
        rows,
        has_base: true,
    }
}

fn delta_row(name: &str, pct: f64, verdict: Verdict) -> Row {
    pair_row(name, pct, verdict, vec![], vec![])
}

fn delta_three() -> Report {
    report(vec![
        delta_row("C", 0.4, Verdict::Unchanged),
        delta_row("B", -12.5, Verdict::Improved),
        delta_row("A", 30.0, Verdict::Regressed),
    ])
}

fn delta_clipped() -> Report {
    report(vec![
        delta_row("BenchmarkParse/size=1000", 450.0, Verdict::Regressed),
        delta_row("BenchmarkSort", -80.0, Verdict::Improved),
        delta_row("BenchmarkHash", 0.3, Verdict::Unchanged),
    ])
}

fn delta_unchanged() -> Report {
    report(vec![
        delta_row("Hash", 0.3, Verdict::Unchanged),
        delta_row("Sort", -0.2, Verdict::Unchanged),
    ])
}

fn ten(offset: f64) -> Vec<f64> {
    (0..10).map(|i| 100.0 + offset + f64::from(i)).collect()
}

fn dist_two_way() -> Report {
    report(vec![
        pair_row("Small", 1.0, Verdict::Unchanged, ten(0.0), ten(1.0)),
        pair_row("Sort", -20.0, Verdict::Unchanged, ten(0.0), ten(-20.0)),
    ])
}

// --- N-way fixtures ---------------------------------------------------------------------

fn ranking_report(columns: &[&str], rows: Vec<MatrixRow>) -> MatrixReport {
    MatrixReport {
        summary: MatrixSummary {
            title: String::new(),
            tag: String::new(),
            threshold_pct: 5.0,
            min_abs: 0.0,
            alpha: 0.05,
            use_stats: true,
            only_changed: false,
            max_rows: None,
            fail_on_regression_pct: None,
            correction: Correction::Holm,
            columns_cap: 8,
            cell_format: CellFormat::Full,
            sort: MatrixSort::Input,
            reference: Some(ReferenceInfo {
                label: columns[0].into(),
                explicit: true,
            }),
            skipped: vec![],
            threshold_overrides: BTreeMap::new(),
            config_sources: vec![],
        },
        columns: columns
            .iter()
            .map(|c| VariantLabel::parse(c).unwrap())
            .collect(),
        rows,
        geomean: vec![],
        reference_absent: vec![],
        notices: vec![],
    }
}

/// Values per column; `None` is a Missing cell. Column 0 is the reference.
fn ranking_row(name: &str, values: &[Option<f64>]) -> MatrixRow {
    let cells = values
        .iter()
        .enumerate()
        .map(|(i, v)| match v {
            None => Cell::Missing,
            Some(v) => {
                let vs_reference = if i == 0 {
                    Comparison::Reference
                } else {
                    Comparison::Compared {
                        verdict: Verdict::Unchanged,
                        delta: values[0].map(|r| (v / r - 1.0) * 100.0),
                        method: Some(Method::Threshold),
                        p_value: None,
                        p_adjusted: None,
                    }
                };
                Cell::Present(PresentCell {
                    value: *v,
                    samples: Default::default(),
                    vs_reference,
                })
            }
        })
        .collect();
    MatrixRow {
        group: String::new(),
        name: name.into(),
        measure: String::new(),
        unit: "ns".into(),
        better: Better::Lower,
        dims: Default::default(),
        cells,
        ranking: vec![],
    }
}

fn ranking_three() -> MatrixReport {
    let rows = [
        ("Sort/size=100", [Some(100.0), Some(130.0), Some(80.0)]),
        ("Sort/size=1000", [Some(1000.0), Some(1500.0), Some(900.0)]),
        ("Parse", [Some(50.0), Some(25.0), None]),
        ("Hash", [Some(10.0), Some(10.0), Some(8.0)]),
        ("Search", [Some(200.0), Some(210.0), Some(150.0)]),
        ("Encode", [Some(40.0), Some(80.0), Some(41.0)]),
    ];
    ranking_report(
        &["quick", "heap", "merge"],
        rows.iter().map(|(n, v)| ranking_row(n, v)).collect(),
    )
}

fn ranking_many() -> MatrixReport {
    let cols: Vec<String> = std::iter::once("ref".to_string())
        .chain((1..=9).map(|i| format!("v{i}")))
        .collect();
    let cols: Vec<&str> = cols.iter().map(String::as_str).collect();
    let vals: Vec<Option<f64>> = (0..10).map(|i| Some(100.0 * 1.4f64.powi(i - 3))).collect();
    ranking_report(&cols, vec![ranking_row("BenchmarkParse/size=1000", &vals)])
}

fn dist_matrix() -> MatrixReport {
    let var = |label: &str, v: f64| Variant {
        label: VariantLabel::parse(label).unwrap(),
        set: MetricSet {
            metrics: vec![{
                let mut m = Metric::new("", "Sort", "", "ns", v, Better::Lower);
                m.samples = (0..10).map(|i| v + f64::from(i)).collect();
                m
            }],
            ..Default::default()
        },
    };
    compare_matrix(
        &[var("quick", 100.0), var("heap", 130.0), var("merge", 80.0)],
        &Options {
            keep_samples: true,
            ..Options::default()
        },
        &MatrixOptions {
            compare_by: "impl".into(),
            ..MatrixOptions::default()
        },
    )
}

// --- the table --------------------------------------------------------------------------

type Render = fn() -> String;

fn svg<E: std::fmt::Debug>(r: Result<benchdiff_core::svg::SvgFile, E>) -> String {
    r.expect("chart rendered").svg
}

/// Every committed snapshot and how to regenerate it.
fn cases() -> Vec<(&'static str, Render)> {
    vec![
        ("delta-light.svg", || {
            svg(delta_chart(&delta_three(), &opts(ChartTheme::Light)))
        }),
        ("delta-dark.svg", || {
            svg(delta_chart(&delta_three(), &opts(ChartTheme::Dark)))
        }),
        ("delta-clipped-light.svg", || {
            svg(delta_chart(&delta_clipped(), &opts(ChartTheme::Light)))
        }),
        ("delta-unchanged-light.svg", || {
            svg(delta_chart(&delta_unchanged(), &opts(ChartTheme::Light)))
        }),
        ("ranking-light.svg", || {
            svg(ranking_chart(&ranking_three(), &opts(ChartTheme::Light)))
        }),
        ("ranking-dark.svg", || {
            svg(ranking_chart(&ranking_three(), &opts(ChartTheme::Dark)))
        }),
        ("ranking-many-light.svg", || {
            svg(ranking_chart(&ranking_many(), &opts(ChartTheme::Light)))
        }),
        ("distribution-light.svg", || {
            svg(distribution_chart(
                &dist_two_way(),
                &opts(ChartTheme::Light),
            ))
        }),
        ("distribution-dark.svg", || {
            svg(distribution_chart(&dist_two_way(), &opts(ChartTheme::Dark)))
        }),
        ("distribution-matrix-light.svg", || {
            svg(distribution_matrix_chart(
                &dist_matrix(),
                &opts(ChartTheme::Light),
            ))
        }),
    ]
}

fn updating() -> bool {
    std::env::var_os("UPDATE_SNAPSHOTS").is_some()
}

#[test]
fn snapshots_should_match_committed_files_when_charts_render() {
    let dir = snapshot_dir();
    let mut failures = vec![];
    for (file, render) in cases() {
        let actual = render();
        let path = dir.join(file);
        if updating() {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, &actual).unwrap();
        }
        match std::fs::read_to_string(&path) {
            Ok(want) if want == actual => {}
            Ok(_) => failures.push(format!("{file} differs")),
            Err(e) => failures.push(format!("{}: {e}", path.display())),
        }
    }
    assert!(
        failures.is_empty(),
        "{}; run with UPDATE_SNAPSHOTS=1 to refresh, then review the PNGs",
        failures.join("; ")
    );
}

#[test]
fn snapshots_should_cover_every_committed_svg_when_directory_listed() {
    let covered: Vec<&str> = cases().into_iter().map(|(f, _)| f).collect();
    let mut stray = vec![];
    for entry in std::fs::read_dir(snapshot_dir()).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if name.ends_with(".svg") && !covered.contains(&name.as_str()) {
            stray.push(name);
        }
    }
    assert!(
        stray.is_empty(),
        "snapshots with no case in cases(): {stray:?}"
    );
}

#[test]
fn snapshots_should_be_deterministic_when_rendered_twice() {
    for (file, render) in cases() {
        assert_eq!(render(), render(), "{file}");
    }
}
