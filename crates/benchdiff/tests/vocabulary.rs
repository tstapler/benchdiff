//! One vocabulary: `action.yml` inputs, CLI `--help` flags and config keys stay in step.
//! Every name outside the one-to-one mapping is classified below; a stale entry fails too.

use benchdiff_core::config::KEYS;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Config-only names: a file can carry them, flags and inputs cannot.
const CONFIG_ONLY: &[&str] = &["version", "$schema", "thresholds"];

/// CLI flags with no Action input, and why.
const CLI_ONLY_FLAGS: &[&str] = &[
    "variant",          // Action: `variants`
    "print-config",     // diagnostics
    "print-dimensions", // diagnostics
    "help",             // diagnostics
    "alpha",            // statistics tuning, not exposed to the Action
    "no-stats",         // statistics tuning, not exposed to the Action
    "markdown", // the Action passes these itself; outputs report-path, json-path, head-json-path
    "json",
    "head-json",
    "no-only-changed", // negative forms of `only-changed` / `fail-on-regression`, emitted by buildArgs
    "no-fail-on-regression",
];

/// Action inputs with no CLI flag, and why.
const ACTION_ONLY_INPUTS: &[&str] = &[
    "comment",      // the comment upsert is done by index.js
    "github-token", // ditto
];

/// Action-only input that Story 4.2.3 adds (CLI: `--variant`); may be absent until then.
const OPTIONAL_ACTION_INPUTS: &[&str] = &["variants"];

