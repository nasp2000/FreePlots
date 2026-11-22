// SPDX-License-Identifier: MIT
//! Linear discriminant analysis.
//!
//! The observations are the complete rows of the predictor columns with a
//! group label. The **pooled within-group covariance** is
//!
//! ```text
//! W = Σ_g (x − m_g)(x − m_g)ᵀ / (n − G)
//! ```
//!
//! and an observation joins the group whose mean is nearest in the
//! **Mahalanobis** distance `(x − m_g)ᵀ·W⁻¹·(x − m_g)` (equal priors). With two
//! groups the direction is Fisher's linear discriminant
//! `a = W⁻¹·(m₁ − m₂)` with the midpoint `½·aᵀ·(m₁ + m₂)`, reported on the
//! original variables and in standardised units (`a_j·s_j`, `s_j` the pooled
//! within-group deviation). The report gives the group means and sizes, the
//! Mahalanobis distances between the group means, the confusion matrix and the
//! resubstitution accuracy.
//!
//! The documented behaviour discriminates; its priors and pooling conventions
//! were not documented, so the numbers are not claimed to match the documented
//! behaviour — see the project notes.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Series, linear_axis, plot_stride, sampling_notes};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// One group of the analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// Label of the group.
    pub label: String,
    /// Observations in the group.
    pub n: usize,
    /// Mean of every predictor.
    pub mean: Vec<f64>,
}

/// Fitted linear discriminant analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct Discriminant {
    /// Predictors, in the requested order.
    pub predictors: Vec<String>,
    /// Group column.
    pub group: String,
    /// Complete observations used.
    pub n: usize,
    /// Groups, in order of first appearance.
    pub groups: Vec<Group>,
    /// Pooled within-group covariance.
    pub covariance: Vec<Vec<f64>>,
    /// Inverse of the pooled covariance.
    pub inverse: Vec<Vec<f64>>,
    /// Mahalanobis distances between the group means (`[g][g]`).
    pub distances: Vec<Vec<f64>>,
    /// Fisher coefficients of the two-group case (`W⁻¹·(m₁ − m₂)`).
    pub coefficients: Option<Vec<f64>>,
    /// Standardised coefficients (`a_j·s_j`).
    pub standardised: Option<Vec<f64>>,
    /// Midpoint of the two-group discriminant scores.
    pub midpoint: Option<f64>,
    /// Group assigned to every observation.
    pub classified: Vec<usize>,
    /// Confusion matrix: `confusion[observed][assigned]`.
    pub confusion: Vec<Vec<usize>>,
    /// Fraction of the observations classified in their own group.
    pub accuracy: f64,
    /// Predictor values of every observation.
    pub data: Vec<Vec<f64>>,
    /// Observed group of every observation.
    pub observed: Vec<usize>,
}

/// Identity matrix.
fn identity(p: usize) -> Vec<Vec<f64>> {
    let mut matrix = vec![vec![0.0; p]; p];
    for (index, row) in matrix.iter_mut().enumerate() {
        row[index] = 1.0;
    }
    matrix
}

/// Gauss–Jordan inverse with partial pivoting.
fn invert(matrix: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let p = matrix.len();
    let mut a = matrix.to_vec();
    let mut inverse = identity(p);
    for column in 0..p {
        let mut pivot = column;
        for row in (column + 1)..p {
            if a[row][column].abs() > a[pivot][column].abs() {
                pivot = row;
            }
        }
        if a[pivot][column].abs() < 1e-12 {
            return None;
        }
        a.swap(column, pivot);
        inverse.swap(column, pivot);
        let scale = a[column][column];
        for value in a[column].iter_mut() {
            *value /= scale;
        }
        for value in inverse[column].iter_mut() {
            *value /= scale;
        }
        for row in 0..p {
            if row == column {
                continue;
            }
            let factor = a[row][column];
            if factor == 0.0 {
                continue;
            }
            for index in 0..p {
                a[row][index] -= factor * a[column][index];
                inverse[row][index] -= factor * inverse[column][index];
            }
        }
    }
    Some(inverse)
}

