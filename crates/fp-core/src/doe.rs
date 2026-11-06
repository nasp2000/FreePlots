//! Two-level full-factorial analysis with coded factors (−1/+1).
//!
//! The design must contain numeric factor columns already coded to −1 and +1.
//! All main effects and interactions are entered in one ordinary least squares
//! model; terms are fitted simultaneously, not one at a time. Center points and
//! incomplete factorials are outside this implementation.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::regression::{self, RegressionFit};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::SplitMix64;
use crate::workflow::AnalysisOutput;

/// Fitted two-level factorial.
#[derive(Debug, Clone, PartialEq)]
pub struct FactorialFit {
    /// Term names including `Constant`.
    pub names: Vec<String>,
    /// Coefficients (coded scale).
    pub coefficients: Vec<f64>,
    /// Effects: twice the coefficient for every non-intercept term.
    pub effects: Vec<f64>,
    /// Standard errors of the effects.
    pub effect_se: Vec<f64>,
    /// t statistics for the terms.
    pub t_values: Vec<f64>,
    /// p-values for the terms.
    pub p_values: Vec<f64>,
    /// Regression fit used for the model summary and residual plot.
    pub regression: RegressionFit,
    /// Original rows excluded.
    pub excluded: usize,
    /// Block effect, when the design is blocked.
    pub blocks: Option<BlockFit>,
    /// Number of center points (the rows with all the factors at zero).
    pub center_points: usize,
    /// Pure error sum of squares from the center points.
    pub pure_error_ss: f64,
    /// Pure error degrees of freedom from the center points.
    pub pure_error_df: f64,
    /// Sequential (Type I) sums of squares, in the order the terms are added.
    pub sequential: Vec<SequentialTerm>,
}

/// A term of the sequential (Type I) decomposition.
#[derive(Debug, Clone, PartialEq)]
pub struct SequentialTerm {
    /// Name of the term.
    pub name: String,
    /// Sum of squares added by the term.
    pub ss: f64,
    /// Mean square of the term.
    pub ms: f64,
    /// F statistic of the sequential test.
    pub f: f64,
    /// p-value of the sequential test.
    pub p_value: f64,
}

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

fn term_name(factors: &[String], mask: usize) -> String {
    let mut parts = Vec::new();
    for (index, name) in factors.iter().enumerate() {
        if mask & (1 << index) != 0 {
            parts.push(name.clone());
        }
    }
    parts.join("*")
}

/// Block effect of a blocked factorial fit.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockFit {
    /// Name of the block column.
    pub column: String,
    /// Block levels in the order they appear in the worksheet.
    pub levels: Vec<String>,
    /// Response mean per level.
    pub means: Vec<f64>,
    /// Number of observations per level.
    pub counts: Vec<usize>,
    /// Degrees of freedom (levels minus one).
    pub df: usize,
    /// Sum of squares of the block effect.
    pub ss: f64,
    /// Mean square of the block effect.
    pub ms: f64,
    /// F statistic against the error term.
    pub f: f64,
    /// p-value of the F statistic.
    pub p_value: f64,
}

/// Fits a full two-level factorial model.
pub fn fit(df: &DataFrame, response: &str, factors: &[String]) -> Result<FactorialFit> {
    fit_with_blocks(df, response, factors, None)
}

