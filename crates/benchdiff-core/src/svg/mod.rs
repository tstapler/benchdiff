pub mod delta;
pub mod distribution;
pub mod ranking;
pub mod style;
pub mod text;
pub mod writer;

use crate::compare::{Report, Row, Verdict};
use crate::matrix::{Cell, Comparison, MatrixReport, MatrixRow};
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

/// The report a chart draws from; the one input type of `select_rows`.
#[derive(Debug, Clone, Copy)]
pub enum SvgInput<'a> {
    Pair(&'a Report),
    Matrix(&'a MatrixReport),
}

/// A report row of either kind.
#[derive(Debug, Clone, Copy)]
pub enum RowRef<'a> {
    Pair(&'a Row),
    Matrix(&'a MatrixRow),
}

impl RowRef<'_> {
    pub fn label(&self) -> String {
        match self {
            RowRef::Pair(r) => r.label(),
            RowRef::Matrix(r) => r.label(),
        }
    }
}

/// The setting that limits how many rows a chart shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitingKey {
    ChartRows,
    MaxRows,
}

impl LimitingKey {
    pub fn name(self) -> &'static str {
        match self {
            LimitingKey::ChartRows => "chart-rows",
            LimitingKey::MaxRows => "max-rows",
        }
    }
}

/// One chart row: its 1-based chart position, the `N. name` label and the |delta| it was ranked by.
#[derive(Debug, Clone)]
pub struct SelectedRow<'a> {
    pub index: usize,
    pub label: String,
    pub delta: f64,
    pub row: RowRef<'a>,
}

/// The rows a chart (and its text block) shows, in drawing order, with the cap bookkeeping.
#[derive(Debug, Clone)]
pub struct Selected<'a> {
    pub rows: Vec<SelectedRow<'a>>,
    /// Rows the chart could have drawn with no cap.
    pub population: usize,
    /// The effective cap, `min(chart-rows, max-rows)` when `max-rows` is set.
    pub cap: usize,
    pub limiting_key: LimitingKey,
}

impl Selected<'_> {
    /// `population - shown`, the one "N more" count of the SVG footer, the Markdown notice and the text block.
    pub fn more(&self) -> usize {
        self.population - self.rows.len()
    }

    /// The in-SVG footer text, `None` when nothing was cut.
    pub fn footer(&self) -> Option<String> {
        (self.more() > 0).then(|| format!("+ {} more not shown", self.more()))
    }

    /// The Markdown/text-block line, naming the limiting key.
    pub fn notice(&self) -> Option<String> {
        self.footer()
            .map(|f| format!("{f} ({}: {})", self.limiting_key.name(), self.cap))
    }

    /// `Some(CapExceeded)` when rows were cut; goes to `SvgFile.skipped`.
    pub fn skip_reason(&self) -> Option<SkipReason> {
        (self.more() > 0).then_some(SkipReason::CapExceeded {
            shown: self.rows.len(),
            total: self.population,
        })
    }
}

impl SvgOptions {
    /// `min(chart-rows, max-rows)` when `max-rows` is set, else `chart-rows`, and the key that limits.
    pub fn effective_cap(&self, max_rows: Option<usize>) -> (usize, LimitingKey) {
        match max_rows {
            Some(m) if m <= self.rows.get() => (m, LimitingKey::MaxRows),
            _ => (self.rows.get(), LimitingKey::ChartRows),
        }
    }
}

/// Selects the rows every chart kind draws, with the default eligibility of its report kind.
/// See [`select_rows_where`].
pub fn select_rows<'a>(input: &SvgInput<'a>, opts: &SvgOptions) -> Selected<'a> {
    select_rows_where(input, opts, &|_| true)
}

