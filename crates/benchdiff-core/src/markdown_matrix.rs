//! Markdown report for an N-way comparison: heading, headline, reference line, the matrix
//! table, legend and Method note. Deterministic output only (no clock).
//!
//! Wording rule (ADR-005): only each variant against the reference is tested, so prose says
//! "slower/faster than reference" and "has the best geomean", never "regression" or "fastest".

use crate::compare::{Method, Options, Verdict};
use crate::markdown::{
    chart_notices, code, esc, flatten, fmt_num, marker, render_chart_blocks, RenderExtras,
};
use crate::matrix::{matrix_failures, FailedCell, FailureKind};
use crate::matrix::{
    Cell, CellFormat, Comparison, Correction, GeomeanFamily, MatrixReport, MatrixRow,
    MatrixSummary, PresentCell,
};
use crate::model::Better;
use crate::notice::{Notice, NoticeClass};
use std::fmt::Write;

/// Longest Holm-adjusted list in the Method note.
const HOLM_LISTED: usize = 10;
/// A change of at least this many times is written in words (`40.5x slower`).
const WORDS_FROM: f64 = 2.0;

/// Renders `report` as sticky-comment Markdown.
pub fn render_matrix_markdown(report: &MatrixReport) -> String {
    render_matrix_markdown_with(report, &RenderExtras::default())
}

/// Like [`render_matrix_markdown`], plus chart placeholders, text blocks and chart notices.
pub fn render_matrix_markdown_with(report: &MatrixReport, extras: &RenderExtras) -> String {
    let s = &report.summary;
    let mut out = String::new();
    let _ = writeln!(out, "{}", marker(&s.tag));
    let _ = writeln!(out, "{}\n", heading(report));
    if let Some(line) = headline(report) {
        let _ = writeln!(out, "{line}\n");
    }
    if let Some(line) = reference_line(s) {
        let _ = writeln!(out, "{line}\n");
    }
    let failures = gate_failures(report);
    if let Some(line) = gate_line(s, failures.as_deref()) {
        let _ = writeln!(out, "{line}\n");
    }
    let mut notices = collect_notices(report);
    notices.extend(chart_notices(extras));
    out.push_str(&render_notices(&notices));
    out.push_str(&render_chart_blocks(extras));

    let cols = display_columns(report);
    let sel = select_rows(report);
    if !sel.shown.is_empty() || !report.geomean.is_empty() {
        let used = render_table(&mut out, report, &cols, &sel.shown);
        let _ = writeln!(out, "\n{}", legend(&used));
    }
    if sel.capped > 0 {
        let _ = writeln!(
            out,
            "\n_…and {} more rows not shown (`max-rows: {}`); the JSON report has them all._",
            sel.capped,
            sel.shown.len()
        );
    }
    if sel.quiet > 0 {
        let noun = if sel.quiet == 1 { "row" } else { "rows" };
        let _ = writeln!(
            out,
            "\n_{} {noun} with no significant difference hidden (`only-changed`)._",
            sel.quiet
        );
    }
    if cols.len() < report.columns.len() {
        let _ = writeln!(
            out,
            "\n_...and {} more variants not shown (see JSON)_",
            report.columns.len() - cols.len()
        );
    }
    out.push('\n');
    render_method(
        &mut out,
        report,
        &notices,
        failures.as_deref().unwrap_or_default(),
    );
    out
}

fn heading(report: &MatrixReport) -> String {
    let s = &report.summary;
    let n = report.columns.len();
    let noun = if n == 1 { "variant" } else { "variants" };
    let mut line = String::from("### ");
    let title = esc(&s.title);
    if !title.trim().is_empty() {
        let _ = write!(line, "{title}: ");
    }
    let _ = write!(line, "{n} {noun}");
    if let Some(r) = &s.reference {
        let _ = write!(line, " (reference: {})", esc(&r.label));
    }
    line
}

fn reference_line(s: &MatrixSummary) -> Option<String> {
    let r = s.reference.as_ref()?;
    let how = if r.explicit {
        "set by 'reference'"
    } else {
        "first listed; set 'reference' to change"
    };
    Some(format!("reference: {} ({how})", esc(&r.label)))
}

fn ref_col(report: &MatrixReport) -> Option<usize> {
    let label = &report.summary.reference.as_ref()?.label;
    report.columns.iter().position(|c| c.as_str() == label)
}

/// Column indices to show: the reference first, the rest in input order, at most `columns_cap`.
fn display_columns(report: &MatrixReport) -> Vec<usize> {
    let r = ref_col(report);
    let mut cols: Vec<usize> = r.into_iter().collect();
    cols.extend((0..report.columns.len()).filter(|c| Some(*c) != r));
    cols.truncate(report.summary.columns_cap.max(1));
    cols
}

fn family_name(f: &GeomeanFamily) -> &str {
    if f.measure.is_empty() {
        &f.unit
    } else {
        &f.measure
    }
}

fn in_family(row: &MatrixRow, f: &GeomeanFamily) -> bool {
    row.measure == f.measure && row.unit == f.unit
}

fn usable(cell: &Cell) -> bool {
    matches!(cell, Cell::Present(p) if !matches!(p.vs_reference, Comparison::Invalid(_)))
}

fn is_regressed(cell: &Cell) -> bool {
    matches!(
        cell,
        Cell::Present(PresentCell {
            vs_reference: Comparison::Compared {
                verdict: Verdict::Regressed,
                ..
            },
            ..
        })
    )
}

/// The first line after the title: who has the best geomean, or why nobody is named.
fn headline(report: &MatrixReport) -> Option<String> {
    let fam = report.primary_geomean()?;
    let h = &fam.headline;
    let r = ref_col(report);
    let mut line = if let Some(best) = h.best.filter(|_| h.comparable) {
        let label = esc(report.columns[best].as_str());
        let scope = format!(
            "over {}/{} common {} rows",
            h.common_n,
            h.total,
            esc(family_name(fam))
        );
        if Some(best) == r {
            format!("**{label}** (the reference) has the best geomean; no variant is better overall ({scope})")
        } else {
            let value = h
                .values
                .iter()
                .find(|v| v.column == best)
                .and_then(|v| v.value)
                .map_or("—".to_string(), |v| format!("{v:.2}x"));
            let against = r.map_or(String::new(), |r| {
                format!(" vs reference {}", esc(report.columns[r].as_str()))
            });
            let slower = report
                .rows
                .iter()
                .filter(|row| in_family(row, fam) && row.cells.iter().all(usable))
                .filter(|row| is_regressed(&row.cells[best]))
                .count();
            format!(
                "**{label}** has the best geomean ({value}{against}, {scope}; variants are not tested against each other); slower than reference in {slower} of {} rows",
                h.common_n
            )
        }
    } else {
        match &h.least_covered {
            Some((label, n)) => format!(
                "**not comparable**: {} covers {n}/{} benchmarks (no overall winner)",
                esc(label),
                h.total
            ),
            None => format!(
                "**not comparable**: only {}/{} benchmarks are shared by every variant (no overall winner)",
                h.common_n, h.total
            ),
        }
    };
    let incomplete = incomplete_rows(report);
    if incomplete > 0 {
        let _ = write!(line, " · {incomplete} incomplete");
    }
    Some(line)
}

