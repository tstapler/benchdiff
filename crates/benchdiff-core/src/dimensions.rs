//! Dimension extraction from benchmark names (ADR-004).

use std::collections::HashMap;
use std::fmt;

use crate::dimensioned::{Dimensioned, Residual};
use crate::model::{Dims, Metric, MetricSet};
use crate::notice::{Notice, NoticeClass};

/// Result of extracting dimensions from one benchmark name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    pub dims: Dims,
    /// The name with every extracted span removed and separators collapsed.
    pub residual: String,
}

/// Zero-config extraction (ADR-004 item 3): `/key=value` path segments and
/// `[k=v, k2=v2]` / `(k = v)` bracket groups. No regex and no `-N` handling:
/// a dash inside a name or value is left whole. The first occurrence of a
/// key wins. A name with no extracted span is returned unchanged.
pub fn extract_defaults(name: &str) -> Extracted {
    let mut dims = Dims::new();
    let mut kept: Vec<String> = Vec::new();
    let mut changed = false;

    for segment in name.split('/') {
        let (rest, pairs) = strip_bracket_groups(segment);
        let removed = rest.len() != segment.len();
        let mut pairs = pairs;
        let rest = rest.trim();
        if let Some((k, v)) = parse_pair(rest) {
            pairs.push((k, v));
            for (k, v) in pairs {
                dims.entry(k).or_insert(v);
            }
            changed = true;
            continue;
        }
        for (k, v) in pairs {
            dims.entry(k).or_insert(v);
        }
        if removed {
            changed = true;
            let collapsed = collapse_ws(rest);
            if !collapsed.is_empty() {
                kept.push(collapsed);
            }
        } else {
            kept.push(segment.to_string());
        }
    }

    if !changed {
        return Extracted {
            dims,
            residual: name.to_string(),
        };
    }
    Extracted {
        dims,
        residual: kept.join("/"),
    }
}

/// Longest accepted rule pattern, in characters (ADR-004 item 4).
const MAX_PATTERN_LEN: usize = 256;
/// Compiled-program size cap handed to `regex-lite`.
const REGEX_SIZE_LIMIT: usize = 64 * 1024;
const REGEX_NEST_LIMIT: u32 = 32;

/// A user extraction rule `"<dim>=<regex>"` (ADR-004 item 4). Named capture
/// groups become dims; the whole matched span leaves the residual name.
#[derive(Debug, Clone)]
pub struct Rule {
    dim: String,
    pattern: String,
    re: regex_lite::Regex,
}

impl Rule {
    /// Static validation: pattern length, compilation, at least one named
    /// group, and `<dim>` equal to one of the named groups. Errors carry the
    /// rule text.
    pub fn parse(rule: &str) -> Result<Rule, String> {
        let (dim, pattern) = rule
            .split_once('=')
            .filter(|(d, p)| !d.is_empty() && !p.is_empty())
            .ok_or_else(|| format!("rule '{rule}' must have the form '<dim>=<pattern>'"))?;
        let len = pattern.chars().count();
        if len > MAX_PATTERN_LEN {
            return Err(format!("pattern too long ({len} > {MAX_PATTERN_LEN})"));
        }
        let re = regex_lite::RegexBuilder::new(pattern)
            .size_limit(REGEX_SIZE_LIMIT)
            .nest_limit(REGEX_NEST_LIMIT)
            .build()
            .map_err(|e| format!("rule '{rule}' has an invalid pattern: {e}"))?;
        let groups: Vec<&str> = re.capture_names().flatten().collect();
        if groups.is_empty() {
            return Err(format!("rule '{dim}=...' has no named capture group"));
        }
        if !groups.contains(&dim) {
            return Err(format!(
                "rule prefix '{dim}' is not a named group of its pattern (groups: {})",
                groups.join(", ")
            ));
        }
        Ok(Rule {
            dim: dim.to_string(),
            pattern: pattern.to_string(),
            re,
        })
    }

    /// The `<dim>` prefix.
    pub fn dim(&self) -> &str {
        &self.dim
    }

    /// Applies the rule to one name; `None` when the pattern does not match.
    pub fn apply(&self, name: &str) -> Option<Extracted> {
        self.apply_skipping(name, &Dims::new())
    }

