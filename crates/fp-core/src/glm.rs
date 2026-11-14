// SPDX-License-Identifier: MIT
//! General linear model with factors, covariates and two-way interactions.
//!
//! Factors enter with reference coding (the first level is the reference and
//! every other level adds one dummy), covariates enter as they are, and an
//! interaction multiplies the basis columns of its two terms. The expanded
//! model is fitted by ordinary least squares through [`crate::regression`],
//! which also supplies the coefficient table and the residual plot.
//!
//! The ANOVA table reports the sequential (Type I) and adjusted (Type III)
//! sums of squares of every term. Type III drops the columns of a term from
//! the full model and compares the residual sums of squares; the F test uses
//! the residual mean square of the full model.

use crate::data::{Column, DataFrame};
use crate::error::{Error, Result};
use crate::regression::{self, RegressionFit};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::f_cdf;
use crate::workflow::AnalysisOutput;

/// Terms requested for a general linear model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GlmOptions {
    /// Categorical factors, with reference coding.
    pub factors: Vec<String>,
    /// Numeric covariates.
    pub covariates: Vec<String>,
    /// Two-way interactions, each a pair of declared factors/covariates.
    pub interactions: Vec<(String, String)>,
}

/// One model term and the expanded columns that represent it.
#[derive(Debug, Clone, PartialEq)]
pub struct GlmTerm {
    /// Display name (`A`, `x`, `A*x`).
    pub name: String,
    /// Expanded columns (dummies or products).
    pub columns: Vec<String>,
}

/// One row of the GLM analysis of variance.
#[derive(Debug, Clone, PartialEq)]
pub struct AnovaRow {
    /// Source (term, error or total).
    pub source: String,
    /// Degrees of freedom.
    pub df: f64,
    /// Sequential (Type I) sum of squares.
    pub sequential_ss: f64,
    /// Adjusted (Type III) sum of squares.
    pub adjusted_ss: f64,
    /// Adjusted mean square.
    pub adjusted_ms: f64,
    /// F statistic.
    pub f: f64,
    /// p-value.
    pub p: f64,
}

/// Fitted general linear model.
#[derive(Debug, Clone, PartialEq)]
pub struct GlmFit {
    /// Terms, in model order.
    pub terms: Vec<GlmTerm>,
    /// Reference level of every factor.
    pub references: Vec<(String, String)>,
    /// Underlying ordinary least squares fit.
    pub regression: RegressionFit,
    /// Analysis of variance rows, then error and total.
    pub anova: Vec<AnovaRow>,
    /// Rows removed listwise.
    pub excluded: usize,
}

