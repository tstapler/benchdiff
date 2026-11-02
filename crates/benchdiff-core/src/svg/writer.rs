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
