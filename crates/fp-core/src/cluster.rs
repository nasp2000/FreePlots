//! K-means clustering.
//!
//! The observations are the complete rows of the selected numeric columns.
//! They are optionally standardised (`z` scores) so that variables measured on
//! different scales weigh the same, the initial centroids are chosen by the
//! **k-means++** rule with the in-tree SplitMix64 generator (so the result is
//! reproducible from the seed), and Lloyd's iterations move the observations to
//! the nearest centroid until no assignment changes.
//!
//! The report gives the cluster sizes, the means of the variables in the
//! original scale, the within-cluster sum of squares, the between-cluster sum
//! of squares and their ratio, and a scatter plot of the first two variables
//! coloured by cluster.
//!
//! The documented behaviour clusters; its initialisation and distance conventions
//! were not documented, so the numbers are not claimed as documented behaviour — see
//! `the project notes`.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Series, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::SplitMix64;
use crate::workflow::AnalysisOutput;

/// Fitted k-means clustering.
#[derive(Debug, Clone, PartialEq)]
pub struct Clusters {
    /// Variables, in the requested order.
    pub columns: Vec<String>,
    /// Number of clusters.
    pub k: usize,
    /// Cluster of each observation (0-based).
    pub assignments: Vec<usize>,
    /// Observations used.
    pub n: usize,
    /// Size of each cluster.
    pub sizes: Vec<usize>,
    /// Mean of each variable in each cluster, original scale.
    pub means: Vec<Vec<f64>>,
    /// Within-cluster sum of squares (standardised scale).
    pub within_ss: f64,
    /// Between-cluster sum of squares (standardised scale).
    pub between_ss: f64,
    /// Fraction of the total sum of squares explained by the clusters.
    pub explained: f64,
    /// Lloyd iterations used.
    pub iterations: usize,
    /// Whether the initialisation used standardised variables.
    pub standardised: bool,
    /// The observations, for the plot.
    pub data: Vec<Vec<f64>>,
}

fn squared_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(left, right)| (left - right) * (left - right))
        .sum()
}

