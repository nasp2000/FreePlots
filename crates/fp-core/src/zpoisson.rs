// SPDX-License-Identifier: MIT
//! One-sample Z, Poisson rates and the Poisson goodness-of-fit test.
//!
//! The Z test uses a known population standard deviation. The Poisson rate
//! tests use the total occurrence count over the total length of observation,
//! with exact (Poisson/conditional binomial) or normal-approximation p-values.
//! The goodness-of-fit test groups the observed integer counts into categories
//! with an expected count of at least two and compares them with a Poisson
//! distribution whose mean is estimated from the data (or supplied).

use crate::data::{DataFrame, MissingPolicy};
use crate::error::{Error, Result};
use crate::hypothesis::Alternative;
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{
    chi_square_cdf, chi_square_quantile, normal_cdf, normal_quantile, normal_sf, regularized_beta,
    regularized_gamma_lower,
};

/// Method of a Poisson rate test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoissonMethod {
    /// Exact Poisson or conditional binomial tail.
    Exact,
    /// Normal approximation.
    Normal,
}

impl PoissonMethod {
    /// Name used by the interface and the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Exact => "Exact",
            Self::Normal => "Normal approximation",
        }
    }
}

fn validate_confidence(confidence: f64) -> Result<()> {
    if !confidence.is_finite() || !(0.5..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence level",
            value: confidence,
        });
    }
    Ok(())
}

fn normal_p_value(z: f64, alternative: Alternative) -> f64 {
    match alternative {
        Alternative::TwoSided => (2.0 * normal_sf(z.abs())).clamp(0.0, 1.0),
        Alternative::Less => normal_cdf(z).clamp(0.0, 1.0),
        Alternative::Greater => normal_sf(z).clamp(0.0, 1.0),
    }
}

fn two_sided_tails(lower: f64, upper: f64) -> f64 {
    (2.0 * lower.min(upper)).clamp(0.0, 1.0)
}

fn interval_bounds(lower: Option<f64>, upper: Option<f64>) -> Vec<(&'static str, f64)> {
    let mut out = Vec::with_capacity(2);
    if let Some(lower) = lower {
        out.push(("CI lower", lower));
    }
    if let Some(upper) = upper {
        out.push(("CI upper", upper));
    }
    out
}

/// One-sample Z test with a known population standard deviation.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for a non-positive or non-finite
/// `sigma`, a non-finite null mean or a confidence outside 50–99.9 %, and the
/// sample errors of the column.
pub fn z_test(
    df: &DataFrame,
    column_name: &str,
    null_mean: f64,
    sigma: f64,
    alternative: Alternative,
    confidence: f64,
) -> Result<AnalysisResult> {
    validate_confidence(confidence)?;
    if !null_mean.is_finite() {
        return Err(Error::InvalidParameter {
            name: "null mean",
            value: null_mean,
        });
    }
    if !sigma.is_finite() || sigma <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "known standard deviation",
            value: sigma,
        });
    }
    let sample = df.column(column_name)?.sample(MissingPolicy::Drop)?;
    let n = sample.n();
    let mean = sample.mean()?;
    let se = sigma / (n as f64).sqrt();
    let z = (mean - null_mean) / se;
    let p_value = normal_p_value(z, alternative);
    let critical = normal_quantile(if alternative == Alternative::TwoSided {
        1.0 - (1.0 - confidence) / 2.0
    } else {
        confidence
    });
    let (lower, upper) = match alternative {
        Alternative::TwoSided => (Some(mean - critical * se), Some(mean + critical * se)),
        Alternative::Greater => (Some(mean - critical * se), None),
        Alternative::Less => (None, Some(mean + critical * se)),
    };

    let mut result = AnalysisResult::new(
        format!("1-Sample Z — column {column_name}"),
        format!(
            "Known standard deviation σ = {sigma:.6}; {} alternative; {:.1}% confidence",
            alternative.as_str(),
            confidence * 100.0
        ),
    );
    result.n = n;
    result.excluded = sample.excluded();
    result.statistics = vec![
        ("Mean", mean),
        ("Null mean", null_mean),
        ("Known standard deviation", sigma),
        ("Standard error of the mean", se),
        ("Z", z),
        ("p-value", p_value),
    ];
    result.statistics.extend(interval_bounds(lower, upper));
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            format!(
                "H₀: μ = {null_mean}; H₁: μ {} {null_mean}",
                alternative.symbol()
            ),
        ),
        (
            "Z".to_owned(),
            "Z = (x̄ − μ0)/(σ/√n), with the known σ and the two-sided p-value by default".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(n, result.excluded, 0);
    Ok(result)
}

