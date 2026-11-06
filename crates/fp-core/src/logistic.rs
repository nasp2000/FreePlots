//! Binary logistic regression.
//!
//! The model is `logit(pᵢ) = β₀ + Σ βⱼ xᵢⱼ`, estimated by maximum likelihood
//! with the **iteratively reweighted least squares** form of Newton–Raphson:
//! at each step `(XᵀWX)·β = XᵀWz` with `W = diag(μ(1 − μ))` and the working
//! response `z = η + (y − μ)/W`. The covariance of the estimate is the inverse
//! of `XᵀWX` at convergence, so the Wald tests and the intervals of the odds
//! ratios follow from the same matrix.
//!
//! The report adds the likelihood-ratio test of the model against the null
//! model, McFadden's pseudo-R², AIC, the classification table at a 0.5 cut-off
//! and the Hosmer–Lemeshow goodness-of-fit test with ten risk groups.
//!
//! Numeric predictors enter the model directly; text, boolean and date
//! predictors are dummy coded with the first level as the reference, so a
//! coefficient is the change of the log-odds against that reference level.
//!
//! The documented behaviour has a binary logistic regression; its convergence
//! and grouping conventions were not documented, so the numbers are not claimed
//! as documented behaviour — see `the project notes`.

use std::collections::BTreeSet;

