// SPDX-License-Identifier: MIT
//! Mixture experiments: Scheffé canonical polynomials.
//!
//! A mixture has components `x₁ … x_q` with `0 ≤ xᵢ ≤ 1` and `Σxᵢ = 1`, so the
//! usual polynomial is not identifiable and the **canonical Scheffé** forms are
//! used instead, without an intercept:
//!
//! - **linear**: `η = Σ βᵢ·xᵢ`;
//! - **quadratic**: `η = Σ βᵢ·xᵢ + Σ_{i<j} βᵢⱼ·xᵢxⱼ`;
//! - **special cubic**: adds `Σ_{i<j<k} βᵢⱼₖ·xᵢxⱼxₖ`.
//!
//! The coefficients are fitted by ordinary least squares on the raw component
//! columns (the constraint is already carried by the regressors, so no
//! intercept is added), and the usual inference follows from
//! `s² = SSE/(n − p)` and `(XᵀX)⁻¹`. A linear coefficient is the predicted
//! response of the pure component; an interaction coefficient is the excess
//! over the additive blend.
//!
//! The plot draws the fitted iso-response contours over the simplex (ternary
//! plot for three components, the fitted line for two) together with the runs.
//!
//! The documented behaviour fits mixture models; its term and rounding
//! conventions were not documented, so the numbers are not claimed to match the
//! documented behaviour — see the project notes.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, Series, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::student_t_cdf;
use crate::workflow::AnalysisOutput;

/// Scheffé canonical model of a mixture experiment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixtureModel {
    /// `Σ βᵢ·xᵢ`.
    Linear,
    /// Linear plus every binary blend `βᵢⱼ·xᵢxⱼ`.
    Quadratic,
    /// Quadratic plus every ternary blend `βᵢⱼₖ·xᵢxⱼxₖ`.
    SpecialCubic,
}

impl MixtureModel {
    /// Name used by the interface and the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Quadratic => "quadratic",
            Self::SpecialCubic => "special cubic",
        }
    }

    /// Short code used by the command line and the preferences.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Quadratic => "quadratic",
            Self::SpecialCubic => "cubic",
        }
    }

    /// Parses the short codes of [`Self::as_str`].
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "linear" => Some(Self::Linear),
            "quadratic" => Some(Self::Quadratic),
            "cubic" | "special-cubic" | "special_cubic" => Some(Self::SpecialCubic),
            _ => None,
        }
    }

    /// Number of terms of `components` components under this model.
    #[must_use]
    pub fn term_count(self, components: usize) -> usize {
        let pairs = components * components.saturating_sub(1) / 2;
        let triples = if components >= 3 {
            components * (components - 1) * (components - 2) / 6
        } else {
            0
        };
        match self {
            Self::Linear => components,
            Self::Quadratic => components + pairs,
            Self::SpecialCubic => components + pairs + triples,
        }
    }
}

/// One fitted coefficient.
#[derive(Debug, Clone, PartialEq)]
pub struct MixtureCoefficient {
    /// Term, such as `A` or `A*B`.
    pub term: String,
    /// Estimated coefficient.
    pub estimate: f64,
    /// Standard error.
    pub standard_error: f64,
    /// t statistic of the coefficient.
    pub t: f64,
    /// Two-sided p-value of the t statistic.
    pub p_value: f64,
}

/// Fitted Scheffé mixture model.
#[derive(Debug, Clone, PartialEq)]
pub struct MixtureFit {
    /// Response column.
    pub response: String,
    /// Components, in the requested order.
    pub components: Vec<String>,
    /// Model.
    pub model: MixtureModel,
    /// Coefficients in term order.
    pub coefficients: Vec<MixtureCoefficient>,
    /// Runs used.
    pub n: usize,
    /// Rows excluded.
    pub excluded: usize,
    /// Coefficient of determination about the mean response.
    pub r_squared: f64,
    /// Adjusted coefficient of determination.
    pub adjusted_r_squared: f64,
    /// Residual standard deviation.
    pub rmse: f64,
    /// Observed responses.
    pub observed: Vec<f64>,
    /// Fitted responses.
    pub fitted: Vec<f64>,
    /// Residuals.
    pub residuals: Vec<f64>,
    /// Component values of every run.
    pub points: Vec<Vec<f64>>,
}

