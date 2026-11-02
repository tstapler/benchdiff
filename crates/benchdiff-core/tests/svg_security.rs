//! Security and determinism corpus for the SVG charts: hostile benchmark names, measures and
//! units must stay inert text in every chart kind and theme, and rendering must be byte-stable.

use benchdiff_core::matrix::{
    CellFormat, Correction, MatrixSort, PresentCell, ReferenceInfo, VariantLabel,
};
use benchdiff_core::svg::delta::{delta_chart, delta_chart_themed};
use benchdiff_core::svg::distribution::{
    distribution_chart, distribution_chart_themed, distribution_matrix_chart,
    distribution_matrix_chart_themed,
};
use benchdiff_core::svg::ranking::{ranking_chart, ranking_chart_themed};
use benchdiff_core::svg::style::Theme;
use benchdiff_core::svg::{ChartTheme, SvgFile, SvgOptions};
use benchdiff_core::{
    Better, Cell, Comparison, MatrixReport, MatrixRow, MatrixSummary, Method, Report, Row, Summary,
    Verdict,
};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------------------
// Strict XML checker (no dependencies). Accepts only the closed subset the writer emits:
// one root element, elements with double-quoted attributes, text with predefined or legal
// numeric entity references. DOCTYPE, comments, CDATA and processing instructions are errors.
// ---------------------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Doc {
    /// (tag name, attributes) in document order.
    elements: Vec<(String, Vec<(String, String)>)>,
    texts: Vec<String>,
}

fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == ':'
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == '-' || c == '.'
}

/// Validates `&...;` references in `s` (text or attribute value) and returns the decoded text.
fn decode(s: &str) -> Result<String, String> {
    if let Some(bad) = s.chars().find(|c| !is_xml_char(*c)) {
        return Err(format!("illegal XML char U+{:04X}", bad as u32));
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i + 1..];
        let end = rest.find(';').ok_or("unterminated entity reference")?;
        let name = &rest[..end];
        let c = match name {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            n if n.starts_with("#x") => char::from_u32(
                u32::from_str_radix(&n[2..], 16).map_err(|_| format!("bad reference &{n};"))?,
            )
            .ok_or_else(|| format!("bad reference &{n};"))?,
            n if n.starts_with('#') => char::from_u32(
                n[1..]
                    .parse::<u32>()
                    .map_err(|_| format!("bad reference &{n};"))?,
            )
            .ok_or_else(|| format!("bad reference &{n};"))?,
            n => return Err(format!("undeclared entity &{n};")),
        };
        if !is_xml_char(c) {
            return Err(format!("reference to illegal char U+{:04X}", c as u32));
        }
        out.push(c);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn parse_xml(src: &str) -> Result<Doc, String> {
    let mut doc = Doc::default();
    let mut stack: Vec<String> = vec![];
    let mut rest = src;
    let mut roots = 0;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('<') {
            if after.starts_with('!') || after.starts_with('?') {
                return Err("DOCTYPE, comment, CDATA and PI are not allowed".into());
            }
            let close = after.find('>').ok_or("unterminated tag")?;
            // An attribute value may not contain '>' here because the writer escapes it.
            let tag = &after[..close];
            rest = &after[close + 1..];
            if let Some(name) = tag.strip_prefix('/') {
                if stack.pop().as_deref() != Some(name) {
                    return Err(format!("mismatched close tag </{name}>"));
                }
                continue;
            }
            let (body, self_closing) = match tag.strip_suffix('/') {
                Some(b) => (b, true),
                None => (tag, false),
            };
            let (name, attrs) = parse_start_tag(body)?;
            if stack.is_empty() {
                roots += 1;
                if roots > 1 {
                    return Err("more than one root element".into());
                }
            }
            doc.elements.push((name.clone(), attrs));
            if !self_closing {
                stack.push(name);
            }
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            let raw = &rest[..end];
            rest = &rest[end..];
            if raw.contains("]]>") {
                return Err("']]>' in text".into());
            }
            let text = decode(raw)?;
            if stack.is_empty() {
                if !text.trim().is_empty() {
                    return Err("text outside the root element".into());
                }
            } else {
                doc.texts.push(text);
            }
        }
    }
    if !stack.is_empty() {
        return Err(format!("unclosed element <{}>", stack.last().unwrap()));
    }
    if roots != 1 {
        return Err("no root element".into());
    }
    Ok(doc)
}

