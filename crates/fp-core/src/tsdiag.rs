// SPDX-License-Identifier: MIT
//! Time-series diagnostics: autocorrelation, cross-correlation, unit-root
//! test, differences/lags and trend models.
//!
//! The series is read in row order. Functions that only read a series drop a
//! missing value and report how many rows were excluded; the differences and
//! lags keep the row alignment and leave a missing value where the
//! observation does not exist.
//!
//! - **Autocorrelation**: `r_k = Σ (y_t − ȳ)(y_{t−k} − ȳ) / Σ (y_t − ȳ)²`
//!   for lags `1…m`, the partial autocorrelation from the Durbin–Levinson
//!   recursion and the Ljung–Box statistic
//!   `Q_k = n(n+2)·Σ_{j≤k} r_j²/(n−j)` with `k` degrees of freedom.
//! - **Cross-correlation**: Pearson correlation of `x_t` with `y_{t+k}` over
//!   the overlapping pairs, for `k = −m…m`.
//! - **Unit root (ADF)**: the augmented Dickey–Fuller regression
//!   `Δy_t = μ + βt + γ·y_{t−1} + Σ δᵢ·Δy_{t−i} + ε_t`; the test statistic is
//!   the t ratio of `γ` against the asymptotic critical values of the τ
//!   distribution (MacKinnon). The p-value interpolates those three points and
//!   is a documented approximation, not an exact quantile of τ.
//! - **Differences and lags**: the `d`-th difference at lag `k`, or the series
//!   lagged by `k`, as a new column with a missing value where the observation
//!   does not exist.
//! - **Trend**: linear, quadratic, exponential (`y = b₀·exp(b₁t)`) and growth
//!   (`y = exp(b₀ + b₁/t)`) models, fitted by least squares, with the
//!   equation, the tests, the fitted values and the residuals.
//!
//! The critical values of the unit-root test and the bands of the
//! correlograms are the usual large-sample values; no documented comparison
//! has been made.

use crate::data::{Column, ColumnData, DataFrame, MissingPolicy};
use crate::error::{Error, Result};
use crate::plot::{
    AxisSpec, Band, PlotNote, PlotSpec, Point, Scale, Segment, Series, Tick, linear_axis,
    panel_plot,
};
use crate::regression;
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::chi_square_cdf;
use crate::workflow::AnalysisOutput;

/// One lag of the autocorrelation function.
#[derive(Debug, Clone, PartialEq)]
pub struct AcfLag {
    /// Lag in observations.
    pub lag: usize,
    /// Autocorrelation at the lag.
    pub acf: f64,
    /// Partial autocorrelation at the lag.
    pub pacf: f64,
    /// Ljung–Box statistic up to this lag.
    pub q: f64,
    /// p-value of the Ljung–Box statistic.
    pub p: f64,
}

/// Autocorrelation of a series.
#[derive(Debug, Clone, PartialEq)]
pub struct AcfFit {
    /// One entry per lag, from 1 to the requested maximum.
    pub lags: Vec<AcfLag>,
    /// Observations in the series.
    pub n: usize,
    /// Series mean.
    pub mean: f64,
    /// Series standard deviation (`n − 1`).
    pub sd: f64,
    /// White-noise band of the correlogram (`1.96/√n`).
    pub band: f64,
    /// Rows excluded for a missing value.
    pub excluded: usize,
}

/// One lag of the cross-correlation function.
#[derive(Debug, Clone, PartialEq)]
pub struct CcfLag {
    /// Lag: a positive `k` pairs `x_t` with `y_{t+k}`.
    pub lag: i64,
    /// Correlation over the overlapping pairs.
    pub r: f64,
    /// Number of overlapping pairs.
    pub pairs: usize,
}

/// Cross-correlation of two series.
#[derive(Debug, Clone, PartialEq)]
pub struct CcfFit {
    /// One entry per lag, from `−m` to `m`.
    pub lags: Vec<CcfLag>,
    /// Pairs before the lag windows are applied.
    pub n: usize,
    /// White-noise band (`1.96/√n`).
    pub band: f64,
    /// Rows excluded for a missing value.
    pub excluded: usize,
}

/// Deterministic part of the unit-root regression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdfModel {
    /// No constant and no trend.
    None,
    /// Constant only (the usual case).
    #[default]
    Constant,
    /// Constant and linear trend.
    Trend,
}

impl AdfModel {
    /// Name used by the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Constant => "constant",
            Self::Trend => "constant and trend",
        }
    }

    /// Asymptotic critical values at 1 %, 5 % and 10 % (MacKinnon).
    #[must_use]
    pub fn critical(self) -> [f64; 3] {
        match self {
            Self::None => [-2.5658, -1.9393, -1.6156],
            Self::Constant => [-3.4308, -2.8615, -2.5668],
            Self::Trend => [-3.9588, -3.4105, -3.1265],
        }
    }
}

/// Fitted augmented Dickey–Fuller test.
#[derive(Debug, Clone, PartialEq)]
pub struct AdfFit {
    /// Coefficient of the lagged level (`γ`).
    pub gamma: f64,
    /// Standard error of `γ`.
    pub se: f64,
    /// t ratio of `γ`, the test statistic.
    pub t: f64,
    /// Residual degrees of freedom.
    pub df: usize,
    /// Observations in the series.
    pub n: usize,
    /// Rows of the regression.
    pub observations: usize,
    /// Augmentation lags.
    pub lags: usize,
    /// Deterministic part.
    pub model: AdfModel,
    /// Critical values at 1 %, 5 % and 10 %.
    pub critical: [f64; 3],
    /// Interpolated p-value.
    pub p: f64,
    /// Whether the unit root is rejected at 5 %.
    pub reject: bool,
    /// Rows excluded for a missing value.
    pub excluded: usize,
}

/// Trend model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendModel {
    /// `y = b₀ + b₁·t`.
    Linear,
    /// `y = b₀ + b₁·t + b₂·t²`.
    Quadratic,
    /// `y = b₀·exp(b₁·t)`, fitted on `ln y`.
    Exponential,
    /// `y = exp(b₀ + b₁/t)`, fitted on `ln y`.
    Growth,
}

impl TrendModel {
    /// Name used by the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Quadratic => "quadratic",
            Self::Exponential => "exponential",
            Self::Growth => "growth",
        }
    }
}