    /// Like `apply`, but a match whose named groups all name dims already in
    /// `taken` is ignored (first match per dim wins).
    fn apply_skipping(&self, name: &str, taken: &Dims) -> Option<Extracted> {
        let caps = self.re.captures(name)?;
        let mut dims = Dims::new();
        for group in self.re.capture_names().flatten() {
            if taken.contains_key(group) {
                continue;
            }
            if let Some(m) = caps.name(group) {
                dims.insert(group.to_string(), m.as_str().to_string());
            }
        }
        if dims.is_empty() {
            return None;
        }
        let span = caps.get(0)?;
        let residual = tidy_residual(&format!("{}{}", &name[..span.start()], &name[span.end()..]));
        Some(Extracted { dims, residual })
    }
}

/// Runs `rules` in order over `name`; each rule sees the residual left by the
/// previous ones, and the first value per dim wins. No match leaves the name
/// whole with empty dims.
pub fn extract_with_rules(rules: &[Rule], name: &str) -> Extracted {
    let mut out = Extracted {
        dims: Dims::new(),
        residual: name.to_string(),
    };
    for rule in rules {
        if let Some(e) = rule.apply_skipping(&out.residual, &out.dims) {
            out.dims.extend(e.dims);
            out.residual = e.residual;
        }
    }
    out
}

/// The built-in pseudo-dimension: a metric's group.
pub const GROUP_AXIS: &str = "group";

/// `measure` is deliberately not an axis: units differ per measure.
pub const MEASURE_AXIS: &str = "measure";

/// A metric's value on `axis`: the group for the pseudo-dimension, else the named dimension.
pub fn axis_value<'a>(group: &'a str, dims: &'a Dims, axis: &str) -> Option<&'a str> {
    if axis == GROUP_AXIS {
        return Some(group).filter(|g| !g.is_empty());
    }
    dims.get(axis).map(String::as_str)
}