/// Poisson cumulative distribution function, `P(X ≤ k)`.
fn poisson_cdf(k: f64, mean: f64) -> f64 {
    if k < 0.0 {
        return 0.0;
    }
    (1.0 - regularized_gamma_lower(k + 1.0, mean)).clamp(0.0, 1.0)
}

/// Binomial cumulative distribution function, `P(X ≤ k)`.
fn binomial_cdf(k: f64, n: f64, p: f64) -> f64 {
    if k < 0.0 {
        return 0.0;
    }
    if k >= n {
        return 1.0;
    }
    regularized_beta(1.0 - p, n - k, k + 1.0).clamp(0.0, 1.0)
}

/// Reads a column of non-negative integer counts, dropping missing values.
fn counts(column: &crate::data::Column, name: &str) -> Result<(f64, usize, usize)> {
    let mut total = 0.0;
    let mut n = 0;
    let mut excluded = 0;
    for row in 0..column.len() {
        match column.value(row) {
            Some(value) if value >= 0.0 && value.fract() == 0.0 => {
                total += value;
                n += 1;
            }
            Some(_) => {
                return Err(Error::Cell {
                    column: name.to_owned(),
                    row: row + 1,
                    message: "counts must be non-negative integers".to_owned(),
                });
            }
            None => excluded += 1,
        }
    }
    if n == 0 {
        return Err(Error::EmptySample);
    }
    Ok((total, n, excluded))
}

