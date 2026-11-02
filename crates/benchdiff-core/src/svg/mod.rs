pub mod delta;
pub mod distribution;
pub mod ranking;
pub mod style;
pub mod text;
pub mod writer;

use self::style::Theme;
use crate::compare::{Report, Row, Verdict};
use crate::markdown::{ChartEmbed, RenderExtras, SkippedChart};
use crate::matrix::{Cell, Comparison, MatrixReport, MatrixRow};
use crate::notice::{Notice, NoticeClass};
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
    CapExceeded {
        shown: usize,
        total: usize,
    },
    Empty,
    /// A coordinate was not finite, e.g. samples near the `f64` limits overflow the axis span.
    NonFinite,
    NotApplicable {
        kind: ChartKind,
    },
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

const TEXT_BAR_CELLS: usize = 5;
const TEXT_BAR_FULL_PCT: f64 = 50.0;
const TEXT_ORDER_NOTE: &str =
    "Rows are in chart order (largest change first); the table above follows the sort setting.";

/// The `Chart data (text)` content of one chart: the rows plus the lines around them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextTable {
    /// Headers after `#` and `Benchmark`, one per `TextRow::cells` entry.
    pub columns: Vec<String>,
    pub rows: Vec<TextRow>,
    /// Rows the ranking chart leaves out because they have no usable reference value.
    pub uncharted: usize,
    /// `+ N more not shown (key: cap)` when the chart is capped.
    pub notice: Option<String>,
}

/// The rows of the `Chart data (text)` table for `kind`: exactly the rows, order and `#` numbers
/// the chart draws (same `select_rows_where` call and predicate). Empty when `kind` does not
/// apply to `input` (delta and distribution need a pair report for the former; ranking a matrix).
pub fn chart_text_table(input: &SvgInput, kind: ChartKind, opts: &SvgOptions) -> Vec<TextRow> {
    text_table(input, kind, opts).map_or_else(Vec::new, |t| t.rows)
}