fn parse_start_tag(body: &str) -> Result<(String, Vec<(String, String)>), String> {
    let name_end = body.find(|c: char| !is_name_char(c)).unwrap_or(body.len());
    let name = &body[..name_end];
    if !name.chars().next().is_some_and(is_name_start) {
        return Err(format!("bad element name {name:?}"));
    }
    let mut attrs: Vec<(String, String)> = vec![];
    let mut rest = &body[name_end..];
    while !rest.is_empty() {
        let trimmed = rest.trim_start_matches([' ', '\n', '\t']);
        if trimmed.len() == rest.len() {
            return Err(format!("attributes need leading whitespace: {rest:?}"));
        }
        rest = trimmed;
        if rest.is_empty() {
            break;
        }
        let eq = rest.find('=').ok_or("attribute without '='")?;
        let attr = &rest[..eq];
        if !attr.chars().next().is_some_and(is_name_start) || !attr.chars().all(is_name_char) {
            return Err(format!("bad attribute name {attr:?}"));
        }
        let after = rest[eq + 1..]
            .strip_prefix('"')
            .ok_or("attribute value must be double-quoted")?;
        let q = after.find('"').ok_or("unterminated attribute value")?;
        let raw = &after[..q];
        if raw.contains('<') {
            return Err(format!("raw '<' in attribute {attr}"));
        }
        if attrs.iter().any(|(n, _)| n == attr) {
            return Err(format!("duplicate attribute {attr}"));
        }
        attrs.push((attr.to_string(), decode(raw)?));
        rest = &after[q + 1..];
    }
    Ok((name.to_string(), attrs))
}

/// The structural security properties every emitted chart must have.
fn assert_inert(svg: &str, ctx: &str) {
    let doc = parse_xml(svg).unwrap_or_else(|e| panic!("{ctx}: not well-formed: {e}"));
    let lower = svg.to_ascii_lowercase();
    for banned in [
        "<script",
        "<foreignobject",
        "<!entity",
        "<!doctype",
        "<![cdata[",
    ] {
        assert!(!lower.contains(banned), "{ctx}: contains {banned}");
    }
    const ELEMENTS: [&str; 8] = ["svg", "title", "desc", "rect", "text", "line", "path", "g"];
    for (name, attrs) in &doc.elements {
        assert!(ELEMENTS.contains(&name.as_str()), "{ctx}: element <{name}>");
        for (attr, value) in attrs {
            let a = attr.to_ascii_lowercase();
            assert!(
                !(a.starts_with("on") || a.contains("href") || a == "style" || a.contains("xlink")),
                "{ctx}: attribute {attr} on <{name}>"
            );
            let v = value.to_ascii_lowercase();
            assert!(
                !v.contains("url(") && !v.contains("javascript:") && !v.contains('<'),
                "{ctx}: attribute {attr}={value:?}"
            );
            if attr == "xmlns" {
                assert_eq!(value, "http://www.w3.org/2000/svg", "{ctx}");
            }
        }
    }
    assert_eq!(doc.elements[0].0, "svg", "{ctx}");
}

// ---------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------

