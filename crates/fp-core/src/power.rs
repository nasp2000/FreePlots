// SPDX-License-Identifier: MIT
//! Power and sample size for the tests the engine implements.
//!
//! The power of a test is the probability of rejecting the null hypothesis
//! when the effect is the one the user cares about. Two of the three
//! procedures are exact under the model they assume:
//!
//! * the **t tests** use the non-central t distribution, integrated over the
//!   chi-square that carries the variance estimate;
//! * the **proportion test** uses the normal approximation of the sample
//!   proportion under the true proportion;
//! * the **correlation test** uses Fisher's `atanh` transformation.
//!
//! The documented behaviour has a power-and-sample-size dialog; its defaults
//! were not documented, so the numbers are not claimed as documented behaviour —
//! see `the project notes`.

use crate::error::{Error, Result};
use crate::hypothesis::Alternative;
use crate::report::{AnalysisResult, Item};
use crate::special::{normal_cdf, normal_quantile, regularized_gamma_lower, student_t_quantile};

/// Procedure of a power and sample-size analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerProcedure {
    /// One-sample Student t test.
    OneSampleT,
    /// Two-sample Student t test with equal group sizes.
    TwoSampleT,
    /// One-sample proportion test.
    Proportion,
    /// Pearson correlation test.
    Correlation,
    /// One-way ANOVA.
    Anova,
    /// Overall F test of a regression.
    Regression,
}

impl PowerProcedure {
    /// Name used in reports.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::OneSampleT => "1-Sample t",
            Self::TwoSampleT => "2-Sample t",
            Self::Proportion => "1-Sample Proportion",
            Self::Correlation => "Pearson Correlation",
            Self::Anova => "One-Way ANOVA",
            Self::Regression => "Regression (overall F test)",
        }
    }

    /// Stable identifier for the CLI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OneSampleT => "t1",
            Self::TwoSampleT => "t2",
            Self::Proportion => "proportion",
            Self::Correlation => "correlation",
            Self::Anova => "anova",
            Self::Regression => "regression",
        }
    }

    /// Parses the CLI spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().replace('_', "-").as_str() {
            "t1" | "1-sample-t" | "one-sample-t" => Some(Self::OneSampleT),
            "t2" | "2-sample-t" | "two-sample-t" => Some(Self::TwoSampleT),
            "proportion" | "p1" | "1-sample-proportion" => Some(Self::Proportion),
            "correlation" | "pearson" | "r" => Some(Self::Correlation),
            "anova" | "one-way-anova" => Some(Self::Anova),
            "regression" | "reg" => Some(Self::Regression),
            _ => None,
        }
    }
}

/// Number of intervals used by the Simpson integration over the chi-square.
const INTEGRATION_INTERVALS: usize = 400;

/// Half-width of the integration range, in standard deviations of the
/// chi-square distribution.
const INTEGRATION_SPAN: f64 = 8.0;

fn validate_alpha(alpha: f64) -> Result<()> {
    if !alpha.is_finite() || !(0.0..1.0).contains(&alpha) {
        return Err(Error::InvalidParameter {
            name: "alpha",
            value: alpha,
        });
    }
    Ok(())
}

fn validate_power(power: f64) -> Result<()> {
    if !power.is_finite() || !(0.0..1.0).contains(&power) {
        return Err(Error::InvalidParameter {
            name: "power",
            value: power,
        });
    }
    Ok(())
}

/// Chi-square density with `df` degrees of freedom.
fn chi_square_density(value: f64, df: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    let half = df / 2.0;
    let ln_density = (half - 1.0) * value.ln()
        - value / 2.0
        - half * 2.0f64.ln()
        - crate::special::ln_gamma(half);
    ln_density.exp()
}

/// Standard Normal upper tail.
fn normal_sf(z: f64) -> f64 {
    1.0 - normal_cdf(z)
}