/// The index sets of the model terms.
fn term_indices(components: usize, model: MixtureModel) -> Vec<Vec<usize>> {
    let mut terms: Vec<Vec<usize>> = (0..components).map(|index| vec![index]).collect();
    if model != MixtureModel::Linear {
        for left in 0..components {
            for right in (left + 1)..components {
                terms.push(vec![left, right]);
            }
        }
    }
    if model == MixtureModel::SpecialCubic {
        for left in 0..components {
            for middle in (left + 1)..components {
                for right in (middle + 1)..components {
                    terms.push(vec![left, middle, right]);
                }
            }
        }
    }
    terms
}

/// Term name, such as `A*B`.
fn term_name(components: &[String], indices: &[usize]) -> String {
    indices
        .iter()
        .map(|index| components[*index].as_str())
        .collect::<Vec<&str>>()
        .join("*")
}

/// Solves `matrix·x = rhs` by Gaussian elimination with partial pivoting.
fn solve(matrix: &[Vec<f64>], rhs: &[f64]) -> Option<Vec<f64>> {
    let size = matrix.len();
    let mut augmented: Vec<Vec<f64>> = matrix
        .iter()
        .zip(rhs)
        .map(|(row, value)| {
            let mut values = row.clone();
            values.push(*value);
            values
        })
        .collect();
    for column in 0..size {
        let mut pivot = column;
        for row in (column + 1)..size {
            if augmented[row][column].abs() > augmented[pivot][column].abs() {
                pivot = row;
            }
        }
        if augmented[pivot][column].abs() < 1e-12 {
            return None;
        }
        augmented.swap(column, pivot);
        let scale = augmented[column][column];
        for value in augmented[column].iter_mut() {
            *value /= scale;
        }
        let pivot_row = augmented[column].clone();
        for (row, values) in augmented.iter_mut().enumerate() {
            if row == column {
                continue;
            }
            let factor = values[column];
            if factor == 0.0 {
                continue;
            }
            for (value, base) in values.iter_mut().zip(&pivot_row) {
                *value -= factor * base;
            }
        }
    }
    Some((0..size).map(|row| augmented[row][size]).collect())
}

/// Inverse of a square matrix, from one solve per unit vector.
#[allow(clippy::needless_range_loop)]
fn invert(matrix: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let size = matrix.len();
    let mut inverse = vec![vec![0.0; size]; size];
    for row in 0..size {
        let mut rhs = vec![0.0; size];
        rhs[row] = 1.0;
        let column = solve(matrix, &rhs)?;
        for index in 0..size {
            inverse[index][row] = column[index];
        }
    }
    Some(inverse)
}

/// Two-sided Student-t tail, `2·(1 − F(|t|; ν))`.
fn t_p_value(t: f64, degrees: f64) -> f64 {
    if !t.is_finite() || degrees <= 0.0 {
        return f64::NAN;
    }
    // F(t) = 1 − ½·I_{ν/(ν+t²)}(ν/2, ½) for t ≥ 0.
    (2.0 * (1.0 - student_t_cdf(t.abs(), degrees))).clamp(0.0, 1.0)
}

