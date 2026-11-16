// SPDX-License-Identifier: MIT
//! Data calculations and resampling.
//!
//! The module works on the worksheet itself: **standardise** columns
//! (`z = (x − x̄)/s`), summarise **columns** and **rows**, **transpose** or
//! **multiply** numeric blocks, build **patterned** sequences, full-factorial
//! **meshes** and **indicator** (dummy) columns, draw **random data** from the
//! Normal, Uniform and Exponential distributions, take non-parametric
//! **bootstrap** resamples and run a two-sample **randomization test** of the
//! difference in means.
//!
//! Everything that uses randomness takes a seed and the in-tree `SplitMix64`
//! generator, so a run is reproducible byte for byte.

use crate::data::{Column, ColumnData, DataFrame, MissingPolicy};
use crate::error::{Error, Result};
use crate::plot::{Band, PlotNote, PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item};
use crate::special::SplitMix64;
use crate::workflow::AnalysisOutput;

/// Statistic computed across the columns of each row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowStatistic {
    /// Sum of the valid values.
    #[default]
    Sum,
    /// Mean of the valid values.
    Mean,
    /// Smallest valid value.
    Minimum,
    /// Largest valid value.
    Maximum,
    /// Largest minus smallest.
    Range,
    /// Sample standard deviation (`n − 1`) of the valid values.
    StandardDeviation,
}

impl RowStatistic {
    /// Name used in reports.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Sum => "Sum",
            Self::Mean => "Mean",
            Self::Minimum => "Minimum",
            Self::Maximum => "Maximum",
            Self::Range => "Range",
            Self::StandardDeviation => "Standard deviation",
        }
    }

    /// Parses the interface spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().replace(' ', "-").as_str() {
            "sum" => Some(Self::Sum),
            "mean" => Some(Self::Mean),
            "minimum" | "min" => Some(Self::Minimum),
            "maximum" | "max" => Some(Self::Maximum),
            "range" => Some(Self::Range),
            "standard-deviation" | "stdev" | "sd" => Some(Self::StandardDeviation),
            _ => None,
        }
    }
}

/// Distribution of generated random data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RandomDistribution {
    /// Normal with a mean and a standard deviation.
    #[default]
    Normal,
    /// Uniform between a minimum and a maximum.
    Uniform,
    /// Exponential with a rate (`1/mean`).
    Exponential,
}

impl RandomDistribution {
    /// Name used in reports.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Uniform => "Uniform",
            Self::Exponential => "Exponential",
        }
    }

    /// Parses the interface spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "uniform" => Some(Self::Uniform),
            "exponential" => Some(Self::Exponential),
            _ => None,
        }
    }
}

/// One factor of a full-factorial mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshFactor {
    /// Column name.
    pub name: String,
    /// First level.
    pub from: f64,
    /// Last level.
    pub to: f64,
    /// Number of equally spaced levels.
    pub levels: usize,
}

/// Summary of one numeric column.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnSummary {
    /// Column name.
    pub name: String,
    /// Valid observations.
    pub n: usize,
    /// Missing or non-finite observations.
    pub missing: usize,
    /// Sum of the valid observations.
    pub sum: f64,
    /// Mean of the valid observations.
    pub mean: f64,
    /// Standard error of the mean.
    pub se: f64,
    /// Sample standard deviation (`n − 1`).
    pub sd: f64,
    /// Smallest valid value.
    pub min: f64,
    /// First quartile.
    pub q1: f64,
    /// Median.
    pub median: f64,
    /// Third quartile.
    pub q3: f64,
    /// Largest valid value.
    pub max: f64,
}

/// Result of the two-sample randomization test.
#[derive(Debug, Clone, PartialEq)]
pub struct RandomizationResult {
    /// Observed difference in means (`first − second`).
    pub observed: f64,
    /// Monte Carlo p-value of the two-sided test.
    pub p_value: f64,
    /// Permutations drawn.
    pub reps: usize,
    /// Permutations at least as extreme as the observed one.
    pub extreme: usize,
    /// Observations of the first sample.
    pub first_n: usize,
    /// Observations of the second sample.
    pub second_n: usize,
    /// Mean of the null distribution.
    pub null_mean: f64,
    /// Standard deviation of the null distribution.
    pub null_sd: f64,
    /// Histogram of the null distribution: bin centre and count.
    pub histogram: Vec<(f64, usize)>,
}

fn check_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Unsupported("the column name is empty".to_owned()));
    }
    Ok(())
}

fn numeric(df: &DataFrame, name: &str) -> Result<Vec<f64>> {
    df.column(name)
        .and_then(|column| column.numeric(MissingPolicy::Drop))
}

/// Appends the standardised copy `<name>_z` of every selected column.
///
/// # Errors
///
/// [`Error::Unsupported`] for an empty selection, [`Error::SampleTooSmall`]
/// for a column with fewer than two valid values and [`Error::ConstantSample`]
/// for a column with zero variance.
pub fn standardize(df: &DataFrame, columns: &[String]) -> Result<DataFrame> {
    if columns.is_empty() {
        return Err(Error::Unsupported(
            "select at least one column to standardise".to_owned(),
        ));
    }
    let mut out = df.clone();
    for name in columns {
        let column = df.column(name)?;
        let sample = column.sample(MissingPolicy::Drop)?;
        if sample.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 2,
            });
        }
        let mean = sample.mean()?;
        let sd = sample.sd()?;
        if sd <= 0.0 {
            return Err(Error::ConstantSample);
        }
        let values: Vec<f64> = (0..column.len())
            .map(|row| match column.value(row) {
                Some(value) if value.is_finite() => (value - mean) / sd,
                _ => f64::NAN,
            })
            .collect();
        let index = out.n_columns();
        out.insert_column(index, Column::new_f64(format!("{name}_z"), values))?;
    }
    Ok(out)
}

