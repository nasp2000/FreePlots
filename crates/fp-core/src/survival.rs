// SPDX-License-Identifier: MIT
//! Survival analysis: comparison of the exponential, Weibull and lognormal
//! models on right-censored data.
//!
//! The three parametric models are fitted by maximum likelihood to the same
//! observations (the individual fits live in [`crate::reliability`]) and are
//! compared by their maximized log-likelihood, the Akaike and Bayesian
//! information criteria, the percentile lives B1/B5/B10/B50 and the
//! reliability and hazard at a mission time. The Kaplan–Meier product-limit
//! curve is drawn under the three fitted survival curves as the
//! non-parametric reference.
//!
//! - exponential: `S(t) = exp(−t/θ)`, one parameter;
//! - Weibull: `S(t) = exp(−(t/β)^α)`, two parameters;
//! - lognormal: `S(t) = 1 − Φ((ln t − µ)/σ)`, two parameters.
//!
//! Lower AIC/BIC is a better compromise between fit and complexity; the
//! models are not nested, so the criteria rank them and do not test them.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, Series, linear_axis};
use crate::reliability::{self, KaplanMeier};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{normal_cdf, normal_quantile};
use crate::workflow::AnalysisOutput;

/// Parametric model of the comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurvivalModel {
    /// Exponential life with mean `θ`.
    Exponential,
    /// Weibull life with shape `α` and scale `β`.
    Weibull,
    /// Lognormal life with log-mean `µ` and log-deviation `σ`.
    Lognormal,
}

impl SurvivalModel {
    /// Name used in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Exponential => "Exponential",
            Self::Weibull => "Weibull",
            Self::Lognormal => "Lognormal",
        }
    }

    /// Number of parameters (the penalty of the information criteria).
    #[must_use]
    pub fn parameters(self) -> usize {
        match self {
            Self::Exponential => 1,
            Self::Weibull | Self::Lognormal => 2,
        }
    }
}

/// One fitted model of the comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct SurvivalFit {
    /// Model.
    pub model: SurvivalModel,
    /// Parameter names and estimates.
    pub estimates: Vec<(String, f64)>,
    /// Maximized log-likelihood.
    pub log_likelihood: f64,
    /// Akaike information criterion.
    pub aic: f64,
    /// Bayesian information criterion.
    pub bic: f64,
    /// Mean life.
    pub mean_life: f64,
    /// Percentile lives at 1 %, 5 %, 10 % and 50 %.
    pub percentiles: [f64; 4],
    /// Reliability at the mission time.
    pub reliability: f64,
    /// Hazard at the mission time.
    pub hazard: f64,
}

impl SurvivalFit {
    /// Survival function at `t`.
    fn survival(&self, t: f64) -> f64 {
        match self.model {
            SurvivalModel::Exponential => {
                let theta = self.estimates[0].1;
                (-t / theta).exp()
            }
            SurvivalModel::Weibull => {
                let alpha = self.estimates[0].1;
                let beta = self.estimates[1].1;
                (-(t / beta).powf(alpha)).exp()
            }
            SurvivalModel::Lognormal => {
                let mu = self.estimates[0].1;
                let sigma = self.estimates[1].1;
                1.0 - normal_cdf((t.ln() - mu) / sigma)
            }
        }
    }
}
/// Comparison of the three parametric models.
#[derive(Debug, Clone, PartialEq)]
pub struct SurvivalComparison {
    /// Fits ordered by AIC, smallest first.
    pub fits: Vec<SurvivalFit>,
    /// Mission time.
    pub mission: f64,
    /// Kaplan–Meier reference.
    pub kaplan_meier: KaplanMeier,
    /// Observations used.
    pub n: usize,
    /// Events observed.
    pub events: usize,
    /// Right-censored observations.
    pub censored: usize,
    /// Rows excluded.
    pub excluded: usize,
}

const LABELS: [f64; 4] = [0.01, 0.05, 0.10, 0.50];

