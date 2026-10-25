//! Importers for the JSON-shaped tools. Specs and the fixtures' provenance are in `docs/research.md`.

use crate::model::{Better, Metric, MetricSet, Skipped};
use crate::stats::median;
use serde_json::Value;

/// A finite JSON number. JMH writes non-finite values as the strings "NaN", "-INF",
/// "+INF"; those (and anything else) yield `None`, so no NaN reaches `validate`.
fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64().filter(|x| x.is_finite()),
        _ => None,
    }
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn finite_samples(values: impl Iterator<Item = f64>) -> Vec<f64> {
    values.filter(|v| v.is_finite()).collect()
}

// ---------------------------------------------------------------- cargo-criterion

/// `cargo criterion --message-format=json`: one JSON object per line; only `benchmark-complete` matters.
pub(super) fn criterion(text: &str) -> Result<MetricSet, String> {
    let mut metrics = Vec::new();
    let mut skipped = Skipped::default();
    for (i, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with('{'))
    {
        let v: Value =
            serde_json::from_str(line).map_err(|e| format!("criterion line {}: {e}", i + 1))?;
        if str_of(&v, "reason") != "benchmark-complete" {
            continue;
        }
        let id = str_of(&v, "id");
        let (group, name) = id.split_once('/').unwrap_or(("", id));
        let typical = v
            .get("typical")
            .ok_or_else(|| format!("criterion line {}: no `typical` estimate", i + 1))?;
        let unit = str_of(typical, "unit");
        let Some(estimate) = typical.get("estimate").and_then(num) else {
            skipped.push(group, name, "time");
            continue;
        };
        let mut m = Metric::new(group, name, "time", unit, estimate, Better::Lower);
        m.lower = typical.get("lower_bound").and_then(num);
        m.upper = typical.get("upper_bound").and_then(num);
        // Each sample measures `iteration_count[i]` iterations in `measured_values[i]` (in `unit`).
        let (iters, totals) = (
            v.get("iteration_count").and_then(Value::as_array),
            v.get("measured_values").and_then(Value::as_array),
        );
        if let (Some(iters), Some(totals)) = (iters, totals) {
            m.samples = finite_samples(
                iters
                    .iter()
                    .zip(totals)
                    .filter_map(|(n, t)| Some(num(t)? / num(n).filter(|n| *n > 0.0)?)),
            );
        }
        metrics.push(m);
    }
    if metrics.is_empty() {
        return Err(
            "no `benchmark-complete` messages found; run `cargo criterion --message-format=json`"
                .into(),
        );
    }
    Ok(MetricSet {
        env: Default::default(),
        metrics,
        skipped,
    })
}

// ---------------------------------------------------------------- cargo bench (libtest)

/// Rust's built-in harness: `test name ... bench:   1,234 ns/iter (+/- 56)`. The `+/-` is
/// max - min, not a confidence interval, so it is deliberately not turned into bounds.
pub(super) fn cargo_bench(text: &str) -> Result<MetricSet, String> {
    let mut metrics = Vec::new();
    let mut skipped = Skipped::default();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("test ") else {
            continue;
        };
        let Some((name, tail)) = rest.split_once(" ... bench:") else {
            continue;
        };
        let mut words = tail.split_whitespace();
        let (group, bench) = name.trim().rsplit_once("::").unwrap_or(("", name.trim()));
        let Some(value) = words
            .next()
            .and_then(|w| w.replace(',', "").parse::<f64>().ok())
            .filter(|v| v.is_finite())
        else {
            skipped.push(group, bench, "time");
            continue;
        };
        let unit = words.next().unwrap_or("ns/iter");
        metrics.push(Metric::new(
            group,
            bench,
            "time",
            unit,
            value,
            Better::Lower,
        ));
    }
    if metrics.is_empty() {
        return Err("no `test x ... bench:` lines found in `cargo bench` output".into());
    }
    Ok(MetricSet {
        env: Default::default(),
        metrics,
        skipped,
    })
}

