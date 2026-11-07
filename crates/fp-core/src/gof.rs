// SPDX-License-Identifier: MIT
//! Goodness of fit: Anderson–Darling, information criteria and p-values.
//!
//! Decision D14: the p-value of Anderson–Darling with **estimated** parameters
//! uses the D'Agostino–Stephens composite-AD approximation, recovered from
//! the documented behaviour . It is deterministic and is derived for the same
//! null as the statistic, so it avoids critical values that assume known
//! parameters. The Monte Carlo version is kept as an independent cross-check.

use crate::data::{DataFrame, MissingPolicy, Sample};
use crate::dists::{Distribution, Normal};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{SplitMix64, chi_square_cdf, student_t_cdf};

/// Anderson–Darling statistic for a specified distribution.
///
/// `A² = −n − (1/n) Σ (2i − 1) · [ln F(x₍ᵢ₎) + ln(1 − F(x₍ₙ₊₁₋ᵢ₎))]`
///
/// Note: this is the classic form, without correction for estimated
/// parameters. The correction is not applied to the statistic; it is the
/// **p-value** that is obtained by simulation under the same conditions
/// (parameters estimated from each replicate), which is the defensible form.
///
/// The probabilities are clamped to `[MIN_POSITIVE, 1 − ε]`: when the model is
/// so bad that the CDF saturates, the returned value is a **lower bound** of
/// the true A², not an exact value. We prefer that to refusing the diagnostic
/// exactly in the cases where it is most needed — and the limitation is
/// recorded in the report.
pub fn anderson_darling<D: Distribution>(dist: &D, sample: &Sample) -> Result<f64> {
    let n = sample.n();
    if n == 0 {
        return Err(Error::EmptySample);
    }
    let sorted = sample.sorted();
    let nf = n as f64;
    let mut acc = 0.0;
    for (i, &x) in sorted.iter().enumerate() {
        let f_low = dist.cdf(x).clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON);
        let f_high = dist
            .cdf(sorted[n - 1 - i])
            .clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON);
        let k = (2 * i + 1) as f64;
        acc += k * (f_low.ln() + (1.0 - f_high).ln());
    }
    Ok(-nf - acc / nf)
}

/// Akaike information criterion, `AIC = −2ℓ + 2k`.
#[must_use]
pub fn aic(log_likelihood: f64, n_params: usize) -> f64 {
    -2.0 * log_likelihood + 2.0 * n_params as f64
}

/// Bayesian information criterion, `BIC = −2ℓ + k·ln(n)`.
#[must_use]
pub fn bic(log_likelihood: f64, n_params: usize, n: usize) -> f64 {
    -2.0 * log_likelihood + n_params as f64 * (n as f64).ln()
}

/// Anderson–Darling p-value for a fitted Normal, by the composite-AD
/// approximation used by the documented behaviour.
///
/// The statistic is first scaled by
/// `t = A² · (1 + 0.75/n + 2.25/n²)` and then mapped by the piecewise
/// D'Agostino–Stephens form:
///
/// | `t` | `p` |
/// |---|---|
/// | `≤ 0.2` | `1 − exp(−13.436 + 101.14 t − 223.73 t²)` |
/// | `≤ 0.34` | `1 − exp(−8.318 + 42.796 t − 59.938 t²)` |
/// | `≤ 0.6` | `exp(0.9177 − 4.279 t − 1.38 t²)` |
/// | `≤ 1.159` | `exp(1.2937 − 5.709 t + 0.0186 t²)` |
/// | `> 1.159` | `0.005` (the documented behaviour's clamp) |
///
/// Valid for the composite hypothesis, i.e. when the mean and standard
/// deviation were estimated from the same sample. Recovered from
/// `reference NormalADTest` / `0x1800051b0`; see `NATIVE_FORMULAS.md` §6.2 in the
/// external evidence notebook.
///
/// # Errors
///
/// `n < 2`, or a non-finite `A²`.
pub fn ad_p_value(n: usize, a2: f64) -> Result<f64> {
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    if !a2.is_finite() {
        return Err(Error::InvalidParameter {
            name: "a2",
            value: a2,
        });
    }
    let nf = n as f64;
    let t = a2 * (1.0 + 0.75 / nf + 2.25 / (nf * nf));
    let p = if t <= 0.2 {
        -(-13.436 + 101.14 * t - 223.73 * t * t).exp_m1()
    } else if t <= 0.34 {
        -(-8.318 + 42.796 * t - 59.938 * t * t).exp_m1()
    } else if t <= 0.6 {
        (0.9177 - 4.279 * t - 1.38 * t * t).exp()
    } else if t <= 1.159 {
        (1.2937 - 5.709 * t + 0.0186 * t * t).exp()
    } else {
        0.005
    };
    Ok(p.clamp(0.0, 1.0))
}