fn hostile_corpus() -> Vec<(&'static str, String)> {
    vec![
        ("script-tag", "\"><script>alert(1)</script>".into()),
        ("svg-onload", "\"><svg onload=alert(1)>".into()),
        ("javascript-url", "javascript:alert(document.cookie)".into()),
        ("close-text", "</text><script>x</script><text>".into()),
        (
            "entity-bomb",
            "<!DOCTYPE x [<!ENTITY a \"&a;&a;&a;\">]>&a;&lol9;".into(),
        ),
        ("numeric-nul", "&#x0;&#0;&#xD800;&amp;#x0;".into()),
        (
            "control-chars",
            "a\u{0}b\u{1}c\u{8}d\u{b}e\u{c}f\u{1f}g\u{7f}h\r\ni".into(),
        ),
        ("noncharacters", "x\u{FFFE}y\u{FFFF}z".into()),
        (
            "rtl-override",
            "\u{202E}evil\u{202C} \u{2067}\u{05d1}\u{05d3}\u{05d9}\u{05e7}\u{05d4}".into(),
        ),
        ("long-10kb", "N".repeat(10 * 1024)),
        (
            "emoji",
            "\u{1F980} bench \u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}".into(),
        ),
        ("cdata-end", "a]]>b ]]&gt;".into()),
        ("url-fn", "url(#x) url(javascript:x)".into()),
        ("href", "<a xlink:href=\"javascript:x\">k</a>".into()),
        ("quotes", "it's \"quoted\" & `ticked`".into()),
        ("empty-ish", " ".into()),
    ]
}

fn pair_row(name: &str, measure: &str, unit: &str, pct: f64, i: usize) -> Row {
    let verdict = match pct {
        p if p > 5.0 => Verdict::Regressed,
        p if p < -5.0 => Verdict::Improved,
        _ => Verdict::Unchanged,
    };
    let mut row = Row {
        group: String::new(),
        name: name.into(),
        measure: measure.into(),
        unit: unit.into(),
        better: Better::Lower,
        base: Some(100.0),
        head: Some(100.0 + pct),
        delta_pct: Some(pct),
        verdict,
        method: Some(Method::Threshold),
        p_value: None,
        n_base: 10,
        n_head: 10,
        head_range: None,
        samples: Default::default(),
    };
    row.samples.base = (0..10).map(|k| 100.0 + (k + i) as f64).collect();
    row.samples.head = (0..10).map(|k| 100.0 + pct + (k + i) as f64).collect();
    row
}

fn pair_report(rows: Vec<Row>) -> Report {
    Report {
        summary: Summary::default(),
        rows,
        has_base: true,
    }
}

fn matrix_row(name: &str, measure: &str, unit: &str, base: f64, i: usize) -> MatrixRow {
    let cells = [1.0, 1.3, 0.8]
        .iter()
        .enumerate()
        .map(|(c, k)| {
            let value = base * k;
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
            cell.samples.head = (0..10).map(|j| value + (j + i) as f64).collect();
            Cell::Present(cell)
        })
        .collect();
    MatrixRow {
        group: String::new(),
        name: name.into(),
        measure: measure.into(),
        unit: unit.into(),
        better: Better::Lower,
        dims: Default::default(),
        cells,
        ranking: vec![],
    }
}

fn matrix_report(rows: Vec<MatrixRow>) -> MatrixReport {
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
            columns_cap: 8,
            cell_format: CellFormat::Full,
            sort: MatrixSort::Input,
            reference: Some(ReferenceInfo {
                label: "quick".into(),
                explicit: true,
            }),
            skipped: vec![],
            threshold_overrides: BTreeMap::new(),
            config_sources: vec![],
        },
        columns: ["quick", "heap", "merge"]
            .iter()
            .map(|c| VariantLabel::parse(c).unwrap())
            .collect(),
        rows,
        reference_absent: vec![],
        geomean: vec![],
        notices: vec![],
    }
}

/// One input of each report kind, built from the given (name, measure, unit) triples.
fn inputs(triples: &[(String, String, String)]) -> (Report, MatrixReport) {
    let pct = [30.0, -12.0, 0.5, 80.0, -40.0];
    let pair = triples
        .iter()
        .enumerate()
        .map(|(i, (n, m, u))| pair_row(n, m, u, pct[i % pct.len()], i))
        .collect();
    let matrix = triples
        .iter()
        .enumerate()
        .map(|(i, (n, m, u))| matrix_row(n, m, u, 100.0 + i as f64, i))
        .collect();
    (pair_report(pair), matrix_report(matrix))
}

