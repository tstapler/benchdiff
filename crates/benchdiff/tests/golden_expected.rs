//! Asserts the CLI's `--markdown` / `--json` output against `testdata/golden/expected/`.
//! `UPDATE_GOLDEN=1` rewrites the expected files; review any resulting diff as a behaviour change.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn run_case(name: &str, args: &[&str]) {
    let root = root();
    let out = std::env::temp_dir().join(format!("bd-golden-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    let (md, json) = (out.join("report.md"), out.join("report.json"));
    let status = Command::new(env!("CARGO_BIN_EXE_benchdiff"))
        .current_dir(&root)
        .args(args)
        .arg("--markdown")
        .arg(&md)
        .arg("--json")
        .arg(&json)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "case `{name}`: benchdiff exited with {status}"
    );

    let expected = root.join("testdata/golden/expected");
    for (actual, ext) in [(&md, "md"), (&json, "json")] {
        let want = expected.join(format!("{name}.{ext}"));
        let got = std::fs::read_to_string(actual).unwrap();
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::write(&want, &got).unwrap();
        } else {
            if let Err(msg) = compare_golden(name, ext, &got, &want) {
                panic!("{msg}");
            }
        }
    }
    let _ = std::fs::remove_dir_all(out);
}

/// Compares `got` with the frozen file `want`; the error names the case.
fn compare_golden(name: &str, ext: &str, got: &str, want: &Path) -> Result<(), String> {
    let frozen = std::fs::read_to_string(want)
        .map_err(|e| format!("case `{name}`: {}: {e}", want.display()))?;
    if frozen == got {
        Ok(())
    } else {
        Err(format!("case `{name}`: {name}.{ext} differs from testdata/golden/expected (UPDATE_GOLDEN=1 to refresh)"))
    }
}

#[test]
fn golden_should_fail_naming_case_when_markdown_header_mutated() {
    let want = root().join("testdata/golden/expected/default.md");
    let frozen = std::fs::read_to_string(&want).unwrap();
    let dir = std::env::temp_dir().join(format!("bd-golden-mutated-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mutated = dir.join("default.md");
    std::fs::write(&mutated, frozen.replacen("### ", "## ", 1)).unwrap();
    let err = compare_golden("default", "md", &frozen, &mutated).unwrap_err();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(err.contains("`default`"), "{err}");
    assert!(compare_golden("default", "md", &frozen, &want).is_ok());
}

const HEAD: &str = "testdata/golden/head.go.txt";
const BASE: &str = "testdata/golden/base.go.txt";

macro_rules! option_case {
    ($test:ident, $name:literal, [$($extra:literal),*]) => {
        #[test]
        fn $test() {
            run_case($name, &["--head", HEAD, "--base", BASE, $($extra),*]);
        }
    };
}

macro_rules! same_file_case {
    ($test:ident, $name:literal, $file:literal, $format:literal) => {
        #[test]
        fn $test() {
            let file = concat!("testdata/", $file);
            run_case(
                $name,
                &["--head", file, "--base", file, "--format", $format],
            );
        }
    };
}

option_case!(default, "default", []);
option_case!(json, "json", ["--format", "go"]);
option_case!(only_changed, "only-changed", ["--only-changed"]);
option_case!(max_rows_2, "max-rows-2", ["--max-rows", "2"]);
option_case!(sort_name, "sort-name", ["--sort", "name"]);
option_case!(
    threshold_1_min_abs_5,
    "threshold-1-min-abs-5",
    ["--threshold", "1", "--min-abs", "5"]
);

same_file_case!(
    native_base,
    "native-base",
    "benchdiff/native.json",
    "benchdiff"
);
same_file_case!(importer_bmf, "importer-bmf", "bmf/bmf.json", "bmf");
same_file_case!(
    importer_cargo_bench,
    "importer-cargo-bench",
    "cargo-bench/bench.txt",
    "cargo-bench"
);
same_file_case!(
    importer_criterion,
    "importer-criterion",
    "criterion/criterion.jsonl",
    "criterion"
);
same_file_case!(
    importer_gab_smaller,
    "importer-gab-smaller",
    "gab/custom.json",
    "gab-smaller"
);
same_file_case!(
    importer_gab_bigger,
    "importer-gab-bigger",
    "gab/custom.json",
    "gab-bigger"
);
same_file_case!(
    importer_googlebench,
    "importer-googlebench",
    "googlebench/googlebench.json",
    "googlebench"
);
same_file_case!(
    importer_hyperfine,
    "importer-hyperfine",
    "hyperfine/hyperfine.json",
    "hyperfine"
);
same_file_case!(importer_jmh, "importer-jmh", "jmh/jmh.json", "jmh");
