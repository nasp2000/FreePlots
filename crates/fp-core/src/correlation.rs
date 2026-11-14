// SPDX-License-Identifier: MIT
//! Correlation matrix of several columns.
//!
//! The matrix reports the Pearson correlation of every pair of columns with
//! its two-sided p-value, and the Spearman rank correlation as a second
//! matrix. Only the pairs with complete observations are used, and the number
//! of pairs that could not be computed is reported.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::scatter::{kendall, pearson_correlation, spearman};
use crate::special::student_t_cdf;

/// Largest number of pair-observations one matrix will process.
///
/// Every pair of columns is correlated over the complete rows, and the rank
/// correlations sort those rows per pair; the cost therefore grows with
/// `pairs × rows`. Beyond this limit the run would hold the worker for minutes,
/// so the analysis asks for fewer columns instead.
const MAX_PAIR_OBSERVATIONS: usize = 40_000_000;

/// Refuses a matrix whose pair count would make the run unreasonably long.
fn check_workload(pairs: usize, rows: usize) -> Result<()> {
    if pairs.saturating_mul(rows) > MAX_PAIR_OBSERVATIONS {
        return Err(Error::Unsupported(format!(
            "the correlation matrix has {pairs} column pairs over {rows} rows, beyond the \
             {MAX_PAIR_OBSERVATIONS} pair-observations processed per run; select fewer columns"
        )));
    }
    Ok(())
}