/// Benchmarks where some non-reference variant did not run.
fn incomplete_rows(report: &MatrixReport) -> usize {
    let r = ref_col(report);
    report
        .rows
        .iter()
        .filter(|row| {
            row.cells
                .iter()
                .enumerate()
                .any(|(c, cell)| Some(c) != r && matches!(cell, Cell::Missing))
        })
        .count()
}

// ---- row selection (3.2.4a swaps this for the shared `select_rows`) ----

struct RowSelection<'a> {
    shown: Vec<&'a MatrixRow>,
    /// Rows beyond `max_rows`.
    capped: usize,
    /// Rows `only_changed` left out.
    quiet: usize,
}

fn is_changed(row: &MatrixRow) -> bool {
    row.cells.iter().any(|c| match c {
        Cell::Present(p) => match &p.vs_reference {
            Comparison::Compared { verdict, .. } => {
                matches!(verdict, Verdict::Regressed | Verdict::Improved)
            }
            Comparison::Invalid(_) | Comparison::DirectionMismatch => true,
            _ => false,
        },
        Cell::Missing => false,
    })
}

fn select_rows(report: &MatrixReport) -> RowSelection<'_> {
    let s = &report.summary;
    let (mut shown, quiet): (Vec<&MatrixRow>, Vec<&MatrixRow>) = if s.only_changed {
        report.rows.iter().partition(|r| is_changed(r))
    } else {
        (report.rows.iter().collect(), Vec::new())
    };
    let capped = s.max_rows.map_or(0, |n| shown.len().saturating_sub(n));
    shown.truncate(shown.len() - capped);
    RowSelection {
        shown,
        capped,
        quiet: quiet.len(),
    }
}

// ---- table ----

/// Which glyphs the table used; the legend names only those.
#[derive(Default)]
struct Used {
    tilde: bool,
    missing: bool,
}

fn render_table(
    out: &mut String,
    report: &MatrixReport,
    cols: &[usize],
    rows: &[&MatrixRow],
) -> Used {
    let r = ref_col(report);
    let mut used = Used::default();
    let multi_measure = report.geomean.len() > 1
        || report
            .rows
            .iter()
            .any(|row| row.measure != report.rows[0].measure);
    let mut header = String::from("| Benchmark |");
    for &c in cols {
        let tag = if Some(c) == r { " (ref)" } else { "" };
        let _ = write!(header, " {}{tag} |", esc(report.columns[c].as_str()));
    }
    header.push_str(" Rank |");
    let _ = writeln!(out, "{header}");
    let _ = writeln!(out, "|---|{}---|", "---|".repeat(cols.len()));
    for row in rows {
        let _ = write!(out, "| {} |", row_label(row, multi_measure));
        for &c in cols {
            let bold = row.ranking.len() > 1 && row.ranking.first() == Some(&c);
            let text = cell_text(row, c, report.summary.cell_format, bold, &mut used);
            let _ = write!(out, " {text} |");
        }
        let rank = row
            .ranking
            .first()
            .map_or(String::new(), |c| esc(report.columns[*c].as_str()));
        let _ = writeln!(out, " {rank} |");
    }
    for fam in &report.geomean {
        render_geomean_row(out, report, fam, cols);
    }
    used
}

fn row_label(row: &MatrixRow, multi_measure: bool) -> String {
    let mut label = row.label();
    if !row.group.is_empty() {
        label = format!("{}/{label}", row.group);
    }
    if multi_measure && !row.measure.is_empty() {
        label = format!("{label} ({})", row.measure);
    }
    code(&label)
}

fn render_geomean_row(
    out: &mut String,
    report: &MatrixReport,
    fam: &GeomeanFamily,
    cols: &[usize],
) {
    let h = &fam.headline;
    let label = if h.comparable {
        format!(
            "geomean ({}, n={}/{})",
            esc(family_name(fam)),
            h.common_n,
            h.total
        )
    } else {
        format!("geomean ({})", esc(family_name(fam)))
    };
    let _ = write!(out, "| {label} |");
    for &c in cols {
        let g = if h.comparable {
            &h.values[c]
        } else {
            &fam.per_variant[c]
        };
        let text = match g.value {
            None => "—".to_string(),
            Some(v) if h.comparable || g.n_used == g.n_total => format!("{v:.2}x"),
            Some(v) => format!("{v:.2}x (n={}/{})", g.n_used, g.n_total),
        };
        let _ = write!(out, " {text} |");
    }
    let _ = writeln!(out, " |");
    let _ = report;
}

fn cell_text(
    row: &MatrixRow,
    col: usize,
    format: CellFormat,
    bold: bool,
    used: &mut Used,
) -> String {
    let Cell::Present(p) = &row.cells[col] else {
        used.missing = true;
        return "—".into();
    };
    let strong = |t: String| if bold { format!("**{t}**") } else { t };
    let absolute = || format!("{} {}", fmt_num(p.value), esc(&row.unit));
    match &p.vs_reference {
        Comparison::Reference => strong(absolute()),
        Comparison::Invalid(_) => "⚠️ invalid".into(),
        Comparison::Incomparable(_) | Comparison::DirectionMismatch => absolute(),
        Comparison::NoReference => match format {
            CellFormat::Full => format!("{} (—)", strong(absolute())),
            CellFormat::Ratio => "—".into(),
        },
        Comparison::Compared { verdict, delta, .. } => {
            let icon = match verdict {
                Verdict::Regressed => "🔴",
                Verdict::Improved => "🟢",
                _ => {
                    used.tilde = true;
                    "~"
                }
            };
            let rel = relative(row.better, *delta);
            match format {
                CellFormat::Full => format!("{} ({rel}) {icon}", strong(absolute())),
                CellFormat::Ratio => format!("{} {icon}", strong(rel)),
            }
        }
    }
}

