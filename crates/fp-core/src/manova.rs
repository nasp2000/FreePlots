// SPDX-License-Identifier: MIT
//! One-way multivariate analysis of variance (MANOVA).
//!
//! With `p` response variables and `g` groups, the hypothesis and error
//! matrices are
//!
//! `H = Σ_g n_g (ȳ_g − ȳ)(ȳ_g − ȳ)ᵀ` and `E = Σ_g Σ_i (y_gi − ȳ_g)(y_gi − ȳ_g)ᵀ`,
//!
//! with `g − 1` and `n − g` degrees of freedom. The four usual statistics come
//! from the eigenvalues `λ` of `H(E + H)⁻¹` (computed through the Cholesky
//! factor of `E` and a symmetric Jacobi decomposition):
//!
//! - **Wilks**: `Λ = Π (1 − λ) = det E / det(E + H)`;
//! - **Pillai**: `V = Σ λ = tr(H(E + H)⁻¹)`;
//! - **Hotelling–Lawley**: `U = Σ λ/(1 − λ) = tr(H E⁻¹)`;
//! - **Roy**: the largest `λ/(1 − λ)`, the largest root of `E⁻¹H`.
//!
//! Each statistic carries Rao's approximate F with the SAS parameterisation
//! (`s = min(p, q)`, `m = (|p − q| − 1)/2`, `n = (v − p − 1)/2`), which is
//! exact when `q = 1` (two groups) and approximate otherwise. At least two
//! response variables are needed; a single response is a one-way ANOVA.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::pca::jacobi;
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::f_cdf;
use crate::workflow::AnalysisOutput;

/// One multivariate test statistic with its approximate F.
#[derive(Debug, Clone, PartialEq)]
pub struct ManovaStatistic {
    /// Name of the statistic.
    pub name: &'static str,
    /// Value of the statistic.
    pub value: f64,
    /// Approximate F.
    pub f: f64,
    /// Numerator degrees of freedom.
    pub df1: f64,
    /// Denominator degrees of freedom.
    pub df2: f64,
    /// p-value of the F test.
    pub p: f64,
}

/// Means of one group.
#[derive(Debug, Clone, PartialEq)]
pub struct ManovaGroup {
    /// Level label.
    pub level: String,
    /// Observations in the group.
    pub n: usize,
    /// Mean of each response variable.
    pub means: Vec<f64>,
}

/// Fitted one-way MANOVA.
#[derive(Debug, Clone, PartialEq)]
pub struct ManovaFit {
    /// Response variable names.
    pub variables: Vec<String>,
    /// Group levels, in label order.
    pub groups: Vec<ManovaGroup>,
    /// Hypothesis matrix `H`.
    pub h: Vec<Vec<f64>>,
    /// Error matrix `E`.
    pub e: Vec<Vec<f64>>,
    /// Eigenvalues of `H(E + H)⁻¹`, decreasing.
    pub eigenvalues: Vec<f64>,
    /// The four statistics in report order.
    pub statistics: Vec<ManovaStatistic>,
    /// Between-group degrees of freedom.
    pub df_between: f64,
    /// Error degrees of freedom.
    pub df_error: f64,
    /// Observations used.
    pub n: usize,
    /// Rows excluded for a missing value.
    pub excluded: usize,
}

fn value(column: &Column, row: usize) -> Option<f64> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => values.get(row).copied().filter(|v| v.is_finite()),
        ColumnData::I64(values) => values.get(row).map(|&v| v as f64),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_) => None,
    }
}

fn label(column: &Column, row: usize) -> Option<String> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::Text(values) => values.get(row).cloned(),
        ColumnData::Date(values) => Some(crate::data::format_date(values[row])),
        ColumnData::I64(values) => values.get(row).map(ToString::to_string),
        ColumnData::F64(values) => values.get(row).map(ToString::to_string),
        ColumnData::Bool(values) => values
            .get(row)
            .map(|&v| if v { "T" } else { "F" }.to_owned()),
    }
}