fn percentiles(model: SurvivalModel, estimates: &[(String, f64)]) -> [f64; 4] {
    LABELS.map(|p| match model {
        SurvivalModel::Exponential => {
            let theta = estimates[0].1;
            -theta * (1.0 - p).ln()
        }
        SurvivalModel::Weibull => {
            let alpha = estimates[0].1;
            let beta = estimates[1].1;
            beta * (-(1.0 - p).ln()).powf(1.0 / alpha)
        }
        SurvivalModel::Lognormal => {
            let mu = estimates[0].1;
            let sigma = estimates[1].1;
            (mu + sigma * normal_quantile(p)).exp()
        }
    })
}

fn hazard(model: SurvivalModel, estimates: &[(String, f64)], t: f64) -> f64 {
    match model {
        SurvivalModel::Exponential => 1.0 / estimates[0].1,
        SurvivalModel::Weibull => {
            let alpha = estimates[0].1;
            let beta = estimates[1].1;
            alpha / beta * (t / beta).powf(alpha - 1.0)
        }
        SurvivalModel::Lognormal => {
            let mu = estimates[0].1;
            let sigma = estimates[1].1;
            let z = (t.ln() - mu) / sigma;
            let density = (-0.5 * z * z).exp() / (t * sigma * (2.0 * std::f64::consts::PI).sqrt());
            let survival = 1.0 - normal_cdf(z);
            density / survival
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build(
    model: SurvivalModel,
    estimates: Vec<(String, f64)>,
    log_likelihood: f64,
    aic: f64,
    mean_life: f64,
    mission: f64,
    n: usize,
) -> SurvivalFit {
    let percentiles = percentiles(model, &estimates);
    let mut fit = SurvivalFit {
        model,
        estimates,
        log_likelihood,
        aic,
        bic: model.parameters() as f64 * (n as f64).ln() - 2.0 * log_likelihood,
        mean_life,
        percentiles,
        reliability: f64::NAN,
        hazard: f64::NAN,
    };
    fit.reliability = fit.survival(mission);
    fit.hazard = hazard(fit.model, &fit.estimates, mission);
    fit
}

/// Compares the three models on right-censored data.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for a mission time that is not finite
/// and positive, and the errors of the individual fits (missing column,
/// non-numeric time, no event, invalid censoring value).
pub fn compare(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
    mission: f64,
) -> Result<SurvivalComparison> {
    if !mission.is_finite() || mission <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "mission time",
            value: mission,
        });
    }
    let kaplan_meier = reliability::fit(df, time_column, censor_column, censor_value, 0.95)?;
    let n = kaplan_meier.n;
    let exponential = reliability::fit_exponential(df, time_column, censor_column, censor_value)?;
    let weibull = reliability::fit_weibull(df, time_column, censor_column, censor_value)?;
    let lognormal = reliability::fit_lognormal(df, time_column, censor_column, censor_value)?;

    let mut fits = Vec::with_capacity(3);
    fits.push(build(
        SurvivalModel::Exponential,
        vec![("θ".to_owned(), exponential.theta)],
        exponential.log_likelihood,
        exponential.aic,
        exponential.theta,
        mission,
        n,
    ));
    fits.push(build(
        SurvivalModel::Weibull,
        vec![
            ("α".to_owned(), weibull.shape),
            ("β".to_owned(), weibull.scale),
        ],
        weibull.log_likelihood,
        weibull.aic,
        weibull.mean_life,
        mission,
        n,
    ));
    fits.push(build(
        SurvivalModel::Lognormal,
        vec![
            ("µ".to_owned(), lognormal.mu),
            ("σ".to_owned(), lognormal.sigma),
        ],
        lognormal.log_likelihood,
        lognormal.aic,
        lognormal.mean_life,
        mission,
        n,
    ));
    fits.sort_by(|left, right| left.aic.total_cmp(&right.aic));

    Ok(SurvivalComparison {
        fits,
        mission,
        n,
        events: kaplan_meier.events,
        censored: kaplan_meier.censored,
        excluded: kaplan_meier.excluded,
        kaplan_meier,
    })
}