/// Fits a full two-level factorial model with an optional block effect.
///
/// The block column is read as a categorical factor with fixed (dummy coded)
/// levels: the first level is the reference and every other level adds one
/// column to the model. The factorial effects are then estimated with the
/// block effect removed from the response.
pub fn fit_with_blocks(
    df: &DataFrame,
    response: &str,
    factors: &[String],
    block_column: Option<&str>,
) -> Result<FactorialFit> {
    if factors.is_empty() {
        return Err(Error::Unsupported(
            "the factorial analysis needs at least one factor".to_owned(),
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
    if factors.len() > 6 {
        return Err(Error::Unsupported(
            "at most six factors are supported by this implementation".to_owned(),
        ));
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
    let factor_columns = factors
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    for (name, column) in factors.iter().zip(&factor_columns) {
        if matches!(
            column.data(),
            ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
        ) {
            return Err(Error::Csv(format!(
                "factor column {name} is not numeric and coded −1/+1"
            )));
        }
    }
    let block = match block_column {
        Some(name) => {
            if name == response || factors.iter().any(|factor| factor == name) {
                return Err(Error::Unsupported(
                    "the block column must be different from the response and the factors"
                        .to_owned(),
                ));
            }
            Some((name.to_owned(), df.column(name)?))
        }
        None => None,
    };

    let term_count = (1usize << factors.len()) - 1;
    let mut terms: Vec<(usize, String)> = (1..=term_count)
        .map(|mask| (mask.count_ones() as usize, term_name(factors, mask)))
        .collect();
    terms.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let term_names: Vec<String> = terms.iter().map(|(_, name)| name.clone()).collect();
    let masks: Vec<usize> = terms
        .iter()
        .map(|(_, name)| {
            (1..=term_count)
                .find(|mask| term_name(factors, *mask) == *name)
                .expect("term was generated")
        })
        .collect();

    let mut response_values = Vec::new();
    let mut center_responses: Vec<f64> = Vec::new();
    let mut factor_values: Vec<Vec<f64>> = vec![Vec::new(); factors.len()];
    let mut block_labels: Vec<String> = Vec::new();
    for row in 0..df.n_rows() {
        let Some(value) = cell(response_column, row) else {
            continue;
        };
        let mut values = Vec::with_capacity(factors.len());
        let mut complete = true;
        let mut has_zero = false;
        for column in &factor_columns {
            match cell(column, row) {
                Some(factor_value) if (factor_value.abs() - 1.0).abs() < 1e-9 => {
                    values.push(factor_value);
                }
                Some(factor_value) if factor_value.abs() < 1e-9 => {
                    // A zero is only allowed in a center point, where every
                    // factor is zero.
                    has_zero = true;
                    values.push(0.0);
                }
                Some(factor_value) => {
                    return Err(Error::Cell {
                        column: column.name().to_owned(),
                        row: row + 1,
                        message: format!(
                            "factor values must be coded −1/+1 (found {factor_value})"
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
            continue;
        }
        if has_zero {
            if values.iter().all(|value| value.abs() < 1e-9) {
                // A center point carries no factorial information: it only
                // contributes to the pure error.
                center_responses.push(value);
                continue;
            }
            return Err(Error::Cell {
                column: factor_columns[0].name().to_owned(),
                row: row + 1,
                message: "the design mixes center points with factorial points".to_owned(),
            });
        }
        let label = match &block {
            Some((_, column)) => match label_of(column, row) {
                Some(label) => label,
                None => continue,
            },
            None => String::new(),
        };
        response_values.push(value);
        block_labels.push(label);
        for (index, value) in values.into_iter().enumerate() {
            factor_values[index].push(value);
        }
    }
    let n = response_values.len();
    let block_levels: Vec<String> = {
        let mut levels: Vec<String> = Vec::new();
        for label in &block_labels {
            if !levels.contains(label) {
                levels.push(label.clone());
            }
        }
        levels
    };
    let minimum = term_count + 1 + block_levels.len().saturating_sub(1);
    if n < minimum {
        return Err(Error::SampleTooSmall { n, min: minimum });
    }

    let mut columns = Vec::with_capacity(term_names.len() + block_levels.len() + 1);
    columns.push(Column::new_f64(response, response_values.clone()));
    let mut predictor_names: Vec<String> = Vec::new();
    for level in block_levels.iter().skip(1) {
        let name = format!("Block[{level}]");
        let values: Vec<f64> = block_labels
            .iter()
            .map(|label| if label == level { 1.0 } else { 0.0 })
            .collect();
        columns.push(Column::new_f64(name.clone(), values));
        predictor_names.push(name);
    }
    for (name, mask) in term_names.iter().zip(&masks) {
        let mut values = Vec::with_capacity(n);
        for row in 0..n {
            let mut product = 1.0;
            for (index, factor) in factor_values.iter().enumerate() {
                if mask & (1 << index) != 0 {
                    product *= factor[row];
                }
            }
            values.push(product);
        }
        columns.push(Column::new_f64(name.clone(), values));
        predictor_names.push(name.clone());
    }
    let temp = DataFrame::new(columns)?;

    // The sequential (Type I) decomposition: the terms are added one by one
    // and each one contributes the sum of squares it explains beyond the
    // previous terms. The block terms are always in the model.
    let mut sequential: Vec<SequentialTerm> = Vec::new();
    let block_terms = block_levels.len().saturating_sub(1);
    let mut cumulative: Vec<String> = predictor_names[..block_terms].to_vec();
    // The intercept-only model is the starting point of the decomposition.
    let mean = response_values.iter().sum::<f64>() / n as f64;
    let mut previous_ss = 0.0;
    let mut previous_residual_ss: f64 = response_values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum();
    let mut previous_error_df = n as f64 - 1.0;
    if !cumulative.is_empty() {
        let model = regression::fit_saturated(&temp, response, &cumulative, 0.95)?;
        previous_ss = model.regression_ss;
        previous_residual_ss = model.residual_ss;
        previous_error_df = model.df_residual;
    }
    let mut order: Vec<usize> = (0..masks.len()).collect();
    order.sort_by_key(|&index| masks[index]);
    for index in order {
        let name = term_names[index].clone();
        cumulative.push(name.clone());
        let model = regression::fit_saturated(&temp, response, &cumulative, 0.95)?;
        let ss = (model.regression_ss - previous_ss).max(0.0);
        // The sequential test uses the error of the reduced model.
        let error_df = previous_error_df;
        let error_ms = if error_df > 0.0 {
            previous_residual_ss / error_df
        } else {
            f64::NAN
        };
        let f = if error_ms > 0.0 && error_ms.is_finite() {
            ss / error_ms
        } else {
            f64::NAN
        };
        let p_value = if f.is_finite() {
            (1.0 - crate::special::f_cdf(f, 1.0, error_df)).clamp(0.0, 1.0)
        } else {
            f64::NAN
        };
        sequential.push(SequentialTerm {
            name,
            ss,
            ms: ss,
            f,
            p_value,
        });
        previous_ss = model.regression_ss;
        previous_residual_ss = model.residual_ss;
        previous_error_df = model.df_residual;
    }

    let regression = regression::fit_saturated(&temp, response, &predictor_names, 0.95)?;
    let names = regression.names.clone();
    let coefficients = regression.coefficients.clone();
    let block_terms = block_levels.len().saturating_sub(1);
    let effects: Vec<f64> = coefficients
        .iter()
        .enumerate()
        .map(|(index, coefficient)| {
            if index == 0 || index <= block_terms {
                *coefficient
            } else {
                2.0 * coefficient
            }
        })
        .collect();
    // The center points give a pure error estimate.
    let pure_error_ss = if center_responses.len() > 1 {
        let mean = center_responses.iter().sum::<f64>() / center_responses.len() as f64;
        center_responses
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum()
    } else {
        0.0
    };
    let pure_error_df = center_responses.len().saturating_sub(1) as f64;

    // The effect tests pool the within-cell residual with the pure error of
    // the center points: a saturated factorial has no residual degrees of
    // freedom of its own, but the center points still estimate the error.
    let error_ss = regression.residual_ss + pure_error_ss;
    let error_df = regression.df_residual + pure_error_df;
    let error_ms = if error_df > 0.0 {
        error_ss / error_df
    } else {
        f64::NAN
    };
    let effect_se: Vec<f64> = (0..names.len())
        .map(|index| {
            if index == 0 || index <= block_terms {
                regression.standard_errors[index]
            } else if error_ms.is_finite() && error_ms > 0.0 {
                2.0 * (error_ms / n as f64).sqrt()
            } else {
                f64::NAN
            }
        })
        .collect();
    let t_values: Vec<f64> = effects
        .iter()
        .zip(&effect_se)
        .map(|(effect, se)| if *se > 0.0 { effect / se } else { f64::NAN })
        .collect();
    let p_values: Vec<f64> = t_values
        .iter()
        .map(|t| {
            if t.is_finite() && error_df > 0.0 {
                (1.0 - crate::special::f_cdf(t * t, 1.0, error_df)).clamp(0.0, 1.0)
            } else {
                f64::NAN
            }
        })
        .collect();

    let blocks = match &block {
        None => None,
        Some((name, _)) => {
            let grand_mean = response_values.iter().sum::<f64>() / n as f64;
            let mut means = Vec::with_capacity(block_levels.len());
            let mut counts = Vec::with_capacity(block_levels.len());
            let mut ss = 0.0;
            for level in &block_levels {
                let values: Vec<f64> = response_values
                    .iter()
                    .zip(&block_labels)
                    .filter(|(_, label)| *label == level)
                    .map(|(value, _)| *value)
                    .collect();
                let count = values.len();
                let mean = if count == 0 {
                    0.0
                } else {
                    values.iter().sum::<f64>() / count as f64
                };
                ss += count as f64 * (mean - grand_mean).powi(2);
                means.push(mean);
                counts.push(count);
            }
            let df = block_levels.len().saturating_sub(1);
            let ms = if df == 0 { 0.0 } else { ss / df as f64 };
            let error_df = regression.df_residual;
            let error_ms = if error_df > 0.0 {
                regression.residual_ss / error_df
            } else {
                f64::NAN
            };
            let f = if error_ms > 0.0 {
                ms / error_ms
            } else {
                f64::NAN
            };
            let p_value = if f.is_finite() {
                1.0 - crate::special::f_cdf(f, df as f64, error_df as f64)
            } else {
                f64::NAN
            };
            Some(BlockFit {
                column: name.clone(),
                levels: block_levels.clone(),
                means,
                counts,
                df,
                ss,
                ms,
                f,
                p_value,
            })
        }
    };

    Ok(FactorialFit {
        names,
        coefficients,
        effects,
        effect_se,
        t_values,
        p_values,
        regression,
        excluded: df.n_rows() - n - center_responses.len(),
        blocks,
        center_points: center_responses.len(),
        pure_error_ss,
        pure_error_df,
        sequential,
    })
}

/// Reads a block label from a column of any type.
fn label_of(column: &Column, row: usize) -> Option<String> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => values
            .get(row)
            .copied()
            .filter(|value| value.is_finite())
            .map(|value| format!("{value}")),
        ColumnData::I64(values) => values.get(row).map(|value| value.to_string()),
        ColumnData::Text(values) => values.get(row).cloned(),
        ColumnData::Bool(values) => values.get(row).map(|&value| {
            if value {
                "T".to_owned()
            } else {
                "F".to_owned()
            }
        }),
        ColumnData::Date(values) => values
            .get(row)
            .map(|&value| crate::data::format_date(value)),
    }
}

/// Fits the factorial, builds the report and reuses the OLS residual plot.
pub fn analyze(df: &DataFrame, response: &str, factors: &[String]) -> Result<AnalysisOutput> {
    analyze_with_blocks(df, response, factors, None)
}

/// Fits the factorial with an optional block effect and builds the report.
pub fn analyze_with_blocks(
    df: &DataFrame,
    response: &str,
    factors: &[String],
    block_column: Option<&str>,
) -> Result<AnalysisOutput> {
    let model = fit_with_blocks(df, response, factors, block_column)?;
    let plot = regression::residual_plot(response, &model.regression);
    Ok(AnalysisOutput {
        results: vec![report(response, factors, &model)],
        plot: Some(plot),
    })
}

fn report(response: &str, factors: &[String], model: &FactorialFit) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("2-Level Factorial — response {response}"),
        "Full factorial, coded factors (−1/+1); OLS with all main effects and interactions",
    );
    result.n = model.regression.n;
    result.excluded = model.excluded;
    let regression = &model.regression;
    // The error of the tests pools the within-cell residual with the pure
    // error of the center points.
    let error_ss = regression.residual_ss + model.pure_error_ss;
    let error_df = regression.df_residual + model.pure_error_df;
    let error_ms = if error_df > 0.0 {
        error_ss / error_df
    } else {
        f64::NAN
    };
    let (model_df, model_ss) = match &model.blocks {
        Some(blocks) => (
            regression.df_regression - blocks.df as f64,
            regression.total_ss - blocks.ss - regression.residual_ss,
        ),
        None => (regression.df_regression, regression.regression_ss),
    };
    let model_ms = if model_df > 0.0 {
        model_ss / model_df
    } else {
        f64::NAN
    };
    let model_f = if error_ms > 0.0 {
        model_ms / error_ms
    } else {
        f64::NAN
    };
    let model_p = if model_f.is_finite() {
        1.0 - crate::special::f_cdf(model_f, model_df, error_df)
    } else {
        f64::NAN
    };
    result.statistics = vec![
        ("R²", regression.r_squared),
        ("Adjusted R²", regression.adjusted_r_squared),
        (
            "Residual standard error",
            if error_ms.is_finite() {
                error_ms.sqrt()
            } else {
                regression.s
            },
        ),
        ("F", model_f),
        ("F p-value", model_p),
        ("Terms", (model.names.len() - 1) as f64),
        ("Center points", model.center_points as f64),
    ];
    result
        .extra
        .push(Item::Message(format!("Factors: {}", factors.join(", "))));
    result.extra.push(Item::Table {
        headers: vec![
            "Term".to_owned(),
            "Effect".to_owned(),
            "Coef".to_owned(),
            "SE Effect".to_owned(),
            "T".to_owned(),
            "P".to_owned(),
        ],
        rows: model
            .names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                vec![
                    name.clone(),
                    format!("{:.6}", model.effects[index]),
                    format!("{:.6}", model.coefficients[index]),
                    format!("{:.6}", model.effect_se[index]),
                    format!("{:.6}", model.t_values[index]),
                    format!("{:.6}", model.p_values[index]),
                ]
            })
            .collect(),
    });
    let mut anova = Vec::new();
    if let Some(blocks) = &model.blocks {
        anova.push(vec![
            "Blocks".to_owned(),
            format!("{:.0}", blocks.df),
            format!("{:.6}", blocks.ss),
            format!("{:.6}", blocks.ms),
            format!("{:.6}", blocks.f),
            format!("{:.6}", blocks.p_value),
        ]);
    }
    anova.push(vec![
        if model.blocks.is_some() {
            "Factorial model".to_owned()
        } else {
            "Model".to_owned()
        },
        format!("{model_df:.0}"),
        format!("{model_ss:.6}"),
        format!("{model_ms:.6}"),
        format!("{model_f:.6}"),
        format!("{model_p:.6}"),
    ]);
    if model.center_points > 0 {
        anova.push(vec![
            "Pure error".to_owned(),
            format!("{:.0}", model.pure_error_df),
            format!("{:.6}", model.pure_error_ss),
            if model.pure_error_df > 0.0 {
                format!("{:.6}", model.pure_error_ss / model.pure_error_df)
            } else {
                String::new()
            },
            String::new(),
            String::new(),
        ]);
    }
    anova.push(vec![
        "Error".to_owned(),
        format!("{error_df:.0}"),
        format!("{error_ss:.6}"),
        format!("{error_ms:.6}"),
        String::new(),
        String::new(),
    ]);
    anova.push(vec![
        "Total".to_owned(),
        format!("{:.0}", regression.df_regression + error_df),
        format!("{:.6}", regression.total_ss + model.pure_error_ss),
        String::new(),
        String::new(),
        String::new(),
    ]);
    result.extra.push(Item::Table {
        headers: vec![
            "Source".to_owned(),
            "DF".to_owned(),
            "SS".to_owned(),
            "MS".to_owned(),
            "F".to_owned(),
            "P".to_owned(),
        ],
        rows: anova,
    });
    if !model.sequential.is_empty() {
        result.extra.push(Item::Table {
            headers: vec![
                "Term".to_owned(),
                "Seq SS".to_owned(),
                "Seq F".to_owned(),
                "Seq P".to_owned(),
            ],
            rows: model
                .sequential
                .iter()
                .map(|term| {
                    vec![
                        term.name.clone(),
                        format!("{:.6}", term.ss),
                        if term.f.is_finite() {
                            format!("{:.6}", term.f)
                        } else {
                            String::new()
                        },
                        if term.p_value.is_finite() {
                            format!("{:.6}", term.p_value)
                        } else {
                            String::new()
                        },
                    ]
                })
                .collect(),
        });
    }
    if let Some(blocks) = &model.blocks {
        result.extra.push(Item::Table {
            headers: vec![
                "Block".to_owned(),
                "Level".to_owned(),
                "Count".to_owned(),
                "Mean".to_owned(),
            ],
            rows: blocks
                .levels
                .iter()
                .enumerate()
                .map(|(index, level)| {
                    vec![
                        blocks.column.clone(),
                        level.clone(),
                        blocks.counts[index].to_string(),
                        format!("{:.6}", blocks.means[index]),
                    ]
                })
                .collect(),
        });
    }
    result.notes = vec![
        (
            "Coding".to_owned(),
            "factors must already be coded to low = −1 and high = +1".to_owned(),
        ),
        (
            "Model".to_owned(),
            "all main effects and interactions are fitted simultaneously; terms are not \
             sequential sums of squares"
                .to_owned(),
        ),
        (
            "Replication".to_owned(),
            "the error pools the within-cell residual with the pure error of the center \
             points; a design without replication or center points has no error estimate"
                .to_owned(),
        ),
        (
            "Scope".to_owned(),
            if model.blocks.is_some() {
                "no center points, randomisation or incomplete factorials; the block effect is \
                 fitted as a fixed (dummy coded) effect"
                    .to_owned()
            } else {
                "no randomisation or incomplete factorials".to_owned()
            },
        ),
    ];
    result.warnings = standard_warnings(model.regression.n, model.excluded, 0);
    result.warnings.push(
        "factorial effect conventions and inference have not been verified against documented \
         documented output"
            .to_owned(),
    );
    result
}

/// Generates a full two-level factorial design.
///
/// The returned frame contains `StdOrder`, `RunOrder`, one coded −1/+1 column
/// per factor and, optionally, center points (all factor values zero). When
/// `randomize` is true, the run order is shuffled deterministically with
/// `seed`.
pub fn full_factorial_design(
    factors: &[String],
    replicates: usize,
    center_points: usize,
    randomize: bool,
    seed: u64,
) -> Result<DataFrame> {
    if factors.is_empty() || factors.len() > 6 {
        return Err(Error::Unsupported(
            "the design needs between one and six factors".to_owned(),
        ));
    }
    let mut unique = factors.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != factors.len() {
        return Err(Error::Unsupported("factor names must be unique".to_owned()));
    }
    if replicates == 0 {
        return Err(Error::InvalidParameter {
            name: "replicates",
            value: 0.0,
        });
    }
    let k = factors.len();
    let combinations = 1usize << k;
    let total = combinations
        .checked_mul(replicates)
        .and_then(|value| value.checked_add(center_points))
        .ok_or_else(|| Error::Unsupported("the design is too large".to_owned()))?;
    if total > 1_000_000 {
        return Err(Error::Unsupported(
            "the design exceeds one million runs".to_owned(),
        ));
    }

    let mut std_order = Vec::with_capacity(total);
    let mut runs: Vec<Vec<f64>> = Vec::with_capacity(total);
    let mut order = 1usize;
    for mask in 0..combinations {
        for _ in 0..replicates {
            std_order.push(order as f64);
            order += 1;
            runs.push(
                (0..k)
                    .map(|index| if mask & (1 << index) != 0 { 1.0 } else { -1.0 })
                    .collect(),
            );
        }
    }
    for _ in 0..center_points {
        std_order.push(order as f64);
        order += 1;
        runs.push(vec![0.0; k]);
    }

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

    let mut columns = Vec::with_capacity(k + 2);
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

/// Axial distance for a central composite design.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CcdAlpha {
    /// Rotatable design: `alpha = (2^k)^(1/4)`.
    #[default]
    Rotatable,
    /// Face-centered design: `alpha = 1`.
    FaceCentered,
    /// Explicit positive axial distance.
    Custom(f64),
}

impl CcdAlpha {
    /// Axial distance for `k` factors.
    #[must_use]
    pub fn value(self, k: usize) -> f64 {
        match self {
            Self::Rotatable => (2f64.powi(k as i32)).powf(0.25),
            Self::FaceCentered => 1.0,
            Self::Custom(value) => value,
        }
    }

    /// Parses `rotatable`, `face` or a positive number.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "rotatable" | "rot" => Some(Self::Rotatable),
            "face" | "face-centered" | "face_centered" => Some(Self::FaceCentered),
            other => other
                .parse::<f64>()
                .ok()
                .filter(|value| *value > 0.0)
                .map(Self::Custom),
        }
    }
}

/// Generates a central composite design.
///
/// Runs: factorial points (`2^k`), axial points (`2k`) and center points.
/// Factor values are coded; the response is not part of the design.
pub fn central_composite_design(
    factors: &[String],
    center_points: usize,
    alpha: CcdAlpha,
    randomize: bool,
    seed: u64,
) -> Result<DataFrame> {
    if factors.len() < 2 || factors.len() > 5 {
        return Err(Error::Unsupported(
            "a central composite design needs between two and five factors".to_owned(),
        ));
    }
    if center_points == 0 {
        return Err(Error::InvalidParameter {
            name: "center points",
            value: 0.0,
        });
    }
    let mut unique = factors.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != factors.len() {
        return Err(Error::Unsupported("factor names must be unique".to_owned()));
    }
    let axial = alpha.value(factors.len());
    if !axial.is_finite() || axial <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "axial distance",
            value: axial,
        });
    }
    let k = factors.len();
    let factorial = 1usize << k;
    let total = factorial + 2 * k + center_points;
    if total > 1_000_000 {
        return Err(Error::Unsupported(
            "the design exceeds one million runs".to_owned(),
        ));
    }

    let mut runs: Vec<Vec<f64>> = Vec::with_capacity(total);
    for mask in 0..factorial {
        runs.push(
            (0..k)
                .map(|index| if mask & (1 << index) != 0 { 1.0 } else { -1.0 })
                .collect(),
        );
    }
    for index in 0..k {
        let mut plus = vec![0.0; k];
        plus[index] = axial;
        runs.push(plus);
        let mut minus = vec![0.0; k];
        minus[index] = -axial;
        runs.push(minus);
    }
    for _ in 0..center_points {
        runs.push(vec![0.0; k]);
    }

    let mut std_order: Vec<f64> = (1..=total).map(|value| value as f64).collect();
    let mut run_order = std_order.clone();
    if randomize {
        let mut rng = SplitMix64::new(seed);
        for index in (1..total).rev() {
            let swap = (rng.next_u64() % (index as u64 + 1)) as usize;
            std_order.swap(index, swap);
            runs.swap(index, swap);
            run_order.swap(index, swap);
        }
    }

    let mut columns = Vec::with_capacity(k + 2);
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