/// Cholesky factor `L` of a symmetric positive definite matrix (`E = L Lᵀ`).
fn cholesky(matrix: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = matrix.len();
    let mut lower = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let mut sum = matrix[i][j];
            for k in 0..j {
                sum -= lower[i][k] * lower[j][k];
            }
            if i == j {
                if !(sum > 0.0) {
                    return None;
                }
                lower[i][j] = sum.sqrt();
            } else {
                lower[i][j] = sum / lower[j][j];
            }
        }
    }
    Some(lower)
}

/// Solves `L X = B` for a lower triangular `L`, `B` p×p.
fn forward(lower: &[Vec<f64>], rhs: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = lower.len();
    let mut out = vec![vec![0.0; n]; n];
    for row in 0..n {
        for column in 0..n {
            let mut sum = rhs[row][column];
            for k in 0..row {
                sum -= lower[row][k] * out[k][column];
            }
            out[row][column] = sum / lower[row][row];
        }
    }
    out
}

fn transpose(matrix: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let rows = matrix.len();
    let columns = matrix.first().map_or(0, Vec::len);
    let mut out = vec![vec![0.0; rows]; columns];
    for (i, row) in matrix.iter().enumerate() {
        for (j, value) in row.iter().enumerate() {
            out[j][i] = *value;
        }
    }
    out
}

fn determinant(matrix: &[Vec<f64>]) -> f64 {
    match cholesky(matrix) {
        Some(lower) => lower
            .iter()
            .enumerate()
            .map(|(i, row)| row[i])
            .product::<f64>()
            .powi(2),
        None => 0.0,
    }
}