/// Every chart kind in both themes (and the `Both` file naming), as `(label, file)`.
fn render_all(pair: &Report, matrix: &MatrixReport, width: u32) -> Vec<(String, SvgFile)> {
    let mut out = vec![];
    for (tname, theme) in [("light", Theme::Light), ("dark", Theme::Dark)] {
        for chart_theme in [ChartTheme::Light, ChartTheme::Dark, ChartTheme::Both] {
            let opts = SvgOptions {
                theme: chart_theme,
                width: benchdiff_core::svg::ChartWidth::new(width).unwrap(),
                ..Default::default()
            };
            let tag = format!("{tname}/{chart_theme:?}/w{width}");
            let results = [
                ("delta", delta_chart_themed(pair, &opts, theme)),
                (
                    "distribution",
                    distribution_chart_themed(pair, &opts, theme),
                ),
                ("ranking", ranking_chart_themed(matrix, &opts, theme)),
                (
                    "distribution-matrix",
                    distribution_matrix_chart_themed(matrix, &opts, theme),
                ),
            ];
            for (kind, res) in results {
                if let Ok(file) = res {
                    out.push((format!("{kind}/{tag}"), file));
                }
            }
        }
    }
    // The non-`_themed` entry points too.
    let opts = SvgOptions::default();
    for (kind, res) in [
        ("delta", delta_chart(pair, &opts)),
        ("distribution", distribution_chart(pair, &opts)),
        ("ranking", ranking_chart(matrix, &opts)),
        (
            "distribution-matrix",
            distribution_matrix_chart(matrix, &opts),
        ),
    ] {
        if let Ok(file) = res {
            out.push((format!("{kind}/default"), file));
        }
    }
    out
}

fn triple(n: &str, m: &str, u: &str) -> (String, String, String) {
    (n.into(), m.into(), u.into())
}

// ---------------------------------------------------------------------------------------
// Checker self-tests: the corpus assertions are only meaningful if the checker can fail.
// ---------------------------------------------------------------------------------------

#[test]
fn xml_checker_should_accept_minimal_valid_document() {
    let doc =
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><title>a &amp; &#x41; &lt;</title></svg>\n";
    assert!(parse_xml(doc).is_ok());
}

#[test]
fn xml_checker_should_reject_malformed_documents() {
    let bad = [
        ("raw lt in text", "<svg>a < b</svg>"),
        ("unbalanced", "<svg><g></svg>"),
        ("mismatch", "<svg><g></svg></g>"),
        ("nul reference", "<svg>&#0;</svg>"),
        ("illegal control", "<svg>\u{1}</svg>"),
        ("noncharacter", "<svg>\u{FFFF}</svg>"),
        ("cdata end", "<svg>]]></svg>"),
        ("doctype", "<!DOCTYPE svg><svg/>"),
        ("comment", "<svg><!-- x --></svg>"),
        ("cdata", "<svg><![CDATA[x]]></svg>"),
        ("undeclared entity", "<svg>&a;</svg>"),
        ("bare ampersand", "<svg>a & b</svg>"),
        ("duplicate attr", "<svg a=\"1\" a=\"2\"/>"),
        ("single quoted attr", "<svg a='1'/>"),
        ("raw lt in attr", "<svg a=\"<\"/>"),
        ("two roots", "<svg/><svg/>"),
        ("trailing text", "<svg/>junk"),
    ];
    for (why, doc) in bad {
        assert!(parse_xml(doc).is_err(), "{why}: {doc:?} should be rejected");
    }
}

#[test]
fn inert_check_should_flag_active_content_when_present() {
    let cases = [
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><script>x</script></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" onload=\"x\"/>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><g><foreignObject/></g></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect fill=\"url(#x)\"/></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><path href=\"x\"/></svg>",
    ];
    for doc in cases {
        let r = std::panic::catch_unwind(|| assert_inert(doc, "negative"));
        assert!(r.is_err(), "{doc} should be flagged");
    }
}

// ---------------------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------------------

