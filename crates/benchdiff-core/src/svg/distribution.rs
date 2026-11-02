//! Distribution chart: one dot per sample in a lane per side (two-way) or per variant
//! (N-way), a median tick per lane, and one axis shared by the lanes of a benchmark (3.2.3a).
//!
//! Draws with the writer's attribute-based primitives so the Light card carries no `<style>`.
//! Jitter depends only on the sample index, so identical input gives identical bytes.

use super::style::{Palette, Theme};
use super::text::truncate_to_width;
use super::writer::{Anchor, NonFiniteError, PathBuilder, SvgWriter};
use super::{
    select_rows_where, ChartTheme, RowRef, Selected, SkipReason, SvgFile, SvgInput, SvgOptions,
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
#[allow(dead_code)] // wired by the dispatcher in 3.3.1b
pub fn distribution_chart(report: &Report, opts: &SvgOptions) -> Result<SvgFile, SkipReason> {
    distribution_chart_themed(report, opts, theme_of(opts))
}

/// As `distribution_chart`, with the palette chosen by the caller.
#[allow(dead_code)] // wired by the dispatcher in 3.3.1b
pub fn distribution_chart_themed(
    report: &Report,
    opts: &SvgOptions,
    theme: Theme,
) -> Result<SvgFile, SkipReason> {
    let both_sided = |row: &RowRef| match row {
        RowRef::Pair(r) => {
            !finite(&r.samples.base).is_empty() && !finite(&r.samples.head).is_empty()
        }
        RowRef::Matrix(_) => false,
    };
    let selected = select_rows_where(&SvgInput::Pair(report), opts, &both_sided);
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
    finish(blocks, &selected, opts, theme, Mode::Pair)
}

/// N-way distribution chart: one lane per variant that has samples, for the `opts.rows`
/// benchmarks with the largest |delta| against the reference. File name `distribution`.
#[allow(dead_code)] // wired by the dispatcher in 3.3.1b
pub fn distribution_matrix_chart(
    report: &MatrixReport,
    opts: &SvgOptions,
) -> Result<SvgFile, SkipReason> {
    distribution_matrix_chart_themed(report, opts, theme_of(opts))
}

/// As `distribution_matrix_chart`, with the palette chosen by the caller.
#[allow(dead_code)] // wired by the dispatcher in 3.3.1b
pub fn distribution_matrix_chart_themed(
    report: &MatrixReport,
    opts: &SvgOptions,
    theme: Theme,
) -> Result<SvgFile, SkipReason> {
    let lanes_of = |row: &MatrixRow| -> Vec<Lane> {
        let mut lanes = vec![];
        for (i, (col, cell)) in report.columns.iter().zip(&row.cells).enumerate() {
            let Cell::Present(p) = cell else { continue };
            // The reference column has no `Compared` data but still has samples to show.
            let values = finite(&p.samples.head);
            if values.is_empty() || lanes.len() >= opts.max_series {
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
    };
    let has_lanes = |row: &RowRef| matches!(row, RowRef::Matrix(r) if !lanes_of(r).is_empty());
    let selected = select_rows_where(&SvgInput::Matrix(report), opts, &has_lanes);
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
    finish(blocks, &selected, opts, theme, Mode::Matrix)
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
    opts: &SvgOptions,
    theme: Theme,
    mode: Mode,
) -> Result<SvgFile, SkipReason> {
    if blocks.is_empty() {
        return Err(SkipReason::NoSamples);
    }
    let blocks: Vec<&Block> = blocks.iter().collect();
    let more = selected.footer();
    let svg = render(
        &blocks,
        more.as_deref(),
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
        alt: alt_text(&blocks, mode),
        skipped: selected.skip_reason(),
    })
}

fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
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

/// Longest benchmark or variant name embedded in a `<title>` or `<desc>`; keeps the SVG bounded.
const NAME_MAX_CHARS: usize = 80;
/// Longest unit string embedded in value labels.
const UNIT_MAX_CHARS: usize = 16;

fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// At most 125 characters; the benchmark name is cut, never the counts.
fn alt_text(blocks: &[&Block], mode: Mode) -> String {
    let what = match mode {
        Mode::Pair => "Sample distributions, base vs head",
        Mode::Matrix => "Sample distributions per variant",
    };
    let n = blocks.len();
    let plural = if n == 1 { "benchmark" } else { "benchmarks" };
    let prefix = format!("{what}, {n} {plural}; first: ");
    let room = ALT_MAX_CHARS.saturating_sub(prefix.chars().count()).max(1);
    format!("{prefix}{}", cut(&blocks[0].name, room))
}

fn lane_summary(lane: &Lane, unit: &str) -> String {
    let (min, max) = bounds(&lane.values);
    format!(
        "{}: median {} ({}-{}, n={})",
        cut(&lane.name, NAME_MAX_CHARS),
        with_unit(median(&lane.values), unit),
        fmt_val(min),
        fmt_val(max),
        lane.values.len()
    )
}

fn desc_text(blocks: &[&Block]) -> String {
    let top: Vec<String> = blocks
        .iter()
        .take(DESC_TOP)
        .map(|b| {
            let lanes: Vec<String> = b.lanes.iter().map(|l| lane_summary(l, &b.unit)).collect();
            format!("{} ({})", cut(&b.name, NAME_MAX_CHARS), lanes.join("; "))
        })
        .collect();
    format!(
        "Each dot is one sample; the tick marks the median. Largest change first: {}.",
        top.join(" | ")
    )
}

/// Deterministic vertical offset in `-4..=4` px from the sample index alone.
fn jitter(i: usize) -> f64 {
    ((i * 7 + 3) % 5) as f64 * 2.0 - 4.0
}

fn dot(p: &mut PathBuilder, cx: f64, cy: f64) {
    p.move_to(cx - DOT_R, cy)
        .arc(DOT_R, DOT_R, true, false, cx + DOT_R, cy)
        .arc(DOT_R, DOT_R, true, false, cx - DOT_R, cy)
        .close();
}

fn render(
    blocks: &[&Block],
    more: Option<&str>,
    width: f64,
    pal: Palette,
    mode: Mode,
) -> Result<String, NonFiniteError> {
    let plot_l = GUTTER;
    let plot_r = width - PAD;
    let block_h = |b: &Block| TITLE_H + b.lanes.len() as f64 * LANE_H + AXIS_H + BLOCK_GAP;
    let body_h: f64 = blocks.iter().map(|b| block_h(b)).sum();
    let footer_h = FOOTER_H + if more.is_some() { MORE_LINE_H } else { 0.0 };
    let height = HEADER_H + body_h + footer_h;

    let mut d = SvgWriter::new_accessible(
        width,
        height,
        "Helvetica, Arial, sans-serif",
        &alt_text(blocks, mode),
        &desc_text(blocks),
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
            let mx = x_of(median(&lane.values));
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
    if let Some(more) = more {
        d.text_styled(PAD, fy + 36.0, Anchor::Start, FONT_PX, pal.secondary, more);
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
    fn distribution_chart_should_handle_identical_values_and_drop_non_finite() {
        let r = report(vec![row(
            "Flat",
            Some(0.0),
            vec![5.0, 5.0, f64::NAN],
            vec![5.0, f64::INFINITY, 5.0],
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
}
