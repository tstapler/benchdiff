//! N-way ranking chart: per benchmark, one bar per variant showing its ratio against the
//! reference on a log2 axis (task 3.2.2a).
//!
//! Like the delta chart this draws with attribute-based primitives only, so the Light card
//! carries no `<style>` (AC-S2-12). Every bar ends in a series-specific marker shape, so
//! colour is never the only cue; a Missing cell is a hollow marker with `—`, never a bar.

use super::style::{shape_for_series, Palette, Shape, Theme, SERIES_STROKE_WIDTH};
use super::text::{text_width, truncate_to_width};
use super::writer::{Anchor, NonFiniteError, PathBuilder, SvgWriter};
use super::{select_rows_where, ChartTheme, RowRef, SkipReason, SvgFile, SvgInput, SvgOptions};
use crate::matrix::{Cell, Comparison, MatrixReport, MatrixRow};
use crate::model::Better;

const LANE_H: f64 = 16.0;
const BAR_H: f64 = 8.0;
const MARKER_R: f64 = 5.0;
const ROW_GAP: f64 = 10.0;
const GUTTER: f64 = 220.0;
const PAD: f64 = 16.0;
const HEADER_H: f64 = 64.0;
const LEGEND_LINE_H: f64 = 18.0;
/// Room kept free at each end of the plot for the end marker and value label.
const END_ROOM: f64 = 84.0;
/// The axis spans at least 2x either way and at most 32x; bars beyond it are cut at the edge.
const MIN_SPAN: f64 = 1.0;
const MAX_SPAN: f64 = 5.0;
const FONT_PX: f64 = 12.0;
const ALT_MAX_CHARS: usize = 125;
const ALT_NAME_CHARS: usize = 16;
const DESC_TOP: usize = 5;

/// What one variant has to show for one benchmark.
#[derive(Clone, Copy)]
enum Slot {
    /// Value divided by the reference's, finite and positive.
    Ratio(f64),
    Missing,
    /// No usable ratio: invalid series, unit or direction mismatch, zero reference.
    Unusable,
}

struct Cluster<'a> {
    row: &'a MatrixRow,
    /// `N. name`, the chart position plus the row label.
    label: String,
    /// One per drawn series, in series order.
    slots: Vec<Slot>,
}

struct Plan<'a> {
    reference: &'a str,
    /// Column indices of the drawn series.
    series: Vec<usize>,
    labels: Vec<&'a str>,
    clusters: Vec<Cluster<'a>>,
    /// Labels of series beyond `max_series`.
    hidden: Vec<&'a str>,
    /// `+ N more not shown` when the row cap cut rows.
    more: Option<String>,
    skipped: Option<SkipReason>,
}

/// Renders the ranking chart for the theme in `opts` (`Both` renders the light file, named
/// `ranking-light`; the dispatcher calls `ranking_chart_themed` for the dark twin).
///
/// Draws the rows `select_rows` picks (largest |delta| first, capped at `min(chart-rows, max-rows)`) and at most
/// `opts.max_series` variants. `Err(NothingToCompare)` without a variant to compare,
/// `Err(Empty)` when no row has a usable reference value.
#[allow(dead_code)] // wired up by render_svgs (3.3.1b)
pub fn ranking_chart(report: &MatrixReport, opts: &SvgOptions) -> Result<SvgFile, SkipReason> {
    let theme = match opts.theme {
        ChartTheme::Dark => Theme::Dark,
        ChartTheme::Light | ChartTheme::Both => Theme::Light,
    };
    ranking_chart_themed(report, opts, theme)
}

/// As `ranking_chart`, with the palette chosen by the caller; the file name carries a
/// `-light`/`-dark` suffix only when `opts.theme` is `Both`.
#[allow(dead_code)] // wired up by render_svgs (3.3.1b)
pub fn ranking_chart_themed(
    report: &MatrixReport,
    opts: &SvgOptions,
    theme: Theme,
) -> Result<SvgFile, SkipReason> {
    let plan = plan(report, opts)?;
    let pal = Palette::for_theme(theme);
    let svg = render(&plan, opts.width.get() as f64, pal).map_err(|_| SkipReason::Empty)?;
    let name = match (opts.theme, theme) {
        (ChartTheme::Both, Theme::Light) => "ranking-light",
        (ChartTheme::Both, Theme::Dark) => "ranking-dark",
        _ => "ranking",
    };
    Ok(SvgFile {
        name: name.to_string(),
        svg,
        alt: alt_text(&plan),
        skipped: plan.skipped.clone(),
    })
}

