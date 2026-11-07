// SPDX-License-Identifier: MIT
//! Principal component analysis.
//!
//! The variables are standardised (`z = (x − mean)/s`, `s` with `n − 1`) and
//! the components are the eigenvectors of the **correlation** matrix, sorted
//! by decreasing eigenvalue. A component with eigenvalue `λⱼ` explains
//! `λⱼ/p` of the total variance (`p` variables, so the eigenvalues sum to
//! `p`), the loading of a variable on a component is
//! `eigenvector·√λⱼ`, and the score of an observation is the standardised
//! value projected on the eigenvector.
//!
//! The eigenvectors come from a Jacobi sweep on the symmetric matrix, and the
//! sign of each one is fixed so that its largest loading is positive — without
//! that rule the sign of a component would depend on rounding.
//!
//! The documented behaviour has a principal-components analysis; its extraction
//! and sign conventions were not documented, so the numbers are not claimed to
//! match the documented behaviour — see the project notes.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// Fitted principal component analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct Pca {
    /// Variables, in the requested order.
    pub columns: Vec<String>,
    /// Complete observations used.
    pub n: usize,
    /// Mean of each variable.
    pub means: Vec<f64>,
    /// Standard deviation (`n − 1`) of each variable.
    pub sds: Vec<f64>,
    /// Eigenvalues of the correlation matrix, decreasing.
    pub eigenvalues: Vec<f64>,
    /// Proportion of the variance explained by each component.
    pub explained: Vec<f64>,
    /// Cumulative proportion of the variance.
    pub cumulative: Vec<f64>,
    /// Loadings, `[component][variable]`.
    pub loadings: Vec<Vec<f64>>,
    /// Scores, `[component][row]`.
    pub scores: Vec<Vec<f64>>,
}

/// Jacobi eigen decomposition of a symmetric matrix.
///
/// Returns the eigenvalues in decreasing order and the eigenvectors as
/// columns of the second value, with the sign of each vector fixed so that its
/// largest entry is positive.
#[allow(clippy::needless_range_loop)]
fn jacobi(matrix: &[Vec<f64>]) -> Option<(Vec<f64>, Vec<Vec<f64>>)> {
    let n = matrix.len();
    if n == 0 || matrix.iter().any(|row| row.len() != n) {
        return None;
    }
    let mut a = matrix.to_vec();
    let mut vectors = vec![vec![0.0; n]; n];
    for (index, row) in vectors.iter_mut().enumerate() {
        row[index] = 1.0;
    }
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
        if largest < 1e-14 {
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
        for row in &mut vectors {
            let vkp = row[p];
            let vkq = row[q];
            row[p] = c * vkp - s * vkq;
            row[q] = s * vkp + c * vkq;
        }
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|left, right| a[*right][*right].total_cmp(&a[*left][*left]));
    let eigenvalues: Vec<f64> = order.iter().map(|index| a[*index][*index]).collect();
    let mut eigenvectors: Vec<Vec<f64>> = Vec::with_capacity(n);
    for index in order {
        let mut vector: Vec<f64> = (0..n).map(|row| vectors[row][index]).collect();
        // Fix the sign through the largest loading.
        let mut best = 0usize;
        for (position, value) in vector.iter().enumerate() {
            if value.abs() > vector[best].abs() {
                best = position;
            }
        }
        if vector[best] < 0.0 {
            for value in &mut vector {
                *value = -*value;
            }
        }
        eigenvectors.push(vector);
    }
    Some((eigenvalues, eigenvectors))
}