/// Fitted trend model.
#[derive(Debug, Clone, PartialEq)]
pub struct TrendFit {
    /// Model.
    pub model: TrendModel,
    /// Term names of the fitted regression (`Constant`, `t`, `t²`, `1/t`).
    pub names: Vec<String>,
    /// Coefficients: on the data scale for the linear and quadratic models,
    /// on `ln y` for the exponential and growth models.
    pub coefficients: Vec<f64>,
    /// Standard errors of the fitted regression.
    pub se: Vec<f64>,
    /// t statistics of the fitted regression.
    pub t: Vec<f64>,
    /// p-values of the fitted regression.
    pub p: Vec<f64>,
    /// Coefficient of determination on the original scale.
    pub r_squared: f64,
    /// Residual standard error on the original scale.
    pub s: f64,
    /// Fitted values on the original scale.
    pub fitted: Vec<f64>,
    /// Residuals on the original scale.
    pub residuals: Vec<f64>,
    /// Fitted equation on the original scale.
    pub equation: String,
    /// Observations used.
    pub n: usize,
    /// Rows excluded for a missing value.
    pub excluded: usize,
}

/// Operation that writes a new column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnOperation {
    /// Difference of the given order at the given lag.
    Difference {
        /// Order, 1 or 2.
        order: usize,
        /// Lag, at least 1.
        lag: usize,
    },
    /// Series lagged by the given number of observations.
    Lag {
        /// Lag, at least 1.
        lag: usize,
    },
}

impl ColumnOperation {
    /// Default name of the new column for `column`.
    #[must_use]
    pub fn column_name(self, column: &str) -> String {
        match self {
            Self::Difference { order, lag } => format!("D{order}({column},{lag})"),
            Self::Lag { lag } => format!("{column}[{lag}]"),
        }
    }
}

/// Reads the finite values of a numeric column.
fn series(df: &DataFrame, column: &str) -> Result<(Vec<f64>, usize)> {
    let sample = df.column(column)?.sample(MissingPolicy::Drop)?;
    Ok((sample.values().to_vec(), sample.excluded()))
}

fn validate_max_lag(n: usize, max_lag: usize) -> Result<()> {
    if max_lag == 0 || max_lag > n.saturating_sub(2) {
        return Err(Error::InvalidParameter {
            name: "number of lags",
            value: max_lag as f64,
        });
    }
    Ok(())
}

/// Autocorrelation and partial autocorrelation of a series.
///
/// # Errors
///
/// Returns [`Error::SampleTooSmall`] for fewer than four observations,
/// [`Error::InvalidParameter`] when the maximum lag is zero or leaves fewer
/// than two observations, and [`Error::ConstantSample`] for a constant series.
pub fn autocorrelation(values: &[f64], max_lag: usize) -> Result<AcfFit> {
    let n = values.len();
    if n < 4 {
        return Err(Error::SampleTooSmall { n, min: 4 });
    }
    validate_max_lag(n, max_lag)?;
    let mean = values.iter().sum::<f64>() / n as f64;
    let deviation: Vec<f64> = values.iter().map(|value| value - mean).collect();
    let ss: f64 = deviation.iter().map(|value| value * value).sum();
    if ss <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let sd = (ss / (n - 1) as f64).sqrt();
    let acf: Vec<f64> = (1..=max_lag)
        .map(|lag| {
            deviation
                .iter()
                .skip(lag)
                .zip(&deviation)
                .map(|(current, previous)| current * previous)
                .sum::<f64>()
                / ss
        })
        .collect();
    // Durbin–Levinson recursion; `phi[k]` holds the coefficients of order k.
    let mut history: Vec<Vec<f64>> = Vec::with_capacity(max_lag);
    for k in 1..=max_lag {
        let mut phi = vec![0.0; k + 1];
        let numerator = if k == 1 {
            acf[0]
        } else {
            let previous = &history[k - 2];
            let running: f64 = (1..k).map(|j| previous[j] * acf[k - j - 1]).sum();
            acf[k - 1] - running
        };
        let denominator = if k == 1 {
            1.0
        } else {
            let previous = &history[k - 2];
            let running: f64 = (1..k).map(|j| previous[j] * acf[j - 1]).sum();
            1.0 - running
        };
        phi[k] = if denominator.abs() < 1e-15 {
            0.0
        } else {
            numerator / denominator
        };
        if k > 1 {
            let previous = history[k - 2].clone();
            for j in 1..k {
                phi[j] = previous[j] - phi[k] * previous[k - j];
            }
        }
        history.push(phi);
    }
    let band = 1.96 / (n as f64).sqrt();
    let mut lags = Vec::with_capacity(max_lag);
    let mut q = 0.0;
    for k in 1..=max_lag {
        q += acf[k - 1] * acf[k - 1] / (n - k) as f64;
        let statistic = n as f64 * (n as f64 + 2.0) * q;
        let p = (1.0 - chi_square_cdf(statistic, k as f64)).clamp(0.0, 1.0);
        lags.push(AcfLag {
            lag: k,
            acf: acf[k - 1],
            pacf: history[k - 1][k],
            q: statistic,
            p,
        });
    }
    Ok(AcfFit {
        lags,
        n,
        mean,
        sd,
        band,
        excluded: 0,
    })
}

/// Cross-correlation of two series at lags `−m…m`.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for series of different lengths,
/// [`Error::SampleTooSmall`] for fewer than four observations and
/// [`Error::InvalidParameter`] for a bad window.
pub fn cross_correlation(x: &[f64], y: &[f64], max_lag: usize) -> Result<CcfFit> {
    if x.len() != y.len() {
        return Err(Error::Unsupported(
            "the two series must have the same length".to_owned(),
        ));
    }
    let n = x.len();
    if n < 4 {
        return Err(Error::SampleTooSmall { n, min: 4 });
    }
    validate_max_lag(n, max_lag)?;
    let mut lags = Vec::with_capacity(2 * max_lag + 1);
    for lag in -(max_lag as i64)..=(max_lag as i64) {
        let mut found: Vec<(f64, f64)> = Vec::new();
        for (index, value) in x.iter().enumerate() {
            let other = index as i64 + lag;
            if other < 0 || other as usize >= n {
                continue;
            }
            found.push((*value, y[other as usize]));
        }
        let r = if found.len() < 3 {
            f64::NAN
        } else {
            let mean_x = found.iter().map(|(left, _)| left).sum::<f64>() / found.len() as f64;
            let mean_y = found.iter().map(|(_, right)| right).sum::<f64>() / found.len() as f64;
            let mut sxy = 0.0;
            let mut sxx = 0.0;
            let mut syy = 0.0;
            for (left, right) in &found {
                sxy += (left - mean_x) * (right - mean_y);
                sxx += (left - mean_x).powi(2);
                syy += (right - mean_y).powi(2);
            }
            if sxx <= 0.0 || syy <= 0.0 {
                f64::NAN
            } else {
                sxy / (sxx * syy).sqrt()
            }
        };
        lags.push(CcfLag {
            lag,
            r,
            pairs: found.len(),
        });
    }
    Ok(CcfFit {
        lags,
        n,
        band: 1.96 / (n as f64).sqrt(),
        excluded: 0,
    })
}

