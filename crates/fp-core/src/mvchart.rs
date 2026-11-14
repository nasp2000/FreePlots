// SPDX-License-Identifier: MIT
//! Multivariate control charts for several variables at once: Hotelling's T²,
//! the generalized variance chart and the multivariate EWMA.
//!
//! The T² chart monitors the mean vector of `p` numeric variables measured on
//! consecutive subgroups of size `n`. With `m` subgroups, the pooled
//! within-subgroup covariance is
//!
//! ```text
//! S = 1/(m(n − 1)) · Σᵢ Σⱼ (xᵢⱼ − x̄ᵢ)(xᵢⱼ − x̄ᵢ)ᵀ
//! ```
//!
//! and every subgroup mean gets `T²ᵢ = n·(x̄ᵢ − x̄̄)ᵀ·S⁻¹·(x̄ᵢ − x̄̄)`. The
//! upper limit is the `F` form of the Phase I chart,
//!
//! ```text
//! UCL = p(m − 1)(n − 1)/(m·n − m − p + 1) · F(1 − α; p, m·n − m − p + 1)
//! ```
//!
//! which keeps the simultaneous false-alarm rate at `α`; the lower limit is
//! zero, because `T²` cannot be negative.
//!
//! The documented behaviour has a multivariate control chart; its limits and
//! subgroup conventions were not documented, so the numbers are not claimed to
//! match the documented behaviour — see the project notes.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::f_quantile;
use crate::workflow::AnalysisOutput;

/// Fitted Hotelling T² chart.
#[derive(Debug, Clone, PartialEq)]
pub struct T2Chart {
    /// Variables, in the requested order.
    pub columns: Vec<String>,
    /// Subgroups used.
    pub groups: usize,
    /// Observations per subgroup.
    pub subgroup: usize,
    /// Overall mean vector.
    pub mean: Vec<f64>,
    /// Pooled within-subgroup covariance matrix.
    pub covariance: Vec<Vec<f64>>,
    /// Statistic of each subgroup.
    pub values: Vec<f64>,
    /// Subgroups above the upper limit (1-based).
    pub out_of_control: Vec<usize>,
    /// Upper control limit.
    pub ucl: f64,
    /// Lower control limit (zero).
    pub lcl: f64,
    /// Critical value of the F distribution.
    pub f_critical: f64,
    /// Denominator degrees of freedom of the F distribution.
    pub f_df: f64,
}

/// Inverts a square matrix by Gauss–Jordan elimination with partial pivoting.
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

/// Builds the T² chart.
pub fn fit(
    df: &DataFrame,
    columns: &[String],
    subgroup: usize,
    confidence: f64,
) -> Result<T2Chart> {
    if columns.len() < 2 {
        return Err(Error::Unsupported(
            "the chart needs at least two variables".to_owned(),
        ));
    }
    if subgroup < 2 {
        return Err(Error::InvalidParameter {
            name: "subgroup size",
            value: subgroup as f64,
        });
    }
    if !(0.5..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence",
            value: confidence,
        });
    }
    let mut sources = Vec::with_capacity(columns.len());
    for name in columns {
        sources.push(df.column(name)?.numeric_by_row());
    }
    let p = columns.len();
    let mut rows: Vec<Vec<f64>> = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        let mut values = Vec::with_capacity(p);
        let mut complete = true;
        for source in &sources {
            if !source[row].is_finite() {
                complete = false;
                break;
            }
            values.push(source[row]);
        }
        if complete {
            rows.push(values);
        }
    }
    let groups = rows.len() / subgroup;
    if groups < 3 {
        return Err(Error::SampleTooSmall {
            n: rows.len(),
            min: 3 * subgroup,
        });
    }
    let used = groups * subgroup;
    let denominator_df = (groups * subgroup - groups - p + 1) as f64;
    if denominator_df <= 0.0 {
        return Err(Error::Unsupported(
            "the chart needs more subgroups than variables".to_owned(),
        ));
    }

    // Overall mean.
    let mut mean = vec![0.0; p];
    for row in rows.iter().take(used) {
        for (j, value) in row.iter().enumerate() {
            mean[j] += value;
        }
    }
    for value in &mut mean {
        *value /= used as f64;
    }

    // Pooled within-subgroup covariance.
    let mut covariance = vec![vec![0.0; p]; p];
    let mut subgroup_means: Vec<Vec<f64>> = Vec::with_capacity(groups);
    for group in 0..groups {
        let block = &rows[group * subgroup..(group + 1) * subgroup];
        let mut block_mean = vec![0.0; p];
        for row in block {
            for (j, value) in row.iter().enumerate() {
                block_mean[j] += value;
            }
        }
        for value in &mut block_mean {
            *value /= subgroup as f64;
        }
        for row in block {
            for i in 0..p {
                for j in 0..p {
                    covariance[i][j] += (row[i] - block_mean[i]) * (row[j] - block_mean[j]);
                }
            }
        }
        subgroup_means.push(block_mean);
    }
    let scale = (groups * (subgroup - 1)) as f64;
    for row in &mut covariance {
        for value in row.iter_mut() {
            *value /= scale;
        }
    }
    let inverse = invert(&covariance).ok_or_else(|| Error::NotConverged {
        detail: "the covariance matrix is singular; check for collinear variables".to_owned(),
    })?;

    let mut values = Vec::with_capacity(groups);
    for block_mean in &subgroup_means {
        let difference: Vec<f64> = block_mean
            .iter()
            .zip(&mean)
            .map(|(value, mean)| value - mean)
            .collect();
        let mut quadratic = 0.0;
        for (i, left) in difference.iter().enumerate() {
            for (j, right) in difference.iter().enumerate() {
                quadratic += left * inverse[i][j] * right;
            }
        }
        values.push((subgroup as f64 * quadratic).max(0.0));
    }

    let f_critical = f_quantile(confidence, p as f64, denominator_df);
    if !f_critical.is_finite() {
        return Err(Error::NotConverged {
            detail: "the F quantile of the T2 limit".to_owned(),
        });
    }
    let ucl = p as f64 * (groups - 1) as f64 * (subgroup - 1) as f64 / denominator_df * f_critical;
    if !ucl.is_finite() || ucl <= 0.0 {
        return Err(Error::NotConverged {
            detail: "the upper control limit of the T2 chart".to_owned(),
        });
    }
    let out_of_control: Vec<usize> = values
        .iter()
        .enumerate()
        .filter(|(_, value)| **value > ucl)
        .map(|(index, _)| index + 1)
        .collect();

    Ok(T2Chart {
        columns: columns.to_vec(),
        groups,
        subgroup,
        mean,
        covariance,
        values,
        out_of_control,
        ucl,
        lcl: 0.0,
        f_critical,
        f_df: denominator_df,
    })
}