/// Quadratic form `xᵀ·M·x`.
fn quadratic(form: &[Vec<f64>], vector: &[f64]) -> f64 {
    let p = vector.len();
    let mut total = 0.0;
    for i in 0..p {
        for j in 0..p {
            total += vector[i] * form[i][j] * vector[j];
        }
    }
    total
}

/// Squared Mahalanobis distance between two points.
fn mahalanobis(inverse: &[Vec<f64>], difference: &[f64]) -> f64 {
    quadratic(inverse, difference).max(0.0)
}

/// Fits the discriminant analysis.
///
/// At least two predictors, two groups and `n > G + p` complete rows are
/// needed; the pooled covariance must be invertible.
#[allow(clippy::needless_range_loop)]
pub fn fit(df: &DataFrame, predictors: &[String], group: &str) -> Result<Discriminant> {
    if predictors.is_empty() {
        return Err(Error::Unsupported(
            "discriminant analysis needs at least one predictor".to_owned(),
        ));
    }
    if predictors.len() > 10 {
        return Err(Error::Unsupported(
            "at most 10 predictors are accepted".to_owned(),
        ));
    }
    let mut seen = predictors.to_vec();
    seen.sort();
    seen.dedup();
    if seen.len() != predictors.len() {
        return Err(Error::Unsupported(
            "the predictors must be distinct columns".to_owned(),
        ));
    }
    if predictors.iter().any(|name| name == group) {
        return Err(Error::Unsupported(
            "the group column cannot also be a predictor".to_owned(),
        ));
    }
    let label_column = df.column(group)?;
    let mut sources = Vec::with_capacity(predictors.len());
    for name in predictors {
        sources.push(df.column(name)?.numeric_by_row());
    }
    let mut labels: Vec<String> = Vec::new();
    let mut index_of: Vec<usize> = Vec::new();
    let mut data: Vec<Vec<f64>> = Vec::new();
    let mut observed: Vec<usize> = Vec::new();
    for row in 0..df.n_rows() {
        let Some(label) = label_column.label(row) else {
            continue;
        };
        let mut values = Vec::with_capacity(predictors.len());
        let mut complete = true;
        for source in &sources {
            if !source[row].is_finite() {
                complete = false;
                break;
            }
            values.push(source[row]);
        }
        if !complete {
            continue;
        }
        let group_index = match labels.iter().position(|value| *value == label) {
            Some(position) => position,
            None => {
                labels.push(label);
                labels.len() - 1
            }
        };
        index_of.push(group_index);
        data.push(values);
        observed.push(group_index);
    }
    let n = data.len();
    let p = predictors.len();
    let g = labels.len();
    if g < 2 {
        return Err(Error::Unsupported(
            "at least two groups are needed".to_owned(),
        ));
    }
    if g > 10 {
        return Err(Error::Unsupported(
            "at most 10 groups are accepted".to_owned(),
        ));
    }
    if n <= g + p {
        return Err(Error::Unsupported(format!(
            "the analysis needs more than G + p = {} complete observations, found {n}",
            g + p
        )));
    }

    let mut sizes = vec![0usize; g];
    let mut means = vec![vec![0.0; p]; g];
    for (row, group_index) in index_of.iter().enumerate() {
        sizes[*group_index] += 1;
        for (column, value) in means[*group_index].iter_mut().enumerate() {
            *value += data[row][column];
        }
    }
    for (group_index, mean) in means.iter_mut().enumerate() {
        for value in mean.iter_mut() {
            *value /= sizes[group_index] as f64;
        }
    }
    let mut covariance = vec![vec![0.0; p]; p];
    for (row, values) in data.iter().enumerate() {
        let group_index = index_of[row];
        for i in 0..p {
            for j in i..p {
                covariance[i][j] +=
                    (values[i] - means[group_index][i]) * (values[j] - means[group_index][j]);
            }
        }
    }
    let degrees = (n - g) as f64;
    for i in 0..p {
        for j in i..p {
            covariance[i][j] /= degrees;
            covariance[j][i] = covariance[i][j];
        }
    }
    let inverse = invert(&covariance).ok_or_else(|| {
        Error::Unsupported("the pooled within-group covariance is singular".to_owned())
    })?;

    let mut distances = vec![vec![0.0; g]; g];
    for i in 0..g {
        for j in 0..g {
            let difference: Vec<f64> = (0..p).map(|k| means[i][k] - means[j][k]).collect();
            distances[i][j] = mahalanobis(&inverse, &difference).sqrt();
        }
    }

    let mut classified = Vec::with_capacity(n);
    let mut confusion = vec![vec![0usize; g]; g];
    let mut correct = 0usize;
    for (row, values) in data.iter().enumerate() {
        let mut best = 0usize;
        let mut smallest = f64::INFINITY;
        for (group_index, mean) in means.iter().enumerate() {
            let difference: Vec<f64> = (0..p).map(|k| values[k] - mean[k]).collect();
            let squared = mahalanobis(&inverse, &difference);
            if squared < smallest {
                smallest = squared;
                best = group_index;
            }
        }
        classified.push(best);
        confusion[observed[row]][best] += 1;
        if best == observed[row] {
            correct += 1;
        }
    }

    let (coefficients, standardised, midpoint) = if g == 2 {
        let difference: Vec<f64> = (0..p).map(|k| means[0][k] - means[1][k]).collect();
        let coefficients: Vec<f64> = (0..p)
            .map(|i| (0..p).map(|j| inverse[i][j] * difference[j]).sum())
            .collect();
        let standardised: Vec<f64> = coefficients
            .iter()
            .enumerate()
            .map(|(index, value)| value * covariance[index][index].max(0.0).sqrt())
            .collect();
        let sum: f64 = (0..p)
            .map(|k| coefficients[k] * (means[0][k] + means[1][k]))
            .sum::<f64>()
            / 2.0;
        (Some(coefficients), Some(standardised), Some(sum))
    } else {
        (None, None, None)
    };

    Ok(Discriminant {
        predictors: predictors.to_vec(),
        group: group.to_owned(),
        n,
        groups: labels
            .into_iter()
            .zip(sizes.iter().zip(&means))
            .map(|(label, (size, mean))| Group {
                label,
                n: *size,
                mean: mean.clone(),
            })
            .collect(),
        covariance,
        inverse,
        distances,
        coefficients,
        standardised,
        midpoint,
        classified,
        confusion,
        accuracy: correct as f64 / n as f64,
        data,
        observed,
    })
}

