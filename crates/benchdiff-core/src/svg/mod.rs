pub mod delta;
pub mod distribution;
pub mod ranking;
pub mod style;
pub mod text;
pub mod writer;

use std::fmt;

/// Names the chart kinds in config/flag order; the single source for error text.
const KIND_NAMES: &str = "none, delta, ranking, distribution";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Delta,
    Ranking,
    Distribution,
}

impl ChartKind {
    pub fn name(self) -> &'static str {
        match self {
            ChartKind::Delta => "delta",
            ChartKind::Ranking => "ranking",
            ChartKind::Distribution => "distribution",
        }
    }

    /// Parses a comma-separated list (`none` alone means no charts); duplicates collapse, order kept.
    /// The error names the offending value and lists the valid ones.
    pub fn parse_list(text: &str) -> Result<Vec<ChartKind>, String> {
        let bad = |s: &str| format!("{s:?} is not valid; use {KIND_NAMES} (comma-separated)");
        let mut kinds = vec![];
        for part in text.split(',').map(str::trim) {
            let kind = match part {
                "delta" => ChartKind::Delta,
                "ranking" => ChartKind::Ranking,
                "distribution" => ChartKind::Distribution,
                "none" if text.trim() == "none" => return Ok(vec![]),
                _ => return Err(bad(text.trim())),
            };
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
        Ok(kinds)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartTheme {
    #[default]
    Light,
    Dark,
    Both,
}

impl ChartTheme {
    pub const NAMES: &'static str = "light, dark, both";

    pub fn from_name(s: &str) -> Result<ChartTheme, String> {
        match s {
            "light" => Ok(ChartTheme::Light),
            "dark" => Ok(ChartTheme::Dark),
            "both" => Ok(ChartTheme::Both),
            other => Err(format!("{other:?} is not valid; use {}", Self::NAMES)),
        }
    }
}

/// Rows drawn per chart, 1..=200.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartRows(usize);

impl ChartRows {
    pub const MIN: usize = 1;
    pub const MAX: usize = 200;
    pub const DEFAULT: usize = 30;

    pub fn new(n: usize) -> Result<ChartRows, String> {
        if (Self::MIN..=Self::MAX).contains(&n) {
            Ok(ChartRows(n))
        } else {
            Err(format!(
                "{n} is not valid; use {} to {}",
                Self::MIN,
                Self::MAX
            ))
        }
    }

    pub fn get(self) -> usize {
        self.0
    }
}

impl Default for ChartRows {
    fn default() -> Self {
        ChartRows(Self::DEFAULT)
    }
}

/// Chart width in pixels, 480..=1600.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartWidth(u32);

impl ChartWidth {
    pub const MIN: u32 = 480;
    pub const MAX: u32 = 1600;
    pub const DEFAULT: u32 = 760;

    pub fn new(n: u32) -> Result<ChartWidth, String> {
        if (Self::MIN..=Self::MAX).contains(&n) {
            Ok(ChartWidth(n))
        } else {
            Err(format!(
                "{n} is not valid; use {} to {}",
                Self::MIN,
                Self::MAX
            ))
        }
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl Default for ChartWidth {
    fn default() -> Self {
        ChartWidth(Self::DEFAULT)
    }
}

/// Resolved embedding decision (see [`EmbedSetting`] for the user-facing `true|false|auto`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmbedMode {
    On,
    #[default]
    Off,
}

/// A `svg-embed` value as given. `Auto` is a default-layer value: it never overrides a concrete one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedSetting {
    True,
    False,
    Auto,
}

impl EmbedSetting {
    pub const NAMES: &'static str = "true, false, auto";

    pub fn from_name(s: &str) -> Result<EmbedSetting, String> {
        match s {
            "true" => Ok(EmbedSetting::True),
            "false" => Ok(EmbedSetting::False),
            "auto" => Ok(EmbedSetting::Auto),
            other => Err(format!("{other:?} is not valid; use {}", Self::NAMES)),
        }
    }
}

/// Chart settings; `Default` is "no charts".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvgOptions {
    pub kinds: Vec<ChartKind>,
    pub rows: ChartRows,
    pub width: ChartWidth,
    pub theme: ChartTheme,
    pub max_series: usize,
    pub embed: EmbedMode,
    /// Consulted only when embedding; the CLI never publishes.
    pub publish_images: bool,
}

impl SvgOptions {
    pub const DEFAULT_MAX_SERIES: usize = 8;
}

impl Default for SvgOptions {
    fn default() -> Self {
        SvgOptions {
            kinds: vec![],
            rows: ChartRows::default(),
            width: ChartWidth::default(),
            theme: ChartTheme::default(),
            max_series: Self::DEFAULT_MAX_SERIES,
            embed: EmbedMode::Off,
            publish_images: true,
        }
    }
}

/// Why a chart produced no file; surfaced in the report and logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    NothingToCompare,
    NoSamples,
    CapExceeded { shown: usize, total: usize },
    Empty,
    NotApplicable { kind: ChartKind },
}

