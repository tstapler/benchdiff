//! Partial settings layered over `Options::default()` (defaults < file < flags, ADR-001).
//! An unset field means "not given", so a flag equal to the default still overrides a file.

use crate::compare::{Options, Sort};
use crate::matrix::{CellFormat, Correction, MatrixOptions, MatrixSort, Reference, VariantLabel};
use crate::svg::{
    ChartKind, ChartRows, ChartTheme, ChartWidth, EmbedMode, EmbedSetting, SvgOptions,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OptionsOverlay {
    pub threshold_pct: Option<f64>,
    pub min_abs: Option<f64>,
    pub alpha: Option<f64>,
    pub use_stats: Option<bool>,
    pub only_changed: Option<bool>,
    pub sort: Option<Sort>,
    pub max_rows: Option<usize>,
    pub title: Option<String>,
    pub tag: Option<String>,
    pub fail_on_regression_pct: Option<f64>,
    /// `--no-fail-on-regression`: clears any limit set by a lower layer.
    pub fail_on_regression_off: bool,
    pub threshold_overrides: Option<BTreeMap<String, f64>>,
}

impl OptionsOverlay {
    /// Overwrites each field of `opts` that this overlay sets.
    pub fn apply(&self, opts: &mut Options) {
        if let Some(v) = self.threshold_pct {
            opts.threshold_pct = v;
        }
        if let Some(v) = self.min_abs {
            opts.min_abs = v;
        }
        if let Some(v) = self.alpha {
            opts.alpha = v;
        }
        if let Some(v) = self.use_stats {
            opts.use_stats = v;
        }
        if let Some(v) = self.only_changed {
            opts.only_changed = v;
        }
        if let Some(v) = self.sort {
            opts.sort = v;
        }
        if let Some(v) = self.max_rows {
            opts.max_rows = Some(v);
        }
        if let Some(v) = &self.title {
            opts.title = v.clone();
        }
        if let Some(v) = &self.tag {
            opts.tag = v.clone();
        }
        if let Some(v) = self.fail_on_regression_pct {
            opts.fail_on_regression_pct = Some(v);
        }
        if self.fail_on_regression_off {
            opts.fail_on_regression_pct = None;
        }
        if let Some(v) = &self.threshold_overrides {
            opts.threshold_overrides = v.clone();
        }
    }

    /// Fields set in `later` replace those set here.
    pub fn merge(mut self, later: OptionsOverlay) -> OptionsOverlay {
        macro_rules! take {
            ($($f:ident),*) => { $(if later.$f.is_some() { self.$f = later.$f; })* };
        }
        take!(
            threshold_pct,
            min_abs,
            alpha,
            use_stats,
            only_changed,
            sort,
            max_rows,
            title,
            tag,
            fail_on_regression_pct,
            threshold_overrides
        );
        if later.fail_on_regression_off {
            self.fail_on_regression_off = true;
            self.fail_on_regression_pct = None;
        } else if later.fail_on_regression_pct.is_some() {
            self.fail_on_regression_off = false;
        }
        self
    }
}

/// Partial chart settings, layered like [`OptionsOverlay`] but kept apart from `Options`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SvgOptionsOverlay {
    pub kinds: Option<Vec<ChartKind>>,
    pub rows: Option<ChartRows>,
    pub width: Option<ChartWidth>,
    pub theme: Option<ChartTheme>,
    pub max_series: Option<usize>,
    pub embed: Option<EmbedSetting>,
    pub publish_images: Option<bool>,
    /// Output directory; not part of [`SvgOptions`] (the renderer returns bytes, callers write).
    pub svg_dir: Option<String>,
}

impl SvgOptionsOverlay {
    /// Overwrites each field of `opts` that this overlay sets. `embed` resolves last, because
    /// `Auto` means "on iff charts are requested".
    pub fn apply(&self, opts: &mut SvgOptions) {
        if let Some(v) = &self.kinds {
            opts.kinds = v.clone();
        }
        if let Some(v) = self.rows {
            opts.rows = v;
        }
        if let Some(v) = self.width {
            opts.width = v;
        }
        if let Some(v) = self.theme {
            opts.theme = v;
        }
        if let Some(v) = self.max_series {
            opts.max_series = v;
        }
        if let Some(v) = self.publish_images {
            opts.publish_images = v;
        }
        match self.embed {
            Some(EmbedSetting::True) => opts.embed = EmbedMode::On,
            Some(EmbedSetting::False) => opts.embed = EmbedMode::Off,
            Some(EmbedSetting::Auto) => {
                opts.embed = if opts.kinds.is_empty() {
                    EmbedMode::Off
                } else {
                    EmbedMode::On
                }
            }
            None => {}
        }
    }

