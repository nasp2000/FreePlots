//! Hotelling's T² control chart for several variables at once.
//!
//! The chart monitors the mean vector of `p` numeric variables measured on
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
//! subgroup conventions were not documented, so the numbers are not claimed as
//! documented behaviour — see `the project notes`.

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
