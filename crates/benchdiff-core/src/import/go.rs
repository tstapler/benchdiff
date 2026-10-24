//! Go benchmark format (golang.org/x/perf benchfmt):
//! <https://go.googlesource.com/proposal/+/master/design/14313-benchmark-format.md>

use crate::model::{Better, Metric, MetricSet, Skipped};
use crate::stats::median;
use std::collections::{BTreeSet, HashMap};

pub(super) fn looks_like_go(text: &str) -> bool {
    text.lines().any(|l| parse_result_line(l).is_some())
}

/// `(name, [(value, unit)])` for a result line, or `None` for anything else.
fn parse_result_line(line: &str) -> Option<(&str, Vec<(f64, &str)>)> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 4 || !fields.len().is_multiple_of(2) {
        return None;
    }
    let rest = fields[0].strip_prefix("Benchmark")?;
    if rest.chars().next().is_some_and(|c| c.is_lowercase()) {
        return None;
    }
    fields[1].parse::<u64>().ok()?;
    let mut pairs = Vec::new();
    for pair in fields[2..].chunks(2) {
        let value = pair[0].parse::<f64>().ok()?;
        pairs.push((value, pair[1]));
    }
    Some((fields[0], pairs))
}

/// Drops the `-N` GOMAXPROCS suffix. Go omits it when GOMAXPROCS is 1, so a
/// sub-benchmark literally named `x-5` is misread then; acceptable and documented.
fn strip_procs(name: &str) -> &str {
    match name.rsplit_once('-') {
        Some((head, tail)) if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => name,
    }
}

fn default_better(unit: &str) -> Better {
    if unit.ends_with("/s") {
        Better::Higher
    } else {
        Better::Lower
    }
}

struct Series {
    group: String,
    name: String,
    unit: String,
    values: Vec<f64>,
}

