//! `benchdiff`: reads benchmark files, compares head with an optional base, writes reports.
//! Built natively and as `wasm32-wasip1` (the GitHub Action runs the latter, with the
//! files in a preopened directory), so it sticks to `std::fs` and `std::env::args`.

use benchdiff_core::config::{parse_config, ConfigFormat};
use benchdiff_core::{
    compare, options::OptionsOverlay, parse, render_json, render_markdown, Format, MetricSet,
    Options, Sort,
};
use serde_json::{json, Value};
use std::process::ExitCode;

const USAGE: &str = "\
benchdiff: compare benchmark results and render a Markdown report

USAGE:
    benchdiff --head FILE... [--base FILE...] [OPTIONS]

OPTIONS:
    --head FILE            head results (repeatable; files are merged)
    --base FILE            base results (repeatable); omit for a head-only report
    --config FILE          settings file (benchdiff.config.json); flags override it
    --print-config         print the effective settings and where each came from, then exit
    --format NAME          input format for head and base [auto]
    --base-format NAME     input format for base, if different from --format
    --threshold PCT        changes smaller than this many percent are noise [5]
    --min-abs N            changes smaller than N units are noise [0]
    --alpha P              significance level for the Mann-Whitney test [0.05]
    --no-stats             skip the significance test; thresholds only
    --only-changed         hide unchanged rows
    --sort delta|name|input [delta]
    --max-rows N           show at most N changed rows
    --title TEXT           report heading [Benchmark comparison]
    --tag TEXT             sticky-comment key [default]
    --fail-on-regression[=PCT]
                           exit 1 if a regression of at least PCT percent exists [any, if bare]
    --markdown FILE        write the Markdown report here [stdout]
    --json FILE            also write the machine-readable report
    --head-json FILE       also write the merged head results as canonical benchdiff JSON
    -h, --help

FORMATS: ";

#[derive(Debug)]
struct Args {
    head: Vec<String>,
    base: Vec<String>,
    config: Option<String>,
    print_config: bool,
    format: Option<Format>,
    base_format: Option<Format>,
    markdown: Option<String>,
    json: Option<String>,
    head_json: Option<String>,
    flags: OptionsOverlay,
}

/// Defaults, then the config file, then flags; `sources` names the layer of each non-default key.
struct Effective {
    options: Options,
    format: Format,
    base_format: Option<Format>,
    sources: Vec<(String, String, &'static str)>,
    file_overrides_set: bool,
}

impl Args {
    /// Flags over defaults only; the config file is layered in by [`Args::effective`].
    #[cfg(test)]
    fn options(&self) -> Options {
        let mut opts = Options::default();
        self.flags.apply(&mut opts);
        opts
    }

    fn effective(&self) -> Result<Effective, String> {
        let mut options = Options::default();
        let mut format = None;
        let mut base_format = None;
        let mut file_values = vec![];
        let mut file_overrides_set = false;
        if let Some(path) = &self.config {
            let text = std::fs::read_to_string(path).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => format!("config file not found: {path}"),
                _ => format!("{path}: {e}"),
            })?;
            let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
            let config = parse_config(&text, name, ConfigFormat::Json)?;
            (format, base_format) = (config.format, config.base_format);
            let overlay = config.into_overlay();
            file_overrides_set = overlay.threshold_overrides.is_some();
            file_values = layer_values(&overlay, format, base_format);
            overlay.apply(&mut options);
        }
        self.flags.apply(&mut options);
        let flag_values = layer_values(&self.flags, self.format, self.base_format);

        let mut sources = vec![];
        for key in SETTING_KEYS {
            let pick = |values: &[(&'static str, String)]| {
                values
                    .iter()
                    .find(|(k, _)| k == &key)
                    .map(|(_, v)| v.clone())
            };
            if let Some(value) = pick(&flag_values) {
                sources.push((key.to_string(), value, "flag"));
            } else if let Some(value) = pick(&file_values) {
                sources.push((key.to_string(), value, "config"));
            }
        }
        Ok(Effective {
            options,
            format: self.format.or(format).unwrap_or(Format::Auto),
            base_format: self.base_format.or(base_format),
            sources,
            file_overrides_set,
        })
    }
}

/// Config-governed keys in display order.
const SETTING_KEYS: [&str; 10] = [
    "threshold",
    "min-abs",
    "sort",
    "max-rows",
    "only-changed",
    "fail-on-regression",
    "title",
    "tag",
    "format",
    "base-format",
];

/// The config-governed settings a layer sets, as display text.
fn layer_values(
    o: &OptionsOverlay,
    format: Option<Format>,
    base_format: Option<Format>,
) -> Vec<(&'static str, String)> {
    let mut v = vec![];
    if let Some(x) = o.threshold_pct {
        v.push(("threshold", x.to_string()));
    }
    if let Some(x) = o.min_abs {
        v.push(("min-abs", x.to_string()));
    }
    if let Some(x) = o.sort {
        v.push(("sort", sort_name(x).into()));
    }
    if let Some(x) = o.max_rows {
        v.push(("max-rows", x.to_string()));
    }
    if let Some(x) = o.only_changed {
        v.push(("only-changed", x.to_string()));
    }
    if let Some(x) = o.fail_on_regression_pct {
        v.push(("fail-on-regression", x.to_string()));
    }
    if let Some(x) = &o.title {
        v.push(("title", x.clone()));
    }
    if let Some(x) = &o.tag {
        v.push(("tag", x.clone()));
    }
    if let Some(x) = format {
        v.push(("format", format_name(x).into()));
    }
    if let Some(x) = base_format {
        v.push(("base-format", format_name(x).into()));
    }
    v
}

