//! N-way projection: split a [`Dimensioned`] set into [`Variant`]s along an axis.

use std::collections::HashMap;
use std::fmt;

use crate::dimensioned::Dimensioned;
use crate::dimensions::{axis_value, row_dims, GROUP_AXIS, MEASURE_AXIS};
use crate::model::{Dims, Metric, MetricSet, SkippedKey};
use crate::notice::{Notice, NoticeClass};

/// Most benchmark names listed in a "no such dimension" notice.
const MAX_LISTED: usize = 10;

/// Validated variant label: `^[A-Za-z0-9._-]{1,40}$`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

    let mut variants = Vec::with_capacity(order.len());
    for value in order {
        let label = VariantLabel::parse(&value)
            .map_err(|e| MatrixError(format!("axis '{axis}' value {e}")))?;
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
    Ok(Projection { variants, notices })
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
    fn project_by_axis_should_error_when_axis_value_is_not_a_valid_label() {
        let d = dimensioned(&["Sort/impl=a_b/size=1"]);
        assert!(project_by_axis(&d, "size", None).is_ok());
        let d = dimensioned(&["Sort[impl=a b]"]);
        let err = project_by_axis(&d, "impl", None).unwrap_err();
        assert!(err.to_string().contains("'a b'"), "{err}");
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
}
