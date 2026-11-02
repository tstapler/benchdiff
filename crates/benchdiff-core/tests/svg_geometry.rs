//! Geometry checks over the worst-case set `testdata/svg-worst/`: byte snapshots only prove
//! sameness, so these prove the layout is legible. Every coordinate lies in the viewBox, every
//! `<text>` (advance-table width plus the 10% margin) lies in the card, and no two text boxes
//! overlap.

use benchdiff_core::matrix::{
    CellFormat, Correction, MatrixSort, PresentCell, ReferenceInfo, VariantLabel,
};
use benchdiff_core::svg::delta::delta_chart_themed;
use benchdiff_core::svg::distribution::{
    distribution_chart_themed, distribution_matrix_chart_themed,
};
use benchdiff_core::svg::ranking::ranking_chart_themed;
use benchdiff_core::svg::style::Theme;
use benchdiff_core::svg::text::text_width;
use benchdiff_core::svg::{ChartRows, ChartWidth, SvgFile, SvgOptions};
use benchdiff_core::{
    Better, Cell, Comparison, MatrixReport, MatrixRow, MatrixSummary, Method, Report, Row, Summary,
    Verdict,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------------------
// Minimal SVG reader for the writer's one-element-per-line output
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TextBox {
    content: String,
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
}

#[derive(Debug, Default)]
struct Geometry {
    width: f64,
    height: f64,
    /// (description, value) for every numeric position/size attribute and path coordinate.
    numbers: Vec<(String, f64)>,
    /// (description, x, y) of every drawn point, for the viewBox check.
    points: Vec<(String, f64, f64)>,
    texts: Vec<TextBox>,
}

fn attr<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = line.find(&key)? + key.len();
    let end = line[start..].find('"')? + start;
    Some(&line[start..end])
}