/// Scatter plot of the first two predictors coloured by group.
///
/// With a single predictor the observation order is the vertical axis.
fn plot(model: &Discriminant) -> PlotSpec {
    let has_second = model.predictors.len() > 1;
    let y_value = |index: usize| {
        if has_second {
            model.data[index][1]
        } else {
            index as f64
        }
    };
    let x_min = model
        .data
        .iter()
        .map(|row| row[0])
        .fold(f64::INFINITY, f64::min);
    let x_max = model
        .data
        .iter()
        .map(|row| row[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let y_min = (0..model.data.len())
        .map(y_value)
        .fold(f64::INFINITY, f64::min);
    let y_max = (0..model.data.len())
        .map(y_value)
        .fold(f64::NEG_INFINITY, f64::max);
    let x_pad = ((x_max - x_min).abs() * 0.05).max(1e-9);
    let y_pad = ((y_max - y_min).abs() * 0.05).max(1e-9);
    let stride = plot_stride(model.data.len());
    let series = model
        .groups
        .iter()
        .enumerate()
        .map(|(group_index, group)| Series {
            label: group.label.clone(),
            points: model
                .data
                .iter()
                .enumerate()
                .filter(|(index, _)| model.observed[*index] == group_index)
                .step_by(stride)
                .map(|(index, row)| Point {
                    x: row[0],
                    y: y_value(index),
                })
                .collect(),
            lines: Vec::new(),
            bands: Vec::new(),
            secondary_axis: false,
        })
        .collect();
    PlotSpec {
        title: "Discriminant Plot".to_owned(),
        x_axis: linear_axis(&model.predictors[0], x_min - x_pad, x_max + x_pad),
        y_axis: linear_axis(
            if has_second {
                model.predictors[1].as_str()
            } else {
                "Observation"
            },
            y_min - y_pad,
            y_max + y_pad,
        ),
        size: (900, 560),
        points: Vec::new(),
        lines: Vec::new(),
        bands: Vec::new(),
        series,
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: sampling_notes(model.data.len()),
    }
}

/// Runs the analysis and builds the report and the group plot.
pub fn analyze(df: &DataFrame, predictors: &[String], group: &str) -> Result<AnalysisOutput> {
    let model = fit(df, predictors, group)?;
    let mut result = AnalysisResult::new(
        "Discriminant Analysis".to_owned(),
        format!(
            "Linear discriminant with {} predictor(s) and {} groups (equal priors)",
            model.predictors.len(),
            model.groups.len()
        ),
    );
    result.n = model.n;
    result.excluded = df.n_rows() - model.n;
    result.statistics = vec![
        ("Observations", model.n as f64),
        ("Predictors", model.predictors.len() as f64),
        ("Groups", model.groups.len() as f64),
        ("Accuracy", model.accuracy),
    ];
    let mut headers = vec!["Group".to_owned(), "n".to_owned()];
    headers.extend(model.predictors.iter().cloned());
    result.extra.push(Item::Table {
        headers,
        rows: model
            .groups
            .iter()
            .map(|group| {
                let mut row = vec![group.label.clone(), group.n.to_string()];
                row.extend(group.mean.iter().map(|value| format!("{value:.6}")));
                row
            })
            .collect(),
    });
    if let Some(coefficients) = &model.coefficients {
        result.extra.push(Item::Table {
            headers: vec![
                "Variable".to_owned(),
                "Coefficient".to_owned(),
                "Standardised".to_owned(),
            ],
            rows: model
                .predictors
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    vec![
                        name.clone(),
                        format!("{:.6}", coefficients[index]),
                        format!(
                            "{:.6}",
                            model
                                .standardised
                                .as_ref()
                                .map_or(f64::NAN, |values| values[index])
                        ),
                    ]
                })
                .collect(),
        });
        result
            .statistics
            .push(("Midpoint", model.midpoint.unwrap_or(f64::NAN)));
    }
    let mut confusion_headers = vec!["Observed".to_owned()];
    confusion_headers.extend(model.groups.iter().map(|group| group.label.clone()));
    result.extra.push(Item::Table {
        headers: confusion_headers,
        rows: model
            .groups
            .iter()
            .enumerate()
            .map(|(observed, group)| {
                let mut row = vec![group.label.clone()];
                row.extend(model.confusion[observed].iter().map(ToString::to_string));
                row
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Distance".to_owned(),
            "Mahalanobis distance with the pooled within-group covariance; an observation joins \
             the group of the nearest mean (equal priors)"
                .to_owned(),
        ),
        (
            "Pooling".to_owned(),
            format!(
                "W = Σ_g Σ_rows (x − m_g)(x − m_g)ᵀ / (n − G), with n = {} and G = {}",
                model.n,
                model.groups.len()
            ),
        ),
        (
            "Accuracy".to_owned(),
            "resubstitution: every observation is classified with the means and the covariance \
             estimated from all the rows"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows complete in every predictor and with a group label are used; the others are \
             excluded and counted"
                .to_owned(),
        ),
    ];
    if model.coefficients.is_some() {
        result.notes.push((
            "Coefficients".to_owned(),
            "Fisher's direction a = W⁻¹·(m₁ − m₂); the standardised coefficient multiplies a_j \
             by the pooled within-group deviation s_j"
                .to_owned(),
        ));
    }
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the pooling and prior conventions are standard choices; they have not been verified \
         against documented output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot(&model)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn two_group_frame() -> DataFrame {
        // Means (1, 1) and (5, 1); the within-group covariance is (2/3)·I.
        let x = vec![0.0, 1.0, 1.0, 2.0, 4.0, 5.0, 5.0, 6.0];
        let y = vec![1.0, 0.0, 2.0, 1.0, 1.0, 0.0, 2.0, 1.0];
        let group = vec![
            "A".to_owned(),
            "A".to_owned(),
            "A".to_owned(),
            "A".to_owned(),
            "B".to_owned(),
            "B".to_owned(),
            "B".to_owned(),
            "B".to_owned(),
        ];
        DataFrame::new(vec![
            Column::new_f64("x", x),
            Column::new_f64("y", y),
            Column::new_text("g", group),
        ])
        .expect("frame")
    }

    #[test]
    fn the_two_group_discriminant_matches_the_hand_calculation() {
        let df = two_group_frame();
        let model = fit(&df, &["x".to_owned(), "y".to_owned()], "g").expect("fit");
        assert_eq!(model.n, 8);
        assert_eq!(model.groups.len(), 2);
        assert_eq!(model.groups[0].label, "A");
        assert!((model.groups[0].mean[0] - 1.0).abs() < 1e-12);
        assert!((model.groups[1].mean[0] - 5.0).abs() < 1e-12);
        // W = (2/3)·I, so a = W⁻¹(m_A − m_B) = (3/2)·(−4, 0) = (−6, 0).
        let coefficients = model.coefficients.expect("coefficients");
        assert!((coefficients[0] + 6.0).abs() < 1e-12);
        assert!(coefficients[1].abs() < 1e-12);
        let standardised = model.standardised.expect("standardised");
        assert!((standardised[0] + 6.0 * (2.0f64 / 3.0).sqrt()).abs() < 1e-12);
        // Midpoint ½·aᵀ·(m_A + m_B) = ½·(−6, 0)·(6, 2) = −18.
        assert!((model.midpoint.expect("midpoint") + 18.0).abs() < 1e-12);
        // D² = (−4, 0)ᵀ·(3/2)·I·(−4, 0) = 24.
        assert!((model.distances[0][1] - 24.0f64.sqrt()).abs() < 1e-12);
        assert_eq!(model.accuracy, 1.0);
        assert_eq!(model.confusion[0][0], 4);
        assert_eq!(model.confusion[1][1], 4);
    }

    #[test]
    fn three_groups_are_classified_by_the_nearest_mean() {
        // One predictor: 1, 2 | 4, 5 | 8, 9; the pooled variance is 0.5.
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 4.0, 5.0, 8.0, 9.0]),
            Column::new_text(
                "g",
                vec![
                    "A".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "C".to_owned(),
                    "C".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        let model = fit(&df, &["x".to_owned()], "g").expect("fit");
        assert_eq!(model.groups.len(), 3);
        assert!(model.coefficients.is_none());
        // D² between A (1.5) and B (4.5) is 3² / 0.5 = 18.
        assert!((model.distances[0][1] - 18.0f64.sqrt()).abs() < 1e-12);
        assert!((model.covariance[0][0] - 0.5).abs() < 1e-12);
        assert_eq!(model.accuracy, 1.0);
        assert_eq!(model.classified, model.observed);
    }

    #[test]
    fn the_report_and_plot_are_built() {
        let df = two_group_frame();
        let out = analyze(&df, &["x".to_owned(), "y".to_owned()], "g").expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Discriminant Analysis"), "{text}");
        assert!(text.contains("Mahalanobis"), "{text}");
        assert!(text.contains("Coefficient"), "{text}");
        assert!(text.contains("Midpoint"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.title, "Discriminant Plot");
        assert_eq!(plot.series.len(), 2);
    }

    #[test]
    fn invalid_discriminant_settings_are_rejected() {
        let df = two_group_frame();
        assert!(matches!(
            fit(&df, &["x".to_owned(), "y".to_owned()], "x"),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            fit(&df, &["x".to_owned(), "x".to_owned()], "g"),
            Err(Error::Unsupported(_))
        ));
        // A constant predictor makes the covariance singular.
        let flat = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_text(
                "g",
                vec![
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        assert!(matches!(
            fit(&flat, &["x".to_owned()], "g"),
            Err(Error::Unsupported(_))
        ));
        // One group only.
        let one = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_text("g", vec!["A".to_owned(); 4]),
        ])
        .expect("frame");
        assert!(matches!(
            fit(&one, &["x".to_owned()], "g"),
            Err(Error::Unsupported(_))
        ));
    }
}
