// SPDX-License-Identifier: MIT
//! ARIMA(p,d,q) by conditional sum of squares, with forecasts and intervals.
//!
//! The series is differenced `d` times and centred. The AR and MA coefficients
//! are estimated by minimising the conditional sum of squared one-step errors
//! with the Nelder–Mead simplex. The parameters are reflection coefficients,
//! mapped to AR/MA coefficients by the Durbin–Levinson recursion, so every
//! candidate is stationary and invertible. The forecast integrates the
//! differences back to the original scale; the interval uses the ψ weights of
//! the integrated model.
//!
//! The best order searches `p, d, q ∈ {0, 1, 2}` and keeps the smallest AICc.

use crate::data::{DataFrame, MissingPolicy, Sample};
use crate::error::{Error, Result};
use crate::optimize::nelder_mead;
use crate::plot::{Band, PlotSpec, Point, Segment, Series, linear_axis, plot_stride};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::normal_quantile;
use crate::workflow::AnalysisOutput;

/// Largest AR or MA order offered.
pub const MAX_ARIMA_ORDER: usize = 2;
/// Largest differencing order offered.
pub const MAX_ARIMA_DIFFERENCING: usize = 2;
/// Largest forecast horizon.
pub const MAX_ARIMA_FORECAST: usize = 120;

/// ARIMA order and forecast request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArimaOptions {
    /// AR order.
    pub p: usize,
    /// Differencing order.
    pub d: usize,
    /// MA order.
    pub q: usize,
    /// Forecast horizon.
    pub horizon: usize,
}

impl Default for ArimaOptions {
    fn default() -> Self {
        Self {
            p: 1,
            d: 0,
            q: 0,
            horizon: 12,
        }
    }
}

/// One candidate order of the best-ARIMA search.
#[derive(Debug, Clone, PartialEq)]
pub struct ArimaCandidate {
    /// Order.
    pub order: (usize, usize, usize),
    /// AICc, or NaN when the order could not be fitted.
    pub aicc: f64,
}

/// Fitted ARIMA model.
#[derive(Debug, Clone, PartialEq)]
pub struct ArimaFit {
    /// AR order.
    pub p: usize,
    /// Differencing order.
    pub d: usize,
    /// MA order.
    pub q: usize,
    /// Mean of the differenced series.
    pub mean: f64,
    /// AR coefficients.
    pub ar: Vec<f64>,
    /// MA coefficients.
    pub ma: Vec<f64>,
    /// Residual variance.
    pub sigma2: f64,
    /// Conditional Gaussian log-likelihood.
    pub log_likelihood: f64,
    /// Akaike information criterion.
    pub aic: f64,
    /// Small-sample AIC.
    pub aicc: f64,
    /// Bayesian information criterion.
    pub bic: f64,
    /// Observations used.
    pub n: usize,
    /// Rows removed as missing.
    pub excluded: usize,
    /// Fitted values on the original scale (the first `d` rows are NaN).
    pub fitted: Vec<f64>,
    /// Residuals on the original scale (the first `d` rows are NaN).
    pub residuals: Vec<f64>,
    /// Forecasts on the original scale.
    pub forecast: Vec<f64>,
    /// Standard error of each forecast.
    pub forecast_se: Vec<f64>,
    /// Lower forecast bounds.
    pub lower: Vec<f64>,
    /// Upper forecast bounds.
    pub upper: Vec<f64>,
    /// Confidence level.
    pub confidence: f64,
}

fn difference(values: &[f64], order: usize) -> Vec<Vec<f64>> {
    let mut levels = Vec::with_capacity(order + 1);
    levels.push(values.to_vec());
    for _ in 0..order {
        let previous = levels.last().expect("level");
        let next: Vec<f64> = previous.windows(2).map(|pair| pair[1] - pair[0]).collect();
        levels.push(next);
    }
    levels
}

