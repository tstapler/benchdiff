//! Markdown report: a sticky-comment marker, a summary line, per-group tables with a small
//! bar, unchanged rows collapsed, environment and method notes. Deterministic output only
//! (no clock), so native and wasm builds agree byte for byte.

use crate::compare::{Method, Report, Row, Summary, Verdict};
use crate::notice::{Notice, NoticeClass};
use crate::svg::{ChartKind, SkipReason};
use std::collections::HashMap;
use std::fmt::Write;

const BAR_CELLS: usize = 5;
/// Percent change at which the bar is full.
const BAR_FULL_PCT: f64 = 50.0;

/// The hidden marker that identifies this report's sticky comment. The tag is reduced to
/// `[alphanumeric - _ . : /]` (anything else becomes `_`) so it can neither close the comment
/// (`-->` needs a `>`) nor split across lines.
pub fn marker(tag: &str) -> String {
    let tag: String = tag
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '/') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("<!-- benchdiff:{tag} -->")
}

/// Control characters (newlines included) become spaces, so text stays on one line; HTML
/// comment delimiters get an invisible break so untrusted text can never forge a marker.
pub(crate) fn flatten(text: &str) -> String {
    let one_line: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    one_line
        .replace("<!--", "<!\u{200b}--")
        .replace("-->", "--\u{200b}>")
}

/// Untrusted text as an inline code span: links, mentions, HTML and headings stay literal.
pub(crate) fn code(text: &str) -> String {
    let flat = flatten(text);
    if flat.is_empty() {
        return "—".into();
    }
    format!("`{}`", flat.replace('`', "'").replace('|', "\\|"))
}

/// Untrusted text as plain Markdown: one line, markup characters backslash-escaped.
pub(crate) fn esc(text: &str) -> String {
    let mut out = String::new();
    for ch in flatten(text).chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '(' | ')' | '#' | '|' | '!' | '@' | '~' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

pub(crate) fn plural(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// Scheme of the image tokens core emits (`benchdiff-svg:NAME.svg`); `lib/images.js` mirrors it.
pub const PLACEHOLDER_SCHEME: &str = "benchdiff-svg:";

/// One chart to reference from the Markdown: the file `render_svgs` produced, its alt text and
/// the already-rendered body of its `Chart data (text)` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartEmbed {
    pub kind: ChartKind,
    /// SVG file name, e.g. `delta.svg`; the placeholder is `PLACEHOLDER_SCHEME` + this.
    pub file: String,
    pub alt: String,
    /// `svg::render_text_table` output; `None` emits the image without a text block.
    pub text: Option<String>,
    /// Dark twin of `file` (`chart-theme: both`); `Some` emits a `<picture>` block instead of `![]()`.
    pub dark_file: Option<String>,
}

/// A chart that produced no file, with the reason shown in the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedChart {
    pub kind: ChartKind,
    pub reason: SkipReason,
}

/// What the Markdown renderers add on top of a report: chart placeholders, text blocks and
/// chart notices. `default()` adds nothing, so output is byte-identical to the plain render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderExtras {
    pub charts: Vec<ChartEmbed>,
    pub skipped: Vec<SkippedChart>,
    /// Extra notices (class `Chart`), e.g. a capped chart.
    pub notices: Vec<Notice>,
    /// `svg-embed`: when false no image and no text block are emitted.
    pub embed: bool,
    /// When false the images are not hosted: no placeholder, a note line, text block opened.
    pub publish_images: bool,
    /// Where the SVG files were written, named in the not-embedded note.
    pub svg_dir: Option<String>,
}

impl Default for RenderExtras {
    fn default() -> Self {
        RenderExtras {
            charts: Vec::new(),
            skipped: Vec::new(),
            notices: Vec::new(),
            embed: false,
            publish_images: true,
            svg_dir: None,
        }
    }
}

fn skip_text(s: &SkippedChart) -> String {
    let kind = s.kind.name();
    match &s.reason {
        SkipReason::NothingToCompare => "chart skipped: nothing to compare".to_string(),
        SkipReason::NotApplicable { kind: k } => match k {
            ChartKind::Delta => "chart skipped: delta is two-way; use ranking".to_string(),
            ChartKind::Ranking => {
                "chart skipped: ranking needs an N-way run (compare-by or variants)".to_string()
            }
            ChartKind::Distribution => format!("chart skipped: {} is not applicable", k.name()),
        },
        SkipReason::NoSamples => {
            format!("{kind} chart skipped: no samples in the input (use -count=10 or Criterion)")
        }
        SkipReason::Empty => format!("chart skipped: {kind}: no rows to chart"),
        SkipReason::NonFinite => {
            format!("chart skipped: {kind}: a value is too large to draw (not finite)")
        }
        SkipReason::CapExceeded { shown, total } => {
            format!("chart skipped: {kind}: {total} series exceed the cap of {shown}")
        }
    }
}