/// `0.74x`; in words (`40.5x slower`) when the change is at least 2x either way.
fn relative(better: Better, delta: Option<f64>) -> String {
    let Some(delta) = delta else {
        return "n/a".into();
    };
    let ratio = 1.0 + delta / 100.0;
    if ratio <= 0.0 || !ratio.is_finite() {
        return "n/a".into();
    }
    let oriented = if better == Better::Lower {
        ratio
    } else {
        1.0 / ratio
    };
    if oriented >= WORDS_FROM {
        format!("{oriented:.1}x slower")
    } else if oriented <= 1.0 / WORDS_FROM {
        format!("{:.1}x faster", 1.0 / oriented)
    } else {
        format!("{ratio:.2}x")
    }
}

fn legend(used: &Used) -> String {
    let mut parts = vec![
        "🟢 faster than reference".to_string(),
        "🔴 slower than reference".to_string(),
    ];
    if used.tilde {
        parts.push("~ no significant difference".into());
    }
    if used.missing {
        parts.push("— not run".into());
    }
    parts.push(
        "**bold** and Rank order the values in a row; only each variant against the reference is tested, not variants against each other"
            .into(),
    );
    format!("_{}_", parts.join(" · "))
}

// ---- notices and the gate ----

/// Notices rendered inline before the rest collapse into one overflow line.
const NOTICES_INLINE: usize = 5;
/// Longest `gate:` line, in characters.
const GATE_LINE_MAX: usize = 200;
/// Cells named in the `gate:` line.
const GATE_NAMED: usize = 3;
/// Longest `variant on row` name in the `gate:` line, before it is trimmed further to fit.
const GATE_NAME_MAX: usize = 40;
/// Failing cells listed in the Method note.
const METHOD_FAILURES: usize = 50;
/// Names listed in a reference-absent notice.
const ABSENT_LISTED: usize = 10;

/// Stable sort by class: the fixed display order, insertion order within a class.
fn sorted(notices: &[Notice]) -> Vec<&Notice> {
    let mut v: Vec<&Notice> = notices.iter().collect();
    v.sort_by_key(|n| n.class);
    v
}

/// One `⚠️ text` paragraph per notice in class order; past the fifth, one overflow line.
/// Empty input renders nothing.
pub(crate) fn render_notices(notices: &[Notice]) -> String {
    let mut out = String::new();
    let all = sorted(notices);
    for n in all.iter().take(NOTICES_INLINE) {
        let _ = writeln!(out, "⚠️ {}\n", esc_notice(&n.text));
    }
    let more = all.len().saturating_sub(NOTICES_INLINE);
    if more > 0 {
        let noun = if more == 1 { "notice" } else { "notices" };
        let _ = writeln!(out, "⚠️ {more} more {noun} (see Method)\n");
    }
    out
}

/// Every notice the report implies: the ones raised while building it, plus skipped metrics,
/// rows the reference lacks and invalid cells.
fn collect_notices(report: &MatrixReport) -> Vec<Notice> {
    let mut out = report.notices.clone();
    for v in &report.summary.skipped {
        if v.total == 0 {
            continue;
        }
        let shown: Vec<String> = v
            .skipped
            .iter()
            .take(ABSENT_LISTED)
            .map(|l| code(l))
            .collect();
        let first = if v.total > shown.len() {
            format!(" (first {})", shown.len())
        } else {
            String::new()
        };
        out.push(Notice {
            class: NoticeClass::Skipped,
            text: format!(
                "{} metric(s) in {} had no usable value and were ignored: {}{first}",
                v.total,
                v.label,
                shown.join(", ")
            ),
        });
    }
    if let (Some(r), n @ 1..) = (&report.summary.reference, report.reference_absent.len()) {
        let names: Vec<&str> = report
            .reference_absent
            .iter()
            .take(ABSENT_LISTED)
            .map(String::as_str)
            .collect();
        let tail = if n > names.len() {
            format!(" (first {ABSENT_LISTED}, and {} more)", n - names.len())
        } else {
            String::new()
        };
        let noun = if n == 1 { "benchmark" } else { "benchmarks" };
        out.push(Notice {
            class: NoticeClass::ReferenceAbsent,
            text: format!(
                "reference \"{}\" absent from {n} {noun}: {}{tail}; set 'reference' to another variant",
                r.label,
                names.join(", ")
            ),
        });
    }
    let per_variant: Vec<(String, usize)> = report
        .columns
        .iter()
        .enumerate()
        .filter_map(|(c, label)| {
            let n = report
                .rows
                .iter()
                .filter(|row| {
                    matches!(&row.cells[c], Cell::Present(p) if matches!(p.vs_reference, Comparison::Invalid(_)))
                })
                .count();
            (n > 0).then(|| (label.to_string(), n))
        })
        .collect();
    let total: usize = per_variant.iter().map(|(_, n)| n).sum();
    if total > 0 {
        let noun = if total == 1 { "cell" } else { "cells" };
        let by: Vec<String> = per_variant
            .iter()
            .map(|(l, n)| format!("{l}: {n}"))
            .collect();
        out.push(Notice {
            class: NoticeClass::Invalid,
            text: format!("{total} invalid {noun} ({})", by.join(", ")),
        });
    }
    out
}

/// Failing cells when the gate applies: `fail-on-regression` set against an explicit reference.
/// A defaulted reference never gates (the option validator rejects it; this is the backstop).
fn gate_failures(report: &MatrixReport) -> Option<Vec<FailedCell>> {
    let s = &report.summary;
    let explicit = s.reference.as_ref().is_some_and(|r| r.explicit);
    s.fail_on_regression_pct
        .filter(|_| explicit)
        .map(|limit| matrix_failures(report, Some(limit)))
}