/// Summary of one numeric column.
///
/// # Errors
///
/// The errors of [`DataFrame::column`], [`Error::SampleTooSmall`] for an empty
/// column and [`Error::Csv`] for a non-numeric column.
pub fn column_summary(df: &DataFrame, name: &str) -> Result<ColumnSummary> {
    let column = df.column(name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    if sample.n() == 0 {
        return Err(Error::SampleTooSmall { n: 0, min: 1 });
    }
    if matches!(
        column.data(),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
    ) {
        return Err(Error::Csv(format!("column {name} is not numeric")));
    }
    let n = sample.n();
    let sd = if n >= 2 { sample.sd()? } else { f64::NAN };
    Ok(ColumnSummary {
        name: name.to_owned(),
        n,
        missing: column.len() - n,
        sum: sample.sum(),
        mean: sample.mean()?,
        se: if n >= 2 {
            sd / (n as f64).sqrt()
        } else {
            f64::NAN
        },
        sd,
        min: sample.min()?,
        q1: sample.quantile(0.25)?,
        median: sample.median()?,
        q3: sample.quantile(0.75)?,
        max: sample.max()?,
    })
}

/// Session report of the column statistics.
///
/// # Errors
///
/// The errors of [`column_summary`] and [`Error::Unsupported`] for an empty
/// selection.
pub fn column_statistics_report(df: &DataFrame, columns: &[String]) -> Result<AnalysisOutput> {
    if columns.is_empty() {
        return Err(Error::Unsupported("select at least one column".to_owned()));
    }
    let mut result = AnalysisResult::new(
        "Column Statistics".to_owned(),
        "N, N*, sum, mean, standard error, standard deviation, quartiles and extremes \
         of the valid observations"
            .to_owned(),
    );
    let mut rows = Vec::with_capacity(columns.len());
    let mut total_n = 0usize;
    for name in columns {
        let summary = column_summary(df, name)?;
        total_n += summary.n;
        rows.push(vec![
            summary.name,
            summary.n.to_string(),
            summary.missing.to_string(),
            format!("{:.6}", summary.sum),
            format!("{:.6}", summary.mean),
            format!("{:.6}", summary.se),
            format!("{:.6}", summary.sd),
            format!("{:.6}", summary.min),
            format!("{:.6}", summary.q1),
            format!("{:.6}", summary.median),
            format!("{:.6}", summary.q3),
            format!("{:.6}", summary.max),
        ]);
    }
    result.n = total_n;
    result.extra.push(Item::Table {
        headers: [
            "Column", "N", "N*", "Sum", "Mean", "SE Mean", "StDev", "Minimum", "Q1", "Median",
            "Q3", "Maximum",
        ]
        .iter()
        .map(|header| (*header).to_owned())
        .collect(),
        rows,
    });
    result.notes = vec![
        (
            "Missing".to_owned(),
            "N* counts the cells left out: empty cells and non-finite values".to_owned(),
        ),
        (
            "Quartiles".to_owned(),
            "linear interpolation between the ordered observations".to_owned(),
        ),
    ];
    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}

/// One statistic across the columns of every row.
///
/// A row without a valid value, or with a single valid value for the standard
/// deviation, gives `NaN` (stored as a missing cell).
///
/// # Errors
///
/// [`Error::Unsupported`] for an empty selection and the errors of
/// [`DataFrame::column`].
pub fn row_statistics(
    df: &DataFrame,
    columns: &[String],
    statistic: RowStatistic,
) -> Result<Vec<f64>> {
    if columns.is_empty() {
        return Err(Error::Unsupported("select at least one column".to_owned()));
    }
    let values: Vec<&Column> = columns
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    let mut out = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        let valid: Vec<f64> = values
            .iter()
            .filter_map(|column| column.value(row))
            .filter(|value| value.is_finite())
            .collect();
        if valid.is_empty() {
            out.push(f64::NAN);
            continue;
        }
        let found = match statistic {
            RowStatistic::Sum => valid.iter().sum(),
            RowStatistic::Mean => valid.iter().sum::<f64>() / valid.len() as f64,
            RowStatistic::Minimum => valid.iter().copied().fold(f64::INFINITY, f64::min),
            RowStatistic::Maximum => valid.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            RowStatistic::Range => {
                valid.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                    - valid.iter().copied().fold(f64::INFINITY, f64::min)
            }
            RowStatistic::StandardDeviation => {
                if valid.len() < 2 {
                    f64::NAN
                } else {
                    let mean = valid.iter().sum::<f64>() / valid.len() as f64;
                    let variance = valid
                        .iter()
                        .map(|value| (value - mean).powi(2))
                        .sum::<f64>()
                        / (valid.len() - 1) as f64;
                    variance.sqrt()
                }
            }
        };
        out.push(found);
    }
    Ok(out)
}

/// Appends the row statistic as a new column.
///
/// # Errors
///
/// The errors of [`row_statistics`] and [`DataFrame::insert_column`].
pub fn add_row_statistics(
    df: &mut DataFrame,
    name: &str,
    columns: &[String],
    statistic: RowStatistic,
) -> Result<()> {
    check_name(name)?;
    let values = row_statistics(df, columns, statistic)?;
    let index = df.n_columns();
    df.insert_column(index, Column::new_f64(name, values))
}

