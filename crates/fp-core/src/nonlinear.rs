// SPDX-License-Identifier: MIT
//! Nonlinear regression by least squares.
//!
//! The model `y = f(x; θ)` is fitted to the complete pairs of two columns by
//! **Levenberg–Marquardt**: at every iteration the normal equations
//! `(JᵀJ + λ·diag(JᵀJ)) δ = −Jᵀr` are solved with a numerical Jacobian
//! (central differences) and `λ` decreases when the residual sum of squares
//! falls and increases otherwise. The starting values are derived from the
//! range of the data, so the user only chooses the response, the predictor and
//! the model.
//!
//! The library contains Michaelis–Menten, asymptotic regression, a
//! three-parameter logistic, Gompertz, Weibull growth and a power curve. The
//! report gives the equation, the estimates with the **asymptotic standard
//! errors** `s²(JᵀJ)⁻¹` (`n − p` degrees of freedom), t, p, the confidence
//! interval of every parameter, R², the residual standard error, the number of
//! iterations and the convergence, and draws the fitted curve over the
//! observations.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, Series, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{student_t_cdf, student_t_quantile};
use crate::workflow::AnalysisOutput;

/// Model of the nonlinear regression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NonlinearModel {
    /// `y = V·x/(K + x)`; parameters `V` and `K`.
    #[default]
    MichaelisMenten,
    /// `y = a + b·exp(−c·x)`; parameters `Asymptote`, `Amplitude` and `Rate`.
    Asymptotic,
    /// `y = A/(1 + exp(−k(x − x₀)))`; parameters `Asymptote`, `Steepness`
    /// and `Midpoint`.
    Logistic,
    /// `y = A·exp(−exp(−k(x − x₀)))`; parameters `Asymptote`, `Rate` and
    /// `Inflection`.
    Gompertz,
    /// `y = A·(1 − exp(−(x/λ)^k))`; parameters `Asymptote`, `Scale` and
    /// `Shape`, for `x ≥ 0`.
    WeibullGrowth,
    /// `y = a·x^b`; parameters `Scale` and `Exponent`, for `x > 0`.
    Power,
}

