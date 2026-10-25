//! Importers: native benchmark output text -> [`MetricSet`]. Pure; callers do the I/O.

mod gab;
mod go;
mod json_formats;

use crate::model::MetricSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Auto,
    Benchdiff,
    Go,
    Criterion,
    CargoBench,
    Jmh,
    Hyperfine,
    GoogleBench,
    GabSmaller,
    GabBigger,
    Bmf,
}

impl Format {
    pub const NAMES: &'static str =
        "auto, benchdiff, go, criterion, cargo-bench, jmh (kotlinx-benchmark), hyperfine, googlebench, gab-smaller, gab-bigger, bmf";

    pub fn from_name(name: &str) -> Result<Format, String> {
        Ok(match name.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Format::Auto,
            "benchdiff" | "native" => Format::Benchdiff,
            "go" | "gotest" | "benchfmt" => Format::Go,
            "criterion" | "cargo-criterion" => Format::Criterion,
            "cargo-bench" | "cargo" | "libtest" => Format::CargoBench,
            "jmh" | "kotlinx" | "kotlinx-benchmark" => Format::Jmh,
            "hyperfine" => Format::Hyperfine,
            "googlebench" | "google-benchmark" | "googlecpp" => Format::GoogleBench,
            "gab-smaller" | "customsmallerisbetter" => Format::GabSmaller,
            "gab-bigger" | "custombiggerisbetter" => Format::GabBigger,
            "bmf" | "bencher" => Format::Bmf,
            other => {
                return Err(format!(
                    "unknown format {other:?}; expected one of: {}",
                    Format::NAMES
                ))
            }
        })
    }
}

/// Parses `text` as `format`; [`Format::Auto`] sniffs the content.
pub fn parse(format: Format, text: &str) -> Result<MetricSet, String> {
    let format = if format == Format::Auto {
        detect(text)?
    } else {
        format
    };
    let set = match format {
        Format::Auto => unreachable!("resolved above"),
        Format::Benchdiff => return MetricSet::parse(text),
        Format::Go => go::parse(text)?,
        Format::Criterion => json_formats::criterion(text)?,
        Format::CargoBench => json_formats::cargo_bench(text)?,
        Format::Jmh => json_formats::jmh(text)?,
        Format::Hyperfine => json_formats::hyperfine(text)?,
        Format::GoogleBench => json_formats::google_bench(text)?,
        Format::GabSmaller => gab::parse(text, crate::model::Better::Lower)?,
        Format::GabBigger => gab::parse(text, crate::model::Better::Higher)?,
        Format::Bmf => json_formats::bmf(text)?,
    };
    set.validate()?;
    Ok(set)
}