/// Fits the one-way MANOVA.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for fewer than two response variables, an
/// unknown or repeated column, a factor with a single level, a singular error
/// matrix and the same factor and response; [`Error::SampleTooSmall`] when the
/// residual degrees of freedom are smaller than the number of variables.
pub fn fit(df: &DataFrame, responses: &[String], factor: &str) -> Result<ManovaFit> {
    if responses.len() < 2 {
        return Err(Error::Unsupported(
            "MANOVA needs at least two response variables".to_owned(),
        ));
    }
    let mut unique = responses.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != responses.len() {
        return Err(Error::Unsupported(
            "the response list contains duplicates".to_owned(),
        ));
    }
    if responses.iter().any(|name| name == factor) {
        return Err(Error::Unsupported(
            "the factor cannot also be a response variable".to_owned(),
        ));
    }
    let response_columns: Vec<&Column> = responses
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    for (name, column) in responses.iter().zip(&response_columns) {
        if matches!(
            column.data(),
            ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
        ) {
            return Err(Error::Csv(format!("response column {name} is not numeric")));
        }
    }
    let factor_column = df.column(factor)?;

    let p = responses.len();
    let mut observations: Vec<(String, Vec<f64>)> = Vec::new();
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        let Some(level) = label(factor_column, row) else {
            excluded += 1;
            continue;
        };
        let mut values = Vec::with_capacity(p);
        let mut complete = true;
        for column in &response_columns {
            match value(column, row) {
                Some(found) => values.push(found),
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if !complete {
            excluded += 1;
            continue;
        }
        observations.push((level, values));
    }
    if observations.is_empty() {
        return Err(Error::EmptySample);
    }
    let mut levels: Vec<String> = Vec::new();
    for (level, _) in &observations {
        if !levels.contains(level) {
            levels.push(level.clone());
        }
    }
    levels.sort();
    let g = levels.len();
    if g < 2 {
        return Err(Error::Unsupported(
            "the factor needs at least two levels".to_owned(),
        ));
    }
    let n = observations.len();
    let df_between = (g - 1) as f64;
    let df_error = (n - g) as f64;
    if df_error < p as f64 {
        return Err(Error::SampleTooSmall { n, min: g + p });
    }

    let mut groups: Vec<ManovaGroup> = Vec::with_capacity(g);
    for level in &levels {
        let rows: Vec<&Vec<f64>> = observations
            .iter()
            .filter(|(found, _)| found == level)
            .map(|(_, values)| values)
            .collect();
        let mut means = vec![0.0; p];
        for row in &rows {
            for (j, value) in row.iter().enumerate() {
                means[j] += value;
            }
        }
        for mean in &mut means {
            *mean /= rows.len() as f64;
        }
        groups.push(ManovaGroup {
            level: level.clone(),
            n: rows.len(),
            means,
        });
    }
    let mut grand = vec![0.0; p];
    for (_, values) in &observations {
        for (j, value) in values.iter().enumerate() {
            grand[j] += value;
        }
    }
    for value in &mut grand {
        *value /= n as f64;
    }

    let mut h = vec![vec![0.0; p]; p];
    let mut e = vec![vec![0.0; p]; p];
    for group in &groups {
        for (i, left) in group.means.iter().enumerate() {
            for (j, right) in group.means.iter().enumerate() {
                h[i][j] += group.n as f64 * (left - grand[i]) * (right - grand[j]);
            }
        }
    }
    for (level, values) in &observations {
        let group = groups
            .iter()
            .find(|group| group.level == *level)
            .expect("group was collected");
        for (i, left) in values.iter().enumerate() {
            for (j, right) in values.iter().enumerate() {
                e[i][j] += (left - group.means[i]) * (right - group.means[j]);
            }
        }
    }

    // Eigenvalues of H(E + H)⁻¹ through the Cholesky factor of E:
    // λ(E⁻¹H) are the eigenvalues of L⁻¹ H L⁻ᵀ.
    let lower = cholesky(&e).ok_or_else(|| {
        Error::Unsupported(
            "the within-group covariance matrix is singular: too few observations for the \
             number of variables"
                .to_owned(),
        )
    })?;
    let first = forward(&lower, &h);
    let transformed = transpose(&forward(&lower, &transpose(&first)));
    let (mut eigenvalues, _) = jacobi(&transformed).ok_or_else(|| {
        Error::Unsupported("the eigenvalue decomposition did not converge".to_owned())
    })?;
    eigenvalues.sort_by(|a, b| b.total_cmp(a));
    // λ = θ/(1 + θ) with θ ≥ 0, so λ ∈ [0, 1).
    for eigenvalue in &mut eigenvalues {
        *eigenvalue = eigenvalue.clamp(0.0, 1.0 - 1e-12);
    }
    let lambda: Vec<f64> = eigenvalues
        .iter()
        .map(|value| value / (1.0 - value))
        .collect();
    let wilks = eigenvalues.iter().map(|value| 1.0 - value).product::<f64>();
    let pillai: f64 = eigenvalues.iter().sum();
    let hotelling: f64 = lambda.iter().sum();
    let roy = lambda.iter().copied().fold(0.0, f64::max);

    let q = df_between;
    let v = df_error;
    let pf = p as f64;
    let s = pf.min(q);
    let m = ((pf - q).abs() - 1.0) / 2.0;
    let nn = (v - pf - 1.0) / 2.0;
    let p_value = |f: f64, df1: f64, df2: f64| {
        if f.is_finite() && df1 > 0.0 && df2 > 0.0 {
            (1.0 - f_cdf(f, df1, df2)).clamp(0.0, 1.0)
        } else {
            f64::NAN
        }
    };

    let r = v - (pf - q + 1.0) / 2.0;
    let u = (pf * q - 2.0) / 4.0;
    let t = if pf * pf + q * q > 5.0 {
        ((pf * pf * q * q - 4.0) / (pf * pf + q * q - 5.0)).sqrt()
    } else {
        1.0
    };
    let df1_wilks = pf * q;
    let df2_wilks = r * t - 2.0 * u;
    let wilks_root = if wilks > 0.0 {
        wilks.powf(1.0 / t)
    } else {
        0.0
    };
    let f_wilks = if wilks_root > 0.0 {
        (1.0 - wilks_root) / wilks_root * df2_wilks / df1_wilks
    } else {
        f64::INFINITY
    };
    let df1_pillai = s * (2.0 * m + s + 1.0);
    let df2_pillai = s * (2.0 * nn + s + 1.0);
    let f_pillai = if s - pillai > 0.0 {
        df2_pillai / df1_pillai * pillai / (s - pillai)
    } else {
        f64::INFINITY
    };
    let (df1_hl, df2_hl, f_hl) = if nn > 0.0 {
        let b = (pf + 2.0 * nn) * (q + 2.0 * nn) / (2.0 * (2.0 * nn + 1.0) * (nn - 1.0));
        let df1 = pf * q;
        let df2 = 4.0 + (pf * q + 2.0) / (b - 1.0);
        let c = (df2 - 2.0) / (2.0 * nn);
        (df1, df2, df2 / df1 * hotelling / c)
    } else {
        let df1 = s * (2.0 * m + s + 1.0);
        let df2 = s * (s * nn + 1.0);
        (df1, df2, df2 / df1 / s * hotelling)
    };
    let df1_roy = pf.max(q);
    let df2_roy = v - df1_roy + q;
    let f_roy = df2_roy / df1_roy * roy;

    let statistics = vec![
        ManovaStatistic {
            name: "Wilks' lambda",
            value: wilks,
            f: f_wilks,
            df1: df1_wilks,
            df2: df2_wilks,
            p: p_value(f_wilks, df1_wilks, df2_wilks),
        },
        ManovaStatistic {
            name: "Pillai's trace",
            value: pillai,
            f: f_pillai,
            df1: df1_pillai,
            df2: df2_pillai,
            p: p_value(f_pillai, df1_pillai, df2_pillai),
        },
        ManovaStatistic {
            name: "Hotelling-Lawley trace",
            value: hotelling,
            f: f_hl,
            df1: df1_hl,
            df2: df2_hl,
            p: p_value(f_hl, df1_hl, df2_hl),
        },
        ManovaStatistic {
            name: "Roy's greatest root",
            value: roy,
            f: f_roy,
            df1: df1_roy,
            df2: df2_roy,
            p: p_value(f_roy, df1_roy, df2_roy),
        },
    ];

    Ok(ManovaFit {
        variables: responses.to_vec(),
        groups,
        h,
        e,
        eigenvalues,
        statistics,
        df_between,
        df_error,
        n,
        excluded,
    })
}

/// Builds the MANOVA report.
///
/// # Errors
///
/// The errors of [`fit`].
pub fn analyze(df: &DataFrame, responses: &[String], factor: &str) -> Result<AnalysisOutput> {
    let model = fit(df, responses, factor)?;
    let mut result = AnalysisResult::new(
        format!("MANOVA — {} responses by {factor}", model.variables.len()),
        "One-way multivariate analysis of variance with Rao's approximate F tests",
    );
    result.n = model.n;
    result.excluded = model.excluded;
    let significant = model
        .statistics
        .iter()
        .filter(|statistic| statistic.p.is_finite() && statistic.p < 0.05)
        .count();
    result.statistics = vec![
        ("Variables", model.variables.len() as f64),
        ("Groups", model.groups.len() as f64),
        ("Between groups df", model.df_between),
        ("Error df", model.df_error),
        (
            "Largest eigenvalue",
            model.eigenvalues.first().copied().unwrap_or(f64::NAN),
        ),
        ("Statistics below 0.05", significant as f64),
    ];
    result.extra.push(Item::Message(format!(
        "Responses: {}",
        model.variables.join(", ")
    )));
    result.extra.push(Item::Table {
        headers: vec![
            "Statistic".to_owned(),
            "Value".to_owned(),
            "F".to_owned(),
            "DF1".to_owned(),
            "DF2".to_owned(),
            "P".to_owned(),
        ],
        rows: model
            .statistics
            .iter()
            .map(|statistic| {
                let number = |value: f64| {
                    if value.is_finite() {
                        format!("{value:.6}")
                    } else {
                        String::new()
                    }
                };
                vec![
                    statistic.name.to_owned(),
                    format!("{:.6}", statistic.value),
                    number(statistic.f),
                    format!("{:.4}", statistic.df1),
                    format!("{:.4}", statistic.df2),
                    number(statistic.p),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: {
            let mut headers = vec!["Group".to_owned(), "n".to_owned()];
            headers.extend(model.variables.iter().cloned());
            headers
        },
        rows: model
            .groups
            .iter()
            .map(|group| {
                let mut row = vec![group.level.clone(), group.n.to_string()];
                row.extend(group.means.iter().map(|mean| format!("{mean:.6}")));
                row
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Model".to_owned(),
            "one factor and several numeric responses; the hypothesis and error matrices are \
             H (between groups) and E (within groups), with g − 1 and n − g degrees of freedom"
                .to_owned(),
        ),
        (
            "Statistics".to_owned(),
            "Wilks Λ = det E/det(E + H); Pillai V = tr(H(E + H)⁻¹); Hotelling–Lawley \
             U = tr(H E⁻¹); Roy is the largest root of E⁻¹H"
                .to_owned(),
        ),
        (
            "F tests".to_owned(),
            "Rao's approximate F with s = min(p, q), m = (|p − q| − 1)/2 and n = (v − p − 1)/2; \
             exact with two groups and approximate otherwise"
                .to_owned(),
        ),
        (
            "Scope".to_owned(),
            "one factor, numeric responses, no covariates and no multivariate post-hoc \
             comparisons; a single response belongs to the one-way ANOVA"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    if let Some(statistic) = model
        .statistics
        .iter()
        .find(|statistic| !statistic.p.is_finite())
    {
        result.warnings.push(format!(
            "{}: the degrees of freedom do not support the F approximation",
            statistic.name
        ));
    }
    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(columns: Vec<Column>) -> DataFrame {
        DataFrame::new(columns).expect("frame")
    }

    /// Two variables, three groups of two observations, built so that
    /// `H = diag(12, 4)` and `E = diag(4, 4)`: every statistic then has a
    /// closed form.
    fn diagonal_frame() -> DataFrame {
        frame(vec![
            Column::new_f64("y1", vec![2.0, 0.0, 2.0, 0.0, -2.0, -2.0]),
            Column::new_f64("y2", vec![2.0, 0.0, -2.0, 0.0, 0.0, 0.0]),
            Column::new_text(
                "g",
                ["A", "A", "B", "B", "C", "C"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ])
    }

    #[test]
    fn the_manova_matrices_and_statistics_have_their_closed_form() {
        let df = diagonal_frame();
        let model = fit(&df, &["y1".to_owned(), "y2".to_owned()], "g").expect("manova");
        assert_eq!(model.n, 6);
        assert_eq!(model.df_between, 2.0);
        assert_eq!(model.df_error, 3.0);
        // Grand means are zero, so H and E are the group spreads.
        assert!((model.h[0][0] - 12.0).abs() < 1e-12, "{}", model.h[0][0]);
        assert!((model.h[1][1] - 4.0).abs() < 1e-12, "{}", model.h[1][1]);
        assert!((model.h[0][1]).abs() < 1e-12, "{}", model.h[0][1]);
        assert!((model.e[0][0] - 4.0).abs() < 1e-12, "{}", model.e[0][0]);
        assert!((model.e[1][1] - 4.0).abs() < 1e-12, "{}", model.e[1][1]);
        assert!((determinant(&model.e) - 16.0).abs() < 1e-9);
        // λ = h/(e + h): 12/16 and 4/8.
        assert!(
            (model.eigenvalues[0] - 0.75).abs() < 1e-9,
            "{:?}",
            model.eigenvalues
        );
        assert!(
            (model.eigenvalues[1] - 0.5).abs() < 1e-9,
            "{:?}",
            model.eigenvalues
        );
        let wilks = model
            .statistics
            .iter()
            .find(|statistic| statistic.name == "Wilks' lambda")
            .expect("wilks");
        // Λ = (1 − 0.75)(1 − 0.5) = 1/8.
        assert!((wilks.value - 0.125).abs() < 1e-9, "{}", wilks.value);
        // Rao's F with Λ^(1/2) = 1/√8 and four and four degrees of freedom.
        assert!(
            (wilks.f - 1.828_427_124_746_190_3).abs() < 1e-9,
            "{}",
            wilks.f
        );
        assert_eq!(wilks.df1, 4.0);
        assert_eq!(wilks.df2, 4.0);
        let pillai = model
            .statistics
            .iter()
            .find(|statistic| statistic.name == "Pillai's trace")
            .expect("pillai");
        assert!((pillai.value - 1.25).abs() < 1e-9, "{}", pillai.value);
        let hotelling = model
            .statistics
            .iter()
            .find(|statistic| statistic.name == "Hotelling-Lawley trace")
            .expect("hotelling");
        assert!((hotelling.value - 4.0).abs() < 1e-9, "{}", hotelling.value);
        let roy = model
            .statistics
            .iter()
            .find(|statistic| statistic.name == "Roy's greatest root")
            .expect("roy");
        assert!((roy.value - 3.0).abs() < 1e-9, "{}", roy.value);
        assert!(wilks.p.is_finite() && wilks.p > 0.05, "{}", wilks.p);
        let report = analyze(&df, &["y1".to_owned(), "y2".to_owned()], "g")
            .expect("report")
            .to_session_text();
        assert!(report.contains("MANOVA — 2 responses"), "{report}");
        assert!(report.contains("Pillai's trace"), "{report}");
    }

    #[test]
    fn the_manova_with_two_groups_matches_the_exact_f() {
        // Two groups of three observations, two responses: q = 1 makes the
        // Wilks and Pillai F tests exact.
        let df = frame(vec![
            Column::new_f64("y1", vec![1.0, 2.0, 3.0, 5.0, 6.0, 7.0]),
            Column::new_f64("y2", vec![2.0, 3.0, 4.0, 8.0, 9.0, 10.0]),
            Column::new_text(
                "g",
                ["A", "A", "A", "B", "B", "B"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ]);
        let model = fit(&df, &["y1".to_owned(), "y2".to_owned()], "g").expect("manova");
        assert_eq!(model.df_between, 1.0);
        assert_eq!(model.df_error, 4.0);
        let wilks = model
            .statistics
            .iter()
            .find(|statistic| statistic.name == "Wilks' lambda")
            .expect("wilks");
        let pillai = model
            .statistics
            .iter()
            .find(|statistic| statistic.name == "Pillai's trace")
            .expect("pillai");
        // Both are exact for two groups: the same F with (p, n − p − 1) df.
        assert!(
            (wilks.f - pillai.f).abs() < 1e-9,
            "{} {}",
            wilks.f,
            pillai.f
        );
        assert!((wilks.f - 18.6).abs() < 0.01, "{}", wilks.f);
        assert_eq!(wilks.df1, 2.0);
        assert_eq!(wilks.df2, 3.0);
        assert!(wilks.p < 0.05, "{}", wilks.p);
    }

    #[test]
    fn the_manova_rejects_invalid_designs() {
        let df = diagonal_frame();
        // A single response is not a MANOVA.
        assert!(
            fit(&df, &["y1".to_owned()], "g")
                .unwrap_err()
                .to_string()
                .contains("two response")
        );
        // The factor cannot be a response.
        assert!(fit(&df, &["y1".to_owned(), "g".to_owned()], "g").is_err());
        // Duplicates and text responses.
        assert!(fit(&df, &["y1".to_owned(), "y1".to_owned()], "g").is_err());
        let words = frame(vec![
            Column::new_text(
                "y1",
                ["1", "2", "3", "4", "5", "6"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
            Column::new_f64("y2", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_text(
                "g",
                ["A", "A", "B", "B", "C", "C"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ]);
        assert!(fit(&words, &["y1".to_owned(), "y2".to_owned()], "g").is_err());
        // A single level cannot be tested.
        let one = frame(vec![
            Column::new_f64("y1", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_f64("y2", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_text(
                "g",
                ["A", "A", "A", "A"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ]);
        assert!(fit(&one, &["y1".to_owned(), "y2".to_owned()], "g").is_err());
        // Too few observations for the variables.
        let few = frame(vec![
            Column::new_f64("y1", vec![1.0, 2.0, 3.0]),
            Column::new_f64("y2", vec![1.0, 2.0, 3.0]),
            Column::new_text(
                "g",
                ["A", "A", "B"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ]);
        assert!(fit(&few, &["y1".to_owned(), "y2".to_owned()], "g").is_err());
    }
}
