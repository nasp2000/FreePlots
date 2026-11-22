// SPDX-License-Identifier: MIT
//! Cox proportional-hazards regression.
//!
//! The model is `h(t|x) = h₀(t)·exp(xᵀβ)`, fitted by maximizing the **partial
//! log-likelihood** of the event times with **Efron's** handling of ties:
//!
//! ```text
//! ℓ(β) = Σ_events xᵢᵀβ − Σ_t Σ_{l=0}^{dₜ−1} ln( Σ_{j∈Rₜ} e^{xⱼᵀβ} − (l/dₜ)·Σ_{j∈Dₜ} e^{xⱼᵀβ} )
//! ```
//!
//! where `Rₜ` is the risk set at `t` (every observation with time at least `t`,
//! censored or not) and `Dₜ` the `dₜ` events at `t`. Newton–Raphson with step
//! halving maximizes the likelihood; the covariance is minus the inverse of the
//! observed information, so the standard errors, the Wald tests, the hazard
//! ratios and their confidence intervals follow directly. The likelihood-ratio
//! test compares the model with the intercept-only one (`β = 0`).
//!
//! The proportional-hazards assumption (constant hazard ratio over time) is not
//! tested here; the report says so.
//!
//! The documented behaviour fits proportional hazards; its tie handling was not
//! documented, so the numbers are not claimed to match the documented behaviour
//! — see the project notes.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{chi_square_cdf, normal_cdf, normal_quantile};
use crate::workflow::AnalysisOutput;

/// One observation of the regression.
#[derive(Debug, Clone, PartialEq)]
struct Observation {
    time: f64,
    event: bool,
    covariates: Vec<f64>,
}

/// Fitted Cox proportional-hazards model.
#[derive(Debug, Clone, PartialEq)]
pub struct CoxFit {
    /// Time column.
    pub time: String,
    /// Covariates, in the requested order.
    pub covariates: Vec<String>,
    /// Coefficients `β`.
    pub coefficients: Vec<f64>,
    /// Standard errors of the coefficients.
    pub standard_errors: Vec<f64>,
    /// Wald statistics `β/SE`.
    pub z: Vec<f64>,
    /// Two-sided Wald p-values.
    pub p_values: Vec<f64>,
    /// Hazard ratios `exp(β)`.
    pub hazard_ratios: Vec<f64>,
    /// Lower confidence bound of each hazard ratio.
    pub hr_lower: Vec<f64>,
    /// Upper confidence bound of each hazard ratio.
    pub hr_upper: Vec<f64>,
    /// Maximized partial log-likelihood.
    pub log_likelihood: f64,
    /// Partial log-likelihood of the intercept-only model.
    pub null_log_likelihood: f64,
    /// Likelihood-ratio statistic.
    pub lr_statistic: f64,
    /// p-value of the likelihood-ratio test.
    pub lr_p_value: f64,
    /// Newton–Raphson iterations.
    pub iterations: usize,
    /// Observations used.
    pub n: usize,
    /// Events observed.
    pub events: usize,
    /// Right-censored observations.
    pub censored: usize,
    /// Rows excluded.
    pub excluded: usize,
    /// Confidence level of the hazard-ratio intervals.
    pub confidence: f64,
}