fn sort_name(s: Sort) -> &'static str {
    match s {
        Sort::Delta => "delta",
        Sort::Name => "name",
        Sort::Input => "input",
    }
}

fn format_name(f: Format) -> &'static str {
    match f {
        Format::Auto => "auto",
        Format::Benchdiff => "benchdiff",
        Format::Go => "go",
        Format::Criterion => "criterion",
        Format::CargoBench => "cargo-bench",
        Format::Jmh => "jmh",
        Format::Hyperfine => "hyperfine",
        Format::GoogleBench => "googlebench",
        Format::GabSmaller => "gab-smaller",
        Format::GabBigger => "gab-bigger",
        Format::Bmf => "bmf",
    }
}

fn print_config(args: &Args, eff: &Effective) -> String {
    let o = &eff.options;
    let mut sources = serde_json::Map::new();
    for s in &eff.sources {
        sources.insert(s.0.clone(), Value::String(s.2.into()));
    }
    if eff.file_overrides_set {
        sources.insert("thresholds".into(), "config".into());
    }
    let config = json!({
        "threshold": o.threshold_pct,
        "thresholds": o.threshold_overrides,
        "min-abs": o.min_abs,
        "sort": sort_name(o.sort),
        "max-rows": o.max_rows,
        "only-changed": o.only_changed,
        "fail-on-regression": o.fail_on_regression_pct,
        "title": o.title,
        "tag": o.tag,
        "format": format_name(eff.format),
        "base-format": eff.base_format.map(format_name),
    });
    let out = json!({"config-file": args.config, "config": config, "sources": sources});
    format!("{out:#}\n")
}