/// Anderson–Darling p-value for a fitted Weibull, from the small-sample table
/// recovered from the documented behaviour (Weibull AD routine).
///
/// `A* = A²·(1 + 0.2/√n)`, interpolated linearly between
/// `0.474→0.25`, `0.637→0.10`, `0.757→0.05`, `0.877→0.025`, `1.038→0.01`.
/// Below/above the table the value is clamped to `0.25`/`0.01`, as in the
/// binary. This is deliberately **not** the normal formula.
///
/// # Errors
///
/// `n < 2` or a non-finite `A²`.
pub fn ad_p_value_weibull(n: usize, a2: f64) -> Result<f64> {
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    if !a2.is_finite() {
        return Err(Error::InvalidParameter {
            name: "a2",
            value: a2,
        });
    }
    const TABLE: [(f64, f64); 5] = [
        (0.474, 0.25),
        (0.637, 0.10),
        (0.757, 0.05),
        (0.877, 0.025),
        (1.038, 0.01),
    ];
    let scaled = a2 * (1.0 + 0.2 / (n as f64).sqrt());
    if scaled <= TABLE[0].0 {
        return Ok(TABLE[0].1);
    }
    if scaled >= TABLE[4].0 {
        return Ok(TABLE[4].1);
    }
    for pair in TABLE.windows(2) {
        let (a0, p0) = pair[0];
        let (a1, p1) = pair[1];
        if scaled <= a1 {
            let p = p0 + (p1 - p0) * (scaled - a0) / (a1 - a0);
            return Ok(p);
        }
    }
    Ok(TABLE[4].1)
}

/// Anderson–Darling p-value for a fitted exponential, from the four-branch
/// formula recovered from the documented behaviour (exponential AD routine).
///
/// `A* = A²·(1 + 0.6/n)` and the branches are:
/// `≤0.26`: `1 − exp(67.459A* − 110.3A*² − 12.2204)`;
/// `≤0.51`: `1 − exp(20.218A* − 18.663A*² − 6.1327)`;
/// `≤0.95`: `exp(0.9209 − 3.353A* + 0.3A*²)`;
/// `≤2.534`: `exp(0.731 − 3.009A* + 0.15A*²)`;
/// above that, `0.01` (the documented behaviour's clamp).
///
/// # Errors
///
/// `n < 2` or a non-finite `A²`.
pub fn ad_p_value_exponential(n: usize, a2: f64) -> Result<f64> {
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    if !a2.is_finite() {
        return Err(Error::InvalidParameter {
            name: "a2",
            value: a2,
        });
    }
    let scaled = a2 * (1.0 + 0.6 / n as f64);
    let p = if scaled <= 0.26 {
        -(67.459 * scaled - 110.3 * scaled * scaled - 12.2204).exp_m1()
    } else if scaled <= 0.51 {
        -(20.218 * scaled - 18.663 * scaled * scaled - 6.1327).exp_m1()
    } else if scaled <= 0.95 {
        (0.9209 - 3.353 * scaled + 0.3 * scaled * scaled).exp()
    } else if scaled <= 2.534 {
        (0.731 - 3.009 * scaled + 0.15 * scaled * scaled).exp()
    } else {
        0.01
    };
    Ok(p.clamp(0.0, 1.0))
}

