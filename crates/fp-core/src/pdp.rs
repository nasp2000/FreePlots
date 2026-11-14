// SPDX-License-Identifier: MIT
//! Probability distribution plot: a theoretical curve without data.
//!
//! The plot draws the density or the cumulative probability of a distribution
//! whose parameters the user supplies, over the central 99.9 % of its support,
//! and optionally shades the area between two values of `x` with the
//! probability `F(b) − F(a)`.

use crate::dists::{
    Distribution, Exponential, ExtremeValue, Gamma, Logistic, Loglogistic, Lognormal, Lognormal3,
    Normal, Weibull, Weibull3,
};
use crate::error::{Error, Result};
use crate::plot::{Band, PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item};
use crate::workflow::{AnalysisOutput, DistributionKind};

/// Quantity drawn on the vertical axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdpView {
    /// Probability density.
    Density,
    /// Cumulative probability.
    Cumulative,
}

impl PdpView {
    /// Name used by the interface and the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Density => "Density",
            Self::Cumulative => "Cumulative probability",
        }
    }
}

/// Request of a probability distribution plot.
#[derive(Debug, Clone, PartialEq)]
pub struct PdpOptions {
    /// Distribution family.
    pub kind: DistributionKind,
    /// Constructor parameters, in the family's order.
    pub parameters: Vec<f64>,
    /// Density or cumulative view.
    pub view: PdpView,
    /// Optional shaded interval, in `x` units.
    pub shade: Option<(f64, f64)>,
}

/// Number of parameters of a family.
#[must_use]
pub fn parameter_count(kind: DistributionKind) -> usize {
    match kind {
        DistributionKind::Normal
        | DistributionKind::Lognormal
        | DistributionKind::Weibull
        | DistributionKind::Logistic
        | DistributionKind::Loglogistic
        | DistributionKind::Gamma
        | DistributionKind::ExtremeValue => 2,
        DistributionKind::Exponential => 1,
        DistributionKind::Weibull3 | DistributionKind::Lognormal3 => 3,
    }
}

/// Parameter labels, in the family's order.
#[must_use]
pub fn parameter_labels(kind: DistributionKind) -> &'static [&'static str] {
    match kind {
        DistributionKind::Normal => &["Mean:", "Standard deviation:"],
        DistributionKind::Lognormal => &["Location (ln):", "Scale (ln):"],
        DistributionKind::Weibull => &["Shape:", "Scale:"],
        DistributionKind::Exponential => &["Mean:"],
        DistributionKind::Logistic => &["Location:", "Scale:"],
        DistributionKind::Loglogistic => &["Scale:", "Shape:"],
        DistributionKind::Gamma => &["Shape:", "Scale:"],
        DistributionKind::ExtremeValue => &["Location:", "Scale:"],
        DistributionKind::Weibull3 => &["Threshold:", "Shape:", "Scale:"],
        DistributionKind::Lognormal3 => &["Threshold:", "Location (ln):", "Scale (ln):"],
    }
}

/// Default parameters of a family.
#[must_use]
pub fn default_parameters(kind: DistributionKind) -> Vec<f64> {
    match kind {
        DistributionKind::Normal | DistributionKind::Lognormal | DistributionKind::Logistic => {
            vec![0.0, 1.0]
        }
        DistributionKind::Exponential => vec![1.0],
        DistributionKind::Weibull | DistributionKind::Loglogistic | DistributionKind::Gamma => {
            vec![1.0, 1.0]
        }
        DistributionKind::ExtremeValue => vec![0.0, 1.0],
        DistributionKind::Weibull3 => vec![0.0, 1.0, 1.0],
        DistributionKind::Lognormal3 => vec![0.0, 0.0, 1.0],
    }
}

/// Builds the distribution from its constructor parameters.
pub fn distribution(kind: DistributionKind, parameters: &[f64]) -> Result<Box<dyn Distribution>> {
    if parameters.len() != parameter_count(kind) {
        return Err(Error::Unsupported(format!(
            "{} needs {} parameter(s)",
            kind.name(),
            parameter_count(kind)
        )));
    }
    let p = parameters;
    match kind {
        DistributionKind::Normal => Ok(Box::new(Normal::new(p[0], p[1])?)),
        DistributionKind::Lognormal => Ok(Box::new(Lognormal::new(p[0], p[1])?)),
        DistributionKind::Weibull => Ok(Box::new(Weibull::new(p[0], p[1])?)),
        DistributionKind::Exponential => Ok(Box::new(Exponential::new(p[0])?)),
        DistributionKind::Logistic => Ok(Box::new(Logistic::new(p[0], p[1])?)),
        DistributionKind::Loglogistic => Ok(Box::new(Loglogistic::new(p[0], p[1])?)),
        DistributionKind::Gamma => Ok(Box::new(Gamma::new(p[0], p[1])?)),
        DistributionKind::ExtremeValue => Ok(Box::new(ExtremeValue::new(p[0], p[1])?)),
        DistributionKind::Weibull3 => Ok(Box::new(Weibull3::new(p[0], p[1], p[2])?)),
        DistributionKind::Lognormal3 => Ok(Box::new(Lognormal3::new(p[0], p[1], p[2])?)),
    }
}