fn num(line: &str, name: &str) -> Result<f64, String> {
    let raw = attr(line, name).ok_or_else(|| format!("missing {name} in {line}"))?;
    let v: f64 = raw
        .parse()
        .map_err(|_| format!("{name}={raw:?} is not a number in {line}"))?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err(format!("{name}={raw:?} is not finite in {line}"))
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Path data from the writer: `M x y`, `L x y`, `A rx ry rot large sweep x y`, `Z`, no separators
/// beyond a space or the command letter.
fn path_points(d: &str) -> Result<Vec<(f64, f64)>, String> {
    let mut points = vec![];
    let mut cmd = ' ';
    let mut args: Vec<f64> = vec![];
    let mut flush = |cmd: char, args: &[f64]| -> Result<(), String> {
        match (cmd, args.len()) {
            ('M' | 'L', 2) => points.push((args[0], args[1])),
            ('A', 7) => points.push((args[5], args[6])),
            ('Z', 0) | (' ', 0) => {}
            _ => return Err(format!("unexpected path segment {cmd}{args:?}")),
        }
        Ok(())
    };
    let mut token = String::new();
    for c in d.chars().chain(std::iter::once('\u{0}')) {
        if c.is_ascii_alphabetic() || c == ' ' || c == '\u{0}' {
            if !token.is_empty() {
                let v: f64 = token
                    .parse()
                    .map_err(|_| format!("bad path number {token:?}"))?;
                if !v.is_finite() {
                    return Err(format!("non-finite path number {token:?}"));
                }
                args.push(v);
                token.clear();
            }
            if c.is_ascii_alphabetic() {
                flush(cmd, &args)?;
                args.clear();
                cmd = c;
            }
        } else {
            token.push(c);
        }
    }
    flush(cmd, &args)?;
    Ok(points)
}

fn read_geometry(svg: &str) -> Result<Geometry, String> {
    let mut g = Geometry::default();
    for line in svg.lines() {
        if line.starts_with("<svg ") {
            g.width = num(line, "width")?;
            g.height = num(line, "height")?;
            let vb = attr(line, "viewBox").ok_or("no viewBox")?;
            if vb
                != format!(
                    "0 0 {} {}",
                    attr(line, "width").unwrap(),
                    attr(line, "height").unwrap()
                )
            {
                return Err(format!("viewBox {vb:?} does not match the size"));
            }
        } else if line.starts_with("<text ") {
            let (x, y, size) = (num(line, "x")?, num(line, "y")?, num(line, "font-size")?);
            let open_end = line.find('>').ok_or("unterminated <text>")? + 1;
            let close = line.rfind("</text>").ok_or("unterminated <text>")?;
            let content = unescape(&line[open_end..close]);
            let w = text_width(&content, size);
            let (x0, x1) = match attr(line, "text-anchor") {
                Some("middle") => (x - w / 2.0, x + w / 2.0),
                Some("end") => (x - w, x),
                _ => (x, x + w),
            };
            g.numbers.push(("text x".into(), x));
            g.numbers.push(("text y".into(), y));
            g.texts.push(TextBox {
                content,
                x0,
                x1,
                y0: y - size,
                y1: y,
            });
        } else if line.starts_with("<rect ") {
            let (x, y) = (num(line, "x")?, num(line, "y")?);
            let (w, h) = (num(line, "width")?, num(line, "height")?);
            g.points.push(("rect".into(), x, y));
            g.points.push(("rect".into(), x + w, y + h));
        } else if line.starts_with("<line ") {
            g.points
                .push(("line".into(), num(line, "x1")?, num(line, "y1")?));
            g.points
                .push(("line".into(), num(line, "x2")?, num(line, "y2")?));
        } else if line.starts_with("<path ") {
            let d = attr(line, "d").ok_or("path without d")?;
            for (x, y) in path_points(d)? {
                g.points.push(("path".into(), x, y));
            }
        }
    }
    if g.width <= 0.0 || g.height <= 0.0 {
        return Err("no <svg> size".into());
    }
    Ok(g)
}

const EPS: f64 = 0.01;

/// Every violation found in one chart; empty means legible.
fn violations(svg: &str) -> Vec<String> {
    let g = match read_geometry(svg) {
        Ok(g) => g,
        Err(e) => return vec![e],
    };
    let mut out = vec![];
    for (what, x, y) in &g.points {
        if *x < -EPS || *x > g.width + EPS || *y < -EPS || *y > g.height + EPS {
            out.push(format!(
                "{what} point ({x}, {y}) outside {}x{}",
                g.width, g.height
            ));
        }
    }
    for t in &g.texts {
        if t.x0 < -EPS || t.x1 > g.width + EPS || t.y0 < -EPS || t.y1 > g.height + EPS {
            out.push(format!(
                "text {:?} spans x {:.1}..{:.1} y {:.1}..{:.1} outside {}x{}",
                short(&t.content),
                t.x0,
                t.x1,
                t.y0,
                t.y1,
                g.width,
                g.height
            ));
        }
    }
    for (i, a) in g.texts.iter().enumerate() {
        for b in &g.texts[i + 1..] {
            let dx = a.x1.min(b.x1) - a.x0.max(b.x0);
            let dy = a.y1.min(b.y1) - a.y0.max(b.y0);
            if dx > 0.5 && dy > 0.5 {
                out.push(format!(
                    "text {:?} overlaps {:?} by {dx:.1}x{dy:.1}",
                    short(&a.content),
                    short(&b.content)
                ));
            }
        }
    }
    out
}

fn short(s: &str) -> String {
    let t: String = s.chars().take(40).collect();
    if t.len() < s.len() {
        format!("{t}...")
    } else {
        t
    }
}

// ---------------------------------------------------------------------------------------
// The checker can fail
// ---------------------------------------------------------------------------------------

#[test]
fn violations_should_flag_overflowing_overlapping_and_non_finite_fixtures() {
    let head = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"100\" viewBox=\"0 0 200 100\">\n";
    let overflow = format!("{head}<text x=\"150\" y=\"20\" text-anchor=\"start\" font-size=\"12\" fill=\"#000\">a very long label here</text>\n</svg>\n");
    let v = violations(&overflow);
    assert!(v.iter().any(|m| m.contains("outside")), "{v:?}");

    let overlap = format!("{head}<text x=\"10\" y=\"20\" text-anchor=\"start\" font-size=\"12\" fill=\"#000\">first label</text>\n<text x=\"20\" y=\"22\" text-anchor=\"start\" font-size=\"12\" fill=\"#000\">second label</text>\n</svg>\n");
    let v = violations(&overlap);
    assert!(v.iter().any(|m| m.contains("overlaps")), "{v:?}");

    let out_of_box = format!("{head}<rect x=\"0\" y=\"0\" width=\"260\" height=\"10\" rx=\"0\" fill=\"#000\"/>\n</svg>\n");
    assert!(!violations(&out_of_box).is_empty());

    let nan = format!("{head}<line x1=\"NaN\" y1=\"0\" x2=\"1\" y2=\"1\" stroke=\"#000\" stroke-width=\"1\"/>\n</svg>\n");
    assert!(!violations(&nan).is_empty());

    let inf_path = format!("{head}<path d=\"M0 0L inf 3Z\" fill=\"none\"/>\n</svg>\n");
    assert!(!violations(&inf_path).is_empty());

    let fine = format!("{head}<text x=\"10\" y=\"20\" text-anchor=\"start\" font-size=\"12\" fill=\"#000\">ok</text>\n</svg>\n");
    assert!(violations(&fine).is_empty());
}

// ---------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------

fn worst(file: &str) -> Vec<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/svg-worst")
        .join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .map(str::to_string)
        .collect()
}