fn plan<'a>(report: &'a MatrixReport, opts: &SvgOptions) -> Result<Plan<'a>, SkipReason> {
    let reference = report
        .summary
        .reference
        .as_ref()
        .map(|r| r.label.as_str())
        .ok_or(SkipReason::NothingToCompare)?;
    let ref_col = report
        .columns
        .iter()
        .position(|c| c.as_str() == reference)
        .ok_or(SkipReason::NothingToCompare)?;
    let others: Vec<usize> = (0..report.columns.len())
        .filter(|&c| c != ref_col)
        .collect();
    if others.is_empty() {
        return Err(SkipReason::NothingToCompare);
    }
    let shown = others.len().min(opts.max_series.max(1));
    let series = others[..shown].to_vec();
    let hidden = others[shown..]
        .iter()
        .map(|&c| report.columns[c].as_str())
        .collect();

    // Rows without a usable reference value are not charted (the text block counts them).
    let has_ref = |row: &RowRef| {
        matches!(row, RowRef::Matrix(r)
            if matches!(r.cells.get(ref_col), Some(Cell::Present(p)) if p.value.is_finite() && p.value > 0.0))
    };
    let selected = select_rows_where(&SvgInput::Matrix(report), opts, &has_ref);
    let clusters: Vec<Cluster> = selected
        .rows
        .iter()
        .filter_map(|sel| {
            let RowRef::Matrix(row) = sel.row else {
                return None;
            };
            let ref_value = match row.cells.get(ref_col)? {
                Cell::Present(p) if p.value.is_finite() && p.value > 0.0 => p.value,
                _ => return None,
            };
            let slots = series
                .iter()
                .map(|&c| slot(row.cells.get(c), ref_value))
                .collect();
            Some(Cluster {
                row,
                label: sel.label.clone(),
                slots,
            })
        })
        .collect();
    if clusters.is_empty() {
        return Err(SkipReason::Empty);
    }
    Ok(Plan {
        reference,
        labels: series.iter().map(|&c| report.columns[c].as_str()).collect(),
        series,
        clusters,
        hidden,
        more: selected.footer(),
        skipped: selected.skip_reason(),
    })
}

fn slot(cell: Option<&Cell>, ref_value: f64) -> Slot {
    match cell {
        None | Some(Cell::Missing) => Slot::Missing,
        Some(Cell::Present(p)) => {
            let comparable = matches!(
                p.vs_reference,
                Comparison::Compared { .. } | Comparison::Reference
            );
            let ratio = p.value / ref_value;
            if comparable && ratio.is_finite() && ratio > 0.0 {
                Slot::Ratio(ratio)
            } else {
                Slot::Unusable
            }
        }
    }
}

fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        noun.to_string()
    } else {
        format!("{noun}s")
    }
}

/// `0.74x`, `12.3x`, `412x`: two significant decimals while small.
fn fmt_ratio(r: f64) -> String {
    if r < 10.0 {
        format!("{r:.2}x")
    } else if r < 100.0 {
        format!("{r:.1}x")
    } else {
        format!("{r:.0}x")
    }
}

/// The ratio oriented so that smaller is better, whatever the row's direction.
fn oriented(ratio: f64, better: Better) -> f64 {
    match better {
        Better::Lower => ratio,
        Better::Higher => 1.0 / ratio,
    }
}

/// Keeps both ends of `s` within `max` chars, joining them with an ellipsis.
fn shorten_middle(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    let head = keep.div_ceil(2);
    let tail = keep - head;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

/// Per drawn series: geometric mean of the oriented ratios over the benchmarks it has.
fn geomeans(plan: &Plan) -> Vec<Option<f64>> {
    (0..plan.series.len())
        .map(|k| {
            let logs: Vec<f64> = plan
                .clusters
                .iter()
                .filter_map(|c| match c.slots[k] {
                    Slot::Ratio(r) => Some(oriented(r, c.row.better).ln()),
                    _ => None,
                })
                .collect();
            (!logs.is_empty()).then(|| (logs.iter().sum::<f64>() / logs.len() as f64).exp())
        })
        .collect()
}

/// `(series, cluster, ratio)` of every drawn ratio, largest log-distance from 1.0 first;
/// ties keep input order.
fn gaps<'a>(plan: &'a Plan) -> Vec<(usize, &'a Cluster<'a>, f64)> {
    let mut all: Vec<(usize, &Cluster, f64)> = plan
        .clusters
        .iter()
        .flat_map(|c| {
            c.slots
                .iter()
                .enumerate()
                .filter_map(move |(k, s)| match s {
                    Slot::Ratio(r) => Some((k, c, *r)),
                    _ => None,
                })
        })
        .collect();
    all.sort_by(|a, b| b.2.ln().abs().total_cmp(&a.2.ln().abs()));
    all
}

/// Best variant by geomean, or the reference when it beats them all. `terse` drops the
/// "no variant is better overall" tail (the alt text has little room).
fn headline(plan: &Plan, short: usize, terse: bool) -> Option<String> {
    let means = geomeans(plan);
    let (k, g) = means
        .iter()
        .enumerate()
        .filter_map(|(k, g)| g.map(|g| (k, g)))
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    Some(if g < 1.0 {
        format!(
            "{} has the best geomean ({})",
            shorten_middle(plan.labels[k], short),
            fmt_ratio(g)
        )
    } else if terse {
        format!(
            "{} has the best geomean",
            shorten_middle(plan.reference, short)
        )
    } else {
        format!(
            "{} has the best geomean; no variant is better overall",
            shorten_middle(plan.reference, short)
        )
    })
}