/// Builds the expanded frame, the terms and the reference levels.
fn build(
    df: &DataFrame,
    response: &str,
    options: &GlmOptions,
) -> Result<(DataFrame, Vec<GlmTerm>, Vec<(String, String)>, usize)> {
    if options.factors.is_empty() && options.covariates.is_empty() {
        return Err(Error::Unsupported(
            "choose at least one factor or covariate".to_owned(),
        ));
    }
    let mut declared: Vec<String> = options.factors.clone();
    declared.extend(options.covariates.iter().cloned());
    let mut unique = declared.clone();
    unique.sort();
    unique.dedup();
    if unique.len() != declared.len() {
        return Err(Error::Unsupported("a term is declared twice".to_owned()));
    }
    if declared.iter().any(|name| name == response) {
        return Err(Error::Unsupported(
            "the response cannot also be a term".to_owned(),
        ));
    }
    let mut seen_interactions = Vec::new();
    for (left, right) in &options.interactions {
        let mut pair = [left.clone(), right.clone()];
        pair.sort();
        if seen_interactions.contains(&pair) {
            return Err(Error::Unsupported(
                "an interaction is declared twice".to_owned(),
            ));
        }
        seen_interactions.push(pair);
        if left == right {
            return Err(Error::Unsupported(
                "an interaction needs two different terms".to_owned(),
            ));
        }
        for name in [left, right] {
            if !declared.contains(name) {
                return Err(Error::Unsupported(format!(
                    "the interaction uses {name}, which is not a factor or covariate"
                )));
            }
        }
    }
    let response_column = df.column(response)?;
    let factor_columns: Vec<&Column> = options
        .factors
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    let covariate_columns: Vec<&Column> = options
        .covariates
        .iter()
        .map(|name| {
            let column = df.column(name)?;
            if !matches!(
                column.dtype(),
                crate::data::DType::Float | crate::data::DType::Integer
            ) {
                return Err(Error::Unsupported(format!(
                    "the covariate {name} is not numeric"
                )));
            }
            Ok(column)
        })
        .collect::<Result<Vec<_>>>()?;

    // Rows complete in the response, every factor label and every covariate.
    let mut kept: Vec<usize> = Vec::new();
    for row in 0..df.n_rows() {
        if response_column.value(row).is_none() {
            continue;
        }
        if factor_columns
            .iter()
            .any(|column| column.label(row).is_none())
        {
            continue;
        }
        if covariate_columns
            .iter()
            .any(|column| column.value(row).is_none())
        {
            continue;
        }
        kept.push(row);
    }
    if kept.len() < 3 {
        return Err(Error::SampleTooSmall {
            n: kept.len(),
            min: 3,
        });
    }
    let excluded = df.n_rows() - kept.len();

    // Factor levels and reference coding.
    let mut generated: Vec<(String, Vec<f64>)> = Vec::new();
    let mut terms: Vec<GlmTerm> = Vec::new();
    let mut references = Vec::new();
    let mut basis: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (name, column) in options.factors.iter().zip(&factor_columns) {
        let mut levels: Vec<String> = kept.iter().filter_map(|row| column.label(*row)).collect();
        levels.sort();
        levels.dedup();
        if levels.len() < 2 {
            return Err(Error::Unsupported(format!(
                "the factor {name} needs at least two levels"
            )));
        }
        references.push((name.clone(), levels[0].clone()));
        let mut columns = Vec::new();
        for level in &levels[1..] {
            let column_name = format!("{name}[{level}]");
            let values = kept
                .iter()
                .map(|row| f64::from(u8::from(column.label(*row).as_deref() == Some(level))))
                .collect();
            generated.push((column_name.clone(), values));
            columns.push(column_name);
        }
        basis.insert(name.clone(), columns.clone());
        terms.push(GlmTerm {
            name: name.clone(),
            columns,
        });
    }
    for (name, column) in options.covariates.iter().zip(&covariate_columns) {
        let values = kept
            .iter()
            .map(|row| column.value(*row).unwrap_or(f64::NAN))
            .collect();
        generated.push((name.clone(), values));
        basis.insert(name.clone(), vec![name.clone()]);
        terms.push(GlmTerm {
            name: name.clone(),
            columns: vec![name.clone()],
        });
    }
    for (left, right) in &options.interactions {
        let left_columns = basis
            .get(left)
            .ok_or_else(|| Error::Unsupported(format!("unknown term {left}")))?;
        let right_columns = basis
            .get(right)
            .ok_or_else(|| Error::Unsupported(format!("unknown term {right}")))?;
        let mut columns = Vec::new();
        for first in left_columns {
            for second in right_columns {
                let column_name = format!("{first}*{second}");
                let first_values = generated
                    .iter()
                    .find(|(name, _)| name == first)
                    .map(|(_, values)| values.clone())
                    .ok_or_else(|| Error::Unsupported(format!("unknown column {first}")))?;
                let second_values = generated
                    .iter()
                    .find(|(name, _)| name == second)
                    .map(|(_, values)| values.clone())
                    .ok_or_else(|| Error::Unsupported(format!("unknown column {second}")))?;
                let values = first_values
                    .iter()
                    .zip(&second_values)
                    .map(|(a, b)| a * b)
                    .collect();
                generated.push((column_name.clone(), values));
                columns.push(column_name);
            }
        }
        terms.push(GlmTerm {
            name: format!("{left}*{right}"),
            columns,
        });
    }

    let mut names = vec![response.to_owned()];
    names.extend(generated.iter().map(|(name, _)| name.clone()));
    let mut columns: Vec<Vec<f64>> = Vec::with_capacity(names.len());
    columns.push(
        kept.iter()
            .map(|row| response_column.value(*row).unwrap_or(f64::NAN))
            .collect(),
    );
    columns.extend(generated.into_iter().map(|(_, values)| values));
    let frame = DataFrame::new(
        names
            .into_iter()
            .zip(columns)
            .map(|(name, values)| Column::new_f64(name, values))
            .collect(),
    )?;
    Ok((frame, terms, references, excluded))
}

