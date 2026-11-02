//! `benchdiff.config.json`: strict, validated file settings that map onto an [`OptionsOverlay`] (ADR-001).
//! [`KEYS`] holds the core, chart and matrix keys; the matrix keys' help text lives in [`MATRIX_DOCS`].

use crate::compare::Sort;
use crate::dimensions::Rule;
use crate::import::Format;
use crate::matrix::{CellFormat, Correction, MatrixSort, VariantLabel};
use crate::options::{matrix_sort_of, MatrixOptionsOverlay, OptionsOverlay, SvgOptionsOverlay};
use crate::svg::{ChartKind, ChartRows, ChartTheme, ChartWidth, EmbedSetting};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// The single key table; the unknown-key suggestion searches it.
pub const KEYS: &[&str] = &[
    "version",
    "$schema",
    "threshold",
    "thresholds",
    "min-abs",
    "sort",
    "max-rows",
    "only-changed",
    "fail-on-regression",
    "title",
    "tag",
    "format",
    "base-format",
    "charts",
    "chart-rows",
    "chart-width",
    "chart-theme",
    "svg-dir",
    "svg-embed",
    "publish-images",
    "compare-by",
    "group-by",
    "dimensions",
    "dimensions-strict",
    "reference",
    "correction",
    "cell-format",
];

/// One help entry per matrix flag, so `--help` cannot drift from the keys (`key` is `None`
/// for CLI-only flags). Rendered by [`matrix_help`].
pub struct FlagDoc {
    pub key: Option<&'static str>,
    pub flag: &'static str,
    pub summary: &'static str,
    pub default: &'static str,
    pub allowed: &'static str,
}

/// Zero-config extraction rules; stated once in `--help`, under `--compare-by`.
pub const ZERO_CONFIG_RULES: &str = "\
zero-config rules read only `key=value` path segments (a/impl=quick) and bracket groups\n\
[k=v, k2=v2] / (k = v); nothing else (Criterion group/function/param ids carry no '=')";

pub const MATRIX_DOCS: &[FlagDoc] = &[
    FlagDoc {
        key: None,
        flag: "--variant LABEL=PATH",
        summary: "one N-way variant (repeatable; split at the first '='; label is\n[A-Za-z0-9._-], max 40); not combinable with --base/--head",
        default: "none",
        allowed: "label=path",
    },
    FlagDoc {
        key: Some("compare-by"),
        flag: "--compare-by DIM",
        summary: "pivot one dimension of the head results into columns (N-way)",
        default: "off",
        allowed: "a dimension name or group",
    },
    FlagDoc {
        key: Some("group-by"),
        flag: "--group-by DIMS",
        summary: "non-axis dimensions that identify a row (comma-separated)",
        default: "all non-axis dimensions",
        allowed: "dimension names",
    },
    FlagDoc {
        key: Some("dimensions"),
        flag: "--dimensions RULE",
        summary: "extra extraction rule `dim=regex` with a (?P<dim>...) group (repeatable,\nor newline-delimited); added to the zero-config rules above",
        default: "none",
        allowed: "dim=regex",
    },
    FlagDoc {
        key: Some("dimensions-strict"),
        flag: "--dimensions-strict",
        summary: "exit 2 instead of printing a notice on data-dependent dimension problems",
        default: "false",
        allowed: "flag",
    },
    FlagDoc {
        key: Some("reference"),
        flag: "--reference LABEL",
        summary: "variant every other column is compared against; required for\n--fail-on-regression in an N-way run",
        default: "first variant",
        allowed: "a --variant label or an axis value",
    },
    FlagDoc {
        key: Some("correction"),
        flag: "--correction NAME",
        summary: "multiple-comparison correction across one row's cells",
        default: "holm",
        allowed: "holm, none",
    },
    FlagDoc {
        key: Some("cell-format"),
        flag: "--cell-format NAME",
        summary: "matrix cell shape: `1210 ns (0.74x) \u{1f7e2}` or `0.74x \u{1f7e2}`",
        default: "full",
        allowed: "full, ratio",
    },
    FlagDoc {
        key: None,
        flag: "--print-dimensions",
        summary: "print the detected dimensions as JSON, then exit (same inputs as a run)",
        default: "off",
        allowed: "flag",
    },
];

