//! Two-way delta chart: diverging bars around a 0% line (task 3.2.1a).
//!
//! `SvgWriter` only emits class attributes and the Light card must carry no
//! `<style>` (AC-S2-12), so this module writes its own elements with literal-hex
//! `fill`/`stroke` attributes. Every number goes through `fmt_coord` and every
//! text through `xml_escape`, the same two choke points the writer uses.

use super::style::{Palette, Theme};
use super::text::{text_width, truncate_to_width};
use super::writer::{fmt_coord, xml_escape, NonFiniteError};
use super::{ChartTheme, SkipReason, SvgFile, SvgOptions};
use crate::compare::{Report, Row, Verdict};
use crate::markdown::fmt_pct;

const ROW_H: f64 = 28.0;
const BAR_H: f64 = 16.0;
const GUTTER: f64 = 220.0;
const PAD: f64 = 16.0;
const HEADER_H: f64 = 56.0;
const FOOTER_H: f64 = 52.0;
/// Room kept free at each end of the plot for the cap marker and value label.
const END_ROOM: f64 = 84.0;
const CLIP_PCT: f64 = 100.0;
const FONT_PX: f64 = 12.0;
const CAP_R: f64 = 6.0;
const ALT_MAX_CHARS: usize = 125;
const DESC_TOP: usize = 5;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cap {
    Up,
    Down,
    Hollow,
}

struct Bar<'a> {
    row: &'a Row,
    pct: f64,
    /// +1 draws right of the 0% line, -1 left.
    side: f64,
    cap: Cap,
}

/// Renders the delta chart for the theme in `opts` (`Both` renders the light file, named
/// `delta-light`; the dispatcher calls `delta_chart_themed` for the dark twin).
///
/// Draws every comparable row it is given (the cap and `+ N more not shown` footer arrive
/// with the row selection in 3.2.4a). `Err(Empty)` when no row has a finite percentage.
pub fn delta_chart(report: &Report, opts: &SvgOptions) -> Result<SvgFile, SkipReason> {
    let theme = match opts.theme {
        ChartTheme::Dark => Theme::Dark,
        ChartTheme::Light | ChartTheme::Both => Theme::Light,
    };
    delta_chart_themed(report, opts, theme)
}

/// As `delta_chart`, with the palette chosen by the caller; the file name carries a
/// `-light`/`-dark` suffix only when `opts.theme` is `Both`.
pub fn delta_chart_themed(
    report: &Report,
    opts: &SvgOptions,
    theme: Theme,
) -> Result<SvgFile, SkipReason> {
    let bars = ordered_bars(report);
    if bars.is_empty() {
        return Err(SkipReason::Empty);
    }
    let svg = render(&bars, opts.width.get() as f64, Palette::for_theme(theme))
        .map_err(|_| SkipReason::Empty)?;
    let name = match (opts.theme, theme) {
        (ChartTheme::Both, Theme::Light) => "delta-light",
        (ChartTheme::Both, Theme::Dark) => "delta-dark",
        _ => "delta",
    };
    Ok(SvgFile {
        name: name.to_string(),
        svg,
        alt: alt_text(&bars),
        skipped: None,
    })
}

/// Comparable rows only (a finite percentage and a verdict that compares), worst first:
/// regressed, then improved, then unchanged; larger |delta| first; ties keep input order.
fn ordered_bars(report: &Report) -> Vec<Bar<'_>> {
    let mut bars: Vec<Bar> = report
        .rows
        .iter()
        .filter_map(|row| {
            let pct = row.delta_pct.filter(|p| p.is_finite())?;
            let (side, cap) = match row.verdict {
                Verdict::Regressed => (1.0, Cap::Up),
                Verdict::Improved => (-1.0, Cap::Down),
                Verdict::Unchanged => (if pct < 0.0 { -1.0 } else { 1.0 }, Cap::Hollow),
                _ => return None,
            };
            Some(Bar {
                row,
                pct,
                side,
                cap,
            })
        })
        .collect();
    let class = |c: Cap| match c {
        Cap::Up => 0,
        Cap::Down => 1,
        Cap::Hollow => 2,
    };
    bars.sort_by(|a, b| {
        class(a.cap)
            .cmp(&class(b.cap))
            .then(b.pct.abs().total_cmp(&a.pct.abs()))
    });
    bars
}

