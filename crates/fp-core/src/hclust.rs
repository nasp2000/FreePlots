// SPDX-License-Identifier: MIT
//! Agglomerative hierarchical clustering.
//!
//! The observations are the complete rows of the selected numeric columns, on
//! the Euclidean distance (optionally after standardising every variable to
//! `z` scores). Pairs of clusters merge from the smallest distance upwards and
//! the distance between a new cluster and the others follows the
//! **Lance–Williams** recurrence:
//!
//! ```text
//! d(A∪B, C) = α_A·d(A, C) + α_B·d(B, C) + β·d(A, B) + γ·|d(A, C) − d(B, C)|
//! ```
//!
//! with the coefficients of each linkage:
//!
//! - **single**: `α = ½`, `β = 0`, `γ = −½`;
//! - **complete**: `α = ½`, `β = 0`, `γ = ½`;
//! - **average**: `α_A = n_A/(n_A + n_B)`, `α_B = n_B/(n_A + n_B)`;
//! - **Ward**: the squared distance grows by
//!   `((n_C + n_A)·d²(A,C) + (n_C + n_B)·d²(B,C) − n_C·d²(A,B)) / T` with
//!   `T = n_A + n_B + n_C`, so the merge height is the Ward distance
//!   `√(2·Δ)`.
//!
//! The tree is reported through its `n − 1` merges and the partition into the
//! requested number of clusters: cutting after the first `n − k` merges leaves
//! exactly `k` groups. The plot is the dendrogram.
//!
//! The documented behaviour clusters; its linkage and cut conventions were not
//! documented, so the numbers are not claimed to match the documented behaviour
//! — see the project notes.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// How the distance between two clusters is defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linkage {
    /// Nearest pair of observations.
    Single,
    /// Farthest pair of observations.
    Complete,
    /// Mean distance over all pairs.
    Average,
    /// Smallest increase of the within-cluster sum of squares.
    Ward,
}

impl Linkage {
    /// Name used by the interface and the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Complete => "complete",
            Self::Average => "average",
            Self::Ward => "Ward",
        }
    }

    /// Short code used by the command line and the preferences.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Complete => "complete",
            Self::Average => "average",
            Self::Ward => "ward",
        }
    }

    /// Parses the command-line spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "single" | "nearest" => Some(Self::Single),
            "complete" | "furthest" => Some(Self::Complete),
            "average" | "upgma" => Some(Self::Average),
            "ward" => Some(Self::Ward),
            _ => None,
        }
    }
}

/// One merge of the dendrogram.
#[derive(Debug, Clone, PartialEq)]
pub struct Merge {
    /// Identifier of the left child: a leaf (`0..n`) or a previous merge
    /// (`n + index`).
    pub left: usize,
    /// Identifier of the right child.
    pub right: usize,
    /// Distance at which the merge happens.
    pub height: f64,
    /// Observations in the new cluster.
    pub size: usize,
}

/// Fitted hierarchical clustering.
#[derive(Debug, Clone, PartialEq)]
pub struct Hclust {
    /// Variables, in the requested order.
    pub columns: Vec<String>,
    /// Linkage used.
    pub linkage: Linkage,
    /// Whether the variables were standardised.
    pub standardised: bool,
    /// Complete observations used.
    pub n: usize,
    /// The `n − 1` merges, in increasing height.
    pub merges: Vec<Merge>,
    /// Leaf order of the dendrogram (observation indices).
    pub order: Vec<usize>,
    /// Requested number of clusters.
    pub k: usize,
    /// Cluster of each observation (0-based, ordered by first observation).
    pub assignments: Vec<usize>,
    /// Size of each cluster.
    pub sizes: Vec<usize>,
    /// Mean of each variable in each cluster, original scale.
    pub means: Vec<Vec<f64>>,
}

/// Euclidean distance between two observations.
fn euclidean(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(left, right)| (left - right) * (left - right))
        .sum::<f64>()
        .sqrt()
}

/// Extracts the complete rows of the selected numeric columns.
fn complete_rows(df: &DataFrame, columns: &[String]) -> Result<Vec<Vec<f64>>> {
    let mut sources = Vec::with_capacity(columns.len());
    for name in columns {
        sources.push(df.column(name)?.numeric_by_row());
    }
    let mut data = Vec::with_capacity(df.n_rows());
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
    Ok(data)
}