/// The OPTIONS lines for the matrix flags, generated from [`MATRIX_DOCS`].
pub fn matrix_help() -> String {
    let mut out = String::new();
    for d in MATRIX_DOCS {
        let head = format!("    {}", d.flag);
        let mut lines = d.summary.lines();
        out.push_str(&format!("{head:<27}{}\n", lines.next().unwrap_or("")));
        for l in lines {
            out.push_str(&format!("{:<27}{l}\n", ""));
        }
        let allowed = if d.allowed == "flag" {
            String::new()
        } else {
            format!("; allowed: {}", d.allowed)
        };
        out.push_str(&format!("{:<27}(default: {}{allowed})\n", "", d.default));
        if d.flag.starts_with("--compare-by") {
            for l in ZERO_CONFIG_RULES.lines() {
                out.push_str(&format!("{:<27}{l}\n", ""));
            }
        }
    }
    out
}

/// `compare-by` is an axis name; `measure` mixes units so it cannot be one.
pub fn check_compare_by(dim: &str) -> Result<(), String> {
    if dim.trim().is_empty() {
        return Err("compare-by: value is empty".into());
    }
    if dim == crate::dimensions::MEASURE_AXIS {
        return Err("compare-by 'measure' is not an axis (units differ per measure)".into());
    }
    Ok(())
}

/// Splits a comma-separated `group-by` list into dimension names.
pub fn parse_group_by(text: &str) -> Result<Vec<String>, String> {
    let dims: Vec<String> = text.split(',').map(|d| d.trim().to_string()).collect();
    if dims.iter().any(String::is_empty) {
        return Err(format!("group-by: {text:?} has an empty dimension name"));
    }
    Ok(dims)
}

/// Splits newline-delimited `dim=regex` rules and validates each with `Rule::parse`.
pub fn parse_dimension_rules(text: &str) -> Result<Vec<String>, String> {
    let mut rules = vec![];
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        Rule::parse(line).map_err(|e| format!("dimensions: {e}"))?;
        rules.push(line.to_string());
    }
    Ok(rules)
}

pub fn parse_correction(s: &str) -> Result<Correction, String> {
    match s {
        "holm" => Ok(Correction::Holm),
        "none" => Ok(Correction::None),
        other => Err(format!(
            "correction: \"{other}\" is not valid; use holm, none"
        )),
    }
}

pub fn parse_cell_format(s: &str) -> Result<CellFormat, String> {
    match s {
        "full" => Ok(CellFormat::Full),
        "ratio" => Ok(CellFormat::Ratio),
        other => Err(format!(
            "cell-format: \"{other}\" is not valid; use full, ratio"
        )),
    }
}

pub fn parse_reference(s: &str) -> Result<VariantLabel, String> {
    VariantLabel::parse(s).map_err(|e| format!("reference: {e}"))
}

const SUPPORTED_VERSION: u64 = 1;
const SORT_NAMES: &str = "input, name, delta";

/// Serialization of the config file; only JSON in v1 (TOML would be an additive variant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    Json,
}

