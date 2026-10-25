//! Time-unit conversion so a base in `ns/op` and a head in `ms/op` still compare.

/// Nanoseconds per one of `unit`, for time units only (`ns`, `us`, `µs`, `ms`, `s`, optionally `/op`).
pub(crate) fn nanos_per(unit: &str) -> Option<f64> {
    let u = unit.strip_suffix("/op").unwrap_or(unit).trim();
    match u {
        "ns" | "nsec" | "nanosecond" | "nanoseconds" => Some(1.0),
        "us" | "µs" | "μs" | "usec" | "microsecond" | "microseconds" => Some(1e3),
        "ms" | "msec" | "millisecond" | "milliseconds" => Some(1e6),
        "s" | "sec" | "second" | "seconds" => Some(1e9),
        _ => None,
    }
}

/// Factor that turns a value in `from` into the same quantity in `to`; `None` if they cannot be compared.
pub(crate) fn factor(from: &str, to: &str) -> Option<f64> {
    if from == to {
        return Some(1.0);
    }
    Some(nanos_per(from)? / nanos_per(to)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factor_should_convert_between_time_units() {
        assert_eq!(factor("ns/op", "ms/op"), Some(1e-6));
        assert_eq!(factor("s", "ms"), Some(1e3));
    }

    #[test]
    fn factor_should_refuse_non_time_mismatches() {
        assert_eq!(factor("B/op", "ns/op"), None);
        assert_eq!(factor("MB/s", "KB/s"), None);
        assert_eq!(factor("B/op", "B/op"), Some(1.0));
    }
}
