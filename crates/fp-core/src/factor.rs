// SPDX-License-Identifier: MIT
//! Factor analysis with principal-axis extraction.
//!
//! The variables are standardised (`n − 1`) and the factors come from the
//! eigenvectors of the **correlation** matrix, with the loadings of the first
//! `m` factors `L[j][i] = eigenvector[j][i]·√λⱼ`. This is the principal
//! component (principal-axis on ones) extraction:
//!
//! - the **communality** of a variable is `Σ_j L[j][i]²` and its
//!   **uniqueness** is `1 − communality`;
//! - a factor explains `Σ_i L[j][i]²` of the total variance `p`;
//! - the **varimax** rotation turns the factors inside their plane to maximise
//!   `Σ_i Σ_j (L[j][i]/h_i)⁴`, the Kaiser criterion, which keeps the
//!   communalities and the total variance and spreads the loadings.
//!
//! The sign of every factor is fixed so that its largest loading is positive,
//! so the solution does not depend on rounding.
//!
//! The documented behaviour has a factor analysis; its extraction and rotation
//! conventions were not documented, so the numbers are not claimed to match the
//! documented behaviour — see the project notes.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::pca::{association_matrix, jacobi};
use crate::plot::{PlotSpec, Point, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// Rotation applied to the factors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    /// Keep the principal-axis loadings.
    None,
    /// Kaiser's varimax (orthogonal, maximises the fourth powers).
    Varimax,
}

impl Rotation {
    /// Name used by the interface and the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Varimax => "varimax",
        }
    }

    /// Short code used by the command line and the preferences.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Varimax => "varimax",
        }
    }

    /// Parses the command-line spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "none" | "unrotated" => Some(Self::None),
            "varimax" => Some(Self::Varimax),
            _ => None,
        }
    }
}

/// Fitted factor analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct FactorAnalysis {
    /// Variables, in the requested order.
    pub columns: Vec<String>,
    /// Complete observations used.
    pub n: usize,
    /// Factors kept.
    pub factors: usize,
    /// Rotation applied.
    pub rotation: Rotation,
    /// Eigenvalues of the correlation matrix, decreasing.
    pub eigenvalues: Vec<f64>,
    /// Loadings after rotation, `[factor][variable]`.
    pub loadings: Vec<Vec<f64>>,
    /// Loadings before rotation, `[factor][variable]`.
    pub unrotated: Vec<Vec<f64>>,
    /// Communality of each variable.
    pub communalities: Vec<f64>,
    /// Uniqueness of each variable (`1 − communality`).
    pub uniqueness: Vec<f64>,
    /// Variance explained by each factor (sum of squared loadings).
    pub variance: Vec<f64>,
    /// Proportion of the total variance explained by the factors together.
    pub total_explained: f64,
}

/// Kaiser's varimax on a `rows × factors` loading matrix (rows are variables).
///
/// Returns `true` when the rotation moved the loadings. The rows are normalised
/// by their communalities during the sweeps and rescaled afterwards.
#[allow(clippy::needless_range_loop)]
fn varimax(loadings: &mut [Vec<f64>], rows: usize, factors: usize) -> bool {
    if factors < 2 || rows == 0 {
        return false;
    }
    let mut norms = vec![0.0f64; rows];
    for (row, values) in loadings.iter_mut().enumerate() {
        norms[row] = values.iter().map(|value| value * value).sum::<f64>().sqrt();
        if norms[row] > 0.0 {
            for value in values.iter_mut() {
                *value /= norms[row];
            }
        }
    }
    let mut moved = false;
    for _ in 0..100 {
        let mut rotated = false;
        for first in 0..factors {
            for second in (first + 1)..factors {
                let mut numerator = 0.0;
                let mut denominator = 0.0;
                let mut sum_difference = 0.0;
                let mut sum_product = 0.0;
                for row in 0..rows {
                    let left = loadings[row][first];
                    let right = loadings[row][second];
                    let difference = left * left - right * right;
                    let product = 2.0 * left * right;
                    numerator += difference * product;
                    denominator += difference * difference - product * product;
                    sum_difference += difference;
                    sum_product += product;
                }
                let p = rows as f64;
                let numerator = 2.0 * numerator - 2.0 * sum_product * sum_difference / p;
                let denominator =
                    denominator - (sum_difference * sum_difference - sum_product * sum_product) / p;
                let angle = -0.25 * numerator.atan2(denominator);
                if angle.abs() < 1e-10 {
                    continue;
                }
                rotated = true;
                moved = true;
                let (sine, cosine) = angle.sin_cos();
                for row in 0..rows {
                    let left = loadings[row][first];
                    let right = loadings[row][second];
                    loadings[row][first] = cosine * left - sine * right;
                    loadings[row][second] = sine * left + cosine * right;
                }
            }
        }
        if !rotated {
            break;
        }
    }
    for (row, values) in loadings.iter_mut().enumerate() {
        if norms[row] > 0.0 {
            for value in values.iter_mut() {
                *value *= norms[row];
            }
        }
    }
    moved
}