pub(super) fn parse(text: &str) -> Result<MetricSet, String> {
    let mut env = std::collections::BTreeMap::new();
    let mut pkgs = BTreeSet::new();
    let mut group = String::new();
    let mut unit_better: HashMap<String, Better> = HashMap::new();
    let mut order: Vec<(String, String, String)> = Vec::new();
    let mut series: HashMap<(String, String, String), Series> = HashMap::new();

    for line in text.lines() {
        if let Some((name, pairs)) = parse_result_line(line) {
            let bench = strip_procs(name.strip_prefix("Benchmark").unwrap_or(name)).to_string();
            for (value, unit) in pairs {
                let key = (group.clone(), bench.clone(), unit.to_string());
                if !series.contains_key(&key) {
                    order.push(key.clone());
                }
                let s = series.entry(key).or_insert_with(|| Series {
                    group: group.clone(),
                    name: bench.clone(),
                    unit: unit.to_string(),
                    values: Vec::new(),
                });
                // NaN/inf lines still register the series so an all-bad one is reported as skipped.
                if value.is_finite() {
                    s.values.push(value);
                }
            }
        } else if let Some(rest) = line.strip_prefix("Unit ") {
            let mut parts = rest.split_whitespace();
            if let Some(unit) = parts.next() {
                for kv in parts {
                    match kv {
                        "better=higher" => {
                            unit_better.insert(unit.to_string(), Better::Higher);
                        }
                        "better=lower" => {
                            unit_better.insert(unit.to_string(), Better::Lower);
                        }
                        _ => {}
                    }
                }
            }
        } else if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            let is_config_key = key.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && !key.contains(char::is_whitespace)
                && !key.chars().any(|c| c.is_ascii_uppercase());
            if !is_config_key {
                continue;
            }
            if key == "pkg" {
                group = value.to_string();
                pkgs.insert(value.to_string());
            } else if matches!(key, "goos" | "goarch" | "cpu") {
                env.insert(key.to_string(), value.to_string());
            }
        }
    }

    if pkgs.len() == 1 {
        env.insert("pkg".into(), pkgs.into_iter().next().unwrap_or_default());
    }
    let mut metrics = Vec::with_capacity(order.len());
    let mut skipped = Skipped::default();
    for key in order {
        let Some(s) = series.remove(&key) else {
            continue;
        };
        if s.values.is_empty() {
            skipped.push(&s.group, &s.name, &s.unit);
            continue;
        }
        let better = unit_better
            .get(&s.unit)
            .copied()
            .unwrap_or_else(|| default_better(&s.unit));
        let mut m = Metric::new(
            &s.group,
            &s.name,
            &s.unit,
            &s.unit,
            median(&s.values),
            better,
        );
        if s.values.len() > 1 {
            m.samples = s.values;
        }
        metrics.push(m);
    }
    if metrics.is_empty() {
        return Err(
            "no Go benchmark result lines found (expected `BenchmarkX-8  N  v ns/op`)".into(),
        );
    }
    Ok(MetricSet {
        env,
        metrics,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_NAMES: [&str; 5] = ["Join", "Sort", "Sub/n=x0", "Sub/n=x1", "Throughput"];

    fn fixture() -> MetricSet {
        parse(include_str!("../../../../testdata/go/bench.txt")).unwrap()
    }

    #[test]
    fn parse_should_group_repeated_lines_into_samples_with_the_median_as_value() {
        let set = fixture();
        let sort = set
            .metrics
            .iter()
            .find(|m| m.name == "Sort" && m.measure == "ns/op")
            .unwrap();
        assert_eq!(sort.samples, vec![9642.0, 9040.0, 8933.0]);
        assert_eq!(sort.value, 9040.0);
        assert_eq!(sort.group, "gob");
    }

    #[test]
    fn parse_should_emit_one_metric_per_unit() {
        let set = fixture();
        let units: Vec<_> = set
            .metrics
            .iter()
            .filter(|m| m.name == "Throughput")
            .map(|m| m.measure.as_str())
            .collect();
        assert_eq!(units, ["ns/op", "MB/s", "B/op", "allocs/op"]);
    }

    #[test]
    fn throughput_units_are_higher_is_better_and_time_is_lower() {
        let set = fixture();
        let by = |measure: &str| {
            set.metrics
                .iter()
                .find(|m| m.name == "Throughput" && m.measure == measure)
                .unwrap()
                .better
        };
        assert_eq!(by("MB/s"), Better::Higher);
        assert_eq!(by("ns/op"), Better::Lower);
    }

    #[test]
    fn parse_should_strip_the_procs_suffix_and_keep_subbenchmark_names() {
        let set = fixture();
        assert!(set.metrics.iter().any(|m| m.name == "Sub/n=x1"));
        let mut names: Vec<_> = set.metrics.iter().map(|m| m.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names, FIXTURE_NAMES);
    }

    #[test]
    fn parse_result_line_table() {
        let cases: [(&str, Option<(&str, usize)>); 8] = [
            ("BenchmarkFoo-8 100 5 ns/op", Some(("BenchmarkFoo-8", 1))),
            (
                "BenchmarkFoo-8 100 5 ns/op 3 B/op",
                Some(("BenchmarkFoo-8", 2)),
            ),
            ("BenchmarkFoo-8 100 5 ns/op 3", None),
            ("BenchmarkFoo-8 many 5 ns/op", None),
            ("Benchmarkfoo-8 100 5 ns/op", None),
            ("BenchmarkFoo-8 --- FAIL", None),
            ("BenchmarkFoo-8 100 NaN ns/op", Some(("BenchmarkFoo-8", 1))),
            ("BenchmarkFoo-8 100", None),
        ];
        for (line, want) in cases {
            let got = parse_result_line(line).map(|(n, pairs)| (n, pairs.len()));
            assert_eq!(got, want, "{line}");
        }
    }

    #[test]
    fn parse_should_capture_config_lines_as_env() {
        let set = fixture();
        assert_eq!(set.env["goos"], "linux");
        assert_eq!(set.env["pkg"], "gob");
        assert!(set.env["cpu"].contains("Ryzen"));
    }

    #[test]
    fn unit_lines_override_the_default_direction() {
        let set = parse("Unit widgets/op better=higher\nBenchmarkX-4 10 5 widgets/op\n").unwrap();
        assert_eq!(set.metrics[0].better, Better::Higher);
    }

    #[test]
    fn a_single_run_has_a_value_but_no_samples() {
        let set = parse("BenchmarkX-4 10 5 ns/op\n").unwrap();
        assert!(set.metrics[0].samples.is_empty());
    }

    #[test]
    fn non_benchmark_input_is_an_error() {
        assert!(parse("PASS\nok  pkg 1s\n").is_err());
    }

    #[test]
    fn different_packages_keep_same_named_benchmarks_apart() {
        let text = "pkg: a\nBenchmarkX-4 10 5 ns/op\npkg: b\nBenchmarkX-4 10 7 ns/op\n";
        let set = parse(text).unwrap();
        assert_eq!(set.metrics.len(), 2);
        assert!(!set.env.contains_key("pkg"));
    }
}