impl NonlinearModel {
    /// Name used in the report and the dialog.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::MichaelisMenten => "Michaelis–Menten",
            Self::Asymptotic => "Asymptotic regression",
            Self::Logistic => "Logistic (3 parameters)",
            Self::Gompertz => "Gompertz",
            Self::WeibullGrowth => "Weibull growth",
            Self::Power => "Power",
        }
    }

    /// Equation of the model.
    #[must_use]
    pub fn equation(self) -> &'static str {
        match self {
            Self::MichaelisMenten => "y = V·x/(K + x)",
            Self::Asymptotic => "y = a + b·exp(−c·x)",
            Self::Logistic => "y = A/(1 + exp(−k·(x − x₀)))",
            Self::Gompertz => "y = A·exp(−exp(−k·(x − x₀)))",
            Self::WeibullGrowth => "y = A·(1 − exp(−(x/λ)^k))",
            Self::Power => "y = a·x^b",
        }
    }

    /// Names of the parameters.
    #[must_use]
    pub fn parameter_names(self) -> Vec<&'static str> {
        match self {
            Self::MichaelisMenten => vec!["V", "K"],
            Self::Asymptotic => vec!["Asymptote", "Amplitude", "Rate"],
            Self::Logistic => vec!["Asymptote", "Steepness", "Midpoint"],
            Self::Gompertz => vec!["Asymptote", "Rate", "Inflection"],
            Self::WeibullGrowth => vec!["Asymptote", "Scale", "Shape"],
            Self::Power => vec!["Scale", "Exponent"],
        }
    }

    /// Value of the model at `x`, or `NaN` outside its domain.
    #[must_use]
    pub fn evaluate(self, x: f64, p: &[f64]) -> f64 {
        if p.len() < self.parameter_count() {
            return f64::NAN;
        }
        match self {
            Self::MichaelisMenten => {
                let denominator = p[1] + x;
                if denominator.abs() < 1e-12 {
                    f64::NAN
                } else {
                    p[0] * x / denominator
                }
            }
            Self::Asymptotic => p[0] + p[1] * (-p[2] * x).exp(),
            Self::Logistic => p[0] / (1.0 + (-p[1] * (x - p[2])).exp()),
            Self::Gompertz => p[0] * (-(-p[1] * (x - p[2])).exp()).exp(),
            Self::WeibullGrowth => {
                if x < 0.0 || p[1] <= 0.0 || p[2] <= 0.0 {
                    return f64::NAN;
                }
                p[0] * (1.0 - (-(x / p[1]).powf(p[2])).exp())
            }
            Self::Power => {
                if x <= 0.0 {
                    return f64::NAN;
                }
                p[0] * x.powf(p[1])
            }
        }
    }

    /// Number of parameters.
    #[must_use]
    pub fn parameter_count(self) -> usize {
        self.parameter_names().len()
    }

    /// Whether the model needs strictly positive `x`.
    #[must_use]
    pub fn needs_positive_x(self) -> bool {
        matches!(self, Self::Power)
    }

    /// Whether the model needs non-negative `x`.
    #[must_use]
    pub fn needs_non_negative_x(self) -> bool {
        matches!(self, Self::WeibullGrowth)
    }

    /// Automatic starting values from the data range.
    #[must_use]
    pub fn start(self, x: &[f64], y: &[f64]) -> Vec<f64> {
        let x_min = x.iter().copied().fold(f64::INFINITY, f64::min);
        let x_max = x.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let y_max = y.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let y_min = y.iter().copied().fold(f64::INFINITY, f64::min);
        let range = (x_max - x_min).max(1e-9);
        let median = 0.5 * (x_min + x_max);
        match self {
            Self::MichaelisMenten => vec![y_max * 1.05, median.max(1e-6)],
            Self::Asymptotic => vec![y_max, y_min - y_max, 1.0 / range],
            Self::Logistic => vec![y_max, 4.0 / range, median],
            Self::Gompertz => vec![y_max, 2.0 / range, median],
            Self::WeibullGrowth => vec![y_max, median.max(1e-6), 1.0],
            Self::Power => {
                let mut sx = 0.0;
                let mut sy = 0.0;
                let mut sxx = 0.0;
                let mut sxy = 0.0;
                let mut count = 0usize;
                for (left, right) in x.iter().zip(y) {
                    if *left > 0.0 && *right > 0.0 {
                        let lx = left.ln();
                        let ly = right.ln();
                        sx += lx;
                        sy += ly;
                        sxx += lx * lx;
                        sxy += lx * ly;
                        count += 1;
                    }
                }
                if count >= 3 {
                    let denominator = count as f64 * sxx - sx * sx;
                    if denominator.abs() > 1e-12 {
                        let slope = (count as f64 * sxy - sx * sy) / denominator;
                        let intercept = (sy - slope * sx) / count as f64;
                        return vec![intercept.exp(), slope];
                    }
                }
                vec![y_max / x_max.max(1e-6), 1.0]
            }
        }
    }
}