/// Fits the Scheffé model of the given components.
///
/// # Errors
///
/// Returns [`Error::EmptySample`] without complete rows, [`Error::Cell`] for a
/// component outside `[0, 1]` or a run whose components do not sum to one,
/// [`Error::Unsupported`] for fewer than two components, columns that are not
/// distinct and a design that does not identify every term, and
/// [`Error::ConstantSample`] for a constant response.
#[allow(clippy::needless_range_loop)]
pub fn fit(
    df: &DataFrame,
    response_column: &str,
    components: &[String],
    model: MixtureModel,
) -> Result<MixtureFit> {
    if components.len() < 2 {
        return Err(Error::Unsupported(
            "a mixture model needs at least two components".to_owned(),
        ));
    }
    if components.len() > 10 {
        return Err(Error::Unsupported(
            "at most 10 components are accepted".to_owned(),
        ));
    }
    let mut seen = components.to_vec();
    seen.sort();
    seen.dedup();
    if seen.len() != components.len() {
        return Err(Error::Unsupported(
            "the components must be distinct columns".to_owned(),
        ));
    }
    if components.iter().any(|name| name == response_column) {
        return Err(Error::Unsupported(
            "the response cannot also be a component".to_owned(),
        ));
    }
    let response = df.column(response_column)?;
    let mut sources = Vec::with_capacity(components.len());
    for name in components {
        sources.push(df.column(name)?.numeric_by_row());
    }
    let mut observed = Vec::new();
    let mut points: Vec<Vec<f64>> = Vec::new();
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        let Some(value) = response.value(row) else {
            excluded += 1;
            continue;
        };
        let mut values = Vec::with_capacity(components.len());
        let mut complete = true;
        for source in &sources {
            if !source[row].is_finite() {
                complete = false;
                break;
            }
            values.push(source[row]);
        }
        if !complete {
            excluded += 1;
            continue;
        }
        for (index, component) in values.iter().enumerate() {
            if !(0.0..=1.0).contains(component) {
                return Err(Error::Cell {
                    column: components[index].clone(),
                    row: row + 1,
                    message: "mixture components must lie in [0, 1]".to_owned(),
                });
            }
        }
        let total: f64 = values.iter().sum();
        // Worksheets store rounded components (0.333333 for a third), so the
        // sum is accepted within a tolerance far below any real deviation.
        if (total - 1.0).abs() > 1e-5 {
            return Err(Error::Cell {
                column: "mixture".to_owned(),
                row: row + 1,
                message: format!("the components sum to {total}, not one"),
            });
        }
        observed.push(value);
        points.push(values);
    }
    let n = points.len();
    if n == 0 {
        return Err(Error::EmptySample);
    }
    let indices = term_indices(components.len(), model);
    let p = indices.len();
    if n <= p {
        return Err(Error::Unsupported(format!(
            "the {} model has {p} terms and needs more than {p} runs, found {n}; add runs",
            model.name()
        )));
    }
    let mut design: Vec<Vec<f64>> = Vec::with_capacity(n);
    for point in &points {
        design.push(
            indices
                .iter()
                .map(|term| term.iter().map(|index| point[*index]).product())
                .collect(),
        );
    }
    let mut normal = vec![vec![0.0; p]; p];
    let mut rhs = vec![0.0; p];
    for (row, values) in design.iter().enumerate() {
        for left in 0..p {
            rhs[left] += values[left] * observed[row];
            for right in 0..p {
                normal[left][right] += values[left] * values[right];
            }
        }
    }
    let estimates = solve(&normal, &rhs).ok_or_else(|| {
        Error::Unsupported(
            "the runs do not identify every term of the model; add runs or lower the model"
                .to_owned(),
        )
    })?;
    let inverse = invert(&normal).ok_or_else(|| {
        Error::Unsupported(
            "the runs do not identify every term of the model; add runs or lower the model"
                .to_owned(),
        )
    })?;
    let fitted: Vec<f64> = design
        .iter()
        .map(|values| values.iter().zip(&estimates).map(|(x, b)| x * b).sum())
        .collect();
    let residuals: Vec<f64> = observed
        .iter()
        .zip(&fitted)
        .map(|(value, fit)| value - fit)
        .collect();
    let sse: f64 = residuals.iter().map(|value| value * value).sum();
    let mean = observed.iter().sum::<f64>() / n as f64;
    let sst: f64 = observed.iter().map(|value| (value - mean).powi(2)).sum();
    if sst <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let degrees = (n - p) as f64;
    let variance = sse / degrees;
    let r_squared = 1.0 - sse / sst;
    let adjusted_r_squared = 1.0 - (1.0 - r_squared) * (n - 1) as f64 / degrees;
    let coefficients = indices
        .iter()
        .enumerate()
        .map(|(index, term)| {
            let standard_error = if variance > 0.0 {
                (variance * inverse[index][index]).sqrt()
            } else {
                0.0
            };
            let t = if standard_error > 0.0 {
                estimates[index] / standard_error
            } else {
                f64::NAN
            };
            MixtureCoefficient {
                term: term_name(components, term),
                estimate: estimates[index],
                standard_error,
                t,
                p_value: t_p_value(t, degrees),
            }
        })
        .collect();
    Ok(MixtureFit {
        response: response_column.to_owned(),
        components: components.to_vec(),
        model,
        coefficients,
        n,
        excluded,
        r_squared,
        adjusted_r_squared,
        rmse: variance.max(0.0).sqrt(),
        observed,
        fitted,
        residuals,
        points,
    })
}