/// Chart of the statistic with the two limits.
fn plot(model: &T2Chart) -> PlotSpec {
    let high = model.values.iter().copied().fold(model.ucl, f64::max);
    let points: Vec<Point> = model
        .values
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
        title: "Hotelling T² Chart".to_owned(),
        x_axis: linear_axis("Subgroup", 1.0, model.groups as f64),
        y_axis: linear_axis("T²", 0.0, (high * 1.05).max(1e-9)),
        size: (900, 540),
        points,
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Runs the chart and builds the report.
pub fn analyze(
    df: &DataFrame,
    columns: &[String],
    subgroup: usize,
    confidence: f64,
) -> Result<AnalysisOutput> {
    let model = fit(df, columns, subgroup, confidence)?;
    let mut result = AnalysisResult::new(
        "Hotelling T² Chart".to_owned(),
        format!(
            "{} variables, subgroups of {}, Phase I limits at {:.1}%",
            model.columns.len(),
            model.subgroup,
            confidence * 100.0
        ),
    );
    result.n = model.groups * model.subgroup;
    result.excluded = df.n_rows().saturating_sub(result.n);
    result.statistics = vec![
        ("Variables", model.columns.len() as f64),
        ("Subgroups", model.groups as f64),
        ("UCL", model.ucl),
        ("LCL", model.lcl),
        ("F critical", model.f_critical),
        ("F denominator df", model.f_df),
        ("Points out of control", model.out_of_control.len() as f64),
        (
            "Largest T²",
            model
                .values
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max),
        ),
    ];

    let mut headers = vec!["Variable".to_owned(), "Mean".to_owned()];
    for column in &model.columns {
        headers.push(format!("Cov {column}"));
    }
    let mut rows = Vec::with_capacity(model.columns.len());
    for (i, name) in model.columns.iter().enumerate() {
        let mut row = vec![name.clone(), format!("{:.6}", model.mean[i])];
        for j in 0..model.columns.len() {
            row.push(format!("{:.6}", model.covariance[i][j]));
        }
        rows.push(row);
    }
    result.extra.push(Item::Table { headers, rows });
    result.extra.push(Item::Table {
        headers: vec!["Subgroup".to_owned(), "T²".to_owned(), "Status".to_owned()],
        rows: model
            .values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                vec![
                    format!("{}", index + 1),
                    format!("{value:.6}"),
                    if *value > model.ucl {
                        "out of control".to_owned()
                    } else {
                        "in control".to_owned()
                    },
                ]
            })
            .collect(),
    });

    result.notes = vec![
        (
            "Statistic".to_owned(),
            "T²ᵢ = n·(x̄ᵢ − x̄̄)ᵀ·S⁻¹·(x̄ᵢ − x̄̄) with the pooled within-subgroup covariance".to_owned(),
        ),
        (
            "Limits".to_owned(),
            "UCL = p(m − 1)(n − 1)/(m·n − m − p + 1)·F(1 − α; p, m·n − m − p + 1) and LCL = 0"
                .to_owned(),
        ),
        (
            "Subgroups".to_owned(),
            "consecutive rows of the worksheet; an incomplete last subgroup is dropped".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing value in any variable are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the Phase I limits and the subgroup convention are standard choices and have not been \
         verified against documented output"
            .to_owned(),
    );
    if !model.out_of_control.is_empty() {
        let list: Vec<String> = model
            .out_of_control
            .iter()
            .map(ToString::to_string)
            .collect();
        result.warnings.push(format!(
            "{} subgroup(s) above the upper limit: {}",
            model.out_of_control.len(),
            list.join(", ")
        ));
    }

    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot(&model)),
    })
}

