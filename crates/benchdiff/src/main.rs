//! `benchdiff`: reads benchmark files, compares head with an optional base, writes reports.
//! Built natively and as `wasm32-wasip1` (the GitHub Action runs the latter, with the
//! files in a preopened directory), so it sticks to `std::fs` and `std::env::args`.

use benchdiff_core::config::{
    check_compare_by, matrix_help, parse_cell_format, parse_config, parse_correction,
    parse_dimension_rules, parse_group_by, parse_reference, ConfigFormat,
};
use benchdiff_core::matrix::{MatrixOptions, MatrixSort, Reference, VariantLabel};
use benchdiff_core::svg::{ChartKind, ChartRows, ChartTheme, ChartWidth, EmbedSetting, SvgOptions};
use benchdiff_core::{
    compare,
    options::{matrix_sort_of, MatrixOptionsOverlay, OptionsOverlay, SvgOptionsOverlay},
    parse, render_json, render_markdown, Format, MetricSet, Options, Sort,
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
    --no-only-changed      show unchanged rows even if the config file hides them
    --sort delta|name|input|rank [delta]; rank needs an N-way run
    --max-rows N           show at most N changed rows
    --title TEXT           report heading [Benchmark comparison]
    --tag TEXT             sticky-comment key [default]
    --fail-on-regression[=PCT]
                           exit 1 if a regression of at least PCT percent exists [any, if bare]
    --no-fail-on-regression
                           never exit 1 on regressions, even if the config file sets a limit
    --markdown FILE        write the Markdown report here [stdout]
    --json FILE            also write the machine-readable report
    --head-json FILE       also write the merged head results as canonical benchdiff JSON
    --charts LIST          SVG charts: none, or delta,ranking,distribution (comma-separated) [none]
    --chart-rows N         rows per chart, 1 to 200 [30]; the effective cap is
                           min(chart-rows, max-rows) when max-rows is set
    --chart-width PX       chart width in pixels, 480 to 1600 [760]
    --chart-theme NAME     light, dark or both [light]
    --svg-dir DIR          write the SVG files here [off]
    --svg-embed[=MODE]     embed charts in the report as benchdiff-svg: placeholders:
                           true, false or auto (on iff --charts is set) [false]
    --publish-images[=BOOL]
                           host embedded charts as images: true or false [true]
";

const USAGE_TAIL: &str = "    -h, --help

FORMATS: ";

/// `--help`: the fixed lines, the matrix flags generated from the config key table, the tail.
fn help_text() -> String {
    format!("{USAGE}{}{USAGE_TAIL}{}", matrix_help(), Format::NAMES)
}

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
    svg_flags: SvgOptionsOverlay,
    matrix_flags: MatrixOptionsOverlay,
    /// `--variant label=path` in input order.
    variants: Vec<(VariantLabel, String)>,
    print_dimensions: bool,
}

