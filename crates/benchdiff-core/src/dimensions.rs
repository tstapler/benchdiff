//! Dimension extraction from benchmark names (ADR-004).

use crate::model::Dims;

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

/// Longest accepted rule pattern, in bytes (ADR-004 item 4).
const MAX_PATTERN_LEN: usize = 256;
/// Compiled-program size cap handed to `regex-lite`.
const REGEX_SIZE_LIMIT: usize = 64 * 1024;
const REGEX_NEST_LIMIT: u32 = 32;

/// A user extraction rule `"<dim>=<regex>"` (ADR-004 item 4). Named capture
/// groups become dims; the whole matched span leaves the residual name.
#[derive(Debug, Clone)]
pub struct Rule {
    dim: String,
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
}