/// A sequence of `length` values from `start` by `step`, repeated.
///
/// # Errors
///
/// [`Error::InvalidParameter`] for a non-finite start or step, a zero length
/// or repeat count, and [`Error::Unsupported`] when the result would have more
/// than 200000 rows.
pub fn patterned_data(
    name: &str,
    start: f64,
    step: f64,
    length: usize,
    repeats: usize,
) -> Result<DataFrame> {
    check_name(name)?;
    if !start.is_finite() || !step.is_finite() {
        return Err(Error::InvalidParameter {
            name: "start or step",
            value: if start.is_finite() { step } else { start },
        });
    }
    if length == 0 || repeats == 0 {
        return Err(Error::InvalidParameter {
            name: "length or repetitions",
            value: if length == 0 { length } else { repeats } as f64,
        });
    }
    let rows = length
        .checked_mul(repeats)
        .filter(|rows| *rows <= 200_000)
        .ok_or_else(|| {
            Error::Unsupported("the sequence would have more than 200000 rows".to_owned())
        })?;
    let mut values = Vec::with_capacity(rows);
    for _ in 0..repeats {
        for index in 0..length {
            values.push(start + index as f64 * step);
        }
    }
    DataFrame::new(vec![Column::new_f64(name, values)])
}

/// Full-factorial mesh over one to three numeric factors.
///
/// # Errors
///
/// [`Error::Unsupported`] for zero or more than three factors, a repeated or
/// empty name or more than 100000 rows; [`Error::InvalidParameter`] for a bad
/// range or a level count outside `2..=100`.
pub fn mesh_data(factors: &[MeshFactor]) -> Result<DataFrame> {
    if factors.is_empty() || factors.len() > 3 {
        return Err(Error::Unsupported(
            "the mesh needs one to three factors".to_owned(),
        ));
    }
    let mut seen: Vec<&str> = Vec::new();
    let mut rows = 1usize;
    for factor in factors {
        check_name(&factor.name)?;
        if seen.contains(&factor.name.as_str()) {
            return Err(Error::Unsupported(format!(
                "factor {} is repeated",
                factor.name
            )));
        }
        seen.push(&factor.name);
        if !factor.from.is_finite() || !factor.to.is_finite() || factor.to < factor.from {
            return Err(Error::InvalidParameter {
                name: "factor range",
                value: factor.to,
            });
        }
        if !(2..=100).contains(&factor.levels) {
            return Err(Error::InvalidParameter {
                name: "levels",
                value: factor.levels as f64,
            });
        }
        rows = rows
            .checked_mul(factor.levels)
            .filter(|rows| *rows <= 100_000)
            .ok_or_else(|| {
                Error::Unsupported("the mesh would have more than 100000 rows".to_owned())
            })?;
    }
    let mut columns = Vec::with_capacity(factors.len());
    for (position, factor) in factors.iter().enumerate() {
        let stride = factors[position + 1..]
            .iter()
            .map(|next| next.levels)
            .product::<usize>()
            .max(1);
        let values: Vec<f64> = (0..rows)
            .map(|row| {
                let level = (row / stride) % factor.levels;
                factor.from + (factor.to - factor.from) * level as f64 / (factor.levels - 1) as f64
            })
            .collect();
        columns.push(Column::new_f64(factor.name.clone(), values));
    }
    DataFrame::new(columns)
}

/// Appends one 0/1 column per level of a categorical column.
///
/// The levels are ordered by first appearance; `drop_first` omits the first
/// level, so the remaining columns are independent of the intercept.
///
/// # Errors
///
/// [`Error::EmptySample`] when the column has no label,
/// [`Error::Unsupported`] for a single level and [`DataFrame::insert_column`]
/// for a repeated column name.
pub fn indicator_variables(df: &DataFrame, column: &str, drop_first: bool) -> Result<DataFrame> {
    let source = df.column(column)?;
    let mut levels: Vec<String> = Vec::new();
    for row in 0..source.len() {
        if let Some(label) = source.label(row) {
            if !levels.contains(&label) {
                levels.push(label);
            }
        }
    }
    if levels.is_empty() {
        return Err(Error::EmptySample);
    }
    if levels.len() < 2 {
        return Err(Error::Unsupported(
            "the factor needs at least two levels".to_owned(),
        ));
    }
    let start = usize::from(drop_first);
    let mut out = df.clone();
    for level in levels.iter().skip(start) {
        let values: Vec<f64> = (0..source.len())
            .map(|row| match source.label(row) {
                Some(label) if label == *level => 1.0,
                _ => 0.0,
            })
            .collect();
        let index = out.n_columns();
        out.insert_column(index, Column::new_f64(format!("{column}_{level}"), values))?;
    }
    Ok(out)
}

/// `n` random values from the chosen distribution.
///
/// `first` and `second` are the mean and the standard deviation (Normal), the
/// minimum and the maximum (Uniform); the Exponential uses only `first` as the
/// rate (its mean is `1/rate`).
///
/// # Errors
///
/// [`Error::InvalidParameter`] for a non-finite parameter, a non-positive
/// standard deviation or rate, a maximum not above the minimum, and
/// [`Error::Unsupported`] for zero or more than 100000 values.
pub fn random_data(
    name: &str,
    distribution: RandomDistribution,
    n: usize,
    first: f64,
    second: f64,
    seed: u64,
) -> Result<DataFrame> {
    check_name(name)?;
    if n == 0 || n > 100_000 {
        return Err(Error::Unsupported(
            "the number of values must be between 1 and 100000".to_owned(),
        ));
    }
    if !first.is_finite() || !second.is_finite() {
        return Err(Error::InvalidParameter {
            name: "distribution parameter",
            value: if first.is_finite() { second } else { first },
        });
    }
    match distribution {
        RandomDistribution::Normal => {
            if second <= 0.0 {
                return Err(Error::InvalidParameter {
                    name: "standard deviation",
                    value: second,
                });
            }
        }
        RandomDistribution::Uniform => {
            if second <= first {
                return Err(Error::InvalidParameter {
                    name: "maximum",
                    value: second,
                });
            }
        }
        RandomDistribution::Exponential => {
            if first <= 0.0 {
                return Err(Error::InvalidParameter {
                    name: "rate",
                    value: first,
                });
            }
        }
    }
    let mut rng = SplitMix64::new(seed);
    let values: Vec<f64> = (0..n)
        .map(|_| match distribution {
            RandomDistribution::Normal => first + second * rng.next_normal(),
            RandomDistribution::Uniform => first + (second - first) * rng.next_f64(),
            RandomDistribution::Exponential => {
                let uniform = rng.next_f64().max(f64::MIN_POSITIVE);
                -(1.0 - uniform).ln() / first
            }
        })
        .collect();
    DataFrame::new(vec![Column::new_f64(name, values)])
}