/// Validated file contents. `None` means the key was absent.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub version: u32,
    pub threshold: Option<f64>,
    /// Per-metric thresholds with the `default` entry already moved into `threshold`.
    pub threshold_overrides: Option<BTreeMap<String, f64>>,
    pub min_abs: Option<f64>,
    pub sort: Option<Sort>,
    pub max_rows: Option<usize>,
    pub only_changed: Option<bool>,
    pub fail_on_regression: Option<f64>,
    pub title: Option<String>,
    pub tag: Option<String>,
    pub format: Option<Format>,
    pub base_format: Option<Format>,
    pub charts: Option<Vec<ChartKind>>,
    pub chart_rows: Option<ChartRows>,
    pub chart_width: Option<ChartWidth>,
    pub chart_theme: Option<ChartTheme>,
    pub svg_dir: Option<String>,
    pub svg_embed: Option<EmbedSetting>,
    pub publish_images: Option<bool>,
    pub compare_by: Option<String>,
    pub group_by: Option<Vec<String>>,
    pub dimensions: Option<Vec<String>>,
    pub dimensions_strict: Option<bool>,
    pub reference: Option<VariantLabel>,
    pub correction: Option<Correction>,
    pub cell_format: Option<CellFormat>,
    /// The `sort` key as a matrix sort; `rank` exists only here (`sort` holds the pair enums).
    pub matrix_sort: Option<MatrixSort>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct Raw {
    version: u64,
    #[serde(rename = "$schema")]
    #[allow(dead_code)]
    schema: Option<String>,
    threshold: Option<f64>,
    thresholds: Option<BTreeMap<String, f64>>,
    min_abs: Option<f64>,
    sort: Option<String>,
    max_rows: Option<usize>,
    only_changed: Option<bool>,
    fail_on_regression: Option<f64>,
    title: Option<String>,
    tag: Option<String>,
    format: Option<String>,
    base_format: Option<String>,
    charts: Option<Value>,
    chart_rows: Option<Value>,
    chart_width: Option<Value>,
    chart_theme: Option<Value>,
    svg_dir: Option<Value>,
    svg_embed: Option<Value>,
    publish_images: Option<Value>,
    compare_by: Option<Value>,
    group_by: Option<Value>,
    dimensions: Option<Value>,
    dimensions_strict: Option<Value>,
    reference: Option<Value>,
    correction: Option<Value>,
    cell_format: Option<Value>,
}

/// Parses and validates `text`; `file_name` only appears in error text.
/// Every error is a static config error (exit 2).
pub fn parse_config(text: &str, file_name: &str, format: ConfigFormat) -> Result<Config, String> {
    match format {
        ConfigFormat::Json => parse_json(text, file_name),
    }
}