/// Interpolated p-value of the Dickey–Fuller τ statistic.
fn adf_p_value(t: f64, critical: [f64; 3]) -> f64 {
    let [one, five, ten] = critical;
    if t <= one {
        (0.01 * (1.0 - (one - t).clamp(0.0, 1.0))).max(0.001)
    } else if t <= five {
        0.01 + 0.04 * (t - one) / (five - one)
    } else if t <= ten {
        0.05 + 0.05 * (t - five) / (ten - five)
    } else {
        (0.10 + 0.85 * ((t - ten) / 2.0).clamp(0.0, 1.0)).min(0.999)
    }
}

/// Inverts a square matrix in place (Gauss–Jordan with partial pivoting).
#[allow(clippy::needless_range_loop)]
fn invert(matrix: &mut [Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = matrix.len();
    let mut inverse = vec![vec![0.0; n]; n];
    for (index, row) in inverse.iter_mut().enumerate() {
        row[index] = 1.0;
    }
    for column in 0..n {
        let mut pivot = column;
        for row in (column + 1)..n {
            if matrix[row][column].abs() > matrix[pivot][column].abs() {
                pivot = row;
            }
        }
        if matrix[pivot][column].abs() < 1e-12 {
            return None;
        }
        matrix.swap(column, pivot);
        inverse.swap(column, pivot);
        let divisor = matrix[column][column];
        for value in matrix[column].iter_mut().skip(column) {
            *value /= divisor;
        }
        for value in inverse[column].iter_mut() {
            *value /= divisor;
        }
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
            for index in 0..n {
                inverse[row][index] -= factor * inverse[column][index];
            }
        }
    }
    Some(inverse)
}

/// Ordinary least squares of `design·β = y`, with the tests of every
/// coefficient.
#[allow(clippy::needless_range_loop)]
struct Ols {
    coefficients: Vec<f64>,
    standard_errors: Vec<f64>,
    t_values: Vec<f64>,
}

fn ols(design: &[Vec<f64>], y: &[f64]) -> Result<Ols> {
    let p = design.first().map(Vec::len).unwrap_or(0);
    let n = design.len();
    if p == 0 || n <= p {
        return Err(Error::SampleTooSmall { n, min: p + 1 });
    }
    let mut xtx = vec![vec![0.0; p]; p];
    let mut xty = vec![0.0; p];
    for (index, row) in design.iter().enumerate() {
        for i in 0..p {
            xty[i] += row[i] * y[index];
            for j in 0..p {
                xtx[i][j] += row[i] * row[j];
            }
        }
    }
    let mut normal = xtx;
    let inverse = invert(&mut normal)
        .ok_or_else(|| Error::Unsupported("the regressors are collinear or constant".to_owned()))?;
    let coefficients: Vec<f64> = (0..p)
        .map(|i| (0..p).map(|j| inverse[i][j] * xty[j]).sum())
        .collect();
    let mut residual_ss = 0.0;
    for (index, row) in design.iter().enumerate() {
        let fitted: f64 = row
            .iter()
            .zip(&coefficients)
            .map(|(value, coefficient)| value * coefficient)
            .sum();
        residual_ss += (y[index] - fitted).powi(2);
    }
    let df = n - p;
    let variance = residual_ss / df as f64;
    let standard_errors: Vec<f64> = (0..p)
        .map(|i| (variance * inverse[i][i]).max(0.0).sqrt())
        .collect();
    let t_values: Vec<f64> = coefficients
        .iter()
        .zip(&standard_errors)
        .map(|(coefficient, se)| {
            if se.is_finite() && *se > 0.0 {
                coefficient / se
            } else {
                f64::NAN
            }
        })
        .collect();
    Ok(Ols {
        coefficients,
        standard_errors,
        t_values,
    })
}

/// Augmented Dickey–Fuller test.
///
/// # Errors
///
/// Returns [`Error::SampleTooSmall`] when the regression has no residual
/// degrees of freedom, [`Error::Unsupported`] when the regressors are
/// collinear and [`Error::InvalidParameter`] for too many lags.
#[allow(clippy::needless_range_loop)]
pub fn adf(values: &[f64], lags: usize, model: AdfModel) -> Result<AdfFit> {
    let n = values.len();
    if n < 3 {
        return Err(Error::SampleTooSmall { n, min: 3 });
    }
    if lags + 2 >= n {
        return Err(Error::InvalidParameter {
            name: "number of lags",
            value: lags as f64,
        });
    }
    let deterministic = match model {
        AdfModel::None => 0,
        AdfModel::Constant => 1,
        AdfModel::Trend => 2,
    };
    let width = 1 + lags + deterministic;
    let first = lags + 1;
    let rows = n - first;
    if rows <= width {
        return Err(Error::SampleTooSmall {
            n,
            min: width + first + 1,
        });
    }
    let difference = |index: usize| values[index] - values[index - 1];
    let mut design = Vec::with_capacity(rows);
    let mut response = Vec::with_capacity(rows);
    for t in first..n {
        let mut row = Vec::with_capacity(width);
        row.push(values[t - 1]);
        for lag in 1..=lags {
            row.push(difference(t - lag));
        }
        if deterministic >= 1 {
            row.push(1.0);
        }
        if deterministic == 2 {
            row.push(t as f64);
        }
        design.push(row);
        response.push(difference(t));
    }
    let fit = ols(&design, &response)?;
    let gamma = fit.coefficients[0];
    let se = fit.standard_errors[0];
    let t = fit.t_values[0];
    let critical = model.critical();
    Ok(AdfFit {
        gamma,
        se,
        t,
        df: rows - width,
        n,
        observations: rows,
        lags,
        model,
        critical,
        p: adf_p_value(t, critical),
        reject: t < critical[1],
        excluded: 0,
    })
}

/// Differences or lags of a numeric column, as a new full-length series.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for a zero order or lag, an order above
/// two or a lag that leaves no observation, and [`Error::Unsupported`] when
/// the column is not numeric.
#[allow(clippy::needless_range_loop)]
pub fn transform_column(column: &Column, operation: ColumnOperation) -> Result<Vec<f64>> {
    if !matches!(column.data(), ColumnData::F64(_) | ColumnData::I64(_)) {
        return Err(Error::Unsupported("the column must be numeric".to_owned()));
    }
    let n = column.len();
    let mut values = vec![f64::NAN; n];
    for (row, value) in values.iter_mut().enumerate() {
        if let Some(found) = column.value(row) {
            *value = found;
        }
    }
    match operation {
        ColumnOperation::Difference { order, lag } => {
            if order == 0 || order > 2 {
                return Err(Error::InvalidParameter {
                    name: "difference order",
                    value: order as f64,
                });
            }
            if lag == 0 || lag * order >= n {
                return Err(Error::InvalidParameter {
                    name: "lag",
                    value: lag as f64,
                });
            }
            let mut current = values;
            for _ in 0..order {
                let mut next = vec![f64::NAN; n];
                for index in lag..n {
                    if current[index].is_finite() && current[index - lag].is_finite() {
                        next[index] = current[index] - current[index - lag];
                    }
                }
                current = next;
            }
            Ok(current)
        }
        ColumnOperation::Lag { lag } => {
            if lag == 0 || lag >= n {
                return Err(Error::InvalidParameter {
                    name: "lag",
                    value: lag as f64,
                });
            }
            let mut lagged = vec![f64::NAN; n];
            lagged[lag..].copy_from_slice(&values[..n - lag]);
            Ok(lagged)
        }
    }
}

