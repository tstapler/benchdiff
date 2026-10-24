//! Comparison of a head run against an optional base run.

use crate::model::{Better, Metric, MetricSet};
use crate::stats::{mann_whitney_p, MIN_SAMPLES};
use crate::units;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// Largest regression first, then largest improvement.
    #[default]
    Delta,
    Name,
    /// The order metrics appear in the head file.
    Input,
}

impl Sort {
    pub fn from_name(name: &str) -> Result<Sort, String> {
        match name.trim().to_ascii_lowercase().as_str() {
            "" | "delta" => Ok(Sort::Delta),
            "name" => Ok(Sort::Name),
            "input" => Ok(Sort::Input),
            other => Err(format!(
                "unknown sort {other:?}; expected delta, name or input"
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    /// A change smaller than this many percent is noise.
    pub threshold_pct: f64,
    /// A change smaller than this many units is noise, whatever the percentage.
    pub min_abs: f64,
    /// Significance level for the Mann-Whitney test.
    pub alpha: f64,
    /// Use the significance test when samples allow; `false` forces threshold-only.
    pub use_stats: bool,
    pub only_changed: bool,
    pub sort: Sort,
    /// Caps the changed rows shown; the rest are summarised.
    pub max_rows: Option<usize>,
    pub title: String,
    /// Distinguishes several reports on one pull request.
    pub tag: String,
    /// `Report::exceeds_failure_threshold` trips on a regression of at least this many percent.
    pub fail_on_regression_pct: Option<f64>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            threshold_pct: 5.0,
            min_abs: 0.0,
            alpha: 0.05,
            use_stats: true,
            only_changed: false,
            sort: Sort::Delta,
            max_rows: None,
            title: "Benchmark comparison".into(),
            tag: "default".into(),
            fail_on_regression_pct: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Improved,
    Regressed,
    Unchanged,
    /// In head only (or no base to compare with).
    New,
    /// In base only.
    Removed,
    /// Present on both sides but in units that cannot be converted.
    Incomparable,
    /// In base, but the head run produced no usable (finite) value for it.
    Invalid,
}

/// How a row's verdict was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    MannWhitney,
    /// Non-overlapping intervals on both sides.
    Interval,
    /// Point estimates only: percentage and absolute floors.
    Threshold,
}

#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub group: String,
    pub name: String,
    pub measure: String,
    pub unit: String,
    pub better: Better,
    pub base: Option<f64>,
    pub head: Option<f64>,
    /// `None` when there is nothing to divide by (new, removed, or base 0).
    pub delta_pct: Option<f64>,
    pub verdict: Verdict,
    pub method: Option<Method>,
    /// Two-sided Mann-Whitney p-value when that test ran.
    pub p_value: Option<f64>,
    pub n_base: usize,
    pub n_head: usize,
    /// Head interval, shown in head-only reports.
    pub head_range: Option<(f64, f64)>,
}

impl Row {
    /// Display label: `name`, plus `[measure]` when the benchmark has one.
    pub fn label(&self) -> String {
        if self.measure.is_empty() {
            self.name.clone()
        } else {
            format!("{} [{}]", self.name, self.measure)
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub title: String,
    pub tag: String,
    pub threshold_pct: f64,
    pub min_abs: f64,
    pub alpha: f64,
    pub use_stats: bool,
    pub only_changed: bool,
    pub max_rows: Option<usize>,
    pub fail_on_regression_pct: Option<f64>,
    pub base_env: Vec<(String, String)>,
    pub head_env: Vec<(String, String)>,
    /// Labels of metrics the importer dropped for lacking a usable value (first 50).
    pub skipped_head: Vec<String>,
    pub skipped_head_total: usize,
    pub skipped_base: Vec<String>,
    pub skipped_base_total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub summary: Summary,
    pub rows: Vec<Row>,
    pub has_base: bool,
}

impl Report {
    pub fn count(&self, v: Verdict) -> usize {
        self.rows.iter().filter(|r| r.verdict == v).count()
    }

    pub fn count_method(&self, m: Method) -> usize {
        self.rows.iter().filter(|r| r.method == Some(m)).count()
    }

    pub fn exceeds_failure_threshold(&self) -> bool {
        let Some(limit) = self.summary.fail_on_regression_pct else {
            return false;
        };
        self.rows.iter().any(|r| {
            r.verdict == Verdict::Invalid
                || (r.verdict == Verdict::Regressed && r.delta_pct.is_none_or(|p| p.abs() >= limit))
        })
    }
}

/// `base` of `None` yields a head-only report with every row `New`.
pub fn compare(base: Option<&MetricSet>, head: &MetricSet, opts: &Options) -> Report {
    let base_by_key: HashMap<_, &Metric> = base
        .map(|b| b.metrics.iter().map(|m| (m.key(), m)).collect())
        .unwrap_or_default();
    let head_keys: HashSet<_> = head.metrics.iter().map(Metric::key).collect();

    let mut rows: Vec<Row> = head
        .metrics
        .iter()
        .map(|h| match base_by_key.get(&h.key()) {
            Some(b) => changed_row(b, h, opts),
            None => one_sided_row(h, Verdict::New),
        })
        .collect();
    if let Some(b) = base {
        rows.extend(
            b.metrics
                .iter()
                .filter(|m| !head_keys.contains(&m.key()))
                .map(|m| {
                    let dropped = head.skipped.contains(m.key());
                    one_sided_row(
                        m,
                        if dropped {
                            Verdict::Invalid
                        } else {
                            Verdict::Removed
                        },
                    )
                }),
        );
    }
    sort_rows(&mut rows, opts.sort);

    let env_of = |s: Option<&MetricSet>| {
        s.map(|s| s.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    };
    Report {
        has_base: base.is_some(),
        summary: Summary {
            title: opts.title.clone(),
            tag: opts.tag.clone(),
            threshold_pct: opts.threshold_pct,
            min_abs: opts.min_abs,
            alpha: opts.alpha,
            use_stats: opts.use_stats,
            only_changed: opts.only_changed,
            max_rows: opts.max_rows,
            fail_on_regression_pct: opts.fail_on_regression_pct,
            base_env: env_of(base),
            head_env: env_of(Some(head)),
            skipped_head: head.skipped.labels(),
            skipped_head_total: head.skipped.total,
            skipped_base: base.map(|b| b.skipped.labels()).unwrap_or_default(),
            skipped_base_total: base.map_or(0, |b| b.skipped.total),
        },
        rows,
    }
}

fn one_sided_row(m: &Metric, verdict: Verdict) -> Row {
    let is_new = verdict == Verdict::New;
    debug_assert!(is_new || matches!(verdict, Verdict::Removed | Verdict::Invalid));
    Row {
        group: m.group.clone(),
        name: m.name.clone(),
        measure: m.measure.clone(),
        unit: m.unit.clone(),
        better: m.better,
        base: (!is_new).then_some(m.value),
        head: is_new.then_some(m.value),
        delta_pct: None,
        verdict,
        method: None,
        p_value: None,
        n_base: if is_new { 0 } else { m.samples.len() },
        n_head: if is_new { m.samples.len() } else { 0 },
        head_range: m.lower.zip(m.upper).filter(|_| is_new),
    }
}

/// Outcome of the significance question for one pair of metrics.
struct Evidence {
    method: Method,
    significant: bool,
    p_value: Option<f64>,
}

fn evidence(base: &Metric, head: &Metric, base_scale: f64, opts: &Options) -> Evidence {
    let threshold_only = Evidence {
        method: Method::Threshold,
        significant: true,
        p_value: None,
    };
    if !opts.use_stats {
        return threshold_only;
    }
    if base.samples.len() >= MIN_SAMPLES && head.samples.len() >= MIN_SAMPLES {
        let scaled: Vec<f64> = base.samples.iter().map(|v| v * base_scale).collect();
        let p = mann_whitney_p(&scaled, &head.samples);
        return Evidence {
            method: Method::MannWhitney,
            significant: p < opts.alpha,
            p_value: Some(p),
        };
    }
    if let (Some(bl), Some(bu), Some(hl), Some(hu)) =
        (base.lower, base.upper, head.lower, head.upper)
    {
        let (bl, bu) = (bl * base_scale, bu * base_scale);
        return Evidence {
            method: Method::Interval,
            significant: bu < hl || hu < bl,
            p_value: None,
        };
    }
    threshold_only
}

fn changed_row(base: &Metric, head: &Metric, opts: &Options) -> Row {
    let mut row = Row {
        group: head.group.clone(),
        name: head.name.clone(),
        measure: head.measure.clone(),
        unit: head.unit.clone(),
        better: head.better,
        base: Some(base.value),
        head: Some(head.value),
        delta_pct: None,
        verdict: Verdict::Incomparable,
        method: None,
        p_value: None,
        n_base: base.samples.len(),
        n_head: head.samples.len(),
        head_range: head.lower.zip(head.upper),
    };
    // Express the base in the head's unit so the delta and the test are on one scale.
    let Some(scale) = units::factor(&base.unit, &head.unit) else {
        return row;
    };
    let base_value = base.value * scale;
    row.base = Some(base_value);

    let diff = head.value - base_value;
    row.delta_pct = (base_value != 0.0).then(|| diff / base_value.abs() * 100.0);
    let ev = evidence(base, head, scale, opts);
    row.method = Some(ev.method);
    row.p_value = ev.p_value;

    let passes_floor = diff != 0.0
        && diff.abs() >= opts.min_abs
        && row.delta_pct.is_none_or(|p| p.abs() >= opts.threshold_pct);
    row.verdict = if !(passes_floor && ev.significant) {
        Verdict::Unchanged
    } else if (diff < 0.0) == (head.better == Better::Lower) {
        Verdict::Improved
    } else {
        Verdict::Regressed
    };
    row
}

fn sort_rows(rows: &mut [Row], sort: Sort) {
    // Stable sorts: ties keep input order.
    match sort {
        Sort::Input => {}
        Sort::Name => rows
            .sort_by(|a, b| (&a.group, &a.name, &a.measure).cmp(&(&b.group, &b.name, &b.measure))),
        Sort::Delta => rows.sort_by(|a, b| {
            let ((ca, ma), (cb, mb)) = (delta_key(a), delta_key(b));
            ca.cmp(&cb).then(mb.total_cmp(&ma))
        }),
    }
}

/// (class, magnitude): regressions, then improvements, then the rest; larger magnitude first.
/// A row with no percentage (zero base) counts as infinitely large.
fn delta_key(r: &Row) -> (u8, f64) {
    let magnitude = r.delta_pct.map_or(f64::INFINITY, f64::abs);
    match r.verdict {
        Verdict::Invalid => (0, f64::INFINITY),
        Verdict::Regressed => (0, magnitude),
        Verdict::Improved => (1, magnitude),
        _ => (2, 0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metric(name: &str, value: f64, better: Better) -> Metric {
        Metric::new("g", name, "", "ms", value, better)
    }

    fn set(items: &[(&str, f64, Better)]) -> MetricSet {
        MetricSet {
            env: Default::default(),
            skipped: Default::default(),
            metrics: items.iter().map(|(n, v, b)| metric(n, *v, *b)).collect(),
        }
    }

    fn verdict_of(base: f64, head: f64, better: Better, opts: &Options) -> Verdict {
        compare(
            Some(&set(&[("m", base, better)])),
            &set(&[("m", head, better)]),
            opts,
        )
        .rows[0]
            .verdict
    }

    fn sampled(values: &[f64], better: Better) -> MetricSet {
        let mut m = metric("m", crate::stats::median(values), better);
        m.samples = values.to_vec();
        MetricSet {
            env: Default::default(),
            skipped: Default::default(),
            metrics: vec![m],
        }
    }

    #[test]
    fn lower_is_better_metric_regresses_when_it_grows_past_the_threshold() {
        let o = Options::default();
        assert_eq!(
            verdict_of(10.0, 11.0, Better::Lower, &o),
            Verdict::Regressed
        );
        assert_eq!(verdict_of(10.0, 9.0, Better::Lower, &o), Verdict::Improved);
    }

    #[test]
    fn higher_is_better_metric_flips_the_direction() {
        let o = Options::default();
        assert_eq!(
            verdict_of(10.0, 11.0, Better::Higher, &o),
            Verdict::Improved
        );
        assert_eq!(
            verdict_of(10.0, 9.0, Better::Higher, &o),
            Verdict::Regressed
        );
    }

    #[test]
    fn change_below_the_threshold_is_unchanged() {
        assert_eq!(
            verdict_of(100.0, 104.9, Better::Lower, &Options::default()),
            Verdict::Unchanged
        );
    }

    #[test]
    fn change_at_exactly_the_threshold_counts() {
        assert_eq!(
            verdict_of(100.0, 105.0, Better::Lower, &Options::default()),
            Verdict::Regressed
        );
    }

    #[test]
    fn large_percentage_but_below_the_absolute_floor_is_unchanged() {
        let o = Options {
            min_abs: 0.05,
            ..Options::default()
        };
        assert_eq!(
            verdict_of(0.02, 0.03, Better::Lower, &o),
            Verdict::Unchanged
        );
        assert_eq!(verdict_of(0.2, 0.3, Better::Lower, &o), Verdict::Regressed);
    }

    #[test]
    fn zero_base_has_no_percentage_but_still_counts_as_a_change() {
        let r = compare(
            Some(&set(&[("m", 0.0, Better::Lower)])),
            &set(&[("m", 1.0, Better::Lower)]),
            &Options::default(),
        );
        assert_eq!(r.rows[0].delta_pct, None);
        assert_eq!(r.rows[0].verdict, Verdict::Regressed);
    }

    #[test]
    fn equal_values_are_unchanged_even_with_zero_threshold() {
        let o = Options {
            threshold_pct: 0.0,
            ..Options::default()
        };
        assert_eq!(verdict_of(3.0, 3.0, Better::Lower, &o), Verdict::Unchanged);
    }

    #[test]
    fn metrics_only_in_one_side_are_new_or_removed() {
        let r = compare(
            Some(&set(&[("old", 1.0, Better::Lower)])),
            &set(&[("fresh", 1.0, Better::Lower)]),
            &Options::default(),
        );
        assert_eq!(r.count(Verdict::New), 1);
        assert_eq!(r.count(Verdict::Removed), 1);
    }

    #[test]
    fn without_a_base_every_row_is_new() {
        let r = compare(
            None,
            &set(&[("a", 1.0, Better::Lower)]),
            &Options::default(),
        );
        assert!(!r.has_base);
        assert_eq!(r.count(Verdict::New), 1);
    }

    #[test]
    fn delta_sort_puts_the_worst_regression_first_and_best_improvement_after_regressions() {
        let base = set(&[
            ("small-reg", 100.0, Better::Lower),
            ("big-reg", 100.0, Better::Lower),
            ("big-imp", 100.0, Better::Lower),
            ("flat", 100.0, Better::Lower),
        ]);
        let head = set(&[
            ("small-reg", 110.0, Better::Lower),
            ("big-reg", 150.0, Better::Lower),
            ("big-imp", 50.0, Better::Lower),
            ("flat", 100.0, Better::Lower),
        ]);
        let r = compare(Some(&base), &head, &Options::default());
        let order: Vec<_> = r.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(order, ["big-reg", "small-reg", "big-imp", "flat"]);
    }

    #[test]
    fn delta_sort_orders_improvements_largest_first_and_keeps_the_rest_in_input_order() {
        let l = Better::Lower;
        let base = set(&[
            ("flat-a", 100.0, l),
            ("small-imp", 100.0, l),
            ("flat-b", 100.0, l),
            ("big-imp", 100.0, l),
        ]);
        let head = set(&[
            ("flat-a", 100.0, l),
            ("small-imp", 90.0, l),
            ("flat-b", 100.0, l),
            ("big-imp", 50.0, l),
        ]);
        let r = compare(Some(&base), &head, &Options::default());
        let order: Vec<_> = r.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(order, ["big-imp", "small-imp", "flat-a", "flat-b"]);
    }

    #[test]
    fn failure_threshold_edge_cases() {
        let l = Better::Lower;
        let trips = |base: f64, head: f64, limit: Option<f64>| {
            compare(
                Some(&set(&[("m", base, l)])),
                &set(&[("m", head, l)]),
                &Options {
                    fail_on_regression_pct: limit,
                    ..Options::default()
                },
            )
            .exceeds_failure_threshold()
        };
        assert!(trips(0.0, 1.0, Some(1000.0)), "zero-base regression trips");
        assert!(!trips(100.0, 50.0, Some(0.0)), "improvement never trips");
        assert!(
            trips(100.0, 110.0, Some(0.0)),
            "bare 0 trips on any regression"
        );

        let mut b = metric("m", 1.0, l);
        b.unit = "B".into();
        let mk = |m| MetricSet {
            env: Default::default(),
            skipped: Default::default(),
            metrics: vec![m],
        };
        let r = compare(
            Some(&mk(b)),
            &mk(metric("m", 2.0, l)),
            &Options {
                fail_on_regression_pct: Some(0.0),
                ..Options::default()
            },
        );
        assert_eq!(r.rows[0].verdict, Verdict::Incomparable);
        assert!(!r.exceeds_failure_threshold());
    }

    #[test]
    fn sampled_base_in_us_vs_head_in_ms_scales_before_the_rank_test() {
        // Base 10-14 us = 0.010-0.014 ms; head 0.02-0.024 ms: a regression once scaled.
        // Unscaled, base (10..14) > head (0.02..) would read as a large improvement.
        let mut base = sampled(&[10.0, 11.0, 12.0, 13.0, 14.0], Better::Lower);
        base.metrics[0].unit = "us".into();
        let head = sampled(&[0.020, 0.021, 0.022, 0.023, 0.024], Better::Lower);
        let r = compare(Some(&base), &head, &Options::default());
        assert_eq!(r.rows[0].method, Some(Method::MannWhitney));
        assert_eq!(r.rows[0].verdict, Verdict::Regressed);
        assert!(r.rows[0].p_value.unwrap() < 0.05);
        assert!((r.rows[0].base.unwrap() - 0.012).abs() < 1e-12);
    }

    #[test]
    fn failure_threshold_trips_only_on_a_large_enough_regression() {
        let base = set(&[("m", 100.0, Better::Lower)]);
        let head = set(&[("m", 120.0, Better::Lower)]);
        let trip = |limit| {
            compare(
                Some(&base),
                &head,
                &Options {
                    fail_on_regression_pct: Some(limit),
                    ..Options::default()
                },
            )
            .exceeds_failure_threshold()
        };
        assert!(trip(15.0));
        assert!(!trip(25.0));
        assert!(!compare(Some(&base), &head, &Options::default()).exceeds_failure_threshold());
    }

    #[test]
    fn differing_time_units_are_converted_before_comparing() {
        let mut b = metric("m", 1_000_000.0, Better::Lower);
        b.unit = "ns".into();
        let mut h = metric("m", 1.5, Better::Lower);
        h.unit = "ms".into();
        let r = compare(
            Some(&MetricSet {
                env: Default::default(),
                skipped: Default::default(),
                metrics: vec![b],
            }),
            &MetricSet {
                env: Default::default(),
                skipped: Default::default(),
                metrics: vec![h],
            },
            &Options::default(),
        );
        assert_eq!(r.rows[0].base, Some(1.0));
        assert_eq!(r.rows[0].verdict, Verdict::Regressed);
        assert!((r.rows[0].delta_pct.unwrap() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn unconvertible_unit_mismatch_is_incomparable() {
        let mut b = metric("m", 1.0, Better::Lower);
        b.unit = "B".into();
        let h = metric("m", 2.0, Better::Lower);
        let r = compare(
            Some(&MetricSet {
                env: Default::default(),
                skipped: Default::default(),
                metrics: vec![b],
            }),
            &MetricSet {
                env: Default::default(),
                skipped: Default::default(),
                metrics: vec![h],
            },
            &Options::default(),
        );
        assert_eq!(r.rows[0].verdict, Verdict::Incomparable);
    }

    #[test]
    fn samples_that_overlap_make_a_large_median_shift_unchanged() {
        // Medians differ by ~30% but the distributions are interleaved: not significant.
        let base = sampled(&[10.0, 11.0, 30.0, 12.0, 10.5, 29.0], Better::Lower);
        let head = sampled(&[13.0, 14.0, 31.0, 15.0, 13.5, 28.0], Better::Lower);
        let r = compare(Some(&base), &head, &Options::default());
        assert_eq!(r.rows[0].method, Some(Method::MannWhitney));
        assert_eq!(
            r.rows[0].verdict,
            Verdict::Unchanged,
            "p={:?}",
            r.rows[0].p_value
        );
    }

    #[test]
    fn separated_samples_regress_with_a_p_value() {
        let base = sampled(&[10.0, 10.1, 10.2, 10.3, 10.4], Better::Lower);
        let head = sampled(&[12.0, 12.1, 12.2, 12.3, 12.4], Better::Lower);
        let r = compare(Some(&base), &head, &Options::default());
        assert_eq!(r.rows[0].verdict, Verdict::Regressed);
        assert!(r.rows[0].p_value.unwrap() < 0.05);
    }

    #[test]
    fn significant_but_tiny_change_is_still_filtered_by_the_threshold() {
        let base = sampled(&[100.0, 100.1, 100.2, 100.3, 100.4], Better::Lower);
        let head = sampled(&[101.0, 101.1, 101.2, 101.3, 101.4], Better::Lower);
        let r = compare(Some(&base), &head, &Options::default());
        assert_eq!(r.rows[0].verdict, Verdict::Unchanged);
        let r = compare(
            Some(&base),
            &head,
            &Options {
                threshold_pct: 0.0,
                ..Options::default()
            },
        );
        assert_eq!(r.rows[0].verdict, Verdict::Regressed);
    }

    #[test]
    fn three_samples_per_side_fall_back_to_the_threshold() {
        let base = sampled(&[10.0, 10.1, 10.2], Better::Lower);
        let head = sampled(&[12.0, 12.1, 12.2], Better::Lower);
        let r = compare(Some(&base), &head, &Options::default());
        assert_eq!(r.rows[0].method, Some(Method::Threshold));
        assert_eq!(r.rows[0].verdict, Verdict::Regressed);
    }

    #[test]
    fn no_stats_option_forces_the_threshold_method() {
        let base = sampled(&[10.0, 11.0, 30.0, 12.0, 10.5, 29.0], Better::Lower);
        let head = sampled(&[13.0, 14.0, 31.0, 15.0, 13.5, 28.0], Better::Lower);
        let r = compare(
            Some(&base),
            &head,
            &Options {
                use_stats: false,
                ..Options::default()
            },
        );
        assert_eq!(r.rows[0].method, Some(Method::Threshold));
        assert_eq!(r.rows[0].verdict, Verdict::Regressed);
    }

    #[test]
    fn overlapping_intervals_without_samples_are_not_significant() {
        let mut b = metric("m", 100.0, Better::Lower);
        (b.lower, b.upper) = (Some(80.0), Some(120.0));
        let mut h = metric("m", 130.0, Better::Lower);
        (h.lower, h.upper) = (Some(110.0), Some(150.0));
        let mk = |m| MetricSet {
            env: Default::default(),
            skipped: Default::default(),
            metrics: vec![m],
        };
        let r = compare(Some(&mk(b.clone())), &mk(h.clone()), &Options::default());
        assert_eq!(
            (r.rows[0].method, r.rows[0].verdict),
            (Some(Method::Interval), Verdict::Unchanged)
        );
        (h.lower, h.upper) = (Some(125.0), Some(135.0));
        (b.lower, b.upper) = (Some(95.0), Some(105.0));
        let r = compare(Some(&mk(b)), &mk(h), &Options::default());
        assert_eq!(r.rows[0].verdict, Verdict::Regressed);
    }

    #[test]
    fn measures_of_one_benchmark_are_compared_independently() {
        let mk = |ns: f64, bytes: f64| {
            let mut a = Metric::new("g", "b", "ns/op", "ns/op", ns, Better::Lower);
            let mut b = Metric::new("g", "b", "B/op", "B/op", bytes, Better::Lower);
            a.samples.clear();
            b.samples.clear();
            MetricSet {
                env: Default::default(),
                skipped: Default::default(),
                metrics: vec![a, b],
            }
        };
        let r = compare(Some(&mk(100.0, 8.0)), &mk(100.0, 16.0), &Options::default());
        assert_eq!(r.count(Verdict::Regressed), 1);
        assert_eq!(r.count(Verdict::Unchanged), 1);
    }
}