/// Anderson–Darling p-value by Monte Carlo, for a fitted Normal.
///
/// Generates `reps` samples of size `n` from the standard Normal, fits each one
/// with the sample moments (`n − 1`) that the probability-plot path uses, and
/// recomputes `A²`. The p-value is the fraction of replicates with `A²` at
/// least as large as the observed one. Explicit seed ⇒ reproducible result.
///
/// Cost: `reps × n`. For `reps = 10 000` and `n = 1000` that is 10 M normals;
/// call outside the draw loop.
pub fn ad_p_value_mc(n: usize, a2_observed: f64, reps: usize, seed: u64) -> Result<f64> {
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    if reps == 0 {
        return Err(Error::InvalidParameter {
            name: "reps",
            value: 0.0,
        });
    }
    if !a2_observed.is_finite() {
        return Err(Error::InvalidParameter {
            name: "a2",
            value: a2_observed,
        });
    }
    let mut rng = SplitMix64::new(seed);
    let mut buf = vec![0.0f64; n];
    let mut at_least_as_extreme = 0usize;
    for _ in 0..reps {
        for slot in &mut buf {
            *slot = rng.next_normal();
        }
        let sample = Sample::new(buf.clone(), crate::data::MissingPolicy::Drop)?;
        let dist = Normal::fit_moments(&sample)?;
        let a2 = anderson_darling(&dist, &sample)?;
        if a2 >= a2_observed {
            at_least_as_extreme += 1;
        }
    }
    Ok((at_least_as_extreme as f64 + 1.0) / (reps as f64 + 1.0))
}

/// Outcome of the chi-square goodness-of-fit test.
#[derive(Debug, Clone, PartialEq)]
pub struct ChiSquareGoodnessOfFit {
    /// Chi-square statistic.
    pub statistic: f64,
    /// Degrees of freedom of the test.
    pub degrees_of_freedom: usize,
    /// p-value of the test.
    pub p_value: f64,
    /// Contribution of every cell to the statistic.
    pub contributions: Vec<f64>,
}

/// Chi-square goodness-of-fit test of observed against expected counts.
///
/// `X² = Σ (Oᵢ − Eᵢ)² / Eᵢ` with `k − 1 − m` degrees of freedom, where `m`
/// is the number of parameters estimated from the data. Every expected count
/// must be positive.
pub fn chi_square_goodness_of_fit(
    observed: &[f64],
    expected: &[f64],
    estimated_parameters: usize,
) -> Result<ChiSquareGoodnessOfFit> {
    if observed.len() != expected.len() {
        return Err(Error::Unsupported(
            "the observed and expected counts must have the same number of cells".to_owned(),
        ));
    }
    let cells = observed.len();
    if cells < 2 {
        return Err(Error::Unsupported(
            "the goodness-of-fit test needs at least two cells".to_owned(),
        ));
    }
    let mut contributions = Vec::with_capacity(cells);
    for (index, (&o, &e)) in observed.iter().zip(expected).enumerate() {
        if !o.is_finite() || !e.is_finite() {
            return Err(Error::InvalidParameter {
                name: "count of a cell",
                value: if o.is_finite() { o } else { e },
            });
        }
        if o < 0.0 {
            return Err(Error::InvalidParameter {
                name: "observed count of a cell",
                value: o,
            });
        }
        if e <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "expected count of a cell",
                value: e,
            });
        }
        let _ = index;
        contributions.push((o - e) * (o - e) / e);
    }
    let degrees_of_freedom = cells
        .checked_sub(1 + estimated_parameters)
        .filter(|df| *df > 0)
        .ok_or_else(|| {
            Error::Unsupported(
                "the test needs more cells than the degrees of freedom it consumes".to_owned(),
            )
        })?;
    let statistic: f64 = contributions.iter().sum();
    let p_value = (1.0 - chi_square_cdf(statistic, degrees_of_freedom as f64)).clamp(0.0, 1.0);
    Ok(ChiSquareGoodnessOfFit {
        statistic,
        degrees_of_freedom,
        p_value,
        contributions,
    })
}

/// Outcome of Grubbs' test for a single outlier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grubbs {
    /// Number of observations.
    pub n: usize,
    /// Grubbs statistic (the largest standardised deviation).
    pub statistic: f64,
    /// Value of the flagged observation.
    pub value: f64,
    /// Index (one-based) of the flagged observation in the sorted sample.
    pub index: usize,
    /// Critical value of the statistic at the requested level.
    pub critical_value: f64,
    /// p-value of the test.
    pub p_value: f64,
    /// `true` when the statistic exceeds the critical value.
    pub rejected: bool,
}