/// The table body as Markdown (order sentence, table, uncharted line, cap line), or `None` when
/// the chart has no rows. The `<details>` wrapper and title belong to the caller.
pub fn render_text_table(input: &SvgInput, kind: ChartKind, opts: &SvgOptions) -> Option<String> {
    let t = text_table(input, kind, opts)?;
    let bar = t.rows.iter().any(|r| r.bar.is_some());
    let mut head = vec!["#".to_string(), "Benchmark".to_string()];
    head.extend(t.columns.iter().map(|c| text_escape(c)));
    if bar {
        head.push("▰ bar".into());
    }
    let mut out = format!("{TEXT_ORDER_NOTE}\n\n| {} |\n", head.join(" | "));
    out.push_str(&format!("|{}\n", "---|".repeat(head.len())));
    for r in &t.rows {
        let mut cells = vec![r.index.to_string(), text_escape(&r.label)];
        cells.extend(r.cells.iter().cloned());
        cells.extend(r.bar.iter().cloned());
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    if t.uncharted > 0 {
        let (n, verb) = if t.uncharted == 1 {
            ("1 row has", "is")
        } else {
            (&*format!("{} rows have", t.uncharted), "are")
        };
        out.push_str(&format!(
            "\n{n} no reference value and {verb} not charted\n"
        ));
    }
    if let Some(n) = t.notice {
        out.push_str(&format!("\n{n}\n"));
    }
    Some(out)
}

fn text_table(input: &SvgInput, kind: ChartKind, opts: &SvgOptions) -> Option<TextTable> {
    let table = match (input, kind) {
        (SvgInput::Pair(r), ChartKind::Delta) => delta_table(r, opts),
        (SvgInput::Pair(r), ChartKind::Distribution) => distribution_pair_table(r, opts),
        (SvgInput::Matrix(r), ChartKind::Ranking) => ranking_table(r, opts)?,
        (SvgInput::Matrix(r), ChartKind::Distribution) => distribution_matrix_table(r, opts),
        _ => return None,
    };
    (!table.rows.is_empty()).then_some(table)
}

fn text_row(sel: &SelectedRow, cells: Vec<String>, bar: Option<String>) -> TextRow {
    TextRow {
        index: sel.index,
        label: sel.row.label(),
        cells,
        bar,
    }
}

fn delta_table(report: &Report, opts: &SvgOptions) -> TextTable {
    let selected = select_rows(&SvgInput::Pair(report), opts);
    let rows = selected
        .rows
        .iter()
        .filter_map(|sel| {
            let RowRef::Pair(r) = sel.row else {
                return None;
            };
            let (icon, pct) = match (r.verdict, r.delta_pct) {
                (Verdict::Regressed, Some(p)) => ("🔴", p),
                (Verdict::Improved, Some(p)) => ("🟢", p),
                (_, Some(p)) => ("⚪", p),
                _ => return None,
            };
            let filled = if r.verdict == Verdict::Unchanged {
                0
            } else {
                ((pct.abs() / TEXT_BAR_FULL_PCT) * TEXT_BAR_CELLS as f64)
                    .ceil()
                    .clamp(1.0, TEXT_BAR_CELLS as f64) as usize
            };
            let bar = format!(
                "{}{}",
                "▰".repeat(filled),
                "▱".repeat(TEXT_BAR_CELLS - filled)
            );
            let cell = format!("{icon} {}", crate::markdown::fmt_pct(pct));
            Some(text_row(sel, vec![cell], Some(bar)))
        })
        .collect();
    TextTable {
        columns: vec!["Change".into()],
        rows,
        uncharted: 0,
        notice: selected.notice(),
    }
}

fn ranking_table(report: &MatrixReport, opts: &SvgOptions) -> Option<TextTable> {
    let reference = report.summary.reference.as_ref()?.label.as_str();
    let ref_col = report
        .columns
        .iter()
        .position(|c| c.as_str() == reference)?;
    let others: Vec<usize> = (0..report.columns.len())
        .filter(|&c| c != ref_col)
        .collect();
    let series = &others[..others.len().min(opts.max_series.max(1))];
    // The same eligibility as the ranking chart: a finite, positive reference value.
    let has_ref = |row: &RowRef| {
        matches!(row, RowRef::Matrix(r)
            if matches!(r.cells.get(ref_col), Some(Cell::Present(p)) if p.value.is_finite() && p.value > 0.0))
    };
    let input = SvgInput::Matrix(report);
    let selected = select_rows_where(&input, opts, &has_ref);
    let uncharted = select_rows(&input, opts).population - selected.population;
    let cols: Vec<usize> = std::iter::once(ref_col)
        .chain(series.iter().copied())
        .collect();
    let rows = selected
        .rows
        .iter()
        .filter_map(|sel| {
            let RowRef::Matrix(row) = sel.row else {
                return None;
            };
            let Some(Cell::Present(rp)) = row.cells.get(ref_col) else {
                return None;
            };
            let cells = cols
                .iter()
                .map(|&c| ratio_cell(row.cells.get(c), rp.value))
                .collect();
            Some(text_row(sel, cells, None))
        })
        .collect();
    let mut columns = vec![format!("{reference} (ref)")];
    columns.extend(
        series
            .iter()
            .map(|&c| report.columns[c].as_str().to_string()),
    );
    Some(TextTable {
        columns,
        rows,
        uncharted,
        notice: selected.notice(),
    })
}

/// `0.74x 🟢`, `1.05x ~`, `1.00x` for the reference, `—` for Missing, `⚠️ invalid`.
fn ratio_cell(cell: Option<&Cell>, ref_value: f64) -> String {
    let Some(Cell::Present(p)) = cell else {
        return "—".into();
    };
    let ratio = p.value / ref_value;
    let text = if ratio < 10.0 {
        format!("{ratio:.2}x")
    } else if ratio < 100.0 {
        format!("{ratio:.1}x")
    } else {
        format!("{ratio:.0}x")
    };
    match &p.vs_reference {
        Comparison::Reference => "1.00x".into(),
        Comparison::Invalid(_) => "⚠️ invalid".into(),
        Comparison::Compared { verdict, .. } if ratio.is_finite() => match verdict {
            Verdict::Regressed => format!("{text} 🔴"),
            Verdict::Improved => format!("{text} 🟢"),
            _ => format!("{text} ~"),
        },
        _ => "not comparable".into(),
    }
}

fn finite_samples(values: &[f64]) -> Vec<f64> {
    values.iter().copied().filter(|v| v.is_finite()).collect()
}

/// `median unit (min-max, n=N)` over non-empty finite samples.
fn distribution_cell(values: &[f64], unit: &str) -> String {
    let n = values.len();
    let median = crate::stats::median(values);
    let lowest = values
        .iter()
        .copied()
        .min_by(f64::total_cmp)
        .unwrap_or(f64::NAN);
    let highest = values
        .iter()
        .copied()
        .max_by(f64::total_cmp)
        .unwrap_or(f64::NAN);
    let fmt = |x: f64| crate::markdown::fmt_num(x);
    let unit = if unit.is_empty() {
        String::new()
    } else {
        format!(" {}", text_escape(unit))
    };
    format!(
        "{}{unit} ({}-{}, n={n})",
        fmt(median),
        fmt(lowest),
        fmt(highest)
    )
}

/// The cap notice plus the collapsed-constant-rows line, as the chart footer shows them.
fn with_constants(
    notice: Option<String>,
    constants: &distribution::ConstantRows,
) -> Option<String> {
    let lines: Vec<String> = notice.into_iter().chain(constants.line()).collect();
    (!lines.is_empty()).then(|| lines.join("\n\n"))
}

fn distribution_pair_table(report: &Report, opts: &SvgOptions) -> TextTable {
    let (selected, constants) = distribution::pair_selection(report, opts);
    let rows = selected
        .rows
        .iter()
        .filter_map(|sel| {
            let RowRef::Pair(r) = sel.row else {
                return None;
            };
            let cells = vec![
                distribution_cell(&finite_samples(&r.samples.base), &r.unit),
                distribution_cell(&finite_samples(&r.samples.head), &r.unit),
            ];
            Some(text_row(sel, cells, None))
        })
        .collect();
    TextTable {
        columns: vec!["base".into(), "head".into()],
        rows,
        uncharted: 0,
        notice: with_constants(selected.notice(), &constants),
    }
}

fn distribution_matrix_table(report: &MatrixReport, opts: &SvgOptions) -> TextTable {
    // One lane per variant with samples, at most `max-series` per row, as the chart draws them.
    let lanes_of = |row: &MatrixRow| -> Vec<(usize, Vec<f64>)> {
        row.cells
            .iter()
            .enumerate()
            .filter_map(|(c, cell)| match cell {
                Cell::Present(p) => Some((c, finite_samples(&p.samples.head))),
                Cell::Missing => None,
            })
            .filter(|(_, v)| !v.is_empty())
            .take(opts.max_series.max(1))
            .collect()
    };
    let (selected, constants) = distribution::matrix_selection(report, opts);
    let lanes: Vec<_> = selected
        .rows
        .iter()
        .map(|sel| match sel.row {
            RowRef::Matrix(r) => lanes_of(r),
            RowRef::Pair(_) => vec![],
        })
        .collect();
    let mut cols: Vec<usize> = lanes.iter().flatten().map(|(c, _)| *c).collect();
    cols.sort_unstable();
    cols.dedup();
    let rows = selected
        .rows
        .iter()
        .zip(&lanes)
        .filter_map(|(sel, lane)| {
            let RowRef::Matrix(r) = sel.row else {
                return None;
            };
            let cells = cols
                .iter()
                .map(|c| match lane.iter().find(|(lc, _)| lc == c) {
                    Some((_, v)) => distribution_cell(v, &r.unit),
                    None => "—".into(),
                })
                .collect();
            Some(text_row(sel, cells, None))
        })
        .collect();
    TextTable {
        columns: cols
            .iter()
            .map(|&c| report.columns[c].as_str().to_string())
            .collect(),
        rows,
        uncharted: 0,
        notice: with_constants(selected.notice(), &constants),
    }
}

/// Untrusted text as one Markdown table-cell line; escapes what could become markup, including
/// GFM bare-URL autolinks (`https://`, `www.`) and `:emoji:` shortcodes.
fn text_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
    {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '|' | '@' | '#' | '!' | '~' | ':' => {
                out.push('\\');
                out.push(ch);
            }
            // `www.example.com` autolinks without a scheme.
            '.' if out.len() >= 3 && out[out.len() - 3..].eq_ignore_ascii_case("www") => {
                out.push_str("\\.");
            }
            _ => out.push(ch),
        }
    }
    out
}

