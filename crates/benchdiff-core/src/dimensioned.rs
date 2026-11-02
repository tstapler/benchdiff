//! A metric set whose benchmark names were split into dimensions and a residual name.

use std::collections::HashSet;

use serde::Serialize;

use crate::dimensions::GROUP_AXIS;
use crate::model::{Dims, Metric, MetricSet, Skipped};

/// Maximum number of distinct values listed per dimension.
const MAX_SUMMARY_VALUES: usize = 20;

/// Benchmark name after the extracted dimension spans are removed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Residual(String);

impl Residual {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One detected dimension (or the `group` pseudo-dimension) with what it takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DimensionSummary {
    pub name: String,
    /// First-seen distinct values, at most 20.
    pub values: Vec<String>,
    /// Number of distinct values, uncapped.
    pub value_count: usize,
    /// Number of metrics carrying the dimension.
    pub benchmarks: usize,
}

/// An importer-dropped metric attributed to its dimensions and row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkippedEntry<'a> {
    pub group: &'a str,
    pub name: &'a str,
    pub measure: &'a str,
    pub dims: &'a Dims,
    pub residual: &'a str,
}

/// Each metric paired with its residual, so a filter or reorder cannot desync them.
///
/// Construct-only: fields are private and the sole constructors are [`Dimensioned::identity`]
/// and the crate-private `from_parts`.
///
/// ```compile_fail
/// let _ = benchdiff_core::dimensioned::Dimensioned::from_parts(vec![], Default::default());
/// ```
#[derive(Debug, Clone, Default)]
pub struct Dimensioned {
    entries: Vec<(Metric, Residual)>,
    skipped: Skipped,
}

impl Dimensioned {
    pub(crate) fn from_parts(entries: Vec<(Metric, Residual)>, skipped: Skipped) -> Self {
        Self { entries, skipped }
    }

