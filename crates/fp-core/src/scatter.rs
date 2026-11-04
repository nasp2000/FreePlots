//! Scatterplot with Pearson correlation and an optional least-squares line.
//!
//! The report also carries the two rank correlations — Spearman's rho and
//! Kendall's tau-b — so a monotone but non-linear relation is visible.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::nonparametric::mid_ranks;
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{normal_cdf, student_t_cdf};
use crate::workflow::AnalysisOutput;

/// Spearman's rank correlation and its p-value.
///
/// The correlation is the Pearson correlation of the mid-ranks; the p-value
/// uses the usual t approximation with `n − 2` degrees of freedom.
pub fn spearman(x: &[f64], y: &[f64]) -> (f64, f64) {
    let n = x.len();
    if n < 3 {
        return (f64::NAN, f64::NAN);
    }
    let rank_x = mid_ranks(x);
    let rank_y = mid_ranks(y);
    let rho = pearson(&rank_x, &rank_y);
    if !rho.is_finite() || rho.abs() >= 1.0 {
        return (rho, 0.0);
    }
    let df = (n - 2) as f64;
    let t = rho * (df / (1.0 - rho * rho)).sqrt();
    let cdf = student_t_cdf(t, df);
    let p = (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0);
    (rho, p)
}

/// Kendall's tau-b and its p-value.
///
/// `tau_b = (C − D)/√((n₀ − n₁)(n₀ − n₂))` with the concordant and
/// discordant pair counts `C` and `D`; the p-value uses the normal
/// approximation `z = tau_b·√(9n(n−1)/(2n+3))`.
pub fn kendall(x: &[f64], y: &[f64]) -> (f64, f64) {
    let n = x.len();
    if n < 3 {
        return (f64::NAN, f64::NAN);
    }
    let mut concordant = 0.0;
    let mut discordant = 0.0;
    let mut tied_x = 0.0;
    let mut tied_y = 0.0;
    for i in 0..n {
        for j in (i + 1)..n {
            let dx = x[i] - x[j];
            let dy = y[i] - y[j];
            let product = dx * dy;
            if product > 0.0 {
                concordant += 1.0;
            } else if product < 0.0 {
                discordant += 1.0;
            } else {
                if dx == 0.0 {
                    tied_x += 1.0;
                }
                if dy == 0.0 {
                    tied_y += 1.0;
                }
            }
        }
    }
    let pairs = n as f64 * (n as f64 - 1.0) / 2.0;
    let denominator = ((pairs - tied_x) * (pairs - tied_y)).sqrt();
    if denominator <= 0.0 {
        return (f64::NAN, f64::NAN);
    }
    let tau = (concordant - discordant) / denominator;
    let nf = n as f64;
    let z = tau * (9.0 * nf * (nf - 1.0) / (2.0 * nf + 3.0)).sqrt();
    let cdf = normal_cdf(z);
    let p = (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0);
    (tau, p)
}

/// Pearson correlation of two samples of the same length.
pub fn pearson_correlation(x: &[f64], y: &[f64]) -> f64 {
    pearson(x, y)
}

/// Pearson correlation of two samples of the same length.
fn pearson(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len();
    if n < 2 {
        return f64::NAN;
    }
    let mean_x = x.iter().sum::<f64>() / n as f64;
    let mean_y = y.iter().sum::<f64>() / n as f64;
    let mut covariance = 0.0;
    let mut variance_x = 0.0;
    let mut variance_y = 0.0;
    for (&xi, &yi) in x.iter().zip(y) {
        let dx = xi - mean_x;
        let dy = yi - mean_y;
        covariance += dx * dy;
        variance_x += dx * dx;
        variance_y += dy * dy;
    }
    if variance_x <= 0.0 || variance_y <= 0.0 {
        return f64::NAN;
    }
    covariance / (variance_x * variance_y).sqrt()
}