/// Correlation (or covariance) matrix of the columns of a matrix of rows.
fn association_matrix(data: &[Vec<f64>], standardise: bool) -> Vec<Vec<f64>> {
    let p = data[0].len();
    let n = data.len();
    let means: Vec<f64> = (0..p)
        .map(|j| data.iter().map(|row| row[j]).sum::<f64>() / n as f64)
        .collect();
    let mut matrix = vec![vec![0.0; p]; p];
    let mut sds = vec![1.0; p];
    if standardise {
        for (j, sd) in sds.iter_mut().enumerate() {
            let variance: f64 = data
                .iter()
                .map(|row| (row[j] - means[j]).powi(2))
                .sum::<f64>()
                / (n - 1) as f64;
            *sd = variance.sqrt();
            if *sd <= 0.0 {
                *sd = 1.0;
            }
        }
    }
    for i in 0..p {
        for j in i..p {
            let mut sum = 0.0;
            for row in data {
                sum += (row[i] - means[i]) / sds[i] * ((row[j] - means[j]) / sds[j]);
            }
            let value = sum / (n - 1) as f64;
            matrix[i][j] = value;
            matrix[j][i] = value;
        }
    }
    matrix
}

/// Fits the principal components of the given columns (correlation matrix).
pub fn fit(df: &DataFrame, columns: &[String], keep: usize) -> Result<Pca> {
    if columns.len() < 2 {
        return Err(Error::Unsupported(
            "the analysis needs at least two variables".to_owned(),
        ));
    }
    let mut seen = columns.to_vec();
    seen.sort();
    seen.dedup();
    if seen.len() != columns.len() {
        return Err(Error::Unsupported(
            "the variables must be distinct columns".to_owned(),
        ));
    }
    let mut sources = Vec::with_capacity(columns.len());
    for name in columns {
        sources.push(df.column(name)?.numeric_by_row());
    }
    let mut data: Vec<Vec<f64>> = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        let mut values = Vec::with_capacity(columns.len());
        let mut complete = true;
        for source in &sources {
            if !source[row].is_finite() {
                complete = false;
                break;
            }
            values.push(source[row]);
        }
        if complete {
            data.push(values);
        }
    }
    let n = data.len();
    if n < 3 {
        return Err(Error::SampleTooSmall { n, min: 3 });
    }
    let p = columns.len();
    const MAX_VARIABLES: usize = 20;
    if p > MAX_VARIABLES {
        return Err(Error::Unsupported(format!(
            "at most {MAX_VARIABLES} variables are accepted"
        )));
    }
    if n < p + 1 {
        return Err(Error::Unsupported(
            "the analysis needs more observations than variables".to_owned(),
        ));
    }
    let matrix = association_matrix(&data, true);
    let (eigenvalues, eigenvectors) = jacobi(&matrix).ok_or_else(|| Error::NotConverged {
        detail: "the Jacobi sweep did not converge".to_owned(),
    })?;
    let total: f64 = eigenvalues.iter().sum();
    if total <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let explained: Vec<f64> = eigenvalues.iter().map(|value| value / total).collect();
    let mut cumulative = Vec::with_capacity(p);
    let mut running = 0.0;
    for value in &explained {
        running += value;
        cumulative.push(running.clamp(0.0, 1.0));
    }
    let keep = keep.clamp(1, p);
    let loadings: Vec<Vec<f64>> = eigenvectors
        .iter()
        .zip(&eigenvalues)
        .map(|(vector, eigenvalue)| {
            let scale = eigenvalue.max(0.0).sqrt();
            vector.iter().map(|value| value * scale).collect()
        })
        .collect();
    let means: Vec<f64> = (0..p)
        .map(|j| data.iter().map(|row| row[j]).sum::<f64>() / n as f64)
        .collect();
    let sds: Vec<f64> = (0..p)
        .map(|j| {
            (data
                .iter()
                .map(|row| (row[j] - means[j]).powi(2))
                .sum::<f64>()
                / (n - 1) as f64)
                .sqrt()
        })
        .collect();
    let scores: Vec<Vec<f64>> = (0..keep)
        .map(|component| {
            data.iter()
                .map(|row| {
                    let mut score = 0.0;
                    for j in 0..p {
                        let sd = if sds[j] > 0.0 { sds[j] } else { 1.0 };
                        score += (row[j] - means[j]) / sd * eigenvectors[component][j];
                    }
                    score
                })
                .collect()
        })
        .collect();
    Ok(Pca {
        columns: columns.to_vec(),
        n,
        means,
        sds,
        eigenvalues,
        explained,
        cumulative,
        loadings,
        scores,
    })
}