/// Generates a **Box–Behnken** design for three to seven factors.
///
/// Every pair of factors is measured at the four corners of a square with the
/// remaining factors held at the centre, so the design estimates all the
/// quadratic terms without the extreme corners of a full factorial. The
/// standard construction is `4·C(k,2) + centre` runs.
pub fn box_behnken_design(
    factors: &[String],
    center_points: usize,
    randomize: bool,
    seed: u64,
) -> Result<DataFrame> {
    if factors.len() < 3 || factors.len() > 7 {
        return Err(Error::Unsupported(
            "a Box-Behnken design needs between three and seven factors".to_owned(),
        ));
    }
    if center_points == 0 {
        return Err(Error::InvalidParameter {
            name: "center points",
            value: 0.0,
        });
    }
    let mut unique = factors.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != factors.len() {
        return Err(Error::Unsupported("factor names must be unique".to_owned()));
    }
    let k = factors.len();
    let pairs = k * (k - 1) / 2;
    let total = 4 * pairs + center_points;
    if total > 1_000_000 {
        return Err(Error::Unsupported(
            "the design exceeds one million runs".to_owned(),
        ));
    }

    let mut runs: Vec<Vec<f64>> = Vec::with_capacity(total);
    for first in 0..k {
        for second in (first + 1)..k {
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let mut run = vec![0.0; k];
                run[first] = a;
                run[second] = b;
                runs.push(run);
            }
        }
    }
    for _ in 0..center_points {
        runs.push(vec![0.0; k]);
    }

    let mut std_order: Vec<f64> = (1..=total).map(|value| value as f64).collect();
    let mut run_order = std_order.clone();
    if randomize {
        let mut rng = SplitMix64::new(seed);
        for index in (1..total).rev() {
            let swap = (rng.next_u64() % (index as u64 + 1)) as usize;
            std_order.swap(index, swap);
            runs.swap(index, swap);
            run_order.swap(index, swap);
        }
    }

    let mut columns = Vec::with_capacity(k + 2);
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