    /// Fields set in `later` replace those set here, except that `Auto` never replaces a
    /// concrete `svg-embed`.
    pub fn merge(mut self, later: SvgOptionsOverlay) -> SvgOptionsOverlay {
        macro_rules! take {
            ($($f:ident),*) => { $(if later.$f.is_some() { self.$f = later.$f.clone(); })* };
        }
        take!(
            kinds,
            rows,
            width,
            theme,
            max_series,
            publish_images,
            svg_dir
        );
        let keep_concrete = later.embed == Some(EmbedSetting::Auto)
            && matches!(self.embed, Some(EmbedSetting::True | EmbedSetting::False));
        if later.embed.is_some() && !keep_concrete {
            self.embed = later.embed;
        }
        self
    }
}

/// The matrix sort a pair sort corresponds to (`rank` has no pair equivalent).
pub fn matrix_sort_of(s: Sort) -> MatrixSort {
    match s {
        Sort::Input => MatrixSort::Input,
        Sort::Name => MatrixSort::Name,
        Sort::Delta => MatrixSort::Delta,
    }
}

/// Partial N-way settings, layered like [`OptionsOverlay`] but kept apart from `Options`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MatrixOptionsOverlay {
    pub compare_by: Option<String>,
    pub group_by: Option<Vec<String>>,
    pub dimensions: Option<Vec<String>>,
    pub strict: Option<bool>,
    pub reference: Option<VariantLabel>,
    pub correction: Option<Correction>,
    pub cell_format: Option<CellFormat>,
    pub sort: Option<MatrixSort>,
}

impl MatrixOptionsOverlay {
    /// Overwrites each field of `opts` that this overlay sets.
    pub fn apply(&self, opts: &mut MatrixOptions) {
        if let Some(v) = &self.compare_by {
            opts.compare_by = v.clone();
        }
        if let Some(v) = &self.group_by {
            opts.group_by = Some(v.clone());
        }
        if let Some(v) = &self.dimensions {
            opts.dimensions = v.clone();
        }
        if let Some(v) = self.strict {
            opts.strict = v;
        }
        if let Some(v) = &self.reference {
            opts.reference = Reference::Label(v.clone());
        }
        if let Some(v) = self.correction {
            opts.correction = v;
        }
        if let Some(v) = self.cell_format {
            opts.cell_format = v;
        }
        if let Some(v) = self.sort {
            opts.sort = v;
        }
    }

