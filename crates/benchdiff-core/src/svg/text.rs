//! Text-width estimation and truncation without font access (WASM has none).
//!
//! Advances are Helvetica widths per 1000 em for ASCII 0x20..=0x7E, from Adobe's
//! Core 14 `Helvetica.afm` (Adobe's AFM licence permits free redistribution with the
//! copyright notice); Arial is metric-compatible. Non-ASCII counts 1.0 em.

/// Safety margin on every estimate, since the real font may differ from Helvetica.
const MARGIN: f64 = 1.10;
const ELLIPSIS: char = '…';

#[rustfmt::skip]
const ADVANCE_1000: [u16; 95] = [
    // space ! " # $ % & ' ( )
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333,
    // * + , - . / 0-9
    389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556, 556, 556, 556,
    // : ; < = > ? @
    278, 278, 584, 584, 584, 556, 1015,
    // A-Z
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833,
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611,
    // [ \ ] ^ _ `
    278, 278, 278, 469, 556, 333,
    // a-z
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833,
    556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500,
    // { | } ~
    334, 260, 334, 584,
];

fn advance_em(c: char) -> f64 {
    match c {
        ' '..='~' => f64::from(ADVANCE_1000[c as usize - 0x20]) / 1000.0,
        _ => 1.0,
    }
}

/// Estimated rendered width in px at `font_px`, including the 10% margin.
pub fn text_width(text: &str, font_px: f64) -> f64 {
    text.chars().map(advance_em).sum::<f64>() * font_px * MARGIN
}

/// `text` shortened with `…` to fit `max_px`; unchanged when it already fits.
///
/// Keeps the suffix from the last `/` (the sub-benchmark label) when one exists and
/// fits, so `Name/impl=quick` becomes `Na…/impl=quick`; otherwise truncates the end.
pub fn truncate_to_width(text: &str, font_px: f64, max_px: f64) -> String {
    if text_width(text, font_px) <= max_px {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    if let Some(slash) = chars.iter().rposition(|&c| c == '/') {
        let suffix: String = chars[slash..].iter().collect();
        if let Some(head) = longest_prefix(&chars[..slash], &suffix, font_px, max_px) {
            return format!("{head}{ELLIPSIS}{suffix}");
        }
    }
    let head = longest_prefix(&chars, "", font_px, max_px).unwrap_or_default();
    format!("{head}{ELLIPSIS}")
}

/// Longest prefix of `chars` such that `prefix + … + suffix` fits; `None` if even the
/// empty prefix does not fit.
fn longest_prefix(chars: &[char], suffix: &str, font_px: f64, max_px: f64) -> Option<String> {
    let fixed = text_width(suffix, font_px) + advance_em(ELLIPSIS) * font_px * MARGIN;
    if fixed > max_px {
        return None;
    }
    let mut used = fixed;
    let mut head = String::new();
    for &c in chars {
        used += advance_em(c) * font_px * MARGIN;
        if used > max_px {
            break;
        }
        head.push(c);
    }
    Some(head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::writer::xml_escape;

    #[test]
    fn text_width_should_use_advance_table_with_ten_percent_margin_when_ascii_label() {
        // H=722 i=222 -> 944/1000 em
        let expected = 0.944 * 12.0 * 1.10;
        assert!((text_width("Hi", 12.0) - expected).abs() < 1e-9);
        assert!((text_width(" ", 10.0) - 0.278 * 10.0 * 1.10).abs() < 1e-9);
        assert!((text_width("~", 10.0) - 0.584 * 10.0 * 1.10).abs() < 1e-9);
    }

    #[test]
    fn text_width_should_count_one_em_when_cjk_char() {
        let expected = 3.0 * 12.0 * 1.10;
        assert!((text_width("日本語", 12.0) - expected).abs() < 1e-9);
    }

    #[test]
    fn truncate_should_middle_truncate_keeping_suffix_when_name_too_long() {
        let name = "BenchmarkSortLargeArrays/impl=quick";
        let out = truncate_to_width(name, 12.0, 212.0);
        assert!(out.starts_with("BenchmarkSort"), "{out}");
        assert!(out.ends_with("…/impl=quick"), "{out}");
        assert!(text_width(&out, 12.0) <= 212.0);
        assert_eq!(truncate_to_width("Short", 12.0, 212.0), "Short");
    }

    #[test]
    fn truncate_should_cut_end_when_no_slash_or_suffix_too_wide() {
        let out = truncate_to_width(&"x".repeat(80), 12.0, 100.0);
        assert!(out.ends_with('…') && text_width(&out, 12.0) <= 100.0);
        let wide = format!("abc/{}", "w".repeat(60));
        let out = truncate_to_width(&wide, 12.0, 100.0);
        assert!(
            out.ends_with('…') && text_width(&out, 12.0) <= 100.0,
            "{out}"
        );
        assert_eq!(truncate_to_width("anything", 12.0, 1.0), "…");
    }

    #[test]
    fn ux_s2_6_name_with_80_chars_and_markup_should_truncate_with_ellipsis_escaped_and_fit_gutter()
    {
        let name = format!("<&\">{}", "a".repeat(76));
        assert_eq!(name.chars().count(), 80);
        let out = truncate_to_width(&name, 12.0, 212.0);
        assert!(out.contains('…'));
        assert!(text_width(&out, 12.0) <= 212.0);
        let escaped = xml_escape(&out);
        assert!(
            escaped.contains("&lt;") && escaped.contains("&amp;") && escaped.contains("&quot;")
        );
        assert!(!escaped.contains('<'));
    }
}
