//! N-way flag parsing and static validation through the real binary (task 2.2.2a).
//! Every case here fails before any file is read, so the variant paths need not exist.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const HEAD: &str = "testdata/golden/head.go.txt";
const BASE: &str = "testdata/golden/base.go.txt";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_benchdiff"))
        .current_dir(root())
        .args(args)
        .output()
        .unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

/// Asserts exit 2 and that stderr contains `want`.
fn rejects(args: &[&str], want: &str) {
    let out = run(args);
    assert_eq!(out.status.code(), Some(2), "{args:?}: {}", stderr(&out));
    assert!(stderr(&out).contains(want), "{args:?}: {}", stderr(&out));
}

#[test]
fn cli_should_exit_two_with_valid_labels_when_reference_is_not_a_variant_label() {
    rejects(
        &[
            "--variant",
            "quick=a.txt",
            "--variant",
            "heap=b.txt",
            "--variant",
            "merge=c.txt",
            "--reference",
            "nope",
        ],
        "unknown reference 'nope'; valid: quick, heap, merge",
    );
}

#[test]
fn cli_should_split_variant_at_first_equals_and_reject_a_missing_label() {
    rejects(
        &["--variant", "results/size=100/a.txt"],
        "--variant: label 'results/size' is invalid (use [A-Za-z0-9._-], max 40) in \
         'results/size=100/a.txt'; expected label=path",
    );
    // A valid label keeps later '=' in the path, so the run gets past parsing.
    let out = run(&[
        "--variant",
        "quick=results/size=100/a.txt",
        "--head-json",
        "x",
    ]);
    assert!(stderr(&out).contains("--head-json is two-way only"));
}

#[test]
fn cli_should_reject_duplicate_label_and_empty_path() {
    rejects(
        &["--variant", "quick=a.txt", "--variant", "quick=b.txt"],
        "label \"quick\" given twice (a.txt, b.txt)",
    );
    rejects(&["--variant", "quick="], "--variant quick=: path is empty");
}

#[test]
fn cli_should_reject_variant_with_base_or_head_and_head_json() {
    rejects(
        &["--variant", "quick=a.txt", "--head", HEAD],
        "--variant cannot be combined with --base/--head",
    );
    rejects(
        &["--variant", "quick=a.txt", "--base", BASE],
        "--variant cannot be combined with --base/--head",
    );
    rejects(
        &["--variant", "quick=a.txt", "--head-json", "out.json"],
        "--head-json is two-way only",
    );
    rejects(
        &[
            "--head",
            HEAD,
            "--compare-by",
            "impl",
            "--head-json",
            "out.json",
        ],
        "--head-json is two-way only",
    );
}

#[test]
fn cli_should_reject_sort_rank_on_a_two_way_run_only() {
    let msg = "--sort rank needs an N-way run (--variant or --compare-by)";
    rejects(&["--head", HEAD, "--sort", "rank"], msg);
    // N-way: the sort is accepted, so the failure is the transitional "not wired" one.
    let out = run(&["--variant", "a=a.txt", "--sort", "rank"]);
    assert!(!stderr(&out).contains(msg), "{}", stderr(&out));
    let out = run(&["--head", HEAD, "--compare-by", "impl", "--sort", "rank"]);
    assert!(!stderr(&out).contains(msg), "{}", stderr(&out));
}

#[test]
fn cli_should_reject_bad_enum_values_with_the_allowed_list() {
    rejects(
        &["--head", HEAD, "--cell-format", "wide"],
        "cell-format: \"wide\" is not valid; use full, ratio",
    );
    rejects(
        &["--head", HEAD, "--correction", "bonf"],
        "correction: \"bonf\" is not valid; use holm, none",
    );
}

#[test]
fn cli_should_reject_compare_by_measure() {
    rejects(
        &["--head", HEAD, "--compare-by", "measure"],
        "compare-by 'measure' is not an axis (units differ per measure)",
    );
}

#[test]
fn cli_should_reject_invalid_dimension_rules() {
    rejects(
        &["--head", HEAD, "--dimensions", "impl=impl=(?P<impl>"],
        "invalid pattern",
    );
    rejects(
        &["--head", HEAD, "--dimensions", r"impl=impl=\w+"],
        "has no named capture group",
    );
    rejects(
        &["--head", HEAD, "--dimensions", r"impl=(?P<other>\w+)"],
        "rule prefix 'impl' is not a named group",
    );
    // The second line of a newline-delimited value is checked too.
    rejects(
        &["--head", HEAD, "--dimensions", "a=(?P<a>x)\nb=nogroup"],
        "has no named capture group",
    );
}

#[test]
fn cli_should_reject_bad_reference_label_grammar() {
    rejects(
        &["--head", HEAD, "--reference", "has space"],
        "not a valid variant label",
    );
}

#[test]
fn cli_should_exit_two_when_fail_on_regression_without_explicit_reference_in_nway() {
    let msg = "fail-on-regression in an N-way run needs an explicit 'reference' (the baseline \
               to protect); set 'reference'";
    rejects(
        &[
            "--variant",
            "a=a.txt",
            "--variant",
            "b=b.txt",
            "--fail-on-regression=10",
        ],
        msg,
    );
    rejects(
        &[
            "--head",
            HEAD,
            "--compare-by",
            "impl",
            "--fail-on-regression=10",
        ],
        msg,
    );
    // An explicit reference lifts the static error.
    let out = run(&[
        "--variant",
        "a=a.txt",
        "--variant",
        "b=b.txt",
        "--fail-on-regression=10",
        "--reference",
        "a",
    ]);
    assert!(!stderr(&out).contains(msg), "{}", stderr(&out));
    // Two-way runs never need a reference.
    let out = run(&["--head", HEAD, "--base", BASE, "--fail-on-regression=10000"]);
    assert!(!stderr(&out).contains(msg));
}

#[test]
fn cli_should_keep_two_way_output_when_no_matrix_flag_is_given() {
    let plain = run(&["--head", HEAD, "--base", BASE]);
    let with_inert = run(&[
        "--head",
        HEAD,
        "--base",
        BASE,
        "--correction",
        "none",
        "--cell-format",
        "ratio",
        "--dimensions-strict",
        "--reference",
        "quick",
        "--group-by",
        "size",
    ]);
    assert!(plain.status.success());
    assert_eq!(plain.stdout, with_inert.stdout);
}

#[test]
fn cli_should_list_every_matrix_flag_in_help_with_defaults() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let help = String::from_utf8(out.stdout).unwrap();
    for flag in [
        "--variant",
        "--compare-by",
        "--group-by",
        "--dimensions",
        "--dimensions-strict",
        "--reference",
        "--correction",
        "--cell-format",
        "--print-dimensions",
        "--config",
        "--print-config",
    ] {
        assert!(help.contains(flag), "{flag} missing from --help");
    }
    assert!(help.contains("default"));
    assert!(help.contains("key=value"), "zero-config rules missing");
    assert_eq!(help.matches("zero-config rules read").count(), 1);
}

#[test]
fn cli_should_accept_print_dimensions_as_a_flag() {
    // Parsing only: the wiring is 2.2.2b, so an inline value is the one parse-level failure.
    rejects(
        &["--head", HEAD, "--print-dimensions=yes"],
        "--print-dimensions does not take a value",
    );
}
