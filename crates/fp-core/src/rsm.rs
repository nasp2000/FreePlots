//! Quadratic response-surface analysis.
//!
//! The model contains all main effects, two-way interactions and squared terms
//! for coded numeric factors. It is fitted by the ordinary least squares QR
//! engine. Design generation lives in [`crate::doe`] (central composite design);
//! this module analyses an existing worksheet.

use std::collections::BTreeMap;

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, Series, linear_axis, panel_plot};
use crate::regression::{self, RegressionFit};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::f_cdf;
use crate::workflow::AnalysisOutput;

/// Fitted quadratic response surface.
#[derive(Debug, Clone, PartialEq)]
pub struct RsmFit {
    /// Regression fit with intercept and all quadratic terms.
    pub regression: RegressionFit,
    /// Rows excluded listwise.
    pub excluded: usize,
    /// Factor names in coded order.
    pub factors: Vec<String>,
    /// Lack-of-fit test, when replicated design points exist.
    pub lack_of_fit: Option<LackOfFit>,
    /// Observed range of each coded factor.
    pub ranges: Vec<(f64, f64)>,
    /// Design points in coded factor order.
    pub design_points: Vec<Vec<f64>>,
}

/// Lack-of-fit test against pure error from replicated design points.
#[derive(Debug, Clone, PartialEq)]
pub struct LackOfFit {
    /// Lack-of-fit sum of squares.
    pub ss: f64,
    /// Lack-of-fit degrees of freedom.
    pub df: f64,
    /// Lack-of-fit mean square.
    pub ms: f64,
    /// Pure-error sum of squares.
    pub pure_ss: f64,
    /// Pure-error degrees of freedom.
    pub pure_df: f64,
    /// F statistic.
    pub f: f64,
    /// p-value.
    pub p_value: f64,
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

/// Fits a full quadratic response-surface model.
pub fn fit(df: &DataFrame, response: &str, factors: &[String]) -> Result<RsmFit> {
    if factors.len() < 2 || factors.len() > 5 {
        return Err(Error::Unsupported(
            "response-surface analysis needs between two and five factors".to_owned(),
        ));
    }
    if factors.iter().any(|name| name == response) {
        return Err(Error::Unsupported(
            "the response cannot also be a factor".to_owned(),
        ));
    }
    let mut unique = factors.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != factors.len() {
        return Err(Error::Unsupported(
            "the factor list contains duplicates".to_owned(),
        ));
    }
    let response_column = df.column(response)?;
    if matches!(
        response_column.data(),
        ColumnData::Text(_) | ColumnData::Bool(_)
    ) {
        return Err(Error::Csv(format!(
            "response column {response} is not numeric"
        )));
    }
    let factor_columns = factors
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    for (name, column) in factors.iter().zip(&factor_columns) {
        if matches!(
            column.data(),
            ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
        ) {
            return Err(Error::Csv(format!("factor column {name} is not numeric")));
        }
    }

    let mut term_names: Vec<String> = factors.to_vec();
    for i in 0..factors.len() {
        for j in (i + 1)..factors.len() {
            term_names.push(format!("{}*{}", factors[i], factors[j]));
        }
    }
    for factor in factors {
        term_names.push(format!("{factor}^2"));
    }
    let p = term_names.len() + 1;
    let mut response_values = Vec::new();
    let mut factor_values: Vec<Vec<f64>> = vec![Vec::new(); factors.len()];
    for row in 0..df.n_rows() {
        let Some(value) = cell(response_column, row) else {
            continue;
        };
        let mut values = Vec::with_capacity(factors.len());
        let mut complete = true;
        for column in &factor_columns {
            match cell(column, row) {
                Some(value) => values.push(value),
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if complete {
            response_values.push(value);
            for (index, value) in values.into_iter().enumerate() {
                factor_values[index].push(value);
            }
        }
    }
    let n = response_values.len();
    if n <= p {
        return Err(Error::SampleTooSmall { n, min: p + 1 });
    }

    let mut columns = Vec::with_capacity(term_names.len() + 1);
    columns.push(Column::new_f64(response, response_values.clone()));
    for term in &term_names {
        let values: Vec<f64> = (0..n)
            .map(|row| {
                if let Some(base) = term.strip_suffix("^2") {
                    let index = factors
                        .iter()
                        .position(|factor| factor == base)
                        .expect("square");
                    factor_values[index][row].powi(2)
                } else if let Some((left, right)) = term.split_once('*') {
                    let i = factors
                        .iter()
                        .position(|factor| factor == left)
                        .expect("left");
                    let j = factors
                        .iter()
                        .position(|factor| factor == right)
                        .expect("right");
                    factor_values[i][row] * factor_values[j][row]
                } else {
                    let index = factors
                        .iter()
                        .position(|factor| factor == term)
                        .expect("main");
                    factor_values[index][row]
                }
            })
            .collect();
        columns.push(Column::new_f64(term.clone(), values));
    }
    let temp = DataFrame::new(columns)?;
    let regression = regression::fit(&temp, response, &term_names, 0.95)?;

    let mut cells: BTreeMap<Vec<u64>, Vec<f64>> = BTreeMap::new();
    for row in 0..n {
        let key: Vec<u64> = factor_values
            .iter()
            .map(|values| values[row].to_bits())
            .collect();
        cells.entry(key).or_default().push(response_values[row]);
    }
    let lack_of_fit = if cells.len() < n {
        let mut pure_ss = 0.0;
        for values in cells.values() {
            let cell_mean = values.iter().sum::<f64>() / values.len() as f64;
            pure_ss += values
                .iter()
                .map(|value| (value - cell_mean).powi(2))
                .sum::<f64>();
        }
        let pure_df = (n - cells.len()) as f64;
        let df = regression.df_residual - pure_df;
        if df > 0.0 {
            let ss = (regression.residual_ss - pure_ss).max(0.0);
            let ms = ss / df;
            let pure_ms = pure_ss / pure_df;
            let f = if pure_ms > 0.0 {
                ms / pure_ms
            } else {
                f64::INFINITY
            };
            let p_value = if f.is_finite() {
                (1.0 - f_cdf(f, df, pure_df)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            Some(LackOfFit {
                ss,
                df,
                ms,
                pure_ss,
                pure_df,
                f,
                p_value,
            })
        } else {
            None
        }
    } else {
        None
    };

    let ranges: Vec<(f64, f64)> = factor_values
        .iter()
        .map(|values| {
            (
                values.iter().copied().fold(f64::INFINITY, f64::min),
                values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            )
        })
        .collect();
    let design_points: Vec<Vec<f64>> = (0..n)
        .map(|row| factor_values.iter().map(|values| values[row]).collect())
        .collect();
    Ok(RsmFit {
        regression,
        excluded: df.n_rows() - n,
        factors: factors.to_vec(),
        lack_of_fit,
        ranges,
        design_points,
    })
}

#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
fn contour_plot(response: &str, model: &RsmFit) -> Result<PlotSpec> {
    if model.factors.len() != 2 {
        return Err(Error::Unsupported(
            "contour plots require exactly two factors".to_owned(),
        ));
    }
    let b0 = model.regression.coefficients[0];
    let b1 = coefficient(model, &model.factors[0])?;
    let b2 = coefficient(model, &model.factors[1])?;
    let b12 = coefficient(model, &format!("{}*{}", model.factors[0], model.factors[1]))?;
    let b11 = coefficient(model, &format!("{}^2", model.factors[0]))?;
    let b22 = coefficient(model, &format!("{}^2", model.factors[1]))?;
    let (x0, x1) = model.ranges[0];
    let (y0, y1) = model.ranges[1];
    let x_pad = ((x1 - x0).abs() * 0.05).max(1e-9);
    let y_pad = ((y1 - y0).abs() * 0.05).max(1e-9);
    let (x_min, x_max) = (x0 - x_pad, x1 + x_pad);
    let (y_min, y_max) = (y0 - y_pad, y1 + y_pad);
    let steps = 60usize;
    let dx = (x_max - x_min) / steps as f64;
    let dy = (y_max - y_min) / steps as f64;
    let predict = |x: f64, y: f64| b0 + b1 * x + b2 * y + b12 * x * y + b11 * x * x + b22 * y * y;
    let mut values = vec![vec![0.0; steps + 1]; steps + 1];
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for j in 0..=steps {
        let y = y_min + dy * j as f64;
        for i in 0..=steps {
            let x = x_min + dx * i as f64;
            let value = predict(x, y);
            values[j][i] = value;
            low = low.min(value);
            high = high.max(value);
        }
    }
    if !low.is_finite() || !high.is_finite() || high <= low {
        return Err(Error::ConstantSample);
    }
    let mut levels: Vec<(f64, Vec<Segment>)> = Vec::new();
    for level_index in 1..=6 {
        let level = low + (high - low) * level_index as f64 / 7.0;
        let mut lines = Vec::new();
        for j in 0..steps {
            let y_a = y_min + dy * j as f64;
            let y_b = y_a + dy;
            for i in 0..steps {
                let x_a = x_min + dx * i as f64;
                let x_b = x_a + dx;
                let v00 = values[j][i];
                let v10 = values[j][i + 1];
                let v11 = values[j + 1][i + 1];
                let v01 = values[j + 1][i];
                let mut crossings: Vec<Point> = Vec::with_capacity(4);
                if (v00 - level) * (v10 - level) < 0.0 {
                    let t = (level - v00) / (v10 - v00);
                    crossings.push(Point {
                        x: x_a + t * dx,
                        y: y_a,
                    });
                }
                if (v10 - level) * (v11 - level) < 0.0 {
                    let t = (level - v10) / (v11 - v10);
                    crossings.push(Point {
                        x: x_b,
                        y: y_a + t * dy,
                    });
                }
                if (v01 - level) * (v11 - level) < 0.0 {
                    let t = (level - v01) / (v11 - v01);
                    crossings.push(Point {
                        x: x_a + t * dx,
                        y: y_b,
                    });
                }
                if (v00 - level) * (v01 - level) < 0.0 {
                    let t = (level - v00) / (v01 - v00);
                    crossings.push(Point {
                        x: x_a,
                        y: y_a + t * dy,
                    });
                }
                if crossings.len() == 2 {
                    lines.push(Segment {
                        from: crossings[0],
                        to: crossings[1],
                        width: 1.2,
                    });
                } else if crossings.len() == 4 {
                    lines.push(Segment {
                        from: crossings[0],
                        to: crossings[1],
                        width: 1.2,
                    });
                    lines.push(Segment {
                        from: crossings[2],
                        to: crossings[3],
                        width: 1.2,
                    });
                }
            }
        }
        levels.push((level, lines));
    }
    // Every contour level becomes a labelled series, so the legend shows the
    // response value of each line.
    let series: Vec<Series> = levels
        .iter()
        .filter(|(_, lines)| !lines.is_empty())
        .map(|(level, lines)| Series {
            label: format!("Response = {level:.3}"),
            points: Vec::new(),
            lines: lines.clone(),
            bands: Vec::new(),
            secondary_axis: false,
        })
        .collect();
    let points = model
        .design_points
        .iter()
        .filter(|point| point.len() == 2)
        .map(|point| Point {
            x: point[0],
            y: point[1],
        })
        .collect();
    Ok(PlotSpec {
        title: format!("Contour Plot — {response}"),
        x_axis: linear_axis(&model.factors[0], x_min, x_max),
        y_axis: linear_axis(&model.factors[1], y_min, y_max),
        size: (900, 650),
        points,
        lines: Vec::new(),
        bands: Vec::new(),
        series,
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: Vec::new(),
    })
}

/// Draws the fitted surface as an isometric wireframe.
///
/// The projection is computed by the engine (the first factor goes down to
/// the right, the second down to the left and the response goes up), so every
/// backend only has to draw lines. The surface needs exactly two factors.
fn surface_plot(response: &str, model: &RsmFit) -> Result<PlotSpec> {
    if model.factors.len() != 2 {
        return Err(Error::Unsupported(
            "the surface plot requires exactly two factors".to_owned(),
        ));
    }
    let b0 = model.regression.coefficients[0];
    let b1 = coefficient(model, &model.factors[0])?;
    let b2 = coefficient(model, &model.factors[1])?;
    let b12 = coefficient(model, &format!("{}*{}", model.factors[0], model.factors[1]))?;
    let b11 = coefficient(model, &format!("{}^2", model.factors[0]))?;
    let b22 = coefficient(model, &format!("{}^2", model.factors[1]))?;
    let (x0, x1) = model.ranges[0];
    let (y0, y1) = model.ranges[1];
    let x_pad = ((x1 - x0).abs() * 0.05).max(1e-9);
    let y_pad = ((y1 - y0).abs() * 0.05).max(1e-9);
    let (x_min, x_max) = (x0 - x_pad, x1 + x_pad);
    let (y_min, y_max) = (y0 - y_pad, y1 + y_pad);
    let predict = |x: f64, y: f64| b0 + b1 * x + b2 * y + b12 * x * y + b11 * x * x + b22 * y * y;

    // Grid of the surface and the response range over it.
    let steps = 16usize;
    let dx = (x_max - x_min) / steps as f64;
    let dy = (y_max - y_min) / steps as f64;
    let mut grid = vec![vec![0.0f64; steps + 1]; steps + 1];
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for (j, row) in grid.iter_mut().enumerate() {
        let y = y_min + dy * j as f64;
        for (i, cell) in row.iter_mut().enumerate() {
            let x = x_min + dx * i as f64;
            let value = predict(x, y);
            *cell = value;
            low = low.min(value);
            high = high.max(value);
        }
    }
    if !low.is_finite() || !high.is_finite() || high <= low {
        return Err(Error::ConstantSample);
    }
    let z_span = high - low;

    // Isometric projection of a point of the surface.
    let project = |x: f64, y: f64, z: f64| -> (f64, f64) {
        let u = if (x_max - x_min).abs() < 1e-12 {
            0.0
        } else {
            (x - x_min) / (x_max - x_min) * 2.0 - 1.0
        };
        let v = if (y_max - y_min).abs() < 1e-12 {
            0.0
        } else {
            (y - y_min) / (y_max - y_min) * 2.0 - 1.0
        };
        let w = (z - low) / z_span;
        ((u - v) * 0.866_025_4, (u + v) * 0.5 - w)
    };

    let mut lines = Vec::new();
    for (j, row) in grid.iter().enumerate() {
        for i in 0..steps {
            let a = project(x_min + dx * i as f64, y_min + dy * j as f64, row[i]);
            let b = project(
                x_min + dx * (i + 1) as f64,
                y_min + dy * j as f64,
                row[i + 1],
            );
            lines.push(Segment {
                from: Point { x: a.0, y: a.1 },
                to: Point { x: b.0, y: b.1 },
                width: 1.0,
            });
        }
    }
    for (i, column) in grid.iter().enumerate() {
        for (j, pair) in column.windows(2).enumerate() {
            let a = project(x_min + dx * i as f64, y_min + dy * j as f64, pair[0]);
            let b = project(x_min + dx * i as f64, y_min + dy * (j + 1) as f64, pair[1]);
            lines.push(Segment {
                from: Point { x: a.0, y: a.1 },
                to: Point { x: b.0, y: b.1 },
                width: 1.0,
            });
        }
    }

    // The observed design points, projected on the surface.
    let observed: Vec<Point> = model
        .design_points
        .iter()
        .filter(|point| point.len() == 2)
        .map(|point| {
            let (x, y) = project(point[0], point[1], predict(point[0], point[1]));
            Point { x, y }
        })
        .collect();

    // The stationary point of the model, when it exists.
    let stationary = optimize(model).ok().and_then(|result| {
        result.stationary.map(|point| {
            let (x, y) = project(point[0], point[1], predict(point[0], point[1]));
            Point { x, y }
        })
    });

    let mut series = vec![Series {
        label: "Observed points".to_owned(),
        points: observed,
        lines: Vec::new(),
        bands: Vec::new(),
        secondary_axis: false,
    }];
    if let Some(point) = stationary {
        series.push(Series {
            label: "Stationary point".to_owned(),
            points: vec![point],
            lines: Vec::new(),
            bands: Vec::new(),
            secondary_axis: false,
        });
    }

    let mut screen_low = f64::INFINITY;
    let mut screen_high = f64::NEG_INFINITY;
    for segment in &lines {
        screen_low = screen_low.min(segment.from.y).min(segment.to.y);
        screen_high = screen_high.max(segment.from.y).max(segment.to.y);
    }
    for item in &series {
        for point in &item.points {
            screen_low = screen_low.min(point.y);
            screen_high = screen_high.max(point.y);
        }
    }
    let mut screen_left = f64::INFINITY;
    let mut screen_right = f64::NEG_INFINITY;
    for segment in &lines {
        screen_left = screen_left.min(segment.from.x).min(segment.to.x);
        screen_right = screen_right.max(segment.from.x).max(segment.to.x);
    }
    let pad = ((screen_high - screen_low).abs() * 0.08).max(1e-9);

    Ok(PlotSpec {
        title: format!("Response Surface — {response}"),
        x_axis: linear_axis("Isometric view", screen_left, screen_right),
        y_axis: linear_axis(
            &format!("{} / {} / response", model.factors[0], model.factors[1]),
            screen_low - pad,
            screen_high + pad,
        ),
        size: (900, 650),
        points: Vec::new(),
        lines,
        bands: Vec::new(),
        series,
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: Vec::new(),
    })
}

/// An extremum of the fitted surface inside the coded design region.
#[derive(Debug, Clone, PartialEq)]
pub struct RsmExtremum {
    /// Coded factor values.
    pub point: Vec<f64>,
    /// Predicted response at that point.
    pub predicted: f64,
    /// Whether at least one factor sits on the ±1 boundary.
    pub on_boundary: bool,
}

/// One step of the steepest-ascent path.
#[derive(Debug, Clone, PartialEq)]
pub struct SteepestPoint {
    /// Step number, starting at 1.
    pub step: usize,
    /// Coded factor values at the step.
    pub coded: Vec<f64>,
    /// Predicted response at the step.
    pub predicted: f64,
}

/// Steepest-ascent path of a fitted response surface.
#[derive(Debug, Clone, PartialEq)]
pub struct SteepestAscent {
    /// Coded increment of the path per step (largest component = `step`).
    pub direction: Vec<f64>,
    /// Step size in coded units.
    pub step: f64,
    /// Points along the path.
    pub points: Vec<SteepestPoint>,
}

/// Default step of the steepest-ascent path, in coded units.
pub const STEEPEST_ASCENT_STEP: f64 = 0.5;
/// Default number of steepest-ascent steps.
pub const STEEPEST_ASCENT_STEPS: usize = 5;

/// Evaluates the fitted quadratic at a coded point.
///
/// # Errors
///
/// Returns [`Error::NotConverged`] when a term cannot be matched to a factor.
pub fn predict(model: &RsmFit, coded: &[f64]) -> Result<f64> {
    let mut value = coefficient(model, "Constant")?;
    for (index, term) in model.regression.names.iter().enumerate().skip(1) {
        let contribution = if let Some(base) = term.strip_suffix("^2") {
            let position = model
                .factors
                .iter()
                .position(|factor| factor == base)
                .ok_or_else(|| Error::NotConverged {
                    detail: format!("missing factor {base}"),
                })?;
            coded[position].powi(2)
        } else if let Some((left, right)) = term.split_once('*') {
            let i = model
                .factors
                .iter()
                .position(|factor| factor == left)
                .ok_or_else(|| Error::NotConverged {
                    detail: format!("missing factor {left}"),
                })?;
            let j = model
                .factors
                .iter()
                .position(|factor| factor == right)
                .ok_or_else(|| Error::NotConverged {
                    detail: format!("missing factor {right}"),
                })?;
            coded[i] * coded[j]
        } else {
            let position = model
                .factors
                .iter()
                .position(|factor| factor == term)
                .ok_or_else(|| Error::NotConverged {
                    detail: format!("missing factor {term}"),
                })?;
            coded[position]
        };
        value += model.regression.coefficients[index] * contribution;
    }
    Ok(value)
}

/// Computes the steepest-ascent path from the centre of the design.
///
/// The gradient at the centre is the vector of main-effect coefficients. The
/// path is scaled so that its largest coded component advances by `step` per
/// point, which is the usual coded-unit convention.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for a non-positive step or no steps and
/// [`Error::Unsupported`] when the fitted surface is flat.
pub fn steepest_ascent(model: &RsmFit, step: f64, steps: usize) -> Result<SteepestAscent> {
    if !step.is_finite() || step <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "steepest-ascent step",
            value: step,
        });
    }
    if steps == 0 {
        return Err(Error::InvalidParameter {
            name: "steepest-ascent steps",
            value: 0.0,
        });
    }
    let gradient = model
        .factors
        .iter()
        .map(|factor| coefficient(model, factor))
        .collect::<Result<Vec<f64>>>()?;
    let largest = gradient.iter().copied().map(f64::abs).fold(0.0, f64::max);
    if largest <= 1e-12 {
        return Err(Error::Unsupported(
            "the fitted surface has no gradient for steepest ascent".to_owned(),
        ));
    }
    let direction: Vec<f64> = gradient
        .iter()
        .map(|value| value / largest * step)
        .collect();
    let mut points = Vec::with_capacity(steps);
    for number in 1..=steps {
        let coded: Vec<f64> = direction
            .iter()
            .map(|value| value * number as f64)
            .collect();
        points.push(SteepestPoint {
            step: number,
            predicted: predict(model, &coded)?,
            coded,
        });
    }
    Ok(SteepestAscent {
        direction,
        step,
        points,
    })
}

/// Stationary-point analysis of a quadratic response surface.
#[derive(Debug, Clone, PartialEq)]
pub struct RsmOptimization {
    /// Coded stationary point, when the Hessian is non-singular.
    pub stationary: Option<Vec<f64>>,
    /// Predicted response at the stationary point.
    pub predicted: Option<f64>,
    /// Eigenvalues of the quadratic-form matrix, ascending.
    pub eigenvalues: Vec<f64>,
    /// Nature of the stationary point.
    pub nature: String,
    /// Smallest predicted response inside the coded design region.
    pub minimum: Option<RsmExtremum>,
    /// Largest predicted response inside the coded design region.
    pub maximum: Option<RsmExtremum>,
}

fn coefficient(model: &RsmFit, name: &str) -> Result<f64> {
    model
        .regression
        .names
        .iter()
        .position(|term| term == name)
        .map(|index| model.regression.coefficients[index])
        .ok_or_else(|| Error::NotConverged {
            detail: format!("missing response-surface term {name}"),
        })
}

#[allow(clippy::needless_range_loop)]
fn solve_linear(matrix: &[Vec<f64>], rhs: &[f64]) -> Option<Vec<f64>> {
    let n = rhs.len();
    if matrix.len() != n {
        return None;
    }
    let mut a = matrix.to_vec();
    let mut b = rhs.to_vec();
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

#[allow(clippy::needless_range_loop)]
fn eigenvalues_symmetric(matrix: &[Vec<f64>]) -> Vec<f64> {
    let n = matrix.len();
    let mut a = matrix.to_vec();
    for _ in 0..100 {
        let mut p = 0;
        let mut q = 1;
        let mut largest = 0.0;
        for i in 0..n {
            for j in (i + 1)..n {
                if a[i][j].abs() > largest {
                    largest = a[i][j].abs();
                    p = i;
                    q = j;
                }
            }
        }
        if largest < 1e-12 {
            break;
        }
        let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
        let t = if theta >= 0.0 {
            1.0 / (theta + (theta * theta + 1.0).sqrt())
        } else {
            -1.0 / (-theta + (theta * theta + 1.0).sqrt())
        };
        let c = 1.0 / (t * t + 1.0).sqrt();
        let s = t * c;
        for k in 0..n {
            let akp = a[k][p];
            let akq = a[k][q];
            a[k][p] = c * akp - s * akq;
            a[k][q] = s * akp + c * akq;
        }
        for k in 0..n {
            let apk = a[p][k];
            let aqk = a[q][k];
            a[p][k] = c * apk - s * aqk;
            a[q][k] = s * apk + c * aqk;
        }
    }
    let mut values: Vec<f64> = (0..n).map(|i| a[i][i]).collect();
    values.sort_by(f64::total_cmp);
    values
}

/// Constant, linear and quadratic terms of the fitted surface, in coded units.
#[allow(clippy::needless_range_loop)]
fn quadratic_form(model: &RsmFit) -> Result<(f64, Vec<f64>, Vec<Vec<f64>>)> {
    let k = model.factors.len();
    let mut linear = vec![0.0; k];
    let mut quadratic = vec![vec![0.0; k]; k];
    for (i, factor) in model.factors.iter().enumerate() {
        linear[i] = coefficient(model, factor)?;
        quadratic[i][i] = coefficient(model, &format!("{factor}^2"))?;
    }
    for i in 0..k {
        for j in (i + 1)..k {
            let value = coefficient(model, &format!("{}*{}", model.factors[i], model.factors[j]))?;
            quadratic[i][j] = value / 2.0;
            quadratic[j][i] = value / 2.0;
        }
    }
    Ok((model.regression.coefficients[0], linear, quadratic))
}

/// Value of `b·x + xᵀQx` (the constant is added by the caller).
#[allow(clippy::needless_range_loop)]
fn quadratic_value(linear: &[f64], quadratic: &[Vec<f64>], point: &[f64]) -> f64 {
    let mut value = 0.0;
    for (i, x) in point.iter().enumerate() {
        value += linear[i] * x;
        for (j, y) in point.iter().enumerate() {
            value += quadratic[i][j] * x * y;
        }
    }
    value
}

/// Minimum of a quadratic over a box.
///
/// The minimum of a quadratic over a box is either the stationary point of the
/// whole box or the minimum of one of its faces, so the search solves the
/// stationary point of every face and recurses on the two values of each free
/// factor. Exact, and small: `2^k` leaves for `k` factors.
#[allow(clippy::needless_range_loop)]
fn minimize_over_box(
    linear: &[f64],
    quadratic: &[Vec<f64>],
    lower: &[f64],
    upper: &[f64],
    free: &[usize],
    fixed: &[(usize, f64)],
    best: &mut Option<(Vec<f64>, f64)>,
) {
    if free.is_empty() {
        let mut point = vec![0.0; linear.len()];
        for &(index, value) in fixed {
            point[index] = value;
        }
        let value = quadratic_value(linear, quadratic, &point);
        if best.as_ref().is_none_or(|(_, current)| value < *current) {
            *best = Some((point, value));
        }
        return;
    }
    let n = free.len();
    let mut matrix = vec![vec![0.0; n]; n];
    let mut rhs = vec![0.0; n];
    for (row, &i) in free.iter().enumerate() {
        // Gradient of b·x + xᵀQx is b + 2Qx; on a face the fixed
        // coordinates contribute 2Q_fc·x_c.
        rhs[row] = -0.5 * linear[i];
        for &(j, value) in fixed {
            rhs[row] -= quadratic[i][j] * value;
        }
        for (column, &j) in free.iter().enumerate() {
            matrix[row][column] = quadratic[i][j];
        }
    }
    if let Some(solution) = solve_linear(&matrix, &rhs) {
        let inside = solution.iter().enumerate().all(|(index, value)| {
            let i = free[index];
            *value >= lower[i] - 1e-12 && *value <= upper[i] + 1e-12
        });
        if inside {
            let mut point = vec![0.0; linear.len()];
            for &(index, value) in fixed {
                point[index] = value;
            }
            for (index, value) in solution.iter().enumerate() {
                point[free[index]] = *value;
            }
            let value = quadratic_value(linear, quadratic, &point);
            if best.as_ref().is_none_or(|(_, current)| value < *current) {
                *best = Some((point, value));
            }
        }
    }
    for (position, &index) in free.iter().enumerate() {
        let mut rest = free.to_vec();
        rest.remove(position);
        for bound in [lower[index], upper[index]] {
            let mut next = fixed.to_vec();
            next.push((index, bound));
            minimize_over_box(linear, quadratic, lower, upper, &rest, &next, best);
        }
    }
}

/// Smallest or largest predicted response inside the coded design region
/// `[−1, 1]^k`.
fn extremum_over_region(model: &RsmFit, maximise: bool) -> Result<Option<RsmExtremum>> {
    let k = model.factors.len();
    if k < 2 {
        return Err(Error::Unsupported(
            "a response surface needs at least two factors".to_owned(),
        ));
    }
    let (constant, linear, quadratic) = quadratic_form(model)?;
    let sign = if maximise { -1.0 } else { 1.0 };
    let signed_linear: Vec<f64> = linear.iter().map(|value| sign * value).collect();
    let signed_quadratic: Vec<Vec<f64>> = quadratic
        .iter()
        .map(|row| row.iter().map(|value| sign * value).collect())
        .collect();
    let lower = vec![-1.0; k];
    let upper = vec![1.0; k];
    let free: Vec<usize> = (0..k).collect();
    let mut best: Option<(Vec<f64>, f64)> = None;
    minimize_over_box(
        &signed_linear,
        &signed_quadratic,
        &lower,
        &upper,
        &free,
        &[],
        &mut best,
    );
    Ok(best.map(|(point, _)| RsmExtremum {
        predicted: constant + quadratic_value(&linear, &quadratic, &point),
        on_boundary: point.iter().any(|value| value.abs() >= 1.0 - 1e-9),
        point,
    }))
}

/// Computes the stationary point, its nature and the extrema inside the design
/// region.
#[allow(clippy::needless_range_loop)]
pub fn optimize(model: &RsmFit) -> Result<RsmOptimization> {
    let k = model.factors.len();
    if k < 2 {
        return Err(Error::Unsupported(
            "a response surface needs at least two factors".to_owned(),
        ));
    }
    let (_, linear, quadratic) = quadratic_form(model)?;
    let rhs: Vec<f64> = linear.iter().map(|value| -value / 2.0).collect();
    let stationary = solve_linear(&quadratic, &rhs);
    let predicted = stationary.as_ref().map(|point| {
        let mut value = model.regression.coefficients[0];
        for (i, x) in point.iter().enumerate() {
            value += linear[i] * x;
            for (j, y) in point.iter().enumerate() {
                value += quadratic[i][j] * x * y;
            }
        }
        value
    });
    let eigenvalues = eigenvalues_symmetric(&quadratic);
    let scale = eigenvalues
        .iter()
        .map(|value| value.abs())
        .fold(1.0, f64::max);
    let tolerance = 1e-9 * scale;
    let positive = eigenvalues.iter().any(|value| *value > tolerance);
    let negative = eigenvalues.iter().any(|value| *value < -tolerance);
    let nature = match (positive, negative) {
        (true, false) => "minimum",
        (false, true) => "maximum",
        (true, true) => "saddle point",
        (false, false) => "flat or indeterminate",
    };
    Ok(RsmOptimization {
        stationary,
        predicted,
        eigenvalues,
        nature: nature.to_owned(),
        minimum: extremum_over_region(model, false)?,
        maximum: extremum_over_region(model, true)?,
    })
}

/// Fits the model, builds the report and the residual plot.
pub fn analyze(
    df: &DataFrame,
    response: &str,
    factors: &[String],
    with_optimization: bool,
    with_contours: bool,
    with_steepest_ascent: bool,
) -> Result<AnalysisOutput> {
    let model = fit(df, response, factors)?;
    let residual = regression::residual_plot(response, &model.regression);
    let plot = if with_contours {
        let contour = contour_plot(response, &model)?;
        let mut panels = vec![residual, contour];
        if model.factors.len() == 2 {
            panels.push(surface_plot(response, &model)?);
        }
        panel_plot(panels, 2, format!("Response Surface — {response}"))
            .ok_or_else(|| Error::Unsupported("invalid response-surface panel".to_owned()))?
    } else {
        residual
    };
    let optimization = if with_optimization {
        Some(optimize(&model)?)
    } else {
        None
    };
    let ascent = if with_steepest_ascent {
        Some(steepest_ascent(
            &model,
            STEEPEST_ASCENT_STEP,
            STEEPEST_ASCENT_STEPS,
        )?)
    } else {
        None
    };
    Ok(AnalysisOutput {
        results: vec![report(
            response,
            factors,
            &model,
            optimization.as_ref(),
            ascent.as_ref(),
        )],
        plot: Some(plot),
    })
}

fn report(
    response: &str,
    factors: &[String],
    model: &RsmFit,
    optimization: Option<&RsmOptimization>,
    ascent: Option<&SteepestAscent>,
) -> AnalysisResult {
    let regression = &model.regression;
    let mut result = AnalysisResult::new(
        format!("Response Surface — response {response}"),
        "Full quadratic model: main effects, two-way interactions and squares",
    );
    result.n = regression.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("R²", regression.r_squared),
        ("Adjusted R²", regression.adjusted_r_squared),
        ("Residual standard error", regression.s),
        ("F", regression.f),
        ("F p-value", regression.f_p_value),
        ("Terms", (regression.names.len() - 1) as f64),
    ];
    result
        .extra
        .push(Item::Message(format!("Factors: {}", factors.join(", "))));
    result.extra.push(Item::Table {
        headers: vec![
            "Term".to_owned(),
            "Coef".to_owned(),
            "SE Coef".to_owned(),
            "T".to_owned(),
            "P".to_owned(),
        ],
        rows: regression
            .names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                vec![
                    name.clone(),
                    format!("{:.6}", regression.coefficients[index]),
                    format!("{:.6}", regression.standard_errors[index]),
                    format!("{:.6}", regression.t_values[index]),
                    format!("{:.6}", regression.p_values[index]),
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
                "Model".to_owned(),
                format!("{:.0}", regression.df_regression),
                format!("{:.6}", regression.regression_ss),
                format!("{:.6}", regression.regression_ss / regression.df_regression),
                format!("{:.6}", regression.f),
                format!("{:.6}", regression.f_p_value),
            ],
            vec![
                "Error".to_owned(),
                format!("{:.0}", regression.df_residual),
                format!("{:.6}", regression.residual_ss),
                format!("{:.6}", regression.residual_ss / regression.df_residual),
                String::new(),
                String::new(),
            ],
            vec![
                "Total".to_owned(),
                format!("{:.0}", regression.df_regression + regression.df_residual),
                format!("{:.6}", regression.total_ss),
                String::new(),
                String::new(),
                String::new(),
            ],
        ],
    });
    if let Some(optimization) = optimization {
        result.extra.push(Item::Message(format!(
            "Stationary point: {}",
            optimization.nature
        )));
        if let Some(stationary) = &optimization.stationary {
            result.extra.push(Item::Table {
                headers: vec!["Factor".to_owned(), "Coded value".to_owned()],
                rows: factors
                    .iter()
                    .zip(stationary)
                    .map(|(factor, value)| vec![factor.clone(), format!("{value:.6}")])
                    .collect(),
            });
            if let Some(predicted) = optimization.predicted {
                result.extra.push(Item::Message(format!(
                    "Predicted response at the stationary point: {predicted:.6}"
                )));
            }
        } else {
            result.extra.push(Item::Message(
                "The quadratic matrix is singular: no unique stationary point.".to_owned(),
            ));
            result.warnings.push(
                "the stationary point could not be solved because the quadratic matrix is singular"
                    .to_owned(),
            );
        }
        result.extra.push(Item::Table {
            headers: vec!["Eigenvalue".to_owned(), "Value".to_owned()],
            rows: optimization
                .eigenvalues
                .iter()
                .enumerate()
                .map(|(index, value)| vec![format!("λ{}", index + 1), format!("{value:.6}")])
                .collect(),
        });
        // The extrema inside the coded design region, which is where the
        // fitted model is supported by data.
        if optimization.minimum.is_some() || optimization.maximum.is_some() {
            result.extra.push(Item::Table {
                headers: vec![
                    "Factor".to_owned(),
                    "Minimum (coded)".to_owned(),
                    "Maximum (coded)".to_owned(),
                ],
                rows: factors
                    .iter()
                    .enumerate()
                    .map(|(index, factor)| {
                        vec![
                            factor.clone(),
                            optimization
                                .minimum
                                .as_ref()
                                .map(|minimum| format!("{:.6}", minimum.point[index]))
                                .unwrap_or_default(),
                            optimization
                                .maximum
                                .as_ref()
                                .map(|maximum| format!("{:.6}", maximum.point[index]))
                                .unwrap_or_default(),
                        ]
                    })
                    .collect(),
            });
            let mut message =
                String::from("Predicted response inside the design region (−1…+1 coded):");
            if let Some(minimum) = &optimization.minimum {
                message.push_str(&format!(" minimum {:.6}", minimum.predicted));
            }
            if let Some(maximum) = &optimization.maximum {
                message.push_str(&format!(", maximum {:.6}", maximum.predicted));
            }
            result.extra.push(Item::Message(message));
        }
    }
    if let Some(ascent) = ascent {
        result.extra.push(Item::Message(format!(
            "Steepest ascent: step {:.6} in coded units along the gradient, {} step(s)",
            ascent.step,
            ascent.points.len()
        )));
        let mut headers = vec!["Step".to_owned()];
        headers.extend(factors.iter().cloned());
        headers.push("Predicted".to_owned());
        result.extra.push(Item::Table {
            headers,
            rows: ascent
                .points
                .iter()
                .map(|point| {
                    let mut row = vec![point.step.to_string()];
                    row.extend(point.coded.iter().map(|value| format!("{value:.6}")));
                    row.push(format!("{:.6}", point.predicted));
                    row
                })
                .collect(),
        });
    }
    if let Some(lack) = &model.lack_of_fit {
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
                    "Lack of fit".to_owned(),
                    format!("{:.0}", lack.df),
                    format!("{:.6}", lack.ss),
                    format!("{:.6}", lack.ms),
                    format!("{:.6}", lack.f),
                    format!("{:.6}", lack.p_value),
                ],
                vec![
                    "Pure error".to_owned(),
                    format!("{:.0}", lack.pure_df),
                    format!("{:.6}", lack.pure_ss),
                    format!("{:.6}", lack.pure_ss / lack.pure_df),
                    String::new(),
                    String::new(),
                ],
            ],
        });
        if lack.p_value < 0.05 {
            result
                .warnings
                .push("the lack-of-fit test rejects the quadratic model at 5%".to_owned());
        }
    }
    result.notes = vec![
        (
            "Model".to_owned(),
            "y = β₀ + Σβᵢxᵢ + Σβᵢⱼxᵢxⱼ + Σβᵢᵢxᵢ² + ε; factors are coded".to_owned(),
        ),
        (
            "Scope".to_owned(),
            "quadratic model with lack-of-fit, the stationary point, its canonical eigenvalues \
             and the extrema inside the coded design region; steepest ascent and multi-response \
             desirability are not implemented"
                .to_owned(),
        ),
        (
            "Design".to_owned(),
            "use the central composite design generator; the model needs residual degrees of \
             freedom"
                .to_owned(),
        ),
    ];
    if let Some(optimization) = optimization {
        let mut note = String::from(
            "the extrema inside the design region are exact for a quadratic over a box \
             (the stationary point of every face is solved); the region is the coded \
             −1…+1 box, where the design has data",
        );
        let outside = optimization
            .stationary
            .as_ref()
            .is_some_and(|point| point.iter().any(|value| value.abs() > 1.0));
        if outside {
            note.push_str(
                "; the stationary point lies outside that box, so the reported \
                           optimum is on the boundary",
            );
        }
        result.notes.push(("Region extrema".to_owned(), note));
    }
    if model.lack_of_fit.is_some() {
        result.notes.push((
            "Lack of fit".to_owned(),
            "replicated design points supply pure error; lack-of-fit SS = residual SS − pure-error SS"
                .to_owned(),
        ));
    }
    result.warnings = standard_warnings(regression.n, model.excluded, 0);
    result.warnings.push(
        "response-surface conventions and diagnostics have not been verified against captured \
         captured output"
            .to_owned(),
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        let mut rows = Vec::new();
        for a in [-1.0, 0.0, 1.0] {
            for b in [-1.0, 0.0, 1.0] {
                rows.push((
                    a,
                    b,
                    10.0 + 2.0 * a + 3.0 * b + a * b + 0.5 * a * a + 0.25 * b * b,
                ));
            }
        }
        DataFrame::new(vec![
            Column::new_f64("A", rows.iter().map(|row| row.0).collect()),
            Column::new_f64("B", rows.iter().map(|row| row.1).collect()),
            Column::new_f64("y", rows.iter().map(|row| row.2).collect()),
        ])
        .unwrap()
    }

    #[test]
    fn quadratic_model_recovers_exact_coefficients() {
        let model = fit(&frame(), "y", &["A".to_owned(), "B".to_owned()]).expect("fit");
        let index = |name: &str| {
            model
                .regression
                .names
                .iter()
                .position(|term| term == name)
                .expect(name)
        };
        assert!((model.regression.coefficients[index("A")] - 2.0).abs() < 1e-10);
        assert!((model.regression.coefficients[index("A^2")] - 0.5).abs() < 1e-10);
        assert!((model.regression.coefficients[index("B^2")] - 0.25).abs() < 1e-10);
        assert!((model.regression.coefficients[index("A*B")] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn optimization_finds_the_stationary_point_and_nature() {
        let model = fit(&frame(), "y", &["A".to_owned(), "B".to_owned()]).expect("fit");
        let opt = optimize(&model).expect("optimize");
        let point = opt.stationary.expect("stationary");
        assert!((point[0] + 4.0).abs() < 1e-10);
        assert!((point[1] - 2.0).abs() < 1e-10);
        assert!((opt.predicted.expect("predicted") - 9.0).abs() < 1e-10);
        assert_eq!(opt.nature, "saddle point");
    }

    fn surface(values: impl Fn(f64, f64) -> f64) -> DataFrame {
        let mut rows = Vec::new();
        for a in [-1.0, 0.0, 1.0] {
            for b in [-1.0, 0.0, 1.0] {
                rows.push((a, b, values(a, b)));
            }
        }
        DataFrame::new(vec![
            Column::new_f64("A", rows.iter().map(|row| row.0).collect()),
            Column::new_f64("B", rows.iter().map(|row| row.1).collect()),
            Column::new_f64("y", rows.iter().map(|row| row.2).collect()),
        ])
        .expect("frame")
    }

    #[test]
    fn region_extrema_are_exact_over_the_coded_box() {
        // A saddle whose stationary point lies outside the box: both extrema
        // sit on the corners.
        let model = fit(&frame(), "y", &["A".to_owned(), "B".to_owned()]).expect("fit");
        let opt = optimize(&model).expect("optimize");
        let minimum = opt.minimum.expect("minimum");
        assert!((minimum.point[0] + 1.0).abs() < 1e-10, "{minimum:?}");
        assert!((minimum.point[1] + 1.0).abs() < 1e-10, "{minimum:?}");
        assert!((minimum.predicted - 6.75).abs() < 1e-10, "{minimum:?}");
        assert!(minimum.on_boundary);
        let maximum = opt.maximum.expect("maximum");
        assert!((maximum.point[0] - 1.0).abs() < 1e-10, "{maximum:?}");
        assert!((maximum.point[1] - 1.0).abs() < 1e-10, "{maximum:?}");
        assert!((maximum.predicted - 16.75).abs() < 1e-10, "{maximum:?}");
    }

    #[test]
    fn an_interior_stationary_point_is_the_region_minimum() {
        // Convex surface with the stationary point at the center of the box.
        let df = surface(|a, b| 10.0 + 0.5 * a * a + 0.25 * b * b);
        let model = fit(&df, "y", &["A".to_owned(), "B".to_owned()]).expect("fit");
        let opt = optimize(&model).expect("optimize");
        assert_eq!(opt.nature, "minimum");
        let minimum = opt.minimum.expect("minimum");
        assert!(!minimum.on_boundary, "{minimum:?}");
        assert!(minimum.point[0].abs() < 1e-10, "{minimum:?}");
        assert!((minimum.predicted - 10.0).abs() < 1e-10, "{minimum:?}");
        // The maximum is still on the boundary.
        let maximum = opt.maximum.expect("maximum");
        assert!(maximum.on_boundary);
        assert!((maximum.predicted - 10.75).abs() < 1e-10, "{maximum:?}");
    }

    #[test]
    fn a_linear_surface_puts_the_extrema_on_the_vertices() {
        let df = surface(|a, b| 10.0 + 2.0 * a + 3.0 * b);
        let model = fit(&df, "y", &["A".to_owned(), "B".to_owned()]).expect("fit");
        let opt = optimize(&model).expect("optimize");
        // A singular quadratic matrix has no unique stationary point...
        assert!(opt.stationary.is_none());
        // ...but the region extrema are still well defined.
        let minimum = opt.minimum.expect("minimum");
        assert!((minimum.predicted - 5.0).abs() < 1e-9, "{minimum:?}");
        assert!(minimum.point[0] < -0.99 && minimum.point[1] < -0.99);
        let maximum = opt.maximum.expect("maximum");
        assert!((maximum.predicted - 15.0).abs() < 1e-9, "{maximum:?}");
        assert!(maximum.point[0] > 0.99 && maximum.point[1] > 0.99);
    }

    #[test]
    fn a_single_factor_never_reaches_the_region_search() {
        let df = DataFrame::new(vec![
            Column::new_f64("A", vec![-1.0, 0.0, 1.0]),
            Column::new_f64("y", vec![9.0, 10.0, 11.0]),
        ])
        .expect("frame");
        assert!(matches!(
            fit(&df, "y", &["A".to_owned()]),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn the_report_lists_the_region_extrema() {
        let out = analyze(
            &frame(),
            "y",
            &["A".to_owned(), "B".to_owned()],
            true,
            false,
            false,
        )
        .expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Minimum (coded)"), "{text}");
        assert!(text.contains("maximum 16.75"), "{text}");
        assert!(text.contains("outside that box"), "{text}");
    }

    #[test]
    fn lack_of_fit_uses_replicated_center_points() {
        let mut rows = Vec::new();
        for a in [-1.0, 0.0, 1.0] {
            for b in [-1.0, 0.0, 1.0] {
                let y = 10.0 + 2.0 * a + 3.0 * b + a * b + 0.5 * a * a + 0.25 * b * b;
                rows.push((a, b, y));
            }
        }
        rows.push((0.0, 0.0, 10.2));
        let df = DataFrame::new(vec![
            Column::new_f64("A", rows.iter().map(|row| row.0).collect()),
            Column::new_f64("B", rows.iter().map(|row| row.1).collect()),
            Column::new_f64("y", rows.iter().map(|row| row.2).collect()),
        ])
        .unwrap();
        let model = fit(&df, "y", &["A".to_owned(), "B".to_owned()]).expect("fit");
        let lack = model.lack_of_fit.expect("lack of fit");
        assert_eq!(lack.pure_df, 1.0);
        assert!(lack.df > 0.0);
        let out = analyze(
            &df,
            "y",
            &["A".to_owned(), "B".to_owned()],
            false,
            false,
            false,
        )
        .expect("analysis");
        assert!(out.results[0].to_session_text().contains("Lack of fit"));
    }

    #[test]
    fn contour_plot_is_added_for_two_factors() {
        let out = analyze(
            &frame(),
            "y",
            &["A".to_owned(), "B".to_owned()],
            false,
            true,
            false,
        )
        .expect("analysis");
        let plot = out.plot.expect("plot");
        // Residual, contour and isometric surface.
        assert_eq!(plot.panels.len(), 3);
        // The contours are labelled series, one per response level.
        assert!(plot.panels[1].series.len() > 1);
        assert!(
            plot.panels[1]
                .series
                .iter()
                .all(|series| series.label.starts_with("Response = "))
        );
        assert!(
            plot.panels[1]
                .series
                .iter()
                .all(|series| !series.lines.is_empty())
        );
        assert!(!plot.panels[1].points.is_empty());
        // The surface is a wireframe with the observed points.
        assert!(!plot.panels[2].lines.is_empty());
        assert!(!plot.panels[2].series.is_empty());
    }

    #[test]
    fn report_contains_quadratic_terms() {
        let out = analyze(
            &frame(),
            "y",
            &["A".to_owned(), "B".to_owned()],
            false,
            false,
            false,
        )
        .expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Response Surface"), "{text}");
        assert!(text.contains("A^2"), "{text}");
        assert!(out.plot.is_some());
    }
}