/// The single row-selection function shared by the charts and the text block.
///
/// Population is the table's: two-way, the changed rows (regressed or improved with a finite
/// delta), or every compared row when none changed; N-way, every matrix row.
/// `keep` narrows it further for a chart kind (e.g. rows that have samples). With `max-rows`
/// set, only rows among the table's first `max-rows` are candidates. The cap is
/// `min(chart-rows, max-rows)`; rows are ordered by |delta| descending, ties in input order.
pub fn select_rows_where<'a>(
    input: &SvgInput<'a>,
    opts: &SvgOptions,
    keep: &dyn Fn(&RowRef<'a>) -> bool,
) -> Selected<'a> {
    let max_rows = match input {
        SvgInput::Pair(r) => r.summary.max_rows,
        SvgInput::Matrix(r) => r.summary.max_rows,
    };
    let (cap, limiting_key) = opts.effective_cap(max_rows);
    // (position in the report, row, |delta|) for every eligible row, in input order.
    let (pool, in_table): Pool<'a> = match input {
        SvgInput::Pair(report) => pair_pool(report, keep),
        SvgInput::Matrix(report) => matrix_pool(report, keep),
    };
    let population = pool.len();
    let mut candidates: Vec<_> = match max_rows {
        Some(m) => {
            let mut n = 0;
            pool.into_iter()
                .filter(|(i, _, _)| in_table(*i))
                .take_while(|_| {
                    n += 1;
                    n <= m
                })
                .collect()
        }
        None => pool,
    };
    // Stable: ties keep input order.
    candidates.sort_by(|a, b| b.2.total_cmp(&a.2));
    candidates.truncate(cap);
    let rows = candidates
        .into_iter()
        .enumerate()
        .map(|(k, (_, row, delta))| SelectedRow {
            index: k + 1,
            label: format!("{}. {}", k + 1, row.label()),
            delta,
            row,
        })
        .collect();
    Selected {
        rows,
        population,
        cap,
        limiting_key,
    }
}

type Pool<'a> = (Vec<(usize, RowRef<'a>, f64)>, Box<dyn Fn(usize) -> bool>);