/// One-sample Poisson rate test.
///
/// `length` is the total length of observation; the rate is the total count
/// divided by it. `method` selects the exact or the normal-approximation test.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for a non-positive length or null rate,
/// and [`Error::Cell`] for a non-integer or negative count.
pub fn poisson_one(
    df: &DataFrame,
    column_name: &str,
    length: f64,
    null_rate: f64,
    alternative: Alternative,
    method: PoissonMethod,
    confidence: f64,
) -> Result<AnalysisResult> {
    validate_confidence(confidence)?;
    if !length.is_finite() || length <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "length of observation",
            value: length,
        });
    }
    if !null_rate.is_finite() || null_rate <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "null rate",
            value: null_rate,
        });
    }
    let column = df.column(column_name)?;
    let (total, n, excluded) = counts(column, column_name)?;
    let rate = total / length;
    let mean = total / n as f64;
    let expected = length * null_rate;
    let (z, p_value) = match method {
        PoissonMethod::Exact => {
            let lower = poisson_cdf(total, expected);
            let upper = 1.0 - poisson_cdf(total - 1.0, expected);
            let p = match alternative {
                Alternative::TwoSided => two_sided_tails(lower, upper),
                Alternative::Less => lower,
                Alternative::Greater => upper,
            };
            (f64::NAN, p.clamp(0.0, 1.0))
        }
        PoissonMethod::Normal => {
            if expected <= 0.0 {
                return Err(Error::Unsupported(
                    "the normal approximation needs a positive expected count".to_owned(),
                ));
            }
            let z = (total - expected) / expected.sqrt();
            (z, normal_p_value(z, alternative))
        }
    };
    let critical = normal_quantile(if alternative == Alternative::TwoSided {
        1.0 - (1.0 - confidence) / 2.0
    } else {
        confidence
    });
    let alpha = 1.0 - confidence;
    let (lower, upper) = match method {
        PoissonMethod::Exact => match alternative {
            Alternative::TwoSided => (
                Some(if total <= 0.0 {
                    0.0
                } else {
                    chi_square_quantile(alpha / 2.0, 2.0 * total) / (2.0 * length)
                }),
                Some(chi_square_quantile(1.0 - alpha / 2.0, 2.0 * (total + 1.0)) / (2.0 * length)),
            ),
            Alternative::Greater => (
                Some(if total <= 0.0 {
                    0.0
                } else {
                    chi_square_quantile(alpha, 2.0 * total) / (2.0 * length)
                }),
                None,
            ),
            Alternative::Less => (
                None,
                Some(chi_square_quantile(1.0 - alpha, 2.0 * (total + 1.0)) / (2.0 * length)),
            ),
        },
        PoissonMethod::Normal => {
            let se = (rate / length).sqrt();
            match alternative {
                Alternative::TwoSided => (
                    Some((rate - critical * se).max(0.0)),
                    Some(rate + critical * se),
                ),
                Alternative::Greater => (Some((rate - critical * se).max(0.0)), None),
                Alternative::Less => (None, Some(rate + critical * se)),
            }
        }
    };

    let mut result = AnalysisResult::new(
        format!("1-Sample Poisson Rate — column {column_name}"),
        format!(
            "{}; length of observation {length:.6}; {} alternative; {:.1}% confidence",
            method.name(),
            alternative.as_str(),
            confidence * 100.0
        ),
    );
    result.n = n;
    result.excluded = excluded;
    result.statistics = vec![
        ("Total occurrences", total),
        ("Length of observation", length),
        ("Sample size", n as f64),
        ("Rate", rate),
        ("Mean occurrence", mean),
        ("Null rate", null_rate),
    ];
    if method == PoissonMethod::Normal {
        result.statistics.push(("Z", z));
    }
    result.statistics.push(("p-value", p_value));
    result.statistics.extend(interval_bounds(lower, upper));
    result.notes =
        vec![
        (
            "Hypotheses".to_owned(),
            format!("H₀: λ = {null_rate}; H₁: λ {} {null_rate}", alternative.symbol()),
        ),
        (
            "Test".to_owned(),
            match method {
                PoissonMethod::Exact => {
                    "exact Poisson tail with mean L·λ0; the two-sided p doubles the smaller tail"
                        .to_owned()
                }
                PoissonMethod::Normal => {
                    "Z = (S − L·λ0)/√(L·λ0), reliable when the total count exceeds 10".to_owned()
                }
            },
        ),
        (
            "Data policy".to_owned(),
            "counts must be non-negative integers; rows with a missing value are excluded and \
             counted"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(n, excluded, 0);
    if method == PoissonMethod::Normal && total <= 10.0 {
        result.warnings.push(
            "the normal approximation is unreliable with 10 or fewer total occurrences".to_owned(),
        );
    }
    Ok(result)
}

/// Two-sample Poisson rate test.
///
/// The two samples are read from their count columns, each with its own total
/// length of observation. The exact p-value conditions on the total count; the
/// interval of the rate difference always uses the normal approximation.
///
/// # Errors
///
/// As [`poisson_one`], plus [`Error::Unsupported`] when both samples have no
/// occurrences.
#[allow(clippy::too_many_arguments)]
pub fn poisson_two(
    df: &DataFrame,
    first_column: &str,
    first_length: f64,
    second_column: &str,
    second_length: f64,
    alternative: Alternative,
    method: PoissonMethod,
    confidence: f64,
) -> Result<AnalysisResult> {
    validate_confidence(confidence)?;
    for (length, name) in [
        (first_length, "length of the first sample"),
        (second_length, "length of the second sample"),
    ] {
        if !length.is_finite() || length <= 0.0 {
            return Err(Error::InvalidParameter {
                name,
                value: length,
            });
        }
    }
    let first = df.column(first_column)?;
    let second = df.column(second_column)?;
    let (first_total, first_n, first_excluded) = counts(first, first_column)?;
    let (second_total, second_n, second_excluded) = counts(second, second_column)?;
    let total = first_total + second_total;
    let first_rate = first_total / first_length;
    let second_rate = second_total / second_length;
    let difference = first_rate - second_rate;
    let variance = first_rate / first_length + second_rate / second_length;
    let z = if variance > 0.0 {
        difference / variance.sqrt()
    } else {
        f64::NAN
    };
    let p_value = match method {
        PoissonMethod::Exact => {
            if total <= 0.0 {
                return Err(Error::Unsupported(
                    "the exact test needs at least one occurrence".to_owned(),
                ));
            }
            let p0 = first_length / (first_length + second_length);
            let lower = binomial_cdf(first_total, total, p0);
            let upper = 1.0 - binomial_cdf(first_total - 1.0, total, p0);
            let p = match alternative {
                Alternative::TwoSided => two_sided_tails(lower, upper),
                Alternative::Less => lower,
                Alternative::Greater => upper,
            };
            p.clamp(0.0, 1.0)
        }
        PoissonMethod::Normal => {
            if variance <= 0.0 {
                return Err(Error::Unsupported(
                    "both samples have a zero rate".to_owned(),
                ));
            }
            normal_p_value(z, alternative)
        }
    };
    let critical = normal_quantile(if alternative == Alternative::TwoSided {
        1.0 - (1.0 - confidence) / 2.0
    } else {
        confidence
    });
    let se = variance.sqrt();
    let (lower, upper) = match alternative {
        Alternative::TwoSided => (
            Some(difference - critical * se),
            Some(difference + critical * se),
        ),
        Alternative::Greater => (Some(difference - critical * se), None),
        Alternative::Less => (None, Some(difference + critical * se)),
    };

    let mut result = AnalysisResult::new(
        format!("2-Sample Poisson Rate — {first_column} and {second_column}"),
        format!(
            "{}; lengths {first_length:.6} and {second_length:.6}; {} alternative; {:.1}% \
             confidence",
            method.name(),
            alternative.as_str(),
            confidence * 100.0
        ),
    );
    result.n = first_n + second_n;
    result.excluded = first_excluded + second_excluded;
    result.statistics = vec![
        ("First sample occurrences", first_total),
        ("First sample length", first_length),
        ("First sample size", first_n as f64),
        ("First sample rate", first_rate),
        ("Second sample occurrences", second_total),
        ("Second sample length", second_length),
        ("Second sample size", second_n as f64),
        ("Second sample rate", second_rate),
        ("Rate difference", difference),
        ("p-value", p_value),
    ];
    if method == PoissonMethod::Normal {
        result.statistics.push(("Z", z));
    }
    result.statistics.extend(interval_bounds(lower, upper));
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            format!("H₀: λ₁ − λ₂ = 0; H₁: λ₁ − λ₂ {} 0", alternative.symbol()),
        ),
        (
            "Test".to_owned(),
            match method {
                PoissonMethod::Exact => {
                    "exact conditional test: S | (S + U) ~ Binomial(S + U, L₁/(L₁ + L₂)); the \
                     two-sided p doubles the smaller tail"
                        .to_owned()
                }
                PoissonMethod::Normal => {
                    "Z = (λ̂₁ − λ̂₂)/√(λ̂₁/L₁ + λ̂₂/L₂), reliable with more than 10 occurrences per \
                     sample"
                        .to_owned()
                }
            },
        ),
        (
            "Interval".to_owned(),
            "the interval of the rate difference uses the normal approximation in both methods"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "counts must be non-negative integers; rows with a missing value are excluded and \
             counted"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    if method == PoissonMethod::Normal && (first_total <= 10.0 || second_total <= 10.0) {
        result.warnings.push(
            "the normal approximation is unreliable with 10 or fewer occurrences in a sample"
                .to_owned(),
        );
    }
    Ok(result)
}

/// One category of the Poisson goodness-of-fit table.
#[derive(Debug, Clone, PartialEq)]
struct Category {
    lo: usize,
    hi: Option<usize>,
    observed: f64,
    expected: f64,
}

impl Category {
    fn new(lo: usize, hi: Option<usize>, observed: f64, expected: f64) -> Self {
        Self {
            lo,
            hi,
            observed,
            expected,
        }
    }

    fn label(&self) -> String {
        match self.hi {
            None => format!("≥{}", self.lo),
            Some(hi) if hi == self.lo => self.lo.to_string(),
            Some(hi) => format!("{}–{}", self.lo, hi),
        }
    }

    fn merge_lower(&mut self, removed: &Category) {
        self.lo = removed.lo;
        self.observed += removed.observed;
        self.expected += removed.expected;
    }

    fn merge_upper(&mut self, removed: &Category) {
        self.hi = removed.hi;
        self.observed += removed.observed;
        self.expected += removed.expected;
    }
}

/// Poisson goodness-of-fit test of a column of integer counts.
///
/// The mean is estimated from the counts unless `mean` is supplied. Categories
/// are the observed integer values, with the lower and upper tails merged until
/// each expected count reaches two.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for a non-positive or non-finite mean,
/// [`Error::Cell`] for a non-integer or negative count, and
/// [`Error::Unsupported`] when fewer than two categories remain.
pub fn poisson_gof(df: &DataFrame, column_name: &str, mean: Option<f64>) -> Result<AnalysisResult> {
    if let Some(value) = mean
        && (!value.is_finite() || value <= 0.0)
    {
        return Err(Error::InvalidParameter {
            name: "Poisson mean",
            value,
        });
    }
    let column = df.column(column_name)?;
    let mut values = Vec::new();
    let mut excluded = 0;
    for row in 0..column.len() {
        match column.value(row) {
            Some(value) if value >= 0.0 && value.fract() == 0.0 => values.push(value),
            Some(_) => {
                return Err(Error::Cell {
                    column: column_name.to_owned(),
                    row: row + 1,
                    message: "counts must be non-negative integers".to_owned(),
                });
            }
            None => excluded += 1,
        }
    }
    let n = values.len();
    if n == 0 {
        return Err(Error::EmptySample);
    }
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    let estimated = mean.is_none();
    let lambda = mean.unwrap_or_else(|| values.iter().sum::<f64>() / n as f64);
    if !lambda.is_finite() || lambda <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "Poisson mean",
            value: lambda,
        });
    }
    let maximum = values.iter().copied().fold(0.0_f64, f64::max) as usize;
    let mut observed = vec![0.0; maximum + 1];
    for value in &values {
        observed[*value as usize] += 1.0;
    }
    let mut probabilities = Vec::with_capacity(maximum + 1);
    for value in 0..=maximum {
        probabilities.push((-lambda).exp() * lambda.powi(value as i32) / factorial(value));
    }
    // The last category takes everything from `maximum` upwards.
    let tail = (1.0 - probabilities[..maximum].iter().sum::<f64>()).max(0.0);
    let mut categories: Vec<Category> = (0..=maximum)
        .map(|value| {
            let probability = if value == maximum {
                tail
            } else {
                probabilities[value]
            };
            Category::new(
                value,
                if value == maximum { None } else { Some(value) },
                observed[value],
                probability * n as f64,
            )
        })
        .collect();
    while categories.len() > 2 && categories[0].expected < 2.0 {
        let first = categories.remove(0);
        categories[0].merge_lower(&first);
    }
    while categories.len() > 2 && categories.last().is_some_and(|last| last.expected < 2.0) {
        let last = categories.pop().expect("last category");
        if let Some(previous) = categories.last_mut() {
            previous.merge_upper(&last);
        }
    }
    if categories.len() < 2 {
        return Err(Error::Unsupported(
            "the goodness-of-fit test needs at least two categories".to_owned(),
        ));
    }
    let categories = categories;
    let chi_square: f64 = categories
        .iter()
        .map(|category| {
            let difference = category.observed - category.expected;
            difference * difference / category.expected
        })
        .sum();
    let degrees = categories.len() as f64 - if estimated { 2.0 } else { 1.0 };
    if degrees < 1.0 {
        return Err(Error::Unsupported(
            "the goodness-of-fit test has no degrees of freedom left".to_owned(),
        ));
    }
    let p_value = (1.0 - chi_square_cdf(chi_square, degrees)).clamp(0.0, 1.0);
    let mut result = AnalysisResult::new(
        format!("Poisson Goodness-of-Fit — column {column_name}"),
        format!(
            "Chi-square test; mean {} at {lambda:.6}",
            if estimated { "estimated" } else { "given" }
        ),
    );
    result.n = n;
    result.excluded = excluded;
    result.statistics = vec![
        ("Observations", n as f64),
        ("Poisson mean", lambda),
        ("Categories", categories.len() as f64),
        ("Degrees of freedom", degrees),
        ("Chi-square", chi_square),
        ("p-value", p_value),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Category".to_owned(),
            "Observed".to_owned(),
            "Expected".to_owned(),
            "Contribution".to_owned(),
        ],
        rows: categories
            .iter()
            .map(|category| {
                let difference = category.observed - category.expected;
                vec![
                    category.label(),
                    format!("{:.0}", category.observed),
                    format!("{:.4}", category.expected),
                    format!("{:.6}", difference * difference / category.expected),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: the counts follow a Poisson distribution with the stated mean".to_owned(),
        ),
        (
            "Degrees of freedom".to_owned(),
            format!(
                "k − 1 with the mean given and k − 2 with the mean estimated (k = {} categories)",
                categories.len()
            ),
        ),
        (
            "Grouping".to_owned(),
            "the lower and upper tails are merged until every expected count reaches two"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "counts must be non-negative integers; rows with a missing value are excluded and \
             counted"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(n, excluded, 0);
    if categories.iter().any(|category| category.expected < 2.0) {
        result.warnings.push(
            "some categories have an expected count below two; the chi-square approximation may \
             be inaccurate"
                .to_owned(),
        );
    }
    Ok(result)
}

fn factorial(value: usize) -> f64 {
    let mut result = 1.0;
    for factor in 2..=value {
        result *= factor as f64;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame(name: &str, values: Vec<f64>) -> DataFrame {
        DataFrame::new(vec![Column::new_f64(name, values)]).expect("frame")
    }

    #[test]
    fn the_z_test_uses_the_known_sigma() {
        let df = frame("x", vec![10.0, 11.0, 12.0, 13.0, 14.0]);
        let result = z_test(&df, "x", 10.0, 2.0, Alternative::TwoSided, 0.95).expect("z test");
        // Mean 12, SE = 2/sqrt(5), Z = 2/0.894427 = 2.236068.
        let z = result
            .statistics
            .iter()
            .find(|(name, _)| *name == "Z")
            .map(|(_, value)| *value)
            .expect("Z");
        assert!((z - 2.236_067_977_499_79).abs() < 1e-12, "Z = {z}");
        let p = result
            .statistics
            .iter()
            .find(|(name, _)| *name == "p-value")
            .map(|(_, value)| *value)
            .expect("p");
        assert!((p - 0.025_347_318_677_468_277).abs() < 1e-12, "p = {p}");
        let lower = result
            .statistics
            .iter()
            .find(|(name, _)| *name == "CI lower")
            .map(|(_, value)| *value)
            .expect("CI lower");
        assert!((lower - (12.0 - 1.959_963_984_540_054 * 2.0 / 5.0_f64.sqrt())).abs() < 1e-12);
    }

    #[test]
    fn the_poisson_one_sample_rate_matches_the_exact_tail() {
        // 12 occurrences over a length of 4: rate 3.
        let df = frame("counts", vec![3.0, 3.0, 2.0, 4.0]);
        let result = poisson_one(
            &df,
            "counts",
            4.0,
            2.0,
            Alternative::TwoSided,
            PoissonMethod::Exact,
            0.95,
        )
        .expect("poisson rate");
        let p = result
            .statistics
            .iter()
            .find(|(name, _)| *name == "p-value")
            .map(|(_, value)| *value)
            .expect("p");
        // S = 12, L·λ0 = 8: P(S ≤ 12) = 0.9362, P(S ≥ 12) = 0.1119 → p = 0.2238.
        let lower = poisson_cdf(12.0, 8.0);
        let upper = 1.0 - poisson_cdf(11.0, 8.0);
        assert!((lower - 0.936_202).abs() < 1e-5, "{lower}");
        assert!((upper - 0.111_924).abs() < 1e-5, "{upper}");
        assert!((p - (2.0 * lower.min(upper))).abs() < 1e-12);
        assert!(result.statistics.iter().any(|(name, _)| *name == "Rate"));
    }

    #[test]
    fn the_poisson_two_sample_rates_use_the_conditional_test() {
        let df = DataFrame::new(vec![
            Column::new_f64("first", vec![4.0, 5.0, 3.0]),
            Column::new_f64("second", vec![1.0, 2.0, 2.0]),
        ])
        .expect("frame");
        let result = poisson_two(
            &df,
            "first",
            3.0,
            "second",
            3.0,
            Alternative::TwoSided,
            PoissonMethod::Exact,
            0.95,
        )
        .expect("two rates");
        // S = 12, W = 17, p0 = 0.5: two-sided p = 0.143463134765625.
        let p = result
            .statistics
            .iter()
            .find(|(name, _)| *name == "p-value")
            .map(|(_, value)| *value)
            .expect("p");
        assert!((p - 0.143_463_134_765_625).abs() < 1e-12, "p = {p}");
    }

    #[test]
    fn the_poisson_goodness_of_fit_matches_the_published_example() {
        let values = vec![
            2.0, 2.0, 3.0, 3.0, 2.0, 4.0, 4.0, 2.0, 1.0, 1.0, 1.0, 4.0, 4.0, 3.0, 0.0, 4.0, 3.0,
            2.0, 3.0, 3.0, 4.0, 1.0, 3.0, 1.0, 4.0, 3.0, 2.0, 2.0, 1.0, 2.0, 0.0, 2.0, 3.0, 2.0,
            3.0,
        ];
        let result = poisson_gof(&frame("counts", values), "counts", None).expect("gof");
        let value = |name: &str| {
            result
                .statistics
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .expect(name)
        };
        assert_eq!(value("Observations"), 35.0);
        assert!((value("Poisson mean") - 2.4).abs() < 1e-12);
        assert_eq!(value("Categories"), 5.0);
        assert_eq!(value("Degrees of freedom"), 3.0);
        assert!((value("Chi-square") - 1.916_22).abs() < 1e-4);
        assert!((value("p-value") - 0.590).abs() < 1e-3);
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let df = frame("x", vec![1.0, 2.0, 3.0]);
        assert!(z_test(&df, "x", 0.0, 0.0, Alternative::TwoSided, 0.95).is_err());
        assert!(
            poisson_one(
                &df,
                "x",
                0.0,
                1.0,
                Alternative::TwoSided,
                PoissonMethod::Exact,
                0.95
            )
            .is_err()
        );
        assert!(
            poisson_one(
                &df,
                "x",
                1.0,
                0.0,
                Alternative::TwoSided,
                PoissonMethod::Exact,
                0.95
            )
            .is_err()
        );
        assert!(poisson_gof(&df, "x", Some(0.0)).is_err());
        let fractional = frame("counts", vec![1.5]);
        assert!(poisson_gof(&fractional, "counts", None).is_err());
    }
}
