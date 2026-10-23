//! Markdown report: a sticky-comment marker, a summary line, per-group tables with a small
//! bar, unchanged rows collapsed, environment and method notes. Deterministic output only
//! (no clock), so native and wasm builds agree byte for byte.

use crate::compare::{Method, Report, Row, Verdict};
use std::fmt::Write;

const BAR_CELLS: usize = 5;
/// Percent change at which the bar is full.
const BAR_FULL_PCT: f64 = 50.0;

/// The hidden marker that identifies this report's sticky comment.
pub fn marker(tag: &str) -> String {
    format!("<!-- benchdiff:{tag} -->")
}

pub fn render_markdown(report: &Report) -> String {
    let s = &report.summary;
    let mut out = String::new();
    let _ = writeln!(out, "{}", marker(&s.tag));
    let _ = writeln!(out, "### {}\n", s.title);
    let _ = writeln!(out, "{}\n", summary_line(report));

    let (changed, quiet): (Vec<&Row>, Vec<&Row>) = report
        .rows
        .iter()
        .partition(|r| is_interesting(r, report.has_base));
    let shown = s.max_rows.map_or(changed.len(), |n| n.min(changed.len()));
    render_groups(&mut out, &changed[..shown], false, report.has_base);
    if shown < changed.len() {
        let _ = writeln!(out, "_…and {} more changed rows not shown (`max-rows: {}`); the JSON report has them all._\n", changed.len() - shown, shown);
    }
    if changed.is_empty() && report.has_base {
        let _ = writeln!(out, "No meaningful changes.\n");
    }

    if !quiet.is_empty() && !s.only_changed {
        let _ = writeln!(
            out,
            "<details>\n<summary>{} unchanged</summary>\n",
            quiet.len()
        );
        render_groups(&mut out, &quiet, true, report.has_base);
        let _ = writeln!(out, "</details>\n");
    }
    render_env(&mut out, report);
    render_method(&mut out, report);
    out
}

/// Without a base there is nothing to call "changed": show the numbers up front.
fn is_interesting(r: &Row, has_base: bool) -> bool {
    let _ = has_base;
    r.verdict != Verdict::Unchanged
}

fn summary_line(report: &Report) -> String {
    if !report.has_base {
        return format!(
            "**{} benchmarks** (no base to compare with)",
            report.rows.len()
        );
    }
    let n = |v| report.count(v);
    let mut line = format!(
        "**{} regressed · {} improved · {} unchanged · {} new · {} removed**",
        n(Verdict::Regressed),
        n(Verdict::Improved),
        n(Verdict::Unchanged),
        n(Verdict::New),
        n(Verdict::Removed)
    );
    if n(Verdict::Incomparable) > 0 {
        let _ = write!(line, " · {} incomparable", n(Verdict::Incomparable));
    }
    line
}

fn render_groups(out: &mut String, rows: &[&Row], collapsed: bool, has_base: bool) {
    let mut groups: Vec<&str> = Vec::new();
    for r in rows {
        if !groups.contains(&r.group.as_str()) {
            groups.push(&r.group);
        }
    }
    let show_test = rows
        .iter()
        .any(|r| r.method.is_some_and(|m| m != Method::Threshold));
    for group in &groups {
        let in_group: Vec<&&Row> = rows.iter().filter(|r| r.group == *group).collect();
        if !group.is_empty() {
            let _ = if collapsed {
                writeln!(out, "**`{group}`**\n")
            } else {
                writeln!(out, "#### `{group}`\n")
            };
        }
        if !has_base {
            let _ = writeln!(out, "| Benchmark | Value | Range |\n|---|--:|--:|");
            for r in in_group {
                let _ = writeln!(
                    out,
                    "| {} | {} | {} |",
                    cell(&r.label()),
                    value(r.head, &r.unit),
                    r.head_range.map_or("—".to_string(), |(lo, hi)| format!(
                        "{} – {}",
                        fmt_num(lo),
                        fmt_num(hi)
                    ))
                );
            }
            out.push('\n');
            continue;
        }
        let _ = writeln!(
            out,
            "| Benchmark | Base | Head | Change |{}",
            if show_test { " Evidence |" } else { "" }
        );
        let _ = writeln!(
            out,
            "|---|--:|--:|--:|{}",
            if show_test { "---|" } else { "" }
        );
        for r in in_group {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} |{}",
                cell(&r.label()),
                value(r.base, &r.unit),
                value(r.head, &r.unit),
                change(r),
                if show_test {
                    format!(" {} |", evidence(r))
                } else {
                    String::new()
                }
            );
        }
        out.push('\n');
    }
}

