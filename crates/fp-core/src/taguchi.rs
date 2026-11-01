//! Taguchi orthogonal arrays and signal-to-noise analysis.
//!
//! The arrays are generated from their classical constructions, so no table is
//! copied by hand:
//!
//! - **L8 (2⁷)**: the seven non-empty products of three two-level factors
//!   (`a`, `b`, `ab`, `c`, `ac`, `bc`, `abc`) in 8 runs.
//! - **L16 (2¹⁵)**: the fifteen non-empty products of four two-level factors.
//! - **L9 (3⁴)**: `a`, `b`, `a + b (mod 3)` and `a + 2b (mod 3)` in 9 runs.
//! - **L12 (2¹¹)**: the Plackett–Burman design of 12 runs, which is the
//!   Taguchi L12 array.
//! - **L27 (3¹³)**: the 3³ full factorial and its thirteen non-equivalent
//!   linear combinations over GF(3).
//!
//! A pair of columns is orthogonal when every combination of levels appears the
//! same number of times; the tests check that property for every supported
//! array.
//!
//! The analysis reports, for each factor and level, the **mean response** and
//! the **signal-to-noise ratio**:
//!
//! | characteristic | ratio |
//! |---|---|
//! | smaller is better | `−10·log₁₀(mean(y²))` |
//! | larger is better | `−10·log₁₀(mean(1/y²))` |
//! | nominal is best | `10·log₁₀(mean(y)²/var(y))` |
//!
//! Factors are ranked by the range (delta) of the ratio, the best level of each
//! factor is the one with the largest ratio, and an analysis of variance of the
//! response gives the significance of each factor.
//!
//! The comparison run has Taguchi designs; its array list and pooling rules
//! were not captured, so the numbers are not claimed as reference parity — see
//! `docs/parity.md`.

use crate::data::{Column, DataFrame};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::SplitMix64;
use crate::workflow::AnalysisOutput;

/// Quality characteristic of the signal-to-noise ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// Smaller is better.
    #[default]
    Smaller,
    /// Larger is better.
    Larger,
    /// Nominal is best (the target is the sample mean of the level).
    Nominal,
}

impl Quality {
    /// Parses `smaller`, `larger`, `nominal`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "smaller" | "smaller-is-better" | "min" => Some(Self::Smaller),
            "larger" | "larger-is-better" | "max" => Some(Self::Larger),
            "nominal" | "nominal-is-best" | "target" => Some(Self::Nominal),
            _ => None,
        }
    }

    /// Name used in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Smaller => "smaller is better",
            Self::Larger => "larger is better",
            Self::Nominal => "nominal is best",
        }
    }

    /// Signal-to-noise ratio of a set of responses.
    #[must_use]
    pub fn ratio(self, values: &[f64]) -> f64 {
        if values.is_empty() {
            return f64::NAN;
        }
        let n = values.len() as f64;
        match self {
            Self::Smaller => {
                let mean_square = values.iter().map(|value| value * value).sum::<f64>() / n;
                if mean_square <= 0.0 {
                    f64::INFINITY
                } else {
                    -10.0 * mean_square.log10()
                }
            }
            Self::Larger => {
                if values.contains(&0.0) {
                    return f64::NEG_INFINITY;
                }
                let mean_inverse_square = values
                    .iter()
                    .map(|value| 1.0 / (value * value))
                    .sum::<f64>()
                    / n;
                -10.0 * mean_inverse_square.log10()
            }
            Self::Nominal => {
                let mean = values.iter().sum::<f64>() / n;
                if n < 2.0 {
                    return f64::NAN;
                }
                let variance = values
                    .iter()
                    .map(|value| (value - mean) * (value - mean))
                    .sum::<f64>()
                    / (n - 1.0);
                if mean.abs() <= 0.0 || variance <= 0.0 {
                    return f64::NAN;
                }
                10.0 * (mean * mean / variance).log10()
            }
        }
    }
}

/// One column of a Taguchi design.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrayColumn {
    /// Factor name (`A`, `B`, …).
    pub name: String,
    /// Level of every run (1-based).
    pub levels: Vec<usize>,
}

/// A generated orthogonal array.
#[derive(Debug, Clone, PartialEq)]
pub struct OrthogonalArray {
    /// Runs of the array.
    pub runs: usize,
    /// Highest number of levels of a factor (2 or 3).
    pub levels: usize,
    /// Number of levels of every column (a mixed-level array has more than
    /// one value here).
    pub level_counts: Vec<usize>,
    /// Columns, in the classic order of the array.
    pub columns: Vec<ArrayColumn>,
}

impl OrthogonalArray {
    /// Highest number of factors the array supports.
    #[must_use]
    pub fn factors(&self) -> usize {
        self.columns.len()
    }
}

/// Products of the non-empty subsets of `k` two-level factors.
fn two_level_products(k: usize) -> Vec<Vec<usize>> {
    let runs = 1usize << k;
    let mut columns = Vec::new();
    // Each mask selects a subset of the factors; the level is the parity of the
    // AND of the subset with the run pattern.
    for mask in 1..(1usize << k) {
        let mut levels = Vec::with_capacity(runs);
        for run in 0..runs {
            let mut parity = 0u32;
            for bit in 0..k {
                if mask & (1 << bit) != 0 {
                    parity ^= ((run >> bit) & 1) as u32;
                }
            }
            levels.push(parity as usize + 1);
        }
        columns.push(levels);
    }
    columns
}

