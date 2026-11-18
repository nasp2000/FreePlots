// SPDX-License-Identifier: MIT
//! CART and random forests.
//!
//! **CART** grows one binary tree by choosing, at every node, the split of a
//! numeric predictor that most reduces the impurity: the within-node sum of
//! squares for a numeric response (regression tree, leaf = mean) or the Gini
//! index for a categorical response (classification tree, leaf = majority
//! class). The growth is pre-pruned by the maximum depth, the minimum number of
//! observations per leaf and a minimum gain; there is no cost-complexity
//! pruning.
//!
//! The **random forest** grows `T` trees on bootstrap samples, each split
//! choosing from a random subset of `mtry` predictors (the square root of the
//! number of predictors, at least one). The predictions aggregate by the mean
//! (regression) or the majority vote (classification), the report gives the
//! **out-of-bag error** of the observations left out of each bootstrap sample
//! and the **variable importance** accumulated from the impurity decrease,
//! normalised to percentages. The generator is `SplitMix64` with the seed of
//! the options, so a run is reproducible.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{Band, PlotNote, PlotSpec, Point, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::SplitMix64;
use crate::workflow::AnalysisOutput;

/// Model of the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TreeMode {
    /// One CART tree.
    #[default]
    Cart,
    /// Random forest of bootstrap trees.
    Forest,
}

impl TreeMode {
    /// Name used in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Cart => "CART",
            Self::Forest => "Random Forest",
        }
    }
}

/// Options of the tree models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeOptions {
    /// Maximum depth (the root has depth zero).
    pub max_depth: usize,
    /// Minimum number of observations of a leaf.
    pub min_leaf: usize,
    /// Number of trees of the forest.
    pub trees: usize,
    /// Seed of the random generator.
    pub seed: u64,
    /// Folds of the CART cross-validation (`0` disables it).
    pub folds: usize,
}

impl Default for TreeOptions {
    fn default() -> Self {
        Self {
            max_depth: 6,
            min_leaf: 5,
            trees: 100,
            seed: 2026,
            folds: 0,
        }
    }
}

/// One node of the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeNode {
    /// Identifier, in creation order.
    pub id: usize,
    /// Depth (the root is zero).
    pub depth: usize,
    /// Parent identifier.
    pub parent: Option<usize>,
    /// Split rule (`x <= 1.5`) or `leaf`.
    pub rule: String,
    /// Observations in the node.
    pub n: usize,
    /// Prediction of the node (leaf mean or majority class code).
    pub prediction: f64,
    /// Impurity of the node (sum of squares or Gini).
    pub impurity: f64,
    /// Left child.
    pub left: Option<usize>,
    /// Right child.
    pub right: Option<usize>,
    /// Predictor of the split.
    pub split_predictor: Option<usize>,
    /// Threshold of the split.
    pub split_threshold: Option<f64>,
}

/// Fitted tree or forest.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeFit {
    /// Mode that was run.
    pub mode: TreeMode,
    /// Whether the response is categorical.
    pub classification: bool,
    /// Class labels (classification).
    pub classes: Vec<String>,
    /// Nodes of the CART tree (empty for a forest).
    pub nodes: Vec<TreeNode>,
    /// Predictors, in column order.
    pub predictors: Vec<String>,
    /// Importance in percentages, largest first.
    pub importance: Vec<(String, f64)>,
    /// Coefficient of determination of the regression tree.
    pub r_squared: f64,
    /// Root mean square error of the regression tree.
    pub rmse: f64,
    /// Accuracy of the classification tree.
    pub accuracy: f64,
    /// Confusion matrix (classification).
    pub confusion: Vec<Vec<usize>>,
    /// Out-of-bag error of the forest (MSE or one minus the accuracy).
    pub oob_error: Option<f64>,
    /// Trees of the forest.
    pub trees: usize,
    /// Leaves of the tree (or the average of the forest).
    pub leaves: usize,
    /// Depth reached (or the average of the forest).
    pub depth: usize,
    /// Observations used.
    pub n: usize,
    /// Rows excluded for a missing value.
    pub excluded: usize,
    /// Cross-validated coefficient of determination (regression, CART only).
    pub cv_r_squared: f64,
    /// Cross-validated root mean square error (regression, CART only).
    pub cv_rmse: f64,
    /// Cross-validated accuracy (classification, CART only).
    pub cv_accuracy: f64,
}

fn value(column: &Column, row: usize) -> Option<f64> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => values.get(row).copied().filter(|v| v.is_finite()),
        ColumnData::I64(values) => values.get(row).map(|&v| v as f64),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_) => None,
    }
}

fn label(column: &Column, row: usize) -> Option<String> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::Text(values) => values.get(row).cloned(),
        ColumnData::Date(values) => Some(crate::data::format_date(values[row])),
        ColumnData::I64(values) => values.get(row).map(ToString::to_string),
        ColumnData::F64(values) => values.get(row).map(ToString::to_string),
        ColumnData::Bool(values) => values
            .get(row)
            .map(|&v| if v { "T" } else { "F" }.to_owned()),
    }
}

/// The prepared data of the run.
struct Training {
    x: Vec<Vec<f64>>,
    y: Vec<f64>,
    class: Vec<usize>,
    classes: Vec<String>,
    classification: bool,
    n: usize,
}