/// Partial log-likelihood, gradient and observed information.
#[allow(clippy::needless_range_loop)]
fn terms(observations: &[Observation], beta: &[f64]) -> (f64, Vec<f64>, Vec<Vec<f64>>) {
    let p = beta.len();
    let weights: Vec<f64> = observations
        .iter()
        .map(|observation| {
            let eta: f64 = observation
                .covariates
                .iter()
                .zip(beta)
                .map(|(value, coefficient)| value * coefficient)
                .sum();
            eta.clamp(-700.0, 700.0).exp()
        })
        .collect();
    let mut log_likelihood = 0.0;
    let mut gradient = vec![0.0; p];
    let mut information = vec![vec![0.0; p]; p];
    let mut times: Vec<f64> = observations
        .iter()
        .filter(|observation| observation.event)
        .map(|observation| observation.time)
        .collect();
    times.sort_by(f64::total_cmp);
    times.dedup();
    for time in times {
        let risk: Vec<usize> = (0..observations.len())
            .filter(|index| observations[*index].time >= time)
            .collect();
        let dead: Vec<usize> = (0..observations.len())
            .filter(|index| observations[*index].time == time && observations[*index].event)
            .collect();
        let deaths = dead.len();
        if deaths == 0 {
            continue;
        }
        let mut risk_weight = 0.0;
        let mut risk_sum = vec![0.0; p];
        let mut risk_cross = vec![vec![0.0; p]; p];
        for index in &risk {
            let weight = weights[*index];
            risk_weight += weight;
            for left in 0..p {
                risk_sum[left] += weight * observations[*index].covariates[left];
                for right in 0..p {
                    risk_cross[left][right] += weight
                        * observations[*index].covariates[left]
                        * observations[*index].covariates[right];
                }
            }
        }
        let mut dead_weight = 0.0;
        let mut dead_sum = vec![0.0; p];
        let mut dead_cross = vec![vec![0.0; p]; p];
        for index in &dead {
            let weight = weights[*index];
            dead_weight += weight;
            for left in 0..p {
                dead_sum[left] += weight * observations[*index].covariates[left];
                for right in 0..p {
                    dead_cross[left][right] += weight
                        * observations[*index].covariates[left]
                        * observations[*index].covariates[right];
                }
            }
        }
        for index in &dead {
            let eta: f64 = observations[*index]
                .covariates
                .iter()
                .zip(beta)
                .map(|(value, coefficient)| value * coefficient)
                .sum();
            log_likelihood += eta;
        }
        for tied in 0..deaths {
            let share = tied as f64 / deaths as f64;
            let denominator = risk_weight - share * dead_weight;
            log_likelihood -= denominator.ln();
            for left in 0..p {
                let numerator = risk_sum[left] - share * dead_sum[left];
                gradient[left] -= numerator / denominator;
                for right in 0..p {
                    // Minus the second derivative: the observed information.
                    information[left][right] +=
                        (risk_cross[left][right] - share * dead_cross[left][right]) / denominator
                            - numerator * (risk_sum[right] - share * dead_sum[right])
                                / (denominator * denominator);
                }
            }
        }
    }
    for observation in observations {
        if observation.event {
            for (left, value) in observation.covariates.iter().enumerate() {
                gradient[left] += value;
            }
        }
    }
    (log_likelihood, gradient, information)
}

/// Solves `matrix·x = rhs` by Gaussian elimination with partial pivoting.
fn solve(matrix: &[Vec<f64>], rhs: &[f64]) -> Option<Vec<f64>> {
    let size = matrix.len();
    let mut augmented: Vec<Vec<f64>> = matrix
        .iter()
        .zip(rhs)
        .map(|(row, value)| {
            let mut values = row.clone();
            values.push(*value);
            values
        })
        .collect();
    for column in 0..size {
        let mut pivot = column;
        for row in (column + 1)..size {
            if augmented[row][column].abs() > augmented[pivot][column].abs() {
                pivot = row;
            }
        }
        if augmented[pivot][column].abs() < 1e-14 {
            return None;
        }
        augmented.swap(column, pivot);
        let scale = augmented[column][column];
        for value in augmented[column].iter_mut() {
            *value /= scale;
        }
        let pivot_row = augmented[column].clone();
        for (row, values) in augmented.iter_mut().enumerate() {
            if row == column {
                continue;
            }
            let factor = values[column];
            if factor == 0.0 {
                continue;
            }
            for (value, base) in values.iter_mut().zip(&pivot_row) {
                *value -= factor * base;
            }
        }
    }
    Some((0..size).map(|row| augmented[row][size]).collect())
}

/// Gauss–Jordan inverse with partial pivoting.
#[allow(clippy::needless_range_loop)]
fn invert(matrix: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let size = matrix.len();
    let mut inverse = vec![vec![0.0; size]; size];
    for row in 0..size {
        let mut rhs = vec![0.0; size];
        rhs[row] = 1.0;
        let column = solve(matrix, &rhs)?;
        for index in 0..size {
            inverse[index][row] = column[index];
        }
    }
    Some(inverse)
}

