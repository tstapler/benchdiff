//! N-way scale: 10 variants over hundreds of benchmarks must complete, honour the column cap
//! and say so. No timing here (see `perf.rs`); the caps and notices are what is checked.

use benchdiff_core::dimensions;
use benchdiff_core::matrix::project_by_axis;
use benchdiff_core::{
    compare_matrix, parse, render_matrix_markdown_with, render_svgs, ChartKind, Format,
    MatrixOptions, Options, RenderExtras, SvgInput, SvgOptions,
};
use std::fmt::Write;

fn text(variants: usize, benchmarks: usize) -> String {
    let mut out = String::from("goos: linux\ngoarch: amd64\npkg: scale\n");
    for v in 0..variants {
        for b in 0..benchmarks {
            for s in 0..5 {
                let ns = 1000.0 + b as f64 * 3.0 + v as f64 * 10.0 + s as f64;
                let _ = writeln!(
                    out,
                    "BenchmarkB{b:03}/impl=v{v:02}-8 \t 100 \t {ns:.1} ns/op"
                );
            }
        }
    }
    out
}

#[test]
fn nway_should_complete_and_report_caps_when_ten_variants_hundreds_of_rows() {
    let set = parse(Format::Go, &text(10, 300)).unwrap();
    let (d, _) = dimensions::apply(set, &[], false).unwrap();
    let projection = project_by_axis(&d, "impl", None).unwrap();
    assert_eq!(projection.variants.len(), 10);

    let options = MatrixOptions::default();
    assert!(
        options.columns_cap < 10,
        "default cap is below the variant count"
    );
    let report = compare_matrix(&projection.variants, &Options::default(), &options);
    assert_eq!(report.rows.len(), 300);

    let md = render_matrix_markdown_with(&report, &RenderExtras::default());
    assert!(!md.is_empty());
    assert!(
        md.contains(&format!("{}", options.columns_cap)) && md.to_lowercase().contains("column"),
        "the column cap is reported:\n{}",
        md.lines().take(12).collect::<Vec<_>>().join("\n")
    );

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
    assert_eq!(
        svgs.files.len() + svgs.skipped.len(),
        3,
        "every chart accounted for"
    );
    assert!(
        !svgs.notices.is_empty() || !svgs.skipped.is_empty(),
        "300 rows exceed the chart row cap; a notice or skip says so"
    );
}