fn cell(text: &str) -> String {
    format!("`{}`", text.replace('|', "\\|").replace('`', "'"))
}

fn value(v: Option<f64>, unit: &str) -> String {
    match v {
        None => "—".into(),
        Some(v) if unit.is_empty() => fmt_num(v),
        Some(v) => format!("{} {}", fmt_num(v), unit),
    }
}

fn change(r: &Row) -> String {
    let icon = match r.verdict {
        Verdict::Regressed => "🔴",
        Verdict::Improved => "🟢",
        Verdict::New => return "🆕 new".into(),
        Verdict::Removed => return "🗑️ removed".into(),
        Verdict::Incomparable => return "⚠️ units differ".into(),
        Verdict::Unchanged => "⚪",
    };
    match r.delta_pct {
        Some(p) => format!("{icon} {:+.1}% {}", p, bar(p, r.verdict)),
        None => format!("{icon} n/a (base is 0)"),
    }
}

/// `▰▰▱▱▱`: filled cells scale with the magnitude of the change; none for unchanged.
fn bar(pct: f64, verdict: Verdict) -> String {
    if verdict == Verdict::Unchanged {
        return "▱".repeat(BAR_CELLS);
    }
    let filled = ((pct.abs() / BAR_FULL_PCT) * BAR_CELLS as f64)
        .ceil()
        .clamp(1.0, BAR_CELLS as f64) as usize;
    format!("{}{}", "▰".repeat(filled), "▱".repeat(BAR_CELLS - filled))
}

fn evidence(r: &Row) -> String {
    match (r.method, r.p_value) {
        (Some(Method::MannWhitney), Some(p)) => {
            format!("p{} (n={}/{})", fmt_p(p), r.n_base, r.n_head)
        }
        (Some(Method::Interval), _) => "intervals".into(),
        _ => "—".into(),
    }
}

fn fmt_p(p: f64) -> String {
    if p < 0.001 {
        "<0.001".into()
    } else {
        format!("={p:.3}")
    }
}

/// Four significant digits, no exponent for ordinary magnitudes, trailing zeros trimmed.
pub(crate) fn fmt_num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let magnitude = v.abs().log10().floor() as i32;
    if !(-4..=12).contains(&magnitude) {
        return format!("{v:.3e}");
    }
    let decimals = (3 - magnitude).clamp(0, 8) as usize;
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

fn render_env(out: &mut String, report: &Report) {
    let s = &report.summary;
    let mut keys: Vec<&str> = s
        .base_env
        .iter()
        .chain(&s.head_env)
        .map(|(k, _)| k.as_str())
        .collect();
    keys.sort_unstable();
    keys.dedup();
    if keys.is_empty() {
        return;
    }
    let get = |env: &[(String, String)], k: &str| {
        env.iter()
            .find(|(key, _)| key == k)
            .map_or("—".to_string(), |(_, v)| v.replace('|', "\\|"))
    };
    let _ = writeln!(out, "<details>\n<summary>Environment</summary>\n");
    if report.has_base {
        let _ = writeln!(out, "| | Base | Head |\n|---|---|---|");
        for k in keys {
            let _ = writeln!(
                out,
                "| {k} | {} | {} |",
                get(&s.base_env, k),
                get(&s.head_env, k)
            );
        }
    } else {
        let _ = writeln!(out, "| | Head |\n|---|---|");
        for k in keys {
            let _ = writeln!(out, "| {k} | {} |", get(&s.head_env, k));
        }
    }
    let _ = writeln!(out, "\n</details>\n");
}