/// Hadamard matrix of order `order` through the **Paley construction**.
///
/// With `q = order − 1` a prime congruent to 3 modulo 4, the matrix
///
/// ```text
/// H = [ 1   1ᵀ      ]
///     [ 1   Q − I   ]
/// ```
///
/// is Hadamard, where `Q[i][j]` is the quadratic character `χ(j − i)` modulo
/// `q`. Returns `None` when the order does not have that form.
fn paley_hadamard(order: usize) -> Option<Vec<Vec<f64>>> {
    if order < 4 {
        return None;
    }
    let q = order - 1;
    // q must be a prime congruent to 3 modulo 4.
    if q % 4 != 3 || (2..q).any(|divisor| q.is_multiple_of(divisor)) {
        return None;
    }
    let residues: Vec<i64> = (1..q as i64)
        .map(|value| (value * value) % q as i64)
        .collect();
    let character = |value: i64| -> f64 {
        let value = value.rem_euclid(q as i64);
        if value == 0 {
            0.0
        } else if residues.contains(&value) {
            1.0
        } else {
            -1.0
        }
    };
    let mut matrix = vec![vec![1.0; order]; order];
    for (row, values) in matrix.iter_mut().enumerate().skip(1) {
        for (column, value) in values.iter_mut().enumerate().skip(1) {
            *value = if column == row {
                -1.0
            } else {
                character(column as i64 - row as i64)
            };
        }
    }
    Some(matrix)
}

