//! Ordinal logistic regression (proportional-odds model).
//!
//! The response takes the ordered codes `1, 2, …, K`. The model is
//!
//! ```text
//! logit P(Y ≤ k | x) = θ_k − βᵀx,   k = 1, …, K − 1
//! ```
//!
//! with ordered intercepts `θ_1 ≤ … ≤ θ_{K−1}` and a single set of slopes: the
//! **proportional-odds** assumption, in which the effect of a predictor is the
//! same on every cumulative probability.
//!
//! The ordinal likelihood is maximised directly with Nelder–Mead over the
//! thresholds (kept ordered through positive increments) and the slopes; the
//! standard errors come from the numerical Hessian of the log-likelihood in
//! the natural `(θ, β)` parameters. Expanding the likelihood into `K − 1`
//! binary observations per row and fitting that with IRLS maximises a
//! different objective, so it is not used.
//!
//! The documented behaviour has ordinal regression; its convergence and
//! parameterisation conventions were not documented, so the numbers are not
//! claimed as documented behaviour — see `the project notes`.

use std::collections::BTreeMap;

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{chi_square_cdf, normal_cdf};
use crate::workflow::AnalysisOutput;

/// Fitted ordinal model.
#[derive(Debug, Clone, PartialEq)]
pub struct OrdinalFit {
    /// Ordered response codes.
    pub categories: Vec<f64>,
    /// Thresholds, one per cut.
    pub thresholds: Vec<f64>,
    /// Predictor names.
    pub predictors: Vec<String>,
    /// Slope of each predictor.
    pub coefficients: Vec<f64>,
    /// Standard errors of the slopes.
    pub standard_errors: Vec<f64>,
    /// Wald z statistics of the slopes.
    pub z: Vec<f64>,
    /// p-values of the slopes.
    pub p_values: Vec<f64>,
    /// Odds ratios `exp(β)`.
    pub odds_ratios: Vec<f64>,
    /// Lower 95 % bounds of the odds ratios.
    pub or_lower: Vec<f64>,
    /// Upper 95 % bounds of the odds ratios.
    pub or_upper: Vec<f64>,
    /// Observations used.
    pub n: usize,
    /// Count of each response code.
    pub counts: Vec<usize>,
    /// Log-likelihood of the fitted model.
    pub log_likelihood: f64,
    /// Log-likelihood of the intercept-only model.
    pub null_log_likelihood: f64,
    /// Likelihood-ratio statistic against the null model.
    pub test_statistic: f64,
    /// Degrees of freedom of the test.
    pub test_df: usize,
    /// p-value of the test.
    pub test_p: f64,
    /// McFadden's pseudo-R².
    pub mcfadden_r2: f64,
    /// Akaike information criterion.
    pub aic: f64,
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

fn logistic(eta: f64) -> f64 {
    if eta >= 0.0 {
        1.0 / (1.0 + (-eta).exp())
    } else {
        let value = eta.exp();
        value / (1.0 + value)
    }
}

/// Log-likelihood of the ordinal model for one observation.
fn category_log_likelihood(category: usize, thresholds: &[f64], linear: f64) -> f64 {
    let cuts = thresholds.len();
    let upper = if category >= cuts {
        1.0
    } else {
        logistic(thresholds[category] - linear)
    };
    let lower = if category == 0 {
        0.0
    } else {
        logistic(thresholds[category - 1] - linear)
    };
    (upper - lower).max(1e-300).ln()
}

/// Fits the proportional-odds model.
pub fn fit(df: &DataFrame, response: &str, predictors: &[String]) -> Result<OrdinalFit> {
    if predictors.is_empty() {
        return Err(Error::Unsupported(
            "the model needs at least one predictor".to_owned(),
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
    let response_values = df.column(response)?.numeric_by_row();
    let mut predictor_values = Vec::with_capacity(predictors.len());
    for name in predictors {
        predictor_values.push(df.column(name)?.numeric_by_row());
    }
    let mut rows: Vec<usize> = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        if !response_values[row].is_finite() {
            continue;
        }
        if predictor_values
            .iter()
            .any(|values| !values[row].is_finite())
        {
            continue;
        }
        let code = response_values[row];
        if (code - code.round()).abs() > 1e-9 {
            return Err(Error::Unsupported(
                "the ordinal response must hold integer codes".to_owned(),
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
    let mut observed: BTreeMap<i64, usize> = BTreeMap::new();
    for row in &rows {
        *observed.entry(response_values[*row] as i64).or_insert(0) += 1;
    }
    if observed.len() < 2 {
        return Err(Error::ConstantSample);
    }
    if observed.len() < 3 {
        return Err(Error::Unsupported(
            "an ordinal response needs at least three categories; use the binary logistic \
             regression for two"
                .to_owned(),
        ));
    }
    let categories: Vec<f64> = observed.keys().map(|code| *code as f64).collect();
    let counts: Vec<usize> = observed.values().copied().collect();
    let cuts = categories.len() - 1;
    let p = predictors.len();
    let k = cuts + p;
    let n = rows.len();

    // Response index and predictor row of every observation.
    let category_of = |row: usize| -> usize {
        categories
            .iter()
            .position(|code| (*code - response_values[row]).abs() < 1e-9)
            .unwrap_or(0)
    };
    let x_of =
        |row: usize| -> Vec<f64> { predictor_values.iter().map(|values| values[row]).collect() };

    // Starting values: thresholds from the cumulative proportions, slopes zero.
    let mut beta = vec![0.0; k];
    let mut cumulative = 0.0;
    for cut in 0..cuts {
        cumulative += counts[cut] as f64;
        let proportion = (cumulative / n as f64).clamp(1e-6, 1.0 - 1e-6);
        beta[cut] = (proportion / (1.0 - proportion)).ln();
    }

    // Maximise the ordinal likelihood directly. The expanded-binomial IRLS
    // used before maximised the sum of the binary likelihoods, which is a
    // different objective and gave a sub-optimal fit. The threshold ordering
    // is kept through the increments:
    // theta_1 = t0 and theta_j = theta_{j-1} + exp(d_j).
    let mut start = vec![0.0; k];
    start[0] = beta[0];
    for cut in 1..cuts {
        let gap = (beta[cut] - beta[cut - 1]).max(0.05);
        start[cut] = gap.ln();
    }

    let natural = |parameters: &[f64]| -> f64 {
        let thresholds = &parameters[..cuts];
        let slopes = &parameters[cuts..];
        let mut log_likelihood = 0.0;
        for row in &rows {
            let category = category_of(*row);
            let x = x_of(*row);
            let linear: f64 = x
                .iter()
                .zip(slopes)
                .map(|(value, slope)| value * slope)
                .sum();
            log_likelihood += category_log_likelihood(category, thresholds, linear);
        }
        log_likelihood
    };

    let mut objective = |parameters: &[f64]| -> f64 {
        let mut thresholds = Vec::with_capacity(cuts);
        thresholds.push(parameters[0]);
        for cut in 1..cuts {
            thresholds.push(thresholds[cut - 1] + parameters[cut].exp());
        }
        let mut natural_parameters = thresholds;
        natural_parameters.extend_from_slice(&parameters[cuts..]);
        -natural(&natural_parameters)
    };
    let steps = vec![0.5; k];
    let minimum = crate::optimize::nelder_mead(&start, &steps, 8000, 1e-11, &mut objective)?;
    let iterations = minimum.iterations;
    let converged = iterations < 8000;
    let mut thresholds = Vec::with_capacity(cuts);
    thresholds.push(minimum.arguments[0]);
    for cut in 1..cuts {
        thresholds.push(thresholds[cut - 1] + minimum.arguments[cut].exp());
    }
    let coefficients = minimum.arguments[cuts..].to_vec();
    let mut natural_parameters = thresholds.clone();
    natural_parameters.extend_from_slice(&coefficients);
    let log_likelihood = natural(&natural_parameters);
    if !log_likelihood.is_finite() {
        return Err(Error::NotConverged {
            detail: "the ordinal likelihood is not finite at the optimum".to_owned(),
        });
    }

    // Observed information from the numerical Hessian in the natural
    // parameters (theta, beta); the covariance is its inverse.
    let covariance = {
        let scale: Vec<f64> = natural_parameters
            .iter()
            .map(|value| 1e-4 * value.abs().max(1.0))
            .collect();
        let mut information = vec![vec![0.0; k]; k];
        for i in 0..k {
            for j in 0..k {
                let mut value = 0.0;
                for (si, sj) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                    let mut point = natural_parameters.clone();
                    point[i] += si * scale[i];
                    point[j] += sj * scale[j];
                    value += si * sj * natural(&point);
                }
                information[i][j] = -value / (4.0 * scale[i] * scale[j]);
            }
        }
        invert(&information).ok_or_else(|| Error::NotConverged {
            detail: "the covariance matrix is singular".to_owned(),
        })?
    };
    let standard_errors: Vec<f64> = (0..p)
        .map(|index| covariance[cuts + index][cuts + index].max(0.0).sqrt())
        .collect();
    let z: Vec<f64> = coefficients
        .iter()
        .zip(&standard_errors)
        .map(|(slope, se)| if *se > 0.0 { slope / se } else { f64::NAN })
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
    let odds_ratios: Vec<f64> = coefficients.iter().map(|slope| slope.exp()).collect();
    let or_lower: Vec<f64> = coefficients
        .iter()
        .zip(&standard_errors)
        .map(|(slope, se)| (slope - 1.959_963_984_540_054 * se).exp())
        .collect();
    let or_upper: Vec<f64> = coefficients
        .iter()
        .zip(&standard_errors)
        .map(|(slope, se)| (slope + 1.959_963_984_540_054 * se).exp())
        .collect();
    // Null model: the thresholds from the cumulative proportions and zero
    // slopes.
    let mut null_log_likelihood = 0.0;
    let mut null_thresholds = vec![0.0; cuts];
    let mut cumulative = 0.0;
    for cut in 0..cuts {
        cumulative += counts[cut] as f64;
        let proportion = (cumulative / n as f64).clamp(1e-6, 1.0 - 1e-6);
        null_thresholds[cut] = (proportion / (1.0 - proportion)).ln();
    }
    for row in &rows {
        let category = category_of(*row);
        null_log_likelihood += category_log_likelihood(category, &null_thresholds, 0.0);
    }

    let test_statistic = (2.0 * (log_likelihood - null_log_likelihood)).max(0.0);
    let test_df = p;
    let test_p = (1.0 - chi_square_cdf(test_statistic, test_df as f64)).clamp(0.0, 1.0);
    let mcfadden_r2 = if null_log_likelihood < 0.0 {
        1.0 - log_likelihood / null_log_likelihood
    } else {
        0.0
    };
    let aic = -2.0 * log_likelihood + 2.0 * k as f64;

    Ok(OrdinalFit {
        categories,
        thresholds,
        predictors: predictors.to_vec(),
        coefficients,
        standard_errors,
        z,
        p_values,
        odds_ratios,
        or_lower,
        or_upper,
        n,
        counts,
        log_likelihood,
        null_log_likelihood,
        test_statistic,
        test_df,
        test_p,
        mcfadden_r2,
        aic,
        iterations,
        converged,
    })
}

/// Runs the model and builds the Session report.
pub fn analyze(df: &DataFrame, response: &str, predictors: &[String]) -> Result<AnalysisOutput> {
    let model = fit(df, response, predictors)?;
    let mut result = AnalysisResult::new(
        format!("Ordinal Logistic Regression — response {response}"),
        format!(
            "Proportional odds (cumulative logit), predictors: {}",
            predictors.join(", ")
        ),
    );
    result.n = model.n;
    result.excluded = df.n_rows() - model.n;
    result.statistics = vec![
        ("Categories", model.categories.len() as f64),
        ("Log-likelihood", model.log_likelihood),
        ("Null log-likelihood", model.null_log_likelihood),
        ("Likelihood ratio", model.test_statistic),
        ("LR p-value", model.test_p),
        ("McFadden R²", model.mcfadden_r2),
        ("AIC", model.aic),
        ("IRLS iterations", model.iterations as f64),
    ];
    result.extra.push(Item::Table {
        headers: vec!["Category".to_owned(), "Count".to_owned()],
        rows: model
            .categories
            .iter()
            .zip(&model.counts)
            .map(|(category, count)| vec![format!("{category:.0}"), format!("{count}")])
            .collect(),
    });
    let mut thresholds = vec!["Thresholds".to_owned()];
    for value in &model.thresholds {
        thresholds.push(format!("{value:.6}"));
    }
    result.extra.push(Item::Table {
        headers: {
            let mut headers = vec!["Quantity".to_owned()];
            for index in 0..model.thresholds.len() {
                headers.push(format!("θ{}", index + 1));
            }
            headers
        },
        rows: vec![thresholds],
    });
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
            .predictors
            .iter()
            .enumerate()
            .map(|(index, name)| {
                vec![
                    name.clone(),
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
    result.notes = vec![
        (
            "Model".to_owned(),
            "logit P(Y ≤ k) = θ_k − βᵀx: the slopes are shared by every cumulative \
             probability (proportional odds)"
                .to_owned(),
        ),
        (
            "Odds ratio".to_owned(),
            "exp(β) is the factor by which the odds of being in a lower category change per \
             unit of the predictor"
                .to_owned(),
        ),
        (
            "Fit".to_owned(),
            "the ordinal likelihood is maximised directly (Nelder–Mead, ordered thresholds); \
             the covariance is the inverse numerical Hessian in (θ, β)"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing response or predictor are excluded and counted; the response \
             must be an integer code with at least three categories"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the proportional-odds fit and the threshold parameterisation are standard choices and \
         have not been verified against documented output"
            .to_owned(),
    );
    if !model.converged {
        result.warnings.push(
            "the fit did not converge in 100 iterations; the coefficients and standard errors \
             are unreliable"
                .to_owned(),
        );
    }
    let ordered = model.thresholds.windows(2).all(|pair| pair[0] <= pair[1]);
    if !ordered {
        result.warnings.push(
            "the thresholds are not in increasing order; the proportional-odds assumption may \
             not hold for these data"
                .to_owned(),
        );
    }

    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}
