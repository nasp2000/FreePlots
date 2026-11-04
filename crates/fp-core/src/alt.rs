//! Accelerated life testing with the Arrhenius relationship.
//!
//! The life of the units is described by a **Weibull** distribution whose
//! characteristic life depends on the stress through the Arrhenius law:
//!
//! ```text
//! ln η(S) = a + b·(1/S − 1/S̄)          (S in kelvin)
//! ```
//!
//! where `S̄` is the mean reciprocal stress of the study, so `a` is the log
//! characteristic life at the centre of the design and `b = Ea/k`. The shape
//! `κ` is shared by every stress level — the usual assumption of an
//! accelerated life model.
//!
//! The three parameters `(ln κ, a, b)` are fitted by maximum likelihood with
//! the in-tree Nelder–Mead simplex; the log-likelihood sums the event terms
//! `ln κ − ln η + (κ − 1)·ln(t/η) − (t/η)^κ` and the censored terms
//! `−(t/η)^κ`. The standard errors come from the numerical Hessian of the
//! log-likelihood at the maximum (Wald tests), and the report extrapolates the
//! characteristic life and the B10 life to a **use stress**, with the
//! acceleration factor against the highest stress of the study.
//!
//! The comparison run has accelerated life testing; its models and
//! conventions were not captured, so the numbers are not claimed as reference
//! parity — see `docs/parity.md`.

use crate::data::{Column, DataFrame};
use crate::error::{Error, Result};
use crate::optimize::nelder_mead;
use crate::plot::{PlotSpec, Point, Segment, linear_axis, panel_plot};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// Life distribution of the accelerated life model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LifeModel {
    /// Weibull with a shared shape.
    #[default]
    Weibull,
    /// Lognormal with a shared log-scale.
    Lognormal,
    /// Exponential (no shape parameter).
    Exponential,
}

impl LifeModel {
    /// Parses `weibull`, `lognormal`, `exponential`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "weibull" | "w" => Some(Self::Weibull),
            "lognormal" | "log-normal" | "ln" => Some(Self::Lognormal),
            "exponential" | "exp" => Some(Self::Exponential),
            _ => None,
        }
    }

    /// Name used in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Weibull => "Weibull",
            Self::Lognormal => "Lognormal",
            Self::Exponential => "Exponential",
        }
    }
}

/// Acceleration relationship between the stress and the life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccelerationModel {
    /// Arrhenius: `ln η = a + b/S`.
    #[default]
    Arrhenius,
    /// Inverse power: `ln η = a + b·ln S`.
    InversePower,
    /// Eyring: `ln η = a + b/S − ln S`.
    Eyring,
}

impl AccelerationModel {
    /// Parses `arrhenius` and `power`/`inverse-power`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "arrhenius" | "temperature" => Some(Self::Arrhenius),
            "power" | "inverse-power" | "inversepower" => Some(Self::InversePower),
            "eyring" => Some(Self::Eyring),
            _ => None,
        }
    }

    /// Name used in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Arrhenius => "Arrhenius (1/S)",
            Self::InversePower => "inverse power (ln S)",
            Self::Eyring => "Eyring (1/S − ln S)",
        }
    }

    /// Transformed stress used by the linear predictor.
    #[must_use]
    pub fn transform(self, stress: f64) -> f64 {
        match self {
            Self::Arrhenius | Self::Eyring => 1.0 / stress,
            Self::InversePower => stress.ln(),
        }
    }

    /// Fixed term added to the log characteristic life at a stress.
    ///
    /// The Eyring law carries the `−ln S` of its pre-exponential factor; the
    /// other laws have none.
    #[must_use]
    pub fn offset(self, stress: f64) -> f64 {
        match self {
            Self::Eyring => -stress.ln(),
            Self::Arrhenius | Self::InversePower => 0.0,
        }
    }
}

/// Fitted accelerated life model.
#[derive(Debug, Clone, PartialEq)]
pub struct AltFit {
    /// Life distribution.
    pub life: LifeModel,
    /// Acceleration relationship.
    pub acceleration: AccelerationModel,
    /// Weibull shape (or lognormal log-scale); `None` for the exponential.
    pub shape: Option<f64>,
    /// Standard error of the shape.
    pub shape_se: Option<f64>,
    /// Log characteristic life at the centre of the design.
    pub intercept: f64,
    /// Standard error of the intercept.
    pub intercept_se: f64,
    /// Arrhenius slope (`Ea/k`).
    pub slope: f64,
    /// Standard error of the slope.
    pub slope_se: f64,
    /// Mean reciprocal stress used to centre the model.
    pub centre: f64,
    /// Stress levels of the study.
    pub stresses: Vec<f64>,
    /// Use stress of the extrapolation.
    pub use_stress: f64,
    /// Characteristic life at the use stress.
    pub life_at_use: f64,
    /// B10 life at the use stress.
    pub b10_at_use: f64,
    /// How many times longer the life at the use stress is than at the highest
    /// tested stress.
    pub acceleration_factor: f64,
    /// Events (failures) used.
    pub events: usize,
    /// Right-censored observations used.
    pub censored: usize,
    /// Log-likelihood at the maximum.
    pub log_likelihood: f64,
    /// Akaike information criterion.
    pub aic: f64,
    /// Whether the simplex converged.
    pub converged: bool,
    /// Observed `(1/S, ln t)` pairs of the failures, for the plot.
    pub observed: Vec<(f64, f64)>,
}

/// Solves a linear system by Gauss–Jordan elimination with partial pivoting.
#[allow(clippy::needless_range_loop)]
fn invert(matrix: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = matrix.len();
    let mut a = matrix.to_vec();
    let mut inverse = vec![vec![0.0; n]; n];
    for row in 0..n {
        inverse[row][row] = 1.0;
    }
    for column in 0..n {
        let mut pivot = column;
        for row in (column + 1)..n {
            if a[row][column].abs() > a[pivot][column].abs() {
                pivot = row;
            }
        }
        if a[pivot][column].abs() < 1e-12 {
            return None;
        }
        a.swap(column, pivot);
        inverse.swap(column, pivot);
        let divisor = a[column][column];
        for value in a[column].iter_mut() {
            *value /= divisor;
        }
        for value in inverse[column].iter_mut() {
            *value /= divisor;
        }
        for row in 0..n {
            if row == column {
                continue;
            }
            let factor = a[row][column];
            if factor == 0.0 {
                continue;
            }
            for k in 0..n {
                a[row][k] -= factor * a[column][k];
                inverse[row][k] -= factor * inverse[column][k];
            }
        }
    }
    Some(inverse)
}

/// Fits the model.
///
/// `use_stress` is the stress at which the life is extrapolated; `censor` is
/// the column holding the censoring indicator and `censor_value` the value of
/// a right-censored observation.
#[allow(clippy::too_many_arguments)]
pub fn fit(
    df: &DataFrame,
    time: &str,
    stress: &str,
    censor: Option<&str>,
    censor_value: f64,
    use_stress: f64,
) -> Result<AltFit> {
    fit_with(
        df,
        time,
        stress,
        censor,
        censor_value,
        use_stress,
        LifeModel::Weibull,
        AccelerationModel::Arrhenius,
    )
}