/// Grubbs' test for a single outlier.
///
/// `G = max|xᵢ − x̄| / s` with `s` the sample standard deviation (divisor
/// `n − 1`). The p-value is obtained from the Student t distribution: the
/// statistic corresponds to
/// `t = G·√(n(n−2) / ((n−1)² − nG²))` and `p = 2n·(1 − F_t(t))`.
pub fn grubbs_test(sample: &Sample, alpha: f64) -> Result<Grubbs> {
    let n = sample.n();
    if n < 3 {
        return Err(Error::SampleTooSmall { n, min: 3 });
    }
    if !alpha.is_finite() || !(0.0..1.0).contains(&alpha) {
        return Err(Error::InvalidParameter {
            name: "alpha",
            value: alpha,
        });
    }
    let mean = sample.mean()?;
    let sd = sample.sd()?;
    if !sd.is_finite() || sd <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let sorted = sample.sorted();
    let mut best = 0usize;
    let mut best_distance = 0.0;
    for (index, &value) in sorted.iter().enumerate() {
        let distance = (value - mean).abs();
        if distance > best_distance {
            best_distance = distance;
            best = index;
        }
    }
    let statistic = best_distance / sd;
    let nf = n as f64;
    let t_value = grubbs_t_value(statistic, n);
    let p_value = (2.0 * nf * (1.0 - student_t_cdf(t_value, nf - 2.0))).clamp(0.0, 1.0);
    // The critical value is the statistic whose p-value is exactly alpha.
    let critical_value = grubbs_critical_value(n, alpha);
    Ok(Grubbs {
        n,
        statistic,
        value: sorted[best],
        index: best + 1,
        critical_value,
        p_value,
        rejected: statistic > critical_value,
    })
}

/// Critical value of Grubbs' statistic for `n` observations and level `alpha`.
///
/// `G_crit = ((n−1)/√n)·√(t²/(n−2+t²))` with
/// `t = t_{1−alpha/(2n), n−2}`.
fn grubbs_critical_value(n: usize, alpha: f64) -> f64 {
    let nf = n as f64;
    // alpha = 2n(1 − F(t)) ⇒ F(t) = 1 − alpha/(2n) ⇒ t = quantile.
    let t_value = crate::special::student_t_quantile(1.0 - alpha / (2.0 * nf), nf - 2.0);
    let t_squared = t_value * t_value;
    ((nf - 1.0) / nf.sqrt()) * (t_squared / (nf - 2.0 + t_squared)).sqrt()
}

/// Student t value that corresponds to a Grubbs statistic.
fn grubbs_t_value(statistic: f64, n: usize) -> f64 {
    let nf = n as f64;
    let denominator = (nf - 1.0) * (nf - 1.0) - nf * statistic * statistic;
    if denominator <= 0.0 {
        return f64::INFINITY;
    }
    statistic * (nf * (nf - 2.0) / denominator).sqrt()
}

/// Builds the Session report of the chi-square goodness-of-fit test.
pub fn chi_square_goodness_of_fit_report(
    df: &DataFrame,
    column_name: &str,
    expected: Vec<f64>,
    estimated_parameters: usize,
) -> Result<AnalysisResult> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let observed: Vec<f64> = sample.values().to_vec();
    let result = chi_square_goodness_of_fit(&observed, &expected, estimated_parameters)?;

    let mut report = AnalysisResult::new(
        format!("Chi-Square Goodness-of-Fit — column {column_name}"),
        "Chi-square test of the observed counts against the expected counts".to_owned(),
    );
    report.n = observed.len();
    report.excluded = sample.excluded();
    report.statistics = vec![
        ("Chi-square", result.statistic),
        ("df", result.degrees_of_freedom as f64),
        ("p-value", result.p_value),
    ];
    let rows = observed
        .iter()
        .zip(&expected)
        .zip(&result.contributions)
        .enumerate()
        .map(|(index, ((observed, expected), contribution))| {
            vec![
                (index + 1).to_string(),
                format!("{observed:.6}"),
                format!("{expected:.6}"),
                format!("{contribution:.6}"),
            ]
        })
        .collect();
    report.extra.push(Item::Table {
        headers: vec![
            "Cell".to_owned(),
            "Observed".to_owned(),
            "Expected".to_owned(),
            "Contribution".to_owned(),
        ],
        rows,
    });
    report.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: the observations follow the expected distribution; H₁: they do not".to_owned(),
        ),
        (
            "Statistic".to_owned(),
            format!(
                "X² = Σ (Oᵢ − Eᵢ)²/Eᵢ with {} − 1 − {} degrees of freedom",
                expected.len(),
                estimated_parameters
            ),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted; every expected count must be positive"
                .to_owned(),
        ),
    ];
    report.warnings = standard_warnings(report.n, report.excluded, 0);
    Ok(report)
}

