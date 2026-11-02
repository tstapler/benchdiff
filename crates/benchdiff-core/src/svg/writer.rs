//! Closed-template SVG builder: the single place that escapes text and
//! formats numbers. User text enters only through `text`/`title`/`desc`;
//! attributes are numbers or `'static` class names, so no script,
//! `foreignObject` or link element can be produced.

use std::fmt;

/// A non-finite number reached the writer; the document is never emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonFiniteError;

impl fmt::Display for NonFiniteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("non-finite number in SVG output")
    }
}

impl std::error::Error for NonFiniteError {}

/// Escape text for an XML 1.0 text node or attribute value. Characters
/// outside the XML 1.0 `Char` production become U+FFFD.
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
    out
}

/// Format a coordinate with at most 2 decimals, trailing zeros trimmed and
/// negative zero normalised to `0`. Errors on NaN or infinity.
pub fn fmt_coord(v: f64) -> Result<String, NonFiniteError> {
    if !v.is_finite() {
        return Err(NonFiniteError);
    }
    let mut s = format!("{v:.2}");
    if s.contains('.') {
        s.truncate(s.trim_end_matches('0').trim_end_matches('.').len());
    }
    if s == "-0" {
        s = "0".to_string();
    }
    Ok(s)
}

/// Horizontal text anchor; a closed set so no caller text reaches the attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Anchor {
    Start,
    Middle,
    End,
}

impl Anchor {
    fn as_str(self) -> &'static str {
        match self {
            Anchor::Start => "start",
            Anchor::Middle => "middle",
            Anchor::End => "end",
        }
    }
}

/// Path data built only from commands and `fmt_coord` numbers, so a path
/// string cannot carry caller text. A non-finite number is remembered and
/// surfaces from `SvgWriter::finish` once the path is drawn.
#[derive(Debug, Default)]
pub(crate) struct PathBuilder {
    d: String,
    error: Option<NonFiniteError>,
}

impl PathBuilder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn pair(&mut self, cmd: char, x: f64, y: f64) {
        match (fmt_coord(x), fmt_coord(y)) {
            (Ok(x), Ok(y)) => {
                self.d.push(cmd);
                self.d.push_str(&format!("{x} {y}"));
            }
            (a, b) => self.error = a.err().or(b.err()),
        }
    }

    pub(crate) fn move_to(&mut self, x: f64, y: f64) -> &mut Self {
        self.pair('M', x, y);
        self
    }

    pub(crate) fn line_to(&mut self, x: f64, y: f64) -> &mut Self {
        self.pair('L', x, y);
        self
    }

    /// Elliptical arc with no rotation.
    pub(crate) fn arc(
        &mut self,
        rx: f64,
        ry: f64,
        large: bool,
        sweep: bool,
        x: f64,
        y: f64,
    ) -> &mut Self {
        match (fmt_coord(rx), fmt_coord(ry), fmt_coord(x), fmt_coord(y)) {
            (Ok(rx), Ok(ry), Ok(x), Ok(y)) => {
                self.d.push_str(&format!(
                    "A{rx} {ry} 0 {} {} {x} {y}",
                    u8::from(large),
                    u8::from(sweep)
                ));
            }
            (a, b, c, d) => self.error = a.err().or(b.err()).or(c.err()).or(d.err()),
        }
        self
    }

    pub(crate) fn close(&mut self) -> &mut Self {
        self.d.push('Z');
        self
    }

    /// Closed circle of radius `r` as two half-circle arcs.
    pub(crate) fn circle(&mut self, cx: f64, cy: f64, r: f64) -> &mut Self {
        self.move_to(cx - r, cy)
            .arc(r, r, true, false, cx + r, cy)
            .arc(r, r, true, false, cx - r, cy)
            .close()
    }
}

/// Accumulates an SVG document. A non-finite number poisons the writer and
/// surfaces from `finish`.
pub struct SvgWriter {
    buf: String,
    error: Option<NonFiniteError>,
    depth: usize,
    next_id: usize,
}