use crate::data::{ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::regression::{cell, label};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{chi_square_cdf, normal_cdf};
use crate::workflow::AnalysisOutput;

/// Hosmer–Lemeshow goodness-of-fit table.
#[derive(Debug, Clone, PartialEq)]
pub struct HosmerLemeshow {
    /// `(lower bound, upper bound, observed events, expected events, n)` per
    /// risk group.
    pub groups: Vec<(f64, f64, f64, f64, usize)>,
    /// Chi-square statistic.
    pub statistic: f64,
    /// Degrees of freedom (`groups − 2`).
    pub df: usize,
    /// p-value of the statistic.
    pub p_value: f64,
}

/// Fitted binary logistic model.
#[derive(Debug, Clone, PartialEq)]
pub struct LogisticFit {
    /// Term names, the constant first.
    pub terms: Vec<String>,
    /// Coefficients in log-odds units.
    pub coefficients: Vec<f64>,
    /// Standard errors of the coefficients.
    pub standard_errors: Vec<f64>,
    /// Wald z statistics.
    pub z: Vec<f64>,
    /// Two-sided p-values of the coefficients.
    pub p_values: Vec<f64>,
    /// Odds ratios, `exp(β)`.
    pub odds_ratios: Vec<f64>,
    /// Lower 95 % bounds of the odds ratios.
    pub or_lower: Vec<f64>,
    /// Upper 95 % bounds of the odds ratios.
    pub or_upper: Vec<f64>,
    /// Observations used.
    pub n: usize,
    /// Responses equal to one.
    pub events: usize,
    /// Log-likelihood of the fitted model.
    pub log_likelihood: f64,
    /// Log-likelihood of the intercept-only model.
    pub null_log_likelihood: f64,
    /// Deviance, `−2·ℓ`.
    pub deviance: f64,
    /// Likelihood-ratio statistic against the null model.
    pub test_statistic: f64,
    /// Degrees of freedom of the likelihood-ratio test.
    pub test_df: usize,
    /// p-value of the likelihood-ratio test.
    pub test_p: f64,
    /// McFadden's pseudo-R².
    pub mcfadden_r2: f64,
    /// Akaike information criterion.
    pub aic: f64,
    /// Responses classified correctly at the 0.5 cut-off.
    pub correct: usize,
    /// Predicted probabilities, in row order of the used rows.
    pub predicted: Vec<f64>,
    /// Responses, in row order of the used rows.
    pub responses: Vec<f64>,
    /// Hosmer–Lemeshow test, when there are enough observations.
    pub hosmer_lemeshow: Option<HosmerLemeshow>,
    /// IRLS iterations used.
    pub iterations: usize,
    /// Whether the coefficients met the convergence tolerance.
    pub converged: bool,
}

/// Solves a linear system by Gaussian elimination with partial pivoting.
#[allow(clippy::needless_range_loop)]
fn solve(matrix: &[Vec<f64>], rhs: &[f64]) -> Option<Vec<f64>> {
    let n = rhs.len();
    if matrix.len() != n {
        return None;
    }
    let mut a = matrix.to_vec();
    let mut b = rhs.to_vec();
    for column in 0..n {
        let mut pivot = column;
        for row in (column + 1)..n {
            if a[row][column].abs() > a[pivot][column].abs() {
                pivot = row;
            }
        }
        if a[pivot][column].abs() < 1e-12 {
            return None;
        }
        a.swap(column, pivot);
        b.swap(column, pivot);
        for row in (column + 1)..n {
            let factor = a[row][column] / a[column][column];
            for k in column..n {
                a[row][k] -= factor * a[column][k];
            }
            b[row] -= factor * b[column];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut sum = b[row];
        for k in (row + 1)..n {
            sum -= a[row][k] * x[k];
        }
        x[row] = sum / a[row][row];
    }
    Some(x)
}

/// Inverts a square matrix through the solution of `A·x = eᵢ`.
fn invert(matrix: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = matrix.len();
    let mut inverse = vec![vec![0.0; n]; n];
    for column in 0..n {
        let mut unit = vec![0.0; n];
        unit[column] = 1.0;
        let solution = solve(matrix, &unit)?;
        for row in 0..n {
            inverse[row][column] = solution[row];
        }
    }
    Some(inverse)
}

/// The logistic function.
fn logistic(eta: f64) -> f64 {
    if eta >= 0.0 {
        1.0 / (1.0 + (-eta).exp())
    } else {
        let value = eta.exp();
        value / (1.0 + value)
    }
}

/// Fits the model.
pub fn fit(df: &DataFrame, response: &str, predictors: &[String]) -> Result<LogisticFit> {
    if predictors.is_empty() {
        return Err(Error::Unsupported(
            "the model needs at least one predictor".to_owned(),
        ));
    }
    if predictors.len() > 20 {
        return Err(Error::Unsupported(
            "the model accepts at most twenty predictors".to_owned(),
        ));
    }
    if predictors.iter().any(|name| name == response) {
        return Err(Error::Unsupported(
            "the response cannot also be a predictor".to_owned(),
        ));
    }
    let mut seen = predictors.to_vec();
    seen.sort();
    seen.dedup();
    if seen.len() != predictors.len() {
        return Err(Error::Unsupported(
            "the predictors must be distinct columns".to_owned(),
        ));
    }

    let response_column = df.column(response)?;
    if matches!(
        response_column.data(),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
    ) {
        return Err(Error::Csv(format!(
            "column {response} is not numeric and cannot be the response"
        )));
    }
    let predictor_columns = predictors
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;

    // Numeric columns enter the model directly; text, boolean and date
    // columns are dummy coded with the first level as the reference, as in
    // regression.
    enum Encoding {
        Numeric,
        Categorical { levels: Vec<String> },
    }
    let mut encodings: Vec<Encoding> = Vec::new();
    let mut term_names: Vec<String> = Vec::new();
    for (name, column) in predictors.iter().zip(&predictor_columns) {
        if matches!(
            column.data(),
            ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
        ) {
            let levels: BTreeSet<String> = (0..df.n_rows())
                .filter_map(|row| label(column, row))
                .collect();
            if levels.len() < 2 {
                return Err(Error::Unsupported(format!(
                    "categorical predictor {name} needs at least two levels"
                )));
            }
            let mut levels: Vec<String> = levels.into_iter().collect();
            levels.remove(0); // first level is the reference
            for level in &levels {
                term_names.push(format!("{name}[{level}]"));
            }
            encodings.push(Encoding::Categorical { levels });
        } else {
            term_names.push(name.clone());
            encodings.push(Encoding::Numeric);
        }
    }

    // Complete rows only, and a response that is strictly binary.
    let mut rows: Vec<usize> = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        if response_column.is_missing(row) {
            continue;
        }
        let mut complete = true;
        for (encoding, column) in encodings.iter().zip(&predictor_columns) {
            let present = match encoding {
                Encoding::Numeric => cell(column, row).is_some(),
                Encoding::Categorical { .. } => label(column, row).is_some(),
            };
            if !present {
                complete = false;
                break;
            }
        }
        if !complete {
            continue;
        }
        let y = cell(response_column, row).unwrap_or(f64::NAN);
        if !(y == 0.0 || y == 1.0) {
            return Err(Error::Unsupported(
                "the response must contain only 0 and 1".to_owned(),
            ));
        }
        rows.push(row);
    }
    if rows.len() < 3 {
        return Err(Error::SampleTooSmall {
            n: rows.len(),
            min: 3,
        });
    }

    let n = rows.len();
    let k = term_names.len() + 1;
    let events = rows
        .iter()
        .filter(|row| cell(response_column, **row) == Some(1.0))
        .count();
    if events == 0 || events == n {
        return Err(Error::ConstantSample);
    }

    let design: Vec<Vec<f64>> = rows
        .iter()
        .map(|row| {
            let mut terms = Vec::with_capacity(k);
            terms.push(1.0);
            for (encoding, column) in encodings.iter().zip(&predictor_columns) {
                match encoding {
                    Encoding::Numeric => terms.push(cell(column, *row).unwrap_or(0.0)),
                    Encoding::Categorical { levels } => {
                        let current = label(column, *row).unwrap_or_default();
                        for level in levels {
                            terms.push(if &current == level { 1.0 } else { 0.0 });
                        }
                    }
                }
            }
            terms
        })
        .collect();
    let y: Vec<f64> = rows
        .iter()
        .map(|row| cell(response_column, *row).unwrap_or(0.0))
        .collect();

    // Newton–Raphson through iteratively reweighted least squares.
    let mut beta = vec![0.0; k];
    let mut iterations = 0;
    let mut converged = false;
    for iteration in 0..100 {
        iterations = iteration + 1;
        let mut information = vec![vec![0.0; k]; k];
        let mut gradient = vec![0.0; k];
        for (row, terms) in design.iter().enumerate() {
            let eta: f64 = terms.iter().zip(&beta).map(|(x, b)| x * b).sum();
            let mu = logistic(eta).clamp(1e-12, 1.0 - 1e-12);
            let weight = (mu * (1.0 - mu)).max(1e-12);
            let working = eta + (y[row] - mu) / weight;
            for i in 0..k {
                gradient[i] += terms[i] * weight * working;
                for j in 0..k {
                    information[i][j] += terms[i] * weight * terms[j];
                }
            }
        }
        let next = solve(&information, &gradient).ok_or_else(|| Error::NotConverged {
            detail: "the information matrix is singular; check for constant or collinear \
                     predictors"
                .to_owned(),
        })?;
        let change = next
            .iter()
            .zip(&beta)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        beta = next;
        if change < 1e-9 {
            converged = true;
            break;
        }
    }
    // A non-converging fit is usually (quasi-)complete separation: the
    // coefficients grow without bound. The estimate at the last iteration is
    // still returned, with the flag and a warning, instead of hiding the data
    // problem behind an error.

    // Fitted probabilities, log-likelihood and the information matrix.
    let mut predicted = Vec::with_capacity(n);
    let mut log_likelihood = 0.0;
    let mut information = vec![vec![0.0; k]; k];
    for (row, terms) in design.iter().enumerate() {
        let eta: f64 = terms.iter().zip(&beta).map(|(x, b)| x * b).sum();
        let mu = logistic(eta).clamp(1e-300, 1.0 - 1e-16);
        predicted.push(mu);
        log_likelihood += y[row] * mu.ln() + (1.0 - y[row]) * (1.0 - mu).ln();
        let weight = (mu * (1.0 - mu)).max(1e-12);
        for i in 0..k {
            for j in 0..k {
                information[i][j] += terms[i] * weight * terms[j];
            }
        }
    }
    let covariance = invert(&information).ok_or_else(|| Error::NotConverged {
        detail: "the covariance matrix is singular".to_owned(),
    })?;
    let standard_errors: Vec<f64> = (0..k).map(|i| covariance[i][i].max(0.0).sqrt()).collect();
    let z: Vec<f64> = beta
        .iter()
        .zip(&standard_errors)
        .map(|(b, se)| if *se > 0.0 { b / se } else { f64::NAN })
        .collect();
    let p_values: Vec<f64> = z
        .iter()
        .map(|value| {
            if value.is_finite() {
                (2.0 * (1.0 - normal_cdf(value.abs()))).clamp(0.0, 1.0)
            } else {
                0.0
            }
        })
        .collect();
    let odds_ratios: Vec<f64> = beta.iter().map(|b| b.exp()).collect();
    let or_lower: Vec<f64> = beta
        .iter()
        .zip(&standard_errors)
        .map(|(b, se)| (b - 1.959_963_984_540_054 * se).exp())
        .collect();
    let or_upper: Vec<f64> = beta
        .iter()
        .zip(&standard_errors)
        .map(|(b, se)| (b + 1.959_963_984_540_054 * se).exp())
        .collect();

    let mean = events as f64 / n as f64;
    let null_log_likelihood = n as f64 * (mean.ln() * mean + (1.0 - mean).ln() * (1.0 - mean));
    let deviance = -2.0 * log_likelihood;
    let test_statistic = (2.0 * (log_likelihood - null_log_likelihood)).max(0.0);
    let test_df = term_names.len();
    let test_p = (1.0 - chi_square_cdf(test_statistic, test_df as f64)).clamp(0.0, 1.0);
    let mcfadden_r2 = if null_log_likelihood < 0.0 {
        1.0 - log_likelihood / null_log_likelihood
    } else {
        0.0
    };
    let aic = -2.0 * log_likelihood + 2.0 * k as f64;
    let correct = predicted
        .iter()
        .zip(&y)
        .filter(|(probability, response)| (**probability >= 0.5) == (**response == 1.0))
        .count();

    let hosmer_lemeshow = hosmer_lemeshow(&predicted, &y);

    let mut terms = vec!["Constant".to_owned()];
    terms.extend(term_names);

    Ok(LogisticFit {
        terms,
        coefficients: beta,
        standard_errors,
        z,
        p_values,
        odds_ratios,
        or_lower,
        or_upper,
        n,
        events,
        log_likelihood,
        null_log_likelihood,
        deviance,
        test_statistic,
        test_df,
        test_p,
        mcfadden_r2,
        aic,
        correct,
        predicted,
        responses: y,
        hosmer_lemeshow,
        iterations,
        converged,
    })
}

/// Hosmer–Lemeshow test with ten groups of predicted risk.
///
/// The observations are sorted by the fitted probability and cut into ten
/// groups of (nearly) equal size; the statistic sums
/// `(O₁ − E₁)²/E₁ + (O₀ − E₀)²/E₀` over the groups and has `groups − 2`
/// degrees of freedom.
fn hosmer_lemeshow(predicted: &[f64], responses: &[f64]) -> Option<HosmerLemeshow> {
    let n = predicted.len();
    if n < 20 {
        return None;
    }
    let groups = 10.min(n / 2);
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| {
        predicted[*a]
            .partial_cmp(&predicted[*b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut table = Vec::with_capacity(groups);
    let mut statistic = 0.0;
    for group in 0..groups {
        let start = group * n / groups;
        let end = (group + 1) * n / groups;
        if end <= start {
            continue;
        }
        let slice = &order[start..end];
        let count = slice.len();
        let observed: f64 = slice.iter().map(|index| responses[*index]).sum();
        let expected: f64 = slice.iter().map(|index| predicted[*index]).sum();
        let observed_zero = count as f64 - observed;
        let expected_zero = count as f64 - expected;
        if expected > 1e-12 {
            statistic += (observed - expected).powi(2) / expected;
        }
        if expected_zero > 1e-12 {
            statistic += (observed_zero - expected_zero).powi(2) / expected_zero;
        }
        let lower = slice
            .iter()
            .map(|index| predicted[*index])
            .fold(f64::INFINITY, f64::min);
        let upper = slice
            .iter()
            .map(|index| predicted[*index])
            .fold(f64::NEG_INFINITY, f64::max);
        table.push((lower, upper, observed, expected, count));
    }
    let df = table.len().saturating_sub(2);
    if df == 0 {
        return None;
    }
    let p_value = (1.0 - chi_square_cdf(statistic, df as f64)).clamp(0.0, 1.0);
    Some(HosmerLemeshow {
        groups: table,
        statistic,
        df,
        p_value,
    })
}

/// Runs the model and builds the Session report.
pub fn analyze(df: &DataFrame, response: &str, predictors: &[String]) -> Result<AnalysisOutput> {
    let model = fit(df, response, predictors)?;
    let mut result = AnalysisResult::new(
        format!("Binary Logistic Regression — response {response}"),
        format!("Maximum likelihood, predictors: {}", predictors.join(", ")),
    );
    result.n = model.n;
    result.excluded = df.n_rows() - model.n;
    result.statistics = vec![
        ("Events (y = 1)", model.events as f64),
        ("Log-likelihood", model.log_likelihood),
        ("Null log-likelihood", model.null_log_likelihood),
        ("Deviance", model.deviance),
        ("Likelihood ratio", model.test_statistic),
        ("LR p-value", model.test_p),
        ("McFadden R²", model.mcfadden_r2),
        ("AIC", model.aic),
        (
            "Correctly classified (%)",
            100.0 * model.correct as f64 / model.n as f64,
        ),
        ("IRLS iterations", model.iterations as f64),
    ];

    result.extra.push(Item::Table {
        headers: vec![
            "Term".to_owned(),
            "Coef".to_owned(),
            "SE".to_owned(),
            "Z".to_owned(),
            "P".to_owned(),
            "Odds ratio".to_owned(),
            "95% CI".to_owned(),
        ],
        rows: model
            .terms
            .iter()
            .enumerate()
            .map(|(index, term)| {
                vec![
                    term.clone(),
                    format!("{:.6}", model.coefficients[index]),
                    format!("{:.6}", model.standard_errors[index]),
                    format!("{:.6}", model.z[index]),
                    format!("{:.6}", model.p_values[index]),
                    format!("{:.6}", model.odds_ratios[index]),
                    format!(
                        "({:.6}, {:.6})",
                        model.or_lower[index], model.or_upper[index]
                    ),
                ]
            })
            .collect(),
    });

    // Confusion matrix at the 0.5 cut-off.
    let mut true_negative = 0;
    let mut false_positive = 0;
    let mut false_negative = 0;
    let mut true_positive = 0;
    for (probability, response) in model.predicted.iter().zip(&model.responses) {
        match (*probability >= 0.5, *response == 1.0) {
            (false, false) => true_negative += 1,
            (true, false) => false_positive += 1,
            (false, true) => false_negative += 1,
            (true, true) => true_positive += 1,
        }
    }
    result.extra.push(Item::Table {
        headers: vec![
            "Observed".to_owned(),
            "Predicted 0".to_owned(),
            "Predicted 1".to_owned(),
            "Total".to_owned(),
        ],
        rows: vec![
            vec![
                "0".to_owned(),
                format!("{true_negative}"),
                format!("{false_positive}"),
                format!("{}", true_negative + false_positive),
            ],
            vec![
                "1".to_owned(),
                format!("{false_negative}"),
                format!("{true_positive}"),
                format!("{}", false_negative + true_positive),
            ],
        ],
    });

    if let Some(hl) = &model.hosmer_lemeshow {
        result.extra.push(Item::Table {
            headers: vec![
                "Risk group".to_owned(),
                "Probability range".to_owned(),
                "Observed 1".to_owned(),
                "Expected 1".to_owned(),
                "n".to_owned(),
            ],
            rows: hl
                .groups
                .iter()
                .enumerate()
                .map(|(index, (lower, upper, observed, expected, count))| {
                    vec![
                        format!("{}", index + 1),
                        format!("{lower:.4} – {upper:.4}"),
                        format!("{observed:.0}"),
                        format!("{expected:.2}"),
                        format!("{count}"),
                    ]
                })
                .collect(),
        });
        result
            .statistics
            .push(("Hosmer-Lemeshow chi-square", hl.statistic));
        result.statistics.push(("Hosmer-Lemeshow p", hl.p_value));
    }

    result.notes = vec![
        (
            "Model".to_owned(),
            "logit(p) = β₀ + Σβⱼxⱼ, fitted by maximum likelihood (IRLS)".to_owned(),
        ),
        (
            "Odds ratio".to_owned(),
            "exp(β): the multiplicative change of the odds per unit of the predictor; the \
             interval is exp(β ± 1.96·SE)"
                .to_owned(),
        ),
        (
            "Tests".to_owned(),
            "the Wald z tests use the inverse information matrix; the likelihood-ratio test \
             compares the model with the intercept-only model"
                .to_owned(),
        ),
        (
            "Goodness of fit".to_owned(),
            "Hosmer-Lemeshow with ten risk groups; a large p-value means no evidence of \
             misfit (at least twenty complete rows are needed)"
                .to_owned(),
        ),
        (
            "Coding".to_owned(),
            "text, boolean and date predictors are dummy coded with the first level as the \
             reference; numeric predictors enter directly"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing response or predictor are excluded and counted; the response \
             must contain only 0 and 1"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the fit, convergence and grouping conventions are standard choices and have not been \
         verified against documented output"
            .to_owned(),
    );
    if !model.converged {
        result.warnings.push(
            "the fit did not converge in 100 iterations; the data are probably separated and \
             the coefficients and standard errors are unreliable"
                .to_owned(),
        );
    }
    if model.coefficients.iter().any(|value| value.abs() > 25.0) {
        result.warnings.push(
            "a coefficient above 25 in absolute value suggests (quasi-)complete separation; the \
             standard errors may be unreliable"
                .to_owned(),
        );
    }

    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}
