//! Chart palette, marker shapes and card colours (task 3.1.3a).
//!
//! Every colour is a literal hex so the SVG needs no `<style>`, `@media` or
//! system-colour keyword (AC-S2-12). Series colour is never used for text.

use super::writer::{fmt_coord, NonFiniteError};
use std::f64::consts::PI;

/// Chart theme; `Light` (opaque light card) stays the default until gate G1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

/// Colour tokens for one theme. All values are `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub card: &'static str,
    pub ink: &'static str,
    pub secondary: &'static str,
    pub regressed: &'static str,
    pub improved: &'static str,
    pub unchanged: &'static str,
    pub series: [&'static str; 6],
}

pub const LIGHT: Palette = Palette {
    card: "#fcfcfb",
    ink: "#0b0b0b",
    secondary: "#52514e",
    regressed: "#d03b3b",
    improved: "#0ca30c",
    unchanged: "#8a8a86",
    series: [
        "#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#6250d6",
    ],
};

// Dark values from the dataviz skill's references/palette.md (Chart chrome & ink,
// Status palette critical/good, Categorical dark column); unchanged = its muted ink.
pub const DARK: Palette = Palette {
    card: "#1a1a19",
    ink: "#f0efec",
    secondary: "#c3c2b7",
    regressed: "#d03b3b",
    improved: "#0ca30c",
    unchanged: "#898781",
    series: [
        "#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#9085e9",
    ],
};

/// Outline width for series markers and bars, in px.
pub const SERIES_STROKE_WIDTH: f64 = 1.5;
const MIN_NON_TEXT_CONTRAST: f64 = 3.0;

impl Palette {
    pub fn for_theme(theme: Theme) -> Palette {
        match theme {
            Theme::Light => LIGHT,
            Theme::Dark => DARK,
        }
    }

    /// Colour for series `i`; colours stay distinct up to 6, then repeat.
    /// Charts cap at 8 series, so series 7 and 8 reuse colours 1 and 2 but
    /// differ in shape-plus-position; see `shape_for_series`.
    pub fn series_color(&self, i: usize) -> &'static str {
        self.series[i % self.series.len()]
    }

    /// Secondary-ink outline for a series whose fill is under 3:1 against the
    /// card, so the object boundary still passes WCAG 1.4.11 (AC-S2-11).
    pub fn series_stroke(&self, i: usize) -> Option<&'static str> {
        let fill = self.series_color(i);
        (contrast_ratio(fill, self.card)? < MIN_NON_TEXT_CONTRAST).then_some(self.secondary)
    }
}

/// Marker shapes, in series order; they repeat for series 7 and 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Circle,
    Square,
    Diamond,
    Triangle,
    Cross,
    Star,
}

pub const SHAPES: [Shape; 6] = [
    Shape::Circle,
    Shape::Square,
    Shape::Diamond,
    Shape::Triangle,
    Shape::Cross,
    Shape::Star,
];

pub fn shape_for_series(i: usize) -> Shape {
    SHAPES[i % SHAPES.len()]
}

impl Shape {
    /// Closed path `d` for a marker centred at (`cx`, `cy`) with radius `r`.
    pub fn path(self, cx: f64, cy: f64, r: f64) -> Result<String, NonFiniteError> {
        let poly = |pts: &[(f64, f64)]| -> Result<String, NonFiniteError> {
            let mut d = String::new();
            for (k, (x, y)) in pts.iter().enumerate() {
                d.push(if k == 0 { 'M' } else { 'L' });
                d.push_str(&fmt_coord(cx + x)?);
                d.push(' ');
                d.push_str(&fmt_coord(cy + y)?);
            }
            d.push('Z');
            Ok(d)
        };
        match self {
            Shape::Circle => {
                let (x0, x1, y, rr) = (
                    fmt_coord(cx - r)?,
                    fmt_coord(cx + r)?,
                    fmt_coord(cy)?,
                    fmt_coord(r)?,
                );
                Ok(format!(
                    "M{x0} {y}A{rr} {rr} 0 1 0 {x1} {y}A{rr} {rr} 0 1 0 {x0} {y}Z"
                ))
            }
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
                ])
            }
            Shape::Star => {
                let inner = r * 0.4;
                let pts: Vec<(f64, f64)> = (0..10)
                    .map(|k| {
                        let rad = if k % 2 == 0 { r } else { inner };
                        let a = -PI / 2.0 + f64::from(k) * PI / 5.0;
                        (rad * a.cos(), rad * a.sin())
                    })
                    .collect();
                poly(&pts)
            }
        }
    }
}

fn parse_hex(hex: &str) -> Option<[f64; 3]> {
    let h = hex.strip_prefix('#').filter(|h| h.len() == 6)?;
    let ch = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(f64::from);
    Some([ch(0)?, ch(2)?, ch(4)?])
}