/// Keeps the head and tail of `text`, replacing the middle with `…`, in at most `max` chars.
fn middle_cut(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    let keep = max.saturating_sub(1);
    let head = keep.div_ceil(2);
    let tail = keep - head;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

fn failure_name(f: &FailedCell, max: usize) -> String {
    let name = esc(&middle_cut(
        &flatten(&format!("{} on {}", f.variant, f.row)),
        max,
    ));
    match f.kind {
        FailureKind::Invalid => format!("{name} invalid"),
        FailureKind::Regressed { .. } => name,
    }
}

/// `gate: fail-on-regression 10% vs quick: passed | FAILED (...)`; `None` without a gate.
fn gate_line(s: &MatrixSummary, failures: Option<&[FailedCell]>) -> Option<String> {
    let failures = failures?;
    let limit = s.fail_on_regression_pct?;
    let reference = esc(&s.reference.as_ref()?.label);
    let head = format!(
        "gate: fail-on-regression {}% vs {reference}:",
        fmt_num(limit)
    );
    if failures.is_empty() {
        return Some(format!("{head} passed"));
    }
    let n = failures.len();
    let noun = if n == 1 { "cell" } else { "cells" };
    let more = n.saturating_sub(GATE_NAMED);
    // Trim the names, never the count, until the line fits.
    let build = |max: usize| {
        let names: Vec<String> = failures
            .iter()
            .take(GATE_NAMED)
            .map(|f| failure_name(f, max))
            .collect();
        let tail = if more > 0 {
            format!(" and {more} more; full list in Method")
        } else {
            String::new()
        };
        format!("{head} FAILED ({n} {noun}: {}{tail})", names.join(", "))
    };
    let mut max = GATE_NAME_MAX;
    let mut line = build(max);
    while line.chars().count() > GATE_LINE_MAX && max > 8 {
        max -= 1;
        line = build(max);
    }
    Some(line)
}

fn method_failures(failures: &[FailedCell]) -> String {
    let items: Vec<String> = failures
        .iter()
        .take(METHOD_FAILURES)
        .map(|f| {
            let name = failure_name(f, usize::MAX);
            match f.kind {
                FailureKind::Regressed { pct: Some(p) } => format!("{name} ({p:+.1}%)"),
                _ => name,
            }
        })
        .collect();
    let more = failures.len().saturating_sub(METHOD_FAILURES);
    let tail = if more > 0 {
        format!(" and {more} more; the JSON report has them all")
    } else {
        String::new()
    };
    format!(
        "Gate failures ({}): {}{tail}.",
        failures.len(),
        items.join("; ")
    )
}

/// The report for a run that could not compare anything: sticky marker, title, the notices
/// in class order, all of them (there is no Method note to collapse into). No verdict, and the
/// gate does not apply.
pub fn render_notice_report(opts: &Options, notices: &[Notice]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", marker(&opts.tag));
    let title = esc(&opts.title);
    let _ = writeln!(
        out,
        "### {}\n",
        if title.trim().is_empty() {
            "benchdiff"
        } else {
            &title
        }
    );
    for n in sorted(notices) {
        let _ = writeln!(out, "⚠️ {}\n", esc_notice(&n.text));
    }
    out
}

// ---- Method note ----

fn render_method(
    out: &mut String,
    report: &MatrixReport,
    notices: &[Notice],
    failures: &[FailedCell],
) {
    let s = &report.summary;
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
    let correction = match s.correction {
        Correction::Holm => format!(
            " Holm-adjusted across {} per benchmark; reference-only.",
            match report.columns.len().saturating_sub(1) {
                1 => "1 comparison".to_string(),
                n => format!("{n} comparisons"),
            }
        ),
        Correction::None => " No multiple-comparison correction is applied.".to_string(),
    };
    let ratio = if s.cell_format == CellFormat::Ratio {
        " Cells show ratio vs reference; absolute values are in the JSON."
    } else {
        ""
    };
    let _ = writeln!(out, "<details>\n<summary>Method</summary>\n");
    let _ = writeln!(
        out,
        "A cell is slower or faster than the reference only when it differs by ≥ {}%{floor} and, where the data allows, the difference is statistically significant against the reference column.{correction}{stats}{}{ratio}{}{}",
        fmt_num(s.threshold_pct),
        method_facts(s),
        missing_sentence(report),
        holm_sentence(report),
    );
    if !failures.is_empty() {
        let _ = writeln!(out, "\n{}", method_failures(failures));
    }
    if !notices.is_empty() {
        let _ = writeln!(out, "\nNotices:\n");
        for n in sorted(notices) {
            let _ = writeln!(out, "- ⚠️ {}", esc_notice(&n.text));
        }
    }
    let _ = writeln!(out, "\n</details>");
}

/// Same sentences as the two-way Method note for `threshold_overrides` and `config_sources`.
fn method_facts(s: &MatrixSummary) -> String {
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

fn missing_sentence(report: &MatrixReport) -> String {
    let r = ref_col(report);
    let items: Vec<String> = report
        .columns
        .iter()
        .enumerate()
        .filter(|(c, _)| Some(*c) != r)
        .filter_map(|(c, label)| {
            let n = report
                .rows
                .iter()
                .filter(|row| matches!(row.cells[c], Cell::Missing))
                .count();
            (n > 0).then(|| {
                let noun = if n == 1 { "benchmark" } else { "benchmarks" };
                format!("{} is missing {n} {noun}", esc(label.as_str()))
            })
        })
        .collect();
    if items.is_empty() {
        String::new()
    } else {
        format!(" {}.", items.join("; "))
    }
}

/// Cells whose raw p passed alpha but whose Holm-adjusted p did not. The text is derived here
/// from `p_adjusted` and the row's family size; `Method` has no Holm variant.
fn holm_sentence(report: &MatrixReport) -> String {
    let alpha = report.summary.alpha;
    let mut items = Vec::new();
    for row in &report.rows {
        let m = row.holm_m();
        if m <= 1 {
            continue;
        }
        for (c, cell) in row.cells.iter().enumerate() {
            if let Cell::Present(PresentCell {
                vs_reference:
                    Comparison::Compared {
                        method: Some(Method::MannWhitney),
                        p_value: Some(p),
                        p_adjusted: Some(adj),
                        ..
                    },
                ..
            }) = cell
            {
                if *p < alpha && adj.get() >= alpha {
                    items.push(format!(
                        "{} on {}: mann-whitney, Holm-adjusted p={:.2} (m={m})",
                        esc(report.columns[c].as_str()),
                        code(&row.label()),
                        adj.get()
                    ));
                }
            }
        }
    }
    if items.is_empty() {
        return String::new();
    }
    let more = items.len().saturating_sub(HOLM_LISTED);
    items.truncate(HOLM_LISTED);
    let tail = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    format!(
        " Raw p was below α but the adjusted p was not: {}{tail}.",
        items.join("; ")
    )
}

/// Notice text is built from untrusted names but written by us, so only what could turn it into
/// a link, tag, table cell or emphasis is escaped; `(s)` and `(first 10)` stay readable.
fn esc_notice(text: &str) -> String {
    let mut out = String::new();
    for ch in flatten(text).chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '|' | '@' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::Options;
    use crate::matrix::{compare_matrix, MatrixOptions, Reference, Variant, VariantLabel};
    use crate::model::{Metric, MetricSet};

    fn variant(label: &str, items: &[(&str, f64)]) -> Variant {
        Variant {
            label: VariantLabel::parse(label).unwrap(),
            set: MetricSet {
                metrics: items
                    .iter()
                    .map(|(n, v)| Metric::new("", n, "ns/op", "ns", *v, Better::Lower))
                    .collect(),
                ..Default::default()
            },
        }
    }

    fn with_samples(label: &str, items: &[(&str, f64)]) -> Variant {
        let mut v = variant(label, items);
        for m in &mut v.set.metrics {
            m.samples = (0..10)
                .map(|k| m.value * (1.0 + 0.001 * (k as f64 - 5.0)))
                .collect();
        }
        v
    }

    fn render(variants: &[Variant], opts: &Options, mopts: &MatrixOptions) -> String {
        render_matrix_markdown(&compare_matrix(variants, opts, mopts))
    }

    fn mopts() -> MatrixOptions {
        MatrixOptions {
            compare_by: "impl".into(),
            ..MatrixOptions::default()
        }
    }

    fn names(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("Bench{i:02}")).collect()
    }

    /// quick 100, heap 128, merge 81 on every row; `merge` is 3x slower on the first
    /// `merge_slow` rows.
    fn fifteen(merge_slow: usize) -> Vec<Variant> {
        let rows = names(15);
        let col = |label: &str, f: &dyn Fn(usize) -> f64| {
            let items: Vec<(&str, f64)> = rows
                .iter()
                .enumerate()
                .map(|(i, n)| (n.as_str(), f(i)))
                .collect();
            variant(label, &items)
        };
        vec![
            col("quick", &|_| 100.0),
            col("heap", &|_| 128.0),
            col("merge", &|i| if i < merge_slow { 300.0 } else { 60.0 }),
        ]
    }

    #[test]
    fn render_matrix_markdown_should_say_best_geomean_not_fastest_and_count_slower_than_reference_rows(
    ) {
        let md = render(&fifteen(2), &Options::default(), &mopts());
        let line = md
            .lines()
            .find(|l| l.contains("has the best geomean"))
            .unwrap();
        assert!(
            line.starts_with("**merge** has the best geomean ("),
            "{line}"
        );
        assert!(
            line.contains("vs reference quick, over 15/15 common ns/op rows; variants are not tested against each other); slower than reference in 2 of 15 rows"),
            "{line}"
        );
        // 0 of 15 is a valid reading.
        let md = render(&fifteen(0), &Options::default(), &mopts());
        assert!(md.contains("slower than reference in 0 of 15 rows"), "{md}");
        // The reference is best.
        let mut v = fifteen(0);
        v[2] = variant(
            "merge",
            &names(15)
                .iter()
                .map(|n| (n.as_str(), 150.0))
                .collect::<Vec<_>>(),
        );
        v[1] = variant(
            "heap",
            &names(15)
                .iter()
                .map(|n| (n.as_str(), 120.0))
                .collect::<Vec<_>>(),
        );
        let md = render(&v, &Options::default(), &mopts());
        assert!(
            md.contains("**quick** (the reference) has the best geomean; no variant is better overall (over 15/15 common ns/op rows)"),
            "{md}"
        );
    }

    #[test]
    fn render_matrix_markdown_should_state_reference_and_print_not_comparable_when_common_set_under_half(
    ) {
        let mut v = fifteen(0);
        v[2] = variant(
            "merge",
            &names(4)
                .iter()
                .map(|n| (n.as_str(), 90.0))
                .collect::<Vec<_>>(),
        );
        let md = render(&v, &Options::default(), &mopts());
        assert!(md.contains("reference: quick (first listed; set 'reference' to change)"));
        assert!(
            md.contains("**not comparable**: merge covers 4/15 benchmarks (no overall winner)"),
            "{md}"
        );
        assert!(md.contains(" · 11 incomplete"), "{md}");
        assert!(md.contains("merge is missing 11 benchmarks"), "{md}");
        let m = MatrixOptions {
            reference: Reference::Label(VariantLabel::parse("heap").unwrap()),
            ..mopts()
        };
        let md = render(&v, &Options::default(), &m);
        assert!(md.contains("reference: heap (set by 'reference')"));
        assert!(md.contains("(reference: heap)"));
    }

    #[test]
    fn render_matrix_markdown_should_render_em_dash_for_missing_and_warning_invalid_for_invalid() {
        let mut report = compare_matrix(&fifteen(0), &Options::default(), &mopts());
        report.rows[0].cells[2] = Cell::Missing;
        if let Cell::Present(p) = &mut report.rows[1].cells[1] {
            p.vs_reference = Comparison::Invalid(crate::matrix::Reason("no finite value".into()));
        }
        let md = render_matrix_markdown(&report);
        assert!(md.contains("| — |"), "{md}");
        assert!(md.contains("| ⚠️ invalid |"), "{md}");
        assert!(md.contains("— not run"), "{md}");
        assert!(!md.contains("| - |") && !md.contains("| 0 |"));
    }

    #[test]
    fn render_matrix_markdown_should_render_legend_line_directly_under_table_with_only_used_glyphs()
    {
        let md = render(&fifteen(2), &Options::default(), &mopts());
        let legend = md
            .lines()
            .find(|l| l.starts_with("_🟢"))
            .expect("legend line");
        assert_eq!(
            legend,
            "_🟢 faster than reference · 🔴 slower than reference · **bold** and Rank order the values in a row; only each variant against the reference is tested, not variants against each other_"
        );
        // Only blank space separates the last table line from the legend.
        let lines: Vec<&str> = md.lines().collect();
        let at = lines.iter().position(|l| *l == legend).unwrap();
        assert_eq!(lines[at - 1], "");
        assert!(lines[at - 2].starts_with("| geomean"));
    }

    #[test]
    fn render_matrix_markdown_should_render_full_and_compact_ratio_cells() {
        let v = vec![
            variant("quick", &[("Sort", 1630.0)]),
            variant("merge", &[("Sort", 1210.0)]),
        ];
        let full = render(&v, &Options::default(), &mopts());
        assert!(
            full.contains("| `Sort` | 1630 ns | **1210 ns** (0.74x) 🟢 | merge |"),
            "{full}"
        );
        assert!(!full.contains("cells show ratio"));
    }

    #[test]
    fn render_matrix_markdown_should_render_compact_ratio_cells_when_cell_format_ratio() {
        let v = vec![
            variant("quick", &[("Sort", 1630.0), ("Parse", 100.0)]),
            variant("merge", &[("Sort", 1210.0), ("Parse", 400.0)]),
        ];
        let m = MatrixOptions {
            cell_format: CellFormat::Ratio,
            ..mopts()
        };
        let md = render(&v, &Options::default(), &m);
        assert!(md.contains("| 1630 ns | **0.74x** 🟢 |"), "{md}");
        assert!(md.contains("| **100 ns** | 4.0x slower 🔴 |"), "{md}");
        assert!(md.contains("Cells show ratio vs reference; absolute values are in the JSON."));
    }

    #[test]
    fn render_matrix_markdown_should_use_summary_tag_title_max_rows_and_only_changed() {
        let rows = names(47);
        let items = |f: f64| -> Vec<(&str, f64)> { rows.iter().map(|n| (n.as_str(), f)).collect() };
        let mut heap: Vec<(&str, f64)> = items(100.0);
        heap[0].1 = 300.0; // one changed row
        let v = vec![variant("std", &items(100.0)), variant("heap", &heap)];
        let opts = Options {
            title: "T".into(),
            tag: "x".into(),
            max_rows: Some(20),
            ..Options::default()
        };
        let md = render(&v, &opts, &mopts());
        assert_eq!(md.lines().next(), Some(marker("x").as_str()));
        assert!(md.contains("### T: 2 variants (reference: std)"), "{md}");
        assert!(
            md.contains(
                "_…and 27 more rows not shown (`max-rows: 20`); the JSON report has them all._"
            ),
            "{md}"
        );
        assert_eq!(md.lines().filter(|l| l.starts_with("| `Bench")).count(), 20);
        assert!(!md.contains("only-changed"));

        let opts = Options {
            only_changed: true,
            ..opts
        };
        let md = render(&v, &opts, &mopts());
        assert!(
            md.contains("_46 rows with no significant difference hidden (`only-changed`)._"),
            "{md}"
        );
        assert_eq!(md.lines().filter(|l| l.starts_with("| `Bench")).count(), 1);
        assert!(!md.contains("more rows not shown"));
    }

    #[test]
    fn render_matrix_markdown_should_show_eight_columns_and_notice_when_eleven_variants() {
        let v: Vec<Variant> = (0..11)
            .map(|i| variant(&format!("v{i:02}"), &[("Sort", 100.0 + i as f64)]))
            .collect();
        let md = render(&v, &Options::default(), &mopts());
        let header = md.lines().find(|l| l.starts_with("| Benchmark")).unwrap();
        // Benchmark + 8 variant columns + Rank.
        assert_eq!(header.matches('|').count(), 11, "{header}");
        assert!(header.contains("v00 (ref)") && header.contains("v07"));
        assert!(!header.contains("v08"));
        assert!(
            md.contains("_...and 3 more variants not shown (see JSON)_"),
            "{md}"
        );
    }

    #[test]
    fn render_matrix_markdown_should_derive_holm_text_and_method_sentences() {
        let mk = |label: &str, v: f64| with_samples(label, &[("Sort", v)]);
        let v = vec![mk("quick", 100.0), mk("heap", 104.0), mk("merge", 104.2)];
        let opts = Options {
            threshold_overrides: [("Sort".to_string(), 2.0)].into(),
            min_abs: 1.5,
            ..Options::default()
        };
        let mut report = compare_matrix(&v, &opts, &mopts());
        report.summary.config_sources = vec![crate::compare::ConfigSource {
            key: "threshold".into(),
            value: "3".into(),
            source: "config".into(),
        }];
        let md = render_matrix_markdown(&report);
        assert!(md.contains("<summary>Method</summary>"));
        assert!(md.contains("≥ 5% and ≥ 1.5 absolute"), "{md}");
        assert!(md.contains("Holm-adjusted across 2 comparisons per benchmark; reference-only."));
        assert!(md.contains("Per-metric thresholds: Sort 2%."), "{md}");
        assert!(
            md.contains("Settings not at their defaults: threshold=3 (config)."),
            "{md}"
        );
        // No m>1 downgrade here, so no Holm text.
        assert!(!md.contains("mann-whitney, Holm-adjusted p="));

        // Force a Holm downgrade: tweak p-values on a hand-built report.
        let mut report = compare_matrix(&v, &Options::default(), &mopts());
        for c in 1..3 {
            if let Cell::Present(PresentCell {
                vs_reference:
                    Comparison::Compared {
                        verdict,
                        method,
                        p_value,
                        p_adjusted,
                        ..
                    },
                ..
            }) = &mut report.rows[0].cells[c]
            {
                *verdict = Verdict::Unchanged;
                *method = Some(Method::MannWhitney);
                *p_value = Some(if c == 1 { 0.04 } else { 0.03 });
                *p_adjusted = Some(crate::matrix::PAdjusted::new(0.06));
            }
        }
        let md = render_matrix_markdown(&report);
        assert!(
            md.contains("mann-whitney, Holm-adjusted p=0.06 (m=2)"),
            "{md}"
        );
    }

    #[test]
    fn render_matrix_markdown_should_escape_hostile_names_when_untrusted_fields() {
        let hostile = "x|[a](http://evil)<img src=x>@all`--> <!-- benchdiff:default -->\n# h";
        let mut v = vec![
            variant("quick", &[("Sort", 100.0)]),
            variant("heap", &[("Sort", 130.0)]),
        ];
        v[0].set.metrics[0].name = hostile.into();
        v[0].set.metrics[0].unit = hostile.into();
        v[1].set.metrics[0].name = hostile.into();
        v[1].set.metrics[0].unit = hostile.into();
        let opts = Options {
            title: hostile.into(),
            ..Options::default()
        };
        let md = render(&v, &opts, &mopts());
        assert_eq!(md.matches("<!-- benchdiff:").count(), 1, "{md}");
        let heading = md.lines().nth(1).unwrap();
        assert!(
            !heading.contains("](http") && !heading.contains("<img"),
            "{heading}"
        );
        for line in md.lines().filter(|l| l.starts_with("| `")) {
            let cells = line.split("` |").nth(1).unwrap();
            assert!(
                !cells.contains("](http") && !cells.contains("<img"),
                "{line}"
            );
        }
        assert!(md.lines().all(|l| !l.starts_with("# h")), "{md}");
    }

    #[test]
    fn render_matrix_markdown_should_not_use_higher_lower_regress_fastest_or_loses_in_prose() {
        for cf in [CellFormat::Full, CellFormat::Ratio] {
            let m = MatrixOptions {
                cell_format: cf,
                ..mopts()
            };
            let mut report = compare_matrix(&fifteen(2), &Options::default(), &m);
            report.rows[0].cells[2] = Cell::Missing;
            let md = render_matrix_markdown(&report).to_lowercase();
            let words: Vec<&str> = md.split(|c: char| !c.is_alphabetic()).collect();
            for word in ["higher", "lower", "fastest", "loses"] {
                assert!(!words.contains(&word), "{word} in {md}");
            }
            assert!(!md.contains("regress"), "{md}");
        }
    }

    fn notice(class: NoticeClass, text: &str) -> Notice {
        Notice {
            class,
            text: text.into(),
        }
    }

    fn explicit(label: &str) -> MatrixOptions {
        MatrixOptions {
            reference: Reference::Label(VariantLabel::parse(label).unwrap()),
            ..mopts()
        }
    }

    fn seven_notices() -> Vec<Notice> {
        vec![
            notice(NoticeClass::Chart, "c1"),
            notice(NoticeClass::Invalid, "i1"),
            notice(NoticeClass::Degradation, "d1"),
            notice(NoticeClass::Skipped, "s1"),
            notice(NoticeClass::Degradation, "d2"),
            notice(NoticeClass::ReferenceAbsent, "r1"),
            notice(NoticeClass::Skipped, "s2"),
        ]
    }

    #[test]
    fn render_notices_should_order_by_class_cap_at_five_and_never_precede_the_verdict() {
        let got = render_notices(&seven_notices());
        let lines: Vec<&str> = got.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(
            lines,
            [
                "⚠️ d1",
                "⚠️ d2",
                "⚠️ s1",
                "⚠️ s2",
                "⚠️ r1",
                "⚠️ 2 more notices (see Method)"
            ]
        );
        assert_eq!(render_notices(&[]), "");

        let mut report = compare_matrix(&fifteen(2), &Options::default(), &mopts());
        report.notices = seven_notices();
        let md = render_matrix_markdown(&report);
        let content: Vec<&str> = md.lines().filter(|l| !l.is_empty()).collect();
        assert!(content[1].starts_with("### "), "{md}");
        assert!(
            content[2].starts_with("**merge** has the best geomean"),
            "{md}"
        );
        assert!(content[3].starts_with("reference: "), "{md}");
        assert_eq!(content[4], "⚠️ d1", "{md}");
        // Method lists all seven, in the same order.
        let method = md.split("<summary>Method</summary>").nth(1).unwrap();
        let listed: Vec<&str> = method.lines().filter(|l| l.starts_with("- ⚠️ ")).collect();
        assert_eq!(
            listed,
            [
                "- ⚠️ d1",
                "- ⚠️ d2",
                "- ⚠️ s1",
                "- ⚠️ s2",
                "- ⚠️ r1",
                "- ⚠️ i1",
                "- ⚠️ c1"
            ]
        );
    }

    #[test]
    fn ux_s1_11_notices_should_follow_fixed_order_cap_at_five_and_collapse_overflow_into_method() {
        let mut v = fifteen(0);
        v[1].set.metrics.retain(|m| m.name != "Bench00");
        let mut report = compare_matrix(&v, &Options::default(), &mopts());
        report.reference_absent = vec!["Zed".into()];
        if let Cell::Present(p) = &mut report.rows[1].cells[2] {
            p.vs_reference = Comparison::Invalid(crate::matrix::Reason("bad".into()));
        }
        report.summary.skipped = vec![crate::matrix::VariantSkipped {
            label: "heap".into(),
            skipped: vec!["Odd".into()],
            total: 1,
        }];
        report.notices = vec![notice(NoticeClass::Chart, "chart one")];
        let md = render_matrix_markdown(&report);
        let at = |needle: &str| {
            md.find(needle)
                .unwrap_or_else(|| panic!("{needle} in {md}"))
        };
        let skipped = at("⚠️ 1 metric(s) in heap");
        let absent = at("⚠️ reference \"quick\" absent from 1 benchmark: Zed; set 'reference'");
        let invalid = at("⚠️ 1 invalid cell (merge: 1)");
        let chart = at("⚠️ chart one");
        assert!(
            skipped < absent && absent < invalid && invalid < chart,
            "{md}"
        );
        assert!(at("reference: quick") < skipped);
        assert!(!md.contains("more notice"));
    }

    fn gate_report(extra_invalid: bool) -> MatrixReport {
        let rows = ["Sort/size=100", "Sort/size=10", "Parse"];
        let col = |label: &str, f: f64| {
            let items: Vec<(&str, f64)> = rows.iter().map(|r| (*r, f)).collect();
            variant(label, &items)
        };
        let v = vec![col("quick", 100.0), col("heap", 130.0), col("merge", 300.0)];
        let opts = Options {
            fail_on_regression_pct: Some(10.0),
            ..Options::default()
        };
        let mut report = compare_matrix(&v, &opts, &explicit("quick"));
        if extra_invalid {
            let last = report.rows.len() - 1;
            if let Cell::Present(p) = &mut report.rows[last].cells[2] {
                p.vs_reference = Comparison::Invalid(crate::matrix::Reason("bad".into()));
            }
        }
        report
    }

    #[test]
    fn gate_line_should_name_one_failing_cell_and_pass_otherwise() {
        let opts = Options {
            fail_on_regression_pct: Some(10.0),
            ..Options::default()
        };
        let v = vec![
            variant("quick", &[("Sort/size=100", 100.0)]),
            variant("heap", &[("Sort/size=100", 130.0)]),
        ];
        let md = render(&v, &opts, &explicit("quick"));
        let lines: Vec<&str> = md.lines().collect();
        let r = lines
            .iter()
            .position(|l| l.starts_with("reference: "))
            .unwrap();
        assert_eq!(
            lines[r + 2],
            "gate: fail-on-regression 10% vs quick: FAILED (1 cell: heap on Sort/size=100)"
        );
        let v = vec![
            variant("quick", &[("Sort/size=100", 100.0)]),
            variant("heap", &[("Sort/size=100", 101.0)]),
        ];
        let md = render(&v, &opts, &explicit("quick"));
        assert!(
            md.contains("\ngate: fail-on-regression 10% vs quick: passed\n"),
            "{md}"
        );
        // No flag: no gate line. Defaulted reference: the gate never applies.
        assert!(!render(&v, &Options::default(), &explicit("quick")).contains("gate:"));
        assert!(!render(&v, &opts, &mopts()).contains("gate:"));
    }

    #[test]
    fn gate_line_should_name_three_cells_cut_each_to_forty_chars_cap_at_two_hundred_and_list_all_in_method_when_seven_fail(
    ) {
        // 3 rows x 2 slower variants = 6 cells, one of them Invalid; a cloned row with one
        // Missing cell adds a seventh.
        let mut report = gate_report(true);
        let mut extra = report.rows[0].clone();
        extra.cells[2] = Cell::Missing;
        report.rows.push(extra);
        let md = render_matrix_markdown(&report);
        let gate = md.lines().find(|l| l.starts_with("gate:")).unwrap();
        assert!(
            gate.starts_with("gate: fail-on-regression 10% vs quick: FAILED (7 cells: "),
            "{gate}"
        );
        assert!(
            gate.ends_with(" and 4 more; full list in Method)"),
            "{gate}"
        );
        assert_eq!(gate.matches(" on ").count(), 3, "{gate}");
        assert!(gate.chars().count() <= 200);
        let method = md.split("<summary>Method</summary>").nth(1).unwrap();
        let list = method
            .lines()
            .find(|l| l.starts_with("Gate failures (7): "))
            .unwrap();
        assert_eq!(list.matches(" on ").count(), 7, "{list}");
        assert!(list.contains("merge on Parse invalid"), "{list}");

        // Long names are cut to 40 characters and the whole line stays within 200.
        let mut report = gate_report(false);
        let long = "L".repeat(120);
        for row in &mut report.rows {
            row.name = format!("{long}{}", row.name);
        }
        let md = render_matrix_markdown(&report);
        let gate = md.lines().find(|l| l.starts_with("gate:")).unwrap();
        assert!(gate.chars().count() <= 200, "{}", gate.chars().count());
        assert!(gate.contains("FAILED (6 cells: "), "{gate}");
        assert!(
            gate.contains('…') && gate.ends_with("and 3 more; full list in Method)"),
            "{gate}"
        );
        assert!(!gate.contains(&"L".repeat(41)), "{gate}");
    }

    #[test]
    fn middle_cut_should_keep_head_and_tail_within_max() {
        assert_eq!(middle_cut("short", 40), "short");
        let cut = middle_cut(
            &"a".repeat(30)
                .chars()
                .chain("b".repeat(30).chars())
                .collect::<String>(),
            40,
        );
        assert_eq!(cut.chars().count(), 40);
        assert!(cut.starts_with('a') && cut.ends_with('b') && cut.contains('…'));
    }

    #[test]
    fn render_notice_report_should_match_the_committed_notice_only_golden() {
        let got = render_notice_report(
            &Options::default(),
            &[notice(
                NoticeClass::Degradation,
                "reference 'quick' produced no data; no comparison made",
            )],
        );
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testdata/nway/expected/notice-only.md"
        );
        if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
            std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
            std::fs::write(path, &got).unwrap();
        }
        let want = std::fs::read_to_string(path).expect("golden file");
        assert_eq!(got, want);
        assert_eq!(got.lines().next(), Some(marker("default").as_str()));
        assert!(!got.contains("gate:") && !got.contains("geomean"));
    }

    #[test]
    fn render_notice_report_should_list_every_notice_in_class_order_and_escape_text() {
        let got = render_notice_report(
            &Options {
                title: String::new(),
                tag: "t".into(),
                ..Options::default()
            },
            &[
                notice(NoticeClass::Chart, "c"),
                notice(
                    NoticeClass::Degradation,
                    "[x](http://evil)\n<!-- benchdiff:t -->",
                ),
            ],
        );
        assert!(got.contains("### benchdiff\n"));
        assert_eq!(got.matches("<!-- benchdiff:").count(), 1, "{got}");
        assert!(!got.contains("[x]("), "{got}");
        assert!(
            got.find("⚠️ \\[x").unwrap() < got.find("⚠️ c").unwrap(),
            "{got}"
        );
    }

    #[test]
    fn render_matrix_markdown_should_match_the_committed_snapshot() {
        let rows = ["Sort/size=10", "Sort/size=100", "Parse", "Hash"];
        let col = |label: &str, vals: [f64; 4]| {
            let items: Vec<(&str, f64)> = rows.iter().copied().zip(vals).collect();
            with_samples(label, &items)
        };
        let v = vec![
            col("quick", [1630.0, 16300.0, 500.0, 90.0]),
            col("heap", [2100.0, 40000.0, 502.0, 90.0]),
            col("merge", [1210.0, 12000.0, 500.0, 91.0]),
        ];
        let mut v = v;
        v[1].set.metrics.retain(|m| m.name != "Hash");
        let report = compare_matrix(&v, &Options::default(), &mopts());
        let got = render_matrix_markdown(&report);
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/snapshots/matrix_markdown_basic.md"
        );
        if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(path, &got).unwrap();
        }
        let want = std::fs::read_to_string(path).expect("snapshot file");
        assert_eq!(got, want);
    }

    #[test]
    fn render_matrix_markdown_with_should_equal_plain_render_when_extras_default() {
        let v = fifteen(2);
        let report = compare_matrix(&v, &Options::default(), &mopts());
        assert_eq!(
            render_matrix_markdown_with(&report, &RenderExtras::default()),
            render_matrix_markdown(&report)
        );
    }

    #[test]
    fn render_matrix_markdown_with_should_place_image_after_notices_and_before_table() {
        use crate::markdown::ChartEmbed;
        use crate::svg::ChartKind;
        let v = fifteen(2);
        let report = compare_matrix(&v, &Options::default(), &mopts());
        let extras = RenderExtras {
            charts: vec![ChartEmbed {
                kind: ChartKind::Ranking,
                file: "ranking.svg".into(),
                alt: "Ratio vs quick".into(),
                text: Some("| # |\n|---|".into()),
                dark_file: None,
            }],
            embed: true,
            notices: vec![Notice {
                class: NoticeClass::Chart,
                text: "capped".into(),
            }],
            ..RenderExtras::default()
        };
        let md = render_matrix_markdown_with(&report, &extras);
        let note = md.find("⚠️ capped").unwrap();
        let img = md
            .find("![Ratio vs quick](benchdiff-svg:ranking.svg)")
            .unwrap();
        let det = md
            .find("<details><summary>Chart data (text)</summary>")
            .unwrap();
        let table = md.find("| Benchmark").unwrap();
        assert!(note < img && img < det && det < table, "{md}");
    }

    #[test]
    fn render_matrix_markdown_with_should_emit_picture_block_when_chart_has_light_and_dark_files() {
        use crate::markdown::ChartEmbed;
        use crate::svg::ChartKind;
        let v = fifteen(2);
        let report = compare_matrix(&v, &Options::default(), &mopts());
        let extras = RenderExtras {
            charts: vec![ChartEmbed {
                kind: ChartKind::Ranking,
                file: "ranking-light.svg".into(),
                alt: "Ratio vs quick".into(),
                text: None,
                dark_file: Some("ranking-dark.svg".into()),
            }],
            embed: true,
            ..RenderExtras::default()
        };
        let md = render_matrix_markdown_with(&report, &extras);
        assert!(
            md.contains("<picture><source media=\"(prefers-color-scheme: dark)\" srcset=\"benchdiff-svg:ranking-dark.svg\"><img alt=\"Ratio vs quick\" src=\"benchdiff-svg:ranking-light.svg\"></picture>"),
            "{md}"
        );
    }
}
