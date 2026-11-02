//! Distribution chart: one dot per sample in a lane per side (two-way) or per variant
//! (N-way), a median tick per lane, and one axis shared by the lanes of a benchmark (3.2.3a).
//!
//! Draws with the writer's attribute-based primitives so the Light card carries no `<style>`.
//! Jitter depends only on the sample index, so identical input gives identical bytes.

use super::style::{Palette, Theme};
use super::text::{cut, truncate_to_width, NAME_MAX_CHARS};
use super::writer::{Anchor, NonFiniteError, PathBuilder, SvgWriter};
use super::{
    select_rows_where, ChartRows, ChartTheme, RowRef, Selected, SkipReason, SvgFile, SvgInput,
    SvgOptions,
};
use crate::compare::{Report, Samples};
use crate::matrix::{Cell, MatrixReport, MatrixRow};

const PAD: f64 = 16.0;
const GUTTER: f64 = 140.0;
const HEADER_H: f64 = 56.0;
const FOOTER_H: f64 = 52.0;
const MORE_LINE_H: f64 = 18.0;
const TITLE_H: f64 = 20.0;
const LANE_H: f64 = 22.0;
const AXIS_H: f64 = 18.0;
const BLOCK_GAP: f64 = 10.0;
/// Inset of the first and last dot from the plot edges.
const PLOT_INSET: f64 = 6.0;
const DOT_R: f64 = 3.0;
const TICK_HALF: f64 = 9.0;
const FONT_PX: f64 = 12.0;
const SMALL_PX: f64 = 11.0;
const ALT_MAX_CHARS: usize = 125;
const DESC_TOP: usize = 5;

struct Lane {
    name: String,
    /// Index into the palette's series colours.
    series: usize,
    hollow: bool,
    /// Finite samples only, in input order (jitter follows this order).
    values: Vec<f64>,
}

struct Block {
    /// The benchmark's own label, for alt and description text.
    name: String,
    /// `N. name`, the chart position plus the label.
    label: String,
    unit: String,
    lanes: Vec<Lane>,
}

#[derive(Clone, Copy)]
enum Mode {
    Pair,
    Matrix,
}

/// Renders the two-way distribution chart for the theme in `opts` (`Both` renders the light
/// file, `distribution-light`; the dispatcher calls `distribution_chart_themed` for the dark twin).
///
/// Shows the benchmarks `select_rows` picks (largest |delta| first, capped at
/// `min(chart-rows, max-rows)`) that have samples on both sides, with a `+ N more not shown`
/// footer and `skipped = CapExceeded` when rows were cut. `Err(NoSamples)` when none has
/// samples, e.g. the report was built without `keep_samples`.
pub fn distribution_chart(report: &Report, opts: &SvgOptions) -> Result<SvgFile, SkipReason> {
    distribution_chart_themed(report, opts, theme_of(opts))
}

/// As `distribution_chart`, with the palette chosen by the caller.
pub fn distribution_chart_themed(
    report: &Report,
    opts: &SvgOptions,
    theme: Theme,
) -> Result<SvgFile, SkipReason> {
    let (selected, constants) = pair_selection(report, opts);
    let blocks = selected
        .rows
        .iter()
        .filter_map(|sel| {
            let RowRef::Pair(row) = sel.row else {
                return None;
            };
            let Samples { base, head } = &row.samples;
            Some(Block {
                name: row.label(),
                label: sel.label.clone(),
                unit: row.unit.clone(),
                lanes: vec![
                    Lane {
                        name: "base".into(),
                        series: 0,
                        hollow: true,
                        values: finite(base),
                    },
                    Lane {
                        name: "head".into(),
                        series: 1,
                        hollow: false,
                        values: finite(head),
                    },
                ],
            })
        })
        .collect();
    finish(blocks, &selected, &constants, opts, theme, Mode::Pair)
}

/// N-way distribution chart: one lane per variant that has samples, for the `opts.rows`
/// benchmarks with the largest |delta| against the reference. File name `distribution`.
pub fn distribution_matrix_chart(
    report: &MatrixReport,
    opts: &SvgOptions,
) -> Result<SvgFile, SkipReason> {
    distribution_matrix_chart_themed(report, opts, theme_of(opts))
}

/// As `distribution_matrix_chart`, with the palette chosen by the caller.
pub fn distribution_matrix_chart_themed(
    report: &MatrixReport,
    opts: &SvgOptions,
    theme: Theme,
) -> Result<SvgFile, SkipReason> {
    let (selected, constants) = matrix_selection(report, opts);
    let lanes_of = |row: &MatrixRow| matrix_lanes(report, row, opts);
    let blocks = selected
        .rows
        .iter()
        .filter_map(|sel| {
            let RowRef::Matrix(row) = sel.row else {
                return None;
            };
            Some(Block {
                name: row.label(),
                label: sel.label.clone(),
                unit: row.unit.clone(),
                lanes: lanes_of(row),
            })
        })
        .collect();
    finish(blocks, &selected, &constants, opts, theme, Mode::Matrix)
}