/// Non-parametric bootstrap of the selected columns.
///
/// The result is a long table with a `Sample` column (1 to `reps`) and every
/// source column, each sample taking `n` rows drawn with replacement.
///
/// # Errors
///
/// [`Error::Unsupported`] for an empty selection, zero or more than 1000
/// replicates, a result above 200000 rows, [`Error::SampleTooSmall`] below two
/// rows and the errors of [`DataFrame::column`].
pub fn bootstrap(df: &DataFrame, columns: &[String], reps: usize, seed: u64) -> Result<DataFrame> {
    if columns.is_empty() {
        return Err(Error::Unsupported("select at least one column".to_owned()));
    }
    if reps == 0 || reps > 1000 {
        return Err(Error::Unsupported(
            "the number of resamples must be between 1 and 1000".to_owned(),
        ));
    }
    let n = df.n_rows();
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    let rows = n
        .checked_mul(reps)
        .filter(|rows| *rows <= 200_000)
        .ok_or_else(|| {
            Error::Unsupported("the resample table would have more than 200000 rows".to_owned())
        })?;
    let sources: Vec<&Column> = columns
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    let mut rng = SplitMix64::new(seed);
    let draws: Vec<usize> = (0..rows)
        .map(|_| (rng.next_u64() % n as u64) as usize)
        .collect();
    let samples: Vec<i64> = (0..reps)
        .flat_map(|sample| (0..n).map(move |_| sample as i64 + 1))
        .collect();
    let mut out = Vec::with_capacity(columns.len() + 1);
    out.push(Column::from_parts(
        "Sample",
        ColumnData::I64(samples),
        vec![false; rows],
    )?);
    for (name, column) in columns.iter().zip(&sources) {
        if matches!(
            column.data(),
            ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
        ) {
            return Err(Error::Csv(format!(
                "column {name} is not numeric and cannot be bootstrapped"
            )));
        }
        let values: Vec<f64> = draws
            .iter()
            .map(|row| column.value(*row).unwrap_or(f64::NAN))
            .collect();
        let missing: Vec<bool> = draws.iter().map(|row| column.is_missing(*row)).collect();
        out.push(Column::from_parts(
            name.clone(),
            ColumnData::F64(values),
            missing,
        )?);
    }
    DataFrame::new(out)
}

/// Two-sample randomization test of the difference in means.
///
/// The labels are permuted keeping both group sizes; the two-sided p-value is
/// `(1 + #{|T*| ≥ |T|})/(1 + reps)`.
///
/// # Errors
///
/// [`Error::SampleTooSmall`] below two observations in either sample,
/// [`Error::Unsupported`] for fewer than 100 permutations and the errors of
/// [`DataFrame::column`].
pub fn randomization_test(
    df: &DataFrame,
    first: &str,
    second: &str,
    reps: usize,
    seed: u64,
) -> Result<RandomizationResult> {
    if first == second {
        return Err(Error::Unsupported(
            "select two different columns".to_owned(),
        ));
    }
    if !(100..=100_000).contains(&reps) {
        return Err(Error::Unsupported(
            "the number of permutations must be between 100 and 100000".to_owned(),
        ));
    }
    let a = numeric(df, first)?;
    let b = numeric(df, second)?;
    if a.len() < 2 {
        return Err(Error::SampleTooSmall { n: a.len(), min: 2 });
    }
    if b.len() < 2 {
        return Err(Error::SampleTooSmall { n: b.len(), min: 2 });
    }
    let mean_a = a.iter().sum::<f64>() / a.len() as f64;
    let mean_b = b.iter().sum::<f64>() / b.len() as f64;
    let observed = mean_a - mean_b;
    let n1 = a.len();
    let total = n1 + b.len();
    let mut pool: Vec<f64> = a.into_iter().chain(b).collect();
    let mut rng = SplitMix64::new(seed);
    let mut extreme = 0usize;
    let mut null = Vec::with_capacity(reps);
    for _ in 0..reps {
        for position in (1..total).rev() {
            let swap = (rng.next_u64() % (position as u64 + 1)) as usize;
            pool.swap(position, swap);
        }
        let left: f64 = pool[..n1].iter().sum::<f64>() / n1 as f64;
        let right: f64 = pool[n1..].iter().sum::<f64>() / (total - n1) as f64;
        let difference = left - right;
        null.push(difference);
        if difference.abs() >= observed.abs() - 1e-12 {
            extreme += 1;
        }
    }
    let null_mean = null.iter().sum::<f64>() / null.len() as f64;
    let null_sd = if null.len() >= 2 {
        (null
            .iter()
            .map(|value| (value - null_mean).powi(2))
            .sum::<f64>()
            / (null.len() - 1) as f64)
            .sqrt()
    } else {
        f64::NAN
    };
    let histogram = build_histogram(&null, 21);
    Ok(RandomizationResult {
        observed,
        p_value: (1 + extreme) as f64 / (1 + reps) as f64,
        reps,
        extreme,
        first_n: n1,
        second_n: total - n1,
        null_mean,
        null_sd,
        histogram,
    })
}

