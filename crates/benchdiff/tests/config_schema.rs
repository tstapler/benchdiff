//! `benchdiff.config.schema.json` and `examples/benchdiff.config.json` stay in step with the
//! parser: same key table, same enum values, example valid, unknown keys rejected by both.

use benchdiff_core::config::{parse_config, ConfigFormat, KEYS};
use benchdiff_core::Format;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn load(rel: &str) -> Value {
    let text = std::fs::read_to_string(root().join(rel)).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn schema() -> Value {
    load("benchdiff.config.schema.json")
}

fn example() -> Value {
    load("examples/benchdiff.config.json")
}

fn parses(config: &Value) -> Result<(), String> {
    parse_config(&config.to_string(), "t.json", ConfigFormat::Json).map(|_| ())
}

// --- tiny structural validator -------------------------------------------------------------

/// Keywords the validator understands or that carry no constraint. `pattern` is accepted but
/// not evaluated (no regex dependency); the parser test covers those values instead.
const KNOWN: &[&str] = &[
    "$schema",
    "$id",
    "$ref",
    "$defs",
    "title",
    "description",
    "type",
    "enum",
    "required",
    "properties",
    "additionalProperties",
    "items",
    "anyOf",
    "minimum",
    "maximum",
    "minLength",
    "pattern",
];

fn type_ok(t: &str, v: &Value) -> bool {
    match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "number" => v.is_number(),
        "integer" => v.as_i64().is_some() || v.as_u64().is_some(),
        other => panic!("validator: unsupported type {other}"),
    }
}

fn validate(root: &Value, schema: &Value, v: &Value, at: &str) -> Result<(), String> {
    let obj = schema.as_object().expect("schema nodes are objects");
    for k in obj.keys() {
        assert!(
            KNOWN.contains(&k.as_str()),
            "validator: unsupported keyword {k}"
        );
    }
    if let Some(r) = obj.get("$ref") {
        let name = r.as_str().unwrap().strip_prefix("#/$defs/").unwrap();
        validate(root, &root["$defs"][name], v, at)?;
    }
    if let Some(t) = obj.get("type") {
        if !type_ok(t.as_str().unwrap(), v) {
            return Err(format!("{at}: expected {t}, got {v}"));
        }
    }
    if let Some(e) = obj.get("enum") {
        if !e.as_array().unwrap().contains(v) {
            return Err(format!("{at}: {v} not in {e}"));
        }
    }
    if let Some(n) = v.as_f64() {
        if obj
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n < m)
        {
            return Err(format!("{at}: {n} below minimum"));
        }
        if obj
            .get("maximum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n > m)
        {
            return Err(format!("{at}: {n} above maximum"));
        }
    }
    if let (Some(s), Some(m)) = (v.as_str(), obj.get("minLength").and_then(Value::as_u64)) {
        if (s.chars().count() as u64) < m {
            return Err(format!("{at}: shorter than {m}"));
        }
    }
    if let Some(any) = obj.get("anyOf") {
        let ok = any
            .as_array()
            .unwrap()
            .iter()
            .any(|s| validate(root, s, v, at).is_ok());
        if !ok {
            return Err(format!("{at}: {v} matches none of anyOf"));
        }
    }
    if let (Some(items), Some(arr)) = (obj.get("items"), v.as_array()) {
        for (i, x) in arr.iter().enumerate() {
            validate(root, items, x, &format!("{at}[{i}]"))?;
        }
    }
    if let Some(map) = v.as_object() {
        for r in obj
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if !map.contains_key(r.as_str().unwrap()) {
                return Err(format!("{at}: missing required {r}"));
            }
        }
        let props = obj.get("properties").and_then(Value::as_object);
        for (k, x) in map {
            let sub = format!("{at}/{k}");
            match (
                props.and_then(|p| p.get(k)),
                obj.get("additionalProperties"),
            ) {
                (Some(s), _) => validate(root, s, x, &sub)?,
                (None, Some(Value::Bool(false))) => return Err(format!("{sub}: unknown key")),
                (None, Some(s)) if s.is_object() => validate(root, s, x, &sub)?,
                (None, _) => {}
            }
        }
    }
    Ok(())
}

fn check(config: &Value) -> Result<(), String> {
    let s = schema();
    validate(&s, &s, config, "")
}

// --- tests ---------------------------------------------------------------------------------

#[test]
fn schema_declares_draft_2020_12_object() {
    let s = schema();
    assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
    assert_eq!(s["type"], "object");
    assert_eq!(s["additionalProperties"], false);
    assert_eq!(s["required"], json!(["version"]));
}