/// Fits a general linear model.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for an empty or duplicated term list, an
/// interaction that names an undeclared term, a factor with one level or a
/// non-numeric covariate, and the fitting errors of [`crate::regression::fit`].
pub fn fit(df: &DataFrame, response: &str, options: &GlmOptions) -> Result<GlmFit> {
    let (frame, terms, references, excluded) = build(df, response, options)?;
    let all_columns: Vec<String> = terms
        .iter()
        .flat_map(|term| term.columns.iter().cloned())
        .collect();
    let regression = regression::fit(&frame, response, &all_columns, 0.95)?;
    let total_ss = regression.total_ss;
    let error_ss = regression.residual_ss;
    let error_df = regression.df_residual;
    let error_ms = error_ss / error_df;
    let mut anova = Vec::with_capacity(terms.len() + 2);
    let mut previous_ss = total_ss;
    for (index, term) in terms.iter().enumerate() {
        let sequential_columns: Vec<String> = terms[..=index]
            .iter()
            .flat_map(|term| term.columns.iter().cloned())
            .collect();
        let sequential_ss = if sequential_columns.len() == all_columns.len() {
            previous_ss - error_ss
        } else {
            let partial = regression::fit(&frame, response, &sequential_columns, 0.95)?;
            previous_ss - partial.residual_ss
        };
        previous_ss -= sequential_ss;
        let reduced_columns: Vec<String> = terms
            .iter()
            .filter(|other| other.name != term.name)
            .flat_map(|other| other.columns.iter().cloned())
            .collect();
        let reduced_ss = if reduced_columns.is_empty() {
            total_ss
        } else {
            regression::fit(&frame, response, &reduced_columns, 0.95)?.residual_ss
        };
        let adjusted_ss = (reduced_ss - error_ss).max(0.0);
        let df_term = term.columns.len() as f64;
        let f = (adjusted_ss / df_term) / error_ms;
        let p = if f.is_finite() {
            (1.0 - f_cdf(f, df_term, error_df)).clamp(0.0, 1.0)
        } else {
            f64::NAN
        };
        anova.push(AnovaRow {
            source: term.name.clone(),
            df: df_term,
            sequential_ss,
            adjusted_ss,
            adjusted_ms: adjusted_ss / df_term,
            f,
            p,
        });
    }
    anova.push(AnovaRow {
        source: "Error".to_owned(),
        df: error_df,
        sequential_ss: error_ss,
        adjusted_ss: error_ss,
        adjusted_ms: error_ms,
        f: f64::NAN,
        p: f64::NAN,
    });
    anova.push(AnovaRow {
        source: "Total".to_owned(),
        df: regression.df_regression + error_df,
        sequential_ss: total_ss,
        adjusted_ss: total_ss,
        adjusted_ms: f64::NAN,
        f: f64::NAN,
        p: f64::NAN,
    });
    Ok(GlmFit {
        terms,
        references,
        regression,
        anova,
        excluded,
    })
}

