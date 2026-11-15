// SPDX-License-Identifier: MIT
//! General factorial analysis and two-level screening of effects.
//!
//! The general factorial fits a full model over factors with two to ten
//! levels. Every factor enters with effect coding (`+1` for the level, `−1`
//! for the last level and `0` elsewhere), so each factor adds `k − 1` columns
//! and each interaction multiplies the columns of its factors. The report
//! carries the sequential (Type I) and adjusted (Type III) sums of squares of
//! every term, the marginal means of every level and the residual plot.
//!
//! The screening analysis reads a two-level design coded to −1/+1 (the
//! Plackett–Burman design, for instance), estimates the effect of every factor
//! as the difference of its level means and decides which effects stand out
//! with Lenth's pseudo standard error: `PSE = 1.5·median|effect|`,
//! `ME = t(0.975; m/3)·PSE` for one effect at a time and
//! `SME = t(1 − 0.05/(2m); m/3)·PSE` for all of them together. A Pareto chart
//! of the absolute effects and a normal plot of the effects show the decision.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{
    AxisSpec, Band, PlotNote, PlotSpec, Point, Scale, Segment, Tick, linear_axis, panel_plot,
};
use crate::regression::{self, RegressionFit};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{f_cdf, normal_quantile, student_t_quantile};
use crate::workflow::AnalysisOutput;

/// Numeric value of a cell, or `None` when it is missing.
fn cell(column: &Column, row: usize) -> Option<f64> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => values.get(row).copied().filter(|value| value.is_finite()),
        ColumnData::I64(values) => values.get(row).map(|&value| value as f64),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_) => None,
    }
}

/// Text label of a cell, or `None` when it is missing.
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
            .map(|&value| if value { "T" } else { "F" }.to_owned()),
    }
}

/// The levels of one factor, in report order.
#[derive(Debug, Clone, PartialEq)]
pub struct FactorLevels {
    /// Factor name.
    pub factor: String,
    /// Level labels: numeric factors ascending, text factors in first-appearance
    /// order.
    pub levels: Vec<String>,
}

/// One term of the general factorial model.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneralTerm {
    /// Display name: `A`, `A*B`, `A*B*C`.
    pub name: String,
    /// Degrees of freedom (`Π (kᵢ − 1)`).
    pub df: f64,
    /// Sum of squares added by the term when it enters the model.
    pub sequential_ss: f64,
    /// Sum of squares lost when the term leaves the full model.
    pub adjusted_ss: f64,
    /// Adjusted mean square.
    pub adjusted_ms: f64,
    /// F statistic against the residual of the full model.
    pub f: f64,
    /// p-value of the F statistic.
    pub p: f64,
}

/// Mean of one level of a factor (ignoring the other factors).
#[derive(Debug, Clone, PartialEq)]
pub struct LevelMean {
    /// Level label.
    pub level: String,
    /// Observations in the level.
    pub n: usize,
    /// Mean of the response in the level.
    pub mean: f64,
}

/// Fitted general factorial.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneralFactorialFit {
    /// Terms in report order (main effects, then interactions).
    pub terms: Vec<GeneralTerm>,
    /// Factors and their levels.
    pub factors: Vec<FactorLevels>,
    /// Marginal means, one entry per factor in the same order as `factors`.
    pub means: Vec<Vec<LevelMean>>,
    /// Ordinary least squares fit of the full model.
    pub regression: RegressionFit,
    /// Observations used.
    pub n: usize,
    /// Rows excluded for a missing or non-finite value.
    pub excluded: usize,
}

/// Decision of a screening effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreeningVerdict {
    /// The effect is below the individual margin.
    Inactive,
    /// The effect exceeds the individual margin.
    Active,
    /// The effect exceeds the simultaneous margin.
    Simultaneous,
}

impl ScreeningVerdict {
    /// Name used by the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Inactive => "Inactive",
            Self::Active => "Active",
            Self::Simultaneous => "Active (simultaneous)",
        }
    }

    /// Whether the effect is called active at any margin.
    #[must_use]
    pub fn is_active(self) -> bool {
        self != Self::Inactive
    }
}

/// One effect of the screening analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreeningEffect {
    /// Factor name.
    pub term: String,
    /// Effect: mean at +1 minus mean at −1.
    pub effect: f64,
    /// Absolute effect, kept for the Pareto chart.
    pub absolute: f64,
    /// Decision for the effect.
    pub verdict: ScreeningVerdict,
}

/// Fitted screening analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreeningFit {
    /// Effects in factor order.
    pub effects: Vec<ScreeningEffect>,
    /// Lenth pseudo standard error.
    pub pse: f64,
    /// Degrees of freedom of the margins (`m/3`).
    pub df: f64,
    /// Individual margin `ME`.
    pub me: f64,
    /// Simultaneous margin `SME`.
    pub sme: f64,
    /// Observations used.
    pub n: usize,
    /// Rows excluded for a missing or non-finite value.
    pub excluded: usize,
}

/// Median of a slice, averaging the two central values on an even length.
fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        0.5 * (values[n / 2 - 1] + values[n / 2])
    }
}

