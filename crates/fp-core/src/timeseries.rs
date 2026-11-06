//! Time series: moving averages, classical decomposition and exponential
//! smoothing.
//!
//! The series is the column in row order. Three methods are offered:
//!
//! - **Moving average**: a centred average of `w` observations (a `2×w`
//!   average for an even `w`), with the residuals of the observations.
//! - **Classical decomposition**: the trend is a centred moving average of one
//!   seasonal period, the seasonal indices are the average of the detrended
//!   values at each position of the period (centred to sum zero, or divided by
//!   their mean in the multiplicative model), and the residual is what is
//!   left. The report gives the seasonal indices, the trend, the seasonally
//!   adjusted series and the strength of the seasonality.
//! - **Exponential smoothing**: simple exponential smoothing for a series
//!   without seasonality, or Holt–Winters additive smoothing with a seasonal
//!   period. The smoothing constants are chosen by a grid search that
//!   minimises the sum of the squared one-step errors, and the report
//!   forecasts the requested number of periods.
//!
//! The documented behaviour has a time-series family; its conventions (centring
//! rule, seasonal normalisation, optimiser) were not documented, so the numbers
//! are not claimed as documented behaviour — see `the project notes`.

use crate::data::{DataFrame, MissingPolicy};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, Series, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// Additive or multiplicative classical decomposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecompositionModel {
    /// `y = trend + seasonal + residual`.
    Additive,
    /// `y = trend × seasonal × residual`.
    Multiplicative,
}

impl DecompositionModel {
    /// Parses `additive`/`multiplicative` (and `add`/`mult`).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "additive" | "add" => Some(Self::Additive),
            "multiplicative" | "mult" | "multiply" => Some(Self::Multiplicative),
            _ => None,
        }
    }

    /// Name used in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Additive => "additive",
            Self::Multiplicative => "multiplicative",
        }
    }
}

/// Method of the time-series analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeSeriesMethod {
    /// Centred moving average of the given window.
    MovingAverage {
        /// Window length, at least 2.
        window: usize,
    },
    /// Classical decomposition with the given seasonal period.
    Decomposition {
        /// Seasonal period, at least 2.
        period: usize,
        /// Additive or multiplicative model.
        model: DecompositionModel,
    },
    /// Exponential smoothing: level only when `season` is 1, Holt–Winters
    /// additive otherwise.
    ExponentialSmoothing {
        /// Seasonal period (1 for simple exponential smoothing).
        season: usize,
        /// Periods to forecast.
        horizon: usize,
    },
}

/// Centred moving average of a series.
///
/// An odd window averages the `w` observations centred on the position; an
/// even window uses the `2×w` rule (weights `1, 2, …, 2, 1` divided by `2w`),
/// so that every average is centred on an observation. Positions without a
/// full window get no value.
#[must_use]
pub fn moving_average(values: &[f64], window: usize) -> Vec<Option<f64>> {
    let n = values.len();
    let mut result = vec![None; n];
    if window < 2 || window > n {
        return result;
    }
    if window % 2 == 1 {
        let half = window / 2;
        for index in half..(n - half) {
            let sum: f64 = values[index - half..=index + half].iter().sum();
            result[index] = Some(sum / window as f64);
        }
    } else {
        let half = window / 2;
        let denominator = 2.0 * window as f64;
        for index in half..(n - half) {
            // Weights 1, 2, …, 2, 1 over the 2·w span centred on the position.
            let mut sum = values[index - half] + values[index + half];
            for value in &values[(index - half + 1)..(index + half)] {
                sum += 2.0 * value;
            }
            result[index] = Some(sum / denominator);
        }
    }
    result
}