/// Integrates over the chi-square distribution with Simpson's rule.
///
/// The integration variable is `t = (v − df)/√(2df)`, so the range
/// `[−8, 8]` covers the whole mass of the distribution for every degree of
/// freedom. The integrand is smooth there, so the rule is accurate to many
/// digits.
fn expectation_over_chi_square<F: Fn(f64) -> f64>(df: f64, function: F) -> f64 {
    let step = 2.0 * INTEGRATION_SPAN / INTEGRATION_INTERVALS as f64;
    let scale = (2.0 * df).sqrt();
    let mut total = 0.0;
    for index in 0..=INTEGRATION_INTERVALS {
        let t = -INTEGRATION_SPAN + step * index as f64;
        let value = df + t * scale;
        if value <= 0.0 {
            continue;
        }
        let density = chi_square_density(value, df);
        let weight = match index {
            0 | INTEGRATION_INTERVALS => 1.0,
            index if index % 2 == 1 => 4.0,
            _ => 2.0,
        };
        total += weight * function(value) * density * scale;
    }
    total * step / 3.0
}

/// Upper-tail probability of the non-central t distribution.
///
/// `T = (Z + ncp) / √(V/df)` with `Z` standard Normal and `V` chi-square,
/// independent; the integral over `V` is evaluated numerically.
fn non_central_t_sf(t: f64, df: f64, ncp: f64) -> f64 {
    expectation_over_chi_square(df, |v| normal_cdf(ncp - t * (v / df).sqrt()))
}

/// Lower-tail probability of the non-central t distribution.
fn non_central_t_cdf(t: f64, df: f64, ncp: f64) -> f64 {
    expectation_over_chi_square(df, |v| normal_cdf(t * (v / df).sqrt() - ncp))
}

/// Power of a one-sample Student t test.
///
/// `effect` is the standardised shift `(µ₁ − µ₀)/σ`; the power is the
/// probability of rejecting `H₀: µ = µ₀` when the true mean is `µ₀ +
/// effect·σ`.
pub fn one_sample_t_power(
    effect: f64,
    n: usize,
    alpha: f64,
    alternative: Alternative,
) -> Result<f64> {
    validate_alpha(alpha)?;
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    if !effect.is_finite() {
        return Err(Error::InvalidParameter {
            name: "effect size",
            value: effect,
        });
    }
    let df = (n - 1) as f64;
    let ncp = effect * (n as f64).sqrt();
    let power = match alternative {
        Alternative::TwoSided => {
            let critical = student_t_quantile(1.0 - alpha / 2.0, df);
            non_central_t_sf(critical, df, ncp) + non_central_t_cdf(-critical, df, ncp)
        }
        Alternative::Greater => {
            let critical = student_t_quantile(1.0 - alpha, df);
            non_central_t_sf(critical, df, ncp)
        }
        Alternative::Less => {
            let critical = student_t_quantile(1.0 - alpha, df);
            non_central_t_cdf(-critical, df, ncp)
        }
    };
    Ok(power.clamp(0.0, 1.0))
}

/// Sample size of a one-sample Student t test with the requested power.
pub fn one_sample_t_sample_size(
    effect: f64,
    alpha: f64,
    power: f64,
    alternative: Alternative,
) -> Result<usize> {
    validate_alpha(alpha)?;
    validate_power(power)?;
    if effect == 0.0 {
        return Err(Error::InvalidParameter {
            name: "effect size",
            value: effect,
        });
    }
    let mut n = 2usize;
    while n < 100_000 {
        let achieved = one_sample_t_power(effect, n, alpha, alternative)?;
        if achieved >= power {
            return Ok(n);
        }
        n += 1;
    }
    Err(Error::Unsupported(
        "the requested power is not reached with a sample size below 100000".to_owned(),
    ))
}

