//! Curve fit: polynomial, exponential and power models.
//!
//! The polynomial model is fitted by ordinary least squares on the powers of
//! X. The exponential and the power model are fitted in the log scale, where
//! they are linear, and the report gives the goodness of fit on the original
//! scale as well — the R² of a log-scale fit is not the R² of the data.
//!
//! The comparison run has a curve-fit dialog; its model list and defaults
//! were not captured, so this is an addition of this engine rather than a
//! recovered behaviour — see `docs/parity.md`.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::regression;
use crate::report::{AnalysisResult, Item, standard_warnings};

/// Model of a curve fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveModel {
    /// Polynomial of the given degree: `y = a₀ + a₁x + a₂x² + …`.
    Polynomial(usize),
    /// Exponential: `y = a·e^{b·x}`.
    Exponential,
    /// Power: `y = a·x^b`.
    Power,
    /// Four-parameter logistic: `y = a + (b − a)/(1 + exp(−(x − c)/d))`.
    Logistic,
}

impl CurveModel {
    /// Name used in reports.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Polynomial(degree) => format!("Polynomial (degree {degree})"),
            Self::Exponential => "Exponential".to_owned(),
            Self::Power => "Power".to_owned(),
            Self::Logistic => "Logistic (4-parameter)".to_owned(),
        }
    }

    /// Stable identifier for the CLI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Polynomial(_) => "polynomial",
            Self::Exponential => "exponential",
            Self::Power => "power",
            Self::Logistic => "logistic",
        }
    }

    /// Parses the CLI spelling; a polynomial accepts the degree.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let lowered = text.to_ascii_lowercase();
        match lowered.as_str() {
            "exponential" => Some(Self::Exponential),
            "power" => Some(Self::Power),
            "logistic" | "logistic4" => Some(Self::Logistic),
            "polynomial" => Some(Self::Polynomial(2)),
            _ => lowered
                .strip_prefix("polynomial")
                .and_then(|rest| rest.trim_start_matches(['-', ' ', ':']).parse().ok())
                .map(Self::Polynomial)
                .or_else(|| {
                    lowered
                        .strip_prefix("poly")
                        .and_then(|rest| rest.trim_start_matches(['-', ' ', ':']).parse().ok())
                        .map(Self::Polynomial)
                }),
        }
    }
}