/// Builds the orthogonal array of the requested run count.
pub fn array(runs: usize) -> Result<OrthogonalArray> {
    match runs {
        8 => Ok(OrthogonalArray {
            runs,
            levels: 2,
            level_counts: vec![2; 7],
            columns: two_level_products(3)
                .into_iter()
                .enumerate()
                .map(|(index, levels)| ArrayColumn {
                    name: factor_name(index),
                    levels,
                })
                .collect(),
        }),
        16 => Ok(OrthogonalArray {
            runs,
            levels: 2,
            level_counts: vec![2; 15],
            columns: two_level_products(4)
                .into_iter()
                .enumerate()
                .map(|(index, levels)| ArrayColumn {
                    name: factor_name(index),
                    levels,
                })
                .collect(),
        }),
        9 => {
            // a, b, a + b (mod 3) and a + 2b (mod 3).
            let mut columns: Vec<Vec<usize>> = Vec::new();
            let a: Vec<usize> = (0..9).map(|run| run / 3).collect();
            let b: Vec<usize> = (0..9).map(|run| run % 3).collect();
            columns.push(a.clone());
            columns.push(b.clone());
            columns.push(a.iter().zip(&b).map(|(x, y)| (x + y) % 3).collect());
            columns.push(a.iter().zip(&b).map(|(x, y)| (x + 2 * y) % 3).collect());
            Ok(OrthogonalArray {
                runs,
                levels: 3,
                level_counts: vec![3; 4],
                columns: columns
                    .into_iter()
                    .enumerate()
                    .map(|(index, levels)| ArrayColumn {
                        name: factor_name(index),
                        levels: levels.into_iter().map(|level| level + 1).collect(),
                    })
                    .collect(),
            })
        }
        27 => {
            // The 3³ full factorial and its 13 non-equivalent linear
            // combinations over GF(3): the classical L27 array.
            let base = [
                (0..27).map(|run| run / 9).collect::<Vec<usize>>(),
                (0..27).map(|run| (run / 3) % 3).collect::<Vec<usize>>(),
                (0..27).map(|run| run % 3).collect::<Vec<usize>>(),
            ];
            let mut columns: Vec<Vec<usize>> = Vec::new();
            for first in 0..3usize {
                for second in 0..3usize {
                    for third in 0..3usize {
                        let coefficients = [first, second, third];
                        if coefficients.iter().all(|value| *value == 0) {
                            continue;
                        }
                        // Keep one representative of each pair (v, 2v).
                        let leading = coefficients
                            .iter()
                            .copied()
                            .find(|value| *value != 0)
                            .unwrap_or(0);
                        if leading != 1 {
                            continue;
                        }
                        let levels: Vec<usize> = (0..27)
                            .map(|run| {
                                (coefficients[0] * base[0][run]
                                    + coefficients[1] * base[1][run]
                                    + coefficients[2] * base[2][run])
                                    % 3
                            })
                            .collect();
                        columns.push(levels);
                    }
                }
            }
            Ok(OrthogonalArray {
                runs,
                levels: 3,
                level_counts: vec![3; 13],
                columns: columns
                    .into_iter()
                    .enumerate()
                    .map(|(index, levels)| ArrayColumn {
                        name: factor_name(index),
                        levels: levels.into_iter().map(|level| level + 1).collect(),
                    })
                    .collect(),
            })
        }
        18 => {
            // The classical L18 (2¹ × 3⁷) mixed-level array: one two-level
            // column and seven three-level columns, every pair balanced.
            const TABLE: [[usize; 8]; 18] = [
                [1, 1, 1, 1, 1, 1, 1, 1],
                [1, 1, 2, 2, 2, 2, 2, 2],
                [1, 1, 3, 3, 3, 3, 3, 3],
                [1, 2, 1, 1, 2, 2, 3, 3],
                [1, 2, 2, 2, 3, 3, 1, 1],
                [1, 2, 3, 3, 1, 1, 2, 2],
                [1, 3, 1, 2, 1, 3, 2, 3],
                [1, 3, 2, 3, 2, 1, 3, 1],
                [1, 3, 3, 1, 3, 2, 1, 2],
                [2, 1, 1, 3, 3, 2, 2, 1],
                [2, 1, 2, 1, 1, 3, 3, 2],
                [2, 1, 3, 2, 2, 1, 1, 3],
                [2, 2, 1, 2, 3, 1, 3, 2],
                [2, 2, 2, 3, 1, 2, 1, 3],
                [2, 2, 3, 1, 2, 3, 2, 1],
                [2, 3, 1, 3, 2, 3, 1, 2],
                [2, 3, 2, 1, 3, 1, 2, 3],
                [2, 3, 3, 2, 1, 2, 3, 1],
            ];
            Ok(OrthogonalArray {
                runs,
                levels: 3,
                level_counts: vec![2, 3, 3, 3, 3, 3, 3, 3],
                columns: (0..8)
                    .map(|index| ArrayColumn {
                        name: factor_name(index),
                        levels: TABLE.iter().map(|row| row[index]).collect(),
                    })
                    .collect(),
            })
        }
        12 => {
            let plackett = crate::doe::plackett_burman_design(12, false, 2026)?;
            let mut columns = Vec::with_capacity(11);
            for index in 0..11 {
                let name = plackett.columns()[index + 2].name().to_owned();
                let values = plackett
                    .column(&name)?
                    .numeric(crate::data::MissingPolicy::Drop)?;
                columns.push(ArrayColumn {
                    name: factor_name(index),
                    levels: values
                        .iter()
                        .map(|value| if *value > 0.0 { 2 } else { 1 })
                        .collect(),
                });
            }
            Ok(OrthogonalArray {
                runs,
                levels: 2,
                level_counts: vec![2; 11],
                columns,
            })
        }
        _ => Err(Error::Unsupported(format!(
            "the Taguchi array L{runs} is not available; try 8, 9, 12, 16, 18 or 27"
        ))),
    }
}