/// Multivariate chart family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MultivariateChart {
    /// Hotelling's T² chart (the existing chart).
    #[default]
    HotellingT2,
    /// Generalized variance chart.
    GeneralizedVariance,
    /// Multivariate exponentially weighted moving average chart.
    Mewma,
}

impl MultivariateChart {
    /// Name used in the dialog title and the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::HotellingT2 => "Hotelling T² Chart",
            Self::GeneralizedVariance => "Generalized Variance Chart",
            Self::Mewma => "Multivariate EWMA Chart",
        }
    }
}

/// The shared preparation of a multivariate chart: the complete rows grouped
/// into consecutive subgroups, the overall mean, the pooled covariance and the
/// mean of every subgroup.
struct Prepared {
    rows: Vec<Vec<f64>>,
    groups: usize,
    subgroup: usize,
    mean: Vec<f64>,
    covariance: Vec<Vec<f64>>,
    subgroup_means: Vec<Vec<f64>>,
}

/// Reads the columns, drops the incomplete rows and aggregates the subgroups.
fn prepare(df: &DataFrame, columns: &[String], subgroup: usize) -> Result<Prepared> {
    if columns.len() < 2 {
        return Err(Error::Unsupported(
            "the chart needs at least two variables".to_owned(),
        ));
    }
    if subgroup < 2 {
        return Err(Error::InvalidParameter {
            name: "subgroup size",
            value: subgroup as f64,
        });
    }
    let mut sources = Vec::with_capacity(columns.len());
    for name in columns {
        sources.push(df.column(name)?.numeric_by_row());
    }
    let p = columns.len();
    let mut rows: Vec<Vec<f64>> = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        let mut values = Vec::with_capacity(p);
        let mut complete = true;
        for source in &sources {
            if !source[row].is_finite() {
                complete = false;
                break;
            }
            values.push(source[row]);
        }
        if complete {
            rows.push(values);
        }
    }
    let groups = rows.len() / subgroup;
    if groups < 3 {
        return Err(Error::SampleTooSmall {
            n: rows.len(),
            min: 3 * subgroup,
        });
    }
    let used = groups * subgroup;
    rows.truncate(used);

    let mut mean = vec![0.0; p];
    for row in &rows {
        for (j, value) in row.iter().enumerate() {
            mean[j] += value;
        }
    }
    for value in &mut mean {
        *value /= used as f64;
    }

    let mut covariance = vec![vec![0.0; p]; p];
    let mut subgroup_means: Vec<Vec<f64>> = Vec::with_capacity(groups);
    for block in rows.chunks_exact(subgroup) {
        let mut block_mean = vec![0.0; p];
        for row in block {
            for (j, value) in row.iter().enumerate() {
                block_mean[j] += value;
            }
        }
        for value in &mut block_mean {
            *value /= subgroup as f64;
        }
        for row in block {
            for i in 0..p {
                for j in 0..p {
                    covariance[i][j] += (row[i] - block_mean[i]) * (row[j] - block_mean[j]);
                }
            }
        }
        subgroup_means.push(block_mean);
    }
    let scale = (groups * (subgroup - 1)) as f64;
    for row in &mut covariance {
        for value in row.iter_mut() {
            *value /= scale;
        }
    }
    Ok(Prepared {
        rows,
        groups,
        subgroup,
        mean,
        covariance,
        subgroup_means,
    })
}

/// Determinant of a square matrix by Gaussian elimination with partial
/// pivoting.
#[allow(clippy::needless_range_loop)]
fn determinant(matrix: &[Vec<f64>]) -> f64 {
    let n = matrix.len();
    let mut a = matrix.to_vec();
    let mut det = 1.0;
    for column in 0..n {
        let mut pivot = column;
        for row in (column + 1)..n {
            if a[row][column].abs() > a[pivot][column].abs() {
                pivot = row;
            }
        }
        if a[pivot][column].abs() < 1e-12 {
            return 0.0;
        }
        if pivot != column {
            a.swap(column, pivot);
            det = -det;
        }
        let divisor = a[column][column];
        det *= divisor;
        for row in (column + 1)..n {
            let factor = a[row][column] / divisor;
            for k in column..n {
                a[row][k] -= factor * a[column][k];
            }
        }
    }
    det
}

/// Fitted generalized variance chart.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneralizedVarianceChart {
    /// Variables, in the requested order.
    pub columns: Vec<String>,
    /// Subgroups used.
    pub groups: usize,
    /// Observations per subgroup.
    pub subgroup: usize,
    /// Overall mean vector.
    pub mean: Vec<f64>,
    /// Pooled within-subgroup covariance matrix.
    pub covariance: Vec<Vec<f64>>,
    /// Determinant of the pooled covariance.
    pub center: f64,
    /// Determinant of every subgroup covariance.
    pub values: Vec<f64>,
    /// Lower control limit.
    pub lcl: f64,
    /// Upper control limit.
    pub ucl: f64,
    /// First factor of the limits.
    pub b1: f64,
    /// Second factor of the limits.
    pub b2: f64,
    /// Subgroups outside the limits (1-based).
    pub out_of_control: Vec<usize>,
}

