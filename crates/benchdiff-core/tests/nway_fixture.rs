//! The N-way fixture in `testdata/nway`: library output equals the frozen `expected/matrix.{md,json}`,
//! the falsifier-question properties (Q1-Q3) hold in it, and `--compare-by impl` on `impl.txt`
//! reproduces the labelled-files matrix.

use benchdiff_core::dimensioned::Dimensioned;
use benchdiff_core::dimensions;
use benchdiff_core::matrix::Reference;
use benchdiff_core::matrix::{label_variant, project_by_axis};
use benchdiff_core::{
    compare_matrix, parse, render_json_with_matrix, render_matrix_markdown_with, Format,
    MatrixOptions, MatrixReport, Options, RenderExtras, Variant, VariantLabel,
};
use serde_json::Value;
use std::path::PathBuf;

fn path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/nway")
        .join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(path(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn dimensioned(rel: &str) -> Dimensioned {
    let set = parse(Format::Go, &read(rel)).unwrap();
    dimensions::apply(set, &[], false).unwrap().0
}

fn options() -> MatrixOptions {
    MatrixOptions {
        reference: Reference::Label(VariantLabel::parse("quick").unwrap()),
        ..MatrixOptions::default()
    }
}

fn render(variants: &[Variant]) -> (MatrixReport, String, String) {
    let report = compare_matrix(variants, &Options::default(), &options());
    let md = render_matrix_markdown_with(&report, &RenderExtras::default());
    let json = render_json_with_matrix(&report, None);
    (report, md, json)
}

fn labelled() -> Vec<Variant> {
    ["quick", "heap", "merge"]
        .iter()
        .map(|l| {
            let d = dimensioned(&format!("{l}.txt"));
            label_variant(VariantLabel::parse(l).unwrap(), &d, "").unwrap()
        })
        .collect()
}

fn by_impl() -> Vec<Variant> {
    project_by_axis(&dimensioned("impl.txt"), "impl", None)
        .unwrap()
        .variants
}

#[test]
fn labelled_files_should_render_the_expected_matrix_files() {
    let (_, md, json) = render(&labelled());
    assert_eq!(
        md,
        read("expected/matrix.md"),
        "matrix.md differs (see testdata/README.md to regenerate)"
    );
    assert_eq!(json, read("expected/matrix.json"), "matrix.json differs");
}

#[test]
fn compare_by_impl_should_equal_the_labelled_files_matrix() {
    // No differences: the three-file and one-file inputs render byte-identical Markdown and JSON.
    let (_, md, json) = render(&by_impl());
    assert_eq!(md, read("expected/matrix.md"));
    assert_eq!(json, read("expected/matrix.json"));
}

#[test]
fn matrix_should_carry_the_falsifier_properties() {
    let v: Value = serde_json::from_str(&read("expected/matrix.json")).unwrap();
    let rows = v["matrix"]["rows"].as_array().unwrap();
    let columns: Vec<&str> = v["matrix"]["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    let verdict = |row: &Value, label: &str| -> String {
        let i = columns.iter().position(|c| *c == label).unwrap();
        row["cells"][i]["cell"]["vs_reference"]["detail"]["verdict"]
            .as_str()
            .unwrap_or("")
            .to_string()
    };
    let ns: Vec<&Value> = rows.iter().filter(|r| r["measure"] == "ns/op").collect();
    let count = |label: &str, want: &str| ns.iter().filter(|r| verdict(r, label) == want).count();
    assert!(
        count("heap", "improved") >= 3,
        "Q1: heap significantly faster on >= 3 rows"
    );
    assert!(
        count("merge", "regressed") >= 1,
        "Q3: merge significantly slower on >= 1 row"
    );
    assert!(
        ns.iter().any(|r| verdict(r, "heap") == "unchanged"),
        "Q2: a row with a non-significant gap (heap on Parse)"
    );
}