/// Builds the Session report of Grubbs' test.
pub fn grubbs_report(df: &DataFrame, column_name: &str, alpha: f64) -> Result<AnalysisResult> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let result = grubbs_test(&sample, alpha)?;

    let mut report = AnalysisResult::new(
        format!("Grubbs Test — column {column_name}"),
        format!("Single outlier test at the {:.0}% level", alpha * 100.0),
    );
    report.n = result.n;
    report.excluded = sample.excluded();
    report.statistics = vec![
        ("G", result.statistic),
        ("Critical value", result.critical_value),
        ("p-value", result.p_value),
        ("Outlier value", result.value),
    ];
    let decision = if result.rejected {
        "rejects"
    } else {
        "does not reject"
    };
    // Report the row as it appears in the worksheet, not the rank of the value.
    let row = (0..df.n_rows())
        .find(|row| column.value(*row) == Some(result.value))
        .map_or(result.index, |row| row + 1);
    report.extra.push(Item::Message(format!(
        "The largest standardised deviation is the observation {row} (value {:.6}); the test {} the \
         hypothesis of no outlier at the {:.0}% level.",
        result.value,
        decision,
        alpha * 100.0
    )));
    report.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: no outlier; H₁: one outlier".to_owned(),
        ),
        (
            "Statistic".to_owned(),
            "G = max|xᵢ − x̄| / s with the sample standard deviation (n−1)".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted".to_owned(),
        ),
    ];
    report.warnings = standard_warnings(report.n, report.excluded, 0);
    Ok(report)
}

#[cfg(test)]
mod tests {
    #[test]
    fn chi_square_goodness_of_fit_matches_the_definition() {
        let observed = [20.0, 30.0, 50.0];
        let expected = [25.0, 25.0, 50.0];
        let result = chi_square_goodness_of_fit(&observed, &expected, 0).expect("gof");
        // (20−25)²/25 + (30−25)²/25 + 0 = 2.
        assert!(
            (result.statistic - 2.0).abs() < 1e-12,
            "{}",
            result.statistic
        );
        assert_eq!(result.degrees_of_freedom, 2);
        assert!(
            (result.p_value - 0.367_879_4).abs() < 1e-6,
            "p {}",
            result.p_value
        );
        assert_eq!(result.contributions.len(), 3);
    }

    #[test]
    fn chi_square_goodness_of_fit_consumes_the_estimated_parameters() {
        let observed = [20.0, 30.0, 50.0];
        let expected = [25.0, 25.0, 50.0];
        let result = chi_square_goodness_of_fit(&observed, &expected, 1).expect("gof");
        assert_eq!(result.degrees_of_freedom, 1);
        assert!(matches!(
            chi_square_goodness_of_fit(&observed, &[1.0, 2.0], 0),
            Err(Error::Unsupported { .. })
        ));
        assert!(matches!(
            chi_square_goodness_of_fit(&observed, &[25.0, 25.0, 0.0], 0),
            Err(Error::InvalidParameter { .. })
        ));
    }