/// Outcome of a curve fit.
#[derive(Debug, Clone, PartialEq)]
pub struct CurveFit {
    /// Model that was fitted.
    pub model: CurveModel,
    /// Names of the coefficients.
    pub names: Vec<String>,
    /// Estimates of the coefficients, in the original scale.
    pub coefficients: Vec<f64>,
    /// Standard errors of the coefficients.
    pub standard_errors: Vec<f64>,
    /// Two-sided p-values of the coefficients.
    pub p_values: Vec<f64>,
    /// R² on the original scale.
    pub r_squared: f64,
    /// Residual standard error on the original scale.
    pub residual_standard_error: f64,
    /// Observations used.
    pub n: usize,
    /// Rows excluded because a coordinate was missing.
    pub excluded: usize,
    /// Complete pairs used by the fit.
    pub x: Vec<f64>,
    /// Complete pairs used by the fit.
    pub y: Vec<f64>,
    /// Fitted values on the original scale.
    pub fitted: Vec<f64>,
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

/// Fits the model to the complete pairs of the two columns.
pub fn fit(
    df: &DataFrame,
    x_column: &str,
    y_column: &str,
    model: CurveModel,
    confidence: f64,
) -> Result<CurveFit> {
    if x_column == y_column {
        return Err(Error::Unsupported(
            "the X and Y columns must be different".to_owned(),
        ));
    }
    if let CurveModel::Polynomial(degree) = model
        && !(1..=5).contains(&degree)
    {
        return Err(Error::InvalidParameter {
            name: "polynomial degree",
            value: degree as f64,
        });
    }
    let x = df.column(x_column)?;
    let y = df.column(y_column)?;
    let rows = df.n_rows();
    let mut pairs: Vec<(f64, f64)> = Vec::new();
    for row in 0..rows {
        if let (Some(xv), Some(yv)) = (cell(x, row), cell(y, row)) {
            pairs.push((xv, yv));
        }
    }
    let excluded = rows - pairs.len();
    let needed = match model {
        CurveModel::Polynomial(degree) => degree + 2,
        _ => 3,
    };
    if pairs.len() < needed {
        return Err(Error::SampleTooSmall {
            n: pairs.len(),
            min: needed,
        });
    }

    let (names, coefficients, standard_errors, p_values) = match model {
        CurveModel::Polynomial(degree) => {
            let mut frame_columns = vec![crate::data::Column::new_f64(
                "y",
                pairs.iter().map(|(_, y)| *y).collect(),
            )];
            let mut names = vec!["Constant".to_owned()];
            for power in 1..=degree {
                let values: Vec<f64> = pairs.iter().map(|(x, _)| x.powi(power as i32)).collect();
                let name = if power == 1 {
                    "x".to_owned()
                } else {
                    format!("x^{power}")
                };
                frame_columns.push(crate::data::Column::new_f64(name.clone(), values));
                names.push(name);
            }
            let frame =
                DataFrame::new(frame_columns).map_err(|error| Error::Csv(error.to_string()))?;
            let predictors: Vec<String> = names[1..].to_vec();
            let fit = regression::fit(&frame, "y", &predictors, confidence)?;
            let mut names = vec!["Constant".to_owned()];
            names.extend(fit.names[1..].to_vec());
            (
                names,
                fit.coefficients.clone(),
                fit.standard_errors.clone(),
                fit.p_values.clone(),
            )
        }
        CurveModel::Exponential | CurveModel::Power => {
            let mut values = Vec::with_capacity(pairs.len());
            for (index, (xv, yv)) in pairs.iter().enumerate() {
                if *yv <= 0.0 {
                    return Err(Error::NonPositive { index, value: *yv });
                }
                let transformed_x = match model {
                    CurveModel::Exponential => *xv,
                    _ => {
                        if *xv <= 0.0 {
                            return Err(Error::NonPositive { index, value: *xv });
                        }
                        xv.ln()
                    }
                };
                values.push((transformed_x, yv.ln()));
            }
            let mut frame_columns = vec![crate::data::Column::new_f64(
                "ln y",
                values.iter().map(|(_, y)| *y).collect(),
            )];
            let name = match model {
                CurveModel::Exponential => "x".to_owned(),
                _ => "ln x".to_owned(),
            };
            frame_columns.push(crate::data::Column::new_f64(
                name.clone(),
                values.iter().map(|(x, _)| *x).collect(),
            ));
            let frame =
                DataFrame::new(frame_columns).map_err(|error| Error::Csv(error.to_string()))?;
            let fit = regression::fit(&frame, "ln y", std::slice::from_ref(&name), confidence)?;
            (
                vec!["Constant".to_owned(), name],
                fit.coefficients.clone(),
                fit.standard_errors.clone(),
                fit.p_values.clone(),
            )
        }
        CurveModel::Logistic => {
            let parameters = fit_logistic(&pairs)?;
            (
                vec![
                    "Lower asymptote".to_owned(),
                    "Upper asymptote".to_owned(),
                    "Location".to_owned(),
                    "Scale".to_owned(),
                ],
                parameters,
                Vec::new(),
                Vec::new(),
            )
        }
    };

    // Fitted values and the goodness of fit on the original scale.
    let mut fitted = Vec::with_capacity(pairs.len());
    for (xv, yv) in &pairs {
        let value = match model {
            CurveModel::Polynomial(_) => {
                // The coefficients multiply x, x², x³, …
                let mut power = *xv;
                let mut total = coefficients[0];
                for coefficient in coefficients.iter().skip(1) {
                    total += coefficient * power;
                    power *= xv;
                }
                total
            }
            CurveModel::Exponential => (coefficients[0] + coefficients[1] * xv).exp(),
            CurveModel::Power => (coefficients[0] + coefficients[1] * xv.ln()).exp(),
            CurveModel::Logistic => logistic_value(&coefficients, *xv),
        };
        fitted.push(value);
        let _ = yv;
    }
    let mean_y: f64 = pairs.iter().map(|(_, y)| y).sum::<f64>() / pairs.len() as f64;
    let total: f64 = pairs.iter().map(|(_, y)| (y - mean_y).powi(2)).sum();
    let residual: f64 = pairs
        .iter()
        .zip(&fitted)
        .map(|((_, y), f)| (y - f).powi(2))
        .sum();
    let degrees_of_freedom = (pairs.len() - coefficients.len()) as f64;
    let r_squared = if total > 0.0 {
        (1.0 - residual / total).clamp(0.0, 1.0)
    } else {
        f64::NAN
    };
    let residual_standard_error = if degrees_of_freedom > 0.0 {
        (residual / degrees_of_freedom).max(0.0).sqrt()
    } else {
        f64::NAN
    };

    Ok(CurveFit {
        model,
        names,
        coefficients,
        standard_errors,
        p_values,
        r_squared,
        residual_standard_error,
        n: pairs.len(),
        excluded,
        x: pairs.iter().map(|(x, _)| *x).collect(),
        y: pairs.iter().map(|(_, y)| *y).collect(),
        fitted,
    })
}

/// Builds the plot of a curve fit: the observations and the fitted curve.
pub fn plot(x_column: &str, fit: &CurveFit) -> PlotSpec {
    let x_min = fit.x.iter().copied().fold(f64::INFINITY, f64::min);
    let x_max = fit.x.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let y_min = fit.y.iter().copied().fold(f64::INFINITY, f64::min);
    let y_max = fit.y.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let x_pad = 0.05 * (x_max - x_min).abs().max(f64::MIN_POSITIVE);
    let y_pad = 0.05 * (y_max - y_min).abs().max(f64::MIN_POSITIVE);

    let points: Vec<Point> = fit
        .x
        .iter()
        .zip(&fit.y)
        .map(|(x, y)| Point { x: *x, y: *y })
        .collect();

    // The fitted curve is drawn over the observed X range.
    const STEPS: usize = 120;
    let mut lines = Vec::new();
    if x_max > x_min {
        let mut previous = Point {
            x: x_min,
            y: evaluate(fit, x_min),
        };
        for step in 1..=STEPS {
            let fraction = step as f64 / STEPS as f64;
            let x = x_min + (x_max - x_min) * fraction;
            let current = Point {
                x,
                y: evaluate(fit, x),
            };
            lines.push(Segment {
                from: previous,
                to: current,
                width: 2.0,
            });
            previous = current;
        }
    }

    PlotSpec {
        title: format!("Curve Fit — {}", fit.model.name()),
        x_axis: linear_axis(x_column, x_min - x_pad, x_max + x_pad),
        y_axis: linear_axis("Y", y_min - y_pad, y_max + y_pad),
        size: (900, 620),
        points,
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Evaluates the fitted model at `x`.
fn evaluate(fit: &CurveFit, x: f64) -> f64 {
    match fit.model {
        CurveModel::Polynomial(_) => {
            // The coefficients multiply x, x², x³, …
            let mut power = x;
            let mut total = fit.coefficients[0];
            for coefficient in fit.coefficients.iter().skip(1) {
                total += coefficient * power;
                power *= x;
            }
            total
        }
        CurveModel::Exponential => (fit.coefficients[0] + fit.coefficients[1] * x).exp(),
        CurveModel::Power => (fit.coefficients[0] + fit.coefficients[1] * x.ln()).exp(),
        CurveModel::Logistic => logistic_value(&fit.coefficients, x),
    }
}

/// Value of the four-parameter logistic at `x`.
///
/// `y = a + (b − a)/(1 + exp(−(x − c)/d))`.
fn logistic_value(coefficients: &[f64], x: f64) -> f64 {
    let (lower, upper, location, scale) = (
        coefficients[0],
        coefficients[1],
        coefficients[2],
        coefficients[3],
    );
    let exponent = (-(x - location) / scale).exp();
    lower + (upper - lower) / (1.0 + exponent)
}

/// Fits the four-parameter logistic with a multi-start Nelder–Mead search.
///
/// The starts are built from the data (the asymptotes from the extremes, the
/// location from the middle of the response and the scale from the width of
/// the X range) and the best of them is kept.
fn fit_logistic(pairs: &[(f64, f64)]) -> Result<Vec<f64>> {
    let x_min = pairs.iter().map(|(x, _)| *x).fold(f64::INFINITY, f64::min);
    let x_max = pairs
        .iter()
        .map(|(x, _)| *x)
        .fold(f64::NEG_INFINITY, f64::max);
    let y_min = pairs.iter().map(|(_, y)| *y).fold(f64::INFINITY, f64::min);
    let y_max = pairs
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::NEG_INFINITY, f64::max);
    let width = (x_max - x_min).abs().max(f64::MIN_POSITIVE);
    let mut best: Option<(Vec<f64>, f64)> = None;
    for scale_fraction in [0.1, 0.05, 0.25] {
        for location_fraction in [0.5, 0.35, 0.65] {
            let location = x_min + width * location_fraction;
            let start = vec![y_min, y_max, location, width * scale_fraction];
            let steps = vec![
                (y_max - y_min).abs().max(1.0) * 0.5,
                (y_max - y_min).abs().max(1.0) * 0.5,
                width * 0.2,
                width * 0.1,
            ];
            let objective = |point: &[f64]| -> f64 {
                if point[3] <= 0.0 || !point.iter().all(|value| value.is_finite()) {
                    return f64::INFINITY;
                }
                pairs
                    .iter()
                    .map(|(x, y)| {
                        let residual = y - logistic_value(point, *x);
                        residual * residual
                    })
                    .sum()
            };
            let result = crate::optimize::nelder_mead(&start, &steps, 2000, 1e-10, objective)?;
            if result.value.is_finite()
                && best.as_ref().is_none_or(|(_, value)| result.value < *value)
            {
                best = Some((result.arguments, result.value));
            }
        }
    }
    let (parameters, _) = best.ok_or(Error::NotConverged {
        detail: "four-parameter logistic fit".to_owned(),
    })?;
    Ok(parameters)
}

/// Builds the Session report of a curve fit.
pub fn report(x_column: &str, y_column: &str, fit: &CurveFit) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("Curve Fit — {y_column} vs {x_column}"),
        format!(
            "{}; {}",
            fit.model.name(),
            match fit.model {
                CurveModel::Logistic => "non-linear least squares",
                _ => "ordinary least squares",
            }
        ),
    );
    result.n = fit.n;
    result.excluded = fit.excluded;
    result.statistics = vec![
        ("R²", fit.r_squared),
        ("Residual standard error", fit.residual_standard_error),
    ];
    let with_standard_errors = fit.standard_errors.len() == fit.names.len();
    let mut headers = vec!["Term".to_owned(), "Coef".to_owned()];
    if with_standard_errors {
        headers.push("SE Coef".to_owned());
        headers.push("P".to_owned());
    }
    let rows = fit
        .names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let mut row = vec![name.clone(), format!("{:.6}", fit.coefficients[index])];
            if with_standard_errors {
                row.push(format!("{:.6}", fit.standard_errors[index]));
                row.push(format!("{:.6}", fit.p_values[index]));
            }
            row
        })
        .collect();
    result.extra.push(Item::Table { headers, rows });
    result
        .extra
        .push(Item::Message(format!("Fitted equation: {}", equation(fit))));
    result.notes = vec![
        (
            "Model".to_owned(),
            match fit.model {
                CurveModel::Polynomial(degree) => format!(
                    "y = a₀ + a₁x + … + a{degree}x^{degree}, fitted by ordinary least squares on the \
                     powers of X"
                ),
                CurveModel::Exponential => {
                    "y = a·e^{b·x}, fitted by ordinary least squares on ln y".to_owned()
                }
                CurveModel::Power => {
                    "y = a·x^b, fitted by ordinary least squares on ln y and ln x".to_owned()
                }
                CurveModel::Logistic => {
                    "y = a + (b − a)/(1 + exp(−(x − c)/d)), fitted by non-linear least squares \
                     (a multi-start Nelder–Mead search)"
                        .to_owned()
                }
            },
        ),
        (
            "Goodness of fit".to_owned(),
            "R² and the residual standard error are computed on the original scale; for the \
             exponential and the power model the coefficients come from the log scale"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows missing either coordinate are removed pairwise and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the curve-fit conventions are standard choices; they have not been verified against \
         captured output"
            .to_owned(),
    );
    result
}