const ROWS: usize = 30;

fn pair_report(names: &[String]) -> Report {
    let rows = (0..ROWS)
        .map(|i| {
            let pct = [35.0, -22.0, 4.0, 120.0, -60.0][i % 5] + i as f64 * 0.37;
            let verdict = if pct > 5.0 {
                Verdict::Regressed
            } else if pct < -5.0 {
                Verdict::Improved
            } else {
                Verdict::Unchanged
            };
            let mut row = Row {
                group: String::new(),
                name: names[i % names.len()].clone(),
                measure: String::new(),
                unit: "ns/op".into(),
                better: Better::Lower,
                base: Some(1000.0),
                head: Some(1000.0 + pct * 10.0),
                delta_pct: Some(pct),
                verdict,
                method: Some(Method::Threshold),
                p_value: None,
                n_base: 10,
                n_head: 10,
                head_range: None,
                samples: Default::default(),
            };
            row.samples.base = (0..10).map(|k| 1000.0 + k as f64).collect();
            row.samples.head = (0..10).map(|k| 1000.0 + pct * 10.0 + k as f64).collect();
            row
        })
        .collect();
    Report {
        summary: Summary::default(),
        rows,
        has_base: true,
    }
}

fn matrix_report(names: &[String], variants: &[String]) -> MatrixReport {
    let rows = (0..ROWS)
        .map(|i| {
            let cells = (0..variants.len())
                .map(|c| {
                    let k = 1.0 + 0.11 * c as f64 + 0.013 * ((i * 7 + c) % 5) as f64;
                    let value = 1000.0 * k;
                    let vs_reference = if c == 0 {
                        Comparison::Reference
                    } else {
                        Comparison::Compared {
                            verdict: Verdict::Unchanged,
                            delta: Some((k - 1.0) * 100.0),
                            method: Some(Method::Threshold),
                            p_value: None,
                            p_adjusted: None,
                        }
                    };
                    let mut cell = PresentCell {
                        value,
                        samples: Default::default(),
                        vs_reference,
                    };
                    cell.samples.head = (0..10).map(|j| value + j as f64).collect();
                    Cell::Present(cell)
                })
                .collect();
            MatrixRow {
                group: String::new(),
                name: names[i % names.len()].clone(),
                measure: String::new(),
                unit: "ns/op".into(),
                better: Better::Lower,
                dims: Default::default(),
                cells,
                ranking: vec![],
            }
        })
        .collect();
    MatrixReport {
        summary: MatrixSummary {
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
            columns_cap: 10,
            cell_format: CellFormat::Full,
            sort: MatrixSort::Input,
            reference: Some(ReferenceInfo {
                label: variants[0].clone(),
                explicit: true,
            }),
            skipped: vec![],
            threshold_overrides: BTreeMap::new(),
            config_sources: vec![],
        },
        columns: variants
            .iter()
            .map(|v| VariantLabel::parse(v).unwrap())
            .collect(),
        rows,
        reference_absent: vec![],
        geomean: vec![],
        notices: vec![],
    }
}