/// Factor name of a column index (`A`, `B`, …, `Z`, `A1`, …).
fn factor_name(index: usize) -> String {
    let letter = (b'A' + (index % 26) as u8) as char;
    match index / 26 {
        0 => letter.to_string(),
        cycle => format!("{letter}{cycle}"),
    }
}

/// Generates the design as a worksheet with `StdOrder`, `RunOrder` and the
/// factor columns (levels 1..k).
pub fn design(runs: usize, factors: usize, randomize: bool, seed: u64) -> Result<DataFrame> {
    let array = array(runs)?;
    if factors == 0 || factors > array.factors() {
        return Err(Error::Unsupported(format!(
            "L{runs} supports between 1 and {} factors",
            array.factors()
        )));
    }
    let mut order: Vec<usize> = (0..array.runs).collect();
    if randomize {
        let mut rng = SplitMix64::new(seed);
        for index in (1..array.runs).rev() {
            let swap = (rng.next_u64() % (index as u64 + 1)) as usize;
            order.swap(index, swap);
        }
    }
    let std_order: Vec<f64> = order.iter().map(|row| *row as f64 + 1.0).collect();
    let run_order: Vec<f64> = (1..=array.runs).map(|value| value as f64).collect();
    let mut columns = vec![
        Column::new_f64("StdOrder", std_order),
        Column::new_f64("RunOrder", run_order),
    ];
    for column in array.columns.iter().take(factors) {
        columns.push(Column::new_f64(
            column.name.clone(),
            order.iter().map(|row| column.levels[*row] as f64).collect(),
        ));
    }
    DataFrame::new(columns)
}

/// Response table of one factor.
#[derive(Debug, Clone, PartialEq)]
pub struct FactorTable {
    /// Factor name.
    pub name: String,
    /// Mean response at each level.
    pub means: Vec<f64>,
    /// Mean signal-to-noise ratio at each level.
    pub ratios: Vec<f64>,
    /// Range of the ratios (delta).
    pub delta: f64,
    /// Rank, one is the strongest factor.
    pub rank: usize,
    /// Level with the largest ratio.
    pub best: usize,
}

/// Analysis of variance of the response by factor.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    /// Factor name.
    pub name: String,
    /// Sum of squares of the factor.
    pub sum_squares: f64,
    /// Degrees of freedom.
    pub df: usize,
    /// Mean square.
    pub mean_square: f64,
    /// F statistic against the pooled error.
    pub f: f64,
    /// p-value of the F statistic.
    pub p_value: f64,
    /// Percentage of the total sum of squares.
    pub contribution: f64,
}

/// Result of the Taguchi analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct TaguchiFit {
    /// Quality characteristic.
    pub quality: Quality,
    /// Highest number of levels of a factor.
    pub levels: usize,
    /// Number of levels of every factor, in the order requested.
    pub level_counts: Vec<usize>,
    /// Factors, in the order requested.
    pub factors: Vec<FactorTable>,
    /// Response used per row.
    pub response: String,
    /// Observations.
    pub n: usize,
    /// Analysis of variance of the response.
    pub effects: Vec<Effect>,
    /// Grand mean of the response.
    pub grand_mean: f64,
    /// Total sum of squares.
    pub total_ss: f64,
    /// Pooled error sum of squares (the part not explained by the factors).
    pub error_ss: f64,
    /// Degrees of freedom of the pooled error.
    pub error_df: usize,
    /// Mean square of the pooled error.
    pub error_ms: f64,
    /// Replicates per run (1 when the runs are not repeated).
    pub replicates: usize,
    /// Analysis of variance of the run signal-to-noise ratios, when the runs
    /// are repeated at least twice.
    pub ratio_effects: Option<Vec<Effect>>,
    /// Total sum of squares of the ratios.
    pub ratio_total_ss: f64,
    /// Pooled error sum of squares of the ratios.
    pub ratio_error_ss: f64,
    /// Degrees of freedom of the ratios' pooled error.
    pub ratio_error_df: usize,
    /// Mean square of the ratios' pooled error.
    pub ratio_error_ms: f64,
}

/// One-way sums of squares of every factor over a set of run values.
///
/// `level_rows[i]` holds the level of the i-th run for every factor and
/// `values[i]` the value analysed (a response or a ratio). The sum of squares
/// of a factor is `Σ n_l(ȳ_l − ȳ)²`; the rest is the pooled error. This is the
/// classical Taguchi table, used both for the response and for the
/// signal-to-noise ratios of the replicates.
fn factor_anova(
    names: &[String],
    level_rows: &[Vec<usize>],
    values: &[f64],
    level_counts: &[usize],
) -> (Vec<Effect>, f64, f64, f64, usize, f64) {
    let counted = values.len();
    let grand_mean = if counted > 0 {
        values.iter().sum::<f64>() / counted as f64
    } else {
        f64::NAN
    };
    let total: f64 = values
        .iter()
        .map(|value| (value - grand_mean).powi(2))
        .sum();
    let mut effects = Vec::with_capacity(names.len());
    let mut explained = 0.0;
    let mut explained_df = 0usize;
    for (index, name) in names.iter().enumerate() {
        let levels = level_counts.get(index).copied().unwrap_or(1);
        let mut sums = vec![0.0; levels];
        let mut counts = vec![0usize; levels];
        for (run, row) in level_rows.iter().enumerate() {
            if index >= row.len() {
                continue;
            }
            let level = row[index];
            if (1..=levels).contains(&level) {
                sums[level - 1] += values[run];
                counts[level - 1] += 1;
            }
        }
        let mut sum_squares = 0.0;
        for level in 0..levels {
            if counts[level] > 0 {
                let mean = sums[level] / counts[level] as f64;
                sum_squares += counts[level] as f64 * (mean - grand_mean).powi(2);
            }
        }
        let df_factor = levels.saturating_sub(1);
        explained += sum_squares;
        explained_df += df_factor;
        effects.push(Effect {
            name: name.clone(),
            sum_squares,
            df: df_factor,
            mean_square: f64::NAN,
            f: f64::NAN,
            p_value: f64::NAN,
            contribution: 0.0,
        });
    }
    let error_ss = (total - explained).max(0.0);
    let error_df = counted.saturating_sub(1 + explained_df);
    let error_ms = if error_df > 0 {
        error_ss / error_df as f64
    } else {
        f64::NAN
    };
    for effect in &mut effects {
        effect.mean_square = if effect.df > 0 {
            effect.sum_squares / effect.df as f64
        } else {
            f64::NAN
        };
        if error_ms.is_finite() && error_ms > 0.0 {
            effect.f = effect.mean_square / error_ms;
            effect.p_value = (1.0
                - crate::special::f_cdf(effect.f, effect.df as f64, error_df as f64))
            .clamp(0.0, 1.0);
        }
        effect.contribution = if total > 0.0 {
            100.0 * effect.sum_squares / total
        } else {
            0.0
        };
    }
    (effects, grand_mean, total, error_ss, error_df, error_ms)
}