/// What [`render_svgs`] produced: the files, the charts that made none, and the chart notices.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenderedSvgs {
    /// In requested order; a `Both` theme yields a `-light` then a `-dark` file per chart.
    pub files: Vec<SvgFile>,
    /// One entry per requested chart kind that produced no file.
    pub skipped: Vec<SkippedChart>,
    /// Class `Chart` notices for charts that were cut to the row cap.
    pub notices: Vec<Notice>,
}

impl RenderedSvgs {
    /// The `produced=N skipped=M` log line.
    pub fn summary(&self, themes: usize) -> String {
        format!(
            "produced={} skipped={}",
            self.files.len() / themes.max(1),
            self.skipped.len()
        )
    }
}

fn max_rows_of(input: &SvgInput) -> Option<usize> {
    match input {
        SvgInput::Pair(r) => r.summary.max_rows,
        SvgInput::Matrix(r) => r.summary.max_rows,
    }
}

/// Dispatches every requested chart kind over the report and returns the files.
///
/// A kind that does not fit the report kind is skipped with `NotApplicable`, a run with nothing
/// to compare with `NothingToCompare`; neither is an error and the other kinds still render.
pub fn render_svgs(input: SvgInput, opts: &SvgOptions) -> RenderedSvgs {
    let nothing_to_compare = match input {
        SvgInput::Pair(r) => !r.has_base,
        SvgInput::Matrix(r) => r.columns.len() < 2,
    };
    let themes: &[Theme] = match opts.theme {
        ChartTheme::Light => &[Theme::Light],
        ChartTheme::Dark => &[Theme::Dark],
        ChartTheme::Both => &[Theme::Light, Theme::Dark],
    };
    let mut out = RenderedSvgs::default();
    for &kind in &opts.kinds {
        if nothing_to_compare {
            out.skipped.push(SkippedChart {
                kind,
                reason: SkipReason::NothingToCompare,
            });
            continue;
        }
        let drawn: Result<Vec<SvgFile>, SkipReason> = themes
            .iter()
            .map(|&theme| draw(kind, &input, opts, theme))
            .collect();
        match drawn {
            Ok(files) => {
                out.notices.extend(
                    files
                        .first()
                        .and_then(|f| cap_notice(f, kind, &input, opts)),
                );
                out.files.extend(files);
            }
            Err(reason) => out.skipped.push(SkippedChart { kind, reason }),
        }
    }
    out
}

