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
    let md = stdout(&flagged);
    assert!(md.contains("chart skipped: nothing to compare"), "{md}");
    assert!(
        !md.contains("benchdiff-svg:"),
        "no placeholder without --svg-embed true"
    );
    let words = |s: &str| -> Vec<String> {
        s.lines()
            .filter(|l| !l.is_empty() && !l.contains("chart skipped"))
            .map(String::from)
            .collect()
    };
    assert_eq!(words(&md), words(&stdout(&plain)));
}

#[test]
fn bare_svg_embed_and_publish_images_should_parse_as_true() {
    let out = run(&["--head", HEAD, "--svg-embed", "--publish-images"]);
    assert!(out.status.success(), "{}", stderr(&out));
}

const BASE: &str = "testdata/golden/base.go.txt";
const NWAY: [&str; 6] = [
    "--variant",
    "quick=testdata/nway/quick.txt",
    "--variant",
    "heap=testdata/nway/heap.txt",
    "--variant",
    "merge=testdata/nway/merge.txt",
];

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("benchdiff-svg-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn two_way(extra: &[&str]) -> Output {
    let mut a = vec!["--head", HEAD, "--base", BASE];
    a.extend(extra);
    run(&a)
}

#[test]
fn svg_dir_should_write_files_and_list_them_on_stderr_when_charts_requested() {
    let dir = scratch("write");
    let d = dir.to_str().unwrap();
    let out = two_way(&["--charts", "delta", "--svg-dir", d]);
    assert!(out.status.success(), "{}", stderr(&out));
    let svg = std::fs::read_to_string(dir.join("delta.svg")).unwrap();
    assert!(svg.starts_with("<svg"), "{svg}");
    assert!(stderr(&out).contains(&format!("wrote {d}/delta.svg")));
    assert!(stderr(&out).contains("charts: produced=1 skipped=0"));
    let md = stdout(&out);
    assert!(!md.contains("wrote") && !md.contains("<svg"), "{md}");
    assert!(!md.contains("benchdiff-svg:"));
}

#[test]
fn svg_embed_should_put_placeholder_and_text_block_in_stdout_when_embedding() {
    let dir = scratch("embed");
    let out = two_way(&[
        "--charts",
        "delta",
        "--svg-dir",
        dir.to_str().unwrap(),
        "--svg-embed",
    ]);
    let md = stdout(&out);
    assert!(md.contains("](benchdiff-svg:delta.svg)"), "{md}");
    assert!(
        md.contains("<details><summary>Chart data (text)</summary>"),
        "{md}"
    );
    assert!(md.find("benchdiff-svg:").unwrap() < md.find("<details>").unwrap());
}

#[test]
fn svg_embed_auto_should_embed_only_when_charts_are_requested() {
    let auto = stdout(&two_way(&["--charts", "delta", "--svg-embed", "auto"]));
    assert!(auto.contains("benchdiff-svg:delta.svg"), "{auto}");
    let no_charts = stdout(&two_way(&["--svg-embed", "auto"]));
    assert!(!no_charts.contains("benchdiff-svg:"));
    let plain = stdout(&two_way(&["--charts", "delta"]));
    assert!(!plain.contains("benchdiff-svg:") && !plain.contains("Chart data"));
}

#[test]
fn publish_images_false_should_note_it_and_open_the_text_block_but_still_write_files() {
    let dir = scratch("nopublish");
    let d = dir.to_str().unwrap();
    let out = two_way(&[
        "--charts",
        "delta",
        "--svg-dir",
        d,
        "--svg-embed",
        "--publish-images=false",
    ]);
    let md = stdout(&out);
    assert!(
        md.contains(&format!(
            "charts not embedded: publish-images is false; SVGs written to {d}"
        )),
        "{md}"
    );
    assert!(
        !md.contains("benchdiff-svg:") && md.contains("<details open>"),
        "{md}"
    );
    assert!(dir.join("delta.svg").exists());
}

#[test]
fn unwritable_svg_dir_should_print_report_then_exit_2_with_cause() {
    let out = two_way(&[
        "--charts",
        "delta",
        "--svg-dir",
        "/proc/nonexistent-ro/out",
        "--svg-embed",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let md = stdout(&out);
    assert!(md.contains("### "), "report must still print: {md}");
    assert!(!md.contains("benchdiff-svg:"), "{md}");
    let err = stderr(&out);
    assert!(
        err.contains("cannot write /proc/nonexistent-ro/out: ")
            && err.contains("; report printed without charts"),
        "{err}"
    );
}

#[test]
fn charts_should_skip_not_applicable_kinds_and_say_so_when_report_kind_mismatches() {
    let out = two_way(&["--charts", "delta,ranking"]);
    let md = stdout(&out);
    assert!(
        md.contains("chart skipped: ranking needs an N-way run (compare-by or variants)"),
        "{md}"
    );
    assert!(stderr(&out).contains("charts: produced=1 skipped=1"));
    let mut a = NWAY.to_vec();
    a.extend(["--charts", "delta,ranking", "--svg-embed"]);
    let md = stdout(&run(&a));
    assert!(
        md.contains("chart skipped: delta is two-way; use ranking"),
        "{md}"
    );
    assert!(md.contains("benchdiff-svg:ranking.svg"), "{md}");
}

#[test]
fn distribution_should_keep_samples_and_render_when_inputs_have_repeats() {
    let dir = scratch("dist");
    let out = two_way(&[
        "--charts",
        "distribution",
        "--svg-dir",
        dir.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(dir.join("distribution.svg").exists(), "{}", stderr(&out));
}

#[test]
fn theme_both_should_write_light_and_dark_twins_and_emit_a_picture() {
    let dir = scratch("both");
    let out = two_way(&[
        "--charts",
        "delta",
        "--chart-theme",
        "both",
        "--svg-dir",
        dir.to_str().unwrap(),
        "--svg-embed",
    ]);
    assert!(dir.join("delta-light.svg").exists() && dir.join("delta-dark.svg").exists());
    let md = stdout(&out);
    assert!(
        md.contains("<picture>") && md.contains("delta-dark.svg"),
        "{md}"
    );
}

#[test]
fn written_files_should_be_byte_identical_across_runs() {
    let (a, b) = (scratch("det-a"), scratch("det-b"));
    for d in [&a, &b] {
        two_way(&["--charts", "delta", "--svg-dir", d.to_str().unwrap()]);
    }
    assert_eq!(
        std::fs::read(a.join("delta.svg")).unwrap(),
        std::fs::read(b.join("delta.svg")).unwrap()
    );
}

#[test]
fn chart_cap_should_add_a_notice_naming_the_limiting_key() {
    let md = stdout(&two_way(&["--charts", "delta", "--chart-rows", "1"]));
    assert!(md.contains("more not shown (chart-rows: 1)"), "{md}");
}