impl MixtureFit {
    /// Predicted response for a mixture.
    #[must_use]
    pub fn predict(&self, point: &[f64]) -> f64 {
        let indices = term_indices(self.components.len(), self.model);
        indices
            .iter()
            .zip(&self.coefficients)
            .map(|(term, coefficient)| {
                term.iter().map(|index| point[*index]).product::<f64>() * coefficient.estimate
            })
            .sum()
    }
}

/// Barycentric coordinates of a three-component mixture.
fn ternary(point: &[f64]) -> Point {
    Point {
        x: point[1] + 0.5 * point[2],
        y: 0.5 * 3.0f64.sqrt() * point[2],
    }
}

/// Iso-response contours over the ternary simplex plus the runs.
#[allow(clippy::needless_range_loop)]
fn ternary_plot(model: &MixtureFit) -> PlotSpec {
    let steps = 40usize;
    // Values on the triangular lattice (i, j), i + j ≤ steps.
    let mut values = vec![vec![f64::NAN; steps + 1]; steps + 1];
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for i in 0..=steps {
        for j in 0..=(steps - i) {
            let point = [
                1.0 - (i + j) as f64 / steps as f64,
                i as f64 / steps as f64,
                j as f64 / steps as f64,
            ];
            let value = model.predict(&point);
            values[i][j] = value;
            low = low.min(value);
            high = high.max(value);
        }
    }
    let mut lines = Vec::new();
    // The triangle itself.
    let corners = [
        ternary(&[1.0, 0.0, 0.0]),
        ternary(&[0.0, 1.0, 0.0]),
        ternary(&[0.0, 0.0, 1.0]),
    ];
    for pair in 0..3 {
        lines.push(Segment {
            from: corners[pair],
            to: corners[(pair + 1) % 3],
            width: 1.5,
        });
    }
    if high > low {
        for level_index in 1..=6 {
            let level = low + (high - low) * level_index as f64 / 7.0;
            for i in 0..steps {
                for j in 0..(steps - i) {
                    let mut triangle = [(i, j), (i + 1, j), (i, j + 1)];
                    for _ in 0..2 {
                        let vertices = [
                            (
                                [
                                    1.0 - (triangle[0].0 + triangle[0].1) as f64 / steps as f64,
                                    triangle[0].0 as f64 / steps as f64,
                                    triangle[0].1 as f64 / steps as f64,
                                ],
                                values[triangle[0].0][triangle[0].1],
                            ),
                            (
                                [
                                    1.0 - (triangle[1].0 + triangle[1].1) as f64 / steps as f64,
                                    triangle[1].0 as f64 / steps as f64,
                                    triangle[1].1 as f64 / steps as f64,
                                ],
                                values[triangle[1].0][triangle[1].1],
                            ),
                            (
                                [
                                    1.0 - (triangle[2].0 + triangle[2].1) as f64 / steps as f64,
                                    triangle[2].0 as f64 / steps as f64,
                                    triangle[2].1 as f64 / steps as f64,
                                ],
                                values[triangle[2].0][triangle[2].1],
                            ),
                        ];
                        let mut crossings: Vec<Point> = Vec::with_capacity(3);
                        for edge in [(0usize, 1usize), (1, 2), (2, 0)] {
                            let (first, second) = (vertices[edge.0], vertices[edge.1]);
                            if (first.1 - level) * (second.1 - level) < 0.0 {
                                let share = (level - first.1) / (second.1 - first.1);
                                let a = ternary(&first.0);
                                let b = ternary(&second.0);
                                crossings.push(Point {
                                    x: a.x + share * (b.x - a.x),
                                    y: a.y + share * (b.y - a.y),
                                });
                            }
                        }
                        if crossings.len() == 2 {
                            lines.push(Segment {
                                from: crossings[0],
                                to: crossings[1],
                                width: 1.2,
                            });
                        }
                        // Second triangle of the cell, when it stays inside.
                        if i + j + 2 > steps {
                            break;
                        }
                        triangle = [(i + 1, j), (i + 1, j + 1), (i, j + 1)];
                    }
                }
            }
        }
    }
    let runs: Vec<Point> = model.points.iter().map(|point| ternary(point)).collect();
    let series = vec![Series {
        label: "Runs".to_owned(),
        points: runs,
        lines: Vec::new(),
        bands: Vec::new(),
        secondary_axis: false,
    }];
    PlotSpec {
        title: "Mixture Contour Plot".to_owned(),
        x_axis: linear_axis("Component 2", 0.0, 1.0),
        y_axis: linear_axis("Component 3", 0.0, 0.9),
        size: (820, 760),
        points: Vec::new(),
        lines,
        bands: Vec::new(),
        series,
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Fitted line of a two-component mixture.
fn line_plot(model: &MixtureFit) -> PlotSpec {
    let mut points = Vec::new();
    for step in 0..=80 {
        let share = step as f64 / 80.0;
        let point = [share, 1.0 - share];
        points.push(Point {
            x: share,
            y: model.predict(&point),
        });
    }
    let mut lines = Vec::new();
    if let (Some(first), Some(last)) = (points.first(), points.last()) {
        lines.push(Segment {
            from: *first,
            to: *last,
            width: 2.0,
        });
    }
    let low = points
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let high = points
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let pad = ((high - low).abs() * 0.05).max(1e-9);
    PlotSpec {
        title: "Mixture Fit".to_owned(),
        x_axis: linear_axis(&model.components[0], 0.0, 1.0),
        y_axis: linear_axis(&model.response, low - pad, high + pad),
        size: (900, 560),
        points: model
            .points
            .iter()
            .zip(&model.observed)
            .map(|(point, value)| Point {
                x: point[0],
                y: *value,
            })
            .collect(),
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Runs the fit and builds the report and the plot.
pub fn analyze(
    df: &DataFrame,
    response_column: &str,
    components: &[String],
    model: MixtureModel,
) -> Result<AnalysisOutput> {
    let fitted = fit(df, response_column, components, model)?;
    let mut result = AnalysisResult::new(
        format!("Mixture Analysis — response {response_column}"),
        format!(
            "Scheffé {} model of {} components, fitted without an intercept",
            fitted.model.name(),
            fitted.components.len()
        ),
    );
    result.n = fitted.n;
    result.excluded = fitted.excluded;
    result.statistics = vec![
        ("Components", fitted.components.len() as f64),
        ("Terms", fitted.coefficients.len() as f64),
        ("Observations", fitted.n as f64),
        ("R²", fitted.r_squared),
        ("R² adjusted", fitted.adjusted_r_squared),
        ("RMSE", fitted.rmse),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Term".to_owned(),
            "Coefficient".to_owned(),
            "Std. error".to_owned(),
            "t".to_owned(),
            "p-value".to_owned(),
        ],
        rows: fitted
            .coefficients
            .iter()
            .map(|coefficient| {
                vec![
                    coefficient.term.clone(),
                    format!("{:.6}", coefficient.estimate),
                    format!("{:.6}", coefficient.standard_error),
                    format!("{:.6}", coefficient.t),
                    format!("{:.6}", coefficient.p_value),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Run".to_owned(),
            "Observed".to_owned(),
            "Fitted".to_owned(),
            "Residual".to_owned(),
        ],
        rows: (0..fitted.n)
            .map(|run| {
                vec![
                    (run + 1).to_string(),
                    format!("{:.6}", fitted.observed[run]),
                    format!("{:.6}", fitted.fitted[run]),
                    format!("{:.6}", fitted.residuals[run]),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Model".to_owned(),
            match fitted.model {
                MixtureModel::Linear => "η = Σ βᵢ·xᵢ".to_owned(),
                MixtureModel::Quadratic => "η = Σ βᵢ·xᵢ + Σ βᵢⱼ·xᵢxⱼ".to_owned(),
                MixtureModel::SpecialCubic => "η = Σ βᵢ·xᵢ + Σ βᵢⱼ·xᵢxⱼ + Σ βᵢⱼₖ·xᵢxⱼxₖ".to_owned(),
            },
        ),
        (
            "Coefficients".to_owned(),
            "a linear coefficient is the response of the pure component; an interaction \
             coefficient is the excess of the blend over the additive prediction"
                .to_owned(),
        ),
        (
            "Constraint".to_owned(),
            "the components of every run must lie in [0, 1] and sum to one (within 1e-5 for \
             rounded worksheets); the canonical form needs no intercept"
                .to_owned(),
        ),
        (
            "Fit".to_owned(),
            "ordinary least squares; s² = SSE/(n − p) and the standard errors come from \
             (XᵀX)⁻¹"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows missing the response or a component are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the Scheffé conventions are standard choices; they have not been verified against \
         documented output. Predictions are only meaningful inside the simplex"
            .to_owned(),
    );
    let plot = match fitted.components.len() {
        2 => Some(line_plot(&fitted)),
        3 => Some(ternary_plot(&fitted)),
        _ => None,
    };
    Ok(AnalysisOutput {
        results: vec![result],
        plot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    /// Simplex lattice of degree 3 with three extra centroid runs.
    fn lattice_frame() -> DataFrame {
        let points: [[f64; 3]; 13] = [
            [0.0, 0.0, 1.0],
            [0.0, 1.0 / 3.0, 2.0 / 3.0],
            [0.0, 2.0 / 3.0, 1.0 / 3.0],
            [0.0, 1.0, 0.0],
            [1.0 / 3.0, 0.0, 2.0 / 3.0],
            [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
            [1.0 / 3.0, 2.0 / 3.0, 0.0],
            [2.0 / 3.0, 0.0, 1.0 / 3.0],
            [2.0 / 3.0, 1.0 / 3.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
            [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
            [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
        ];
        let response = [
            40.7, 37.944_444, 35.411_111, 30.0, 31.322_222, 34.066_667, 29.088_889, 25.255_556,
            26.155_556, 19.8, 33.766_667, 33.266_667, 34.166_667,
        ];
        DataFrame::new(vec![
            Column::new_f64("x1", points.iter().map(|point| point[0]).collect()),
            Column::new_f64("x2", points.iter().map(|point| point[1]).collect()),
            Column::new_f64("x3", points.iter().map(|point| point[2]).collect()),
            Column::new_f64("y", response.to_vec()),
        ])
        .expect("frame")
    }

    fn components() -> Vec<String> {
        vec!["x1".to_owned(), "x2".to_owned(), "x3".to_owned()]
    }

    fn coefficient<'a>(model: &'a MixtureFit, term: &str) -> &'a MixtureCoefficient {
        model
            .coefficients
            .iter()
            .find(|coefficient| coefficient.term == term)
            .unwrap_or_else(|| panic!("{term} missing"))
    }

    #[test]
    fn the_quadratic_coefficients_match_the_hand_calculation() {
        let df = lattice_frame();
        let model = fit(&df, "y", &components(), MixtureModel::Quadratic).expect("fit");
        assert_eq!(model.n, 13);
        assert_eq!(model.coefficients.len(), 6);
        for (term, estimate, error) in [
            ("x1", 19.382_166_277_311, 1.165_245_918_437),
            ("x2", 29.467_880_420_168, 1.165_245_918_437),
            ("x3", 39.810_737_420_168, 1.165_245_918_437),
            ("x1*x2", 18.525_631_915_966, 4.819_157_477_177),
            ("x1*x3", -1.745_797_084_034, 4.819_157_477_177),
            ("x2*x3", 13.311_344_344_538, 4.819_157_477_177),
        ] {
            let fitted = coefficient(&model, term);
            assert!(
                (fitted.estimate - estimate).abs() < 1e-9,
                "{term}: {} vs {estimate}",
                fitted.estimate
            );
            assert!(
                (fitted.standard_error - error).abs() < 1e-9,
                "{term}: se {} vs {error}",
                fitted.standard_error
            );
        }
        assert!((model.r_squared - 0.970_780_077_385_859_7).abs() < 1e-10);
        assert!((model.adjusted_r_squared - 0.949_908_704_090_045_2).abs() < 1e-10);
        assert!((model.rmse - 1.252_484_784_713_520_3).abs() < 1e-10);
        // t = 16.6335 with 7 degrees of freedom: p = 6.936e-7.
        let x1 = coefficient(&model, "x1");
        assert!((x1.t - 16.633_541_444_461_12).abs() < 1e-9);
        assert!((x1.p_value - 6.936_085_437_155_082e-7).abs() < 1e-12);
        let x1x3 = coefficient(&model, "x1*x3");
        assert!((x1x3.p_value - 0.727_846_102_731_936_7).abs() < 1e-9);
    }

    #[test]
    fn the_special_cubic_recovers_the_cubic_coefficients() {
        let df = lattice_frame();
        let model = fit(&df, "y", &components(), MixtureModel::SpecialCubic).expect("fit");
        assert_eq!(model.coefficients.len(), 7);
        for (term, estimate) in [
            ("x1", 19.995_238_238_095),
            ("x2", 30.080_952_380_952),
            ("x3", 40.423_809_380_952),
            ("x1*x2*x3", 70.350_007_5),
        ] {
            let fitted = coefficient(&model, term);
            assert!(
                (fitted.estimate - estimate).abs() < 1e-9,
                "{term}: {} vs {estimate}",
                fitted.estimate
            );
        }
        assert!((model.r_squared - 0.996_283_542_545_888_5).abs() < 1e-10);
        assert!((model.rmse - 0.482_470_657_056_657_3).abs() < 1e-10);
        let x1 = coefficient(&model, "x1");
        assert!((x1.p_value - 9.785_318_622_465_317e-9).abs() < 1e-12);
    }

    #[test]
    fn the_linear_model_of_a_two_component_blend_is_identified() {
        // y = 2·x1 + 6·x2 exactly at the three points of a two-component blend.
        let df = DataFrame::new(vec![
            Column::new_f64("x1", vec![1.0, 0.5, 0.0]),
            Column::new_f64("x2", vec![0.0, 0.5, 1.0]),
            Column::new_f64("y", vec![2.0, 4.0, 6.0]),
        ])
        .expect("frame");
        let model = fit(
            &df,
            "y",
            &["x1".to_owned(), "x2".to_owned()],
            MixtureModel::Linear,
        )
        .expect("fit");
        assert_eq!(model.coefficients.len(), 2);
        assert!((coefficient(&model, "x1").estimate - 2.0).abs() < 1e-12);
        assert!((coefficient(&model, "x2").estimate - 6.0).abs() < 1e-12);
        assert!((model.predict(&[0.25, 0.75]) - 5.0).abs() < 1e-12);
        // The fit is exact: every residual is at rounding level.
        assert!(model.r_squared > 1.0 - 1e-12);
        assert!(model.residuals.iter().all(|value| value.abs() < 1e-12));
        let out = analyze(
            &df,
            "y",
            &["x1".to_owned(), "x2".to_owned()],
            MixtureModel::Linear,
        )
        .expect("analysis");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.title, "Mixture Fit");
    }

    #[test]
    fn the_ternary_plot_carries_the_runs_and_the_contours() {
        let df = lattice_frame();
        let out = analyze(&df, "y", &components(), MixtureModel::Quadratic).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Mixture Analysis"), "{text}");
        assert!(text.contains("Scheffé quadratic"), "{text}");
        assert!(text.contains("x1*x2"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.title, "Mixture Contour Plot");
        assert_eq!(plot.series.len(), 1);
        assert_eq!(plot.series[0].points.len(), 13);
        // The outline and the contours are segments of the plot.
        assert!(plot.lines.len() >= 6);
    }

    #[test]
    fn invalid_mixture_settings_are_rejected() {
        let df = DataFrame::new(vec![
            Column::new_f64("x1", vec![1.0, 0.5, 0.0]),
            Column::new_f64("x2", vec![0.0, 0.5, 1.0]),
            Column::new_f64("y", vec![2.0, 4.0, 6.0]),
        ])
        .expect("frame");
        assert!(matches!(
            fit(&df, "y", &["x1".to_owned()], MixtureModel::Linear),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            fit(
                &df,
                "y",
                &["x1".to_owned(), "x1".to_owned()],
                MixtureModel::Linear
            ),
            Err(Error::Unsupported(_))
        ));
        let off = DataFrame::new(vec![
            Column::new_f64("x1", vec![0.8]),
            Column::new_f64("x2", vec![0.8]),
            Column::new_f64("y", vec![1.0]),
        ])
        .expect("frame");
        assert!(matches!(
            fit(
                &off,
                "y",
                &["x1".to_owned(), "x2".to_owned()],
                MixtureModel::Linear
            ),
            Err(Error::Cell { .. })
        ));
        let negative = DataFrame::new(vec![
            Column::new_f64("x1", vec![1.5]),
            Column::new_f64("x2", vec![-0.5]),
            Column::new_f64("y", vec![1.0]),
        ])
        .expect("frame");
        assert!(matches!(
            fit(
                &negative,
                "y",
                &["x1".to_owned(), "x2".to_owned()],
                MixtureModel::Linear
            ),
            Err(Error::Cell { .. })
        ));
        assert_eq!(
            MixtureModel::parse("cubic"),
            Some(MixtureModel::SpecialCubic)
        );
        // A three-component special cubic has 3 + 3 + 1 terms.
        assert_eq!(MixtureModel::SpecialCubic.term_count(3), 7);
        assert_eq!(MixtureModel::Quadratic.term_count(4), 10);
    }
}