/// Runs the GLM, builds the report and reuses the OLS residual plot.
pub fn analyze(df: &DataFrame, response: &str, options: &GlmOptions) -> Result<AnalysisOutput> {
    let model = fit(df, response, options)?;
    let mut result = regression::report(response, &model.regression);
    result.title = format!("General Linear Model — response {response}");
    result.method =
        "Ordinary least squares with factors (reference coding), covariates and two-way \
         interactions"
            .to_owned();
    result.excluded = model.excluded;
    result.extra.retain(|item| {
        !matches!(item, Item::Table { headers, .. } if headers.first().is_some_and(|h| h.as_str() == "Source"))
    });
    let mut table = vec![Item::Message(format!(
        "Factors: {}",
        if model.references.is_empty() {
            "none".to_owned()
        } else {
            model
                .references
                .iter()
                .map(|(name, level)| format!("{name} (reference {level})"))
                .collect::<Vec<String>>()
                .join(", ")
        }
    ))];
    table.push(Item::Table {
        headers: vec![
            "Source".to_owned(),
            "DF".to_owned(),
            "Seq SS".to_owned(),
            "Adj SS".to_owned(),
            "Adj MS".to_owned(),
            "F".to_owned(),
            "P".to_owned(),
        ],
        rows: model
            .anova
            .iter()
            .map(|row| {
                vec![
                    row.source.clone(),
                    format!("{:.0}", row.df),
                    format!("{:.6}", row.sequential_ss),
                    format!("{:.6}", row.adjusted_ss),
                    if row.adjusted_ms.is_finite() {
                        format!("{:.6}", row.adjusted_ms)
                    } else {
                        String::new()
                    },
                    if row.f.is_finite() {
                        format!("{:.4}", row.f)
                    } else {
                        String::new()
                    },
                    if row.p.is_finite() {
                        format!("{:.4}", row.p)
                    } else {
                        String::new()
                    },
                ]
            })
            .collect(),
    });
    table.append(&mut result.extra);
    result.extra = table;
    result.notes.insert(
        0,
        (
            "Coding".to_owned(),
            "factors use reference coding (the first level is the reference); an interaction \
             multiplies the basis columns of its two terms"
                .to_owned(),
        ),
    );
    result.notes.insert(
        1,
        (
            "ANOVA".to_owned(),
            "Seq SS is the sequential (Type I) sum of squares in model order; Adj SS is the \
             Type III sum of squares obtained by dropping the term's columns from the full model"
                .to_owned(),
        ),
    );
    result.notes.push((
        "Data policy".to_owned(),
        "rows with a missing response, factor label or covariate are removed listwise".to_owned(),
    ));
    result.warnings = standard_warnings(model.regression.n, model.excluded, 0);
    result.warnings.push(
        "the Type III sums of squares are a standard convention and have not been compared with \
         documented output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(regression::residual_plot(response, &model.regression)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> DataFrame {
        let mut a = Vec::new();
        let mut x = Vec::new();
        let mut y = Vec::new();
        for row in 0..24 {
            let level = if row % 2 == 0 { "A" } else { "B" };
            let covariate = (row % 6) as f64 + 1.0;
            let effect = if level == "A" { 0.0 } else { 3.0 };
            a.push(level.to_owned());
            x.push(covariate);
            let noise = ((row * 7 % 3) as f64 - 1.0) * 0.05;
            y.push(
                10.0 + effect
                    + 2.0 * covariate
                    + 0.5 * f64::from(u8::from(level == "B")) * covariate
                    + noise,
            );
        }
        DataFrame::new(vec![
            Column::new_f64("y", y),
            Column::new_text("A", a),
            Column::new_f64("x", x),
        ])
        .expect("frame")
    }

    fn options() -> GlmOptions {
        GlmOptions {
            factors: vec!["A".to_owned()],
            covariates: vec!["x".to_owned()],
            interactions: vec![("A".to_owned(), "x".to_owned())],
        }
    }

    #[test]
    fn the_glm_expands_the_factor_and_the_interaction() {
        let model = fit(&frame(), "y", &options()).expect("fit");
        let names: Vec<&str> = model.terms.iter().map(|term| term.name.as_str()).collect();
        assert_eq!(names, vec!["A", "x", "A*x"]);
        assert_eq!(model.terms[0].columns, vec!["A[B]"]);
        assert_eq!(model.terms[2].columns, vec!["A[B]*x"]);
        assert_eq!(model.references, vec![("A".to_owned(), "A".to_owned())]);
        assert_eq!(model.regression.n, 24);
        assert!(
            model.regression.r_squared > 0.999,
            "{}",
            model.regression.r_squared
        );
        assert_eq!(model.anova.len(), 5);
        assert_eq!(model.anova[0].df, 1.0);
        assert_eq!(model.anova[2].source, "A*x");
        assert!(model.anova[2].p.is_finite());
        assert_eq!(model.anova[3].source, "Error");
        assert_eq!(model.anova[4].source, "Total");
    }

    #[test]
    fn the_glm_report_and_residual_plot_are_built() {
        let out = analyze(&frame(), "y", &options()).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("General Linear Model"), "{text}");
        assert!(text.contains("Adj SS"), "{text}");
        assert!(text.contains("A[B]*x"), "{text}");
        assert!(text.contains("reference A"), "{text}");
        assert!(out.plot.is_some());
    }

    #[test]
    fn a_factor_covariate_model_without_interaction_works() {
        let options = GlmOptions {
            factors: vec!["A".to_owned()],
            covariates: vec!["x".to_owned()],
            interactions: Vec::new(),
        };
        let model = fit(&frame(), "y", &options).expect("fit");
        assert_eq!(model.terms.len(), 2);
        assert!(model.anova[1].source == "x");
        assert!(model.regression.r_squared > 0.9);
    }

    #[test]
    fn invalid_glm_requests_are_rejected() {
        let df = frame();
        let empty = GlmOptions::default();
        assert!(fit(&df, "y", &empty).is_err());
        let duplicate = GlmOptions {
            factors: vec!["A".to_owned(), "A".to_owned()],
            covariates: Vec::new(),
            interactions: Vec::new(),
        };
        assert!(fit(&df, "y", &duplicate).is_err());
        let unknown = GlmOptions {
            factors: vec!["A".to_owned()],
            covariates: vec!["x".to_owned()],
            interactions: vec![("A".to_owned(), "missing".to_owned())],
        };
        assert!(fit(&df, "y", &unknown).is_err());
        let one_level = GlmOptions {
            factors: vec!["group".to_owned()],
            covariates: Vec::new(),
            interactions: Vec::new(),
        };
        let mut single = df;
        single
            .insert_column(
                single.n_columns(),
                Column::new_text("group", vec!["A".to_owned(); single.n_rows()]),
            )
            .expect("column");
        assert!(fit(&single, "y", &one_level).is_err());
    }
}