/// Power of a two-sample Student t test with equal sample sizes.
///
/// `effect` is the standardised difference `(µ₁ − µ₂)/σ`; the test uses the
/// pooled degrees of freedom `2n − 2`.
pub fn two_sample_t_power(
    effect: f64,
    n_per_group: usize,
    alpha: f64,
    alternative: Alternative,
) -> Result<f64> {
    validate_alpha(alpha)?;
    if n_per_group < 2 {
        return Err(Error::SampleTooSmall {
            n: n_per_group,
            min: 2,
        });
    }
    if !effect.is_finite() {
        return Err(Error::InvalidParameter {
            name: "effect size",
            value: effect,
        });
    }
    let df = (2 * n_per_group - 2) as f64;
    let ncp = effect * (n_per_group as f64 / 2.0).sqrt();
    let power = match alternative {
        Alternative::TwoSided => {
            let critical = student_t_quantile(1.0 - alpha / 2.0, df);
            non_central_t_sf(critical, df, ncp) + non_central_t_cdf(-critical, df, ncp)
        }
        Alternative::Greater => {
            let critical = student_t_quantile(1.0 - alpha, df);
            non_central_t_sf(critical, df, ncp)
        }
        Alternative::Less => {
            let critical = student_t_quantile(1.0 - alpha, df);
            non_central_t_cdf(-critical, df, ncp)
        }
    };
    Ok(power.clamp(0.0, 1.0))
}

/// Sample size per group of a two-sample Student t test.
pub fn two_sample_t_sample_size(
    effect: f64,
    alpha: f64,
    power: f64,
    alternative: Alternative,
) -> Result<usize> {
    validate_alpha(alpha)?;
    validate_power(power)?;
    if effect == 0.0 {
        return Err(Error::InvalidParameter {
            name: "effect size",
            value: effect,
        });
    }
    let mut n = 2usize;
    while n < 100_000 {
        let achieved = two_sample_t_power(effect, n, alpha, alternative)?;
        if achieved >= power {
            return Ok(n);
        }
        n += 1;
    }
    Err(Error::Unsupported(
        "the requested power is not reached with a sample size below 100000 per group".to_owned(),
    ))
}

/// Power of a one-sample proportion test (normal approximation).
///
/// `p0` is the hypothesised proportion and `p1` the true proportion.
pub fn proportion_power(
    p0: f64,
    p1: f64,
    n: usize,
    alpha: f64,
    alternative: Alternative,
) -> Result<f64> {
    validate_alpha(alpha)?;
    if n == 0 {
        return Err(Error::EmptySample);
    }
    if !(0.0..1.0).contains(&p0) || !(0.0..1.0).contains(&p1) {
        return Err(Error::InvalidParameter {
            name: "proportion",
            value: if (0.0..1.0).contains(&p0) { p1 } else { p0 },
        });
    }
    let n_f = n as f64;
    let standard_error = (p0 * (1.0 - p0) / n_f).sqrt();
    let shift = (p1 - p0) / standard_error;
    let power = match alternative {
        Alternative::TwoSided => {
            let critical = normal_quantile(1.0 - alpha / 2.0);
            normal_sf(critical - shift) + normal_cdf(-critical - shift)
        }
        Alternative::Greater => {
            let critical = normal_quantile(1.0 - alpha);
            normal_sf(critical - shift)
        }
        Alternative::Less => {
            let critical = normal_quantile(1.0 - alpha);
            normal_cdf(-critical - shift)
        }
    };
    Ok(power.clamp(0.0, 1.0))
}

/// Sample size of a one-sample proportion test (normal approximation).
pub fn proportion_sample_size(
    p0: f64,
    p1: f64,
    alpha: f64,
    power: f64,
    alternative: Alternative,
) -> Result<usize> {
    validate_alpha(alpha)?;
    validate_power(power)?;
    if !(0.0..1.0).contains(&p0) || !(0.0..1.0).contains(&p1) {
        return Err(Error::InvalidParameter {
            name: "proportion",
            value: if (0.0..1.0).contains(&p0) { p1 } else { p0 },
        });
    }
    if p0 == p1 {
        return Err(Error::InvalidParameter {
            name: "proportions",
            value: p0,
        });
    }
    let mut n = 1usize;
    while n < 10_000_000 {
        let achieved = proportion_power(p0, p1, n, alpha, alternative)?;
        if achieved >= power {
            return Ok(n);
        }
        n += 1;
    }
    Err(Error::Unsupported(
        "the requested power is not reached with a sample size below 10000000".to_owned(),
    ))
}

