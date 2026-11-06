//! Poisson regression for count data.
//!
//! The model is `ln(μᵢ) = β₀ + Σ βⱼ xᵢⱼ`, fitted by maximum likelihood with
//! the iteratively reweighted least squares form of Newton–Raphson: at each
//! step `(XᵀWX)·β = XᵀWz` with `W = diag(μ)` and the working response
//! `z = η + (y − μ)/μ`. The covariance of the estimate is the inverse of
//! `XᵀWX` at convergence, so the Wald tests and the intervals of the rate
//! ratios follow from the same matrix.
//!
//! The report adds the deviance, the likelihood-ratio test against the
//! intercept-only model, McFadden's pseudo-R², AIC, the Pearson dispersion
//! (overdispersion when it is well above one) and the fitted counts.
//!
//! The documented behaviour has a Poisson regression; its convergence and
//! dispersion conventions were not documented, so the numbers are not claimed as
//! documented behaviour — see `the project notes`.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{chi_square_cdf, normal_cdf};
use crate::workflow::AnalysisOutput;

/// Fitted Poisson model.
#[derive(Debug, Clone, PartialEq)]
pub struct PoissonFit {
    /// Term names, the constant first.
    pub terms: Vec<String>,
    /// Coefficients in log-rate units.
    pub coefficients: Vec<f64>,
    /// Standard errors of the coefficients.
    pub standard_errors: Vec<f64>,
    /// Wald z statistics.
    pub z: Vec<f64>,
    /// Two-sided p-values of the coefficients.
    pub p_values: Vec<f64>,
    /// Rate ratios, `exp(β)`.
    pub rate_ratios: Vec<f64>,
    /// Lower 95 % bounds of the rate ratios.
    pub rr_lower: Vec<f64>,
    /// Upper 95 % bounds of the rate ratios.
    pub rr_upper: Vec<f64>,
    /// Observations used.
    pub n: usize,
    /// Sum of the observed counts.
    pub total: f64,
    /// Log-likelihood of the fitted model.
    pub log_likelihood: f64,
    /// Log-likelihood of the intercept-only model.
    pub null_log_likelihood: f64,
    /// Deviance, `2Σ[y·ln(y/μ) − (y − μ)]`.
    pub deviance: f64,
    /// Deviance of the intercept-only model.
    pub null_deviance: f64,
    /// Degrees of freedom of the deviance (`n − terms`).
    pub residual_df: usize,
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
    /// Pearson chi-square, `Σ(y − μ)²/μ`.
    pub pearson: f64,
    /// Pearson chi-square divided by the residual degrees of freedom.
    pub dispersion: f64,
    /// Fitted counts, in row order of the used rows.
    pub fitted: Vec<f64>,
    /// Observed counts, in row order of the used rows.
    pub observed: Vec<f64>,
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

/// Fits the model.
pub fn fit(df: &DataFrame, response: &str, predictors: &[String]) -> Result<PoissonFit> {
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
    let response_values = response_column.numeric_by_row();
    let mut predictor_values = Vec::with_capacity(predictors.len());
    for name in predictors {
        predictor_values.push(df.column(name)?.numeric_by_row());
    }

    let mut rows: Vec<usize> = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        if response_column.is_missing(row) {
            continue;
        }
        let mut complete = true;
        for values in &predictor_values {
            if !values[row].is_finite() {
                complete = false;
                break;
            }
        }
        if !complete {
            continue;
        }
        let y = response_values[row];
        if !y.is_finite() || y < 0.0 || (y - y.round()).abs() > 1e-9 {
            return Err(Error::Unsupported(
                "the response of a Poisson model must be a non-negative integer count".to_owned(),
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
    let k = predictors.len() + 1;
    let y: Vec<f64> = rows.iter().map(|row| response_values[*row]).collect();
    let total: f64 = y.iter().sum();
    if total <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let design: Vec<Vec<f64>> = rows
        .iter()
        .map(|row| {
            let mut terms = Vec::with_capacity(k);
            terms.push(1.0);
            for values in &predictor_values {
                terms.push(values[*row]);
            }
            terms
        })
        .collect();

    // Newton–Raphson through iteratively reweighted least squares.
    let mean = total / n as f64;
    let mut beta = vec![0.0; k];
    beta[0] = mean.ln().max(-50.0);
    let mut iterations = 0;
    let mut converged = false;
    for iteration in 0..100 {
        iterations = iteration + 1;
        let mut information = vec![vec![0.0; k]; k];
        let mut gradient = vec![0.0; k];
        for (row, terms) in design.iter().enumerate() {
            let eta: f64 = terms.iter().zip(&beta).map(|(x, b)| x * b).sum();
            let mu = eta.exp().clamp(1e-12, 1e12);
            let working = eta + (y[row] - mu) / mu;
            for i in 0..k {
                gradient[i] += terms[i] * mu * working;
                for j in 0..k {
                    information[i][j] += terms[i] * mu * terms[j];
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

    let mut fitted = Vec::with_capacity(n);
    let mut log_likelihood = 0.0;
    let mut information = vec![vec![0.0; k]; k];
    for (row, terms) in design.iter().enumerate() {
        let eta: f64 = terms.iter().zip(&beta).map(|(x, b)| x * b).sum();
        let mu = eta.exp().clamp(1e-12, 1e12);
        fitted.push(mu);
        // log-likelihood of the Poisson law, with 0·ln 0 taken as zero.
        let log_factorial: f64 = crate::special::ln_gamma(y[row] + 1.0);
        log_likelihood += y[row] * mu.ln() - mu - log_factorial;
        for i in 0..k {
            for j in 0..k {
                information[i][j] += terms[i] * mu * terms[j];
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
    let rate_ratios: Vec<f64> = beta.iter().map(|b| b.exp()).collect();
    let rr_lower: Vec<f64> = beta
        .iter()
        .zip(&standard_errors)
        .map(|(b, se)| (b - 1.959_963_984_540_054 * se).exp())
        .collect();
    let rr_upper: Vec<f64> = beta
        .iter()
        .zip(&standard_errors)
        .map(|(b, se)| (b + 1.959_963_984_540_054 * se).exp())
        .collect();

    let null_mu = mean;
    let null_log_likelihood: f64 = y
        .iter()
        .map(|value| value * null_mu.ln() - null_mu - crate::special::ln_gamma(value + 1.0))
        .sum();
    let mut deviance = 0.0;
    let mut pearson = 0.0;
    for (index, observed) in y.iter().enumerate() {
        let mu = fitted[index];
        if *observed > 0.0 {
            deviance += 2.0 * (observed * (observed / mu).ln() - (observed - mu));
        } else {
            deviance += 2.0 * mu;
        }
        pearson += (observed - mu) * (observed - mu) / mu;
    }
    let null_deviance = 2.0
        * y.iter()
            .map(|value| {
                if *value > 0.0 {
                    value * (value / null_mu).ln() - (value - null_mu)
                } else {
                    null_mu
                }
            })
            .sum::<f64>();
    let residual_df = n.saturating_sub(k);
    let test_statistic = (null_deviance - deviance).max(0.0);
    let test_df = predictors.len();
    let test_p = (1.0 - chi_square_cdf(test_statistic, test_df as f64)).clamp(0.0, 1.0);
    let mcfadden_r2 = if null_log_likelihood < 0.0 {
        1.0 - log_likelihood / null_log_likelihood
    } else {
        0.0
    };
    let aic = -2.0 * log_likelihood + 2.0 * k as f64;
    let dispersion = if residual_df > 0 {
        pearson / residual_df as f64
    } else {
        f64::NAN
    };

    let mut terms = vec!["Constant".to_owned()];
    terms.extend(predictors.iter().cloned());

    Ok(PoissonFit {
        terms,
        coefficients: beta,
        standard_errors,
        z,
        p_values,
        rate_ratios,
        rr_lower,
        rr_upper,
        n,
        total,
        log_likelihood,
        null_log_likelihood,
        deviance,
        null_deviance,
        residual_df,
        test_statistic,
        test_df,
        test_p,
        mcfadden_r2,
        aic,
        pearson,
        dispersion,
        fitted,
        observed: y,
        iterations,
        converged,
    })
}

/// Runs the model and builds the Session report.
pub fn analyze(df: &DataFrame, response: &str, predictors: &[String]) -> Result<AnalysisOutput> {
    let model = fit(df, response, predictors)?;
    let mut result = AnalysisResult::new(
        format!("Poisson Regression — response {response}"),
        format!(
            "Maximum likelihood (log link), predictors: {}",
            predictors.join(", ")
        ),
    );
    result.n = model.n;
    result.excluded = df.n_rows() - model.n;
    result.statistics = vec![
        ("Total counts", model.total),
        ("Log-likelihood", model.log_likelihood),
        ("Null log-likelihood", model.null_log_likelihood),
        ("Deviance", model.deviance),
        ("Null deviance", model.null_deviance),
        ("Residual df", model.residual_df as f64),
        ("Likelihood ratio", model.test_statistic),
        ("LR p-value", model.test_p),
        ("McFadden R²", model.mcfadden_r2),
        ("AIC", model.aic),
        ("Pearson chi-square", model.pearson),
        ("Dispersion", model.dispersion),
        ("IRLS iterations", model.iterations as f64),
    ];

    result.extra.push(Item::Table {
        headers: vec![
            "Term".to_owned(),
            "Coef".to_owned(),
            "SE".to_owned(),
            "Z".to_owned(),
            "P".to_owned(),
            "Rate ratio".to_owned(),
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
                    format!("{:.6}", model.rate_ratios[index]),
                    format!(
                        "({:.6}, {:.6})",
                        model.rr_lower[index], model.rr_upper[index]
                    ),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Observation".to_owned(),
            "Count".to_owned(),
            "Fitted".to_owned(),
            "Residual".to_owned(),
        ],
        rows: model
            .observed
            .iter()
            .enumerate()
            .map(|(index, observed)| {
                vec![
                    format!("{}", index + 1),
                    format!("{observed:.0}"),
                    format!("{:.4}", model.fitted[index]),
                    format!("{:.4}", observed - model.fitted[index]),
                ]
            })
            .collect(),
    });

    result.notes = vec![
        (
            "Model".to_owned(),
            "ln(μ) = β₀ + Σβⱼxⱼ, fitted by maximum likelihood (IRLS)".to_owned(),
        ),
        (
            "Rate ratio".to_owned(),
            "exp(β): the multiplicative change of the expected count per unit of the predictor; \
             the interval is exp(β ± 1.96·SE)"
                .to_owned(),
        ),
        (
            "Tests".to_owned(),
            "the Wald z tests use the inverse information matrix; the likelihood-ratio test \
             compares the deviance with the null deviance"
                .to_owned(),
        ),
        (
            "Dispersion".to_owned(),
            "Pearson chi-square divided by the residual degrees of freedom; well above one \
             means overdispersion, where the standard errors are optimistic"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing response or predictor are excluded and counted; the response \
             must be a non-negative integer count"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the fit and convergence conventions are standard choices and have not been verified \
         against documented output"
            .to_owned(),
    );
    if model.dispersion.is_finite() && model.dispersion > 2.0 {
        result.warnings.push(format!(
            "the Pearson dispersion is {:.2}; the data are overdispersed and a quasi-Poisson or \
             negative-binomial model may fit better",
            model.dispersion
        ));
    }
    if !model.converged {
        result.warnings.push(
            "the fit did not converge in 100 iterations; the coefficients and standard errors \
             are unreliable"
                .to_owned(),
        );
    }

    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}
