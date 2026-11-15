// SPDX-License-Identifier: MIT
//! Partial least squares regression (PLS1) by NIPALS.
//!
//! One response and one or more numeric predictors are centred and scaled.
//! Every component extracts the direction `w = Xᵀy/||Xᵀy||`, the scores
//! `t = Xw`, the X loadings `p = Xᵀt/(tᵀt)` and the response loading
//! `q = yᵀt/(tᵀt)`, and deflates `X` and `y`. The cumulative coefficients are
//! `B = W(PᵀW)⁻¹q` on the standardised scale and are converted back to the
//! original predictor units.
//!
//! Sources: H. Wold (1966); Wold, Ruhe, Wold & Dunn (1984); Tenenhaus (1998).
//! Cross-validation, Q², VIP and PLS2 are outside this implementation.

use crate::data::{DType, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis, panel_plot};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// Fitted PLS1 model.
#[derive(Debug, Clone, PartialEq)]
pub struct PlsFit {
    /// Response column.
    pub response: String,
    /// Predictor columns.
    pub predictors: Vec<String>,
    /// Components requested.
    pub requested: usize,
    /// Components actually extracted.
    pub components: usize,
    /// Predictor means.
    pub x_mean: Vec<f64>,
    /// Predictor standard deviations (`n−1`).
    pub x_sd: Vec<f64>,
    /// Response mean.
    pub y_mean: f64,
    /// Response standard deviation (`n−1`).
    pub y_sd: f64,
    /// Weights of every component (one vector per component).
    pub weights: Vec<Vec<f64>>,
    /// X loadings of every component.
    pub loadings: Vec<Vec<f64>>,
    /// Response loading of every component.
    pub y_loadings: Vec<f64>,
    /// Scores of every component.
    pub scores: Vec<Vec<f64>>,
    /// Coefficients on the original predictor scale.
    pub coefficients: Vec<f64>,
    /// Intercept on the original scale.
    pub intercept: f64,
    /// Cumulative R² after each component.
    pub r2_by_component: Vec<f64>,
    /// Final coefficient of determination.
    pub r_squared: f64,
    /// Fitted values.
    pub fitted: Vec<f64>,
    /// Residuals.
    pub residuals: Vec<f64>,
    /// Observations used.
    pub n: usize,
    /// Rows removed listwise.
    pub excluded: usize,
}

/// Solves a small dense linear system with partial pivoting.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = a.len();
    if n == 0 || b.len() != n {
        return None;
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
        b.swap(column, pivot);
        let divisor = a[column][column];
        for value in a[column].iter_mut().skip(column) {
            *value /= divisor;
        }
        b[column] /= divisor;
        for row in 0..n {
            if row == column {
                continue;
            }
            let factor = a[row][column];
            if factor == 0.0 {
                continue;
            }
            for k in column..n {
                a[row][k] -= factor * a[column][k];
            }
            b[row] -= factor * b[column];
        }
    }
    Some(b)
}

/// Cumulative standardised coefficients from the first `count` components.
fn standardized_coefficients(
    weights: &[Vec<f64>],
    loadings: &[Vec<f64>],
    y_loadings: &[f64],
    count: usize,
) -> Result<Vec<f64>> {
    let p = weights[0].len();
    let mut matrix = vec![vec![0.0; count]; count];
    for i in 0..count {
        for j in 0..count {
            matrix[i][j] = (0..p).map(|k| loadings[i][k] * weights[j][k]).sum();
        }
    }
    let z = solve(matrix, y_loadings[..count].to_vec()).ok_or_else(|| Error::NotConverged {
        detail: "the PLS loadings are singular".to_owned(),
    })?;
    let mut coefficients = vec![0.0; p];
    for (j, coefficient) in coefficients.iter_mut().enumerate() {
        *coefficient = (0..count).map(|a| weights[a][j] * z[a]).sum();
    }
    Ok(coefficients)
}