/// Rows the chart leaves out because every sample of every lane is one value: they would draw as
/// a column of identical dots, so they are counted and named in one footer line instead.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct ConstantRows {
    count: usize,
    /// The first `CONST_NAMES_SHOWN` names, in chart order (largest change first).
    names: Vec<String>,
}

const CONST_NAMES_SHOWN: usize = 5;
const CONST_NAME_MAX_CHARS: usize = 40;

impl ConstantRows {
    /// `N rows with constant values not drawn: a, b and M more`; `None` when none were collapsed.
    pub(super) fn line(&self) -> Option<String> {
        if self.count == 0 {
            return None;
        }
        let noun = if self.count == 1 { "row" } else { "rows" };
        let rest = self.count.saturating_sub(self.names.len());
        let tail = if rest > 0 {
            format!(" and {rest} more")
        } else {
            String::new()
        };
        let list = self.names.join(", ");
        Some(format!(
            "{} {noun} with constant values not drawn: {list}{tail}",
            self.count
        ))
    }
}

/// `label (unit)`: rows such as `Sort` in `ns/op` and `B/op` share a label, the unit tells them apart.
fn constant_name(row: &RowRef) -> String {
    let (label, unit) = match row {
        RowRef::Pair(r) => (r.label(), &r.unit),
        RowRef::Matrix(r) => (r.label(), &r.unit),
    };
    if unit.is_empty() {
        cut(&label, CONST_NAME_MAX_CHARS)
    } else {
        format!(
            "{} ({})",
            cut(&label, CONST_NAME_MAX_CHARS - 12),
            cut(unit, 10)
        )
    }
}

/// True when the finite samples of all `lanes` together are one single value (and there is one).
fn all_one_value<'a>(lanes: impl IntoIterator<Item = &'a [f64]>) -> bool {
    let mut seen: Option<f64> = None;
    for v in lanes.into_iter().flatten().filter(|v| v.is_finite()) {
        match seen {
            Some(s) if s != *v => return false,
            _ => seen = Some(*v),
        }
    }
    seen.is_some()
}

/// Selects the drawn rows (constant rows dropped before the cap, so the cap counts only drawn
/// rows) and separately counts the constant rows among the same eligible rows.
fn select_with_constants<'a>(
    input: &SvgInput<'a>,
    opts: &SvgOptions,
    usable: &dyn Fn(&RowRef<'a>) -> bool,
    constant: &dyn Fn(&RowRef<'a>) -> bool,
) -> (Selected<'a>, ConstantRows) {
    let selected = select_rows_where(input, opts, &|r| usable(r) && !constant(r));
    let wide = SvgOptions {
        rows: ChartRows::new(ChartRows::MAX).expect("MAX is a valid chart-rows"),
        ..opts.clone()
    };
    let collapsed = select_rows_where(input, &wide, &|r| usable(r) && constant(r));
    let constants = ConstantRows {
        count: collapsed.population,
        names: collapsed
            .rows
            .iter()
            .take(CONST_NAMES_SHOWN)
            .map(|s| constant_name(&s.row))
            .collect(),
    };
    (selected, constants)
}

/// The rows the two-way distribution chart and its text block draw, plus the collapsed ones.
pub(super) fn pair_selection<'a>(
    report: &'a Report,
    opts: &SvgOptions,
) -> (Selected<'a>, ConstantRows) {
    let both_sided = |row: &RowRef| match row {
        RowRef::Pair(r) => {
            !finite(&r.samples.base).is_empty() && !finite(&r.samples.head).is_empty()
        }
        RowRef::Matrix(_) => false,
    };
    let constant = |row: &RowRef| match row {
        RowRef::Pair(r) => all_one_value([&r.samples.base[..], &r.samples.head[..]]),
        RowRef::Matrix(_) => false,
    };
    select_with_constants(&SvgInput::Pair(report), opts, &both_sided, &constant)
}

/// As [`pair_selection`] for the N-way chart; a row is constant when all its drawn lanes are.
pub(super) fn matrix_selection<'a>(
    report: &'a MatrixReport,
    opts: &SvgOptions,
) -> (Selected<'a>, ConstantRows) {
    let has_lanes = |row: &RowRef| matches!(row, RowRef::Matrix(r) if !matrix_lanes(report, r, opts).is_empty());
    let constant = |row: &RowRef| match row {
        RowRef::Matrix(r) => {
            let lanes = matrix_lanes(report, r, opts);
            all_one_value(lanes.iter().map(|l| &l.values[..]))
        }
        RowRef::Pair(_) => false,
    };
    select_with_constants(&SvgInput::Matrix(report), opts, &has_lanes, &constant)
}