/// Defaults, then the config file, then flags; `sources` names the layer of each non-default key.
struct Effective {
    options: Options,
    format: Format,
    base_format: Option<Format>,
    sources: Vec<(String, String, &'static str)>,
    file_overrides_set: bool,
    /// Consumed by the `--svg-dir` write (3.3.1b).
    #[allow(dead_code)]
    svg: SvgOptions,
    #[allow(dead_code)]
    svg_dir: Option<String>,
    /// Consumed by the N-way wiring (2.2.2b).
    #[allow(dead_code)]
    matrix: MatrixOptions,
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
        let mut svg_overlay = SvgOptionsOverlay::default();
        let mut matrix_overlay = MatrixOptionsOverlay::default();
        if let Some(path) = &self.config {
            let text = std::fs::read_to_string(path).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => format!("config file not found: {path}"),
                _ => format!("{path}: {e}"),
            })?;
            let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
            let config = parse_config(&text, name, ConfigFormat::Json)?;
            (format, base_format) = (config.format, config.base_format);
            svg_overlay = config.svg_overlay();
            matrix_overlay = config.matrix_overlay();
            let overlay = config.into_overlay();
            file_overrides_set = overlay.threshold_overrides.is_some();
            file_values = layer_values(&overlay, &matrix_overlay, format, base_format);
            overlay.apply(&mut options);
        }
        self.flags.apply(&mut options);
        let matrix_overlay = matrix_overlay.merge(self.matrix_flags.clone());
        let mut matrix = MatrixOptions::default();
        matrix_overlay.apply(&mut matrix);
        self.check_run_shape(&options, &matrix)?;
        let svg_overlay = svg_overlay.merge(self.svg_flags.clone());
        let mut svg = SvgOptions::default();
        svg_overlay.apply(&mut svg);
        let flag_values = layer_values(
            &self.flags,
            &self.matrix_flags,
            self.format,
            self.base_format,
        );

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
            svg,
            svg_dir: svg_overlay.svg_dir,
            matrix,
        })
    }

    /// Static checks that need the layered settings (exit 2, whatever the data).
    fn check_run_shape(&self, options: &Options, matrix: &MatrixOptions) -> Result<(), String> {
        let n_way = !self.variants.is_empty() || !matrix.compare_by.is_empty();
        if matrix.sort == MatrixSort::Rank && !n_way {
            return Err("--sort rank needs an N-way run (--variant or --compare-by)".into());
        }
        if n_way && self.head_json.is_some() {
            return Err("--head-json is two-way only".into());
        }
        if let (Reference::Label(label), false) = (&matrix.reference, self.variants.is_empty()) {
            if !self.variants.iter().any(|(l, _)| l == label) {
                let valid: Vec<&str> = self.variants.iter().map(|(l, _)| l.as_str()).collect();
                return Err(format!(
                    "unknown reference '{label}'; valid: {}",
                    valid.join(", ")
                ));
            }
        }
        if n_way && options.fail_on_regression_pct.is_some() && matrix.reference == Reference::First
        {
            return Err("fail-on-regression in an N-way run needs an explicit 'reference' (the baseline to protect); set 'reference'".into());
        }
        Ok(())
    }
}

/// Config-governed keys in display order.
const SETTING_KEYS: [&str; 17] = [
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
    "compare-by",
    "group-by",
    "dimensions",
    "dimensions-strict",
    "reference",
    "correction",
    "cell-format",
];