fn parse_json(text: &str, file: &str) -> Result<Config, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| {
        let at = located(file, &e);
        if e.classify() == serde_json::error::Category::Data {
            at
        } else {
            format!("{at}; the file is strict JSON (no comments)")
        }
    })?;
    let obj = value
        .as_object()
        .ok_or_else(|| format!("{file}: the config must be a JSON object"))?;

    match obj.get("version") {
        None => return Err("config: missing required key 'version'".into()),
        Some(v) => match v.as_u64() {
            Some(n) if n > SUPPORTED_VERSION => {
                return Err(format!(
                    "config version {n} is newer than supported ({SUPPORTED_VERSION})"
                ))
            }
            Some(SUPPORTED_VERSION) => {}
            _ => {
                return Err(format!(
                    "version: {v} is not valid; use {SUPPORTED_VERSION}"
                ))
            }
        },
    }
    if let Some(s) = obj.get("$schema") {
        if !s.is_string() {
            return Err("$schema: expected a string".into());
        }
    }
    for key in obj.keys() {
        if !KEYS.contains(&key.as_str()) {
            return Err(unknown_key(file, key));
        }
    }

    let raw: Raw = serde_json::from_str(text).map_err(|e| located(file, &e))?;
    let (threshold, overrides) = split_thresholds(&raw, file)?;
    for (key, v) in [
        ("min-abs", raw.min_abs),
        ("fail-on-regression", raw.fail_on_regression),
    ] {
        if let Some(v) = v {
            check_non_negative(file, key, v)?;
        }
    }
    Ok(Config {
        version: raw.version as u32,
        threshold,
        threshold_overrides: overrides,
        min_abs: raw.min_abs,
        sort: match raw.sort.as_deref() {
            Some("rank") | None => None,
            Some(s) => Some(parse_sort(file, s)?),
        },
        matrix_sort: match raw.sort.as_deref() {
            Some("rank") => Some(MatrixSort::Rank),
            Some(s) => Some(matrix_sort_of(parse_sort(file, s)?)),
            None => None,
        },
        max_rows: raw.max_rows,
        only_changed: raw.only_changed,
        fail_on_regression: raw.fail_on_regression,
        title: raw.title,
        tag: raw.tag,
        format: parse_input_format(file, "format", raw.format.as_deref())?,
        base_format: parse_input_format(file, "base-format", raw.base_format.as_deref())?,
        charts: raw
            .charts
            .as_ref()
            .map(|v| {
                ChartKind::parse_list(string_value(file, "charts", v)?)
                    .map_err(|e| format!("{file}: charts: {e}"))
            })
            .transpose()?,
        chart_rows: raw
            .chart_rows
            .as_ref()
            .map(|v| {
                let n = whole_number(file, "chart-rows", v)?;
                // Saturate (a bare `as usize` wraps on wasm32) so an oversized value reports as out of range.
                ChartRows::new(usize::try_from(n).unwrap_or(usize::MAX))
                    .map_err(|e| format!("{file}: chart-rows: {e}"))
            })
            .transpose()?,
        chart_width: raw
            .chart_width
            .as_ref()
            .map(|v| {
                let n = whole_number(file, "chart-width", v)?;
                // Saturate so an oversized value still reports as out of range.
                ChartWidth::new(u32::try_from(n).unwrap_or(u32::MAX))
                    .map_err(|e| format!("{file}: chart-width: {e}"))
            })
            .transpose()?,
        chart_theme: raw
            .chart_theme
            .as_ref()
            .map(|v| {
                ChartTheme::from_name(string_value(file, "chart-theme", v)?)
                    .map_err(|e| format!("{file}: chart-theme: {e}"))
            })
            .transpose()?,
        svg_dir: raw
            .svg_dir
            .as_ref()
            .map(|v| string_value(file, "svg-dir", v).map(str::to_string))
            .transpose()?,
        svg_embed: raw
            .svg_embed
            .as_ref()
            .map(|v| match v {
                Value::Bool(true) => Ok(EmbedSetting::True),
                Value::Bool(false) => Ok(EmbedSetting::False),
                Value::String(s) => {
                    EmbedSetting::from_name(s).map_err(|e| format!("{file}: svg-embed: {e}"))
                }
                other => Err(format!(
                    "{file}: svg-embed: {other} is not valid; use {}",
                    EmbedSetting::NAMES
                )),
            })
            .transpose()?,
        publish_images: raw
            .publish_images
            .as_ref()
            .map(|v| match v {
                Value::Bool(b) => Ok(*b),
                other => Err(format!(
                    "{file}: publish-images: {other} is not valid; use true, false"
                )),
            })
            .transpose()?,
        compare_by: raw
            .compare_by
            .as_ref()
            .map(|v| {
                let dim = string_value(file, "compare-by", v)?;
                check_compare_by(dim).map_err(|e| format!("{file}: {e}"))?;
                Ok::<_, String>(dim.to_string())
            })
            .transpose()?,
        group_by: raw
            .group_by
            .as_ref()
            .map(|v| {
                let text = match v {
                    Value::Array(items) => items
                        .iter()
                        .map(|i| string_value(file, "group-by", i))
                        .collect::<Result<Vec<_>, _>>()?
                        .join(","),
                    other => string_value(file, "group-by", other)?.to_string(),
                };
                parse_group_by(&text).map_err(|e| format!("{file}: {e}"))
            })
            .transpose()?,
        dimensions: raw
            .dimensions
            .as_ref()
            .map(|v| {
                let text = match v {
                    Value::Array(items) => items
                        .iter()
                        .map(|i| string_value(file, "dimensions", i))
                        .collect::<Result<Vec<_>, _>>()?
                        .join("\n"),
                    other => string_value(file, "dimensions", other)?.to_string(),
                };
                parse_dimension_rules(&text).map_err(|e| format!("{file}: {e}"))
            })
            .transpose()?,
        dimensions_strict: raw
            .dimensions_strict
            .as_ref()
            .map(|v| match v {
                Value::Bool(b) => Ok(*b),
                other => Err(format!(
                    "{file}: dimensions-strict: {other} is not valid; use true, false"
                )),
            })
            .transpose()?,
        reference: raw
            .reference
            .as_ref()
            .map(|v| {
                parse_reference(string_value(file, "reference", v)?)
                    .map_err(|e| format!("{file}: {e}"))
            })
            .transpose()?,
        correction: raw
            .correction
            .as_ref()
            .map(|v| {
                parse_correction(string_value(file, "correction", v)?)
                    .map_err(|e| format!("{file}: {e}"))
            })
            .transpose()?,
        cell_format: raw
            .cell_format
            .as_ref()
            .map(|v| {
                parse_cell_format(string_value(file, "cell-format", v)?)
                    .map_err(|e| format!("{file}: {e}"))
            })
            .transpose()?,
    })
}