/// Eigenvalues and loadings extracted from a correlation matrix.
pub(crate) struct Extraction {
    eigenvalues: Vec<f64>,
    unrotated: Vec<Vec<f64>>,
    loadings: Vec<Vec<f64>>,
}

/// Builds the loadings and variance from a correlation matrix.
///
/// Exposed for tests: `matrix` must be a valid correlation matrix and
/// `factors` between one and its dimension.
pub(crate) fn from_correlation(
    matrix: &[Vec<f64>],
    factors: usize,
    rotation: Rotation,
) -> Result<Extraction> {
    let p = matrix.len();
    if p < 2 {
        return Err(Error::Unsupported(
            "factor analysis needs at least two variables".to_owned(),
        ));
    }
    if factors < 1 || factors > p {
        return Err(Error::InvalidParameter {
            name: "factors",
            value: factors as f64,
        });
    }
    let (eigenvalues, eigenvectors) = jacobi(matrix).ok_or_else(|| Error::NotConverged {
        detail: "the Jacobi sweep did not converge".to_owned(),
    })?;
    let total: f64 = eigenvalues.iter().sum();
    if total <= 0.0 {
        return Err(Error::ConstantSample);
    }
    // eigenvectors[factor][variable]
    let unrotated: Vec<Vec<f64>> = (0..factors)
        .map(|factor| {
            let scale = eigenvalues[factor].max(0.0).sqrt();
            (0..p)
                .map(|variable| eigenvectors[factor][variable] * scale)
                .collect()
        })
        .collect();
    let mut rows: Vec<Vec<f64>> = (0..p)
        .map(|variable| {
            (0..factors)
                .map(|factor| unrotated[factor][variable])
                .collect()
        })
        .collect();
    if rotation == Rotation::Varimax {
        varimax(&mut rows, p, factors);
    }
    let loadings: Vec<Vec<f64>> = (0..factors)
        .map(|factor| (0..p).map(|variable| rows[variable][factor]).collect())
        .collect();
    Ok(Extraction {
        eigenvalues,
        unrotated,
        loadings,
    })
}