/// The config-governed settings a layer sets, as display text.
fn layer_values(
    o: &OptionsOverlay,
    m: &MatrixOptionsOverlay,
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
    } else if m.sort == Some(MatrixSort::Rank) {
        v.push(("sort", "rank".into()));
    }
    if let Some(x) = o.max_rows {
        v.push(("max-rows", x.to_string()));
    }
    if let Some(x) = o.only_changed {
        v.push(("only-changed", x.to_string()));
    }
    if o.fail_on_regression_off {
        v.push(("fail-on-regression", "off".into()));
    } else if let Some(x) = o.fail_on_regression_pct {
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
    if let Some(x) = &m.compare_by {
        v.push(("compare-by", x.clone()));
    }
    if let Some(x) = &m.group_by {
        v.push(("group-by", x.join(",")));
    }
    if let Some(x) = &m.dimensions {
        v.push(("dimensions", x.join("; ")));
    }
    if let Some(x) = m.strict {
        v.push(("dimensions-strict", x.to_string()));
    }
    if let Some(x) = &m.reference {
        v.push(("reference", x.to_string()));
    }
    if let Some(x) = m.correction {
        v.push(("correction", format!("{x:?}").to_lowercase()));
    }
    if let Some(x) = m.cell_format {
        v.push(("cell-format", format!("{x:?}").to_lowercase()));
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
        "sort": if eff.matrix.sort == MatrixSort::Rank { "rank" } else { sort_name(o.sort) },
        "max-rows": o.max_rows,
        "only-changed": o.only_changed,
        "fail-on-regression": o.fail_on_regression_pct,
        "title": o.title,
        "tag": o.tag,
        "format": format_name(eff.format),
        "base-format": eff.base_format.map(format_name),
        "compare-by": (!eff.matrix.compare_by.is_empty()).then_some(&eff.matrix.compare_by),
        "group-by": eff.matrix.group_by,
        "dimensions": eff.matrix.dimensions,
        "dimensions-strict": eff.matrix.strict,
        "reference": match &eff.matrix.reference {
            Reference::First => None,
            Reference::Label(l) => Some(l.to_string()),
        },
        "correction": eff.matrix.correction,
        "cell-format": eff.matrix.cell_format,
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
        println!("{}", help_text());
        return Ok(false);
    };
    let eff = args.effective()?;
    if args.print_config {
        print!("{}", print_config(&args, &eff));
        return Ok(false);
    }
    if !args.variants.is_empty() || args.print_dimensions {
        return Err(
            "N-way runs (--variant, --print-dimensions) are parsed but not wired yet".into(),
        );
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

/// A flag whose value is optional: `--flag=V` always takes V; `--flag V` takes V only when it
/// is one of `words`; a bare `--flag` means `true`.
fn optional_value(
    raw: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    inline: Option<String>,
    words: &[&str],
) -> String {
    inline
        .or_else(|| raw.next_if(|n| words.contains(&n.as_str())))
        .unwrap_or_else(|| "true".into())
}

/// `label=path`, split at the first `=`; the label must be a valid [`VariantLabel`].
fn parse_variant(arg: &str) -> Result<(VariantLabel, String), String> {
    let (label, path) = arg.split_once('=').unwrap_or((arg, ""));
    let label = VariantLabel::parse(label).map_err(|_| {
        format!(
            "--variant: label '{label}' is invalid (use [A-Za-z0-9._-], max 40) in '{arg}'; \
             expected label=path"
        )
    })?;
    if path.is_empty() {
        return Err(format!("--variant {label}=: path is empty"));
    }
    Ok((label, path.to_string()))
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
        svg_flags: SvgOptionsOverlay::default(),
        matrix_flags: MatrixOptionsOverlay::default(),
        variants: vec![],
        print_dimensions: false,
    };
    let mut raw = raw.peekable();
    let (mut saw_only_changed, mut saw_no_only_changed) = (false, false);
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
                saw_only_changed = true;
            }
            "--no-only-changed" => {
                reject_inline("--no-only-changed")?;
                a.flags.only_changed = Some(false);
                saw_no_only_changed = true;
            }
            "--sort" => match value("--sort")?.as_str() {
                "rank" => a.matrix_flags.sort = Some(MatrixSort::Rank),
                other => {
                    let sort = Sort::from_name(other)?;
                    a.flags.sort = Some(sort);
                    a.matrix_flags.sort = Some(matrix_sort_of(sort));
                }
            },
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
            "--no-fail-on-regression" => {
                reject_inline("--no-fail-on-regression")?;
                a.flags.fail_on_regression_off = true;
            }
            "--charts" => {
                let v = value("--charts")?;
                a.svg_flags.kinds =
                    Some(ChartKind::parse_list(&v).map_err(|e| format!("--charts: {e}"))?);
            }
            "--chart-rows" => {
                let v = value("--chart-rows")?;
                let n = v
                    .trim()
                    .parse()
                    .map_err(|_| format!("--chart-rows: {v:?} is not a whole number"))?;
                a.svg_flags.rows =
                    Some(ChartRows::new(n).map_err(|e| format!("--chart-rows: {e}"))?);
            }
            "--chart-width" => {
                let v = value("--chart-width")?;
                let n = v
                    .trim()
                    .parse()
                    .map_err(|_| format!("--chart-width: {v:?} is not a whole number"))?;
                a.svg_flags.width =
                    Some(ChartWidth::new(n).map_err(|e| format!("--chart-width: {e}"))?);
            }
            "--chart-theme" => {
                a.svg_flags.theme = Some(
                    ChartTheme::from_name(&value("--chart-theme")?)
                        .map_err(|e| format!("--chart-theme: {e}"))?,
                );
            }
            "--svg-dir" => a.svg_flags.svg_dir = Some(value("--svg-dir")?),
            "--svg-embed" => {
                let v = optional_value(&mut raw, inline, &["true", "false", "auto"]);
                a.svg_flags.embed =
                    Some(EmbedSetting::from_name(&v).map_err(|e| format!("--svg-embed: {e}"))?);
            }
            "--publish-images" => {
                a.svg_flags.publish_images = Some(
                    match optional_value(&mut raw, inline, &["true", "false"]).as_str() {
                        "true" => true,
                        "false" => false,
                        other => {
                            return Err(format!(
                                "--publish-images: {other:?} is not valid; use true, false"
                            ))
                        }
                    },
                );
            }
            "--variant" => {
                let (label, path) = parse_variant(&value("--variant")?)?;
                if let Some((_, first)) = a.variants.iter().find(|(l, _)| *l == label) {
                    return Err(format!("label \"{label}\" given twice ({first}, {path})"));
                }
                a.variants.push((label, path));
            }
            "--compare-by" => {
                let dim = value("--compare-by")?;
                check_compare_by(&dim)?;
                a.matrix_flags.compare_by = Some(dim);
            }
            "--group-by" => {
                a.matrix_flags.group_by = Some(parse_group_by(&value("--group-by")?)?);
            }
            "--dimensions" => {
                let rules = parse_dimension_rules(&value("--dimensions")?)?;
                a.matrix_flags
                    .dimensions
                    .get_or_insert_with(Vec::new)
                    .extend(rules);
            }
            "--dimensions-strict" => {
                reject_inline("--dimensions-strict")?;
                a.matrix_flags.strict = Some(true);
            }
            "--reference" => {
                a.matrix_flags.reference = Some(parse_reference(&value("--reference")?)?)
            }
            "--correction" => {
                a.matrix_flags.correction = Some(parse_correction(&value("--correction")?)?)
            }
            "--cell-format" => {
                a.matrix_flags.cell_format = Some(parse_cell_format(&value("--cell-format")?)?)
            }
            "--print-dimensions" => {
                reject_inline("--print-dimensions")?;
                a.print_dimensions = true;
            }
            "--markdown" => a.markdown = Some(value("--markdown")?),
            "--json" => a.json = Some(value("--json")?),
            "--head-json" => a.head_json = Some(value("--head-json")?),
            other => return Err(format!("unknown argument {other:?}; try --help")),
        }
    }
    if saw_only_changed && saw_no_only_changed {
        return Err("--only-changed conflicts with --no-only-changed; use one".into());
    }
    if a.flags.fail_on_regression_pct.is_some() && a.flags.fail_on_regression_off {
        return Err("--fail-on-regression conflicts with --no-fail-on-regression; use one".into());
    }
    if !a.variants.is_empty() && (!a.head.is_empty() || !a.base.is_empty()) {
        return Err("--variant cannot be combined with --base/--head".into());
    }
    if !a.variants.is_empty() && a.head_json.is_some() {
        return Err("--head-json is two-way only".into());
    }
    if a.head.is_empty() && a.variants.is_empty() && !a.print_config {
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
    #[test]
    fn svg_embed_auto_resolves_on_only_when_charts_are_requested() {
        let embed = |extra: &[&str]| {
            let mut l = vec!["--head", "a", "--svg-embed", "auto"];
            l.extend(extra);
            args(&l).unwrap().unwrap().effective().unwrap().svg.embed
        };
        assert_eq!(
            embed(&["--charts", "delta"]),
            benchdiff_core::svg::EmbedMode::On
        );
        assert_eq!(embed(&[]), benchdiff_core::svg::EmbedMode::Off);
    }
}