/// Durbin–Levinson step-up: reflection coefficients to AR coefficients.
fn reflection_to_ar(reflection: &[f64]) -> Vec<f64> {
    let mut phi: Vec<f64> = Vec::with_capacity(reflection.len());
    for &kappa in reflection {
        let order = phi.len();
        let mut next = vec![0.0; order + 1];
        for j in 0..order {
            next[j] = phi[j] - kappa * phi[order - 1 - j];
        }
        next[order] = kappa;
        phi = next;
    }
    phi
}

/// Conditional one-step errors from a centred series.
fn conditional_errors(z: &[f64], mean: f64, ar: &[f64], ma: &[f64]) -> (f64, Vec<f64>, usize) {
    let n = z.len();
    let start = ar.len().max(ma.len());
    let mut errors = vec![0.0; n];
    let mut sse = 0.0;
    for t in start..n {
        let mut prediction = mean;
        for (i, coefficient) in ar.iter().enumerate() {
            prediction += coefficient * (z[t - 1 - i] - mean);
        }
        for (j, coefficient) in ma.iter().enumerate() {
            prediction += coefficient * errors[t - 1 - j];
        }
        errors[t] = z[t] - prediction;
        sse += errors[t] * errors[t];
    }
    (sse, errors, n - start)
}

/// Fits one order to a numeric column.
fn fit_sample(sample: &Sample, options: ArimaOptions, confidence: f64) -> Result<ArimaFit> {
    if options.p > MAX_ARIMA_ORDER || options.q > MAX_ARIMA_ORDER {
        return Err(Error::Unsupported(format!(
            "the AR and MA orders are at most {MAX_ARIMA_ORDER}"
        )));
    }
    if options.d > MAX_ARIMA_DIFFERENCING {
        return Err(Error::Unsupported(format!(
            "the differencing order is at most {MAX_ARIMA_DIFFERENCING}"
        )));
    }
    if options.horizon == 0 || options.horizon > MAX_ARIMA_FORECAST {
        return Err(Error::Unsupported(format!(
            "the forecast horizon must be between 1 and {MAX_ARIMA_FORECAST}"
        )));
    }
    if !(0.5..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence",
            value: confidence,
        });
    }
    let values = sample.values();
    if values.len() < 10 + options.d {
        return Err(Error::SampleTooSmall {
            n: values.len(),
            min: 10 + options.d,
        });
    }
    let levels = difference(values, options.d);
    let z = levels.last().expect("level");
    if z.len() < 6 {
        return Err(Error::SampleTooSmall { n: z.len(), min: 6 });
    }
    let mean = z.iter().sum::<f64>() / z.len() as f64;
    let (ar, ma) = if options.p + options.q == 0 {
        (Vec::new(), Vec::new())
    } else {
        let dimension = options.p + options.q;
        let start = vec![0.05; dimension];
        let steps = vec![0.1; dimension];
        let minimum = nelder_mead(&start, &steps, 800, 1e-10, |parameters| {
            let clamped: Vec<f64> = parameters
                .iter()
                .map(|value| value.clamp(-0.98, 0.98))
                .collect();
            let ar = reflection_to_ar(&clamped[..options.p]);
            let reflection = reflection_to_ar(&clamped[options.p..]);
            let ma: Vec<f64> = reflection.iter().map(|value| -value).collect();
            let (sse, _, terms) = conditional_errors(z, mean, &ar, &ma);
            if terms == 0 || !sse.is_finite() {
                f64::MAX
            } else {
                sse
            }
        })?;
        let clamped: Vec<f64> = minimum
            .arguments
            .iter()
            .map(|value| value.clamp(-0.98, 0.98))
            .collect();
        let ar = reflection_to_ar(&clamped[..options.p]);
        let reflection = reflection_to_ar(&clamped[options.p..]);
        let ma: Vec<f64> = reflection.iter().map(|value| -value).collect();
        (ar, ma)
    };
    let (sse, errors, terms) = conditional_errors(z, mean, &ar, &ma);
    if terms <= options.p + options.q + 1 || !sse.is_finite() || sse <= 0.0 {
        return Err(Error::NotConverged {
            detail: "the ARIMA conditional sum of squares has no residual variance".to_owned(),
        });
    }
    let sigma2 = sse / terms as f64;
    let log_likelihood = -0.5 * terms as f64 * ((2.0 * std::f64::consts::PI * sigma2).ln() + 1.0);
    let k = options.p + options.q + 2;
    let aic = -2.0 * log_likelihood + 2.0 * k as f64;
    let aicc = if terms as f64 > k as f64 + 1.0 {
        aic + 2.0 * k as f64 * (k as f64 + 1.0) / (terms as f64 - k as f64 - 1.0)
    } else {
        f64::NAN
    };
    let bic = -2.0 * log_likelihood + k as f64 * (terms as f64).ln();

    // Fitted values and residuals on the original scale.
    let mut fitted = vec![f64::NAN; z.len()];
    for t in 0..z.len() {
        fitted[t] = z[t] - errors[t];
    }
    let mut original_fitted = vec![f64::NAN; values.len()];
    let offset = values.len() - z.len();
    for (index, value) in fitted.iter().enumerate() {
        original_fitted[offset + index] = *value;
    }
    let mut original_residuals = vec![f64::NAN; values.len()];
    for (index, value) in errors.iter().enumerate() {
        original_residuals[offset + index] = *value;
    }

    // Forecast the differenced series.
    let mut z_work = z.clone();
    let mut e_work = errors.clone();
    let mut future = Vec::with_capacity(options.horizon);
    for _ in 0..options.horizon {
        let t = z_work.len();
        let mut prediction = mean;
        for (i, coefficient) in ar.iter().enumerate() {
            prediction += coefficient * (z_work[t - 1 - i] - mean);
        }
        for (j, coefficient) in ma.iter().enumerate() {
            prediction += coefficient * e_work[t - 1 - j];
        }
        z_work.push(prediction);
        e_work.push(0.0);
        future.push(prediction);
    }
    // Integrate the differences back to the original scale.
    let mut forecasts = future;
    for level in (0..options.d).rev() {
        let last = levels[level].last().copied().unwrap_or(0.0);
        let mut cumulative = 0.0;
        forecasts = forecasts
            .into_iter()
            .map(|value| {
                cumulative += value;
                last + cumulative
            })
            .collect();
    }
    // ψ weights of the integrated model: (1 − φ(B))(1 − B)^d ψ(B) = θ(B).
    let mut polynomial = vec![1.0];
    for _ in 0..options.d {
        let mut next = vec![0.0; polynomial.len() + 1];
        for (index, value) in polynomial.iter().enumerate() {
            next[index] += value;
            next[index + 1] -= value;
        }
        polynomial = next;
    }
    let mut with_ar = vec![0.0; polynomial.len() + ar.len()];
    for (index, value) in polynomial.iter().enumerate() {
        with_ar[index] += value;
        for (j, coefficient) in ar.iter().enumerate() {
            with_ar[index + j + 1] -= coefficient * value;
        }
    }
    let mut psi = vec![0.0; options.horizon];
    for j in 0..options.horizon {
        let mut value = if j == 0 {
            1.0
        } else if j <= ma.len() {
            ma[j - 1]
        } else {
            0.0
        };
        if j > 0 {
            for i in 1..with_ar.len().min(j + 1) {
                value -= with_ar[i] * psi[j - i];
            }
        }
        psi[j] = value;
    }
    let mut cumulative_variance = 0.0;
    let z_value = normal_quantile(0.5 + confidence / 2.0);
    let forecast_se: Vec<f64> = psi
        .iter()
        .map(|weight| {
            cumulative_variance += sigma2 * weight * weight;
            cumulative_variance.sqrt()
        })
        .collect();
    let lower: Vec<f64> = forecasts
        .iter()
        .zip(&forecast_se)
        .map(|(value, se)| value - z_value * se)
        .collect();
    let upper: Vec<f64> = forecasts
        .iter()
        .zip(&forecast_se)
        .map(|(value, se)| value + z_value * se)
        .collect();

    Ok(ArimaFit {
        p: options.p,
        d: options.d,
        q: options.q,
        mean,
        ar,
        ma,
        sigma2,
        log_likelihood,
        aic,
        aicc,
        bic,
        n: values.len(),
        excluded: sample.excluded(),
        fitted: original_fitted,
        residuals: original_residuals,
        forecast: forecasts,
        forecast_se,
        lower,
        upper,
        confidence,
    })
}