/// Cumulative distribution of the non-central chi-square.
///
/// `P(X ≤ x)` for `X ~ χ²(df, λ)`, written as the Poisson mixture of the
/// central chi-square distributions:
/// `Σⱼ e^{−λ/2}(λ/2)ⱼ/j! · P(χ²_{df+2j} ≤ x)`.
fn non_central_chi_square_cdf(x: f64, df: f64, lambda: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let half_lambda = lambda / 2.0;
    let mut weight = (-half_lambda).exp();
    let mut total = 0.0;
    for index in 0..500 {
        let df_j = df + 2.0 * index as f64;
        total += weight * regularized_gamma_lower(0.5 * df_j, 0.5 * x);
        weight *= half_lambda / (index as f64 + 1.0);
        if weight <= 1e-16 * total.max(1e-300) {
            break;
        }
    }
    total.clamp(0.0, 1.0)
}

/// Upper tail of the non-central F distribution.
///
/// `F = (U/d₁)/(V/d₂)` with `U ~ χ²(d₁, λ)` and `V ~ χ²(d₂)`; the tail is
/// the expectation of the non-central chi-square tail over `V`.
fn non_central_f_sf(f: f64, df1: f64, df2: f64, lambda: f64) -> f64 {
    if f <= 0.0 {
        return 1.0;
    }
    expectation_over_chi_square(df2, |v| {
        let threshold = f * df1 * v / df2;
        1.0 - non_central_chi_square_cdf(threshold, df1, lambda)
    })
    .clamp(0.0, 1.0)
}

/// Power of an F test with the given non-centrality.
///
/// `non_centrality` is `λ` of the non-central chi-square in the numerator:
/// for a one-way ANOVA it is `n·Σ(μᵢ − μ̄)²/σ²`, and for the overall test
/// of a regression it is `N·R²/(1 − R²)`.
pub fn f_test_power(non_centrality: f64, df1: usize, df2: usize, alpha: f64) -> Result<f64> {
    validate_alpha(alpha)?;
    if df1 == 0 || df2 == 0 {
        return Err(Error::InvalidParameter {
            name: "degrees of freedom",
            value: 0.0,
        });
    }
    if !non_centrality.is_finite() || non_centrality < 0.0 {
        return Err(Error::InvalidParameter {
            name: "non-centrality",
            value: non_centrality,
        });
    }
    let critical = crate::special::f_quantile(1.0 - alpha, df1 as f64, df2 as f64);
    Ok(non_central_f_sf(
        critical,
        df1 as f64,
        df2 as f64,
        non_centrality,
    ))
}

/// Power of a one-way ANOVA with `groups` groups.
///
/// `difference` is the standardized difference between the group that differs
/// and the others, in units of the within-group standard deviation. The
/// non-centrality is `n·Δ²·(k − 1)/k`, which for two groups is the square of
/// the non-centrality of the corresponding two-sample t test.
pub fn anova_power(difference: f64, groups: usize, n_per_group: usize, alpha: f64) -> Result<f64> {
    validate_alpha(alpha)?;
    if groups < 2 {
        return Err(Error::InvalidParameter {
            name: "number of groups",
            value: groups as f64,
        });
    }
    if n_per_group < 2 {
        return Err(Error::SampleTooSmall {
            n: n_per_group,
            min: 2,
        });
    }
    if !difference.is_finite() {
        return Err(Error::InvalidParameter {
            name: "effect size",
            value: difference,
        });
    }
    let lambda = anova_non_centrality(difference, groups, n_per_group);
    let total = groups * n_per_group;
    f_test_power(lambda, groups - 1, total - groups, alpha)
}

/// Non-centrality of the one-way ANOVA F test.
fn anova_non_centrality(difference: f64, groups: usize, n_per_group: usize) -> f64 {
    let k = groups as f64;
    n_per_group as f64 * difference * difference * (k - 1.0) / k
}