/// Fits the Cox model of the given covariates.
///
/// # Errors
///
/// Returns [`Error::EmptySample`] without complete rows, [`Error::Cell`] for a
/// non-positive time, [`Error::Unsupported`] for fewer observations than
/// covariates or no event, and [`Error::NotConverged`] when Newton–Raphson does
/// not settle.
#[allow(clippy::needless_range_loop)]
pub fn fit(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
    covariates: &[String],
    confidence: f64,
) -> Result<CoxFit> {
    if !confidence.is_finite() || !(0.5..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence level",
            value: confidence,
        });
    }
    if covariates.is_empty() {
        return Err(Error::Unsupported(
            "the regression needs at least one covariate".to_owned(),
        ));
    }
    if covariates.len() > 10 {
        return Err(Error::Unsupported(
            "at most 10 covariates are accepted".to_owned(),
        ));
    }
    let mut seen = covariates.to_vec();
    seen.sort();
    seen.dedup();
    if seen.len() != covariates.len() {
        return Err(Error::Unsupported(
            "the covariates must be distinct columns".to_owned(),
        ));
    }
    let time = df.column(time_column)?;
    let censor = match censor_column {
        Some(name) => Some(df.column(name)?),
        None => None,
    };
    let mut sources = Vec::with_capacity(covariates.len());
    for name in covariates {
        sources.push(df.column(name)?.numeric_by_row());
    }
    let mut observations = Vec::with_capacity(df.n_rows());
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        let Some(value) = time.value(row) else {
            excluded += 1;
            continue;
        };
        if value <= 0.0 {
            return Err(Error::Cell {
                column: time_column.to_owned(),
                row: row + 1,
                message: "survival times must be positive".to_owned(),
            });
        }
        let censored = match &censor {
            Some(column) => match column.value(row) {
                Some(flag) => (flag - censor_value).abs() <= f64::EPSILON,
                None => {
                    excluded += 1;
                    continue;
                }
            },
            None => false,
        };
        let mut values = Vec::with_capacity(covariates.len());
        let mut complete = true;
        for source in &sources {
            if !source[row].is_finite() {
                complete = false;
                break;
            }
            values.push(source[row]);
        }
        if !complete {
            excluded += 1;
            continue;
        }
        observations.push(Observation {
            time: value,
            event: !censored,
            covariates: values,
        });
    }
    let n = observations.len();
    let p = covariates.len();
    if n == 0 {
        return Err(Error::EmptySample);
    }
    let events = observations.iter().filter(|value| value.event).count();
    if events == 0 {
        return Err(Error::Unsupported(
            "the sample has no events; the regression cannot be fitted".to_owned(),
        ));
    }
    if n <= p + 1 {
        return Err(Error::Unsupported(format!(
            "the regression needs more than p + 1 = {} observations, found {n}",
            p + 1
        )));
    }
    const MAX_OBSERVATIONS: usize = 5000;
    if n > MAX_OBSERVATIONS {
        return Err(Error::Unsupported(format!(
            "at most {MAX_OBSERVATIONS} complete observations are accepted"
        )));
    }

    let mut beta = vec![0.0f64; p];
    let (null_log_likelihood, _, _) = terms(&observations, &beta);
    let mut log_likelihood = null_log_likelihood;
    let mut iterations = 0usize;
    for step in 0..60 {
        iterations = step + 1;
        let (_, gradient, information) = terms(&observations, &beta);
        let largest_gradient = gradient
            .iter()
            .fold(0.0f64, |largest, value| largest.max(value.abs()));
        if largest_gradient < 1e-10 {
            break;
        }
        // The observed information is minus the Hessian, so the Newton step
        // solves I·δ = g.
        let direction = solve(&information, &gradient).ok_or_else(|| Error::NotConverged {
            detail: "the Cox information matrix is singular".to_owned(),
        })?;
        let mut scale = 1.0f64;
        let mut advanced = false;
        for _ in 0..30 {
            let candidate: Vec<f64> = beta
                .iter()
                .zip(&direction)
                .map(|(value, step)| value + scale * step)
                .collect();
            let (candidate_likelihood, _, _) = terms(&observations, &candidate);
            if candidate_likelihood >= log_likelihood {
                beta = candidate;
                log_likelihood = candidate_likelihood;
                advanced = true;
                break;
            }
            scale *= 0.5;
        }
        if !advanced {
            break;
        }
        let largest: f64 = direction
            .iter()
            .fold(0.0f64, |largest, value| largest.max(value.abs()));
        if largest * scale < 1e-12 {
            break;
        }
    }
    if !log_likelihood.is_finite() {
        return Err(Error::NotConverged {
            detail: "the Cox likelihood did not settle".to_owned(),
        });
    }
    let (_, gradient, information) = terms(&observations, &beta);
    let largest_gradient = gradient
        .iter()
        .fold(0.0f64, |largest, value| largest.max(value.abs()));
    if largest_gradient > 1e-4 {
        return Err(Error::NotConverged {
            detail: "the Cox likelihood did not settle".to_owned(),
        });
    }
    let covariance = invert(&information).ok_or_else(|| Error::NotConverged {
        detail: "the Cox information matrix is singular".to_owned(),
    })?;
    let quantile = normal_quantile(1.0 - (1.0 - confidence) / 2.0);
    let mut standard_errors = Vec::with_capacity(p);
    let mut z = Vec::with_capacity(p);
    let mut p_values = Vec::with_capacity(p);
    let mut hazard_ratios = Vec::with_capacity(p);
    let mut hr_lower = Vec::with_capacity(p);
    let mut hr_upper = Vec::with_capacity(p);
    for index in 0..p {
        let variance = covariance[index][index];
        if !variance.is_finite() || variance <= 0.0 {
            return Err(Error::NotConverged {
                detail: "the Cox covariance is not positive".to_owned(),
            });
        }
        let error = variance.sqrt();
        let statistic = beta[index] / error;
        standard_errors.push(error);
        z.push(statistic);
        p_values.push((2.0 * (1.0 - normal_cdf(statistic.abs()))).clamp(0.0, 1.0));
        let ratio = beta[index].exp();
        hazard_ratios.push(ratio);
        hr_lower.push((beta[index] - quantile * error).exp());
        hr_upper.push((beta[index] + quantile * error).exp());
    }
    let lr_statistic = (2.0 * (log_likelihood - null_log_likelihood)).max(0.0);
    let lr_p_value = (1.0 - chi_square_cdf(lr_statistic, p as f64)).clamp(0.0, 1.0);
    Ok(CoxFit {
        time: time_column.to_owned(),
        covariates: covariates.to_vec(),
        coefficients: beta,
        standard_errors,
        z,
        p_values,
        hazard_ratios,
        hr_lower,
        hr_upper,
        log_likelihood,
        null_log_likelihood,
        lr_statistic,
        lr_p_value,
        iterations,
        n,
        events,
        censored: n - events,
        excluded,
        confidence,
    })
}