/// Fitted nonlinear regression.
#[derive(Debug, Clone, PartialEq)]
pub struct NonlinearFit {
    /// Model.
    pub model: NonlinearModel,
    /// Parameter names.
    pub names: Vec<String>,
    /// Estimates.
    pub parameters: Vec<f64>,
    /// Standard errors.
    pub standard_errors: Vec<f64>,
    /// t statistics.
    pub t_values: Vec<f64>,
    /// p-values.
    pub p_values: Vec<f64>,
    /// Lower confidence bounds.
    pub ci_lower: Vec<f64>,
    /// Upper confidence bounds.
    pub ci_upper: Vec<f64>,
    /// Coefficient of determination.
    pub r_squared: f64,
    /// Residual standard error (`n − p`).
    pub s: f64,
    /// Iterations performed.
    pub iterations: usize,
    /// Whether the last step converged.
    pub converged: bool,
    /// Predictor values used.
    pub x: Vec<f64>,
    /// Response values used.
    pub y: Vec<f64>,
    /// Fitted values.
    pub fitted: Vec<f64>,
    /// Residuals.
    pub residuals: Vec<f64>,
    /// Observations used.
    pub n: usize,
    /// Rows excluded for a missing value.
    pub excluded: usize,
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

fn residuals(pairs: &[(f64, f64)], model: NonlinearModel, p: &[f64]) -> Vec<f64> {
    pairs
        .iter()
        .map(|(x, y)| y - model.evaluate(*x, p))
        .collect()
}

fn sum_squares(values: &[f64]) -> f64 {
    values.iter().map(|value| value * value).sum()
}

/// Solves `A·x = b` by Gauss–Jordan with partial pivoting.
#[allow(clippy::needless_range_loop)]
fn solve(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = a.len();
    if n == 0 || b.len() != n {
        return None;
    }
    let mut matrix = a.to_vec();
    let mut rhs = b.to_vec();
    for column in 0..n {
        let mut pivot = column;
        for row in (column + 1)..n {
            if matrix[row][column].abs() > matrix[pivot][column].abs() {
                pivot = row;
            }
        }
        if matrix[pivot][column].abs() < 1e-14 {
            return None;
        }
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        let divisor = matrix[column][column];
        for value in matrix[column].iter_mut().skip(column) {
            *value /= divisor;
        }
        rhs[column] /= divisor;
        for row in 0..n {
            if row == column {
                continue;
            }
            let factor = matrix[row][column];
            if factor == 0.0 {
                continue;
            }
            for index in column..n {
                matrix[row][index] -= factor * matrix[column][index];
            }
            rhs[row] -= factor * rhs[column];
        }
    }
    Some(rhs)
}

/// Inverse of a small square matrix.
fn invert(matrix: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = matrix.len();
    let mut inverse = vec![vec![0.0; n]; n];
    for index in 0..n {
        let mut unit = vec![0.0; n];
        unit[index] = 1.0;
        let column = solve(matrix, &unit)?;
        for (target, value) in inverse.iter_mut().zip(column) {
            target[index] = value;
        }
    }
    Some(inverse)
}

/// Fits the model with automatic starting values.
///
/// # Errors
///
/// The errors of [`fit_with_start`].
pub fn fit(
    df: &DataFrame,
    response: &str,
    predictor: &str,
    model: NonlinearModel,
) -> Result<NonlinearFit> {
    fit_with_start(df, response, predictor, model, None)
}

/// Fits the model, optionally from the given starting values.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for the same column twice, a non-numeric
/// response or predictor, a wrong number of starting values and a model
/// domain without observations, [`Error::SampleTooSmall`] when the design has
/// no residual degrees of freedom and [`Error::NotConverged`] when the
/// iteration cannot improve the fit.
#[allow(clippy::needless_range_loop)]
pub fn fit_with_start(
    df: &DataFrame,
    response: &str,
    predictor: &str,
    model: NonlinearModel,
    start: Option<&[f64]>,
) -> Result<NonlinearFit> {
    if response == predictor {
        return Err(Error::Unsupported(
            "the response and the predictor must be different columns".to_owned(),
        ));
    }
    let response_column = df.column(response)?;
    let predictor_column = df.column(predictor)?;
    for (name, column) in [(response, response_column), (predictor, predictor_column)] {
        if matches!(
            column.data(),
            ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
        ) {
            return Err(Error::Csv(format!("column {name} is not numeric")));
        }
    }
    let mut pairs: Vec<(f64, f64)> = Vec::new();
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        match (cell(predictor_column, row), cell(response_column, row)) {
            (Some(x), Some(y)) => pairs.push((x, y)),
            _ => excluded += 1,
        }
    }
    let parameters = model.parameter_count();
    if pairs.len() < parameters + 1 {
        return Err(Error::SampleTooSmall {
            n: pairs.len(),
            min: parameters + 1,
        });
    }
    if model.needs_positive_x() && pairs.iter().any(|(x, _)| *x <= 0.0) {
        return Err(Error::Unsupported(format!(
            "{} needs strictly positive predictor values",
            model.name()
        )));
    }
    if model.needs_non_negative_x() && pairs.iter().any(|(x, _)| *x < 0.0) {
        return Err(Error::Unsupported(format!(
            "{} needs non-negative predictor values",
            model.name()
        )));
    }
    let x: Vec<f64> = pairs.iter().map(|(x, _)| *x).collect();
    let y: Vec<f64> = pairs.iter().map(|(_, y)| *y).collect();
    let mut current = match start {
        Some(values) if values.len() == parameters => values.to_vec(),
        Some(_) => {
            return Err(Error::InvalidParameter {
                name: "starting values",
                value: parameters as f64,
            });
        }
        None => model.start(&x, &y),
    };
    if current.iter().any(|value| !value.is_finite()) {
        return Err(Error::Unsupported(
            "the starting values are not finite: check the data range".to_owned(),
        ));
    }