/// Fits the model with the chosen life distribution and acceleration law.
#[allow(clippy::too_many_arguments)]
pub fn fit_with(
    df: &DataFrame,
    time: &str,
    stress: &str,
    censor: Option<&str>,
    censor_value: f64,
    use_stress: f64,
    life: LifeModel,
    acceleration: AccelerationModel,
) -> Result<AltFit> {
    if !use_stress.is_finite() || use_stress <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "use stress",
            value: use_stress,
        });
    }
    let time_column = df.column(time)?;
    let stress_column = df.column(stress)?;
    let times = time_column.numeric_by_row();
    let stresses = stress_column.numeric_by_row();
    let censor_values = match censor {
        Some(name) => Some(df.column(name)?.numeric_by_row()),
        None => None,
    };

    let mut observations: Vec<(f64, f64, bool)> = Vec::new();
    for row in 0..df.n_rows() {
        if time_column.is_missing(row) || stress_column.is_missing(row) {
            continue;
        }
        let value = times[row];
        let level = stresses[row];
        if !value.is_finite() || value <= 0.0 {
            return Err(Error::NonPositive { index: row, value });
        }
        if !level.is_finite() || level <= 0.0 {
            return Err(Error::NonPositive {
                index: row,
                value: level,
            });
        }
        let event = match &censor_values {
            Some(flags) => {
                let flag = flags[row];
                if !flag.is_finite() {
                    continue;
                }
                (flag - censor_value).abs() > 1e-12
            }
            None => true,
        };
        observations.push((value, level, event));
    }
    if observations.len() < 4 {
        return Err(Error::SampleTooSmall {
            n: observations.len(),
            min: 4,
        });
    }
    let events = observations.iter().filter(|(_, _, event)| *event).count();
    if events < 2 {
        return Err(Error::Unsupported(
            "the model needs at least two failures".to_owned(),
        ));
    }
    let mut stresses_seen: Vec<f64> = observations.iter().map(|(_, level, _)| *level).collect();
    stresses_seen.sort_by(f64::total_cmp);
    stresses_seen.dedup_by(|left, right| (*left - *right).abs() < 1e-12);
    if stresses_seen.len() < 2 {
        return Err(Error::Unsupported(
            "accelerated life testing needs at least two stress levels".to_owned(),
        ));
    }
    let centre = observations
        .iter()
        .map(|(_, level, _)| acceleration.transform(*level))
        .sum::<f64>()
        / observations.len() as f64;
    let dimension = match life {
        LifeModel::Exponential => 2,
        _ => 3,
    };

    // Log-likelihood of (shape, intercept, slope); the exponential has no shape
    // so its vector is (intercept, slope).
    let log_likelihood = |parameters: &[f64]| -> f64 {
        let (shape, intercept, slope) = match life {
            LifeModel::Exponential => (1.0, parameters[0], parameters[1]),
            _ => (parameters[0].exp(), parameters[1], parameters[2]),
        };
        if !shape.is_finite() || shape <= 0.0 {
            return f64::NEG_INFINITY;
        }
        let mut total = 0.0;
        for (value, level, event) in &observations {
            let eta = (intercept
                + slope * (acceleration.transform(*level) - centre)
                + acceleration.offset(*level))
            .exp();
            if !eta.is_finite() || eta <= 0.0 {
                return f64::NEG_INFINITY;
            }
            total += match life {
                LifeModel::Weibull => {
                    let ratio = value / eta;
                    if *event {
                        shape.ln() - eta.ln() + (shape - 1.0) * ratio.ln() - ratio.powf(shape)
                    } else {
                        -ratio.powf(shape)
                    }
                }
                LifeModel::Lognormal => {
                    // `shape` is the log-scale sigma.
                    let z = (value.ln() - eta.ln()) / shape;
                    if *event {
                        -value.ln()
                            - shape.ln()
                            - 0.5 * z * z
                            - 0.5 * (2.0 * std::f64::consts::PI).ln()
                    } else {
                        crate::special::normal_cdf(-z).max(1e-300).ln()
                    }
                }
                LifeModel::Exponential => {
                    if *event {
                        -eta.ln() - value / eta
                    } else {
                        -value / eta
                    }
                }
            };
        }
        if total.is_finite() {
            total
        } else {
            f64::NEG_INFINITY
        }
    };

    // Starting values: the pooled shape and the mean log life at the centre.
    let mut pooled = observations
        .iter()
        .map(|(value, _, _)| *value)
        .collect::<Vec<_>>();
    pooled.sort_by(f64::total_cmp);
    let mean_log = pooled.iter().map(|value| value.ln()).sum::<f64>() / pooled.len() as f64;
    let variance_log = pooled
        .iter()
        .map(|value| (value.ln() - mean_log).powi(2))
        .sum::<f64>()
        / (pooled.len() - 1) as f64;
    let start_shape = (1.2 / variance_log.sqrt().max(1e-6)).clamp(0.1, 10.0);
    let mut start = match life {
        LifeModel::Exponential => vec![mean_log, 0.0],
        _ => vec![start_shape.ln(), mean_log, 0.0],
    };
    let mut steps = vec![0.1; dimension];
    for step in steps.iter_mut().skip(1) {
        *step = 0.05;
    }
    let minimum = nelder_mead(&start, &steps, 4000, 1e-10, |parameters| {
        -log_likelihood(parameters)
    })?;
    start = minimum.arguments.clone();
    let parameters = start;
    let converged = minimum.value.is_finite();
    let (shape, intercept, slope) = match life {
        LifeModel::Exponential => (None, parameters[0], parameters[1]),
        _ => (Some(parameters[0].exp()), parameters[1], parameters[2]),
    };
    if let Some(shape) = shape
        && (!shape.is_finite() || shape <= 0.0)
    {
        return Err(Error::NotConverged {
            detail: "the accelerated life shape".to_owned(),
        });
    }

    // Numerical Hessian of the log-likelihood for the standard errors.
    let step_sizes: Vec<f64> = parameters
        .iter()
        .map(|value| 1e-4 * value.abs().max(1.0))
        .collect();
    let mut hessian = vec![vec![0.0; dimension]; dimension];
    for j in 0..dimension {
        for k in 0..dimension {
            let mut value = 0.0;
            for (sj, sk) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                let mut point = parameters.clone();
                point[j] += sj * step_sizes[j];
                point[k] += sk * step_sizes[k];
                value += sj * sk * log_likelihood(&point);
            }
            hessian[j][k] = value / (4.0 * step_sizes[j] * step_sizes[k]);
        }
    }
    let covariance = invert(&hessian).map(|matrix| {
        matrix
            .iter()
            .map(|row| row.iter().map(|value| -value).collect::<Vec<f64>>())
            .collect::<Vec<Vec<f64>>>()
    });
    let standard_errors: Vec<f64> = covariance.as_ref().map_or_else(
        || vec![f64::NAN; dimension],
        |matrix| {
            (0..dimension)
                .map(|index| matrix[index][index].max(0.0).sqrt())
                .collect()
        },
    );

    let characteristic = |level: f64| -> f64 {
        (intercept + slope * (acceleration.transform(level) - centre) + acceleration.offset(level))
            .exp()
    };
    let life_at_use = characteristic(use_stress);
    if !life_at_use.is_finite() || life_at_use <= 0.0 {
        return Err(Error::NotConverged {
            detail: "the life at the use stress".to_owned(),
        });
    }
    // The B10 life: the 10th percentile of each distribution.
    let b10_at_use = match life {
        LifeModel::Weibull => life_at_use * (-(0.9f64.ln())).powf(1.0 / shape.unwrap_or(1.0)),
        LifeModel::Lognormal => {
            let sigma = shape.unwrap_or(1.0);
            (life_at_use.ln() + crate::special::normal_quantile(0.1) * sigma).exp()
        }
        LifeModel::Exponential => life_at_use * -(0.9f64.ln()),
    };
    let highest = *stresses_seen.last().unwrap_or(&use_stress);
    // The acceleration factor: how many times longer the units last at the use
    // stress than at the highest tested stress.
    let acceleration_factor = if life_at_use > 0.0 {
        life_at_use / characteristic(highest)
    } else {
        f64::NAN
    };
    let observed = observations
        .iter()
        .filter(|(_, _, event)| *event)
        .map(|(value, level, _)| (acceleration.transform(*level), value.ln()))
        .collect();
    let log_likelihood_value = log_likelihood(&parameters);

    // The shape parameter is the first one (the exponential has none).
    let shape_se = shape.map(|shape| standard_errors[0] * shape);
    let (intercept_se, slope_se) = match life {
        LifeModel::Exponential => (standard_errors[0], standard_errors[1]),
        _ => (standard_errors[1], standard_errors[2]),
    };
    Ok(AltFit {
        life,
        acceleration,
        shape,
        shape_se,
        intercept,
        intercept_se,
        slope,
        slope_se,
        centre,
        stresses: stresses_seen,
        use_stress,
        life_at_use,
        b10_at_use,
        acceleration_factor,
        events,
        censored: observations.len() - events,
        log_likelihood: log_likelihood_value,
        aic: -2.0 * log_likelihood_value + 2.0 * dimension as f64,
        converged,
        observed,
    })
}