/// Seasonal indices of a decomposition, one per position of the period.
fn seasonal_indices(
    values: &[f64],
    trend: &[Option<f64>],
    period: usize,
    model: DecompositionModel,
) -> Vec<f64> {
    let mut sums = vec![0.0; period];
    let mut counts = vec![0usize; period];
    for (index, value) in values.iter().enumerate() {
        if let Some(trend) = trend[index] {
            let detrended = match model {
                DecompositionModel::Additive => value - trend,
                DecompositionModel::Multiplicative => {
                    if trend.abs() < 1e-12 {
                        continue;
                    }
                    value / trend
                }
            };
            sums[index % period] += detrended;
            counts[index % period] += 1;
        }
    }
    let mut indices: Vec<f64> = sums
        .iter()
        .zip(&counts)
        .map(|(sum, count)| if *count > 0 { sum / *count as f64 } else { 0.0 })
        .collect();
    match model {
        DecompositionModel::Additive => {
            let mean = indices.iter().sum::<f64>() / period as f64;
            for index in &mut indices {
                *index -= mean;
            }
        }
        DecompositionModel::Multiplicative => {
            let mean = indices.iter().sum::<f64>() / period as f64;
            if mean.abs() > 1e-12 {
                for index in &mut indices {
                    *index /= mean;
                }
            }
        }
    }
    indices
}

/// Accuracy measures of a fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Accuracy {
    /// Mean absolute deviation.
    pub mad: f64,
    /// Mean squared deviation.
    pub msd: f64,
    /// Mean absolute percentage error, `NaN` when a value is zero.
    pub mape: f64,
    /// Sum of the squared errors.
    pub sse: f64,
}

fn accuracy(observed: &[f64], fitted: &[Option<f64>]) -> Accuracy {
    let mut count = 0usize;
    let mut absolute = 0.0;
    let mut squared = 0.0;
    let mut percentage = 0.0;
    let mut percentage_ok = true;
    for (value, estimate) in observed.iter().zip(fitted) {
        let Some(estimate) = estimate else {
            continue;
        };
        let error = value - estimate;
        absolute += error.abs();
        squared += error * error;
        if value.abs() < 1e-12 {
            percentage_ok = false;
        } else {
            percentage += (error / value).abs();
        }
        count += 1;
    }
    if count == 0 {
        return Accuracy {
            mad: f64::NAN,
            msd: f64::NAN,
            mape: f64::NAN,
            sse: f64::NAN,
        };
    }
    Accuracy {
        mad: absolute / count as f64,
        msd: squared / count as f64,
        mape: if percentage_ok {
            100.0 * percentage / count as f64
        } else {
            f64::NAN
        },
        sse: squared,
    }
}

/// Result of an exponential smoothing fit.
#[derive(Debug, Clone, PartialEq)]
pub struct Smoothing {
    /// Fitted values, `None` where the recursion has no estimate.
    pub fitted: Vec<Option<f64>>,
    /// Forecasts for the requested horizon.
    pub forecasts: Vec<f64>,
    /// Level smoothing constant.
    pub alpha: f64,
    /// Trend smoothing constant (zero for simple smoothing).
    pub beta: f64,
    /// Seasonal smoothing constant (zero without a seasonal period).
    pub gamma: f64,
    /// Seasonal period (1 for simple smoothing).
    pub season: usize,
    /// Accuracy measures.
    pub accuracy: Accuracy,
}

/// Simple exponential smoothing.
fn simple_smoothing(values: &[f64], alpha: f64, horizon: usize) -> Smoothing {
    let mut level = values[0];
    let mut fitted = vec![None; values.len()];
    for (index, value) in values.iter().enumerate().skip(1) {
        fitted[index] = Some(level);
        level = alpha * value + (1.0 - alpha) * level;
    }
    let accuracy = accuracy(values, &fitted);
    Smoothing {
        fitted,
        forecasts: vec![level; horizon],
        alpha,
        beta: 0.0,
        gamma: 0.0,
        season: 1,
        accuracy,
    }
}