/// Fits a PLS1 model.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for an empty, duplicated or non-numeric
/// predictor list, a constant predictor or response, or an invalid component
/// count, and [`Error::SampleTooSmall`] for too few complete rows.
pub fn fit(
    df: &DataFrame,
    response: &str,
    predictors: &[String],
    components: usize,
) -> Result<PlsFit> {
    if predictors.is_empty() {
        return Err(Error::Unsupported(
            "choose at least one predictor".to_owned(),
        ));
    }
    let mut unique = predictors.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != predictors.len() {
        return Err(Error::Unsupported(
            "the predictor list contains duplicates".to_owned(),
        ));
    }
    if predictors.iter().any(|name| name == response) {
        return Err(Error::Unsupported(
            "the response cannot also be a predictor".to_owned(),
        ));
    }
    if components == 0 {
        return Err(Error::Unsupported(
            "the model needs at least one component".to_owned(),
        ));
    }
    let response_column = df.column(response)?;
    if !matches!(response_column.dtype(), DType::Float | DType::Integer) {
        return Err(Error::Unsupported(
            "the response must be numeric".to_owned(),
        ));
    }
    let mut columns = Vec::with_capacity(predictors.len());
    for name in predictors {
        let column = df.column(name)?;
        if !matches!(column.dtype(), DType::Float | DType::Integer) {
            return Err(Error::Unsupported(format!(
                "the predictor {name} is not numeric"
            )));
        }
        columns.push(column);
    }
    let mut xs: Vec<Vec<f64>> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for row in 0..df.n_rows() {
        let Some(y) = response_column.value(row).filter(|value| value.is_finite()) else {
            continue;
        };
        let mut values = Vec::with_capacity(columns.len());
        let mut complete = true;
        for column in &columns {
            match column.value(row).filter(|value| value.is_finite()) {
                Some(value) => values.push(value),
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if complete {
            xs.push(values);
            ys.push(y);
        }
    }
    let n = ys.len();
    let p = predictors.len();
    if n < 3 {
        return Err(Error::SampleTooSmall { n, min: 3 });
    }
    if components > p || components >= n {
        return Err(Error::Unsupported(format!(
            "the model accepts at most {p} components with {n} observations"
        )));
    }
    let y_mean = ys.iter().sum::<f64>() / n as f64;
    let y_sd =
        (ys.iter().map(|value| (value - y_mean).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt();
    if !y_sd.is_finite() || y_sd <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let mut x_mean = vec![0.0; p];
    let mut x_sd = vec![0.0; p];
    for j in 0..p {
        let mean = xs.iter().map(|row| row[j]).sum::<f64>() / n as f64;
        let sd =
            (xs.iter().map(|row| (row[j] - mean).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt();
        if !sd.is_finite() || sd <= 0.0 {
            return Err(Error::Unsupported(format!(
                "the predictor {} is constant",
                predictors[j]
            )));
        }
        x_mean[j] = mean;
        x_sd[j] = sd;
    }
    let mut x: Vec<Vec<f64>> = xs
        .iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(j, value)| (value - x_mean[j]) / x_sd[j])
                .collect()
        })
        .collect();
    let mut y: Vec<f64> = ys.iter().map(|value| (value - y_mean) / y_sd).collect();

    let mut weights = Vec::new();
    let mut loadings = Vec::new();
    let mut y_loadings = Vec::new();
    let mut scores = Vec::new();
    for _ in 0..components {
        let mut weight = vec![0.0; p];
        for (row, target) in x.iter().zip(&y) {
            for (j, value) in row.iter().enumerate() {
                weight[j] += value * target;
            }
        }
        let norm = weight.iter().map(|value| value * value).sum::<f64>().sqrt();
        if !norm.is_finite() || norm < 1e-12 {
            break;
        }
        for value in &mut weight {
            *value /= norm;
        }
        let score: Vec<f64> = x
            .iter()
            .map(|row| row.iter().zip(&weight).map(|(value, w)| value * w).sum())
            .collect();
        let score_ss = score.iter().map(|value| value * value).sum::<f64>();
        if !score_ss.is_finite() || score_ss <= 1e-12 {
            break;
        }
        let mut loading = vec![0.0; p];
        for (row, t) in x.iter().zip(&score) {
            for (j, value) in row.iter().enumerate() {
                loading[j] += value * t;
            }
        }
        for value in &mut loading {
            *value /= score_ss;
        }
        let q = y
            .iter()
            .zip(&score)
            .map(|(value, t)| value * t)
            .sum::<f64>()
            / score_ss;
        for (row, t) in x.iter_mut().zip(&score) {
            for (j, value) in row.iter_mut().enumerate() {
                *value -= t * loading[j];
            }
        }
        for (value, t) in y.iter_mut().zip(&score) {
            *value -= t * q;
        }
        weights.push(weight);
        loadings.push(loading);
        y_loadings.push(q);
        scores.push(score);
    }
    if weights.is_empty() {
        return Err(Error::NotConverged {
            detail: "the PLS algorithm extracted no component".to_owned(),
        });
    }
    let extracted = weights.len();
    let mut r2_by_component = Vec::with_capacity(extracted);
    let mut final_coefficients = vec![0.0; p];
    let mut final_intercept = y_mean;
    let mut final_fitted = vec![y_mean; n];
    for count in 1..=extracted {
        let standardized = standardized_coefficients(&weights, &loadings, &y_loadings, count)?;
        let coefficients: Vec<f64> = standardized
            .iter()
            .enumerate()
            .map(|(j, value)| value * y_sd / x_sd[j])
            .collect();
        let intercept = y_mean
            - coefficients
                .iter()
                .enumerate()
                .map(|(j, value)| value * x_mean[j])
                .sum::<f64>();
        let fitted: Vec<f64> = xs
            .iter()
            .map(|row| {
                intercept
                    + row
                        .iter()
                        .zip(&coefficients)
                        .map(|(value, coefficient)| value * coefficient)
                        .sum::<f64>()
            })
            .collect();
        let sse = ys
            .iter()
            .zip(&fitted)
            .map(|(value, fitted)| (value - fitted).powi(2))
            .sum::<f64>();
        let sst = ys.iter().map(|value| (value - y_mean).powi(2)).sum::<f64>();
        r2_by_component.push((1.0 - sse / sst).clamp(0.0, 1.0));
        final_coefficients = coefficients;
        final_intercept = intercept;
        final_fitted = fitted;
    }
    let residuals: Vec<f64> = ys
        .iter()
        .zip(&final_fitted)
        .map(|(value, fitted)| value - fitted)
        .collect();
    let r_squared = *r2_by_component.last().unwrap_or(&0.0);
    Ok(PlsFit {
        response: response.to_owned(),
        predictors: predictors.to_vec(),
        requested: components,
        components: extracted,
        x_mean,
        x_sd,
        y_mean,
        y_sd,
        weights,
        loadings,
        y_loadings,
        scores,
        coefficients: final_coefficients,
        intercept: final_intercept,
        r2_by_component,
        r_squared,
        fitted: final_fitted,
        residuals,
        n,
        excluded: df.n_rows() - n,
    })
}

/// Fits the model, builds the report and the selection/agreement plots.
///
/// # Errors
///
/// As [`fit`].
pub fn analyze(
    df: &DataFrame,
    response: &str,
    predictors: &[String],
    components: usize,
) -> Result<AnalysisOutput> {
    let model = fit(df, response, predictors, components)?;
    Ok(AnalysisOutput {
        results: vec![report(&model)],
        plot: plot(&model),
    })
}

fn report(model: &PlsFit) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("PLS Regression — response {}", model.response),
        format!(
            "PLS1 by NIPALS; predictors centred and scaled; {} component(s)",
            model.components
        ),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Predictors", model.predictors.len() as f64),
        ("Components requested", model.requested as f64),
        ("Components fitted", model.components as f64),
        ("R²", model.r_squared),
        ("Intercept", model.intercept),
    ];
    let mut headers = vec![
        "Predictor".to_owned(),
        "Coefficient".to_owned(),
        "Mean".to_owned(),
        "Scale".to_owned(),
    ];
    for component in 1..=model.components {
        headers.push(format!("Weight {component}"));
    }
    let rows = model
        .predictors
        .iter()
        .enumerate()
        .map(|(j, name)| {
            let mut row = vec![
                name.clone(),
                format!("{:.6}", model.coefficients[j]),
                format!("{:.6}", model.x_mean[j]),
                format!("{:.6}", model.x_sd[j]),
            ];
            for component in 0..model.components {
                row.push(format!("{:.6}", model.weights[component][j]));
            }
            row
        })
        .collect();
    result.extra.push(Item::Table { headers, rows });
    result.extra.push(Item::Table {
        headers: vec![
            "Component".to_owned(),
            "R²".to_owned(),
            "R² added".to_owned(),
        ],
        rows: model
            .r2_by_component
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let previous = if index == 0 {
                    0.0
                } else {
                    model.r2_by_component[index - 1]
                };
                vec![
                    (index + 1).to_string(),
                    format!("{value:.6}"),
                    format!("{:.6}", value - previous),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Model".to_owned(),
            "y is centred and scaled; each component uses w = Xᵀy/||Xᵀy||, t = Xw, \
             p = Xᵀt/(tᵀt), q = yᵀt/(tᵀt) and deflates X and y"
                .to_owned(),
        ),
        (
            "Coefficients".to_owned(),
            "B = W(PᵀW)⁻¹q on the standardised scale; the report converts B back to the original \
             predictor units and gives the intercept"
                .to_owned(),
        ),
        (
            "Scope".to_owned(),
            "PLS1 with one response; cross-validation/Q², VIP, PLS2 and automatic component \
             selection are outside this implementation"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing response or predictor are removed listwise; a constant predictor \
             is an error"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    result.warnings.push(
        "PLS is a descriptive latent-variable fit; the number of components is chosen by the \
         user and no documented output comparison has been made"
            .to_owned(),
    );
    result
}

fn plot(model: &PlsFit) -> Option<PlotSpec> {
    let y_values = model.fitted.iter().copied().chain(
        model
            .residuals
            .iter()
            .zip(&model.fitted)
            .map(|(residual, fitted)| fitted + residual),
    );
    let low = y_values.clone().fold(f64::INFINITY, f64::min);
    let high = y_values.fold(f64::NEG_INFINITY, f64::max);
    let pad = ((high - low).abs() * 0.06).max(1e-9);
    let observed: Vec<f64> = model
        .fitted
        .iter()
        .zip(&model.residuals)
        .map(|(fitted, residual)| fitted + residual)
        .collect();
    let agreement = PlotSpec {
        title: "Observed vs fitted".to_owned(),
        x_axis: linear_axis("Observed", low - pad, high + pad),
        y_axis: linear_axis("Fitted", low - pad, high + pad),
        size: (420, 360),
        points: observed
            .iter()
            .zip(&model.fitted)
            .map(|(observed, fitted)| Point {
                x: *observed,
                y: *fitted,
            })
            .collect(),
        lines: vec![Segment {
            from: Point {
                x: low - pad,
                y: low - pad,
            },
            to: Point {
                x: high + pad,
                y: high + pad,
            },
            width: 1.2,
        }],
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    };
    let selection_points: Vec<Point> = model
        .r2_by_component
        .iter()
        .enumerate()
        .map(|(index, value)| Point {
            x: index as f64 + 1.0,
            y: *value,
        })
        .collect();
    let selection = PlotSpec {
        title: "R² by component".to_owned(),
        x_axis: linear_axis("Component", 0.5, model.components as f64 + 0.5),
        y_axis: linear_axis("R²", 0.0, 1.0),
        size: (420, 360),
        points: selection_points.clone(),
        lines: selection_points
            .windows(2)
            .map(|pair| Segment {
                from: pair[0],
                to: pair[1],
                width: 2.0,
            })
            .collect(),
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    };
    panel_plot(
        vec![agreement, selection],
        2,
        format!("PLS Regression — {}", model.response),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        let mut x1 = Vec::new();
        let mut x2 = Vec::new();
        let mut x3 = Vec::new();
        let mut y = Vec::new();
        for index in 0..30 {
            let first = (index % 7) as f64 + 1.0;
            let second = ((index * 3) % 5) as f64 + 1.0;
            let noise = ((index * 11 % 5) as f64 - 2.0) * 0.02;
            x1.push(first);
            x2.push(second);
            x3.push(first + second);
            y.push(2.0 + 3.0 * first - 1.5 * second + noise);
        }
        DataFrame::new(vec![
            Column::new_f64("x1", x1),
            Column::new_f64("x2", x2),
            Column::new_f64("x3", x3),
            Column::new_f64("y", y),
        ])
        .expect("frame")
    }

    #[test]
    fn the_pls_fit_recovers_the_linear_relation() {
        let df = frame();
        let predictors = ["x1".to_owned(), "x2".to_owned()];
        let model = fit(&df, "y", &predictors, 2).expect("fit");
        assert_eq!(model.components, 2);
        assert!(model.r_squared > 0.99, "{}", model.r_squared);
        assert!(
            (model.coefficients[0] - 3.0).abs() < 0.1,
            "{:?}",
            model.coefficients
        );
        assert!(
            (model.coefficients[1] + 1.5).abs() < 0.1,
            "{:?}",
            model.coefficients
        );
        assert!((model.intercept - 2.0).abs() < 0.3, "{}", model.intercept);
        assert_eq!(model.r2_by_component.len(), 2);
        assert!(model.r2_by_component[1] >= model.r2_by_component[0]);
    }

    #[test]
    fn a_collinear_predictor_is_handled_with_one_component() {
        let df = frame();
        let predictors = ["x1".to_owned(), "x2".to_owned(), "x3".to_owned()];
        let model = fit(&df, "y", &predictors, 1).expect("fit");
        assert_eq!(model.components, 1);
        assert!(model.r_squared > 0.9, "{}", model.r_squared);
        assert!(model.coefficients.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn the_report_and_the_two_panels_are_built() {
        let out = analyze(&frame(), "y", &["x1".to_owned(), "x2".to_owned()], 2).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("PLS Regression"), "{text}");
        assert!(text.contains("R² added"), "{text}");
        assert!(text.contains("Weight 2"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.panels.len(), 2);
    }

    #[test]
    fn invalid_pls_requests_are_rejected() {
        let df = frame();
        assert!(fit(&df, "y", &[], 1).is_err());
        assert!(fit(&df, "y", &["x1".to_owned(), "x1".to_owned()], 1).is_err());
        assert!(fit(&df, "y", &["y".to_owned()], 1).is_err());
        assert!(fit(&df, "y", &["x1".to_owned()], 0).is_err());
        assert!(fit(&df, "y", &["x1".to_owned()], 2).is_err());
        let constant = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0; 20]),
            Column::new_f64("y", (0..20).map(|i| i as f64).collect()),
        ])
        .expect("frame");
        assert!(fit(&constant, "y", &["x".to_owned()], 1).is_err());
    }
}