impl SvgWriter {
    /// Start a document with the given pixel size.
    pub fn new(width: f64, height: f64) -> Self {
        let mut w = SvgWriter {
            buf: String::new(),
            error: None,
            depth: 1,
            next_id: 0,
        };
        let (ws, hs) = (w.num(width), w.num(height));
        w.buf.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{ws}\" height=\"{hs}\" viewBox=\"0 0 {ws} {hs}\">\n"
        ));
        w
    }

    /// Start an opaque, style-free document: `role="img"` labelled by the
    /// `<title id="t">`/`<desc id="d">` written here, with a root `font-family`.
    /// Pair with the attribute primitives below (no `class`, no `<style>`).
    pub(crate) fn new_accessible(
        width: f64,
        height: f64,
        font_family: &'static str,
        title: &str,
        desc: &str,
    ) -> Self {
        let mut w = SvgWriter {
            buf: String::new(),
            error: None,
            depth: 1,
            next_id: 0,
        };
        let (ws, hs) = (w.num(width), w.num(height));
        w.buf.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{ws}\" height=\"{hs}\" viewBox=\"0 0 {ws} {hs}\" role=\"img\" aria-labelledby=\"t d\" font-family=\"{}\">\n",
            xml_escape(font_family)
        ));
        w.buf.push_str(&format!(
            "<title id=\"t\">{}</title>\n<desc id=\"d\">{}</desc>\n",
            xml_escape(title),
            xml_escape(desc)
        ));
        w
    }

    /// Plain `<g>` whose first child is a `<title>` (hover/a11y name).
    pub(crate) fn open_titled_group(&mut self, title: &str) {
        self.buf.push_str("<g>\n");
        self.depth += 1;
        self.title(title);
    }

    /// Filled rect with corner radius; `fill` is escaped.
    pub(crate) fn rect_fill(&mut self, x: f64, y: f64, w: f64, h: f64, rx: f64, fill: &str) {
        let (x, y, w, h, rx) = (
            self.num(x),
            self.num(y),
            self.num(w),
            self.num(h),
            self.num(rx),
        );
        self.buf.push_str(&format!(
            "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"{rx}\" fill=\"{}\"/>\n",
            xml_escape(fill)
        ));
    }

    /// Stroked line; `stroke` is escaped.
    pub(crate) fn line_stroke(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke: &str,
        stroke_width: f64,
    ) {
        let (x1, y1, x2, y2, sw) = (
            self.num(x1),
            self.num(y1),
            self.num(x2),
            self.num(y2),
            self.num(stroke_width),
        );
        self.buf.push_str(&format!(
            "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" stroke=\"{}\" stroke-width=\"{sw}\"/>\n",
            xml_escape(stroke)
        ));
    }

    /// Text with anchor, font size and fill; `fill` and `content` are escaped.
    pub(crate) fn text_styled(
        &mut self,
        x: f64,
        y: f64,
        anchor: Anchor,
        font_size: f64,
        fill: &str,
        content: &str,
    ) {
        let (x, y, size) = (self.num(x), self.num(y), self.num(font_size));
        self.buf.push_str(&format!(
            "<text x=\"{x}\" y=\"{y}\" text-anchor=\"{}\" font-size=\"{size}\" fill=\"{}\">{}</text>\n",
            anchor.as_str(),
            xml_escape(fill),
            xml_escape(content)
        ));
    }

    /// Path from a `PathBuilder`; `fill` and `stroke` are escaped.
    pub(crate) fn path_styled(
        &mut self,
        path: &PathBuilder,
        fill: &str,
        stroke: &str,
        stroke_width: f64,
    ) {
        if let Some(e) = path.error {
            self.error = Some(e);
        }
        let sw = self.num(stroke_width);
        self.buf.push_str(&format!(
            "<path d=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{sw}\"/>\n",
            path.d,
            xml_escape(fill),
            xml_escape(stroke)
        ));
    }

    /// Deterministic id from a counter (`s0`, `s1`, ...).
    pub fn next_id(&mut self) -> String {
        let id = format!("s{}", self.next_id);
        self.next_id += 1;
        id
    }

    fn num(&mut self, v: f64) -> String {
        fmt_coord(v).unwrap_or_else(|e| {
            self.error = Some(e);
            "0".to_string()
        })
    }

    pub fn open_group(&mut self, class: &'static str) {
        self.buf
            .push_str(&format!("<g class=\"{}\">\n", xml_escape(class)));
        self.depth += 1;
    }

    pub fn close_group(&mut self) {
        if self.depth > 1 {
            self.depth -= 1;
            self.buf.push_str("</g>\n");
        }
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, class: &'static str) {
        let (x, y, w, h) = (self.num(x), self.num(y), self.num(w), self.num(h));
        self.buf.push_str(&format!(
            "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" class=\"{}\"/>\n",
            xml_escape(class)
        ));
    }

    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, class: &'static str) {
        let (x1, y1, x2, y2) = (self.num(x1), self.num(y1), self.num(x2), self.num(y2));
        self.buf.push_str(&format!(
            "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" class=\"{}\"/>\n",
            xml_escape(class)
        ));
    }

    /// Text element; `content` is escaped.
    pub fn text(&mut self, x: f64, y: f64, class: &'static str, content: &str) {
        let (x, y) = (self.num(x), self.num(y));
        self.buf.push_str(&format!(
            "<text x=\"{x}\" y=\"{y}\" class=\"{}\">{}</text>\n",
            xml_escape(class),
            xml_escape(content)
        ));
    }

    pub fn title(&mut self, content: &str) {
        self.buf
            .push_str(&format!("<title>{}</title>\n", xml_escape(content)));
    }

    pub fn desc(&mut self, content: &str) {
        self.buf
            .push_str(&format!("<desc>{}</desc>\n", xml_escape(content)));
    }

    /// Close open groups and the root, returning the document.
    pub fn finish(mut self) -> Result<String, NonFiniteError> {
        if let Some(e) = self.error {
            return Err(e);
        }
        while self.depth > 1 {
            self.close_group();
        }
        self.buf.push_str("</svg>\n");
        Ok(self.buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal tag-balance check: every open tag closes in order.
    fn balanced(doc: &str) -> bool {
        let mut stack: Vec<String> = Vec::new();
        let mut rest = doc;
        while let Some(i) = rest.find('<') {
            rest = &rest[i + 1..];
            let Some(j) = rest.find('>') else {
                return false;
            };
            let tag = &rest[..j];
            rest = &rest[j + 1..];
            if let Some(name) = tag.strip_prefix('/') {
                if stack.pop().as_deref() != Some(name) {
                    return false;
                }
            } else if !tag.ends_with('/') {
                let name = tag.split_whitespace().next().unwrap_or("");
                stack.push(name.to_string());
            }
        }
        stack.is_empty() && !rest.contains('>')
    }

    #[test]
    fn xml_escape_should_encode_markup_chars_when_script_payload() {
        let out = xml_escape("\"><script>alert(1)</script> & ]]>");
        assert_eq!(
            out,
            "&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt; &amp; ]]&gt;"
        );
        assert!(!out.contains("<script"));
    }

    #[test]
    fn xml_escape_should_replace_control_chars_with_fffd_when_nul_or_backspace() {
        assert_eq!(xml_escape("a\u{0}b\u{8}c\td"), "a\u{FFFD}b\u{FFFD}c\td");
    }

    #[test]
    fn fmt_coord_should_normalise_numbers() {
        assert_eq!(fmt_coord(-0.0).unwrap(), "0");
        assert_eq!(fmt_coord(1.0).unwrap(), "1");
        assert_eq!(fmt_coord(0.30000000000000004).unwrap(), "0.3");
        assert_eq!(fmt_coord(-12.345).unwrap(), "-12.35");
        assert_eq!(fmt_coord(100.0).unwrap(), "100");
        assert_eq!(fmt_coord(-0.001).unwrap(), "0");
    }

    #[test]
    fn fmt_coord_should_error_when_non_finite() {
        assert!(fmt_coord(f64::NAN).is_err());
        assert!(fmt_coord(f64::INFINITY).is_err());
        assert!(fmt_coord(f64::NEG_INFINITY).is_err());
    }

    #[test]
    fn writer_should_emit_balanced_closed_template_when_hostile_text() {
        let mut w = SvgWriter::new(200.0, 100.5);
        w.title("<script>x</script>");
        w.desc("a & b");
        w.open_group("bars");
        w.rect(1.0, 2.0, 3.0, 4.0, "bar");
        w.line(0.0, 0.0, 10.0, 10.0, "axis");
        w.text(5.0, 6.0, "label", "\u{0}<foreignObject>&");
        let id = w.next_id();
        assert_eq!(id, "s0");
        assert_eq!(w.next_id(), "s1");
        let doc = w.finish().unwrap();
        assert!(balanced(&doc), "{doc}");
        assert!(!doc.contains("<script") && !doc.contains("<foreignObject"));
        assert!(!doc.contains("href") && !doc.contains('\u{0}'));
        assert!(doc.starts_with(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"100.5\""
        ));
        assert!(doc.ends_with("</svg>\n"));
    }

    #[test]
    fn writer_should_fail_finish_when_nan_coordinate() {
        let mut w = SvgWriter::new(10.0, 10.0);
        w.rect(f64::NAN, 0.0, 1.0, 1.0, "bar");
        assert_eq!(w.finish(), Err(NonFiniteError));
    }

    const HOSTILE: &str = "\"><script>x</script>&'";
    const HOSTILE_ESC: &str = "&quot;&gt;&lt;script&gt;x&lt;/script&gt;&amp;&apos;";

    #[test]
    fn attribute_primitives_should_escape_attribute_values_and_text() {
        let mut w = SvgWriter::new_accessible(100.0, 50.0, HOSTILE, HOSTILE, HOSTILE);
        w.open_titled_group(HOSTILE);
        w.rect_fill(1.0, 2.0, 3.0, 4.0, 0.5, HOSTILE);
        w.line_stroke(0.0, 0.0, 1.0, 1.0, HOSTILE, 1.0);
        w.text_styled(1.0, 2.0, Anchor::End, 12.0, HOSTILE, HOSTILE);
        let mut p = PathBuilder::new();
        p.move_to(1.0, 2.0).line_to(3.0, 4.0).close();
        w.path_styled(&p, HOSTILE, HOSTILE, 1.5);
        let doc = w.finish().unwrap();
        assert!(balanced(&doc), "{doc}");
        assert!(!doc.contains("<script") && !doc.contains("\"><"));
        assert!(doc.contains(&format!("font-family=\"{HOSTILE_ESC}\"")));
        assert!(doc.contains(&format!("<title id=\"t\">{HOSTILE_ESC}</title>")));
        assert!(doc.contains(&format!("fill=\"{HOSTILE_ESC}\"/>")));
        assert!(doc.contains(&format!("stroke=\"{HOSTILE_ESC}\" stroke-width=\"1\"/>")));
        assert!(doc.contains(&format!(
            "text-anchor=\"end\" font-size=\"12\" fill=\"{HOSTILE_ESC}\">{HOSTILE_ESC}</text>"
        )));
        assert!(doc.contains("<path d=\"M1 2L3 4Z\""));
        assert!(doc.contains(&format!("<g>\n<title>{HOSTILE_ESC}</title>\n")));
        assert!(doc.contains("role=\"img\" aria-labelledby=\"t d\""));
    }

    #[test]
    fn attribute_primitives_should_fail_finish_when_non_finite() {
        let cases: [fn(&mut SvgWriter); 5] = [
            |w| w.rect_fill(f64::NAN, 0.0, 1.0, 1.0, 0.0, "#000"),
            |w| w.line_stroke(0.0, 0.0, f64::INFINITY, 1.0, "#000", 1.0),
            |w| w.text_styled(0.0, f64::NEG_INFINITY, Anchor::Start, 12.0, "#000", "x"),
            |w| w.text_styled(0.0, 0.0, Anchor::Start, f64::NAN, "#000", "x"),
            |w| {
                let mut p = PathBuilder::new();
                p.move_to(0.0, 0.0)
                    .arc(1.0, f64::NAN, true, false, 1.0, 1.0);
                w.path_styled(&p, "none", "#000", 1.0);
            },
        ];
        for draw in cases {
            let mut w = SvgWriter::new_accessible(10.0, 10.0, "sans-serif", "t", "d");
            draw(&mut w);
            assert_eq!(w.finish(), Err(NonFiniteError));
        }
        let mut w = SvgWriter::new_accessible(f64::NAN, 10.0, "sans-serif", "t", "d");
        w.close_group();
        assert_eq!(w.finish(), Err(NonFiniteError));
    }

    #[test]
    fn path_builder_should_format_arcs_with_flags() {
        let mut p = PathBuilder::new();
        p.move_to(0.0, 5.0)
            .arc(6.0, 6.0, true, false, 12.0, 5.0)
            .close();
        let mut w = SvgWriter::new_accessible(10.0, 10.0, "f", "t", "d");
        w.path_styled(&p, "none", "#111", 1.5);
        let doc = w.finish().unwrap();
        assert!(doc.contains("d=\"M0 5A6 6 0 1 0 12 5Z\""), "{doc}");
    }

    #[test]
    fn writer_should_be_deterministic() {
        let build = || {
            let mut w = SvgWriter::new(10.0, 10.0);
            w.text(0.1 + 0.2, 1.0, "t", "x");
            w.finish().unwrap()
        };
        assert_eq!(build(), build());
    }
}