    /// Residual = name, no dimensions: what a labelled file gets when no rules are given.
    pub fn identity(set: MetricSet) -> Self {
        let entries = set
            .metrics
            .into_iter()
            .map(|m| {
                let residual = Residual::new(m.name.clone());
                (m, residual)
            })
            .collect();
        Self {
            entries,
            skipped: set.skipped,
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = (&Metric, &Residual)> {
        self.entries.iter().map(|(m, r)| (m, r))
    }

    /// All entries of `parts` in order, for summaries over several files. Skipped keys are not
    /// carried: a union is only ever summarized, never compared.
    pub fn concat(parts: impl IntoIterator<Item = Dimensioned>) -> Self {
        Self {
            entries: parts.into_iter().flat_map(|d| d.entries).collect(),
            skipped: Skipped::default(),
        }
    }

    /// Skipped keys with the dimensions and residual extracted from their names.
    pub fn skipped_entries(&self) -> impl Iterator<Item = SkippedEntry<'_>> {
        self.skipped.keys().iter().map(|k| SkippedEntry {
            group: &k.group,
            name: &k.name,
            measure: &k.measure,
            dims: &k.dims,
            residual: k.residual.as_deref().unwrap_or(&k.name),
        })
    }

    /// Full count of skipped metrics, including those past the stored cap.
    pub fn skipped_total(&self) -> usize {
        self.skipped.total
    }

    /// Metrics that carry no dimension at all.
    pub fn unmatched(&self) -> usize {
        self.entries
            .iter()
            .filter(|(m, _)| m.dims.is_empty())
            .count()
    }

    /// Detected dimensions in first-seen order, then the `group` pseudo-dimension.
    pub fn dimension_summary(&self) -> Vec<DimensionSummary> {
        let mut out: Vec<DimensionSummary> = Vec::new();
        let mut seen: Vec<HashSet<&str>> = Vec::new();
        for (metric, _) in &self.entries {
            for (key, value) in &metric.dims {
                let i = match out.iter().position(|d| &d.name == key) {
                    Some(i) => i,
                    None => {
                        out.push(blank(key));
                        seen.push(HashSet::new());
                        out.len() - 1
                    }
                };
                tally(&mut out[i], &mut seen[i], value);
            }
        }
        let mut group = blank(GROUP_AXIS);
        let mut group_seen = HashSet::new();
        for (metric, _) in &self.entries {
            tally(&mut group, &mut group_seen, &metric.group);
        }
        out.push(group);
        out
    }

    /// The `available dimensions: ...` suffix used by every axis error.
    pub fn available_dimensions_text(&self) -> String {
        let summary = self.dimension_summary();
        let detected: Vec<String> = summary
            .iter()
            .filter(|d| d.name != GROUP_AXIS)
            .map(|d| {
                let noun = if d.value_count == 1 {
                    "value"
                } else {
                    "values"
                };
                format!("{} ({} {noun})", d.name, d.value_count)
            })
            .collect();
        if detected.is_empty() {
            "available dimensions: group only (default rules read /key=value segments and \
             [k=v] groups; add a 'dimensions' rule or use labelled variants)"
                .to_string()
        } else {
            format!("available dimensions: {}, group", detected.join(", "))
        }
    }
}

fn blank(name: &str) -> DimensionSummary {
    DimensionSummary {
        name: name.to_string(),
        values: Vec::new(),
        value_count: 0,
        benchmarks: 0,
    }
}

fn tally<'a>(dim: &mut DimensionSummary, seen: &mut HashSet<&'a str>, value: &'a str) {
    dim.benchmarks += 1;
    if seen.insert(value) {
        dim.value_count += 1;
        if dim.values.len() < MAX_SUMMARY_VALUES {
            dim.values.push(value.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Dims;

    fn metric(group: &str, name: &str, dims: &[(&str, &str)]) -> Metric {
        let d: Dims = dims
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Metric::new(group, name, "", "ns", 1.0, crate::Better::Lower).with_dims(d)
    }

    fn built(metrics: Vec<(Metric, &str)>) -> Dimensioned {
        Dimensioned::from_parts(
            metrics
                .into_iter()
                .map(|(m, r)| (m, Residual::new(r)))
                .collect(),
            Skipped::default(),
        )
    }

    fn sort_set() -> Dimensioned {
        built(vec![
            (
                metric(
                    "",
                    "Sort/impl=quick/size=10",
                    &[("impl", "quick"), ("size", "10")],
                ),
                "Sort",
            ),
            (
                metric(
                    "",
                    "Sort/impl=heap/size=10",
                    &[("impl", "heap"), ("size", "10")],
                ),
                "Sort",
            ),
            (metric("", "Setup", &[]), "Setup"),
        ])
    }

    #[test]
    fn identity_should_pair_each_metric_with_its_own_name_and_keep_skipped() {
        let mut set = MetricSet {
            metrics: vec![metric("g", "A", &[]), metric("g", "B", &[])],
            ..Default::default()
        };
        set.skipped.push("g", "C", "");
        let d = Dimensioned::identity(set);
        let pairs: Vec<_> = d
            .entries()
            .map(|(m, r)| (m.name.as_str(), r.as_str()))
            .collect();
        assert_eq!(pairs, vec![("A", "A"), ("B", "B")]);
        assert_eq!(d.skipped_total(), 1);
        let e = d.skipped_entries().next().unwrap();
        assert_eq!((e.residual, e.dims.is_empty()), ("C", true));
    }

    #[test]
    fn entries_should_yield_each_metric_with_its_own_residual() {
        let d = sort_set();
        let pairs: Vec<_> = d
            .entries()
            .map(|(m, r)| (m.name.as_str(), r.as_str()))
            .collect();
        assert_eq!(pairs[0], ("Sort/impl=quick/size=10", "Sort"));
        assert_eq!(pairs[2], ("Setup", "Setup"));
    }

    #[test]
    fn dimension_summary_should_list_dims_first_seen_values_and_counts_plus_group_pseudo_dimension()
    {
        let s = sort_set().dimension_summary();
        let names: Vec<_> = s.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["impl", "size", "group"]);
        assert_eq!(s[0].values, vec!["quick", "heap"]);
        assert_eq!(s[0].benchmarks, 2);
        assert_eq!(s[1].values, vec!["10"]);
        assert_eq!(s[1].benchmarks, 2);
        assert_eq!(s[2].benchmarks, 3);
    }

    #[test]
    fn dimension_summary_should_cap_values_at_twenty_but_count_all() {
        let d = built(
            (0..25)
                .map(|i| {
                    let v = i.to_string();
                    (metric("", "B", &[("n", v.as_str())]), "B")
                })
                .collect(),
        );
        let n = &d.dimension_summary()[0];
        assert_eq!((n.values.len(), n.value_count, n.benchmarks), (20, 25, 25));
        assert_eq!(n.values[0], "0");
    }

    #[test]
    fn available_dimensions_text_should_render_counts_and_group() {
        assert_eq!(
            sort_set().available_dimensions_text(),
            "available dimensions: impl (2 values), size (1 value), group"
        );
    }

    #[test]
    fn available_dimensions_text_should_say_group_only_when_no_dims_detected() {
        let d = built(vec![(metric("", "Setup", &[]), "Setup")]);
        assert_eq!(
            d.available_dimensions_text(),
            "available dimensions: group only (default rules read /key=value segments and \
             [k=v] groups; add a 'dimensions' rule or use labelled variants)"
        );
        assert_eq!(d.unmatched(), 1);
    }

    #[test]
    fn unmatched_should_count_metrics_with_empty_dims() {
        assert_eq!(sort_set().unmatched(), 1);
    }
}
