//! Kaplan–Meier product-limit survival estimate with right censoring.
//!
//! The censoring column is numeric: observations equal to the configured
//! censoring value are treated as right-censored, and all other observations as
//! events. With no censoring column, every observation is an event. The
//! confidence interval for `S(t)` uses the Greenwood standard error and the
//! normal (linear) approximation, the convention of the captured run.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{ln_gamma, normal_quantile, normal_sf};
use crate::workflow::AnalysisOutput;

/// One row of the product-limit table.
#[derive(Debug, Clone, PartialEq)]
pub struct KaplanMeierPoint {
    /// Event/censoring time.
    pub time: f64,
    /// Observations at risk at this time.
    pub at_risk: usize,
    /// Events at this time.
    pub events: usize,
    /// Censored observations at this time.
    pub censored: usize,
    /// Estimated survival probability after this time.
    pub survival: f64,
    /// Greenwood standard error.
    pub se: f64,
    /// Lower confidence bound.
    pub ci_lower: f64,
    /// Upper confidence bound.
    pub ci_upper: f64,
}

/// Fitted Kaplan–Meier curve.
#[derive(Debug, Clone, PartialEq)]
pub struct KaplanMeier {
    /// Product-limit table in ascending time order.
    pub points: Vec<KaplanMeierPoint>,
    /// Observations used.
    pub n: usize,
    /// Events observed.
    pub events: usize,
    /// Right-censored observations.
    pub censored: usize,
    /// Rows removed because time or censoring was missing/invalid.
    pub excluded: usize,
    /// Median survival time, when the curve reaches 0.5.
    pub median: Option<f64>,
    /// Approximate confidence interval for the median.
    pub median_ci: Option<(f64, f64)>,
    /// Confidence level.
    pub confidence: f64,
}

fn cell(column: &Column, row: usize) -> Option<f64> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => values.get(row).copied().filter(|value| value.is_finite()),
        ColumnData::I64(values) => values.get(row).map(|&value| value as f64),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_) => None,
    }
}

fn validate_confidence(confidence: f64) -> Result<()> {
    if !confidence.is_finite() || !(0.5..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence level",
            value: confidence,
        });
    }
    Ok(())
}

/// Fits the Kaplan–Meier curve.
pub fn fit(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
    confidence: f64,
) -> Result<KaplanMeier> {
    validate_confidence(confidence)?;
    if !censor_value.is_finite() {
        return Err(Error::InvalidParameter {
            name: "censoring value",
            value: censor_value,
        });
    }
    let time = df.column(time_column)?;
    if matches!(
        time.data(),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
    ) {
        return Err(Error::Csv(format!(
            "time column {time_column} is not numeric"
        )));
    }
    let censor = match censor_column {
        Some(name) => {
            let column = df.column(name)?;
            if matches!(
                column.data(),
                ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
            ) {
                return Err(Error::Csv(format!(
                    "censoring column {name} is not numeric"
                )));
            }
            Some(column)
        }
        None => None,
    };

    let mut observations: Vec<(f64, bool)> = Vec::new();
    for row in 0..df.n_rows() {
        let Some(value) = cell(time, row) else {
            continue;
        };
        if value <= 0.0 {
            return Err(Error::Cell {
                column: time_column.to_owned(),
                row: row + 1,
                message: "survival times must be positive".to_owned(),
            });
        }
        let censored = match censor {
            Some(column) => match cell(column, row) {
                Some(flag) => (flag - censor_value).abs() <= f64::EPSILON,
                None => continue,
            },
            None => false,
        };
        observations.push((value, censored));
    }
    if observations.is_empty() {
        return Err(Error::EmptySample);
    }
    observations.sort_by(|a, b| a.0.total_cmp(&b.0));

    let n = observations.len();
    let events = observations
        .iter()
        .filter(|(_, censored)| !censored)
        .count();
    let censored = n - events;
    if events == 0 {
        return Err(Error::Unsupported(
            "the sample has no events; a survival curve cannot be estimated".to_owned(),
        ));
    }

    let mut points = Vec::new();
    let mut survival = 1.0;
    let mut greenwood = 0.0;
    let mut index = 0usize;
    while index < observations.len() {
        let time_value = observations[index].0;
        let mut event_count = 0usize;
        let mut censor_count = 0usize;
        while index < observations.len() && observations[index].0 == time_value {
            if observations[index].1 {
                censor_count += 1;
            } else {
                event_count += 1;
            }
            index += 1;
        }
        // At risk includes everything at this time or later.
        let at_risk = n - observations
            .iter()
            .take_while(|(value, _)| *value < time_value)
            .count();
        if event_count > 0 {
            survival *= 1.0 - event_count as f64 / at_risk as f64;
            if at_risk > event_count {
                greenwood += event_count as f64 / (at_risk as f64 * (at_risk - event_count) as f64);
            }
            survival = survival.clamp(0.0, 1.0);
        }
        let se = if survival > 0.0 {
            survival * greenwood.sqrt()
        } else {
            0.0
        };
        let z = normal_quantile(1.0 - (1.0 - confidence) / 2.0);
        let (lower, upper) = (
            (survival - z * se).clamp(0.0, 1.0),
            (survival + z * se).clamp(0.0, 1.0),
        );
        points.push(KaplanMeierPoint {
            time: time_value,
            at_risk,
            events: event_count,
            censored: censor_count,
            survival,
            se,
            ci_lower: lower,
            ci_upper: upper,
        });
    }

    let median = points
        .iter()
        .find(|point| point.survival <= 0.5)
        .map(|point| point.time);
    // Brookmeyer–Crowley interval: the earliest and latest times at which a
    // confidence limit reaches 0.5, reported low to high.
    let median_ci = median.and_then(|_| {
        let first_upper = points
            .iter()
            .find(|point| point.ci_upper <= 0.5)
            .map(|point| point.time)?;
        let first_lower = points
            .iter()
            .find(|point| point.ci_lower <= 0.5)
            .map(|point| point.time)?;
        Some((first_upper.min(first_lower), first_upper.max(first_lower)))
    });

    Ok(KaplanMeier {
        points,
        n,
        events,
        censored,
        excluded: df.n_rows() - n,
        median,
        median_ci,
        confidence,
    })
}

