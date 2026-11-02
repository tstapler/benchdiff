//! N-way projection: split a [`Dimensioned`] set into [`Variant`]s along an axis.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use serde::Serialize;

use crate::compare::{compare_pair, ConfigSource, Method, Options, Row, Samples, Verdict};
use crate::dimensioned::Dimensioned;
use crate::dimensions::{axis_value, row_dims, GROUP_AXIS, MEASURE_AXIS};
use crate::model::{Better, Dims, Metric, MetricSet, SkippedKey};
use crate::notice::{Notice, NoticeClass};
use crate::stats;
use crate::units;

/// Most benchmark names listed in a "no such dimension" notice.
const MAX_LISTED: usize = 10;

/// Validated variant label: `^[A-Za-z0-9._-]{1,40}$`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct VariantLabel(String);

impl VariantLabel {
    pub fn parse(s: &str) -> Result<Self, String> {
        let ok = (1..=40).contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if ok {
            Ok(Self(s.to_string()))
        } else {
            Err(format!(
                "'{s}' is not a valid variant label (1 to 40 characters from A-Z a-z 0-9 . _ -)"
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VariantLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One column of an N-way comparison. Metrics are clones with `name` = residual and only
/// the row-identity dims; `set.skipped` holds this variant's skipped keys, named by residual.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub label: VariantLabel,
    pub set: MetricSet,
}

/// Variants plus the data-dependent problems found while projecting (class `Degradation`).
/// No variants and a notice means the axis degraded; the caller decides strict vs notice.
#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    pub variants: Vec<Variant>,
    pub notices: Vec<Notice>,
}

/// A static or unrecoverable projection error (exit 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixError(pub String);

impl fmt::Display for MatrixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MatrixError {}

/// `residual` plus ` [k=v, ...]` for the row-identity dims in key order, e.g. `Sort [size=10]`.
pub fn row_label(metric: &Metric) -> String {
    if metric.dims.is_empty() {
        return metric.name.clone();
    }
    let dims: Vec<String> = metric
        .dims
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    format!("{} [{}]", metric.name, dims.join(", "))
}

/// Wraps a file as a variant under an explicit label. Never merged with other files; the axis
/// must not also come from a name rule.
pub fn label_variant(
    label: VariantLabel,
    d: &Dimensioned,
    axis: &str,
) -> Result<Variant, MatrixError> {
    if axis != GROUP_AXIS && d.entries().any(|(m, _)| m.dims.contains_key(axis)) {
        return Err(MatrixError(format!(
            "axis '{axis}' set by both file label and name rule"
        )));
    }
    let mut set = MetricSet::default();
    for (m, r) in d.entries() {
        let mut clone = m.clone();
        clone.name = r.as_str().to_string();
        set.metrics.push(clone);
    }
    for e in d.skipped_entries() {
        set.skipped.push_key(SkippedKey {
            group: e.group.into(),
            name: e.residual.into(),
            measure: e.measure.into(),
            dims: e.dims.clone(),
            residual: None,
        });
    }
    Ok(Variant { label, set })
}

#[derive(Default)]
struct Column {
    metrics: Vec<Metric>,
    skipped: Vec<SkippedKey>,
    /// Row identity -> (original name, all non-axis dims) of the first metric, for collisions.
    rows: HashMap<(String, String, String, Dims), (String, Dims)>,
}

/// Splits `d` into one variant per value of `axis` (a dimension name or `group`), in first-seen
/// order (metrics, then skipped-only values). `group_by` limits which non-axis dims take part in
/// row identity (default all). Pure; the source names are never rewritten.
pub fn project_by_axis(
    d: &Dimensioned,
    axis: &str,
    group_by: Option<&[String]>,
) -> Result<Projection, MatrixError> {
    let suffix = d.available_dimensions_text();
    if axis == MEASURE_AXIS {
        return Err(MatrixError(format!(
            "compare-by 'measure' is not an axis (units differ per measure); {suffix}"
        )));
    }
    let carries =
        |name: &str| name == GROUP_AXIS || d.entries().any(|(m, _)| m.dims.contains_key(name));
    let degraded = |text: String| Projection {
        variants: Vec::new(),
        notices: vec![Notice {
            class: NoticeClass::Degradation,
            text,
        }],
    };
    if !carries(axis) || (axis == GROUP_AXIS && d.entries().all(|(m, _)| m.group.is_empty())) {
        return Ok(degraded(format!(
            "compare-by '{axis}': no benchmark carries this dimension; {suffix}"
        )));
    }
    if let Some(unknown) = group_by.and_then(|g| g.iter().find(|x| !carries(x))) {
        return Ok(degraded(format!(
            "group-by '{unknown}': no benchmark carries this dimension; {suffix}"
        )));
    }

    let mut order: Vec<String> = Vec::new();
    let mut columns: HashMap<String, Column> = HashMap::new();
    let mut lacking: Vec<&str> = Vec::new();
    let mut lacking_total = 0usize;
    for (metric, residual) in d.entries() {
        let Some(value) = axis_value(&metric.group, &metric.dims, axis) else {
            lacking_total += 1;
            if lacking.len() < MAX_LISTED {
                lacking.push(&metric.name);
            }
            continue;
        };
        let col = column(&mut order, &mut columns, value);
        let dims = row_dims(&metric.dims, axis, group_by);
        let identity = (
            metric.group.clone(),
            residual.as_str().to_string(),
            metric.measure.clone(),
            dims.clone(),
        );
        let full = row_dims(&metric.dims, axis, None);
        if let Some((first, first_dims)) = col.rows.get(&identity) {
            return Err(collision(first, first_dims, &metric.name, &full));
        }
        col.rows.insert(identity, (metric.name.clone(), full));
        let mut clone = metric.clone();
        clone.name = residual.as_str().to_string();
        clone.dims = dims;
        col.metrics.push(clone);
    }
    for e in d.skipped_entries() {
        let Some(value) = axis_value(e.group, e.dims, axis) else {
            continue;
        };
        column(&mut order, &mut columns, value)
            .skipped
            .push(SkippedKey {
                group: e.group.into(),
                name: e.residual.into(),
                measure: e.measure.into(),
                dims: row_dims(e.dims, axis, group_by),
                residual: None,
            });
    }

    let (labels, renamed) = derive_labels(&order);
    let mut variants = Vec::with_capacity(order.len());
    for (value, label) in order.into_iter().zip(labels) {
        let col = columns.remove(&value).unwrap_or_default();
        let mut set = MetricSet {
            metrics: col.metrics,
            ..Default::default()
        };
        for key in col.skipped {
            set.skipped.push_key(key);
        }
        variants.push(Variant { label, set });
    }

    let mut notices = Vec::new();
    if lacking_total > 0 {
        let (noun, verb) = if lacking_total == 1 {
            ("benchmark", "has")
        } else {
            ("benchmarks", "have")
        };
        notices.push(Notice {
            class: NoticeClass::Degradation,
            text: format!(
                "{lacking_total} {noun} {verb} no '{axis}' dimension (first {MAX_LISTED}: {})",
                lacking.join(", ")
            ),
        });
    }
    if !renamed.is_empty() {
        let mut items: Vec<String> = renamed
            .iter()
            .take(MAX_RENAMED_LISTED)
            .map(|(from, to)| format!("axis value '{from}' shown as '{to}'"))
            .collect();
        if renamed.len() > MAX_RENAMED_LISTED {
            items.push(format!("and {} more", renamed.len() - MAX_RENAMED_LISTED));
        }
        notices.push(Notice {
            class: NoticeClass::Degradation,
            text: items.join("; "),
        });
    }
    Ok(Projection { variants, notices })
}

const MAX_RENAMED_LISTED: usize = 5;
const MAX_LABEL_LEN: usize = 40;

/// A label for every projected axis value: valid values as they are, others with each disallowed
/// character replaced by `_` and cut to 40, made unique with `-2`, `-3`. Also returns the
/// `(value, label)` pairs that changed. Data-dependent, so never an error (explicit `--variant`
/// labels still go through the strict `VariantLabel::parse`).
fn derive_labels(values: &[String]) -> (Vec<VariantLabel>, Vec<(String, String)>) {
    let mut used: HashSet<String> = values
        .iter()
        .filter(|v| VariantLabel::parse(v).is_ok())
        .cloned()
        .collect();
    let mut renamed = Vec::new();
    let labels = values
        .iter()
        .map(|value| {
            if let Ok(label) = VariantLabel::parse(value) {
                return label;
            }
            let mut base: String = value
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                        c
                    } else {
                        '_'
                    }
                })
                .take(MAX_LABEL_LEN)
                .collect();
            if base.is_empty() {
                base.push('_');
            }
            let mut candidate = base.clone();
            let mut n = 2;
            while used.contains(&candidate) {
                let suffix = format!("-{n}");
                candidate = format!(
                    "{}{suffix}",
                    &base[..base.len().min(MAX_LABEL_LEN - suffix.len())]
                );
                n += 1;
            }
            used.insert(candidate.clone());
            renamed.push((value.clone(), candidate.clone()));
            VariantLabel(candidate)
        })
        .collect();
    (labels, renamed)
}

fn column<'a>(
    order: &mut Vec<String>,
    columns: &'a mut HashMap<String, Column>,
    value: &str,
) -> &'a mut Column {
    if !columns.contains_key(value) {
        order.push(value.to_string());
    }
    columns.entry(value.to_string()).or_default()
}

fn collision(first: &str, first_dims: &Dims, second: &str, second_dims: &Dims) -> MatrixError {
    let dropped = first_dims
        .keys()
        .chain(second_dims.keys())
        .find(|k| first_dims.get(*k) != second_dims.get(*k));
    let which = dropped.map_or(String::new(), |k| format!(" non-axis dimension '{k}'"));
    MatrixError(format!(
        "rows collide after dropping{which}; add it to group-by \
         (benchmarks '{first}' and '{second}')"
    ))
}

// ---- N-way comparison ----

/// Which variant every other column is compared against (ADR-005).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Reference {
    /// The first variant in input order; defaulted, so it cannot back the regression gate.
    #[default]
    First,
    /// A declared baseline.
    Label(VariantLabel),
}

/// Table cell shape (`cell-format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CellFormat {
    /// `1210 ns (0.74x) 🟢`
    #[default]
    Full,
    /// `0.74x 🟢`
    Ratio,
}

/// Multiple-comparison correction across one row's cells (ADR-002; applied by `compare_matrix`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Correction {
    #[default]
    Holm,
    None,
}

/// Row order of the matrix table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MatrixSort {
    Input,
    Name,
    /// Worst against the reference first.
    #[default]
    Delta,
    /// Rows where the reference ranks worst first.
    Rank,
}

/// Matrix-specific settings; the shared ones stay in [`Options`].
#[derive(Debug, Clone)]
pub struct MatrixOptions {
    pub compare_by: String,
    pub group_by: Option<Vec<String>>,
    /// `<dim>=<pattern>` rules as written by the user.
    pub dimensions: Vec<String>,
    /// Turn data-dependent problems from notices into errors.
    pub strict: bool,
    pub reference: Reference,
    pub correction: Correction,
    pub columns_cap: usize,
    pub cell_format: CellFormat,
    pub sort: MatrixSort,
}