fn build_histogram(values: &[f64], bins: usize) -> Vec<(f64, usize)> {
    if values.is_empty() || bins == 0 {
        return Vec::new();
    }
    let mut lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let mut hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !(hi > lo) {
        lo -= 1.0;
        hi += 1.0;
    }
    let width = (hi - lo) / bins as f64;
    let mut counts = vec![0usize; bins];
    for value in values {
        let index = (((value - lo) / width).floor() as isize).clamp(0, bins as isize - 1);
        counts[index as usize] += 1;
    }
    counts
        .iter()
        .enumerate()
        .map(|(index, count)| (lo + (index as f64 + 0.5) * width, *count))
        .collect()
}

/// Session report and null-distribution chart of the randomization test.
///
/// # Errors
///
/// The errors of [`randomization_test`].
pub fn randomization_analysis(
    df: &DataFrame,
    first: &str,
    second: &str,
    reps: usize,
    seed: u64,
) -> Result<AnalysisOutput> {
    let result = randomization_test(df, first, second, reps, seed)?;
    let mut report = AnalysisResult::new(
        format!("Randomization Test — {first} vs {second}"),
        "Two-sided permutation test of the difference in means (first − second)".to_owned(),
    );
    report.n = result.first_n + result.second_n;
    report.statistics = vec![
        ("Observed difference", result.observed),
        ("P-value", result.p_value),
        ("Permutations", result.reps as f64),
        ("Extreme permutations", result.extreme as f64),
        ("Null mean", result.null_mean),
        ("Null standard deviation", result.null_sd),
        ("Sample 1", result.first_n as f64),
        ("Sample 2", result.second_n as f64),
    ];
    report.extra.push(Item::Message(format!(
        "{} permutation(s) reached a difference at least as extreme as the observed one; \
         the p-value is (extreme + 1)/(permutations + 1).",
        result.extreme
    )));
    report.notes = vec![
        (
            "Method".to_owned(),
            "the group labels are permuted with both sample sizes fixed; there is no \
             distributional assumption"
                .to_owned(),
        ),
        (
            "Seed".to_owned(),
            format!("{seed} (SplitMix64): the same seed gives the same p-value"),
        ),
        (
            "Conventions".to_owned(),
            "the interface defaults are documented conventions; the p-value follows the \
             formula stated above"
                .to_owned(),
        ),
    ];
    if result.reps < 1000 {
        report
            .warnings
            .push("few permutations: the p-value has a coarse resolution".to_owned());
    }
    if result.extreme == 0 {
        report.warnings.push(
            "no permutation reached the observed difference: the reported p-value is a bound"
                .to_owned(),
        );
    }
    Ok(AnalysisOutput {
        results: vec![report],
        plot: Some(randomization_plot(&result, first, second)),
    })
}

fn randomization_plot(result: &RandomizationResult, first: &str, second: &str) -> PlotSpec {
    let bin_width = result
        .histogram
        .windows(2)
        .next()
        .map_or(0.1, |pair| (pair[1].0 - pair[0].0).abs());
    let bands = result
        .histogram
        .iter()
        .map(|(centre, count)| Band {
            lower: vec![
                Point {
                    x: centre - bin_width * 0.45,
                    y: 0.0,
                },
                Point {
                    x: centre + bin_width * 0.45,
                    y: 0.0,
                },
            ],
            upper: vec![
                Point {
                    x: centre - bin_width * 0.45,
                    y: *count as f64,
                },
                Point {
                    x: centre + bin_width * 0.45,
                    y: *count as f64,
                },
            ],
        })
        .collect();
    let top = result
        .histogram
        .iter()
        .map(|(_, count)| *count as f64)
        .fold(1.0, f64::max);
    let lo = result
        .histogram
        .first()
        .map_or(-1.0, |(centre, _)| *centre - bin_width);
    let hi = result
        .histogram
        .last()
        .map_or(1.0, |(centre, _)| *centre + bin_width);
    PlotSpec {
        title: format!("Randomization Test — {first} vs {second}"),
        x_axis: linear_axis("Difference in means", lo, hi),
        y_axis: linear_axis("Frequency", 0.0, top * 1.1),
        size: (900, 480),
        points: Vec::new(),
        lines: vec![Segment {
            from: Point {
                x: result.observed,
                y: 0.0,
            },
            to: Point {
                x: result.observed,
                y: top,
            },
            width: 2.0,
        }],
        bands,
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: vec![PlotNote::new(vec![
            ("Observed".to_owned(), format!("{:.4}", result.observed)),
            ("P-value".to_owned(), format!("{:.4}", result.p_value)),
        ])],
    }
}

/// Transposes a numeric block: every selected column becomes a row.
///
/// The output has one column per source row (`Row 1`, `Row 2`, …) and one row
/// per selected source column.
///
/// # Errors
///
/// [`Error::Unsupported`] for an empty selection and the errors of
/// [`DataFrame::column`].
pub fn transpose(df: &DataFrame, columns: &[String]) -> Result<DataFrame> {
    if columns.is_empty() {
        return Err(Error::Unsupported("select at least one column".to_owned()));
    }
    let sources: Vec<&Column> = columns
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    let rows = df.n_rows();
    let mut out = Vec::with_capacity(rows);
    for row in 0..rows {
        let values: Vec<f64> = sources
            .iter()
            .map(|column| column.value(row).unwrap_or(f64::NAN))
            .collect();
        out.push(Column::new_f64(format!("Row {}", row + 1), values));
    }
    DataFrame::new(out)
}

