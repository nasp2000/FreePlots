// SPDX-License-Identifier: MIT
//! Ordinary least squares regression with numeric predictors.
//!
//! The design matrix always includes an intercept. The fit uses a Householder
//! QR decomposition rather than the normal equations, so the coefficient
//! standard errors stay meaningful when the predictors are correlated. Rows
//! with a missing value in the response or in any predictor are removed
//! listwise and counted.

use std::collections::BTreeSet;

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{f_cdf, student_t_cdf, student_t_quantile};
use crate::workflow::AnalysisOutput;

/// A fitted ordinary least squares model.
#[derive(Debug, Clone, PartialEq)]
pub struct RegressionFit {
    /// Coefficient names: `Constant`, then the predictor names.
    pub names: Vec<String>,
    /// Estimated coefficients.
    pub coefficients: Vec<f64>,
    /// Standard errors.
    pub standard_errors: Vec<f64>,
    /// t statistics.
    pub t_values: Vec<f64>,
    /// Two-sided p-values.
    pub p_values: Vec<f64>,
    /// Lower confidence bounds.
    pub ci_lower: Vec<f64>,
    /// Upper confidence bounds.
    pub ci_upper: Vec<f64>,
    /// Fitted values.
    pub fitted: Vec<f64>,
    /// Residuals.
    pub residuals: Vec<f64>,
    /// Residual standard error.
    pub s: f64,
    /// Regression sum of squares.
    pub regression_ss: f64,
    /// Residual sum of squares.
    pub residual_ss: f64,
    /// Total sum of squares.
    pub total_ss: f64,
    /// Coefficient of determination.
    pub r_squared: f64,
    /// Adjusted coefficient of determination.
    pub adjusted_r_squared: f64,
    /// Regression F statistic.
    pub f: f64,
    /// p-value of the regression F test.
    pub f_p_value: f64,
    /// Regression degrees of freedom.
    pub df_regression: f64,
    /// Residual degrees of freedom.
    pub df_residual: f64,
    /// Rows used.
    pub n: usize,
    /// Rows removed listwise.
    pub excluded: usize,
    /// Confidence level.
    pub confidence: f64,
    /// Durbin–Watson statistic of the residuals, in row order.
    pub durbin_watson: f64,
    /// Variance inflation factor of every term (the constant is excluded).
    pub variance_inflation: Vec<f64>,
    /// Leverage of every observation.
    pub leverage: Vec<f64>,
    /// Cook's distance of every observation.
    pub cooks_distance: Vec<f64>,
    /// Predicted R² from the PRESS statistic (`1 − Σ(eᵢ/(1 − hᵢ))² / SS_total`).
    pub predicted_r_squared: f64,
}

pub(crate) fn cell(column: &Column, row: usize) -> Option<f64> {
    column.value(row)
}