/// Chart-class notices: the skipped charts, then the caller's own.
pub(crate) fn chart_notices(extras: &RenderExtras) -> Vec<Notice> {
    extras
        .skipped
        .iter()
        .map(|s| Notice {
            class: NoticeClass::Chart,
            text: skip_text(s),
        })
        .chain(extras.notices.iter().cloned())
        .collect()
}

/// Escapes text for a double-quoted HTML attribute (the `<picture>` form is raw HTML, not Markdown).
fn attr_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in flatten(text).chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

fn alt_escape(alt: &str) -> String {
    let mut out = String::new();
    for ch in flatten(alt).chars() {
        match ch {
            '\\' | '[' | ']' => {
                out.push('\\');
                out.push(ch);
            }
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            _ => out.push(ch),
        }
    }
    out
}

/// The image placeholders and `Chart data (text)` blocks, directly under the verdict block.
/// Empty unless `extras.embed` and there is a chart.
pub(crate) fn render_chart_blocks(extras: &RenderExtras) -> String {
    let mut out = String::new();
    if !extras.embed || extras.charts.is_empty() {
        return out;
    }
    if !extras.publish_images {
        out.push_str("charts not embedded: publish-images is false");
        if let Some(dir) = &extras.svg_dir {
            let _ = write!(out, "; SVGs written to {}", flatten(dir));
        }
        out.push_str("\n\n");
    }
    let several = extras.charts.len() > 1;
    for c in &extras.charts {
        if extras.publish_images {
            match &c.dark_file {
                Some(dark) => {
                    let _ = writeln!(
                        out,
                        "<picture><source media=\"(prefers-color-scheme: dark)\" srcset=\"{PLACEHOLDER_SCHEME}{dark}\"><img alt=\"{}\" src=\"{PLACEHOLDER_SCHEME}{}\"></picture>\n",
                        attr_escape(&c.alt),
                        c.file
                    );
                }
                None => {
                    let _ = writeln!(
                        out,
                        "![{}]({PLACEHOLDER_SCHEME}{})\n",
                        alt_escape(&c.alt),
                        c.file
                    );
                }
            }
        }
        if let Some(text) = &c.text {
            let open = if extras.publish_images { "" } else { " open" };
            let title = if several {
                format!("Chart data (text): {}", c.kind.name())
            } else {
                "Chart data (text)".to_string()
            };
            let _ = writeln!(
                out,
                "<details{open}><summary>{title}</summary>\n\n{}\n\n</details>\n",
                text.trim_end()
            );
        }
    }
    out
}

pub fn render_markdown(report: &Report) -> String {
    render_markdown_with(report, &RenderExtras::default())
}

