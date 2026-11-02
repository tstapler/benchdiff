//! `benchdiff.config.json`: strict, validated file settings that map onto an [`OptionsOverlay`] (ADR-001).
//! Later tasks extend [`KEYS`] and [`Config`] with chart keys (3.3.1a) and matrix keys (2.2.2a).

use crate::compare::Sort;
use crate::import::Format;
use crate::options::{OptionsOverlay, SvgOptionsOverlay};
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
];

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
        sort: raw
            .sort
            .as_deref()
            .map(|s| parse_sort(file, s))
            .transpose()?,
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
                ChartRows::new(whole_number(file, "chart-rows", v)? as usize)
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
}