    /// Fields set in `later` replace those set here.
    pub fn merge(mut self, later: MatrixOptionsOverlay) -> MatrixOptionsOverlay {
        macro_rules! take {
            ($($f:ident),*) => { $(if later.$f.is_some() { self.$f = later.$f; })* };
        }
        take!(
            compare_by,
            group_by,
            dimensions,
            strict,
            reference,
            correction,
            cell_format,
            sort
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(o: &OptionsOverlay) -> Options {
        let mut opts = Options::default();
        o.apply(&mut opts);
        opts
    }

    #[test]
    fn options_overlay_should_equal_default_when_no_flags_or_config() {
        let opts = applied(&OptionsOverlay::default());
        let d = Options::default();
        assert_eq!(format!("{opts:?}"), format!("{d:?}"));
    }

    #[test]
    fn overlay_should_change_only_threshold_when_threshold_set() {
        let opts = applied(&OptionsOverlay {
            threshold_pct: Some(3.0),
            ..Default::default()
        });
        let want = Options {
            threshold_pct: 3.0,
            ..Options::default()
        };
        assert_eq!(opts.threshold_pct, 3.0);
        assert_eq!(format!("{opts:?}"), format!("{want:?}"));
    }

    #[test]
    fn overlay_should_apply_defaults_then_file_then_flags_when_all_three_set() {
        let file = OptionsOverlay {
            threshold_pct: Some(3.0),
            sort: Some(Sort::Name),
            max_rows: Some(10),
            ..Default::default()
        };
        let flags = OptionsOverlay {
            threshold_pct: Some(5.0), // equal to the default, still wins over the file
            max_rows: Some(2),
            ..Default::default()
        };
        let opts = applied(&file.merge(flags));
        assert_eq!(opts.threshold_pct, 5.0);
        assert!(matches!(opts.sort, Sort::Name));
        assert_eq!(opts.max_rows, Some(2));
    }

    #[test]
    fn fail_on_regression_off_should_clear_a_lower_layer_limit() {
        let file = OptionsOverlay {
            fail_on_regression_pct: Some(5.0),
            ..Default::default()
        };
        let off = OptionsOverlay {
            fail_on_regression_off: true,
            ..Default::default()
        };
        assert_eq!(
            applied(&file.clone().merge(off.clone())).fail_on_regression_pct,
            None
        );
        assert_eq!(applied(&off.merge(file)).fail_on_regression_pct, Some(5.0));
    }

    #[test]
    fn overlay_should_replace_threshold_overrides_when_set() {
        let ov = |k: &str, v: f64| OptionsOverlay {
            threshold_overrides: Some([(k.to_string(), v)].into()),
            ..Default::default()
        };
        let opts = applied(&ov("net", 8.0).merge(ov("net/Parse", 20.0)));
        assert_eq!(opts.threshold_overrides.len(), 1);
        assert_eq!(opts.threshold_overrides["net/Parse"], 20.0);
        assert!(applied(&OptionsOverlay::default())
            .threshold_overrides
            .is_empty());
    }
    fn svg_applied(o: &SvgOptionsOverlay) -> SvgOptions {
        let mut opts = SvgOptions::default();
        o.apply(&mut opts);
        opts
    }

    fn embed(e: EmbedSetting) -> SvgOptionsOverlay {
        SvgOptionsOverlay {
            embed: Some(e),
            ..Default::default()
        }
    }

    #[test]
    fn svg_overlay_should_equal_default_when_nothing_set() {
        assert_eq!(
            svg_applied(&SvgOptionsOverlay::default()),
            SvgOptions::default()
        );
    }

    #[test]
    fn svg_overlay_should_keep_concrete_embed_when_later_layer_is_auto() {
        let charts = SvgOptionsOverlay {
            kinds: Some(vec![ChartKind::Delta]),
            ..Default::default()
        };
        let file = embed(EmbedSetting::False).merge(charts.clone());
        let merged = file.merge(embed(EmbedSetting::Auto));
        assert_eq!(svg_applied(&merged).embed, EmbedMode::Off);
        // A later concrete value still beats an earlier one, and beats Auto.
        let on = embed(EmbedSetting::Auto).merge(embed(EmbedSetting::True));
        assert_eq!(svg_applied(&on).embed, EmbedMode::On);
    }

    #[test]
    fn svg_overlay_should_resolve_auto_from_whether_charts_are_requested() {
        let with = SvgOptionsOverlay {
            kinds: Some(vec![ChartKind::Delta]),
            ..embed(EmbedSetting::Auto)
        };
        assert_eq!(svg_applied(&with).embed, EmbedMode::On);
        assert_eq!(
            svg_applied(&embed(EmbedSetting::Auto)).embed,
            EmbedMode::Off
        );
    }

    #[test]
    fn svg_overlay_should_let_flags_win_over_file_for_scalars() {
        let file = SvgOptionsOverlay {
            rows: Some(ChartRows::new(10).unwrap()),
            publish_images: Some(false),
            ..Default::default()
        };
        let flags = SvgOptionsOverlay {
            rows: Some(ChartRows::new(30).unwrap()),
            ..Default::default()
        };
        let o = svg_applied(&file.merge(flags));
        assert_eq!((o.rows.get(), o.publish_images), (30, false));
    }

    #[test]
    fn matrix_overlay_should_equal_default_when_nothing_set() {
        let mut m = MatrixOptions::default();
        MatrixOptionsOverlay::default().apply(&mut m);
        assert_eq!(format!("{m:?}"), format!("{:?}", MatrixOptions::default()));
    }

    #[test]
    fn matrix_overlay_should_let_flags_win_over_file() {
        let label = |s: &str| VariantLabel::parse(s).unwrap();
        let file = MatrixOptionsOverlay {
            reference: Some(label("quick")),
            cell_format: Some(CellFormat::Ratio),
            sort: Some(MatrixSort::Rank),
            ..Default::default()
        };
        let flags = MatrixOptionsOverlay {
            reference: Some(label("heap")),
            sort: Some(MatrixSort::Name),
            ..Default::default()
        };
        let mut m = MatrixOptions::default();
        file.merge(flags).apply(&mut m);
        assert_eq!(m.reference, Reference::Label(label("heap")));
        assert_eq!(m.cell_format, CellFormat::Ratio);
        assert_eq!(m.sort, MatrixSort::Name);
    }
}