    let mut residual = residuals(&pairs, model, &current);
    let mut rss = sum_squares(&residual);
    if !rss.is_finite() {
        return Err(Error::NotConverged {
            detail: "the starting values give a non-finite residual sum of squares".to_owned(),
        });
    }
    let mut damping = 1e-3;
    let mut iterations = 0usize;
    let mut converged = false;
    for iteration in 0..200 {
        iterations = iteration + 1;
        // Numerical Jacobian, one column per parameter.
        let mut jacobian = vec![vec![0.0; parameters]; pairs.len()];
        for (index, value) in current.iter().enumerate() {
            let step = 1e-6 * value.abs().max(1.0);
            let mut forward = current.clone();
            let mut backward = current.clone();
            forward[index] = value + step;
            backward[index] = value - step;
            let plus = residuals(&pairs, model, &forward);
            let minus = residuals(&pairs, model, &backward);
            // The residuals are y − f, so the derivative of f is
            // (f(θ+h) − f(θ−h))/(2h) = (minus − plus)/(2h).
            for row in 0..pairs.len() {
                jacobian[row][index] = (minus[row] - plus[row]) / (2.0 * step);
            }
        }
        let mut normal = vec![vec![0.0; parameters]; parameters];
        let mut gradient = vec![0.0; parameters];
        for row in 0..pairs.len() {
            for i in 0..parameters {
                gradient[i] += jacobian[row][i] * residual[row];
                for j in 0..parameters {
                    normal[i][j] += jacobian[row][i] * jacobian[row][j];
                }
            }
        }
        // Add the damping to the diagonal and solve for the step.
        let mut damped = normal.clone();
        for (index, row) in damped.iter_mut().enumerate() {
            let diagonal = normal[index][index].abs().max(1e-12);
            row[index] += damping * diagonal;
        }
        let step = match solve(&damped, &gradient) {
            Some(step) => step,
            None => {
                damping *= 10.0;
                if damping > 1e12 {
                    break;
                }
                continue;
            }
        };
        let largest = step
            .iter()
            .zip(&current)
            .map(|(delta, value)| delta.abs() / (1.0 + value.abs()))
            .fold(0.0, f64::max);
        let candidate: Vec<f64> = current
            .iter()
            .zip(&step)
            .map(|(value, delta)| value + delta)
            .collect();
        let candidate_residual = residuals(&pairs, model, &candidate);
        let candidate_rss = sum_squares(&candidate_residual);
        if candidate_rss.is_finite() && candidate_rss <= rss {
            let gain = if rss > 0.0 {
                (rss - candidate_rss) / rss
            } else {
                0.0
            };
            current = candidate;
            residual = candidate_residual;
            rss = candidate_rss;
            damping = (damping / 10.0).max(1e-12);
            if gain < 1e-12 || largest < 1e-9 {
                converged = true;
                break;
            }
        } else {
            damping *= 10.0;
            if damping > 1e12 {
                break;
            }
        }
    }
    let fitted: Vec<f64> = x
        .iter()
        .map(|value| model.evaluate(*value, &current))
        .collect();
    if fitted.iter().any(|value| !value.is_finite()) || rss < 0.0 || !rss.is_finite() {
        return Err(Error::NotConverged {
            detail: "the iteration did not produce a finite fit".to_owned(),
        });
    }
    let n = pairs.len();
    let df = (n - parameters) as f64;
    let mean = y.iter().sum::<f64>() / n as f64;
    let total: f64 = y.iter().map(|value| (value - mean).powi(2)).sum();
    let r_squared = if total > 0.0 {
        (1.0 - rss / total).clamp(0.0, 1.0)
    } else {
        f64::NAN
    };
    let s = (rss / df).sqrt();
    // Asymptotic covariance: s²(JᵀJ)⁻¹ at the solution.
    let mut jacobian = vec![vec![0.0; parameters]; n];
    for (index, value) in current.iter().enumerate() {
        let step = 1e-6 * value.abs().max(1.0);
        let mut forward = current.clone();
        let mut backward = current.clone();
        forward[index] = value + step;
        backward[index] = value - step;
        let plus = residuals(&pairs, model, &forward);
        let minus = residuals(&pairs, model, &backward);
        for row in 0..n {
            jacobian[row][index] = (minus[row] - plus[row]) / (2.0 * step);
        }
    }
    let mut normal = vec![vec![0.0; parameters]; parameters];
    for row in 0..n {
        for i in 0..parameters {
            for j in 0..parameters {
                normal[i][j] += jacobian[row][i] * jacobian[row][j];
            }
        }
    }
    let inverse = invert(&normal);
    let critical = student_t_quantile(0.975, df);
    let mut standard_errors = vec![f64::NAN; parameters];
    let mut t_values = vec![f64::NAN; parameters];
    let mut p_values = vec![f64::NAN; parameters];
    let mut ci_lower = vec![f64::NAN; parameters];
    let mut ci_upper = vec![f64::NAN; parameters];
    if let Some(inverse) = &inverse {
        for index in 0..parameters {
            let variance = s * s * inverse[index][index];
            if variance >= 0.0 && variance.is_finite() {
                let se = variance.sqrt();
                standard_errors[index] = se;
                if se > 0.0 {
                    let t = current[index] / se;
                    t_values[index] = t;
                    p_values[index] = (2.0 * student_t_cdf(-t.abs(), df)).clamp(0.0, 1.0);
                }
                ci_lower[index] = current[index] - critical * se;
                ci_upper[index] = current[index] + critical * se;
            }
        }
    }