/// Adds the differences or lags of a column to the worksheet.
///
/// # Errors
///
/// The errors of [`transform_column`] and of the worksheet insertion.
pub fn add_transformed_column(
    df: &mut DataFrame,
    column: &str,
    operation: ColumnOperation,
) -> Result<(String, usize, usize)> {
    let source = df.column(column)?.clone();
    let values = transform_column(&source, operation)?;
    let base = operation.column_name(column);
    let mut name = base.clone();
    let mut attempt = 1;
    while df.column(&name).is_ok() {
        attempt += 1;
        name = format!("{base}_{attempt}");
    }
    let created = Column::new_f64(name.clone(), values);
    let missing = (0..created.len())
        .filter(|row| created.is_missing(*row))
        .count();
    let present = created.len() - missing;
    df.insert_column(df.n_columns(), created)?;
    Ok((name, present, missing))
}

/// Autocorrelation report and the two correlograms.
///
/// # Errors
///
/// The errors of [`series`] and [`autocorrelation`].
pub fn autocorrelation_report(
    df: &DataFrame,
    column: &str,
    max_lag: usize,
) -> Result<AnalysisOutput> {
    let (values, excluded) = series(df, column)?;
    let mut model = autocorrelation(&values, max_lag)?;
    model.excluded = excluded;
    let mut result = AnalysisResult::new(
        format!("Autocorrelation — column {column}"),
        "Autocorrelations and partial autocorrelations of the series in row order",
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Observations", model.n as f64),
        ("Mean", model.mean),
        ("Standard deviation", model.sd),
        ("Lags", model.lags.len() as f64),
        ("White-noise band (1.96/√n)", model.band),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Lag".to_owned(),
            "ACF".to_owned(),
            "PACF".to_owned(),
            "Q (Ljung–Box)".to_owned(),
            "P".to_owned(),
        ],
        rows: model
            .lags
            .iter()
            .map(|lag| {
                vec![
                    lag.lag.to_string(),
                    format!("{:+.6}", lag.acf),
                    format!("{:+.6}", lag.pacf),
                    format!("{:.6}", lag.q),
                    format!("{:.6}", lag.p),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Autocorrelation".to_owned(),
            "r_k = Σ(y_t − ȳ)(y_{t−k} − ȳ) / Σ(y_t − ȳ)² over the observations that share both \
             positions"
                .to_owned(),
        ),
        (
            "Partial autocorrelation".to_owned(),
            "Durbin–Levinson recursion over the sample autocorrelations; the last coefficient of \
             each order is the partial autocorrelation"
                .to_owned(),
        ),
        (
            "Ljung–Box".to_owned(),
            "Q_k = n(n+2)·Σ r_j²/(n−j) with k degrees of freedom; a small p-value rejects the \
             absence of autocorrelation up to that lag"
                .to_owned(),
        ),
        (
            "Band".to_owned(),
            "the ±1.96/√n band is the large-sample white-noise band; it ignores the estimated \
             autocorrelations, so it is a guide and not an exact test"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    if model.lags.len() > model.n / 4 {
        result.warnings.push(
            "more than a quarter of the series in lags: the long-lag estimates use few \
             observations"
                .to_owned(),
        );
    }
    let acf_pairs: Vec<(f64, f64)> = model
        .lags
        .iter()
        .map(|lag| (lag.lag as f64, lag.acf))
        .collect();
    let pacf_pairs: Vec<(f64, f64)> = model
        .lags
        .iter()
        .map(|lag| (lag.lag as f64, lag.pacf))
        .collect();
    let plot = panel_plot(
        vec![
            correlogram("Autocorrelation", &acf_pairs, model.band, model.lags.len()),
            correlogram(
                "Partial Autocorrelation",
                &pacf_pairs,
                model.band,
                model.lags.len(),
            ),
        ],
        2,
        format!("Autocorrelation — {column}"),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot,
    })
}

/// Correlogram of `(lag, value)` pairs with the white-noise band.
fn correlogram(title: &str, values: &[(f64, f64)], band: f64, count: usize) -> PlotSpec {
    let top = values
        .iter()
        .map(|(_, value)| value.abs())
        .fold(band * 1.2, f64::max)
        * 1.15;
    let bands = values
        .iter()
        .map(|(lag, value)| Band {
            lower: vec![
                Point {
                    x: lag - 0.3,
                    y: 0.0,
                },
                Point {
                    x: lag + 0.3,
                    y: 0.0,
                },
            ],
            upper: vec![
                Point {
                    x: lag - 0.3,
                    y: *value,
                },
                Point {
                    x: lag + 0.3,
                    y: *value,
                },
            ],
        })
        .collect();
    let lines = vec![
        Segment {
            from: Point { x: 0.4, y: 0.0 },
            to: Point {
                x: count as f64 + 0.6,
                y: 0.0,
            },
            width: 1.0,
        },
        Segment {
            from: Point { x: 0.4, y: band },
            to: Point {
                x: count as f64 + 0.6,
                y: band,
            },
            width: 1.0,
        },
        Segment {
            from: Point { x: 0.4, y: -band },
            to: Point {
                x: count as f64 + 0.6,
                y: -band,
            },
            width: 1.0,
        },
    ];
    PlotSpec {
        title: title.to_owned(),
        x_axis: linear_axis("Lag", 0.4, count as f64 + 0.6),
        y_axis: linear_axis("Correlation", -top, top),
        size: (900, 460),
        points: Vec::new(),
        lines,
        bands,
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: vec![PlotNote::new(vec![(
            "Band".to_owned(),
            format!("±{band:.4}"),
        )])],
    }
}

/// Cross-correlation report and chart.
///
/// # Errors
///
/// The errors of [`cross_correlation`], a missing column and the same column
/// twice.
pub fn cross_correlation_report(
    df: &DataFrame,
    first: &str,
    second: &str,
    max_lag: usize,
) -> Result<AnalysisOutput> {
    if first == second {
        return Err(Error::Unsupported(
            "the two series must be different columns".to_owned(),
        ));
    }
    let x_column = df.column(first)?;
    let y_column = df.column(second)?;
    let mut x = Vec::new();
    let mut y = Vec::new();
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        match (x_column.value(row), y_column.value(row)) {
            (Some(left), Some(right)) => {
                x.push(left);
                y.push(right);
            }
            _ => excluded += 1,
        }
    }
    let mut model = cross_correlation(&x, &y, max_lag)?;
    model.excluded = excluded;
    let best = model
        .lags
        .iter()
        .filter(|lag| lag.r.is_finite())
        .max_by(|a, b| {
            a.r.abs()
                .partial_cmp(&b.r.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    let mut result = AnalysisResult::new(
        format!("Cross Correlation — {first} and {second}"),
        "Correlations of the first series with the second at lags −m…m; a positive lag pairs \
         x(t) with y(t+k)",
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Observations", model.n as f64),
        ("Lags", model.lags.len() as f64),
        ("White-noise band (1.96/√n)", model.band),
        ("Largest |r|", best.map_or(f64::NAN, |lag| lag.r.abs())),
        ("At lag", best.map_or(f64::NAN, |lag| lag.lag as f64)),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Lag".to_owned(),
            "Correlation".to_owned(),
            "Pairs".to_owned(),
        ],
        rows: model
            .lags
            .iter()
            .map(|lag| {
                vec![
                    lag.lag.to_string(),
                    if lag.r.is_finite() {
                        format!("{:+.6}", lag.r)
                    } else {
                        String::new()
                    },
                    lag.pairs.to_string(),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Lags".to_owned(),
            "a positive lag k pairs x(t) with y(t+k), so the correlation at +k measures how y \
             follows x by k observations"
                .to_owned(),
        ),
        (
            "Band".to_owned(),
            "the ±1.96/√n band is the large-sample white-noise band of either series; a pair \
             over few observations is not reliable"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    let plot = ccf_plot(&model, max_lag);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn ccf_plot(model: &CcfFit, count: usize) -> PlotSpec {
    let top = model
        .lags
        .iter()
        .filter(|lag| lag.r.is_finite())
        .map(|lag| lag.r.abs())
        .fold(model.band * 1.2, f64::max)
        * 1.15;
    let bands = model
        .lags
        .iter()
        .filter(|lag| lag.r.is_finite())
        .map(|lag| Band {
            lower: vec![
                Point {
                    x: lag.lag as f64 - 0.3,
                    y: 0.0,
                },
                Point {
                    x: lag.lag as f64 + 0.3,
                    y: 0.0,
                },
            ],
            upper: vec![
                Point {
                    x: lag.lag as f64 - 0.3,
                    y: lag.r,
                },
                Point {
                    x: lag.lag as f64 + 0.3,
                    y: lag.r,
                },
            ],
        })
        .collect();
    let lines = vec![
        Segment {
            from: Point {
                x: -(count as f64) - 0.5,
                y: 0.0,
            },
            to: Point {
                x: count as f64 + 0.5,
                y: 0.0,
            },
            width: 1.0,
        },
        Segment {
            from: Point {
                x: -(count as f64) - 0.5,
                y: model.band,
            },
            to: Point {
                x: count as f64 + 0.5,
                y: model.band,
            },
            width: 1.0,
        },
        Segment {
            from: Point {
                x: -(count as f64) - 0.5,
                y: -model.band,
            },
            to: Point {
                x: count as f64 + 0.5,
                y: -model.band,
            },
            width: 1.0,
        },
    ];
    let step = if count <= 10 { 1 } else { count.div_ceil(10) };
    PlotSpec {
        title: "Cross Correlation".to_owned(),
        x_axis: AxisSpec {
            label: "Lag".to_owned(),
            scale: Scale::Linear,
            min: -(count as f64) - 0.5,
            max: count as f64 + 0.5,
            ticks: (-(count as i64)..=count as i64)
                .filter(|lag| lag.rem_euclid(step as i64) == 0)
                .map(|lag| Tick {
                    position: lag as f64,
                    label: lag.to_string(),
                })
                .collect(),
            minor: Vec::new(),
        },
        y_axis: linear_axis("Correlation", -top, top),
        size: (900, 460),
        points: Vec::new(),
        lines,
        bands,
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: vec![PlotNote::new(vec![(
            "Band".to_owned(),
            format!("±{:.4}", model.band),
        )])],
    }
}

/// Unit-root report.
///
/// # Errors
///
/// The errors of [`series`] and [`adf`].
pub fn adf_report(
    df: &DataFrame,
    column: &str,
    lags: usize,
    model: AdfModel,
) -> Result<AnalysisOutput> {
    let (values, excluded) = series(df, column)?;
    let mut fit = adf(&values, lags, model)?;
    fit.excluded = excluded;
    let mut result = AnalysisResult::new(
        format!("Unit Root — column {column}"),
        "Augmented Dickey–Fuller test of a unit root against a stationary alternative",
    );
    result.n = fit.n;
    result.excluded = fit.excluded;
    result.statistics = vec![
        ("Lags", fit.lags as f64),
        ("Observations", fit.observations as f64),
        ("Gamma", fit.gamma),
        ("SE of gamma", fit.se),
        ("T (tau)", fit.t),
        ("Degrees of freedom", fit.df as f64),
        ("P (interpolated)", fit.p),
        ("Critical 1%", fit.critical[0]),
        ("Critical 5%", fit.critical[1]),
        ("Critical 10%", fit.critical[2]),
    ];
    result.extra.push(Item::Message(format!(
        "Deterministic terms: {}",
        fit.model.name()
    )));
    result.extra.push(Item::Message(format!(
        "Decision at 5 %: {}",
        if fit.reject {
            "the unit root is rejected (the series is stationary or trend-stationary)"
        } else {
            "the unit root is not rejected (difference the series before modelling it)"
        }
    )));
    result.notes = vec![
        (
            "Regression".to_owned(),
            "Δy_t = μ + βt + γ·y_{t−1} + Σ δᵢ·Δy_{t−i} + ε_t; the test statistic is the t ratio \
             of γ and a negative value rejects the unit root"
                .to_owned(),
        ),
        (
            "Critical values".to_owned(),
            "asymptotic MacKinnon values: −2.57/−1.94/−1.62 without deterministic terms, \
             −3.43/−2.86/−2.57 with a constant and −3.96/−3.41/−3.13 with a constant and trend \
             (1 %, 5 %, 10 %)"
                .to_owned(),
        ),
        (
            "P-value".to_owned(),
            "the p-value interpolates the three critical values and approximates the τ \
             distribution; it is not an exact quantile"
                .to_owned(),
        ),
        (
            "Scope".to_owned(),
            "one series, a constant and/or a linear trend and the chosen augmentation lags; \
             seasonal unit roots and structural breaks are not tested"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(fit.n, fit.excluded, 0);
    if fit.observations < 20 {
        result.warnings.push(
            "few observations in the regression: the asymptotic critical values are a rough \
             guide"
                .to_owned(),
        );
    }
    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}

/// Fits a trend model to a series.
///
/// # Errors
///
/// Returns [`Error::SampleTooSmall`] for fewer than four observations,
/// [`Error::Unsupported`] for non-positive values in the exponential and
/// growth models and the fitting errors.
pub fn trend(values: &[f64], model: TrendModel) -> Result<TrendFit> {
    let n = values.len();
    if n < 4 {
        return Err(Error::SampleTooSmall { n, min: 4 });
    }
    if matches!(model, TrendModel::Exponential | TrendModel::Growth)
        && values.iter().any(|value| *value <= 0.0)
    {
        return Err(Error::Unsupported(
            "the exponential and growth models need strictly positive values".to_owned(),
        ));
    }
    let t: Vec<f64> = (1..=n).map(|index| index as f64).collect();
    let (names, predictors, response): (Vec<String>, Vec<Vec<f64>>, Vec<f64>) = match model {
        TrendModel::Linear => (vec!["t".to_owned()], vec![t.clone()], values.to_vec()),
        TrendModel::Quadratic => (
            vec!["t".to_owned(), "t²".to_owned()],
            vec![t.clone(), t.iter().map(|value| value * value).collect()],
            values.to_vec(),
        ),
        TrendModel::Exponential => (
            vec!["t".to_owned()],
            vec![t.clone()],
            values.iter().map(|value| value.ln()).collect(),
        ),
        TrendModel::Growth => (
            vec!["1/t".to_owned()],
            vec![t.iter().map(|value| 1.0 / value).collect()],
            values.iter().map(|value| value.ln()).collect(),
        ),
    };
    let mut columns = vec![Column::new_f64("__trend_response", response)];
    for (name, predictor) in names.iter().zip(&predictors) {
        columns.push(Column::new_f64(name.clone(), predictor.clone()));
    }
    let temp = DataFrame::new(columns)?;
    let fit = regression::fit_saturated(&temp, "__trend_response", &names, 0.95)?;
    let (fitted, equation) = match model {
        TrendModel::Linear => {
            let base = fit.coefficients[0];
            let slope = fit.coefficients[1];
            let fitted = t.iter().map(|value| base + slope * value).collect();
            (fitted, format!("y = {base:.6} + {slope:.6}·t"))
        }
        TrendModel::Quadratic => {
            let base = fit.coefficients[0];
            let linear = fit.coefficients[1];
            let square = fit.coefficients[2];
            let fitted = t
                .iter()
                .map(|value| base + linear * value + square * value * value)
                .collect();
            (
                fitted,
                format!("y = {base:.6} + {linear:.6}·t + {square:.6}·t²"),
            )
        }
        TrendModel::Exponential => {
            let first = fit.coefficients[0].exp();
            let second = fit.coefficients[1];
            let fitted = t
                .iter()
                .map(|value| first * (second * value).exp())
                .collect();
            (fitted, format!("y = {first:.6}·exp({second:.6}·t)"))
        }
        TrendModel::Growth => {
            let first = fit.coefficients[0];
            let second = fit.coefficients[1];
            let fitted = t
                .iter()
                .map(|value| (first + second / value).exp())
                .collect();
            (fitted, format!("y = exp({first:.6} + {second:.6}/t)"))
        }
    };
    let mean = values.iter().sum::<f64>() / n as f64;
    let total_ss: f64 = values.iter().map(|value| (value - mean).powi(2)).sum();
    let residuals: Vec<f64> = values
        .iter()
        .zip(&fitted)
        .map(|(value, fitted)| value - fitted)
        .collect();
    let residual_ss: f64 = residuals.iter().map(|value| value * value).sum();
    let r_squared = if total_ss > 0.0 {
        (1.0 - residual_ss / total_ss).clamp(0.0, 1.0)
    } else {
        f64::NAN
    };
    let parameters = fit.coefficients.len() as f64;
    let s = (residual_ss / (n as f64 - parameters)).sqrt();
    let mut names_out = vec!["Constant".to_owned()];
    names_out.extend(names.iter().cloned());
    Ok(TrendFit {
        model,
        names: names_out,
        coefficients: fit.coefficients.clone(),
        se: fit.standard_errors.clone(),
        t: fit.t_values.clone(),
        p: fit.p_values.clone(),
        r_squared,
        s,
        fitted,
        residuals,
        equation,
        n,
        excluded: 0,
    })
}

/// Trend report and the fitted curve.
///
/// # Errors
///
/// The errors of [`series`] and [`trend`].
pub fn trend_report(df: &DataFrame, column: &str, model: TrendModel) -> Result<AnalysisOutput> {
    let (values, excluded) = series(df, column)?;
    let mut fit = trend(&values, model)?;
    fit.excluded = excluded;
    let mut result = AnalysisResult::new(
        format!("Trend Analysis — column {column}"),
        format!(
            "Least-squares {} trend of the series in row order",
            model.name()
        ),
    );
    result.n = fit.n;
    result.excluded = fit.excluded;
    result.statistics = vec![
        ("Observations", fit.n as f64),
        ("R²", fit.r_squared),
        ("Residual standard error", fit.s),
    ];
    result
        .extra
        .push(Item::Message(format!("Equation: {}", fit.equation)));
    if model == TrendModel::Exponential {
        let growth = (fit.coefficients[1].exp() - 1.0) * 100.0;
        result
            .extra
            .push(Item::Message(format!("Growth per period: {growth:+.4} %")));
    }
    result.extra.push(Item::Table {
        headers: vec![
            "Term".to_owned(),
            "Coef".to_owned(),
            "SE Coef".to_owned(),
            "T".to_owned(),
            "P".to_owned(),
        ],
        rows: fit
            .names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                vec![
                    name.clone(),
                    format!("{:.6}", fit.coefficients[index]),
                    format!("{:.6}", fit.se[index]),
                    format!("{:.6}", fit.t[index]),
                    format!("{:.6}", fit.p[index]),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Models".to_owned(),
            "the linear and quadratic models are fitted to y; the exponential (y = b₀·exp(b₁t)) \
             and growth (y = exp(b₀ + b₁/t)) models are fitted to ln y, so their coefficients and \
             tests belong to the linearised fit"
                .to_owned(),
        ),
        (
            "Statistics".to_owned(),
            "R² and the residual standard error are computed on the original scale".to_owned(),
        ),
        (
            "Scope".to_owned(),
            "one series against time; seasonal components, ARIMA errors and forecast intervals \
             are outside this command"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(fit.n, fit.excluded, 0);
    let plot = trend_plot(column, &values, &fit.fitted);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn trend_plot(column: &str, values: &[f64], fitted: &[f64]) -> PlotSpec {
    let low = values
        .iter()
        .chain(fitted)
        .copied()
        .fold(f64::INFINITY, f64::min);
    let high = values
        .iter()
        .chain(fitted)
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let pad = ((high - low).abs() * 0.06).max(1e-9);
    let points: Vec<Point> = fitted
        .iter()
        .enumerate()
        .map(|(index, value)| Point {
            x: index as f64 + 1.0,
            y: *value,
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
    PlotSpec {
        title: format!("Trend Analysis — {column}"),
        x_axis: linear_axis("Observation", 1.0, values.len() as f64),
        y_axis: linear_axis(column, low - pad, high + pad),
        size: (900, 520),
        points: values
            .iter()
            .enumerate()
            .map(|(index, value)| Point {
                x: index as f64 + 1.0,
                y: *value,
            })
            .collect(),
        lines: Vec::new(),
        bands: Vec::new(),
        series: vec![Series {
            label: "Fitted".to_owned(),
            points,
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

    fn ramp(n: usize) -> Vec<f64> {
        (1..=n).map(|value| value as f64).collect()
    }

    fn frame(columns: Vec<Column>) -> DataFrame {
        DataFrame::new(columns).expect("frame")
    }

    /// A curved series with an irregular, deterministic noise so the
    /// unit-root design is not degenerate.
    fn wavy(n: usize) -> Vec<f64> {
        (0..n)
            .map(|index| {
                (index as f64 * 0.7).sin()
                    + 0.02 * index as f64
                    + (((index * 37) % 17) as f64 - 8.0) * 0.05
            })
            .collect()
    }

    #[test]
    fn the_autocorrelation_of_a_ramp_matches_the_closed_form() {
        let values = ramp(10);
        let model = autocorrelation(&values, 2).expect("acf");
        assert_eq!(model.n, 10);
        // r1 = 57.75/82.5 = 0.7 and r2 = 34/82.5 over Σ(y−ȳ)² = 82.5.
        let r1 = 0.7;
        let r2 = 34.0 / 82.5;
        assert!(
            (model.lags[0].acf - r1).abs() < 1e-12,
            "{}",
            model.lags[0].acf
        );
        assert!(
            (model.lags[1].acf - r2).abs() < 1e-12,
            "{}",
            model.lags[1].acf
        );
        // Durbin–Levinson: phi_11 = r1 and phi_22 = (r2 − r1²)/(1 − r1²).
        assert!((model.lags[0].pacf - r1).abs() < 1e-12);
        assert!(
            (model.lags[1].pacf - -0.152_703_505_644_682_13).abs() < 1e-12,
            "{}",
            model.lags[1].pacf
        );
        // Q_1 = n(n+2)·r1²/(n−1).
        let expected = 10.0 * 12.0 * r1 * r1 / 9.0;
        assert!(
            (model.lags[0].q - expected).abs() < 1e-9,
            "{}",
            model.lags[0].q
        );
        assert!(model.lags[0].p < 0.05, "{}", model.lags[0].p);
        let report = autocorrelation_report(&frame(vec![Column::new_f64("y", values)]), "y", 2)
            .expect("report");
        assert!(report.results[0].to_session_text().contains("Ljung"));
        let plot = report.plot.expect("plot");
        assert_eq!(plot.panels.len(), 2);
    }

    #[test]
    fn the_autocorrelation_rejects_short_series_and_bad_windows() {
        assert!(matches!(
            autocorrelation(&[1.0, 2.0, 3.0], 1),
            Err(Error::SampleTooSmall { .. })
        ));
        let values = ramp(10);
        assert!(autocorrelation(&values, 0).is_err());
        assert!(autocorrelation(&values, 9).is_err());
        assert!(matches!(
            autocorrelation(&[2.0; 10], 2),
            Err(Error::ConstantSample)
        ));
    }

    #[test]
    fn the_cross_correlation_finds_the_shift() {
        // A non-monotone pattern, so that only the true shift correlates.
        let x: Vec<f64> = (0..20)
            .map(|index| (((index * 37) % 17) as f64 - 8.0).sin())
            .collect();
        // y follows x by two observations.
        let mut y = vec![0.0; 20];
        y[2..].copy_from_slice(&x[..18]);
        let model = cross_correlation(&x, &y, 4).expect("ccf");
        assert_eq!(model.lags.len(), 9);
        let best = model
            .lags
            .iter()
            .max_by(|a, b| {
                a.r.abs()
                    .partial_cmp(&b.r.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("lag");
        assert_eq!(best.lag, 2);
        assert!(best.r > 0.999, "{}", best.r);
        assert_eq!(best.pairs, 18);
        // The lag window is symmetric in the number of pairs.
        assert_eq!(model.lags[0].pairs, 16);
        assert!(cross_correlation(&x, &y[..10], 3).is_err());
    }

    #[test]
    fn the_unit_root_rejects_a_mean_reverting_series() {
        // A stationary two-cycle: y_t = 0.2·y_{t−1} + 1 ± 0.4.
        let mut values = vec![1.0];
        for index in 1..80 {
            let noise = if index % 2 == 0 { 0.4 } else { -0.4 };
            values.push(0.2 * values[index - 1] + noise + 1.0);
        }
        let fit = adf(&values, 1, AdfModel::Constant).expect("adf");
        assert!(fit.t < fit.critical[0], "{}", fit.t);
        assert!(fit.reject);
        assert!(fit.p < 0.01, "{}", fit.p);
        // A random walk: the unit root stands.
        let mut walk = vec![0.0];
        for index in 1..120 {
            walk.push(walk[index - 1] + if index % 3 == 0 { 1.0 } else { 0.9 });
        }
        let fit = adf(&walk, 0, AdfModel::Constant).expect("adf");
        assert!(fit.t > fit.critical[2], "{}", fit.t);
        assert!(!fit.reject);
        let report = adf_report(
            &frame(vec![Column::new_f64("y", walk)]),
            "y",
            0,
            AdfModel::Constant,
        )
        .expect("report");
        assert!(report.results[0].to_session_text().contains("not rejected"));
    }

    #[test]
    fn the_unit_root_matches_the_regression_module() {
        let values = wavy(40);
        let fit = adf(&values, 2, AdfModel::Constant).expect("adf");
        let mut response = Vec::new();
        let mut y_lag = Vec::new();
        let mut lag_one = Vec::new();
        let mut lag_two = Vec::new();
        for index in 3..values.len() {
            response.push(values[index] - values[index - 1]);
            y_lag.push(values[index - 1]);
            lag_one.push(values[index - 1] - values[index - 2]);
            lag_two.push(values[index - 2] - values[index - 3]);
        }
        let df = frame(vec![
            Column::new_f64("dy", response),
            Column::new_f64("y_lag", y_lag),
            Column::new_f64("dy1", lag_one),
            Column::new_f64("dy2", lag_two),
        ]);
        let reference = regression::fit_saturated(
            &df,
            "dy",
            &["y_lag".to_owned(), "dy1".to_owned(), "dy2".to_owned()],
            0.95,
        )
        .expect("regression");
        assert!((fit.gamma - reference.coefficients[1]).abs() < 1e-9);
        // The normal equations and the QR fit agree to the conditioning of the
        // design, not to the last bit.
        let reference_t = reference.t_values[1];
        assert!(
            ((fit.t - reference_t) / reference_t).abs() < 1e-4,
            "{} {}",
            fit.t,
            reference_t
        );
        assert_eq!(fit.df, reference.df_residual as usize);
        assert_eq!(fit.observations, reference.n);
    }

    #[test]
    fn the_unit_root_rejects_bad_requests() {
        assert!(matches!(
            adf(&[1.0, 2.0], 0, AdfModel::Constant),
            Err(Error::SampleTooSmall { .. })
        ));
        assert!(adf(&ramp(10), 8, AdfModel::Constant).is_err());
        assert!(adf(&ramp(10), 5, AdfModel::Constant).is_err());
        // Too few rows for the regression once the lags are taken out.
        assert!(adf(&wavy(8), 3, AdfModel::Constant).is_err());
    }

    #[test]
    fn the_differences_and_lags_write_the_expected_values() {
        let column = Column::new_f64("y", ramp(6));
        let first = transform_column(&column, ColumnOperation::Difference { order: 1, lag: 1 })
            .expect("difference");
        assert!(first[0].is_nan());
        assert_eq!(&first[1..], &[1.0, 1.0, 1.0, 1.0, 1.0]);
        let second = transform_column(&column, ColumnOperation::Difference { order: 2, lag: 1 })
            .expect("difference");
        assert!(second[0].is_nan() && second[1].is_nan());
        assert_eq!(&second[2..], &[0.0, 0.0, 0.0, 0.0]);
        let lagged = transform_column(&column, ColumnOperation::Lag { lag: 2 }).expect("lag");
        assert_eq!(&lagged[2..], &[1.0, 2.0, 3.0, 4.0]);
        assert!(lagged[0].is_nan() && lagged[1].is_nan());
        // A missing value propagates instead of being treated as zero.
        let holed = Column::new_f64("y", vec![1.0, f64::NAN, 3.0, 6.0]);
        let differenced =
            transform_column(&holed, ColumnOperation::Difference { order: 1, lag: 1 })
                .expect("difference");
        // The second difference does not depend on the missing first value.
        assert!(differenced[2].is_nan());
        assert_eq!(differenced[3], 3.0);
        assert!(transform_column(&column, ColumnOperation::Lag { lag: 0 }).is_err());
        assert!(
            transform_column(&column, ColumnOperation::Difference { order: 3, lag: 1 }).is_err()
        );
    }

    #[test]
    fn the_trend_models_recover_the_coefficients() {
        let t = ramp(12);
        let linear = trend(&t, TrendModel::Linear).expect("linear");
        assert!((linear.coefficients[1] - 1.0).abs() < 1e-9);
        assert!(linear.r_squared > 0.999_999);
        let quadratic = trend(&t, TrendModel::Quadratic).expect("quadratic");
        assert!(quadratic.r_squared > 0.999_999);
        assert_eq!(quadratic.names, vec!["Constant", "t", "t²"]);
        // y = 5·2^t: the linearised coefficients are ln 5 and ln 2.
        let exponential: Vec<f64> = t.iter().map(|value| 5.0 * 2f64.powf(*value)).collect();
        let fitted = trend(&exponential, TrendModel::Exponential).expect("exponential");
        assert!((fitted.coefficients[0].exp() - 5.0).abs() < 1e-6);
        assert!((fitted.coefficients[1] - 2f64.ln()).abs() < 1e-9);
        assert!(fitted.equation.contains("·exp("), "{}", fitted.equation);
        // y = exp(2 + 3/t).
        let growth: Vec<f64> = t.iter().map(|value| (2.0 + 3.0 / value).exp()).collect();
        let fitted = trend(&growth, TrendModel::Growth).expect("growth");
        assert!((fitted.coefficients[0] - 2.0).abs() < 1e-5);
        assert!((fitted.coefficients[1] - 3.0).abs() < 1e-4);
        assert!(fitted.equation.contains("/t"), "{}", fitted.equation);
    }

    #[test]
    fn the_trend_rejects_short_and_non_positive_series() {
        assert!(trend(&[1.0, 2.0, 3.0], TrendModel::Linear).is_err());
        let with_zero = vec![1.0, 0.0, 2.0, 3.0, 4.0];
        assert!(
            trend(&with_zero, TrendModel::Exponential)
                .unwrap_err()
                .to_string()
                .contains("positive")
        );
    }

    #[test]
    fn the_trend_report_lists_the_table_and_the_plot() {
        let values = ramp(12);
        let report = trend_report(
            &frame(vec![Column::new_f64("y", values)]),
            "y",
            TrendModel::Linear,
        )
        .expect("report");
        let text = report.results[0].to_session_text();
        assert!(text.contains("Trend Analysis"), "{text}");
        assert!(text.contains("Equation"), "{text}");
        assert!(report.plot.is_some());
    }

    #[test]
    fn the_cross_correlation_report_lists_the_lags() {
        let x = ramp(20);
        let y: Vec<f64> = x.iter().map(|value| 3.0 * value + 1.0).collect();
        let report = cross_correlation_report(
            &frame(vec![Column::new_f64("x", x), Column::new_f64("y", y)]),
            "x",
            "y",
            3,
        )
        .expect("report");
        let text = report.results[0].to_session_text();
        assert!(text.contains("Cross Correlation"), "{text}");
        assert!(text.contains("Largest |r|"), "{text}");
        assert!(report.plot.is_some());
        assert!(
            cross_correlation_report(&frame(vec![Column::new_f64("x", ramp(10))]), "x", "x", 2)
                .is_err()
        );
    }

    #[test]
    fn the_transformation_adds_a_named_column() {
        let mut df = frame(vec![Column::new_f64("y", ramp(5))]);
        let (name, present, missing) = add_transformed_column(
            &mut df,
            "y",
            ColumnOperation::Difference { order: 1, lag: 1 },
        )
        .expect("column");
        assert_eq!(name, "D1(y,1)");
        assert_eq!(present, 4);
        assert_eq!(missing, 1);
        assert!(df.column("D1(y,1)").is_ok());
        let (second, _, _) =
            add_transformed_column(&mut df, "y", ColumnOperation::Lag { lag: 1 }).expect("column");
        assert_eq!(second, "y[1]");
        // A repeated name gets a suffix.
        let (third, _, _) =
            add_transformed_column(&mut df, "y", ColumnOperation::Lag { lag: 1 }).expect("column");
        assert_eq!(third, "y[1]_2");
    }
}