/// Builds the generalized variance chart.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] when the subgroup size is not larger than
/// the number of variables or the pooled covariance is singular, and the
/// usual sample errors for a short or invalid sample.
pub fn fit_generalized_variance(
    df: &DataFrame,
    columns: &[String],
    subgroup: usize,
) -> Result<GeneralizedVarianceChart> {
    let prepared = prepare(df, columns, subgroup)?;
    let p = columns.len();
    let subgroup = prepared.subgroup;
    if subgroup <= p {
        return Err(Error::Unsupported(
            "the generalized variance chart needs a subgroup size larger than the number of \
             variables"
                .to_owned(),
        ));
    }
    let center = determinant(&prepared.covariance);
    if center <= 0.0 || !center.is_finite() {
        return Err(Error::NotConverged {
            detail: "the pooled covariance matrix is singular; check for collinear variables"
                .to_owned(),
        });
    }
    let denominator = (subgroup - 1) as f64;
    let mut values = Vec::with_capacity(prepared.groups);
    for block in prepared.rows.chunks_exact(subgroup) {
        let mut block_mean = vec![0.0; p];
        for row in block {
            for (j, value) in row.iter().enumerate() {
                block_mean[j] += value;
            }
        }
        for value in &mut block_mean {
            *value /= subgroup as f64;
        }
        let mut cov = vec![vec![0.0; p]; p];
        for row in block {
            for i in 0..p {
                for j in 0..p {
                    cov[i][j] += (row[i] - block_mean[i]) * (row[j] - block_mean[j]);
                }
            }
        }
        for row in &mut cov {
            for value in row.iter_mut() {
                *value /= denominator;
            }
        }
        values.push(determinant(&cov).max(0.0));
    }
    let (mut product, mut shifted) = (1.0_f64, 1.0_f64);
    for i in 1..=p {
        product *= (subgroup - i) as f64;
        shifted *= (subgroup + 2 - i) as f64;
    }
    let power = (subgroup - 1) as f64;
    let b1 = product / power.powi(p as i32);
    let b2 = product * (shifted - product) / power.powi(2 * p as i32);
    let spread = 3.0 * b2.max(0.0).sqrt();
    let ucl = center * (b1 + spread);
    let lcl = (center * (b1 - spread)).max(0.0);
    let out_of_control: Vec<usize> = values
        .iter()
        .enumerate()
        .filter(|(_, value)| **value > ucl || **value < lcl)
        .map(|(index, _)| index + 1)
        .collect();
    Ok(GeneralizedVarianceChart {
        columns: columns.to_vec(),
        groups: prepared.groups,
        subgroup,
        mean: prepared.mean,
        covariance: prepared.covariance,
        center,
        values,
        lcl,
        ucl,
        b1,
        b2,
        out_of_control,
    })
}

/// Runs the generalized variance chart and builds its report.
pub fn generalized_variance(
    df: &DataFrame,
    columns: &[String],
    subgroup: usize,
) -> Result<AnalysisOutput> {
    let model = fit_generalized_variance(df, columns, subgroup)?;
    let mut result = AnalysisResult::new(
        "Generalized Variance Chart".to_owned(),
        format!(
            "{} variables, subgroups of {}; determinant of the subgroup covariance and the \
             b1/b2 limits",
            model.columns.len(),
            model.subgroup
        ),
    );
    result.n = model.groups * model.subgroup;
    result.excluded = df.n_rows().saturating_sub(result.n);
    result.statistics = vec![
        ("Variables", model.columns.len() as f64),
        ("Subgroups", model.groups as f64),
        ("Subgroup size", model.subgroup as f64),
        ("Center |S|", model.center),
        ("LCL", model.lcl),
        ("UCL", model.ucl),
        ("b1", model.b1),
        ("b2", model.b2),
        ("Points out of control", model.out_of_control.len() as f64),
    ];
    let mut headers = vec!["Variable".to_owned(), "Mean".to_owned()];
    for column in &model.columns {
        headers.push(format!("Cov {column}"));
    }
    let mut rows = Vec::with_capacity(model.columns.len());
    for (i, name) in model.columns.iter().enumerate() {
        let mut row = vec![name.clone(), format!("{:.6}", model.mean[i])];
        for j in 0..model.columns.len() {
            row.push(format!("{:.6}", model.covariance[i][j]));
        }
        rows.push(row);
    }
    result.extra.push(Item::Table { headers, rows });
    result.extra.push(Item::Table {
        headers: vec!["Subgroup".to_owned(), "|S|".to_owned(), "Status".to_owned()],
        rows: model
            .values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                vec![
                    format!("{}", index + 1),
                    format!("{value:.6}"),
                    if *value > model.ucl || *value < model.lcl {
                        "out of control".to_owned()
                    } else {
                        "in control".to_owned()
                    },
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Statistic".to_owned(),
            "|Sᵢ| is the determinant of the covariance matrix of subgroup i".to_owned(),
        ),
        (
            "Limits".to_owned(),
            "UCL = |S|·(b1 + 3√b2) and LCL = |S|·max(0, b1 − 3√b2), with b1 and b2 from the \
             subgroup size and the number of variables"
                .to_owned(),
        ),
        (
            "Center".to_owned(),
            "|S|, the determinant of the pooled within-subgroup covariance".to_owned(),
        ),
        (
            "Subgroups".to_owned(),
            "consecutive rows of the worksheet; an incomplete last subgroup is dropped".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing value in any variable are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the 3-sigma b1/b2 limits are the published Montgomery form and have not been verified \
         against documented output"
            .to_owned(),
    );
    if !model.out_of_control.is_empty() {
        let list: Vec<String> = model
            .out_of_control
            .iter()
            .map(ToString::to_string)
            .collect();
        result.warnings.push(format!(
            "{} subgroup(s) outside the limits: {}",
            model.out_of_control.len(),
            list.join(", ")
        ));
    }
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(statistic_plot(
            "Generalized Variance Chart",
            "|S|",
            &model.values,
            Some(model.center),
            Some(model.lcl),
            Some(model.ucl),
        )),
    })
}