/// Builds the report and the survival plot.
///
/// # Errors
///
/// The errors of [`compare`].
pub fn analyze(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
    mission: f64,
) -> Result<AnalysisOutput> {
    let comparison = compare(df, time_column, censor_column, censor_value, mission)?;
    let mut result = AnalysisResult::new(
        format!("Survival Analysis — time {time_column}"),
        "Exponential, Weibull and lognormal maximum likelihood on right-censored data, \
         compared by AIC and BIC",
    );
    result.n = comparison.n;
    result.excluded = comparison.excluded;
    result.statistics = vec![
        ("Events", comparison.events as f64),
        ("Censored", comparison.censored as f64),
        ("Mission time", comparison.mission),
        (
            "Kaplan–Meier median",
            comparison.kaplan_meier.median.unwrap_or(f64::NAN),
        ),
        ("Models", comparison.fits.len() as f64),
    ];
    if let Some(best) = comparison.fits.first() {
        result.extra.push(Item::Message(format!(
            "Best by AIC: {} (AIC {:.3})",
            best.model.name(),
            best.aic
        )));
    }
    result.extra.push(Item::Table {
        headers: vec![
            "Model".to_owned(),
            "Log-likelihood".to_owned(),
            "AIC".to_owned(),
            "BIC".to_owned(),
            "Mean life".to_owned(),
            "B10".to_owned(),
        ],
        rows: comparison
            .fits
            .iter()
            .map(|fit| {
                vec![
                    fit.model.name().to_owned(),
                    format!("{:.6}", fit.log_likelihood),
                    format!("{:.6}", fit.aic),
                    format!("{:.6}", fit.bic),
                    format!("{:.6}", fit.mean_life),
                    format!("{:.6}", fit.percentiles[2]),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Model".to_owned(),
            "Parameters".to_owned(),
            "B1".to_owned(),
            "B5".to_owned(),
            "B10".to_owned(),
            "B50".to_owned(),
        ],
        rows: comparison
            .fits
            .iter()
            .map(|fit| {
                vec![
                    fit.model.name().to_owned(),
                    fit.estimates
                        .iter()
                        .map(|(name, value)| format!("{name} = {value:.6}"))
                        .collect::<Vec<String>>()
                        .join(", "),
                    format!("{:.6}", fit.percentiles[0]),
                    format!("{:.6}", fit.percentiles[1]),
                    format!("{:.6}", fit.percentiles[2]),
                    format!("{:.6}", fit.percentiles[3]),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Model".to_owned(),
            format!("R({})", comparison.mission),
            format!("h({})", comparison.mission),
        ],
        rows: comparison
            .fits
            .iter()
            .map(|fit| {
                vec![
                    fit.model.name().to_owned(),
                    format!("{:.6}", fit.reliability),
                    format!("{:.6}", fit.hazard),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Models".to_owned(),
            "exponential S(t) = exp(−t/θ) with one parameter; Weibull \
             S(t) = exp(−(t/β)^α) and lognormal S(t) = 1 − Φ((ln t − µ)/σ) with two"
                .to_owned(),
        ),
        (
            "Criteria".to_owned(),
            "AIC = 2k − 2ℓ and BIC = k·ln n − 2ℓ, with k the number of parameters; the smallest \
             value is the best compromise between fit and complexity, and the models are not \
             nested, so the criteria rank them without testing them"
                .to_owned(),
        ),
        (
            "Percentiles".to_owned(),
            "Bp is the time at which p of the items have failed: B10 is the time at 10 % \
             failures and B50 the median life of the fitted model"
                .to_owned(),
        ),
        (
            "Mission".to_owned(),
            "R(t) is the fitted reliability at the mission time and h(t) = f(t)/S(t) the \
             instantaneous failure rate; both are model values, not the observed proportion"
                .to_owned(),
        ),
        (
            "Scope".to_owned(),
            "right censoring only, no covariates, no left or interval censoring and no \
             confidence intervals for the percentiles"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(comparison.n, comparison.excluded, 0);
    result.warnings.push(
        "the comparison ranks the three models; it does not show that the best one is \
         adequate, so the fitted curve is drawn over the Kaplan–Meier estimate"
            .to_owned(),
    );
    let plot = survival_plot(&comparison, time_column);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn survival_plot(comparison: &SurvivalComparison, time_label: &str) -> PlotSpec {
    let last = comparison
        .kaplan_meier
        .points
        .last()
        .map_or(1.0, |point| point.time);
    let x_max = last * 1.05;
    let mut series = Vec::with_capacity(4);
    // Kaplan–Meier step: from (0, 1) down at every event time.
    let mut km_points: Vec<Point> = vec![Point { x: 0.0, y: 1.0 }];
    let mut previous = 1.0;
    for point in &comparison.kaplan_meier.points {
        km_points.push(Point {
            x: point.time,
            y: previous,
        });
        km_points.push(Point {
            x: point.time,
            y: point.survival,
        });
        previous = point.survival;
    }
    km_points.push(Point {
        x: x_max,
        y: previous,
    });
    series.push(Series {
        label: "Kaplan–Meier".to_owned(),
        points: km_points,
        lines: Vec::new(),
        bands: Vec::new(),
        secondary_axis: false,
    });
    for fit in &comparison.fits {
        let mut points = Vec::with_capacity(120);
        for step in 0..120 {
            let share = step as f64 / 119.0;
            let t = x_max * share;
            points.push(Point {
                x: t,
                y: fit.survival(t),
            });
        }
        series.push(Series {
            label: fit.model.name().to_owned(),
            points,
            lines: Vec::new(),
            bands: Vec::new(),
            secondary_axis: false,
        });
    }
    // The renderer joins the points of a series when they come with line
    // segments; build them once for every series.
    for entry in &mut series {
        for pair in entry.points.windows(2) {
            entry.lines.push(Segment {
                from: pair[0],
                to: pair[1],
                width: 2.0,
            });
        }
    }
    PlotSpec {
        title: "Survival Analysis".to_owned(),
        x_axis: linear_axis(time_label, 0.0, x_max),
        y_axis: linear_axis("Survival", 0.0, 1.0),
        size: (900, 560),
        points: Vec::new(),
        lines: Vec::new(),
        bands: Vec::new(),
        series,
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    /// Twelve times with the last three censored (status 0).
    fn censored_frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64(
                "time",
                vec![
                    1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 15.0,
                ],
            ),
            Column::new_f64(
                "status",
                vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0],
            ),
        ])
        .expect("frame")
    }

    #[test]
    fn the_exponential_fit_matches_the_closed_form() {
        let df = censored_frame();
        let comparison = compare(&df, "time", Some("status"), 0.0, 5.0).expect("compare");
        assert_eq!(comparison.n, 12);
        assert_eq!(comparison.events, 9);
        assert_eq!(comparison.censored, 3);
        let exponential = comparison
            .fits
            .iter()
            .find(|fit| fit.model == SurvivalModel::Exponential)
            .expect("exponential");
        // Total time on test 82 over 9 events.
        let theta = 82.0 / 9.0;
        assert!((exponential.estimates[0].1 - theta).abs() < 1e-9);
        // ℓ = −r·ln θ − r; AIC = 2 − 2ℓ; BIC = ln 12 − 2ℓ.
        let expected = -9.0 * theta.ln() - 9.0;
        assert!((exponential.log_likelihood - expected).abs() < 1e-9);
        assert!((exponential.aic - (2.0 - 2.0 * expected)).abs() < 1e-9);
        assert!(
            (exponential.bic - (12f64.ln() - 2.0 * expected)).abs() < 1e-9,
            "{}",
            exponential.bic
        );
        // B10 = −θ·ln 0.9 and B50 = θ·ln 2.
        assert!((exponential.percentiles[2] - (-theta * 0.9f64.ln())).abs() < 1e-9);
        assert!((exponential.percentiles[3] - theta * 2f64.ln()).abs() < 1e-9);
        // R(5) = exp(−5/θ) and h = 1/θ.
        assert!((exponential.reliability - (-5.0 / theta).exp()).abs() < 1e-12);
        assert!((exponential.hazard - 1.0 / theta).abs() < 1e-12);
    }

    #[test]
    fn the_weibull_and_lognormal_percentiles_follow_their_models() {
        let df = censored_frame();
        let comparison = compare(&df, "time", Some("status"), 0.0, 5.0).expect("compare");
        assert_eq!(comparison.fits.len(), 3);
        for fit in &comparison.fits {
            // Every model has increasing percentiles.
            assert!(fit.percentiles[0] < fit.percentiles[1]);
            assert!(fit.percentiles[1] < fit.percentiles[2]);
            assert!(fit.percentiles[2] < fit.percentiles[3]);
            // Reliability falls with the mission time.
            let later = compare(&df, "time", Some("status"), 0.0, 10.0).expect("compare");
            let found = later
                .fits
                .iter()
                .find(|other| other.model == fit.model)
                .expect("model");
            assert!(found.reliability < fit.reliability, "{}", fit.model.name());
        }
        let weibull = comparison
            .fits
            .iter()
            .find(|fit| fit.model == SurvivalModel::Weibull)
            .expect("weibull");
        let alpha = weibull.estimates[0].1;
        let beta = weibull.estimates[1].1;
        assert!((weibull.percentiles[2] - beta * (-0.9f64.ln()).powf(1.0 / alpha)).abs() < 1e-9);
        let lognormal = comparison
            .fits
            .iter()
            .find(|fit| fit.model == SurvivalModel::Lognormal)
            .expect("lognormal");
        let mu = lognormal.estimates[0].1;
        let sigma = lognormal.estimates[1].1;
        assert!(
            (lognormal.percentiles[2] - (mu + sigma * normal_quantile(0.10)).exp()).abs() < 1e-9
        );
        // The fits are ordered by AIC.
        assert!(comparison.fits[0].aic <= comparison.fits[1].aic);
        assert!(comparison.fits[1].aic <= comparison.fits[2].aic);
    }

    #[test]
    fn the_comparison_reports_the_mission_metrics_and_plot() {
        let df = censored_frame();
        let output = analyze(&df, "time", Some("status"), 0.0, 4.0).expect("analysis");
        let text = output.results[0].to_session_text();
        assert!(text.contains("Survival Analysis — time time"), "{text}");
        assert!(text.contains("Best by AIC"), "{text}");
        assert!(text.contains("B10"), "{text}");
        assert!(text.contains("R(4)"), "{text}");
        let plot = output.plot.expect("plot");
        assert_eq!(plot.series.len(), 4);
        assert_eq!(plot.series[0].label, "Kaplan–Meier");
        assert!(plot.series[1].lines.len() > 50);
    }

    #[test]
    fn the_comparison_rejects_invalid_requests() {
        let df = censored_frame();
        assert!(compare(&df, "time", Some("status"), 0.0, 0.0).is_err());
        assert!(compare(&df, "time", Some("status"), 0.0, f64::NAN).is_err());
        assert!(compare(&df, "missing", None, 0.0, 1.0).is_err());
        // A text time column.
        let words = DataFrame::new(vec![
            Column::new_text(
                "time",
                ["1", "2", "3"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
            Column::new_f64("status", vec![1.0, 1.0, 0.0]),
        ])
        .expect("frame");
        assert!(compare(&words, "time", Some("status"), 0.0, 1.0).is_err());
    }
}