fn draw(
    kind: ChartKind,
    input: &SvgInput,
    opts: &SvgOptions,
    theme: Theme,
) -> Result<SvgFile, SkipReason> {
    match (kind, *input) {
        (ChartKind::Delta, SvgInput::Pair(r)) => delta::delta_chart_themed(r, opts, theme),
        (ChartKind::Ranking, SvgInput::Matrix(r)) => ranking::ranking_chart_themed(r, opts, theme),
        (ChartKind::Distribution, SvgInput::Pair(r)) => {
            distribution::distribution_chart_themed(r, opts, theme)
        }
        (ChartKind::Distribution, SvgInput::Matrix(r)) => {
            distribution::distribution_matrix_chart_themed(r, opts, theme)
        }
        (ChartKind::Delta | ChartKind::Ranking, _) => Err(SkipReason::NotApplicable { kind }),
    }
}

fn cap_notice(
    file: &SvgFile,
    kind: ChartKind,
    input: &SvgInput,
    opts: &SvgOptions,
) -> Option<Notice> {
    let Some(SkipReason::CapExceeded { shown, total }) = file.skipped else {
        return None;
    };
    let (cap, key) = opts.effective_cap(max_rows_of(input));
    Some(Notice {
        class: NoticeClass::Chart,
        text: format!(
            "{kind} chart: + {} more not shown ({}: {cap})",
            total - shown,
            key.name()
        ),
    })
}