/// Fitted multivariate EWMA chart.
#[derive(Debug, Clone, PartialEq)]
pub struct MewmaChart {
    /// Variables, in the requested order.
    pub columns: Vec<String>,
    /// Subgroups used.
    pub groups: usize,
    /// Observations per subgroup.
    pub subgroup: usize,
    /// Smoothing constant.
    pub lambda: f64,
    /// Overall mean vector.
    pub mean: Vec<f64>,
    /// Pooled within-subgroup covariance matrix.
    pub covariance: Vec<Vec<f64>>,
    /// Statistic of every subgroup.
    pub values: Vec<f64>,
    /// Upper control limit.
    pub ucl: f64,
    /// Critical value of the F distribution.
    pub f_critical: f64,
    /// Denominator degrees of freedom of the F distribution.
    pub f_df: f64,
    /// Subgroups above the upper limit (1-based).
    pub out_of_control: Vec<usize>,
}

/// Builds the multivariate EWMA chart.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for a smoothing constant outside
/// `(0, 1]` or a confidence outside `[0.5, 1)`, and [`Error::Unsupported`]
/// when there are not more subgroups than variables or the covariance is
/// singular.
pub fn fit_mewma(
    df: &DataFrame,
    columns: &[String],
    subgroup: usize,
    lambda: f64,
    confidence: f64,
) -> Result<MewmaChart> {
    if !(lambda > 0.0 && lambda <= 1.0) {
        return Err(Error::InvalidParameter {
            name: "lambda",
            value: lambda,
        });
    }
    if !(0.5..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence",
            value: confidence,
        });
    }
    let prepared = prepare(df, columns, subgroup)?;
    let p = columns.len();
    let groups = prepared.groups;
    if groups <= p {
        return Err(Error::Unsupported(
            "the multivariate EWMA chart needs more subgroups than variables".to_owned(),
        ));
    }
    let inverse = invert(&prepared.covariance).ok_or_else(|| Error::NotConverged {
        detail: "the covariance matrix is singular; check for collinear variables".to_owned(),
    })?;
    let mut values = Vec::with_capacity(groups);
    let mut smoothed = prepared.mean.clone();
    for (index, block_mean) in prepared.subgroup_means.iter().enumerate() {
        for (j, value) in smoothed.iter_mut().enumerate() {
            *value = lambda * block_mean[j] + (1.0 - lambda) * *value;
        }
        let difference: Vec<f64> = smoothed
            .iter()
            .zip(&prepared.mean)
            .map(|(value, mean)| value - mean)
            .collect();
        let mut quadratic = 0.0;
        for (i, left) in difference.iter().enumerate() {
            for (j, right) in difference.iter().enumerate() {
                quadratic += left * inverse[i][j] * right;
            }
        }
        let step = (index + 1) as i32;
        let factor = lambda / (2.0 - lambda) * (1.0 - (1.0 - lambda).powi(2 * step));
        values.push((quadratic / factor).max(0.0));
    }
    let df = (groups - p) as f64;
    let f_critical = f_quantile(confidence, p as f64, df);
    let ucl = p as f64 * (groups - 1) as f64 / df * f_critical;
    if !ucl.is_finite() || ucl <= 0.0 {
        return Err(Error::NotConverged {
            detail: "the upper control limit of the multivariate EWMA chart".to_owned(),
        });
    }
    let out_of_control: Vec<usize> = values
        .iter()
        .enumerate()
        .filter(|(_, value)| **value > ucl)
        .map(|(index, _)| index + 1)
        .collect();
    Ok(MewmaChart {
        columns: columns.to_vec(),
        groups,
        subgroup: prepared.subgroup,
        lambda,
        mean: prepared.mean,
        covariance: prepared.covariance,
        values,
        ucl,
        f_critical,
        f_df: df,
        out_of_control,
    })
}