/// Paired scatter data and correlation.
#[derive(Debug, Clone, PartialEq)]
pub struct ScatterFit {
    /// X observations.
    pub x: Vec<f64>,
    /// Y observations.
    pub y: Vec<f64>,
    /// Pairwise rows excluded.
    pub excluded: usize,
    /// Pearson correlation.
    pub r: f64,
    /// Two-sided p-value for `r = 0`.
    pub p_value: f64,
    /// Least-squares slope.
    pub slope: f64,
    /// Least-squares intercept.
    pub intercept: f64,
    /// Whether to draw the fit line.
    pub fit_line: bool,
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

/// Builds the paired data and correlation.
pub fn fit(df: &DataFrame, x_column: &str, y_column: &str, fit_line: bool) -> Result<ScatterFit> {
    if x_column == y_column {
        return Err(Error::Unsupported(
            "the X and Y columns must be different".to_owned(),
        ));
    }
    let x = df.column(x_column)?;
    let y = df.column(y_column)?;
    if matches!(
        x.data(),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
    ) || matches!(
        y.data(),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
    ) {
        return Err(Error::Csv("scatterplot columns must be numeric".to_owned()));
    }
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for row in 0..df.n_rows() {
        if let (Some(xv), Some(yv)) = (cell(x, row), cell(y, row)) {
            xs.push(xv);
            ys.push(yv);
        }
    }
    let n = xs.len();
    if n < 3 {
        return Err(Error::SampleTooSmall { n, min: 3 });
    }
    let mean_x = xs.iter().sum::<f64>() / n as f64;
    let mean_y = ys.iter().sum::<f64>() / n as f64;
    let sxx = xs.iter().map(|value| (value - mean_x).powi(2)).sum::<f64>();
    let syy = ys.iter().map(|value| (value - mean_y).powi(2)).sum::<f64>();
    let sxy = xs
        .iter()
        .zip(&ys)
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum::<f64>();
    if sxx <= 0.0 || syy <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let slope = sxy / sxx;
    let intercept = mean_y - slope * mean_x;
    let r = (sxy / (sxx * syy).sqrt()).clamp(-1.0, 1.0);
    let p_value = if r.abs() >= 1.0 {
        0.0
    } else {
        let t = r * ((n as f64 - 2.0) / (1.0 - r * r)).sqrt();
        let cdf = student_t_cdf(t, n as f64 - 2.0);
        (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0)
    };
    Ok(ScatterFit {
        x: xs,
        y: ys,
        excluded: df.n_rows() - n,
        r,
        p_value,
        slope,
        intercept,
        fit_line,
    })
}

/// Builds the report and plot.
pub fn analyze(
    df: &DataFrame,
    x_column: &str,
    y_column: &str,
    fit_line: bool,
) -> Result<AnalysisOutput> {
    let model = fit(df, x_column, y_column, fit_line)?;
    let plot = plot(x_column, y_column, &model);
    Ok(AnalysisOutput {
        results: vec![report(x_column, y_column, &model)],
        plot: Some(plot),
    })
}

fn report(x_column: &str, y_column: &str, model: &ScatterFit) -> AnalysisResult {
    let (rho, rho_p) = spearman(&model.x, &model.y);
    let (tau, tau_p) = kendall(&model.x, &model.y);
    let mut result = AnalysisResult::new(
        format!("Scatterplot — {y_column} vs {x_column}"),
        "Pearson correlation, rank correlations and ordinary least-squares line",
    );
    result.n = model.x.len();
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Pearson r", model.r),
        ("p-value", model.p_value),
        ("Spearman rho", rho),
        ("p-value (Spearman)", rho_p),
        ("Kendall tau-b", tau),
        ("p-value (Kendall)", tau_p),
        ("R²", model.r * model.r),
        ("Slope", model.slope),
        ("Intercept", model.intercept),
    ];
    result.extra.push(Item::Table {
        headers: vec!["Quantity".to_owned(), "Value".to_owned()],
        rows: vec![
            vec!["Pearson r".to_owned(), format!("{:.6}", model.r)],
            vec!["p-value".to_owned(), format!("{:.6}", model.p_value)],
            vec!["Spearman rho".to_owned(), format!("{rho:.6}")],
            vec!["p-value (Spearman)".to_owned(), format!("{rho_p:.6}")],
            vec!["Kendall tau-b".to_owned(), format!("{tau:.6}")],
            vec!["p-value (Kendall)".to_owned(), format!("{tau_p:.6}")],
            vec!["R²".to_owned(), format!("{:.6}", model.r * model.r)],
            vec!["Slope".to_owned(), format!("{:.6}", model.slope)],
            vec!["Intercept".to_owned(), format!("{:.6}", model.intercept)],
        ],
    });
    result.notes = vec![
        (
            "Correlation".to_owned(),
            "Pearson r; two-sided t test with n−2 df under bivariate normality".to_owned(),
        ),
        (
            "Rank correlations".to_owned(),
            "Spearman rho (Pearson correlation of the mid-ranks, t approximation with n−2 df) and \
             Kendall tau-b (normal approximation)"
                .to_owned(),
        ),
        (
            "Line".to_owned(),
            if model.fit_line {
                "least-squares line y = a + bx drawn over the observed X range".to_owned()
            } else {
                "not drawn (fit disabled)".to_owned()
            },
        ),
        (
            "Data".to_owned(),
            "rows missing either X or Y are removed pairwise and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, model.excluded, 0);
    result.warnings.push(
        "scatterplot/correlation conventions have not been verified against captured output"
            .to_owned(),
    );
    result
}

fn plot(x_column: &str, y_column: &str, model: &ScatterFit) -> PlotSpec {
    let x_min = model.x.iter().copied().fold(f64::INFINITY, f64::min);
    let x_max = model.x.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let y_min = model.y.iter().copied().fold(f64::INFINITY, f64::min);
    let y_max = model.y.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let x_pad = ((x_max - x_min).abs() * 0.05).max(1e-9);
    let y_pad = ((y_max - y_min).abs() * 0.05).max(1e-9);
    let lines = if model.fit_line {
        vec![Segment {
            from: Point {
                x: x_min,
                y: model.intercept + model.slope * x_min,
            },
            to: Point {
                x: x_max,
                y: model.intercept + model.slope * x_max,
            },
            width: 2.0,
        }]
    } else {
        Vec::new()
    };
    PlotSpec {
        title: format!("Scatterplot — {y_column} vs {x_column}"),
        x_axis: linear_axis(x_column, x_min - x_pad, x_max + x_pad),
        y_axis: linear_axis(y_column, y_min - y_pad, y_max + y_pad),
        size: (900, 560),
        points: model
            .x
            .iter()
            .zip(&model.y)
            .map(|(&x, &y)| Point { x, y })
            .collect(),
        lines,
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
    fn spearman_detects_a_monotone_relation() {
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let y = vec![2.0, 4.0, 6.0, 8.0, 10.0, 12.0];
        let (rho, p) = spearman(&x, &y);
        assert!((rho - 1.0).abs() < 1e-12, "rho {rho}");
        assert!(p < 0.01, "p {p}");
    }

    #[test]
    fn spearman_and_kendall_survive_ties() {
        let x = vec![1.0, 2.0, 2.0, 3.0, 4.0];
        let y = vec![2.0, 2.0, 3.0, 4.0, 5.0];
        let (rho, rho_p) = spearman(&x, &y);
        assert!(rho.is_finite() && rho > 0.8, "rho {rho}");
        assert!(rho_p < 0.2, "p {rho_p}");
        let (tau, tau_p) = kendall(&x, &y);
        assert!(tau.is_finite() && tau > 0.7, "tau {tau}");
        assert!(tau_p < 0.2, "p {tau_p}");
    }

    #[test]
    fn kendall_detects_a_reversed_relation() {
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let y = vec![5.0, 4.0, 3.0, 2.0, 1.0];
        let (tau, p) = kendall(&x, &y);
        assert!((tau + 1.0).abs() < 1e-12, "tau {tau}");
        assert!(p < 0.05, "p {p}");
    }

    #[test]
    fn kendall_of_a_constant_sample_is_not_defined() {
        let x = vec![1.0, 1.0, 1.0];
        let y = vec![1.0, 2.0, 3.0];
        let (tau, p) = kendall(&x, &y);
        assert!(tau.is_nan());
        assert!(p.is_nan());
    }

    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("y", vec![2.0, 4.0, 6.0, 8.0, 10.0]),
        ])
        .unwrap()
    }

    #[test]
    fn perfect_line_has_unit_correlation() {
        let model = fit(&frame(), "x", "y", true).expect("fit");
        assert!((model.r - 1.0).abs() < 1e-12);
        assert!((model.slope - 2.0).abs() < 1e-12);
        assert!((model.intercept).abs() < 1e-12);
        assert_eq!(model.p_value, 0.0);
    }

    #[test]
    fn fit_line_can_be_disabled() {
        let model = fit(&frame(), "x", "y", false).expect("fit");
        assert!(!model.fit_line);
    }

    #[test]
    fn constant_x_is_rejected() {
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 1.0, 1.0]),
            Column::new_f64("y", vec![1.0, 2.0, 3.0]),
        ])
        .unwrap();
        assert!(matches!(
            fit(&df, "x", "y", true),
            Err(Error::ConstantSample)
        ));
    }

    #[test]
    fn report_and_plot_are_built() {
        let out = analyze(&frame(), "x", "y", true).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Scatterplot"), "{text}");
        assert!(text.contains("Pearson r"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.points.len(), 5);
        assert_eq!(plot.lines.len(), 1);
    }
}