/// The non-axis dims that take part in row identity: all of them, or only those in `group_by`.
pub fn row_dims(dims: &Dims, axis: &str, group_by: Option<&[String]>) -> Dims {
    dims.iter()
        .filter(|(k, _)| k.as_str() != axis)
        .filter(|(k, _)| group_by.is_none_or(|g| g.iter().any(|x| x == *k)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Most unmatched names listed in diagnostics and strict errors.
const MAX_LISTED: usize = 10;

/// A data-dependent problem found by [`apply`] when not strict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic(pub String);

impl Diagnostic {
    /// The `⚠️` notice this diagnostic is shown as.
    pub fn to_notice(&self) -> Notice {
        Notice {
            class: NoticeClass::Degradation,
            text: self.0.clone(),
        }
    }
}

/// What [`apply`] noticed without failing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostics {
    /// Zero-match rules and dropped collisions, in discovery order.
    pub entries: Vec<Diagnostic>,
    /// Names that carry no dimension, first 10 only.
    pub unmatched: Vec<String>,
}

impl Diagnostics {
    pub fn notices(&self) -> Vec<Notice> {
        self.entries.iter().map(Diagnostic::to_notice).collect()
    }
}

/// A data-dependent problem promoted to an error by `dimensions-strict`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimError(pub String);

impl fmt::Display for DimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DimError {}

/// Splits every name of `set` into dims and a residual (ADR-004). With no
/// `rules` the default extractor runs; with rules, names the rules leave
/// without dims fall back to it. Zero-match rules and collisions on
/// `(group, residual, measure, dims)` are diagnostics, or errors when `strict`
/// (which also errors on any name left without dims). A collision keeps the
/// first metric in input order. `set.skipped` keys are run through the same extraction.
pub fn apply(
    set: MetricSet,
    rules: &[Rule],
    strict: bool,
) -> Result<(Dimensioned, Diagnostics), DimError> {
    let mut diag = Diagnostics::default();
    let extracted: Vec<Extracted> = set
        .metrics
        .iter()
        .map(|m| extract_one(rules, &m.name))
        .collect();

    for rule in rules {
        if let Some(text) = zero_match_text(rule, &set.metrics) {
            if strict {
                return Err(DimError(text));
            }
            diag.entries.push(Diagnostic(format!(
                "{text}; rule skipped (set dimensions-strict to fail)"
            )));
        }
    }

    let mut entries: Vec<(Metric, Residual)> = Vec::with_capacity(set.metrics.len());
    let mut seen: HashMap<(String, String, String, Dims), String> = HashMap::new();
    let mut unmatched_total = 0usize;
    for (mut metric, e) in set.metrics.into_iter().zip(extracted) {
        if e.dims.is_empty() {
            unmatched_total += 1;
            if diag.unmatched.len() < MAX_LISTED {
                diag.unmatched.push(metric.name.clone());
            }
        }
        let identity = (
            metric.group.clone(),
            e.residual.clone(),
            metric.measure.clone(),
            e.dims.clone(),
        );
        if let Some(first) = seen.get(&identity) {
            let (kept, dropped) = (first.as_str(), metric.name.as_str());
            if strict {
                return Err(DimError(format!(
                    "benchmark names '{kept}' and '{dropped}' both reduce to '{}' with the same dimensions",
                    e.residual
                )));
            }
            diag.entries.push(Diagnostic(format!(
                "kept '{kept}', dropped '{dropped}' (both reduce to '{}' with the same dimensions)",
                e.residual
            )));
            continue;
        }
        seen.insert(identity, metric.name.clone());
        metric.dims = e.dims;
        entries.push((metric, Residual::new(e.residual)));
    }

    if strict && unmatched_total > 0 {
        let more = unmatched_total.saturating_sub(diag.unmatched.len());
        let tail = if more > 0 {
            format!(" (and {more} more)")
        } else {
            String::new()
        };
        return Err(DimError(format!(
            "dimensions-strict: {unmatched_total} benchmark names carry no dimension: {}{tail}",
            diag.unmatched.join(", ")
        )));
    }
    let mut skipped = set.skipped;
    skipped.attribute(|name| {
        let e = extract_one(rules, name);
        (e.dims, e.residual)
    });
    Ok((Dimensioned::from_parts(entries, skipped), diag))
}

fn extract_one(rules: &[Rule], name: &str) -> Extracted {
    if rules.is_empty() {
        return extract_defaults(name);
    }
    let e = extract_with_rules(rules, name);
    if e.dims.is_empty() {
        extract_defaults(name)
    } else {
        e
    }
}

/// `Some(text)` when `rule` matches none of the raw names.
fn zero_match_text(rule: &Rule, metrics: &[Metric]) -> Option<String> {
    let any = metrics.iter().any(|m| rule.re.is_match(&m.name));
    if any || metrics.is_empty() {
        return None;
    }
    Some(format!(
        "dimension \"{}\": pattern {} matched 0 of {} benchmark names (example: \"{}\")",
        rule.dim,
        rule.pattern,
        metrics.len(),
        metrics[0].name
    ))
}

/// Collapses doubled separators and whitespace left behind by span removal.
fn tidy_residual(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.trim().chars() {
        if c == '/' && out.ends_with('/') {
            continue;
        }
        out.push(c);
    }
    collapse_ws(out.trim_matches('/'))
}

/// Removes bracket groups whose content is entirely `k=v` pairs; groups with
/// any other content (e.g. `[abc]`) stay in the text.
fn strip_bracket_groups(segment: &str) -> (String, Vec<(String, String)>) {
    let mut out = String::with_capacity(segment.len());
    let mut pairs = Vec::new();
    let mut rest = segment;
    while let Some(open) = rest.find(['[', '(']) {
        let close_ch = if rest.as_bytes()[open] == b'[' {
            ']'
        } else {
            ')'
        };
        let after = &rest[open + 1..];
        let group = after
            .find(close_ch)
            .and_then(|end| parse_group(&after[..end]).map(|p| (end, p)));
        match group {
            Some((end, mut p)) => {
                out.push_str(&rest[..open]);
                pairs.append(&mut p);
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    (out, pairs)
}

fn parse_group(content: &str) -> Option<Vec<(String, String)>> {
    let pairs: Option<Vec<_>> = content.split(',').map(|p| parse_pair(p.trim())).collect();
    pairs.filter(|p| !p.is_empty())
}

/// `key=value` with a non-empty whitespace-free key and non-empty value.
fn parse_pair(s: &str) -> Option<(String, String)> {
    let (k, v) = s.split_once('=')?;
    let (k, v) = (k.trim(), v.trim());
    let key_ok = !k.is_empty()
        && !k
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '[' | ']' | '(' | ')' | '='));
    (key_ok && !v.is_empty()).then(|| (k.to_string(), v.to_string()))
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dims(pairs: &[(&str, &str)]) -> Dims {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn extract_defaults_should_return_impl_and_size_dims_when_name_has_slash_kv_segments() {
        let e = extract_defaults("BenchmarkSort/impl=quick/size=1000");
        assert_eq!(e.dims, dims(&[("impl", "quick"), ("size", "1000")]));
        assert_eq!(e.residual, "BenchmarkSort");
        let e = extract_defaults("BenchmarkX/size=10/impl=a");
        assert_eq!(e.dims, dims(&[("size", "10"), ("impl", "a")]));
        assert_eq!(e.residual, "BenchmarkX");
    }

    #[test]
    fn extract_defaults_should_keep_dash_suffix_whole_when_size_10_dash_8() {
        let e = extract_defaults("BenchmarkX/size=10-8");
        assert_eq!(e.dims, dims(&[("size", "10-8")]));
        assert_eq!(e.residual, "BenchmarkX");
        let e = extract_defaults("BenchmarkX/size=10/impl=a-8");
        assert_eq!(e.dims["impl"], "a-8");
        let e = extract_defaults("BenchmarkQuick-sort");
        assert!(e.dims.is_empty());
        assert_eq!(e.residual, "BenchmarkQuick-sort");
    }

    #[test]
    fn extract_defaults_should_not_emit_procs_dim_when_go_importer_output() {
        let e = extract_defaults("BenchmarkSort/impl=quick-sort/size=1000");
        assert!(!e.dims.contains_key("procs"));
        assert_eq!(e.dims["impl"], "quick-sort");
        assert_eq!(e.dims.len(), 2);
    }

    #[test]
    fn extract_defaults_should_leave_name_untouched_when_no_equals_segments_criterion() {
        for name in [
            "group/function/param",
            "BM_Sort/arg:1/real_time",
            "BenchmarkQuick-sort",
            "",
            "a//b",
            "[abc]",
            "x/=v",
            "x/k=",
        ] {
            let e = extract_defaults(name);
            assert!(e.dims.is_empty(), "{name}");
            assert_eq!(e.residual, name);
        }
    }

    #[test]
    fn extract_defaults_should_read_bracket_groups_when_jmh_or_multi_pair() {
        type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a str);
        let cases: [Case; 6] = [
            (
                "Bench.sort (size = 1000)",
                &[("size", "1000")],
                "Bench.sort",
            ),
            (
                "Bench.sort[size=10, impl=a]",
                &[("size", "10"), ("impl", "a")],
                "Bench.sort",
            ),
            ("Bench/run[k=v]/x=1", &[("k", "v"), ("x", "1")], "Bench/run"),
            ("Bench [abc]", &[], "Bench [abc]"),
            ("Bench [k=v, oops]", &[], "Bench [k=v, oops]"),
            ("A/k=1/k=2", &[("k", "1")], "A"),
        ];
        for (name, want, residual) in cases {
            let e = extract_defaults(name);
            assert_eq!(e.dims, dims(want), "{name}");
            assert_eq!(e.residual, residual, "{name}");
        }
    }

    #[test]
    fn extract_defaults_should_never_leave_extracted_span_in_residual() {
        for name in [
            "B/a=1/b=2",
            "B (a = 1) tail",
            "B[a=1]/c",
            "B/a=1/x/b=2/y",
            "a=1",
        ] {
            let e = extract_defaults(name);
            for (k, v) in &e.dims {
                assert!(
                    !e.residual.contains(&format!("{k}={v}")),
                    "{name} -> {}",
                    e.residual
                );
            }
        }
        assert_eq!(extract_defaults("B/a=1/x/b=2/y").residual, "B/x/y");
        assert_eq!(extract_defaults("B (a = 1) tail").residual, "B tail");
    }

    #[test]
    fn rule_parse_should_reject_pattern_without_named_group_or_over_256_chars() {
        assert_eq!(
            Rule::parse(r"impl=(\w+)Sort").unwrap_err(),
            "rule 'impl=...' has no named capture group"
        );
        assert_eq!(
            Rule::parse(r"foo=(?P<impl>\w+)").unwrap_err(),
            "rule prefix 'foo' is not a named group of its pattern (groups: impl)"
        );
        let long = format!("(?P<impl>{})", "a".repeat(300 - 10));
        assert_eq!(long.len(), 300);
        assert_eq!(
            Rule::parse(&format!("impl={long}")).unwrap_err(),
            "pattern too long (300 > 256)"
        );
        let ok = format!("(?P<impl>{})", "a".repeat(256 - 10));
        assert!(Rule::parse(&format!("impl={ok}")).is_ok());
        assert!(Rule::parse("no-equals").unwrap_err().contains("no-equals"));
        assert!(Rule::parse("impl=(?P<impl>[")
            .unwrap_err()
            .contains("impl=(?P<impl>["));
        assert!(Rule::parse(r"=(?P<impl>\w+)").is_err());
    }

    #[test]
    fn rule_apply_should_extract_named_group_and_remove_span_when_slash_impl_rule() {
        let rule = Rule::parse(r"impl=/impl=(?P<impl>\w+)").unwrap();
        let e = rule.apply("BenchmarkSort/impl=quick").unwrap();
        assert_eq!(e.dims, dims(&[("impl", "quick")]));
        assert_eq!(e.residual, "BenchmarkSort");
        assert!(rule.apply("BenchmarkSort/size=10").is_none());
    }

    #[test]
    fn rule_apply_should_remove_whole_match_span_when_pattern_has_trailing_literal() {
        let rule = Rule::parse(r"impl=(?P<impl>[A-Za-z]+)Sort$").unwrap();
        let e = rule.apply("BenchmarkQuickSort").unwrap();
        assert_eq!(e.dims, dims(&[("impl", "BenchmarkQuick")]));
    }

    #[test]
    fn extract_with_rules_should_let_first_match_per_dim_win_and_run_in_order() {
        let rules = [
            Rule::parse(r"impl=/impl=(?P<impl>\w+)").unwrap(),
            Rule::parse(r"impl=-(?P<impl>\w+)$").unwrap(),
            Rule::parse(r"size=/n(?P<size>\d+)").unwrap(),
        ];
        let e = extract_with_rules(&rules, "Sort/impl=quick/n10-heap");
        assert_eq!(e.dims, dims(&[("impl", "quick"), ("size", "10")]));
        assert_eq!(e.residual, "Sort-heap");
        let e = extract_with_rules(&rules, "Plain");
        assert!(e.dims.is_empty());
        assert_eq!(e.residual, "Plain");
    }

    fn set(names: &[&str]) -> MetricSet {
        MetricSet {
            metrics: names
                .iter()
                .map(|n| Metric::new("", n, "", "ns", 1.0, crate::Better::Lower))
                .collect(),
            ..Default::default()
        }
    }

    fn residuals(d: &Dimensioned) -> Vec<&str> {
        d.entries().map(|(_, r)| r.as_str()).collect()
    }

    #[test]
    fn apply_should_set_dims_and_residuals_when_default_rules() {
        let (d, diag) = apply(set(&["Sort/impl=quick/size=10", "Setup"]), &[], false).unwrap();
        assert_eq!(residuals(&d), ["Sort", "Setup"]);
        let metrics: Vec<_> = d.entries().map(|(m, _)| m).collect();
        assert_eq!(metrics[0].dims, dims(&[("impl", "quick"), ("size", "10")]));
        assert_eq!(metrics[0].name, "Sort/impl=quick/size=10");
        assert_eq!(diag.unmatched, ["Setup"]);
        assert!(diag.entries.is_empty());
        assert_eq!(d.unmatched(), 1);
    }

    #[test]
    fn apply_should_fall_back_to_defaults_when_rules_leave_name_without_dims() {
        let rules = [Rule::parse(r"impl=-(?P<impl>\w+)$").unwrap()];
        let (d, _) = apply(set(&["Sort-heap", "Sort/size=3"]), &rules, false).unwrap();
        let dims_seen: Vec<_> = d.entries().map(|(m, _)| m.dims.clone()).collect();
        assert_eq!(
            dims_seen,
            [dims(&[("impl", "heap")]), dims(&[("size", "3")])]
        );
    }

    #[test]
    fn apply_should_note_zero_match_rule_when_not_strict() {
        let names: Vec<String> = (0..48).map(|i| format!("BenchmarkSort/size={i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let rules = [Rule::parse(r"impl=impl=(?P<impl>\w+)").unwrap()];
        let (_, diag) = apply(set(&refs), &rules, false).unwrap();
        let want = r#"dimension "impl": pattern impl=(?P<impl>\w+) matched 0 of 48 benchmark names (example: "BenchmarkSort/size=0"); rule skipped (set dimensions-strict to fail)"#;
        assert_eq!(diag.entries, [Diagnostic(want.to_string())]);
        let notice = diag.notices().remove(0);
        assert_eq!(notice.class, NoticeClass::Degradation);
        assert_eq!(notice.text, want);
    }

    #[test]
    fn apply_should_error_on_zero_match_rule_when_strict() {
        let rules = [Rule::parse(r"impl=impl=(?P<impl>\w+)").unwrap()];
        let err = apply(set(&["BenchmarkSort/size=100"]), &rules, true).unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"dimension "impl": pattern impl=(?P<impl>\w+) matched 0 of 1 benchmark names (example: "BenchmarkSort/size=100")"#
        );
    }

    #[test]
    fn apply_should_error_naming_both_originals_when_collision_and_strict() {
        let err = apply(set(&["Sort/impl=1", "Sort[impl=1]"]), &[], true).unwrap_err();
        assert!(err.0.contains("'Sort/impl=1'"), "{err}");
        assert!(err.0.contains("'Sort[impl=1]'"), "{err}");
    }

    #[test]
    fn apply_should_keep_first_and_note_both_originals_when_collision_and_not_strict() {
        let (d, diag) = apply(
            set(&["Sort/impl=1", "Sort[impl=1]", "Sort/impl=2"]),
            &[],
            false,
        )
        .unwrap();
        let names: Vec<_> = d.entries().map(|(m, _)| m.name.as_str()).collect();
        assert_eq!(names, ["Sort/impl=1", "Sort/impl=2"]);
        assert_eq!(diag.entries.len(), 1);
        assert!(
            diag.entries[0]
                .0
                .starts_with("kept 'Sort/impl=1', dropped 'Sort[impl=1]'"),
            "{:?}",
            diag.entries[0]
        );
    }

    #[test]
    fn apply_should_error_listing_unmatched_names_when_strict() {
        let err = apply(set(&["Sort/impl=a", "Foo"]), &[], true).unwrap_err();
        assert!(err.0.contains("Foo"), "{err}");
        assert!(!err.0.contains("Sort"), "{err}");
    }

    #[test]
    fn apply_should_list_only_first_ten_unmatched_names_when_not_strict() {
        let names: Vec<String> = (0..15).map(|i| format!("n{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let (_, diag) = apply(set(&refs), &[], false).unwrap();
        assert_eq!(diag.unmatched.len(), 10);
        assert_eq!(diag.unmatched[0], "n0");
    }

    #[test]
    fn apply_should_keep_same_residual_apart_when_dims_differ() {
        let (d, diag) = apply(set(&["A/impl=1", "A/impl=2"]), &[], false).unwrap();
        assert_eq!(d.entries().count(), 2);
        assert!(diag.entries.is_empty());
    }

    #[test]
    fn apply_should_attribute_skipped_keys_to_dims_and_residual_with_default_extractor() {
        let mut s = set(&["Sort/impl=quick"]);
        s.skipped.push("", "Sort/impl=merge", "");
        s.skipped.push("", "Setup", "");
        let (d, _) = apply(s, &[], false).unwrap();
        let e: Vec<_> = d.skipped_entries().collect();
        assert_eq!(e[0].residual, "Sort");
        assert_eq!(e[0].name, "Sort/impl=merge");
        assert_eq!(e[0].dims, &dims(&[("impl", "merge")]));
        assert_eq!((e[1].residual, e[1].dims.is_empty()), ("Setup", true));
    }
}