pub(crate) fn label(column: &Column, row: usize) -> Option<String> {
    column.label(row)
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

/// Fits `response ~ predictors` by ordinary least squares.
pub fn fit(
    df: &DataFrame,
    response: &str,
    predictors: &[String],
    confidence: f64,
) -> Result<RegressionFit> {
    fit_impl(df, response, predictors, confidence, false)
}

/// Fits a saturated model (`n == p`), used by the factorial analysis where the
/// center points supply the error degrees of freedom.
pub(crate) fn fit_saturated(
    df: &DataFrame,
    response: &str,
    predictors: &[String],
    confidence: f64,
) -> Result<RegressionFit> {
    fit_impl(df, response, predictors, confidence, true)
}

/// Predicts the response at new points from an ordinary least squares model.
///
/// The model is the one identified by `response` and `predictors` and fitted to
/// the current worksheet. Each point holds one value per predictor. The report
/// gives the fitted value, its standard error, the confidence interval of the
/// mean response and the prediction interval of a single observation.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for no points, a point with the wrong number
/// of values or a predictor that is not numeric, and the fitting errors of
/// [`fit`].
pub fn predict_points(
    df: &DataFrame,
    response: &str,
    predictors: &[String],
    confidence: f64,
    points: &[Vec<f64>],
) -> Result<AnalysisResult> {
    validate_confidence(confidence)?;
    if points.is_empty() {
        return Err(Error::Unsupported(
            "the prediction needs at least one new point".to_owned(),
        ));
    }
    let response_column = df.column(response)?;
    let predictor_columns = predictors
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    for (name, column) in predictors.iter().zip(&predictor_columns) {
        if matches!(
            column.data(),
            ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
        ) {
            return Err(Error::Unsupported(format!(
                "prediction needs numeric predictors; {name} is not numeric"
            )));
        }
    }
    let model = fit(df, response, predictors, confidence)?;
    let p = predictors.len() + 1;
    for (index, point) in points.iter().enumerate() {
        if point.len() != predictors.len() {
            return Err(Error::Unsupported(format!(
                "point {} has {} value(s); {} predictor(s) were requested",
                index + 1,
                point.len(),
                predictors.len()
            )));
        }
        if point.iter().any(|value| !value.is_finite()) {
            return Err(Error::Unsupported(format!(
                "point {} has a non-finite value",
                index + 1
            )));
        }
    }

    // `XᵀX` over the complete rows, with the same listwise filter as the fit.
    let mut xx = vec![vec![0.0; p]; p];
    let mut rows = 0usize;
    for row in 0..df.n_rows() {
        if cell(response_column, row).is_none() {
            continue;
        }
        let mut design = Vec::with_capacity(p);
        design.push(1.0);
        let mut complete = true;
        for column in &predictor_columns {
            match cell(column, row) {
                Some(value) => design.push(value),
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if !complete {
            continue;
        }
        for (i, left) in design.iter().enumerate() {
            for (j, right) in design.iter().enumerate() {
                xx[i][j] += left * right;
            }
        }
        rows += 1;
    }
    if rows != model.n {
        return Err(Error::NotConverged {
            detail: "the prediction design does not match the fitted rows".to_owned(),
        });
    }
    let critical = student_t_quantile(1.0 - (1.0 - confidence) / 2.0, model.df_residual);

    let mut table = Vec::with_capacity(points.len());
    for (index, point) in points.iter().enumerate() {
        let mut design = Vec::with_capacity(p);
        design.push(1.0);
        design.extend(point.iter().copied());
        let solved = solve(&xx, &design).ok_or_else(|| Error::NotConverged {
            detail: "the predictor matrix is singular at a new point".to_owned(),
        })?;
        let leverage = design
            .iter()
            .zip(&solved)
            .map(|(value, solution)| value * solution)
            .sum::<f64>()
            .max(0.0);
        let fitted = design
            .iter()
            .zip(&model.coefficients)
            .map(|(value, coefficient)| value * coefficient)
            .sum::<f64>();
        let se_fit = model.s * leverage.sqrt();
        let half_ci = critical * se_fit;
        let half_pi = critical * model.s * (1.0 + leverage).sqrt();
        let mut row = Vec::with_capacity(predictors.len() + 6);
        row.push((index + 1).to_string());
        row.extend(point.iter().map(|value| format!("{value:.6}")));
        row.push(format!("{fitted:.6}"));
        row.push(format!("{se_fit:.6}"));
        row.push(format!("{:.6}", fitted - half_ci));
        row.push(format!("{:.6}", fitted + half_ci));
        row.push(format!("{:.6}", fitted - half_pi));
        row.push(format!("{:.6}", fitted + half_pi));
        table.push(row);
    }

    let mut headers = Vec::with_capacity(predictors.len() + 6);
    headers.push("Point".to_owned());
    headers.extend(predictors.iter().cloned());
    for name in [
        "Fit", "SE fit", "CI lower", "CI upper", "PI lower", "PI upper",
    ] {
        headers.push(name.to_owned());
    }

    let mut result = AnalysisResult::new(
        format!("Prediction — response {response}"),
        format!(
            "Ordinary least squares with {} predictor(s); {:.1}% confidence and prediction \
             intervals",
            predictors.len(),
            confidence * 100.0
        ),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Residual standard error", model.s),
        ("Residual degrees of freedom", model.df_residual),
        ("R²", model.r_squared),
        ("Confidence", confidence),
    ];
    result.extra.push(Item::Message(format!(
        "Model equation: {}",
        equation(&model)
    )));
    result.extra.push(Item::Table {
        headers,
        rows: table,
    });
    result.notes = vec![
        (
            "Intervals".to_owned(),
            "the confidence interval is for the mean response at the point; the prediction \
             interval is for a single new observation"
                .to_owned(),
        ),
        (
            "Model".to_owned(),
            "the model is identified by the response and the predictors and fitted to every \
             complete row of the worksheet when the command runs"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing value in the response or a predictor are excluded listwise and \
             counted"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "prediction outside the range of the fitted data is an extrapolation; the intervals \
         assume the model is correct"
            .to_owned(),
    );
    Ok(result)
}

/// Solves `A·x = b` by Gaussian elimination with partial pivoting.
#[allow(clippy::needless_range_loop)]
fn solve(matrix: &[Vec<f64>], rhs: &[f64]) -> Option<Vec<f64>> {
    let n = rhs.len();
    if matrix.len() != n {
        return None;
    }
    let mut a = matrix.to_vec();
    let mut b = rhs.to_vec();
    let scale = matrix
        .iter()
        .flat_map(|row| row.iter())
        .fold(0.0_f64, |largest, value| largest.max(value.abs()))
        .max(1.0);
    let tolerance = 1e-12 * scale;
    for column in 0..n {
        let pivot =
            (column..n).max_by(|&i, &j| a[i][column].abs().total_cmp(&a[j][column].abs()))?;
        if a[pivot][column].abs() < tolerance {
            return None;
        }
        a.swap(column, pivot);
        b.swap(column, pivot);
        for row in (column + 1)..n {
            let factor = a[row][column] / a[column][column];
            for k in column..n {
                a[row][k] -= factor * a[column][k];
            }
            b[row] -= factor * b[column];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut sum = b[row];
        for k in (row + 1)..n {
            sum -= a[row][k] * x[k];
        }
        x[row] = sum / a[row][row];
    }
    Some(x)
}

fn fit_impl(
    df: &DataFrame,
    response: &str,
    predictors: &[String],
    confidence: f64,
    allow_saturated: bool,
) -> Result<RegressionFit> {
    validate_confidence(confidence)?;
    if predictors.is_empty() {
        return Err(Error::Unsupported(
            "regression needs at least one predictor".to_owned(),
        ));
    }
    if predictors.iter().any(|name| name == response) {
        return Err(Error::Unsupported(
            "the response cannot also be a predictor".to_owned(),
        ));
    }
    let mut unique = predictors.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != predictors.len() {
        return Err(Error::Unsupported(
            "the predictor list contains duplicate columns".to_owned(),
        ));
    }

    let response_column = df.column(response)?;
    if matches!(
        response_column.data(),
        ColumnData::Text(_) | ColumnData::Bool(_)
    ) {
        return Err(Error::Csv(format!(
            "column {response} is not numeric and cannot enter a regression"
        )));
    }
    let predictor_columns = predictors
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;

    enum Encoding {
        Numeric,
        Categorical { levels: Vec<String> },
    }
    let mut encodings: Vec<Encoding> = Vec::new();
    let mut term_names: Vec<String> = Vec::new();
    for (name, column) in predictors.iter().zip(&predictor_columns) {
        if matches!(
            column.data(),
            ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
        ) {
            let levels: BTreeSet<String> = (0..df.n_rows())
                .filter_map(|row| label(column, row))
                .collect();
            if levels.len() < 2 {
                return Err(Error::Unsupported(format!(
                    "categorical predictor {name} needs at least two levels"
                )));
            }
            let mut levels: Vec<String> = levels.into_iter().collect();
            levels.remove(0); // first level is the reference
            for level in &levels {
                term_names.push(format!("{name}[{level}]"));
            }
            encodings.push(Encoding::Categorical { levels });
        } else {
            term_names.push(name.clone());
            encodings.push(Encoding::Numeric);
        }
    }

    let mut xs: Vec<Vec<f64>> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for row in 0..df.n_rows() {
        let Some(y) = cell(response_column, row) else {
            continue;
        };
        let mut values = Vec::with_capacity(term_names.len());
        let mut complete = true;
        for (encoding, column) in encodings.iter().zip(&predictor_columns) {
            match encoding {
                Encoding::Numeric => match cell(column, row) {
                    Some(value) => values.push(value),
                    None => {
                        complete = false;
                        break;
                    }
                },
                Encoding::Categorical { levels } => {
                    let Some(value) = label(column, row) else {
                        complete = false;
                        break;
                    };
                    for level in levels {
                        values.push(if value == *level { 1.0 } else { 0.0 });
                    }
                }
            }
        }
        if complete {
            xs.push(values);
            ys.push(y);
        }
    }

    let k = term_names.len();
    let p = k + 1;
    let n = ys.len();
    if n < p || (!allow_saturated && n == p) {
        return Err(Error::SampleTooSmall { n, min: p + 1 });
    }
    let mean_y = ys.iter().sum::<f64>() / n as f64;
    let sst = ys.iter().map(|value| (value - mean_y).powi(2)).sum::<f64>();
    if !sst.is_finite() || sst <= 0.0 {
        return Err(Error::ConstantSample);
    }

    // Design matrix with an intercept column.
    let mut design = vec![vec![0.0; p]; n];
    for (row, values) in xs.iter().enumerate() {
        design[row][0] = 1.0;
        for (j, &value) in values.iter().enumerate() {
            design[row][j + 1] = value;
        }
    }
    let design_copy = design.clone();
    let coefficients = least_squares(&mut design, &ys)?;
    let r_matrix = r_matrix_of(&design, p);

    let fitted: Vec<f64> = xs
        .iter()
        .map(|values| {
            coefficients[0]
                + values
                    .iter()
                    .zip(&coefficients[1..])
                    .map(|(value, coefficient)| value * coefficient)
                    .sum::<f64>()
        })
        .collect();
    let residuals: Vec<f64> = ys
        .iter()
        .zip(&fitted)
        .map(|(value, fitted)| value - fitted)
        .collect();
    let sse = residuals.iter().map(|value| value * value).sum::<f64>();
    let df_residual = n as f64 - p as f64;
    let df_regression = k as f64;
    // A saturated model has no residual degrees of freedom: the variance,
    // the standard errors and the tests are undefined and stay NaN. The
    // factorial analysis replaces them with the pure error of the center
    // points.
    let variance = if df_residual > 0.0 {
        sse / df_residual
    } else {
        f64::NAN
    };
    let s = if variance.is_finite() {
        variance.max(0.0).sqrt()
    } else {
        f64::NAN
    };
    let ssr = (sst - sse).max(0.0);
    let r_squared = (ssr / sst).clamp(0.0, 1.0);
    let adjusted_r_squared = if df_residual > 0.0 {
        1.0 - (1.0 - r_squared) * (n as f64 - 1.0) / df_residual
    } else {
        1.0
    };
    let f = if !variance.is_finite() {
        f64::NAN
    } else if sse > 0.0 {
        (ssr / df_regression) / variance
    } else {
        f64::INFINITY
    };
    let f_p_value = if f.is_finite() {
        (1.0 - f_cdf(f, df_regression, df_residual)).clamp(0.0, 1.0)
    } else {
        f64::NAN
    };

    // Inverse of the upper triangular R, then (XᵀX)⁻¹ = R⁻¹R⁻ᵀ.
    let r_inv = inverse_upper(&r_matrix, p)?;

    let mut standard_errors = vec![f64::NAN; p];
    if variance.is_finite() {
        for j in 0..p {
            standard_errors[j] = variance.max(0.0).sqrt()
                * r_inv[j]
                    .iter()
                    .map(|value| value * value)
                    .sum::<f64>()
                    .sqrt();
        }
    }

    let mut t_values = vec![0.0; p];
    let mut p_values = vec![0.0; p];
    for j in 0..p {
        if standard_errors[j].is_finite() && standard_errors[j] > 0.0 {
            let t = coefficients[j] / standard_errors[j];
            t_values[j] = t;
            let cdf = student_t_cdf(t, df_residual);
            p_values[j] = (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0);
        } else if !standard_errors[j].is_finite() {
            t_values[j] = f64::NAN;
            p_values[j] = f64::NAN;
        } else if coefficients[j].abs() < 1e-15 {
            t_values[j] = 0.0;
            p_values[j] = 1.0;
        } else {
            t_values[j] = f64::INFINITY;
            p_values[j] = 0.0;
        }
    }
    let critical = student_t_quantile(1.0 - (1.0 - confidence) / 2.0, df_residual);
    let ci_lower = coefficients
        .iter()
        .zip(&standard_errors)
        .map(|(estimate, se)| estimate - critical * se)
        .collect();
    let ci_upper = coefficients
        .iter()
        .zip(&standard_errors)
        .map(|(estimate, se)| estimate + critical * se)
        .collect();

    // Durbin–Watson: the residuals in row order.
    let numerator: f64 = residuals
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).powi(2))
        .sum();
    let durbin_watson = if sse > 0.0 { numerator / sse } else { f64::NAN };

    let variance_inflation = variance_inflation_factors(&design_copy, p)?;

    // The inverse of the cross-product matrix, (XᵀX)⁻¹ = R⁻¹R⁻ᵀ, gives the
    // leverage of every observation and, with it, Cook's distance.
    let mut inverse_xtx = vec![vec![0.0; p]; p];
    for (i, row) in inverse_xtx.iter_mut().enumerate().take(p) {
        for (j, cell) in row.iter_mut().enumerate().take(p) {
            let sum: f64 = r_inv[i]
                .iter()
                .zip(&r_inv[j])
                .map(|(left, right)| left * right)
                .sum();
            *cell = sum;
        }
    }
    let leverage: Vec<f64> = design_copy
        .iter()
        .map(|row| {
            let mut sum = 0.0;
            for i in 0..p {
                for j in 0..p {
                    sum += row[i] * inverse_xtx[i][j] * row[j];
                }
            }
            sum.max(0.0)
        })
        .collect();
    let cooks_distance: Vec<f64> = leverage
        .iter()
        .zip(&residuals)
        .map(|(h, residual)| {
            if *h >= 1.0 {
                return f64::INFINITY;
            }
            residual * residual / (p as f64 * variance) * h / (1.0 - h).powi(2)
        })
        .collect();

    // Predicted R²: leave-one-out residuals from the leverage.
    let press = residuals
        .iter()
        .zip(&leverage)
        .map(|(residual, h)| {
            if *h >= 1.0 {
                f64::INFINITY
            } else {
                (residual / (1.0 - h)).powi(2)
            }
        })
        .sum::<f64>();
    let predicted_r_squared = if sst > 0.0 && press.is_finite() {
        1.0 - press / sst
    } else {
        0.0
    };

    let mut names = Vec::with_capacity(p);
    names.push("Constant".to_owned());
    names.extend(term_names);

    Ok(RegressionFit {
        names,
        coefficients,
        standard_errors,
        t_values,
        p_values,
        ci_lower,
        ci_upper,
        fitted,
        residuals,
        s,
        regression_ss: ssr,
        residual_ss: sse,
        total_ss: sst,
        r_squared,
        adjusted_r_squared,
        f,
        f_p_value,
        df_regression,
        df_residual,
        n,
        excluded: df.n_rows() - n,
        confidence,
        durbin_watson,
        variance_inflation,
        leverage,
        cooks_distance,
        predicted_r_squared,
    })
}

/// Solves the least-squares problem `Xβ = y` with Householder reflections.
///
/// The design matrix is consumed: the reflections are written into it, so the
/// caller must keep a copy when it needs the original columns.
fn least_squares(design: &mut [Vec<f64>], y: &[f64]) -> Result<Vec<f64>> {
    let n = design.len();
    let p = design[0].len();
    let original_norms: Vec<f64> = (0..p)
        .map(|j| (0..n).map(|i| design[i][j].powi(2)).sum::<f64>().sqrt())
        .collect();
    let mut rhs = y.to_vec();
    for j in 0..p {
        let norm = (j..n).map(|i| design[i][j].powi(2)).sum::<f64>().sqrt();
        let tolerance = 1e-12 * original_norms[j].max(1.0);
        if !norm.is_finite() || norm <= tolerance {
            return Err(Error::Unsupported(
                "the predictors are collinear or constant".to_owned(),
            ));
        }
        let alpha = if design[j][j] >= 0.0 { -norm } else { norm };
        let mut v = vec![0.0; n];
        v[j] = design[j][j] - alpha;
        for i in (j + 1)..n {
            v[i] = design[i][j];
        }
        let vnorm2 = v.iter().map(|value| value * value).sum::<f64>();
        if vnorm2 <= 0.0 {
            return Err(Error::Unsupported(
                "the predictors are collinear or constant".to_owned(),
            ));
        }
        // Apply H = I − 2vvᵀ/(vᵀv) to the remaining columns and to y.
        #[allow(clippy::needless_range_loop)]
        for column in j..p {
            let dot = (j..n).map(|i| v[i] * design[i][column]).sum::<f64>();
            let factor = 2.0 * dot / vnorm2;
            for i in j..n {
                design[i][column] -= factor * v[i];
            }
        }
        let dot = (j..n).map(|i| v[i] * rhs[i]).sum::<f64>();
        let factor = 2.0 * dot / vnorm2;
        for i in j..n {
            rhs[i] -= factor * v[i];
        }
        design[j][j] = alpha;
        for row in design.iter_mut().take(n).skip(j + 1) {
            row[j] = 0.0;
        }
    }

    let r_matrix = r_matrix_of(design, p);

    // Solve Rβ = Qᵀy.
    let mut coefficients = vec![0.0; p];
    for i in (0..p).rev() {
        let mut sum = rhs[i];
        for j in (i + 1)..p {
            sum -= r_matrix[i][j] * coefficients[j];
        }
        coefficients[i] = sum / r_matrix[i][i];
    }
    Ok(coefficients)
}

/// Reads the upper triangular factor out of the transformed design.
fn r_matrix_of(design: &[Vec<f64>], p: usize) -> Vec<Vec<f64>> {
    let mut r_matrix = vec![vec![0.0; p]; p];
    for i in 0..p {
        for j in 0..p {
            r_matrix[i][j] = if j >= i { design[i][j] } else { 0.0 };
        }
    }
    r_matrix
}

/// Inverse of the upper triangular factor `R`.
fn inverse_upper(r_matrix: &[Vec<f64>], p: usize) -> Result<Vec<Vec<f64>>> {
    let mut r_inv = vec![vec![0.0; p]; p];
    for i in (0..p).rev() {
        if r_matrix[i][i].abs() < 1e-15 {
            return Err(Error::Unsupported(
                "the predictors are collinear or constant".to_owned(),
            ));
        }
        r_inv[i][i] = 1.0 / r_matrix[i][i];
        for j in (i + 1..p).rev() {
            let mut sum = 0.0;
            for k in (i + 1)..=j {
                sum += r_matrix[i][k] * r_inv[k][j];
            }
            r_inv[i][j] = -sum / r_matrix[i][i];
        }
    }
    Ok(r_inv)
}

/// Variance inflation factor of every term of the design.
///
/// The factor of the term `j` is `1/(1 − R²ⱼ)`, where `R²ⱼ` is the
/// coefficient of determination of the auxiliary regression of that column on
/// the intercept and the remaining columns.
fn variance_inflation_factors(design: &[Vec<f64>], p: usize) -> Result<Vec<f64>> {
    let n = design.len();
    let mut factors = Vec::with_capacity(p - 1);
    for j in 1..p {
        let response: Vec<f64> = design.iter().map(|row| row[j]).collect();
        let mean = response.iter().sum::<f64>() / n as f64;
        let total: f64 = response.iter().map(|value| (value - mean).powi(2)).sum();
        if total <= 0.0 {
            factors.push(f64::NAN);
            continue;
        }
        // Auxiliary design: the intercept and every other column.
        let mut auxiliary = vec![vec![0.0; p - 1]; n];
        for (row, values) in auxiliary.iter_mut().enumerate() {
            values[0] = 1.0;
            let mut column = 1;
            for (k, original) in design[row].iter().enumerate().skip(1) {
                if k == j {
                    continue;
                }
                values[column] = *original;
                column += 1;
            }
        }
        // The solver consumes the design, so the original columns are kept.
        let original = auxiliary.clone();
        let Ok(coefficients) = least_squares(&mut auxiliary, &response) else {
            factors.push(f64::INFINITY);
            continue;
        };
        let residual: f64 = response
            .iter()
            .enumerate()
            .map(|(row, value)| {
                let fitted = original[row]
                    .iter()
                    .zip(&coefficients)
                    .map(|(value, coefficient)| value * coefficient)
                    .sum::<f64>();
                (value - fitted).powi(2)
            })
            .sum();
        let r_squared = (1.0 - residual / total).clamp(0.0, 1.0);
        if r_squared >= 1.0 - 1e-12 {
            factors.push(f64::INFINITY);
        } else {
            factors.push(1.0 / (1.0 - r_squared));
        }
    }
    Ok(factors)
}

/// Fits the model, builds the Session report and a residual-versus-fitted plot.
pub fn analyze(
    df: &DataFrame,
    response: &str,
    predictors: &[String],
    confidence: f64,
) -> Result<AnalysisOutput> {
    let model = fit(df, response, predictors, confidence)?;
    let result = report(response, &model);
    let plot = residual_plot(response, &model);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn equation(model: &RegressionFit) -> String {
    let mut text = format!("y = {:.6}", model.coefficients[0]);
    for (name, coefficient) in model.names[1..].iter().zip(&model.coefficients[1..]) {
        if *coefficient < 0.0 {
            text.push_str(&format!(" − {:.6}·{name}", -coefficient));
        } else {
            text.push_str(&format!(" + {:.6}·{name}", coefficient));
        }
    }
    text
}

pub(crate) fn report(response: &str, model: &RegressionFit) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("Regression — response {response}"),
        format!(
            "Ordinary least squares with intercept; {:.1}% confidence",
            model.confidence * 100.0
        ),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("R²", model.r_squared),
        ("Adjusted R²", model.adjusted_r_squared),
        ("Predicted R²", model.predicted_r_squared),
        ("Residual standard error", model.s),
        ("F", model.f),
        ("F p-value", model.f_p_value),
        ("Regression df", model.df_regression),
        ("Residual df", model.df_residual),
    ];
    result.extra.push(Item::Message(format!(
        "Regression equation: {}",
        equation(model)
    )));
    result.extra.push(Item::Table {
        headers: vec![
            "Predictor".to_owned(),
            "Coef".to_owned(),
            "SE Coef".to_owned(),
            "T".to_owned(),
            "P".to_owned(),
            format!("{:.1}% CI low", model.confidence * 100.0),
            format!("{:.1}% CI high", model.confidence * 100.0),
        ],
        rows: model
            .names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                vec![
                    name.clone(),
                    format!("{:.6}", model.coefficients[index]),
                    format!("{:.6}", model.standard_errors[index]),
                    format!("{:.6}", model.t_values[index]),
                    format!("{:.6}", model.p_values[index]),
                    format!("{:.6}", model.ci_lower[index]),
                    format!("{:.6}", model.ci_upper[index]),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Source".to_owned(),
            "DF".to_owned(),
            "SS".to_owned(),
            "MS".to_owned(),
            "F".to_owned(),
            "P".to_owned(),
        ],
        rows: vec![
            vec![
                "Regression".to_owned(),
                format!("{:.0}", model.df_regression),
                format!("{:.6}", model.regression_ss),
                format!("{:.6}", model.regression_ss / model.df_regression),
                format!("{:.6}", model.f),
                format!("{:.6}", model.f_p_value),
            ],
            vec![
                "Residual error".to_owned(),
                format!("{:.0}", model.df_residual),
                format!("{:.6}", model.residual_ss),
                format!("{:.6}", model.s.powi(2)),
                String::new(),
                String::new(),
            ],
            vec![
                "Total".to_owned(),
                format!("{:.0}", model.df_regression + model.df_residual),
                format!("{:.6}", model.total_ss),
                String::new(),
                String::new(),
                String::new(),
            ],
        ],
    });
    result.extra.push(Item::Table {
        headers: vec!["Diagnostic".to_owned(), "Value".to_owned()],
        rows: vec![
            vec![
                "Durbin-Watson".to_owned(),
                format!("{:.6}", model.durbin_watson),
            ],
            vec!["VIF".to_owned(), String::new()],
        ]
        .into_iter()
        .chain(
            model
                .names
                .iter()
                .skip(1)
                .zip(&model.variance_inflation)
                .map(|(name, vif)| vec![format!("VIF ({name})"), format!("{vif:.6}")]),
        )
        .collect(),
    });
    // The documented behaviour's "Fits and Diagnostics for Unusual Observations":
    // a large residual (|standardised residual| > 2), an unusual x
    // (leverage > 3p/n) or an influential point (Cook's D > 1).
    let terms = model.names.len() as f64;
    let unusual: Vec<(usize, f64)> = (0..model.n)
        .filter_map(|index| {
            let h = model.leverage[index];
            let standardized = if h < 1.0 {
                model.residuals[index] / (model.s * (1.0 - h).sqrt())
            } else {
                f64::INFINITY
            };
            let mut flags = String::new();
            if standardized.abs() > 2.0 {
                flags.push('R');
            }
            if h > 3.0 * terms / model.n as f64 {
                flags.push('X');
            }
            if model.cooks_distance[index] > 1.0 {
                flags.push('D');
            }
            if flags.is_empty() {
                None
            } else {
                Some((index, standardized))
            }
        })
        .collect();
    if !unusual.is_empty() {
        result.extra.push(Item::Table {
            headers: vec![
                "Obs".to_owned(),
                "Response".to_owned(),
                "Fit".to_owned(),
                "Residual".to_owned(),
                "Std residual".to_owned(),
                "Flag".to_owned(),
            ],
            rows: unusual
                .iter()
                .map(|(index, standardized)| {
                    let residual = model.residuals[*index];
                    let h = model.leverage[*index];
                    let mut flags = String::new();
                    if standardized.abs() > 2.0 {
                        flags.push('R');
                    }
                    if h > 3.0 * terms / model.n as f64 {
                        flags.push('X');
                    }
                    if model.cooks_distance[*index] > 1.0 {
                        flags.push('D');
                    }
                    vec![
                        (index + 1).to_string(),
                        format!("{:.6}", model.fitted[*index] + residual),
                        format!("{:.6}", model.fitted[*index]),
                        format!("{residual:.6}"),
                        format!("{standardized:.6}"),
                        flags,
                    ]
                })
                .collect(),
        });
        result.extra.push(Item::Message(
            "R large residual; X unusual x; D influential (Cook's D > 1)".to_owned(),
        ));
    }
    // The influential observations, if any.
    let influential: Vec<(usize, f64, f64)> = model
        .cooks_distance
        .iter()
        .enumerate()
        .filter(|(_, distance)| **distance > 1.0)
        .map(|(index, distance)| (index + 1, *distance, model.leverage[index]))
        .collect();
    if !influential.is_empty() {
        result.extra.push(Item::Table {
            headers: vec![
                "Observation".to_owned(),
                "Cook's D".to_owned(),
                "Leverage".to_owned(),
            ],
            rows: influential
                .iter()
                .map(|(index, distance, leverage)| {
                    vec![
                        index.to_string(),
                        format!("{distance:.6}"),
                        format!("{leverage:.6}"),
                    ]
                })
                .collect(),
        });
    }
    result.notes = vec![
        (
            "Model".to_owned(),
            "y = β₀ + β₁x₁ + … + βₖxₖ + ε; numeric predictors enter directly and categorical predictors use dummy coding with the first level as reference".to_owned(),
        ),
        (
            "Influence".to_owned(),
            "the leverage of an observation is the weight of its row in the fit; Cook's \
             distance combines it with the residual and the observations above one are listed"
                .to_owned(),
        ),
        (
            "Diagnostics".to_owned(),
            "the Durbin-Watson statistic measures the autocorrelation of the residuals in row \
             order (2 means no autocorrelation); the variance inflation factor of a term is \
             1/(1 − R²) of its auxiliary regression and grows with collinearity"
                .to_owned(),
        ),
        (
            "Missing data".to_owned(),
            "rows with a missing response or predictor are removed listwise and counted".to_owned(),
        ),
        (
            "Inference".to_owned(),
            "coefficient tests use Student t with n − p df; the model F test uses its own df"
                .to_owned(),
        ),
        (
            "Plot".to_owned(),
            "residuals versus fitted values; a horizontal line marks zero".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    result.warnings.push(
        "coefficients, the ANOVA table, predicted R² and the unusual-observation table match the documented report (n = 16); other conventions follow standard OLS"
            .to_owned(),
    );
    result
}

pub(crate) fn residual_plot(response: &str, model: &RegressionFit) -> PlotSpec {
    let x_min = model.fitted.iter().copied().fold(f64::INFINITY, f64::min);
    let x_max = model
        .fitted
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let x_pad = ((x_max - x_min).abs() * 0.05).max(1e-9);
    let y_min = model.residuals.iter().copied().fold(0.0, f64::min);
    let y_max = model.residuals.iter().copied().fold(0.0, f64::max);
    let y_pad = ((y_max - y_min).abs() * 0.05).max(1e-9);
    PlotSpec {
        title: format!("Residuals vs Fitted — {response}"),
        x_axis: linear_axis("Fitted value", x_min - x_pad, x_max + x_pad),
        y_axis: linear_axis("Residual", y_min - y_pad, y_max + y_pad),
        size: (900, 520),
        points: model
            .fitted
            .iter()
            .zip(&model.residuals)
            .map(|(&x, &y)| Point { x, y })
            .collect(),
        lines: vec![Segment {
            from: Point {
                x: x_min - x_pad,
                y: 0.0,
            },
            to: Point {
                x: x_max + x_pad,
                y: 0.0,
            },
            width: 1.5,
        }],
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
    #[test]
    fn durbin_watson_and_vif_are_computed() {
        // A perfect fit leaves no residual, so the statistic is undefined; use
        // a noisy but deterministic series instead.
        let n = 12;
        let values: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let noise: Vec<f64> = (0..n).map(|i| ((i * 7) % 5) as f64 * 0.1).collect();
        let responses: Vec<f64> = values
            .iter()
            .zip(&noise)
            .map(|(x, e)| 2.0 * x + e)
            .collect();
        let df = DataFrame::new(vec![
            Column::new_f64("y", responses),
            Column::new_f64("x", values),
        ])
        .expect("frame");
        let model = fit(&df, "y", &["x".to_owned()], 0.95).expect("fit");
        assert!(model.durbin_watson.is_finite(), "{}", model.durbin_watson);
        assert!(model.durbin_watson > 0.0);
        // A single predictor cannot be collinear: the VIF is one.
        assert_eq!(model.variance_inflation.len(), 1);
        assert!(
            (model.variance_inflation[0] - 1.0).abs() < 1e-9,
            "vif {}",
            model.variance_inflation[0]
        );
    }

    #[test]
    fn the_leverage_and_cooks_distance_flag_an_outlier() {
        // A single wild observation among a regular series.
        let n = 20;
        let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let y: Vec<f64> = x.iter().map(|v| 2.0 * v + 1.0).collect();
        let mut y = y;
        y[10] = 500.0;
        let df = DataFrame::new(vec![
            Column::new_f64("y", y.clone()),
            Column::new_f64("x", x),
        ])
        .expect("frame");
        let model = fit(&df, "y", &["x".to_owned()], 0.95).expect("fit");
        assert_eq!(model.leverage.len(), n);
        assert_eq!(model.cooks_distance.len(), n);
        // The wild observation dominates the fit: its distance is far above
        // the others (the classic threshold of one is not reached with twenty
        // observations, because the wild point inflates the mean square).
        let others: Vec<f64> = model
            .cooks_distance
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != 10)
            .map(|(_, distance)| *distance)
            .collect();
        let median = {
            let mut sorted = others.clone();
            sorted.sort_by(|a, b| a.total_cmp(b));
            sorted[sorted.len() / 2]
        };
        assert!(
            model.cooks_distance[10] > 10.0 * median.max(1e-12),
            "distance {} against {median}",
            model.cooks_distance[10]
        );
        assert!(others.iter().all(|distance| *distance < 1.0), "{others:?}");
        // The leverage of a regular series is the same everywhere.
        assert!((model.leverage[0] - model.leverage[19]).abs() < 1e-12);
    }

    #[test]
    fn the_vif_grows_with_collinearity() {
        let n = 20;
        let x1: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let x2: Vec<f64> = x1.iter().map(|value| value * 2.0 + 1.0).collect();
        let responses: Vec<f64> = x1.iter().map(|value| 3.0 * value + 0.5).collect();
        let df = DataFrame::new(vec![
            Column::new_f64("y", responses.clone()),
            Column::new_f64("x1", x1.clone()),
            Column::new_f64("x2", x2),
        ])
        .expect("frame");
        // The two predictors are collinear, so the fit is refused.
        assert!(matches!(
            fit(&df, "y", &["x1".to_owned(), "x2".to_owned()], 0.95),
            Err(Error::Unsupported { .. })
        ));

        // Nearly collinear predictors are accepted, but the factor is large.
        let x2: Vec<f64> = (0..n)
            .map(|i| i as f64 * 2.0 + 1.0 + ((i % 5) as f64 - 2.0) * 0.05)
            .collect();
        let df = DataFrame::new(vec![
            Column::new_f64("y", responses.clone()),
            Column::new_f64("x1", x1.clone()),
            Column::new_f64("x2", x2),
        ])
        .expect("frame");
        let model = fit(&df, "y", &["x1".to_owned(), "x2".to_owned()], 0.95).expect("fit");
        assert_eq!(model.variance_inflation.len(), 2);
        for vif in &model.variance_inflation {
            assert!(*vif > 5.0, "vif {vif}");
        }
    }

    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("y", vec![2.0, 4.0, 5.0, 4.0, 5.0]),
        ])
        .unwrap()
    }

    #[test]
    fn simple_regression_matches_hand_values() {
        let df = frame();
        let model = fit(&df, "y", &["x".to_owned()], 0.95).expect("fit");
        assert!((model.coefficients[0] - 2.2).abs() < 1e-12);
        assert!((model.coefficients[1] - 0.6).abs() < 1e-12);
        assert!((model.r_squared - 0.6).abs() < 1e-12);
        assert!((model.s - 0.8f64.sqrt()).abs() < 1e-12);
        assert!((model.standard_errors[1] - (0.8f64.sqrt() / 10f64.sqrt())).abs() < 1e-12);
        assert!((model.t_values[1] - 0.6 / (0.8f64.sqrt() / 10f64.sqrt())).abs() < 1e-12);
        assert_eq!(model.n, 5);
    }

    #[test]
    fn multiple_regression_recovers_an_exact_plane() {
        let df = DataFrame::new(vec![
            Column::new_f64("x1", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_f64("x2", vec![2.0, 1.0, 4.0, 3.0, 6.0, 5.0]),
            Column::new_f64("y", vec![6.0, 5.0, 12.0, 11.0, 18.0, 17.0]),
        ])
        .unwrap();
        let model = fit(&df, "y", &["x1".to_owned(), "x2".to_owned()], 0.95).expect("fit");
        assert!((model.coefficients[0] - 1.0).abs() < 1e-10);
        assert!((model.coefficients[1] - 1.0).abs() < 1e-10);
        assert!((model.coefficients[2] - 2.0).abs() < 1e-10);
        assert!(model.s < 1e-10);
    }

    #[test]
    fn categorical_predictor_uses_dummy_coding() {
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_text(
                "group",
                vec![
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                ],
            ),
            Column::new_f64("y", vec![4.0, 6.0, 8.0, 14.0, 16.0, 18.0]),
        ])
        .unwrap();
        let model = fit(&df, "y", &["x".to_owned(), "group".to_owned()], 0.95).expect("fit");
        let index = |name: &str| model.names.iter().position(|term| term == name).unwrap();
        assert!((model.coefficients[index("x")] - 2.0).abs() < 1e-10);
        assert!((model.coefficients[index("group[B]")] - 4.0).abs() < 1e-10);
        assert!((model.coefficients[0] - 2.0).abs() < 1e-10);
    }

    #[test]
    fn missing_rows_are_removed_listwise() {
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, f64::NAN, 4.0, 5.0]),
            Column::new_f64("y", vec![2.0, 4.0, 5.0, 4.0, 5.0]),
        ])
        .unwrap();
        let model = fit(&df, "y", &["x".to_owned()], 0.95).expect("fit");
        assert_eq!(model.n, 4);
        assert_eq!(model.excluded, 1);
    }

    #[test]
    fn collinear_predictors_are_rejected() {
        let df = DataFrame::new(vec![
            Column::new_f64("x1", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_f64("x2", vec![2.0, 4.0, 6.0, 8.0]),
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 4.0]),
        ])
        .unwrap();
        assert!(matches!(
            fit(&df, "y", &["x1".to_owned(), "x2".to_owned()], 0.95),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn report_and_residual_plot_are_built() {
        let df = frame();
        let out = analyze(&df, "y", &["x".to_owned()], 0.95).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Regression equation"), "{text}");
        assert!(text.contains("Residual standard error"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.points.len(), 5);
        assert_eq!(plot.lines.len(), 1);
    }
}

#[cfg(test)]
mod unusual_observations {
    use super::*;
    use crate::data::Column;

    fn straight(n: usize, outlier: Option<(usize, f64)>) -> DataFrame {
        let x: Vec<f64> = (0..n).map(|i| 1.0 + i as f64).collect();
        let mut y: Vec<f64> = x
            .iter()
            .enumerate()
            .map(|(index, v)| 3.0 + 1.5 * v + ((index * 7) % 5) as f64 * 0.05)
            .collect();
        if let Some((index, value)) = outlier {
            y[index] = value;
        }
        DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame")
    }

    #[test]
    fn predicted_r_squared_uses_the_press_statistic() {
        let df = straight(16, None);
        let model = fit(&df, "y", &["x".to_owned()], 0.95).expect("fit");
        let press: f64 = model
            .residuals
            .iter()
            .zip(&model.leverage)
            .map(|(residual, h)| (residual / (1.0 - h)).powi(2))
            .sum();
        assert!((model.predicted_r_squared - (1.0 - press / model.total_ss)).abs() < 1e-12);
        assert!(model.predicted_r_squared <= model.r_squared);

        let df = straight(16, Some((14, 13.74)));
        let model = fit(&df, "y", &["x".to_owned()], 0.95).expect("fit");
        let press: f64 = model
            .residuals
            .iter()
            .zip(&model.leverage)
            .map(|(residual, h)| (residual / (1.0 - h)).powi(2))
            .sum();
        let expected = 1.0 - press / model.total_ss;
        assert!((model.predicted_r_squared - expected).abs() < 1e-12);
        assert!(model.predicted_r_squared < model.r_squared);
    }

    #[test]
    fn a_large_residual_is_listed_with_the_r_flag() {
        let df = straight(16, Some((14, 13.74)));
        let out = analyze(&df, "y", &["x".to_owned()], 0.95).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Std residual"), "{text}");
        assert!(text.contains("R large residual"), "{text}");
        let row = text
            .lines()
            .find(|line| line.trim_start().starts_with("15 "))
            .unwrap_or_else(|| panic!("observation 15 not listed:\n{text}"));
        assert!(row.contains('R'), "{row}");
    }

    #[test]
    fn a_clean_fit_has_no_unusual_observation_table() {
        let df = straight(16, None);
        let out = analyze(&df, "y", &["x".to_owned()], 0.95).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(!text.contains("Std residual"), "{text}");
        assert!(!text.contains("R large residual"), "{text}");
    }

    #[test]
    fn a_constant_response_is_an_error() {
        let x: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let df = DataFrame::new(vec![
            Column::new_f64("x", x),
            Column::new_f64("y", vec![2.0; 10]),
        ])
        .expect("frame");
        assert!(fit(&df, "y", &["x".to_owned()], 0.95).is_err());
    }

    fn prediction_frame() -> DataFrame {
        let x: Vec<f64> = (0..10).map(|index| (index + 1) as f64).collect();
        let noise = [0.1, -0.1, 0.05, -0.05, 0.1, -0.1, 0.05, -0.05, 0.1, -0.1];
        let y: Vec<f64> = x
            .iter()
            .zip(noise)
            .map(|(value, error)| 10.0 + 2.0 * value + error)
            .collect();
        DataFrame::new(vec![Column::new_f64("y", y), Column::new_f64("x", x)]).expect("frame")
    }

    #[test]
    fn the_prediction_uses_the_equation_and_both_intervals() {
        let df = prediction_frame();
        let model = fit(&df, "y", &["x".to_owned()], 0.95).expect("fit");
        let result = predict_points(&df, "y", &["x".to_owned()], 0.95, &[vec![5.0], vec![12.0]])
            .expect("prediction");
        assert!(result.title.contains("Prediction"), "{}", result.title);
        assert_eq!(result.n, 10);
        let (headers, rows) = result
            .extra
            .iter()
            .find_map(|item| match item {
                Item::Table { headers, rows } => Some((headers, rows)),
                _ => None,
            })
            .expect("table");
        assert_eq!(
            headers,
            &vec![
                "Point".to_owned(),
                "x".to_owned(),
                "Fit".to_owned(),
                "SE fit".to_owned(),
                "CI lower".to_owned(),
                "CI upper".to_owned(),
                "PI lower".to_owned(),
                "PI upper".to_owned(),
            ]
        );
        assert_eq!(rows.len(), 2);
        let row = &rows[0];
        let fitted: f64 = row[2].parse().expect("fit");
        let expected = model.coefficients[0] + model.coefficients[1] * 5.0;
        assert!((fitted - expected).abs() < 1e-6, "{fitted} vs {expected}");
        let ci_low: f64 = row[4].parse().expect("ci low");
        let ci_high: f64 = row[5].parse().expect("ci high");
        let pi_low: f64 = row[6].parse().expect("pi low");
        let pi_high: f64 = row[7].parse().expect("pi high");
        assert!(ci_low < fitted && fitted < ci_high);
        // The prediction interval is wider than the confidence interval.
        assert!(pi_low < ci_low && ci_high < pi_high);
        // Extrapolation still returns a finite fit.
        let far: f64 = rows[1][2].parse().expect("far fit");
        assert!(far.is_finite());
    }

    #[test]
    fn the_prediction_rejects_bad_points_and_non_numeric_predictors() {
        let df = prediction_frame();
        let error = predict_points(&df, "y", &["x".to_owned()], 0.95, &[]).expect_err("no points");
        assert!(
            error.to_string().contains("at least one new point"),
            "{error}"
        );
        let error = predict_points(&df, "y", &["x".to_owned()], 0.95, &[vec![1.0, 2.0]])
            .expect_err("too many values");
        assert!(error.to_string().contains("value(s)"), "{error}");
        let error = predict_points(&df, "y", &["x".to_owned()], 0.95, &[vec![f64::NAN]])
            .expect_err("non-finite");
        assert!(error.to_string().contains("non-finite"), "{error}");
        let categorical = DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_text(
                "g",
                vec!["a", "a", "a", "b", "b", "b"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            ),
        ])
        .expect("frame");
        let error = predict_points(&categorical, "y", &["g".to_owned()], 0.95, &[vec![1.0]])
            .expect_err("categorical predictor");
        assert!(error.to_string().contains("numeric predictors"), "{error}");
    }

    #[test]
    fn the_prediction_counts_the_listwise_excluded_rows() {
        let mut df = prediction_frame();
        // A missing predictor keeps the row out of the fit and the design.
        df.insert_column(
            df.n_columns(),
            Column::new_f64(
                "z",
                vec![1.5, 4.2, 5.8, f64::NAN, 10.1, 12.4, 13.8, 16.2, 17.9, 20.3],
            ),
        )
        .expect("column");
        let result = predict_points(
            &df,
            "y",
            &["x".to_owned(), "z".to_owned()],
            0.95,
            &[vec![2.0, 2.0]],
        )
        .expect("prediction");
        assert_eq!(result.n, 9);
        assert_eq!(result.excluded, 1);
    }
}