/// Plot of the fitted life against the reciprocal stress.
fn plot(model: &AltFit, time: &str) -> PlotSpec {
    let x_min = model.observed.first().map_or(0.0, |(x, _)| *x);
    let x_max = model.observed.first().map_or(1.0, |(x, _)| *x);
    let (mut x_min, mut x_max) = (x_min, x_max);
    for (x, _) in &model.observed {
        x_min = x_min.min(*x);
        x_max = x_max.max(*x);
    }
    let use_x = 1.0 / model.use_stress;
    x_min = x_min.min(use_x);
    x_max = x_max.max(use_x);
    let pad = ((x_max - x_min).abs() * 0.05).max(1e-12);
    let at = |x: f64| model.intercept + model.slope * (x - model.centre);
    let low = at(x_min - pad).min(at(x_max + pad));
    let high = at(x_min - pad).max(at(x_max + pad));
    PlotSpec {
        title: "Accelerated Life — Arrhenius".to_owned(),
        x_axis: linear_axis("1 / stress", x_min - pad, x_max + pad),
        y_axis: linear_axis(&format!("ln({time})"), low - 0.5, high + 0.5),
        size: (900, 540),
        points: model
            .observed
            .iter()
            .map(|(x, y)| Point { x: *x, y: *y })
            .collect(),
        lines: vec![Segment {
            from: Point {
                x: x_min - pad,
                y: at(x_min - pad),
            },
            to: Point {
                x: x_max + pad,
                y: at(x_max + pad),
            },
            width: 2.0,
        }],
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Runs the analysis and builds the report.
#[allow(clippy::too_many_arguments)]
pub fn analyze(
    df: &DataFrame,
    time: &str,
    stress: &str,
    censor: Option<&str>,
    censor_value: f64,
    use_stress: f64,
) -> Result<AnalysisOutput> {
    analyze_with(
        df,
        time,
        stress,
        censor,
        censor_value,
        use_stress,
        LifeModel::Weibull,
        AccelerationModel::Arrhenius,
    )
}

/// Runs the analysis with the chosen life distribution and acceleration law.
#[allow(clippy::too_many_arguments)]
pub fn analyze_with(
    df: &DataFrame,
    time: &str,
    stress: &str,
    censor: Option<&str>,
    censor_value: f64,
    use_stress: f64,
    life: LifeModel,
    acceleration: AccelerationModel,
) -> Result<AnalysisOutput> {
    let model = fit_with(
        df,
        time,
        stress,
        censor,
        censor_value,
        use_stress,
        life,
        acceleration,
    )?;
    let mut result = AnalysisResult::new(
        format!(
            "Accelerated Life Testing — {} / {}",
            model.acceleration.name(),
            model.life.name()
        ),
        format!(
            "Maximum likelihood on {time} against the stress in {stress}, extrapolated to {use_stress}"
        ),
    );
    result.n = model.events + model.censored;
    result.excluded = df.n_rows().saturating_sub(result.n);
    let mut statistics = vec![
        ("Intercept", model.intercept),
        ("Intercept SE", model.intercept_se),
        ("Acceleration slope", model.slope),
        ("Slope SE", model.slope_se),
        ("Use stress", model.use_stress),
        ("Life at use", model.life_at_use),
        ("B10 at use", model.b10_at_use),
        ("Acceleration factor", model.acceleration_factor),
        ("Failures", model.events as f64),
        ("Censored", model.censored as f64),
        ("Log-likelihood", model.log_likelihood),
        ("AIC", model.aic),
    ];
    if let (Some(shape), Some(shape_se)) = (model.shape, model.shape_se) {
        let name = if model.life == LifeModel::Lognormal {
            "Sigma (lognormal)"
        } else {
            "Shape (Weibull)"
        };
        statistics.push((name, shape));
        statistics.push(("Shape SE", shape_se));
    }
    result.statistics = statistics;
    let mut terms = Vec::new();
    if let (Some(shape), Some(shape_se)) = (model.shape, model.shape_se) {
        terms.push(vec![
            "Shape".to_owned(),
            format!("{shape:.6}"),
            format!("{shape_se:.6}"),
        ]);
    }
    terms.push(vec![
        "ln life (centre)".to_owned(),
        format!("{:.6}", model.intercept),
        format!("{:.6}", model.intercept_se),
    ]);
    terms.push(vec![
        "Acceleration slope".to_owned(),
        format!("{:.6}", model.slope),
        format!("{:.6}", model.slope_se),
    ]);
    result.extra.push(Item::Table {
        headers: vec!["Term".to_owned(), "Estimate".to_owned(), "SE".to_owned()],
        rows: terms,
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Stress".to_owned(),
            "Characteristic life".to_owned(),
            "B10 life".to_owned(),
        ],
        rows: {
            let mut rows: Vec<Vec<String>> = model
                .stresses
                .iter()
                .map(|level| {
                    let eta = (model.intercept
                        + model.slope * (model.acceleration.transform(*level) - model.centre)
                        + model.acceleration.offset(*level))
                    .exp();
                    let b10 = match model.life {
                        LifeModel::Weibull => {
                            eta * (-(0.9f64.ln())).powf(1.0 / model.shape.unwrap_or(1.0))
                        }
                        LifeModel::Lognormal => (eta.ln()
                            + crate::special::normal_quantile(0.1) * model.shape.unwrap_or(1.0))
                        .exp(),
                        LifeModel::Exponential => eta * -(0.9f64.ln()),
                    };
                    vec![
                        format!("{level:.4}"),
                        format!("{eta:.4}"),
                        format!("{b10:.4}"),
                    ]
                })
                .collect();
            rows.push(vec![
                format!("{:.4} (use)", model.use_stress),
                format!("{:.4}", model.life_at_use),
                format!("{:.4}", model.b10_at_use),
            ]);
            rows
        },
    });
    result.notes = vec![
        (
            "Model".to_owned(),
            "ln η(S) = a + b·(1/S − 1/S̄) with a Weibull shape shared by the stress levels"
                .to_owned(),
        ),
        (
            "Stress units".to_owned(),
            "the Arrhenius law needs an absolute temperature (kelvin); other units describe the \
             same functional form"
                .to_owned(),
        ),
        (
            "Extrapolation".to_owned(),
            "the life at the use stress is an extrapolation beyond the tested levels; the further \
             it sits from the data, the less it can be trusted"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing time, stress or censoring value are excluded; times and stresses \
             must be positive"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the Arrhenius/Weibull fit and the Wald standard errors are standard choices and have not \
         been verified against captured output"
            .to_owned(),
    );
    if !model.converged {
        result
            .warnings
            .push("the simplex did not converge; the estimates are unreliable".to_owned());
    }
    if model.censored == 0 {
        result.warnings.push(
            "every unit failed; a censoring column would make the estimate more realistic"
                .to_owned(),
        );
    }
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot(&model, time)),
    })
}

/// Cause-specific survival of a fitted accelerated-life model at `time`.
#[must_use]
pub fn survival_at(model: &AltFit, time: f64) -> f64 {
    if time <= 0.0 {
        return 1.0;
    }
    match model.life {
        LifeModel::Weibull => {
            let shape = model.shape.unwrap_or(1.0);
            (-(time / model.life_at_use).powf(shape)).exp()
        }
        LifeModel::Lognormal => {
            let sigma = model.shape.unwrap_or(1.0);
            let z = (time.ln() - model.life_at_use.ln()) / sigma;
            crate::special::normal_sf(z)
        }
        LifeModel::Exponential => (-time / model.life_at_use).exp(),
    }
}

fn hazard_at(model: &AltFit, time: f64) -> f64 {
    if time <= 0.0 {
        return 0.0;
    }
    match model.life {
        LifeModel::Weibull => {
            let shape = model.shape.unwrap_or(1.0);
            (shape / model.life_at_use) * (time / model.life_at_use).powf(shape - 1.0)
        }
        LifeModel::Lognormal => {
            let sigma = model.shape.unwrap_or(1.0);
            let z = (time.ln() - model.life_at_use.ln()) / sigma;
            let survival = survival_at(model, time);
            if survival <= 0.0 {
                0.0
            } else {
                crate::special::normal_pdf(z) / (time * sigma * survival)
            }
        }
        LifeModel::Exponential => 1.0 / model.life_at_use,
    }
}

/// Overall survival of the competing-risks model: the product of the
/// cause-specific survivals.
#[must_use]
pub fn combined_survival(models: &[AltFit], time: f64) -> f64 {
    models
        .iter()
        .map(|model| survival_at(model, time))
        .product::<f64>()
        .clamp(0.0, 1.0)
}

/// Cumulative incidence of cause `index` at `time`.
///
/// `F_k(t) = ∫₀ᵗ S(s)·h_k(s) ds`, integrated on a log grid so that a Weibull
/// shape below 1 does not evaluate an infinite hazard at zero.
#[must_use]
pub fn cumulative_incidence(models: &[AltFit], index: usize, time: f64) -> f64 {
    if time <= 0.0 || index >= models.len() {
        return 0.0;
    }
    let low = (time * 1e-12).max(f64::MIN_POSITIVE).ln();
    let high = time.ln();
    if high <= low {
        return 0.0;
    }
    let steps = 4_000;
    let width = (high - low) / steps as f64;
    let integrand = |u: f64| {
        let value = u.exp();
        combined_survival(models, value) * hazard_at(&models[index], value) * value
    };
    let mut total = integrand(low) + integrand(high);
    for step in 1..steps {
        let u = low + width * step as f64;
        total += integrand(u) * if step % 2 == 0 { 2.0 } else { 4.0 };
    }
    (total * width / 3.0).clamp(0.0, 1.0)
}