/// Standardises every variable to `z` scores with the `n − 1` deviation.
fn standardise(data: &mut [Vec<f64>]) {
    if data.is_empty() {
        return;
    }
    let p = data[0].len();
    let n = data.len() as f64;
    for column in 0..p {
        let mean: f64 = data.iter().map(|row| row[column]).sum::<f64>() / n;
        let variance: f64 = data
            .iter()
            .map(|row| (row[column] - mean).powi(2))
            .sum::<f64>()
            / (n - 1.0);
        let sd = variance.sqrt();
        let scale = if sd > 0.0 { sd } else { 1.0 };
        for row in data.iter_mut() {
            row[column] = (row[column] - mean) / scale;
        }
    }
}

/// The merged distance of one linkage, following Lance–Williams.
fn merged_distance(
    linkage: Linkage,
    left: f64,
    right: f64,
    between: f64,
    left_size: usize,
    right_size: usize,
    other_size: usize,
) -> f64 {
    match linkage {
        Linkage::Single => left.min(right),
        Linkage::Complete => left.max(right),
        Linkage::Average => {
            (left_size as f64 * left + right_size as f64 * right) / (left_size + right_size) as f64
        }
        Linkage::Ward => {
            let total = (left_size + right_size + other_size) as f64;
            let squared = ((other_size + left_size) as f64 * left * left
                + (other_size + right_size) as f64 * right * right
                - other_size as f64 * between * between)
                / total;
            squared.max(0.0).sqrt()
        }
    }
}