impl Default for MatrixOptions {
    fn default() -> Self {
        MatrixOptions {
            compare_by: String::new(),
            group_by: None,
            dimensions: Vec::new(),
            strict: false,
            reference: Reference::First,
            correction: Correction::Holm,
            columns_cap: 8,
            cell_format: CellFormat::Full,
            sort: MatrixSort::Delta,
        }
    }
}

/// Holm-adjusted p-value, always within `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(transparent)]
pub struct PAdjusted(f64);

impl PAdjusted {
    /// Clamps into `[0, 1]`; NaN becomes 1 (not significant).
    pub fn new(p: f64) -> Self {
        Self(if p.is_nan() { 1.0 } else { p.clamp(0.0, 1.0) })
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

/// Why a cell is not a comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Reason(pub String);

/// A present cell's relation to the reference column. `Regressed`/`Improved` here mean
/// significantly worse/better than the reference, never a change over time.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum Comparison {
    /// This is the reference column.
    Reference,
    Compared {
        verdict: Verdict,
        /// Percent against the reference; `None` when the reference value is zero.
        delta: Option<f64>,
        method: Option<Method>,
        p_value: Option<f64>,
        /// Set by the Holm post-pass for cells with a p-value; `None` otherwise or under
        /// `Correction::None`.
        p_adjusted: Option<PAdjusted>,
    },
    Incomparable(Reason),
    /// `better` differs from the reference's; shown raw, never reaches the kernel.
    DirectionMismatch,
    /// The reference has the metric but this variant's series was skipped as unusable.
    /// The cell's `value` is NaN.
    Invalid(Reason),
    /// The reference lacks the metric, so there is nothing to compare against.
    NoReference,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresentCell {
    pub value: f64,
    #[serde(skip)]
    pub samples: Samples,
    pub vs_reference: Comparison,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "cell", rename_all = "snake_case")]
pub enum Cell {
    /// Never rendered or counted as zero.
    Missing,
    Present(PresentCell),
}

impl Cell {
    /// Present with a value the ratio maths can use (not `Invalid`).
    pub fn is_usable(&self) -> bool {
        matches!(self, Cell::Present(p) if !matches!(p.vs_reference, Comparison::Invalid(_)))
    }
}

/// One benchmark across all variants.
#[derive(Debug, Clone, Serialize)]
pub struct MatrixRow {
    pub group: String,
    /// The residual name.
    pub name: String,
    pub measure: String,
    /// The reference's unit (the first present cell's when the reference is absent).
    pub unit: String,
    /// The reference's direction (the first present cell's when the reference is absent).
    pub better: Better,
    pub dims: Dims,
    /// One per column, in column order.
    pub cells: Vec<Cell>,
    /// Column indices best first, oriented by `better`; empty when the reference is absent.
    pub ranking: Vec<usize>,
}

impl MatrixRow {
    /// `name` plus ` [k=v, ...]` for the row-identity dims.
    pub fn label(&self) -> String {
        if self.dims.is_empty() {
            return self.name.clone();
        }
        let dims: Vec<String> = self.dims.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!("{} [{}]", self.name, dims.join(", "))
    }

    /// The Holm family size `m`: cells whose Mann-Whitney test ran (they carry a p-value),
    /// whether or not the threshold floor then suppressed them. Rendered as `(m=..)`.
    pub fn holm_m(&self) -> usize {
        self.cells.iter().filter(|c| tested_p(c).is_some()).count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReferenceInfo {
    pub label: String,
    pub explicit: bool,
}

/// What one variant skipped on import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VariantSkipped {
    pub label: String,
    pub skipped: Vec<String>,
    pub total: usize,
}

/// The matrix analogue of `Summary`: copied from `Options` and `MatrixOptions`, so renderers
/// need only the report.
#[derive(Debug, Clone, Serialize)]
pub struct MatrixSummary {
    pub title: String,
    pub tag: String,
    pub threshold_pct: f64,
    pub min_abs: f64,
    pub alpha: f64,
    pub use_stats: bool,
    pub only_changed: bool,
    pub max_rows: Option<usize>,
    pub fail_on_regression_pct: Option<f64>,
    pub correction: Correction,
    pub columns_cap: usize,
    pub cell_format: CellFormat,
    pub sort: MatrixSort,
    /// `None` when no reference could be resolved (no variants, or an unknown label).
    pub reference: Option<ReferenceInfo>,
    pub skipped: Vec<VariantSkipped>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub threshold_overrides: BTreeMap<String, f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub config_sources: Vec<ConfigSource>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatrixReport {
    pub summary: MatrixSummary,
    /// Variant labels in input order.
    pub columns: Vec<VariantLabel>,
    pub rows: Vec<MatrixRow>,
    /// Labels of rows the reference lacks (or skipped); these rows carry no deltas.
    pub reference_absent: Vec<String>,
    /// Geomean families in first-seen row order, each with at least 3 rows. Computed after the
    /// Holm pass; empty when the reference is unresolved.
    pub geomean: Vec<GeomeanFamily>,
    /// Notices raised here (class `Degradation`); rendering and ordering belong to the renderer.
    #[serde(skip)]
    pub notices: Vec<Notice>,
}

impl MatrixReport {
    fn comparisons(&self) -> impl Iterator<Item = (usize, &Comparison)> {
        self.rows.iter().flat_map(|r| {
            r.cells
                .iter()
                .enumerate()
                .filter_map(|(c, cell)| match cell {
                    Cell::Present(p) => Some((c, &p.vs_reference)),
                    Cell::Missing => None,
                })
        })
    }

    /// Cells with this verdict against the reference, over all non-reference columns.
    pub fn count(&self, v: Verdict) -> usize {
        self.comparisons()
            .filter(|(_, cmp)| comparison_verdict(cmp) == Some(v))
            .count()
    }

    /// Cells of column `col` that are `Invalid`.
    pub fn invalid_in(&self, col: usize) -> usize {
        self.comparisons()
            .filter(|(c, cmp)| *c == col && matches!(cmp, Comparison::Invalid(_)))
            .count()
    }
}

fn tested_p(cell: &Cell) -> Option<f64> {
    match cell {
        Cell::Present(PresentCell {
            vs_reference: Comparison::Compared {
                p_value: Some(p), ..
            },
            ..
        }) => Some(*p),
        _ => None,
    }
}

/// Per-row Holm pass (ADR-002). Downgrade-only: a significant cell whose adjusted p reaches
/// `alpha` becomes `Unchanged`; the raw p-value stays and `p_adjusted` is filled.
fn apply_holm(row: &mut MatrixRow, alpha: f64) {
    let raw: Vec<f64> = row.cells.iter().filter_map(tested_p).collect();
    let mut adjusted = stats::holm_adjust(&raw).into_iter();
    for cell in &mut row.cells {
        if tested_p(cell).is_none() {
            continue;
        }
        let adj = adjusted.next().expect("one adjusted p per tested cell");
        if let Cell::Present(PresentCell {
            vs_reference:
                Comparison::Compared {
                    verdict,
                    p_adjusted,
                    ..
                },
            ..
        }) = cell
        {
            *p_adjusted = Some(PAdjusted::new(adj));
            if matches!(verdict, Verdict::Regressed | Verdict::Improved) && adj >= alpha {
                *verdict = Verdict::Unchanged;
            }
        }
    }
}

/// Fewest benchmarks a geomean family needs to be shown, and for the headline to compare.
const GEOMEAN_MIN_ROWS: usize = 3;

/// One column's geomean of oriented ratios vs the reference: below 1.0 is better than the
/// reference, whatever the metric's `better`. `value` is `None` when no ratio was usable.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VariantGeomean {
    pub column: usize,
    pub label: String,
    pub value: Option<f64>,
    /// Ratios that entered the mean; the rest were missing, invalid, incomparable, zero,
    /// negative or non-finite.
    pub n_used: usize,
    pub n_total: usize,
}

/// The headline view over rows where the reference and every variant are present and valid.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeadlineGeomean {
    pub common_n: usize,
    pub total: usize,
    /// False when the common set is under 3 rows or under half the rows; then no winner is named.
    pub comparable: bool,
    /// The variant covering the fewest benchmarks, with its coverage; `None` with one column.
    pub least_covered: Option<(String, usize)>,
    /// One per column over the common rows (the reference is exactly 1.0).
    pub values: Vec<VariantGeomean>,
    /// Column with the lowest oriented geomean (ties: input order); `None` unless comparable.
    pub best: Option<usize>,
}

/// Geomeans for one `(measure, unit)` family; families are never mixed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeomeanFamily {
    pub measure: String,
    pub unit: String,
    /// Rows of the family where the reference is present.
    pub total: usize,
    /// Per-variant view over each column's rows shared with the reference, one per column.
    pub per_variant: Vec<VariantGeomean>,
    pub headline: HeadlineGeomean,
}

impl MatrixReport {
    /// The family the headline sentence uses: the first with a time unit, else the largest.
    pub fn primary_geomean(&self) -> Option<&GeomeanFamily> {
        let time = |f: &&GeomeanFamily| matches!(f.unit.as_str(), "ns" | "us" | "ms" | "s");
        self.geomean.iter().find(time).or_else(|| {
            // `max_by_key` keeps the last of equal maxima; reverse so the first wins.
            self.geomean.iter().rev().max_by_key(|f| f.total)
        })
    }
}

/// Oriented ratio of a cell against the reference, `None` when unusable. Lower is better.
fn oriented_ratio(row: &MatrixRow, ref_col: usize, col: usize) -> Option<f64> {
    let Cell::Present(reference) = &row.cells[ref_col] else {
        return None;
    };
    if col == ref_col {
        return (reference.value.is_finite() && reference.value > 0.0).then_some(1.0);
    }
    let Cell::Present(PresentCell {
        vs_reference: Comparison::Compared {
            delta: Some(delta), ..
        },
        ..
    }) = &row.cells[col]
    else {
        return None;
    };
    if !(reference.value.is_finite() && reference.value > 0.0) {
        return None;
    }
    // delta is (variant - base) / |base| in percent, so with a positive base this is variant / base.
    let ratio = 1.0 + delta / 100.0;
    if !ratio.is_finite() || ratio <= 0.0 {
        return None;
    }
    Some(if row.better == Better::Lower {
        ratio
    } else {
        1.0 / ratio
    })
}

fn variant_geomean(
    rows: &[&MatrixRow],
    columns: &[VariantLabel],
    ref_col: usize,
    col: usize,
) -> VariantGeomean {
    let ratios: Vec<f64> = rows
        .iter()
        .filter_map(|r| oriented_ratio(r, ref_col, col))
        .collect();
    VariantGeomean {
        column: col,
        label: columns[col].to_string(),
        value: stats::geomean(&ratios),
        n_used: ratios.len(),
        n_total: rows.len(),
    }
}

