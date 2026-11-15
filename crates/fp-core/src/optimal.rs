// SPDX-License-Identifier: MIT
//! Optimal experimental designs: D-optimal and maximin-distance selection from
//! a factorial candidate set, augmentation of an existing coded design and
//! evaluation of a design.
//!
//! The candidate set is the full `−1/+1` factorial plus the centre point. The
//! D criterion maximises `|XᵀX|` for the first-order model with intercept; the
//! selection starts from a nonsingular subset, adds the candidate with the
//! largest determinant and improves the set with pairwise exchanges. The
//! distance criterion greedily maximises the smallest pairwise Euclidean
//! distance (maximin). The evaluation reports the D-efficiency, the condition
//! number of `XᵀX` and the minimum/maximum pairwise distances.

use crate::data::Column;
use crate::data::{DType, DataFrame};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::SplitMix64;
use crate::workflow::AnalysisOutput;

/// Selection criterion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptimalCriterion {
    /// Maximise `|XᵀX|`.
    #[default]
    D,
    /// Maximise the minimum pairwise distance.
    Distance,
}

impl OptimalCriterion {
    /// Name used by the interface and the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::D => "D-optimal",
            Self::Distance => "Distance (maximin)",
        }
    }
}

/// Operation of the optimal-design dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptimalMode {
    /// Create a new design.
    #[default]
    Create,
    /// Add runs to the current coded design.
    Augment,
    /// Evaluate a coded design.
    Evaluate,
}

impl OptimalMode {
    /// Name used by the interface.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Create => "Create",
            Self::Augment => "Augment",
            Self::Evaluate => "Evaluate",
        }
    }
}

/// Request of an optimal design.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimalOptions {
    /// Factor names.
    pub factors: Vec<String>,
    /// Number of runs.
    pub runs: usize,
    /// Selection criterion.
    pub criterion: OptimalCriterion,
    /// Randomise the run order.
    pub randomize: bool,
    /// Seed of the randomiser.
    pub seed: u64,
}

/// Candidate points: the full `−1/+1` factorial and the centre.
fn candidates(q: usize) -> Vec<Vec<f64>> {
    let mut points = vec![vec![0.0; q]];
    for mask in 0..(1usize << q) {
        points.push(
            (0..q)
                .map(|j| if mask & (1 << j) != 0 { 1.0 } else { -1.0 })
                .collect(),
        );
    }
    points
}

fn design_row(point: &[f64]) -> Vec<f64> {
    let mut row = Vec::with_capacity(point.len() + 1);
    row.push(1.0);
    row.extend_from_slice(point);
    row
}

/// Determinant by Gaussian elimination with partial pivoting.
#[allow(clippy::needless_range_loop)]
fn determinant(mut matrix: Vec<Vec<f64>>) -> f64 {
    let n = matrix.len();
    let mut det = 1.0;
    for column in 0..n {
        let mut pivot = column;
        for row in (column + 1)..n {
            if matrix[row][column].abs() > matrix[pivot][column].abs() {
                pivot = row;
            }
        }
        if matrix[pivot][column].abs() < 1e-12 {
            return 0.0;
        }
        if pivot != column {
            matrix.swap(column, pivot);
            det = -det;
        }
        let divisor = matrix[column][column];
        det *= divisor;
        for row in (column + 1)..n {
            let factor = matrix[row][column] / divisor;
            for k in column..n {
                matrix[row][k] -= factor * matrix[column][k];
            }
        }
    }
    det
}

/// `|XᵀX|` for the rows of a design.
#[allow(clippy::needless_range_loop)]
fn information_determinant(rows: &[Vec<f64>]) -> f64 {
    let p = rows.first().map_or(0, Vec::len);
    if rows.len() < p || p == 0 {
        return 0.0;
    }
    let mut xtx = vec![vec![0.0; p]; p];
    for row in rows {
        for i in 0..p {
            for j in 0..p {
                xtx[i][j] += row[i] * row[j];
            }
        }
    }
    determinant(xtx)
}