/// At most 125 characters; names shrink and the benchmark name is cut before any number is.
fn alt_text(plan: &Plan) -> String {
    for name_chars in [ALT_NAME_CHARS, 10, 6] {
        let alt = alt_with(plan, name_chars);
        if alt.chars().count() <= ALT_MAX_CHARS {
            return alt;
        }
    }
    alt_with(plan, 6)
        .chars()
        .take(ALT_MAX_CHARS - 1)
        .collect::<String>()
        + "…"
}

fn alt_with(plan: &Plan, name_chars: usize) -> String {
    let mut out = format!("Ratio vs {}: ", shorten_middle(plan.reference, name_chars));
    match headline(plan, name_chars, true) {
        Some(h) => out.push_str(&h),
        None => out.push_str(&format!("{} variants", plan.series.len())),
    }
    if let Some((k, c, r)) = gaps(plan).first() {
        let lead = format!(
            "; largest gap: {} {} on ",
            shorten_middle(plan.labels[*k], name_chars),
            fmt_ratio(*r)
        );
        let room = ALT_MAX_CHARS.saturating_sub(out.chars().count() + lead.chars().count());
        out.push_str(&lead);
        out.push_str(&shorten_middle(&c.row.label(), room.max(1)));
    }
    out
}

fn desc_text(plan: &Plan) -> String {
    let mut out = format!(
        "{} {}, {} variants against reference {}; log2 axis, bars start at 1.00x.",
        plan.clusters.len(),
        plural(plan.clusters.len(), "benchmark"),
        plan.series.len(),
        plan.reference
    );
    if let Some(h) = headline(plan, 40, false) {
        out.push_str(&format!(" {h}."));
    }
    let top: Vec<String> = gaps(plan)
        .iter()
        .take(DESC_TOP)
        .map(|(k, c, r)| format!("{} {} on {}", plan.labels[*k], fmt_ratio(*r), c.row.label()))
        .collect();
    if !top.is_empty() {
        out.push_str(&format!(" Largest gaps: {}.", top.join("; ")));
    }
    if !plan.hidden.is_empty() {
        out.push_str(&format!(
            " {} more not shown: {}.",
            plan.hidden.len(),
            plan.hidden.join(", ")
        ));
    }
    out
}

/// Marker outline for `shape` centred at (`cx`, `cy`); same geometry as `Shape::path`, built
/// through `PathBuilder` because the writer takes no raw path data.
fn marker(shape: Shape, cx: f64, cy: f64, r: f64) -> PathBuilder {
    let mut p = PathBuilder::new();
    if shape == Shape::Circle {
        p.move_to(cx - r, cy)
            .arc(r, r, true, false, cx + r, cy)
            .arc(r, r, true, false, cx - r, cy)
            .close();
        return p;
    }
    let mut poly = |pts: &[(f64, f64)]| {
        for (k, (x, y)) in pts.iter().enumerate() {
            if k == 0 {
                p.move_to(cx + x, cy + y);
            } else {
                p.line_to(cx + x, cy + y);
            }
        }
        p.close();
    };
    match shape {
        Shape::Circle => {}
        Shape::Square => poly(&[(-r, -r), (r, -r), (r, r), (-r, r)]),
        Shape::Diamond => poly(&[(0.0, -r), (r, 0.0), (0.0, r), (-r, 0.0)]),
        Shape::Triangle => poly(&[(0.0, -r), (r, r), (-r, r)]),
        Shape::Cross => {
            let t = r / 3.0;
            poly(&[
                (-t, -r),
                (t, -r),
                (t, -t),
                (r, -t),
                (r, t),
                (t, t),
                (t, r),
                (-t, r),
                (-t, t),
                (-r, t),
                (-r, -t),
                (-t, -t),
            ]);
        }
        Shape::Star => {
            let pts: Vec<(f64, f64)> = (0..10)
                .map(|k| {
                    let rad = if k % 2 == 0 { r } else { r * 0.4 };
                    let a =
                        -std::f64::consts::FRAC_PI_2 + f64::from(k) * std::f64::consts::PI / 5.0;
                    (rad * a.cos(), rad * a.sin())
                })
                .collect();
            poly(&pts);
        }
    }
    p
}

fn draw_marker(d: &mut SvgWriter, series: usize, pal: Palette, cx: f64, cy: f64, hollow: bool) {
    let path = marker(shape_for_series(series), cx, cy, MARKER_R);
    if hollow {
        d.path_styled(&path, "none", pal.secondary, SERIES_STROKE_WIDTH);
    } else {
        let color = pal.series_color(series);
        let stroke = pal.series_stroke(series).unwrap_or(color);
        d.path_styled(&path, color, stroke, SERIES_STROKE_WIDTH);
    }
}

fn tick_label(k: i32) -> String {
    format!("{}x", 2f64.powi(k))
}