/// B10 life of cause `index`, when the incidence reaches 0.1.
#[must_use]
pub fn b10_life(models: &[AltFit], index: usize) -> Option<f64> {
    let mut high = 1.0;
    while cumulative_incidence(models, index, high) < 0.1 {
        high *= 2.0;
        if high > 1e12 {
            return None;
        }
    }
    let mut low = 0.0;
    for _ in 0..80 {
        let middle = 0.5 * (low + high);
        if cumulative_incidence(models, index, middle) < 0.1 {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some(0.5 * (low + high))
}

/// Fitted competing-risks accelerated-life model.
#[derive(Debug, Clone, PartialEq)]
pub struct CompetingRisks {
    /// Failure modes, in first-seen order.
    pub modes: Vec<String>,
    /// Cause-specific fits, in the same order.
    pub models: Vec<AltFit>,
    /// Time at which the incidence and survival are reported.
    pub horizon: f64,
    /// Overall survival at the horizon.
    pub survival: f64,
    /// Cumulative incidence of each cause at the horizon.
    pub incidence: Vec<f64>,
    /// B10 life of each cause, when it is reached.
    pub b10: Vec<Option<f64>>,
}

/// Fits one cause-specific accelerated-life model per failure mode.
///
/// A failure of one mode is an event for its own model and a right-censored
/// observation for every other model; a censored observation is censored for
/// all of them. The cumulative incidence is then
/// `F_k(t) = ∫₀ᵗ S(s)·h_k(s) ds` with `S = Π S_k`.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for fewer than two modes or a mode with too
/// few failures, and propagates the fit errors of each cause.
#[allow(clippy::too_many_arguments)]
pub fn competing_risks(
    df: &DataFrame,
    time: &str,
    stress: &str,
    censor: Option<&str>,
    censor_value: f64,
    mode: &str,
    use_stress: f64,
    life: LifeModel,
    acceleration: AccelerationModel,
    horizon: Option<f64>,
) -> Result<CompetingRisks> {
    let time_column = df.column(time)?;
    let stress_column = df.column(stress)?;
    let mode_column = df.column(mode)?;
    let censor_column = censor.map(|name| df.column(name)).transpose()?;
    let mut modes: Vec<String> = Vec::new();
    for row in 0..df.n_rows() {
        if let Some(label) = mode_column.label(row)
            && !modes.contains(&label)
        {
            modes.push(label);
        }
    }
    if modes.len() < 2 {
        return Err(Error::Unsupported(
            "competing risks need at least two failure modes".to_owned(),
        ));
    }
    let mut models = Vec::with_capacity(modes.len());
    let mut last_time = 0.0_f64;
    for mode_name in &modes {
        let mut times = Vec::new();
        let mut stresses = Vec::new();
        let mut indicators = Vec::new();
        for row in 0..df.n_rows() {
            let Some(value) = time_column.value(row) else {
                continue;
            };
            let Some(stress_value) = stress_column.value(row) else {
                continue;
            };
            let event = match &censor_column {
                Some(column) => column.value(row).is_some_and(|value| value != censor_value),
                None => true,
            };
            let indicator = if event && mode_column.label(row).as_deref() == Some(mode_name) {
                0.0
            } else {
                1.0
            };
            if indicator == 0.0 {
                last_time = last_time.max(value);
            }
            times.push(value);
            stresses.push(stress_value);
            indicators.push(indicator);
        }
        let frame = DataFrame::new(vec![
            Column::new_f64("time", times),
            Column::new_f64("stress", stresses),
            Column::new_f64("censor", indicators),
        ])?;
        models.push(fit_with(
            &frame,
            "time",
            "stress",
            Some("censor"),
            1.0,
            use_stress,
            life,
            acceleration,
        )?);
    }
    let horizon = horizon.unwrap_or(last_time);
    if !horizon.is_finite() || horizon <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "competing-risks horizon",
            value: horizon,
        });
    }
    let survival = combined_survival(&models, horizon);
    let incidence = (0..models.len())
        .map(|index| cumulative_incidence(&models, index, horizon))
        .collect();
    let b10 = (0..models.len())
        .map(|index| b10_life(&models, index))
        .collect();
    Ok(CompetingRisks {
        modes,
        models,
        horizon,
        survival,
        incidence,
        b10,
    })
}