/// Generates a **Plackett–Burman** screening design for `runs` runs.
///
/// The design comes from a Hadamard matrix of order `runs` (Paley
/// construction), dropping the first (all +1) column, so it has `runs − 1`
/// factors and every pair of factors is orthogonal. Supported run counts are
/// the orders with a prime `runs − 1 ≡ 3 (mod 4)`: 8, 12, 20, 24, 32, 44, …
pub fn plackett_burman_design(runs: usize, randomize: bool, seed: u64) -> Result<DataFrame> {
    let Some(matrix) = paley_hadamard(runs) else {
        return Err(Error::Unsupported(format!(
            "a Plackett-Burman design of {runs} runs is not available; try 8, 12, 20, 24 or 32"
        )));
    };
    let factors = runs - 1;
    // A..Z, then A1..Z1, A2..Z2 for the wider designs.
    let names: Vec<String> = (0..factors)
        .map(|index| {
            let letter = (b'A' + (index % 26) as u8) as char;
            match index / 26 {
                0 => letter.to_string(),
                cycle => format!("{letter}{cycle}"),
            }
        })
        .collect();
    let mut rows: Vec<Vec<f64>> = matrix.iter().map(|row| row[1..].to_vec()).collect();

    let mut std_order: Vec<f64> = (1..=runs).map(|value| value as f64).collect();
    let mut run_order = std_order.clone();
    if randomize {
        let mut rng = SplitMix64::new(seed);
        for index in (1..runs).rev() {
            let swap = (rng.next_u64() % (index as u64 + 1)) as usize;
            std_order.swap(index, swap);
            rows.swap(index, swap);
            run_order.swap(index, swap);
        }
    }

    let mut columns = Vec::with_capacity(factors + 2);
    columns.push(Column::new_f64("StdOrder", std_order));
    columns.push(Column::new_f64("RunOrder", run_order));
    for (index, name) in names.iter().enumerate() {
        columns.push(Column::new_f64(
            name.clone(),
            rows.iter().map(|row| row[index]).collect(),
        ));
    }
    DataFrame::new(columns)
}