/// Fits an ARIMA model to one column.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for an order, horizon or confidence outside
/// the supported range, and sample or convergence errors.
pub fn fit(
    df: &DataFrame,
    column: &str,
    options: ArimaOptions,
    confidence: f64,
) -> Result<ArimaFit> {
    let sample = df.column(column)?.sample(MissingPolicy::Drop)?;
    fit_sample(&sample, options, confidence)
}

/// Fits an ARIMA model and builds the report and the forecast plot.
///
/// # Errors
///
/// As [`fit`].
pub fn analyze(
    df: &DataFrame,
    column: &str,
    options: ArimaOptions,
    confidence: f64,
) -> Result<AnalysisOutput> {
    let model = fit(df, column, options, confidence)?;
    Ok(AnalysisOutput {
        results: vec![report(column, &model, None)],
        plot: Some(plot(column, df, &model)),
    })
}

/// Searches every order in `0..=2` for `p`, `d` and `q` and keeps the lowest
/// AICc.
///
/// # Errors
///
/// Returns [`Error::NotConverged`] when no order could be fitted, and the
/// errors of [`fit`] beyond the order search.
pub fn best(
    df: &DataFrame,
    column: &str,
    horizon: usize,
    confidence: f64,
) -> Result<AnalysisOutput> {
    let sample = df.column(column)?.sample(MissingPolicy::Drop)?;
    let mut candidates = Vec::new();
    let mut best: Option<ArimaFit> = None;
    for d in 0..=MAX_ARIMA_DIFFERENCING {
        for p in 0..=MAX_ARIMA_ORDER {
            for q in 0..=MAX_ARIMA_ORDER {
                let options = ArimaOptions { p, d, q, horizon };
                match fit_sample(&sample, options, confidence) {
                    Ok(model) => {
                        candidates.push(ArimaCandidate {
                            order: (p, d, q),
                            aicc: model.aicc,
                        });
                        let better = model.aicc.is_finite()
                            && best
                                .as_ref()
                                .is_none_or(|current| model.aicc < current.aicc);
                        if better {
                            best = Some(model);
                        }
                    }
                    Err(_) => candidates.push(ArimaCandidate {
                        order: (p, d, q),
                        aicc: f64::NAN,
                    }),
                }
            }
        }
    }
    let Some(model) = best else {
        return Err(Error::NotConverged {
            detail: "no ARIMA order could be fitted".to_owned(),
        });
    };
    Ok(AnalysisOutput {
        results: vec![report(column, &model, Some(&candidates))],
        plot: Some(plot(column, df, &model)),
    })
}