    #[test]
    fn grubbs_flags_the_outlier_and_keeps_the_critical_value() {
        let s = Sample::new(
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 100.0],
            MissingPolicy::Drop,
        )
        .unwrap();
        let result = grubbs_test(&s, 0.05).expect("grubbs");
        assert_eq!(result.value, 100.0);
        assert_eq!(result.index, 10);
        assert!(result.statistic > 2.5, "G {}", result.statistic);
        assert!(result.p_value < 0.01, "p {}", result.p_value);
        assert!(result.rejected);
        // The published two-sided critical value for n = 10 and alpha = 5 %.
        assert!(
            (result.critical_value - 2.290_0).abs() < 1e-3,
            "critical {}",
            result.critical_value
        );
    }

    #[test]
    fn grubbs_does_not_flag_a_clean_sample() {
        let s = Sample::new(
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
            MissingPolicy::Drop,
        )
        .unwrap();
        let result = grubbs_test(&s, 0.05).expect("grubbs");
        assert!(!result.rejected, "{result:?}");
        assert!(result.p_value > 0.05, "p {}", result.p_value);
    }

    use super::*;
    use crate::data::MissingPolicy;

    fn sample(values: &[f64]) -> Sample {
        Sample::new(values.to_vec(), MissingPolicy::Drop).unwrap()
    }

    #[test]
    fn a2_is_positive_for_normal_data() {
        let s = sample(&[1.02, 0.98, 1.05, 0.95, 1.01, 0.99, 1.03, 0.97]);
        let n = Normal::fit(&s).unwrap();
        let a2 = anderson_darling(&n, &s).unwrap();
        assert!(a2 > 0.0 && a2 < 2.0, "A² = {a2}");
    }

    #[test]
    fn a2_grows_with_deviation_from_the_model() {
        let s = sample(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);
        let good = Normal::fit(&s).unwrap();
        let bad = Normal::new(5.5, 0.2).unwrap();
        assert!(anderson_darling(&bad, &s).unwrap() > anderson_darling(&good, &s).unwrap());
    }

    #[test]
    fn a2_is_invariant_to_affine_transformation() {
        let s = sample(&[-2.0, -1.0, 0.0, 0.5, 1.0, 2.5, 3.0]);
        let d = Normal::fit(&s).unwrap();
        let shifted = sample(&[8.0, 14.0, 20.0, 23.0, 26.0, 35.0, 38.0]);
        let d2 = Normal::fit(&shifted).unwrap();
        let a = anderson_darling(&d, &s).unwrap();
        let b = anderson_darling(&d2, &shifted).unwrap();
        assert!((a - b).abs() < 1e-10, "{a} vs {b}");
    }

    #[test]
    fn saturated_cdf_returns_lower_bound_instead_of_failing() {
        // Absurdly narrow model: the CDF saturates, but the diagnostic must
        // still exist (with a large A² and the documented limitation).
        let s = sample(&[-1e6, 0.0, 1e6]);
        let d = Normal::new(0.0, 1e-9).unwrap();
        let a2 = anderson_darling(&d, &s).expect("must not fail");
        assert!(a2.is_finite() && a2 > 100.0, "A² = {a2}");
    }

    #[test]
    fn aic_and_bic_exact_arithmetic() {
        // ℓ = −10, k = 2, n = 100 → AIC = 24, BIC = 20 + 2·ln(100)
        assert!((aic(-10.0, 2) - 24.0).abs() < 1e-15);
        assert!((bic(-10.0, 2, 100) - (20.0 + 2.0 * 100f64.ln())).abs() < 1e-15);
        assert!(bic(-10.0, 2, 100) > aic(-10.0, 2) - 1e-15);
    }

    #[test]
    fn ad_p_value_matches_the_recovered_table() {
        // Reference values produced by an independent implementation of the
        // the documented behaviour formula (NATIVE_FORMULAS.md §6.2).
        let cases = [
            (2, 0.1, 0.893_407_215_070_051_4),
            (5, 0.2, 0.751_178_312_124_852_1),
            (10, 0.2, 0.836_690_127_480_800_1),
            (10, 0.5, 0.157_874_335_673_104_24),
            (30, 0.3, 0.560_151_808_149_938_1),
            (30, 0.5, 0.193_037_239_820_954_72),
            (30, 1.0, 0.010_539_128_400_373_654),
            (20, 0.312, 0.522_178_545_378_094),
        ];
        for (n, a2, expected) in cases {
            let got = ad_p_value(n, a2).unwrap();
            assert!(
                (got - expected).abs() < 1e-12,
                "n={n}, A²={a2}: got {got}, expected {expected}"
            );
        }
    }

    #[test]
    fn weibull_ad_p_value_uses_the_recovered_table() {
        // Recover the A* value for a given p by inverting the table at n=1? No:
        // choose A² so that A* hits each knot exactly (n is fixed by the test).
        let n = 100usize;
        let correction = 1.0 + 0.2 / (n as f64).sqrt();
        for (a_star, expected) in [
            (0.474, 0.25),
            (0.637, 0.10),
            (0.757, 0.05),
            (0.877, 0.025),
            (1.038, 0.01),
        ] {
            let p = ad_p_value_weibull(n, a_star / correction).unwrap();
            assert!((p - expected).abs() < 1e-12, "A*={a_star}: {p}");
        }
        let mid = ad_p_value_weibull(n, 0.7 / correction).unwrap();
        assert!(mid < 0.10 && mid > 0.05, "{mid}");
        assert_eq!(ad_p_value_weibull(n, 0.0).unwrap(), 0.25);
        assert_eq!(ad_p_value_weibull(n, 10.0).unwrap(), 0.01);
        assert!(ad_p_value_weibull(1, 0.5).is_err());
    }

    #[test]
    fn exponential_ad_p_value_matches_the_recovered_formula() {
        let p = ad_p_value_exponential(20, 0.3).unwrap();
        assert!((p - 0.811_243_962_0).abs() < 1e-9);
        let p = ad_p_value_exponential(10, 0.5).unwrap();
        assert!((p - 0.462_124_936_0).abs() < 1e-9);
        assert_eq!(ad_p_value_exponential(20, 10.0).unwrap(), 0.01);
        assert!((0.0..=1.0).contains(&p));
        assert!(ad_p_value_exponential(1, 0.5).is_err());
    }

    #[test]
    fn ad_p_value_is_bounded_monotone_and_clamped() {
        let low = ad_p_value(20, 0.1).unwrap();
        let high = ad_p_value(20, 2.0).unwrap();
        assert!(low > high);
        assert_eq!(ad_p_value(20, 1.159 * 10.0).unwrap(), 0.005);
        for a2 in [0.0, 0.1, 0.5, 1.0, 5.0] {
            let p = ad_p_value(20, a2).unwrap();
            assert!((0.0..=1.0).contains(&p), "p({a2}) = {p}");
        }
    }

    #[test]
    fn ad_p_value_agrees_with_monte_carlo() {
        // Independent check: simulate under the fitted-normal null.
        let exact = ad_p_value(30, 0.5).unwrap();
        let mc = ad_p_value_mc(30, 0.5, 4000, 2026).unwrap();
        assert!(
            (exact - mc).abs() < 0.04,
            "exact {exact} vs Monte Carlo {mc}"
        );
    }

    #[test]
    fn ad_p_value_rejects_small_n_and_non_finite_a2() {
        assert!(ad_p_value(1, 0.5).is_err());
        assert!(ad_p_value(20, f64::NAN).is_err());
        assert!(ad_p_value(20, f64::INFINITY).is_err());
    }

    #[test]
    fn p_value_is_reproducible_and_decreases_with_a2() {
        // Fixed seed: same number of replicates ⇒ same p-value.
        let p1 = ad_p_value_mc(30, 0.5, 200, 2026).unwrap();
        let p2 = ad_p_value_mc(30, 0.5, 200, 2026).unwrap();
        assert_eq!(p1, p2);
        let high = ad_p_value_mc(30, 5.0, 200, 2026).unwrap();
        assert!(high < p1, "p(A²=5)={high} should be < p(A²=0.5)={p1}");
        assert!((0.0..=1.0).contains(&p1));
    }

    #[test]
    fn p_value_rejects_reps_zero_and_n_one() {
        assert!(ad_p_value_mc(30, 0.5, 0, 1).is_err());
        assert!(ad_p_value_mc(1, 0.5, 10, 1).is_err());
    }
}