/// Runs after the Holm pass; verdicts do not feed the ratios, but this keeps one aggregate stage.
fn compute_geomeans(
    rows: &[MatrixRow],
    columns: &[VariantLabel],
    ref_col: usize,
) -> Vec<GeomeanFamily> {
    let mut order: Vec<(String, String)> = Vec::new();
    let mut families: HashMap<(String, String), Vec<&MatrixRow>> = HashMap::new();
    for row in rows {
        if !matches!(row.cells[ref_col], Cell::Present(_)) {
            continue;
        }
        let key = (row.measure.clone(), row.unit.clone());
        families
            .entry(key.clone())
            .or_insert_with(|| {
                order.push(key);
                Vec::new()
            })
            .push(row);
    }
    let mut out = Vec::new();
    for key in order {
        let members = &families[&key];
        let total = members.len();
        if total < GEOMEAN_MIN_ROWS {
            continue;
        }
        let per_variant = (0..columns.len())
            .map(|c| variant_geomean(members, columns, ref_col, c))
            .collect();
        let common: Vec<&MatrixRow> = members
            .iter()
            .filter(|r| r.cells.iter().all(Cell::is_usable))
            .copied()
            .collect();
        let comparable = common.len() >= GEOMEAN_MIN_ROWS && common.len() * 2 >= total;
        let least_covered = (0..columns.len())
            .filter(|c| *c != ref_col)
            .map(|c| (c, members.iter().filter(|r| r.cells[c].is_usable()).count()))
            .min_by_key(|(_, n)| *n)
            .map(|(c, n)| (columns[c].to_string(), n));
        let values: Vec<VariantGeomean> = (0..columns.len())
            .map(|c| variant_geomean(&common, columns, ref_col, c))
            .collect();
        let best = comparable
            .then(|| {
                values
                    .iter()
                    .filter_map(|v| v.value.map(|g| (v.column, g)))
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                    .map(|(c, _)| c)
            })
            .flatten();
        out.push(GeomeanFamily {
            measure: key.0,
            unit: key.1,
            total,
            per_variant,
            headline: HeadlineGeomean {
                common_n: common.len(),
                total,
                comparable,
                least_covered,
                values,
                best,
            },
        });
    }
    out
}

fn comparison_verdict(c: &Comparison) -> Option<Verdict> {
    match c {
        Comparison::Compared { verdict, .. } => Some(*verdict),
        Comparison::Incomparable(_) => Some(Verdict::Incomparable),
        Comparison::Invalid(_) => Some(Verdict::Invalid),
        Comparison::Reference | Comparison::DirectionMismatch | Comparison::NoReference => None,
    }
}

/// Why a cell trips the regression gate.
#[derive(Debug, Clone, PartialEq)]
pub enum FailureKind {
    /// Significantly slower than the reference by `pct` percent (`None`: reference value zero).
    Regressed {
        pct: Option<f64>,
    },
    Invalid,
}

/// One cell that trips the gate.
#[derive(Debug, Clone, PartialEq)]
pub struct FailedCell {
    pub variant: String,
    pub row: String,
    pub kind: FailureKind,
}

/// Cells that trip the regression gate, in table order. `gate` is `fail-on-regression`: without
/// it nothing fails; with it, a `Regressed` cell of at least that many percent or any `Invalid`
/// cell does. Requiring an explicit reference is the option validator's job (2.2.2a).
pub fn matrix_failures(report: &MatrixReport, gate: Option<f64>) -> Vec<FailedCell> {
    let Some(limit) = gate else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for row in &report.rows {
        for (col, cell) in row.cells.iter().enumerate() {
            let Cell::Present(p) = cell else { continue };
            let kind = match &p.vs_reference {
                Comparison::Invalid(_) => FailureKind::Invalid,
                Comparison::Compared {
                    verdict: Verdict::Regressed,
                    delta,
                    ..
                } if delta.is_none_or(|d| d.abs() >= limit) => {
                    FailureKind::Regressed { pct: *delta }
                }
                _ => continue,
            };
            out.push(FailedCell {
                variant: report.columns[col].to_string(),
                row: row.label(),
                kind,
            });
        }
    }
    out
}

/// The regression gate verdict: `!matrix_failures(..).is_empty()`.
pub fn matrix_failed(report: &MatrixReport, gate: Option<f64>) -> bool {
    !matrix_failures(report, gate).is_empty()
}