fn matrix_lanes(report: &MatrixReport, row: &MatrixRow, opts: &SvgOptions) -> Vec<Lane> {
    let mut lanes = vec![];
    for (i, (col, cell)) in report.columns.iter().zip(&row.cells).enumerate() {
        let Cell::Present(p) = cell else { continue };
        // The reference column has no `Compared` data but still has samples to show.
        let values = finite(&p.samples.head);
        if values.is_empty() || lanes.len() >= opts.max_series.max(1) {
            continue;
        }
        lanes.push(Lane {
            name: col.as_str().to_string(),
            series: i,
            hollow: false,
            values,
        });
    }
    lanes
}

fn theme_of(opts: &SvgOptions) -> Theme {
    match opts.theme {
        ChartTheme::Dark => Theme::Dark,
        ChartTheme::Light | ChartTheme::Both => Theme::Light,
    }
}

fn finite(values: &[f64]) -> Vec<f64> {
    values.iter().copied().filter(|v| v.is_finite()).collect()
}

fn finish(
    blocks: Vec<Block>,
    selected: &Selected,
    constants: &ConstantRows,
    opts: &SvgOptions,
    theme: Theme,
    mode: Mode,
) -> Result<SvgFile, SkipReason> {
    if blocks.is_empty() {
        return Err(if constants.count > 0 {
            SkipReason::Empty
        } else {
            SkipReason::NoSamples
        });
    }
    let blocks: Vec<&Block> = blocks.iter().collect();
    let more = selected.footer();
    let const_line = constants.line();
    let svg = render(
        &blocks,
        more.as_deref(),
        const_line.as_deref(),
        constants.count,
        opts.width.get() as f64,
        Palette::for_theme(theme),
        mode,
    )
    .map_err(|_| SkipReason::NoSamples)?;
    let name = match (opts.theme, theme) {
        (ChartTheme::Both, Theme::Light) => "distribution-light",
        (ChartTheme::Both, Theme::Dark) => "distribution-dark",
        _ => "distribution",
    };
    Ok(SvgFile {
        name: name.to_string(),
        svg,
        alt: alt_text(&blocks, mode, constants.count),
        skipped: selected.skip_reason(),
    })
}

fn bounds(values: &[f64]) -> (f64, f64) {
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (min, max)
}

/// Three significant digits, no exponent, trailing zeros trimmed.
fn fmt_val(v: f64) -> String {
    let decimals = match v.abs() {
        a if a >= 100.0 || a == 0.0 => 0,
        a if a >= 10.0 => 1,
        a if a >= 1.0 => 2,
        a if a >= 0.1 => 3,
        _ => 4,
    };
    let mut s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.truncate(s.trim_end_matches('0').trim_end_matches('.').len());
    }
    if s == "-0" {
        s = "0".into();
    }
    s
}

fn with_unit(v: f64, unit: &str) -> String {
    if unit.is_empty() {
        fmt_val(v)
    } else {
        format!("{} {}", fmt_val(v), cut(unit, UNIT_MAX_CHARS))
    }
}

/// Longest unit string embedded in value labels.
const UNIT_MAX_CHARS: usize = 16;

/// At most 125 characters; the benchmark name is cut, never the counts.
fn alt_text(blocks: &[&Block], mode: Mode, constant: usize) -> String {
    let what = match mode {
        Mode::Pair => "Sample distributions, base vs head",
        Mode::Matrix => "Sample distributions per variant",
    };
    let n = blocks.len();
    let plural = if n == 1 { "benchmark" } else { "benchmarks" };
    let skipped = match constant {
        0 => String::new(),
        1 => ", 1 constant row not drawn".to_string(),
        c => format!(", {c} constant rows not drawn"),
    };
    let prefix = format!("{what}, {n} {plural}{skipped}; first: ");
    let room = ALT_MAX_CHARS.saturating_sub(prefix.chars().count()).max(1);
    format!("{prefix}{}", cut(&blocks[0].name, room))
}

fn lane_summary(lane: &Lane, unit: &str) -> String {
    let (min, max) = bounds(&lane.values);
    format!(
        "{}: median {} ({}-{}, n={})",
        cut(&lane.name, NAME_MAX_CHARS),
        with_unit(crate::stats::median(&lane.values), unit),
        fmt_val(min),
        fmt_val(max),
        lane.values.len()
    )
}

fn desc_text(blocks: &[&Block], const_line: Option<&str>) -> String {
    let top: Vec<String> = blocks
        .iter()
        .take(DESC_TOP)
        .map(|b| {
            let lanes: Vec<String> = b.lanes.iter().map(|l| lane_summary(l, &b.unit)).collect();
            format!("{} ({})", cut(&b.name, NAME_MAX_CHARS), lanes.join("; "))
        })
        .collect();
    let collapsed = const_line.map(|l| format!(" {l}.")).unwrap_or_default();
    format!(
        "Each dot is one sample; the tick marks the median. Largest change first: {}.{collapsed}",
        top.join(" | ")
    )
}