fn render_method(out: &mut String, report: &Report) {
    if !report.has_base {
        let _ = writeln!(
            out,
            "<sub>Head-only report: add a `base` to see changes.</sub>"
        );
        return;
    }
    let s = &report.summary;
    let mw = report.count_method(Method::MannWhitney);
    let iv = report.count_method(Method::Interval);
    let th = report.count_method(Method::Threshold);
    let mut parts = Vec::new();
    if mw > 0 {
        parts.push(format!(
            "{mw} by Mann-Whitney U test (α={}, ≥4 samples per side)",
            s.alpha
        ));
    }
    if iv > 0 {
        parts.push(format!("{iv} by non-overlapping intervals"));
    }
    if th > 0 {
        parts.push(format!("{th} by threshold only (point estimates)"));
    }
    let method = if parts.is_empty() {
        "no common benchmarks".to_string()
    } else {
        parts.join(", ")
    };
    let floor = if s.min_abs > 0.0 {
        format!(" and ≥ {} absolute", fmt_num(s.min_abs))
    } else {
        String::new()
    };
    let stats = if s.use_stats {
        ""
    } else {
        " Significance tests are off."
    };
    let _ = writeln!(
        out,
        "<sub>A change counts when it is ≥ {}%{floor} and, where the data allows, statistically significant. Judged: {method}.{stats}</sub>",
        fmt_num(s.threshold_pct)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::{compare, Options};
    use crate::model::{Better, Metric, MetricSet};

    fn set(items: &[(&str, &str, f64)]) -> MetricSet {
        MetricSet {
            env: [("sha".to_string(), "abc".to_string())].into(),
            metrics: items
                .iter()
                .map(|(g, n, v)| Metric::new(g, n, "", "ms", *v, Better::Lower))
                .collect(),
        }
    }

    fn render(base: Option<&MetricSet>, head: &MetricSet, opts: &Options) -> String {
        render_markdown(&compare(base, head, opts))
    }

    #[test]
    fn report_should_start_with_the_sticky_marker_for_its_tag() {
        let md = render(
            None,
            &set(&[("g", "a", 1.0)]),
            &Options {
                tag: "perf".into(),
                ..Options::default()
            },
        );
        assert!(md.starts_with("<!-- benchdiff:perf -->\n"), "{md}");
    }

    #[test]
    fn summary_line_should_count_every_verdict() {
        let base = set(&[
            ("g", "reg", 100.0),
            ("g", "imp", 100.0),
            ("g", "same", 100.0),
            ("g", "gone", 1.0),
        ]);
        let head = set(&[
            ("g", "reg", 150.0),
            ("g", "imp", 50.0),
            ("g", "same", 100.0),
            ("g", "fresh", 1.0),
        ]);
        let md = render(Some(&base), &head, &Options::default());
        assert!(
            md.contains("**1 regressed · 1 improved · 1 unchanged · 1 new · 1 removed**"),
            "{md}"
        );
    }

    #[test]
    fn changes_show_signed_percent_icon_and_bar() {
        let md = render(
            Some(&set(&[("g", "a", 100.0)])),
            &set(&[("g", "a", 125.0)]),
            &Options::default(),
        );
        assert!(md.contains("🔴 +25.0% ▰▰▰▱▱"), "{md}");
        let md = render(
            Some(&set(&[("g", "a", 100.0)])),
            &set(&[("g", "a", 90.0)]),
            &Options::default(),
        );
        assert!(md.contains("🟢 -10.0% ▰▱▱▱▱"), "{md}");
    }

    #[test]
    fn unchanged_rows_are_collapsed_and_omitted_with_only_changed() {
        let base = set(&[("g", "a", 100.0), ("g", "b", 100.0)]);
        let head = set(&[("g", "a", 200.0), ("g", "b", 100.0)]);
        let md = render(Some(&base), &head, &Options::default());
        assert!(md.contains("<summary>1 unchanged</summary>"), "{md}");
        let md = render(
            Some(&base),
            &head,
            &Options {
                only_changed: true,
                ..Options::default()
            },
        );
        assert!(!md.contains("unchanged</summary>"), "{md}");
        assert!(md.contains("`a`") && !md.contains("`b`"), "{md}");
    }

    #[test]
    fn max_rows_caps_changed_rows_and_says_how_many_were_cut() {
        let base = set(&[("g", "a", 100.0), ("g", "b", 100.0), ("g", "c", 100.0)]);
        let head = set(&[("g", "a", 300.0), ("g", "b", 200.0), ("g", "c", 150.0)]);
        let md = render(
            Some(&base),
            &head,
            &Options {
                max_rows: Some(2),
                ..Options::default()
            },
        );
        assert!(md.contains("…and 1 more changed rows not shown"), "{md}");
        assert!(
            md.contains("`a`") && md.contains("`b`") && !md.contains("`c`"),
            "{md}"
        );
    }

    #[test]
    fn groups_get_their_own_table() {
        let md = render(
            None,
            &set(&[("alpha", "a", 1.0), ("beta", "b", 2.0)]),
            &Options::default(),
        );
        assert!(
            md.contains("#### `alpha`") && md.contains("#### `beta`"),
            "{md}"
        );
    }

    #[test]
    fn head_only_report_lists_values_and_explains_the_missing_base() {
        let md = render(None, &set(&[("g", "a", 1.5)]), &Options::default());
        assert!(
            md.contains("**1 benchmarks** (no base to compare with)"),
            "{md}"
        );
        assert!(md.contains("1.5 ms") && md.contains("add a `base`"), "{md}");
    }

    #[test]
    fn environment_table_pairs_base_and_head() {
        let mut base = set(&[("g", "a", 1.0)]);
        base.env.insert("sha".into(), "base123".into());
        let md = render(Some(&base), &set(&[("g", "a", 1.0)]), &Options::default());
        assert!(md.contains("| sha | base123 | abc |"), "{md}");
    }

    #[test]
    fn method_note_names_the_test_that_ran() {
        let mk = |vals: &[f64]| {
            let mut m = Metric::new(
                "g",
                "a",
                "",
                "ms",
                crate::stats::median(vals),
                Better::Lower,
            );
            m.samples = vals.to_vec();
            MetricSet {
                env: Default::default(),
                metrics: vec![m],
            }
        };
        let md = render(
            Some(&mk(&[10.0, 10.1, 10.2, 10.3, 10.4])),
            &mk(&[20.0, 20.1, 20.2, 20.3, 20.4]),
            &Options::default(),
        );
        assert!(md.contains("by Mann-Whitney U test"), "{md}");
        assert!(md.contains("p=0.008 (n=5/5)"), "{md}");
        let md = render(
            Some(&set(&[("g", "a", 1.0)])),
            &set(&[("g", "a", 2.0)]),
            &Options::default(),
        );
        assert!(md.contains("by threshold only"), "{md}");
    }

    #[test]
    fn pipes_and_backticks_in_names_cannot_break_the_table() {
        let md = render(None, &set(&[("g", "a|b`c", 1.0)]), &Options::default());
        assert!(md.contains("`a\\|b'c`"), "{md}");
    }

    #[test]
    fn fmt_num_uses_four_significant_digits() {
        assert_eq!(fmt_num(0.0), "0");
        assert_eq!(fmt_num(1234.5678), "1235");
        assert_eq!(fmt_num(12.3456), "12.35");
        assert_eq!(fmt_num(0.012345), "0.01235");
        assert_eq!(fmt_num(1.5), "1.5");
        assert_eq!(fmt_num(2.0e15), "2.000e15");
    }

    #[test]
    fn title_is_rendered_as_the_heading() {
        let md = render(
            None,
            &set(&[("g", "a", 1.0)]),
            &Options {
                title: "Perf".into(),
                ..Options::default()
            },
        );
        assert!(md.contains("### Perf\n"), "{md}");
    }
}
