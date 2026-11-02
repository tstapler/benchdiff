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
        let mut removed = rest.len() != segment.len();
        let mut pairs = pairs;
        let rest = rest.trim();
        if let Some((k, v)) = parse_pair(rest) {
            pairs.push((k, v));
            removed = true;
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
        let cases: [(&str, &[(&str, &str)], &str); 6] = [
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
}