/// Fits a main-effects-only model for a fractional factorial design.
///
/// The model is `y = β₀ + Σβᵢxᵢ`; interactions are not estimated, so aliasing
/// between main effects and higher-order terms is not resolved by this report.
pub fn analyze_main_effects(
    df: &DataFrame,
    response: &str,
    factors: &[String],
) -> Result<AnalysisOutput> {
    if factors.is_empty() || factors.len() > 6 {
        return Err(Error::Unsupported(
            "the main-effects model needs between one and six factors".to_owned(),
        ));
    }
    let model = regression::fit_saturated(df, response, factors, 0.95)?;
    let plot = regression::residual_plot(response, &model);
    let mut result = AnalysisResult::new(
        format!("Fractional Factorial — response {response}"),
        "Main-effects-only OLS; coded factors; interactions omitted",
    );
    result.n = model.n;
    result.excluded = df.n_rows() - model.n;
    result.statistics = vec![
        ("R²", model.r_squared),
        ("Adjusted R²", model.adjusted_r_squared),
        ("Residual standard error", model.s),
        ("F", model.f),
        ("F p-value", model.f_p_value),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Term".to_owned(),
            "Effect".to_owned(),
            "Coef".to_owned(),
            "SE Effect".to_owned(),
            "T".to_owned(),
            "P".to_owned(),
        ],
        rows: model
            .names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let effect = if index == 0 {
                    model.coefficients[index]
                } else {
                    2.0 * model.coefficients[index]
                };
                let se = if index == 0 {
                    model.standard_errors[index]
                } else {
                    2.0 * model.standard_errors[index]
                };
                vec![
                    name.clone(),
                    format!("{effect:.6}"),
                    format!("{:.6}", model.coefficients[index]),
                    format!("{se:.6}"),
                    format!("{:.6}", model.t_values[index]),
                    format!("{:.6}", model.p_values[index]),
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
        ],
        rows: vec![
            vec![
                "Model".to_owned(),
                format!("{:.0}", model.df_regression),
                format!("{:.6}", model.regression_ss),
                format!("{:.6}", model.regression_ss / model.df_regression),
                format!("{:.6}", model.f),
                format!("{:.6}", model.f_p_value),
            ],
            vec![
                "Error".to_owned(),
                format!("{:.0}", model.df_residual),
                format!("{:.6}", model.residual_ss),
                format!("{:.6}", model.residual_ss / model.df_residual),
                String::new(),
                String::new(),
            ],
            vec![
                "Total".to_owned(),
                format!("{:.0}", model.df_regression + model.df_residual),
                format!("{:.6}", model.total_ss),
                String::new(),
                String::new(),
                String::new(),
            ],
        ],
    });
    result.notes = vec![
        (
            "Coding".to_owned(),
            "factors must be coded to low = −1 and high = +1".to_owned(),
        ),
        (
            "Aliasing".to_owned(),
            "main effects only; interactions and alias chains are not estimated".to_owned(),
        ),
        (
            "Replication".to_owned(),
            "residual degrees of freedom are required; replicate the design or add runs".to_owned(),
        ),
    ];
    result.warnings.push(
        "fractional-factorial alias structure and main-effect conventions have not been verified against documented output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

/// Generates a full factorial design with an optional `Block` column.
///
/// Blocks are assigned to the run order in groups of `block_size` rows.
pub fn blocked_factorial_design(
    factors: &[String],
    replicates: usize,
    center_points: usize,
    block_size: Option<usize>,
    randomize: bool,
    seed: u64,
) -> Result<DataFrame> {
    let design = full_factorial_design(factors, replicates, center_points, randomize, seed)?;
    let Some(block_size) = block_size else {
        return Ok(design);
    };
    if block_size == 0 {
        return Ok(design);
    }
    let rows = design.n_rows();
    let mut columns: Vec<Column> = design.columns().to_vec();
    let blocks: Vec<String> = (0..rows)
        .map(|index| format!("{}", index / block_size + 1))
        .collect();
    columns.push(Column::new_text("Block", blocks));
    DataFrame::new(columns)
}

/// Alias groups of a two-level fractional factorial design.
///
/// Each generator is written `D=ABC`: the word is `D·A·B·C = I`. The alias
/// group of a term contains the term itself and the terms obtained by
/// multiplying it with every generator word.
pub fn fractional_aliases(
    base_factors: &[String],
    generators: &[String],
) -> Result<Vec<Vec<String>>> {
    use std::collections::{BTreeMap, BTreeSet};
    if base_factors.is_empty() {
        return Err(Error::Unsupported(
            "the alias table needs at least one base factor".to_owned(),
        ));
    }
    let mut base: BTreeSet<char> = BTreeSet::new();
    for name in base_factors {
        let name = name.trim();
        let mut chars = name.chars();
        let Some(ch) = chars.next() else {
            return Err(Error::Unsupported(
                "base factor names must be single letters".to_owned(),
            ));
        };
        if chars.next().is_some() || !ch.is_ascii_alphabetic() {
            return Err(Error::Unsupported(
                "base factor names must be single letters".to_owned(),
            ));
        }
        let ch = ch.to_ascii_uppercase();
        if !base.insert(ch) {
            return Err(Error::Unsupported(
                "base factor names must be unique".to_owned(),
            ));
        }
    }
    let mut words: Vec<BTreeSet<char>> = vec![BTreeSet::new()];
    let mut factors: BTreeSet<char> = base.clone();
    for text in generators {
        let (lhs, rhs) = text.split_once('=').ok_or_else(|| {
            Error::Unsupported(format!("generator {text} must have the form D=ABC"))
        })?;
        let lhs = lhs.trim().chars().next().ok_or_else(|| {
            Error::Unsupported(format!("generator {text} must have the form D=ABC"))
        })?;
        let lhs = lhs.to_ascii_uppercase();
        if !lhs.is_ascii_alphabetic() || base.contains(&lhs) {
            return Err(Error::Unsupported(format!(
                "generator {text} must define a new factor"
            )));
        }
        if !factors.insert(lhs) {
            return Err(Error::Unsupported(format!(
                "generator {text} redefines factor {lhs}"
            )));
        }
        let mut word: BTreeSet<char> = BTreeSet::new();
        word.insert(lhs);
        for ch in rhs
            .trim()
            .chars()
            .filter(|ch| !ch.is_whitespace() && *ch != '*')
        {
            let ch = ch.to_ascii_uppercase();
            if !base.contains(&ch) {
                return Err(Error::Unsupported(format!(
                    "generator {text} uses {ch}, which is not a base factor"
                )));
            }
            if !word.insert(ch) {
                return Err(Error::Unsupported(format!(
                    "generator {text} repeats factor {ch}"
                )));
            }
        }
        words.push(word);
    }
    let mut groups: BTreeMap<Vec<char>, Vec<String>> = BTreeMap::new();
    for &term in &factors {
        let mut group: BTreeSet<char> = BTreeSet::from([term]);
        for word in &words {
            let mut product: BTreeSet<char> = BTreeSet::from([term]);
            for ch in word {
                if !product.remove(ch) {
                    product.insert(*ch);
                }
            }
            group.extend(product);
        }
        let key: Vec<char> = group.iter().copied().collect();
        let entry = groups.entry(key).or_default();
        let name = term.to_string();
        if !entry.contains(&name) {
            entry.push(name);
        }
    }
    Ok(groups
        .into_values()
        .filter(|members| members.len() > 1)
        .collect())
}

/// Generates a two-level fractional factorial design from generators.
///
/// `base_factors` are the fully crossed factors (single letters), and each
/// generator has the form `"D=ABC"`: the new factor is the product of the
/// listed base factors. Replicates are allowed; center points are not.
pub fn fractional_factorial_design(
    base_factors: &[String],
    generators: &[String],
    replicates: usize,
    randomize: bool,
    seed: u64,
) -> Result<DataFrame> {
    if replicates == 0 {
        return Err(Error::InvalidParameter {
            name: "replicates",
            value: 0.0,
        });
    }
    let (base, generated) = parse_fractional(base_factors, generators)?;
    let mut runs = fractional_runs(&base, &generated, replicates)?;
    let total = runs.len();
    let mut std_order: Vec<f64> = (1..=total).map(|value| value as f64).collect();
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
    let mut names: Vec<String> = base.iter().map(char::to_string).collect();
    names.extend(generated.iter().map(|(name, _)| name.to_string()));
    let mut columns = Vec::with_capacity(names.len() + 2);
    columns.push(Column::new_f64("StdOrder", std_order));
    columns.push(Column::new_f64("RunOrder", run_order));
    for (index, name) in names.iter().enumerate() {
        columns.push(Column::new_f64(
            name.clone(),
            runs.iter().map(|run| run[index]).collect(),
        ));
    }
    DataFrame::new(columns)
}

/// A generated factor and the base factors that define it.
type GeneratedFactor = (char, Vec<char>);

/// Validates the base factors and the generators of a fractional design.
fn parse_fractional(
    base_factors: &[String],
    generators: &[String],
) -> Result<(Vec<char>, Vec<GeneratedFactor>)> {
    if base_factors.is_empty() || base_factors.len() > 5 {
        return Err(Error::Unsupported(
            "the fractional design needs between one and five base factors".to_owned(),
        ));
    }
    let mut base: Vec<char> = Vec::new();
    for name in base_factors {
        let name = name.trim();
        if name.len() != 1 || !name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            return Err(Error::Unsupported(
                "base factor names must be single letters".to_owned(),
            ));
        }
        let ch = name
            .chars()
            .next()
            .expect("single letter")
            .to_ascii_uppercase();
        if base.contains(&ch) {
            return Err(Error::Unsupported(
                "base factor names must be unique".to_owned(),
            ));
        }
        base.push(ch);
    }
    let mut generated: Vec<(char, Vec<char>)> = Vec::new();
    for text in generators {
        let (lhs, rhs) = text.split_once('=').ok_or_else(|| {
            Error::Unsupported(format!("generator {text} must have the form D=ABC"))
        })?;
        let lhs = lhs.trim();
        let rhs = rhs.trim();
        if lhs.len() != 1 || !lhs.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            return Err(Error::Unsupported(
                "generator left-hand sides must be single letters".to_owned(),
            ));
        }
        let lhs = lhs
            .chars()
            .next()
            .expect("single letter")
            .to_ascii_uppercase();
        if base.contains(&lhs) || generated.iter().any(|(name, _)| *name == lhs) {
            return Err(Error::Unsupported(format!(
                "generated factor {lhs} duplicates an existing factor"
            )));
        }
        let mut terms = Vec::new();
        for ch in rhs.chars().filter(|ch| !ch.is_whitespace() && *ch != '*') {
            let ch = ch.to_ascii_uppercase();
            if !base.contains(&ch) {
                return Err(Error::Unsupported(format!(
                    "generator {text} uses {ch}, which is not a base factor"
                )));
            }
            if terms.contains(&ch) {
                return Err(Error::Unsupported(format!(
                    "generator {text} repeats factor {ch}"
                )));
            }
            terms.push(ch);
        }
        if terms.is_empty() {
            return Err(Error::Unsupported(format!(
                "generator {text} has an empty right-hand side"
            )));
        }
        generated.push((lhs, terms));
    }
    if base.len() + generated.len() > 6 {
        return Err(Error::Unsupported(
            "at most six factors are supported".to_owned(),
        ));
    }
    Ok((base, generated))
}