/// Fits the curve, builds the report and the survival plot.
pub fn analyze(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
    confidence: f64,
) -> Result<AnalysisOutput> {
    let model = fit(df, time_column, censor_column, censor_value, confidence)?;
    let result = report(time_column, &model);
    let plot = plot(time_column, &model);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn report(time_column: &str, model: &KaplanMeier) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("Kaplan–Meier — column {time_column}"),
        format!(
            "Product-limit estimate; Greenwood SE; {:.1}% normal intervals",
            model.confidence * 100.0
        ),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Events", model.events as f64),
        ("Censored", model.censored as f64),
    ];
    // Restricted mean survival: the step integral of S(t) up to the largest
    // observed time, the captured run's Mean (MTTF).
    let mut mean = 0.0;
    let mut previous_time = 0.0;
    let mut previous_survival = 1.0;
    for point in &model.points {
        mean += previous_survival * (point.time - previous_time);
        previous_time = point.time;
        previous_survival = point.survival;
    }
    result.statistics.push(("Mean (MTTF)", mean));
    if let Some(median) = model.median {
        result.statistics.push(("Median survival", median));
        let quartile = |level: f64| {
            model
                .points
                .iter()
                .find(|point| point.survival <= level)
                .map(|point| point.time)
        };
        if let (Some(q1), Some(q3)) = (quartile(0.75), quartile(0.25)) {
            result.statistics.push(("First quartile (Q1)", q1));
            result.statistics.push(("Third quartile (Q3)", q3));
            result.statistics.push(("Interquartile range", q3 - q1));
        }
    }
    if let Some((lower, upper)) = model.median_ci {
        result.statistics.push(("Median CI lower", lower));
        result.statistics.push(("Median CI upper", upper));
    }
    result.extra.push(Item::Table {
        headers: vec![
            "Time".to_owned(),
            "At risk".to_owned(),
            "Events".to_owned(),
            "Censored".to_owned(),
            "Survival".to_owned(),
            "SE".to_owned(),
            format!("{:.1}% CI", model.confidence * 100.0),
        ],
        rows: model
            .points
            .iter()
            .map(|point| {
                vec![
                    format!("{:.6}", point.time),
                    point.at_risk.to_string(),
                    point.events.to_string(),
                    point.censored.to_string(),
                    format!("{:.6}", point.survival),
                    format!("{:.6}", point.se),
                    format!("({:.6}, {:.6})", point.ci_lower, point.ci_upper),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Estimator".to_owned(),
            "S(t) = Π(1 − dᵢ/nᵢ) over event times; Greenwood variance \
             S(t)²·Σ dᵢ/(nᵢ(nᵢ−dᵢ))"
                .to_owned(),
        ),
        (
            "Censoring".to_owned(),
            "right-censored observations are those equal to the configured censoring value; \
             all other observations are events"
                .to_owned(),
        ),
        (
            "Interval".to_owned(),
            "normal (linear) on the survival scale, clipped to [0, 1], as the captured run"
                .to_owned(),
        ),
        (
            "Median".to_owned(),
            "first time at which the estimated survival is at or below 0.5; the interval runs \
             from the first time the upper bound reaches 0.5 to the first time the lower bound \
             does"
                .to_owned(),
        ),
        (
            "Assumption".to_owned(),
            "independent censoring and non-informative censoring".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    result.warnings.push(
        "Kaplan–Meier survival and the normal interval reproduce the captured run \
         (n = 12: S(8) 0.75, CI 0.505–0.995)"
            .to_owned(),
    );
    result
}

fn plot(time_column: &str, model: &KaplanMeier) -> PlotSpec {
    let max_time = model.points.last().map_or(1.0, |point| point.time);
    let x_max = (max_time * 1.05).max(1.0);
    let mut lines = Vec::new();
    let mut previous_time = 0.0;
    let mut previous_survival = 1.0;
    for point in &model.points {
        if point.time > previous_time {
            lines.push(Segment {
                from: Point {
                    x: previous_time,
                    y: previous_survival,
                },
                to: Point {
                    x: point.time,
                    y: previous_survival,
                },
                width: 2.0,
            });
        }
        if (point.survival - previous_survival).abs() > 0.0 {
            lines.push(Segment {
                from: Point {
                    x: point.time,
                    y: previous_survival,
                },
                to: Point {
                    x: point.time,
                    y: point.survival,
                },
                width: 2.0,
            });
        }
        previous_time = point.time;
        previous_survival = point.survival;
    }
    if previous_time < x_max {
        lines.push(Segment {
            from: Point {
                x: previous_time,
                y: previous_survival,
            },
            to: Point {
                x: x_max,
                y: previous_survival,
            },
            width: 2.0,
        });
    }
    let points = model
        .points
        .iter()
        .filter(|point| point.censored > 0)
        .map(|point| Point {
            x: point.time,
            y: point.survival,
        })
        .collect();

    PlotSpec {
        title: format!("Kaplan–Meier — {time_column}"),
        x_axis: linear_axis("Time", 0.0, x_max),
        y_axis: linear_axis("Survival probability", 0.0, 1.0),
        size: (900, 520),
        points,
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Right-censored Weibull maximum-likelihood fit.
#[derive(Debug, Clone, PartialEq)]
pub struct CensoredWeibull {
    /// Shape parameter.
    pub shape: f64,
    /// Scale parameter.
    pub scale: f64,
    /// Maximized log-likelihood.
    pub log_likelihood: f64,
    /// Akaike information criterion (`2k − 2ℓ`, `k = 2`).
    pub aic: f64,
    /// Mean life `β·Γ(1 + 1/α)`.
    pub mean_life: f64,
    /// B10 life (time at 10% failure).
    pub b10: f64,
    /// 95% confidence interval for the shape.
    pub shape_ci: Option<(f64, f64)>,
    /// 95% confidence interval for the scale.
    pub scale_ci: Option<(f64, f64)>,
    /// 95% confidence interval for B10 life.
    pub b10_ci: Option<(f64, f64)>,
    /// Observations used.
    pub n: usize,
    /// Events.
    pub events: usize,
    /// Right-censored observations.
    pub censored: usize,
    /// Rows excluded.
    pub excluded: usize,
}

fn weibull_profile(
    alpha: f64,
    times: &[f64],
    events: &[bool],
    event_logs: f64,
    event_count: usize,
) -> (f64, f64) {
    if !alpha.is_finite() || alpha <= 0.0 {
        return (f64::NEG_INFINITY, f64::NAN);
    }
    let sum_power = times.iter().map(|time| time.powf(alpha)).sum::<f64>();
    if !sum_power.is_finite() || sum_power <= 0.0 {
        return (f64::NEG_INFINITY, f64::NAN);
    }
    let beta = (sum_power / event_count as f64).powf(1.0 / alpha);
    if !beta.is_finite() || beta <= 0.0 {
        return (f64::NEG_INFINITY, f64::NAN);
    }
    let _ = events;
    let log_likelihood = event_count as f64 * alpha.ln() - event_count as f64 * alpha * beta.ln()
        + (alpha - 1.0) * event_logs
        - event_count as f64;
    (log_likelihood, beta)
}

fn weibull_log_likelihood(alpha: f64, beta: f64, times: &[f64], events: &[bool]) -> f64 {
    if !alpha.is_finite() || alpha <= 0.0 || !beta.is_finite() || beta <= 0.0 {
        return f64::NEG_INFINITY;
    }
    let mut total = 0.0;
    for (time, event) in times.iter().zip(events) {
        let z = (time / beta).powf(alpha);
        if *event {
            total += alpha.ln() - alpha * beta.ln() + (alpha - 1.0) * time.ln() - z;
        } else {
            total -= z;
        }
    }
    total
}

fn weibull_log_covariance(
    alpha: f64,
    beta: f64,
    times: &[f64],
    events: &[bool],
) -> Option<((f64, f64), (f64, f64))> {
    let f = |x: f64, y: f64| weibull_log_likelihood(x.exp(), y.exp(), times, events);
    let x0 = alpha.ln();
    let y0 = beta.ln();
    let h = 1e-4;
    let f0 = f(x0, y0);
    let fxx = (f(x0 + h, y0) - 2.0 * f0 + f(x0 - h, y0)) / (h * h);
    let fyy = (f(x0, y0 + h) - 2.0 * f0 + f(x0, y0 - h)) / (h * h);
    let fxy = (f(x0 + h, y0 + h) - f(x0 + h, y0 - h) - f(x0 - h, y0 + h) + f(x0 - h, y0 - h))
        / (4.0 * h * h);
    let a = -fxx;
    let b = -fxy;
    let c = -fyy;
    let det = a * c - b * b;
    if !det.is_finite() || det <= 0.0 || a <= 0.0 || c <= 0.0 {
        return None;
    }
    Some(((c / det, -b / det), (-b / det, a / det)))
}

/// Fits a right-censored Weibull by profiling the scale.
pub fn fit_weibull(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
) -> Result<CensoredWeibull> {
    if !censor_value.is_finite() {
        return Err(Error::InvalidParameter {
            name: "censoring value",
            value: censor_value,
        });
    }
    let time = df.column(time_column)?;
    if matches!(
        time.data(),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
    ) {
        return Err(Error::Csv(format!(
            "time column {time_column} is not numeric"
        )));
    }
    let censor = match censor_column {
        Some(name) => {
            let column = df.column(name)?;
            if matches!(
                column.data(),
                ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
            ) {
                return Err(Error::Csv(format!(
                    "censoring column {name} is not numeric"
                )));
            }
            Some(column)
        }
        None => None,
    };
    let mut times = Vec::new();
    let mut events = Vec::new();
    for row in 0..df.n_rows() {
        let Some(value) = cell(time, row) else {
            continue;
        };
        if value <= 0.0 {
            return Err(Error::Cell {
                column: time_column.to_owned(),
                row: row + 1,
                message: "survival times must be positive".to_owned(),
            });
        }
        let censored = match censor {
            Some(column) => match cell(column, row) {
                Some(flag) => (flag - censor_value).abs() <= f64::EPSILON,
                None => continue,
            },
            None => false,
        };
        times.push(value);
        events.push(!censored);
    }
    if times.is_empty() {
        return Err(Error::EmptySample);
    }
    let event_count = events.iter().filter(|event| **event).count();
    if event_count == 0 {
        return Err(Error::Unsupported(
            "the sample has no events; a Weibull fit is not identified".to_owned(),
        ));
    }
    if times.len() < 2 {
        return Err(Error::SampleTooSmall {
            n: times.len(),
            min: 2,
        });
    }
    // A constant sample makes the Weibull likelihood unbounded (shape → ∞).
    let min_time = times.iter().copied().fold(f64::INFINITY, f64::min);
    let max_time = times.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if max_time <= min_time {
        return Err(Error::ConstantSample);
    }
    let event_logs = times
        .iter()
        .zip(&events)
        .filter(|(_, event)| **event)
        .map(|(time, _)| time.ln())
        .sum::<f64>();

    let mut best_log = 0.0;
    let mut best_value = f64::NEG_INFINITY;
    for step in 0..=400 {
        let log_alpha = -10.0 + 20.0 * step as f64 / 400.0;
        let (value, _) = weibull_profile(log_alpha.exp(), &times, &events, event_logs, event_count);
        if value > best_value {
            best_value = value;
            best_log = log_alpha;
        }
    }
    let mut low = (best_log - 0.05).max(-10.0);
    let mut high = (best_log + 0.05).min(10.0);
    const PHI: f64 = 0.618_033_988_749_894_9;
    for _ in 0..80 {
        let a = high - PHI * (high - low);
        let b = low + PHI * (high - low);
        let (fa, _) = weibull_profile(a.exp(), &times, &events, event_logs, event_count);
        let (fb, _) = weibull_profile(b.exp(), &times, &events, event_logs, event_count);
        if fa > fb {
            high = b;
        } else {
            low = a;
        }
        if high - low < 1e-10 {
            break;
        }
    }
    let shape = (0.5 * (low + high)).exp();
    let (log_likelihood, scale) = weibull_profile(shape, &times, &events, event_logs, event_count);
    if !log_likelihood.is_finite() || !scale.is_finite() || scale <= 0.0 {
        return Err(Error::NotConverged {
            detail: "censored Weibull profile likelihood".to_owned(),
        });
    }
    let mean_life = scale * ln_gamma(1.0 + 1.0 / shape).exp();
    let b10 = scale * (-0.9f64.ln()).powf(1.0 / shape);
    let covariance = weibull_log_covariance(shape, scale, &times, &events);
    let z = normal_quantile(0.975);
    let (shape_ci, scale_ci, b10_ci) = match covariance {
        Some(((v11, v12), (_, v22))) if v11 >= 0.0 && v22 >= 0.0 => {
            let shape_se = v11.sqrt();
            let scale_se = v22.sqrt();
            let b10_log = b10.ln();
            let gradient = [-(1.0 / shape) * (-0.9f64.ln()).ln(), 1.0];
            let b10_variance = gradient[0] * gradient[0] * v11
                + 2.0 * gradient[0] * gradient[1] * v12
                + gradient[1] * gradient[1] * v22;
            let b10_se = b10_variance.max(0.0).sqrt();
            (
                Some((
                    (shape.ln() - z * shape_se).exp(),
                    (shape.ln() + z * shape_se).exp(),
                )),
                Some((
                    (scale.ln() - z * scale_se).exp(),
                    (scale.ln() + z * scale_se).exp(),
                )),
                Some(((b10_log - z * b10_se).exp(), (b10_log + z * b10_se).exp())),
            )
        }
        _ => (None, None, None),
    };
    Ok(CensoredWeibull {
        shape,
        scale,
        log_likelihood,
        aic: 4.0 - 2.0 * log_likelihood,
        mean_life,
        b10,
        shape_ci,
        scale_ci,
        b10_ci,
        n: times.len(),
        events: event_count,
        censored: times.len() - event_count,
        excluded: df.n_rows() - times.len(),
    })
}

/// Builds the report and fitted survival curve.
pub fn analyze_weibull(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
) -> Result<AnalysisOutput> {
    let model = fit_weibull(df, time_column, censor_column, censor_value)?;
    let max_time = df
        .column(time_column)?
        .sample(crate::data::MissingPolicy::Drop)?
        .max()?;
    let x_max = (max_time * 1.05).max(1.0);
    let mut lines = Vec::new();
    let mut previous = (0.0, 1.0);
    for step in 1..=200 {
        let time = x_max * step as f64 / 200.0;
        let survival = (-(time / model.scale).powf(model.shape)).exp();
        lines.push(Segment {
            from: Point {
                x: previous.0,
                y: previous.1,
            },
            to: Point {
                x: time,
                y: survival,
            },
            width: 2.0,
        });
        previous = (time, survival);
    }
    let mut result = AnalysisResult::new(
        format!("Weibull Reliability — column {time_column}"),
        "Right-censored Weibull MLE; scale profiled from the shape",
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Shape (α)", model.shape),
        ("Scale (β)", model.scale),
        ("Log-likelihood", model.log_likelihood),
        ("AIC", model.aic),
        ("Mean life", model.mean_life),
        ("B10 life", model.b10),
        ("Events", model.events as f64),
        ("Censored", model.censored as f64),
    ];
    result.extra.push(Item::Table {
        headers: vec!["Quantity".to_owned(), "Value".to_owned()],
        rows: vec![
            vec!["Shape (α)".to_owned(), format!("{:.6}", model.shape)],
            vec!["Scale (β)".to_owned(), format!("{:.6}", model.scale)],
            vec![
                "Log-likelihood".to_owned(),
                format!("{:.6}", model.log_likelihood),
            ],
            vec!["AIC".to_owned(), format!("{:.6}", model.aic)],
            vec!["Mean life".to_owned(), format!("{:.6}", model.mean_life)],
            vec!["B10 life".to_owned(), format!("{:.6}", model.b10)],
        ],
    });
    result.notes = vec![
        (
            "Estimator".to_owned(),
            "right-censored Weibull likelihood; for fixed α, β = (Σtᵢ^α/n_events)^(1/α);              the shape is maximized by grid + golden-section search"
                .to_owned(),
        ),
        (
            "Censoring".to_owned(),
            "observations equal to the configured censoring value contribute ln S(t); others              contribute ln f(t)"
                .to_owned(),
        ),
        (
            "Intervals".to_owned(),
            "95% Wald intervals from the observed Hessian in log-parameter space; B10 uses the              delta method"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    result.warnings.push(
        "censored Weibull conventions and interval estimates have not been verified against captured output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(PlotSpec {
            title: format!("Weibull Reliability — {time_column}"),
            x_axis: linear_axis("Time", 0.0, x_max),
            y_axis: linear_axis("Survival probability", 0.0, 1.0),
            size: (900, 520),
            points: Vec::new(),
            lines,
            bands: Vec::new(),
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: Vec::new(),
        }),
    })
}

/// Right-censored lognormal maximum-likelihood fit.
#[derive(Debug, Clone, PartialEq)]
pub struct CensoredLognormal {
    /// Log-scale location.
    pub mu: f64,
    /// Log-scale standard deviation.
    pub sigma: f64,
    /// Median life `exp(mu)`.
    pub median: f64,
    /// Mean life `exp(mu + sigma²/2)`.
    pub mean_life: f64,
    /// B10 life.
    pub b10: f64,
    /// 95% confidence interval for µ.
    pub mu_ci: Option<(f64, f64)>,
    /// 95% confidence interval for σ.
    pub sigma_ci: Option<(f64, f64)>,
    /// 95% confidence interval for B10.
    pub b10_ci: Option<(f64, f64)>,
    /// Maximized log-likelihood.
    pub log_likelihood: f64,
    /// Akaike information criterion.
    pub aic: f64,
    /// Observations used.
    pub n: usize,
    /// Events.
    pub events: usize,
    /// Right-censored observations.
    pub censored: usize,
    /// Rows excluded.
    pub excluded: usize,
}

fn lognormal_loglik(mu: f64, sigma: f64, logs: &[f64], events: &[bool]) -> f64 {
    if !mu.is_finite() || !sigma.is_finite() || sigma <= 0.0 {
        return f64::NEG_INFINITY;
    }
    let mut total = 0.0;
    for (log_time, event) in logs.iter().zip(events) {
        let z = (log_time - mu) / sigma;
        if *event {
            total += -log_time - sigma.ln() - 0.5 * z * z - 0.5 * (2.0 * std::f64::consts::PI).ln();
        } else {
            total += normal_sf(z).ln();
        }
    }
    total
}

fn golden_max<F: Fn(f64) -> f64>(f: F, mut low: f64, mut high: f64) -> f64 {
    const PHI: f64 = 0.618_033_988_749_894_9;
    for _ in 0..90 {
        let a = high - PHI * (high - low);
        let b = low + PHI * (high - low);
        if f(a) > f(b) {
            high = b;
        } else {
            low = a;
        }
        if high - low < 1e-10 {
            break;
        }
    }
    0.5 * (low + high)
}

fn lognormal_log_covariance(
    mu: f64,
    sigma: f64,
    logs: &[f64],
    events: &[bool],
) -> Option<((f64, f64), (f64, f64))> {
    let f = |x: f64, y: f64| lognormal_loglik(x, y.exp(), logs, events);
    let y0 = sigma.ln();
    let h = 1e-4;
    let f0 = f(mu, y0);
    let fxx = (f(mu + h, y0) - 2.0 * f0 + f(mu - h, y0)) / (h * h);
    let fyy = (f(mu, y0 + h) - 2.0 * f0 + f(mu, y0 - h)) / (h * h);
    let fxy = (f(mu + h, y0 + h) - f(mu + h, y0 - h) - f(mu - h, y0 + h) + f(mu - h, y0 - h))
        / (4.0 * h * h);
    let a = -fxx;
    let b = -fxy;
    let c = -fyy;
    let det = a * c - b * b;
    if !det.is_finite() || det <= 0.0 || a <= 0.0 || c <= 0.0 {
        return None;
    }
    Some(((c / det, -b / det), (-b / det, a / det)))
}

/// Fits a right-censored lognormal by coordinate ascent.
pub fn fit_lognormal(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
) -> Result<CensoredLognormal> {
    if !censor_value.is_finite() {
        return Err(Error::InvalidParameter {
            name: "censoring value",
            value: censor_value,
        });
    }
    let time = df.column(time_column)?;
    if matches!(
        time.data(),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
    ) {
        return Err(Error::Csv(format!(
            "time column {time_column} is not numeric"
        )));
    }
    let censor = match censor_column {
        Some(name) => {
            let column = df.column(name)?;
            if matches!(
                column.data(),
                ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
            ) {
                return Err(Error::Csv(format!(
                    "censoring column {name} is not numeric"
                )));
            }
            Some(column)
        }
        None => None,
    };
    let mut logs = Vec::new();
    let mut events = Vec::new();
    for row in 0..df.n_rows() {
        let Some(value) = cell(time, row) else {
            continue;
        };
        if value <= 0.0 {
            return Err(Error::Cell {
                column: time_column.to_owned(),
                row: row + 1,
                message: "survival times must be positive".to_owned(),
            });
        }
        let censored = match censor {
            Some(column) => match cell(column, row) {
                Some(flag) => (flag - censor_value).abs() <= f64::EPSILON,
                None => continue,
            },
            None => false,
        };
        logs.push(value.ln());
        events.push(!censored);
    }
    if logs.is_empty() {
        return Err(Error::EmptySample);
    }
    let event_count = events.iter().filter(|event| **event).count();
    if event_count == 0 {
        return Err(Error::Unsupported(
            "the sample has no events; a lognormal fit is not identified".to_owned(),
        ));
    }
    if logs.len() < 2 {
        return Err(Error::SampleTooSmall {
            n: logs.len(),
            min: 2,
        });
    }
    // A constant sample makes the lognormal scale collapse to zero.
    let min_log = logs.iter().copied().fold(f64::INFINITY, f64::min);
    let max_log = logs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if max_log <= min_log {
        return Err(Error::ConstantSample);
    }
    let event_logs: Vec<f64> = logs
        .iter()
        .zip(&events)
        .filter(|(_, event)| **event)
        .map(|(log, _)| *log)
        .collect();
    let event_mean = event_logs.iter().sum::<f64>() / event_logs.len() as f64;
    let event_variance = event_logs
        .iter()
        .map(|value| (value - event_mean).powi(2))
        .sum::<f64>()
        / event_logs.len() as f64;
    let mut mu = event_mean;
    let mut sigma = event_variance.sqrt().max(0.1);
    let span = (max_log - min_log).max(1.0);
    for _ in 0..80 {
        let old_mu = mu;
        let old_sigma = sigma;
        mu = golden_max(
            |candidate| lognormal_loglik(candidate, sigma, &logs, &events),
            min_log - 8.0 * span,
            max_log + 8.0 * span,
        );
        let log_sigma = golden_max(
            |candidate| lognormal_loglik(mu, candidate.exp(), &logs, &events),
            -10.0,
            4.0,
        );
        sigma = log_sigma.exp();
        if (mu - old_mu).abs() <= 1e-10 * (1.0 + mu.abs())
            && (sigma - old_sigma).abs() <= 1e-10 * (1.0 + sigma.abs())
        {
            break;
        }
    }
    let log_likelihood = lognormal_loglik(mu, sigma, &logs, &events);
    if !log_likelihood.is_finite() {
        return Err(Error::NotConverged {
            detail: "censored lognormal coordinate ascent".to_owned(),
        });
    }
    let b10 = (mu + normal_quantile(0.1) * sigma).exp();
    let z = normal_quantile(0.975);
    let (mu_ci, sigma_ci, b10_ci) = match lognormal_log_covariance(mu, sigma, &logs, &events) {
        Some(((v11, v12), (_, v22))) if v11 >= 0.0 && v22 >= 0.0 => {
            let mu_se = v11.sqrt();
            let sigma_se = v22.sqrt();
            let z10 = normal_quantile(0.1);
            let gradient = [1.0, z10 * sigma];
            let b10_variance = gradient[0] * gradient[0] * v11
                + 2.0 * gradient[0] * gradient[1] * v12
                + gradient[1] * gradient[1] * v22;
            let b10_se = b10_variance.max(0.0).sqrt();
            (
                Some((mu - z * mu_se, mu + z * mu_se)),
                Some((
                    (sigma.ln() - z * sigma_se).exp(),
                    (sigma.ln() + z * sigma_se).exp(),
                )),
                Some(((b10.ln() - z * b10_se).exp(), (b10.ln() + z * b10_se).exp())),
            )
        }
        _ => (None, None, None),
    };
    Ok(CensoredLognormal {
        mu,
        sigma,
        median: mu.exp(),
        mean_life: (mu + 0.5 * sigma * sigma).exp(),
        b10,
        mu_ci,
        sigma_ci,
        b10_ci,
        log_likelihood,
        aic: 4.0 - 2.0 * log_likelihood,
        n: logs.len(),
        events: event_count,
        censored: logs.len() - event_count,
        excluded: df.n_rows() - logs.len(),
    })
}

/// Builds the report and fitted survival curve.
pub fn analyze_lognormal(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
) -> Result<AnalysisOutput> {
    let model = fit_lognormal(df, time_column, censor_column, censor_value)?;
    let max_time = df
        .column(time_column)?
        .sample(crate::data::MissingPolicy::Drop)?
        .max()?;
    let x_max = (max_time * 1.05).max(1.0);
    let mut lines = Vec::new();
    let mut previous = (0.0, 1.0);
    for step in 1..=200 {
        let time = x_max * step as f64 / 200.0;
        let z = (time.ln() - model.mu) / model.sigma;
        let survival = normal_sf(z);
        lines.push(Segment {
            from: Point {
                x: previous.0,
                y: previous.1,
            },
            to: Point {
                x: time,
                y: survival,
            },
            width: 2.0,
        });
        previous = (time, survival);
    }
    let mut result = AnalysisResult::new(
        format!("Lognormal Reliability — column {time_column}"),
        "Right-censored lognormal MLE; coordinate ascent on ln(x)",
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Location (µ)", model.mu),
        ("Scale (σ)", model.sigma),
        ("Median", model.median),
        ("Mean life", model.mean_life),
        ("B10 life", model.b10),
        ("Log-likelihood", model.log_likelihood),
        ("AIC", model.aic),
        ("Events", model.events as f64),
        ("Censored", model.censored as f64),
    ];
    let interval = |value: Option<(f64, f64)>| {
        value.map_or_else(
            || "—".to_owned(),
            |(lower, upper)| format!("({lower:.6}, {upper:.6})"),
        )
    };
    result.extra.push(Item::Table {
        headers: vec![
            "Quantity".to_owned(),
            "Value".to_owned(),
            "95% CI".to_owned(),
        ],
        rows: vec![
            vec![
                "Location (µ)".to_owned(),
                format!("{:.6}", model.mu),
                interval(model.mu_ci),
            ],
            vec![
                "Scale (σ)".to_owned(),
                format!("{:.6}", model.sigma),
                interval(model.sigma_ci),
            ],
            vec![
                "Median".to_owned(),
                format!("{:.6}", model.median),
                "—".to_owned(),
            ],
            vec![
                "Mean life".to_owned(),
                format!("{:.6}", model.mean_life),
                "—".to_owned(),
            ],
            vec![
                "B10 life".to_owned(),
                format!("{:.6}", model.b10),
                interval(model.b10_ci),
            ],
            vec![
                "Log-likelihood".to_owned(),
                format!("{:.6}", model.log_likelihood),
                "—".to_owned(),
            ],
            vec![
                "AIC".to_owned(),
                format!("{:.6}", model.aic),
                "—".to_owned(),
            ],
        ],
    });
    result.notes = vec![
        (
            "Estimator".to_owned(),
            "right-censored lognormal likelihood; coordinate ascent on (µ, ln σ)".to_owned(),
        ),
        (
            "Censoring".to_owned(),
            "events contribute ln f(t), censored observations contribute ln S(t)".to_owned(),
        ),
        (
            "Intervals".to_owned(),
            "95% Wald intervals from the observed Hessian in (µ, ln σ); B10 uses the delta method"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    result.warnings.push(
        "lognormal reliability conventions and interval estimates have not been verified against captured output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(PlotSpec {
            title: format!("Lognormal Reliability — {time_column}"),
            x_axis: linear_axis("Time", 0.0, x_max),
            y_axis: linear_axis("Survival probability", 0.0, 1.0),
            size: (900, 520),
            points: Vec::new(),
            lines,
            bands: Vec::new(),
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: Vec::new(),
        }),
    })
}

/// Right-censored exponential maximum-likelihood fit.
#[derive(Debug, Clone, PartialEq)]
pub struct CensoredExponential {
    /// Mean/scale parameter.
    pub theta: f64,
    /// 95% confidence interval for theta.
    pub theta_ci: Option<(f64, f64)>,
    /// Maximized log-likelihood.
    pub log_likelihood: f64,
    /// Akaike information criterion.
    pub aic: f64,
    /// B10 life.
    pub b10: f64,
    /// Observations used.
    pub n: usize,
    /// Events.
    pub events: usize,
    /// Right-censored observations.
    pub censored: usize,
    /// Rows excluded.
    pub excluded: usize,
}

/// Fits a right-censored exponential distribution.
pub fn fit_exponential(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
) -> Result<CensoredExponential> {
    if !censor_value.is_finite() {
        return Err(Error::InvalidParameter {
            name: "censoring value",
            value: censor_value,
        });
    }
    let time = df.column(time_column)?;
    if matches!(
        time.data(),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
    ) {
        return Err(Error::Csv(format!(
            "time column {time_column} is not numeric"
        )));
    }
    let censor = match censor_column {
        Some(name) => {
            let column = df.column(name)?;
            if matches!(
                column.data(),
                ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
            ) {
                return Err(Error::Csv(format!(
                    "censoring column {name} is not numeric"
                )));
            }
            Some(column)
        }
        None => None,
    };
    let mut total_time = 0.0;
    let mut n = 0usize;
    let mut events = 0usize;
    for row in 0..df.n_rows() {
        let Some(value) = cell(time, row) else {
            continue;
        };
        if value <= 0.0 {
            return Err(Error::Cell {
                column: time_column.to_owned(),
                row: row + 1,
                message: "survival times must be positive".to_owned(),
            });
        }
        let censored = match censor {
            Some(column) => match cell(column, row) {
                Some(flag) => (flag - censor_value).abs() <= f64::EPSILON,
                None => continue,
            },
            None => false,
        };
        n += 1;
        total_time += value;
        if !censored {
            events += 1;
        }
    }
    if n == 0 {
        return Err(Error::EmptySample);
    }
    if events == 0 {
        return Err(Error::Unsupported(
            "the sample has no events; an exponential fit is not identified".to_owned(),
        ));
    }
    let theta = total_time / events as f64;
    let log_likelihood = -(events as f64) * theta.ln() - total_time / theta;
    let alpha = 0.05;
    // The captured run uses the normal approximation on the log scale:
    // θ·exp(±z/√events), which is asymmetric on the time axis.
    let _ = alpha;
    let half = 1.959_963_984_540_054 / (events as f64).sqrt();
    let lower = theta * (-half).exp();
    let upper = theta * half.exp();
    Ok(CensoredExponential {
        theta,
        theta_ci: Some((lower, upper)),
        log_likelihood,
        aic: 2.0 - 2.0 * log_likelihood,
        b10: theta * (-0.9f64.ln()),
        n,
        events,
        censored: n - events,
        excluded: df.n_rows() - n,
    })
}

/// Builds the report and fitted survival curve.
pub fn analyze_exponential(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
) -> Result<AnalysisOutput> {
    let model = fit_exponential(df, time_column, censor_column, censor_value)?;
    let max_time = df
        .column(time_column)?
        .sample(crate::data::MissingPolicy::Drop)?
        .max()?;
    let x_max = (max_time * 1.05).max(1.0);
    let mut lines = Vec::new();
    let mut previous = (0.0, 1.0);
    for step in 1..=200 {
        let time = x_max * step as f64 / 200.0;
        let survival = (-time / model.theta).exp();
        lines.push(Segment {
            from: Point {
                x: previous.0,
                y: previous.1,
            },
            to: Point {
                x: time,
                y: survival,
            },
            width: 2.0,
        });
        previous = (time, survival);
    }
    let mut result = AnalysisResult::new(
        format!("Exponential Reliability — column {time_column}"),
        "Right-censored exponential MLE; θ = total time / events",
    );
    result.n = model.n;
    result.excluded = model.excluded;
    let se = model.theta / (model.events as f64).sqrt();
    let factor = (1.959_963_984_540_054 / (model.events as f64).sqrt()).exp();
    let interval = |estimate: f64| (estimate / factor, estimate * factor);
    result.statistics = vec![
        ("Mean/scale (θ)", model.theta),
        ("Standard error", se),
        ("Log-likelihood", model.log_likelihood),
        ("AIC", model.aic),
        ("B10 life", model.b10),
        ("Events", model.events as f64),
        ("Censored", model.censored as f64),
    ];
    if let Some((lower, upper)) = model.theta_ci {
        result.statistics.push(("θ CI lower", lower));
        result.statistics.push(("θ CI upper", upper));
    }
    // Characteristics of the fitted distribution, with the same normal
    // approximation on the log scale that the captured run prints.
    let median = model.theta * std::f64::consts::LN_2;
    let q1 = model.theta * (4.0f64 / 3.0).ln();
    let q3 = model.theta * 4.0f64.ln();
    let characteristics: [(&str, f64); 5] = [
        ("Mean(MTTF)", model.theta),
        ("Standard deviation", model.theta),
        ("Median", median),
        ("First quartile (Q1)", q1),
        ("Third quartile (Q3)", q3),
    ];
    let mut characteristic_rows = Vec::with_capacity(6);
    for (name, estimate) in characteristics {
        let (lower, upper) = interval(estimate);
        characteristic_rows.push(vec![
            name.to_owned(),
            format!("{estimate:.6}"),
            format!("{:.6}", estimate / (model.events as f64).sqrt()),
            format!("{lower:.6}"),
            format!("{upper:.6}"),
        ]);
    }
    let iqr = q3 - q1;
    let (iqr_lower, iqr_upper) = interval(iqr);
    characteristic_rows.push(vec![
        "Interquartile range (IQR)".to_owned(),
        format!("{iqr:.6}"),
        format!("{:.6}", iqr / (model.events as f64).sqrt()),
        format!("{iqr_lower:.6}"),
        format!("{iqr_upper:.6}"),
    ]);
    result.extra.push(Item::Table {
        headers: vec![
            "Characteristic".to_owned(),
            "Estimate".to_owned(),
            "Standard error".to_owned(),
            "CI lower".to_owned(),
            "CI upper".to_owned(),
        ],
        rows: characteristic_rows,
    });
    // Percentiles: the captured table lists 1–10, the deciles and 91–99.
    let percentages: Vec<u32> = (1..=10)
        .chain((20..=90).step_by(10))
        .chain(91..=99)
        .collect();
    let mut percentile_rows = Vec::with_capacity(percentages.len());
    for percent in percentages {
        let p = f64::from(percent) / 100.0;
        let estimate = -model.theta * (1.0 - p).ln();
        let (lower, upper) = interval(estimate);
        percentile_rows.push(vec![
            percent.to_string(),
            format!("{estimate:.6}"),
            format!("{:.6}", estimate / (model.events as f64).sqrt()),
            format!("{lower:.6}"),
            format!("{upper:.6}"),
        ]);
    }
    result.extra.push(Item::Table {
        headers: vec![
            "Percent".to_owned(),
            "Percentile".to_owned(),
            "Standard error".to_owned(),
            "CI lower".to_owned(),
            "CI upper".to_owned(),
        ],
        rows: percentile_rows,
    });
    result.notes = vec![
        (
            "Estimator".to_owned(),
            "θ = total observed time / number of events; right-censored observations              contribute their time to the denominator"
                .to_owned(),
        ),
        (
            "Interval".to_owned(),
            "95% normal interval on the log scale: θ·exp(±z/√events), the captured convention".to_owned(),
        ),
        (
            "Assumption".to_owned(),
            "constant hazard, independent non-informative censoring".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);

    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(PlotSpec {
            title: format!("Exponential Reliability — {time_column}"),
            x_axis: linear_axis("Time", 0.0, x_max),
            y_axis: linear_axis("Survival probability", 0.0, 1.0),
            size: (900, 520),
            points: Vec::new(),
            lines,
            bands: Vec::new(),
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: Vec::new(),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("status", vec![1.0, 0.0, 1.0, 1.0, 0.0]),
        ])
        .unwrap()
    }

    #[test]
    fn kaplan_meier_matches_a_hand_calculation() {
        let model = fit(&frame(), "time", Some("status"), 0.0, 0.95).expect("fit");
        assert_eq!(model.n, 5);
        assert_eq!(model.events, 3);
        assert_eq!(model.censored, 2);
        assert_eq!(model.points.len(), 5);
        assert!((model.points[0].survival - 0.8).abs() < 1e-12);
        assert!((model.points[2].survival - 0.8 * 2.0 / 3.0).abs() < 1e-12);
        assert!((model.points[3].survival - 0.8 / 3.0).abs() < 1e-12);
        assert_eq!(model.median, Some(4.0));
        if let Some((lower, upper)) = model.median_ci {
            assert!(lower <= 4.0 && 4.0 <= upper);
        }
        assert!(model.points[0].ci_lower <= model.points[0].survival);
        assert!(model.points[0].ci_upper >= model.points[0].survival);
    }

    #[test]
    fn no_censor_column_treats_every_observation_as_an_event() {
        let model = fit(&frame(), "time", None, 0.0, 0.95).expect("fit");
        assert_eq!(model.events, 5);
        assert_eq!(model.censored, 0);
        assert!((model.points[0].survival - 0.8).abs() < 1e-12);
    }

    #[test]
    fn non_positive_times_are_rejected() {
        let df = DataFrame::new(vec![Column::new_f64("time", vec![0.0, 1.0])]).unwrap();
        assert!(matches!(
            fit(&df, "time", None, 0.0, 0.95),
            Err(Error::Cell { .. })
        ));
    }

    #[test]
    fn no_events_is_an_error() {
        let df = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0]),
            Column::new_f64("status", vec![0.0, 0.0]),
        ])
        .unwrap();
        assert!(matches!(
            fit(&df, "time", Some("status"), 0.0, 0.95),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn censored_weibull_recovers_a_complete_synthetic_sample() {
        let n = 60;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                10.0 * (-(1.0 - p).ln()).powf(0.5)
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("time", values)]).unwrap();
        let model = fit_weibull(&df, "time", None, 0.0).expect("fit");
        assert!((model.shape - 2.0).abs() < 0.05, "shape {}", model.shape);
        assert!((model.scale - 10.0).abs() < 0.2, "scale {}", model.scale);
        assert_eq!(model.events, n);
        assert_eq!(model.censored, 0);
        let (shape_low, shape_high) = model.shape_ci.expect("shape CI");
        assert!(shape_low < model.shape && model.shape < shape_high);
        let (b10_low, b10_high) = model.b10_ci.expect("B10 CI");
        assert!(b10_low < model.b10 && model.b10 < b10_high);
    }

    #[test]
    fn kaplan_meier_matches_the_captured_normal_interval() {
        // Captured run, n = 12, two right-censored at 0: S(8) = 0.75 with the
        // normal 95% interval 0.505005-0.995, median 15.
        let times = [
            5.0, 8.0, 8.0, 11.0, 12.5, 15.0, 18.0, 20.0, 22.0, 25.0, 27.0, 30.0,
        ];
        let mut censor = vec![1.0; 10];
        censor.extend(vec![0.0; 2]);
        let df = DataFrame::new(vec![
            Column::new_f64("time", times.to_vec()),
            Column::new_f64("censor", censor),
        ])
        .unwrap();
        let model = fit(&df, "time", Some("censor"), 0.0, 0.95).expect("model");
        let point = model
            .points
            .iter()
            .find(|point| point.time == 8.0)
            .expect("t = 8");
        assert!((point.survival - 0.75).abs() < 1e-12);
        assert!(
            (point.ci_lower - 0.505005).abs() < 1e-5,
            "{}",
            point.ci_lower
        );
        assert!((point.ci_upper - 0.995).abs() < 1e-5, "{}", point.ci_upper);
        assert_eq!(model.median, Some(15.0));

        let text = report("time", &model).to_session_text();
        assert!(text.contains("Mean (MTTF)"), "{text}");
        assert!(text.contains("17.041"), "{text}");
        assert!(text.contains("First quartile (Q1)"), "{text}");
        assert!(text.contains("22.000000"), "{text}");
    }

    #[test]
    fn censored_weibull_uses_censored_observations() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0];
        let status = vec![1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let df = DataFrame::new(vec![
            Column::new_f64("time", values),
            Column::new_f64("status", status),
        ])
        .unwrap();
        let model = fit_weibull(&df, "time", Some("status"), 0.0).expect("fit");
        assert_eq!(model.events, 4);
        assert_eq!(model.censored, 4);
        assert!(model.shape > 0.0 && model.scale > 0.0);
    }

    #[test]
    fn censored_weibull_report_and_curve_are_built() {
        let df = frame();
        let out = analyze_weibull(&df, "time", Some("status"), 0.0).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Weibull Reliability"), "{text}");
        assert!(text.contains("B10 life"), "{text}");
        assert!(!out.plot.expect("plot").lines.is_empty());
    }

    #[test]
    fn exponential_reliability_uses_total_time_over_events() {
        let df = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 10.0]),
            Column::new_f64("status", vec![1.0, 1.0, 1.0, 1.0, 0.0]),
        ])
        .unwrap();
        let model = fit_exponential(&df, "time", Some("status"), 0.0).expect("fit");
        assert_eq!(model.events, 4);
        assert_eq!(model.censored, 1);
        assert!((model.theta - 20.0 / 4.0).abs() < 1e-12);
        let (lower, upper) = model.theta_ci.expect("CI");
        assert!(lower < model.theta && model.theta < upper);
    }

    #[test]
    fn exponential_reliability_report_and_curve_are_built() {
        let df = DataFrame::new(vec![Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0])]).unwrap();
        let out = analyze_exponential(&df, "time", None, 0.0).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Exponential Reliability"), "{text}");
        assert!(text.contains("B10 life"), "{text}");
        assert!(!out.plot.expect("plot").lines.is_empty());
    }

    #[test]
    fn lognormal_reliability_recovers_complete_synthetic_data() {
        let values: Vec<f64> = (1..=80)
            .map(|i| {
                let p = (i as f64 - 0.5) / 80.0;
                (2.0 + 0.5 * normal_quantile(p)).exp()
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("time", values)]).unwrap();
        let model = fit_lognormal(&df, "time", None, 0.0).expect("fit");
        assert!((model.mu - 2.0).abs() < 0.05, "mu {}", model.mu);
        assert!((model.sigma - 0.5).abs() < 0.05, "sigma {}", model.sigma);
        assert_eq!(model.events, 80);
        let (mu_low, mu_high) = model.mu_ci.expect("mu CI");
        assert!(mu_low < model.mu && model.mu < mu_high);
        let (sigma_low, sigma_high) = model.sigma_ci.expect("sigma CI");
        assert!(sigma_low < model.sigma && model.sigma < sigma_high);
        let (b10_low, b10_high) = model.b10_ci.expect("B10 CI");
        assert!(b10_low < model.b10 && model.b10 < b10_high);
    }

    #[test]
    fn lognormal_reliability_handles_censoring() {
        let df = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 5.0, 8.0, 10.0]),
            Column::new_f64("status", vec![1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]),
        ])
        .unwrap();
        let model = fit_lognormal(&df, "time", Some("status"), 0.0).expect("fit");
        assert_eq!(model.events, 4);
        assert_eq!(model.censored, 3);
        assert!(model.sigma > 0.0);
    }

    #[test]
    fn lognormal_reliability_report_and_curve_are_built() {
        let df =
            DataFrame::new(vec![Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 5.0])]).unwrap();
        let out = analyze_lognormal(&df, "time", None, 0.0).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Lognormal Reliability"), "{text}");
        assert!(text.contains("B10 life"), "{text}");
        assert!(!out.plot.expect("plot").lines.is_empty());
    }

    #[test]
    fn report_and_step_plot_are_built() {
        let out = analyze(&frame(), "time", Some("status"), 0.0, 0.95).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Kaplan–Meier"), "{text}");
        assert!(text.contains("Median survival"), "{text}");
        let plot = out.plot.expect("plot");
        assert!(!plot.lines.is_empty());
    }
}
