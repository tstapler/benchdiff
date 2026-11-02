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