/// Agglomerates the complete rows of the given columns.
///
/// `k` is the number of clusters the tree is cut into; at least two variables,
/// two observations and a `k` between 2 and the observations are needed.
#[allow(clippy::needless_range_loop)]
pub fn fit(
    df: &DataFrame,
    columns: &[String],
    linkage: Linkage,
    standardised: bool,
    k: usize,
) -> Result<Hclust> {
    if columns.len() < 2 {
        return Err(Error::Unsupported(
            "clustering needs at least two variables".to_owned(),
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
    let mut data = complete_rows(df, columns)?;
    let n = data.len();
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    const MAX_OBSERVATIONS: usize = 500;
    if n > MAX_OBSERVATIONS {
        return Err(Error::Unsupported(format!(
            "at most {MAX_OBSERVATIONS} complete observations are accepted"
        )));
    }
    if k < 2 || k > n {
        return Err(Error::InvalidParameter {
            name: "clusters",
            value: k as f64,
        });
    }
    if standardised {
        standardise(&mut data);
    }

    // Distance matrix over the leaves, grown as merges create new ids.
    let mut distances = vec![vec![0.0f64; 2 * n - 1]; 2 * n - 1];
    for i in 0..n {
        for j in (i + 1)..n {
            let distance = euclidean(&data[i], &data[j]);
            distances[i][j] = distance;
            distances[j][i] = distance;
        }
    }
    let mut sizes = vec![1usize; 2 * n];
    let mut active = vec![true; 2 * n];
    let mut members: Vec<Vec<usize>> = (0..n).map(|leaf| vec![leaf]).collect();
    let mut merges = Vec::with_capacity(n - 1);
    for step in 0..(n - 1) {
        let mut best = (0usize, 1usize);
        let mut smallest = f64::INFINITY;
        for i in 0..(n + step) {
            if !active[i] {
                continue;
            }
            for j in (i + 1)..(n + step) {
                if active[j] && distances[i][j] < smallest {
                    smallest = distances[i][j];
                    best = (i, j);
                }
            }
        }
        let (left, right) = best;
        let id = n + step;
        sizes[id] = sizes[left] + sizes[right];
        let mut combined = members[left].clone();
        combined.extend(&members[right]);
        members.push(combined);
        merges.push(Merge {
            left,
            right,
            height: smallest,
            size: sizes[id],
        });
        for other in 0..(n + step) {
            if !active[other] || other == left || other == right {
                continue;
            }
            let distance = merged_distance(
                linkage,
                distances[left][other],
                distances[right][other],
                distances[left][right],
                sizes[left],
                sizes[right],
                sizes[other],
            );
            distances[id][other] = distance;
            distances[other][id] = distance;
        }
        active[left] = false;
        active[right] = false;
        active[id] = true;
    }

    // Cut after the first n - k merges: every observation takes the first
    // observation of the last cluster that absorbed it.
    let mut labels: Vec<usize> = (0..n).collect();
    for step in 0..(n - k) {
        let id = n + step;
        let first = members[id][0];
        for leaf in &members[id] {
            labels[*leaf] = first;
        }
    }
    let mut roots: Vec<usize> = Vec::new();
    let mut assignments: Vec<usize> = Vec::with_capacity(n);
    for label in labels {
        let index = match roots.iter().position(|value| *value == label) {
            Some(position) => position,
            None => {
                roots.push(label);
                roots.len() - 1
            }
        };
        assignments.push(index);
    }
    let k = roots.len();
    let mut counts = vec![0usize; k];
    for label in &assignments {
        counts[*label] += 1;
    }
    let mut means = vec![vec![0.0; columns.len()]; k];
    for (row, label) in assignments.iter().enumerate() {
        for (column, value) in means[*label].iter_mut().enumerate() {
            *value += data[row][column];
        }
    }
    for (cluster, mean) in means.iter_mut().enumerate() {
        for value in mean.iter_mut() {
            *value /= counts[cluster] as f64;
        }
    }

    let mut order = Vec::with_capacity(n);
    fn visit(id: usize, n: usize, merges: &[Merge], order: &mut Vec<usize>) {
        if id < n {
            order.push(id);
        } else {
            let merge = &merges[id - n];
            visit(merge.left, n, merges, order);
            visit(merge.right, n, merges, order);
        }
    }
    visit(2 * n - 2, n, &merges, &mut order);

    Ok(Hclust {
        columns: columns.to_vec(),
        linkage,
        standardised,
        n,
        merges,
        order,
        k,
        assignments,
        sizes: counts,
        means,
    })
}

/// Dendrogram of the tree.
fn dendrogram(model: &Hclust) -> PlotSpec {
    let n = model.n;
    let mut xs = vec![0.0f64; 2 * n - 1];
    for (position, leaf) in model.order.iter().enumerate() {
        xs[*leaf] = position as f64 + 1.0;
    }
    let mut heights = vec![0.0f64; 2 * n - 1];
    let mut lines = Vec::new();
    let mut points = Vec::new();
    for (index, merge) in model.merges.iter().enumerate() {
        let id = n + index;
        xs[id] = 0.5 * (xs[merge.left] + xs[merge.right]);
        heights[id] = merge.height;
        points.push(Point {
            x: xs[id],
            y: merge.height,
        });
        for child in [merge.left, merge.right] {
            lines.push(Segment {
                from: Point {
                    x: xs[child],
                    y: heights[child],
                },
                to: Point {
                    x: xs[child],
                    y: merge.height,
                },
                width: 2.0,
            });
        }
        lines.push(Segment {
            from: Point {
                x: xs[merge.left],
                y: merge.height,
            },
            to: Point {
                x: xs[merge.right],
                y: merge.height,
            },
            width: 2.0,
        });
    }
    let highest = model
        .merges
        .iter()
        .map(|merge| merge.height)
        .fold(0.0, f64::max);
    PlotSpec {
        title: "Dendrogram".to_owned(),
        x_axis: linear_axis("Observation", 1.0, n as f64),
        y_axis: linear_axis("Distance", 0.0, (highest * 1.05).max(1e-9)),
        size: (900, 540),
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

/// Runs the clustering and builds the report and the dendrogram.
pub fn analyze(
    df: &DataFrame,
    columns: &[String],
    linkage: Linkage,
    standardised: bool,
    k: usize,
) -> Result<AnalysisOutput> {
    let model = fit(df, columns, linkage, standardised, k)?;
    let mut result = AnalysisResult::new(
        "Hierarchical Clustering".to_owned(),
        format!(
            "{} linkage, {} clusters, {} variables{}",
            model.linkage.name(),
            model.k,
            model.columns.len(),
            if model.standardised {
                ", standardised"
            } else {
                ""
            }
        ),
    );
    result.n = model.n;
    result.excluded = df.n_rows() - model.n;
    result.statistics = vec![
        ("Observations", model.n as f64),
        ("Variables", model.columns.len() as f64),
        ("Clusters", model.k as f64),
        ("First merge", model.merges[0].height),
        (
            "Last merge",
            model
                .merges
                .last()
                .map(|merge| merge.height)
                .unwrap_or(f64::NAN),
        ),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Merge".to_owned(),
            "Left".to_owned(),
            "Right".to_owned(),
            "Height".to_owned(),
            "Size".to_owned(),
        ],
        rows: model
            .merges
            .iter()
            .enumerate()
            .map(|(index, merge)| {
                vec![
                    (index + 1).to_string(),
                    merge.left.to_string(),
                    merge.right.to_string(),
                    format!("{:.6}", merge.height),
                    merge.size.to_string(),
                ]
            })
            .collect(),
    });
    let mut headers = vec!["Cluster".to_owned(), "Size".to_owned()];
    headers.extend(model.columns.iter().cloned());
    result.extra.push(Item::Table {
        headers,
        rows: (0..model.k)
            .map(|cluster| {
                let mut row = vec![(cluster + 1).to_string(), model.sizes[cluster].to_string()];
                row.extend(
                    model.means[cluster]
                        .iter()
                        .map(|value| format!("{value:.6}")),
                );
                row
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Distance".to_owned(),
            "Euclidean distance between the observations; every merge joins the pair of \
             clusters with the smallest distance"
                .to_owned(),
        ),
        (
            "Linkage".to_owned(),
            match model.linkage {
                Linkage::Single => {
                    "single linkage: the distance between clusters is the distance of their \
                     nearest pair"
                        .to_owned()
                }
                Linkage::Complete => {
                    "complete linkage: the distance between clusters is the distance of their \
                     farthest pair"
                        .to_owned()
                }
                Linkage::Average => {
                    "average linkage: the distance between clusters is the mean over every pair"
                        .to_owned()
                }
                Linkage::Ward => {
                    "Ward: each merge adds the smallest increase of the within-cluster sum of \
                     squares, and the height is √(2·Δ)"
                        .to_owned()
                }
            },
        ),
        (
            "Cut".to_owned(),
            format!(
                "the tree is cut after the first {} merges, which leaves {} clusters",
                model.n - model.k,
                model.k
            ),
        ),
        (
            "Data policy".to_owned(),
            "rows complete in every variable are used; the others are excluded and counted"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the linkage and cut conventions are standard choices; they have not been verified \
         against documented output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(dendrogram(&model)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame(values: Vec<f64>) -> DataFrame {
        DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame")
    }

    #[test]
    fn the_single_linkage_merges_the_nearest_pair() {
        // 1, 2, 4, 8: merges at 1, then {1,2} with 4 at 2, then with 8 at 4.
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 4.0, 8.0]),
            Column::new_f64("y", vec![1.0, 1.0, 1.0, 1.0]),
        ])
        .expect("frame");
        let model = fit(
            &df,
            &["x".to_owned(), "y".to_owned()],
            Linkage::Single,
            false,
            2,
        )
        .expect("fit");
        let heights: Vec<f64> = model.merges.iter().map(|merge| merge.height).collect();
        assert_eq!(heights.len(), 3);
        assert!((heights[0] - 1.0).abs() < 1e-12);
        assert!((heights[1] - 2.0).abs() < 1e-12);
        assert!((heights[2] - 4.0).abs() < 1e-12);
        // Cutting after two merges leaves the chain {1, 2, 4} and {8}.
        assert_eq!(model.assignments, vec![0, 0, 0, 1]);
        assert_eq!(model.sizes, vec![3, 1]);
        assert!((model.means[0][0] - 7.0 / 3.0).abs() < 1e-12);
        assert!((model.means[1][0] - 8.0).abs() < 1e-12);
    }

    #[test]
    fn the_complete_and_average_linkages_match_the_definition() {
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 4.0, 8.0]),
            Column::new_f64("y", vec![1.0, 1.0, 1.0, 1.0]),
        ])
        .expect("frame");
        let columns = ["x".to_owned(), "y".to_owned()];
        let complete = fit(&df, &columns, Linkage::Complete, false, 2).expect("fit");
        let heights: Vec<f64> = complete.merges.iter().map(|merge| merge.height).collect();
        // 1-2 at 1, then {1,2} with 4 at max(3, 2) = 3, and 1-8 at 7.
        assert!((heights[0] - 1.0).abs() < 1e-12);
        assert!((heights[1] - 3.0).abs() < 1e-12);
        assert!((heights[2] - 7.0).abs() < 1e-12);
        let average = fit(&df, &columns, Linkage::Average, false, 2).expect("fit");
        let heights: Vec<f64> = average.merges.iter().map(|merge| merge.height).collect();
        // {1,2} with 4 at (3 + 2)/2 = 2.5, then (2·6.5 + 4)/3 = 17/3.
        assert!((heights[1] - 2.5).abs() < 1e-12);
        assert!((heights[2] - 17.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn the_ward_height_is_the_increase_of_the_within_sum_of_squares() {
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 4.0, 8.0]),
            Column::new_f64("y", vec![1.0, 1.0, 1.0, 1.0]),
        ])
        .expect("frame");
        let model = fit(
            &df,
            &["x".to_owned(), "y".to_owned()],
            Linkage::Ward,
            false,
            2,
        )
        .expect("fit");
        let heights: Vec<f64> = model.merges.iter().map(|merge| merge.height).collect();
        // Δ = (|A||B|/T)·||cA − cB||²; the height is √(2Δ).
        let first = ((2.0 * 1.0 / 3.0) * (1.5f64 - 4.0).powi(2)) * 2.0;
        assert!((heights[0] - 1.0).abs() < 1e-12);
        assert!((heights[1] - first.sqrt()).abs() < 1e-12);
        // The last merge joins the mean 7/3 with 8.
        let last = ((3.0 * 1.0 / 4.0) * (7.0f64 / 3.0 - 8.0).powi(2)) * 2.0;
        assert!((heights[2] - last.sqrt()).abs() < 1e-12);
        assert_eq!(model.assignments, vec![0, 0, 0, 1]);
    }

    #[test]
    fn the_standardised_distances_ignore_the_units() {
        // x and y are the same variable, in one frame scaled by ten: the raw
        // distances see the units, the standardised ones do not.
        let plain = DataFrame::new(vec![
            Column::new_f64("x", vec![0.0, 1.0, 2.0, 3.0]),
            Column::new_f64("y", vec![0.0, 1.0, 2.0, 3.0]),
        ])
        .expect("frame");
        let scaled = DataFrame::new(vec![
            Column::new_f64("x", vec![0.0, 1.0, 2.0, 3.0]),
            Column::new_f64("y", vec![0.0, 10.0, 20.0, 30.0]),
        ])
        .expect("frame");
        let columns = ["x".to_owned(), "y".to_owned()];
        let raw = fit(&plain, &columns, Linkage::Single, false, 2).expect("plain");
        let standardised = fit(&scaled, &columns, Linkage::Single, true, 2).expect("scaled");
        assert_eq!(raw.assignments, standardised.assignments);
        // Raw distance between neighbours: √(1² + 10²) ≈ 10.05.
        let scaled_raw = fit(&scaled, &columns, Linkage::Single, false, 2).expect("scaled raw");
        assert!((scaled_raw.merges[0].height - 101.0f64.sqrt()).abs() < 1e-12);
        assert!(standardised.merges[0].height < 2.0);
        // z spacing is 1/√(5/3), so the standardised neighbour distance is
        // √(2·3/5) = √1.2.
        assert!((standardised.merges[0].height - 1.2f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn the_report_and_dendrogram_are_built() {
        let df = frame(vec![1.0, 2.0, 3.0, 8.0, 9.0, 10.0]);
        let out =
            analyze(&df, &["x".to_owned()], Linkage::Average, false, 2).expect_err("one variable");
        assert!(matches!(out, Error::Unsupported(_)));
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 8.0, 9.0, 10.0]),
            Column::new_f64("y", vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]),
        ])
        .expect("frame");
        let out = analyze(
            &df,
            &["x".to_owned(), "y".to_owned()],
            Linkage::Average,
            false,
            2,
        )
        .expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Hierarchical Clustering"), "{text}");
        assert!(text.contains("average linkage"), "{text}");
        assert!(text.contains("Merge"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.title, "Dendrogram");
        assert!(!plot.lines.is_empty());
    }

    #[test]
    fn invalid_settings_are_rejected() {
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0]),
            Column::new_f64("y", vec![1.0, 1.0, 1.0]),
        ])
        .expect("frame");
        let columns = ["x".to_owned(), "y".to_owned()];
        assert!(matches!(
            fit(&df, &columns, Linkage::Single, false, 1),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            fit(&df, &columns, Linkage::Single, false, 4),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            fit(
                &df,
                &["x".to_owned(), "x".to_owned()],
                Linkage::Single,
                false,
                2
            ),
            Err(Error::Unsupported(_))
        ));
        assert_eq!(Linkage::parse("ward"), Some(Linkage::Ward));
        assert!(Linkage::parse("median").is_none());
    }
}
