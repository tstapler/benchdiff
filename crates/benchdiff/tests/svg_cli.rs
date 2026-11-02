//! Chart flags through the real binary: help text and static validation (3.3.1a).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const HEAD: &str = "testdata/golden/head.go.txt";

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

fn help_entry(help: &str, flag: &str) -> String {
    let lines: Vec<&str> = help.lines().collect();
    let i = lines
        .iter()
        .position(|l| l.trim_start().starts_with(flag))
        .unwrap_or_else(|| panic!("{flag} missing from help:\n{help}"));
    let mut entry = lines[i].to_string();
    for l in &lines[i + 1..] {
        if l.starts_with("                ") {
            entry.push(' ');
            entry.push_str(l.trim());
        } else {
            break;
        }
    }
    entry
}

#[test]
fn help_should_list_chart_flags_with_default_and_allowed_values_and_state_effective_cap() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let help = stdout(&out);
    for (flag, default, allowed) in [
        ("--charts", "[none]", "delta"),
        ("--chart-rows", "[30]", "200"),
        ("--chart-width", "[760]", "1600"),
        ("--chart-theme", "[light]", "both"),
        ("--svg-dir", "[off]", "DIR"),
        ("--svg-embed", "[false", "auto"),
        ("--publish-images", "[true]", "false"),
    ] {
        let e = help_entry(&help, flag);
        assert!(e.contains(default) && e.contains(allowed), "{flag}: {e}");
    }
    assert!(help_entry(&help, "--chart-rows").contains("min(chart-rows, max-rows)"));
}

#[test]
fn chart_flags_should_fail_with_valid_values_listed_when_invalid() {
    for (args, want) in [
        (
            vec!["--charts", "bars"],
            r#"--charts: "bars" is not valid; use none, delta, ranking, distribution (comma-separated)"#,
        ),
        (
            vec!["--chart-rows", "0"],
            "--chart-rows: 0 is not valid; use 1 to 200",
        ),
        (
            vec!["--chart-width=100"],
            "--chart-width: 100 is not valid; use 480 to 1600",
        ),
        (
            vec!["--chart-theme", "neon"],
            r#"--chart-theme: "neon" is not valid; use light, dark, both"#,
        ),
        (
            vec!["--svg-embed=yes"],
            r#"--svg-embed: "yes" is not valid; use true, false, auto"#,
        ),
        (
            vec!["--publish-images=yes"],
            r#"--publish-images: "yes" is not valid; use true, false"#,
        ),
    ] {
        let mut a = vec!["--head", HEAD];
        a.extend(&args);
        let out = run(&a);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(stderr(&out).contains(want), "{args:?}: {}", stderr(&out));
        assert!(stdout(&out).is_empty());
    }
}

#[test]
fn chart_flags_should_leave_report_unchanged_when_accepted() {
    let plain = run(&["--head", HEAD]);
    let flagged = run(&[
        "--head",
        HEAD,
        "--charts",
        "delta,ranking",
        "--chart-rows",
        "10",
        "--chart-width=900",
        "--chart-theme",
        "dark",
        "--svg-embed",
        "auto",
        "--publish-images=false",
    ]);
    assert!(flagged.status.success(), "{}", stderr(&flagged));
    assert_eq!(stdout(&plain), stdout(&flagged));
}

#[test]
fn bare_svg_embed_and_publish_images_should_parse_as_true() {
    let out = run(&["--head", HEAD, "--svg-embed", "--publish-images"]);
    assert!(out.status.success(), "{}", stderr(&out));
}