/// Sample size per group of a one-way ANOVA.
pub fn anova_sample_size(difference: f64, groups: usize, alpha: f64, power: f64) -> Result<usize> {
    validate_alpha(alpha)?;
    validate_power(power)?;
    if groups < 2 {
        return Err(Error::InvalidParameter {
            name: "number of groups",
            value: groups as f64,
        });
    }
    if difference == 0.0 {
        return Err(Error::InvalidParameter {
            name: "effect size",
            value: difference,
        });
    }
    let mut n = 2usize;
    while n < 100_000 {
        let achieved = anova_power(difference, groups, n, alpha)?;
        if achieved >= power {
            return Ok(n);
        }
        n += 1;
    }
    Err(Error::Unsupported(
        "the requested power is not reached with a sample size below 100000 per group".to_owned(),
    ))
}

/// Power of the overall F test of a regression.
///
/// `r_squared` is the population coefficient of determination; the
/// non-centrality is `N·R²/(1 − R²)`.
pub fn regression_power(r_squared: f64, predictors: usize, n: usize, alpha: f64) -> Result<f64> {
    validate_alpha(alpha)?;
    if predictors == 0 {
        return Err(Error::InvalidParameter {
            name: "number of predictors",
            value: 0.0,
        });
    }
    if n <= predictors + 1 {
        return Err(Error::SampleTooSmall {
            n,
            min: predictors + 2,
        });
    }
    if !r_squared.is_finite() || !(0.0..1.0).contains(&r_squared) {
        return Err(Error::InvalidParameter {
            name: "coefficient of determination",
            value: r_squared,
        });
    }
    let lambda = n as f64 * r_squared / (1.0 - r_squared);
    f_test_power(lambda, predictors, n - predictors - 1, alpha)
}

/// Sample size of the overall F test of a regression.
pub fn regression_sample_size(
    r_squared: f64,
    predictors: usize,
    alpha: f64,
    power: f64,
) -> Result<usize> {
    validate_alpha(alpha)?;
    validate_power(power)?;
    if predictors == 0 {
        return Err(Error::InvalidParameter {
            name: "number of predictors",
            value: 0.0,
        });
    }
    if !(0.0..1.0).contains(&r_squared) {
        return Err(Error::InvalidParameter {
            name: "coefficient of determination",
            value: r_squared,
        });
    }
    let mut n = predictors + 2;
    while n < 1_000_000 {
        let achieved = regression_power(r_squared, predictors, n, alpha)?;
        if achieved >= power {
            return Ok(n);
        }
        n += 1;
    }
    Err(Error::Unsupported(
        "the requested power is not reached with a sample size below 1000000".to_owned(),
    ))
}

/// Power of a Pearson correlation test (Fisher's transformation).
///
/// `rho` is the true population correlation.
pub fn correlation_power(rho: f64, n: usize, alpha: f64) -> Result<f64> {
    validate_alpha(alpha)?;
    if n < 4 {
        return Err(Error::SampleTooSmall { n, min: 4 });
    }
    if !rho.is_finite() || rho.abs() >= 1.0 {
        return Err(Error::InvalidParameter {
            name: "correlation",
            value: rho,
        });
    }
    let fisher = libm::atanh(rho);
    let shift = fisher * ((n - 3) as f64).sqrt();
    let critical = normal_quantile(1.0 - alpha / 2.0);
    Ok((normal_sf(critical - shift) + normal_cdf(-critical - shift)).clamp(0.0, 1.0))
}

/// Sample size of a Pearson correlation test.
pub fn correlation_sample_size(rho: f64, alpha: f64, power: f64) -> Result<usize> {
    validate_alpha(alpha)?;
    validate_power(power)?;
    if !rho.is_finite() || rho.abs() >= 1.0 {
        return Err(Error::InvalidParameter {
            name: "correlation",
            value: rho,
        });
    }
    let mut n = 4usize;
    while n < 1_000_000 {
        let achieved = correlation_power(rho, n, alpha)?;
        if achieved >= power {
            return Ok(n);
        }
        n += 1;
    }
    Err(Error::Unsupported(
        "the requested power is not reached with a sample size below 1000000".to_owned(),
    ))
}