/// Scree plot of the eigenvalues.
fn scree_plot(model: &Pca) -> PlotSpec {
    let high = model
        .eigenvalues
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let points: Vec<Point> = model
        .eigenvalues
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
        title: "Scree Plot".to_owned(),
        x_axis: linear_axis("Component", 1.0, model.eigenvalues.len() as f64),
        y_axis: linear_axis("Eigenvalue", 0.0, (high * 1.05).max(1e-9)),
        size: (900, 520),
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

/// Runs the analysis and builds the report and the scree plot.
pub fn analyze(df: &DataFrame, columns: &[String], keep: usize) -> Result<AnalysisOutput> {
    let model = fit(df, columns, keep)?;
    let mut result = AnalysisResult::new(
        "Principal Component Analysis".to_owned(),
        format!(
            "Eigenvectors of the correlation matrix of {} variables",
            model.columns.len()
        ),
    );
    result.n = model.n;
    result.excluded = df.n_rows() - model.n;
    result.statistics = vec![
        ("Variables", model.columns.len() as f64),
        ("Eigenvalue 1", model.eigenvalues[0]),
        (
            "Proportion 1",
            model.explained.first().copied().unwrap_or(f64::NAN),
        ),
        (
            "Cumulative 1",
            model.cumulative.first().copied().unwrap_or(f64::NAN),
        ),
        (
            "Proportion 2",
            model.explained.get(1).copied().unwrap_or(f64::NAN),
        ),
        (
            "Cumulative 2",
            model.cumulative.get(1).copied().unwrap_or(f64::NAN),
        ),
    ];

    result.extra.push(Item::Table {
        headers: vec![
            "Component".to_owned(),
            "Eigenvalue".to_owned(),
            "Proportion".to_owned(),
            "Cumulative".to_owned(),
        ],
        rows: model
            .eigenvalues
            .iter()
            .enumerate()
            .map(|(index, value)| {
                vec![
                    format!("PC{}", index + 1),
                    format!("{value:.6}"),
                    format!("{:.6}", model.explained[index]),
                    format!("{:.6}", model.cumulative[index]),
                ]
            })
            .collect(),
    });
    let mut headers = vec!["Variable".to_owned()];
    for index in 0..model.eigenvalues.len() {
        headers.push(format!("PC{}", index + 1));
    }
    let loadings: Vec<Vec<String>> = model
        .columns
        .iter()
        .enumerate()
        .map(|(variable, name)| {
            let mut row = vec![name.clone()];
            for component in &model.loadings {
                row.push(format!("{:.6}", component[variable]));
            }
            row
        })
        .collect();
    result.extra.push(Item::Table {
        headers,
        rows: loadings,
    });
    if !model.scores.is_empty() {
        let mut headers = vec!["Row".to_owned()];
        for index in 0..model.scores.len() {
            headers.push(format!("Score PC{}", index + 1));
        }
        let rows: Vec<Vec<String>> = (0..model.n)
            .map(|row| {
                let mut cells = vec![format!("{}", row + 1)];
                for component in &model.scores {
                    cells.push(format!("{:.6}", component[row]));
                }
                cells
            })
            .collect();
        result.extra.push(Item::Table { headers, rows });
    }
    result.notes = vec![
        (
            "Method".to_owned(),
            "eigenvectors of the correlation matrix (variables standardised with n − 1); the \
             eigenvalues sum to the number of variables"
                .to_owned(),
        ),
        (
            "Loadings".to_owned(),
            "eigenvector·√eigenvalue: the correlation of each variable with the component"
                .to_owned(),
        ),
        (
            "Signs".to_owned(),
            "each eigenvector is flipped so that its largest loading is positive".to_owned(),
        ),
        (
            "Scores".to_owned(),
            "standardised observations projected on the eigenvectors".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing value in any variable are excluded (listwise) and counted"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the extraction (correlation matrix) and sign conventions are standard choices and have \
         not been verified against documented output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(scree_plot(&model)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    /// Two variables that are almost the same, plus an independent third.
    fn frame() -> DataFrame {
        let mut a = Vec::new();
        let mut b = Vec::new();
        let mut c = Vec::new();
        for index in 0..40 {
            let x = index as f64;
            a.push(x);
            b.push(2.0 * x + ((index % 3) as f64) * 0.01);
            c.push(((index * 7) % 11) as f64);
        }
        DataFrame::new(vec![
            Column::new_f64("a", a),
            Column::new_f64("b", b),
            Column::new_f64("c", c),
        ])
        .expect("frame")
    }

    #[test]
    fn the_eigenvalues_of_a_correlation_matrix_sum_to_the_variables() {
        let model = fit(
            &frame(),
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
            2,
        )
        .expect("fit");
        let total: f64 = model.eigenvalues.iter().sum();
        assert!((total - 3.0).abs() < 1e-9, "{total}");
        // The eigenvalues come in decreasing order.
        for pair in model.eigenvalues.windows(2) {
            assert!(pair[0] >= pair[1] - 1e-12);
        }
        // The explained proportions are normalised and cumulative.
        let explained: f64 = model.explained.iter().sum();
        assert!((explained - 1.0).abs() < 1e-9);
        assert!((model.cumulative.last().copied().unwrap_or(0.0) - 1.0).abs() < 1e-9);
        // The nearly duplicated variables share the first component.
        assert!(model.explained[0] > 0.55, "{:?}", model.explained);
    }

    #[test]
    fn the_scores_are_centred_and_orthogonal() {
        let model = fit(
            &frame(),
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
            3,
        )
        .expect("fit");
        assert_eq!(model.scores.len(), 3);
        for scores in &model.scores {
            let mean = scores.iter().sum::<f64>() / scores.len() as f64;
            assert!(mean.abs() < 1e-9, "mean {mean}");
        }
        let first = &model.scores[0];
        let second = &model.scores[1];
        let dot: f64 = first.iter().zip(second).map(|(a, b)| a * b).sum();
        assert!(dot.abs() < 1e-6, "dot {dot}");
    }

    #[test]
    fn duplicate_columns_are_rejected() {
        let df = frame();
        assert!(matches!(
            fit(&df, &["a".to_owned(), "a".to_owned()], 1),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            fit(&df, &["a".to_owned()], 1),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn the_report_and_the_scree_plot_are_built() {
        let out = analyze(
            &frame(),
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
            2,
        )
        .expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Principal Component Analysis"), "{text}");
        assert!(text.contains("Eigenvalue"), "{text}");
        assert!(text.contains("Score PC1"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.points.len(), 3);
    }

    #[test]
    fn a_missing_row_does_not_shift_the_columns() {
        let rows = [
            (1.0, 1.1, 5.0),
            (2.0, 2.1, 4.0),
            (3.0, 2.9, 3.0),
            (4.0, 4.1, 2.0),
            (5.0, 4.9, 1.0),
            (6.0, 6.1, 0.5),
        ];
        let mut with_missing = rows.to_vec();
        with_missing.insert(0, (f64::NAN, 99.0, 99.0));
        let frame_of = |rows: &[(f64, f64, f64)]| {
            DataFrame::new(vec![
                Column::new_f64("a", rows.iter().map(|row| row.0).collect()),
                Column::new_f64("b", rows.iter().map(|row| row.1).collect()),
                Column::new_f64("c", rows.iter().map(|row| row.2).collect()),
            ])
            .expect("frame")
        };
        let columns = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let a = fit(&frame_of(&with_missing), &columns, 2).expect("pca");
        let b = fit(&frame_of(&rows), &columns, 2).expect("pca");
        assert_eq!(a.n, 6);
        for (left, right) in a.eigenvalues.iter().zip(&b.eigenvalues) {
            assert!(
                (left - right).abs() < 1e-9,
                "{:?} vs {:?}",
                a.eigenvalues,
                b.eigenvalues
            );
        }
    }
}