/// Runs the Taguchi analysis.
pub fn fit(
    df: &DataFrame,
    response: &str,
    factors: &[String],
    quality: Quality,
) -> Result<TaguchiFit> {
    if factors.is_empty() {
        return Err(Error::Unsupported(
            "the analysis needs at least one factor".to_owned(),
        ));
    }
    if factors.iter().any(|name| name == response) {
        return Err(Error::Unsupported(
            "the response cannot also be a factor".to_owned(),
        ));
    }
    // Read the values per row: dropping the missing cells first would shift
    // every later row and read past the end of the shortened vector.
    let response_column = df.column(response)?;
    let factor_columns = factors
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<&Column>>>()?;
    let mut tables: Vec<Vec<(f64, f64)>> = vec![Vec::new(); factors.len()];
    let mut level_counts = vec![0usize; factors.len()];
    let mut n = 0usize;
    for row in 0..df.n_rows() {
        let Some(response_value) = response_column.value(row) else {
            continue;
        };
        let mut level_row = Vec::with_capacity(factors.len());
        let mut complete = true;
        for column in &factor_columns {
            let Some(level) = column.value(row) else {
                complete = false;
                break;
            };
            if level < 1.0 || (level - level.round()).abs() > 1e-9 {
                return Err(Error::Unsupported(
                    "factor levels must be positive integers (1, 2, …)".to_owned(),
                ));
            }
            level_row.push(level.round() as usize);
        }
        if !complete {
            continue;
        }
        for (index, level) in level_row.iter().enumerate() {
            level_counts[index] = level_counts[index].max(*level);
            if tables[index].len() < *level {
                tables[index].resize(*level, (0.0, 0.0));
            }
            let entry = &mut tables[index][level - 1];
            entry.0 += response_value;
            entry.1 += 1.0;
        }
        n += 1;
    }
    if n == 0 {
        return Err(Error::EmptySample);
    }
    if level_counts.iter().any(|count| *count < 2) {
        return Err(Error::Unsupported(
            "the factors need at least two levels".to_owned(),
        ));
    }
    let levels = level_counts.iter().copied().max().unwrap_or(1);

    // Signal-to-noise ratio of each level: the ratio is computed over the
    // responses of that level, not from the average of the ratios.
    let mut per_level: Vec<Vec<Vec<f64>>> = level_counts
        .iter()
        .map(|count| vec![Vec::new(); *count])
        .collect();
    for row in 0..df.n_rows() {
        let Some(response_value) = response_column.value(row) else {
            continue;
        };
        let mut levels = Vec::with_capacity(factors.len());
        let mut complete = true;
        for column in &factor_columns {
            let Some(level) = column.value(row) else {
                complete = false;
                break;
            };
            levels.push(level.round() as usize);
        }
        if !complete {
            continue;
        }
        for (index, level) in levels.iter().enumerate() {
            if *level >= 1 && *level <= level_counts[index] {
                per_level[index][level - 1].push(response_value);
            }
        }
    }

    let mut factor_tables = Vec::with_capacity(factors.len());
    for (index, name) in factors.iter().enumerate() {
        let factor_levels = level_counts[index];
        let mut means = Vec::with_capacity(factor_levels);
        let mut ratios = Vec::with_capacity(factor_levels);
        for level in 0..factor_levels {
            let (sum, count) = tables[index][level];
            means.push(if count > 0.0 { sum / count } else { f64::NAN });
            ratios.push(quality.ratio(&per_level[index][level]));
        }
        let finite: Vec<f64> = ratios
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .collect();
        let delta = if finite.is_empty() {
            0.0
        } else {
            finite.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - finite.iter().copied().fold(f64::INFINITY, f64::min)
        };
        let best = ratios
            .iter()
            .enumerate()
            .filter(|(_, value)| value.is_finite())
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(level, _)| level + 1)
            .unwrap_or(1);
        factor_tables.push(FactorTable {
            name: name.clone(),
            means,
            ratios,
            delta,
            rank: 0,
            best,
        });
    }
    // Ranking by the range of the ratio.
    let mut order: Vec<usize> = (0..factor_tables.len()).collect();
    order.sort_by(|left, right| {
        factor_tables[*right]
            .delta
            .total_cmp(&factor_tables[*left].delta)
    });
    for (rank, index) in order.iter().enumerate() {
        factor_tables[*index].rank = rank + 1;
    }

    // Analysis of variance of the response: the sum of squares of a factor is
    // Σ n_l (ȳ_l − ȳ)² over its levels, and everything the factors do not
    // explain is pooled into the error (the classical Taguchi table).
    let mut level_rows: Vec<Vec<usize>> = Vec::with_capacity(n);
    let mut run_values: Vec<f64> = Vec::with_capacity(n);
    for row in 0..df.n_rows() {
        let Some(response_value) = response_column.value(row) else {
            continue;
        };
        let mut levels = Vec::with_capacity(factors.len());
        let mut complete = true;
        for column in &factor_columns {
            let Some(level) = column.value(row) else {
                complete = false;
                break;
            };
            levels.push(level.round() as usize);
        }
        if !complete {
            continue;
        }
        level_rows.push(levels);
        run_values.push(response_value);
    }
    let (effects, grand_mean, total, error_ss, error_df, error_ms) =
        factor_anova(factors, &level_rows, &run_values, &level_counts);

    // Replicates: when every combination of levels repeats the same number of
    // times (at least twice), the signal-to-noise ratio of each run is
    // computed from its replicates and analysed with the same table.
    let mut run_order: Vec<Vec<usize>> = Vec::new();
    let mut run_index: std::collections::BTreeMap<Vec<usize>, usize> =
        std::collections::BTreeMap::new();
    let mut run_responses: Vec<Vec<f64>> = Vec::new();
    for (row, value) in level_rows.iter().zip(&run_values) {
        match run_index.get(row) {
            Some(index) => run_responses[*index].push(*value),
            None => {
                run_index.insert(row.clone(), run_order.len());
                run_order.push(row.clone());
                run_responses.push(vec![*value]);
            }
        }
    }
    let replicates = run_responses.first().map_or(0, Vec::len);
    let uniform = replicates >= 2
        && run_responses
            .iter()
            .all(|values| values.len() == replicates);
    let mut ratio_effects = None;
    let mut ratio_total = 0.0;
    let mut ratio_error_ss = 0.0;
    let mut ratio_error_df = 0usize;
    let mut ratio_error_ms = f64::NAN;
    if uniform {
        let ratios: Vec<f64> = run_responses
            .iter()
            .map(|values| quality.ratio(values))
            .collect();
        let (table, _, total, error_ss, error_df, error_ms) =
            factor_anova(factors, &run_order, &ratios, &level_counts);
        ratio_effects = Some(table);
        ratio_total = total;
        ratio_error_ss = error_ss;
        ratio_error_df = error_df;
        ratio_error_ms = error_ms;
    }

    Ok(TaguchiFit {
        quality,
        levels,
        level_counts,
        factors: factor_tables,
        response: response.to_owned(),
        n,
        effects,
        grand_mean,
        total_ss: total,
        error_ss,
        error_df,
        error_ms,
        replicates,
        ratio_effects,
        ratio_total_ss: ratio_total,
        ratio_error_ss,
        ratio_error_df,
        ratio_error_ms,
    })
}