fn report(column: &str, model: &ArimaFit, candidates: Option<&[ArimaCandidate]>) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!(
            "ARIMA({},{},{}) — column {column}",
            model.p, model.d, model.q
        ),
        "Conditional sum of squares; Gaussian likelihood; forecast intervals from the ψ weights"
            .to_owned(),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("AR terms", model.p as f64),
        ("MA terms", model.q as f64),
        ("Differencing", model.d as f64),
        ("Mean", model.mean),
        ("Residual variance", model.sigma2),
        ("Log-likelihood", model.log_likelihood),
        ("AIC", model.aic),
        ("AICc", model.aicc),
        ("BIC", model.bic),
        ("Forecasts", model.forecast.len() as f64),
    ];
    let mut coefficients = vec![vec!["Mean".to_owned(), format!("{:.6}", model.mean)]];
    for (index, coefficient) in model.ar.iter().enumerate() {
        coefficients.push(vec![
            format!("AR{}", index + 1),
            format!("{coefficient:.6}"),
        ]);
    }
    for (index, coefficient) in model.ma.iter().enumerate() {
        coefficients.push(vec![
            format!("MA{}", index + 1),
            format!("{coefficient:.6}"),
        ]);
    }
    result.extra.push(Item::Table {
        headers: vec!["Term".to_owned(), "Estimate".to_owned()],
        rows: coefficients,
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Step".to_owned(),
            "Forecast".to_owned(),
            "SE".to_owned(),
            "Lower".to_owned(),
            "Upper".to_owned(),
        ],
        rows: model
            .forecast
            .iter()
            .enumerate()
            .map(|(index, value)| {
                vec![
                    (index + 1).to_string(),
                    format!("{value:.6}"),
                    format!("{:.6}", model.forecast_se[index]),
                    format!("{:.6}", model.lower[index]),
                    format!("{:.6}", model.upper[index]),
                ]
            })
            .collect(),
    });
    if let Some(candidates) = candidates {
        let mut rows: Vec<Vec<String>> = candidates
            .iter()
            .filter(|candidate| candidate.aicc.is_finite())
            .map(|candidate| {
                vec![
                    format!(
                        "({},{},{})",
                        candidate.order.0, candidate.order.1, candidate.order.2
                    ),
                    format!("{:.3}", candidate.aicc),
                ]
            })
            .collect();
        rows.sort_by(|left, right| {
            left[1]
                .parse::<f64>()
                .unwrap_or(f64::INFINITY)
                .total_cmp(&right[1].parse::<f64>().unwrap_or(f64::INFINITY))
        });
        rows.truncate(10);
        result.extra.push(Item::Table {
            headers: vec!["Order (p,d,q)".to_owned(), "AICc".to_owned()],
            rows,
        });
        result.notes.push((
            "Selection".to_owned(),
            "the order with the smallest AICc among p, d and q from 0 to 2".to_owned(),
        ));
    }
    result.notes.push((
        "Estimation".to_owned(),
        "conditional sum of squares with reflection parameters (stationary and invertible); the \
         first max(p,q) errors are zero"
            .to_owned(),
    ));
    result.notes.push((
        "Forecast".to_owned(),
        "the differenced forecasts are integrated back with the last observed values; the interval \
         uses the ψ weights of the integrated model"
            .to_owned(),
    ));
    result.notes.push((
        "Data policy".to_owned(),
        "non-finite values are excluded and counted; the series is read in worksheet order"
            .to_owned(),
    ));
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    result.warnings.push(
        "the conditional likelihood and the intervals ignore parameter uncertainty; they have not \
         been compared with documented output"
            .to_owned(),
    );
    result
}

