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
    rejects(
        &["--head", HEAD, "--print-dimensions=yes"],
        "--print-dimensions does not take a value",
    );
}

// ---- 2.2.2b: matrix wiring, degradation ladder, --print-dimensions ----

const QUICK: &str = "testdata/nway/quick.txt";
const HEAP: &str = "testdata/nway/heap.txt";
const MERGE: &str = "testdata/nway/merge.txt";

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn tmp(test: &str, name: &str, text: &str) -> String {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("matrix-cli-{test}"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

fn labelled(label: &str, path: &str) -> String {
    format!("{label}={path}")
}

/// Args for the three nway variants with `quick` as the declared reference.
fn three(extra: &[&str]) -> Output {
    let (q, h, m) = (
        labelled("quick", QUICK),
        labelled("heap", HEAP),
        labelled("merge", MERGE),
    );
    let mut args = vec![
        "--variant",
        &q,
        "--variant",
        &h,
        "--variant",
        &m,
        "--reference",
        "quick",
    ];
    args.extend(extra);
    run(&args)
}

/// Ten Go samples per (impl, size) with a per-impl speed so every cell is significant.
fn impl_file() -> String {
    let mut text = String::new();
    for (imp, ns) in [("quick", 100.0), ("heap", 150.0)] {
        for size in [10, 20] {
            for i in 0..10 {
                let v = ns * f64::from(size) / 10.0 + f64::from(i) * 0.1;
                text +=
                    &format!("BenchmarkSort/impl={imp}/size={size}-8 \t 1000 \t {v:.1} ns/op\n");
            }
        }
    }
    text
}

#[test]
fn cli_should_print_the_matrix_with_the_reference_first_when_given_three_variants() {
    let out = three(&[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let md = stdout(&out);
    assert!(md.starts_with("<!-- benchdiff:default -->"), "{md}");
    let header = md.lines().find(|l| l.starts_with("| Benchmark")).unwrap();
    let pos = |s: &str| header.find(s).unwrap_or_else(|| panic!("{s} in {header}"));
    assert!(
        pos("quick") < pos("heap") && pos("heap") < pos("merge"),
        "{header}"
    );
}

#[test]
fn cli_should_render_ratio_cells_when_cell_format_is_ratio() {
    let md = stdout(&three(&["--cell-format", "ratio"]));
    assert!(md.contains("0.69x** 🟢"), "{md}");
    assert!(
        !md.lines()
            .any(|l| l.starts_with('|') && l.contains(" ns/op** (")),
        "{md}"
    );
}

#[test]
fn cli_should_write_markdown_and_json_files_when_asked_for_a_matrix() {
    let md = tmp("files", "m.md", "");
    let js = tmp("files", "m.json", "");
    let out = three(&["--markdown", &md, "--json", &js]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).is_empty());
    assert!(std::fs::read_to_string(&md)
        .unwrap()
        .contains("benchdiff:default"));
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&js).unwrap()).unwrap();
    assert!(json.get("matrix").is_some(), "{json}");
    assert_eq!(json["failed"], false);
}

#[test]
fn cli_should_exit_one_with_a_failed_gate_line_when_a_cell_regresses_past_the_gate() {
    // Reversed roles: heap is the baseline, so quick-vs-heap swings can regress.
    let (a, b) = (labelled("heap", HEAP), labelled("quick", QUICK));
    let out = run(&[
        "--variant",
        &a,
        "--variant",
        &b,
        "--reference",
        "heap",
        "--fail-on-regression=0",
    ]);
    let out2 = run(&[
        "--variant",
        &b,
        "--variant",
        &a,
        "--reference",
        "quick",
        "--fail-on-regression=0",
    ]);
    let failed = [&out, &out2]
        .into_iter()
        .filter(|o| o.status.code() == Some(1))
        .count();
    assert!(failed >= 1, "{} / {}", stdout(&out), stdout(&out2));
    let o = if out.status.code() == Some(1) {
        out
    } else {
        out2
    };
    assert!(stdout(&o).contains("FAILED"), "{}", stdout(&o));
}

#[test]
fn cli_should_exit_zero_when_the_gate_is_far_above_every_regression() {
    let out = three(&["--fail-on-regression=100000"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).contains("passed"), "{}", stdout(&out));
}

#[test]
fn cli_should_exit_zero_without_a_gate_whatever_the_cells_are() {
    let out = three(&[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!stdout(&out).contains("FAILED"));
}

#[test]
fn cli_should_exit_two_when_a_variant_file_is_missing() {
    let (q, h) = (
        labelled("quick", QUICK),
        labelled("heap", "nowhere/heap.txt"),
    );
    let out = run(&["--variant", &q, "--variant", &h]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("nowhere/heap.txt"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn cli_should_drop_an_empty_variant_with_a_notice_and_exit_two_when_strict() {
    let empty = tmp("empty", "heap.txt", "");
    let (q, h, m) = (
        labelled("quick", QUICK),
        labelled("heap", &empty),
        labelled("merge", MERGE),
    );
    let args = ["--variant", &q, "--variant", &h, "--variant", &m];
    let out = run(&args);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    // Markdown escapes `_` in the path, so match around it.
    assert!(
        stdout(&out).contains("variant 'heap' produced no data ("),
        "{}",
        stdout(&out)
    );
    assert!(
        stdout(&out).contains("heap.txt); comparing quick and merge"),
        "{}",
        stdout(&out)
    );
    let header = stdout(&out)
        .lines()
        .find(|l| l.starts_with("| Benchmark"))
        .unwrap()
        .to_string();
    assert!(!header.contains("heap"), "{header}");
    let mut strict = args.to_vec();
    strict.push("--dimensions-strict");
    let out = run(&strict);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains(&format!("variant 'heap' produced no data ({empty})")),
        "{}",
        stderr(&out)
    );
}

#[test]
fn cli_should_post_a_notice_only_report_and_skip_the_gate_when_the_reference_has_no_data() {
    let empty = tmp("noref", "quick.txt", "");
    let (q, h) = (labelled("quick", &empty), labelled("heap", HEAP));
    let args = [
        "--variant",
        &q,
        "--variant",
        &h,
        "--reference",
        "quick",
        "--fail-on-regression=10",
    ];
    let out = run(&args);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let md = stdout(&out);
    assert!(
        md.starts_with("<!-- benchdiff:default -->\n### Benchmark comparison"),
        "{md}"
    );
    assert!(
        md.contains("⚠️ reference 'quick' produced no data; no comparison made"),
        "{md}"
    );
    assert!(!md.contains("| Benchmark"));
    let mut strict = args.to_vec();
    strict.push("--dimensions-strict");
    assert_eq!(run(&strict).status.code(), Some(2));
}

#[test]
fn cli_should_post_a_notice_only_report_when_fewer_than_two_variants_have_data() {
    let empty = tmp("one", "heap.txt", "");
    let (q, h) = (labelled("quick", QUICK), labelled("heap", &empty));
    let out = run(&["--variant", &q, "--variant", &h]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("⚠️ variant 'heap' produced no data"),
        "{}",
        stdout(&out)
    );
    assert!(!stdout(&out).contains("| Benchmark"));
}

#[test]
fn cli_should_fall_back_to_the_two_way_report_when_compare_by_axis_is_absent() {
    let args = ["--head", HEAD, "--base", BASE, "--compare-by", "impl"];
    let out = run(&args);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let md = stdout(&out);
    let want = "compare-by 'impl': no benchmark carries this dimension; available dimensions: n (2 values), group; showing the two-way report";
    assert!(md.contains(want), "{md}");
    assert!(md.contains("### Benchmark comparison"), "{md}");
    let mut strict = args.to_vec();
    strict.push("--dimensions-strict");
    let out = run(&strict);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("compare-by 'impl': no benchmark carries this dimension; available dimensions: n (2 values), group"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn cli_should_fall_back_to_a_head_only_report_when_compare_by_axis_is_absent_without_base() {
    let out = run(&["--head", HEAD, "--compare-by", "impl"]);
    // `--head` alone is a head-only two-way input, so it falls back too.
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).contains("no benchmark carries this dimension"));
}

#[test]
fn cli_should_split_one_input_into_a_matrix_when_compare_by_names_a_carried_dimension() {
    let file = tmp("split", "impl.txt", &impl_file());
    let out = run(&[
        "--head",
        &file,
        "--compare-by",
        "impl",
        "--reference",
        "quick",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let md = stdout(&out);
    let header = md.lines().find(|l| l.starts_with("| Benchmark")).unwrap();
    assert!(
        header.contains("quick") && header.contains("heap"),
        "{header}"
    );
    assert!(md.contains("size=10") && md.contains("size=20"), "{md}");
}

#[test]
fn cli_should_gate_a_compare_by_run_against_the_explicit_reference() {
    let file = tmp("split-gate", "impl.txt", &impl_file());
    // heap is 1.5x slower than quick: a regression against quick.
    let out = run(&[
        "--head",
        &file,
        "--compare-by",
        "impl",
        "--reference",
        "quick",
        "--fail-on-regression=10",
    ]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
}

#[test]
fn cli_should_drop_a_zero_match_rule_with_a_notice_and_exit_two_when_strict() {
    let rule = "impl=/impl=(?P<impl>\\w+)";
    let args = ["--dimensions", rule];
    let (q, h) = (labelled("quick", QUICK), labelled("heap", HEAP));
    let mut full = vec!["--variant", &q, "--variant", &h];
    full.extend(args);
    let out = run(&full);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).contains("⚠️"), "{}", stdout(&out));
    assert!(stdout(&out).contains("| Benchmark"), "{}", stdout(&out));
    full.push("--dimensions-strict");
    assert_eq!(run(&full).status.code(), Some(2));
}

#[test]
fn cli_should_print_dimensions_as_json_without_a_report() {
    let file = tmp("dims", "impl.txt", &impl_file());
    let out = run(&["--head", &file, "--print-dimensions"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let dims = v["dimensions"].as_array().unwrap();
    let impl_dim = dims.iter().find(|d| d["name"] == "impl").unwrap();
    assert_eq!(impl_dim["values"], serde_json::json!(["quick", "heap"]));
    assert_eq!(impl_dim["benchmarks"], 4);
    assert_eq!(v["pseudo"], serde_json::json!(["group"]));
    assert_eq!(v["rules"], "default");
    assert_eq!(v["unmatched"], 0);
    assert!(v.get("hint").is_none());
}

#[test]
fn cli_should_hint_the_zero_config_rules_when_names_carry_no_dimension() {
    let file = tmp(
        "dims-none",
        "p.txt",
        "BenchmarkPlain-8 \t 10 \t 5.0 ns/op\n",
    );
    let out = run(&[
        "--head",
        &file,
        "--print-dimensions",
        "--dimensions",
        "x=/x(?P<x>\\d)",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(v["dimensions"], serde_json::json!([]));
    assert_eq!(v["rules"], "custom (1)");
    assert_eq!(v["unmatched"], 1);
    assert!(v["hint"].as_str().unwrap().contains("key=value"), "{v}");
}

#[test]
fn cli_should_print_dimensions_for_variant_files_too() {
    let (q, h) = (labelled("quick", QUICK), labelled("heap", HEAP));
    let out = run(&["--variant", &q, "--variant", &h, "--print-dimensions"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert!(
        v["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["name"] == "n"),
        "{v}"
    );
}

#[test]
fn cli_should_cap_dimension_values_at_twenty_and_flag_the_truncation() {
    let mut text = String::new();
    for i in 0..25 {
        text += &format!("BenchmarkS/n={i}-8 \t 10 \t 5.0 ns/op\n");
    }
    let file = tmp("dims-cap", "n.txt", &text);
    let out = run(&["--head", &file, "--print-dimensions"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let n = &v["dimensions"][0];
    assert_eq!(n["values"].as_array().unwrap().len(), 20);
    assert_eq!(n["values-truncated"], true);
}

// ---- Phase 5 review fixes: matrix thresholds, --base with --compare-by, union dimensions ----

fn config(test: &str, json: &str) -> String {
    tmp(test, "c.json", json)
}

/// Data rows whose Quick-vs-other cell carries a verdict emoji.
fn verdict_rows(md: &str) -> usize {
    md.lines()
        .filter(|l| l.starts_with("| `") && (l.contains('🔴') || l.contains('🟢')))
        .count()
}

fn two_variants(extra: &[&str]) -> Output {
    let (q, h) = (labelled("quick", QUICK), labelled("heap", HEAP));
    let mut args = vec!["--variant", &q, "--variant", &h];
    args.extend_from_slice(extra);
    run(&args)
}

#[test]
fn cli_should_apply_config_thresholds_to_variant_matrix_by_group_residual_then_group() {
    let none = config("thr-var-0", r#"{"version":1}"#);
    assert_eq!(
        verdict_rows(&stdout(&two_variants(&["--config", &none]))),
        4
    );
    let group = config(
        "thr-var-1",
        r#"{"version":1,"thresholds":{"sortbench":1000}}"#,
    );
    assert_eq!(
        verdict_rows(&stdout(&two_variants(&["--config", &group]))),
        0
    );
    let exact = config(
        "thr-var-2",
        r#"{"version":1,"thresholds":{"sortbench/Sort":1000}}"#,
    );
    let out = two_variants(&["--config", &exact]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(verdict_rows(&stdout(&out)), 2, "{}", stdout(&out));
    // The exact key beats the group key.
    let both = config(
        "thr-var-3",
        r#"{"version":1,"thresholds":{"sortbench":1000,"sortbench/Sort":1}}"#,
    );
    assert_eq!(
        verdict_rows(&stdout(&two_variants(&["--config", &both]))),
        2
    );
}

#[test]
fn cli_should_apply_config_thresholds_to_compare_by_matrix_on_the_residual_name() {
    let file = tmp("thr-cmp", "impl.txt", &impl_file());
    let cfg = config("thr-cmp", r#"{"version":1,"thresholds":{"Sort":80}}"#);
    let out = run(&["--head", &file, "--compare-by", "impl", "--config", &cfg]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(verdict_rows(&stdout(&out)), 0, "{}", stdout(&out));
    assert!(
        !stdout(&out).contains("matches no benchmark"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn cli_should_apply_the_reference_group_threshold_when_compare_by_group() {
    let mut text = String::new();
    for (pkg, ns) in [("pkgA", 100.0), ("pkgB", 112.0)] {
        text += &format!("pkg: {pkg}\n");
        for i in 0..10 {
            text += &format!(
                "BenchmarkSort-8 \t 1000 \t {:.1} ns/op\n",
                ns + f64::from(i) * 0.1
            );
        }
    }
    let file = tmp("thr-grp", "g.txt", &text);
    let by_group = |cfg: &str| {
        let c = config("thr-grp", cfg);
        stdout(&run(&[
            "--head",
            &file,
            "--compare-by",
            "group",
            "--config",
            &c,
        ]))
    };
    assert_eq!(verdict_rows(&by_group(r#"{"version":1}"#)), 1);
    assert_eq!(
        verdict_rows(&by_group(r#"{"version":1,"thresholds":{"pkgA":20}}"#)),
        0
    );
    assert_eq!(
        verdict_rows(&by_group(r#"{"version":1,"thresholds":{"pkgB":20}}"#)),
        1
    );
}

#[test]
fn cli_should_notice_a_threshold_key_that_matches_no_matrix_benchmark() {
    let cfg = config(
        "thr-notice",
        r#"{"version":1,"thresholds":{"Sort/impl=quick":20,"sortbench/Sort":9,"sortbench":8,"Nope":3}}"#,
    );
    let out = two_variants(&["--config", &cfg]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let md = stdout(&out);
    let text = |k: &str| {
        format!(
            "threshold key '{k}' matches no benchmark (matrix keys are group/residual or group)"
        )
    };
    assert!(md.contains(&text("Sort/impl=quick")), "{md}");
    assert!(md.contains(&text("Nope")), "{md}");
    assert!(!md.contains(&text("sortbench/Sort")), "{md}");
    assert!(!md.contains(&text("sortbench")), "{md}");
    let one = config(
        "thr-notice-strict",
        r#"{"version":1,"thresholds":{"Sort/impl=quick":20}}"#,
    );
    let strict = two_variants(&["--config", &one, "--dimensions-strict"]);
    assert_eq!(strict.status.code(), Some(2));
    assert!(
        stderr(&strict).contains(&text("Sort/impl=quick")),
        "{}",
        stderr(&strict)
    );
}

#[test]
fn cli_should_notice_that_base_is_ignored_when_compare_by_builds_the_matrix() {
    let file = tmp("base-ignored", "impl.txt", &impl_file());
    let out = run(&["--head", &file, "--base", BASE, "--compare-by", "impl"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let md = stdout(&out);
    assert!(md.contains("| Benchmark"), "{md}");
    assert!(
        md.contains(
            "--base is ignored: the matrix is built from --head split by compare-by 'impl'; \
             --base only feeds the two-way fallback"
        ),
        "{md}"
    );
    let plain = run(&["--head", &file, "--compare-by", "impl"]);
    assert!(!stdout(&plain).contains("--base is ignored"));
}

#[test]
fn cli_should_union_print_dimensions_across_all_variant_files() {
    let a = tmp(
        "dims-union",
        "a.txt",
        "BenchmarkS/impl=quick-8 \t 10 \t 5.0 ns/op\nBenchmarkS/impl=heap-8 \t 10 \t 5.0 ns/op\n",
    );
    let b = tmp(
        "dims-union",
        "b.txt",
        "BenchmarkS/impl=merge-8 \t 10 \t 5.0 ns/op\nBenchmarkS/impl=heap/size=3-8 \t 10 \t 5.0 ns/op\n",
    );
    let (va, vb) = (labelled("a", &a), labelled("b", &b));
    let out = run(&["--variant", &va, "--variant", &vb, "--print-dimensions"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let dim = |n: &str| {
        v["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == n)
            .cloned()
    };
    let imp = dim("impl").unwrap_or_else(|| panic!("{v}"));
    assert_eq!(imp["values"], serde_json::json!(["quick", "heap", "merge"]));
    assert_eq!(imp["benchmarks"], 4);
    assert!(dim("size").is_some(), "dimension only in the 2nd file: {v}");
}

#[test]
fn cli_should_pivot_same_input_on_impl_then_size() {
    let text = impl_file();
    let file = tmp("pivot", "impl.txt", &text);
    let header_of = |dim: &str| {
        let out = run(&["--head", &file, "--compare-by", dim]);
        assert_eq!(out.status.code(), Some(0), "{dim}: {}", stderr(&out));
        let md = stdout(&out);
        md.lines()
            .find(|l| l.starts_with("| Benchmark"))
            .unwrap_or_else(|| panic!("{dim}: no matrix header:\n{md}"))
            .to_string()
    };
    let by_impl = header_of("impl");
    let by_size = header_of("size");
    assert!(
        by_impl.contains("quick") && by_impl.contains("heap"),
        "{by_impl}"
    );
    assert!(
        by_size.contains("10") && by_size.contains("20"),
        "{by_size}"
    );
    assert_ne!(by_impl, by_size);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        text,
        "input unchanged"
    );
}

#[test]
fn cli_should_write_ranking_svg_when_variants_are_charted_with_svg_dir() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("matrix-cli-ranking-svg");
    let _ = std::fs::remove_dir_all(&dir);
    let dir_arg = dir.to_string_lossy().into_owned();
    let (q, h, m) = (
        labelled("quick", QUICK),
        labelled("heap", HEAP),
        labelled("merge", MERGE),
    );
    let out = run(&[
        "--variant",
        &q,
        "--variant",
        &h,
        "--variant",
        &m,
        "--charts",
        "ranking",
        "--svg-dir",
        &dir_arg,
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let md = stdout(&out);
    let header = md.lines().find(|l| l.starts_with("| Benchmark")).unwrap();
    assert!(
        header.contains("quick") && header.contains("merge"),
        "{header}"
    );
    assert!(dir.join("ranking.svg").is_file(), "ranking.svg in {dir:?}");
}

fn run_with_outputs(test: &str, args: &[&str]) -> (Output, String) {
    let js = tmp(test, "out.json", "");
    let mut all = args.to_vec();
    all.extend(["--json", &js]);
    let out = run(&all);
    (out, std::fs::read_to_string(&js).unwrap_or_default())
}

#[test]
fn cli_should_accept_compare_by_run_equivalent_to_base_head_when_two_columns() {
    let (axis, axis_json) = run_with_outputs(
        "run-axis",
        &["--base", BASE, "--head", HEAD, "--compare-by", "run"],
    );
    let (var, var_json) = run_with_outputs(
        "run-variants",
        &[
            "--variant",
            &labelled("base", BASE),
            "--variant",
            &labelled("head", HEAD),
        ],
    );
    assert_eq!(axis.status.code(), Some(0), "{}", stderr(&axis));
    assert_eq!(axis.status.code(), var.status.code());
    assert_eq!(stdout(&axis), stdout(&var));
    assert!(!axis_json.is_empty());
    assert_eq!(axis_json, var_json);
    let md = stdout(&axis);
    let header = md.lines().find(|l| l.starts_with("| Benchmark")).unwrap();
    let (b, h) = (header.find("base").unwrap(), header.find("head").unwrap());
    assert!(b < h, "base is the first column: {header}");
}

#[test]
fn cli_should_take_head_as_the_reference_when_compare_by_run_names_it() {
    let out = run(&[
        "--base",
        BASE,
        "--head",
        HEAD,
        "--compare-by",
        "run",
        "--reference",
        "head",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let md = stdout(&out);
    let header = md.lines().find(|l| l.starts_with("| Benchmark")).unwrap();
    assert!(
        header.find("head").unwrap() < header.find("base").unwrap(),
        "{header}"
    );
}

#[test]
fn cli_should_reject_compare_by_run_without_base_or_head() {
    let want = "compare-by 'run' needs --base and --head";
    rejects(&["--head", HEAD, "--compare-by", "run"], want);
    rejects(
        &[
            "--base",
            BASE,
            "--head",
            HEAD,
            "--head",
            HEAD,
            "--compare-by",
            "run",
        ],
        "compare-by 'run' takes one --base and one --head file",
    );
}

#[test]
fn cli_should_reject_compare_by_run_with_variant_and_an_unknown_reference() {
    rejects(
        &[
            "--variant",
            "a=x.txt",
            "--base",
            BASE,
            "--head",
            HEAD,
            "--compare-by",
            "run",
        ],
        "--variant cannot be combined with --base/--head",
    );
    rejects(
        &[
            "--base",
            BASE,
            "--head",
            HEAD,
            "--compare-by",
            "run",
            "--reference",
            "nope",
        ],
        "unknown reference 'nope'; valid: base, head",
    );
}