pub fn render_markdown_with(report: &Report, extras: &RenderExtras) -> String {
    let s = &report.summary;
    let mut out = String::new();
    let _ = writeln!(out, "{}", marker(&s.tag));
    let _ = writeln!(out, "### {}\n", flatten(&s.title));
    let _ = writeln!(out, "{}\n", summary_line(report));
    render_skipped(&mut out, "head", &s.skipped_head, s.skipped_head_total);
    render_skipped(&mut out, "base", &s.skipped_base, s.skipped_base_total);
    out.push_str(&crate::markdown_matrix::render_notices(&chart_notices(
        extras,
    )));
    out.push_str(&render_chart_blocks(extras));

    let (changed, quiet): (Vec<&Row>, Vec<&Row>) =
        report.rows.iter().partition(|r| is_interesting(r));
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

const SKIPPED_SHOWN: usize = 10;

fn render_skipped(out: &mut String, side: &str, labels: &[String], total: usize) {
    if total == 0 {
        return;
    }
    let shown: Vec<String> = labels.iter().take(SKIPPED_SHOWN).map(|l| code(l)).collect();
    let more = if total > shown.len() {
        format!(" (first {})", shown.len())
    } else {
        String::new()
    };
    let _ = writeln!(
        out,
        "⚠️ {total} metric(s) in {side} had no usable value and were ignored: {}{more}\n",
        shown.join(", ")
    );
}

/// Without a base there is nothing to call "changed": show the numbers up front.
fn is_interesting(r: &Row) -> bool {
    r.verdict != Verdict::Unchanged
}

fn summary_line(report: &Report) -> String {
    if !report.has_base {
        return format!(
            "**{}** (no base to compare with)",
            plural(report.rows.len(), "benchmark")
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
    if n(Verdict::Invalid) > 0 {
        let _ = write!(line, " · {} invalid", n(Verdict::Invalid));
    }
    if n(Verdict::Incomparable) > 0 {
        let _ = write!(line, " · {} incomparable", n(Verdict::Incomparable));
    }
    line
}

fn render_groups(out: &mut String, rows: &[&Row], collapsed: bool, has_base: bool) {
    let mut groups: Vec<&str> = Vec::new();
    let mut by_group: HashMap<&str, Vec<&Row>> = HashMap::new();
    for &r in rows {
        by_group
            .entry(r.group.as_str())
            .or_insert_with(|| {
                groups.push(r.group.as_str());
                Vec::new()
            })
            .push(r);
    }
    let show_test = rows
        .iter()
        .any(|r| r.method.is_some_and(|m| m != Method::Threshold));
    for group in groups {
        let in_group = &by_group[group];
        if !group.is_empty() {
            let _ = if collapsed {
                writeln!(out, "**{}**\n", code(group))
            } else {
                writeln!(out, "#### {}\n", code(group))
            };
        }
        if !has_base {
            let _ = writeln!(out, "| Benchmark | Value | Range |\n|---|--:|--:|");
            for r in in_group {
                let _ = writeln!(
                    out,
                    "| {} | {} | {} |",
                    code(&r.label()),
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
                code(&r.label()),
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

pub(crate) fn value(v: Option<f64>, unit: &str) -> String {
    match v {
        None => "—".into(),
        Some(v) if unit.is_empty() => fmt_num(v),
        Some(v) => format!("{} {}", fmt_num(v), code(unit)),
    }
}

/// Signed percent with one decimal; a value that rounds to zero prints as plain `0.0%`.
pub(crate) fn fmt_pct(p: f64) -> String {
    let s = format!("{p:+.1}");
    if s.trim_start_matches(['+', '-']) == "0.0" {
        "0.0%".into()
    } else {
        format!("{s}%")
    }
}

fn change(r: &Row) -> String {
    let icon = match r.verdict {
        Verdict::Regressed => "🔴",
        Verdict::Improved => "🟢",
        Verdict::New => return "🆕 new".into(),
        Verdict::Removed => return "🗑️ removed".into(),
        Verdict::Incomparable => return "⚠️ units differ".into(),
        Verdict::Invalid => return "⛔ no valid value".into(),
        Verdict::Unchanged => "⚪",
    };
    match r.delta_pct {
        Some(p) => format!("{icon} {} {}", fmt_pct(p), bar(p, r.verdict)),
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
            .map_or("—".to_string(), |(_, v)| code(v))
    };
    let _ = writeln!(out, "<details>\n<summary>Environment</summary>\n");
    if report.has_base {
        let _ = writeln!(out, "| | Base | Head |\n|---|---|---|");
        for k in keys {
            let _ = writeln!(
                out,
                "| {} | {} | {} |",
                code(k),
                get(&s.base_env, k),
                get(&s.head_env, k)
            );
        }
    } else {
        let _ = writeln!(out, "| | Head |\n|---|---|");
        for k in keys {
            let _ = writeln!(out, "| {} | {} |", code(k), get(&s.head_env, k));
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
        "<sub>A change counts when it is ≥ {}%{floor} and, where the data allows, statistically significant. Judged: {method}.{stats}{}</sub>",
        fmt_num(s.threshold_pct),
        method_facts(s)
    );
}

/// Sentences for `Summary.threshold_overrides` and `config_sources`; empty when both are.
fn method_facts(s: &Summary) -> String {
    let mut out = String::new();
    if !s.threshold_overrides.is_empty() {
        let items: Vec<String> = s
            .threshold_overrides
            .iter()
            .map(|(k, v)| format!("{} {}%", esc(k), fmt_num(*v)))
            .collect();
        let _ = write!(out, " Per-metric thresholds: {}.", items.join(", "));
    }
    if !s.config_sources.is_empty() {
        let items: Vec<String> = s
            .config_sources
            .iter()
            .map(|c| format!("{}={} ({})", esc(&c.key), esc(&c.value), esc(&c.source)))
            .collect();
        let _ = write!(
            out,
            " Settings not at their defaults: {}.",
            items.join(", ")
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::{compare, Options};
    use crate::model::{Better, Metric, MetricSet};

    fn set(items: &[(&str, &str, f64)]) -> MetricSet {
        MetricSet {
            env: [("sha".to_string(), "abc".to_string())].into(),
            skipped: Default::default(),
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
            md.contains("**1 benchmark** (no base to compare with)"),
            "{md}"
        );
        assert!(
            md.contains("1.5 `ms`") && md.contains("add a `base`"),
            "{md}"
        );
        let md = render(
            None,
            &set(&[("g", "a", 1.0), ("g", "b", 2.0)]),
            &Options::default(),
        );
        assert!(md.contains("**2 benchmarks**"), "{md}");
    }

    #[test]
    fn environment_table_pairs_base_and_head() {
        let mut base = set(&[("g", "a", 1.0)]);
        base.env.insert("sha".into(), "base123".into());
        let md = render(Some(&base), &set(&[("g", "a", 1.0)]), &Options::default());
        assert!(md.contains("| `sha` | `base123` | `abc` |"), "{md}");
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
                skipped: Default::default(),
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

    const PAYLOAD: &str =
        "x\n</details>\n# FAKE `t` | [x](https://evil) @team <!-- benchdiff:default -->";

    /// Head and base each carry `PAYLOAD` in one field; returns the rendered report.
    fn render_with_payload(field: &str) -> String {
        let p = PAYLOAD;
        let mk = |value: f64| {
            let (mut group, mut name, mut measure, mut unit) = ("g", "n", "m", "ms");
            let mut env = std::collections::BTreeMap::new();
            match field {
                "group" => group = p,
                "name" => name = p,
                "measure" => measure = p,
                "unit" => unit = p,
                "env key" => {
                    env.insert(p.to_string(), "v".to_string());
                }
                _ => {
                    env.insert("k".to_string(), p.to_string());
                }
            }
            MetricSet {
                env,
                metrics: vec![Metric::new(
                    group,
                    name,
                    measure,
                    unit,
                    value,
                    Better::Lower,
                )],
                skipped: Default::default(),
            }
        };
        render(Some(&mk(100.0)), &mk(200.0), &Options::default())
    }

    #[test]
    fn untrusted_fields_cannot_inject_markup_or_forge_the_marker() {
        for field in ["group", "name", "measure", "unit", "env key", "env value"] {
            let md = render_with_payload(field);
            assert!(md.starts_with("<!-- benchdiff:default -->\n"), "{field}");
            assert_eq!(md.matches("<!-- benchdiff").count(), 1, "{field}: {md}");
            assert_eq!(md.matches("-->").count(), 1, "{field}: {md}");
            assert!(
                md.lines().all(|l| !l.starts_with("# FAKE")),
                "{field}: {md}"
            );
            for line in md.lines().filter(|l| l.contains("evil")) {
                // Inside a code span: opened before the link text, backticks balanced.
                assert!(line.find('`') < line.find("[x]"), "{field}: {line}");
                assert_eq!(line.matches('`').count() % 2, 0, "{field}: {line}");
                assert!(
                    !line.contains(" | [x]"),
                    "{field}: pipe not escaped: {line}"
                );
            }
        }
    }

    #[test]
    fn title_is_flattened_and_cannot_forge_the_marker() {
        let md = render(
            None,
            &set(&[("g", "a", 1.0)]),
            &Options {
                title: "A\n# FAKE <!-- benchdiff:default --> end".into(),
                ..Options::default()
            },
        );
        assert_eq!(md.matches("<!-- benchdiff").count(), 1, "{md}");
        assert!(md.lines().all(|l| !l.starts_with("# FAKE")), "{md}");
        assert!(md.lines().nth(1).unwrap().starts_with("### A "), "{md}");
    }

    #[test]
    fn marker_tag_cannot_close_the_comment_or_span_lines() {
        let m = marker("a b\n--> <x>\r");
        assert_eq!(m.lines().count(), 1, "{m}");
        assert_eq!(m.matches("-->").count(), 1, "{m}");
        assert!(m.ends_with(" -->") && !m["<!-- benchdiff:".len()..m.len() - 4].contains(' '));
        assert_eq!(marker("perf-1.x"), "<!-- benchdiff:perf-1.x -->");
    }

    #[test]
    fn two_thousand_groups_render_once_each_in_input_order() {
        let items: Vec<(String, String)> = (0..2000)
            .map(|i| (format!("grp{i}"), format!("bench{i}")))
            .collect();
        let head = MetricSet {
            env: Default::default(),
            skipped: Default::default(),
            metrics: items
                .iter()
                .map(|(g, n)| Metric::new(g, n, "", "ms", 1.0, Better::Lower))
                .collect(),
        };
        let md = render(
            None,
            &head,
            &Options {
                sort: crate::compare::Sort::Input,
                ..Options::default()
            },
        );
        assert_eq!(md.matches("#### `grp").count(), 2000);
        assert!(md.find("`grp0`").unwrap() < md.find("`grp1999`").unwrap());
    }

    #[test]
    fn percent_that_rounds_to_zero_has_no_sign() {
        assert_eq!(fmt_pct(0.04), "0.0%");
        assert_eq!(fmt_pct(-0.04), "0.0%");
        assert_eq!(fmt_pct(-0.06), "-0.1%");
        assert_eq!(fmt_pct(12.34), "+12.3%");
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

    #[test]
    fn summary_should_serialize_identically_and_method_note_should_print_nothing_extra_when_overrides_and_sources_empty(
    ) {
        let base = set(&[("g", "a", 100.0)]);
        let head = set(&[("g", "a", 150.0)]);
        let report = compare(Some(&base), &head, &Options::default());
        assert!(report.summary.threshold_overrides.is_empty());
        assert!(report.summary.config_sources.is_empty());
        let json = crate::render_json(&report);
        assert!(!json.contains("threshold_overrides"), "{json}");
        assert!(!json.contains("config_sources"), "{json}");
        let md = render_markdown(&report);
        assert!(!md.contains("Per-metric thresholds"), "{md}");
        assert!(!md.contains("Settings not at their defaults"), "{md}");
        assert!(
            md.contains(
                "statistically significant. Judged: 1 by threshold only (point estimates).</sub>\n"
            ),
            "{md}"
        );
    }

    #[test]
    fn render_method_should_list_threshold_overrides_and_non_default_sources_when_present() {
        let base = set(&[("net", "Parse", 100.0)]);
        let head = set(&[("net", "Parse", 150.0)]);
        let mut report = compare(Some(&base), &head, &Options::default());
        report
            .summary
            .threshold_overrides
            .insert("net/Parse".into(), 20.0);
        report.summary.config_sources = vec![
            crate::compare::ConfigSource {
                key: "threshold".into(),
                value: "3".into(),
                source: "config".into(),
            },
            crate::compare::ConfigSource {
                key: "sort".into(),
                value: "name".into(),
                source: "flag".into(),
            },
        ];
        let md = render_markdown(&report);
        assert!(md.contains("Per-metric thresholds: net/Parse 20%."), "{md}");
        assert!(
            md.contains("Settings not at their defaults: threshold=3 (config), sort=name (flag)."),
            "{md}"
        );
        let json = crate::render_json(&report);
        assert!(json.contains("\"threshold_overrides\""), "{json}");
        assert!(json.contains("\"config_sources\""), "{json}");
    }

    #[test]
    fn render_method_should_escape_hostile_threshold_keys_and_config_sources() {
        let base = set(&[("net", "Parse", 100.0)]);
        let head = set(&[("net", "Parse", 150.0)]);
        let mut report = compare(Some(&base), &head, &Options::default());
        for k in ["<!-- benchdiff:x -->", "</sub><img src=x>", "@user"] {
            report.summary.threshold_overrides.insert(k.into(), 5.0);
        }
        report.summary.config_sources = vec![crate::compare::ConfigSource {
            key: "</sub><img src=x>".into(),
            value: "@user".into(),
            source: "<!-- benchdiff:x -->".into(),
        }];
        let md = render_markdown(&report);
        assert!(!md.contains("<img"), "{md}");
        assert!(!md.contains("</sub><"), "{md}");
        assert!(!md.replace("\\@user", "").contains("@user"), "{md}");
        // Only the report's own sticky marker (first line) may appear.
        assert_eq!(md.matches("<!-- benchdiff:").count(), 1, "{md}");
        assert_eq!(
            md.matches("</sub>").count(),
            md.matches("<sub>").count(),
            "{md}"
        );
    }

    fn chart(text: Option<&str>) -> ChartEmbed {
        ChartEmbed {
            kind: ChartKind::Delta,
            file: "delta.svg".into(),
            alt: "Benchmark [change] <b>".into(),
            text: text.map(String::from),
            dark_file: None,
        }
    }

    fn themed(light: &str, dark: &str) -> ChartEmbed {
        ChartEmbed {
            file: light.into(),
            dark_file: Some(dark.into()),
            ..chart(None)
        }
    }

    #[test]
    fn render_markdown_with_should_emit_picture_block_when_chart_has_light_and_dark_files() {
        let mut c = themed("delta-light.svg", "delta-dark.svg");
        c.alt = "A & \"B\" <b> [x]".into();
        let md = render_markdown_with(&two_way(), &embedded(vec![c]));
        assert!(
            md.contains("<picture><source media=\"(prefers-color-scheme: dark)\" srcset=\"benchdiff-svg:delta-dark.svg\"><img alt=\"A &amp; &quot;B&quot; &lt;b&gt; [x]\" src=\"benchdiff-svg:delta-light.svg\"></picture>\n\n"),
            "{md}"
        );
        assert!(!md.contains("!["), "{md}");
    }

    #[test]
    fn render_markdown_with_should_emit_plain_image_when_chart_has_one_file() {
        let md = render_markdown_with(&two_way(), &embedded(vec![chart(None)]));
        assert!(md.contains("](benchdiff-svg:delta.svg)"), "{md}");
        assert!(!md.contains("<picture>"), "{md}");
    }

    #[test]
    fn render_markdown_with_should_emit_no_picture_when_publish_images_false() {
        let mut e = embedded(vec![themed("d-light.svg", "d-dark.svg")]);
        e.publish_images = false;
        let md = render_markdown_with(&two_way(), &e);
        assert!(
            !md.contains("<picture>") && !md.contains("benchdiff-svg:"),
            "{md}"
        );
    }

    fn two_way() -> Report {
        let base = set(&[("net", "Parse", 100.0)]);
        let head = set(&[("net", "Parse", 150.0)]);
        compare(Some(&base), &head, &Options::default())
    }

    fn embedded(charts: Vec<ChartEmbed>) -> RenderExtras {
        RenderExtras {
            charts,
            embed: true,
            ..RenderExtras::default()
        }
    }

    #[test]
    fn render_markdown_with_should_equal_render_markdown_when_extras_default() {
        let r = two_way();
        assert_eq!(
            render_markdown_with(&r, &RenderExtras::default()),
            render_markdown(&r)
        );
        // Charts present but embedding off adds nothing either.
        let off = RenderExtras {
            charts: vec![chart(Some("t"))],
            ..RenderExtras::default()
        };
        assert_eq!(render_markdown_with(&r, &off), render_markdown(&r));
    }

    #[test]
    fn render_markdown_with_should_emit_chart_data_text_details_after_image_when_embedded_and_none_when_no_chart(
    ) {
        let r = two_way();
        let md = render_markdown_with(&r, &embedded(vec![chart(Some("| # |\n|---|\n| 1 |"))]));
        let img = md
            .find("![Benchmark \\[change\\] &lt;b&gt;](benchdiff-svg:delta.svg)")
            .unwrap_or_else(|| panic!("{md}"));
        let det = md
            .find("<details><summary>Chart data (text)</summary>")
            .unwrap();
        let table = md.find("### ").unwrap();
        assert!(table < img && img < det, "{md}");
        assert!(md[det..].contains("| 1 |\n\n</details>"), "{md}");
        assert!(!md.contains("<details open>"));
        // Directly under the summary line, before any row table.
        assert!(img < md.find("| Benchmark").unwrap_or(usize::MAX), "{md}");
        let none = render_markdown_with(&r, &embedded(vec![]));
        assert!(!none.contains("Chart data"), "{none}");
        assert!(!none.contains(PLACEHOLDER_SCHEME), "{none}");
    }

    #[test]
    fn render_markdown_with_should_title_each_block_by_kind_when_several_charts() {
        let mut second = chart(Some("b"));
        second.kind = ChartKind::Distribution;
        second.file = "distribution.svg".into();
        let md = render_markdown_with(&two_way(), &embedded(vec![chart(Some("a")), second]));
        assert!(
            md.contains("<summary>Chart data (text): delta</summary>"),
            "{md}"
        );
        assert!(
            md.contains("<summary>Chart data (text): distribution</summary>"),
            "{md}"
        );
        assert!(md.contains("(benchdiff-svg:distribution.svg)"), "{md}");
    }

    #[test]
    fn render_markdown_with_should_emit_no_placeholder_and_note_line_when_publish_images_false() {
        let extras = RenderExtras {
            publish_images: false,
            svg_dir: Some("out/svg".into()),
            ..embedded(vec![chart(Some("rows"))])
        };
        let md = render_markdown_with(&two_way(), &extras);
        assert!(!md.contains(PLACEHOLDER_SCHEME), "{md}");
        assert!(
            md.contains("charts not embedded: publish-images is false; SVGs written to out/svg\n\n<details open><summary>Chart data (text)</summary>"),
            "{md}"
        );
        assert_eq!(md.matches("charts not embedded").count(), 1);
    }

    #[test]
    fn render_markdown_with_should_list_skipped_charts_with_reason_when_extras_contain_skips() {
        let extras = RenderExtras {
            skipped: vec![
                SkippedChart {
                    kind: ChartKind::Delta,
                    reason: SkipReason::NothingToCompare,
                },
                SkippedChart {
                    kind: ChartKind::Ranking,
                    reason: SkipReason::NotApplicable {
                        kind: ChartKind::Ranking,
                    },
                },
            ],
            ..RenderExtras::default()
        };
        let md = render_markdown_with(&two_way(), &extras);
        assert!(
            md.contains("⚠️ chart skipped: nothing to compare\n\n"),
            "{md}"
        );
        assert!(
            md.contains("⚠️ chart skipped: ranking needs an N-way run"),
            "{md}"
        );
        assert!(!md.contains(PLACEHOLDER_SCHEME));
    }

    #[test]
    fn render_markdown_with_should_render_no_samples_skip_as_specified_in_ux() {
        let extras = RenderExtras {
            skipped: vec![SkippedChart {
                kind: ChartKind::Distribution,
                reason: SkipReason::NoSamples,
            }],
            ..RenderExtras::default()
        };
        let md = render_markdown_with(&two_way(), &extras);
        assert!(
            md.contains("⚠️ distribution chart skipped: no samples in the input (use -count=10 or Criterion)\n"),
            "{md}"
        );
    }

    #[test]
    fn render_markdown_with_should_name_the_non_finite_skip_cause() {
        let extras = RenderExtras {
            skipped: vec![SkippedChart {
                kind: ChartKind::Distribution,
                reason: SkipReason::NonFinite,
            }],
            ..RenderExtras::default()
        };
        let md = render_markdown_with(&two_way(), &extras);
        assert!(
            md.contains(
                "⚠️ chart skipped: distribution: a value is too large to draw (not finite)\n"
            ),
            "{md}"
        );
    }

    #[test]
    fn ux_s1_2_bold_summary_should_be_first_line_after_heading_with_notice_and_gate() {
        let extras = RenderExtras {
            notices: vec![Notice {
                class: NoticeClass::Chart,
                text: "a notice".into(),
            }],
            ..RenderExtras::default()
        };
        let mut report = two_way();
        report.summary.fail_on_regression_pct = Some(10.0);
        let md = render_markdown_with(&report, &extras);
        assert!(md.contains("⚠️ a notice"), "{md}");
        let after: Vec<&str> = md
            .lines()
            .skip_while(|l| !l.starts_with("### "))
            .skip(1)
            .filter(|l| !l.is_empty())
            .collect();
        assert!(after[0].starts_with("**"), "{md}");
    }

    #[test]
    fn render_markdown_with_should_cap_chart_notices_at_five_lines_when_many() {
        let notices = (0..7)
            .map(|i| Notice {
                class: NoticeClass::Chart,
                text: format!("capped {i}"),
            })
            .collect();
        let md = render_markdown_with(
            &two_way(),
            &RenderExtras {
                notices,
                ..RenderExtras::default()
            },
        );
        assert_eq!(md.matches("⚠️ capped").count(), 5, "{md}");
        assert!(md.contains("⚠️ 2 more notices (see Method)"), "{md}");
    }

    #[test]
    fn placeholder_scheme_should_end_with_colon() {
        assert_eq!(PLACEHOLDER_SCHEME, "benchdiff-svg:");
    }
}