/// Product of two numeric blocks, `Aᵀ·B`: `A` has `n` rows and `k` selected
/// columns, `B` has the same `n` rows and `m` selected columns, and the result
/// has `k` rows and `m` columns (`M1`, `M2`, …).
///
/// # Errors
///
/// [`Error::Unsupported`] for an empty block or a different number of rows and
/// the errors of [`DataFrame::column`].
pub fn matrix_product(
    a: &DataFrame,
    a_columns: &[String],
    b: &DataFrame,
    b_columns: &[String],
) -> Result<DataFrame> {
    if a_columns.is_empty() || b_columns.is_empty() {
        return Err(Error::Unsupported(
            "select the columns of both blocks".to_owned(),
        ));
    }
    if a.n_rows() != b.n_rows() {
        return Err(Error::Unsupported(format!(
            "the blocks have {} and {} rows",
            a.n_rows(),
            b.n_rows()
        )));
    }
    let left: Vec<Vec<f64>> = a_columns
        .iter()
        .map(|name| numeric(a, name))
        .collect::<Result<Vec<_>>>()?;
    let right: Vec<Vec<f64>> = b_columns
        .iter()
        .map(|name| numeric(b, name))
        .collect::<Result<Vec<_>>>()?;
    let n = a.n_rows();
    let mut columns = Vec::with_capacity(b_columns.len());
    for (j, source) in right.iter().enumerate() {
        let values: Vec<f64> = left
            .iter()
            .map(|row| {
                (0..n)
                    .map(|observation| row[observation] * source[observation])
                    .sum()
            })
            .collect();
        columns.push(Column::new_f64(format!("M{}", j + 1), values));
    }
    DataFrame::new(columns)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("y", vec![2.0, 4.0, 6.0, 8.0, 10.0]),
        ])
        .expect("frame")
    }

    #[test]
    fn standardize_centres_and_scales_every_column() {
        let out = standardize(&frame(), &["x".to_owned()]).expect("standardise");
        assert_eq!(out.n_columns(), 3);
        let z = out.column("x_z").expect("x_z");
        let values = z.numeric(MissingPolicy::Drop).expect("values");
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        assert!(mean.abs() < 1e-12, "mean {mean}");
        let variance = values.iter().map(|v| v * v).sum::<f64>() / (values.len() - 1) as f64;
        assert!((variance - 1.0).abs() < 1e-12, "variance {variance}");
        assert!((values[0] + 1.264_911).abs() < 1e-4, "{:?}", values);

        // A missing cell stays missing.
        let with_missing =
            DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, f64::NAN, 4.0])])
                .expect("frame");
        let out = standardize(&with_missing, &["x".to_owned()]).expect("standardise");
        assert!(out.column("x_z").expect("x_z").is_missing(2));

        assert!(standardize(&frame(), &[]).is_err());
        let constant =
            DataFrame::new(vec![Column::new_f64("x", vec![2.0, 2.0, 2.0])]).expect("frame");
        assert!(matches!(
            standardize(&constant, &["x".to_owned()]),
            Err(Error::ConstantSample)
        ));
    }

    #[test]
    fn the_column_summary_reports_the_usual_statistics() {
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_text("g", vec!["a".to_owned(); 5]),
        ])
        .expect("frame");
        let summary = column_summary(&df, "x").expect("summary");
        assert_eq!(summary.n, 5);
        assert_eq!(summary.missing, 0);
        assert!((summary.sum - 15.0).abs() < 1e-12);
        assert!((summary.mean - 3.0).abs() < 1e-12);
        assert!((summary.sd - 1.581_138_830_084_189_8).abs() < 1e-12);
        assert!((summary.se - summary.sd / 5.0f64.sqrt()).abs() < 1e-12);
        assert!((summary.q1 - 1.5).abs() < 1e-12);
        assert!((summary.median - 3.0).abs() < 1e-12);
        assert!((summary.q3 - 4.5).abs() < 1e-12);

        let report = column_statistics_report(&df, &["x".to_owned()]).expect("report");
        let text = report.results[0].to_session_text();
        assert!(text.contains("Column Statistics"), "{text}");
        assert!(text.contains("SE Mean"), "{text}");
        assert_eq!(report.results[0].n, 5);

        assert!(column_summary(&df, "g").is_err());
        assert!(column_statistics_report(&df, &[]).is_err());
    }

    #[test]
    fn the_row_statistics_skip_missing_cells() {
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 4.0, f64::NAN]),
            Column::new_f64("b", vec![3.0, f64::NAN, f64::NAN]),
        ])
        .expect("frame");
        let columns = ["a".to_owned(), "b".to_owned()];
        let sum = row_statistics(&df, &columns, RowStatistic::Sum).expect("sum");
        assert!((sum[0] - 4.0).abs() < 1e-12);
        assert!((sum[1] - 4.0).abs() < 1e-12);
        assert!(sum[2].is_nan());

        let mean = row_statistics(&df, &columns, RowStatistic::Mean).expect("mean");
        assert!((mean[0] - 2.0).abs() < 1e-12);
        let minimum = row_statistics(&df, &columns, RowStatistic::Minimum).expect("min");
        assert!((minimum[0] - 1.0).abs() < 1e-12);
        let maximum = row_statistics(&df, &columns, RowStatistic::Maximum).expect("max");
        assert!((maximum[0] - 3.0).abs() < 1e-12);
        let range = row_statistics(&df, &columns, RowStatistic::Range).expect("range");
        assert!((range[0] - 2.0).abs() < 1e-12);
        let sd = row_statistics(&df, &columns, RowStatistic::StandardDeviation).expect("sd");
        assert!((sd[0] - 2.0f64.sqrt()).abs() < 1e-12);
        assert!(sd[1].is_nan());

        let mut with_row = df.clone();
        add_row_statistics(&mut with_row, "total", &columns, RowStatistic::Sum).expect("column");
        assert_eq!(with_row.n_columns(), 3);
        assert!((with_row.column("total").expect("total").value(0).unwrap() - 4.0).abs() < 1e-12);
        assert!(row_statistics(&df, &[], RowStatistic::Sum).is_err());
    }

    #[test]
    fn the_patterned_sequence_repeats_the_step() {
        let df = patterned_data("t", 10.0, 2.0, 3, 2).expect("pattern");
        let values = df
            .column("t")
            .expect("t")
            .numeric(MissingPolicy::Drop)
            .expect("values");
        assert_eq!(values, vec![10.0, 12.0, 14.0, 10.0, 12.0, 14.0]);
        assert!(patterned_data("", 0.0, 1.0, 3, 1).is_err());
        assert!(patterned_data("t", 0.0, 1.0, 0, 1).is_err());
        assert!(patterned_data("t", 0.0, 1.0, 3, 0).is_err());
        assert!(patterned_data("t", 0.0, f64::NAN, 3, 1).is_err());
        assert!(patterned_data("t", 0.0, 1.0, 1000, 1000).is_err());
    }

    #[test]
    fn the_mesh_is_the_full_factorial_grid() {
        let df = mesh_data(&[
            MeshFactor {
                name: "a".to_owned(),
                from: 0.0,
                to: 10.0,
                levels: 3,
            },
            MeshFactor {
                name: "b".to_owned(),
                from: 1.0,
                to: 2.0,
                levels: 2,
            },
        ])
        .expect("mesh");
        assert_eq!(df.n_rows(), 6);
        let a = df
            .column("a")
            .expect("a")
            .numeric(MissingPolicy::Drop)
            .expect("a");
        let b = df
            .column("b")
            .expect("b")
            .numeric(MissingPolicy::Drop)
            .expect("b");
        assert_eq!(a, vec![0.0, 0.0, 5.0, 5.0, 10.0, 10.0]);
        assert_eq!(b, vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0]);
        assert!(mesh_data(&[]).is_err());
        assert!(
            mesh_data(&[
                MeshFactor {
                    name: "a".to_owned(),
                    from: 0.0,
                    to: 1.0,
                    levels: 2,
                },
                MeshFactor {
                    name: "a".to_owned(),
                    from: 0.0,
                    to: 1.0,
                    levels: 2,
                },
            ])
            .is_err()
        );
        assert!(
            mesh_data(&[MeshFactor {
                name: "a".to_owned(),
                from: 0.0,
                to: 1.0,
                levels: 1,
            }])
            .is_err()
        );
        assert!(
            mesh_data(&[MeshFactor {
                name: "a".to_owned(),
                from: 2.0,
                to: 1.0,
                levels: 3,
            }])
            .is_err()
        );
        assert!(
            mesh_data(&[MeshFactor {
                name: "a".to_owned(),
                from: 0.0,
                to: 1.0,
                levels: 100,
            }])
            .is_ok()
        );
    }

    #[test]
    fn the_indicator_columns_code_the_levels() {
        let df = DataFrame::new(vec![Column::new_text(
            "g",
            vec![
                "b".to_owned(),
                "a".to_owned(),
                "b".to_owned(),
                "c".to_owned(),
            ],
        )])
        .expect("frame");
        let out = indicator_variables(&df, "g", true).expect("indicators");
        assert_eq!(out.n_columns(), 3);
        let a = out.column("g_a").expect("g_a");
        assert_eq!(
            a.numeric(MissingPolicy::Drop).expect("a"),
            vec![0.0, 1.0, 0.0, 0.0]
        );
        let c = out.column("g_c").expect("g_c");
        assert_eq!(
            c.numeric(MissingPolicy::Drop).expect("c"),
            vec![0.0, 0.0, 0.0, 1.0]
        );
        assert!(out.column("g_b").is_err());

        let full = indicator_variables(&df, "g", false).expect("full");
        assert_eq!(full.n_columns(), 4);

        let one = DataFrame::new(vec![Column::new_text(
            "g",
            vec!["a".to_owned(), "a".to_owned()],
        )])
        .expect("frame");
        assert!(indicator_variables(&one, "g", true).is_err());
    }

    #[test]
    fn the_random_data_is_reproducible_and_stays_in_range() {
        let first = random_data("r", RandomDistribution::Normal, 20, 5.0, 2.0, 42).expect("normal");
        let again = random_data("r", RandomDistribution::Normal, 20, 5.0, 2.0, 42).expect("normal");
        assert_eq!(first, again);
        let values = first
            .column("r")
            .expect("r")
            .numeric(MissingPolicy::Drop)
            .expect("values");
        assert_eq!(values.len(), 20);
        assert!(values.iter().all(|value| value.is_finite()));

        let uniform =
            random_data("u", RandomDistribution::Uniform, 100, 1.0, 3.0, 7).expect("uniform");
        let values = uniform
            .column("u")
            .expect("u")
            .numeric(MissingPolicy::Drop)
            .expect("values");
        assert!(values.iter().all(|value| (1.0..=3.0).contains(value)));

        let exponential =
            random_data("e", RandomDistribution::Exponential, 2000, 2.0, 0.0, 9).expect("exp");
        let values = exponential
            .column("e")
            .expect("e")
            .numeric(MissingPolicy::Drop)
            .expect("values");
        assert!(values.iter().all(|value| *value > 0.0));
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        assert!((mean - 0.5).abs() < 0.1, "mean {mean}");

        assert!(random_data("r", RandomDistribution::Normal, 0, 0.0, 1.0, 1).is_err());
        assert!(random_data("r", RandomDistribution::Normal, 5, 0.0, 0.0, 1).is_err());
        assert!(random_data("r", RandomDistribution::Uniform, 5, 2.0, 1.0, 1).is_err());
        assert!(random_data("r", RandomDistribution::Exponential, 5, 0.0, 0.0, 1).is_err());
    }

    #[test]
    fn the_bootstrap_draws_whole_rows_with_replacement() {
        let df = frame();
        let out = bootstrap(&df, &["x".to_owned()], 3, 2026).expect("bootstrap");
        assert_eq!(out.n_rows(), 15);
        assert_eq!(out.n_columns(), 2);
        let samples = out
            .column("Sample")
            .expect("sample")
            .numeric(MissingPolicy::Drop)
            .expect("samples");
        assert_eq!(
            samples,
            vec![1.0; 5]
                .into_iter()
                .chain(vec![2.0; 5])
                .chain(vec![3.0; 5])
                .collect::<Vec<_>>()
        );
        let again = bootstrap(&df, &["x".to_owned()], 3, 2026).expect("bootstrap");
        assert_eq!(out, again);
        // Every drawn value comes from the source column.
        let drawn = out
            .column("x")
            .expect("x")
            .numeric(MissingPolicy::Drop)
            .expect("drawn");
        assert!(drawn.iter().all(|value| (1.0..=5.0).contains(value)));

        assert!(bootstrap(&df, &[], 3, 1).is_err());
        assert!(bootstrap(&df, &["x".to_owned()], 0, 1).is_err());
        let text =
            DataFrame::new(vec![Column::new_text("g", vec!["a".to_owned(); 5])]).expect("frame");
        assert!(bootstrap(&text, &["g".to_owned()], 2, 1).is_err());
    }

    #[test]
    fn the_randomization_test_detects_a_clear_difference() {
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("b", vec![11.0, 12.0, 13.0, 14.0, 15.0]),
        ])
        .expect("frame");
        let result = randomization_test(&df, "a", "b", 999, 7).expect("test");
        assert!((result.observed + 10.0).abs() < 1e-12);
        assert!(result.p_value < 0.02, "p {}", result.p_value);
        assert_eq!(result.first_n, 5);
        assert_eq!(result.second_n, 5);
        assert_eq!(result.histogram.len(), 21);

        let again = randomization_test(&df, "a", "b", 999, 7).expect("test");
        assert_eq!(result, again);

        // Two identical columns are never more extreme than the observation.
        let same = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_f64("b", vec![1.0, 2.0, 3.0, 4.0]),
        ])
        .expect("frame");
        let result = randomization_test(&same, "a", "b", 199, 3).expect("test");
        assert!((result.p_value - 1.0).abs() < 1e-12, "p {}", result.p_value);

        assert!(randomization_test(&df, "a", "a", 199, 1).is_err());
        assert!(randomization_test(&df, "a", "b", 10, 1).is_err());
    }

    #[test]
    fn the_randomization_report_carries_the_chart() {
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_f64("b", vec![5.0, 6.0, 7.0, 8.0]),
        ])
        .expect("frame");
        let output = randomization_analysis(&df, "a", "b", 299, 11).expect("report");
        let text = output.results[0].to_session_text();
        assert!(text.contains("Randomization Test"), "{text}");
        assert!(text.contains("P-value"), "{text}");
        let plot = output.plot.expect("plot");
        assert!(plot.title.contains("Randomization Test"));
        assert_eq!(plot.bands.len(), 21);
        assert_eq!(plot.lines.len(), 1);
    }

    #[test]
    fn the_transpose_swaps_rows_and_columns() {
        let df = frame();
        let out = transpose(&df, &["x".to_owned(), "y".to_owned()]).expect("transpose");
        assert_eq!(out.n_rows(), 5);
        assert_eq!(out.n_columns(), 2);
        assert_eq!(
            out.column("Row 1")
                .expect("row")
                .numeric(MissingPolicy::Drop)
                .expect("values"),
            vec![1.0, 2.0]
        );
        assert!(transpose(&df, &[]).is_err());
    }

    #[test]
    fn the_matrix_product_is_the_cross_product_of_the_blocks() {
        let df = frame();
        let out = matrix_product(
            &df,
            &["x".to_owned()],
            &df,
            &["x".to_owned(), "y".to_owned()],
        )
        .expect("product");
        assert_eq!(out.n_rows(), 1);
        assert_eq!(out.n_columns(), 2);
        // AᵀB: [1,2,3,4,5]·[1,2,3,4,5] = 55 and [1,2,3,4,5]·[2,4,6,8,10] = 110.
        let first = out
            .column("M1")
            .expect("M1")
            .numeric(MissingPolicy::Drop)
            .expect("values");
        let second = out
            .column("M2")
            .expect("M2")
            .numeric(MissingPolicy::Drop)
            .expect("values");
        assert!((first[0] - 55.0).abs() < 1e-12);
        assert!((second[0] - 110.0).abs() < 1e-12);

        assert!(matrix_product(&df, &[], &df, &["x".to_owned()]).is_err());
        let other = DataFrame::new(vec![Column::new_f64("z", vec![1.0, 2.0])]).expect("frame");
        assert!(matrix_product(&df, &["x".to_owned()], &other, &["z".to_owned()]).is_err());
    }
}