fn render(plan: &Plan, width: f64, pal: Palette) -> Result<String, NonFiniteError> {
    let n = plan.series.len();
    let cluster_h = n as f64 * LANE_H + ROW_GAP;

    let max_log = plan
        .clusters
        .iter()
        .flat_map(|c| c.slots.iter())
        .filter_map(|s| match s {
            Slot::Ratio(r) => Some(r.log2().abs()),
            _ => None,
        })
        .fold(0.0, f64::max);
    let span = max_log.ceil().clamp(MIN_SPAN, MAX_SPAN);

    let plot_l = GUTTER;
    let plot_r = width - PAD;
    let cx = (plot_l + plot_r) / 2.0;
    let per_log = ((plot_r - plot_l) / 2.0 - END_ROOM).max(8.0) / span;

    // Legend: reference entry, then one entry per series, wrapped to the card width.
    let ref_text = format!("{} (reference)", shorten_middle(plan.reference, 24));
    let mut legend: Vec<(Option<usize>, String, f64, f64, f64)> = Vec::new();
    let (mut lx, mut ly) = (PAD, 0.0);
    let entries = std::iter::once((None, ref_text)).chain(
        plan.labels
            .iter()
            .enumerate()
            .map(|(k, l)| (Some(k), shorten_middle(l, 24))),
    );
    for (k, text) in entries {
        let w = 2.0 * MARKER_R + 6.0 + text_width(&text, FONT_PX) + 18.0;
        if lx > PAD && lx + w > width - PAD {
            lx = PAD;
            ly += LEGEND_LINE_H;
        }
        legend.push((k, text, lx, ly, w));
        lx += w;
    }
    let legend_lines = (ly / LEGEND_LINE_H) as usize + 1;
    let mut notes = vec![format!(
        "Bars: value ÷ {}, log2 scale; left of the line is smaller.",
        shorten_middle(plan.reference, 24)
    )];
    if !plan.hidden.is_empty() {
        let names = truncate_to_width(&plan.hidden.join(", "), FONT_PX, width - 2.0 * PAD - 150.0);
        notes.push(format!("+ {} more not shown: {names}", plan.hidden.len()));
    }
    notes.push("Largest change first.".to_string());
    if let Some(more) = &plan.more {
        notes.push(more.clone());
    }
    let rows_h = plan.clusters.len() as f64 * cluster_h;
    let footer_h =
        16.0 + legend_lines as f64 * LEGEND_LINE_H + notes.len() as f64 * LEGEND_LINE_H + 10.0;
    let height = HEADER_H + rows_h + footer_h;

    let mut d = SvgWriter::new_accessible(
        width,
        height,
        "Helvetica, Arial, sans-serif",
        &alt_text(plan),
        &desc_text(plan),
    );
    d.rect_fill(0.0, 0.0, width, height, 8.0, pal.card);
    d.text_styled(
        PAD,
        24.0,
        Anchor::Start,
        13.0,
        pal.ink,
        &format!("Ratio vs {}", shorten_middle(plan.reference, 24)),
    );
    d.text_styled(
        PAD,
        42.0,
        Anchor::Start,
        FONT_PX,
        pal.secondary,
        &format!(
            "{} {} · {} variants · log2 scale",
            plan.clusters.len(),
            plural(plan.clusters.len(), "benchmark"),
            n
        ),
    );

    // Axis: faint ticks at powers of two, the 1.00x reference line in ink.
    let step = if span > 3.0 { 2 } else { 1 };
    let mut k = -(span as i32);
    while k <= span as i32 {
        if k != 0 {
            let x = cx + f64::from(k) * per_log;
            d.line_stroke(x, HEADER_H, x, HEADER_H + rows_h, pal.unchanged, 0.5);
            d.text_styled(
                x,
                HEADER_H - 4.0,
                Anchor::Middle,
                11.0,
                pal.secondary,
                &tick_label(k),
            );
        }
        k += step;
    }
    d.line_stroke(cx, HEADER_H, cx, HEADER_H + rows_h, pal.ink, 1.5);
    d.text_styled(
        cx,
        HEADER_H - 4.0,
        Anchor::Middle,
        11.0,
        pal.ink,
        &format!("{} = 1.00x", shorten_middle(plan.reference, 16)),
    );

    for (i, c) in plan.clusters.iter().enumerate() {
        let top = HEADER_H + i as f64 * cluster_h + ROW_GAP / 2.0;
        let label = &c.label;
        let parts: Vec<String> = c
            .slots
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let what = match s {
                    Slot::Ratio(r) => fmt_ratio(*r),
                    Slot::Missing => "not run".to_string(),
                    Slot::Unusable => "no usable ratio".to_string(),
                };
                format!("{} {what}", plan.labels[k])
            })
            .collect();
        d.open_titled_group(&format!("{label}: {}", parts.join(", ")));
        let shown = truncate_to_width(label, FONT_PX, GUTTER - PAD - 8.0);
        d.text_styled(
            PAD,
            top + LANE_H / 2.0 + 4.0,
            Anchor::Start,
            FONT_PX,
            pal.ink,
            &shown,
        );
        for (k, s) in c.slots.iter().enumerate() {
            let mid = top + k as f64 * LANE_H + LANE_H / 2.0;
            match s {
                Slot::Ratio(r) => {
                    let log = r.log2().clamp(-span, span);
                    let end = cx + log * per_log;
                    let side = if log < 0.0 { -1.0 } else { 1.0 };
                    if (end - cx).abs() > 0.005 {
                        let color = pal.series_color(k);
                        let stroke = pal.series_stroke(k).unwrap_or(color);
                        let mut bar = PathBuilder::new();
                        bar.move_to(cx, mid - BAR_H / 2.0)
                            .line_to(end, mid - BAR_H / 2.0)
                            .line_to(end, mid + BAR_H / 2.0)
                            .line_to(cx, mid + BAR_H / 2.0)
                            .close();
                        d.path_styled(&bar, color, stroke, SERIES_STROKE_WIDTH);
                    }
                    draw_marker(&mut d, k, pal, end, mid, false);
                    let anchor = if side > 0.0 {
                        Anchor::Start
                    } else {
                        Anchor::End
                    };
                    d.text_styled(
                        end + side * (MARKER_R + 4.0),
                        mid + 4.0,
                        anchor,
                        FONT_PX,
                        pal.ink,
                        &fmt_ratio(*r),
                    );
                }
                Slot::Missing | Slot::Unusable => {
                    draw_marker(&mut d, k, pal, cx, mid, true);
                    let text = if matches!(s, Slot::Missing) {
                        "—"
                    } else {
                        "n/a"
                    };
                    d.text_styled(
                        cx + MARKER_R + 4.0,
                        mid + 4.0,
                        Anchor::Start,
                        FONT_PX,
                        pal.secondary,
                        text,
                    );
                }
            }
        }
        d.close_group();
    }

    let fy = HEADER_H + rows_h + 22.0;
    for (k, text, x, y, _) in &legend {
        let my = fy - 4.0 + y;
        match k {
            Some(k) => draw_marker(&mut d, *k, pal, x + MARKER_R, my, false),
            None => d.line_stroke(
                x + MARKER_R,
                my - MARKER_R,
                x + MARKER_R,
                my + MARKER_R,
                pal.ink,
                1.5,
            ),
        }
        d.text_styled(
            x + 2.0 * MARKER_R + 6.0,
            fy + y,
            Anchor::Start,
            FONT_PX,
            pal.secondary,
            text,
        );
    }
    let mut ny = fy + legend_lines as f64 * LEGEND_LINE_H;
    for note in &notes {
        let fit = truncate_to_width(note, FONT_PX, width - 2.0 * PAD);
        d.text_styled(PAD, ny, Anchor::Start, FONT_PX, pal.secondary, &fit);
        ny += LEGEND_LINE_H;
    }
    d.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::{Method, Samples, Verdict};
    use crate::matrix::{
        CellFormat, Correction, MatrixSort, MatrixSummary, PresentCell, ReferenceInfo, VariantLabel,
    };
    use crate::svg::ChartWidth;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn summary(reference: &str) -> MatrixSummary {
        MatrixSummary {
            title: String::new(),
            tag: String::new(),
            threshold_pct: 5.0,
            min_abs: 0.0,
            alpha: 0.05,
            use_stats: true,
            only_changed: false,
            max_rows: None,
            fail_on_regression_pct: None,
            correction: Correction::Holm,
            columns_cap: 8,
            cell_format: CellFormat::Full,
            sort: MatrixSort::Input,
            reference: Some(ReferenceInfo {
                label: reference.into(),
                explicit: true,
            }),
            skipped: vec![],
            threshold_overrides: BTreeMap::new(),
            config_sources: vec![],
        }
    }

    fn present(value: f64, cmp: Comparison) -> Cell {
        Cell::Present(PresentCell {
            value,
            samples: Samples::default(),
            vs_reference: cmp,
        })
    }

    fn compared(verdict: Verdict, delta: Option<f64>) -> Comparison {
        Comparison::Compared {
            verdict,
            delta,
            method: Some(Method::Threshold),
            p_value: None,
            p_adjusted: None,
        }
    }

    /// Values per column; `None` is a Missing cell. Column 0 is the reference.
    fn row(name: &str, better: Better, values: &[Option<f64>]) -> MatrixRow {
        let cells = values
            .iter()
            .enumerate()
            .map(|(i, v)| match v {
                None => Cell::Missing,
                Some(v) if i == 0 => present(*v, Comparison::Reference),
                Some(v) => {
                    let delta = values[0].map(|r| (v / r - 1.0) * 100.0);
                    present(*v, compared(Verdict::Unchanged, delta))
                }
            })
            .collect();
        MatrixRow {
            group: String::new(),
            name: name.into(),
            measure: String::new(),
            unit: "ns".into(),
            better,
            dims: Default::default(),
            cells,
            ranking: vec![],
        }
    }

    fn report(columns: &[&str], rows: Vec<MatrixRow>) -> MatrixReport {
        MatrixReport {
            summary: summary(columns[0]),
            columns: columns
                .iter()
                .map(|c| VariantLabel::parse(c).unwrap())
                .collect(),
            rows,
            geomean: vec![],
            reference_absent: vec![],
            notices: vec![],
        }
    }

    fn three() -> MatrixReport {
        let lo = Better::Lower;
        report(
            &["quick", "heap", "merge"],
            vec![
                row("Sort/size=100", lo, &[Some(100.0), Some(130.0), Some(80.0)]),
                row(
                    "Sort/size=1000",
                    lo,
                    &[Some(1000.0), Some(1500.0), Some(900.0)],
                ),
                row("Parse", lo, &[Some(50.0), Some(25.0), None]),
                row("Hash", lo, &[Some(10.0), Some(10.0), Some(8.0)]),
                row("Search", lo, &[Some(200.0), Some(210.0), Some(150.0)]),
                row("Encode", lo, &[Some(40.0), Some(80.0), Some(41.0)]),
            ],
        )
    }

    fn opts(theme: ChartTheme, width: u32) -> SvgOptions {
        SvgOptions {
            theme,
            width: ChartWidth::new(width).unwrap(),
            ..Default::default()
        }
    }

    fn light(r: &MatrixReport) -> SvgFile {
        ranking_chart(r, &opts(ChartTheme::Light, 760)).unwrap()
    }

    fn assert_snapshot(file: &str, actual: &str) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/snapshots/svg")
            .join(file);
        if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, actual).unwrap();
        }
        let want = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; run with UPDATE_SNAPSHOTS=1", path.display()));
        assert_eq!(
            want, actual,
            "{file} differs; UPDATE_SNAPSHOTS=1 to refresh"
        );
    }

    /// The `x` of the vertical gridline drawn with stroke `#0b0b0b` (the reference line).
    fn ref_line_x(svg: &str) -> f64 {
        let line = svg
            .lines()
            .find(|l| l.starts_with("<line") && l.contains("stroke=\"#0b0b0b\""))
            .unwrap();
        line.split("x1=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    #[test]
    fn ranking_chart_should_use_log2_scale_gridline_and_hollow_slot_when_missing_cell() {
        let f = light(&three());
        assert_eq!(f.name, "ranking");
        assert!(f.svg.contains("viewBox=\"0 0 760 "));
        assert!(f.svg.contains(">quick = 1.00x</text>"));
        for want in ["1.30x", "0.80x", "1.50x", "0.90x", "0.50x", "2.00x"] {
            assert!(f.svg.contains(&format!(">{want}</text>")), "{want}");
        }
        // 2x slower and 2x faster sit symmetrically about the reference line.
        let r = report(
            &["quick", "slow", "fast"],
            vec![row(
                "B",
                Better::Lower,
                &[Some(100.0), Some(200.0), Some(50.0)],
            )],
        );
        let svg = light(&r).svg;
        let cx = ref_line_x(&svg);
        let ends: Vec<f64> = svg
            .lines()
            .filter(|l| {
                l.starts_with("<path d=\"M") && l.contains("L") && !l.contains("fill=\"none\"")
            })
            .filter_map(|l| {
                // bar paths: M cx y L end y ...
                let d = l.split("d=\"M").nth(1)?.split('"').next()?;
                let mut it = d.split('L');
                let first: f64 = it.next()?.split(' ').next()?.parse().ok()?;
                let second: f64 = it.next()?.split(' ').next()?.parse().ok()?;
                ((first - cx).abs() < 0.01 && (second - cx).abs() > 0.01).then_some(second)
            })
            .collect();
        assert_eq!(ends.len(), 2, "{svg}");
        assert!(
            ((ends[0] - cx) + (ends[1] - cx)).abs() < 0.02,
            "{ends:?} {cx}"
        );
    }

    #[test]
    fn ux_s2_9_missing_cell_should_draw_hollow_slot_with_dash_and_no_zero_length_bar() {
        let f = light(&three());
        // Parse/merge is Missing: hollow marker plus an em dash, no bar.
        assert!(f.svg.contains(">—</text>"));
        assert!(f.svg.contains("fill=\"none\" stroke=\"#52514e\""));
        let group = f
            .svg
            .split("<g>\n")
            .find(|g| g.contains(". Parse:"))
            .unwrap();
        let group = group.split("</g>").next().unwrap();
        assert!(group.contains("merge not run"));
        // one filled bar (heap) and its marker; the merge slot adds only the hollow marker
        assert_eq!(group.matches("fill=\"#2a78d6\"").count(), 2);
        assert_eq!(group.matches("fill=\"#eb6834\"").count(), 0);
        assert!(!f.svg.contains("width=\"0\""));
    }

    #[test]
    fn ranking_chart_should_give_each_series_distinct_shape_and_colour_and_value_labels() {
        let f = light(&three());
        // series 0 circle (arcs) in blue, series 1 square in orange
        let g = f.svg.split("<g>\n").nth(1).unwrap();
        assert!(g.contains("A5 5 0 1 0") && g.contains("fill=\"#2a78d6\""));
        assert!(g.contains("fill=\"#eb6834\""));
        // legend names the reference and both variants
        for want in ["quick (reference)", ">heap<", ">merge<"] {
            assert!(f.svg.contains(want), "{want}");
        }
        // every drawn ratio has a direct label
        assert_eq!(
            f.svg.matches("<title>").count(),
            6,
            "{}",
            f.svg.matches("<title>").count()
        );
    }

    #[test]
    fn ranking_chart_should_outline_light_series_three_to_five_in_secondary_ink() {
        let cols = ["r", "a", "b", "c", "d", "e"];
        let vals: Vec<Option<f64>> = (0..6).map(|i| Some(100.0 + f64::from(i) * 10.0)).collect();
        let r = report(&cols, vec![row("B", Better::Lower, &vals)]);
        let svg = light(&r).svg;
        // series index 2 (third variant, #1baf7a) fails 3:1 as fill, so it is outlined
        assert!(svg.contains("fill=\"#1baf7a\" stroke=\"#52514e\""));
        // series 0 passes on its own and is outlined in its own colour
        assert!(svg.contains("fill=\"#2a78d6\" stroke=\"#2a78d6\""));
    }

    #[test]
    fn ranking_chart_should_list_ninth_series_in_footer_when_nine_variants() {
        let cols: Vec<String> = std::iter::once("ref".to_string())
            .chain((1..=9).map(|i| format!("v{i}")))
            .collect();
        let cols: Vec<&str> = cols.iter().map(String::as_str).collect();
        let vals: Vec<Option<f64>> = (0..10).map(|i| Some(100.0 + f64::from(i))).collect();
        let r = report(&cols, vec![row("B", Better::Lower, &vals)]);
        let f = light(&r);
        assert!(f.svg.contains("+ 1 more not shown: v9"), "{}", f.svg);
        assert_eq!(f.svg.matches("<title>").count(), 1);
        assert!(f.svg.contains("B: v1 1.01x"));
        assert!(!f.svg.contains("v9 1.09x"));
    }

    #[test]
    fn ranking_chart_should_flip_geomean_orientation_when_better_is_higher() {
        let hi = Better::Higher;
        let r = report(
            &["quick", "heap", "merge"],
            vec![
                row("A", hi, &[Some(100.0), Some(130.0), Some(80.0)]),
                row("B", hi, &[Some(100.0), Some(120.0), Some(90.0)]),
            ],
        );
        // higher is better: heap (bigger values) wins
        assert!(
            light(&r).alt.contains("heap has the best geomean"),
            "{}",
            light(&r).alt
        );
    }

    #[test]
    fn ranking_chart_should_report_reference_when_no_variant_beats_it() {
        let r = report(
            &["quick", "heap"],
            vec![row("A", Better::Lower, &[Some(100.0), Some(130.0)])],
        );
        assert!(light(&r).alt.contains("quick has the best geomean"));
    }

    #[test]
    fn ranking_chart_should_skip_when_nothing_to_compare_or_empty() {
        let o = SvgOptions::default();
        let one = report(&["quick"], vec![row("A", Better::Lower, &[Some(1.0)])]);
        assert_eq!(
            ranking_chart(&one, &o).unwrap_err(),
            SkipReason::NothingToCompare
        );
        let mut no_ref = three();
        no_ref.summary.reference = None;
        assert_eq!(
            ranking_chart(&no_ref, &o).unwrap_err(),
            SkipReason::NothingToCompare
        );
        // the reference lacks every benchmark
        let r = report(
            &["quick", "heap"],
            vec![row("A", Better::Lower, &[None, Some(1.0)])],
        );
        assert_eq!(ranking_chart(&r, &o).unwrap_err(), SkipReason::Empty);
    }

    #[test]
    fn ranking_chart_should_draw_unusable_cells_as_hollow_slots_with_na() {
        let mut r = report(
            &["quick", "heap"],
            vec![
                row("A", Better::Lower, &[Some(100.0), Some(110.0)]),
                row("B", Better::Lower, &[Some(100.0), Some(f64::NAN)]),
            ],
        );
        if let Cell::Present(p) = &mut r.rows[1].cells[1] {
            p.vs_reference = Comparison::Invalid(crate::matrix::Reason("bad".into()));
        }
        let f = light(&r);
        assert!(f.svg.contains(">n/a</text>"));
        assert!(!f.svg.contains("NaN"));
    }

    #[test]
    fn ranking_chart_should_set_accessibility_and_alt_text_within_125_chars() {
        let f = light(&three());
        assert!(f.svg.contains("role=\"img\" aria-labelledby=\"t d\""));
        assert!(f.svg.contains("<title id=\"t\">") && f.svg.contains("<desc id=\"d\">"));
        assert_eq!(
            f.alt,
            "Ratio vs quick: merge has the best geomean (0.85x); largest gap: heap 2.00x on Encode"
        );
        let long = "N".repeat(300);
        let r = report(
            &[
                "a-very-long-reference-label-x",
                "another-very-long-variant-name",
            ],
            vec![row(&long, Better::Lower, &[Some(1.0), Some(40.5)])],
        );
        let f = light(&r);
        assert!(
            f.alt.chars().count() <= 125,
            "{} {}",
            f.alt.chars().count(),
            f.alt
        );
        assert!(f.alt.contains("40.5x"));
        for bad in ["higher", "lower", "regress"] {
            assert!(!f.alt.contains(bad));
        }
    }

    #[test]
    fn ranking_chart_should_be_deterministic_opaque_and_free_of_style_and_links() {
        let a = light(&three()).svg;
        assert_eq!(a, light(&three()).svg);
        assert!(a.contains("rx=\"8\" fill=\"#fcfcfb\"/>"));
        for bad in [
            "<style",
            "@media",
            "currentColor",
            "href",
            "<script",
            "foreignObject",
            "url(",
            "NaN",
            "inf",
        ] {
            assert!(!a.contains(bad), "{bad}");
        }
    }

    #[test]
    fn ranking_chart_should_use_text_colours_from_ink_or_secondary_only() {
        for theme in [ChartTheme::Light, ChartTheme::Dark] {
            let f = ranking_chart(&three(), &opts(theme, 760)).unwrap();
            let pal = Palette::for_theme(if theme == ChartTheme::Dark {
                Theme::Dark
            } else {
                Theme::Light
            });
            for line in f.svg.lines().filter(|l| l.starts_with("<text")) {
                assert!(
                    line.contains(&format!("fill=\"{}\"", pal.ink))
                        || line.contains(&format!("fill=\"{}\"", pal.secondary)),
                    "{line}"
                );
            }
        }
    }

    #[test]
    fn ranking_chart_should_use_dark_palette_and_theme_names() {
        let dark = ranking_chart(&three(), &opts(ChartTheme::Dark, 760)).unwrap();
        assert_eq!(dark.name, "ranking");
        assert!(dark.svg.contains("fill=\"#1a1a19\"") && !dark.svg.contains("#fcfcfb"));
        let both = ranking_chart(&three(), &opts(ChartTheme::Both, 760)).unwrap();
        assert_eq!(both.name, "ranking-light");
        let d = ranking_chart_themed(&three(), &opts(ChartTheme::Both, 760), Theme::Dark).unwrap();
        assert_eq!(d.name, "ranking-dark");
    }

    #[test]
    fn ranking_chart_should_keep_labels_inside_the_card_at_min_and_max_width() {
        for width in [480u32, 1600] {
            let r = report(
                &["quick", "huge", "tiny"],
                vec![row(
                    "B",
                    Better::Lower,
                    &[Some(1.0), Some(4000.0), Some(0.0005)],
                )],
            );
            let svg = ranking_chart(&r, &opts(ChartTheme::Light, width))
                .unwrap()
                .svg;
            for line in svg.lines().filter(|l| l.starts_with("<text")) {
                let x: f64 = line
                    .split("x=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                let text = line.split('>').nth(1).unwrap().split('<').next().unwrap();
                let w = text_width(text, FONT_PX);
                let (l, r) = if line.contains("text-anchor=\"end\"") {
                    (x - w, x)
                } else if line.contains("text-anchor=\"middle\"") {
                    (x - w / 2.0, x + w / 2.0)
                } else {
                    (x, x + w)
                };
                assert!(l >= 0.0 && r <= f64::from(width), "{width} {line}");
            }
        }
    }

    #[test]
    fn ranking_chart_should_match_committed_snapshots() {
        assert_snapshot("ranking-light.svg", &light(&three()).svg);
        assert_snapshot(
            "ranking-dark.svg",
            &ranking_chart(&three(), &opts(ChartTheme::Dark, 760))
                .unwrap()
                .svg,
        );
        let cols: Vec<String> = std::iter::once("ref".to_string())
            .chain((1..=9).map(|i| format!("v{i}")))
            .collect();
        let cols: Vec<&str> = cols.iter().map(String::as_str).collect();
        let vals: Vec<Option<f64>> = (0..10).map(|i| Some(100.0 * 1.4f64.powi(i - 3))).collect();
        let r = report(
            &cols,
            vec![row("BenchmarkParse/size=1000", Better::Lower, &vals)],
        );
        assert_snapshot("ranking-many-light.svg", &light(&r).svg);
    }

    fn n_rows(n: usize) -> MatrixReport {
        let rows = (0..n)
            .map(|i| {
                row(
                    &format!("b{i}"),
                    Better::Lower,
                    &[Some(100.0), Some(101.0 + i as f64)],
                )
            })
            .collect();
        report(&["quick", "heap"], rows)
    }

    #[test]
    fn ranking_chart_should_order_by_largest_delta_with_numbered_labels() {
        let f = light(&three());
        let order: Vec<usize> = ["1. Encode", "2. Sort/size=1000", "3. Parse"]
            .iter()
            .map(|n| f.svg.find(&format!(">{n}</text>")).unwrap())
            .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
        assert!(f.svg.contains("Largest change first."));
        assert_eq!(f.skipped, None);
    }

    #[test]
    fn ranking_chart_should_cap_rows_and_print_n_more_not_shown_when_47_rows_cap_30() {
        let f = light(&n_rows(47));
        assert_eq!(f.svg.matches("<g>").count(), 30);
        assert!(f.svg.contains(">+ 17 more not shown</text>"));
        assert!(f.svg.contains("1. b46") && !f.svg.contains("b16"));
        assert_eq!(
            f.skipped,
            Some(SkipReason::CapExceeded {
                shown: 30,
                total: 47
            })
        );
    }

    #[test]
    fn ranking_chart_should_bound_rows_when_500_rows_and_honour_max_rows() {
        assert_eq!(light(&n_rows(500)).svg.matches("<g>").count(), 30);
        let mut r = n_rows(47);
        r.summary.max_rows = Some(20);
        let f = light(&r);
        assert_eq!(f.svg.matches("<g>").count(), 20);
        assert!(f.svg.contains("+ 27 more not shown"));
        assert!(f.svg.contains("1. b19") && !f.svg.contains("b20"));
    }
}