    Ok(NonlinearFit {
        model,
        names: model
            .parameter_names()
            .into_iter()
            .map(ToString::to_string)
            .collect(),
        parameters: current,
        standard_errors,
        t_values,
        p_values,
        ci_lower,
        ci_upper,
        r_squared,
        s,
        iterations,
        converged,
        x,
        y,
        residuals: residual,
        fitted,
        n,
        excluded,
    })
}

/// Builds the report and the fitted curve.
///
/// # Errors
///
/// The errors of [`fit`].
pub fn analyze(
    df: &DataFrame,
    response: &str,
    predictor: &str,
    model: NonlinearModel,
) -> Result<AnalysisOutput> {
    let fit = fit(df, response, predictor, model)?;
    let mut result = AnalysisResult::new(
        format!("Nonlinear Regression — {response} by {predictor}"),
        format!(
            "{} fitted by least squares (Levenberg–Marquardt, numerical Jacobian)",
            model.name()
        ),
    );
    result.n = fit.n;
    result.excluded = fit.excluded;
    result.statistics = vec![
        ("R²", fit.r_squared),
        ("Residual standard error", fit.s),
        ("Parameters", fit.parameters.len() as f64),
        ("Iterations", fit.iterations as f64),
        ("Converged", if fit.converged { 1.0 } else { 0.0 }),
    ];
    result
        .extra
        .push(Item::Message(format!("Equation: {}", model.equation())));
    result.extra.push(Item::Message(format!(
        "Estimates: {}",
        fit.names
            .iter()
            .zip(&fit.parameters)
            .map(|(name, value)| format!("{name} = {value:.6}"))
            .collect::<Vec<String>>()
            .join(", ")
    )));
    result.extra.push(Item::Table {
        headers: vec![
            "Parameter".to_owned(),
            "Estimate".to_owned(),
            "SE".to_owned(),
            "T".to_owned(),
            "P".to_owned(),
            "CI lower".to_owned(),
            "CI upper".to_owned(),
        ],
        rows: fit
            .names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let number = |value: f64| {
                    if value.is_finite() {
                        format!("{value:.6}")
                    } else {
                        String::new()
                    }
                };
                vec![
                    name.clone(),
                    format!("{:.6}", fit.parameters[index]),
                    number(fit.standard_errors[index]),
                    number(fit.t_values[index]),
                    number(fit.p_values[index]),
                    number(fit.ci_lower[index]),
                    number(fit.ci_upper[index]),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Model".to_owned(),
            format!(
                "{}; {} parameter(s), fitted to the complete pairs of the two columns",
                model.equation(),
                fit.parameters.len()
            ),
        ),
        (
            "Algorithm".to_owned(),
            "Levenberg–Marquardt with a central-difference Jacobian and automatic starting \
             values from the data range; the iteration stops when a step changes the parameters \
             by less than 1e-9 relative or the residual sum of squares by less than 1e-12"
                .to_owned(),
        ),
        (
            "Inference".to_owned(),
            "asymptotic standard errors s²(JᵀJ)⁻¹ with n − p degrees of freedom; the intervals \
             are parameter-wise at 95 %"
                .to_owned(),
        ),
        (
            "Scope".to_owned(),
            "one predictor, no weights, no parameter bounds and no confidence band for the \
             curve; the curve is drawn over the range of the observations"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(fit.n, fit.excluded, 0);
    if !fit.converged {
        result.warnings.push(
            "the iteration did not reach the tolerance: the estimates may be at a local \
             minimum or the model may not fit the data"
                .to_owned(),
        );
    }
    if fit.standard_errors.iter().any(|value| !value.is_finite()) {
        result.warnings.push(
            "a standard error is not available: the Jacobian is singular at the solution"
                .to_owned(),
        );
    }
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot(predictor, &fit)),
    })
}