fn string_value<'a>(file: &str, key: &str, v: &'a Value) -> Result<&'a str, String> {
    v.as_str()
        .ok_or_else(|| format!("{file}: {key}: expected a string, got {v}"))
}

fn whole_number(file: &str, key: &str, v: &Value) -> Result<u64, String> {
    v.as_u64()
        .ok_or_else(|| format!("{file}: {key}: {v} is not valid; use a whole number"))
}

type Split = (Option<f64>, Option<BTreeMap<String, f64>>);

/// Moves `thresholds.default` into the plain threshold; the rest are per-metric overrides.
fn split_thresholds(raw: &Raw, file: &str) -> Result<Split, String> {
    let mut threshold = raw.threshold;
    if let Some(t) = threshold {
        check_non_negative(file, "threshold", t)?;
    }
    let Some(map) = &raw.thresholds else {
        return Ok((threshold, None));
    };
    let mut overrides = BTreeMap::new();
    for (k, &v) in map {
        check_non_negative(file, &format!("thresholds.{k}"), v)?;
        if k == "default" {
            if threshold.is_some() {
                return Err(format!("{file}: threshold and thresholds.default both set"));
            }
            threshold = Some(v);
        } else {
            overrides.insert(k.clone(), v);
        }
    }
    Ok((threshold, (!overrides.is_empty()).then_some(overrides)))
}

fn check_non_negative(file: &str, key: &str, v: f64) -> Result<(), String> {
    if v.is_finite() && v >= 0.0 {
        Ok(())
    } else {
        Err(format!(
            "{file}: {key}: {v} is not valid; use a finite number >= 0"
        ))
    }
}

fn parse_sort(file: &str, s: &str) -> Result<Sort, String> {
    match s {
        "input" => Ok(Sort::Input),
        "name" => Ok(Sort::Name),
        "delta" => Ok(Sort::Delta),
        other => Err(format!(
            "{file}: sort: \"{other}\" is not valid; use {SORT_NAMES}"
        )),
    }
}

fn parse_input_format(file: &str, key: &str, s: Option<&str>) -> Result<Option<Format>, String> {
    let Some(s) = s else { return Ok(None) };
    Format::from_name(s).map(Some).map_err(|_| {
        let names = Format::NAMES.replace(" (kotlinx-benchmark)", "");
        format!("{file}: {key}: \"{s}\" is not valid; use {names}")
    })
}

fn unknown_key(file: &str, key: &str) -> String {
    let best = KEYS
        .iter()
        .map(|k| (edit_distance(key, k), *k))
        .min_by_key(|(d, _)| *d);
    match best {
        Some((d, k)) if d <= 2 => {
            format!("{file}: unknown key '{key}'; did you mean '{k}'?")
        }
        _ => format!(
            "{file}: unknown key '{key}'; valid keys: {}",
            KEYS.join(", ")
        ),
    }
}

