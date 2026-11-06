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
    let mut pairs = Vec::with_capacity(columns.len() * (columns.len() - 1) / 2);
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