/// A nonsingular starting set with the largest determinant among a small
/// combination of candidates.
fn initial_set(points: &[Vec<f64>], p: usize) -> Option<Vec<usize>> {
    let limit = (p + 6).min(points.len());
    let mut best: Option<(f64, Vec<usize>)> = None;
    let mut current = Vec::new();
    fn walk(
        start: usize,
        limit: usize,
        p: usize,
        points: &[Vec<f64>],
        current: &mut Vec<usize>,
        best: &mut Option<(f64, Vec<usize>)>,
    ) {
        if current.len() == p {
            let rows: Vec<Vec<f64>> = current
                .iter()
                .map(|index| design_row(&points[*index]))
                .collect();
            let det = information_determinant(&rows);
            if det > 1e-12 && best.as_ref().is_none_or(|(value, _)| det > *value) {
                *best = Some((det, current.clone()));
            }
            return;
        }
        for index in start..=limit - (p - current.len()) {
            current.push(index);
            walk(index + 1, limit, p, points, current, best);
            current.pop();
        }
    }
    if limit >= p {
        walk(0, limit, p, points, &mut current, &mut best);
    }
    best.map(|(_, indices)| indices)
}

/// Adds one candidate with the largest determinant (D) or distance.
#[allow(clippy::needless_range_loop)]
fn greedy_add(
    points: &[Vec<f64>],
    selected: &[usize],
    count: usize,
    criterion: OptimalCriterion,
) -> Vec<usize> {
    let mut selected = selected.to_vec();
    while selected.len() < count {
        let mut best: Option<(f64, usize)> = None;
        for index in 0..points.len() {
            if selected.contains(&index) {
                continue;
            }
            let score = match criterion {
                OptimalCriterion::D => {
                    let mut rows: Vec<Vec<f64>> = selected
                        .iter()
                        .map(|chosen| design_row(&points[*chosen]))
                        .collect();
                    rows.push(design_row(&points[index]));
                    information_determinant(&rows)
                }
                OptimalCriterion::Distance => {
                    if selected.is_empty() {
                        f64::INFINITY
                    } else {
                        selected
                            .iter()
                            .map(|chosen| {
                                points[*chosen]
                                    .iter()
                                    .zip(&points[index])
                                    .map(|(a, b)| (a - b).powi(2))
                                    .sum::<f64>()
                                    .sqrt()
                            })
                            .fold(f64::INFINITY, f64::min)
                    }
                }
            };
            if best.as_ref().is_none_or(|(value, _)| score > *value) {
                best = Some((score, index));
            }
        }
        match best {
            Some((_, index)) => selected.push(index),
            None => break,
        }
    }
    selected
}

/// Pairwise exchange that improves the D determinant.
#[allow(clippy::needless_range_loop)]
fn improve_d(selected: &mut Vec<usize>, points: &[Vec<f64>]) {
    for _ in 0..30 {
        let mut improved = false;
        let current_det = {
            let rows: Vec<Vec<f64>> = selected
                .iter()
                .map(|index| design_row(&points[*index]))
                .collect();
            information_determinant(&rows)
        };
        for slot in 0..selected.len() {
            for index in 0..points.len() {
                if selected.contains(&index) {
                    continue;
                }
                let mut trial: Vec<usize> = selected.clone();
                trial[slot] = index;
                let rows: Vec<Vec<f64>> = trial
                    .iter()
                    .map(|chosen| design_row(&points[*chosen]))
                    .collect();
                let det = information_determinant(&rows);
                if det > current_det * (1.0 + 1e-12) {
                    *selected = trial;
                    improved = true;
                    break;
                }
            }
            if improved {
                break;
            }
        }
        if !improved {
            break;
        }
    }
}