/// Guesses the format from content. Ambiguous custom-JSON arrays resolve to `gab-smaller`.
pub fn detect(text: &str) -> Result<Format, String> {
    let trimmed = text.trim_start();
    if trimmed.contains(r#""reason":"benchmark-complete""#)
        || trimmed.contains(r#""reason": "benchmark-complete""#)
    {
        return Ok(Format::Criterion);
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        let value: serde_json::Value = serde_json::from_str(trimmed)
            .map_err(|e| format!("input looks like JSON but does not parse: {e}"))?;
        return detect_json(&value);
    }
    if text
        .lines()
        .any(|l| l.starts_with("test ") && l.contains(" ... bench:"))
    {
        return Ok(Format::CargoBench);
    }
    if go::looks_like_go(text) {
        return Ok(Format::Go);
    }
    Err("cannot detect the input format; pass the `format` option explicitly".into())
}

fn detect_json(v: &serde_json::Value) -> Result<Format, String> {
    use serde_json::Value::{Array, Object};
    match v {
        Array(items) => match items.first() {
            Some(Object(o)) if o.contains_key("primaryMetric") => Ok(Format::Jmh),
            Some(Object(o)) if o.contains_key("name") && o.contains_key("value") => {
                Ok(Format::GabSmaller)
            }
            _ => Err("JSON array is neither JMH nor a github-action-benchmark custom array".into()),
        },
        Object(o) if o.contains_key("metrics") => Ok(Format::Benchdiff),
        Object(o) if o.contains_key("benchmarks") && o.contains_key("context") => {
            Ok(Format::GoogleBench)
        }
        Object(o) if o.contains_key("results") => Ok(Format::Hyperfine),
        Object(o) if looks_like_bmf(o) => Ok(Format::Bmf),
        _ => Err("JSON object matches no known benchmark format".into()),
    }
}

fn looks_like_bmf(o: &serde_json::Map<String, serde_json::Value>) -> bool {
    !o.is_empty()
        && o.values().all(|bench| {
            bench.as_object().is_some_and(|measures| {
                !measures.is_empty()
                    && measures
                        .values()
                        .all(|m| m.get("value").is_some_and(|v| v.is_number()))
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/../../testdata/{path}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn detect_should_recognise_every_fixture() {
        for (path, want) in [
            ("go/bench.txt", Format::Go),
            ("criterion/criterion.jsonl", Format::Criterion),
            ("cargo-bench/bench.txt", Format::CargoBench),
            ("hyperfine/hyperfine.json", Format::Hyperfine),
            ("hyperfine/hyperfine-v1.json", Format::Hyperfine),
            ("jmh/jmh.json", Format::Jmh),
            ("googlebench/googlebench.json", Format::GoogleBench),
            ("gab/custom.json", Format::GabSmaller),
            ("bmf/bmf.json", Format::Bmf),
            ("benchdiff/native.json", Format::Benchdiff),
        ] {
            assert_eq!(detect(&fixture(path)).unwrap(), want, "{path}");
        }
    }

    #[test]
    fn detect_should_explain_unknown_input() {
        assert!(detect("hello").unwrap_err().contains("format"));
        assert!(detect(r#"{"x":1}"#).is_err());
    }

    #[test]
    fn head_json_output_is_redetected_and_compares_as_unchanged() {
        use crate::compare::{compare, Options, Verdict};
        let head = parse(Format::Auto, &fixture("go/bench.txt")).unwrap();
        let json = head.to_json();
        assert_eq!(detect(&json).unwrap(), Format::Benchdiff);
        let reparsed = parse(Format::Auto, &json).unwrap();
        let report = compare(Some(&reparsed), &head, &Options::default());
        assert!(report.rows.iter().all(|r| r.verdict == Verdict::Unchanged));
        assert_eq!(report.rows.len(), head.metrics.len());
    }

    #[test]
    fn bmf_lookalike_with_a_non_numeric_value_is_not_detected_as_bmf() {
        assert!(detect(r#"{"b":{"latency":{"value":"fast"}}}"#).is_err());
    }

    #[test]
    fn gab_like_array_missing_unit_is_an_error_not_a_panic() {
        let text = r#"[{"name":"a","value":1}]"#;
        assert_eq!(detect(text).unwrap(), Format::GabSmaller);
        assert!(parse(Format::Auto, text).is_err());
    }

    #[test]
    fn from_name_should_accept_aliases_and_reject_unknowns() {
        assert_eq!(Format::from_name("kotlinx-benchmark").unwrap(), Format::Jmh);
        assert_eq!(
            Format::from_name("customBiggerIsBetter").unwrap(),
            Format::GabBigger
        );
        assert!(Format::from_name("xml")
            .unwrap_err()
            .contains("expected one of"));
    }

    #[test]
    fn parse_should_accept_every_fixture_with_auto() {
        for path in [
            "go/bench.txt",
            "criterion/criterion.jsonl",
            "cargo-bench/bench.txt",
            "hyperfine/hyperfine.json",
            "hyperfine/hyperfine-v1.json",
            "jmh/jmh.json",
            "googlebench/googlebench.json",
            "gab/custom.json",
            "bmf/bmf.json",
            "benchdiff/native.json",
        ] {
            let set = parse(Format::Auto, &fixture(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert!(!set.metrics.is_empty(), "{path}");
        }
    }
}