fn charts(
    names: &[String],
    variants: &[String],
    width: u32,
    theme: Theme,
) -> Vec<(&'static str, SvgFile)> {
    let opts = SvgOptions {
        width: ChartWidth::new(width).unwrap(),
        rows: ChartRows::new(ROWS).unwrap(),
        max_series: variants.len(),
        ..Default::default()
    };
    let (pair, matrix) = (pair_report(names), matrix_report(names, variants));
    [
        ("delta", delta_chart_themed(&pair, &opts, theme)),
        (
            "distribution",
            distribution_chart_themed(&pair, &opts, theme),
        ),
        ("ranking", ranking_chart_themed(&matrix, &opts, theme)),
        (
            "distribution-matrix",
            distribution_matrix_chart_themed(&matrix, &opts, theme),
        ),
    ]
    .into_iter()
    .map(|(k, r)| (k, r.unwrap_or_else(|e| panic!("{k} skipped: {e:?}"))))
    .collect()
}

/// Violations for one worst-case set across both widths and themes, keyed by chart kind.
fn collect(set: &str, names: &[String], variants: &[String]) -> Vec<String> {
    let mut all = vec![];
    for width in [480, 1600] {
        for (tname, theme) in [("light", Theme::Light), ("dark", Theme::Dark)] {
            for (kind, file) in charts(names, variants, width, theme) {
                for v in violations(&file.svg) {
                    all.push(format!("{set}/{kind}/{tname}/w{width}: {v}"));
                }
            }
        }
    }
    all
}

fn report_violations(set: &str, names: &[String], variants: &[String], kinds: &[&str]) {
    let all: Vec<String> = collect(set, names, variants)
        .into_iter()
        .filter(|v| kinds.iter().any(|k| v.contains(&format!("/{k}/"))))
        .collect();
    assert!(
        all.is_empty(),
        "{} violations, first 8:\n{}",
        all.len(),
        all.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
    );
}

fn variants(n: usize) -> Vec<String> {
    worst(&format!("variants-{n}.txt"))
}

fn sets() -> Vec<(&'static str, Vec<String>)> {
    vec![
        ("long-name", worst("names-long.txt")),
        ("cjk", worst("names-cjk.txt")),
        ("rtl", worst("names-rtl.txt")),
    ]
}

// ---------------------------------------------------------------------------------------
// Worst-case geometry, one test per chart kind so a defect is isolated to its chart
// ---------------------------------------------------------------------------------------

#[test]
fn worst_case_files_should_have_the_documented_shape() {
    let long = worst("names-long.txt");
    assert_eq!((long.len(), long[0].chars().count()), (1, 500));
    assert!(worst("names-cjk.txt").len() >= 3 && worst("names-rtl.txt").len() >= 3);
    assert_eq!(variants(8).len(), 8);
    assert_eq!(variants(10).len(), 10);
}

#[test]
fn delta_chart_should_fit_worst_case_names_when_widths_480_and_1600() {
    for (set, names) in sets() {
        report_violations(set, &names, &variants(8), &["delta"]);
    }
}

#[test]
fn ranking_chart_should_fit_worst_case_names_when_8_and_10_variants() {
    for (set, names) in sets() {
        for n in [8, 10] {
            let set = format!("{set}-{n}v");
            report_violations(&set, &names, &variants(n), &["ranking"]);
        }
    }
}

/// Regression: ranking axis labels (centre `1.00x` and the thinned ticks) must not overlap at width 480.
#[test]
fn ranking_chart_should_not_overlap_reference_label_when_long_variant_label() {
    let names = worst("names-long.txt");
    report_violations("long-variant", &names, &variants(8), &["ranking"]);
}

#[test]
fn distribution_chart_should_fit_worst_case_names_when_pair() {
    for (set, names) in sets() {
        report_violations(set, &names, &variants(8), &["distribution"]);
    }
}

#[test]
fn distribution_matrix_chart_should_fit_worst_case_names_when_8_and_10_variants() {
    for (set, names) in sets() {
        for n in [8, 10] {
            report_violations(
                &format!("{set}-{n}v"),
                &names,
                &variants(n),
                &["distribution-matrix"],
            );
        }
    }
}

#[test]
fn charts_should_have_finite_coordinates_when_ordinary_short_names() {
    let names = vec!["Sort".to_string(), "Parse/size=100".to_string()];
    let short: Vec<String> = ["quick", "heap", "merge", "radix"]
        .map(String::from)
        .to_vec();
    for width in [480, 1600] {
        for (kind, file) in charts(&names, &short, width, Theme::Light) {
            let v = violations(&file.svg);
            assert!(v.is_empty(), "{kind} w{width}: {v:?}");
        }
    }
}