/// Runs the regression and builds the report.
pub fn analyze(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
    covariates: &[String],
    confidence: f64,
) -> Result<AnalysisOutput> {
    let model = fit(
        df,
        time_column,
        censor_column,
        censor_value,
        covariates,
        confidence,
    )?;
    let mut result = AnalysisResult::new(
        format!("Cox Regression — time {time_column}"),
        format!(
            "Proportional hazards with {} covariate(s), Efron ties, {:.1}% confidence",
            model.covariates.len(),
            model.confidence * 100.0
        ),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Observations", model.n as f64),
        ("Events", model.events as f64),
        ("Censored", model.censored as f64),
        ("Covariates", model.covariates.len() as f64),
        ("Log-likelihood", model.log_likelihood),
        ("Null log-likelihood", model.null_log_likelihood),
        ("LR chi-square", model.lr_statistic),
        ("LR p-value", model.lr_p_value),
        ("Iterations", model.iterations as f64),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Covariate".to_owned(),
            "Coefficient".to_owned(),
            "Std. error".to_owned(),
            "z".to_owned(),
            "p-value".to_owned(),
            "Hazard ratio".to_owned(),
            "CI lower".to_owned(),
            "CI upper".to_owned(),
        ],
        rows: model
            .covariates
            .iter()
            .enumerate()
            .map(|(index, name)| {
                vec![
                    name.clone(),
                    format!("{:.6}", model.coefficients[index]),
                    format!("{:.6}", model.standard_errors[index]),
                    format!("{:.6}", model.z[index]),
                    format!("{:.6}", model.p_values[index]),
                    format!("{:.6}", model.hazard_ratios[index]),
                    format!("{:.6}", model.hr_lower[index]),
                    format!("{:.6}", model.hr_upper[index]),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Model".to_owned(),
            "h(t|x) = h₀(t)·exp(xᵀβ); the coefficients are fitted by maximum partial likelihood \
             with Efron's handling of tied event times"
                .to_owned(),
        ),
        (
            "Hazard ratio".to_owned(),
            format!(
                "exp(β): the multiplier of the hazard per unit of the covariate, with the {:.1}% \
                 interval exp(β ± z·SE)",
                model.confidence * 100.0
            ),
        ),
        (
            "Test".to_owned(),
            "the likelihood-ratio test compares the model with the intercept-only one; each \
             coefficient carries a Wald z test"
                .to_owned(),
        ),
        (
            "Assumption".to_owned(),
            "the hazard ratio is assumed constant over time; the proportional-hazards assumption \
             is not tested here"
                .to_owned(),
        ),
        (
            "Censoring".to_owned(),
            format!(
                "an observation is censored when its flag equals {censor_value}; censored \
                 observations stay in the risk set up to their time"
            ),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing time, covariate or flag are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the proportional-hazards assumption is not tested; the hazard ratios are only \
         meaningful while it holds"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame(times: Vec<f64>, events: Vec<f64>, covariates: Vec<(&str, Vec<f64>)>) -> DataFrame {
        let mut columns = vec![
            Column::new_f64("time", times),
            Column::new_f64("event", events),
        ];
        for (name, values) in covariates {
            columns.push(Column::new_f64(name, values));
        }
        DataFrame::new(columns).expect("frame")
    }

    #[test]
    fn the_single_covariate_fit_matches_the_partial_likelihood() {
        // Times 1..6, all events, x = 0, 1, 0, 1, 0, 1.
        let df = frame(
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            vec![1.0; 6],
            vec![("x", vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0])],
        );
        let model = fit(&df, "time", Some("event"), 0.0, &["x".to_owned()], 0.95).expect("fit");
        assert!((model.coefficients[0] + 0.632_051_696_154_372_2).abs() < 1e-9);
        assert!((model.standard_errors[0] - 0.921_982_104_134_099_2).abs() < 1e-9);
        assert!((model.hazard_ratios[0] - 0.531_500_204_651_862).abs() < 1e-9);
        assert!((model.z[0] + 0.685_535_753_156_486_8).abs() < 1e-9);
        assert!((model.p_values[0] - 0.493_005_907_371_207_2).abs() < 1e-9);
        assert!((model.log_likelihood + 6.338_172_708_403_64).abs() < 1e-9);
        assert!((model.null_log_likelihood + 6.579_251_212_010_101).abs() < 1e-9);
        assert!((model.lr_statistic - 0.482_157_007_212_922_65).abs() < 1e-9);
        assert!((model.lr_p_value - 0.487_446_902_140_953_1).abs() < 1e-9);
        assert!((model.hr_lower[0] - 0.087_239_033_647_963_26).abs() < 1e-9);
        assert!((model.hr_upper[0] - 3.238_143_016_174_577_2).abs() < 1e-9);
    }

    #[test]
    fn two_covariates_with_censoring_match_the_hand_calculation() {
        let df = frame(
            vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            vec![1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0],
            vec![
                ("x1", vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]),
                ("x2", vec![1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0]),
            ],
        );
        let model = fit(
            &df,
            "time",
            Some("event"),
            0.0,
            &["x1".to_owned(), "x2".to_owned()],
            0.95,
        )
        .expect("fit");
        assert!((model.coefficients[0] + 0.086_226_656_049_593_19).abs() < 1e-8);
        assert!((model.coefficients[1] - 0.509_413_888_472_849_5).abs() < 1e-8);
        assert!((model.hazard_ratios[0] - 0.917_386_276_504_560_5).abs() < 1e-8);
        assert!((model.hazard_ratios[1] - 1.664_315_434_560_604_4).abs() < 1e-8);
        assert!((model.log_likelihood + 7.001_914_468_988_315).abs() < 1e-8);
        assert!((model.lr_statistic - 0.273_905_061_914_417_66).abs() < 1e-8);
        assert!((model.lr_p_value - 0.872_011_618_706_511_4).abs() < 1e-8);
        assert_eq!(model.censored, 1);
    }

    #[test]
    fn efron_ties_match_the_hand_calculation() {
        // Times 1, 1, 2, 3 with x = 0, 1, 1, 0: the fit is ln 2.
        let df = frame(
            vec![1.0, 1.0, 2.0, 3.0],
            vec![1.0; 4],
            vec![("x", vec![0.0, 1.0, 1.0, 0.0])],
        );
        let model = fit(&df, "time", Some("event"), 0.0, &["x".to_owned()], 0.95).expect("fit");
        assert!((model.coefficients[0] - 0.693_147_180_559_945_4).abs() < 1e-9);
        assert!((model.standard_errors[0] - 1.224_744_871_391_589).abs() < 1e-9);
        assert!((model.hazard_ratios[0] - 2.0).abs() < 1e-9);
        assert!((model.log_likelihood + 3.008_154_793_552_548_3).abs() < 1e-9);
        assert!((model.lr_statistic - 0.339_798_073_590_794_84).abs() < 1e-9);
        assert!((model.lr_p_value - 0.559_945_800_853_630_9).abs() < 1e-9);
        assert!((model.p_values[0] - 0.571_426_204_958_314_5).abs() < 1e-9);
    }

    #[test]
    fn the_report_carries_the_hazard_ratios() {
        let df = frame(
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            vec![1.0; 6],
            vec![("x", vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0])],
        );
        let out =
            analyze(&df, "time", Some("event"), 0.0, &["x".to_owned()], 0.95).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Cox Regression"), "{text}");
        assert!(text.contains("Hazard ratio"), "{text}");
        assert!(text.contains("LR chi-square"), "{text}");
        assert!(out.plot.is_none());
    }

    #[test]
    fn invalid_cox_settings_are_rejected() {
        let df = frame(
            vec![1.0, 2.0, 3.0, 4.0],
            vec![1.0; 4],
            vec![("x", vec![0.0, 1.0, 0.0, 1.0])],
        );
        assert!(matches!(
            fit(&df, "time", Some("event"), 0.0, &[], 0.95),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            fit(&df, "time", Some("event"), 0.0, &["x".to_owned()], 0.2),
            Err(Error::InvalidParameter { .. })
        ));
        let censored = frame(
            vec![1.0, 2.0, 3.0],
            vec![0.0, 0.0, 0.0],
            vec![("x", vec![0.0, 1.0, 0.0])],
        );
        assert!(matches!(
            fit(
                &censored,
                "time",
                Some("event"),
                0.0,
                &["x".to_owned()],
                0.95
            ),
            Err(Error::Unsupported(_))
        ));
        let negative = frame(
            vec![-1.0, 2.0, 3.0, 4.0],
            vec![1.0; 4],
            vec![("x", vec![0.0, 1.0, 0.0, 1.0])],
        );
        assert!(matches!(
            fit(
                &negative,
                "time",
                Some("event"),
                0.0,
                &["x".to_owned()],
                0.95
            ),
            Err(Error::Cell { .. })
        ));
    }
}