/// Clusters the complete rows of the given columns.
///
/// `k` must be at least 2 and at most half of the observations; `standardise`
/// converts every variable to `z` scores before the distance is computed.
pub fn fit(
    df: &DataFrame,
    columns: &[String],
    k: usize,
    standardise: bool,
    seed: u64,
    max_iterations: usize,
) -> Result<Clusters> {
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
    if n < 4 {
        return Err(Error::SampleTooSmall { n, min: 4 });
    }
    if k < 2 {
        return Err(Error::InvalidParameter {
            name: "clusters",
            value: k as f64,
        });
    }
    if k > n / 2 {
        return Err(Error::Unsupported(
            "the number of clusters must not exceed half of the observations".to_owned(),
        ));
    }
    let p = columns.len();
    let means: Vec<f64> = (0..p)
        .map(|j| data.iter().map(|row| row[j]).sum::<f64>() / n as f64)
        .collect();
    let sds: Vec<f64> = (0..p)
        .map(|j| {
            let variance: f64 = data
                .iter()
                .map(|row| (row[j] - means[j]).powi(2))
                .sum::<f64>()
                / (n - 1) as f64;
            variance.sqrt()
        })
        .collect();
    let scaled: Vec<Vec<f64>> = data
        .iter()
        .map(|row| {
            row.iter()
                .enumerate()
                .map(|(j, value)| {
                    if standardise && sds[j] > 0.0 {
                        (value - means[j]) / sds[j]
                    } else {
                        *value
                    }
                })
                .collect()
        })
        .collect();

    // k-means++ initialisation: the first centroid is random, the next ones
    // are drawn with probability proportional to the squared distance.
    let mut rng = SplitMix64::new(seed);
    let mut centroids: Vec<Vec<f64>> = Vec::with_capacity(k);
    let first = (rng.next_u64() % n as u64) as usize;
    centroids.push(scaled[first].clone());
    while centroids.len() < k {
        let distances: Vec<f64> = scaled
            .iter()
            .map(|row| {
                centroids
                    .iter()
                    .map(|centroid| squared_distance(row, centroid))
                    .fold(f64::INFINITY, f64::min)
            })
            .collect();
        let total: f64 = distances.iter().sum();
        if total <= 0.0 {
            // Every observation coincides with a centroid: the remaining
            // clusters cannot separate anything, so stop with what we have.
            break;
        }
        let mut target = rng.next_f64() * total;
        let mut chosen = n - 1;
        for (index, distance) in distances.iter().enumerate() {
            target -= distance;
            if target <= 0.0 {
                chosen = index;
                break;
            }
        }
        centroids.push(scaled[chosen].clone());
    }
    let k = centroids.len();
    if k < 2 {
        return Err(Error::ConstantSample);
    }

    let mut assignments = vec![usize::MAX; n];
    let mut iterations = 0;
    for iteration in 0..max_iterations.max(1) {
        iterations = iteration + 1;
        let mut changed = false;
        for (index, row) in scaled.iter().enumerate() {
            let mut best = 0;
            let mut best_distance = f64::INFINITY;
            for (cluster, centroid) in centroids.iter().enumerate() {
                let distance = squared_distance(row, centroid);
                if distance < best_distance {
                    best_distance = distance;
                    best = cluster;
                }
            }
            if assignments[index] != best {
                assignments[index] = best;
                changed = true;
            }
        }
        // Recompute the centroids; an empty cluster keeps its previous point.
        let mut sums = vec![vec![0.0; p]; k];
        let mut counts = vec![0usize; k];
        for (index, row) in scaled.iter().enumerate() {
            let cluster = assignments[index];
            counts[cluster] += 1;
            for (j, value) in row.iter().enumerate() {
                sums[cluster][j] += value;
            }
        }
        for cluster in 0..k {
            if counts[cluster] > 0 {
                for j in 0..p {
                    centroids[cluster][j] = sums[cluster][j] / counts[cluster] as f64;
                }
            }
        }
        if !changed {
            break;
        }
    }

    let mut sizes = vec![0usize; k];
    for cluster in &assignments {
        sizes[*cluster] += 1;
    }
    let mut cluster_means = vec![vec![0.0; p]; k];
    for (index, row) in data.iter().enumerate() {
        let cluster = assignments[index];
        for (j, value) in row.iter().enumerate() {
            cluster_means[cluster][j] += value;
        }
    }
    for (cluster, means) in cluster_means.iter_mut().enumerate() {
        if sizes[cluster] > 0 {
            for value in means.iter_mut() {
                *value /= sizes[cluster] as f64;
            }
        }
    }
    let grand: Vec<f64> = (0..p)
        .map(|j| scaled.iter().map(|row| row[j]).sum::<f64>() / n as f64)
        .collect();
    let mut within_ss = 0.0;
    for (index, row) in scaled.iter().enumerate() {
        within_ss += squared_distance(row, &centroids[assignments[index]]);
    }
    let mut between_ss = 0.0;
    for cluster in 0..k {
        between_ss += sizes[cluster] as f64 * squared_distance(&centroids[cluster], &grand);
    }
    let total_ss = within_ss + between_ss;
    let explained = if total_ss > 0.0 {
        (between_ss / total_ss).clamp(0.0, 1.0)
    } else {
        0.0
    };

    Ok(Clusters {
        columns: columns.to_vec(),
        k,
        assignments,
        n,
        sizes,
        means: cluster_means,
        within_ss,
        between_ss,
        explained,
        iterations,
        standardised: standardise,
        data,
    })
}