/// Fits and reports the competing-risks accelerated-life model.
///
/// # Errors
///
/// As [`competing_risks`].
#[allow(clippy::too_many_arguments)]
pub fn analyze_competing(
    df: &DataFrame,
    time: &str,
    stress: &str,
    censor: Option<&str>,
    censor_value: f64,
    mode: &str,
    use_stress: f64,
    life: LifeModel,
    acceleration: AccelerationModel,
    horizon: Option<f64>,
) -> Result<AnalysisOutput> {
    let fit = competing_risks(
        df,
        time,
        stress,
        censor,
        censor_value,
        mode,
        use_stress,
        life,
        acceleration,
        horizon,
    )?;
    let mut result = AnalysisResult::new(
        format!(
            "Accelerated Life Testing — competing risks, {} mode(s)",
            fit.modes.len()
        ),
        format!(
            "Cause-specific {} / {} fits; incidence at {:.6}",
            acceleration.name(),
            life.name(),
            fit.horizon
        ),
    );
    result.n = fit.models.iter().map(|model| model.events).sum();
    result.statistics = vec![
        ("Modes", fit.modes.len() as f64),
        ("Failures", result.n as f64),
        ("Horizon", fit.horizon),
        ("Survival at horizon", fit.survival),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Mode".to_owned(),
            "Failures".to_owned(),
            "Shape/Sigma".to_owned(),
            "Life at use".to_owned(),
            "B10".to_owned(),
            "CIF at horizon".to_owned(),
        ],
        rows: fit
            .modes
            .iter()
            .zip(&fit.models)
            .zip(&fit.incidence)
            .zip(&fit.b10)
            .map(|(((mode, model), incidence), b10)| {
                vec![
                    mode.clone(),
                    model.events.to_string(),
                    model
                        .shape
                        .map_or_else(|| "—".to_owned(), |value| format!("{value:.6}")),
                    format!("{:.6}", model.life_at_use),
                    b10.map_or_else(|| "—".to_owned(), |value| format!("{value:.6}")),
                    format!("{incidence:.6}"),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Cause-specific fits".to_owned(),
            "a failure of one mode is an event for its own model and censored for the others;              censored observations are censored for every model"
                .to_owned(),
        ),
        (
            "Incidence".to_owned(),
            "F_k(t) = ∫₀ᵗ S(s)·h_k(s) ds with S = Π S_k, integrated on a log grid"
                .to_owned(),
        ),
        (
            "B10".to_owned(),
            "first time at which the cumulative incidence of that cause reaches 0.1".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, 0, 0);
    result.warnings.push(
        "the competing-risks conventions are standard choices and have not been verified against          captured output"
            .to_owned(),
    );
    let panels: Vec<PlotSpec> = fit.models.iter().map(|model| plot(model, time)).collect();
    let plot = panel_plot(
        panels,
        2,
        "Accelerated Life Testing — competing risks".to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot,
    })
}

/// One unit of a step-stress test: its stress steps and whether it failed.
#[derive(Debug, Clone, PartialEq)]
pub struct StepStressUnit {
    /// `(stress, duration)` pairs, in test order.
    pub steps: Vec<(f64, f64)>,
    /// `true` when the last step ended in a failure, `false` for censoring.
    pub event: bool,
}

/// Fitted step-stress accelerated-life model.
#[derive(Debug, Clone, PartialEq)]
pub struct StepStressFit {
    /// Life distribution (Weibull or exponential).
    pub life: LifeModel,
    /// Acceleration relationship.
    pub acceleration: AccelerationModel,
    /// Weibull shape; `None` for the exponential.
    pub shape: Option<f64>,
    /// Log characteristic life at the centre of the design.
    pub intercept: f64,
    /// Acceleration slope.
    pub slope: f64,
    /// Centre of the transformed stress.
    pub centre: f64,
    /// Use stress of the extrapolation.
    pub use_stress: f64,
    /// Characteristic life at the use stress.
    pub life_at_use: f64,
    /// B10 life at the use stress.
    pub b10_at_use: f64,
    /// Life at the use stress over the life at the highest tested stress.
    pub acceleration_factor: f64,
    /// Units that failed.
    pub events: usize,
    /// Units censored at the end of their last step.
    pub censored: usize,
    /// Steps used.
    pub steps: usize,
    /// Rows excluded for a missing value.
    pub excluded: usize,
    /// Log-likelihood at the maximum.
    pub log_likelihood: f64,
    /// Akaike information criterion.
    pub aic: f64,
    /// Whether the simplex converged.
    pub converged: bool,
    /// Units, for the report table.
    pub units: Vec<StepStressUnit>,
}

fn step_characteristic(
    intercept: f64,
    slope: f64,
    centre: f64,
    acceleration: AccelerationModel,
    stress: f64,
) -> f64 {
    (intercept + slope * (acceleration.transform(stress) - centre) + acceleration.offset(stress))
        .exp()
}

/// Log-likelihood of the cumulative-exposure step-stress model.
///
/// The exposure of a unit is `Σ dᵢ/η(Sᵢ)`; the last step carries the event
/// indicator. `shape` is the Weibull shape (ignored for the exponential).
#[must_use]
pub fn step_stress_log_likelihood(
    units: &[StepStressUnit],
    life: LifeModel,
    acceleration: AccelerationModel,
    intercept: f64,
    slope: f64,
    centre: f64,
    shape: f64,
) -> f64 {
    let kappa = if life == LifeModel::Exponential {
        1.0
    } else {
        shape
    };
    if !kappa.is_finite() || kappa <= 0.0 {
        return f64::NEG_INFINITY;
    }
    let mut total = 0.0;
    for unit in units {
        let mut exposure = 0.0;
        let mut last_stress = 0.0;
        for &(stress, duration) in &unit.steps {
            let eta = step_characteristic(intercept, slope, centre, acceleration, stress);
            if !eta.is_finite() || eta <= 0.0 || duration <= 0.0 {
                return f64::NEG_INFINITY;
            }
            exposure += duration / eta;
            last_stress = stress;
        }
        if !exposure.is_finite() || exposure <= 0.0 {
            return f64::NEG_INFINITY;
        }
        let eta_last = step_characteristic(intercept, slope, centre, acceleration, last_stress);
        if unit.event {
            total +=
                kappa.ln() + (kappa - 1.0) * exposure.ln() - exposure.powf(kappa) - eta_last.ln();
        } else {
            total += -exposure.powf(kappa);
        }
    }
    total
}

fn step_stress_units(
    df: &DataFrame,
    duration: &str,
    stress: &str,
    unit: &str,
    event: &str,
) -> Result<(Vec<StepStressUnit>, usize)> {
    let duration_column = df.column(duration)?;
    let stress_column = df.column(stress)?;
    let unit_column = df.column(unit)?;
    let event_column = df.column(event)?;
    let mut order: Vec<String> = Vec::new();
    let mut buckets: std::collections::BTreeMap<String, Vec<(f64, f64, bool)>> =
        std::collections::BTreeMap::new();
    let mut used = 0usize;
    for row in 0..df.n_rows() {
        let (Some(time), Some(level), Some(label)) = (
            duration_column.value(row),
            stress_column.value(row),
            unit_column.label(row),
        ) else {
            continue;
        };
        let event = event_column.value(row).unwrap_or(0.0);
        if !time.is_finite() || time <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "step duration",
                value: time,
            });
        }
        if !level.is_finite() || level <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "step stress",
                value: level,
            });
        }
        if event != 0.0 && event != 1.0 {
            return Err(Error::Unsupported(
                "the event column must contain 0 or 1".to_owned(),
            ));
        }
        if !buckets.contains_key(&label) {
            order.push(label.clone());
        }
        buckets
            .entry(label)
            .or_default()
            .push((level, time, event == 1.0));
        used += 1;
    }
    let mut units = Vec::with_capacity(order.len());
    for label in order {
        let rows = buckets.remove(&label).unwrap_or_default();
        if rows.is_empty() {
            continue;
        }
        let event_rows: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, (_, _, event))| *event)
            .map(|(index, _)| index)
            .collect();
        if event_rows.len() > 1 {
            return Err(Error::Unsupported(format!(
                "unit {label} has more than one failure step"
            )));
        }
        if event_rows
            .first()
            .is_some_and(|index| *index + 1 != rows.len())
        {
            return Err(Error::Unsupported(format!(
                "a failure must be the last step of unit {label}"
            )));
        }
        units.push(StepStressUnit {
            event: !event_rows.is_empty(),
            steps: rows
                .into_iter()
                .map(|(stress, duration, _)| (stress, duration))
                .collect(),
        });
    }
    if units.len() < 2 {
        return Err(Error::Unsupported(
            "step-stress testing needs at least two units".to_owned(),
        ));
    }
    Ok((units, df.n_rows().saturating_sub(used)))
}

/// Fits the cumulative-exposure step-stress model.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for the lognormal life model, a failure that
/// is not the last step of its unit, fewer than two units or no failures, and
/// [`Error::NotConverged`] when the simplex cannot find a finite likelihood.
#[allow(clippy::too_many_arguments)]
pub fn step_stress_fit(
    df: &DataFrame,
    duration: &str,
    stress: &str,
    unit: &str,
    event: &str,
    use_stress: f64,
    life: LifeModel,
    acceleration: AccelerationModel,
) -> Result<StepStressFit> {
    if life == LifeModel::Lognormal {
        return Err(Error::Unsupported(
            "step-stress testing is implemented for the Weibull and exponential life models"
                .to_owned(),
        ));
    }
    if !use_stress.is_finite() || use_stress <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "use stress",
            value: use_stress,
        });
    }
    let (units, excluded) = step_stress_units(df, duration, stress, unit, event)?;
    let events = units.iter().filter(|unit| unit.event).count();
    if events == 0 {
        return Err(Error::Unsupported(
            "step-stress testing needs at least one failure".to_owned(),
        ));
    }
    let steps: usize = units.iter().map(|unit| unit.steps.len()).sum();
    let centre = units
        .iter()
        .flat_map(|unit| &unit.steps)
        .map(|(stress, _)| acceleration.transform(*stress))
        .sum::<f64>()
        / steps as f64;
    let mean_total = units
        .iter()
        .map(|unit| unit.steps.iter().map(|(_, duration)| duration).sum::<f64>())
        .sum::<f64>()
        / units.len() as f64;
    let intercept_start = mean_total.ln();
    let dimension = if life == LifeModel::Exponential { 2 } else { 3 };
    let objective = |parameters: &[f64]| -> f64 {
        let (shape, intercept, slope) = if dimension == 3 {
            (parameters[0].exp(), parameters[1], parameters[2])
        } else {
            (1.0, parameters[0], parameters[1])
        };
        let likelihood =
            step_stress_log_likelihood(&units, life, acceleration, intercept, slope, centre, shape);
        if likelihood.is_finite() {
            -likelihood
        } else {
            f64::INFINITY
        }
    };
    let mut starts = Vec::new();
    for slope in [-0.5, 0.0, 0.5] {
        if dimension == 3 {
            for shape in [0.5_f64, 1.0, 2.0] {
                starts.push(vec![shape.ln(), intercept_start, slope]);
            }
        } else {
            starts.push(vec![intercept_start, slope]);
        }
    }
    let mut best: Option<(f64, Vec<f64>)> = None;
    for start in starts {
        let steps_vector = vec![0.5; dimension];
        let Ok(minimum) = nelder_mead(&start, &steps_vector, 3000, 1e-10, objective) else {
            continue;
        };
        if minimum.value.is_finite()
            && best
                .as_ref()
                .is_none_or(|(value, _)| minimum.value < *value)
        {
            best = Some((minimum.value, minimum.arguments));
        }
    }
    let Some((negative, parameters)) = best else {
        return Err(Error::NotConverged {
            detail: "step-stress maximum likelihood".to_owned(),
        });
    };
    let (shape, intercept, slope) = if dimension == 3 {
        (parameters[0].exp(), parameters[1], parameters[2])
    } else {
        (1.0, parameters[0], parameters[1])
    };
    let life_at_use = step_characteristic(intercept, slope, centre, acceleration, use_stress);
    if !life_at_use.is_finite() || life_at_use <= 0.0 {
        return Err(Error::NotConverged {
            detail: "step-stress use-stress life".to_owned(),
        });
    }
    let highest = units
        .iter()
        .flat_map(|unit| &unit.steps)
        .map(|(stress, _)| *stress)
        .fold(f64::NEG_INFINITY, f64::max);
    let life_at_highest = step_characteristic(intercept, slope, centre, acceleration, highest);
    let b10_at_use = life_at_use * (-0.9_f64.ln()).powf(1.0 / shape);
    Ok(StepStressFit {
        life,
        acceleration,
        shape: (life == LifeModel::Weibull).then_some(shape),
        intercept,
        slope,
        centre,
        use_stress,
        life_at_use,
        b10_at_use,
        acceleration_factor: life_at_use / life_at_highest,
        events,
        censored: units.len() - events,
        steps,
        excluded,
        log_likelihood: -negative,
        aic: 2.0 * dimension as f64 + 2.0 * negative,
        converged: true,
        units,
    })
}