#[test]
fn schema_keys_equal_config_keys_table() {
    let s = schema();
    let schema_keys: BTreeSet<&str> = s["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let table: BTreeSet<&str> = KEYS.iter().copied().collect();
    assert_eq!(table.len(), KEYS.len(), "KEYS has duplicates");
    assert_eq!(schema_keys, table);
}

/// The values the parser lists in its "use a, b, c" error, with any "(alias)" notes removed.
fn parser_listed_values(key: &str) -> BTreeSet<String> {
    let err = parse_config(
        &json!({ "version": 1, key: "__bogus__" }).to_string(),
        "t.json",
        ConfigFormat::Json,
    )
    .expect_err("bogus enum value must be rejected");
    let tail = err.split("use ").nth(1).unwrap_or_else(|| panic!("{err}"));
    let tail = tail.split(" (comma").next().unwrap();
    tail.split(", ")
        .map(|v| v.split(" (").next().unwrap().trim().to_string())
        .collect()
}

fn enum_values(v: &Value) -> BTreeSet<String> {
    v["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn schema_enums_match_parser_values() {
    let s = schema();
    let p = &s["properties"];
    // `rank` is accepted by `sort` but absent from the parser's listed pair-sort names.
    let mut sort = parser_listed_values("sort");
    sort.insert("rank".into());
    let formats = enum_values(&s["$defs"]["inputFormat"]);
    let listed_formats: BTreeSet<String> = Format::NAMES
        .split(", ")
        .map(|v| v.split(" (").next().unwrap().to_string())
        .collect();
    let cases = [
        ("sort", enum_values(&p["sort"]), sort),
        (
            "chart-theme",
            enum_values(&p["chart-theme"]),
            parser_listed_values("chart-theme"),
        ),
        (
            "correction",
            enum_values(&p["correction"]),
            parser_listed_values("correction"),
        ),
        (
            "cell-format",
            enum_values(&p["cell-format"]),
            parser_listed_values("cell-format"),
        ),
        ("format", formats.clone(), listed_formats.clone()),
        ("base-format", formats, parser_listed_values("base-format")),
        (
            "svg-embed",
            enum_values(&p["svg-embed"]["anyOf"][1]),
            parser_listed_values("svg-embed"),
        ),
    ];
    for (key, schema_vals, parser_vals) in cases {
        assert_eq!(schema_vals, parser_vals, "enum for {key}");
        for v in &schema_vals {
            parses(&json!({ "version": 1, key: v }))
                .unwrap_or_else(|e| panic!("{key}={v} rejected by parser: {e}"));
        }
    }
    assert_eq!(
        parser_listed_values("charts"),
        ["none", "delta", "ranking", "distribution"]
            .map(String::from)
            .into()
    );
}

#[test]
fn schema_numeric_bounds_match_parser() {
    let s = schema();
    let p = &s["properties"];
    for (key, lo, hi) in [("chart-rows", 1, 200), ("chart-width", 480, 1600)] {
        assert_eq!(p[key]["minimum"], lo);
        assert_eq!(p[key]["maximum"], hi);
        assert!(parses(&json!({ "version": 1, key: lo })).is_ok());
        assert!(parses(&json!({ "version": 1, key: hi })).is_ok());
        assert!(parses(&json!({ "version": 1, key: lo - 1 })).is_err());
        assert!(parses(&json!({ "version": 1, key: hi + 1 })).is_err());
        assert!(check(&json!({ "version": 1, key: lo - 1 })).is_err());
        assert!(check(&json!({ "version": 1, key: hi + 1 })).is_err());
    }
}

#[test]
fn example_has_schema_and_version_and_parses() {
    let ex = example();
    assert_eq!(ex["version"], 1);
    assert!(ex["$schema"]
        .as_str()
        .unwrap()
        .ends_with("/benchdiff.config.schema.json"));
    parses(&ex).unwrap();
}

#[test]
fn example_validates_against_schema() {
    check(&example()).unwrap();
}

#[test]
fn example_exercises_the_binary() {
    let out = Command::new(env!("CARGO_BIN_EXE_benchdiff"))
        .current_dir(root())
        .args([
            "--config",
            "examples/benchdiff.config.json",
            "--print-config",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn unknown_key_fails_schema_and_parser() {
    let mut ex = example();
    ex["chart-row"] = json!(10);
    let schema_err = check(&ex).unwrap_err();
    assert!(schema_err.contains("chart-row"), "{schema_err}");
    let parse_err = parses(&ex).unwrap_err();
    assert!(
        parse_err.contains("did you mean 'chart-rows'"),
        "{parse_err}"
    );

    // Same through a temp copy and the real binary.
    let dir = std::env::temp_dir().join(format!("bd-schema-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bad.json");
    std::fs::write(&path, ex.to_string()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_benchdiff"))
        .current_dir(root())
        .arg("--config")
        .arg(&path)
        .arg("--print-config")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("chart-row"));
}

#[test]
fn validator_rejects_bad_values() {
    for bad in [
        json!({}),
        json!({ "version": 2 }),
        json!({ "version": 1, "sort": "bogus" }),
        json!({ "version": 1, "threshold": -1 }),
        json!({ "version": 1, "only-changed": "yes" }),
        json!({ "version": 1, "svg-embed": "maybe" }),
        json!({ "version": 1, "group-by": [1] }),
        json!({ "version": 1, "thresholds": { "x": "high" } }),
    ] {
        assert!(check(&bad).is_err(), "{bad}");
    }
}