/// One rendered chart. Chart functions return `Result<SvgFile, SkipReason>` (`Err` = no file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvgFile {
    pub name: String,
    pub svg: String,
    pub alt: String,
    pub skipped: Option<SkipReason>,
}

/// One row of the `Chart data (text)` table; `index` is the `#` shown in the chart label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRow {
    pub index: usize,
    pub label: String,
    pub cells: Vec<String>,
    pub bar: Option<String>,
}

impl fmt::Display for ChartKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_kind_parse_list_should_accept_lists_and_none_and_dedupe() {
        assert_eq!(ChartKind::parse_list("none").unwrap(), vec![]);
        assert_eq!(
            ChartKind::parse_list("ranking, delta,ranking").unwrap(),
            vec![ChartKind::Ranking, ChartKind::Delta]
        );
    }

    #[test]
    fn chart_kind_parse_list_should_list_valid_values_when_unknown_or_none_mixed() {
        let want =
            r#""bars" is not valid; use none, delta, ranking, distribution (comma-separated)"#;
        assert_eq!(ChartKind::parse_list("bars").unwrap_err(), want);
        assert!(ChartKind::parse_list("none,delta").is_err());
        assert!(ChartKind::parse_list("").is_err());
    }

    #[test]
    fn newtypes_should_enforce_ranges_with_stated_bounds() {
        assert_eq!(
            ChartRows::new(0).unwrap_err(),
            "0 is not valid; use 1 to 200"
        );
        assert!(ChartRows::new(201).is_err());
        assert_eq!(ChartRows::new(200).unwrap().get(), 200);
        assert_eq!(
            ChartWidth::new(100).unwrap_err(),
            "100 is not valid; use 480 to 1600"
        );
        assert!(ChartWidth::new(1601).is_err());
        assert_eq!(ChartWidth::new(480).unwrap().get(), 480);
    }

    #[test]
    fn svg_options_default_should_be_no_charts_with_documented_defaults() {
        let d = SvgOptions::default();
        assert!(d.kinds.is_empty());
        assert_eq!((d.rows.get(), d.width.get()), (30, 760));
        assert_eq!(
            (d.theme, d.embed, d.publish_images),
            (ChartTheme::Light, EmbedMode::Off, true)
        );
        assert_eq!(d.max_series, 8);
    }

    #[test]
    fn theme_and_embed_setting_should_list_valid_values_when_unknown() {
        assert_eq!(ChartTheme::from_name("both").unwrap(), ChartTheme::Both);
        assert_eq!(
            ChartTheme::from_name("x").unwrap_err(),
            r#""x" is not valid; use light, dark, both"#
        );
        assert_eq!(EmbedSetting::from_name("auto").unwrap(), EmbedSetting::Auto);
        assert!(EmbedSetting::from_name("yes")
            .unwrap_err()
            .contains("use true, false, auto"));
    }
}