/// Fits and reports the step-stress model.
///
/// # Errors
///
/// As [`step_stress_fit`].
#[allow(clippy::too_many_arguments)]
pub fn step_stress_analyze(
    df: &DataFrame,
    duration: &str,
    stress: &str,
    unit: &str,
    event: &str,
    use_stress: f64,
    life: LifeModel,
    acceleration: AccelerationModel,
) -> Result<AnalysisOutput> {
    let fit = step_stress_fit(
        df,
        duration,
        stress,
        unit,
        event,
        use_stress,
        life,
        acceleration,
    )?;
    let mut result = AnalysisResult::new(
        format!(
            "Step-Stress ALT — {} / {}",
            acceleration.name(),
            life.name()
        ),
        format!(
            "Cumulative-exposure model on {duration}, stress in {stress}, extrapolated to {use_stress}"
        ),
    );
    result.n = fit.events + fit.censored;
    result.excluded = fit.excluded;
    result.statistics = vec![
        ("Units", result.n as f64),
        ("Steps", fit.steps as f64),
        ("Failures", fit.events as f64),
        ("Censored", fit.censored as f64),
        ("Intercept", fit.intercept),
        ("Acceleration slope", fit.slope),
        ("Use stress", fit.use_stress),
        ("Life at use", fit.life_at_use),
        ("B10 at use", fit.b10_at_use),
        ("Acceleration factor", fit.acceleration_factor),
        ("Log-likelihood", fit.log_likelihood),
        ("AIC", fit.aic),
    ];
    if let Some(shape) = fit.shape {
        result.statistics.push(("Shape (Weibull)", shape));
    }
    result.extra.push(Item::Table {
        headers: vec![
            "Unit".to_owned(),
            "Steps".to_owned(),
            "Total time".to_owned(),
            "Last stress".to_owned(),
            "Event".to_owned(),
        ],
        rows: fit
            .units
            .iter()
            .enumerate()
            .take(20)
            .map(|(index, unit)| {
                let total: f64 = unit.steps.iter().map(|(_, duration)| duration).sum();
                let last = unit.steps.last().map_or(0.0, |(stress, _)| *stress);
                vec![
                    (index + 1).to_string(),
                    unit.steps.len().to_string(),
                    format!("{total:.6}"),
                    format!("{last:.6}"),
                    if unit.event { "failed" } else { "censored" }.to_owned(),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Model".to_owned(),
            "cumulative exposure: a unit accumulates Σ dᵢ/η(Sᵢ); the last step carries the event"
                .to_owned(),
        ),
        (
            "Life".to_owned(),
            "Weibull (shared shape) or exponential; the lognormal step-stress model is not implemented"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "one row per step, the failure row last in its unit; missing rows are excluded"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the step-stress cumulative-exposure conventions are standard choices and have not been          verified against captured output"
            .to_owned(),
    );
    let plot = step_stress_plot(&fit, stress);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn step_stress_plot(fit: &StepStressFit, stress: &str) -> PlotSpec {
    let low = fit
        .units
        .iter()
        .flat_map(|unit| &unit.steps)
        .map(|(stress, _)| *stress)
        .fold(f64::INFINITY, f64::min)
        .min(fit.use_stress);
    let high = fit
        .units
        .iter()
        .flat_map(|unit| &unit.steps)
        .map(|(stress, _)| *stress)
        .fold(f64::NEG_INFINITY, f64::max)
        .max(fit.use_stress);
    let pad = ((high - low).abs() * 0.05).max(1e-9);
    let at = |stress: f64| {
        step_characteristic(
            fit.intercept,
            fit.slope,
            fit.centre,
            fit.acceleration,
            stress,
        )
        .ln()
    };
    let y_low = at(low - pad).min(at(high + pad));
    let y_high = at(low - pad).max(at(high + pad));
    let line = Segment {
        from: Point {
            x: low - pad,
            y: at(low - pad),
        },
        to: Point {
            x: high + pad,
            y: at(high + pad),
        },
        width: 2.0,
    };
    let mut points = vec![Point {
        x: fit.use_stress,
        y: at(fit.use_stress),
    }];
    for unit in &fit.units {
        for (stress, _) in &unit.steps {
            points.push(Point {
                x: *stress,
                y: at(*stress),
            });
        }
    }
    points.sort_by(|a, b| a.x.total_cmp(&b.x));
    points.dedup_by(|a, b| (a.x - b.x).abs() < 1e-12);
    PlotSpec {
        title: "Step-Stress ALT — life versus stress".to_owned(),
        x_axis: linear_axis(stress, low - pad, high + pad),
        y_axis: linear_axis("ln(characteristic life)", y_low - 0.5, y_high + 0.5),
        size: (900, 540),
        points,
        lines: vec![line],
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Column, MissingPolicy};
    use crate::special::SplitMix64;

    /// Draws Weibull times with the Arrhenius characteristic life.
    fn synthetic(shape: f64, intercept: f64, slope: f64, centre: f64) -> DataFrame {
        let levels = [300.0, 350.0, 400.0, 450.0];
        let mut rng = SplitMix64::new(7);
        let mut times = Vec::new();
        let mut stresses = Vec::new();
        for level in levels {
            let eta = (intercept + slope * (1.0 / level - centre)).exp();
            for _ in 0..12 {
                let u = rng.next_f64().clamp(1e-6, 1.0 - 1e-6);
                times.push(eta * (-(1.0 - u).ln()).powf(1.0 / shape));
                stresses.push(level);
            }
        }
        DataFrame::new(vec![
            Column::new_f64("time", times),
            Column::new_f64("stress", stresses),
        ])
        .expect("frame")
    }

    #[test]
    fn the_fit_recovers_a_synthetic_arrhenius_model() {
        let centre = [300.0, 350.0, 400.0, 450.0]
            .iter()
            .map(|level| 1.0 / level)
            .sum::<f64>()
            / 4.0;
        let shape = 2.0;
        let intercept = 8.0;
        let slope = 1500.0;
        let df = synthetic(shape, intercept, slope, centre);
        let model = fit(&df, "time", "stress", None, 1.0, 300.0).expect("fit");
        assert_eq!(model.events, 48);
        assert_eq!(model.censored, 0);
        let fitted_shape = model.shape.expect("shape");
        assert!((fitted_shape - shape).abs() < 0.45, "shape {fitted_shape}");
        assert!(
            (model.intercept - intercept).abs() < 0.6,
            "intercept {}",
            model.intercept
        );
        assert!(
            (model.slope / slope - 1.0).abs() < 0.25,
            "slope {}",
            model.slope
        );
        // The life at the lowest stress is the longest one.
        let at_300 = (model.intercept + model.slope * (1.0 / 300.0 - model.centre)).exp();
        let at_450 = (model.intercept + model.slope * (1.0 / 450.0 - model.centre)).exp();
        assert!(at_300 > at_450);
        assert!(model.acceleration_factor > 1.0);
        assert!(model.b10_at_use < model.life_at_use);
    }

    #[test]
    fn censored_units_are_used() {
        let df = synthetic(2.0, 8.0, 1500.0, 0.002_792);
        let times = df
            .column("time")
            .expect("time")
            .numeric(MissingPolicy::Drop)
            .expect("times");
        // Censor everything above the fortieth percentile.
        let mut sorted = times.clone();
        sorted.sort_by(f64::total_cmp);
        let limit = sorted[sorted.len() * 2 / 5];
        let flags: Vec<f64> = times
            .iter()
            .map(|value| if *value > limit { 1.0 } else { 0.0 })
            .collect();
        let with_flags = DataFrame::new(vec![
            Column::new_f64("time", times),
            Column::new_f64(
                "stress",
                df.column("stress")
                    .expect("stress")
                    .numeric(MissingPolicy::Drop)
                    .expect("levels"),
            ),
            Column::new_f64("censored", flags),
        ])
        .expect("frame");
        let model = fit(&with_flags, "time", "stress", Some("censored"), 1.0, 300.0).expect("fit");
        assert!(model.censored > 0, "{model:?}");
        assert_eq!(model.events + model.censored, 48);
        let fitted_shape = model.shape.expect("shape");
        assert!(
            fitted_shape > 0.5 && fitted_shape < 6.0,
            "shape {fitted_shape}"
        );
    }

    #[test]
    fn the_other_life_models_and_laws_fit() {
        let df = synthetic(2.0, 8.0, 1500.0, 0.002_792);
        for life in [LifeModel::Lognormal, LifeModel::Exponential] {
            let model = fit_with(
                &df,
                "time",
                "stress",
                None,
                1.0,
                300.0,
                life,
                AccelerationModel::Arrhenius,
            )
            .expect("fit");
            assert_eq!(model.life, life);
            assert!(model.life_at_use > 0.0);
            assert!(model.b10_at_use > 0.0);
            assert!(model.b10_at_use < model.life_at_use, "{life:?}");
            match life {
                LifeModel::Exponential => assert!(model.shape.is_none()),
                _ => assert!(model.shape.is_some()),
            }
        }
        // The inverse power law uses ln S; a synthetic power-law sample must
        // still fit and extrapolate.
        let levels: [f64; 4] = [1.0, 2.0, 4.0, 8.0];
        let mut times = Vec::new();
        let mut stresses = Vec::new();
        let mut rng = SplitMix64::new(11);
        for level in levels {
            let eta = (2.0 - 0.8 * level.ln()).exp();
            for _ in 0..10 {
                let u = rng.next_f64().clamp(1e-6, 1.0 - 1e-6);
                times.push(eta * (-(1.0 - u).ln()).powf(1.0 / 2.5));
                stresses.push(level);
            }
        }
        let power = DataFrame::new(vec![
            Column::new_f64("time", times),
            Column::new_f64("stress", stresses),
        ])
        .expect("frame");
        let model = fit_with(
            &power,
            "time",
            "stress",
            None,
            1.0,
            1.0,
            LifeModel::Weibull,
            AccelerationModel::InversePower,
        )
        .expect("fit");
        assert_eq!(model.acceleration, AccelerationModel::InversePower);
        // The slope is the power of the inverse power law (negative here).
        assert!(model.slope < 0.0, "slope {}", model.slope);
        assert!(model.acceleration_factor > 1.0);
    }

    #[test]
    fn the_eyring_law_uses_the_log_stress_term() {
        // Synthetic data from the Eyring law: ln η = a + b/S − ln S.
        let levels: [f64; 4] = [300.0, 350.0, 400.0, 450.0];
        let (a, b, shape) = (6.0, 1500.0, 2.0);
        let mut rng = SplitMix64::new(3);
        let mut times = Vec::new();
        let mut stresses = Vec::new();
        for level in levels {
            let eta = (a + b / level - level.ln()).exp();
            for _ in 0..12 {
                let u = rng.next_f64().clamp(1e-6, 1.0 - 1e-6);
                times.push(eta * (-(1.0 - u).ln()).powf(1.0 / shape));
                stresses.push(level);
            }
        }
        let df = DataFrame::new(vec![
            Column::new_f64("time", times),
            Column::new_f64("stress", stresses),
        ])
        .expect("frame");
        let model = fit_with(
            &df,
            "time",
            "stress",
            None,
            1.0,
            300.0,
            LifeModel::Weibull,
            AccelerationModel::Eyring,
        )
        .expect("fit");
        assert_eq!(model.acceleration, AccelerationModel::Eyring);
        // The characteristic life at the lowest stress is the longest one and
        // the offset keeps the extrapolation positive.
        let at_300 = (model.intercept
            + model.slope * (model.acceleration.transform(300.0) - model.centre)
            + model.acceleration.offset(300.0))
        .exp();
        let at_450 = (model.intercept
            + model.slope * (model.acceleration.transform(450.0) - model.centre)
            + model.acceleration.offset(450.0))
        .exp();
        assert!(at_300 > at_450);
        assert!(model.life_at_use > 0.0);
        assert!(model.b10_at_use > 0.0 && model.b10_at_use < model.life_at_use);
        // The offset of the other laws is zero.
        assert_eq!(AccelerationModel::Arrhenius.offset(350.0), 0.0);
        assert!(AccelerationModel::Eyring.offset(350.0) < 0.0);
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let df = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_f64("stress", vec![300.0, 300.0, 300.0, 300.0]),
        ])
        .expect("frame");
        // One stress level only.
        assert!(fit(&df, "time", "stress", None, 1.0, 300.0).is_err());
        // A non-positive use stress.
        let df = synthetic(2.0, 8.0, 1500.0, 0.002_792);
        assert!(matches!(
            fit(&df, "time", "stress", None, 1.0, 0.0),
            Err(Error::InvalidParameter { .. })
        ));
    }

    #[test]
    fn the_report_and_the_plot_are_built() {
        let df = synthetic(2.0, 8.0, 1500.0, 0.002_792);
        let out = analyze(&df, "time", "stress", None, 1.0, 300.0).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Accelerated Life"), "{text}");
        assert!(text.contains("B10 at use"), "{text}");
        assert!(text.contains("Acceleration factor"), "{text}");
        let plot = out.plot.expect("plot");
        assert!(!plot.points.is_empty());
        assert_eq!(plot.lines.len(), 1);
    }

    #[test]
    fn a_missing_row_does_not_shift_the_columns() {
        let df = synthetic(2.0, 8.0, 1500.0, 0.002_792);
        let times = df.column("time").expect("time").numeric_by_row();
        let stresses = df.column("stress").expect("stress").numeric_by_row();
        let mut with_missing_times = vec![f64::NAN];
        with_missing_times.extend(&times);
        let mut with_missing_stresses = vec![999.0];
        with_missing_stresses.extend(&stresses);
        let padded = DataFrame::new(vec![
            Column::new_f64("time", with_missing_times),
            Column::new_f64("stress", with_missing_stresses),
        ])
        .expect("frame");
        let clean = DataFrame::new(vec![
            Column::new_f64("time", times),
            Column::new_f64("stress", stresses),
        ])
        .expect("frame");
        let a = fit(&padded, "time", "stress", None, 1.0, 300.0).expect("alt");
        let b = fit(&clean, "time", "stress", None, 1.0, 300.0).expect("alt");
        assert_eq!(a.events, b.events);
        assert!((a.life_at_use - b.life_at_use).abs() < 1e-9);
        assert!((a.acceleration_factor - b.acceleration_factor).abs() < 1e-9);
    }
}

#[cfg(test)]
mod competing_tests {
    use super::*;

    fn model(shape: f64, life_at_use: f64) -> AltFit {
        AltFit {
            life: LifeModel::Weibull,
            acceleration: AccelerationModel::Arrhenius,
            shape: Some(shape),
            shape_se: Some(0.05),
            intercept: life_at_use.ln(),
            intercept_se: 0.01,
            slope: 0.0,
            slope_se: 0.01,
            centre: 1.0 / 300.0,
            stresses: vec![300.0],
            use_stress: 300.0,
            life_at_use,
            b10_at_use: life_at_use * 0.1_f64.powf(1.0 / shape),
            acceleration_factor: 1.0,
            events: 100,
            censored: 0,
            log_likelihood: -100.0,
            aic: 206.0,
            converged: true,
            observed: Vec::new(),
        }
    }

    fn analytic_incidence(models: &[AltFit], index: usize, time: f64) -> f64 {
        // Independent uniform-grid Simpson in s (not the log grid of the
        // implementation).
        let steps = 200_000;
        let width = time / steps as f64;
        let integrand = |s: f64| {
            let survival = models
                .iter()
                .map(|model| survival_at(model, s))
                .product::<f64>();
            survival * hazard_at(&models[index], s)
        };
        let mut total = integrand(0.0) + integrand(time);
        for step in 1..steps {
            total += integrand(width * step as f64) * if step % 2 == 0 { 2.0 } else { 4.0 };
        }
        total * width / 3.0
    }

    #[test]
    fn the_cif_matches_an_independent_integral() {
        let models = vec![model(2.0, 100.0), model(1.5, 200.0)];
        for time in [10.0, 50.0, 120.0] {
            let survival: f64 = models
                .iter()
                .map(|model| survival_at(model, time))
                .product();
            let expected: f64 = models
                .iter()
                .map(|model| (-(time / model.life_at_use).powf(model.shape.unwrap())).exp())
                .product();
            assert!((survival - expected).abs() < 1e-12);
            for index in 0..models.len() {
                let actual = cumulative_incidence(&models, index, time);
                let reference = analytic_incidence(&models, index, time);
                assert!(
                    (actual - reference).abs() < 1e-9,
                    "cause {index} at {time}: {actual} vs {reference}"
                );
            }
        }
    }

    #[test]
    fn the_b10_is_the_first_ten_percent_of_the_cif() {
        let models = vec![model(2.0, 100.0), model(1.5, 200.0)];
        let b10 = b10_life(&models, 0).expect("B10");
        assert!((cumulative_incidence(&models, 0, b10) - 0.1).abs() < 1e-6);
    }

    fn simulated() -> DataFrame {
        // Two stress levels, two causes, deterministic splitmix draws.
        let mut generator = crate::special::SplitMix64::new(20261029);
        let mut times = Vec::new();
        let mut stresses = Vec::new();
        let mut modes = Vec::new();
        let mut censor = Vec::new();
        for _ in 0..600 {
            let stress: f64 = if generator.next_f64() < 0.5 {
                300.0
            } else {
                350.0
            };
            let life = (7.0 - 0.02 * stress).exp();
            let first = life * (-(1.0 - generator.next_f64()).ln()).powf(1.0 / 2.0);
            let second = 1.6 * life * (-(1.0 - generator.next_f64()).ln()).powf(1.0 / 1.5);
            if generator.next_f64() < 0.1 {
                times.push(first.min(second) * 0.8);
                stresses.push(stress);
                modes.push(if first < second { "A" } else { "B" });
                censor.push(1.0);
            } else if first < second {
                times.push(first);
                stresses.push(stress);
                modes.push("A");
                censor.push(0.0);
            } else {
                times.push(second);
                stresses.push(stress);
                modes.push("B");
                censor.push(0.0);
            }
        }
        DataFrame::new(vec![
            Column::new_f64("time", times),
            Column::new_f64("stress", stresses),
            Column::new_text("mode", modes.into_iter().map(str::to_owned).collect()),
            Column::new_f64("censor", censor),
        ])
        .expect("frame")
    }

    #[test]
    fn the_competing_risks_fit_covers_both_modes() {
        let fit = competing_risks(
            &simulated(),
            "time",
            "stress",
            Some("censor"),
            1.0,
            "mode",
            300.0,
            LifeModel::Weibull,
            AccelerationModel::Arrhenius,
            None,
        )
        .expect("competing risks");
        assert_eq!(fit.modes.len(), 2);
        assert!(fit.survival > 0.0 && fit.survival < 1.0);
        assert_eq!(fit.incidence.len(), 2);
        assert!(
            fit.incidence
                .iter()
                .all(|value| (0.0..=1.0).contains(value))
        );
        assert!(fit.incidence.iter().sum::<f64>() <= 1.0 + 1e-9);
        assert!(fit.b10.iter().all(Option::is_some));
    }

    #[test]
    fn invalid_competing_risks_are_rejected() {
        let df = simulated();
        assert!(
            competing_risks(
                &df,
                "time",
                "stress",
                Some("censor"),
                1.0,
                "mode",
                300.0,
                LifeModel::Weibull,
                AccelerationModel::Arrhenius,
                Some(-1.0),
            )
            .is_err()
        );
        let single = DataFrame::new(vec![
            Column::new_f64("time", vec![10.0, 20.0]),
            Column::new_f64("stress", vec![300.0, 300.0]),
            Column::new_text("mode", vec!["A".to_owned(), "A".to_owned()]),
        ])
        .expect("frame");
        assert!(
            competing_risks(
                &single,
                "time",
                "stress",
                None,
                1.0,
                "mode",
                300.0,
                LifeModel::Weibull,
                AccelerationModel::Arrhenius,
                None,
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod step_stress_tests {
    use super::*;

    #[test]
    fn the_log_likelihood_matches_the_hand_calculation() {
        let units = vec![StepStressUnit {
            steps: vec![(300.0, 10.0), (350.0, 5.0)],
            event: true,
        }];
        let intercept = 100.0_f64.ln();
        let centre = 1.0 / 325.0;
        let likelihood = step_stress_log_likelihood(
            &units,
            LifeModel::Weibull,
            AccelerationModel::Arrhenius,
            intercept,
            0.0,
            centre,
            2.0,
        );
        let exposure = 0.15_f64;
        let expected = 2.0_f64.ln() + exposure.ln() - exposure.powi(2) - 100.0_f64.ln();
        assert!(
            (likelihood - expected).abs() < 1e-12,
            "{likelihood} vs {expected}"
        );
    }

    fn simulated() -> DataFrame {
        let mut generator = crate::special::SplitMix64::new(20261029);
        let shape = 1.5_f64;
        let intercept = 200.0_f64.ln();
        let slope = 3000.0;
        let centre = 1.0 / 325.0;
        let eta = |stress: f64| (intercept + slope * (1.0 / stress - centre)).exp();
        let eta1 = eta(300.0);
        let eta2 = eta(350.0);
        let mut unit = Vec::new();
        let mut duration = Vec::new();
        let mut stress = Vec::new();
        let mut event = Vec::new();
        for index in 0..500 {
            let u = generator.next_f64().clamp(1e-12, 1.0 - 1e-12);
            let exposure = (-(1.0 - u).ln()).powf(1.0 / shape);
            let first = 20.0 / eta1;
            let label = format!("U{index}");
            if exposure <= first {
                unit.push(label);
                duration.push(exposure * eta1);
                stress.push(300.0);
                event.push(1.0);
            } else {
                let second = (exposure - first) * eta2;
                unit.push(label.clone());
                duration.push(20.0);
                stress.push(300.0);
                event.push(0.0);
                unit.push(label);
                stress.push(350.0);
                if second <= 40.0 {
                    duration.push(second);
                    event.push(1.0);
                } else {
                    duration.push(40.0);
                    event.push(0.0);
                }
            }
        }
        DataFrame::new(vec![
            Column::new_text("unit", unit),
            Column::new_f64("duration", duration),
            Column::new_f64("stress", stress),
            Column::new_f64("event", event),
        ])
        .expect("frame")
    }

    #[test]
    fn the_step_stress_fit_recovers_the_truth() {
        let df = simulated();
        let fit = step_stress_fit(
            &df,
            "duration",
            "stress",
            "unit",
            "event",
            300.0,
            LifeModel::Weibull,
            AccelerationModel::Arrhenius,
        )
        .expect("fit");
        let shape = fit.shape.expect("shape");
        assert!((shape - 1.5).abs() < 0.2, "shape {shape}");
        assert!((fit.slope - 3000.0).abs() < 300.0, "slope {}", fit.slope);
        assert!(fit.events > 0 && fit.censored > 0);
        assert!(fit.log_likelihood.is_finite());
        // The maximum is at least as likely as the true parameters.
        let truth_intercept = 200.0_f64.ln() + 3000.0 * (1.0 / 325.0 - fit.centre);
        let truth = step_stress_log_likelihood(
            &fit.units,
            LifeModel::Weibull,
            AccelerationModel::Arrhenius,
            truth_intercept,
            3000.0,
            fit.centre,
            1.5,
        );
        assert!(
            fit.log_likelihood >= truth - 1e-6,
            "{} vs {truth}",
            fit.log_likelihood
        );
        assert!(fit.b10_at_use < fit.life_at_use);
    }

    #[test]
    fn the_exponential_step_stress_fit_runs() {
        let df = simulated();
        let fit = step_stress_fit(
            &df,
            "duration",
            "stress",
            "unit",
            "event",
            300.0,
            LifeModel::Exponential,
            AccelerationModel::Arrhenius,
        )
        .expect("fit");
        assert!(fit.shape.is_none());
        assert!(fit.log_likelihood.is_finite());
    }

    #[test]
    fn invalid_step_stress_inputs_are_rejected() {
        let df = simulated();
        assert!(
            step_stress_fit(
                &df,
                "duration",
                "stress",
                "unit",
                "event",
                300.0,
                LifeModel::Lognormal,
                AccelerationModel::Arrhenius,
            )
            .is_err()
        );
        let bad = DataFrame::new(vec![
            Column::new_text("unit", vec!["U1".to_owned(), "U1".to_owned()]),
            Column::new_f64("duration", vec![10.0, 5.0]),
            Column::new_f64("stress", vec![300.0, 350.0]),
            Column::new_f64("event", vec![1.0, 0.0]),
        ])
        .expect("frame");
        assert!(
            step_stress_fit(
                &bad,
                "duration",
                "stress",
                "unit",
                "event",
                300.0,
                LifeModel::Weibull,
                AccelerationModel::Arrhenius,
            )
            .is_err()
        );
    }
}