fn located(file: &str, e: &serde_json::Error) -> String {
    let suffix = format!(" at line {} column {}", e.line(), e.column());
    let msg = e.to_string();
    let msg = msg.strip_suffix(&suffix).unwrap_or(&msg);
    format!("{file}: line {} column {}: {msg}", e.line(), e.column())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, &cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

impl Config {
    /// The chart settings; call before [`Config::into_overlay`], which consumes the config.
    pub fn svg_overlay(&self) -> SvgOptionsOverlay {
        SvgOptionsOverlay {
            kinds: self.charts.clone(),
            rows: self.chart_rows,
            width: self.chart_width,
            theme: self.chart_theme,
            embed: self.svg_embed,
            publish_images: self.publish_images,
            svg_dir: self.svg_dir.clone(),
            ..Default::default()
        }
    }

    /// The matrix settings; call before [`Config::into_overlay`], which consumes the config.
    pub fn matrix_overlay(&self) -> MatrixOptionsOverlay {
        MatrixOptionsOverlay {
            compare_by: self.compare_by.clone(),
            group_by: self.group_by.clone(),
            dimensions: self.dimensions.clone(),
            strict: self.dimensions_strict,
            reference: self.reference.clone(),
            correction: self.correction,
            cell_format: self.cell_format,
            sort: self.matrix_sort,
        }
    }

    /// The settings `Options` carries; `format`/`base-format` are read from the fields directly.
    pub fn into_overlay(self) -> OptionsOverlay {
        OptionsOverlay {
            threshold_pct: self.threshold,
            min_abs: self.min_abs,
            sort: self.sort,
            max_rows: self.max_rows,
            only_changed: self.only_changed,
            fail_on_regression_pct: self.fail_on_regression,
            title: self.title,
            tag: self.tag,
            threshold_overrides: self.threshold_overrides,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config, String> {
        parse_config(text, "benchdiff.config.json", ConfigFormat::Json)
    }

    #[test]
    fn parse_config_should_map_valid_file_to_overlay_when_threshold_sort_thresholds() {
        let o = parse(r#"{"version":1,"sort":"name","thresholds":{"default":4,"net/Parse":20}}"#)
            .unwrap()
            .into_overlay();
        assert_eq!(o.threshold_pct, Some(4.0));
        assert_eq!(o.sort, Some(Sort::Name));
        assert_eq!(
            o.threshold_overrides,
            Some([("net/Parse".to_string(), 20.0)].into())
        );
        let plain = parse(r#"{"version":1,"threshold":3}"#)
            .unwrap()
            .into_overlay();
        assert_eq!(plain.threshold_pct, Some(3.0));
        assert_eq!(plain.threshold_overrides, None);
    }

    #[test]
    fn parse_config_should_suggest_nearest_key_when_treshold_typo() {
        let e = parse(r#"{"version":1,"treshold":10}"#).unwrap_err();
        assert!(
            e.contains("unknown key 'treshold'; did you mean 'threshold'?"),
            "{e}"
        );
        let e = parse(r#"{"version":1,"zzzzzzzzzz":1}"#).unwrap_err();
        assert!(
            e.contains("unknown key 'zzzzzzzzzz'") && !e.contains("did you mean"),
            "{e}"
        );
    }

    #[test]
    fn parse_config_should_list_valid_enum_values_when_sort_size() {
        let e = parse(r#"{"version":1,"sort":"size"}"#).unwrap_err();
        assert!(
            e.contains(r#"sort: "size" is not valid; use input, name, delta"#),
            "{e}"
        );
    }

    #[test]
    fn parse_config_should_error_missing_version_and_newer_version() {
        assert_eq!(
            parse(r#"{"schema":1,"threshold":3}"#).unwrap_err(),
            "config: missing required key 'version'"
        );
        assert_eq!(
            parse(r#"{"version":2}"#).unwrap_err(),
            "config version 2 is newer than supported (1)"
        );
    }

    #[test]
    fn parse_config_should_accept_format_and_base_format_and_list_valid_formats_when_unknown() {
        let c = parse(r#"{"version":1,"format":"go","base-format":"benchdiff"}"#).unwrap();
        assert_eq!(c.format, Some(Format::Go));
        assert_eq!(c.base_format, Some(Format::Benchdiff));
        let e = parse(r#"{"version":1,"format":"golang"}"#).unwrap_err();
        assert!(
            e.contains(r#"format: "golang" is not valid; use auto, benchdiff, go, criterion, cargo-bench, jmh, hyperfine, googlebench, gab-smaller, gab-bigger, bmf"#),
            "{e}"
        );
    }

    #[test]
    fn parse_config_should_report_file_line_column_when_json_malformed_or_comment_present() {
        let e = parse("{\"version\":1,\n  // note\n  \"threshold\":3}").unwrap_err();
        assert!(
            e.starts_with("benchdiff.config.json: line 2 column 3: ")
                && e.ends_with("; the file is strict JSON (no comments)"),
            "{e}"
        );
        let e = parse("{\"version\":1,\n\"threshold\":\"high\"}").unwrap_err();
        assert!(
            e.starts_with("benchdiff.config.json: line 2 column ")
                && e.ends_with(r#"invalid type: string "high", expected f64"#),
            "{e}"
        );
    }

    #[test]
    fn parse_config_should_accept_schema_key_as_ignored_string_and_reject_non_string() {
        let url = "https://example.test/benchdiff.config.schema.json";
        let with = parse(&format!(
            r#"{{"$schema":"{url}","version":1,"threshold":3}}"#
        ))
        .unwrap();
        assert_eq!(with, parse(r#"{"version":1,"threshold":3}"#).unwrap());
        assert_eq!(
            parse(r#"{"$schema":3,"version":1}"#).unwrap_err(),
            "$schema: expected a string"
        );
    }

    #[test]
    fn parse_config_should_reject_results_file_with_schema_key_when_passed_by_mistake() {
        let e = parse(r#"{"schema":"benchdiff/1","metrics":[]}"#).unwrap_err();
        assert_eq!(e, "config: missing required key 'version'");
    }

    #[test]
    fn parse_config_should_error_when_threshold_and_thresholds_default_both_set() {
        let e = parse(r#"{"version":1,"threshold":3,"thresholds":{"default":4}}"#).unwrap_err();
        assert!(
            e.contains("threshold and thresholds.default both set"),
            "{e}"
        );
    }

    #[test]
    fn parse_config_should_accept_group_key_in_thresholds_and_format_keys_and_reject_markdown_json_output_keys(
    ) {
        let c = parse(r#"{"version":1,"format":"auto","thresholds":{"net":8,"net/Parse":20}}"#)
            .unwrap();
        assert_eq!(c.threshold, None);
        assert_eq!(c.threshold_overrides.unwrap().len(), 2);
        for key in ["markdown", "json"] {
            let e = parse(&format!(r#"{{"version":1,"{key}":"out"}}"#)).unwrap_err();
            assert!(e.contains(&format!("unknown key '{key}'")), "{e}");
        }
    }

    #[test]
    fn parse_config_should_reject_chart_rows_beyond_usize_when_value_wraps_to_valid() {
        // 4294967306 = 2^32 + 10: a wrapping cast to a 32-bit usize would yield 10 and pass 1..=200.
        let e = parse(r#"{"version":1,"chart-rows":4294967306}"#).unwrap_err();
        assert!(e.contains("chart-rows"), "{e}");
        assert!(parse(r#"{"version":1,"chart-rows":10}"#).is_ok());
    }

    #[test]
    fn parse_config_should_reject_negative_or_non_finite_thresholds() {
        assert!(parse(r#"{"version":1,"thresholds":{"net":-1}}"#).is_err());
        assert!(parse(r#"{"version":1,"threshold":-0.5}"#).is_err());
    }

    #[test]
    fn parse_config_should_map_every_core_key_into_overlay() {
        let o = parse(
            r#"{"version":1,"min-abs":2,"max-rows":7,"only-changed":true,
                "fail-on-regression":9,"title":"T","tag":"g"}"#,
        )
        .unwrap()
        .into_overlay();
        assert_eq!(o.min_abs, Some(2.0));
        assert_eq!(o.max_rows, Some(7));
        assert_eq!(o.only_changed, Some(true));
        assert_eq!(o.fail_on_regression_pct, Some(9.0));
        assert_eq!(o.title.as_deref(), Some("T"));
        assert_eq!(o.tag.as_deref(), Some("g"));
    }
    #[test]
    fn parse_config_should_map_chart_keys_into_svg_overlay() {
        let c = parse(
            r#"{"version":1,"charts":"delta,ranking","chart-rows":10,"chart-width":900,
                "chart-theme":"both","svg-dir":"out","svg-embed":"auto","publish-images":false}"#,
        )
        .unwrap();
        let o = c.svg_overlay();
        assert_eq!(o.kinds, Some(vec![ChartKind::Delta, ChartKind::Ranking]));
        assert_eq!(o.rows.map(ChartRows::get), Some(10));
        assert_eq!(o.width.map(ChartWidth::get), Some(900));
        assert_eq!(o.theme, Some(ChartTheme::Both));
        assert_eq!(o.svg_dir.as_deref(), Some("out"));
        assert_eq!(o.embed, Some(EmbedSetting::Auto));
        assert_eq!(o.publish_images, Some(false));
        let b = parse(r#"{"version":1,"svg-embed":false}"#).unwrap();
        assert_eq!(b.svg_embed, Some(EmbedSetting::False));
    }

    #[test]
    fn parse_config_should_list_valid_values_when_chart_value_invalid() {
        let e = parse(r#"{"version":1,"charts":"bars"}"#).unwrap_err();
        assert!(
            e.contains(r#"charts: "bars" is not valid; use none, delta, ranking, distribution (comma-separated)"#),
            "{e}"
        );
        let e = parse(r#"{"version":1,"publish-images":"yes"}"#).unwrap_err();
        assert!(
            e.contains(r#"publish-images: "yes" is not valid; use true, false"#),
            "{e}"
        );
        let e = parse(r#"{"version":1,"chart-theme":"neon"}"#).unwrap_err();
        assert!(e.contains(r#"use light, dark, both"#), "{e}");
        let e = parse(r#"{"version":1,"svg-embed":"yes"}"#).unwrap_err();
        assert!(e.contains("use true, false, auto"), "{e}");
    }

    #[test]
    fn parse_config_should_reject_out_of_range_chart_rows_and_width() {
        let e = parse(r#"{"version":1,"chart-rows":0}"#).unwrap_err();
        assert!(
            e.contains("chart-rows: 0 is not valid; use 1 to 200"),
            "{e}"
        );
        let e = parse(r#"{"version":1,"chart-width":100}"#).unwrap_err();
        assert!(
            e.contains("chart-width: 100 is not valid; use 480 to 1600"),
            "{e}"
        );
        let e = parse(r#"{"version":1,"chart-rows":-3}"#).unwrap_err();
        assert!(e.contains("chart-rows"), "{e}");
    }

    #[test]
    fn parse_config_should_suggest_chart_rows_when_chart_row_typo() {
        let e = parse(r#"{"version":1,"chart-row":10}"#).unwrap_err();
        assert!(
            e.contains("unknown key 'chart-row'; did you mean 'chart-rows'?"),
            "{e}"
        );
    }

    #[test]
    fn parse_config_should_map_matrix_keys_into_matrix_overlay() {
        let c = parse(
            r#"{"version":1,"threshold":3,"sort":"name","reference":"quick",
                "dimensions":["impl=/impl=(?P<impl>\\w+)"]}"#,
        )
        .unwrap();
        let m = c.matrix_overlay();
        assert_eq!(m.reference, Some(VariantLabel::parse("quick").unwrap()));
        assert_eq!(m.dimensions.as_ref().map(Vec::len), Some(1));
        assert_eq!(m.sort, Some(MatrixSort::Name));
        let o = c.into_overlay();
        assert_eq!((o.threshold_pct, o.sort), (Some(3.0), Some(Sort::Name)));

        let c = parse(r#"{"version":1,"cell-format":"ratio","sort":"rank"}"#).unwrap();
        assert_eq!(c.matrix_overlay().cell_format, Some(CellFormat::Ratio));
        assert_eq!(c.matrix_overlay().sort, Some(MatrixSort::Rank));
        assert_eq!(c.sort, None);
    }

    #[test]
    fn parse_config_should_report_enum_and_key_errors_for_matrix_keys() {
        let e = parse(r#"{"version":1,"correction":"bonf"}"#).unwrap_err();
        assert!(
            e.contains("correction: \"bonf\" is not valid; use holm, none"),
            "{e}"
        );
        let e = parse(r#"{"version":1,"compare-bi":"impl"}"#).unwrap_err();
        assert!(
            e.contains("unknown key 'compare-bi'; did you mean 'compare-by'?"),
            "{e}"
        );
    }

    #[test]
    fn matrix_docs_should_name_only_keys_in_the_key_table() {
        for d in MATRIX_DOCS {
            if let Some(k) = d.key {
                assert!(KEYS.contains(&k), "{k}");
            }
        }
        for k in [
            "compare-by",
            "group-by",
            "dimensions",
            "dimensions-strict",
            "reference",
            "correction",
            "cell-format",
        ] {
            assert!(
                MATRIX_DOCS.iter().any(|d| d.key == Some(k)),
                "{k} undocumented"
            );
        }
    }
}
