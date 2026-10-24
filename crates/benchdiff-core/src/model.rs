//! The canonical model: a flat list of metrics. See `docs/research.md` for why.
//!
//! ```json
//! {"schema": 1, "env": {"sha": "abc123"},
//!  "metrics": [{"group": "api", "name": "create", "measure": "p50", "unit": "ms",
//!               "better": "lower", "value": 0.44, "lower": 0.43, "upper": 0.46,
//!               "samples": [0.44, 0.45, 0.43]}]}
//! ```
//! A metric is identified by `(group, name, measure)`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Better {
    #[default]
    Lower,
    Higher,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    #[serde(default)]
    pub group: String,
    pub name: String,
    /// Which number of a benchmark this is (`ns/op`, `p95`, `allocs`); empty if the benchmark has one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub measure: String,
    #[serde(default)]
    pub unit: String,
    pub value: f64,
    #[serde(default)]
    pub better: Better,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper: Option<f64>,
    /// Repeated measurements; enable the significance test when both sides have enough.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<f64>,
}

impl Metric {
    // kibitzer:ignore long-parameter-list -- flat constructor for a flat record; every importer passes all six
    pub fn new(
        group: &str,
        name: &str,
        measure: &str,
        unit: &str,
        value: f64,
        better: Better,
    ) -> Self {
        Metric {
            group: group.into(),
            name: name.into(),
            measure: measure.into(),
            unit: unit.into(),
            value,
            better,
            lower: None,
            upper: None,
            samples: Vec::new(),
        }
    }

    pub(crate) fn key(&self) -> (&str, &str, &str) {
        (&self.group, &self.name, &self.measure)
    }

    pub fn has_interval(&self) -> bool {
        self.lower.is_some() && self.upper.is_some()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetricSet {
    /// Free-form context (commit, runner, ...) shown with the report.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub metrics: Vec<Metric>,
}

impl MetricSet {
    /// Parses the canonical JSON.
    pub fn parse(text: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct File {
            // Policy: v1 only gains optional fields; unknown fields are ignored on read.
            schema: Option<u64>,
            #[serde(default)]
            env: BTreeMap<String, String>,
            metrics: Vec<Metric>,
        }
        let f: File = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if let Some(n) = f.schema.filter(|&n| n > SCHEMA_VERSION) {
            return Err(format!(
                "benchdiff JSON schema {n} is newer than this tool supports ({SCHEMA_VERSION})"
            ));
        }
        let set = MetricSet {
            env: f.env,
            metrics: f.metrics,
        };
        set.validate()?;
        Ok(set)
    }

    pub fn to_json(&self) -> String {
        #[derive(Serialize)]
        struct File<'a> {
            schema: u64,
            #[serde(flatten)]
            set: &'a MetricSet,
        }
        serde_json::to_string_pretty(&File {
            schema: SCHEMA_VERSION,
            set: self,
        })
        .expect("model serializes")
    }

    /// Concatenates sets; a metric present in two of them is an error rather
    /// than a silent override.
    pub fn merge(sets: impl IntoIterator<Item = MetricSet>) -> Result<Self, String> {
        let mut merged = MetricSet::default();
        for set in sets {
            merged.env.extend(set.env);
            merged.metrics.extend(set.metrics);
        }
        merged.validate()?;
        Ok(merged)
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        let mut seen = std::collections::HashSet::new();
        for m in &self.metrics {
            let bad = !m.value.is_finite()
                || m.lower.is_some_and(|v| !v.is_finite())
                || m.upper.is_some_and(|v| !v.is_finite())
                || m.samples.iter().any(|v| !v.is_finite());
            if bad {
                return Err(format!(
                    "metric {:?} (group {:?}, measure {:?}) has a non-finite number",
                    m.name, m.group, m.measure
                ));
            }
            if !seen.insert(m.key()) {
                return Err(format!(
                    "duplicate metric {:?} (group {:?}, measure {:?})",
                    m.name, m.group, m.measure
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_should_default_group_unit_and_direction() {
        let set = MetricSet::parse(r#"{"metrics":[{"name":"x","value":1.5}]}"#).unwrap();
        let m = &set.metrics[0];
        assert_eq!((m.group.as_str(), m.unit.as_str()), ("", ""));
        assert_eq!(m.better, Better::Lower);
    }

    #[test]
    fn parse_should_reject_duplicate_metrics() {
        let err =
            MetricSet::parse(r#"{"metrics":[{"name":"x","value":1},{"name":"x","value":2}]}"#)
                .unwrap_err();
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn merge_should_reject_a_metric_present_in_two_sets() {
        let a = MetricSet::parse(r#"{"metrics":[{"name":"x","value":1}]}"#).unwrap();
        assert!(MetricSet::merge([a.clone(), a]).is_err());
    }

    #[test]
    fn same_name_in_different_groups_or_measures_is_not_a_duplicate() {
        let set = MetricSet::parse(
            r#"{"metrics":[{"group":"a","name":"x","value":1},{"group":"b","name":"x","value":2},
                           {"group":"b","name":"x","measure":"p95","value":3}]}"#,
        );
        assert!(set.is_ok());
    }

    #[test]
    fn parse_should_accept_missing_or_v1_schema_and_reject_newer() {
        let body = r#""metrics":[{"name":"x","value":1}]"#;
        assert!(MetricSet::parse(&format!("{{{body}}}")).is_ok());
        assert!(MetricSet::parse(&format!(r#"{{"schema":1,{body}}}"#)).is_ok());
        let err = MetricSet::parse(&format!(r#"{{"schema":2,{body}}}"#)).unwrap_err();
        assert_eq!(
            err,
            "benchdiff JSON schema 2 is newer than this tool supports (1)"
        );
    }

    #[test]
    fn validate_error_names_the_offending_metric() {
        let m = Metric::new("grp", "nm", "msr", "ms", f64::NAN, Better::Lower);
        let err = MetricSet {
            env: Default::default(),
            metrics: vec![m],
        }
        .validate()
        .unwrap_err();
        assert!(
            err.contains("grp") && err.contains("nm") && err.contains("msr"),
            "{err}"
        );
    }

    #[test]
    fn to_json_should_round_trip() {
        let mut m = Metric::new("g", "n", "p50", "ms", 1.0, Better::Higher);
        m.samples = vec![1.0, 2.0];
        let set = MetricSet {
            env: [("k".to_string(), "v".to_string())].into(),
            metrics: vec![m],
        };
        assert_eq!(MetricSet::parse(&set.to_json()).unwrap(), set);
    }
}