fn plot(column: &str, df: &DataFrame, model: &ArimaFit) -> PlotSpec {
    let sample = df
        .column(column)
        .ok()
        .and_then(|c| c.sample(MissingPolicy::Drop).ok());
    let observed: Vec<f64> = sample
        .as_ref()
        .map(|sample| sample.values().to_vec())
        .unwrap_or_default();
    let n = observed.len();
    let points: Vec<Point> = observed
        .iter()
        .enumerate()
        .step_by(plot_stride(n))
        .map(|(index, value)| Point {
            x: index as f64 + 1.0,
            y: *value,
        })
        .collect();
    let finite_forecasts: Vec<Point> = model
        .forecast
        .iter()
        .enumerate()
        .map(|(index, value)| Point {
            x: (n + index + 1) as f64,
            y: *value,
        })
        .collect();
    let mut lines: Vec<Segment> = points
        .windows(2)
        .map(|pair| Segment {
            from: pair[0],
            to: pair[1],
            width: 1.6,
        })
        .collect();
    lines.extend(finite_forecasts.windows(2).map(|pair| Segment {
        from: pair[0],
        to: pair[1],
        width: 2.0,
    }));
    let band = Band {
        lower: model
            .lower
            .iter()
            .enumerate()
            .map(|(index, value)| Point {
                x: (n + index + 1) as f64,
                y: *value,
            })
            .collect(),
        upper: model
            .upper
            .iter()
            .enumerate()
            .map(|(index, value)| Point {
                x: (n + index + 1) as f64,
                y: *value,
            })
            .collect(),
    };
    let low = observed
        .iter()
        .chain(&model.lower)
        .copied()
        .fold(f64::INFINITY, f64::min);
    let high = observed
        .iter()
        .chain(&model.upper)
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let pad = ((high - low).abs() * 0.05).max(1e-9);
    PlotSpec {
        title: format!("ARIMA({},{},{}) — {column}", model.p, model.d, model.q),
        x_axis: linear_axis("Observation", 0.5, (n + model.forecast.len()) as f64 + 0.5),
        y_axis: linear_axis(column, low - pad, high + pad),
        size: (900, 520),
        points,
        lines,
        bands: vec![band],
        series: vec![Series {
            label: "Forecast".to_owned(),
            points: finite_forecasts,
            lines: Vec::new(),
            bands: Vec::new(),
            secondary_axis: false,
        }],
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

    /// A deterministic AR(1) series with innovations from a fixed cycle.
    fn ar_frame() -> DataFrame {
        let mut values = vec![0.0];
        let mut state = 12345u64;
        for index in 1..120 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let noise = ((state >> 33) as f64 / (1u64 << 31) as f64 - 1.0) * 0.8;
            values.push(0.6 * values[index - 1] + noise);
        }
        DataFrame::new(vec![Column::new_f64("y", values)]).expect("frame")
    }

    fn options(p: usize, d: usize, q: usize) -> ArimaOptions {
        ArimaOptions {
            p,
            d,
            q,
            horizon: 6,
        }
    }

    #[test]
    fn the_ar_fit_recovers_the_autoregressive_coefficient() {
        let model = fit(&ar_frame(), "y", options(1, 0, 0), 0.95).expect("fit");
        assert!((model.ar[0] - 0.6).abs() < 0.15, "{}", model.ar[0]);
        assert_eq!(model.forecast.len(), 6);
        assert!(model.aicc.is_finite());
        assert!(model.lower.iter().zip(&model.upper).all(|(lo, hi)| lo < hi));
        assert!(model.sigma2 > 0.0);
    }

    #[test]
    fn a_random_walk_uses_the_differencing() {
        let frame = ar_frame();
        let model = fit(&frame, "y", options(0, 1, 0), 0.95).expect("fit");
        assert_eq!(model.d, 1);
        // The forecast of a random walk is the last value (the mean of the
        // differences, here close to zero).
        let values = frame.column("y").expect("column").numeric_by_row();
        let last = values.last().copied().expect("last");
        assert!(
            (model.forecast[0] - last).abs() < 2.0,
            "{}",
            model.forecast[0]
        );
        assert!(model.lower[0] < model.forecast[0]);
        assert!(model.upper[0] > model.forecast[0]);
    }

    #[test]
    fn the_ma_fit_and_the_best_order_are_built() {
        let frame = ar_frame();
        let model = fit(&frame, "y", options(0, 0, 1), 0.95).expect("fit");
        assert!(model.ma[0].is_finite() && model.ma[0].abs() < 1.0);
        let out = best(&frame, "y", 4, 0.95).expect("best");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Order (p,d,q)"), "{text}");
        assert!(!out.results[0].title.is_empty());
        assert!(out.plot.is_some());
    }

    #[test]
    fn the_report_contains_the_forecast_table() {
        let out = analyze(&ar_frame(), "y", options(1, 0, 0), 0.95).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("AR1"), "{text}");
        assert!(text.contains("Forecast"), "{text}");
        assert!(text.contains("AICc"), "{text}");
    }

    #[test]
    fn invalid_arima_requests_are_rejected() {
        let frame = ar_frame();
        assert!(fit(&frame, "y", options(3, 0, 0), 0.95).is_err());
        assert!(fit(&frame, "y", options(0, 3, 0), 0.95).is_err());
        assert!(fit(&frame, "y", options(1, 0, 0), 0.2).is_err());
        let short = DataFrame::new(vec![Column::new_f64("y", vec![1.0, 2.0, 3.0])]).expect("frame");
        assert!(fit(&short, "y", options(1, 0, 0), 0.95).is_err());
        let constant = DataFrame::new(vec![Column::new_f64("y", vec![2.0; 20])]).expect("frame");
        assert!(fit(&constant, "y", options(1, 0, 0), 0.95).is_err());
    }
}