fn relative_luminance(hex: &str) -> Option<f64> {
    let [r, g, b] = parse_hex(hex)?.map(|c| {
        let c = c / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    Some(0.2126 * r + 0.7152 * g + 0.0722 * b)
}

/// WCAG 2.x contrast ratio between two `#rrggbb` colours.
pub fn contrast_ratio(a: &str, b: &str) -> Option<f64> {
    let (la, lb) = (relative_luminance(a)?, relative_luminance(b)?);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    Some((hi + 0.05) / (lo + 0.05))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cr(a: &str, b: &str) -> f64 {
        contrast_ratio(a, b).expect("valid hex")
    }

    #[test]
    fn default_theme_is_opaque_light_card() {
        assert_eq!(Theme::default(), Theme::Light);
        assert_eq!(Palette::for_theme(Theme::default()).card, "#fcfcfb");
    }

    #[test]
    fn contrast_known_values() {
        assert!((cr("#000000", "#ffffff") - 21.0).abs() < 1e-9);
        assert!((cr("#ffffff", "#ffffff") - 1.0).abs() < 1e-9);
        assert!(contrast_ratio("nope", "#fff").is_none());
    }

    #[test]
    fn text_contrast_at_least_4_5_in_both_themes() {
        for p in [LIGHT, DARK] {
            assert!(cr(p.ink, p.card) >= 4.5, "ink on {}", p.card);
            assert!(cr(p.secondary, p.card) >= 4.5, "secondary on {}", p.card);
        }
        assert!(cr(LIGHT.ink, LIGHT.card) > 19.0);
        assert!(cr(LIGHT.secondary, LIGHT.card) > 7.7);
    }

    #[test]
    fn light_series_that_fail_as_fills_get_secondary_outline() {
        let fails: Vec<usize> = (0..6)
            .filter(|&i| cr(LIGHT.series[i], LIGHT.card) < 3.0)
            .collect();
        assert_eq!(fails, vec![2, 3, 4]);
        for i in 0..6 {
            assert_eq!(
                LIGHT.series_stroke(i).is_some(),
                fails.contains(&i),
                "series {i}"
            );
        }
        assert_eq!(LIGHT.series_stroke(2), Some("#52514e"));
    }

    #[test]
    fn non_text_contrast_at_least_3_by_fill_or_stroke() {
        for p in [LIGHT, DARK] {
            for c in [p.regressed, p.improved, p.unchanged] {
                assert!(cr(c, p.card) >= 3.0, "{c} on {}", p.card);
            }
            for i in 0..8 {
                let fill = cr(p.series_color(i), p.card);
                let stroke = p.series_stroke(i).map_or(0.0, |s| cr(s, p.card));
                assert!(fill >= 3.0 || stroke >= 3.0, "series {i} on {}", p.card);
            }
        }
        assert!(cr("#8a8a86", LIGHT.card) >= 3.3);
        assert_eq!(LIGHT.unchanged, "#8a8a86");
    }

    #[test]
    fn palette_values_are_literal_hex() {
        for p in [LIGHT, DARK] {
            let all = [
                p.card,
                p.ink,
                p.secondary,
                p.regressed,
                p.improved,
                p.unchanged,
            ]
            .into_iter()
            .chain(p.series);
            for c in all {
                assert!(parse_hex(c).is_some(), "{c}");
                assert!(!c.contains("currentColor") && !c.contains("rgb"));
            }
        }
    }

    #[test]
    fn six_shapes_distinct_and_repeat_for_series_7_and_8() {
        let paths: Vec<String> = SHAPES
            .iter()
            .map(|s| s.path(10.0, 10.0, 5.0).unwrap())
            .collect();
        for (i, a) in paths.iter().enumerate() {
            assert!(a.starts_with('M') && a.ends_with('Z'), "{a}");
            for b in &paths[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(shape_for_series(6), Shape::Circle);
        assert_eq!(shape_for_series(7), Shape::Square);
        assert_ne!(
            Palette::series_color(&LIGHT, 0),
            Palette::series_color(&LIGHT, 1)
        );
    }

    #[test]
    fn shape_path_vertices_stay_within_radius() {
        assert_eq!(
            Shape::Diamond.path(10.0, 10.0, 4.0).unwrap(),
            "M10 6L14 10L10 14L6 10Z"
        );
        assert_eq!(
            Shape::Triangle.path(0.0, 0.0, 2.0).unwrap(),
            "M0 -2L2 2L-2 2Z"
        );
        let star = Shape::Star.path(0.0, 0.0, 10.0).unwrap();
        assert_eq!(star.matches('L').count(), 9);
    }

    #[test]
    fn shape_path_rejects_non_finite() {
        assert!(Shape::Square.path(f64::NAN, 0.0, 1.0).is_err());
        assert!(Shape::Circle.path(0.0, 0.0, f64::INFINITY).is_err());
    }

    #[test]
    fn light_card_has_no_style_media_or_system_colours() {
        let mut w = super::super::writer::SvgWriter::new(100.0, 50.0);
        w.rect(0.0, 0.0, 100.0, 50.0, "card");
        let svg = w.finish().unwrap();
        for banned in ["<style", "@media", "currentColor", "prefers-color-scheme"] {
            assert!(!svg.contains(banned), "{banned}");
        }
    }
}