/// Runs the analysis and builds the report.
pub fn analyze(
    df: &DataFrame,
    response: &str,
    factors: &[String],
    quality: Quality,
) -> Result<AnalysisOutput> {
    let model = fit(df, response, factors, quality)?;
    let mut result = AnalysisResult::new(
        format!("Taguchi Analysis — response {response}"),
        format!(
            "Signal-to-noise ratios, quality characteristic: {}",
            model.quality.name()
        ),
    );
    result.n = model.n;
    result.excluded = df.n_rows() - model.n;
    result.statistics = vec![
        ("Factors", model.factors.len() as f64),
        ("Levels", model.levels as f64),
        (
            "Strongest factor",
            model
                .factors
                .iter()
                .find(|table| table.rank == 1)
                .map_or(f64::NAN, |_| 1.0),
        ),
    ];
    result.extra.push(Item::Table {
        headers: {
            let mut headers = vec!["Factor".to_owned(), "Level".to_owned()];
            headers.push("Mean".to_owned());
            headers.push("S/N ratio".to_owned());
            headers
        },
        rows: model
            .factors
            .iter()
            .zip(&model.level_counts)
            .flat_map(|(table, levels)| {
                (0..*levels).map(move |level| {
                    vec![
                        table.name.clone(),
                        format!("{}", level + 1),
                        format!("{:.6}", table.means[level]),
                        format!("{:.6}", table.ratios[level]),
                    ]
                })
            })
            .collect(),
    });
    if model
        .level_counts
        .iter()
        .any(|count| *count != model.levels)
    {
        result.extra.push(Item::Message(format!(
            "Mixed levels: {}",
            model
                .factors
                .iter()
                .zip(&model.level_counts)
                .map(|(table, count)| format!("{} = {count}", table.name))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    result.extra.push(Item::Table {
        headers: vec![
            "Factor".to_owned(),
            "Delta".to_owned(),
            "Rank".to_owned(),
            "Best level".to_owned(),
        ],
        rows: model
            .factors
            .iter()
            .map(|table| {
                vec![
                    table.name.clone(),
                    format!("{:.6}", table.delta),
                    format!("{}", table.rank),
                    format!("{}", table.best),
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
            "Contribution %".to_owned(),
        ],
        rows: {
            let mut rows: Vec<Vec<String>> = model
                .effects
                .iter()
                .map(|effect| {
                    vec![
                        effect.name.clone(),
                        format!("{}", effect.df),
                        format!("{:.6}", effect.sum_squares),
                        format!("{:.6}", effect.mean_square),
                        format!("{:.6}", effect.f),
                        format!("{:.6}", effect.p_value),
                        format!("{:.2}", effect.contribution),
                    ]
                })
                .collect();
            rows.push(vec![
                "Error (pooled)".to_owned(),
                format!("{}", model.error_df),
                format!("{:.6}", model.error_ss),
                format!("{:.6}", model.error_ms),
                String::new(),
                String::new(),
                format!(
                    "{:.2}",
                    if model.total_ss > 0.0 {
                        100.0 * model.error_ss / model.total_ss
                    } else {
                        0.0
                    }
                ),
            ]);
            rows.push(vec![
                "Total".to_owned(),
                format!("{}", model.n.saturating_sub(1)),
                format!("{:.6}", model.total_ss),
                String::new(),
                String::new(),
                String::new(),
                "100.00".to_owned(),
            ]);
            rows
        },
    });
    if let Some(ratio_effects) = &model.ratio_effects {
        result
            .statistics
            .push(("Replicates per run", model.replicates as f64));
        result.extra.push(Item::Table {
            headers: vec![
                "Source".to_owned(),
                "DF".to_owned(),
                "SS (S/N)".to_owned(),
                "MS".to_owned(),
                "F".to_owned(),
                "P".to_owned(),
                "Contribution %".to_owned(),
            ],
            rows: {
                let mut rows: Vec<Vec<String>> = ratio_effects
                    .iter()
                    .map(|effect| {
                        vec![
                            effect.name.clone(),
                            format!("{}", effect.df),
                            format!("{:.6}", effect.sum_squares),
                            format!("{:.6}", effect.mean_square),
                            format!("{:.6}", effect.f),
                            format!("{:.6}", effect.p_value),
                            format!("{:.2}", effect.contribution),
                        ]
                    })
                    .collect();
                rows.push(vec![
                    "Error (pooled)".to_owned(),
                    format!("{}", model.ratio_error_df),
                    format!("{:.6}", model.ratio_error_ss),
                    format!("{:.6}", model.ratio_error_ms),
                    String::new(),
                    String::new(),
                    format!(
                        "{:.2}",
                        if model.ratio_total_ss > 0.0 {
                            100.0 * model.ratio_error_ss / model.ratio_total_ss
                        } else {
                            0.0
                        }
                    ),
                ]);
                rows.push(vec![
                    "Total".to_owned(),
                    format!(
                        "{}",
                        model.ratio_error_df
                            + ratio_effects.iter().map(|effect| effect.df).sum::<usize>()
                    ),
                    format!("{:.6}", model.ratio_total_ss),
                    String::new(),
                    String::new(),
                    String::new(),
                    "100.00".to_owned(),
                ]);
                rows
            },
        });
    }
    let strongest = model
        .factors
        .iter()
        .find(|table| table.rank == 1)
        .map(|table| (table.name.clone(), table.best));
    if let Some((name, level)) = strongest {
        result.extra.push(Item::Message(format!(
            "The factor with the largest effect is {name}; its best level is {level}."
        )));
    }
    let settings: Vec<String> = model
        .factors
        .iter()
        .map(|table| format!("{}={}", table.name, table.best))
        .collect();
    result.extra.push(Item::Message(format!(
        "Predicted best setting: {}.",
        settings.join(", ")
    )));
    result.notes = vec![
        (
            "Signal-to-noise".to_owned(),
            "smaller is better: −10·log₁₀(mean y²); larger is better: −10·log₁₀(mean 1/y²); \
             nominal is best: 10·log₁₀(mean²/var)"
                .to_owned(),
        ),
        (
            "Ranking".to_owned(),
            "factors are ranked by the range (delta) of their ratios; the best level has the \
             largest ratio"
                .to_owned(),
        ),
        (
            "Replicates".to_owned(),
            if model.ratio_effects.is_some() {
                "the runs repeat the same number of times, so the signal-to-noise ratio of every \
                 run is analysed as well"
            } else {
                "one observation per run: the ratios are computed per factor level and the \
                 variance is not estimable"
            }
            .to_owned(),
        ),
        (
            "Levels".to_owned(),
            "the factor columns hold the level numbers 1, 2, … as written by the design \
             generator"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing response or factor value are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the array construction and the ranking rule are standard choices and have not been \
         verified against captured output"
            .to_owned(),
    );
    if model.quality == Quality::Nominal {
        result.warnings.push(
            "nominal-is-best needs repeated runs per level; with one observation per run the \
             variance is not estimable and the ratio is missing"
                .to_owned(),
        );
    }

    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::MissingPolicy;

    fn is_orthogonal(array: &OrthogonalArray) -> bool {
        for i in 0..array.columns.len() {
            for j in (i + 1)..array.columns.len() {
                let left_levels = array.level_counts[i];
                let right_levels = array.level_counts[j];
                let mut counts = vec![vec![0usize; right_levels]; left_levels];
                for run in 0..array.runs {
                    let left = array.columns[i].levels[run] - 1;
                    let right = array.columns[j].levels[run] - 1;
                    counts[left][right] += 1;
                }
                let expected = array.runs / (left_levels * right_levels);
                for row in &counts {
                    for value in row {
                        if *value != expected {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    #[test]
    fn the_arrays_are_orthogonal() {
        for runs in [8usize, 9, 12, 16, 18, 27] {
            let array = array(runs).expect("array");
            assert_eq!(array.runs, runs);
            assert!(is_orthogonal(&array), "L{runs} is not orthogonal");
            for (column, count) in array.columns.iter().zip(&array.level_counts) {
                let highest = column.levels.iter().copied().max().unwrap_or(0);
                let lowest = column.levels.iter().copied().min().unwrap_or(0);
                assert_eq!(lowest, 1, "L{runs} {} starts at 1", column.name);
                assert_eq!(highest, *count, "L{runs} {} level count", column.name);
            }
        }
        let l8 = array(8).expect("L8");
        assert_eq!(l8.factors(), 7);
        assert_eq!(l8.levels, 2);
        let l9 = array(9).expect("L9");
        assert_eq!(l9.factors(), 4);
        assert_eq!(l9.levels, 3);
        let l12 = array(12).expect("L12");
        assert_eq!(l12.factors(), 11);
        let l18 = array(18).expect("L18");
        assert_eq!(l18.factors(), 8);
        assert_eq!(l18.levels, 3);
        assert_eq!(l18.level_counts, vec![2, 3, 3, 3, 3, 3, 3, 3]);
        let l27 = array(27).expect("L27");
        assert_eq!(l27.factors(), 13);
        assert_eq!(l27.levels, 3);
        assert_eq!(l27.runs, 27);
    }

    #[test]
    fn unavailable_arrays_are_rejected() {
        assert!(matches!(array(7), Err(Error::Unsupported(_))));
        assert!(matches!(array(20), Err(Error::Unsupported(_))));
    }

    #[test]
    fn the_design_writes_the_requested_factors() {
        let df = design(9, 3, false, 2026).expect("design");
        assert_eq!(df.n_rows(), 9);
        assert_eq!(df.n_columns(), 5);
        let a = df
            .column("A")
            .expect("A")
            .numeric(MissingPolicy::Drop)
            .expect("levels");
        assert!(a.iter().all(|level| (1.0..=3.0).contains(level)));
        assert!(matches!(
            design(9, 5, false, 2026),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            design(9, 0, false, 2026),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn the_ratio_formulas_match_the_definition() {
        // Smaller is better: −10·log10((1² + 3²)/2) = −10·log10(5).
        let smaller = Quality::Smaller.ratio(&[1.0, 3.0]);
        assert!((smaller + 10.0 * 5.0f64.log10()).abs() < 1e-12);
        // Larger is better: −10·log10((1/1² + 1/3²)/2).
        let larger = Quality::Larger.ratio(&[1.0, 3.0]);
        let inner: f64 = (1.0 + 1.0 / 9.0) / 2.0;
        assert!((larger + 10.0 * inner.log10()).abs() < 1e-12);
        // Nominal: 10·log10(mean²/var) with var (n−1).
        let nominal = Quality::Nominal.ratio(&[2.0, 4.0]);
        let variance: f64 = 2.0;
        assert!((nominal - 10.0 * (9.0 / variance).log10()).abs() < 1e-12);
        assert!(Quality::parse("larger").is_some());
        assert!(Quality::parse("other").is_none());
    }

    #[test]
    fn the_analysis_ranks_the_dominant_factor() {
        // L8 with a response driven by factor A only.
        let df = design(8, 3, false, 2026).expect("design");
        let levels = df
            .column("A")
            .expect("A")
            .numeric(MissingPolicy::Drop)
            .expect("levels")
            .to_vec();
        let response: Vec<f64> = levels.iter().map(|level| level * 10.0).collect();
        let mut with_response = df.clone();
        with_response.add_column(crate::data::DType::Float);
        with_response
            .rename_column(with_response.n_columns() - 1, "y")
            .expect("rename");
        for (row, value) in response.iter().enumerate() {
            with_response
                .set_cell(with_response.n_columns() - 1, row, &value.to_string())
                .expect("set");
        }
        let model = fit(
            &with_response,
            "y",
            &["A".to_owned(), "B".to_owned()],
            Quality::Smaller,
        )
        .expect("fit");
        assert_eq!(model.factors.len(), 2);
        assert_eq!(model.factors[0].rank, 1, "{:?}", model.factors);
        // The ratio of the two levels of A differ; B is flat.
        assert!(model.factors[0].delta > 0.0);
        assert!(model.factors[1].delta < 1e-9, "{:?}", model.factors[1]);
        let out = analyze(
            &with_response,
            "y",
            &["A".to_owned(), "B".to_owned()],
            Quality::Smaller,
        )
        .expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Taguchi Analysis"), "{text}");
        assert!(text.contains("Best level"), "{text}");
    }

    #[test]
    fn the_l18_mixed_level_array_analyzes_per_factor_levels() {
        // L18 with factor A at two levels and B at three.
        let df = design(18, 2, false, 2026).expect("design");
        let a = df
            .column("A")
            .expect("A")
            .numeric(MissingPolicy::Drop)
            .expect("A levels")
            .to_vec();
        let b = df
            .column("B")
            .expect("B")
            .numeric(MissingPolicy::Drop)
            .expect("B levels")
            .to_vec();
        let response: Vec<f64> = a.iter().zip(&b).map(|(a, b)| a * 10.0 + b).collect();
        let mut with_response = df.clone();
        with_response.add_column(crate::data::DType::Float);
        with_response
            .rename_column(with_response.n_columns() - 1, "y")
            .expect("rename");
        for (row, value) in response.iter().enumerate() {
            with_response
                .set_cell(with_response.n_columns() - 1, row, &value.to_string())
                .expect("set");
        }
        let model = fit(
            &with_response,
            "y",
            &["A".to_owned(), "B".to_owned()],
            Quality::Larger,
        )
        .expect("fit");
        assert_eq!(model.level_counts, vec![2, 3]);
        assert_eq!(model.levels, 3);
        assert_eq!(model.factors[0].means.len(), 2);
        assert_eq!(model.factors[1].means.len(), 3);
        assert_eq!(model.effects[0].df, 1);
        assert_eq!(model.effects[1].df, 2);
        let out = analyze(
            &with_response,
            "y",
            &["A".to_owned(), "B".to_owned()],
            Quality::Larger,
        )
        .expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Mixed levels: A = 2, B = 3"), "{text}");
        assert!(
            text.contains("L18") || text.contains("Taguchi Analysis"),
            "{text}"
        );
        assert!(matches!(
            design(18, 9, false, 2026),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn the_anova_partitions_the_sum_of_squares() {
        // Build an L8 with a response driven by A and a small effect of B.
        let df = design(8, 3, false, 2026).expect("design");
        let levels = |name: &str| {
            df.column(name)
                .expect("column")
                .numeric(MissingPolicy::Drop)
                .expect("levels")
                .to_vec()
        };
        let a = levels("A");
        let b = levels("B");
        let mut with_response = df.clone();
        with_response.add_column(crate::data::DType::Float);
        let last = with_response.n_columns() - 1;
        with_response.rename_column(last, "y").expect("rename");
        for row in 0..8 {
            let value = 10.0 * a[row] + b[row];
            with_response
                .set_cell(last, row, &value.to_string())
                .expect("set");
        }
        let model = fit(
            &with_response,
            "y",
            &["A".to_owned(), "B".to_owned()],
            Quality::Smaller,
        )
        .expect("fit");
        // The two factors explain everything: the pooled error is zero.
        let explained: f64 = model.effects.iter().map(|effect| effect.sum_squares).sum();
        assert!(
            (explained - model.total_ss).abs() < 1e-9,
            "{explained} vs {}",
            model.total_ss
        );
        assert!(model.error_ss.abs() < 1e-9);
        // A carries the larger contribution.
        assert!(model.effects[0].contribution > model.effects[1].contribution);
        assert!(
            (model.effects[0].contribution + model.effects[1].contribution - 100.0).abs() < 1e-9
        );
        // The grand mean sits between the two levels of the factors.
        assert!(
            (model.grand_mean - 16.5).abs() < 1e-9,
            "{}",
            model.grand_mean
        );
        let text = analyze(
            &with_response,
            "y",
            &["A".to_owned(), "B".to_owned()],
            Quality::Smaller,
        )
        .expect("analysis")
        .results[0]
            .to_session_text();
        assert!(text.contains("Contribution %"), "{text}");
        assert!(text.contains("Error (pooled)"), "{text}");
    }

    #[test]
    fn replicated_runs_give_the_ratio_anova() {
        // L8 with four factor columns, every run measured twice; A, B and D are
        // independent (C is the A·B product).
        let df = design(8, 4, false, 2026).expect("design");
        let levels = |name: &str| {
            df.column(name)
                .expect("column")
                .numeric(MissingPolicy::Drop)
                .expect("levels")
                .to_vec()
        };
        // A, B and D are independent columns of L8 (C is the A·B product).
        let a = levels("A");
        let b = levels("B");
        let c = levels("D");
        let mut with_response = df.clone();
        with_response.add_column(crate::data::DType::Float);
        let last = with_response.n_columns() - 1;
        with_response.rename_column(last, "y").expect("rename");
        for row in 0..8 {
            let value = 10.0 * a[row] + b[row] + 0.5 * c[row];
            with_response
                .set_cell(last, row, &value.to_string())
                .expect("set");
            // Second replicate, perturbed by half a unit on odd runs.
            with_response.add_row().expect("row");
            let target = 8 + row;
            for (column, value) in [
                (0, (row + 1) as f64),
                (1, (row + 1) as f64),
                (2, a[row]),
                (3, b[row]),
                (5, c[row]),
            ] {
                with_response
                    .set_cell(column, target, &value.to_string())
                    .expect("copy");
            }
            let value = 10.0 * a[row] + b[row] + 0.5 * c[row] + 0.5 * (row as f64 % 2.0);
            with_response
                .set_cell(last, target, &value.to_string())
                .expect("y");
        }
        let model = fit(
            &with_response,
            "y",
            &["A".to_owned(), "B".to_owned(), "D".to_owned()],
            Quality::Smaller,
        )
        .expect("fit");
        assert_eq!(model.n, 16);
        assert_eq!(model.replicates, 2, "{:?}", model.ratio_effects);
        let ratio = model.ratio_effects.as_ref().expect("ratio ANOVA");
        assert_eq!(ratio.len(), 3);
        let explained: f64 = ratio.iter().map(|effect| effect.sum_squares).sum();
        assert!(
            (explained + model.ratio_error_ss - model.ratio_total_ss).abs() < 1e-9,
            "{explained} + {} vs {}",
            model.ratio_error_ss,
            model.ratio_total_ss
        );
        assert!(ratio[0].contribution > 0.0);
        let text = analyze(
            &with_response,
            "y",
            &["A".to_owned(), "B".to_owned(), "D".to_owned()],
            Quality::Smaller,
        )
        .expect("analysis")
        .results[0]
            .to_session_text();
        assert!(text.contains("SS (S/N)"), "{text}");
    }

    #[test]
    fn a_missing_response_does_not_shift_the_rows() {
        let df = DataFrame::new(vec![
            Column::new_f64("y", vec![10.0, f64::NAN, 12.0, 13.0]),
            Column::new_f64("A", vec![1.0, 1.0, 2.0, 2.0]),
            Column::new_f64("B", vec![1.0, 2.0, 1.0, 2.0]),
        ])
        .expect("frame");
        let model = fit(
            &df,
            "y",
            &["A".to_owned(), "B".to_owned()],
            Quality::Smaller,
        )
        .expect("fit");
        assert_eq!(model.n, 3, "the missing row is skipped");
        assert_eq!(model.factors.len(), 2);
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let df = design(8, 2, false, 2026).expect("design");
        assert!(matches!(
            fit(&df, "A", &["A".to_owned()], Quality::Smaller),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            fit(&df, "StdOrder", &[], Quality::Smaller),
            Err(Error::Unsupported(_))
        ));
    }
}
