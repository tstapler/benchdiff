//! Process-level failure modes: undecodable arguments, unwritable stdout, malformed variants.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const HEAD: &str = "testdata/golden/head.go.txt";
const QUICK: &str = "testdata/nway/quick.txt";
const MERGE: &str = "testdata/nway/merge.txt";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_benchdiff"));
    c.current_dir(root());
    c
}

fn run(args: &[&str]) -> Output {
    bin().args(args).output().unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn tmp(test: &str, name: &str, text: &str) -> String {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("robust-{test}"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

#[cfg(unix)]
#[test]
fn cli_should_exit_two_with_a_usage_error_when_an_argument_is_not_utf8() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let out = bin()
        .arg(OsStr::from_bytes(b"--head=\xff.txt"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("argument \"--head=\u{fffd}.txt\" is not valid UTF-8"),
        "{}",
        stderr(&out)
    );
}

#[cfg(unix)]
#[test]
fn cli_should_exit_two_naming_stdout_when_stdout_is_full() {
    let Ok(full) = std::fs::OpenOptions::new().write(true).open("/dev/full") else {
        return;
    };
    let out = bin()
        .args(["--head", HEAD])
        .stdout(Stdio::from(full))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("stdout: "), "{}", stderr(&out));
    assert!(!stderr(&out).contains("panicked"), "{}", stderr(&out));
}

#[cfg(unix)]
#[test]
fn cli_should_exit_zero_quietly_when_the_stdout_pipe_is_closed() {
    let mut child = bin()
        .args(["--head", HEAD])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(!stderr(&out).contains("panicked"), "{}", stderr(&out));
    assert_eq!(stderr(&out), "");
}

#[test]
fn cli_should_append_the_parse_error_when_a_variant_file_is_malformed() {
    let bad = tmp("bad", "heap.json", "{not json");
    let good = tmp("good", "good.json", &good_json());
    let args = [
        "--format",
        "benchdiff",
        "--variant",
        &format!("quick={good}"),
        "--variant",
        &format!("heap={bad}"),
        "--variant",
        &format!("merge={good}"),
    ];
    let out = run(&args);
    let md = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        md.contains("variant 'heap' produced no data (") && md.contains("): key must be a string"),
        "{md}"
    );
    let mut strict = args.to_vec();
    strict.push("--dimensions-strict");
    let out = run(&strict);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("): key must be a string"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn cli_should_keep_the_empty_file_notice_free_of_a_parse_error() {
    let empty = tmp("empty", "heap.txt", "");
    let out = run(&[
        "--variant",
        &format!("quick={QUICK}"),
        "--variant",
        &format!("heap={empty}"),
        "--variant",
        &format!("merge={MERGE}"),
        "--dimensions-strict",
    ]);
    assert!(
        stderr(&out)
            .trim_end()
            .ends_with(&format!("variant 'heap' produced no data ({empty})")),
        "{}",
        stderr(&out)
    );
}

#[test]
fn cli_should_report_the_parse_error_of_a_malformed_variant_under_print_dimensions() {
    let bad = tmp("pd", "heap.json", "{not json");
    let out = run(&[
        "--format",
        "benchdiff",
        "--print-dimensions",
        "--variant",
        &format!("heap={bad}"),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("variant 'heap' produced no data ("),
        "{}",
        stderr(&out)
    );
}

#[test]
fn cli_should_reject_empty_values_for_path_flags() {
    for flag in [
        "--config",
        "--svg-dir",
        "--markdown",
        "--json",
        "--head-json",
    ] {
        let out = run(&["--head", HEAD, &format!("{flag}=")]);
        assert_eq!(out.status.code(), Some(2), "{flag}");
        assert!(
            stderr(&out).contains(&format!("{flag}= needs a value")),
            "{flag}: {}",
            stderr(&out)
        );
    }
}

#[test]
fn cli_should_point_at_the_equals_form_when_a_value_starts_with_dashes() {
    let out = run(&["--head", HEAD, "--title", "-- x"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("--title=VALUE"), "{}", stderr(&out));
    let out = run(&["--head", HEAD, "--title=-- x"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

/// A valid benchdiff-JSON file made from the golden head.
fn good_json() -> String {
    let path = tmp("conv", "head.json", "");
    let out = run(&[
        "--head",
        HEAD,
        "--head-json",
        &path,
        "--markdown",
        &format!("{path}.md"),
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    std::fs::read_to_string(path).unwrap()
}