/// Euclidean distance between two candidate points.
fn distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(left, right)| (left - right).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// Selects `count` indices from the candidates, optionally forcing the first
/// `forced` points.
fn select(
    points: &[Vec<f64>],
    count: usize,
    criterion: OptimalCriterion,
    forced: &[usize],
) -> Result<Vec<usize>> {
    let q = points.first().map_or(0, Vec::len);
    let p = q + 1;
    if count < p || count > points.len() {
        return Err(Error::Unsupported(format!(
            "choose between {p} and {} runs",
            points.len()
        )));
    }
    let mut selected = if forced.is_empty() {
        initial_set(points, p).ok_or_else(|| Error::NotConverged {
            detail: "no full-rank starting design".to_owned(),
        })?
    } else {
        forced.to_vec()
    };
    match criterion {
        OptimalCriterion::D => {
            selected = greedy_add(points, &selected, count, OptimalCriterion::D);
            if forced.is_empty() {
                improve_d(&mut selected, points);
            } else {
                // Exchange only the new runs, keeping the existing ones.
                for _ in 0..30 {
                    let mut improved = false;
                    let rows_base: Vec<Vec<f64>> = forced
                        .iter()
                        .map(|index| design_row(&points[*index]))
                        .collect();
                    let current_det = {
                        let mut rows = rows_base.clone();
                        rows.extend(
                            selected
                                .iter()
                                .filter(|index| !forced.contains(index))
                                .map(|index| design_row(&points[*index])),
                        );
                        information_determinant(&rows)
                    };
                    for slot in forced.len()..selected.len() {
                        for index in 0..points.len() {
                            if selected.contains(&index) {
                                continue;
                            }
                            let mut trial = selected.clone();
                            trial[slot] = index;
                            let mut rows = rows_base.clone();
                            rows.extend(
                                trial
                                    .iter()
                                    .filter(|value| !forced.contains(value))
                                    .map(|value| design_row(&points[*value])),
                            );
                            if information_determinant(&rows) > current_det * (1.0 + 1e-12) {
                                selected = trial;
                                improved = true;
                                break;
                            }
                        }
                        if improved {
                            break;
                        }
                    }
                    if !improved {
                        break;
                    }
                }
            }
        }
        OptimalCriterion::Distance => {
            selected = greedy_add(points, &selected, count, OptimalCriterion::Distance);
        }
    }
    Ok(selected)
}

/// Builds the worksheet of a selected design.
fn frame(
    factors: &[String],
    points: &[Vec<f64>],
    selected: &[usize],
    randomize: bool,
    seed: u64,
) -> Result<DataFrame> {
    let total = selected.len();
    if total == 0 || total > 1_000_000 {
        return Err(Error::Unsupported("the design is too large".to_owned()));
    }
    let mut std_order: Vec<f64> = (1..=total).map(|value| value as f64).collect();
    let mut runs: Vec<Vec<f64>> = selected
        .iter()
        .map(|index| points[*index].clone())
        .collect();
    let mut run_order: Vec<f64> = (1..=total).map(|value| value as f64).collect();
    if randomize {
        let mut rng = SplitMix64::new(seed);
        for index in (1..total).rev() {
            let swap = (rng.next_u64() % (index as u64 + 1)) as usize;
            std_order.swap(index, swap);
            runs.swap(index, swap);
            run_order.swap(index, swap);
        }
    }
    let mut columns = Vec::with_capacity(factors.len() + 2);
    columns.push(Column::new_f64("StdOrder", std_order));
    columns.push(Column::new_f64("RunOrder", run_order));
    for (index, name) in factors.iter().enumerate() {
        columns.push(Column::new_f64(
            name.clone(),
            runs.iter().map(|run| run[index]).collect(),
        ));
    }
    DataFrame::new(columns)
}

/// Creates a D-optimal or maximin-distance design.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for fewer than two or more than six factors,
/// duplicate names or an invalid run count, and [`Error::NotConverged`] when
/// no full-rank starting design exists.
pub fn optimal_design(options: &OptimalOptions) -> Result<DataFrame> {
    let q = options.factors.len();
    if !(2..=6).contains(&q) {
        return Err(Error::Unsupported(
            "an optimal design needs between two and six factors".to_owned(),
        ));
    }
    let mut unique = options.factors.clone();
    unique.sort();
    unique.dedup();
    if unique.len() != q {
        return Err(Error::Unsupported(
            "the factor names must be unique".to_owned(),
        ));
    }
    let points = candidates(q);
    let selected = select(&points, options.runs, options.criterion, &[])?;
    frame(
        &options.factors,
        &points,
        &selected,
        options.randomize,
        options.seed,
    )
}