/// Holt–Winters additive smoothing.
fn winters(
    values: &[f64],
    season: usize,
    alpha: f64,
    beta: f64,
    gamma: f64,
    horizon: usize,
) -> Smoothing {
    let n = values.len();
    let mut fitted = vec![None; n];
    if n < 2 * season {
        return Smoothing {
            forecasts: Vec::new(),
            alpha,
            beta,
            gamma,
            season,
            accuracy: accuracy(values, &fitted),
            fitted,
        };
    }
    // Initial level and trend from the first two seasons; the initial seasonal
    // terms are the deviations of the first season from its mean.
    let first_season = &values[..season];
    let second_season = &values[season..2 * season];
    let mean_first = first_season.iter().sum::<f64>() / season as f64;
    let mean_second = second_season.iter().sum::<f64>() / season as f64;
    let mut level = mean_first;
    let mut trend = (mean_second - mean_first) / season as f64;
    let mut seasonal: Vec<f64> = first_season
        .iter()
        .map(|value| value - mean_first)
        .collect();

    for index in 0..n {
        let position = index % season;
        let estimate = level + trend + seasonal[position];
        if index >= season {
            fitted[index] = Some(estimate);
        }
        let observed = values[index];
        let previous_level = level;
        level = alpha * (observed - seasonal[position]) + (1.0 - alpha) * (level + trend);
        trend = beta * (level - previous_level) + (1.0 - beta) * trend;
        seasonal[position] = gamma * (observed - level) + (1.0 - gamma) * seasonal[position];
    }
    let measures = accuracy(values, &fitted);
    let mut forecasts = Vec::with_capacity(horizon);
    for step in 1..=horizon {
        let position = (n + step - 1) % season;
        forecasts.push(level + step as f64 * trend + seasonal[position]);
    }
    Smoothing {
        fitted,
        forecasts,
        alpha,
        beta,
        gamma,
        season,
        accuracy: measures,
    }
}

/// Chooses the smoothing constants by a grid search on the sum of the squared
/// one-step errors, refined by halving the step around the best point.
fn optimise_smoothing(values: &[f64], season: usize, horizon: usize) -> Smoothing {
    if season < 2 {
        let mut best = simple_smoothing(values, 0.5, horizon);
        let mut start = 0.05;
        let mut end = 0.95;
        for _ in 0..3 {
            let step = (end - start) / 10.0;
            let mut candidate = start;
            while candidate <= end + 1e-9 {
                let fit = simple_smoothing(values, candidate, horizon);
                if fit.accuracy.sse < best.accuracy.sse {
                    best = fit;
                }
                candidate += step;
            }
            start = (best.alpha - step).max(0.01);
            end = (best.alpha + step).min(0.99);
        }
        return best;
    }
    let mut best = winters(values, season, 0.3, 0.1, 0.3, horizon);
    let mut start = 0.05;
    let mut end = 0.95;
    for _ in 0..3 {
        let step = (end - start) / 6.0;
        let mut alpha = start;
        while alpha <= end + 1e-9 {
            let mut beta = start;
            while beta <= end + 1e-9 {
                let mut gamma = start;
                while gamma <= end + 1e-9 {
                    let fit = winters(values, season, alpha, beta, gamma, horizon);
                    if fit.accuracy.sse < best.accuracy.sse {
                        best = fit;
                    }
                    gamma += step;
                }
                beta += step;
            }
            alpha += step;
        }
        start = (best.alpha.min(best.beta).min(best.gamma) - step).max(0.01);
        end = (best.alpha.max(best.beta).max(best.gamma) + step).min(0.99);
    }
    best
}

/// Observed, trend and seasonally adjusted series of a decomposition.
#[derive(Debug, Clone, PartialEq)]
pub struct Decomposition {
    /// Seasonal indices, one per position of the period.
    pub seasonal: Vec<f64>,
    /// Trend (centred moving average).
    pub trend: Vec<Option<f64>>,
    /// Seasonally adjusted series.
    pub adjusted: Vec<Option<f64>>,
    /// Residuals.
    pub residual: Vec<Option<f64>>,
    /// Seasonal period.
    pub period: usize,
    /// Model.
    pub model: DecompositionModel,
    /// Strength of the seasonality in `[0, 1]`.
    pub seasonal_strength: f64,
}