/// The fitted equation in a readable form.
fn equation(fit: &CurveFit) -> String {
    match fit.model {
        CurveModel::Polynomial(_) => {
            let mut text = format!("y = {:.6}", fit.coefficients[0]);
            for (name, coefficient) in fit.names[1..].iter().zip(&fit.coefficients[1..]) {
                if *coefficient < 0.0 {
                    text.push_str(&format!(" − {:.6}·{name}", -coefficient));
                } else {
                    text.push_str(&format!(" + {:.6}·{name}", coefficient));
                }
            }
            text
        }
        CurveModel::Exponential => format!(
            "y = exp({:.6} + {:.6}·x)",
            fit.coefficients[0], fit.coefficients[1]
        ),
        CurveModel::Power => format!(
            "y = exp({:.6} + {:.6}·ln x)",
            fit.coefficients[0], fit.coefficients[1]
        ),
        CurveModel::Logistic => format!(
            "y = {:.6} + ({:.6} − {:.6})/(1 + exp(−(x − {:.6})/{:.6}))",
            fit.coefficients[0],
            fit.coefficients[1],
            fit.coefficients[0],
            fit.coefficients[2],
            fit.coefficients[3]
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    #[test]
    fn a_polynomial_fit_recovers_the_coefficients() {
        let x: Vec<f64> = (0..20).map(|i| i as f64 * 0.5).collect();
        let y: Vec<f64> = x.iter().map(|v| 1.0 + 2.0 * v + 0.5 * v * v).collect();
        let df =
            DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame");
        let fit = fit(&df, "x", "y", CurveModel::Polynomial(2), 0.95).expect("fit");
        assert!((fit.coefficients[0] - 1.0).abs() < 1e-8, "{fit:?}");
        assert!((fit.coefficients[1] - 2.0).abs() < 1e-8, "{fit:?}");
        assert!((fit.coefficients[2] - 0.5).abs() < 1e-8, "{fit:?}");
        assert!((fit.r_squared - 1.0).abs() < 1e-9, "R² {}", fit.r_squared);
    }

    #[test]
    fn an_exponential_fit_recovers_the_parameters() {
        let x: Vec<f64> = (0..20).map(|i| i as f64 * 0.25).collect();
        let y: Vec<f64> = x.iter().map(|v| 3.0 * (1.5 * v).exp()).collect();
        let df =
            DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame");
        let fit = fit(&df, "x", "y", CurveModel::Exponential, 0.95).expect("fit");
        // The coefficients live in the log scale: ln a = 1.0986 and b = 1.5.
        assert!((fit.coefficients[0] - 3.0f64.ln()).abs() < 1e-6, "{fit:?}");
        assert!((fit.coefficients[1] - 1.5).abs() < 1e-6, "{fit:?}");
        assert!((fit.r_squared - 1.0).abs() < 1e-9, "R² {}", fit.r_squared);
    }

    #[test]
    fn a_power_fit_recovers_the_parameters() {
        let x: Vec<f64> = (1..=20).map(|i| i as f64).collect();
        let y: Vec<f64> = x.iter().map(|v| 2.0 * v.powf(2.5)).collect();
        let df =
            DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame");
        let fit = fit(&df, "x", "y", CurveModel::Power, 0.95).expect("fit");
        assert!((fit.coefficients[0] - 2.0f64.ln()).abs() < 1e-6, "{fit:?}");
        assert!((fit.coefficients[1] - 2.5).abs() < 1e-6, "{fit:?}");
    }

    #[test]
    fn a_logistic_fit_recovers_the_parameters() {
        let truth = [1.0, 9.0, 5.0, 1.5];
        let x: Vec<f64> = (0..40).map(|i| i as f64 * 0.25).collect();
        let y: Vec<f64> = x
            .iter()
            .map(|v| {
                let exponent = (-(v - truth[2]) / truth[3]).exp();
                truth[0] + (truth[1] - truth[0]) / (1.0 + exponent)
            })
            .collect();
        let df =
            DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame");
        let fit = fit(&df, "x", "y", CurveModel::Logistic, 0.95).expect("fit");
        assert!((fit.coefficients[0] - 1.0).abs() < 0.2, "{fit:?}");
        assert!((fit.coefficients[1] - 9.0).abs() < 0.3, "{fit:?}");
        assert!((fit.coefficients[2] - 5.0).abs() < 0.3, "{fit:?}");
        assert!((fit.coefficients[3] - 1.5).abs() < 0.3, "{fit:?}");
        assert!((fit.r_squared - 1.0).abs() < 1e-6, "R² {}", fit.r_squared);
    }

    #[test]
    fn negative_values_are_refused_by_the_log_models() {
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_f64("y", vec![1.0, -2.0, 3.0, 4.0]),
        ])
        .expect("frame");
        assert!(matches!(
            fit(&df, "x", "y", CurveModel::Exponential, 0.95),
            Err(Error::NonPositive { .. })
        ));
    }

    #[test]
    fn the_model_names_are_parsed() {
        assert_eq!(
            CurveModel::parse("exponential"),
            Some(CurveModel::Exponential)
        );
        assert_eq!(CurveModel::parse("power"), Some(CurveModel::Power));
        assert_eq!(
            CurveModel::parse("polynomial3"),
            Some(CurveModel::Polynomial(3))
        );
        assert_eq!(CurveModel::parse("poly-2"), Some(CurveModel::Polynomial(2)));
        assert_eq!(CurveModel::parse("unknown"), None);
    }
}