// ---------------------------------------------------------------- JMH / kotlinx-benchmark

pub(super) fn jmh(text: &str) -> Result<MetricSet, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("JMH JSON: {e}"))?;
    let runs = v.as_array().ok_or("JMH JSON must be a top-level array")?;
    let mut env = std::collections::BTreeMap::new();
    let mut metrics = Vec::new();
    let mut skipped = Skipped::default();
    for run in runs {
        let full = str_of(run, "benchmark");
        let (group, name) = full.rsplit_once('.').unwrap_or(("", full));
        let mode = str_of(run, "mode");
        let better = if mode == "thrpt" {
            Better::Higher
        } else {
            Better::Lower
        };
        let params = run
            .get("params")
            .and_then(Value::as_object)
            .map(|p| {
                p.iter()
                    .map(|(k, v)| format!("{k}={}", v.as_str().unwrap_or(&v.to_string())))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        let name = if params.is_empty() {
            name.to_string()
        } else {
            format!("{name}[{params}]")
        };
        if let Some(pm) = run.get("primaryMetric") {
            match jmh_metric(group, &name, mode, better, pm) {
                Some(m) => metrics.push(m),
                None => skipped.push(group, &name, mode),
            }
        }
        if let Some(secondary) = run.get("secondaryMetrics").and_then(Value::as_object) {
            for (label, sm) in secondary {
                match jmh_metric(group, &name, label, Better::Lower, sm) {
                    Some(m) => metrics.push(m),
                    None => skipped.push(group, &name, label),
                }
            }
        }
        for (key, field) in [("jvm", "jdkVersion"), ("vm", "vmName")] {
            if let Some(s) = run.get(field).and_then(Value::as_str) {
                env.entry(key.to_string()).or_insert_with(|| s.to_string());
            }
        }
    }
    if metrics.is_empty() {
        return Err("JMH JSON contains no benchmarks".into());
    }
    Ok(MetricSet {
        env,
        metrics,
        skipped,
    })
}

/// `None` when the score is missing or non-finite (e.g. `"NaN"`), so one bad metric is skipped.
fn jmh_metric(
    group: &str,
    name: &str,
    measure: &str,
    better: Better,
    pm: &Value,
) -> Option<Metric> {
    let score = pm.get("score").and_then(num)?;
    let mut m = Metric::new(group, name, measure, str_of(pm, "scoreUnit"), score, better);
    if let Some(ci) = pm.get("scoreConfidence").and_then(Value::as_array) {
        m.lower = ci.first().and_then(num);
        m.upper = ci.get(1).and_then(num);
    }
    // rawData is [fork][iteration]; iterations of one fork are not independent of each
    // other, so this is generous evidence, but it is what benchstat-style tests can use.
    if let Some(forks) = pm.get("rawData").and_then(Value::as_array) {
        m.samples = finite_samples(
            forks
                .iter()
                .filter_map(Value::as_array)
                .flatten()
                .filter_map(num),
        );
    }
    Some(m)
}

// ---------------------------------------------------------------- hyperfine

pub(super) fn hyperfine(text: &str) -> Result<MetricSet, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("hyperfine JSON: {e}"))?;
    let results = v
        .get("results")
        .and_then(Value::as_array)
        .ok_or("hyperfine JSON has no `results` array")?;
    let mut env = std::collections::BTreeMap::new();
    if let Some(ver) = v
        .pointer("/metadata/hyperfine_version")
        .and_then(Value::as_str)
    {
        env.insert("hyperfine".into(), ver.into());
    }
    let mut metrics = Vec::new();
    let mut skipped = Skipped::default();
    for r in results {
        let command = str_of(r, "command");
        if let Some(summary) = r.get("summary").and_then(Value::as_object) {
            // hyperfine >= 2: per-metric summaries plus per-run `measurements`.
            for (metric, s) in summary {
                if metric.starts_with("time_") || metric == "memory_peak_resident" {
                    match hyperfine_v2(command, metric, s, r) {
                        Some(m) => metrics.push(m),
                        None => skipped.push("", command, metric),
                    }
                }
            }
        } else if let Some(median_s) = r.get("median").and_then(num) {
            // hyperfine 1.x: seconds, `times` holds the wall-clock runs. Same measure name as v2
            // so v1 and v2 files compare.
            let mut m = Metric::new("", command, "time_wall_clock", "s", median_s, Better::Lower);
            m.samples = finite_samples(
                r.get("times")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(num),
            );
            metrics.push(m);
        } else if r.get("median").is_some() {
            skipped.push("", command, "time_wall_clock");
        }
    }
    if metrics.is_empty() {
        return Err("hyperfine JSON contains no usable results".into());
    }
    Ok(MetricSet {
        env,
        metrics,
        skipped,
    })
}