/// Classical decomposition of a series.
pub fn decompose(
    values: &[f64],
    period: usize,
    model: DecompositionModel,
) -> Result<Decomposition> {
    if period < 2 {
        return Err(Error::InvalidParameter {
            name: "seasonal period",
            value: period as f64,
        });
    }
    if values.len() < 2 * period {
        return Err(Error::SampleTooSmall {
            n: values.len(),
            min: 2 * period,
        });
    }
    let trend = moving_average(values, period);
    let seasonal = seasonal_indices(values, &trend, period, model);
    let mut adjusted = vec![None; values.len()];
    let mut residual = vec![None; values.len()];
    for index in 0..values.len() {
        let position = index % period;
        let index_value = seasonal[position];
        adjusted[index] = Some(match model {
            DecompositionModel::Additive => values[index] - index_value,
            DecompositionModel::Multiplicative => {
                if index_value.abs() < 1e-12 {
                    f64::NAN
                } else {
                    values[index] / index_value
                }
            }
        });
        if let Some(trend) = trend[index] {
            residual[index] = Some(match model {
                DecompositionModel::Additive => values[index] - trend - index_value,
                DecompositionModel::Multiplicative => {
                    let denominator = trend * index_value;
                    if denominator.abs() < 1e-12 {
                        f64::NAN
                    } else {
                        values[index] / denominator
                    }
                }
            });
        }
    }
    // Seasonal strength: 1 − Var(residual)/Var(detrended).
    let pairs: Vec<(f64, f64)> = residual
        .iter()
        .zip(&trend)
        .filter_map(|(residual, trend)| match (residual, trend) {
            (Some(residual), Some(trend)) => Some((*residual, *trend)),
            _ => None,
        })
        .collect();
    let seasonal_strength = if pairs.len() > 2 {
        let mean_residual =
            pairs.iter().map(|(residual, _)| residual).sum::<f64>() / pairs.len() as f64;
        let mean_trend = pairs.iter().map(|(_, trend)| trend).sum::<f64>() / pairs.len() as f64;
        let variance_residual = pairs
            .iter()
            .map(|(residual, _)| (residual - mean_residual).powi(2))
            .sum::<f64>()
            / (pairs.len() - 1) as f64;
        let variance_trend = pairs
            .iter()
            .map(|(_, trend)| (trend - mean_trend).powi(2))
            .sum::<f64>()
            / (pairs.len() - 1) as f64;
        if variance_trend > 0.0 {
            (1.0 - variance_residual / variance_trend).clamp(0.0, 1.0)
        } else {
            0.0
        }
    } else {
        f64::NAN
    };
    Ok(Decomposition {
        seasonal,
        trend,
        adjusted,
        residual,
        period,
        model,
        seasonal_strength,
    })
}