/// `config` is a flag and an input but not a key: a file cannot name itself.
const NOT_A_KEY: &[&str] = &["config"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Top-level keys of a section (`inputs:` / `outputs:`) with their `description:` text.
fn section(name: &str) -> Vec<(String, String)> {
    let text = std::fs::read_to_string(root().join("action.yml")).unwrap();
    let mut in_section = false;
    let mut out: Vec<(String, String)> = vec![];
    for line in text.lines() {
        if !line.starts_with(' ') && !line.is_empty() {
            in_section = line.trim_end() == format!("{name}:");
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some(key) = line.strip_prefix("  ").and_then(|l| l.strip_suffix(':')) {
            if !key.starts_with(' ') {
                out.push((key.to_string(), String::new()));
            }
        } else if let Some(desc) = line.trim_start().strip_prefix("description:") {
            if let Some(last) = out.last_mut() {
                last.1 = desc.trim().to_string();
            }
        }
    }
    out
}

fn inputs() -> Vec<(String, String)> {
    section("inputs")
}

fn input_names() -> BTreeSet<String> {
    inputs().into_iter().map(|(k, _)| k).collect()
}

/// Flag names (without `--`) that begin an entry in the real `--help` text.
fn help_flags() -> BTreeSet<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_benchdiff"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8(out.stdout).unwrap();
    help.lines()
        .filter_map(|l| {
            let l = l.trim_start();
            let l = l.strip_prefix("-h, ").unwrap_or(l);
            let flag = l.strip_prefix("--")?;
            let end = flag
                .find(|c: char| !(c.is_ascii_lowercase() || c == '-'))
                .unwrap_or(flag.len());
            Some(flag[..end].to_string())
        })
        .collect()
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

fn assert_all_present(allowlist: &[&str], universe: &BTreeSet<String>, what: &str) {
    for name in allowlist {
        assert!(
            universe.contains(*name),
            "allowlisted `{name}` no longer exists in {what}; remove it from the allowlist"
        );
    }
}

#[test]
fn vocabulary_should_map_every_config_key_to_a_flag_and_an_input() {
    let flags = help_flags();
    let inputs = input_names();
    for key in KEYS.iter().filter(|k| !CONFIG_ONLY.contains(k)) {
        assert!(
            flags.contains(*key),
            "config key `{key}` has no CLI flag in --help"
        );
        assert!(
            inputs.contains(*key),
            "config key `{key}` has no Action input in action.yml"
        );
    }
    assert_all_present(
        CONFIG_ONLY,
        &KEYS.iter().map(|k| k.to_string()).collect(),
        "the config key table",
    );
}

#[test]
fn vocabulary_should_classify_every_flag_without_an_input() {
    let inputs = input_names();
    let flags = help_flags();
    let classified = set(CLI_ONLY_FLAGS);
    for flag in &flags {
        assert!(
            inputs.contains(flag) || classified.contains(flag),
            "CLI flag `--{flag}` has no Action input and is not in CLI_ONLY_FLAGS"
        );
    }
    assert_all_present(CLI_ONLY_FLAGS, &flags, "--help");
}

#[test]
fn vocabulary_should_classify_every_input_without_a_flag() {
    let inputs = input_names();
    let flags = help_flags();
    let classified: BTreeSet<String> = set(ACTION_ONLY_INPUTS)
        .union(&set(OPTIONAL_ACTION_INPUTS))
        .cloned()
        .collect();
    for input in &inputs {
        assert!(
            flags.contains(input) || classified.contains(input),
            "Action input `{input}` has no CLI flag and is not classified"
        );
    }
    assert_all_present(ACTION_ONLY_INPUTS, &inputs, "action.yml inputs");
    // `head`/`base` and the rest of the flag-backed inputs are checked by the loop above;
    // `config` must stay a real flag even though it is not a key.
    assert_all_present(NOT_A_KEY, &inputs, "action.yml inputs");
    assert_all_present(NOT_A_KEY, &flags, "--help");
    assert!(!KEYS.contains(&"config"));
}

#[test]
fn action_yml_should_describe_every_input() {
    for (name, desc) in inputs() {
        assert!(!desc.is_empty(), "input `{name}` has no description");
    }
}

#[test]
fn action_yml_should_describe_hosting_side_effects() {
    let inputs = inputs();
    for name in ["charts", "svg-embed", "publish-images"] {
        let desc = &inputs.iter().find(|(k, _)| k == name).unwrap().1;
        for needle in [
            "contents: write",
            "benchdiff-assets",
            "at most one commit",
            "never pruned",
            "anyone",
            "private repositories are never hosted",
        ] {
            assert!(
                desc.to_lowercase().contains(&needle.to_lowercase()),
                "`{name}` description must say `{needle}`"
            );
        }
    }
    let charts = &inputs.iter().find(|(k, _)| k == "charts").unwrap().1;
    assert!(
        charts.contains("pull-requests: write"),
        "`charts` must carry the permissions snippet"
    );
    assert!(
        charts.contains("read-only"),
        "`charts` must mention the read-only-token first run"
    );
}

#[test]
fn action_yml_should_declare_the_svg_outputs() {
    let outputs: BTreeSet<String> = section("outputs").into_iter().map(|(k, _)| k).collect();
    for o in ["svg-dir", "svg-paths", "incomparable", "invalid"] {
        assert!(outputs.contains(o), "output `{o}` missing");
    }
}

/// Enum-valued keys whose `--help` line has no `allowed:` list: (key, values, has a default).
const ENUMS_WITHOUT_ALLOWED_TEXT: &[(&str, &[&str], bool)] = &[
    ("sort", &["delta", "name", "input", "rank"], true),
    ("chart-theme", &["light", "dark", "both"], true),
    ("svg-embed", &["true", "false", "auto"], false),
    ("publish-images", &["true", "false"], true),
];

/// `(flag, values)` for every `--help` entry with an `allowed:` list of plain words, e.g.
/// `(default: holm; allowed: holm, none)`.
fn help_allowed_enums() -> Vec<(String, Vec<String>)> {
    let out = Command::new(env!("CARGO_BIN_EXE_benchdiff"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8(out.stdout).unwrap();
    let mut flag = String::new();
    let mut found = vec![];
    for line in help.lines() {
        let t = line.trim_start();
        if let Some(f) = t.strip_prefix("--") {
            flag = f
                .split(|c: char| !(c.is_ascii_lowercase() || c == '-'))
                .next()
                .unwrap()
                .to_string();
        }
        if let Some(rest) = t.split("allowed: ").nth(1) {
            let list = rest.trim_end_matches(')');
            let values: Vec<String> = list.split(", ").map(str::to_string).collect();
            let plain = values
                .iter()
                .all(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_lowercase() || c == '-'));
            if plain && values.len() > 1 {
                found.push((flag.clone(), values));
            }
        }
    }
    found
}

#[test]
fn ux_s4_1_action_inputs_should_name_every_allowed_value_and_the_default() {
    let inputs = inputs();
    let desc_of = |k: &str| {
        inputs
            .iter()
            .find(|(n, _)| n == k)
            .unwrap_or_else(|| panic!("no input `{k}`"))
            .1
            .to_lowercase()
    };
    let derived = help_allowed_enums();
    assert!(
        derived.iter().any(|(k, _)| k == "correction")
            && derived.iter().any(|(k, _)| k == "cell-format"),
        "expected correction and cell-format in --help allowed: lists: {derived:?}"
    );
    let mut cases: Vec<(String, Vec<String>, bool)> =
        derived.into_iter().map(|(k, v)| (k, v, true)).collect();
    for (k, v, d) in ENUMS_WITHOUT_ALLOWED_TEXT {
        cases.push((k.to_string(), v.iter().map(|s| s.to_string()).collect(), *d));
    }
    for (key, values, has_default) in cases {
        let desc = desc_of(&key);
        for v in &values {
            assert!(
                desc.contains(v.as_str()),
                "input `{key}` description must contain `{v}`: {desc}"
            );
        }
        if has_default {
            assert!(
                desc.contains("default"),
                "input `{key}` description must say `default`: {desc}"
            );
        }
    }
}