fn labels_text(variants: &[Variant]) -> String {
    variants
        .iter()
        .map(|v| v.label.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Index of the reference variant. Errors with `unknown reference 'x'; valid: a, b` for an
/// unknown label and, for `First`, when there are no variants.
pub fn resolve_reference(
    variants: &[Variant],
    reference: &Reference,
) -> Result<usize, MatrixError> {
    match reference {
        Reference::First if variants.is_empty() => Err(MatrixError("no variants".into())),
        Reference::First => Ok(0),
        Reference::Label(l) => variants.iter().position(|v| &v.label == l).ok_or_else(|| {
            MatrixError(format!(
                "unknown reference '{l}'; valid: {}",
                labels_text(variants)
            ))
        }),
    }
}

/// Data-dependent reference problem: a notice when not `strict`, the error when `strict`.
pub fn check_reference(
    variants: &[Variant],
    reference: &Reference,
    strict: bool,
) -> Result<Option<Notice>, MatrixError> {
    let Reference::Label(l) = reference else {
        return Ok(None);
    };
    match resolve_reference(variants, reference) {
        Ok(_) => Ok(None),
        Err(e) if strict => Err(e),
        Err(_) => Ok(Some(Notice {
            class: NoticeClass::Degradation,
            text: format!(
                "reference '{l}' is not one of: {}; no comparison made",
                labels_text(variants)
            ),
        })),
    }
}

#[cfg(test)]
thread_local! {
    /// Counts kernel calls so tests can assert a path never reaches the kernel.
    static KERNEL_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn kernel(base: &Metric, head: &Metric, opts: &Options) -> Row {
    #[cfg(test)]
    KERNEL_CALLS.with(|c| c.set(c.get() + 1));
    compare_pair(base, head, opts)
}

type RowKey = (String, String, String, Dims);

fn row_key(m: &Metric, group_axis: bool) -> RowKey {
    // Under compare-by=group the group is the column, so it must not split rows.
    let group = if group_axis {
        String::new()
    } else {
        m.group.clone()
    };
    (group, m.name.clone(), m.measure.clone(), m.dims.clone())
}

fn is_skipped(v: &Variant, key: &RowKey, group_axis: bool) -> bool {
    v.set.skipped.keys().iter().any(|k| {
        (group_axis || k.group == key.0) && k.name == key.1 && k.measure == key.2 && k.dims == key.3
    })
}

fn empty_report(
    summary: MatrixSummary,
    columns: Vec<VariantLabel>,
    notices: Vec<Notice>,
) -> MatrixReport {
    MatrixReport {
        summary,
        columns,
        rows: Vec::new(),
        reference_absent: Vec::new(),
        geomean: Vec::new(),
        notices,
    }
}

/// Compares every variant to the reference column (ADR-005). Pure and deterministic. An
/// unknown reference label yields a notice-only report; use [`check_reference`] to make that
/// an error under `strict`.
pub fn compare_matrix(variants: &[Variant], opts: &Options, mopts: &MatrixOptions) -> MatrixReport {
    let mut summary = MatrixSummary {
        title: opts.title.clone(),
        tag: opts.tag.clone(),
        threshold_pct: opts.threshold_pct,
        min_abs: opts.min_abs,
        alpha: opts.alpha,
        use_stats: opts.use_stats,
        only_changed: opts.only_changed,
        max_rows: opts.max_rows,
        fail_on_regression_pct: opts.fail_on_regression_pct,
        correction: mopts.correction,
        columns_cap: mopts.columns_cap,
        cell_format: mopts.cell_format,
        sort: mopts.sort,
        reference: None,
        skipped: variants
            .iter()
            .map(|v| VariantSkipped {
                label: v.label.to_string(),
                skipped: v.set.skipped.labels(),
                total: v.set.skipped.total,
            })
            .collect(),
        threshold_overrides: opts.threshold_overrides.clone(),
        config_sources: Vec::new(),
    };
    let columns: Vec<VariantLabel> = variants.iter().map(|v| v.label.clone()).collect();
    if variants.is_empty() {
        return empty_report(summary, Vec::new(), Vec::new());
    }
    let Ok(ref_col) = resolve_reference(variants, &mopts.reference) else {
        let notice = check_reference(variants, &mopts.reference, false)
            .ok()
            .flatten();
        return empty_report(summary, Vec::new(), notice.into_iter().collect());
    };
    summary.reference = Some(ReferenceInfo {
        label: variants[ref_col].label.to_string(),
        explicit: matches!(mopts.reference, Reference::Label(_)),
    });
    let group_axis = mopts.compare_by == GROUP_AXIS;

    // Rows in first-seen order, reference column first, so the row order does not depend on
    // where the explicit reference sits in the input.
    let mut order: Vec<RowKey> = Vec::new();
    let mut slots: HashMap<RowKey, Vec<Option<&Metric>>> = HashMap::new();
    let walk = std::iter::once(ref_col).chain((0..variants.len()).filter(|c| *c != ref_col));
    for col in walk {
        for m in &variants[col].set.metrics {
            let key = row_key(m, group_axis);
            let row = slots.entry(key.clone()).or_insert_with(|| {
                order.push(key);
                vec![None; variants.len()]
            });
            row[col].get_or_insert(m);
        }
    }

    let mut rows = Vec::with_capacity(order.len());
    for key in order {
        let metrics = &slots[&key];
        rows.push(build_row(
            variants, metrics, &key, ref_col, group_axis, opts,
        ));
    }
    let reference_absent = rows
        .iter()
        .filter(|r| matches!(r.cells[ref_col], Cell::Missing))
        .map(MatrixRow::label)
        .collect();
    if mopts.correction == Correction::Holm {
        // Before sorting and before any aggregate, so all of them see post-Holm verdicts.
        rows.iter_mut().for_each(|r| apply_holm(r, opts.alpha));
    }
    let geomean = compute_geomeans(&rows, &columns, ref_col);
    sort_matrix_rows(&mut rows, mopts.sort, ref_col);
    MatrixReport {
        summary,
        columns,
        rows,
        reference_absent,
        geomean,
        notices: Vec::new(),
    }
}

fn build_row(
    variants: &[Variant],
    metrics: &[Option<&Metric>],
    key: &RowKey,
    ref_col: usize,
    group_axis: bool,
    opts: &Options,
) -> MatrixRow {
    let reference = metrics[ref_col];
    let first = reference
        .or_else(|| metrics.iter().flatten().next().copied())
        .expect("a row has at least one metric");
    let mut ranked: Vec<(usize, f64)> = Vec::new();
    let mut cells = Vec::with_capacity(variants.len());
    for (col, variant) in variants.iter().enumerate() {
        let cell = match (metrics[col], reference) {
            (Some(m), _) if col == ref_col => Cell::Present(PresentCell {
                value: m.value,
                samples: reference_samples(m, opts),
                vs_reference: Comparison::Reference,
            }),
            (Some(m), None) => present(m.value, Samples::default(), Comparison::NoReference),
            (Some(m), Some(r)) => {
                let (cmp, samples) = compare_cell(r, m, opts);
                if matches!(cmp, Comparison::Compared { .. }) {
                    // Only comparable units reach here, so the factor exists.
                    let f = units::factor(&m.unit, &r.unit).unwrap_or(1.0);
                    ranked.push((col, m.value * f));
                }
                present(m.value, samples, cmp)
            }
            (None, Some(_)) if is_skipped(variant, key, group_axis) => present(
                f64::NAN,
                Samples::default(),
                Comparison::Invalid(Reason("no usable value in this variant".into())),
            ),
            (None, _) => Cell::Missing,
        };
        cells.push(cell);
    }
    if let Some(r) = reference {
        ranked.push((ref_col, r.value));
    }
    ranked.sort_by_key(|(c, _)| *c);
    let lower = first.better == Better::Lower;
    // Stable sort over column order, so ties keep input order.
    ranked.sort_by(|a, b| {
        if lower {
            a.1.total_cmp(&b.1)
        } else {
            b.1.total_cmp(&a.1)
        }
    });
    let ranking = if reference.is_some() {
        ranked.into_iter().map(|(c, _)| c).collect()
    } else {
        Vec::new()
    };
    MatrixRow {
        group: first.group.clone(),
        name: first.name.clone(),
        measure: first.measure.clone(),
        unit: first.unit.clone(),
        better: first.better,
        dims: first.dims.clone(),
        cells,
        ranking,
    }
}

fn present(value: f64, samples: Samples, vs_reference: Comparison) -> Cell {
    Cell::Present(PresentCell {
        value,
        samples,
        vs_reference,
    })
}

fn reference_samples(m: &Metric, opts: &Options) -> Samples {
    if opts.keep_samples {
        Samples {
            base: Vec::new(),
            head: m.samples.clone(),
        }
    } else {
        Samples::default()
    }
}

fn compare_cell(r: &Metric, m: &Metric, opts: &Options) -> (Comparison, Samples) {
    // Direction is the matrix's job: a mismatch never reaches the kernel.
    if r.better != m.better {
        return (Comparison::DirectionMismatch, Samples::default());
    }
    // Under compare-by=group the variant's group differs; thresholds key on the reference's.
    let row = if r.group == m.group {
        kernel(r, m, opts)
    } else {
        let mut head = m.clone();
        head.group = r.group.clone();
        kernel(r, &head, opts)
    };
    if row.verdict == Verdict::Incomparable {
        let reason = format!("units '{}' and '{}' cannot be compared", r.unit, m.unit);
        return (Comparison::Incomparable(Reason(reason)), row.samples);
    }
    let cmp = Comparison::Compared {
        verdict: row.verdict,
        delta: row.delta_pct,
        method: row.method,
        p_value: row.p_value,
        p_adjusted: None,
    };
    (cmp, row.samples)
}

/// One `Degradation` notice per threshold key that is neither a row's `group/residual` (bare
/// residual when the group is empty) nor its `group`; such a key can never apply in the matrix.
/// Keys written against raw benchmark names are the usual cause.
pub fn unmatched_threshold_notices(report: &MatrixReport, opts: &Options) -> Vec<Notice> {
    let matched = |key: &str| {
        report.rows.iter().any(|row| {
            let exact = if row.group.is_empty() {
                row.name.clone()
            } else {
                format!("{}/{}", row.group, row.name)
            };
            key == exact || (!row.group.is_empty() && key == row.group)
        })
    };
    opts.threshold_overrides
        .keys()
        .filter(|key| !matched(key))
        .map(|key| Notice {
            class: NoticeClass::Degradation,
            text: format!(
                "threshold key '{key}' matches no benchmark (matrix keys are group/residual or group)"
            ),
        })
        .collect()
}

/// Delta order: invalid and regressed cells first, then improved, then the rest; larger
/// magnitude first within a class.
fn delta_rank(row: &MatrixRow) -> (u8, f64) {
    row.cells
        .iter()
        .filter_map(|c| match c {
            Cell::Present(p) => Some(&p.vs_reference),
            Cell::Missing => None,
        })
        .map(|c| match c {
            Comparison::Invalid(_) => (0, f64::INFINITY),
            Comparison::Compared { verdict, delta, .. } => {
                let mag = delta.map_or(f64::INFINITY, f64::abs);
                match verdict {
                    Verdict::Regressed => (0, mag),
                    Verdict::Improved => (1, mag),
                    _ => (2, 0.0),
                }
            }
            _ => (2, 0.0),
        })
        .fold((2, 0.0), |best, c| {
            if c.0 < best.0 || (c.0 == best.0 && c.1 > best.1) {
                c
            } else {
                best
            }
        })
}

fn sort_matrix_rows(rows: &mut [MatrixRow], sort: MatrixSort, ref_col: usize) {
    // Stable sorts: ties keep input order.
    match sort {
        MatrixSort::Input => {}
        MatrixSort::Name => rows.sort_by(|a, b| {
            (&a.group, &a.name, &a.measure, &a.dims).cmp(&(&b.group, &b.name, &b.measure, &b.dims))
        }),
        MatrixSort::Delta => rows.sort_by(|a, b| {
            let ((ca, ma), (cb, mb)) = (delta_rank(a), delta_rank(b));
            ca.cmp(&cb).then(mb.total_cmp(&ma))
        }),
        MatrixSort::Rank => rows.sort_by_key(|r| {
            // Reference farthest from best first; rows without a ranking go last.
            let pos = r.ranking.iter().position(|c| *c == ref_col);
            std::cmp::Reverse(pos.map_or(-1, |p| p as i64))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dimensions::apply;
    use crate::model::{Dims, Metric, MetricSet};
    use crate::notice::NoticeClass;
    use crate::Better;

    fn set(items: &[(&str, &str)]) -> MetricSet {
        MetricSet {
            metrics: items
                .iter()
                .map(|(g, n)| Metric::new(g, n, "", "ns", 1.0, Better::Lower))
                .collect(),
            ..Default::default()
        }
    }

    fn dimensioned(names: &[&str]) -> Dimensioned {
        let items: Vec<(&str, &str)> = names.iter().map(|n| ("", *n)).collect();
        apply(set(&items), &[], false).unwrap().0
    }

    fn labels(p: &Projection) -> Vec<&str> {
        p.variants.iter().map(|v| v.label.as_str()).collect()
    }

    fn names(v: &Variant) -> Vec<&str> {
        v.set.metrics.iter().map(|m| m.name.as_str()).collect()
    }

    fn dims(pairs: &[(&str, &str)]) -> Dims {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn two_dim() -> Dimensioned {
        dimensioned(&[
            "Sort/impl=quick/size=10",
            "Sort/impl=quick/size=1000",
            "Sort/impl=heap/size=10",
            "Sort/impl=heap/size=1000",
        ])
    }

    #[test]
    fn project_by_axis_should_split_variants_in_first_seen_order_with_residual_names() {
        let d = dimensioned(&["Sort/impl=quick", "Sort/impl=heap", "Sort/impl=merge"]);
        let p = project_by_axis(&d, "impl", None).unwrap();
        assert_eq!(labels(&p), ["quick", "heap", "merge"]);
        assert!(p.variants.iter().all(|v| names(v) == ["Sort"]));
        assert!(p.notices.is_empty());
    }

    #[test]
    fn project_by_axis_should_default_reference_to_first_encountered_and_be_deterministic_when_variants_split_from_names(
    ) {
        let d = dimensioned(&["Sort/impl=heap", "Sort/impl=quick", "Sort/impl=merge"]);
        let a = project_by_axis(&d, "impl", None).unwrap();
        let b = project_by_axis(&d, "impl", None).unwrap();
        assert_eq!(labels(&a), ["heap", "quick", "merge"]);
        assert_eq!(labels(&a), labels(&b));
        assert_eq!(a.variants[0].label.as_str(), "heap");
    }

    #[test]
    fn project_by_axis_should_split_variants_by_size_param_when_compare_by_size() {
        let p = project_by_axis(&two_dim(), "size", None).unwrap();
        assert_eq!(labels(&p), ["10", "1000"]);
        assert_eq!(
            p.variants[0].set.metrics[0].dims,
            dims(&[("impl", "quick")])
        );
    }

    #[test]
    fn project_by_axis_should_label_rows_with_non_axis_dims_when_two_dimensions() {
        let p = project_by_axis(&two_dim(), "impl", None).unwrap();
        assert_eq!(labels(&p), ["quick", "heap"]);
        for v in &p.variants {
            let rows: Vec<String> = v.set.metrics.iter().map(row_label).collect();
            assert_eq!(rows, ["Sort [size=10]", "Sort [size=1000]"]);
        }
    }

    #[test]
    fn project_by_axis_should_error_rows_collide_naming_both_names_when_group_by_drops_dimension() {
        let d = two_dim();
        let group_by = ["foo".to_string()];
        // `foo` is unknown: that is the data-dependent path, so use a known dim that excludes size.
        let d2 = dimensioned(&[
            "Sort/impl=quick/size=10/mode=a",
            "Sort/impl=quick/size=1000/mode=a",
        ]);
        let err = project_by_axis(&d2, "impl", Some(&["mode".to_string()])).unwrap_err();
        assert_eq!(
            err.to_string(),
            "rows collide after dropping non-axis dimension 'size'; add it to group-by \
             (benchmarks 'Sort/impl=quick/size=10/mode=a' and 'Sort/impl=quick/size=1000/mode=a')"
        );
        assert!(project_by_axis(&d, "impl", Some(&group_by)).is_ok());
    }

    #[test]
    fn project_by_axis_should_keep_only_group_by_dims_in_row_identity() {
        let p = project_by_axis(&two_dim(), "impl", Some(&["size".to_string()])).unwrap();
        assert_eq!(p.variants[0].set.metrics.len(), 2);
        let d = dimensioned(&["Sort/impl=quick/size=10/m=a", "Sort/impl=heap/size=10/m=b"]);
        let p = project_by_axis(&d, "impl", Some(&["size".to_string()])).unwrap();
        assert_eq!(p.variants[0].set.metrics[0].dims, dims(&[("size", "10")]));
    }

    #[test]
    fn project_by_axis_should_reject_measure_as_axis_with_message_when_compare_by_measure() {
        let d = dimensioned(&[
            "Sort/impl=quick/size=10",
            "Sort/impl=heap/size=1000",
            "Sort/impl=merge/size=10",
        ]);
        let err = project_by_axis(&d, "measure", None).unwrap_err();
        assert_eq!(
            err.to_string(),
            "compare-by 'measure' is not an axis (units differ per measure); \
             available dimensions: impl (3 values), size (2 values), group"
        );
    }

    #[test]
    fn project_by_axis_should_error_no_benchmark_carries_dimension_when_axis_absent_everywhere() {
        let d = dimensioned(&["Sort/size=10", "Sort/size=1000"]);
        let p = project_by_axis(&d, "impl", None).unwrap();
        assert!(p.variants.is_empty());
        assert_eq!(p.notices.len(), 1);
        assert_eq!(p.notices[0].class, NoticeClass::Degradation);
        assert_eq!(
            p.notices[0].text,
            "compare-by 'impl': no benchmark carries this dimension; \
             available dimensions: size (2 values), group"
        );
    }

    #[test]
    fn project_by_axis_should_end_axis_errors_with_available_dimensions_when_axis_absent_unknown_or_measure(
    ) {
        let d = dimensioned(&["Sort/impl=quick", "Setup"]);
        let suffix = d.available_dimensions_text();
        let absent = project_by_axis(&d, "nope", None).unwrap();
        assert!(absent.notices[0].text.ends_with(&suffix));
        let unknown = project_by_axis(&d, "impl", Some(&["foo".to_string()])).unwrap();
        assert!(unknown.variants.is_empty());
        assert_eq!(
            unknown.notices[0].text,
            format!("group-by 'foo': no benchmark carries this dimension; {suffix}")
        );
        let measure = project_by_axis(&d, "measure", None).unwrap_err();
        assert!(measure.to_string().ends_with(&suffix));
        let bare = dimensioned(&["Setup"]);
        let n = &project_by_axis(&bare, "impl", None).unwrap().notices[0].text;
        assert!(n.ends_with("add a 'dimensions' rule or use labelled variants)"));
    }

    #[test]
    fn project_by_axis_should_split_by_group_when_compare_by_group() {
        let s = set(&[("pkgA", "Sort"), ("pkgB", "Sort")]);
        let d = apply(s, &[], false).unwrap().0;
        let p = project_by_axis(&d, "group", None).unwrap();
        assert_eq!(labels(&p), ["pkgA", "pkgB"]);
        assert_eq!(names(&p.variants[1]), ["Sort"]);
    }

    #[test]
    fn project_by_axis_should_exclude_metrics_lacking_the_axis_and_say_so() {
        let d = dimensioned(&["Sort/impl=quick", "Setup", "Sort/impl=heap"]);
        let p = project_by_axis(&d, "impl", None).unwrap();
        assert!(p.variants.iter().all(|v| names(v) == ["Sort"]));
        assert_eq!(p.notices.len(), 1);
        assert_eq!(p.notices[0].class, NoticeClass::Degradation);
        assert_eq!(
            p.notices[0].text,
            "1 benchmark has no 'impl' dimension (first 10: Setup)"
        );
        let many: Vec<String> = (0..12).map(|i| format!("S{i}")).collect();
        let mut all: Vec<&str> = many.iter().map(String::as_str).collect();
        all.push("Sort/impl=quick");
        let p = project_by_axis(&dimensioned(&all), "impl", None).unwrap();
        assert!(p.notices[0]
            .text
            .starts_with("12 benchmarks have no 'impl' dimension (first 10: S0, S1,"));
    }

    #[test]
    fn project_by_axis_should_never_rewrite_source_names() {
        let d = dimensioned(&["Sort/impl=quick"]);
        project_by_axis(&d, "impl", None).unwrap();
        assert_eq!(
            d.entries().next().unwrap().0.name,
            "Sort/impl=quick",
            "source name must stay"
        );
    }

    #[test]
    fn project_by_axis_should_attribute_skipped_keys_to_variant_and_row() {
        let mut s = set(&[("", "Sort/impl=quick"), ("", "Sort/impl=heap")]);
        s.skipped.push("", "Sort/impl=merge", "");
        s.skipped.push("", "Setup", "");
        let d = apply(s, &[], false).unwrap().0;
        let p = project_by_axis(&d, "impl", None).unwrap();
        assert_eq!(labels(&p), ["quick", "heap", "merge"]);
        let merge = &p.variants[2];
        assert!(merge.set.metrics.is_empty());
        assert_eq!(merge.set.skipped.labels(), ["Sort"]);
        assert!(p.variants[0].set.skipped.labels().is_empty());
    }

    #[test]
    fn variant_label_should_accept_only_the_label_grammar() {
        assert!(VariantLabel::parse("quick-1.x_y").is_ok());
        assert!(VariantLabel::parse(&"a".repeat(40)).is_ok());
        for bad in ["", "a b", "a/b", "a=b", &"a".repeat(41)] {
            assert!(VariantLabel::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn project_by_axis_should_sanitise_invalid_axis_values_and_say_so() {
        let d = dimensioned(&["Sort[impl=a b]", "Sort[impl=ok]"]);
        let p = project_by_axis(&d, "impl", None).unwrap();
        assert_eq!(labels(&p), ["a_b", "ok"]);
        assert_eq!(p.notices.len(), 1);
        assert_eq!(p.notices[0].class, NoticeClass::Degradation);
        assert_eq!(p.notices[0].text, "axis value 'a b' shown as 'a_b'");
    }

    #[test]
    fn project_by_axis_should_accept_go_package_paths_under_compare_by_group() {
        let s = set(&[("net/http", "Get"), ("net/url", "Get")]);
        let d = apply(s, &[], false).unwrap().0;
        let p = project_by_axis(&d, "group", None).unwrap();
        assert_eq!(labels(&p), ["net_http", "net_url"]);
        assert_eq!(names(&p.variants[0]), ["Get"]);
    }

    #[test]
    fn project_by_axis_should_make_colliding_sanitised_labels_unique() {
        let s = set(&[
            ("net/http", "Get"),
            ("net_http", "Get"),
            ("net:http", "Get"),
        ]);
        let d = apply(s, &[], false).unwrap().0;
        let p = project_by_axis(&d, "group", None).unwrap();
        assert_eq!(labels(&p), ["net_http-2", "net_http", "net_http-3"]);
        assert_eq!(
            p.notices[0].text,
            "axis value 'net/http' shown as 'net_http-2'; axis value 'net:http' shown as 'net_http-3'"
        );
    }

    #[test]
    fn project_by_axis_should_truncate_long_values_and_cap_the_notice_list() {
        let long = format!("p/{}", "x".repeat(60));
        let names: Vec<String> = (0..7).map(|i| format!("a/{i}")).collect();
        let mut items: Vec<(&str, &str)> = vec![(long.as_str(), "B")];
        items.extend(names.iter().map(|n| (n.as_str(), "B")));
        let d = apply(set(&items), &[], false).unwrap().0;
        let p = project_by_axis(&d, "group", None).unwrap();
        assert!(p.variants.iter().all(|v| v.label.as_str().len() <= 40));
        assert_eq!(p.variants[0].label.as_str().len(), 40);
        let text = &p.notices[0].text;
        assert_eq!(text.matches("shown as").count(), 5, "{text}");
        assert!(text.ends_with("; and 3 more"), "{text}");
    }

    #[test]
    fn variant_label_parse_should_still_error_for_explicit_labels() {
        assert!(VariantLabel::parse("net/http").is_err());
        assert!(VariantLabel::parse("a b").is_err());
    }

    #[test]
    fn labelled_files_should_never_merge_and_keep_identity_residuals() {
        let a = Dimensioned::identity(set(&[("", "BenchmarkSort")]));
        let b = Dimensioned::identity(set(&[("", "BenchmarkSort")]));
        let va = label_variant(VariantLabel::parse("quick").unwrap(), &a, "impl").unwrap();
        let vb = label_variant(VariantLabel::parse("heap").unwrap(), &b, "impl").unwrap();
        assert_eq!(names(&va), ["BenchmarkSort"]);
        assert_eq!(names(&vb), ["BenchmarkSort"]);
        assert!(va.set.metrics[0].dims.is_empty());
    }

    #[test]
    fn label_variant_should_error_when_axis_set_by_both_file_label_and_name_rule() {
        let d = dimensioned(&["Sort/impl=quick"]);
        let err = label_variant(VariantLabel::parse("quick").unwrap(), &d, "impl").unwrap_err();
        assert_eq!(
            err.to_string(),
            "axis 'impl' set by both file label and name rule"
        );
        let ok = label_variant(VariantLabel::parse("quick").unwrap(), &d, "size").unwrap();
        assert_eq!(names(&ok), ["Sort"]);
    }

    // ---- compare_matrix ----

    fn series(median: f64) -> Vec<f64> {
        (0..10)
            .map(|k| median * (1.0 + 0.001 * (k as f64 - 5.0)))
            .collect()
    }

    fn metric_with(group: &str, name: &str, unit: &str, median: f64, better: Better) -> Metric {
        let mut m = Metric::new(group, name, "", unit, median, better);
        m.samples = series(median);
        m
    }

    fn var(label: &str, items: &[(&str, f64)]) -> Variant {
        Variant {
            label: VariantLabel::parse(label).unwrap(),
            set: MetricSet {
                metrics: items
                    .iter()
                    .map(|(n, v)| metric_with("", n, "ns", *v, Better::Lower))
                    .collect(),
                ..Default::default()
            },
        }
    }

    fn three() -> Vec<Variant> {
        vec![
            var("quick", &[("Sort", 100.0)]),
            var("heap", &[("Sort", 130.0)]),
            var("merge", &[("Sort", 80.0)]),
        ]
    }

    fn mopts() -> MatrixOptions {
        MatrixOptions {
            compare_by: "impl".into(),
            ..MatrixOptions::default()
        }
    }

    fn compared(cell: &Cell) -> (&Verdict, Option<f64>) {
        match cell {
            Cell::Present(PresentCell {
                vs_reference: Comparison::Compared { verdict, delta, .. },
                ..
            }) => (verdict, *delta),
            other => panic!("not compared: {other:?}"),
        }
    }

    fn kernel_calls() -> usize {
        KERNEL_CALLS.with(std::cell::Cell::get)
    }

    #[test]
    fn compare_matrix_should_emit_cells_for_all_variants_against_reference_when_three_variants() {
        let r = compare_matrix(&three(), &Options::default(), &mopts());
        assert_eq!(r.columns.len(), 3);
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0].cells.len(), 3);
        assert!(matches!(
            r.rows[0].cells[0],
            Cell::Present(PresentCell {
                vs_reference: Comparison::Reference,
                ..
            })
        ));
        assert_eq!(r.summary.reference.as_ref().unwrap().label, "quick");
        assert!(!r.summary.reference.as_ref().unwrap().explicit);
    }

    #[test]
    fn compare_matrix_should_mark_regressed_and_improved_cells_when_130_and_80_vs_100() {
        let r = compare_matrix(&three(), &Options::default(), &mopts());
        let row = &r.rows[0];
        let (v, d) = compared(&row.cells[1]);
        assert_eq!(
            (*v, d.map(|x| (x * 10.0).round() / 10.0)),
            (Verdict::Regressed, Some(30.0))
        );
        let (v, d) = compared(&row.cells[2]);
        assert_eq!(
            (*v, d.map(|x| (x * 10.0).round() / 10.0)),
            (Verdict::Improved, Some(-20.0))
        );
        if let Cell::Present(PresentCell {
            vs_reference:
                Comparison::Compared {
                    method,
                    p_value,
                    p_adjusted,
                    ..
                },
            ..
        }) = &row.cells[1]
        {
            assert_eq!(*method, Some(crate::Method::MannWhitney));
            assert!(p_value.is_some());
            assert!(p_adjusted.unwrap().get() >= p_value.unwrap());
        }
        assert_eq!(row.ranking, [2, 0, 1]);
    }

    #[test]
    fn compare_matrix_should_orient_verdicts_and_ranking_by_reference_better_when_higher_is_better()
    {
        let mut vs = three();
        for v in &mut vs {
            for m in &mut v.set.metrics {
                m.better = Better::Higher;
            }
        }
        let r = compare_matrix(&vs, &Options::default(), &mopts());
        assert_eq!(*compared(&r.rows[0].cells[1]).0, Verdict::Improved);
        assert_eq!(*compared(&r.rows[0].cells[2]).0, Verdict::Regressed);
        assert_eq!(r.rows[0].ranking, [1, 0, 2]);
        assert_eq!(r.rows[0].better, Better::Higher);
    }

    #[test]
    fn compare_matrix_should_mark_missing_cell_not_zero_when_variant_lacks_benchmark() {
        let vs = vec![
            var("quick", &[("Sort", 100.0), ("Search", 10.0)]),
            var("merge", &[("Sort", 80.0)]),
        ];
        let r = compare_matrix(&vs, &Options::default(), &mopts());
        let search = r.rows.iter().find(|x| x.name == "Search").unwrap();
        assert!(matches!(search.cells[1], Cell::Missing));
        assert_eq!(search.ranking, [0]);
    }

    #[test]
    fn compare_matrix_should_list_reference_absent_rows_without_deltas_when_reference_lacks_benchmark(
    ) {
        let vs = vec![
            var("quick", &[("Sort", 100.0)]),
            var("merge", &[("Sort", 80.0), ("Insert", 5.0)]),
        ];
        let r = compare_matrix(&vs, &Options::default(), &mopts());
        assert_eq!(r.reference_absent, ["Insert"]);
        let insert = r.rows.iter().find(|x| x.name == "Insert").unwrap();
        assert!(matches!(insert.cells[0], Cell::Missing));
        assert!(matches!(
            insert.cells[1],
            Cell::Present(PresentCell {
                vs_reference: Comparison::NoReference,
                ..
            })
        ));
        assert!(insert.ranking.is_empty());
    }

    #[test]
    fn compare_matrix_should_return_direction_mismatch_without_calling_kernel_when_better_differs()
    {
        let mut vs = three();
        vs[1].set.metrics[0].better = Better::Higher;
        vs.truncate(2);
        let before = kernel_calls();
        let r = compare_matrix(&vs, &Options::default(), &mopts());
        assert_eq!(kernel_calls(), before, "kernel must not be reached");
        assert!(matches!(
            r.rows[0].cells[1],
            Cell::Present(PresentCell {
                vs_reference: Comparison::DirectionMismatch,
                value,
                ..
            }) if value == 130.0
        ));
        assert_eq!(r.count(Verdict::Regressed) + r.count(Verdict::Improved), 0);
        assert!(r.rows[0].ranking == [0]);
        compare_matrix(&three(), &Options::default(), &mopts());
        assert_eq!(kernel_calls(), before + 2);
    }

    #[test]
    fn compare_matrix_should_give_identical_deltas_and_verdicts_when_inputs_permuted_with_explicit_reference(
    ) {
        let base = three();
        let perms = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let mo = MatrixOptions {
            reference: Reference::Label(VariantLabel::parse("quick").unwrap()),
            ..mopts()
        };
        let mut seen: Option<BTreeMap<String, String>> = None;
        for p in perms {
            let vs: Vec<Variant> = p.iter().map(|&i| base[i].clone()).collect();
            let r = compare_matrix(&vs, &Options::default(), &mo);
            let mut by_label = BTreeMap::new();
            for (c, label) in r.columns.iter().enumerate() {
                let cell = serde_json::to_string(&r.rows[0].cells[c]).unwrap();
                by_label.insert(label.to_string(), cell);
            }
            let ranked: Vec<&str> = r.rows[0]
                .ranking
                .iter()
                .map(|&c| r.columns[c].as_str())
                .collect();
            by_label.insert("rank".into(), ranked.join(","));
            match &seen {
                None => seen = Some(by_label),
                Some(s) => assert_eq!(*s, by_label, "permutation {p:?}"),
            }
            assert!(r.summary.reference.as_ref().unwrap().explicit);
        }
    }

    #[test]
    fn compare_matrix_should_mark_incomparable_cell_when_units_differ() {
        let mut vs = three();
        vs[1].set.metrics[0].unit = "B/op".into();
        let r = compare_matrix(&vs, &Options::default(), &mopts());
        assert!(matches!(
            r.rows[0].cells[1],
            Cell::Present(PresentCell {
                vs_reference: Comparison::Incomparable(_),
                ..
            })
        ));
        assert_eq!(r.rows[0].ranking, [2, 0]);
        assert_eq!(r.count(Verdict::Incomparable), 1);
    }

    fn twelve_percent(group: &str, name: &str) -> Vec<Variant> {
        let mk = |label: &str, v: f64| Variant {
            label: VariantLabel::parse(label).unwrap(),
            set: MetricSet {
                metrics: vec![metric_with(group, name, "ns", v, Better::Lower)],
                ..Default::default()
            },
        };
        vec![mk("quick", 100.0), mk("heap", 112.0)]
    }

    #[test]
    fn compare_matrix_should_apply_group_residual_threshold_override_when_sort_20_percent() {
        let opts = Options {
            threshold_overrides: BTreeMap::from([("Sort".to_string(), 20.0)]),
            ..Options::default()
        };
        let sort = compare_matrix(&twelve_percent("", "Sort"), &opts, &mopts());
        assert_eq!(*compared(&sort.rows[0].cells[1]).0, Verdict::Unchanged);
        let other = compare_matrix(&twelve_percent("", "Other"), &opts, &mopts());
        assert_eq!(*compared(&other.rows[0].cells[1]).0, Verdict::Regressed);
    }

    #[test]
    fn compare_matrix_should_apply_group_threshold_when_only_group_key_matches() {
        let opts = Options {
            threshold_overrides: BTreeMap::from([("pkgA".to_string(), 20.0)]),
            ..Options::default()
        };
        let a = compare_matrix(&twelve_percent("pkgA", "Sort"), &opts, &mopts());
        assert_eq!(*compared(&a.rows[0].cells[1]).0, Verdict::Unchanged);
        let b = compare_matrix(&twelve_percent("pkgB", "Sort"), &opts, &mopts());
        assert_eq!(*compared(&b.rows[0].cells[1]).0, Verdict::Regressed);
    }

    #[test]
    fn compare_matrix_should_join_rows_across_groups_and_use_reference_group_threshold_when_compare_by_group(
    ) {
        let mk = |label: &str, v: f64| Variant {
            label: VariantLabel::parse(label).unwrap(),
            set: MetricSet {
                metrics: vec![metric_with(label, "Sort", "ns", v, Better::Lower)],
                ..Default::default()
            },
        };
        let vs = vec![mk("pkgA", 100.0), mk("pkgB", 112.0)];
        let mo = MatrixOptions {
            compare_by: "group".into(),
            ..MatrixOptions::default()
        };
        let opts = Options {
            threshold_overrides: BTreeMap::from([("pkgA".to_string(), 20.0)]),
            ..Options::default()
        };
        let r = compare_matrix(&vs, &opts, &mo);
        assert_eq!(r.rows.len(), 1, "group is not part of the row key");
        assert_eq!(*compared(&r.rows[0].cells[1]).0, Verdict::Unchanged);
    }

    fn with_skipped(mut v: Variant, name: &str) -> Variant {
        v.set.skipped.push_key(SkippedKey {
            group: String::new(),
            name: name.into(),
            measure: String::new(),
            dims: Dims::new(),
            residual: None,
        });
        v
    }

    #[test]
    fn compare_matrix_should_return_invalid_cell_and_trip_failed_when_variant_series_all_nan() {
        let vs = vec![
            var("quick", &[("Sort", 100.0)]),
            with_skipped(var("merge", &[]), "Sort"),
        ];
        let r = compare_matrix(&vs, &Options::default(), &mopts());
        let cell = &r.rows[0].cells[1];
        assert!(matches!(
            cell,
            Cell::Present(PresentCell {
                vs_reference: Comparison::Invalid(_),
                ..
            })
        ));
        assert_eq!(r.invalid_in(1), 1);
        assert_eq!(r.invalid_in(0), 0);
        assert!(matrix_failed(&r, Some(10.0)));
        assert!(!matrix_failed(&r, None));
        assert_eq!(r.rows[0].ranking, [0]);
    }

    #[test]
    fn compare_matrix_should_match_skipped_by_group_residual_measure_and_dims_not_raw_name() {
        let mut ref_v = var("quick", &[("Sort", 100.0)]);
        ref_v.set.metrics[0].dims = dims(&[("size", "10")]);
        let mut other = var("merge", &[]);
        let key = |size: &str| SkippedKey {
            group: String::new(),
            name: "Sort".into(),
            measure: String::new(),
            dims: dims(&[("size", size)]),
            residual: None,
        };
        other.set.skipped.push_key(key("1000"));
        let r = compare_matrix(
            &[ref_v.clone(), other.clone()],
            &Options::default(),
            &mopts(),
        );
        assert!(
            matches!(r.rows[0].cells[1], Cell::Missing),
            "different dims is not this row"
        );
        other.set.skipped.push_key(key("10"));
        let r = compare_matrix(&[ref_v, other], &Options::default(), &mopts());
        assert!(matches!(
            r.rows[0].cells[1],
            Cell::Present(PresentCell {
                vs_reference: Comparison::Invalid(_),
                ..
            })
        ));
    }

    #[test]
    fn compare_matrix_should_keep_samples_for_charts_only_when_asked() {
        let vs = vec![
            var("quick", &[("Sort", 100.0)]),
            var("heap", &[("Sort", 130.0)]),
        ];
        let off = compare_matrix(&vs, &Options::default(), &mopts());
        let Cell::Present(p) = &off.rows[0].cells[1] else {
            panic!()
        };
        assert!(p.samples.base.is_empty() && p.samples.head.is_empty());
        let on = compare_matrix(
            &vs,
            &Options {
                keep_samples: true,
                ..Options::default()
            },
            &mopts(),
        );
        let Cell::Present(p) = &on.rows[0].cells[1] else {
            panic!()
        };
        assert_eq!(p.samples.base, series(100.0));
        assert_eq!(p.samples.head, series(130.0));
        assert!(!serde_json::to_string(&on).unwrap().contains("samples"));
    }

    #[test]
    fn compare_matrix_should_copy_options_and_matrix_options_into_summary_so_renderers_need_no_second_argument(
    ) {
        let opts = Options {
            title: "T".into(),
            tag: "x".into(),
            max_rows: Some(20),
            only_changed: true,
            fail_on_regression_pct: Some(10.0),
            ..Options::default()
        };
        let mo = MatrixOptions {
            cell_format: CellFormat::Ratio,
            columns_cap: 8,
            reference: Reference::Label(VariantLabel::parse("heap").unwrap()),
            ..mopts()
        };
        let r = compare_matrix(&three(), &opts, &mo);
        let s = &r.summary;
        assert_eq!((s.title.as_str(), s.tag.as_str()), ("T", "x"));
        assert_eq!(s.max_rows, Some(20));
        assert!(s.only_changed);
        assert_eq!(s.fail_on_regression_pct, Some(10.0));
        assert_eq!(s.cell_format, CellFormat::Ratio);
        assert_eq!(s.columns_cap, 8);
        assert_eq!(s.correction, Correction::Holm);
        assert_eq!((s.threshold_pct, s.alpha), (5.0, 0.05));
        let reference = s.reference.as_ref().unwrap();
        assert_eq!(
            (reference.label.as_str(), reference.explicit),
            ("heap", true)
        );
    }

    #[test]
    fn matrix_failed_should_be_false_without_gate_and_true_only_for_post_holm_regressed_over_threshold_or_invalid(
    ) {
        let mk = |heap: f64| {
            compare_matrix(
                &[
                    var("quick", &[("Sort", 100.0)]),
                    var("heap", &[("Sort", heap)]),
                ],
                &Options::default(),
                &mopts(),
            )
        };
        assert!(!matrix_failed(&mk(130.0), None));
        assert!(!matrix_failed(&mk(107.0), Some(10.0)));
        assert!(matrix_failed(&mk(130.0), Some(10.0)));
        assert!(!matrix_failed(&mk(80.0), Some(10.0)));
    }

    #[test]
    fn compare_matrix_should_fail_with_unknown_reference_listing_valid_labels_when_reference_not_a_label(
    ) {
        let slow = Reference::Label(VariantLabel::parse("slow").unwrap());
        let err = resolve_reference(&three(), &slow).unwrap_err();
        assert_eq!(
            err.to_string(),
            "unknown reference 'slow'; valid: quick, heap, merge"
        );
        assert_eq!(resolve_reference(&three(), &Reference::First).unwrap(), 0);
    }

    #[test]
    fn project_by_axis_should_return_notice_not_error_when_reference_absent_from_axis_values_and_not_strict(
    ) {
        let d = dimensioned(&["Sort/impl=quick", "Sort/impl=heap"]);
        let p = project_by_axis(&d, "impl", None).unwrap();
        let slow = Reference::Label(VariantLabel::parse("slow").unwrap());
        let notice = check_reference(&p.variants, &slow, false).unwrap().unwrap();
        assert_eq!(notice.class, NoticeClass::Degradation);
        assert_eq!(
            notice.text,
            "reference 'slow' is not one of: quick, heap; no comparison made"
        );
        let err = check_reference(&p.variants, &slow, true).unwrap_err();
        assert_eq!(
            err.to_string(),
            "unknown reference 'slow'; valid: quick, heap"
        );
        assert!(check_reference(&p.variants, &Reference::First, true)
            .unwrap()
            .is_none());

        let r = compare_matrix(
            &p.variants,
            &Options::default(),
            &MatrixOptions {
                reference: slow,
                ..mopts()
            },
        );
        assert!(r.rows.is_empty() && r.columns.is_empty());
        assert_eq!(r.notices.len(), 1);
    }

    #[test]
    fn compare_matrix_should_sort_rows_by_mode() {
        let mk = |name: &str, heap: f64| (name.to_string(), heap);
        let rows = [mk("B", 80.0), mk("A", 130.0), mk("C", 80.0)];
        let quick: Vec<(&str, f64)> = rows.iter().map(|(n, _)| (n.as_str(), 100.0)).collect();
        let heap: Vec<(&str, f64)> = rows.iter().map(|(n, v)| (n.as_str(), *v)).collect();
        let vs = vec![var("quick", &quick), var("heap", &heap)];
        let names = |sort| {
            let r = compare_matrix(&vs, &Options::default(), &MatrixOptions { sort, ..mopts() });
            r.rows.iter().map(|x| x.name.clone()).collect::<Vec<_>>()
        };
        assert_eq!(names(MatrixSort::Input), ["B", "A", "C"]);
        assert_eq!(names(MatrixSort::Name), ["A", "B", "C"]);
        assert_eq!(names(MatrixSort::Delta), ["A", "B", "C"]);
        assert_eq!(names(MatrixSort::Rank), ["B", "C", "A"]);
    }

    #[test]
    fn compare_matrix_should_be_deterministic_and_report_nothing_for_no_variants() {
        let a = serde_json::to_string(&compare_matrix(&three(), &Options::default(), &mopts()))
            .unwrap();
        let b = serde_json::to_string(&compare_matrix(&three(), &Options::default(), &mopts()))
            .unwrap();
        assert_eq!(a, b);
        let r = compare_matrix(&[], &Options::default(), &mopts());
        assert!(r.rows.is_empty() && r.summary.reference.is_none());
    }

    #[test]
    fn p_adjusted_should_clamp_into_unit_interval() {
        assert_eq!(PAdjusted::new(1.7).get(), 1.0);
        assert_eq!(PAdjusted::new(-0.2).get(), 0.0);
        assert_eq!(PAdjusted::new(f64::NAN).get(), 1.0);
        assert_eq!(PAdjusted::new(0.06).get(), 0.06);
    }

    // ---- Holm post-pass ----

    fn tested(verdict: Verdict, p: f64) -> Cell {
        present(
            100.0,
            Samples::default(),
            Comparison::Compared {
                verdict,
                delta: Some(30.0),
                method: Some(Method::MannWhitney),
                p_value: Some(p),
                p_adjusted: None,
            },
        )
    }

    fn row_of(cells: Vec<Cell>) -> MatrixRow {
        MatrixRow {
            group: String::new(),
            name: "Sort".into(),
            measure: String::new(),
            unit: "ns".into(),
            better: Better::Lower,
            dims: Dims::default(),
            cells,
            ranking: Vec::new(),
        }
    }

    fn outcome(cell: &Cell) -> (Verdict, Option<f64>, Option<f64>) {
        match cell {
            Cell::Present(PresentCell {
                vs_reference:
                    Comparison::Compared {
                        verdict,
                        p_value,
                        p_adjusted,
                        ..
                    },
                ..
            }) => (*verdict, *p_value, p_adjusted.map(PAdjusted::get)),
            other => panic!("not compared: {other:?}"),
        }
    }

    #[test]
    fn holm_pass_should_only_downgrade_and_use_m_equal_cells_with_pvalue_when_floor_suppressed_cell(
    ) {
        let reference = present(100.0, Samples::default(), Comparison::Reference);
        let interval = present(
            100.0,
            Samples::default(),
            Comparison::Compared {
                verdict: Verdict::Regressed,
                delta: Some(30.0),
                method: Some(Method::Interval),
                p_value: None,
                p_adjusted: None,
            },
        );
        let mut row = row_of(vec![
            reference,
            tested(Verdict::Regressed, 0.01),
            tested(Verdict::Regressed, 0.04),
            tested(Verdict::Unchanged, 0.03),
            interval,
        ]);
        assert_eq!(row.holm_m(), 3);
        apply_holm(&mut row, 0.05);
        assert_eq!(
            outcome(&row.cells[1]),
            (Verdict::Regressed, Some(0.01), Some(0.03))
        );
        let (v, p, adj) = outcome(&row.cells[2]);
        assert_eq!((v, p), (Verdict::Unchanged, Some(0.04)));
        assert!((adj.unwrap() - 0.06).abs() < 1e-12);
        let (v, _, adj) = outcome(&row.cells[3]);
        assert_eq!(v, Verdict::Unchanged);
        assert!((adj.unwrap() - 0.06).abs() < 1e-12);
        assert_eq!(outcome(&row.cells[4]), (Verdict::Regressed, None, None));
        assert_eq!(row.holm_m(), 3);
    }

    #[test]
    fn holm_pass_should_be_identity_when_single_variant() {
        let mut row = row_of(vec![
            present(100.0, Samples::default(), Comparison::Reference),
            tested(Verdict::Regressed, 0.04),
        ]);
        apply_holm(&mut row, 0.05);
        assert_eq!(
            outcome(&row.cells[1]),
            (Verdict::Regressed, Some(0.04), Some(0.04))
        );
    }

    /// 4 vs 4 fully separated is the smallest sample that can test: exact p = 2/70 = 0.0286.
    fn separated(label: &str, base: f64) -> Variant {
        let mut m = Metric::new("", "Sort", "", "ns", base + 1.5, Better::Lower);
        m.samples = (0..4).map(|k| base + k as f64).collect();
        Variant {
            label: VariantLabel::parse(label).unwrap(),
            set: MetricSet {
                metrics: vec![m],
                ..Default::default()
            },
        }
    }

    fn two_regressions() -> Vec<Variant> {
        vec![
            separated("quick", 100.0),
            separated("heap", 130.0),
            separated("merge", 140.0),
        ]
    }

    #[test]
    fn counts_geomean_and_matrix_failed_should_use_post_holm_verdicts_when_cell_downgraded() {
        let opts = Options::default();
        let holm = compare_matrix(&two_regressions(), &opts, &mopts());
        for cell in &holm.rows[0].cells[1..] {
            let (v, p, adj) = outcome(cell);
            assert_eq!(v, Verdict::Unchanged);
            assert!((p.unwrap() - 2.0 / 70.0).abs() < 1e-9);
            assert!((adj.unwrap() - 4.0 / 70.0).abs() < 1e-9);
        }
        assert_eq!(holm.rows[0].holm_m(), 2);
        assert_eq!(holm.count(Verdict::Regressed), 0);
        assert_eq!(holm.count(Verdict::Unchanged), 2);
        assert!(!matrix_failed(&holm, Some(10.0)));

        let none = MatrixOptions {
            correction: Correction::None,
            ..mopts()
        };
        let raw = compare_matrix(&two_regressions(), &opts, &none);
        assert_eq!(raw.count(Verdict::Regressed), 2);
        assert!(matrix_failed(&raw, Some(10.0)));
        assert_eq!(outcome(&raw.rows[0].cells[1]).2, None);
    }

    #[test]
    fn matrix_should_not_fail_when_300_identical_distribution_rows() {
        let mut seed = 42u64;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut variants: Vec<Variant> = ["a", "b", "c"]
            .iter()
            .map(|l| Variant {
                label: VariantLabel::parse(l).unwrap(),
                set: MetricSet::default(),
            })
            .collect();
        for row in 0..300 {
            for v in &mut variants {
                let mut m = Metric::new("", &format!("B{row}"), "", "ns", 100.0, Better::Lower);
                // Within +-2% of 100, so no median pair differs by the default 5%.
                m.samples = (0..10)
                    .map(|_| 100.0 * (1.0 + 0.04 * (next() - 0.5)))
                    .collect();
                m.value = crate::stats::median(&m.samples);
                v.set.metrics.push(m);
            }
        }
        let r = compare_matrix(&variants, &Options::default(), &mopts());
        assert_eq!(r.rows.len(), 300);
        assert_eq!(r.count(Verdict::Regressed), 0);
        assert!(!matrix_failed(&r, Some(5.0)));
    }

    // ---- Geomean ----

    /// One variant; each item is (name, measure, unit, value, better).
    fn gvariant(label: &str, items: &[(&str, &str, &str, f64, Better)]) -> Variant {
        Variant {
            label: VariantLabel::parse(label).unwrap(),
            set: MetricSet {
                metrics: items
                    .iter()
                    .map(|(n, m, u, v, b)| Metric::new("", n, m, u, *v, *b))
                    .collect(),
                ..Default::default()
            },
        }
    }

    fn ns(name: &str, value: f64) -> (String, f64) {
        (name.to_string(), value)
    }

    fn lower_variant(label: &str, rows: &[(String, f64)]) -> Variant {
        let items: Vec<(&str, &str, &str, f64, Better)> = rows
            .iter()
            .map(|(n, v)| (n.as_str(), "", "ns", *v, Better::Lower))
            .collect();
        gvariant(label, &items)
    }

    fn only_family(r: &MatrixReport) -> &GeomeanFamily {
        assert_eq!(r.geomean.len(), 1);
        &r.geomean[0]
    }

    #[test]
    fn geomean_should_be_orientation_aware_and_exclude_zero_with_n_used_of_total() {
        // Ratios 0.5 and 2.0 vs reference, better=lower: geomean 1.00x, n=2/2 (hand: sqrt(0.5*2)=1).
        let base: Vec<_> = (1..=3).map(|i| ns(&format!("B{i}"), 100.0)).collect();
        let head = vec![ns("B1", 50.0), ns("B2", 200.0), ns("B3", 100.0)];
        let r = compare_matrix(
            &[lower_variant("a", &base), lower_variant("b", &head)],
            &Options::default(),
            &mopts(),
        );
        let g = &only_family(&r).per_variant[1];
        assert!((g.value.unwrap() - 1.0).abs() < 1e-12);
        assert_eq!((g.n_used, g.n_total), (3, 3));

        // better=higher with ratio 2.0 contributes 0.5; hand: cbrt(0.5 * 0.5 * 2) = 0.5^(1/3).
        let hi = |label: &str, v: [f64; 3]| {
            gvariant(
                label,
                &[
                    ("B1", "", "ops/s", v[0], Better::Higher),
                    ("B2", "", "ops/s", v[1], Better::Higher),
                    ("B3", "", "ops/s", v[2], Better::Higher),
                ],
            )
        };
        let r = compare_matrix(
            &[hi("a", [100.0; 3]), hi("b", [200.0, 200.0, 50.0])],
            &Options::default(),
            &mopts(),
        );
        let g = &only_family(&r).per_variant[1];
        assert!((g.value.unwrap() - 0.5f64.powf(1.0 / 3.0)).abs() < 1e-12);

        // A zero reference value in 1 of 15 rows: n=14/15, no NaN or inf anywhere.
        let mut base: Vec<_> = (0..15).map(|i| ns(&format!("B{i}"), 100.0)).collect();
        base[7].1 = 0.0;
        let head: Vec<_> = (0..15).map(|i| ns(&format!("B{i}"), 150.0)).collect();
        let r = compare_matrix(
            &[lower_variant("a", &base), lower_variant("b", &head)],
            &Options::default(),
            &mopts(),
        );
        let f = only_family(&r);
        assert_eq!(
            (f.per_variant[1].n_used, f.per_variant[1].n_total),
            (14, 15)
        );
        assert!((f.per_variant[1].value.unwrap() - 1.5).abs() < 1e-12);
        assert!(f
            .headline
            .values
            .iter()
            .all(|v| v.value.is_none_or(f64::is_finite)));
        assert_eq!(f.per_variant[0].n_used, 14);
    }

    #[test]
    fn geomean_should_not_mix_measures_and_hide_family_under_three_rows() {
        let mk = |factor: f64| {
            let mut items: Vec<(String, &str, &str, f64, Better)> = Vec::new();
            for i in 0..10 {
                items.push((format!("T{i}"), "time", "ns", 100.0 * factor, Better::Lower));
            }
            for i in 0..5 {
                items.push((
                    format!("A{i}"),
                    "alloc",
                    "B/op",
                    64.0 * factor,
                    Better::Lower,
                ));
            }
            for i in 0..2 {
                items.push((format!("S{i}"), "tiny", "ms", 1.0, Better::Lower));
            }
            items
        };
        let to_variant = |label: &str, f: f64| {
            let items = mk(f);
            let refs: Vec<(&str, &str, &str, f64, Better)> = items
                .iter()
                .map(|(n, m, u, v, b)| (n.as_str(), *m, *u, *v, *b))
                .collect();
            gvariant(label, &refs)
        };
        let r = compare_matrix(
            &[to_variant("a", 1.0), to_variant("b", 2.0)],
            &Options::default(),
            &mopts(),
        );
        assert_eq!(r.geomean.len(), 2, "the 2-row family is hidden");
        let time = &r.geomean[0];
        let alloc = &r.geomean[1];
        assert_eq!(
            (time.unit.as_str(), time.per_variant[1].n_used, time.total),
            ("ns", 10, 10)
        );
        assert_eq!(
            (
                alloc.unit.as_str(),
                alloc.per_variant[1].n_used,
                alloc.total
            ),
            ("B/op", 5, 5)
        );
        assert!((time.per_variant[1].value.unwrap() - 2.0).abs() < 1e-12);
        assert!((alloc.per_variant[1].value.unwrap() - 2.0).abs() < 1e-12);
        assert_eq!(r.primary_geomean().unwrap().unit, "ns");
    }

    #[test]
    fn primary_geomean_should_fall_back_to_the_largest_family_without_a_time_unit() {
        let mk = |label: &str, f: f64| {
            let mut items: Vec<(String, &str, f64)> = Vec::new();
            for i in 0..3 {
                items.push((format!("A{i}"), "B/op", 10.0 * f));
            }
            for i in 0..4 {
                items.push((format!("C{i}"), "allocs/op", 2.0 * f));
            }
            let refs: Vec<(&str, &str, &str, f64, Better)> = items
                .iter()
                .map(|(n, u, v)| (n.as_str(), *u, *u, *v, Better::Lower))
                .collect();
            gvariant(label, &refs)
        };
        let r = compare_matrix(&[mk("a", 1.0), mk("b", 1.0)], &Options::default(), &mopts());
        assert_eq!(r.primary_geomean().unwrap().unit, "allocs/op");
    }

    #[test]
    fn geomean_headline_should_use_common_row_set_with_per_variant_n_used_of_total_when_variant_misses_rows(
    ) {
        // merge lacks its 6 slowest of 15 benchmarks.
        let names: Vec<String> = (0..15).map(|i| format!("B{i:02}")).collect();
        let quick: Vec<_> = names.iter().map(|n| ns(n, 100.0)).collect();
        let heap: Vec<_> = names.iter().map(|n| ns(n, 200.0)).collect();
        let merge: Vec<_> = names[..9].iter().map(|n| ns(n, 50.0)).collect();
        let r = compare_matrix(
            &[
                lower_variant("quick", &quick),
                lower_variant("heap", &heap),
                lower_variant("merge", &merge),
            ],
            &Options::default(),
            &mopts(),
        );
        let f = only_family(&r);
        assert_eq!(
            (f.per_variant[1].n_used, f.per_variant[1].n_total),
            (15, 15)
        );
        assert_eq!((f.per_variant[2].n_used, f.per_variant[2].n_total), (9, 15));
        let h = &f.headline;
        assert_eq!((h.common_n, h.total, h.comparable), (9, 15, true));
        assert!(h.values.iter().all(|v| (v.n_used, v.n_total) == (9, 9)));
        assert!((h.values[0].value.unwrap() - 1.0).abs() < 1e-12);
        assert!((h.values[1].value.unwrap() - 2.0).abs() < 1e-12);
        assert!((h.values[2].value.unwrap() - 0.5).abs() < 1e-12);
        assert_eq!(h.best, Some(2));
        assert_eq!(h.least_covered, Some(("merge".to_string(), 9)));
    }

    #[test]
    fn geomean_headline_should_be_not_comparable_when_common_set_is_under_three_or_under_half() {
        let names: Vec<String> = (0..15).map(|i| format!("B{i:02}")).collect();
        let quick: Vec<_> = names.iter().map(|n| ns(n, 100.0)).collect();
        let heap: Vec<_> = names.iter().map(|n| ns(n, 200.0)).collect();
        let merge: Vec<_> = names[..4].iter().map(|n| ns(n, 50.0)).collect();
        let r = compare_matrix(
            &[
                lower_variant("quick", &quick),
                lower_variant("heap", &heap),
                lower_variant("merge", &merge),
            ],
            &Options::default(),
            &mopts(),
        );
        let h = &only_family(&r).headline;
        assert_eq!((h.common_n, h.total, h.comparable), (4, 15, false));
        assert_eq!(h.least_covered, Some(("merge".to_string(), 4)));
        assert_eq!(h.best, None);

        // Under 3 rows even though that is more than half of 4.
        let q4: Vec<_> = names[..4].iter().map(|n| ns(n, 100.0)).collect();
        let m2: Vec<_> = names[..2].iter().map(|n| ns(n, 50.0)).collect();
        let r = compare_matrix(
            &[lower_variant("quick", &q4), lower_variant("merge", &m2)],
            &Options::default(),
            &mopts(),
        );
        let h = &only_family(&r).headline;
        assert_eq!((h.common_n, h.total, h.comparable), (2, 4, false));
    }

    #[test]
    fn geomean_should_skip_rows_without_reference_and_exclude_invalid_cells_from_common_set() {
        let names: Vec<String> = (0..5).map(|i| format!("B{i}")).collect();
        let quick: Vec<_> = names[..4].iter().map(|n| ns(n, 100.0)).collect();
        let heap: Vec<_> = names.iter().map(|n| ns(n, 200.0)).collect();
        let mut heap_v = lower_variant("heap", &heap);
        // B1 is present in the reference but unusable in heap.
        heap_v.set.metrics.remove(1);
        heap_v = with_skipped(heap_v, "B1");
        let r = compare_matrix(
            &[lower_variant("quick", &quick), heap_v],
            &Options::default(),
            &mopts(),
        );
        let f = only_family(&r);
        assert_eq!(f.total, 4, "the reference-absent row B4 is excluded");
        assert_eq!((f.per_variant[1].n_used, f.per_variant[1].n_total), (3, 4));
        assert_eq!(f.headline.common_n, 3);
    }
}