/// Scatter plot of the observations with the fitted curve.
fn plot(predictor: &str, fit: &NonlinearFit) -> PlotSpec {
    let x_min = fit.x.iter().copied().fold(f64::INFINITY, f64::min);
    let x_max = fit.x.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut low = fit.y.iter().copied().fold(f64::INFINITY, f64::min);
    let mut high = fit.y.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    for value in &fit.fitted {
        if value.is_finite() {
            low = low.min(*value);
            high = high.max(*value);
        }
    }
    let pad = ((high - low).abs() * 0.06).max(1e-9);
    let steps = 120;
    let mut series_points: Vec<Point> = Vec::with_capacity(steps);
    for step in 0..steps {
        let share = step as f64 / (steps - 1) as f64;
        let x = x_min + (x_max - x_min) * share;
        let y = fit.model.evaluate(x, &fit.parameters);
        if y.is_finite() {
            series_points.push(Point { x, y });
        }
    }
    let mut lines = Vec::new();
    for pair in series_points.windows(2) {
        lines.push(Segment {
            from: pair[0],
            to: pair[1],
            width: 2.0,
        });
    }
    PlotSpec {
        title: format!("Nonlinear Regression — {}", fit.model.name()),
        x_axis: linear_axis(predictor, x_min, x_max),
        y_axis: linear_axis("Response", low - pad, high + pad),
        size: (900, 520),
        points: fit
            .x
            .iter()
            .zip(&fit.y)
            .map(|(x, y)| Point { x: *x, y: *y })
            .collect(),
        lines: Vec::new(),
        bands: Vec::new(),
        series: vec![Series {
            label: "Fitted".to_owned(),
            points: series_points,
            lines,
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

    fn frame(x: Vec<f64>, y: Vec<f64>) -> DataFrame {
        DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame")
    }

    /// The treated group of the classic enzyme-kinetics experiment: twelve
    /// observations whose nonlinear least-squares fit is published
    /// (`V = 212.6837`, `K = 0.0641213`, residual sum of squares `1195.449`).
    fn kinetics() -> DataFrame {
        frame(
            vec![
                0.02, 0.02, 0.06, 0.06, 0.11, 0.11, 0.22, 0.22, 0.56, 0.56, 1.10, 1.10,
            ],
            vec![
                76.0, 47.0, 97.0, 107.0, 123.0, 139.0, 159.0, 152.0, 191.0, 201.0, 207.0, 200.0,
            ],
        )
    }

    #[test]
    fn the_michaelis_menten_matches_the_published_fit() {
        let df = kinetics();
        let model = fit(&df, "y", "x", NonlinearModel::MichaelisMenten).expect("fit");
        assert!(model.converged);
        assert!(
            (model.parameters[0] - 212.683_743).abs() < 1e-3,
            "{:?}",
            model.parameters
        );
        assert!(
            (model.parameters[1] - 0.064_121_281_7).abs() < 1e-7,
            "{:?}",
            model.parameters
        );
        assert!(
            (model.r_squared - 0.961_26).abs() < 1e-4,
            "{}",
            model.r_squared
        );
        assert_eq!(model.n, 12);
        assert!((model.s - (1_195.448_8_f64 / 10.0).sqrt()).abs() < 1e-3);
        // The standard errors of the published fit.
        assert!((model.standard_errors[0] - 6.947_16).abs() < 1e-2);
        assert!((model.standard_errors[1] - 0.008_281).abs() < 1e-5);
        assert!(model.p_values.iter().all(|value| *value < 0.001));
        let report = analyze(&df, "y", "x", NonlinearModel::MichaelisMenten).expect("report");
        let text = report.results[0].to_session_text();
        assert!(text.contains("Michaelis–Menten"), "{text}");
        assert!(text.contains("V·x/(K + x)"), "{text}");
        assert!(report.plot.is_some());
    }

    #[test]
    fn every_model_recovers_a_noise_free_curve() {
        let cases: [(NonlinearModel, Vec<f64>, Vec<f64>); 5] = [
            (
                NonlinearModel::Asymptotic,
                vec![10.0, -5.0, 0.7],
                (0..40).map(|index| index as f64 * 0.25).collect(),
            ),
            (
                NonlinearModel::Logistic,
                vec![8.0, 1.2, 5.0],
                (0..40).map(|index| index as f64 * 0.25).collect(),
            ),
            (
                NonlinearModel::Gompertz,
                vec![10.0, 0.5, 4.0],
                (0..40).map(|index| index as f64 * 0.3).collect(),
            ),
            (
                NonlinearModel::WeibullGrowth,
                vec![9.0, 4.0, 2.0],
                (0..40).map(|index| index as f64 * 0.3).collect(),
            ),
            (
                NonlinearModel::Power,
                vec![2.0, 0.7],
                (1..41).map(|index| index as f64 * 0.25).collect(),
            ),
        ];
        for (model, truth, x) in cases {
            let y: Vec<f64> = x
                .iter()
                .map(|value| model.evaluate(*value, &truth))
                .collect();
            let fit = fit(&frame(x, y), "y", "x", model).expect("fit");
            assert!(fit.converged, "{}", model.name());
            for (found, expected) in fit.parameters.iter().zip(&truth) {
                let tolerance = 1e-3 * expected.abs().max(1e-3);
                assert!(
                    (found - expected).abs() < tolerance,
                    "{}: {found} vs {expected}",
                    model.name()
                );
            }
            assert!(
                fit.r_squared > 0.999_999,
                "{}: {}",
                model.name(),
                fit.r_squared
            );
        }
    }

    #[test]
    fn the_fit_accepts_explicit_starting_values() {
        let df = kinetics();
        let model = fit_with_start(
            &df,
            "y",
            "x",
            NonlinearModel::MichaelisMenten,
            Some(&[200.0, 0.1]),
        )
        .expect("fit");
        assert!((model.parameters[0] - 212.683_743).abs() < 1e-3);
        assert!(
            fit_with_start(
                &df,
                "y",
                "x",
                NonlinearModel::MichaelisMenten,
                Some(&[200.0])
            )
            .is_err()
        );
    }

    #[test]
    fn the_fit_rejects_invalid_requests() {
        let df = kinetics();
        assert!(fit(&df, "y", "y", NonlinearModel::MichaelisMenten).is_err());
        // The power model needs positive predictors.
        let negative = frame(vec![-1.0, 1.0, 2.0, 3.0], vec![1.0, 2.0, 3.0, 4.0]);
        assert!(
            fit(&negative, "y", "x", NonlinearModel::Power)
                .unwrap_err()
                .to_string()
                .contains("positive")
        );
        // Too few observations for the parameters.
        let short = frame(vec![1.0, 2.0], vec![1.0, 2.0]);
        assert!(matches!(
            fit(&short, "y", "x", NonlinearModel::Logistic),
            Err(Error::SampleTooSmall { .. })
        ));
        // A text response.
        let words = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_text(
                "y",
                ["1", "2", "3", "4"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ])
        .expect("frame");
        assert!(fit(&words, "y", "x", NonlinearModel::Power).is_err());
    }
}