/// Adds runs to an existing coded design.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for factors that are not columns, a point
/// outside the `−1/0/+1` candidate set, a singular existing design or too many
/// requested runs.
pub fn augment_design(
    df: &DataFrame,
    factors: &[String],
    add: usize,
    criterion: OptimalCriterion,
    randomize: bool,
    seed: u64,
) -> Result<DataFrame> {
    let q = factors.len();
    if !(2..=6).contains(&q) {
        return Err(Error::Unsupported(
            "augmenting needs between two and six factors".to_owned(),
        ));
    }
    let points = candidates(q);
    let mut existing: Vec<usize> = Vec::new();
    for row in 0..df.n_rows() {
        let mut values = Vec::with_capacity(q);
        let mut complete = true;
        for name in factors {
            let column = df.column(name)?;
            if !matches!(column.dtype(), DType::Float | DType::Integer) {
                return Err(Error::Unsupported(format!(
                    "the factor {name} is not numeric"
                )));
            }
            match column.value(row) {
                Some(value) => values.push(value),
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if !complete {
            continue;
        }
        let index = points
            .iter()
            .position(|point| {
                point
                    .iter()
                    .zip(&values)
                    .all(|(left, right)| (left - right).abs() < 1e-9)
            })
            .ok_or_else(|| {
                Error::Unsupported(
                    "every existing run must be a −1/0/+1 point of the candidate grid".to_owned(),
                )
            })?;
        if existing.contains(&index) {
            return Err(Error::Unsupported(
                "the existing design has duplicate runs; remove them before augmenting".to_owned(),
            ));
        }
        existing.push(index);
    }
    if existing.is_empty() {
        return Err(Error::SampleTooSmall { n: 0, min: 1 });
    }
    let base_rows: Vec<Vec<f64>> = existing
        .iter()
        .map(|index| design_row(&points[*index]))
        .collect();
    if information_determinant(&base_rows) <= 1e-12 {
        return Err(Error::Unsupported(
            "the existing design is singular for the first-order model".to_owned(),
        ));
    }
    let total = existing.len() + add;
    let selected = select(&points, total, criterion, &existing)?;
    frame(factors, &points, &selected, randomize, seed)
}

/// Evaluates a coded design: D-efficiency, condition number and distances.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for a design that is not coded to the
/// candidate grid or is singular, and the column errors.
#[allow(clippy::needless_range_loop)]
pub fn evaluate_design(df: &DataFrame, factors: &[String]) -> Result<AnalysisOutput> {
    let q = factors.len();
    if !(2..=6).contains(&q) {
        return Err(Error::Unsupported(
            "evaluating needs between two and six factors".to_owned(),
        ));
    }
    let points = candidates(q);
    let mut rows: Vec<Vec<f64>> = Vec::new();
    let mut coded: Vec<Vec<f64>> = Vec::new();
    for row in 0..df.n_rows() {
        let mut values = Vec::with_capacity(q);
        for name in factors {
            let column = df.column(name)?;
            let Some(value) = column.value(row) else {
                continue;
            };
            values.push(value);
        }
        if values.len() != q {
            continue;
        }
        let valid = points.iter().any(|point| {
            point
                .iter()
                .zip(&values)
                .all(|(left, right)| (left - right).abs() < 1e-9)
        });
        if !valid {
            return Err(Error::Unsupported(
                "every evaluation run must be a −1/0/+1 point of the candidate grid".to_owned(),
            ));
        }
        rows.push(design_row(&values));
        coded.push(values);
    }
    if rows.len() < q + 1 {
        return Err(Error::SampleTooSmall {
            n: rows.len(),
            min: q + 1,
        });
    }
    let determinant = information_determinant(&rows);
    if determinant <= 1e-12 {
        return Err(Error::Unsupported(
            "the design is singular for the first-order model".to_owned(),
        ));
    }
    let p = q + 1;
    let d_efficiency = determinant.powf(1.0 / p as f64) / rows.len() as f64;
    let mut min_distance = f64::INFINITY;
    let mut max_distance = 0.0_f64;
    for i in 0..coded.len() {
        for j in (i + 1)..coded.len() {
            let value = distance(&coded[i], &coded[j]);
            min_distance = min_distance.min(value);
            max_distance = max_distance.max(value);
        }
    }
    let condition = condition_number(&rows);
    let mut result = AnalysisResult::new(
        format!("Design Evaluation — {}", factors.join(", ")),
        "First-order model with intercept; coded candidate grid",
    );
    result.n = rows.len();
    result.excluded = df.n_rows() - rows.len();
    result.statistics = vec![
        ("Runs", rows.len() as f64),
        ("Factors", q as f64),
        ("D-efficiency", d_efficiency),
        ("Condition number", condition),
        ("Minimum distance", min_distance),
        ("Maximum distance", max_distance),
    ];
    result.extra.push(Item::Table {
        headers: vec!["Quantity".to_owned(), "Value".to_owned()],
        rows: vec![
            vec!["D-efficiency".to_owned(), format!("{d_efficiency:.6}")],
            vec!["Condition number".to_owned(), format!("{condition:.6}")],
            vec!["Minimum distance".to_owned(), format!("{min_distance:.6}")],
            vec!["Maximum distance".to_owned(), format!("{max_distance:.6}")],
        ],
    });
    result.notes = vec![
        (
            "D-efficiency".to_owned(),
            "(|XᵀX|^(1/p))/n with p = q + 1 parameters; one is the best possible value".to_owned(),
        ),
        (
            "Condition".to_owned(),
            "square root of the largest over the smallest eigenvalue of XᵀX".to_owned(),
        ),
        (
            "Distance".to_owned(),
            "Euclidean distances between the coded runs; the minimum measures the space filling"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the criteria are computed for the first-order model and have not been compared with \
         documented output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}

/// Condition number of `XᵀX` by the Jacobi eigenvalue method.
#[allow(clippy::needless_range_loop)]
fn condition_number(rows: &[Vec<f64>]) -> f64 {
    let p = rows.first().map_or(0, Vec::len);
    let mut matrix = vec![vec![0.0; p]; p];
    for row in rows {
        for i in 0..p {
            for j in 0..p {
                matrix[i][j] += row[i] * row[j];
            }
        }
    }
    // Jacobi rotations.
    for _ in 0..50 {
        let mut off = 0.0;
        for i in 0..p {
            for j in (i + 1)..p {
                off += matrix[i][j] * matrix[i][j];
            }
        }
        if off < 1e-18 {
            break;
        }
        for i in 0..p {
            for j in (i + 1)..p {
                if matrix[i][j].abs() < 1e-15 {
                    continue;
                }
                let theta = (matrix[j][j] - matrix[i][i]) / (2.0 * matrix[i][j]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                let aii = matrix[i][i];
                let ajj = matrix[j][j];
                let aij = matrix[i][j];
                matrix[i][i] = c * c * aii - 2.0 * s * c * aij + s * s * ajj;
                matrix[j][j] = s * s * aii + 2.0 * s * c * aij + c * c * ajj;
                matrix[i][j] = 0.0;
                matrix[j][i] = 0.0;
                for k in 0..p {
                    if k != i && k != j {
                        let aik = matrix[i][k];
                        let ajk = matrix[j][k];
                        matrix[i][k] = c * aik - s * ajk;
                        matrix[k][i] = matrix[i][k];
                        matrix[j][k] = s * aik + c * ajk;
                        matrix[k][j] = matrix[j][k];
                    }
                }
            }
        }
    }
    let low = (0..p)
        .map(|index| matrix[index][index])
        .fold(f64::INFINITY, f64::min);
    let high = (0..p)
        .map(|index| matrix[index][index])
        .fold(f64::NEG_INFINITY, f64::max);
    if low <= 0.0 {
        f64::INFINITY
    } else {
        (high / low).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(q: usize, runs: usize, criterion: OptimalCriterion) -> OptimalOptions {
        OptimalOptions {
            factors: (0..q).map(|index| format!("F{index}")).collect(),
            runs,
            criterion,
            randomize: false,
            seed: 7,
        }
    }

    #[test]
    fn the_d_optimal_design_is_full_rank_and_orthogonal() {
        let options = options(3, 8, OptimalCriterion::D);
        let df = optimal_design(&options).expect("design");
        assert_eq!(df.n_rows(), 8);
        let factors: Vec<String> = options.factors.clone();
        let evaluation = evaluate_design(&df, &factors).expect("evaluation");
        let text = evaluation.results[0].to_session_text();
        assert!(text.contains("D-efficiency"), "{text}");
        let d = evaluation.results[0]
            .statistics
            .iter()
            .find(|(name, _)| *name == "D-efficiency")
            .map(|(_, value)| *value)
            .expect("d efficiency");
        assert!(d > 0.9, "{d}");
    }

    #[test]
    fn the_distance_design_spreads_the_points() {
        let df = optimal_design(&options(2, 4, OptimalCriterion::Distance)).expect("design");
        assert_eq!(df.n_rows(), 4);
        let values: Vec<(f64, f64)> = (0..df.n_rows())
            .map(|row| {
                (
                    df.column("F0").expect("F0").value(row).expect("value"),
                    df.column("F1").expect("F1").value(row).expect("value"),
                )
            })
            .collect();
        // The maximin choice is the four corners, not the centre.
        assert!(values.iter().all(|(x, y)| x.abs() == 1.0 && y.abs() == 1.0));
    }

    #[test]
    fn augmenting_keeps_the_existing_runs() {
        let df = DataFrame::new(vec![
            Column::new_f64("F0", vec![-1.0, 1.0, -1.0, 1.0]),
            Column::new_f64("F1", vec![-1.0, -1.0, 1.0, 1.0]),
        ])
        .expect("frame");
        let factors = ["F0".to_owned(), "F1".to_owned()];
        let augmented =
            augment_design(&df, &factors, 2, OptimalCriterion::D, false, 7).expect("augment");
        assert_eq!(augmented.n_rows(), 6);
        for row in 0..4 {
            assert_eq!(
                augmented.column("F0").expect("F0").value(row),
                df.column("F0").expect("F0").value(row)
            );
            assert_eq!(
                augmented.column("F1").expect("F1").value(row),
                df.column("F1").expect("F1").value(row)
            );
        }
    }

    #[test]
    fn invalid_optimal_requests_are_rejected() {
        assert!(optimal_design(&options(2, 2, OptimalCriterion::D)).is_err());
        assert!(optimal_design(&options(2, 6, OptimalCriterion::D)).is_err());
        let mut duplicate = options(2, 4, OptimalCriterion::D);
        duplicate.factors = vec!["A".to_owned(), "A".to_owned()];
        assert!(optimal_design(&duplicate).is_err());
        let singular = DataFrame::new(vec![
            Column::new_f64("F0", vec![0.0, 0.0, 0.0]),
            Column::new_f64("F1", vec![0.0, 0.0, 0.0]),
        ])
        .expect("frame");
        assert!(evaluate_design(&singular, &["F0".to_owned(), "F1".to_owned()]).is_err());
        let uncoded = DataFrame::new(vec![
            Column::new_f64("F0", vec![2.0, 1.0]),
            Column::new_f64("F1", vec![-1.0, 1.0]),
        ])
        .expect("frame");
        assert!(
            augment_design(
                &uncoded,
                &["F0".to_owned(), "F1".to_owned()],
                1,
                OptimalCriterion::D,
                false,
                1,
            )
            .is_err()
        );
    }
}