/// Fits the factor analysis of the given columns.
///
/// At least two variables, more observations than variables and a factor count
/// between one and the variables are needed.
pub fn fit(
    df: &DataFrame,
    columns: &[String],
    factors: usize,
    rotation: Rotation,
) -> Result<FactorAnalysis> {
    if columns.len() < 2 {
        return Err(Error::Unsupported(
            "factor analysis needs at least two variables".to_owned(),
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
    let p = columns.len();
    if n < 3 {
        return Err(Error::SampleTooSmall { n, min: 3 });
    }
    if n <= p {
        return Err(Error::Unsupported(
            "the analysis needs more observations than variables".to_owned(),
        ));
    }
    let matrix = association_matrix(&data, true);
    let Extraction {
        eigenvalues,
        unrotated,
        loadings,
    } = from_correlation(&matrix, factors, rotation)?;
    let communalities: Vec<f64> = (0..p)
        .map(|variable| {
            (0..factors)
                .map(|factor| loadings[factor][variable].powi(2))
                .sum()
        })
        .collect();
    let uniqueness = communalities.iter().map(|value| 1.0 - value).collect();
    let variance: Vec<f64> = (0..factors)
        .map(|factor| {
            (0..p)
                .map(|variable| loadings[factor][variable].powi(2))
                .sum()
        })
        .collect();
    let total_explained = (variance.iter().sum::<f64>() / p as f64).clamp(0.0, 1.0);
    Ok(FactorAnalysis {
        columns: columns.to_vec(),
        n,
        factors,
        rotation,
        eigenvalues,
        loadings,
        unrotated,
        communalities,
        uniqueness,
        variance,
        total_explained,
    })
}

/// Loadings plot on the first two factors.
fn loading_plot(model: &FactorAnalysis) -> PlotSpec {
    let points: Vec<Point> = model
        .columns
        .iter()
        .enumerate()
        .map(|(variable, _)| Point {
            x: model.loadings[0][variable],
            y: model.loadings.get(1).map_or(0.0, |row| row[variable]),
        })
        .collect();
    let extent = points
        .iter()
        .map(|point| point.x.abs().max(point.y.abs()))
        .fold(0.0, f64::max)
        * 1.15;
    let extent = extent.max(1e-9);
    PlotSpec {
        title: "Factor Loadings".to_owned(),
        x_axis: linear_axis("Factor 1", -extent, extent),
        y_axis: linear_axis("Factor 2", -extent, extent),
        size: (800, 560),
        points,
        lines: Vec::new(),
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Runs the analysis and builds the report and the loadings plot.
pub fn analyze(
    df: &DataFrame,
    columns: &[String],
    factors: usize,
    rotation: Rotation,
) -> Result<AnalysisOutput> {
    let model = fit(df, columns, factors, rotation)?;
    let mut result = AnalysisResult::new(
        "Factor Analysis".to_owned(),
        format!(
            "Principal-axis extraction of {} factors from {} variables, {} rotation",
            model.factors,
            model.columns.len(),
            model.rotation.name()
        ),
    );
    result.n = model.n;
    result.excluded = df.n_rows() - model.n;
    result.statistics = vec![
        ("Variables", model.columns.len() as f64),
        ("Factors", model.factors as f64),
        ("Total variance explained", model.total_explained),
        ("Eigenvalue 1", model.eigenvalues[0]),
        (
            "Eigenvalue 2",
            model.eigenvalues.get(1).copied().unwrap_or(f64::NAN),
        ),
    ];
    let mut headers = vec!["Variable".to_owned()];
    headers.extend((0..model.factors).map(|factor| format!("Factor {}", factor + 1)));
    headers.extend(["Communality".to_owned(), "Uniqueness".to_owned()]);
    result.extra.push(Item::Table {
        headers,
        rows: model
            .columns
            .iter()
            .enumerate()
            .map(|(variable, name)| {
                let mut row = vec![name.clone()];
                row.extend(
                    (0..model.factors)
                        .map(|factor| format!("{:.6}", model.loadings[factor][variable])),
                );
                row.push(format!("{:.6}", model.communalities[variable]));
                row.push(format!("{:.6}", model.uniqueness[variable]));
                row
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Factor".to_owned(),
            "Variance".to_owned(),
            "Proportion".to_owned(),
        ],
        rows: (0..model.factors)
            .map(|factor| {
                vec![
                    format!("Factor {}", factor + 1),
                    format!("{:.6}", model.variance[factor]),
                    format!("{:.6}", model.variance[factor] / model.columns.len() as f64),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Extraction".to_owned(),
            "principal-axis extraction from the correlation matrix: the loadings are the \
             eigenvectors scaled by the square root of their eigenvalues"
                .to_owned(),
        ),
        (
            "Rotation".to_owned(),
            match model.rotation {
                Rotation::None => "no rotation; the factors are the principal axes".to_owned(),
                Rotation::Varimax => {
                    "varimax: the factors turn inside their plane to maximise the sum of the \
                     fourth powers of the normalised loadings, keeping the communalities"
                        .to_owned()
                }
            },
        ),
        (
            "Communality".to_owned(),
            "sum of the squared loadings of a variable; the uniqueness is what the factors do \
             not explain"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows complete in every variable are used; the others are excluded and counted"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the extraction and rotation conventions are standard choices; they have not been \
         verified against documented output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: (model.factors >= 2).then(|| loading_plot(&model)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn correlation(p: usize, r: f64) -> Vec<Vec<f64>> {
        let mut matrix = vec![vec![r; p]; p];
        for (index, row) in matrix.iter_mut().enumerate() {
            row[index] = 1.0;
        }
        matrix
    }

    #[test]
    fn the_loadings_of_a_two_variable_correlation_match_the_eigenvalues() {
        // R = [[1, .5], [.5, 1]]: eigenvalues 1.5 and 0.5, one-factor loadings
        // √0.75 on both variables, communality 0.75.
        let matrix = correlation(2, 0.5);
        let extraction = from_correlation(&matrix, 1, Rotation::None).expect("factors");
        assert!((extraction.eigenvalues[0] - 1.5).abs() < 1e-12);
        assert!((extraction.eigenvalues[1] - 0.5).abs() < 1e-12);
        for variable in 0..2 {
            assert!((extraction.loadings[0][variable] - 0.75f64.sqrt()).abs() < 1e-12);
            assert!((extraction.unrotated[0][variable] - 0.75f64.sqrt()).abs() < 1e-12);
        }
    }

    #[test]
    fn three_equally_correlated_variables_give_equal_loadings() {
        // R with 1 on the diagonal and 0.5 elsewhere: eigenvalues 2, 0.5, 0.5.
        // The first loading is √(2/3) on every variable and the communality
        // with one factor is 2/3; with two factors it reaches one.
        let matrix = correlation(3, 0.5);
        let extraction = from_correlation(&matrix, 1, Rotation::None).expect("factors");
        assert!((extraction.eigenvalues[0] - 2.0).abs() < 1e-12);
        assert!((extraction.eigenvalues[1] - 0.5).abs() < 1e-12);
        for variable in 0..3 {
            assert!((extraction.loadings[0][variable] - (2.0f64 / 3.0).sqrt()).abs() < 1e-12);
        }
        let two = from_correlation(&matrix, 2, Rotation::None).expect("factors");
        for variable in 0..3 {
            let communality: f64 = (0..2)
                .map(|factor| two.loadings[factor][variable].powi(2))
                .sum();
            assert!((communality - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn varimax_recovers_the_simple_structure() {
        // A perfect simple structure turned by 25°: varimax must undo the
        // rotation, keep every communality and never lower the criterion.
        let angle = std::f64::consts::PI * 25.0 / 180.0;
        let (sine, cosine) = angle.sin_cos();
        let target = [[0.9, 0.0], [0.8, 0.0], [0.0, 0.9], [0.0, 0.8]];
        let mut rows: Vec<Vec<f64>> = target
            .iter()
            .map(|row| {
                vec![
                    row[0] * cosine + row[1] * sine,
                    -row[0] * sine + row[1] * cosine,
                ]
            })
            .collect();
        let before: Vec<f64> = rows
            .iter()
            .map(|row| row.iter().map(|value| value * value).sum())
            .collect();
        let criterion = |matrix: &[Vec<f64>]| {
            matrix
                .iter()
                .map(|row| row.iter().map(|value| value.powi(4)).sum::<f64>())
                .sum::<f64>()
        };
        let normalized = |matrix: &[Vec<f64>]| -> Vec<Vec<f64>> {
            matrix
                .iter()
                .map(|row| {
                    let norm = row.iter().map(|value| value * value).sum::<f64>().sqrt();
                    row.iter().map(|value| value / norm).collect()
                })
                .collect()
        };
        let fourth_before = criterion(&normalized(&rows));
        assert!(varimax(&mut rows, 4, 2));
        let after: Vec<f64> = rows
            .iter()
            .map(|row| row.iter().map(|value| value * value).sum())
            .collect();
        for (left, right) in before.iter().zip(&after) {
            assert!((left - right).abs() < 1e-10, "{left} vs {right}");
        }
        assert!(criterion(&normalized(&rows)) > fourth_before);
        // The rotation undoes the 25° turn: every variable loads on one factor.
        for (row, expected) in rows.iter().zip(&target) {
            assert!((row[0].abs() - expected[0]).abs() < 1e-9, "{row:?}");
            assert!((row[1].abs() - expected[1]).abs() < 1e-9, "{row:?}");
        }
    }

    #[test]
    fn the_factor_report_and_plot_are_built() {
        let a: Vec<f64> = (0..20).map(|index| ((index * index) % 17) as f64).collect();
        let b: Vec<f64> = (0..20).map(|index| ((index * 7) % 13) as f64).collect();
        let c: Vec<f64> = (0..20).map(|index| ((index * 5) % 11) as f64).collect();
        let df = DataFrame::new(vec![
            Column::new_f64("a", a),
            Column::new_f64("b", b),
            Column::new_f64("c", c),
        ])
        .expect("frame");
        let out = analyze(
            &df,
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
            2,
            Rotation::Varimax,
        )
        .expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Factor Analysis"), "{text}");
        assert!(text.contains("varimax"), "{text}");
        assert!(text.contains("Communality"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.title, "Factor Loadings");
        assert_eq!(plot.points.len(), 3);
        // One factor has no plane to draw.
        let one = analyze(
            &df,
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
            1,
            Rotation::None,
        )
        .expect("one factor");
        assert!(one.plot.is_none());
    }

    #[test]
    fn invalid_factor_settings_are_rejected() {
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_f64("b", vec![2.0, 3.0, 5.0, 8.0]),
        ])
        .expect("frame");
        let columns = ["a".to_owned(), "b".to_owned()];
        assert!(matches!(
            fit(&df, &columns, 0, Rotation::None),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            fit(&df, &columns, 3, Rotation::None),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            fit(&df, &["a".to_owned(), "a".to_owned()], 1, Rotation::None),
            Err(Error::Unsupported(_))
        ));
        assert_eq!(Rotation::parse("varimax"), Some(Rotation::Varimax));
        assert!(Rotation::parse("quartimax").is_none());
    }
}