fn counts(bars: &[Bar]) -> (usize, usize, usize) {
    let n = |c| bars.iter().filter(|b| b.cap == c).count();
    (n(Cap::Up), n(Cap::Down), n(Cap::Hollow))
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

/// At most 125 characters: verdict counts and the top change; the name is cut, never the number.
fn alt_text(bars: &[Bar]) -> String {
    let (reg, imp, _) = counts(bars);
    let mut verdict = Vec::new();
    if reg > 0 {
        verdict.push(format!("{reg} regressed"));
    }
    if imp > 0 {
        verdict.push(format!("{imp} improved"));
    }
    let verdict = if verdict.is_empty() {
        "no significant changes".to_string()
    } else {
        verdict.join(", ")
    };
    let top = &bars[0];
    let value = fmt_pct(top.pct);
    let prefix = format!("Benchmark change, head vs base: {verdict}; top: ");
    let room = ALT_MAX_CHARS
        .saturating_sub(prefix.chars().count() + 1 + value.chars().count())
        .max(1);
    format!("{prefix}{} {value}", shorten_middle(&top.row.label(), room))
}

fn desc_text(bars: &[Bar]) -> String {
    let (reg, imp, unch) = counts(bars);
    let top: Vec<String> = bars
        .iter()
        .take(DESC_TOP)
        .map(|b| format!("{} {}", b.row.label(), fmt_pct(b.pct)))
        .collect();
    format!(
        "{reg} regressed, {imp} improved, {unch} unchanged. Largest changes: {}. Bars clip at {CLIP_PCT:.0}%; value labels show the true change.",
        top.join("; ")
    )
}

struct Doc {
    buf: String,
    err: Option<NonFiniteError>,
}

impl Doc {
    fn n(&mut self, v: f64) -> String {
        fmt_coord(v).unwrap_or_else(|e| {
            self.err = Some(e);
            "0".into()
        })
    }

    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, rx: f64, fill: &str) {
        let (x, y, w, h, rx) = (self.n(x), self.n(y), self.n(w), self.n(h), self.n(rx));
        self.buf.push_str(&format!(
            "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"{rx}\" fill=\"{fill}\"/>\n"
        ));
    }

    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, stroke: &str) {
        let (x1, y1, x2, y2) = (self.n(x1), self.n(y1), self.n(x2), self.n(y2));
        self.buf.push_str(&format!(
            "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" stroke=\"{stroke}\" stroke-width=\"1\"/>\n"
        ));
    }

    /// `anchor` is one of `start`, `middle`, `end`.
    fn text(&mut self, x: f64, y: f64, anchor: &str, size: f64, fill: &str, content: &str) {
        let (x, y, size) = (self.n(x), self.n(y), self.n(size));
        self.buf.push_str(&format!(
            "<text x=\"{x}\" y=\"{y}\" text-anchor=\"{anchor}\" font-size=\"{size}\" fill=\"{fill}\">{}</text>\n",
            xml_escape(content)
        ));
    }

    fn path(&mut self, d: &str, fill: &str, stroke: &str) {
        self.buf.push_str(&format!(
            "<path d=\"{d}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"1.5\"/>\n"
        ));
    }

    fn cap(&mut self, cap: Cap, cx: f64, cy: f64, color: &str) {
        let d = match cap_path(cap, cx, cy, CAP_R) {
            Ok(d) => d,
            Err(e) => {
                self.err = Some(e);
                return;
            }
        };
        match cap {
            Cap::Hollow => self.path(&d, "none", color),
            _ => self.path(&d, color, color),
        }
    }
}

fn cap_path(cap: Cap, cx: f64, cy: f64, r: f64) -> Result<String, NonFiniteError> {
    let pts = match cap {
        Cap::Up => [(0.0, -r), (r, r * 0.8), (-r, r * 0.8)],
        Cap::Down => [(0.0, r), (r, -r * 0.8), (-r, -r * 0.8)],
        Cap::Hollow => {
            let (x0, x1, y, rr) = (
                fmt_coord(cx - r)?,
                fmt_coord(cx + r)?,
                fmt_coord(cy)?,
                fmt_coord(r)?,
            );
            return Ok(format!(
                "M{x0} {y}A{rr} {rr} 0 1 0 {x1} {y}A{rr} {rr} 0 1 0 {x0} {y}Z"
            ));
        }
    };
    let mut d = String::new();
    for (k, (x, y)) in pts.iter().enumerate() {
        d.push(if k == 0 { 'M' } else { 'L' });
        d.push_str(&format!("{} {}", fmt_coord(cx + x)?, fmt_coord(cy + y)?));
    }
    d.push('Z');
    Ok(d)
}