/// Scatter plot of the first two variables coloured by cluster.
fn plot(model: &Clusters) -> PlotSpec {
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
    let y_min = model
        .data
        .iter()
        .map(|row| row[1])
        .fold(f64::INFINITY, f64::min);
    let y_max = model
        .data
        .iter()
        .map(|row| row[1])
        .fold(f64::NEG_INFINITY, f64::max);
    let x_pad = ((x_max - x_min).abs() * 0.05).max(1e-9);
    let y_pad = ((y_max - y_min).abs() * 0.05).max(1e-9);
    let mut series = Vec::with_capacity(model.k);
    for cluster in 0..model.k {
        let points: Vec<Point> = model
            .data
            .iter()
            .zip(&model.assignments)
            .filter(|(_, assignment)| **assignment == cluster)
            .map(|(row, _)| Point {
                x: row[0],
                y: row[1],
            })
            .collect();
        series.push(Series {
            label: format!("Cluster {}", cluster + 1),
            points,
            lines: Vec::new(),
            bands: Vec::new(),
            secondary_axis: false,
        });
    }
    PlotSpec {
        title: "Cluster Plot".to_owned(),
        x_axis: linear_axis(&model.columns[0], x_min - x_pad, x_max + x_pad),
        y_axis: linear_axis(&model.columns[1], y_min - y_pad, y_max + y_pad),
        size: (900, 560),
        points: Vec::new(),
        lines: Vec::new(),
        bands: Vec::new(),
        series,
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Runs the clustering and builds the report and the cluster plot.
pub fn analyze(
    df: &DataFrame,
    columns: &[String],
    k: usize,
    standardise: bool,
    seed: u64,
) -> Result<AnalysisOutput> {
    let model = fit(df, columns, k, standardise, seed, 100)?;
    let mut result = AnalysisResult::new(
        "K-Means Clustering".to_owned(),
        format!(
            "Lloyd iterations from a k-means++ initialisation (seed {seed}), {} variables{}",
            model.columns.len(),
            if model.standardised {
                ", standardised"
            } else {
                ", original scale"
            }
        ),
    );
    result.n = model.n;
    result.excluded = df.n_rows() - model.n;
    result.statistics = vec![
        ("Clusters", model.k as f64),
        ("Within sum of squares", model.within_ss),
        ("Between sum of squares", model.between_ss),
        ("Explained proportion", model.explained),
        ("Iterations", model.iterations as f64),
        (
            "Smallest cluster",
            *model.sizes.iter().min().unwrap_or(&0) as f64,
        ),
        (
            "Largest cluster",
            *model.sizes.iter().max().unwrap_or(&0) as f64,
        ),
    ];

    result.extra.push(Item::Table {
        headers: vec!["Cluster".to_owned(), "Size".to_owned()],
        rows: model
            .sizes
            .iter()
            .enumerate()
            .map(|(cluster, size)| vec![format!("{}", cluster + 1), format!("{size}")])
            .collect(),
    });
    let mut headers = vec!["Cluster".to_owned()];
    headers.extend(model.columns.iter().cloned());
    result.extra.push(Item::Table {
        headers,
        rows: model
            .means
            .iter()
            .enumerate()
            .map(|(cluster, means)| {
                let mut row = vec![format!("{}", cluster + 1)];
                for value in means {
                    row.push(format!("{value:.6}"));
                }
                row
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Observation".to_owned(),
            "Cluster".to_owned(),
            "Distance".to_owned(),
        ],
        rows: model
            .data
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let cluster = model.assignments[index];
                let mean = &model.means[cluster];
                let distance = row
                    .iter()
                    .zip(mean)
                    .map(|(value, mean)| (value - mean) * (value - mean))
                    .sum::<f64>()
                    .sqrt();
                vec![
                    format!("{}", index + 1),
                    format!("{}", cluster + 1),
                    format!("{distance:.6}"),
                ]
            })
            .collect(),
    });

    result.notes = vec![
        (
            "Method".to_owned(),
            "k-means with Euclidean distance; each observation belongs to the nearest centroid"
                .to_owned(),
        ),
        (
            "Initialisation".to_owned(),
            "k-means++ with SplitMix64, so the same seed reproduces the same clusters".to_owned(),
        ),
        (
            "Sums of squares".to_owned(),
            "within and between are computed on the scale used for the distance".to_owned(),
        ),
        (
            "Standardisation".to_owned(),
            if model.standardised {
                "the variables are z scores, so they weigh the same"
            } else {
                "the variables are on their original scale, so their units weigh"
            }
            .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing value in any variable are excluded (listwise) and counted"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the initialisation, distance and standardisation conventions are standard choices and \
         have not been verified against documented output"
            .to_owned(),
    );
    if let Some(empty) = model.sizes.iter().position(|size| *size == 0) {
        result.warnings.push(format!(
            "cluster {} ended empty; try fewer clusters or a different seed",
            empty + 1
        ));
    }

    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot(&model)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    /// Three well-separated groups of six observations.
    fn frame() -> DataFrame {
        let mut x = Vec::new();
        let mut y = Vec::new();
        for centre in [(0.0, 0.0), (10.0, 10.0), (0.0, 10.0)] {
            for step in 0..6 {
                x.push(centre.0 + step as f64 * 0.1);
                y.push(centre.1 + step as f64 * 0.1);
            }
        }
        DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame")
    }

    #[test]
    fn three_groups_are_recovered() {
        let df = frame();
        let model = fit(&df, &["x".to_owned(), "y".to_owned()], 3, true, 2026, 100).expect("fit");
        assert_eq!(model.n, 18);
        assert_eq!(model.sizes.iter().sum::<usize>(), 18);
        // Every group of six consecutive observations keeps its own cluster.
        for group in 0..3 {
            let label = model.assignments[group * 6];
            for index in group * 6..group * 6 + 6 {
                assert_eq!(
                    model.assignments[index], label,
                    "group {group} index {index}"
                );
            }
            assert_eq!(model.sizes[label], 6);
        }
        assert!(model.explained > 0.9, "explained {}", model.explained);
        assert!(model.within_ss < model.between_ss);
    }

    #[test]
    fn the_seed_is_reproducible_and_the_scale_is_respected() {
        let df = frame();
        let columns = ["x".to_owned(), "y".to_owned()];
        let first = fit(&df, &columns, 2, true, 7, 100).expect("fit");
        let second = fit(&df, &columns, 2, true, 7, 100).expect("fit");
        assert_eq!(first.assignments, second.assignments);
        // Without standardisation the distances use the original units.
        let raw = fit(&df, &columns, 2, false, 7, 100).expect("fit");
        assert!(!raw.standardised);
        assert_eq!(raw.sizes.iter().sum::<usize>(), 18);
    }

    #[test]
    fn invalid_cluster_counts_are_rejected() {
        let df = frame();
        let columns = ["x".to_owned(), "y".to_owned()];
        assert!(matches!(
            fit(&df, &columns, 1, true, 1, 100),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            fit(&df, &columns, 20, true, 1, 100),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            fit(&df, &["x".to_owned()], 2, true, 1, 100),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn the_report_and_the_plot_are_built() {
        let out =
            analyze(&frame(), &["x".to_owned(), "y".to_owned()], 3, true, 2026).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("K-Means Clustering"), "{text}");
        assert!(text.contains("Cluster"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.series.len(), 3);
        let points: usize = plot.series.iter().map(|series| series.points.len()).sum();
        assert_eq!(points, 18);
    }

    #[test]
    fn a_missing_row_does_not_shift_the_columns() {
        let with_missing = DataFrame::new(vec![
            Column::new_f64("x", vec![f64::NAN, 0.0, 0.1, 10.0, 10.1, 20.0, 20.1]),
            Column::new_f64("y", vec![99.0, 0.0, 0.1, 10.0, 10.1, 20.0, 20.1]),
        ])
        .expect("frame");
        let without = DataFrame::new(vec![
            Column::new_f64("x", vec![0.0, 0.1, 10.0, 10.1, 20.0, 20.1]),
            Column::new_f64("y", vec![0.0, 0.1, 10.0, 10.1, 20.0, 20.1]),
        ])
        .expect("frame");
        let columns = vec!["x".to_owned(), "y".to_owned()];
        let a = fit(&with_missing, &columns, 3, false, 2026, 100).expect("clusters");
        let b = fit(&without, &columns, 3, false, 2026, 100).expect("clusters");
        assert_eq!(a.n, 6);
        assert_eq!(a.n, b.n);
        for (left, right) in a.means.iter().zip(&b.means) {
            for (x, y) in left.iter().zip(right) {
                assert!((x - y).abs() < 1e-9, "{:?} vs {:?}", a.means, b.means);
            }
        }
    }
}