/// Deterministic vertical offset in `-4..=4` px from the sample index alone.
fn jitter(i: usize) -> f64 {
    ((i * 7 + 3) % 5) as f64 * 2.0 - 4.0
}

fn dot(p: &mut PathBuilder, cx: f64, cy: f64) {
    p.circle(cx, cy, DOT_R);
}

fn render(
    blocks: &[&Block],
    more: Option<&str>,
    const_line: Option<&str>,
    constant: usize,
    width: f64,
    pal: Palette,
    mode: Mode,
) -> Result<String, NonFiniteError> {
    let plot_l = GUTTER;
    let plot_r = width - PAD;
    let block_h = |b: &Block| TITLE_H + b.lanes.len() as f64 * LANE_H + AXIS_H + BLOCK_GAP;
    let body_h: f64 = blocks.iter().map(|b| block_h(b)).sum();
    let extra_lines = usize::from(more.is_some()) + usize::from(const_line.is_some());
    let footer_h = FOOTER_H + extra_lines as f64 * MORE_LINE_H;
    let height = HEADER_H + body_h + footer_h;

    let mut d = SvgWriter::new_accessible(
        width,
        height,
        "Helvetica, Arial, sans-serif",
        &alt_text(blocks, mode, constant),
        &desc_text(blocks, const_line),
    );
    d.rect_fill(0.0, 0.0, width, height, 8.0, pal.card);
    let title = match mode {
        Mode::Pair => "Sample distributions, base vs head",
        Mode::Matrix => "Sample distributions per variant",
    };
    d.text_styled(PAD, 24.0, Anchor::Start, 13.0, pal.ink, title);
    let n = blocks.len();
    d.text_styled(
        PAD,
        42.0,
        Anchor::Start,
        FONT_PX,
        pal.secondary,
        &format!("{n} {}", if n == 1 { "benchmark" } else { "benchmarks" }),
    );

    let mut top = HEADER_H;
    for block in blocks.iter() {
        let all: Vec<f64> = block
            .lanes
            .iter()
            .flat_map(|l| l.values.iter().copied())
            .collect();
        let (min, max) = bounds(&all);
        let span = max - min;
        let x_of = |v: f64| {
            if span > 0.0 {
                plot_l + PLOT_INSET + (v - min) / span * (plot_r - plot_l - 2.0 * PLOT_INSET)
            } else {
                (plot_l + plot_r) / 2.0
            }
        };
        let label = truncate_to_width(&block.label, FONT_PX, width - 2.0 * PAD);
        d.text_styled(PAD, top + 14.0, Anchor::Start, FONT_PX, pal.ink, &label);

        for (li, lane) in block.lanes.iter().enumerate() {
            let lane_top = top + TITLE_H + li as f64 * LANE_H;
            let mid = lane_top + LANE_H / 2.0;
            let color = pal.series_color(lane.series);
            let stroke = if lane.hollow {
                color
            } else {
                pal.series_stroke(lane.series).unwrap_or(color)
            };
            d.open_titled_group(&format!(
                "{} {}",
                cut(&block.label, NAME_MAX_CHARS),
                lane_summary(lane, &block.unit)
            ));
            let name = truncate_to_width(&lane.name, SMALL_PX, GUTTER - PAD - 8.0);
            d.text_styled(
                plot_l - 8.0,
                mid + 4.0,
                Anchor::End,
                SMALL_PX,
                pal.secondary,
                &name,
            );
            let mut dots = PathBuilder::new();
            for (i, v) in lane.values.iter().enumerate() {
                dot(&mut dots, x_of(*v), mid + jitter(i));
            }
            let fill = if lane.hollow { "none" } else { color };
            d.path_styled(&dots, fill, stroke, 1.0);
            let mx = x_of(crate::stats::median(&lane.values));
            d.line_stroke(mx, mid - TICK_HALF, mx, mid + TICK_HALF, pal.ink, 2.0);
            d.close_group();
        }

        let axis_y = top + TITLE_H + block.lanes.len() as f64 * LANE_H;
        d.line_stroke(plot_l, axis_y, plot_r, axis_y, pal.secondary, 1.0);
        d.text_styled(
            plot_l,
            axis_y + 13.0,
            Anchor::Start,
            SMALL_PX,
            pal.secondary,
            &fmt_val(min),
        );
        d.text_styled(
            plot_r,
            axis_y + 13.0,
            Anchor::End,
            SMALL_PX,
            pal.secondary,
            &with_unit(max, &block.unit),
        );
        top += block_h(block);
    }

    let fy = HEADER_H + body_h + 22.0;
    let mut legend = PathBuilder::new();
    dot(&mut legend, PAD + DOT_R, fy - 4.0);
    d.path_styled(&legend, pal.series_color(0), pal.series_color(0), 1.0);
    d.text_styled(
        PAD + 2.0 * DOT_R + 6.0,
        fy,
        Anchor::Start,
        FONT_PX,
        pal.secondary,
        "one sample",
    );
    let tx = PAD + 130.0;
    d.line_stroke(tx, fy - 12.0, tx, fy + 2.0, pal.ink, 2.0);
    d.text_styled(
        tx + 8.0,
        fy,
        Anchor::Start,
        FONT_PX,
        pal.secondary,
        "median",
    );
    d.text_styled(
        PAD,
        fy + 18.0,
        Anchor::Start,
        FONT_PX,
        pal.secondary,
        "Largest change first. Lanes in a benchmark share one axis.",
    );
    let mut note_y = fy + 36.0;
    for line in more.into_iter().chain(const_line) {
        let line = truncate_to_width(line, FONT_PX, width - 2.0 * PAD);
        d.text_styled(PAD, note_y, Anchor::Start, FONT_PX, pal.secondary, &line);
        note_y += MORE_LINE_H;
    }
    d.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::{compare, Method, Options, Row, Summary, Verdict};
    use crate::matrix::{compare_matrix, MatrixOptions, Variant, VariantLabel};
    use crate::model::{Better, Metric, MetricSet};
    use std::path::PathBuf;

    fn row(name: &str, pct: Option<f64>, base: Vec<f64>, head: Vec<f64>) -> Row {
        Row {
            group: String::new(),
            name: name.into(),
            measure: String::new(),
            unit: "ns".into(),
            better: Better::Lower,
            base: Some(100.0),
            head: Some(100.0),
            delta_pct: pct,
            verdict: Verdict::Unchanged,
            method: Some(Method::Threshold),
            p_value: None,
            n_base: base.len(),
            n_head: head.len(),
            head_range: None,
            samples: Samples { base, head },
        }
    }

    fn report(rows: Vec<Row>) -> Report {
        Report {
            summary: Summary::default(),
            rows,
            has_base: true,
        }
    }

    fn ten(offset: f64) -> Vec<f64> {
        (0..10).map(|i| 100.0 + offset + i as f64).collect()
    }

    fn two_way() -> Report {
        report(vec![
            row("Small", Some(1.0), ten(0.0), ten(1.0)),
            row("Sort", Some(-20.0), ten(0.0), ten(-20.0)),
        ])
    }

    fn opts(theme: ChartTheme) -> SvgOptions {
        SvgOptions {
            theme,
            ..Default::default()
        }
    }

    fn snapshot_path(file: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/snapshots/svg")
            .join(file)
    }

    fn assert_snapshot(file: &str, actual: &str) {
        let path = snapshot_path(file);
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

    #[test]
    fn distribution_chart_should_draw_lanes_with_median_tick_when_ten_samples() {
        let f = distribution_chart(&two_way(), &opts(ChartTheme::Light)).unwrap();
        assert_eq!(f.name, "distribution");
        let svg = &f.svg;
        // largest |delta| first
        assert!(svg.find("1. Sort").unwrap() < svg.find("2. Small").unwrap());
        // 2 benchmarks x 2 lanes: one dot path per lane, each with 10 dots (20 arcs)
        let dot_paths: Vec<&str> = svg
            .lines()
            .filter(|l| l.starts_with("<path") && l.matches('A').count() == 20)
            .collect();
        assert_eq!(dot_paths.len(), 4, "{svg}");
        // base hollow, head filled
        assert!(dot_paths[0].contains("fill=\"none\""));
        assert!(!dot_paths[1].contains("fill=\"none\""));
        // median tick: a 2px ink line per lane plus one in the legend
        assert_eq!(
            svg.matches("stroke=\"#0b0b0b\" stroke-width=\"2\"").count(),
            5
        );
        assert!(svg.contains("Sort") && svg.contains("<title>"));
        assert!(svg.contains("<desc"), "{svg}");
        assert!(!svg.contains("<style"));
    }

    #[test]
    fn distribution_chart_should_share_one_axis_across_lanes() {
        let r = report(vec![row(
            "Sort",
            Some(-20.0),
            vec![100.0, 110.0],
            vec![80.0, 90.0],
        )]);
        let svg = distribution_chart(&r, &opts(ChartTheme::Light))
            .unwrap()
            .svg;
        // axis spans 80..110 ns: min label and max label with unit
        assert!(svg.contains(">80</text>"), "{svg}");
        assert!(svg.contains(">110 ns</text>"), "{svg}");
    }

    #[test]
    fn distribution_chart_should_skip_with_no_samples_when_keep_samples_false() {
        let r = report(vec![row("Sort", Some(5.0), vec![], vec![])]);
        assert_eq!(
            distribution_chart(&r, &opts(ChartTheme::Light)).unwrap_err(),
            SkipReason::NoSamples
        );
        let one_sided = report(vec![row("Sort", Some(5.0), vec![1.0], vec![])]);
        assert_eq!(
            distribution_chart(&one_sided, &opts(ChartTheme::Light)).unwrap_err(),
            SkipReason::NoSamples
        );
        assert_eq!(
            distribution_chart(&report(vec![]), &opts(ChartTheme::Light)).unwrap_err(),
            SkipReason::NoSamples
        );
    }

    #[test]
    fn distribution_chart_should_skip_when_real_report_built_without_keep_samples() {
        let mut m = Metric::new("", "Sort", "", "ns", 100.0, Better::Lower);
        m.samples = ten(0.0);
        let set = MetricSet {
            metrics: vec![m],
            ..Default::default()
        };
        let plain = compare(Some(&set), &set, &Options::default());
        assert_eq!(
            distribution_chart(&plain, &opts(ChartTheme::Light)).unwrap_err(),
            SkipReason::NoSamples
        );
        let kept = compare(
            Some(&set),
            &set,
            &Options {
                keep_samples: true,
                ..Options::default()
            },
        );
        assert!(distribution_chart(&kept, &opts(ChartTheme::Light)).is_ok());
    }

    #[test]
    fn distribution_chart_should_be_deterministic_and_cap_alt_at_125_chars() {
        let long = "B".repeat(300);
        let r = report(vec![row(&long, Some(3.0), ten(0.0), ten(1.0))]);
        let a = distribution_chart(&r, &opts(ChartTheme::Light)).unwrap();
        let b = distribution_chart(&r, &opts(ChartTheme::Light)).unwrap();
        assert_eq!(a, b);
        assert!(a.alt.chars().count() <= 125, "{}", a.alt);
        assert!(a.alt.ends_with('…'));
    }

    #[test]
    fn distribution_chart_should_drop_non_finite_samples() {
        let mut moving = ten(0.0);
        moving[3] = f64::NAN;
        let r = report(vec![row(
            "Moving",
            Some(2.0),
            moving,
            vec![5.0, f64::INFINITY, 6.0],
        )]);
        let f = distribution_chart(&r, &opts(ChartTheme::Light)).unwrap();
        assert!(!f.svg.contains("NaN") && !f.svg.contains("inf"));
    }

    #[test]
    fn distribution_chart_should_name_files_by_theme_when_both() {
        let r = two_way();
        let o = opts(ChartTheme::Both);
        assert_eq!(
            distribution_chart(&r, &o).unwrap().name,
            "distribution-light"
        );
        assert_eq!(
            distribution_chart_themed(&r, &o, Theme::Dark).unwrap().name,
            "distribution-dark"
        );
    }

    #[test]
    fn distribution_chart_should_honour_row_cap() {
        let o = SvgOptions {
            rows: crate::svg::ChartRows::new(1).unwrap(),
            ..Default::default()
        };
        let svg = distribution_chart(&two_way(), &o).unwrap().svg;
        assert!(svg.contains("1. Sort") && !svg.contains("Small"));
    }

    fn matrix() -> MatrixReport {
        let var = |label: &str, v: f64| Variant {
            label: VariantLabel::parse(label).unwrap(),
            set: MetricSet {
                metrics: vec![{
                    let mut m = Metric::new("", "Sort", "", "ns", v, Better::Lower);
                    m.samples = (0..10).map(|i| v + i as f64).collect();
                    m
                }],
                ..Default::default()
            },
        };
        compare_matrix(
            &[var("quick", 100.0), var("heap", 130.0), var("merge", 80.0)],
            &Options {
                keep_samples: true,
                ..Options::default()
            },
            &MatrixOptions {
                compare_by: "impl".into(),
                ..MatrixOptions::default()
            },
        )
    }

    #[test]
    fn distribution_matrix_chart_should_draw_one_lane_per_variant() {
        let f = distribution_matrix_chart(&matrix(), &opts(ChartTheme::Light)).unwrap();
        assert_eq!(f.name, "distribution");
        for v in ["quick", "heap", "merge"] {
            assert!(f.svg.contains(&format!(">{v}</text>")), "{v}");
        }
        let lanes = f
            .svg
            .lines()
            .filter(|l| l.starts_with("<path") && l.matches('A').count() == 20)
            .count();
        assert_eq!(lanes, 3);
    }

    #[test]
    fn distribution_matrix_chart_should_draw_one_lane_when_max_series_is_zero() {
        let o = SvgOptions {
            max_series: 0,
            ..opts(ChartTheme::Light)
        };
        let f = distribution_matrix_chart(&matrix(), &o).unwrap();
        let lanes = f
            .svg
            .lines()
            .filter(|l| l.starts_with("<path") && l.matches('A').count() == 20)
            .count();
        assert_eq!(lanes, 1, "same floor as the ranking chart's max(1)");
    }

    #[test]
    fn distribution_matrix_chart_should_skip_when_no_samples_kept() {
        let mut m = matrix();
        for row in &mut m.rows {
            for cell in &mut row.cells {
                if let Cell::Present(p) = cell {
                    p.samples = Samples::default();
                }
            }
        }
        assert_eq!(
            distribution_matrix_chart(&m, &opts(ChartTheme::Light)).unwrap_err(),
            SkipReason::NoSamples
        );
    }

    #[test]
    fn distribution_snapshots_should_match_committed_files() {
        let r = two_way();
        let light = distribution_chart(&r, &opts(ChartTheme::Light)).unwrap();
        assert_snapshot("distribution-light.svg", &light.svg);
        let dark = distribution_chart(&r, &opts(ChartTheme::Dark)).unwrap();
        assert_snapshot("distribution-dark.svg", &dark.svg);
        let n = distribution_matrix_chart(&matrix(), &opts(ChartTheme::Light)).unwrap();
        assert_snapshot("distribution-matrix-light.svg", &n.svg);
    }

    fn many_with_samples(n: usize) -> Report {
        report(
            (0..n)
                .map(|i| {
                    row(
                        &format!("b{i}"),
                        Some(1.0 + i as f64),
                        ten(100.0),
                        ten(101.0 + i as f64),
                    )
                })
                .collect(),
        )
    }

    #[test]
    fn distribution_chart_should_cap_rows_and_print_n_more_not_shown_when_47_rows_cap_30() {
        let f = distribution_chart(&many_with_samples(47), &SvgOptions::default()).unwrap();
        // per benchmark: its label plus one title per lane
        assert_eq!(f.svg.matches(". b").count(), 30 * 3);
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
    fn distribution_chart_should_bound_rows_when_500_rows_and_honour_max_rows() {
        let f = distribution_chart(&many_with_samples(500), &SvgOptions::default()).unwrap();
        assert!(f.svg.contains("+ 470 more not shown") && !f.svg.contains("b469"));
        let mut r = many_with_samples(47);
        r.summary.max_rows = Some(20);
        let f = distribution_chart(&r, &SvgOptions::default()).unwrap();
        assert!(f.svg.contains("+ 27 more not shown"));
        assert!(f.svg.contains("1. b19") && !f.svg.contains("b20"));
    }

    fn flat(name: &str, v: f64) -> Row {
        row(name, Some(0.0), vec![v; 10], vec![v; 10])
    }

    #[test]
    fn distribution_chart_should_collapse_constant_rows_into_one_footer_line_when_mixed() {
        let r = report(vec![
            flat("B/op", 0.0),
            row("Sort", Some(-20.0), ten(0.0), ten(-20.0)),
            flat("allocs/op", 0.0),
        ]);
        let f = distribution_chart(&r, &opts(ChartTheme::Light)).unwrap();
        assert!(
            f.svg.contains(
                ">2 rows with constant values not drawn: B/op (ns), allocs/op (ns)</text>"
            ),
            "{}",
            f.svg
        );
        assert!(
            f.svg.contains("1. Sort") && !f.svg.contains("2. "),
            "{}",
            f.svg
        );
        assert!(f.svg.contains(">1 benchmark</text>"));
        assert!(f.alt.contains("2 constant rows not drawn"), "{}", f.alt);
        assert!(f.alt.chars().count() <= 125);
        assert!(f.svg.contains("2 rows with constant values not drawn") && f.svg.contains("<desc"));
        let desc = &f.svg[f.svg.find("<desc").unwrap()..];
        assert!(desc[..desc.find("</desc>").unwrap()].contains("constant values not drawn"));
        assert_eq!(f.skipped, None);
    }

    #[test]
    fn distribution_chart_should_skip_empty_when_every_row_is_constant() {
        let r = report(vec![flat("B/op", 0.0), flat("allocs/op", 3.0)]);
        assert_eq!(
            distribution_chart(&r, &opts(ChartTheme::Light)).unwrap_err(),
            SkipReason::Empty
        );
        // constant after dropping non-finite samples
        let nan = report(vec![row(
            "Flat",
            Some(0.0),
            vec![5.0, 5.0, f64::NAN],
            vec![5.0, f64::INFINITY, 5.0],
        )]);
        assert_eq!(
            distribution_chart(&nan, &opts(ChartTheme::Light)).unwrap_err(),
            SkipReason::Empty
        );
        // no samples at all stays NoSamples
        let none = report(vec![row("Sort", Some(5.0), vec![], vec![])]);
        assert_eq!(
            distribution_chart(&none, &opts(ChartTheme::Light)).unwrap_err(),
            SkipReason::NoSamples
        );
    }

    #[test]
    fn distribution_chart_should_treat_rows_as_constant_only_when_all_lanes_share_one_value() {
        let one = |b: f64, h: f64| {
            report(vec![
                row("Moving", Some(9.0), ten(0.0), ten(5.0)),
                row("Probe", Some(0.0), vec![b], vec![h]),
            ])
        };
        // single-sample lanes, same value: collapsed
        let f = distribution_chart(&one(5.0, 5.0), &opts(ChartTheme::Light)).unwrap();
        assert!(f
            .svg
            .contains("1 row with constant values not drawn: Probe"));
        // identical within each lane but lanes differ: drawn
        let f = distribution_chart(&one(5.0, 6.0), &opts(ChartTheme::Light)).unwrap();
        assert!(f.svg.contains("2. Probe") && !f.svg.contains("constant values"));
        let two = report(vec![
            row("Moving", Some(9.0), ten(0.0), ten(5.0)),
            row("Probe", Some(0.0), vec![5.0; 3], vec![6.0; 3]),
        ]);
        let f = distribution_chart(&two, &opts(ChartTheme::Light)).unwrap();
        assert!(f.svg.contains("2. Probe") && !f.svg.contains("constant values"));
    }

    #[test]
    fn distribution_chart_should_count_only_drawn_rows_against_the_cap_when_constants_present() {
        let mut rows: Vec<Row> = (0..40)
            .map(|i| row(&format!("v{i}"), Some(1.0 + i as f64), ten(0.0), ten(1.0)))
            .collect();
        rows.extend((0..8).map(|i| flat(&format!("c{i}"), 0.0)));
        let f = distribution_chart(&report(rows), &SvgOptions::default()).unwrap();
        // 30 drawn (cap counts only non-constant rows), 10 cut, 8 constant
        assert_eq!(f.svg.matches(". v").count(), 30 * 3);
        assert!(f.svg.contains(">+ 10 more not shown</text>"), "{}", f.svg);
        assert!(
            f.svg.contains(
                ">8 rows with constant values not drawn: c0 (ns), c1 (ns), c2 (ns), c3 (ns), c4 (ns) and 3 more</text>"
            ),
            "{}",
            f.svg
        );
        assert_eq!(
            f.skipped,
            Some(SkipReason::CapExceeded {
                shown: 30,
                total: 40
            })
        );
    }

    fn matrix_with_allocs(second: [f64; 3]) -> MatrixReport {
        let var = |label: &str, v: f64, alloc: f64| Variant {
            label: VariantLabel::parse(label).unwrap(),
            set: MetricSet {
                metrics: vec![
                    {
                        let mut m = Metric::new("", "Sort", "", "ns", v, Better::Lower);
                        m.samples = (0..10).map(|i| v + i as f64).collect();
                        m
                    },
                    {
                        let mut m =
                            Metric::new("", "allocs", "", "allocs/op", alloc, Better::Lower);
                        m.samples = vec![alloc; 10];
                        m
                    },
                ],
                ..Default::default()
            },
        };
        compare_matrix(
            &[
                var("quick", 100.0, second[0]),
                var("heap", 130.0, second[1]),
                var("merge", 80.0, second[2]),
            ],
            &Options {
                keep_samples: true,
                ..Options::default()
            },
            &MatrixOptions {
                compare_by: "impl".into(),
                ..MatrixOptions::default()
            },
        )
    }

    #[test]
    fn distribution_matrix_chart_should_collapse_rows_constant_across_all_lanes() {
        let f = distribution_matrix_chart(&matrix_with_allocs([0.0; 3]), &opts(ChartTheme::Light))
            .unwrap();
        assert!(f
            .svg
            .contains("1 row with constant values not drawn: allocs"));
        assert!(f.svg.contains(">1 benchmark</text>") && f.svg.contains("1. Sort"));
        assert!(f.alt.contains("1 constant row not drawn"), "{}", f.alt);
        // lanes with different constants are not constant
        let g = distribution_matrix_chart(
            &matrix_with_allocs([0.0, 1.0, 0.0]),
            &opts(ChartTheme::Light),
        )
        .unwrap();
        assert!(!g.svg.contains("constant values"));
        assert!(g.svg.contains(">2 benchmarks</text>"));
        // every row constant
        let mut all = matrix_with_allocs([0.0; 3]);
        all.rows.retain(|r| r.label().contains("allocs"));
        assert_eq!(
            distribution_matrix_chart(&all, &opts(ChartTheme::Light)).unwrap_err(),
            SkipReason::Empty
        );
    }

    #[test]
    fn distribution_text_table_should_follow_the_chart_and_mention_collapsed_rows() {
        let m = matrix_with_allocs([0.0; 3]);
        let md = super::super::render_text_table(
            &SvgInput::Matrix(&m),
            super::super::ChartKind::Distribution,
            &opts(ChartTheme::Light),
        )
        .unwrap();
        assert!(
            md.contains("| 1 | Sort |") && !md.contains("allocs |"),
            "{md}"
        );
        assert!(
            md.contains("1 row with constant values not drawn: allocs"),
            "{md}"
        );
    }
}