/// Builds the Session report of a power and sample-size analysis.
///
/// When `n` is greater than zero the report gives the power of the test
/// with that sample size; otherwise it gives the sample size that reaches
/// the requested power.
#[allow(clippy::too_many_arguments)]
pub fn power_report(
    procedure: PowerProcedure,
    effect: f64,
    null_value: f64,
    n: usize,
    groups: usize,
    alpha: f64,
    power: f64,
    alternative: Alternative,
) -> Result<AnalysisResult> {
    let mut report = AnalysisResult::new(
        format!("Power and Sample Size — {}", procedure.name()),
        format!("{} alternative; alpha {:.4}", alternative.as_str(), alpha),
    );
    let mut notes = vec![
        (
            "Effect".to_owned(),
            match procedure {
                PowerProcedure::OneSampleT | PowerProcedure::TwoSampleT => {
                    format!(
                        "standardised difference {effect:.6} (in units of the standard deviation)"
                    )
                }
                PowerProcedure::Proportion => {
                    format!("true proportion {effect:.6} against the null {null_value:.6}")
                }
                PowerProcedure::Correlation => format!("population correlation {effect:.6}"),
                PowerProcedure::Anova => format!(
                    "standardized difference {effect:.6} between the group that differs and the \
                     others, with {groups} groups"
                ),
                PowerProcedure::Regression => format!(
                    "population coefficient of determination {effect:.6} with {groups} predictors"
                ),
            },
        ),
        (
            "Method".to_owned(),
            match procedure {
                PowerProcedure::OneSampleT | PowerProcedure::TwoSampleT => {
                    "non-central t distribution integrated over the chi-square variance estimate"
                        .to_owned()
                }
                PowerProcedure::Proportion => {
                    "normal approximation of the sample proportion under the true proportion"
                        .to_owned()
                }
                PowerProcedure::Correlation => {
                    "Fisher's atanh transformation of the correlation".to_owned()
                }
                PowerProcedure::Anova => {
                    "non-central F distribution with the non-centrality of the ANOVA".to_owned()
                }
                PowerProcedure::Regression => {
                    "non-central F distribution with the non-centrality N·R²/(1 − R²)".to_owned()
                }
            },
        ),
    ];

    let (achieved, required) = if n > 0 {
        let achieved = match procedure {
            PowerProcedure::OneSampleT => one_sample_t_power(effect, n, alpha, alternative)?,
            PowerProcedure::TwoSampleT => two_sample_t_power(effect, n, alpha, alternative)?,
            PowerProcedure::Proportion => {
                proportion_power(null_value, effect, n, alpha, alternative)?
            }
            PowerProcedure::Correlation => correlation_power(effect, n, alpha)?,
            PowerProcedure::Anova => anova_power(effect, groups, n, alpha)?,
            PowerProcedure::Regression => regression_power(effect, groups, n, alpha)?,
        };
        (Some(achieved), None)
    } else {
        let required = match procedure {
            PowerProcedure::OneSampleT => {
                one_sample_t_sample_size(effect, alpha, power, alternative)?
            }
            PowerProcedure::TwoSampleT => {
                two_sample_t_sample_size(effect, alpha, power, alternative)?
            }
            PowerProcedure::Proportion => {
                proportion_sample_size(null_value, effect, alpha, power, alternative)?
            }
            PowerProcedure::Correlation => correlation_sample_size(effect, alpha, power)?,
            PowerProcedure::Anova => anova_sample_size(effect, groups, alpha, power)?,
            PowerProcedure::Regression => regression_sample_size(effect, groups, alpha, power)?,
        };
        (None, Some(required))
    };

    report.n = required.unwrap_or(match procedure {
        PowerProcedure::Anova => n * groups,
        _ => n,
    });
    report.statistics = vec![
        ("Alpha", alpha),
        ("Requested power", power),
        (
            "Sample size",
            match required {
                Some(required) => required as f64,
                None => n as f64,
            },
        ),
    ];
    if let Some(achieved) = achieved {
        report.statistics.push(("Power", achieved));
    }
    report.extra.push(Item::Message(match (achieved, required) {
        (Some(achieved), _) => format!(
            "With {} observation(s) the test rejects the null with probability {achieved:.4}.",
            n
        ),
        (None, Some(required)) => {
            format!("{required} observation(s) are needed to reach a power of {power:.4}.")
        }
        _ => String::new(),
    }));
    notes.push((
        "Conventions".to_owned(),
        "the power dialog defaults are documented conventions; the numbers follow the formulas \
         stated above"
            .to_owned(),
    ));
    report.notes = notes;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_sample_t_power_matches_the_known_values() {
        // d = 0.5, alpha = 0.05 (two-sided) and n = 64: the normal
        // approximation gives Phi(0.5·8 − 1.998) = 0.977, and the
        // non-central t is very close to it.
        let power = one_sample_t_power(0.5, 64, 0.05, Alternative::TwoSided).expect("power");
        assert!((power - 0.976).abs() < 0.01, "power {power}");
        // With no effect the power is the level of the test.
        let null = one_sample_t_power(0.0, 30, 0.05, Alternative::TwoSided).expect("power");
        assert!((null - 0.05).abs() < 1e-6, "power {null}");
    }

    #[test]
    fn one_sample_t_power_grows_with_the_sample_size() {
        let mut previous = 0.0;
        for n in [10, 20, 40, 80, 160] {
            let power = one_sample_t_power(0.8, n, 0.05, Alternative::TwoSided).expect("power");
            assert!(power > previous, "n={n}, power {power}");
            previous = power;
        }
    }

    #[test]
    fn one_sample_t_sample_size_reaches_the_requested_power() {
        // The standard table for d = 0.5, alpha = 0.05 and power 0.80 gives
        // n = 34 for a one-sample test.
        let n = one_sample_t_sample_size(0.5, 0.05, 0.8, Alternative::TwoSided).expect("n");
        assert!((33..=35).contains(&n), "n {n}");
        let power = one_sample_t_power(0.5, n, 0.05, Alternative::TwoSided).expect("power");
        assert!(power >= 0.8, "power {power}");
        let smaller = one_sample_t_sample_size(0.5, 0.05, 0.8, Alternative::TwoSided).expect("n");
        assert_eq!(n, smaller);
    }

    #[test]
    fn two_sample_t_power_and_sample_size_are_consistent() {
        // d = 1.0 with ten observations per group: the normal approximation
        // gives Phi(√5 − 2.101) = 0.55.
        let power = two_sample_t_power(1.0, 10, 0.05, Alternative::TwoSided).expect("power");
        assert!((power - 0.56).abs() < 0.02, "power {power}");
        let n = two_sample_t_sample_size(1.0, 0.05, 0.8, Alternative::TwoSided).expect("n");
        let achieved = two_sample_t_power(1.0, n, 0.05, Alternative::TwoSided).expect("power");
        assert!(achieved >= 0.8, "power {achieved}");
    }

    #[test]
    fn proportion_power_matches_the_normal_approximation() {
        // p0 = 0.5 against p1 = 0.6 with n = 100: the shift is 2 σ, so the
        // two-sided power is about 0.51.
        let power = proportion_power(0.5, 0.6, 100, 0.05, Alternative::TwoSided).expect("power");
        assert!((power - 0.51).abs() < 0.02, "power {power}");
        let null = proportion_power(0.5, 0.5, 100, 0.05, Alternative::TwoSided).expect("power");
        assert!((null - 0.05).abs() < 1e-9, "power {null}");
    }

    #[test]
    fn proportion_sample_size_reaches_the_requested_power() {
        let n = proportion_sample_size(0.5, 0.6, 0.05, 0.8, Alternative::TwoSided).expect("n");
        let power = proportion_power(0.5, 0.6, n, 0.05, Alternative::TwoSided).expect("power");
        assert!(power >= 0.8, "power {power}");
        assert!(n > 100, "n {n}");
    }

    #[test]
    fn correlation_power_uses_the_fisher_transformation() {
        // rho = 0.3 with n = 100: atanh(0.3)·√97 = 3.05, so the power is
        // Phi(3.05 − 1.96) = 0.86.
        let power = correlation_power(0.3, 100, 0.05).expect("power");
        assert!((power - 0.86).abs() < 0.02, "power {power}");
        let n = correlation_sample_size(0.3, 0.05, 0.8).expect("n");
        let achieved = correlation_power(0.3, n, 0.05).expect("power");
        assert!(achieved >= 0.8, "power {achieved}");
    }

    #[test]
    fn the_report_covers_every_procedure() {
        for procedure in [
            PowerProcedure::OneSampleT,
            PowerProcedure::TwoSampleT,
            PowerProcedure::Proportion,
            PowerProcedure::Correlation,
            PowerProcedure::Anova,
            PowerProcedure::Regression,
        ] {
            let effect = match procedure {
                PowerProcedure::Proportion => 0.6,
                PowerProcedure::Regression => 0.3,
                _ => 0.5,
            };
            let report = power_report(
                procedure,
                effect,
                0.5,
                40,
                3,
                0.05,
                0.8,
                Alternative::TwoSided,
            )
            .expect("report");
            let text = report.to_session_text();
            assert!(text.contains("Power and Sample Size"), "{text}");
            assert!(text.contains("Power"), "{text}");

            let report = power_report(
                procedure,
                effect,
                0.5,
                0,
                3,
                0.05,
                0.8,
                Alternative::TwoSided,
            )
            .expect("report");
            let text = report.to_session_text();
            assert!(text.contains("Sample size"), "{text}");
        }
    }

    #[test]
    fn the_procedure_names_are_parsed() {
        assert_eq!(
            PowerProcedure::parse("t1"),
            Some(PowerProcedure::OneSampleT)
        );
        assert_eq!(
            PowerProcedure::parse("2-Sample-T"),
            Some(PowerProcedure::TwoSampleT)
        );
        assert_eq!(
            PowerProcedure::parse("proportion"),
            Some(PowerProcedure::Proportion)
        );
        assert_eq!(
            PowerProcedure::parse("pearson"),
            Some(PowerProcedure::Correlation)
        );
        assert_eq!(PowerProcedure::parse("unknown"), None);
        assert_eq!(PowerProcedure::OneSampleT.as_str(), "t1");
    }

    #[test]
    fn the_anova_power_matches_the_two_sample_t_test() {
        // With two groups the ANOVA F test is the square of the t test, so the
        // powers agree: Δ = 1 with ten observations per group gives about
        // 0.56.
        let anova = anova_power(1.0, 2, 10, 0.05).expect("anova power");
        let t = two_sample_t_power(1.0, 10, 0.05, Alternative::TwoSided).expect("t power");
        assert!((anova - t).abs() < 0.01, "anova {anova}, t {t}");
    }

    #[test]
    fn the_anova_and_regression_powers_grow_with_the_sample_size() {
        let mut previous = 0.0;
        for n in [5, 10, 20, 40, 80] {
            let power = anova_power(1.0, 3, n, 0.05).expect("anova power");
            assert!(power > previous, "n={n}, power {power}");
            previous = power;
        }
        let n = anova_sample_size(1.0, 3, 0.05, 0.8).expect("sample size");
        let achieved = anova_power(1.0, 3, n, 0.05).expect("anova power");
        assert!(achieved >= 0.8, "power {achieved}");

        let power = regression_power(0.3, 2, 50, 0.05).expect("regression power");
        assert!(power > 0.8, "power {power}");
        let n = regression_sample_size(0.3, 2, 0.05, 0.8).expect("sample size");
        let achieved = regression_power(0.3, 2, n, 0.05).expect("regression power");
        assert!(achieved >= 0.8, "power {achieved}");
    }

    #[test]
    fn invalid_arguments_are_rejected() {
        assert!(matches!(
            one_sample_t_power(0.5, 1, 0.05, Alternative::TwoSided),
            Err(Error::SampleTooSmall { .. })
        ));
        assert!(matches!(
            one_sample_t_power(0.5, 30, 1.5, Alternative::TwoSided),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            proportion_power(0.5, 1.5, 30, 0.05, Alternative::TwoSided),
            Err(Error::InvalidParameter { .. })
        ));
    }
}