/// Runs the multivariate EWMA chart and builds its report.
pub fn mewma(
    df: &DataFrame,
    columns: &[String],
    subgroup: usize,
    lambda: f64,
    confidence: f64,
) -> Result<AnalysisOutput> {
    let model = fit_mewma(df, columns, subgroup, lambda, confidence)?;
    let mut result = AnalysisResult::new(
        "Multivariate EWMA Chart".to_owned(),
        format!(
            "{} variables, subgroups of {}, λ = {} and a Phase I F limit at {:.1}%",
            model.columns.len(),
            model.subgroup,
            model.lambda,
            confidence * 100.0
        ),
    );
    result.n = model.groups * model.subgroup;
    result.excluded = df.n_rows().saturating_sub(result.n);
    result.statistics = vec![
        ("Variables", model.columns.len() as f64),
        ("Subgroups", model.groups as f64),
        ("Subgroup size", model.subgroup as f64),
        ("Lambda", model.lambda),
        ("UCL", model.ucl),
        ("F critical", model.f_critical),
        ("F denominator df", model.f_df),
        ("Points out of control", model.out_of_control.len() as f64),
        (
            "Largest T²",
            model
                .values
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max),
        ),
    ];
    let mut headers = vec!["Variable".to_owned(), "Mean".to_owned()];
    for column in &model.columns {
        headers.push(format!("Cov {column}"));
    }
    let mut rows = Vec::with_capacity(model.columns.len());
    for (i, name) in model.columns.iter().enumerate() {
        let mut row = vec![name.clone(), format!("{:.6}", model.mean[i])];
        for j in 0..model.columns.len() {
            row.push(format!("{:.6}", model.covariance[i][j]));
        }
        rows.push(row);
    }
    result.extra.push(Item::Table { headers, rows });
    result.extra.push(Item::Table {
        headers: vec!["Subgroup".to_owned(), "T²".to_owned(), "Status".to_owned()],
        rows: model
            .values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                vec![
                    format!("{}", index + 1),
                    format!("{value:.6}"),
                    if *value > model.ucl {
                        "out of control".to_owned()
                    } else {
                        "in control".to_owned()
                    },
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Statistic".to_owned(),
            "Zᵢ = λ·x̄ᵢ + (1 − λ)·Zᵢ₋₁ with Z₀ at the overall mean and T²ᵢ = (Zᵢ − μ̂)ᵀ·S⁻¹·(Zᵢ − \
             μ̂)/(λ/(2 − λ)·(1 − (1 − λ)²ⁱ))"
                .to_owned(),
        ),
        (
            "Limits".to_owned(),
            "UCL = p(m − 1)/(m − p)·F(1 − α; p, m − p), the Phase I form of the T² limit with \
             the estimated covariance"
                .to_owned(),
        ),
        (
            "Subgroups".to_owned(),
            "consecutive rows of the worksheet; an incomplete last subgroup is dropped".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing value in any variable are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the upper limit is the Phase I F form applied to the smoothed vectors; the ARL-calibrated \
         limit of Lowry et al. is not reproduced"
            .to_owned(),
    );
    if !model.out_of_control.is_empty() {
        let list: Vec<String> = model
            .out_of_control
            .iter()
            .map(ToString::to_string)
            .collect();
        result.warnings.push(format!(
            "{} subgroup(s) above the upper limit: {}",
            model.out_of_control.len(),
            list.join(", ")
        ));
    }
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(statistic_plot(
            "Multivariate EWMA Chart",
            "T²",
            &model.values,
            None,
            None,
            Some(model.ucl),
        )),
    })
}