/// Covariance matrix of several columns.
///
/// Each cell uses the rows that are complete in both columns and divides the
/// cross-product by `n − 1`; the diagonal is the sample variance.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for fewer than two columns, duplicate names
/// or a workload beyond the matrix limit, and [`Error::SampleTooSmall`] when a
/// pair has fewer than two complete observations.
pub fn covariance(df: &DataFrame, columns: &[String]) -> Result<AnalysisResult> {
    if columns.len() < 2 {
        return Err(Error::Unsupported(
            "the covariance matrix needs at least two columns".to_owned(),
        ));
    }
    let mut unique = columns.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != columns.len() {
        return Err(Error::Unsupported(
            "the column list contains duplicates".to_owned(),
        ));
    }
    let data: Vec<&Column> = columns
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    let rows = df.n_rows();
    let pairs_count = columns.len() * (columns.len() + 1) / 2;
    check_workload(pairs_count, rows)?;
    let mut matrix = vec![vec![f64::NAN; columns.len()]; columns.len()];
    let mut largest = 0;
    for i in 0..columns.len() {
        for j in i..columns.len() {
            let (xs, ys, n) = complete_pairs(data[i], data[j], rows);
            if n < 2 {
                return Err(Error::SampleTooSmall { n, min: 2 });
            }
            largest = largest.max(n);
            let mean_x = xs.iter().sum::<f64>() / n as f64;
            let mean_y = ys.iter().sum::<f64>() / n as f64;
            let sum = xs
                .iter()
                .zip(&ys)
                .map(|(x, y)| (x - mean_x) * (y - mean_y))
                .sum::<f64>();
            matrix[i][j] = sum / (n - 1) as f64;
            matrix[j][i] = matrix[i][j];
        }
    }
    let mut result = AnalysisResult::new(
        format!("Covariance — {}", columns.join(", ")),
        "Sample covariance matrix; each cell uses the complete pairs of its columns".to_owned(),
    );
    result.n = largest;
    result.statistics = vec![("Columns", columns.len() as f64)];
    let mut headers = Vec::with_capacity(columns.len() + 1);
    headers.push("Variable".to_owned());
    headers.extend(columns.iter().cloned());
    result.extra.push(Item::Table {
        headers,
        rows: (0..columns.len())
            .map(|i| {
                let mut row = Vec::with_capacity(columns.len() + 1);
                row.push(columns[i].clone());
                for j in 0..columns.len() {
                    row.push(if matrix[i][j].is_finite() {
                        format!("{:.6}", matrix[i][j])
                    } else {
                        "—".to_owned()
                    });
                }
                row
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Covariance".to_owned(),
            "cᵢⱼ = Σ(xᵢ − x̄ᵢ)(xⱼ − x̄ⱼ)/(n − 1); the diagonal is the sample variance".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "each cell uses the rows complete in both of its columns (pairwise); a pair with \
             fewer than two observations is an error"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, 0, 0);
    result.warnings.push(
        "the sample covariance uses n − 1 and pairwise complete rows; it has not been compared \
         with documented output"
            .to_owned(),
    );
    Ok(result)
}

/// A pair of columns and their correlations.
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    /// Name of the first column.
    pub first: String,
    /// Name of the second column.
    pub second: String,
    /// Number of complete pairs.
    pub n: usize,
    /// Pearson correlation.
    pub pearson: f64,
    /// p-value of the Pearson correlation.
    pub p_value: f64,
    /// Spearman rank correlation.
    pub spearman: f64,
    /// p-value of the Spearman correlation.
    pub spearman_p_value: f64,
    /// Kendall's tau-b.
    pub kendall: f64,
    /// p-value of Kendall's tau-b.
    pub kendall_p_value: f64,
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

/// Pairs of the values that are complete in both columns.
fn complete_pairs(first: &Column, second: &Column, rows: usize) -> (Vec<f64>, Vec<f64>, usize) {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for row in 0..rows {
        if let (Some(x), Some(y)) = (cell(first, row), cell(second, row)) {
            xs.push(x);
            ys.push(y);
        }
    }
    let n = xs.len();
    (xs, ys, n)
}

/// Partial correlation of two columns, controlling for a third.
///
/// `r_{xy·z} = (r_{xy} − r_{xz}·r_{yz}) / √((1 − r_{xz}²)(1 − r_{yz}²))`; the
/// p-value uses the Student t approximation with `n − 3` degrees of freedom.
pub fn partial_correlation(
    df: &DataFrame,
    x_column: &str,
    y_column: &str,
    control_column: &str,
) -> Result<AnalysisResult> {
    if x_column == y_column || x_column == control_column || y_column == control_column {
        return Err(Error::Unsupported(
            "the two variables and the control must be different columns".to_owned(),
        ));
    }
    let x_source = df.column(x_column)?;
    let y_source = df.column(y_column)?;
    let control_source = df.column(control_column)?;
    let rows = df.n_rows();
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut controls = Vec::new();
    for row in 0..rows {
        let (Some(x), Some(y), Some(control)) = (
            cell(x_source, row),
            cell(y_source, row),
            cell(control_source, row),
        ) else {
            continue;
        };
        xs.push(x);
        ys.push(y);
        controls.push(control);
    }
    let n = xs.len();
    if n < 5 {
        return Err(Error::SampleTooSmall { n, min: 5 });
    }
    let r_xy = pearson_correlation(&xs, &ys);
    let r_xz = pearson_correlation(&xs, &controls);
    let r_yz = pearson_correlation(&ys, &controls);
    let denominator = ((1.0 - r_xz * r_xz) * (1.0 - r_yz * r_yz)).sqrt();
    if denominator <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let r = (r_xy - r_xz * r_yz) / denominator;
    let degrees_of_freedom = (n - 3) as f64;
    let p_value = if r.abs() < 1.0 {
        let t = r * (degrees_of_freedom / (1.0 - r * r)).sqrt();
        let cdf = student_t_cdf(t, degrees_of_freedom);
        (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let mut result = AnalysisResult::new(
        format!("Partial Correlation — {x_column} and {y_column} given {control_column}"),
        "Correlation with the linear effect of the control removed".to_owned(),
    );
    result.n = n;
    result.excluded = rows - n;
    result.statistics = vec![
        ("Partial correlation", r),
        ("p-value", p_value),
        ("df", degrees_of_freedom),
        ("Correlation (x, y)", r_xy),
        ("Correlation (x, control)", r_xz),
        ("Correlation (y, control)", r_yz),
    ];
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: the partial correlation is zero; the p-value is the two-sided t test with n−3 \
             degrees of freedom"
                .to_owned(),
        ),
        (
            "Method".to_owned(),
            "the linear effect of the control is removed from both variables before the \
             correlation is computed"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows missing any of the three variables are removed and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    Ok(result)
}

/// Correlates every pair of the given columns.
pub fn correlate(df: &DataFrame, columns: &[String]) -> Result<AnalysisResult> {
    if columns.len() < 2 {
        return Err(Error::Unsupported(
            "the correlation matrix needs at least two columns".to_owned(),
        ));
    }
    let mut unique = columns.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != columns.len() {
        return Err(Error::Unsupported(
            "the column list contains duplicates".to_owned(),
        ));
    }
    let data: Vec<&Column> = columns
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    let rows = df.n_rows();
    let pairs_count = columns.len() * (columns.len() - 1) / 2;
    check_workload(pairs_count, rows)?;
    let mut pairs = Vec::with_capacity(pairs_count);
    for i in 0..columns.len() {
        for j in (i + 1)..columns.len() {
            let (xs, ys, n) = complete_pairs(data[i], data[j], rows);
            if n < 3 {
                return Err(Error::SampleTooSmall { n, min: 3 });
            }
            let r = pearson_correlation(&xs, &ys);
            let p_value = if r.is_finite() && r.abs() < 1.0 {
                let df = (n - 2) as f64;
                let t = r * (df / (1.0 - r * r)).sqrt();
                let cdf = student_t_cdf(t, df);
                (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0)
            } else if r.is_finite() {
                0.0
            } else {
                f64::NAN
            };
            let (rho, rho_p) = spearman(&xs, &ys);
            let (tau, tau_p) = kendall(&xs, &ys);
            pairs.push(Pair {
                first: columns[i].clone(),
                second: columns[j].clone(),
                n,
                pearson: r,
                p_value,
                spearman: rho,
                spearman_p_value: rho_p,
                kendall: tau,
                kendall_p_value: tau_p,
            });
        }
    }

    let mut result = AnalysisResult::new(
        format!("Correlation — {}", columns.join(", ")),
        "Pearson, Spearman and Kendall correlations of every pair".to_owned(),
    );
    result.n = pairs.iter().map(|pair| pair.n).max().unwrap_or(0);
    result.excluded = 0;
    result.statistics = vec![("Columns", columns.len() as f64)];
    result.extra.push(Item::Table {
        headers: vec![
            "Pair".to_owned(),
            "n".to_owned(),
            "Pearson r".to_owned(),
            "p".to_owned(),
            "Spearman rho".to_owned(),
            "p".to_owned(),
            "Kendall tau-b".to_owned(),
            "p".to_owned(),
        ],
        rows: pairs
            .iter()
            .map(|pair| {
                vec![
                    format!("{} × {}", pair.first, pair.second),
                    pair.n.to_string(),
                    format!("{:.6}", pair.pearson),
                    format!("{:.6}", pair.p_value),
                    format!("{:.6}", pair.spearman),
                    format!("{:.6}", pair.spearman_p_value),
                    format!("{:.6}", pair.kendall),
                    format!("{:.6}", pair.kendall_p_value),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: the two columns are independent; the p-value is the two-sided t test of the \\
             correlation with n−2 degrees of freedom"
                .to_owned(),
        ),
        (
            "Rank correlations".to_owned(),
            "Spearman rho (Pearson correlation of the mid-ranks) and Kendall tau-b (normal \\
             approximation) do not assume a linear relation"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows missing either column are removed pairwise and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, 0, 0);
    result.warnings.push(
        "Pearson correlations match the documented matrix (n = 16); Spearman and Kendall are \
         standard choices with no documented output"
            .to_owned(),
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    #[test]
    fn the_matrix_reports_every_pair() {
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_f64("y", vec![2.0, 4.0, 6.0, 8.0, 10.0, 12.0]),
            Column::new_f64("z", vec![5.0, 1.0, 4.0, 2.0, 6.0, 3.0]),
        ])
        .expect("frame");
        let result =
            correlate(&df, &["x".to_owned(), "y".to_owned(), "z".to_owned()]).expect("correlation");
        let text = result.to_session_text();
        assert!(text.contains("Correlation"), "{text}");
        assert!(text.contains("x × y"), "{text}");
        assert!(text.contains("x × z"), "{text}");
        assert!(text.contains("y × z"), "{text}");
    }

    #[test]
    fn the_partial_correlation_removes_the_effect_of_the_control() {
        // z drives x and y; the partial correlation of x and y given z is
        // near zero.
        let n = 40;
        let mut x = Vec::new();
        let mut y = Vec::new();
        let mut z = Vec::new();
        for index in 0..n {
            let control = (index % 7) as f64;
            x.push(2.0 * control + (index % 3) as f64 * 0.1);
            y.push(-1.5 * control + (index % 5) as f64 * 0.1);
            z.push(control);
        }
        let df = DataFrame::new(vec![
            Column::new_f64("x", x),
            Column::new_f64("y", y),
            Column::new_f64("z", z),
        ])
        .expect("frame");
        let result = partial_correlation(&df, "x", "y", "z").expect("partial");
        let text = result.to_session_text();
        assert!(text.contains("Partial Correlation"), "{text}");
        // The plain correlation is strong, the partial one is not.
        let partial = result.statistics[0].1;
        assert!(partial.abs() < 0.6, "partial {partial}");
    }

    #[test]
    fn a_single_column_is_refused() {
        let df = DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, 3.0])]).expect("frame");
        assert!(matches!(
            correlate(&df, &["x".to_owned()]),
            Err(Error::Unsupported { .. })
        ));
    }

    #[test]
    fn the_workload_limit_accepts_the_boundary_and_rejects_more() {
        assert!(check_workload(40, 1_000_000).is_ok());
        assert!(check_workload(40, 1_000_001).is_err());
        assert!(check_workload(45, 1_000_000).is_err());
        assert!(check_workload(usize::MAX, usize::MAX).is_err());
    }

    #[test]
    fn a_matrix_beyond_the_work_limit_is_refused_before_the_pairs_run() {
        // 30 columns are 435 pairs; over 100 000 rows that is 43 500 000
        // pair-observations, past the 40 000 000 limit.
        let columns: Vec<Column> = (0..30)
            .map(|column| {
                Column::new_f64(
                    format!("c{column}"),
                    (0..100_000).map(|row| (row + column) as f64).collect(),
                )
            })
            .collect();
        let df = DataFrame::new(columns).expect("frame");
        let names: Vec<String> = (0..30).map(|column| format!("c{column}")).collect();
        let error = correlate(&df, &names).expect_err("the work limit");
        assert!(
            error.to_string().contains("select fewer columns"),
            "{error}"
        );
    }

    #[test]
    fn the_covariance_matrix_uses_n_minus_one() {
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_f64("y", vec![2.0, 4.0, 6.0, 8.0]),
        ])
        .expect("frame");
        let result = covariance(&df, &["x".to_owned(), "y".to_owned()]).expect("covariance");
        // Var(x) = 5/3, Var(y) = 20/3, Cov(x, y) = 10/3.
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
            &vec!["Variable".to_owned(), "x".to_owned(), "y".to_owned()]
        );
        assert_eq!(rows[0][1], "1.666667");
        assert_eq!(rows[0][2], "3.333333");
        assert_eq!(rows[1][2], "6.666667");
    }

    #[test]
    fn the_covariance_uses_the_complete_pairs() {
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_f64("y", vec![2.0, f64::NAN, 6.0, 8.0]),
        ])
        .expect("frame");
        let result = covariance(&df, &["x".to_owned(), "y".to_owned()]).expect("covariance");
        let (_, rows) = result
            .extra
            .iter()
            .find_map(|item| match item {
                Item::Table { headers, rows } => Some((headers, rows)),
                _ => None,
            })
            .expect("table");
        // Three complete pairs: Cov = 28/6 = 4.666667.
        assert_eq!(rows[0][2], "4.666667");
    }

    #[test]
    fn the_covariance_rejects_bad_column_lists() {
        let df = DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, 3.0])]).expect("frame");
        assert!(covariance(&df, &["x".to_owned()]).is_err());
        assert!(covariance(&df, &["x".to_owned(), "x".to_owned()]).is_err());
        assert!(covariance(&df, &["x".to_owned(), "missing".to_owned()]).is_err());
    }
}