fn main() -> ExitCode {
    match run() {
        Ok(failed) => {
            if failed {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(message) => {
            eprintln!("benchdiff: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool, String> {
    let Some(args) = parse_args(std::env::args().skip(1))? else {
        println!("{USAGE}{}", Format::NAMES);
        return Ok(false);
    };
    let eff = args.effective()?;
    if args.print_config {
        print!("{}", print_config(&args, &eff));
        return Ok(false);
    }
    let head = load(&args.head, eff.format)?;
    let base = if args.base.is_empty() {
        None
    } else {
        Some(load(&args.base, eff.base_format.unwrap_or(eff.format))?)
    };

    let mut report = compare(base.as_ref(), &head, &eff.options);
    // Only an explicit config surfaces sources, so flag-only runs keep their exact output.
    if args.config.is_some() {
        // `compare` is a private module of the core crate, so the type is named by inference.
        for (key, value, source) in eff.sources {
            report.summary.config_sources.push(Default::default());
            if let Some(c) = report.summary.config_sources.last_mut() {
                (c.key, c.value, c.source) = (key, value, source.into());
            }
        }
    }
    let markdown = render_markdown(&report);
    match &args.markdown {
        Some(path) => write(path, &markdown)?,
        None => print!("{markdown}"),
    }
    if let Some(path) = &args.json {
        write(path, &render_json(&report))?;
    }
    if let Some(path) = &args.head_json {
        write(path, &head.to_json())?;
    }
    Ok(report.exceeds_failure_threshold())
}

fn load(paths: &[String], format: Format) -> Result<MetricSet, String> {
    let sets = paths
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            parse(format, &text).map_err(|e| format!("{path}: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    MetricSet::merge(sets)
}

fn write(path: &str, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{path}: {e}"))
}

fn parse_args(raw: impl Iterator<Item = String>) -> Result<Option<Args>, String> {
    let mut a = Args {
        head: vec![],
        base: vec![],
        config: None,
        print_config: false,
        format: None,
        base_format: None,
        markdown: None,
        json: None,
        head_json: None,
        flags: OptionsOverlay::default(),
    };
    let mut raw = raw.peekable();
    while let Some(arg) = raw.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            if inline.is_some() {
                return Ok(inline.clone().unwrap_or_default());
            }
            // A following flag means the value was forgotten, not that it starts with `--`.
            raw.next_if(|next| !next.starts_with("--"))
                .ok_or_else(|| format!("{name} needs a value"))
        };
        let number = |name: &str, text: String| {
            text.trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .ok_or_else(|| format!("{name}: {text:?} is not a non-negative number"))
        };
        let reject_inline = |name: &str| match &inline {
            Some(_) => Err(format!("{name} does not take a value")),
            None => Ok(()),
        };
        match flag.as_str() {
            "-h" | "--help" => return Ok(None),
            "--head" => a.head.push(value("--head")?),
            "--base" => a.base.push(value("--base")?),
            "--config" => a.config = Some(value("--config")?),
            "--print-config" => {
                reject_inline("--print-config")?;
                a.print_config = true;
            }
            "--format" => a.format = Some(Format::from_name(&value("--format")?)?),
            "--base-format" => a.base_format = Some(Format::from_name(&value("--base-format")?)?),
            "--threshold" => {
                a.flags.threshold_pct = Some(number("--threshold", value("--threshold")?)?)
            }
            "--min-abs" => a.flags.min_abs = Some(number("--min-abs", value("--min-abs")?)?),
            "--alpha" => {
                let alpha = number("--alpha", value("--alpha")?)?;
                if alpha == 0.0 || alpha > 1.0 {
                    return Err(format!("--alpha: {alpha} must be in (0, 1]"));
                }
                a.flags.alpha = Some(alpha);
            }
            "--no-stats" => {
                reject_inline("--no-stats")?;
                a.flags.use_stats = Some(false);
            }
            "--only-changed" => {
                reject_inline("--only-changed")?;
                a.flags.only_changed = Some(true);
            }
            "--sort" => a.flags.sort = Some(Sort::from_name(&value("--sort")?)?),
            "--max-rows" => {
                let n = value("--max-rows")?;
                a.flags.max_rows = Some(
                    n.trim()
                        .parse()
                        .map_err(|_| format!("--max-rows: {n:?} is not a whole number"))?,
                );
            }
            "--title" => a.flags.title = Some(value("--title")?),
            "--tag" => a.flags.tag = Some(value("--tag")?),
            "--fail-on-regression" => {
                a.flags.fail_on_regression_pct = Some(match inline {
                    Some(v) => number("--fail-on-regression", v)?,
                    None => 0.0,
                })
            }
            "--markdown" => a.markdown = Some(value("--markdown")?),
            "--json" => a.json = Some(value("--json")?),
            "--head-json" => a.head_json = Some(value("--head-json")?),
            other => return Err(format!("unknown argument {other:?}; try --help")),
        }
    }
    if a.head.is_empty() && !a.print_config {
        return Err("--head is required; try --help".into());
    }
    Ok(Some(a))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Option<Args>, String> {
        parse_args(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn repeated_head_and_base_collect_and_equals_form_works() {
        let a = args(&["--head", "a", "--head=b", "--base", "c", "--threshold=2.5"])
            .unwrap()
            .unwrap();
        assert_eq!(
            (a.head.len(), a.base.len(), a.options().threshold_pct),
            (2, 1, 2.5)
        );
    }

    #[test]
    fn bare_fail_on_regression_means_any_regression() {
        let a = args(&["--head", "a", "--fail-on-regression"])
            .unwrap()
            .unwrap();
        assert_eq!(a.options().fail_on_regression_pct, Some(0.0));
        let a = args(&["--head", "a", "--fail-on-regression=10"])
            .unwrap()
            .unwrap();
        assert_eq!(a.options().fail_on_regression_pct, Some(10.0));
    }

    #[test]
    fn missing_head_unknown_flag_and_bad_number_are_errors() {
        assert!(args(&[]).unwrap_err().contains("--head"));
        assert!(args(&["--head", "a", "--wat"])
            .unwrap_err()
            .contains("unknown"));
        assert!(args(&["--head", "a", "--threshold", "x"])
            .unwrap_err()
            .contains("non-negative number"));
    }

    fn err(list: &[&str]) -> String {
        args(list).unwrap_err()
    }

    #[test]
    fn a_flag_where_a_value_belongs_is_a_missing_value() {
        assert_eq!(err(&["--head", "--base", "b"]), "--head needs a value");
        assert_eq!(err(&["--head"]), "--head needs a value");
    }

    #[test]
    fn numbers_must_be_finite_and_non_negative() {
        for bad in ["NaN", "inf", "-1", "-0.5"] {
            let e = err(&["--head", "a", "--threshold", bad]);
            assert!(e.contains("non-negative number"), "{bad}: {e}");
            let e = err(&["--head", "a", &format!("--min-abs={bad}")]);
            assert!(e.contains("non-negative number"), "{bad}: {e}");
            let e = err(&["--head", "a", &format!("--fail-on-regression={bad}")]);
            assert!(e.contains("non-negative number"), "{bad}: {e}");
        }
    }

    #[test]
    fn alpha_must_be_in_the_half_open_unit_interval() {
        for bad in ["0", "1.5", "-0.1", "NaN"] {
            assert!(args(&["--head", "a", "--alpha", bad]).is_err(), "{bad}");
        }
        let a = args(&["--head", "a", "--alpha", "1"]).unwrap().unwrap();
        assert_eq!(a.options().alpha, 1.0);
    }

    #[test]
    fn boolean_flags_reject_an_inline_value() {
        for flag in ["--no-stats=false", "--only-changed=0"] {
            assert!(err(&["--head", "a", flag]).contains("does not take a value"));
        }
        assert!(args(&["--head", "a", "--no-stats", "--only-changed"]).is_ok());
    }

    #[test]
    fn help_short_circuits() {
        assert!(args(&["--help"]).unwrap().is_none());
    }
}