/// A one-statistic chart with the points joined by a line and, optionally,
/// the center, lower and upper limits drawn across it.
fn statistic_plot(
    title: &str,
    y_label: &str,
    values: &[f64],
    center: Option<f64>,
    lower: Option<f64>,
    upper: Option<f64>,
) -> PlotSpec {
    let high = values.iter().copied().fold(upper.unwrap_or(0.0), f64::max);
    let points: Vec<Point> = values
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
    for y in [center, lower, upper].into_iter().flatten() {
        lines.push(Segment {
            from: Point { x: 1.0, y },
            to: Point {
                x: values.len().max(1) as f64,
                y,
            },
            width: 1.5,
        });
    }
    PlotSpec {
        title: title.to_owned(),
        x_axis: linear_axis("Subgroup", 1.0, values.len().max(1) as f64),
        y_axis: linear_axis(y_label, 0.0, (high * 1.05).max(1e-9)),
        size: (900, 540),
        points,
        lines,
        bands: Vec::new(),
        series: Vec::new(),
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
    use crate::special::f_quantile as fq;

    /// Ten subgroups of three observations on two variables around a centre;
    /// the last subgroup is shifted far away.
    fn frame(shift: f64) -> DataFrame {
        let mut a = Vec::new();
        let mut b = Vec::new();
        // Within-subgroup offsets that are not collinear, so the covariance is
        // invertible.
        let offsets = [(-0.1, 0.05), (0.0, -0.1), (0.1, 0.02)];
        for group in 0..10 {
            for (offset_a, offset_b) in offsets {
                let jump = if group == 9 { shift } else { 0.0 };
                a.push(10.0 + offset_a + jump);
                b.push(20.0 + offset_b + jump);
            }
        }
        DataFrame::new(vec![Column::new_f64("a", a), Column::new_f64("b", b)]).expect("frame")
    }

    #[test]
    fn the_limits_follow_the_f_distribution() {
        let model = fit(&frame(0.0), &["a".to_owned(), "b".to_owned()], 3, 0.95).expect("fit");
        assert_eq!(model.groups, 10);
        assert_eq!(model.subgroup, 3);
        let p = 2.0;
        let groups = 10.0;
        let subgroup = 3.0;
        let df = groups * subgroup - groups - p + 1.0;
        let expected = p * (groups - 1.0) * (subgroup - 1.0) / df * fq(0.95, p, df);
        assert!((model.ucl - expected).abs() < 1e-9, "{}", model.ucl);
        assert!((model.mean[0] - 10.0).abs() < 1e-9);
        // The second variable's offsets average −0.03/3 = −0.01.
        assert!((model.mean[1] - 19.99).abs() < 1e-9, "{}", model.mean[1]);
        // Stable data: every subgroup sits inside the limits.
        assert!(model.out_of_control.is_empty(), "{:?}", model.values);
        assert!(model.values.iter().all(|value| *value < model.ucl));
    }

    #[test]
    fn a_shifted_subgroup_leaves_the_chart() {
        let model = fit(&frame(5.0), &["a".to_owned(), "b".to_owned()], 3, 0.95).expect("fit");
        // The shifted subgroup is flagged and carries the largest statistic.
        assert!(
            model.out_of_control.contains(&10),
            "{:?}",
            model.out_of_control
        );
        assert!(model.values[9] > model.ucl);
        let largest = model
            .values
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .unwrap_or(0);
        assert_eq!(largest, 9, "{:?}", model.values);
    }

    #[test]
    fn invalid_configurations_are_rejected() {
        let df = frame(0.0);
        let columns = ["a".to_owned(), "b".to_owned()];
        assert!(matches!(
            fit(&df, &columns, 1, 0.95),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            fit(&df, &["a".to_owned()], 3, 0.95),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            fit(&df, &columns, 3, 0.3),
            Err(Error::InvalidParameter { .. })
        ));
    }

    #[test]
    fn the_report_and_the_chart_are_built() {
        let out =
            analyze(&frame(0.0), &["a".to_owned(), "b".to_owned()], 3, 0.95).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Hotelling T"), "{text}");
        assert!(text.contains("UCL"), "{text}");
        assert!(text.contains("Cov"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.points.len(), 10);
    }

    #[test]
    fn a_missing_row_does_not_shift_the_columns() {
        let rows: Vec<(f64, f64)> = (0..12)
            .map(|index| {
                let wiggle = if index % 2 == 0 { 0.4 } else { -0.4 };
                (
                    10.0 + index as f64 * 0.1,
                    20.0 + index as f64 * 0.2 + wiggle,
                )
            })
            .collect();
        let mut with_missing = rows.clone();
        with_missing.insert(0, (f64::NAN, 999.0));
        let frame_of = |rows: &[(f64, f64)]| {
            DataFrame::new(vec![
                Column::new_f64("x", rows.iter().map(|row| row.0).collect()),
                Column::new_f64("y", rows.iter().map(|row| row.1).collect()),
            ])
            .expect("frame")
        };
        let columns = vec!["x".to_owned(), "y".to_owned()];
        let a = fit(&frame_of(&with_missing), &columns, 3, 0.95).expect("chart");
        let b = fit(&frame_of(&rows), &columns, 3, 0.95).expect("chart");
        assert_eq!(a.groups, 4);
        assert!((a.mean[0] - b.mean[0]).abs() < 1e-9);
        for (left, right) in a.values.iter().zip(&b.values) {
            assert!(
                (left - right).abs() < 1e-9,
                "{:?} vs {:?}",
                a.values,
                b.values
            );
        }
    }

    /// Three subgroups of three observations on two variables; the last
    /// subgroup is twice as spread as the first two.
    fn variance_frame() -> DataFrame {
        let rows = [
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 1.0),
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 1.0),
            (0.0, 0.0),
            (2.0, 0.0),
            (0.0, 2.0),
        ];
        DataFrame::new(vec![
            Column::new_f64("x", rows.iter().map(|row| row.0).collect()),
            Column::new_f64("y", rows.iter().map(|row| row.1).collect()),
        ])
        .expect("frame")
    }

    #[test]
    fn the_generalized_variance_uses_the_b1_b2_limits() {
        let columns = vec!["x".to_owned(), "y".to_owned()];
        let model = fit_generalized_variance(&variance_frame(), &columns, 3).expect("chart");
        // Subgroup determinants: 1/12, 1/12 and 4/3; the pooled determinant
        // is 1/3, b1 = 0.5 and b2 = 1.25.
        assert!((model.center - 1.0 / 3.0).abs() < 1e-9, "{}", model.center);
        assert!((model.b1 - 0.5).abs() < 1e-12, "{}", model.b1);
        assert!((model.b2 - 1.25).abs() < 1e-12, "{}", model.b2);
        assert!((model.values[0] - 1.0 / 12.0).abs() < 1e-9);
        assert!((model.values[2] - 4.0 / 3.0).abs() < 1e-9);
        let expected_ucl = (1.0 / 3.0) * (0.5 + 3.0 * 1.25_f64.sqrt());
        assert!((model.ucl - expected_ucl).abs() < 1e-9, "{}", model.ucl);
        assert_eq!(model.lcl, 0.0);
        assert_eq!(model.out_of_control, vec![3]);
        assert_eq!(model.groups, 3);
        assert_eq!(model.subgroup, 3);
        assert!(
            (model.mean[0] - 4.0 / 9.0).abs() < 1e-12,
            "{:?}",
            model.mean
        );
    }

    #[test]
    fn the_generalized_variance_rejects_degenerate_input() {
        let columns = vec!["x".to_owned(), "y".to_owned()];
        // A subgroup of two observations on two variables has a zero
        // determinant, so the chart asks for more observations per subgroup.
        assert!(matches!(
            fit_generalized_variance(&variance_frame(), &columns, 2),
            Err(Error::Unsupported(_))
        ));
        // Constant data make the pooled covariance singular.
        let constant = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0; 9]),
            Column::new_f64("y", vec![2.0; 9]),
        ])
        .expect("frame");
        assert!(matches!(
            fit_generalized_variance(&constant, &columns, 3),
            Err(Error::NotConverged { .. })
        ));
    }

    /// Four subgroups of three observations on two variables; the third is
    /// twice as spread as the others.
    fn mewma_frame() -> DataFrame {
        let rows = [
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 1.0),
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 1.0),
            (0.0, 0.0),
            (2.0, 0.0),
            (0.0, 2.0),
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 1.0),
        ];
        DataFrame::new(vec![
            Column::new_f64("x", rows.iter().map(|row| row.0).collect()),
            Column::new_f64("y", rows.iter().map(|row| row.1).collect()),
        ])
        .expect("frame")
    }

    #[test]
    fn the_mewma_statistic_and_limit_are_finite() {
        let columns = vec!["x".to_owned(), "y".to_owned()];
        let model = fit_mewma(&mewma_frame(), &columns, 3, 0.5, 0.95).expect("chart");
        // UCL = p(m − 1)/(m − p)·F(0.95; 2, 2) = 2·3/2·19 = 57.
        assert!((model.ucl - 57.0).abs() < 1e-9, "{}", model.ucl);
        // The first smoothed vector is 3/8, the mean is 5/12 and the
        // quadratic form is 1/84, so T² = (1/84)/(1/4) = 1/21.
        assert!(
            (model.values[0] - 1.0 / 21.0).abs() < 1e-9,
            "{:?}",
            model.values
        );
        assert!(model.values.iter().all(|value| value.is_finite()));
        assert!(model.out_of_control.is_empty(), "{:?}", model.values);
        assert!((model.f_df - 2.0).abs() < 1e-12);
    }

    #[test]
    fn the_mewma_rejects_invalid_options_and_short_panels() {
        let columns = vec!["x".to_owned(), "y".to_owned()];
        assert!(matches!(
            fit_mewma(&mewma_frame(), &columns, 3, 0.0, 0.95),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            fit_mewma(&mewma_frame(), &columns, 3, 1.5, 0.95),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            fit_mewma(&mewma_frame(), &columns, 3, 0.2, 0.3),
            Err(Error::InvalidParameter { .. })
        ));
        // Three variables with only three subgroups leave no degrees of
        // freedom for the F limit.
        let frame = DataFrame::new(vec![
            Column::new_f64("x", vec![0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0]),
            Column::new_f64("y", vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]),
            Column::new_f64("z", vec![1.0, 2.0, 3.0, 2.0, 3.0, 4.0, 3.0, 4.0, 5.0]),
        ])
        .expect("frame");
        let three = vec!["x".to_owned(), "y".to_owned(), "z".to_owned()];
        assert!(matches!(
            fit_mewma(&frame, &three, 3, 0.2, 0.95),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn the_multivariate_reports_and_charts_are_built() {
        let columns = vec!["x".to_owned(), "y".to_owned()];
        let variance =
            generalized_variance(&variance_frame(), &columns, 3).expect("variance chart");
        let text = variance.results[0].to_session_text();
        assert!(text.contains("Generalized Variance Chart"), "{text}");
        assert!(text.contains("out of control"), "{text}");
        let plot = variance.plot.expect("plot");
        assert_eq!(plot.points.len(), 3);

        let ewma = mewma(&mewma_frame(), &columns, 3, 0.2, 0.95).expect("MEWMA");
        let text = ewma.results[0].to_session_text();
        assert!(text.contains("Multivariate EWMA Chart"), "{text}");
        assert!(text.contains("Lambda"), "{text}");
        let plot = ewma.plot.expect("plot");
        assert_eq!(plot.points.len(), 4);
    }
}