/// Generates the runs of a fractional design in standard order.
fn fractional_runs(
    base: &[char],
    generated: &[GeneratedFactor],
    replicates: usize,
) -> Result<Vec<Vec<f64>>> {
    let k = base.len();
    let factorial = 1usize << k;
    let total = factorial
        .checked_mul(replicates)
        .ok_or_else(|| Error::Unsupported("the design is too large".to_owned()))?;
    if total > 1_000_000 {
        return Err(Error::Unsupported(
            "the design exceeds one million runs".to_owned(),
        ));
    }
    let mut runs: Vec<Vec<f64>> = Vec::with_capacity(total);
    for mask in 0..factorial {
        let base_values: Vec<f64> = (0..k)
            .map(|index| if mask & (1 << index) != 0 { 1.0 } else { -1.0 })
            .collect();
        let mut run = base_values.clone();
        for (_, terms) in generated {
            let mut value = 1.0;
            for term in terms {
                let index = base.iter().position(|ch| ch == term).expect("validated");
                value *= base_values[index];
            }
            run.push(value);
        }
        for _ in 0..replicates {
            runs.push(run.clone());
        }
    }
    Ok(runs)
}

/// Generates a fold-over design from a two-level fractional factorial.
///
/// The base fraction is repeated with the sign of every generated factor
/// reversed (the mirror fraction). The `Fold` column marks the two halves.
/// A single generator folded over rebuilds the full factorial; with more
/// generators the main effects of the generated factors become free from
/// their defining interactions.
pub fn fold_over_design(
    base_factors: &[String],
    generators: &[String],
    randomize: bool,
    seed: u64,
) -> Result<DataFrame> {
    let (base, generated) = parse_fractional(base_factors, generators)?;
    let runs = fractional_runs(&base, &generated, 1)?;
    let half = runs.len();
    let mut names: Vec<String> = base.iter().map(char::to_string).collect();
    names.extend(generated.iter().map(|(name, _)| name.to_string()));
    let mut all_runs = runs.clone();
    for run in &runs {
        let mut mirrored = run.clone();
        for index in base.len()..run.len() {
            mirrored[index] = -run[index];
        }
        all_runs.push(mirrored);
    }
    let total = all_runs.len();
    let mut order: Vec<usize> = (0..total).collect();
    if randomize {
        let mut rng = SplitMix64::new(seed);
        for index in (1..total).rev() {
            let swap = (rng.next_u64() % (index as u64 + 1)) as usize;
            order.swap(index, swap);
        }
    }
    let mut columns = Vec::with_capacity(names.len() + 3);
    columns.push(Column::new_f64(
        "StdOrder",
        order.iter().map(|index| (*index + 1) as f64).collect(),
    ));
    columns.push(Column::new_f64(
        "RunOrder",
        (1..=total).map(|value| value as f64).collect(),
    ));
    for (index, name) in names.iter().enumerate() {
        columns.push(Column::new_f64(
            name.clone(),
            order
                .iter()
                .map(|position| all_runs[*position][index])
                .collect(),
        ));
    }
    columns.push(Column::new_text(
        "Fold",
        order
            .iter()
            .map(|position| {
                if *position < half {
                    "Base".to_owned()
                } else {
                    "Fold".to_owned()
                }
            })
            .collect(),
    ));
    DataFrame::new(columns)
}