/// `None` when neither the summary median nor the samples give a finite value.
fn hyperfine_v2(command: &str, metric: &str, summary: &Value, result: &Value) -> Option<Metric> {
    let unit = match str_of(summary, "unit") {
        "second" => "s",
        "byte" => "B",
        other => other,
    };
    let samples = finite_samples(
        result
            .get("measurements")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|run| run.pointer(&format!("/{metric}/value")).and_then(num)),
    );
    let value = if samples.len() > 1 {
        median(&samples)
    } else {
        summary.get("median").and_then(num)?
    };
    let mut m = Metric::new("", command, metric, unit, value, Better::Lower);
    m.samples = samples;
    Some(m)
}

// ---------------------------------------------------------------- Google Benchmark

pub(super) fn google_bench(text: &str) -> Result<MetricSet, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("Google Benchmark JSON: {e}"))?;
    let entries = v
        .get("benchmarks")
        .and_then(Value::as_array)
        .ok_or("no `benchmarks` array")?;
    let mut env = std::collections::BTreeMap::new();
    for key in ["host_name", "num_cpus", "mhz_per_cpu", "library_build_type"] {
        if let Some(x) = v.pointer(&format!("/context/{key}")) {
            env.insert(
                key.to_string(),
                x.as_str().map_or_else(|| x.to_string(), str::to_string),
            );
        }
    }
    // Repetitions arrive as repeated "iteration" entries; aggregates (mean/median/stddev/cv) are
    // derived from them and would double-count, so only raw iterations are kept.
    let mut skipped = Skipped::default();
    let mut order: Vec<String> = Vec::new();
    let mut by_name: std::collections::HashMap<String, (Metric, Vec<f64>)> =
        std::collections::HashMap::new();
    for e in entries
        .iter()
        .filter(|e| str_of(e, "run_type") != "aggregate")
    {
        let name = str_of(e, "run_name").to_string();
        let name = if name.is_empty() {
            str_of(e, "name").to_string()
        } else {
            name
        };
        let unit = str_of(e, "time_unit");
        let unit = if unit.is_empty() { "ns" } else { unit };
        let Some(real) = e.get("real_time").and_then(num) else {
            skipped.push("", &name, "real_time");
            continue;
        };
        let entry = by_name.entry(name.clone()).or_insert_with(|| {
            order.push(name.clone());
            (
                Metric::new("", &name, "real_time", unit, real, Better::Lower),
                Vec::new(),
            )
        });
        entry.1.push(real);
    }
    let mut metrics = Vec::new();
    for name in order {
        if let Some((mut m, reals)) = by_name.remove(&name) {
            m.value = median(&reals);
            if reals.len() > 1 {
                m.samples = reals;
            }
            metrics.push(m);
        }
    }
    if metrics.is_empty() {
        return Err("Google Benchmark JSON contains no iteration results".into());
    }
    Ok(MetricSet {
        env,
        metrics,
        skipped,
    })
}

// ---------------------------------------------------------------- Bencher Metric Format