#[test]
fn charts_should_stay_inert_when_every_hostile_string_is_name_measure_and_unit() {
    for (label, h) in hostile_corpus() {
        let (pair, matrix) = inputs(&[
            triple(&h, "", "ns"),
            triple("Plain", &h, "ns"),
            triple("Other", "m", &h),
            triple(&h, &h, &h),
        ]);
        let rendered = render_all(&pair, &matrix, 760);
        assert!(!rendered.is_empty(), "{label}: nothing rendered");
        for (kind, file) in rendered {
            assert_inert(&file.svg, &format!("{label}/{kind}"));
            assert_inert_alt(&file.alt, &format!("{label}/{kind}"));
        }
    }
}

/// `alt` goes into Markdown/HTML elsewhere; it is plain text here, so only bound its size.
fn assert_inert_alt(alt: &str, ctx: &str) {
    assert!(
        alt.chars().count() <= 125,
        "{ctx}: alt is {} chars",
        alt.chars().count()
    );
}

#[test]
fn charts_should_stay_inert_when_all_hostile_strings_share_one_chart() {
    let triples: Vec<_> = hostile_corpus()
        .into_iter()
        .map(|(_, h)| triple(&h, &h, &h))
        .collect();
    let (pair, matrix) = inputs(&triples);
    for width in [480, 760, 1600] {
        for (kind, file) in render_all(&pair, &matrix, width) {
            assert_inert(&file.svg, &format!("combined/{kind}"));
        }
    }
}

#[test]
fn escaped_payloads_should_survive_as_text_when_rendered() {
    let (pair, _) = inputs(&[triple("\"><script>alert(1)</script>", "", "ns")]);
    let svg = delta_chart(&pair, &SvgOptions::default()).unwrap().svg;
    assert!(svg.contains("&lt;script&gt;"), "{svg}");
}

/// Output sizes of every rendering whose label starts with `prefix`, for one 10 KB name.
fn sizes_for_10kb_name(prefix: &str) -> Vec<(String, usize)> {
    let big = "N".repeat(10 * 1024);
    let (pair, matrix) = inputs(&[triple(&big, &big, &big)]);
    render_all(&pair, &matrix, 760)
        .into_iter()
        .filter(|(kind, _)| kind.starts_with(prefix))
        .map(|(kind, file)| (kind, file.svg.len()))
        .collect()
}

const ONE_NAME_BUDGET: usize = 64 * 1024;

#[test]
fn delta_and_ranking_should_not_grow_without_bound_when_names_are_10kb() {
    for prefix in ["delta", "ranking"] {
        let sizes = sizes_for_10kb_name(prefix);
        assert!(!sizes.is_empty(), "{prefix} rendered nothing");
        for (kind, len) in sizes {
            assert!(
                len < ONE_NAME_BUDGET,
                "{kind}: {len} bytes for one 10 KB name"
            );
        }
    }
}

/// Regression: names and units embedded in titles/desc are bounded, so one 10 KB name stays under budget.
#[test]
fn distribution_should_not_grow_without_bound_when_names_are_10kb() {
    for (kind, len) in sizes_for_10kb_name("distribution") {
        assert!(
            len < ONE_NAME_BUDGET,
            "{kind}: {len} bytes for one 10 KB name"
        );
    }
}

// ---------------------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------------------

#[test]
fn rendering_should_be_byte_identical_when_same_input_twice() {
    let triples: Vec<_> = hostile_corpus()
        .into_iter()
        .map(|(_, h)| triple(&h, "m", "ns"))
        .collect();
    let (pair, matrix) = inputs(&triples);
    let first = render_all(&pair, &matrix, 760);
    let second = render_all(&pair, &matrix, 760);
    assert_eq!(first.len(), second.len());
    for ((ka, a), (kb, b)) in first.iter().zip(&second) {
        assert_eq!(ka, kb);
        assert_eq!(a, b, "{ka} differs between runs");
        assert_eq!(a.svg.as_bytes(), b.svg.as_bytes(), "{ka}");
    }
}

#[test]
fn rendering_should_not_mutate_input_when_called_repeatedly() {
    let (pair, matrix) = inputs(&[triple("A", "", "ns"), triple("B", "", "ns")]);
    let a = render_all(&pair, &matrix, 760);
    let _ = render_all(&pair, &matrix, 480);
    let b = render_all(&pair, &matrix, 760);
    assert_eq!(a, b);
}