fn pair_pool<'a>(report: &'a Report, keep: &dyn Fn(&RowRef<'a>) -> bool) -> Pool<'a> {
    let comparable: Vec<(usize, &Row, f64)> = report
        .rows
        .iter()
        .enumerate()
        .filter_map(|(i, row)| {
            let pct = row.delta_pct.filter(|p| p.is_finite())?;
            matches!(
                row.verdict,
                Verdict::Regressed | Verdict::Improved | Verdict::Unchanged
            )
            .then_some((i, row, pct.abs()))
        })
        .filter(|(_, row, _)| keep(&RowRef::Pair(row)))
        .collect();
    let changed = comparable
        .iter()
        .any(|(_, r, _)| r.verdict != Verdict::Unchanged);
    let pool = comparable
        .into_iter()
        .filter(|(_, r, _)| (r.verdict != Verdict::Unchanged) == changed)
        .map(|(i, r, d)| (i, RowRef::Pair(r), d))
        .collect();
    // The table lists the non-unchanged rows; with none changed it lists no rows, so any cap applies to the pool itself.
    let table: Vec<bool> = report
        .rows
        .iter()
        .map(|r| (r.verdict != Verdict::Unchanged) == changed)
        .collect();
    (pool, Box::new(move |i| table[i]))
}

fn matrix_pool<'a>(report: &'a MatrixReport, keep: &dyn Fn(&RowRef<'a>) -> bool) -> Pool<'a> {
    let pool = report
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| (i, RowRef::Matrix(row), matrix_delta(row)))
        .filter(|(_, row, _)| keep(row))
        .collect();
    (pool, Box::new(|_| true))
}

/// Largest |delta| against the reference over the row's compared cells; 0 when none.
fn matrix_delta(row: &MatrixRow) -> f64 {
    row.cells
        .iter()
        .filter_map(|c| match c {
            Cell::Present(p) => match &p.vs_reference {
                Comparison::Compared { delta: Some(d), .. } if d.is_finite() => Some(d.abs()),
                _ => None,
            },
            _ => None,
        })
        .fold(0.0, f64::max)
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

    use crate::compare::{Method, Summary};
    use crate::model::Better;

    fn prow(name: &str, pct: f64, verdict: Verdict) -> Row {
        Row {
            group: String::new(),
            name: name.into(),
            measure: String::new(),
            unit: "ns".into(),
            better: Better::Lower,
            base: Some(100.0),
            head: Some(100.0),
            delta_pct: Some(pct),
            verdict,
            method: Some(Method::Threshold),
            p_value: None,
            n_base: 1,
            n_head: 1,
            head_range: None,
            samples: Default::default(),
        }
    }

    fn preport(rows: Vec<Row>, max_rows: Option<usize>) -> Report {
        Report {
            summary: Summary {
                max_rows,
                ..Summary::default()
            },
            rows,
            has_base: true,
        }
    }

    /// `n` changed rows whose |delta| grows with the input index, so input order is the reverse of rank.
    fn changed(n: usize) -> Vec<Row> {
        (0..n)
            .map(|i| prow(&format!("b{i}"), 1.0 + i as f64, Verdict::Regressed))
            .collect()
    }

    fn opts_rows(n: usize) -> SvgOptions {
        SvgOptions {
            rows: ChartRows::new(n).unwrap(),
            ..Default::default()
        }
    }

    #[test]
    fn select_rows_should_cap_by_abs_delta_and_count_the_rest_when_47_rows_cap_30() {
        let r = preport(changed(47), None);
        let sel = select_rows(&SvgInput::Pair(&r), &opts_rows(30));
        assert_eq!((sel.rows.len(), sel.population, sel.more()), (30, 47, 17));
        assert_eq!(sel.rows[0].label, "1. b46");
        assert_eq!(sel.rows[29].label, "30. b17");
        assert_eq!(sel.footer().unwrap(), "+ 17 more not shown");
        assert_eq!(
            sel.notice().unwrap(),
            "+ 17 more not shown (chart-rows: 30)"
        );
        assert_eq!(
            sel.skip_reason(),
            Some(SkipReason::CapExceeded {
                shown: 30,
                total: 47
            })
        );
    }

    #[test]
    fn select_rows_should_break_delta_ties_by_input_order() {
        let rows = vec![
            prow("a", 5.0, Verdict::Regressed),
            prow("b", -5.0, Verdict::Improved),
            prow("c", 5.0, Verdict::Regressed),
        ];
        let r = preport(rows, None);
        let sel = select_rows(&SvgInput::Pair(&r), &opts_rows(2));
        let names: Vec<_> = sel.rows.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(names, ["1. a", "2. b"]);
    }

    #[test]
    fn select_rows_should_apply_min_of_chart_rows_and_max_rows_and_name_the_key() {
        let r = preport(changed(47), Some(20));
        let sel = select_rows(&SvgInput::Pair(&r), &opts_rows(30));
        assert_eq!((sel.rows.len(), sel.more()), (20, 27));
        assert_eq!(sel.limiting_key, LimitingKey::MaxRows);
        assert_eq!(sel.notice().unwrap(), "+ 27 more not shown (max-rows: 20)");

        let r = preport(changed(47), Some(40));
        let sel = select_rows(&SvgInput::Pair(&r), &opts_rows(30));
        assert_eq!((sel.rows.len(), sel.more()), (30, 17));
        assert_eq!(sel.limiting_key, LimitingKey::ChartRows);
    }

    #[test]
    fn select_rows_should_draw_only_rows_the_table_displays_when_max_rows_set() {
        // Table order is input order: the first 20 rows are b0..b19; the chart ranks within them.
        let r = preport(changed(47), Some(20));
        let sel = select_rows(&SvgInput::Pair(&r), &opts_rows(30));
        assert!(sel.rows.iter().all(|s| match s.row {
            RowRef::Pair(row) => row.name[1..].parse::<usize>().unwrap() < 20,
            RowRef::Matrix(_) => false,
        }));
        assert_eq!(sel.rows[0].label, "1. b19");
    }

    #[test]
    fn select_rows_should_use_changed_rows_or_all_compared_rows_when_none_changed() {
        let mixed = preport(
            vec![
                prow("quiet", 0.1, Verdict::Unchanged),
                prow("loud", 9.0, Verdict::Regressed),
            ],
            None,
        );
        let sel = select_rows(&SvgInput::Pair(&mixed), &opts_rows(30));
        assert_eq!((sel.rows.len(), sel.population), (1, 1));
        let quiet = preport(
            vec![
                prow("q1", 0.1, Verdict::Unchanged),
                prow("q2", -0.3, Verdict::Unchanged),
            ],
            None,
        );
        let sel = select_rows(&SvgInput::Pair(&quiet), &opts_rows(30));
        assert_eq!(sel.rows[0].label, "1. q2");
        assert_eq!(sel.population, 2);
    }

    #[test]
    fn select_rows_should_skip_rows_without_a_comparison() {
        let r = preport(
            vec![
                prow("new", 0.0, Verdict::New),
                Row {
                    delta_pct: None,
                    ..prow("nan", 0.0, Verdict::Regressed)
                },
                prow("ok", 3.0, Verdict::Improved),
            ],
            None,
        );
        let sel = select_rows(&SvgInput::Pair(&r), &opts_rows(30));
        assert_eq!(sel.rows.len(), 1);
        assert_eq!(sel.more(), 0);
        assert!(sel.footer().is_none() && sel.skip_reason().is_none());
    }

    #[test]
    fn select_rows_should_stay_capped_when_500_rows() {
        let r = preport(changed(500), None);
        let sel = select_rows(&SvgInput::Pair(&r), &SvgOptions::default());
        assert_eq!((sel.rows.len(), sel.more()), (30, 470));
    }
}