/// Builds a line plot with an optional overlay.
fn line_plot(
    title: &str,
    y_label: &str,
    x_len: usize,
    main: &[f64],
    overlays: Vec<(&str, Vec<Option<f64>>)>,
) -> PlotSpec {
    let mut low = main.iter().copied().fold(f64::INFINITY, f64::min);
    let mut high = main.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    for (_, series) in &overlays {
        for value in series.iter().flatten() {
            low = low.min(*value);
            high = high.max(*value);
        }
    }
    let pad = ((high - low).abs() * 0.05).max(1e-9);
    let mut series = Vec::with_capacity(overlays.len() + 1);
    for (label, values) in overlays {
        let points: Vec<Point> = values
            .iter()
            .enumerate()
            .filter_map(|(index, value)| {
                value.map(|y| Point {
                    x: index as f64 + 1.0,
                    y,
                })
            })
            .collect();
        let mut lines = Vec::new();
        for pair in points.windows(2) {
            lines.push(Segment {
                from: pair[0],
                to: pair[1],
                width: 2.0,
            });
        }
        series.push(Series {
            label: label.to_owned(),
            points,
            lines,
            bands: Vec::new(),
            secondary_axis: false,
        });
    }
    PlotSpec {
        title: title.to_owned(),
        x_axis: linear_axis("Observation", 1.0, x_len as f64),
        y_axis: linear_axis(y_label, low - pad, high + pad),
        size: (900, 560),
        points: main
            .iter()
            .enumerate()
            .map(|(index, value)| Point {
                x: index as f64 + 1.0,
                y: *value,
            })
            .collect(),
        lines: Vec::new(),
        bands: Vec::new(),
        series,
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Runs the time-series analysis and builds the report (and plot).
pub fn analyze(
    df: &DataFrame,
    column_name: &str,
    method: TimeSeriesMethod,
) -> Result<AnalysisOutput> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let values = sample.values().to_vec();
    if values.len() < 3 {
        return Err(Error::SampleTooSmall {
            n: values.len(),
            min: 3,
        });
    }
    let mut result =
        AnalysisResult::new(format!("Time Series — column {column_name}"), String::new());
    result.n = sample.n();
    result.excluded = sample.excluded();
    let plot = match method {
        TimeSeriesMethod::MovingAverage { window } => {
            if window < 2 {
                return Err(Error::InvalidParameter {
                    name: "moving-average window",
                    value: window as f64,
                });
            }
            if window > values.len() {
                return Err(Error::Unsupported(
                    "the moving-average window is longer than the series".to_owned(),
                ));
            }
            let average = moving_average(&values, window);
            let accuracy = accuracy(&values, &average);
            result.method = format!(
                "Centred moving average of {window} observations{}",
                if window % 2 == 0 { " (2×w rule)" } else { "" }
            );
            result.statistics = vec![
                ("Window", window as f64),
                ("MAD", accuracy.mad),
                ("MSD", accuracy.msd),
                ("MAPE", accuracy.mape),
            ];
            result.extra.push(Item::Table {
                headers: vec![
                    "Observation".to_owned(),
                    "Value".to_owned(),
                    "Moving average".to_owned(),
                    "Residual".to_owned(),
                ],
                rows: values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        vec![
                            format!("{}", index + 1),
                            format!("{value:.6}"),
                            average[index].map_or("—".to_owned(), |ma| format!("{ma:.6}")),
                            average[index]
                                .map_or("—".to_owned(), |ma| format!("{:.6}", value - ma)),
                        ]
                    })
                    .collect(),
            });
            result.notes = vec![
                (
                    "Method".to_owned(),
                    "centred moving average; an even window uses the 2×w weights".to_owned(),
                ),
                (
                    "Accuracy".to_owned(),
                    "MAD and MSD are computed on the observations that have an average".to_owned(),
                ),
                (
                    "Data policy".to_owned(),
                    "non-finite values excluded and counted".to_owned(),
                ),
            ];
            line_plot(
                "Moving Average",
                column_name,
                values.len(),
                &values,
                vec![("Moving average", average)],
            )
        }
        TimeSeriesMethod::Decomposition { period, model } => {
            let model_fit = decompose(&values, period, model)?;
            result.method = format!(
                "Classical {} decomposition with a seasonal period of {period}",
                model.name()
            );
            result.statistics = vec![
                ("Seasonal period", period as f64),
                ("Seasonal strength", model_fit.seasonal_strength),
                ("Seasonal indices", model_fit.seasonal.iter().sum::<f64>()),
            ];
            result.extra.push(Item::Table {
                headers: vec!["Position".to_owned(), "Seasonal index".to_owned()],
                rows: model_fit
                    .seasonal
                    .iter()
                    .enumerate()
                    .map(|(index, value)| vec![format!("{}", index + 1), format!("{value:.6}")])
                    .collect(),
            });
            result.extra.push(Item::Table {
                headers: vec![
                    "Observation".to_owned(),
                    "Value".to_owned(),
                    "Trend".to_owned(),
                    "Seasonal".to_owned(),
                    "Adjusted".to_owned(),
                    "Residual".to_owned(),
                ],
                rows: values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        let trend = model_fit.trend[index];
                        let seasonal = model_fit.seasonal[index % period];
                        vec![
                            format!("{}", index + 1),
                            format!("{value:.6}"),
                            trend.map_or("—".to_owned(), |trend| format!("{trend:.6}")),
                            format!("{seasonal:.6}"),
                            model_fit.adjusted[index]
                                .map_or("—".to_owned(), |value| format!("{value:.6}")),
                            model_fit.residual[index]
                                .map_or("—".to_owned(), |value| format!("{value:.6}")),
                        ]
                    })
                    .collect(),
            });
            result.notes = vec![
                (
                    "Method".to_owned(),
                    format!(
                        "trend from a centred moving average of one period; {} seasonal indices",
                        model.name()
                    ),
                ),
                (
                    "Seasonal strength".to_owned(),
                    "1 − Var(residual)/Var(detrended), clamped to [0, 1]".to_owned(),
                ),
                (
                    "Normalisation".to_owned(),
                    match model {
                        DecompositionModel::Additive => "the indices are shifted to sum zero",
                        DecompositionModel::Multiplicative => {
                            "the indices are divided by their mean, so the mean is one"
                        }
                    }
                    .to_owned(),
                ),
                (
                    "Data policy".to_owned(),
                    "non-finite values excluded and counted".to_owned(),
                ),
            ];
            line_plot(
                "Classical Decomposition",
                column_name,
                values.len(),
                &values,
                vec![
                    ("Trend", model_fit.trend.clone()),
                    ("Seasonally adjusted", model_fit.adjusted.clone()),
                ],
            )
        }
        TimeSeriesMethod::ExponentialSmoothing { season, horizon } => {
            if horizon == 0 {
                return Err(Error::InvalidParameter {
                    name: "forecast horizon",
                    value: 0.0,
                });
            }
            if season > 0 && season < 2 && values.len() < 4 {
                return Err(Error::SampleTooSmall {
                    n: values.len(),
                    min: 4,
                });
            }
            let season = season.max(1);
            let fit = optimise_smoothing(&values, season, horizon);
            let name = if season < 2 {
                "Simple exponential smoothing"
            } else {
                "Holt-Winters additive smoothing"
            };
            result.method = format!("{name} with constants chosen by least squares");
            result.statistics = vec![
                ("Seasonal period", season as f64),
                ("Alpha", fit.alpha),
                ("Beta", fit.beta),
                ("Gamma", fit.gamma),
                ("MAD", fit.accuracy.mad),
                ("MSD", fit.accuracy.msd),
                ("MAPE", fit.accuracy.mape),
                ("SSE", fit.accuracy.sse),
            ];
            result.extra.push(Item::Table {
                headers: vec![
                    "Observation".to_owned(),
                    "Value".to_owned(),
                    "Fitted".to_owned(),
                    "Error".to_owned(),
                ],
                rows: values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        vec![
                            format!("{}", index + 1),
                            format!("{value:.6}"),
                            fit.fitted[index]
                                .map_or("—".to_owned(), |fitted| format!("{fitted:.6}")),
                            fit.fitted[index].map_or("—".to_owned(), |fitted| {
                                format!("{:.6}", value - fitted)
                            }),
                        ]
                    })
                    .collect(),
            });
            result.extra.push(Item::Table {
                headers: vec!["Period".to_owned(), "Forecast".to_owned()],
                rows: fit
                    .forecasts
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        vec![
                            format!("{}", values.len() + index + 1),
                            format!("{value:.6}"),
                        ]
                    })
                    .collect(),
            });
            result.notes = vec![
                (
                    "Method".to_owned(),
                    if season < 2 {
                        "level recursion Sₜ = α·yₜ + (1 − α)·Sₜ₋₁ with a flat forecast".to_owned()
                    } else {
                        "Holt-Winters additive: level, trend and seasonal recursions".to_owned()
                    },
                ),
                (
                    "Constants".to_owned(),
                    "grid search on the sum of the squared one-step errors, refined by halving \
                     the step"
                        .to_owned(),
                ),
                (
                    "Accuracy".to_owned(),
                    "computed on the one-step-ahead fitted values".to_owned(),
                ),
                (
                    "Data policy".to_owned(),
                    "non-finite values excluded and counted".to_owned(),
                ),
            ];
            line_plot(
                "Exponential Smoothing",
                column_name,
                values.len(),
                &values,
                vec![("Fitted", fit.fitted.clone())],
            )
        }
    };
    result.warnings = standard_warnings(result.n, result.excluded, sample.ties());
    result.warnings.push(
        "the time-series conventions (centring, seasonal normalisation, smoothing grid) are \
         standard choices and have not been verified against documented output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}