fn render(bars: &[Bar], width: f64, pal: Palette) -> Result<String, NonFiniteError> {
    let plot_l = GUTTER;
    let plot_r = width - PAD;
    let cx = (plot_l + plot_r) / 2.0;
    let per_pct = ((plot_r - plot_l) / 2.0 - END_ROOM).max(8.0) / CLIP_PCT;
    let rows_h = bars.len() as f64 * ROW_H;
    let height = HEADER_H + rows_h + FOOTER_H;

    let mut d = Doc {
        buf: String::new(),
        err: None,
    };
    let (ws, hs) = (d.n(width), d.n(height));
    d.buf.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{ws}\" height=\"{hs}\" viewBox=\"0 0 {ws} {hs}\" role=\"img\" aria-labelledby=\"t d\" font-family=\"Helvetica, Arial, sans-serif\">\n"
    ));
    d.buf.push_str(&format!(
        "<title id=\"t\">{}</title>\n<desc id=\"d\">{}</desc>\n",
        xml_escape(&alt_text(bars)),
        xml_escape(&desc_text(bars))
    ));
    d.rect(0.0, 0.0, width, height, 8.0, pal.card);

    let (reg, imp, unch) = counts(bars);
    d.text(
        PAD,
        24.0,
        "start",
        13.0,
        pal.ink,
        "Benchmark change, head vs base",
    );
    d.text(
        PAD,
        42.0,
        "start",
        FONT_PX,
        pal.secondary,
        &format!("{reg} regressed · {imp} improved · {unch} unchanged"),
    );
    d.text(cx, HEADER_H - 4.0, "middle", 11.0, pal.secondary, "0%");
    d.line(cx, HEADER_H, cx, HEADER_H + rows_h, pal.secondary);

    for (i, bar) in bars.iter().enumerate() {
        let top = HEADER_H + i as f64 * ROW_H;
        let mid = top + ROW_H / 2.0;
        let color = match bar.cap {
            Cap::Up => pal.regressed,
            Cap::Down => pal.improved,
            Cap::Hollow => pal.unchanged,
        };
        let len = bar.pct.abs().min(CLIP_PCT) * per_pct;
        let end = cx + bar.side * len;
        let label = format!("{}. {}", i + 1, bar.row.label());
        let shown = truncate_to_width(&label, FONT_PX, GUTTER - PAD - 8.0);

        d.buf.push_str("<g>\n");
        d.buf.push_str(&format!(
            "<title>{}</title>\n",
            xml_escape(&format!("{label}: {}", fmt_pct(bar.pct)))
        ));
        d.text(PAD, mid + 4.0, "start", FONT_PX, pal.ink, &shown);
        if len > 0.0 {
            d.rect(cx.min(end), mid - BAR_H / 2.0, len, BAR_H, 0.0, color);
        }
        if bar.pct.abs() > CLIP_PCT {
            // Two outward chevrons in card colour: the bar is cut at the clip.
            let s = bar.side;
            let mut chev = String::new();
            for off in [-12.0, -6.0] {
                let x = end + s * off;
                chev.push_str(&format!(
                    "M{} {}L{} {}L{} {}",
                    d.n(x - s * 3.0),
                    d.n(mid - 4.0),
                    d.n(x),
                    d.n(mid),
                    d.n(x - s * 3.0),
                    d.n(mid + 4.0)
                ));
            }
            d.path(&chev, "none", pal.card);
        }
        let cap_cx = end + bar.side * (CAP_R + 2.0);
        d.cap(bar.cap, cap_cx, mid, color);
        let label_x = cap_cx + bar.side * (CAP_R + 6.0);
        let anchor = if bar.side > 0.0 { "start" } else { "end" };
        d.text(
            label_x,
            mid + 4.0,
            anchor,
            FONT_PX,
            pal.ink,
            &fmt_pct(bar.pct),
        );
        d.buf.push_str("</g>\n");
    }

    let fy = HEADER_H + rows_h + 22.0;
    let mut x = PAD;
    for (cap, color, text) in [
        (Cap::Up, pal.regressed, "regressed"),
        (Cap::Down, pal.improved, "improved"),
        (Cap::Hollow, pal.unchanged, "no significant change"),
    ] {
        d.cap(cap, x + CAP_R, fy - 4.0, color);
        d.text(
            x + 2.0 * CAP_R + 6.0,
            fy,
            "start",
            FONT_PX,
            pal.secondary,
            text,
        );
        x += 2.0 * CAP_R + 6.0 + text_width(text, FONT_PX) + 16.0;
    }
    d.text(
        PAD,
        fy + 18.0,
        "start",
        FONT_PX,
        pal.secondary,
        &format!("Worst change first. Bars clip at {CLIP_PCT:.0}%; labels show the true change."),
    );
    d.buf.push_str("</svg>\n");
    match d.err {
        Some(e) => Err(e),
        None => Ok(d.buf),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::{Method, Summary};
    use crate::model::Better;
    use std::path::PathBuf;

    fn row(name: &str, pct: Option<f64>, verdict: Verdict) -> Row {
        Row {
            group: String::new(),
            name: name.into(),
            measure: String::new(),
            unit: "ns".into(),
            better: Better::Lower,
            base: Some(100.0),
            head: Some(100.0),
            delta_pct: pct,
            verdict,
            method: Some(Method::Threshold),
            p_value: None,
            n_base: 1,
            n_head: 1,
            head_range: None,
            samples: Default::default(),
        }
    }

    fn report(rows: Vec<Row>) -> Report {
        Report {
            summary: Summary::default(),
            rows,
            has_base: true,
        }
    }

    fn three() -> Report {
        report(vec![
            row("C", Some(0.4), Verdict::Unchanged),
            row("B", Some(-12.5), Verdict::Improved),
            row("A", Some(30.0), Verdict::Regressed),
        ])
    }

    fn render_with(r: &Report, theme: ChartTheme, width: u32) -> SvgFile {
        let opts = SvgOptions {
            theme,
            width: crate::svg::ChartWidth::new(width).unwrap(),
            ..Default::default()
        };
        delta_chart(r, &opts).unwrap()
    }

    fn light(r: &Report) -> SvgFile {
        render_with(r, ChartTheme::Light, 760)
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
    fn delta_chart_should_draw_diverging_bars_with_shapes_and_value_labels_when_three_rows() {
        let f = light(&three());
        let svg = &f.svg;
        assert_eq!(f.name, "delta");
        assert!(svg.contains("viewBox=\"0 0 760 "), "{svg}");
        // header 56 + 3 rows * 28 + footer 52
        assert!(svg.contains("height=\"192\""));
        for want in ["+30.0%", "-12.5%", "+0.4%"] {
            assert!(svg.contains(&format!(">{want}</text>")), "{want}");
        }
        // worst first: A, then B, then C
        let a = svg.find("1. A").unwrap();
        let b = svg.find("2. B").unwrap();
        let c = svg.find("3. C").unwrap();
        assert!(a < b && b < c);
        // cap markers: triangle up (regressed), down (improved), hollow circle (unchanged)
        assert!(svg.contains("fill=\"#d03b3b\" stroke=\"#d03b3b\""));
        assert!(svg.contains("fill=\"#0ca30c\" stroke=\"#0ca30c\""));
        assert!(svg.contains("fill=\"none\" stroke=\"#8a8a86\""));
        // regressed bar right of the 0% line, improved left
        let cx = (220.0 + 744.0) / 2.0;
        let rect_x = |color: &str| -> f64 {
            let i = svg.find(&format!("fill=\"{color}\"/>")).unwrap();
            let start = svg[..i].rfind("<rect").unwrap();
            let tag = &svg[start..i];
            let x = tag.split("x=\"").nth(1).unwrap().split('"').next().unwrap();
            x.parse().unwrap()
        };
        assert!((rect_x("#d03b3b") - cx).abs() < 0.01);
        assert!(rect_x("#0ca30c") < cx);
        assert!(svg.contains(">0%</text>"));
    }

    #[test]
    fn delta_chart_should_clip_at_100_percent_with_arrow_and_true_label_when_450_percent() {
        let r = report(vec![
            row("Big", Some(450.0), Verdict::Regressed),
            row("Edge", Some(100.0), Verdict::Regressed),
        ]);
        let svg = light(&r).svg;
        assert!(svg.contains(">+450.0%</text>"));
        let w = |name: &str| -> f64 {
            let i = svg.find(name).unwrap();
            let rest = &svg[i..];
            let rect = &rest[rest.find("<rect").unwrap()..];
            let w = rect
                .split("width=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap();
            w.parse().unwrap()
        };
        assert_eq!(
            w("1. Big"),
            w("2. Edge"),
            "450% bar is cut at the 100% length"
        );
        // chevrons exist only on the clipped bar
        assert_eq!(svg.matches("stroke=\"#fcfcfb\"").count(), 1);
    }

    #[test]
    fn delta_chart_should_sort_worst_first_with_stable_ties() {
        let r = report(vec![
            row("U", Some(2.0), Verdict::Unchanged),
            row("I9", Some(-9.0), Verdict::Improved),
            row("R5a", Some(5.0), Verdict::Regressed),
            row("R5b", Some(5.0), Verdict::Regressed),
            row("R50", Some(50.0), Verdict::Regressed),
            row("I20", Some(-20.0), Verdict::Improved),
        ]);
        let svg = light(&r).svg;
        let order: Vec<usize> = ["R50", "R5a", "R5b", "I20", "I9", "U"]
            .iter()
            .map(|n| svg.find(&format!(". {n}</text>")).unwrap())
            .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    }

    #[test]
    fn delta_chart_should_skip_rows_without_a_delta_and_return_empty_when_none_remain() {
        let r = report(vec![
            row("New", None, Verdict::New),
            row("Gone", None, Verdict::Removed),
            row("Zero", None, Verdict::Regressed),
            row("Nan", Some(f64::NAN), Verdict::Regressed),
        ]);
        assert_eq!(
            delta_chart(&r, &SvgOptions::default()).unwrap_err(),
            SkipReason::Empty
        );
        let mut rows = r.rows;
        rows.push(row("Ok", Some(5.0), Verdict::Regressed));
        let svg = light(&report(rows)).svg;
        assert!(svg.contains("1. Ok") && !svg.contains("New"));
    }

    #[test]
    fn delta_chart_should_truncate_long_names_escaped_with_full_name_in_title() {
        let name = format!("Bench<&\"{}>/impl=quick", "x".repeat(80));
        let r = report(vec![row(&name, Some(8.0), Verdict::Regressed)]);
        let svg = light(&r).svg;
        assert!(svg.contains('…'), "{svg}");
        assert!(svg.contains("/impl=quick</text>"));
        let esc = xml_escape(&name);
        assert!(svg.contains(&format!("<title>1. {esc}: +8.0%</title>")));
        assert!(!svg.contains("<script") && !svg.contains("Bench<&"));
    }

    #[test]
    fn ux_s2_4_every_bar_should_have_shape_marker_and_value_label() {
        let f = light(&three());
        let groups: Vec<&str> = f.svg.split("<g>\n").skip(1).collect();
        assert_eq!(groups.len(), 3);
        for g in groups {
            let g = g.split("</g>").next().unwrap();
            assert!(g.contains("<path d=\"M"), "marker missing: {g}");
            assert!(g.matches("<text").count() == 2, "label + value: {g}");
            assert!(g.contains("<title>"));
        }
    }

    #[test]
    fn delta_chart_should_set_accessibility_and_alt_text_within_125_chars() {
        let name = "N".repeat(300);
        let r = report(vec![
            row(&name, Some(30.0), Verdict::Regressed),
            row("B", Some(-12.5), Verdict::Improved),
        ]);
        let f = light(&r);
        assert!(f.svg.contains("role=\"img\" aria-labelledby=\"t d\""));
        assert!(f.svg.contains("<title id=\"t\">") && f.svg.contains("<desc id=\"d\">"));
        assert!(
            f.alt.chars().count() <= 125,
            "{} {}",
            f.alt.chars().count(),
            f.alt
        );
        assert!(f
            .alt
            .starts_with("Benchmark change, head vs base: 1 regressed, 1 improved; top: "));
        assert!(f.alt.ends_with(" +30.0%"));
        let plain = light(&three());
        assert_eq!(
            plain.alt,
            "Benchmark change, head vs base: 1 regressed, 1 improved; top: A +30.0%"
        );
        assert!(plain.svg.contains("A +30.0%; B -12.5%; C +0.4%"));
    }

    #[test]
    fn delta_chart_should_draw_all_unchanged_rows_in_gray_with_verdict_text() {
        let r = report(vec![row("Hash", Some(0.3), Verdict::Unchanged)]);
        let f = light(&r);
        assert!(f.svg.contains("0 regressed · 0 improved · 1 unchanged"));
        assert!(f.alt.contains("no significant changes"));
        assert!(!f.svg.contains("#d03b3b\"/>"));
    }

    #[test]
    fn delta_chart_should_be_deterministic_opaque_and_free_of_style_and_links() {
        let a = light(&three()).svg;
        assert_eq!(a, light(&three()).svg);
        assert!(a.contains(
            "<rect x=\"0\" y=\"0\" width=\"760\" height=\"192\" rx=\"8\" fill=\"#fcfcfb\"/>"
        ));
        for bad in [
            "<style",
            "@media",
            "currentColor",
            "href",
            "<script",
            "foreignObject",
            "url(",
        ] {
            assert!(!a.contains(bad), "{bad}");
        }
        assert!(!a.contains("NaN") && !a.contains("inf"));
    }

    #[test]
    fn delta_chart_should_use_text_colours_from_ink_or_secondary_only() {
        for theme in [ChartTheme::Light, ChartTheme::Dark] {
            let f = render_with(&three(), theme, 760);
            let pal = if theme == ChartTheme::Dark {
                crate::svg::style::DARK
            } else {
                crate::svg::style::LIGHT
            };
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
    fn delta_chart_should_use_dark_palette_and_theme_names() {
        let dark = render_with(&three(), ChartTheme::Dark, 760);
        assert_eq!(dark.name, "delta");
        assert!(dark.svg.contains("fill=\"#1a1a19\""));
        assert!(!dark.svg.contains("#fcfcfb"));
        let both = render_with(&three(), ChartTheme::Both, 760);
        assert_eq!(both.name, "delta-light");
        let opts = SvgOptions {
            theme: ChartTheme::Both,
            ..Default::default()
        };
        let d = delta_chart_themed(&three(), &opts, Theme::Dark).unwrap();
        assert_eq!(d.name, "delta-dark");
    }

    #[test]
    fn delta_chart_should_keep_bars_and_labels_inside_the_card_at_min_and_max_width() {
        for width in [480u32, 1600] {
            let r = report(vec![
                row("Big", Some(450.0), Verdict::Regressed),
                row("Down", Some(-450.0), Verdict::Improved),
            ]);
            let svg = render_with(&r, ChartTheme::Light, width).svg;
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
                let anchor_end = line.contains("text-anchor=\"end\"");
                let text = line.split('>').nth(1).unwrap().split('<').next().unwrap();
                let w = text_width(text, FONT_PX);
                let (l, r) = if anchor_end { (x - w, x) } else { (x, x + w) };
                if text.contains('%') && !text.contains(' ') {
                    assert!(l >= 0.0 && r <= width as f64, "{line}");
                }
            }
        }
    }

    #[test]
    fn delta_chart_should_match_committed_snapshots() {
        assert_snapshot("delta-light.svg", &light(&three()).svg);
        assert_snapshot(
            "delta-dark.svg",
            &render_with(&three(), ChartTheme::Dark, 760).svg,
        );
        let r = report(vec![
            row("BenchmarkParse/size=1000", Some(450.0), Verdict::Regressed),
            row("BenchmarkSort", Some(-80.0), Verdict::Improved),
            row("BenchmarkHash", Some(0.3), Verdict::Unchanged),
        ]);
        assert_snapshot("delta-clipped-light.svg", &light(&r).svg);
    }
}