/// BMF carries neither unit nor direction, so direction is guessed from the measure name.
pub(super) fn bmf(text: &str) -> Result<MetricSet, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("BMF JSON: {e}"))?;
    let benches = v
        .as_object()
        .ok_or("BMF must be a JSON object of benchmarks")?;
    let mut metrics = Vec::new();
    let mut skipped = Skipped::default();
    for (bench, measures) in benches {
        let measures = measures
            .as_object()
            .ok_or_else(|| format!("BMF benchmark {bench:?} must be an object of measures"))?;
        for (measure, leaf) in measures {
            let (group, name) = bench.rsplit_once("::").unwrap_or(("", bench));
            let Some(value) = leaf.get("value").and_then(num) else {
                skipped.push(group, name, measure);
                continue;
            };
            let mut m = Metric::new(group, name, measure, "", value, bmf_direction(measure));
            m.lower = leaf.get("lower_value").and_then(num);
            m.upper = leaf.get("upper_value").and_then(num);
            metrics.push(m);
        }
    }
    if metrics.is_empty() {
        return Err("BMF JSON contains no benchmarks".into());
    }
    Ok(MetricSet {
        env: Default::default(),
        metrics,
        skipped,
    })
}

fn bmf_direction(measure: &str) -> Better {
    let m = measure.to_ascii_lowercase();
    if [
        "throughput",
        "ops",
        "per_second",
        "per-second",
        "rps",
        "qps",
        "score",
        "speedup",
    ]
    .iter()
    .any(|k| m.contains(k))
    {
        Better::Higher
    } else {
        Better::Lower
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/../../testdata/{path}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[test]
    fn criterion_should_split_group_and_derive_per_iteration_samples() {
        let set = criterion(&fixture("criterion/criterion.jsonl")).unwrap();
        let m = set
            .metrics
            .iter()
            .find(|m| m.group == "sort" && m.name == "ints")
            .unwrap();
        assert_eq!(m.unit, "ns");
        assert_eq!(m.samples.len(), 100);
        assert!(m.lower.unwrap() < m.value && m.value < m.upper.unwrap());
        // Samples are per-iteration times near the estimate, not raw totals.
        assert!(
            (m.samples[50] / m.value - 1.0).abs() < 0.2,
            "{} vs {}",
            m.samples[50],
            m.value
        );
        assert!(set
            .metrics
            .iter()
            .any(|m| m.group.is_empty() && m.name == "join"));
    }

    #[test]
    fn criterion_without_complete_messages_is_an_error() {
        assert!(criterion(r#"{"reason":"group-complete"}"#)
            .unwrap_err()
            .contains("message-format"));
    }

    #[test]
    fn cargo_bench_should_parse_libtest_lines_with_thousands_separators() {
        let set = cargo_bench(&fixture("cargo-bench/bench.txt")).unwrap();
        assert!(set.metrics.len() >= 2);
        let m = &set.metrics[0];
        assert_eq!(m.unit, "ns/iter");
        assert!(m.value > 0.0 && m.samples.is_empty() && m.lower.is_none());
    }

    #[test]
    fn cargo_bench_should_split_module_path_into_group() {
        let set = cargo_bench("test sort::ints ... bench:       1,234 ns/iter (+/- 56)\n").unwrap();
        assert_eq!(
            (
                set.metrics[0].group.as_str(),
                set.metrics[0].name.as_str(),
                set.metrics[0].value
            ),
            ("sort", "ints", 1234.0)
        );
    }

    #[test]
    fn jmh_should_read_scores_intervals_raw_data_and_direction_from_mode() {
        let set = jmh(&fixture("jmh/jmh.json")).unwrap();
        let thr = set.metrics.iter().find(|m| m.measure == "thrpt").unwrap();
        assert_eq!(thr.better, Better::Higher);
        assert_eq!(thr.group, "dev.stapler.stelekit.benchmarks.SearchBenchmark");
        assert!(thr.lower.is_some() && thr.upper.is_some());
        assert_eq!(thr.samples.len(), 6);
        let avg = set.metrics.iter().find(|m| m.measure == "avgt").unwrap();
        assert_eq!(avg.better, Better::Lower);
        assert_eq!(avg.unit, "ns/op");
    }

    #[test]
    fn jmh_params_should_disambiguate_parameterised_benchmarks() {
        let set = jmh(&fixture("jmh/jmh.json")).unwrap();
        assert!(set.metrics.iter().any(|m| m.name.contains("[size=1000]")));
    }

    #[test]
    fn hyperfine_v2_should_use_measurements_as_samples() {
        let set = hyperfine(&fixture("hyperfine/hyperfine.json")).unwrap();
        let wall = set
            .metrics
            .iter()
            .find(|m| m.name == "true" && m.measure == "time_wall_clock")
            .unwrap();
        assert_eq!(wall.unit, "s");
        assert_eq!(wall.samples.len(), 10);
        assert_eq!(set.env["hyperfine"], "2.0.0");
        assert!(set
            .metrics
            .iter()
            .any(|m| m.measure == "memory_peak_resident" && m.unit == "B"));
    }

    #[test]
    fn hyperfine_v1_should_use_times_as_samples() {
        let set = hyperfine(&fixture("hyperfine/hyperfine-v1.json")).unwrap();
        assert_eq!(set.metrics.len(), 1);
        assert_eq!(set.metrics[0].samples.len(), 5);
        assert_eq!(set.metrics[0].measure, "time_wall_clock");
    }

    const JMH_NAN_SECONDARY: &str = r#"[{"benchmark":"a.B.run","mode":"avgt",
        "primaryMetric":{"score":1.5,"scoreUnit":"ns/op","scoreConfidence":["NaN","NaN"],"rawData":[[1.0,"NaN"]]},
        "secondaryMetrics":{"gc":{"score":"NaN","scoreUnit":"MB/s"}}}]"#;

    #[test]
    fn jmh_secondary_metric_with_nan_score_is_skipped_and_the_rest_parse() {
        let set = jmh(JMH_NAN_SECONDARY).unwrap();
        assert_eq!(set.metrics.len(), 1);
        assert_eq!(set.metrics[0].measure, "avgt");
        assert!(set.validate().is_ok());
    }

    #[test]
    fn jmh_non_finite_confidence_becomes_no_interval() {
        let m = &jmh(JMH_NAN_SECONDARY).unwrap().metrics[0];
        assert_eq!((m.lower, m.upper), (None, None));
        assert_eq!(m.samples, vec![1.0]);
    }

    #[test]
    fn a_file_where_every_metric_is_skipped_reports_no_benchmarks() {
        let err = jmh(r#"[{"benchmark":"a.B.run","mode":"avgt","primaryMetric":{"score":"NaN"}}]"#)
            .unwrap_err();
        assert!(err.contains("no benchmarks"), "{err}");
        let err = bmf(r#"{"b":{"latency":{"value":"x"}}}"#).unwrap_err();
        assert!(err.contains("no benchmarks"), "{err}");
    }

    #[test]
    fn google_bench_should_keep_iterations_and_drop_aggregates() {
        let set = google_bench(&fixture("googlebench/googlebench.json")).unwrap();
        let m = set
            .metrics
            .iter()
            .find(|m| m.name == "BM_Sort/1024")
            .unwrap();
        assert_eq!(m.samples.len(), 3);
        assert_eq!(m.unit, "ns");
        assert_eq!(set.metrics.len(), 2);
    }

    #[test]
    fn bmf_should_flatten_benchmark_and_measure_and_guess_direction() {
        let set = bmf(&fixture("bmf/bmf.json")).unwrap();
        let lat = set.metrics.iter().find(|m| m.measure == "latency").unwrap();
        assert_eq!((lat.lower, lat.upper), (Some(87.42), Some(88.88)));
        assert_eq!(lat.better, Better::Lower);
        assert_eq!(
            set.metrics
                .iter()
                .find(|m| m.measure == "throughput")
                .unwrap()
                .better,
            Better::Higher
        );
    }
}