/// Validates the shared arguments of both analyses.
fn validate(df: &DataFrame, response: &str, factors: &[String], maximum: usize) -> Result<()> {
    if factors.is_empty() {
        return Err(Error::Unsupported(
            "the analysis needs at least one factor".to_owned(),
        ));
    }
    if factors.iter().any(|factor| factor == response) {
        return Err(Error::Unsupported(
            "the response cannot also be a factor".to_owned(),
        ));
    }
    let mut unique = factors.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != factors.len() {
        return Err(Error::Unsupported(
            "the factor list contains duplicates".to_owned(),
        ));
    }
    if factors.len() > maximum {
        return Err(Error::Unsupported(format!(
            "at most {maximum} factors are supported by this analysis"
        )));
    }
    let response_column = df.column(response)?;
    if matches!(
        response_column.data(),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
    ) {
        return Err(Error::Csv(format!(
            "response column {response} is not numeric"
        )));
    }
    for name in factors {
        df.column(name)?;
    }
    Ok(())
}

/// Levels of a factor in report order.
fn levels_of(column: &Column) -> Vec<String> {
    let numeric = matches!(column.data(), ColumnData::F64(_) | ColumnData::I64(_));
    let mut seen: Vec<String> = Vec::new();
    for row in 0..column.len() {
        if let Some(value) = label(column, row)
            && !seen.contains(&value)
        {
            seen.push(value);
        }
    }
    if numeric {
        seen.sort_by(|a, b| {
            let left = a.parse::<f64>().unwrap_or(f64::NAN);
            let right = b.parse::<f64>().unwrap_or(f64::NAN);
            left.partial_cmp(&right)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    seen
}

/// Effect coded columns of one factor: `k − 1` columns with `+1` in their own
/// level, `−1` in the last level and `0` elsewhere.
fn effect_code(level_index: &[usize], k: usize) -> Vec<Vec<f64>> {
    let mut columns = Vec::with_capacity(k.saturating_sub(1));
    for position in 0..k.saturating_sub(1) {
        columns.push(
            level_index
                .iter()
                .map(|&index| {
                    if index == position {
                        1.0
                    } else if index == k - 1 {
                        -1.0
                    } else {
                        0.0
                    }
                })
                .collect(),
        );
    }
    columns
}

/// Fits the general factorial model.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for a duplicate factor, more than six
/// factors, a factor with fewer than two or more than ten levels, a response
/// that is not numeric and the same response and factor column; the usual
/// column errors otherwise.
pub fn general_fit(
    df: &DataFrame,
    response: &str,
    factors: &[String],
) -> Result<GeneralFactorialFit> {
    validate(df, response, factors, 6)?;
    let response_column = df.column(response)?;
    let factor_columns: Vec<&Column> = factors
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    let mut levels: Vec<FactorLevels> = Vec::with_capacity(factors.len());
    for (name, column) in factors.iter().zip(&factor_columns) {
        let found = levels_of(column);
        if found.len() < 2 {
            return Err(Error::Unsupported(format!(
                "factor {name} has a single level: the analysis needs at least two"
            )));
        }
        if found.len() > 10 {
            return Err(Error::Unsupported(format!(
                "factor {name} has {} levels: at most ten are supported",
                found.len()
            )));
        }
        levels.push(FactorLevels {
            factor: name.clone(),
            levels: found,
        });
    }

    let mut response_values: Vec<f64> = Vec::new();
    let mut level_indices: Vec<Vec<usize>> = vec![Vec::new(); factors.len()];
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        let Some(value) = cell(response_column, row) else {
            excluded += 1;
            continue;
        };
        let mut indices = Vec::with_capacity(factors.len());
        let mut complete = true;
        for (factor, column) in levels.iter().zip(&factor_columns) {
            match label(column, row) {
                Some(found) => match factor.levels.iter().position(|level| *level == found) {
                    Some(index) => indices.push(index),
                    None => {
                        complete = false;
                        break;
                    }
                },
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
        response_values.push(value);
        for (index, value) in indices.into_iter().enumerate() {
            level_indices[index].push(value);
        }
    }
    let n = response_values.len();
    if n == 0 {
        return Err(Error::EmptySample);
    }

    let coded: Vec<Vec<Vec<f64>>> = levels
        .iter()
        .zip(&level_indices)
        .map(|(factor, indices)| effect_code(indices, factor.levels.len()))
        .collect();

    // Every non-empty subset of factors, main effects first.
    let q = factors.len();
    let mut masks: Vec<usize> = (1..(1usize << q)).collect();
    masks.sort_by(|a, b| a.count_ones().cmp(&b.count_ones()).then_with(|| a.cmp(b)));
    let mut term_names: Vec<String> = Vec::with_capacity(masks.len());
    let mut term_columns: Vec<Vec<Vec<f64>>> = Vec::with_capacity(masks.len());
    for mask in &masks {
        let name = factors
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, name)| name.clone())
            .collect::<Vec<String>>()
            .join("*");
        // Cartesian product of the coded columns of the factors in the term.
        let mut product: Vec<Vec<f64>> = vec![vec![1.0; n]];
        for (index, columns) in coded.iter().enumerate() {
            if mask & (1 << index) == 0 {
                continue;
            }
            let mut expanded = Vec::with_capacity(product.len() * columns.len());
            for left in &product {
                for right in columns {
                    expanded.push(
                        left.iter()
                            .zip(right)
                            .map(|(a, b)| a * b)
                            .collect::<Vec<f64>>(),
                    );
                }
            }
            product = expanded;
        }
        term_names.push(name);
        term_columns.push(product);
    }

    let term_df: Vec<f64> = term_columns.iter().map(|term| term.len() as f64).collect();
    let total_terms: usize = term_columns.iter().map(Vec::len).sum();
    if n <= total_terms {
        return Err(Error::SampleTooSmall {
            n,
            min: total_terms + 1,
        });
    }

    let mut columns = Vec::with_capacity(total_terms + 1);
    columns.push(Column::new_f64(response, response_values.clone()));
    let mut predictor_names: Vec<String> = Vec::with_capacity(total_terms);
    let mut starts: Vec<usize> = Vec::with_capacity(term_columns.len());
    for (term, term_values) in term_columns.iter().enumerate() {
        starts.push(predictor_names.len());
        for (position, values) in term_values.iter().enumerate() {
            let name = format!("__t{term}_{position}");
            columns.push(Column::new_f64(name.clone(), values.clone()));
            predictor_names.push(name);
        }
    }
    let temp = DataFrame::new(columns)?;
    let full = regression::fit_saturated(&temp, response, &predictor_names, 0.95)?;

    let mean = response_values.iter().sum::<f64>() / n as f64;
    let centered_total: f64 = response_values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum();
    let mut sequential: Vec<f64> = Vec::with_capacity(term_columns.len());
    let mut previous_ss = 0.0;
    let mut cumulative: Vec<String> = Vec::new();
    for (term, term_values) in term_columns.iter().enumerate() {
        for position in 0..term_values.len() {
            cumulative.push(format!("__t{term}_{position}"));
        }
        let model = regression::fit_saturated(&temp, response, &cumulative, 0.95)?;
        sequential.push((model.regression_ss - previous_ss).max(0.0));
        previous_ss = model.regression_ss;
    }

    let error_df = full.df_residual;
    let error_ms = if error_df > 0.0 {
        full.residual_ss / error_df
    } else {
        f64::NAN
    };
    let mut terms: Vec<GeneralTerm> = Vec::with_capacity(term_columns.len());
    for (index, name) in term_names.iter().enumerate() {
        let start = starts[index];
        let end = start + term_columns[index].len();
        let reduced: Vec<String> = predictor_names
            .iter()
            .enumerate()
            .filter(|(position, _)| *position < start || *position >= end)
            .map(|(_, name)| name.clone())
            .collect();
        let reduced_residual = if reduced.is_empty() {
            centered_total
        } else {
            regression::fit_saturated(&temp, response, &reduced, 0.95)?.residual_ss
        };
        let adjusted_ss = (reduced_residual - full.residual_ss).max(0.0);
        let df = term_df[index];
        let adjusted_ms = if df > 0.0 { adjusted_ss / df } else { f64::NAN };
        let f = if error_ms > 0.0 && error_ms.is_finite() {
            adjusted_ms / error_ms
        } else {
            f64::NAN
        };
        let p = if f.is_finite() {
            (1.0 - f_cdf(f, df, error_df)).clamp(0.0, 1.0)
        } else {
            f64::NAN
        };
        terms.push(GeneralTerm {
            name: name.clone(),
            df,
            sequential_ss: sequential[index],
            adjusted_ss,
            adjusted_ms,
            f,
            p,
        });
    }

    let mut means: Vec<Vec<LevelMean>> = Vec::with_capacity(levels.len());
    for (factor_index, factor) in levels.iter().enumerate() {
        let mut rows = Vec::with_capacity(factor.levels.len());
        for (index, level) in factor.levels.iter().enumerate() {
            let mut count = 0usize;
            let mut sum = 0.0;
            for (row, found) in level_indices[factor_index].iter().enumerate() {
                if *found == index {
                    count += 1;
                    sum += response_values[row];
                }
            }
            rows.push(LevelMean {
                level: level.clone(),
                n: count,
                mean: if count > 0 {
                    sum / count as f64
                } else {
                    f64::NAN
                },
            });
        }
        means.push(rows);
    }

    Ok(GeneralFactorialFit {
        terms,
        factors: levels,
        means,
        regression: full,
        n,
        excluded,
    })
}

/// Builds the report and the residual plot of the general factorial.
///
/// # Errors
///
/// The errors of [`general_fit`].
pub fn general_analyze(
    df: &DataFrame,
    response: &str,
    factors: &[String],
) -> Result<AnalysisOutput> {
    let model = general_fit(df, response, factors)?;
    let mut result = AnalysisResult::new(
        format!("General Factorial — response {response}"),
        "Full factorial with effect coded levels; ordinary least squares with all main effects \
         and interactions",
    );
    result.n = model.n;
    result.excluded = model.excluded;
    let regression = &model.regression;
    let error_df = regression.df_residual;
    let error_ss = regression.residual_ss;
    let error_ms = if error_df > 0.0 {
        error_ss / error_df
    } else {
        f64::NAN
    };
    let model_df = regression.df_regression;
    let model_ss = regression.regression_ss;
    let model_ms = if model_df > 0.0 {
        model_ss / model_df
    } else {
        f64::NAN
    };
    let model_f = if error_ms > 0.0 && error_ms.is_finite() {
        model_ms / error_ms
    } else {
        f64::NAN
    };
    let model_p = if model_f.is_finite() {
        (1.0 - f_cdf(model_f, model_df, error_df)).clamp(0.0, 1.0)
    } else {
        f64::NAN
    };
    result.statistics = vec![
        ("R²", regression.r_squared),
        ("Adjusted R²", regression.adjusted_r_squared),
        (
            "Residual standard error",
            if error_ms.is_finite() {
                error_ms.max(0.0).sqrt()
            } else {
                regression.s
            },
        ),
        ("Model DF", model_df),
        ("Error DF", error_df),
        ("F", model_f),
        ("F p-value", model_p),
    ];
    result
        .extra
        .push(Item::Message(format!("Factors: {}", factors.join(", "))));
    result.extra.push(Item::Message(format!(
        "Levels: {}",
        model
            .factors
            .iter()
            .map(|factor| format!("{} ({})", factor.factor, factor.levels.len()))
            .collect::<Vec<String>>()
            .join(", ")
    )));
    result.extra.push(Item::Table {
        headers: vec![
            "Source".to_owned(),
            "DF".to_owned(),
            "Seq SS".to_owned(),
            "Adj SS".to_owned(),
            "Adj MS".to_owned(),
            "F".to_owned(),
            "P".to_owned(),
        ],
        rows: {
            let mut rows: Vec<Vec<String>> = model
                .terms
                .iter()
                .map(|term| {
                    vec![
                        term.name.clone(),
                        format!("{:.0}", term.df),
                        format!("{:.6}", term.sequential_ss),
                        format!("{:.6}", term.adjusted_ss),
                        format!("{:.6}", term.adjusted_ms),
                        if term.f.is_finite() {
                            format!("{:.6}", term.f)
                        } else {
                            String::new()
                        },
                        if term.p.is_finite() {
                            format!("{:.6}", term.p)
                        } else {
                            String::new()
                        },
                    ]
                })
                .collect();
            rows.push(vec![
                "Model".to_owned(),
                format!("{model_df:.0}"),
                format!("{model_ss:.6}"),
                String::new(),
                format!("{model_ms:.6}"),
                format!("{model_f:.6}"),
                format!("{model_p:.6}"),
            ]);
            rows.push(vec![
                "Error".to_owned(),
                format!("{error_df:.0}"),
                format!("{error_ss:.6}"),
                String::new(),
                format!("{error_ms:.6}"),
                String::new(),
                String::new(),
            ]);
            rows.push(vec![
                "Total".to_owned(),
                format!("{:.0}", model_df + error_df),
                format!("{:.6}", model_ss + error_ss),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]);
            rows
        },
    });
    for (factor, means) in model.factors.iter().zip(&model.means) {
        result.extra.push(Item::Table {
            headers: vec![
                "Factor".to_owned(),
                "Level".to_owned(),
                "n".to_owned(),
                "Mean".to_owned(),
            ],
            rows: means
                .iter()
                .map(|row| {
                    vec![
                        factor.factor.clone(),
                        row.level.clone(),
                        row.n.to_string(),
                        format!("{:.6}", row.mean),
                    ]
                })
                .collect(),
        });
    }
    result.notes = vec![
        (
            "Coding".to_owned(),
            "every level enters as +1, the last level as −1 and the remaining levels as 0, so \
             a factor with k levels adds k − 1 columns and an interaction multiplies the columns \
             of its factors"
                .to_owned(),
        ),
        (
            "Sums of squares".to_owned(),
            "Seq SS adds the terms in the order of the table; Adj SS drops the columns of the \
             term from the full model and compares the residual sums of squares, and the F test \
             uses the residual mean square of the full model"
                .to_owned(),
        ),
        (
            "Replication".to_owned(),
            "the error needs more observations than model parameters; a design with one \
             observation per combination has no error estimate and the tests are undefined"
                .to_owned(),
        ),
        (
            "Scope".to_owned(),
            "fixed effects only: no random factors, no covariates, no response surface terms and \
             no definitive screening designs"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    if model.regression.df_residual <= 0.0 {
        result.warnings.push(
            "the design is saturated: the error has no degrees of freedom and the F tests are \
             undefined"
                .to_owned(),
        );
    }
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(regression::residual_plot(response, &model.regression)),
    })
}

/// Fits the screening analysis of a two-level design.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for a duplicate factor, more than fifteen
/// factors, a factor value other than −1/+1, a factor with a single level, all
/// effects equal to zero (no scale for Lenth's pseudo standard error) and the
/// same response and factor column; [`Error::SampleTooSmall`] when the design
/// has no more observations than factors, and the usual column errors.
pub fn screening_fit(df: &DataFrame, response: &str, factors: &[String]) -> Result<ScreeningFit> {
    validate(df, response, factors, 15)?;
    let response_column = df.column(response)?;
    let factor_columns: Vec<&Column> = factors
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    let mut response_values: Vec<f64> = Vec::new();
    let mut factor_values: Vec<Vec<f64>> = vec![Vec::new(); factors.len()];
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        let Some(value) = cell(response_column, row) else {
            excluded += 1;
            continue;
        };
        let mut values = Vec::with_capacity(factors.len());
        let mut complete = true;
        for column in &factor_columns {
            match cell(column, row) {
                Some(factor_value) if (factor_value.abs() - 1.0).abs() < 1e-9 => {
                    values.push(factor_value);
                }
                Some(factor_value) => {
                    return Err(Error::Cell {
                        column: column.name().to_owned(),
                        row: row + 1,
                        message: format!(
                            "screening factors must be coded −1/+1 (found {factor_value})"
                        ),
                    });
                }
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
        response_values.push(value);
        for (index, factor_value) in values.into_iter().enumerate() {
            factor_values[index].push(factor_value);
        }
    }
    let n = response_values.len();
    if n <= factors.len() {
        return Err(Error::SampleTooSmall {
            n,
            min: factors.len() + 1,
        });
    }

    let mut effects: Vec<f64> = Vec::with_capacity(factors.len());
    for values in &factor_values {
        let mut high = (0usize, 0.0);
        let mut low = (0usize, 0.0);
        for (row, value) in values.iter().enumerate() {
            if *value > 0.0 {
                high = (high.0 + 1, high.1 + response_values[row]);
            } else {
                low = (low.0 + 1, low.1 + response_values[row]);
            }
        }
        if high.0 == 0 || low.0 == 0 {
            return Err(Error::Unsupported(
                "every factor needs observations at both −1 and +1".to_owned(),
            ));
        }
        effects.push(high.1 / high.0 as f64 - low.1 / low.0 as f64);
    }
    let mut absolute: Vec<f64> = effects.iter().map(|effect| effect.abs()).collect();
    let pse = 1.5 * median(&mut absolute);
    if pse <= 0.0 || pse.is_nan() {
        return Err(Error::Unsupported(
            "at least half of the effects are zero: Lenth's pseudo standard error is undefined"
                .to_owned(),
        ));
    }
    let m = effects.len() as f64;
    let df = m / 3.0;
    let me = student_t_quantile(0.975, df) * pse;
    let sme = student_t_quantile(1.0 - 0.05 / (2.0 * m), df) * pse;
    let rows: Vec<ScreeningEffect> = factors
        .iter()
        .zip(&effects)
        .map(|(name, effect)| {
            let magnitude = effect.abs();
            let verdict = if magnitude > sme {
                ScreeningVerdict::Simultaneous
            } else if magnitude > me {
                ScreeningVerdict::Active
            } else {
                ScreeningVerdict::Inactive
            };
            ScreeningEffect {
                term: name.clone(),
                effect: *effect,
                absolute: magnitude,
                verdict,
            }
        })
        .collect();

    Ok(ScreeningFit {
        effects: rows,
        pse,
        df,
        me,
        sme,
        n,
        excluded,
    })
}

/// Pareto chart of the absolute effects with the two margins.
fn pareto_chart(model: &ScreeningFit, response: &str) -> PlotSpec {
    let mut order: Vec<usize> = (0..model.effects.len()).collect();
    order.sort_by(|a, b| {
        model.effects[*b]
            .absolute
            .partial_cmp(&model.effects[*a].absolute)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let count = order.len();
    let top = order
        .iter()
        .map(|index| model.effects[*index].absolute)
        .fold(0.0f64, f64::max)
        .max(model.sme.max(model.me));
    let bands: Vec<Band> = order
        .iter()
        .enumerate()
        .map(|(position, index)| {
            let centre = position as f64 + 1.0;
            let value = model.effects[*index].absolute;
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
                        y: value,
                    },
                    Point {
                        x: centre + 0.35,
                        y: value,
                    },
                ],
            }
        })
        .collect();
    let lines = vec![
        Segment {
            from: Point {
                x: 0.4,
                y: model.me,
            },
            to: Point {
                x: count as f64 + 0.6,
                y: model.me,
            },
            width: 1.5,
        },
        Segment {
            from: Point {
                x: 0.4,
                y: model.sme,
            },
            to: Point {
                x: count as f64 + 0.6,
                y: model.sme,
            },
            width: 1.5,
        },
    ];
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
    let step = if count <= 12 { 1 } else { count.div_ceil(12) };
    PlotSpec {
        title: "Pareto Chart of the Effects".to_owned(),
        x_axis: AxisSpec {
            label: response.to_owned(),
            scale: Scale::Linear,
            min: 0.4,
            max: count as f64 + 0.6,
            ticks: order
                .iter()
                .enumerate()
                .filter(|(position, _)| position % step == 0)
                .map(|(position, index)| Tick {
                    position: position as f64 + 1.0,
                    label: shorten(&model.effects[*index].term),
                })
                .collect(),
            minor: Vec::new(),
        },
        y_axis: linear_axis("|Effect|", 0.0, top * 1.08),
        size: (900, 520),
        points: Vec::new(),
        lines,
        bands,
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: vec![PlotNote::new(vec![
            ("ME".to_owned(), format!("{:.6}", model.me)),
            ("SME".to_owned(), format!("{:.6}", model.sme)),
        ])],
    }
}

/// Normal plot of the effects with the line through the origin of slope PSE.
fn normal_chart(model: &ScreeningFit) -> PlotSpec {
    let mut sorted: Vec<f64> = model.effects.iter().map(|effect| effect.effect).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let count = sorted.len();
    let positions: Vec<f64> = (1..=count)
        .map(|index| (index as f64 - 0.375) / (count as f64 + 0.25))
        .collect();
    let scores: Vec<f64> = positions
        .iter()
        .map(|value| normal_quantile(*value))
        .collect();
    let points: Vec<Point> = sorted
        .iter()
        .zip(&scores)
        .map(|(effect, score)| Point {
            x: *effect,
            y: *score,
        })
        .collect();
    let low = scores.iter().copied().fold(f64::INFINITY, f64::min) - 0.25;
    let high = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max) + 0.25;
    let pad = (sorted
        .iter()
        .map(|value| value.abs())
        .fold(0.0f64, f64::max)
        * 0.08)
        .max(1e-9);
    let low_x = sorted.first().copied().unwrap_or(0.0);
    let high_x = sorted.last().copied().unwrap_or(0.0);
    PlotSpec {
        title: "Normal Plot of the Effects".to_owned(),
        x_axis: linear_axis("Effect", low_x - pad, high_x + pad),
        y_axis: linear_axis("Normal score", low, high),
        size: (900, 520),
        points,
        lines: vec![Segment {
            from: Point {
                x: model.pse * low,
                y: low,
            },
            to: Point {
                x: model.pse * high,
                y: high,
            },
            width: 2.0,
        }],
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: vec![PlotNote::new(vec![(
            "PSE".to_owned(),
            format!("{:.6}", model.pse),
        )])],
    }
}

/// Builds the screening report and the two effect charts.
///
/// # Errors
///
/// The errors of [`screening_fit`].
pub fn screening_analyze(
    df: &DataFrame,
    response: &str,
    factors: &[String],
) -> Result<AnalysisOutput> {
    let model = screening_fit(df, response, factors)?;
    let mut result = AnalysisResult::new(
        format!("Screening — response {response}"),
        "Two-level screening: the effect of every factor with Lenth's pseudo standard error",
    );
    result.n = model.n;
    result.excluded = model.excluded;
    let active = model
        .effects
        .iter()
        .filter(|effect| effect.verdict.is_active())
        .count();
    let simultaneous = model
        .effects
        .iter()
        .filter(|effect| effect.verdict == ScreeningVerdict::Simultaneous)
        .count();
    result.statistics = vec![
        ("Factors", model.effects.len() as f64),
        ("PSE", model.pse),
        ("DF", model.df),
        ("ME (individual)", model.me),
        ("SME (simultaneous)", model.sme),
        ("Active at ME", active as f64),
        ("Active at SME", simultaneous as f64),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Term".to_owned(),
            "Effect".to_owned(),
            "|Effect|".to_owned(),
            "Decision".to_owned(),
        ],
        rows: model
            .effects
            .iter()
            .map(|effect| {
                vec![
                    effect.term.clone(),
                    format!("{:+.6}", effect.effect),
                    format!("{:.6}", effect.absolute),
                    effect.verdict.name().to_owned(),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Design".to_owned(),
            "every factor must be coded −1/+1 with observations at both levels; the effect of a \
             factor is the mean at +1 minus the mean at −1 and the factors are assumed \
             independent"
                .to_owned(),
        ),
        (
            "Lenth".to_owned(),
            "the pseudo standard error is 1.5 times the median of the absolute effects, with \
             m/3 degrees of freedom; ME = t(0.975; m/3)·PSE decides one effect at a time and \
             SME = t(1 − 0.05/(2m); m/3)·PSE decides all of them together"
                .to_owned(),
        ),
        (
            "Scope".to_owned(),
            "main effects only: interactions are not estimated, the design is assumed \
             unreplicated or nearly so and a screening decision points to the factors worth a \
             confirmation experiment"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    if model.df < 1.0 {
        result.warnings.push(
            "fewer than four effects: the margins use less than one degree of freedom and are \
             very wide"
                .to_owned(),
        );
    }
    let plot = panel_plot(
        vec![pareto_chart(&model, response), normal_chart(&model)],
        2,
        format!("Screening — {response}"),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(columns: Vec<Column>) -> DataFrame {
        DataFrame::new(columns).expect("frame")
    }

    /// A 3×2 full factorial with two replicates and a known model.
    fn general_frame() -> DataFrame {
        let mut a = Vec::new();
        let mut b = Vec::new();
        let mut y = Vec::new();
        for _ in 0..2 {
            for level in [0usize, 1, 2] {
                for block in [-1.0f64, 1.0] {
                    let c1 = if level == 0 {
                        1.0
                    } else if level == 2 {
                        -1.0
                    } else {
                        0.0
                    };
                    let c2 = if level == 1 {
                        1.0
                    } else if level == 2 {
                        -1.0
                    } else {
                        0.0
                    };
                    a.push(level as f64);
                    b.push(block);
                    y.push(10.0 + 2.0 * c1 + 3.0 * c2 + 4.0 * block + 5.0 * c1 * block);
                }
            }
        }
        frame(vec![
            Column::new_f64("y", y),
            Column::new_f64("a", a),
            Column::new_f64("b", b),
        ])
    }

    #[test]
    fn the_general_factorial_reports_the_terms_and_means() {
        let df = general_frame();
        let model = general_fit(&df, "y", &["a".to_owned(), "b".to_owned()]).expect("fit");
        assert_eq!(model.n, 12);
        assert_eq!(model.factors[0].levels, vec!["0", "1", "2"]);
        assert_eq!(model.factors[1].levels, vec!["-1", "1"]);
        let names: Vec<&str> = model.terms.iter().map(|term| term.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "a*b"]);
        assert_eq!(model.terms[0].df, 2.0);
        assert_eq!(model.terms[1].df, 1.0);
        assert_eq!(model.terms[2].df, 2.0);
        // The model reproduces the data exactly.
        assert!(model.regression.r_squared > 0.999_999);
        assert!(model.regression.residual_ss < 1e-18);
        // The coefficient of b is 4 on twelve observations: SS = 4²·12.
        let b = model.terms[1].adjusted_ss;
        assert!((b - 192.0).abs() < 1e-9, "{b}");
        // Marginal means of a: the model gives 12 + 9b, 13 + 4b and 5 − b.
        let means = &model.means[0];
        assert_eq!(means.len(), 3);
        assert!((means[0].mean - 12.0).abs() < 1e-9, "{}", means[0].mean);
        assert!((means[1].mean - 13.0).abs() < 1e-9, "{}", means[1].mean);
        assert!((means[2].mean - 5.0).abs() < 1e-9, "{}", means[2].mean);
        assert_eq!(means.iter().map(|row| row.n).sum::<usize>(), 12);
    }

    #[test]
    fn the_general_factorial_handles_a_saturated_design_and_missing_rows() {
        // One observation per combination: no residual degrees of freedom.
        let df = frame(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 5.0]),
            Column::new_f64("a", vec![-1.0, 1.0, -1.0, 1.0]),
            Column::new_f64("b", vec![-1.0, -1.0, 1.0, 1.0]),
        ]);
        let model = general_fit(&df, "y", &["a".to_owned(), "b".to_owned()]).expect("fit");
        assert_eq!(model.regression.df_residual, 0.0);
        assert!(model.terms.iter().all(|term| term.f.is_nan()));
        let output = general_analyze(&df, "y", &["a".to_owned(), "b".to_owned()]).expect("report");
        let text = output.results[0].to_session_text();
        assert!(text.contains("saturated"), "{text}");

        // A missing response and a missing factor drop the row and count it.
        let df = frame(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 5.0, f64::NAN, 6.0]),
            Column::new_text(
                "a",
                vec!["low", "high", "low", "high", "low", "high"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<String>>(),
            ),
            Column::new_f64("b", vec![-1.0, -1.0, 1.0, 1.0, -1.0, 1.0]),
        ]);
        let model = general_fit(&df, "y", &["a".to_owned(), "b".to_owned()]).expect("fit");
        assert_eq!(model.excluded, 1);
        assert_eq!(model.n, 5);
        assert_eq!(model.factors[0].levels, vec!["low", "high"]);
    }

    #[test]
    fn the_general_factorial_rejects_invalid_requests() {
        let df = general_frame();
        // The response cannot be a factor.
        assert!(general_fit(&df, "y", &["y".to_owned()]).is_err());
        // Duplicate factors.
        assert!(
            general_fit(&df, "y", &["a".to_owned(), "a".to_owned()])
                .unwrap_err()
                .to_string()
                .contains("duplicates")
        );
        // A single-level factor.
        let flat = frame(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0]),
            Column::new_f64("a", vec![1.0, 1.0, 1.0]),
            Column::new_f64("b", vec![-1.0, 1.0, -1.0]),
        ]);
        assert!(
            general_fit(&flat, "y", &["a".to_owned(), "b".to_owned()])
                .unwrap_err()
                .to_string()
                .contains("single level")
        );
        // Too many observations are needed for the parameters.
        let short = frame(vec![
            Column::new_f64("y", vec![1.0, 2.0]),
            Column::new_f64("a", vec![-1.0, 1.0]),
            Column::new_f64("b", vec![-1.0, 1.0]),
        ]);
        assert!(matches!(
            general_fit(&short, "y", &["a".to_owned(), "b".to_owned()]),
            Err(Error::SampleTooSmall { .. })
        ));
        // A text response.
        let words = frame(vec![
            Column::new_text(
                "y",
                vec!["1", "2", "3", "7"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<String>>(),
            ),
            Column::new_f64("a", vec![-1.0, 1.0, -1.0, 1.0]),
            Column::new_f64("b", vec![-1.0, -1.0, 1.0, 1.0]),
        ]);
        assert!(general_fit(&words, "y", &["a".to_owned(), "b".to_owned()]).is_err());
    }

    /// A twelve-run Plackett–Burman design with known effects.
    fn screening_frame() -> DataFrame {
        let design = crate::doe::plackett_burman_design(12, false, 7).expect("design");
        let factors: Vec<String> = design
            .columns()
            .iter()
            .map(|column| column.name().to_owned())
            .filter(|name| name != "StdOrder" && name != "RunOrder")
            .collect();
        let effects = [8.0, 0.4, -0.3, 0.6, 0.2, -0.5, 0.1, 0.3, -0.2, 0.45, 0.15];
        assert_eq!(factors.len(), effects.len());
        let mut y = vec![20.0; design.n_rows()];
        for (index, name) in factors.iter().enumerate() {
            let column = design.column(name).expect("factor");
            for (row, value) in y.iter_mut().enumerate() {
                *value += 0.5 * effects[index] * cell(column, row).unwrap_or(0.0);
            }
        }
        let mut columns = vec![Column::new_f64("y", y)];
        for name in &factors {
            columns.push(design.column(name).expect("factor").clone());
        }
        frame(columns)
    }

    #[test]
    fn the_screening_recovers_the_effects_and_flags_the_large_one() {
        let df = screening_frame();
        let factors: Vec<String> = df
            .columns()
            .iter()
            .map(|column| column.name().to_owned())
            .filter(|name| name != "y")
            .collect();
        let model = screening_fit(&df, "y", &factors).expect("fit");
        assert_eq!(model.n, 12);
        assert_eq!(model.effects.len(), 11);
        let first = &model.effects[0];
        assert!((first.effect - 8.0).abs() < 1e-9, "{}", first.effect);
        assert!((model.pse - 0.45).abs() < 1e-9, "{}", model.pse);
        assert!((model.df - 11.0 / 3.0).abs() < 1e-12);
        // The margins are multiples of the PSE: t(0.975; 11/3) ≈ 2.93 and
        // t(1 − 0.05/22; 11/3) ≈ 4.5, so ME/PSE ∈ (2.5, 3.5) and SME > ME.
        let me_ratio = model.me / model.pse;
        assert!((2.5..3.5).contains(&me_ratio), "{me_ratio}");
        assert!(model.sme > model.me, "{} {}", model.sme, model.me);
        assert!(model.me / model.pse < model.sme / model.pse);
        assert_eq!(first.verdict, ScreeningVerdict::Simultaneous);
        assert_eq!(model.effects[1].verdict, ScreeningVerdict::Inactive);
        let output = screening_analyze(&df, "y", &factors).expect("report");
        let text = output.results[0].to_session_text();
        assert!(text.contains("Active at SME"), "{text}");
        let plot = output.plot.expect("plot");
        assert_eq!(plot.panels.len(), 2);
        assert!(plot.panels[0].title.contains("Pareto"));
        assert!(plot.panels[1].title.contains("Normal"));
    }

    #[test]
    fn the_screening_needs_a_scale_and_coded_factors() {
        // Three effects, two of them exactly zero: the median is zero.
        let df = frame(vec![
            Column::new_f64("y", vec![1.0, 3.0, 1.0, 3.0, 1.0, 3.0, 1.0, 3.0]),
            Column::new_f64("a", vec![-1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0]),
            Column::new_f64("b", vec![-1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0]),
            Column::new_f64("c", vec![-1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0]),
        ]);
        let error =
            screening_fit(&df, "y", &["a".to_owned(), "b".to_owned(), "c".to_owned()]).unwrap_err();
        assert!(error.to_string().contains("undefined"), "{error}");
        // Factors coded 0/1 are refused.
        let uncoded = frame(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_f64("a", vec![0.0, 1.0, 0.0, 1.0]),
            Column::new_f64("b", vec![0.0, 0.0, 1.0, 1.0]),
        ]);
        assert!(
            screening_fit(&uncoded, "y", &["a".to_owned(), "b".to_owned()])
                .unwrap_err()
                .to_string()
                .contains("coded")
        );
        // A factor with a single level has no effect to estimate.
        let single = frame(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_f64("a", vec![-1.0, 1.0, -1.0, 1.0]),
            Column::new_f64("b", vec![1.0, 1.0, 1.0, 1.0]),
        ]);
        assert!(
            screening_fit(&single, "y", &["a".to_owned(), "b".to_owned()])
                .unwrap_err()
                .to_string()
                .contains("both")
        );
        // Duplicate factors.
        assert!(
            screening_fit(&df, "y", &["a".to_owned(), "a".to_owned()])
                .unwrap_err()
                .to_string()
                .contains("duplicates")
        );
    }
}