/// The `RenderExtras` for a rendered run: placeholders and text bodies only when embedding.
/// With a `Both` theme `file` is the light twin and `dark_file` the dark one.
pub fn render_extras(
    input: &SvgInput,
    opts: &SvgOptions,
    rendered: &RenderedSvgs,
    svg_dir: Option<&str>,
) -> RenderExtras {
    let embed = opts.embed == EmbedMode::On;
    let mut charts = vec![];
    if embed {
        for kind in &opts.kinds {
            let Some(file) = rendered
                .files
                .iter()
                .find(|f| f.name.starts_with(kind.name()))
            else {
                continue;
            };
            let dark_file = rendered
                .files
                .iter()
                .find(|f| f.name == format!("{kind}-dark"))
                .map(|f| format!("{}.svg", f.name));
            charts.push(ChartEmbed {
                kind: *kind,
                file: format!("{}.svg", file.name),
                dark_file,
                alt: file.alt.clone(),
                text: render_text_table(input, *kind, opts),
            });
        }
    }
    RenderExtras {
        charts,
        skipped: rendered.skipped.clone(),
        notices: rendered.notices.clone(),
        embed,
        publish_images: opts.publish_images,
        svg_dir: svg_dir.map(str::to_string),
    }
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

    use crate::compare::{Method as M2, Samples};
    use crate::matrix::{
        CellFormat, Correction, MatrixSort, MatrixSummary, PresentCell, ReferenceInfo, VariantLabel,
    };

    fn mcell(value: f64, cmp: Comparison, samples: Vec<f64>) -> Cell {
        Cell::Present(PresentCell {
            value,
            samples: Samples {
                base: vec![],
                head: samples,
            },
            vs_reference: cmp,
        })
    }

    fn cmp(verdict: Verdict, delta: f64) -> Comparison {
        Comparison::Compared {
            verdict,
            delta: Some(delta),
            method: Some(M2::Threshold),
            p_value: None,
            p_adjusted: None,
        }
    }

    fn mrow(name: &str, cells: Vec<Cell>) -> MatrixRow {
        MatrixRow {
            group: String::new(),
            name: name.into(),
            measure: String::new(),
            unit: "ns".into(),
            better: Better::Lower,
            dims: Default::default(),
            cells,
            ranking: vec![],
        }
    }

    fn mreport(rows: Vec<MatrixRow>) -> MatrixReport {
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
                threshold_overrides: Default::default(),
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

    fn ranking_fixture() -> MatrixReport {
        let r = |v: f64| mcell(v, Comparison::Reference, vec![]);
        mreport(vec![
            mrow(
                "Sort",
                vec![
                    r(100.0),
                    mcell(130.0, cmp(Verdict::Regressed, 30.0), vec![]),
                    mcell(74.0, cmp(Verdict::Improved, -26.0), vec![]),
                ],
            ),
            mrow(
                "Parse",
                vec![
                    r(100.0),
                    Cell::Missing,
                    mcell(
                        f64::NAN,
                        Comparison::Invalid(crate::matrix::Reason("bad".into())),
                        vec![],
                    ),
                ],
            ),
            mrow(
                "Hash",
                vec![
                    r(100.0),
                    mcell(105.0, cmp(Verdict::Unchanged, 5.0), vec![]),
                    mcell(100.0, cmp(Verdict::Unchanged, 0.0), vec![]),
                ],
            ),
            // The reference lacks this benchmark.
            mrow(
                "Orphan",
                vec![
                    Cell::Missing,
                    mcell(5.0, Comparison::NoReference, vec![]),
                    Cell::Missing,
                ],
            ),
        ])
    }

    #[test]
    fn chart_text_table_should_emit_ratio_columns_dash_for_missing_and_cap_line_when_ranking_and_median_range_when_distribution(
    ) {
        let rep = ranking_fixture();
        let input = SvgInput::Matrix(&rep);
        let md = render_text_table(&input, ChartKind::Ranking, &opts_rows(30)).unwrap();
        assert!(md.starts_with(TEXT_ORDER_NOTE), "{md}");
        assert!(
            md.contains("| # | Benchmark | quick (ref) | heap | merge |"),
            "{md}"
        );
        assert!(
            md.contains("| 1 | Sort | 1.00x | 1.30x 🔴 | 0.74x 🟢 |"),
            "{md}"
        );
        assert!(
            md.contains("| 3 | Parse | 1.00x | — | ⚠️ invalid |"),
            "{md}"
        );
        assert!(
            md.contains("| 2 | Hash | 1.00x | 1.05x ~ | 1.00x ~ |"),
            "{md}"
        );
        assert!(
            md.contains("1 row has no reference value and is not charted"),
            "{md}"
        );
        assert!(!md.contains("Orphan"), "{md}");

        // Capped: the cap line names the key and equals the chart's count.
        let capped = render_text_table(&input, ChartKind::Ranking, &opts_rows(1)).unwrap();
        assert!(
            capped.contains("+ 2 more not shown (chart-rows: 1)"),
            "{capped}"
        );

        // Distribution: median (min-max, n) per variant, rows with samples only.
        let rep = mreport(vec![
            mrow(
                "Sort",
                vec![
                    mcell(1.0, Comparison::Reference, vec![1.0, 2.0, 3.0]),
                    mcell(2.0, cmp(Verdict::Regressed, 100.0), vec![]),
                    mcell(2.0, cmp(Verdict::Regressed, 100.0), vec![4.0, 6.0]),
                ],
            ),
            mrow("Bare", vec![Cell::Missing, Cell::Missing, Cell::Missing]),
        ]);
        let md = render_text_table(
            &SvgInput::Matrix(&rep),
            ChartKind::Distribution,
            &opts_rows(30),
        )
        .unwrap();
        assert!(md.contains("| # | Benchmark | quick | merge |"), "{md}");
        assert!(
            md.contains("| 1 | Sort | 2 ns (1-3, n=3) | 5 ns (4-6, n=2) |"),
            "{md}"
        );
        assert!(!md.contains("Bare"), "{md}");
    }

    #[test]
    fn chart_text_table_should_match_chart_rows_index_and_bar_when_delta() {
        let r = preport(changed(40), None);
        let input = SvgInput::Pair(&r);
        let rows = chart_text_table(&input, ChartKind::Delta, &opts_rows(30));
        let sel = select_rows(&input, &opts_rows(30));
        assert_eq!(rows.len(), sel.rows.len());
        for (t, s) in rows.iter().zip(&sel.rows) {
            assert_eq!(
                (t.index, format!("{}. {}", t.index, t.label)),
                (s.index, s.label.clone())
            );
        }
        assert_eq!(rows[0].cells, ["🔴 +40.0%"]);
        assert_eq!(rows[0].bar.as_deref(), Some("▰▰▰▰▱"));
        let md = render_text_table(&input, ChartKind::Delta, &opts_rows(30)).unwrap();
        assert!(md.contains("| # | Benchmark | Change | ▰ bar |"), "{md}");
        assert!(
            md.ends_with("+ 10 more not shown (chart-rows: 30)\n"),
            "{md}"
        );
    }

    #[test]
    fn chart_text_table_should_be_empty_when_kind_does_not_fit_input_or_no_rows() {
        let r = preport(changed(3), None);
        assert!(
            chart_text_table(&SvgInput::Pair(&r), ChartKind::Ranking, &opts_rows(30)).is_empty()
        );
        assert!(
            render_text_table(&SvgInput::Pair(&r), ChartKind::Distribution, &opts_rows(30))
                .is_none()
        );
    }

    #[test]
    fn render_text_table_should_escape_hostile_names() {
        let r = preport(vec![prow("a|b<img>`x`", 9.0, Verdict::Regressed)], None);
        let md = render_text_table(&SvgInput::Pair(&r), ChartKind::Delta, &opts_rows(30)).unwrap();
        assert!(md.contains(r"| 1 | a\|b&lt;img&gt;\`x\` |"), "{md}");
    }

    #[test]
    fn text_escape_should_break_bare_url_autolinks_and_emoji_shortcodes() {
        for hostile in [
            "https://evil.example/x",
            "HTTP://evil.example",
            "see www.evil.example",
            "WWW.evil.example",
            ":smile: :+1:",
            "mailto:a@evil.example",
        ] {
            let out = text_escape(hostile);
            assert!(!out.replace("\\:", "").contains(':'), "{out}");
            assert!(!out.to_lowercase().contains("www."), "{out}");
        }
        assert_eq!(text_escape("www.a.com"), "www\\.a.com");
        assert_eq!(text_escape("a: b"), "a\\: b");
    }

    fn kinds(list: &[ChartKind]) -> SvgOptions {
        SvgOptions {
            kinds: list.to_vec(),
            ..SvgOptions::default()
        }
    }

    fn pair_report(with_base: bool) -> Report {
        use crate::import::{parse, Format};
        let head = parse(
            Format::Go,
            "BenchmarkA-8 100 130 ns/op\nBenchmarkB-8 100 70 ns/op\n",
        )
        .unwrap();
        let base = parse(
            Format::Go,
            "BenchmarkA-8 100 100 ns/op\nBenchmarkB-8 100 100 ns/op\n",
        )
        .unwrap();
        crate::compare(
            with_base.then_some(&base),
            &head,
            &crate::Options::default(),
        )
    }

    #[test]
    fn render_svgs_should_skip_every_kind_as_nothing_to_compare_when_head_only() {
        let rep = pair_report(false);
        let r = render_svgs(
            SvgInput::Pair(&rep),
            &kinds(&[ChartKind::Delta, ChartKind::Ranking]),
        );
        assert!(r.files.is_empty());
        assert_eq!(r.skipped.len(), 2);
        assert!(r
            .skipped
            .iter()
            .all(|s| s.reason == SkipReason::NothingToCompare));
    }

    #[test]
    fn render_svgs_should_render_delta_and_skip_ranking_as_not_applicable_when_two_way() {
        let rep = pair_report(true);
        let r = render_svgs(
            SvgInput::Pair(&rep),
            &kinds(&[ChartKind::Delta, ChartKind::Ranking]),
        );
        assert_eq!(
            r.files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["delta"]
        );
        assert_eq!(
            r.skipped,
            [SkippedChart {
                kind: ChartKind::Ranking,
                reason: SkipReason::NotApplicable {
                    kind: ChartKind::Ranking
                }
            }]
        );
        assert_eq!(r.summary(1), "produced=1 skipped=1");
    }

    #[test]
    fn render_svgs_should_render_ranking_and_skip_delta_as_not_applicable_when_n_way() {
        let rep = ranking_fixture();
        let r = render_svgs(
            SvgInput::Matrix(&rep),
            &kinds(&[ChartKind::Delta, ChartKind::Ranking]),
        );
        assert_eq!(
            r.files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["ranking"]
        );
        assert_eq!(
            r.skipped[0].reason,
            SkipReason::NotApplicable {
                kind: ChartKind::Delta
            }
        );
    }

    #[test]
    fn render_svgs_should_skip_distribution_as_no_samples_when_report_kept_none() {
        let rep = pair_report(true);
        let r = render_svgs(SvgInput::Pair(&rep), &kinds(&[ChartKind::Distribution]));
        assert!(r.files.is_empty());
        assert_eq!(r.skipped[0].reason, SkipReason::NoSamples);
    }

    #[test]
    fn render_svgs_should_emit_light_and_dark_twins_when_theme_is_both() {
        let rep = pair_report(true);
        let mut o = kinds(&[ChartKind::Delta]);
        o.theme = ChartTheme::Both;
        o.embed = EmbedMode::On;
        let r = render_svgs(SvgInput::Pair(&rep), &o);
        let names: Vec<_> = r.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["delta-light", "delta-dark"]);
        assert_ne!(r.files[0].svg, r.files[1].svg);
        let extras = render_extras(&SvgInput::Pair(&rep), &o, &r, Some("out"));
        assert_eq!(extras.charts.len(), 1);
        assert_eq!(extras.charts[0].file, "delta-light.svg");
        assert_eq!(
            extras.charts[0].dark_file.as_deref(),
            Some("delta-dark.svg")
        );
        assert_eq!(r.summary(2), "produced=1 skipped=0");
    }

    #[test]
    fn render_extras_should_carry_no_charts_when_embed_is_off() {
        let rep = pair_report(true);
        let o = kinds(&[ChartKind::Delta]);
        let r = render_svgs(SvgInput::Pair(&rep), &o);
        let extras = render_extras(&SvgInput::Pair(&rep), &o, &r, None);
        assert!(extras.charts.is_empty() && !extras.embed);
    }

    #[test]
    fn render_svgs_should_notice_the_cap_and_name_the_limiting_key_when_rows_are_cut() {
        let rep = pair_report(true);
        let mut o = kinds(&[ChartKind::Delta]);
        o.rows = ChartRows::new(1).unwrap();
        let r = render_svgs(SvgInput::Pair(&rep), &o);
        assert_eq!(r.notices.len(), 1);
        assert_eq!(r.notices[0].class, NoticeClass::Chart);
        assert_eq!(
            r.notices[0].text,
            "delta chart: + 1 more not shown (chart-rows: 1)"
        );
    }
}