/// Builds the plot and its report.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for a wrong parameter count or a shade with
/// equal bounds, and the distribution's own parameter errors.
pub fn build(options: &PdpOptions) -> Result<AnalysisOutput> {
    let dist = distribution(options.kind, &options.parameters)?;
    let lower_support = match options.kind {
        DistributionKind::Weibull
        | DistributionKind::Lognormal
        | DistributionKind::Loglogistic
        | DistributionKind::Gamma
        | DistributionKind::Exponential => Some(0.0),
        DistributionKind::Weibull3 | DistributionKind::Lognormal3 => Some(options.parameters[0]),
        DistributionKind::Normal | DistributionKind::Logistic | DistributionKind::ExtremeValue => {
            None
        }
    };
    let mut x_min = dist.quantile(0.0005);
    let mut x_max = dist.quantile(0.9995);
    if let Some(lower) = lower_support {
        x_min = x_min.max(lower);
        let span = (x_max - lower).abs().max(1.0);
        x_min = x_min.max(lower + span * 1e-9);
    }
    if !x_min.is_finite() || !x_max.is_finite() || x_max <= x_min {
        return Err(Error::Unsupported(
            "the distribution has no finite plotting range".to_owned(),
        ));
    }
    let steps = 400usize;
    let mut points = Vec::with_capacity(steps + 1);
    for index in 0..=steps {
        let x = x_min + (x_max - x_min) * index as f64 / steps as f64;
        let y = match options.view {
            PdpView::Density => dist.pdf(x),
            PdpView::Cumulative => dist.cdf(x),
        };
        points.push(Point { x, y });
    }
    let lines: Vec<Segment> = points
        .windows(2)
        .map(|pair| Segment {
            from: pair[0],
            to: pair[1],
            width: 2.0,
        })
        .collect();
    let mut bands = Vec::new();
    let mut shade = None;
    if let Some((first, second)) = options.shade {
        let (low, high) = if first <= second {
            (first, second)
        } else {
            (second, first)
        };
        let probability = (dist.cdf(high) - dist.cdf(low)).clamp(0.0, 1.0);
        shade = Some((low, high, probability));
        let clipped_low = low.max(x_min);
        let clipped_high = high.min(x_max);
        if options.view == PdpView::Density && clipped_high > clipped_low {
            let shade_steps = 120usize;
            let mut lower = Vec::with_capacity(shade_steps + 1);
            let mut upper = Vec::with_capacity(shade_steps + 1);
            for index in 0..=shade_steps {
                let x =
                    clipped_low + (clipped_high - clipped_low) * index as f64 / shade_steps as f64;
                lower.push(Point { x, y: 0.0 });
                upper.push(Point { x, y: dist.pdf(x) });
            }
            bands.push(Band { lower, upper });
        }
    }

    let mut result = AnalysisResult::new(
        format!("Probability Distribution Plot — {}", options.kind.name()),
        format!(
            "Theoretical {}; no data are used",
            options.view.name().to_lowercase()
        ),
    );
    let parameters = dist.parameters();
    result.statistics = parameters.clone();
    if let Some((low, high, probability)) = shade {
        result.statistics.push(("Shaded lower", low));
        result.statistics.push(("Shaded upper", high));
        result.statistics.push(("Shaded probability", probability));
    }
    result.extra.push(Item::Table {
        headers: vec!["Parameter".to_owned(), "Value".to_owned()],
        rows: parameters
            .iter()
            .map(|(name, value)| vec![(*name).to_owned(), format!("{value:.6}")])
            .collect(),
    });
    result.notes = vec![
        (
            "Curve".to_owned(),
            format!(
                "the {} is drawn over the central 99.9 % of the distribution (x from {x_min:.6} \
                 to {x_max:.6}) with {steps} intervals",
                options.view.name().to_lowercase()
            ),
        ),
        (
            "Shaded area".to_owned(),
            "P(a < X < b) = F(b) − F(a); the area is filled under the density curve only"
                .to_owned(),
        ),
        (
            "Data".to_owned(),
            "the plot is theoretical: no worksheet column is used".to_owned(),
        ),
    ];

    let plot = PlotSpec {
        title: format!("Probability Distribution Plot — {}", options.kind.name()),
        x_axis: linear_axis("x", x_min, x_max),
        y_axis: linear_axis(
            match options.view {
                PdpView::Density => "Density",
                PdpView::Cumulative => "Cumulative probability",
            },
            0.0,
            match options.view {
                PdpView::Density => {
                    points.iter().map(|point| point.y).fold(0.0_f64, f64::max) * 1.05
                }
                PdpView::Cumulative => 1.02,
            },
        ),
        size: (900, 520),
        points: Vec::new(),
        lines,
        bands,
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: Vec::new(),
    };
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(kind: DistributionKind, parameters: Vec<f64>, view: PdpView) -> PdpOptions {
        PdpOptions {
            kind,
            parameters,
            view,
            shade: None,
        }
    }

    #[test]
    fn the_normal_density_peaks_at_the_mean() {
        let mut request = options(DistributionKind::Normal, vec![0.0, 1.0], PdpView::Density);
        request.shade = Some((-1.0, 1.0));
        let output = build(&request).expect("plot");
        let result = &output.results[0];
        assert!(
            result.title.contains("Probability Distribution Plot"),
            "{}",
            result.title
        );
        let probability = result
            .statistics
            .iter()
            .find(|(name, _)| *name == "Shaded probability")
            .map(|(_, value)| *value)
            .expect("probability");
        assert!(
            (probability - 0.682_689_492_137_085_9).abs() < 1e-9,
            "{probability}"
        );
        let plot = output.plot.expect("plot");
        assert_eq!(plot.lines.len(), 400);
        assert_eq!(plot.bands.len(), 1);
        let peak = plot
            .lines
            .iter()
            .map(|segment| segment.to.y)
            .fold(0.0_f64, f64::max);
        assert!((peak - 0.398_942_280_401_432_7).abs() < 1e-4, "{peak}");
        assert!(plot.bands[0].upper.iter().all(|point| point.y >= 0.0));
    }

    #[test]
    fn the_cumulative_view_reaches_one_half_at_the_mean() {
        let output = build(&options(
            DistributionKind::Normal,
            vec![0.0, 1.0],
            PdpView::Cumulative,
        ))
        .expect("plot");
        let plot = output.plot.expect("plot");
        let middle = &plot.lines[plot.lines.len() / 2];
        assert!((middle.to.y - 0.5).abs() < 1e-3, "{}", middle.to.y);
        assert!(plot.bands.is_empty());
    }

    #[test]
    fn every_family_builds_a_finite_curve() {
        for kind in [
            DistributionKind::Normal,
            DistributionKind::Lognormal,
            DistributionKind::Weibull,
            DistributionKind::Exponential,
            DistributionKind::Logistic,
            DistributionKind::Loglogistic,
            DistributionKind::Gamma,
            DistributionKind::ExtremeValue,
            DistributionKind::Weibull3,
            DistributionKind::Lognormal3,
        ] {
            let output = build(&options(kind, default_parameters(kind), PdpView::Density))
                .unwrap_or_else(|error| panic!("{}: {error}", kind.name()));
            let plot = output.plot.expect("plot");
            assert!(
                plot.lines.iter().all(|segment| segment.to.y.is_finite()),
                "{} has a non-finite density",
                kind.name()
            );
        }
    }

    #[test]
    fn the_threshold_bounds_the_plot() {
        let output = build(&options(
            DistributionKind::Weibull3,
            vec![5.0, 2.0, 1.0],
            PdpView::Density,
        ))
        .expect("plot");
        let plot = output.plot.expect("plot");
        assert!(plot.x_axis.min >= 5.0, "{}", plot.x_axis.min);
    }

    #[test]
    fn invalid_parameters_and_shades_are_rejected() {
        assert!(
            build(&options(
                DistributionKind::Normal,
                vec![0.0],
                PdpView::Density
            ))
            .is_err()
        );
        assert!(
            build(&options(
                DistributionKind::Normal,
                vec![0.0, 0.0],
                PdpView::Density
            ))
            .is_err()
        );
        let mut request = options(DistributionKind::Normal, vec![0.0, 1.0], PdpView::Density);
        request.shade = Some((3.0, 1.0));
        let output = build(&request).expect("reversed shade");
        let probability = output.results[0]
            .statistics
            .iter()
            .find(|(name, _)| *name == "Shaded probability")
            .map(|(_, value)| *value)
            .expect("probability");
        assert!((probability - 0.157_305_339_016_251_36).abs() < 1e-9);
    }
}