fn prepare(df: &DataFrame, response: &str, predictors: &[String]) -> Result<(Training, usize)> {
    if predictors.is_empty() {
        return Err(Error::Unsupported(
            "the model needs at least one predictor".to_owned(),
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
    let response_column = df.column(response)?;
    let predictor_columns: Vec<&Column> = predictors
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    for (name, column) in predictors.iter().zip(&predictor_columns) {
        if matches!(
            column.data(),
            ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
        ) {
            return Err(Error::Csv(format!(
                "predictor column {name} is not numeric"
            )));
        }
    }
    let classification = matches!(
        response_column.data(),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
    );
    let mut classes: Vec<String> = Vec::new();
    let mut x: Vec<Vec<f64>> = Vec::new();
    let mut y: Vec<f64> = Vec::new();
    let mut class: Vec<usize> = Vec::new();
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        let mut values = Vec::with_capacity(predictors.len());
        let mut complete = true;
        for column in &predictor_columns {
            match value(column, row) {
                Some(found) => values.push(found),
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if !complete {
            excluded += 1;
            continue;
        }
        if classification {
            let Some(found) = label(response_column, row) else {
                excluded += 1;
                continue;
            };
            let index = match classes.iter().position(|class| *class == found) {
                Some(index) => index,
                None => {
                    classes.push(found);
                    classes.len() - 1
                }
            };
            class.push(index);
            y.push(index as f64);
        } else {
            let Some(found) = value(response_column, row) else {
                excluded += 1;
                continue;
            };
            y.push(found);
            class.push(0);
        }
        x.push(values);
    }
    let n = x.len();
    if classification && classes.len() < 2 {
        return Err(Error::Unsupported(
            "the classification needs at least two classes".to_owned(),
        ));
    }
    if classification && classes.len() > 20 {
        return Err(Error::Unsupported(
            "the response has more than twenty classes".to_owned(),
        ));
    }
    Ok((
        Training {
            x,
            y,
            class,
            classes,
            classification,
            n,
        },
        excluded,
    ))
}

fn mean(values: &[f64], indices: &[usize]) -> f64 {
    if indices.is_empty() {
        return f64::NAN;
    }
    indices.iter().map(|index| values[*index]).sum::<f64>() / indices.len() as f64
}

fn sse(values: &[f64], indices: &[usize]) -> f64 {
    let mean = mean(values, indices);
    indices
        .iter()
        .map(|index| (values[*index] - mean).powi(2))
        .sum()
}

fn gini(classes: &[usize], indices: &[usize], count: usize) -> f64 {
    if indices.is_empty() {
        return 0.0;
    }
    let mut counts = vec![0usize; count];
    for index in indices {
        counts[classes[*index]] += 1;
    }
    let n = indices.len() as f64;
    1.0 - counts
        .iter()
        .map(|found| (*found as f64 / n).powi(2))
        .sum::<f64>()
}

/// Impurity of a node: sum of squares or Gini times the node size.
fn node_impurity(data: &Training, indices: &[usize]) -> f64 {
    if data.classification {
        gini(&data.class, indices, data.classes.len())
    } else {
        sse(&data.y, indices)
    }
}

fn majority(classes: &[usize], indices: &[usize], count: usize) -> usize {
    let mut counts = vec![0usize; count];
    for index in indices {
        counts[classes[*index]] += 1;
    }
    let mut best = 0usize;
    for (candidate, found) in counts.iter().enumerate() {
        if *found > counts[best] {
            best = candidate;
        }
    }
    best
}

/// Best split of a node: predictor, threshold and impurity decrease.
fn best_split(
    data: &Training,
    indices: &[usize],
    predictors: &[usize],
    min_leaf: usize,
) -> Option<(usize, f64, f64)> {
    let n = indices.len();
    if n < 2 * min_leaf {
        return None;
    }
    let parent = node_impurity(data, indices);
    let step = if n <= 512 { 1 } else { n / 128 };
    let mut best: Option<(usize, f64, f64)> = None;
    for predictor in predictors {
        let mut order: Vec<usize> = indices.to_vec();
        order.sort_by(|left, right| {
            data.x[*left][*predictor].total_cmp(&data.x[*right][*predictor])
        });
        if data.classification {
            let classes = data.classes.len();
            let mut left_counts = vec![0usize; classes];
            let mut right_counts = vec![0usize; classes];
            for index in &order {
                right_counts[data.class[*index]] += 1;
            }
            let mut left_n = 0usize;
            for position in 0..n - 1 {
                let current = order[position];
                left_counts[data.class[current]] += 1;
                right_counts[data.class[current]] -= 1;
                left_n += 1;
                if (position + 1) % step != 0 && position + 1 != n - 1 {
                    continue;
                }
                let next = order[position + 1];
                if data.x[current][*predictor] == data.x[next][*predictor] {
                    continue;
                }
                if left_n < min_leaf || n - left_n < min_leaf {
                    continue;
                }
                let left_size = left_n as f64;
                let right_size = (n - left_n) as f64;
                let left_gini = 1.0
                    - left_counts
                        .iter()
                        .map(|c| (*c as f64 / left_size).powi(2))
                        .sum::<f64>();
                let right_gini = 1.0
                    - right_counts
                        .iter()
                        .map(|c| (*c as f64 / right_size).powi(2))
                        .sum::<f64>();
                let weighted = left_size * left_gini + right_size * right_gini;
                let gain = parent * n as f64 - weighted;
                if gain > 1e-12 && best.is_none_or(|(_, _, found)| gain > found) {
                    let threshold = 0.5 * (data.x[current][*predictor] + data.x[next][*predictor]);
                    best = Some((*predictor, threshold, gain));
                }
            }
        } else {
            let total: f64 = order.iter().map(|index| data.y[*index]).sum();
            let total_sq: f64 = order.iter().map(|index| data.y[*index].powi(2)).sum();
            let mut left_sum = 0.0;
            let mut left_sq = 0.0;
            let mut left_n = 0usize;
            for position in 0..n - 1 {
                let current = order[position];
                left_sum += data.y[current];
                left_sq += data.y[current].powi(2);
                left_n += 1;
                if (position + 1) % step != 0 && position + 1 != n - 1 {
                    continue;
                }
                let next = order[position + 1];
                if data.x[current][*predictor] == data.x[next][*predictor] {
                    continue;
                }
                if left_n < min_leaf || n - left_n < min_leaf {
                    continue;
                }
                let right_n = n - left_n;
                let sse_left = (left_sq - left_sum * left_sum / left_n as f64).max(0.0);
                let right_sum = total - left_sum;
                let right_sq = total_sq - left_sq;
                let sse_right = (right_sq - right_sum * right_sum / right_n as f64).max(0.0);
                let gain = parent - (sse_left + sse_right);
                if gain > 1e-12 && best.is_none_or(|(_, _, found)| gain > found) {
                    let threshold = 0.5 * (data.x[current][*predictor] + data.x[next][*predictor]);
                    best = Some((*predictor, threshold, gain));
                }
            }
        }
    }
    best
}

/// Grows one tree over `data`, optionally with a random subset of predictors
/// per split, and returns the nodes and the importance sums.
fn grow(
    data: &Training,
    options: &TreeOptions,
    mut rng: Option<&mut SplitMix64>,
    mtry: usize,
) -> (Vec<TreeNode>, Vec<f64>) {
    let predictors = data.x.first().map_or(0, Vec::len);
    let mut importance = vec![0.0; predictors];
    let mut nodes: Vec<TreeNode> = vec![TreeNode {
        id: 0,
        depth: 0,
        parent: None,
        rule: "root".to_owned(),
        n: data.n,
        prediction: f64::NAN,
        impurity: 0.0,
        left: None,
        right: None,
        split_predictor: None,
        split_threshold: None,
    }];
    let mut stack: Vec<(usize, Vec<usize>)> = vec![(0, (0..data.n).collect())];
    while let Some((index, indices)) = stack.pop() {
        let impurity = node_impurity(data, &indices);
        let prediction = if data.classification {
            majority(&data.class, &indices, data.classes.len()) as f64
        } else {
            mean(&data.y, &indices)
        };
        {
            let node = &mut nodes[index];
            node.n = indices.len();
            node.impurity = impurity;
            node.prediction = prediction;
        }
        let depth = nodes[index].depth;
        if depth >= options.max_depth || indices.len() < 2 * options.min_leaf || impurity <= 1e-12 {
            nodes[index].rule = "leaf".to_owned();
            continue;
        }
        let candidates: Vec<usize> = match rng.as_deref_mut() {
            Some(generator) => {
                let mut pool: Vec<usize> = (0..predictors).collect();
                for position in (1..pool.len()).rev() {
                    let swap = (generator.next_u64() % (position as u64 + 1)) as usize;
                    pool.swap(position, swap);
                }
                pool.truncate(mtry.min(pool.len()));
                pool
            }
            None => (0..predictors).collect(),
        };
        match best_split(data, &indices, &candidates, options.min_leaf) {
            Some((predictor, threshold, gain)) => {
                importance[predictor] += gain;
                let left_indices: Vec<usize> = indices
                    .iter()
                    .copied()
                    .filter(|row| data.x[*row][predictor] <= threshold)
                    .collect();
                let right_indices: Vec<usize> = indices
                    .iter()
                    .copied()
                    .filter(|row| data.x[*row][predictor] > threshold)
                    .collect();
                let left_id = nodes.len();
                let right_id = left_id + 1;
                nodes[index].rule = format!("x{} <= {:.6}", predictor + 1, threshold);
                nodes[index].split_predictor = Some(predictor);
                nodes[index].split_threshold = Some(threshold);
                nodes[index].left = Some(left_id);
                nodes[index].right = Some(right_id);
                let child_depth = depth + 1;
                for (id, parent, rule) in [
                    (
                        left_id,
                        index,
                        format!("x{} <= {:.6}", predictor + 1, threshold),
                    ),
                    (
                        right_id,
                        index,
                        format!("x{} > {:.6}", predictor + 1, threshold),
                    ),
                ] {
                    nodes.push(TreeNode {
                        id,
                        depth: child_depth,
                        parent: Some(parent),
                        rule,
                        n: 0,
                        prediction: f64::NAN,
                        impurity: 0.0,
                        left: None,
                        right: None,
                        split_predictor: None,
                        split_threshold: None,
                    });
                }
                stack.push((left_id, left_indices));
                stack.push((right_id, right_indices));
            }
            None => nodes[index].rule = "leaf".to_owned(),
        }
    }
    (nodes, importance)
}

/// Descends the tree to the leaf of one observation.
fn predict(nodes: &[TreeNode], x: &[f64]) -> f64 {
    let mut index = 0usize;
    loop {
        let Some(left) = nodes[index].left else {
            return nodes[index].prediction;
        };
        let Some(right) = nodes[index].right else {
            return nodes[index].prediction;
        };
        let predictor = nodes[index].split_predictor.unwrap_or(0);
        let threshold = nodes[index].split_threshold.unwrap_or(f64::NAN);
        index = if x[predictor] <= threshold {
            left
        } else {
            right
        };
    }
}

fn confusion_matrix(actual: &[usize], predicted: &[usize], classes: usize) -> Vec<Vec<usize>> {
    let mut matrix = vec![vec![0usize; classes]; classes];
    for (truth, found) in actual.iter().zip(predicted) {
        matrix[*truth][*found] += 1;
    }
    matrix
}

/// Cross-validated fit statistics of a CART tree.
fn cross_validate(data: &Training, options: &TreeOptions) -> (f64, f64, f64) {
    let folds = options.folds;
    let mut rng = SplitMix64::new(options.seed ^ 0xA5A5_5A5A_1234_5678);
    let mut order: Vec<usize> = (0..data.n).collect();
    for position in (1..data.n).rev() {
        let swap = (rng.next_u64() % (position as u64 + 1)) as usize;
        order.swap(position, swap);
    }
    let fold_size = data.n.div_ceil(folds);
    let predictors = data.x.first().map_or(0, Vec::len);
    let mut predictions: Vec<(usize, f64)> = Vec::new();
    for fold in 0..folds {
        let start = fold * fold_size;
        let end = ((fold + 1) * fold_size).min(data.n);
        if start >= end {
            continue;
        }
        let test = &order[start..end];
        let mut in_test = vec![false; data.n];
        for &row in test {
            in_test[row] = true;
        }
        let train_rows: Vec<usize> = (0..data.n).filter(|row| !in_test[*row]).collect();
        if train_rows.len() < 2 {
            continue;
        }
        let train = Training {
            x: train_rows.iter().map(|row| data.x[*row].clone()).collect(),
            y: train_rows.iter().map(|row| data.y[*row]).collect(),
            class: train_rows.iter().map(|row| data.class[*row]).collect(),
            classes: data.classes.clone(),
            classification: data.classification,
            n: train_rows.len(),
        };
        let (nodes, _) = grow(&train, options, None, predictors);
        for &row in test {
            predictions.push((row, predict(&nodes, &data.x[row])));
        }
    }
    if predictions.is_empty() {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
    if data.classification {
        let correct = predictions
            .iter()
            .filter(|(row, found)| found.round() as usize == data.class[*row])
            .count();
        (
            f64::NAN,
            f64::NAN,
            correct as f64 / predictions.len() as f64,
        )
    } else {
        let mean =
            predictions.iter().map(|(row, _)| data.y[*row]).sum::<f64>() / predictions.len() as f64;
        let sse: f64 = predictions
            .iter()
            .map(|(row, found)| (data.y[*row] - found).powi(2))
            .sum();
        let sst: f64 = predictions
            .iter()
            .map(|(row, _)| (data.y[*row] - mean).powi(2))
            .sum();
        let r_squared = if sst > 0.0 { 1.0 - sse / sst } else { f64::NAN };
        (r_squared, (sse / predictions.len() as f64).sqrt(), f64::NAN)
    }
}

/// Fits the CART tree or the random forest.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for an empty or repeated predictor list, a
/// non-numeric predictor, fewer than two classes, more than twenty classes and
/// the same response and predictor; [`Error::SampleTooSmall`] when the design
/// has fewer observations than twice the minimum leaf; [`Error::ConstantSample`]
/// for a constant numeric response and [`Error::InvalidParameter`] for bad
/// options.
pub fn fit(
    df: &DataFrame,
    response: &str,
    predictors: &[String],
    mode: TreeMode,
    options: TreeOptions,
) -> Result<TreeFit> {
    if options.max_depth == 0 {
        return Err(Error::InvalidParameter {
            name: "maximum depth",
            value: 0.0,
        });
    }
    if options.min_leaf == 0 {
        return Err(Error::InvalidParameter {
            name: "minimum leaf",
            value: 0.0,
        });
    }
    if options.trees == 0 || options.trees > 1000 {
        return Err(Error::InvalidParameter {
            name: "number of trees",
            value: options.trees as f64,
        });
    }
    if options.folds == 1 || options.folds > 10 {
        return Err(Error::InvalidParameter {
            name: "cross-validation folds",
            value: options.folds as f64,
        });
    }
    if mode == TreeMode::Forest && options.folds >= 2 {
        return Err(Error::Unsupported(
            "cross-validation folds apply to CART; the random forest reports the out-of-bag \
             error"
                .to_owned(),
        ));
    }
    let (data, excluded) = prepare(df, response, predictors)?;
    if data.n < 2 * options.min_leaf {
        return Err(Error::SampleTooSmall {
            n: data.n,
            min: 2 * options.min_leaf,
        });
    }
    if !data.classification {
        let first = data.y[0];
        if data.y.iter().all(|value| (*value - first).abs() <= 1e-12) {
            return Err(Error::ConstantSample);
        }
    }
    let p = data.x.first().map_or(0, Vec::len);
    let mtry = if mode == TreeMode::Forest {
        (p as f64).sqrt().ceil() as usize
    } else {
        p
    }
    .clamp(1, p);
    let mut rng = SplitMix64::new(options.seed);
    let mut fit = TreeFit {
        mode,
        classification: data.classification,
        classes: data.classes.clone(),
        nodes: Vec::new(),
        predictors: predictors.to_vec(),
        importance: Vec::new(),
        r_squared: f64::NAN,
        rmse: f64::NAN,
        accuracy: f64::NAN,
        confusion: Vec::new(),
        oob_error: None,
        trees: 1,
        leaves: 0,
        depth: 0,
        n: data.n,
        excluded,
        cv_r_squared: f64::NAN,
        cv_rmse: f64::NAN,
        cv_accuracy: f64::NAN,
    };

    if mode == TreeMode::Cart {
        let (nodes, importance) = grow(&data, &options, None, p);
        fit.leaves = nodes.iter().filter(|node| node.left.is_none()).count();
        fit.depth = nodes.iter().map(|node| node.depth).max().unwrap_or(0);
        if data.classification {
            let predicted: Vec<usize> = (0..data.n)
                .map(|row| predict(&nodes, &data.x[row]).round() as usize)
                .collect();
            let correct = predicted
                .iter()
                .zip(&data.class)
                .filter(|(found, truth)| **found == **truth)
                .count();
            fit.accuracy = correct as f64 / data.n as f64;
            fit.confusion = confusion_matrix(&data.class, &predicted, data.classes.len());
        } else {
            let predicted: Vec<f64> = (0..data.n)
                .map(|row| predict(&nodes, &data.x[row]))
                .collect();
            let mse = data
                .y
                .iter()
                .zip(&predicted)
                .map(|(value, found)| (value - found).powi(2))
                .sum::<f64>()
                / data.n as f64;
            let grand = data.y.iter().sum::<f64>() / data.n as f64;
            let variance = data
                .y
                .iter()
                .map(|value| (value - grand).powi(2))
                .sum::<f64>()
                / data.n as f64;
            fit.r_squared = if variance > 0.0 {
                1.0 - mse / variance
            } else {
                f64::NAN
            };
            fit.rmse = mse.sqrt();
        }
        fit.nodes = nodes;
        fit.importance = normalize(predictors, &importance);
        if options.folds >= 2 {
            let (cv_r_squared, cv_rmse, cv_accuracy) = cross_validate(&data, &options);
            fit.cv_r_squared = cv_r_squared;
            fit.cv_rmse = cv_rmse;
            fit.cv_accuracy = cv_accuracy;
        }
        return Ok(fit);
    }

    // Random forest.
    let mut importance = vec![0.0; p];
    let mut leaves_total = 0usize;
    let mut depth_total = 0usize;
    let mut sum = vec![0.0; data.n];
    let mut count = vec![0usize; data.n];
    let mut votes = vec![vec![0usize; data.classes.len()]; data.n];
    for _ in 0..options.trees {
        let mut sample = Vec::with_capacity(data.n);
        let mut in_bag = vec![false; data.n];
        for _ in 0..data.n {
            let drawn = (rng.next_u64() % data.n as u64) as usize;
            sample.push(drawn);
            in_bag[drawn] = true;
        }
        let sample_data = Training {
            x: sample.iter().map(|row| data.x[*row].clone()).collect(),
            y: sample.iter().map(|row| data.y[*row]).collect(),
            class: sample.iter().map(|row| data.class[*row]).collect(),
            classes: data.classes.clone(),
            classification: data.classification,
            n: sample.len(),
        };
        let (nodes, found) = grow(&sample_data, &options, Some(&mut rng), mtry);
        for (index, gain) in found.iter().enumerate() {
            importance[index] += gain;
        }
        leaves_total += nodes.iter().filter(|node| node.left.is_none()).count();
        depth_total += nodes.iter().map(|node| node.depth).max().unwrap_or(0);
        for row in 0..data.n {
            if in_bag[row] {
                continue;
            }
            let found = predict(&nodes, &data.x[row]);
            if data.classification {
                let class = found.round() as usize;
                if class < votes[row].len() {
                    votes[row][class] += 1;
                }
            }
            sum[row] += found;
            count[row] += 1;
        }
    }
    fit.trees = options.trees;
    fit.leaves = leaves_total / options.trees.max(1);
    fit.depth = depth_total / options.trees.max(1);
    if data.classification {
        let mut matrix = vec![vec![0usize; data.classes.len()]; data.classes.len()];
        let mut correct = 0usize;
        let mut out = 0usize;
        for row in 0..data.n {
            if count[row] == 0 {
                continue;
            }
            let mut best = 0usize;
            for (class, found) in votes[row].iter().enumerate() {
                if *found > votes[row][best] {
                    best = class;
                }
            }
            matrix[data.class[row]][best] += 1;
            out += 1;
            if best == data.class[row] {
                correct += 1;
            }
        }
        fit.accuracy = if out > 0 {
            correct as f64 / out as f64
        } else {
            f64::NAN
        };
        fit.oob_error = Some(1.0 - fit.accuracy);
        fit.confusion = matrix;
    } else {
        let mut squared = 0.0;
        let mut out = 0usize;
        for row in 0..data.n {
            if count[row] == 0 {
                continue;
            }
            let predicted = sum[row] / count[row] as f64;
            squared += (data.y[row] - predicted).powi(2);
            out += 1;
        }
        fit.oob_error = Some(if out > 0 {
            (squared / out as f64).sqrt()
        } else {
            f64::NAN
        });
    }
    fit.importance = normalize(predictors, &importance);
    Ok(fit)
}

/// Importance in percentages, largest first.
fn normalize(predictors: &[String], importance: &[f64]) -> Vec<(String, f64)> {
    let total: f64 = importance.iter().sum();
    let mut found: Vec<(String, f64)> = predictors
        .iter()
        .cloned()
        .zip(importance.iter())
        .map(|(name, value)| {
            (
                name,
                if total > 0.0 {
                    100.0 * value / total
                } else {
                    0.0
                },
            )
        })
        .collect();
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    found
}

/// Builds the report and the importance chart.
///
/// # Errors
///
/// The errors of [`fit`].
pub fn analyze(
    df: &DataFrame,
    response: &str,
    predictors: &[String],
    mode: TreeMode,
    options: TreeOptions,
) -> Result<AnalysisOutput> {
    let fit = fit(df, response, predictors, mode, options)?;
    let mut result = AnalysisResult::new(
        format!("{} — response {response}", mode.name()),
        if fit.classification {
            "Binary splits that reduce the Gini index; leaves are the majority class".to_owned()
        } else {
            "Binary splits that reduce the sum of squares; leaves are the mean".to_owned()
        },
    );
    result.n = fit.n;
    result.excluded = fit.excluded;
    result.extra.push(Item::Message(format!(
        "Task: {}",
        if fit.classification {
            "classification"
        } else {
            "regression"
        }
    )));
    result.extra.push(Item::Message(format!(
        "Predictors: {}",
        fit.predictors.join(", ")
    )));
    let mut statistics = vec![
        ("Trees", fit.trees as f64),
        ("Leaves", fit.leaves as f64),
        ("Depth", fit.depth as f64),
    ];
    if fit.classification {
        statistics.push(("Accuracy", fit.accuracy));
        if let Some(error) = fit.oob_error {
            statistics.push(("Out-of-bag error", error));
        }
        if fit.cv_accuracy.is_finite() {
            statistics.push(("CV accuracy", fit.cv_accuracy));
        }
    } else {
        statistics.push(("R²", fit.r_squared));
        statistics.push(("RMSE", fit.rmse));
        if fit.cv_r_squared.is_finite() {
            statistics.push(("CV R²", fit.cv_r_squared));
            statistics.push(("CV RMSE", fit.cv_rmse));
        }
        if let Some(error) = fit.oob_error {
            statistics.push(("Out-of-bag RMSE", error));
        }
    }
    result.statistics = statistics;
    result.extra.push(Item::Table {
        headers: vec!["Predictor".to_owned(), "Importance (%)".to_owned()],
        rows: fit
            .importance
            .iter()
            .map(|(name, value)| vec![name.clone(), format!("{value:.2}")])
            .collect(),
    });
    if !fit.nodes.is_empty() {
        result.extra.push(Item::Table {
            headers: vec![
                "Node".to_owned(),
                "Depth".to_owned(),
                "Rule".to_owned(),
                "n".to_owned(),
                "Prediction".to_owned(),
                "Impurity".to_owned(),
            ],
            rows: fit
                .nodes
                .iter()
                .take(60)
                .map(|node| {
                    let prediction = if fit.classification {
                        fit.classes
                            .get(node.prediction.round() as usize)
                            .cloned()
                            .unwrap_or_else(|| format!("{:.4}", node.prediction))
                    } else {
                        format!("{:.6}", node.prediction)
                    };
                    vec![
                        node.id.to_string(),
                        node.depth.to_string(),
                        node.rule.clone(),
                        node.n.to_string(),
                        prediction,
                        format!("{:.6}", node.impurity),
                    ]
                })
                .collect(),
        });
    }
    if fit.classification && !fit.confusion.is_empty() {
        result.extra.push(Item::Table {
            headers: {
                let mut headers = vec!["Actual \\ Predicted".to_owned()];
                headers.extend(fit.classes.iter().cloned());
                headers
            },
            rows: fit
                .confusion
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let mut cells = vec![
                        fit.classes
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| index.to_string()),
                    ];
                    cells.extend(row.iter().map(ToString::to_string));
                    cells
                })
                .collect(),
        });
    }
    result.notes = vec![
        (
            "Splits".to_owned(),
            "every split puts an observation left when the predictor is at most the threshold and \
             right otherwise; the threshold is the midpoint between adjacent distinct values"
                .to_owned(),
        ),
        (
            "Pruning".to_owned(),
            "pre-pruning by the maximum depth, the minimum leaf and a minimum gain; there is no \
             cost-complexity pruning"
                .to_owned(),
        ),
        (
            "Validation".to_owned(),
            if options.folds >= 2 {
                format!(
                    "the tree is validated by {}-fold cross-validation; the forest reports the \
                     out-of-bag error",
                    options.folds
                )
            } else {
                "the tree reports the in-sample fit; the forest reports the out-of-bag error"
                    .to_owned()
            },
        ),
        (
            "Forest".to_owned(),
            "each tree uses a bootstrap sample and a random subset of the predictors per split; \
             the out-of-bag error uses only the observations left out of the sample"
                .to_owned(),
        ),
        (
            "Importance".to_owned(),
            "share of the total impurity decrease attributed to each predictor, in per cent"
                .to_owned(),
        ),
        (
            "Scope".to_owned(),
            "numeric predictors only, no categorical predictors, no missing-value splits and no \
             prediction of new cases"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(fit.n, fit.excluded, 0);
    if fit.n < 20 {
        result
            .warnings
            .push("very small sample: the tree and the out-of-bag error are unstable".to_owned());
    }
    if fit.mode == TreeMode::Cart && fit.leaves > fit.n / 4 {
        result
            .warnings
            .push("many leaves for the sample size: the tree may be overfitting".to_owned());
    }
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(importance_plot(&fit)),
    })
}

fn importance_plot(fit: &TreeFit) -> PlotSpec {
    let top = fit
        .importance
        .iter()
        .map(|(_, value)| *value)
        .fold(1.0, f64::max);
    let bands = fit
        .importance
        .iter()
        .enumerate()
        .map(|(index, (_, value))| {
            let centre = index as f64 + 1.0;
            Band {
                lower: vec![
                    Point {
                        x: centre - 0.35,
                        y: 0.0,
                    },
                    Point {
                        x: centre + 0.35,
                        y: 0.0,
                    },
                ],
                upper: vec![
                    Point {
                        x: centre - 0.35,
                        y: *value,
                    },
                    Point {
                        x: centre + 0.35,
                        y: *value,
                    },
                ],
            }
        })
        .collect();
    let shorten = |text: &str| -> String {
        let characters: Vec<char> = text.chars().collect();
        if characters.len() <= 12 {
            text.to_owned()
        } else {
            let mut short: String = characters.into_iter().take(11).collect();
            short.push('…');
            short
        }
    };
    PlotSpec {
        title: format!("{} — Variable Importance", fit.mode.name()),
        x_axis: linear_axis("Predictor", 0.4, fit.importance.len() as f64 + 0.6),
        y_axis: linear_axis("Importance (%)", 0.0, top * 1.15),
        size: (900, 480),
        points: Vec::new(),
        lines: Vec::new(),
        bands,
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: vec![PlotNote::new(
            fit.importance
                .iter()
                .take(12)
                .map(|(name, value)| (shorten(name), format!("{value:.2}")))
                .collect(),
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(columns: Vec<Column>) -> DataFrame {
        DataFrame::new(columns).expect("frame")
    }

    /// A step response: y = 10 below 0.5 and 20 above, with a noise predictor.
    fn step_frame() -> DataFrame {
        let mut x1 = Vec::new();
        let mut x2 = Vec::new();
        let mut y = Vec::new();
        for index in 0..40 {
            let value = index as f64 / 39.0;
            x1.push(value);
            x2.push(((index * 37) % 17) as f64);
            y.push(if value < 0.5 { 10.0 } else { 20.0 });
        }
        frame(vec![
            Column::new_f64("x1", x1),
            Column::new_f64("x2", x2),
            Column::new_f64("y", y),
        ])
    }

    #[test]
    fn the_cart_finds_the_step() {
        let df = step_frame();
        let fit = fit(
            &df,
            "y",
            &["x1".to_owned(), "x2".to_owned()],
            TreeMode::Cart,
            TreeOptions {
                min_leaf: 5,
                max_depth: 4,
                ..TreeOptions::default()
            },
        )
        .expect("fit");
        assert!(!fit.classification);
        assert!(fit.r_squared > 0.99, "{}", fit.r_squared);
        assert!((fit.rmse).abs() < 1e-9, "{}", fit.rmse);
        assert_eq!(fit.leaves, 2, "{:?}", fit.nodes);
        assert_eq!(fit.nodes[0].split_predictor, Some(0));
        assert!((fit.nodes[0].split_threshold.unwrap() - 0.5).abs() < 0.02);
        assert!(fit.importance[0].0 == "x1");
        assert!(fit.importance[0].1 > 99.0, "{}", fit.importance[0].1);
        let report = analyze(
            &df,
            "y",
            &["x1".to_owned(), "x2".to_owned()],
            TreeMode::Cart,
            TreeOptions::default(),
        )
        .expect("report");
        let text = report.results[0].to_session_text();
        assert!(text.contains("CART — response y"), "{text}");
        assert!(text.contains("Importance"), "{text}");
        assert!(report.plot.is_some());
    }

    #[test]
    fn the_classification_tree_separates_the_classes() {
        let mut x = Vec::new();
        let mut group = Vec::new();
        for index in 0..20 {
            x.push(index as f64);
            group.push(if index < 10 { "low" } else { "high" }.to_owned());
        }
        let df = frame(vec![
            Column::new_f64("x", x),
            Column::new_text("group", group),
        ]);
        let fit = fit(
            &df,
            "group",
            &["x".to_owned()],
            TreeMode::Cart,
            TreeOptions {
                min_leaf: 2,
                max_depth: 4,
                ..TreeOptions::default()
            },
        )
        .expect("fit");
        assert!(fit.classification);
        assert_eq!(fit.classes, vec!["low", "high"]);
        assert!((fit.accuracy - 1.0).abs() < 1e-12);
        assert_eq!(fit.confusion.len(), 2);
        assert_eq!(fit.confusion[0][0], 10);
        assert_eq!(fit.confusion[1][1], 10);
        assert!(fit.confusion[0][1] == 0 && fit.confusion[1][0] == 0);
    }

    #[test]
    fn the_forest_reports_the_out_of_bag_error_and_the_important_predictor() {
        // y follows x1 and x2 is noise.
        let mut x1 = Vec::new();
        let mut x2 = Vec::new();
        let mut y = Vec::new();
        for index in 0..60 {
            let value = index as f64 / 10.0;
            x1.push(value);
            x2.push(((index * 37) % 17) as f64);
            y.push(3.0 * value + 2.0 + 0.05 * (index % 5) as f64);
        }
        let df = frame(vec![
            Column::new_f64("x1", x1),
            Column::new_f64("x2", x2),
            Column::new_f64("y", y),
        ]);
        let forest = fit(
            &df,
            "y",
            &["x1".to_owned(), "x2".to_owned()],
            TreeMode::Forest,
            TreeOptions {
                trees: 40,
                min_leaf: 3,
                seed: 7,
                ..TreeOptions::default()
            },
        )
        .expect("forest");
        assert_eq!(forest.trees, 40);
        assert!(forest.oob_error.unwrap().is_finite());
        assert!(forest.oob_error.unwrap() < 1.0, "{:?}", forest.oob_error);
        assert_eq!(forest.importance[0].0, "x1", "{:?}", forest.importance);
        assert!(forest.importance[0].1 > 60.0, "{:?}", forest.importance);
        // The run is reproducible with the same seed.
        let again = fit(
            &df,
            "y",
            &["x1".to_owned(), "x2".to_owned()],
            TreeMode::Forest,
            TreeOptions {
                trees: 40,
                min_leaf: 3,
                seed: 7,
                ..TreeOptions::default()
            },
        )
        .expect("forest");
        assert_eq!(forest.importance, again.importance);
    }

    #[test]
    fn the_cart_cross_validation_reports_the_held_out_fit() {
        let df = step_frame();
        let forest = fit(
            &df,
            "y",
            &["x1".to_owned(), "x2".to_owned()],
            TreeMode::Cart,
            TreeOptions {
                min_leaf: 5,
                max_depth: 4,
                folds: 3,
                ..TreeOptions::default()
            },
        )
        .expect("cross-validated tree");
        assert!(forest.cv_r_squared > 0.99, "{}", forest.cv_r_squared);
        assert!(forest.cv_rmse < 1e-9, "{}", forest.cv_rmse);
        assert!(forest.cv_accuracy.is_nan());
        // The forest has the out-of-bag error instead.
        assert!(
            fit(
                &df,
                "y",
                &["x1".to_owned()],
                TreeMode::Forest,
                TreeOptions {
                    folds: 3,
                    ..TreeOptions::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn the_trees_reject_invalid_requests() {
        let df = step_frame();
        assert!(
            fit(&df, "y", &[], TreeMode::Cart, TreeOptions::default())
                .unwrap_err()
                .to_string()
                .contains("predictor")
        );
        assert!(
            fit(
                &df,
                "y",
                &["x1".to_owned(), "x1".to_owned()],
                TreeMode::Cart,
                TreeOptions::default()
            )
            .is_err()
        );
        assert!(
            fit(
                &df,
                "x1",
                &["x1".to_owned()],
                TreeMode::Cart,
                TreeOptions::default()
            )
            .is_err()
        );
        // A constant response has no split.
        let flat = frame(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_f64("y", vec![2.0; 6]),
        ]);
        assert!(matches!(
            fit(
                &flat,
                "y",
                &["x".to_owned()],
                TreeMode::Cart,
                TreeOptions {
                    min_leaf: 1,
                    ..TreeOptions::default()
                }
            ),
            Err(Error::ConstantSample)
        ));
        // A single class cannot be classified.
        let one = frame(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_text(
                "g",
                ["A", "A", "A", "A", "A", "A"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ]);
        assert!(
            fit(
                &one,
                "g",
                &["x".to_owned()],
                TreeMode::Cart,
                TreeOptions::default()
            )
            .is_err()
        );
        // Bad options and too few observations.
        assert!(
            fit(
                &df,
                "y",
                &["x1".to_owned()],
                TreeMode::Cart,
                TreeOptions {
                    max_depth: 0,
                    ..TreeOptions::default()
                }
            )
            .is_err()
        );
        assert!(
            fit(
                &df,
                "y",
                &["x1".to_owned()],
                TreeMode::Cart,
                TreeOptions {
                    min_leaf: 500,
                    ..TreeOptions::default()
                }
            )
            .is_err()
        );
        assert!(
            fit(
                &df,
                "y",
                &["x1".to_owned()],
                TreeMode::Cart,
                TreeOptions {
                    folds: 1,
                    ..TreeOptions::default()
                }
            )
            .is_err()
        );
    }
}
