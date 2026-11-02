//! `--config` layering, `--print-config` and `Summary.config_sources` through the real binary.

use serde_json::Value;
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

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

/// Writes `text` to a per-test file named `name` and returns its path.
fn config_file(test: &str, name: &str, text: &str) -> String {
    let dir = std::env::temp_dir().join(format!("bd-config-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

fn print_config(args: &[&str]) -> Value {
    let mut all = vec!["--print-config"];
    all.extend_from_slice(args);
    let out = run(&all);
    assert!(out.status.success(), "{}", stderr(&out));
    serde_json::from_str(&stdout(&out)).unwrap()
}

#[test]
fn config_file_should_set_threshold_when_no_flag() {
    let c = config_file("layer", "c.json", r#"{"version":1,"threshold":3}"#);
    let v = print_config(&["--config", &c]);
    assert_eq!(v["config"]["threshold"], 3.0);
    assert_eq!(v["sources"]["threshold"], "config");
    assert_eq!(v["config-file"], c.as_str());
}

#[test]
fn flag_should_override_config_file_when_both_set_threshold() {
    let c = config_file("flag", "c.json", r#"{"version":1,"threshold":3}"#);
    let v = print_config(&["--config", &c, "--threshold", "7"]);
    assert_eq!(v["config"]["threshold"], 7.0);
    assert_eq!(v["sources"], serde_json::json!({"threshold": "flag"}));
}

#[test]
fn print_config_should_list_defaults_without_sources_when_no_config() {
    let v = print_config(&[]);
    assert_eq!(v["config-file"], Value::Null);
    assert_eq!(v["config"]["threshold"], 5.0);
    assert_eq!(v["config"]["sort"], "delta");
    assert_eq!(v["config"]["format"], "auto");
    assert_eq!(v["sources"], serde_json::json!({}));
}

#[test]
fn print_config_should_show_chart_keys_with_config_and_flag_sources() {
    let c = config_file(
        "charts",
        "c.json",
        r#"{"version":1,"charts":"delta,ranking","chart-rows":12,"svg-embed":"true"}"#,
    );
    let v = print_config(&["--config", &c, "--chart-rows", "7", "--svg-dir", "out"]);
    assert_eq!(v["config"]["charts"], "delta,ranking");
    assert_eq!(v["config"]["chart-rows"], 7);
    assert_eq!(v["config"]["chart-width"], 760);
    assert_eq!(v["config"]["chart-theme"], "light");
    assert_eq!(v["config"]["svg-embed"], true);
    assert_eq!(v["config"]["svg-dir"], "out");
    assert_eq!(
        v["sources"],
        serde_json::json!({"charts": "config", "chart-rows": "flag", "svg-embed": "config", "svg-dir": "flag"})
    );
}

#[test]
fn print_config_should_not_render_a_report_or_need_head() {
    let out = run(&["--print-config", "--head", HEAD, "--base", BASE]);
    assert!(out.status.success());
    let v: Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert!(v["config"].is_object() && !stdout(&out).contains("# "));
}

#[test]
fn config_format_should_apply_when_flag_absent() {
    let c = config_file(
        "format",
        "c.json",
        r#"{"version":1,"format":"go","base-format":"go"}"#,
    );
    let out = run(&["--config", &c, "--head", HEAD, "--base", BASE]);
    assert!(out.status.success(), "{}", stderr(&out));
    let v = print_config(&["--config", &c]);
    assert_eq!(v["config"]["format"], "go");
    assert_eq!(v["sources"]["base-format"], "config");
}

#[test]
fn missing_config_path_should_exit_2_when_explicit() {
    let out = run(&["--config", "nope.json", "--head", HEAD]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("config file not found: nope.json"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn invalid_config_should_exit_2_with_file_name() {
    let c = config_file(
        "bad",
        "benchdiff.config.json",
        r#"{"version":1,"treshold":3}"#,
    );
    let out = run(&["--config", &c, "--head", HEAD]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("unknown key 'treshold'; did you mean 'threshold'?"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn method_note_should_list_sources_when_config_supplied() {
    let c = config_file("note", "c.json", r#"{"version":1,"threshold":3}"#);
    let json = config_file("note-json", "r.json", "");
    let out = run(&[
        "--config", &c, "--sort", "name", "--head", HEAD, "--base", BASE, "--json", &json,
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let md = stdout(&out);
    assert!(md.contains("threshold=3 (config)"), "{md}");
    assert!(md.contains("sort=name (flag)"), "{md}");
    let report: Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(
        report["summary"]["config_sources"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn report_should_be_unchanged_when_flags_only_and_no_config() {
    let plain = run(&["--head", HEAD, "--base", BASE]);
    let flagged = run(&["--head", HEAD, "--base", BASE, "--sort", "name"]);
    assert!(!stdout(&plain).contains("Settings not at their defaults"));
    assert!(!stdout(&flagged).contains("Settings not at their defaults"));
}

#[test]
fn binary_defaults_should_equal_removed_action_yml_defaults_table_with_explicit_exceptions() {
    // The pre-1.2.4a action.yml defaults; the Action no longer injects them, so the binary must.
    // No exceptions: the CLI title and tag defaults match the old Action's.
    let v = print_config(&[]);
    let c = &v["config"];
    assert_eq!(c["format"], "auto");
    assert_eq!(c["threshold"], 5.0);
    assert_eq!(c["min-abs"], 0.0);
    assert_eq!(c["sort"], "delta");
    assert_eq!(c["title"], "Benchmark comparison");
    assert_eq!(c["tag"], "default");
    assert_eq!(c["only-changed"], false);
    assert_eq!(c["fail-on-regression"], Value::Null);
    assert_eq!(c["max-rows"], Value::Null);
}

#[test]
fn no_only_changed_should_override_config_true() {
    let c = config_file("no-only", "c.json", r#"{"version":1,"only-changed":true}"#);
    let v = print_config(&["--config", &c]);
    assert_eq!(v["config"]["only-changed"], true);
    let v = print_config(&["--config", &c, "--no-only-changed"]);
    assert_eq!(v["config"]["only-changed"], false);
    assert_eq!(v["sources"]["only-changed"], "flag");
}

#[test]
fn no_fail_on_regression_should_override_config_limit() {
    let c = config_file(
        "no-fail",
        "c.json",
        r#"{"version":1,"fail-on-regression":9}"#,
    );
    let v = print_config(&["--config", &c]);
    assert_eq!(v["config"]["fail-on-regression"], 9.0);
    let v = print_config(&["--config", &c, "--no-fail-on-regression"]);
    assert_eq!(v["config"]["fail-on-regression"], Value::Null);
    assert_eq!(v["sources"]["fail-on-regression"], "flag");
}

#[test]
fn no_fail_on_regression_should_not_fail_run_that_config_would_fail() {
    let c = config_file(
        "no-fail-run",
        "c.json",
        r#"{"version":1,"fail-on-regression":0}"#,
    );
    let failing = run(&["--config", &c, "--head", HEAD, "--base", BASE]);
    assert_eq!(failing.status.code(), Some(1), "{}", stderr(&failing));
    let off = run(&[
        "--config",
        &c,
        "--no-fail-on-regression",
        "--head",
        HEAD,
        "--base",
        BASE,
    ]);
    assert!(off.status.success(), "{}", stderr(&off));
}

#[test]
fn conflicting_positive_and_negative_flags_should_exit_2_naming_both() {
    for (a, b) in [
        ("--only-changed", "--no-only-changed"),
        ("--fail-on-regression", "--no-fail-on-regression"),
        ("--fail-on-regression=5", "--no-fail-on-regression"),
    ] {
        for pair in [[a, b], [b, a]] {
            let out = run(&["--head", HEAD, pair[0], pair[1]]);
            assert_eq!(out.status.code(), Some(2), "{pair:?}");
            let e = stderr(&out);
            let name = |f: &str| f.split('=').next().unwrap().to_string();
            assert!(e.contains(&name(a)) && e.contains(&name(b)), "{e}");
        }
    }
}

#[test]
fn negative_flags_should_reject_inline_values() {
    let out = run(&["--head", HEAD, "--no-only-changed=true"]);
    assert_eq!(out.status.code(), Some(2));
}

fn config_error(test: &str, text: &str) -> String {
    let c = config_file(test, "c.json", text);
    let out = run(&["--config", &c, "--head", HEAD]);
    assert_eq!(out.status.code(), Some(2), "{text}: {}", stderr(&out));
    stderr(&out)
}

#[test]
fn print_config_should_show_matrix_keys_with_sources_when_config_and_flags_set() {
    let c = config_file(
        "matrix",
        "c.json",
        r#"{"version":1,"threshold":3,"sort":"rank","reference":"quick","cell-format":"ratio",
            "compare-by":"impl","dimensions":["impl=/impl=(?P<impl>\\w+)"]}"#,
    );
    let v = print_config(&[
        "--config",
        &c,
        "--reference",
        "heap",
        "--correction",
        "none",
    ]);
    let cfg = &v["config"];
    assert_eq!(cfg["sort"], "rank");
    assert_eq!(cfg["reference"], "heap");
    assert_eq!(cfg["cell-format"], "ratio");
    assert_eq!(cfg["compare-by"], "impl");
    assert_eq!(cfg["correction"], "none");
    assert_eq!(cfg["dimensions"].as_array().unwrap().len(), 1);
    assert_eq!(v["sources"]["reference"], "flag");
    assert_eq!(v["sources"]["cell-format"], "config");
    assert_eq!(v["sources"]["sort"], "config");
}

#[test]
fn print_config_should_leave_matrix_keys_null_when_not_set() {
    let v = print_config(&[]);
    assert_eq!(v["config"]["reference"], Value::Null);
    assert_eq!(v["config"]["compare-by"], Value::Null);
    assert_eq!(v["config"]["correction"], "holm");
    assert_eq!(v["config"]["cell-format"], "full");
}

#[test]
fn config_should_exit_two_for_each_static_matrix_key_error() {
    let cases = [
        (
            r#"{"version":1,"correction":"bonf"}"#,
            "correction: \"bonf\" is not valid; use holm, none",
        ),
        (
            r#"{"version":1,"cell-format":"wide"}"#,
            "cell-format: \"wide\" is not valid; use full, ratio",
        ),
        (
            r#"{"version":1,"compare-bi":"impl"}"#,
            "unknown key 'compare-bi'; did you mean 'compare-by'?",
        ),
        (r#"{"version":1,"strict":true}"#, "unknown key 'strict'"),
        (r#"{"version":1,"variant":"a=b"}"#, "unknown key 'variant'"),
        (
            r#"{"version":1,"compare-by":"measure"}"#,
            "compare-by 'measure' is not an axis",
        ),
        (
            r#"{"version":1,"dimensions":["impl=(?P<impl>"]}"#,
            "invalid pattern",
        ),
        (
            r#"{"version":1,"dimensions":["impl=impl=x"]}"#,
            "no named capture group",
        ),
        (
            r#"{"version":1,"reference":"bad label"}"#,
            "not a valid variant label",
        ),
        (
            r#"{"version":1,"dimensions-strict":"yes"}"#,
            "dimensions-strict",
        ),
    ];
    for (text, want) in cases {
        let e = config_error("static", text);
        assert!(e.contains(want), "{text}: {e}");
    }
}

#[test]
fn config_should_apply_the_same_run_shape_rules_as_flags() {
    let e = config_error("rank", r#"{"version":1,"sort":"rank"}"#);
    assert!(e.contains("--sort rank needs an N-way run"), "{e}");
    let e = config_error(
        "gate",
        r#"{"version":1,"compare-by":"impl","fail-on-regression":10}"#,
    );
    assert!(e.contains("needs an explicit 'reference'"), "{e}");
    // A flag moves the sort off rank, so the file's rank no longer applies.
    let c = config_file("rank-flag", "c.json", r#"{"version":1,"sort":"rank"}"#);
    let out = run(&[
        "--config", &c, "--sort", "name", "--head", HEAD, "--base", BASE,
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
}

#[test]
fn help_should_list_every_new_flag_with_default_and_allowed_values_from_key_table() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let help = stdout(&out);
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
    ] {
        assert!(help.contains(flag), "{flag}");
    }
    assert!(help.contains("allowed: holm, none") && help.contains("allowed: full, ratio"));
    assert!(help.contains("default"));
}

#[test]
fn config_should_drive_an_nway_run_and_label_its_sources_in_the_matrix_json() {
    let cfg = config_file(
        "nway",
        "benchdiff.config.json",
        r#"{"version":1,"reference":"quick","cell-format":"ratio"}"#,
    );
    let json_path = config_file("nway-json", "out.json", "");
    let out = run(&[
        "--config",
        &cfg,
        "--variant",
        "quick=testdata/nway/quick.txt",
        "--variant",
        "heap=testdata/nway/heap.txt",
        "--json",
        &json_path,
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let md = stdout(&out);
    assert!(
        md.contains("x 🟢") || md.contains("x ~") || md.contains("x 🔴"),
        "{md}"
    );
    let json: Value = serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
    let sources = json["matrix"]["summary"]["config_sources"].to_string();
    assert!(
        sources.contains("cell-format") && sources.contains("config"),
        "{sources}"
    );
}

#[test]
fn config_should_let_a_flag_override_the_file_in_an_nway_run() {
    let cfg = config_file(
        "nway-flag",
        "benchdiff.config.json",
        r#"{"version":1,"reference":"quick","cell-format":"ratio"}"#,
    );
    let out = run(&[
        "--config",
        &cfg,
        "--cell-format",
        "full",
        "--variant",
        "quick=testdata/nway/quick.txt",
        "--variant",
        "heap=testdata/nway/heap.txt",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(
        stdout(&out).contains(" ns/op** (0.69x)"),
        "{}",
        stdout(&out)
    );
}

/// Ten samples per side in two groups; head is 6% slower than base in both.
fn two_group_inputs(test: &str) -> (String, String) {
    let side = |ns: f64| {
        let mut text = String::new();
        for pkg in ["pkgA", "pkgB"] {
            text += &format!("pkg: {pkg}\n");
            for i in 0..10 {
                text += &format!(
                    "BenchmarkSort-8 \t 1000 \t {:.1} ns/op\n",
                    ns + f64::from(i) * 0.1
                );
            }
        }
        text
    };
    (
        config_file(test, "base.txt", &side(1000.0)),
        config_file(test, "head.txt", &side(1060.0)),
    )
}

fn two_way_summary(test: &str, config_json: Option<&str>) -> String {
    let (base, head) = two_group_inputs(test);
    let mut args = vec!["--base", &base, "--head", &head];
    let cfg;
    if let Some(json) = config_json {
        cfg = config_file(test, "c.json", json);
        args.extend(["--config", &cfg]);
    }
    let out = run(&args);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    stdout(&out)
        .lines()
        .find(|l| l.contains(" regressed"))
        .unwrap_or_else(|| panic!("no summary line"))
        .to_string()
}

#[test]
fn thresholds_should_clear_a_six_percent_regression_when_two_way_key_raises_it_to_ten() {
    assert!(two_way_summary("thr2-base", None).contains("2 regressed"));
    let exact = two_way_summary(
        "thr2-exact",
        Some(r#"{"version":1,"thresholds":{"pkgA/Sort":10,"pkgB/Sort":10}}"#),
    );
    assert!(exact.contains("0 regressed"), "{exact}");
}

#[test]
fn thresholds_should_affect_only_the_named_group_when_two_way_key_is_a_group() {
    let one = two_way_summary(
        "thr2-group",
        Some(r#"{"version":1,"thresholds":{"pkgA":10}}"#),
    );
    assert!(one.contains("1 regressed"), "{one}");
}

#[test]
fn config_file_should_pivot_the_runs_when_compare_by_is_run() {
    let c = config_file("run-axis", "c.json", r#"{"version":1,"compare-by":"run"}"#);
    let from_config = run(&["--config", &c, "--base", BASE, "--head", HEAD]);
    let from_flag = run(&["--base", BASE, "--head", HEAD, "--compare-by", "run"]);
    assert!(from_config.status.success(), "{}", stderr(&from_config));
    let md = stdout(&from_config);
    assert!(md.contains("2 variants (reference: base)"), "{md}");
    // The config run lists its sources; apart from that it is the flag run.
    assert!(md.contains("compare-by"), "{md}");
    assert!(stdout(&from_flag).contains("2 variants (reference: base)"));
}
