// SPDX-License-Identifier: MIT
//! Normal and distribution-free tolerance intervals.
//!
//! A two-sided tolerance interval covers at least a proportion `p` of the
//! population with a confidence `γ`. The factor is Howe's approximation
//!
//! ```text
//! k = sqrt( (ν·(1 + 1/n)·z²_{(1+p)/2}) / χ²_{1−γ,ν} )
//! ```
//!
//! with `ν = n − 1`, which reproduces the published tables (n = 10 and
//! n = 30 give 3.379 and 2.549 for 95 %/95 %).
//!
//! A **one-sided** interval keeps only one limit, so it needs less data for the
//! same content and confidence. Its factor is the exact non-central-t solution
//!
//! ```text
//! k = t′_{ν,γ,δ} / √n,   δ = z_p·√n,
//! ```
//!
//! where `t′` is the `γ` quantile of the non-central t with `ν` degrees of
//! freedom and non-centrality `δ`. The probability is integrated over the
//! chi-square law of the variance estimate: `P(T ≤ t) = E[Φ(t·√(V/ν) − δ)]`.
//!
//! Without a distributional assumption the bounds are **order statistics**: the
//! interval between `X₍ᵣ₎` and `X₍ₙ₋ᵣ₊₁₎` covers a proportion with confidence
//! `1 − I_p(n − 2r + 1, 2r)`, and a one-sided bound `X₍ᵣ₎` (or `X₍ₙ₋ᵣ₊₁₎`)
//! has confidence `1 − I_p(n − r + 1, r)`.
//!
//! The documented behaviour has a tolerance-interval dialog; its conventions
//! were not documented, so the numbers are not claimed to match the documented behaviour —
//! see the project notes.

use crate::data::{DataFrame, MissingPolicy, Sample};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{
    chi_square_quantile, ln_gamma, normal_cdf, normal_quantile, regularized_beta,
};

/// Which limit (or limits) a tolerance interval keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Side {
    /// Both limits, from the two-sided factor.
    #[default]
    TwoSided,
    /// Only the lower limit: at least `p` of the population lies above it.
    Lower,
    /// Only the upper limit: at least `p` of the population lies below it.
    Upper,
}

impl Side {
    /// Parses the command-line spellings of a side.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "two" | "two-sided" | "two_sided" | "both" => Some(Self::TwoSided),
            "lower" | "low" => Some(Self::Lower),
            "upper" | "high" => Some(Self::Upper),
            _ => None,
        }
    }

    /// Short code used by the preferences and the command line.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TwoSided => "two",
            Self::Lower => "lower",
            Self::Upper => "upper",
        }
    }

    /// Name used in the report method line.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::TwoSided => "two-sided",
            Self::Lower => "lower one-sided",
            Self::Upper => "upper one-sided",
        }
    }

    /// Whether both limits are reported.
    #[must_use]
    pub fn is_two_sided(self) -> bool {
        matches!(self, Self::TwoSided)
    }
}

/// Validates a proportion or a confidence level.
fn validate_level(name: &'static str, value: f64) -> Result<()> {
    if !value.is_finite() || value <= 0.0 || value >= 1.0 {
        return Err(Error::InvalidParameter { name, value });
    }
    Ok(())
}

/// The two-sided normal tolerance factor.
///
/// `proportion` is the population proportion the interval must cover and
/// `confidence` the confidence of the statement.
pub fn factor(n: usize, proportion: f64, confidence: f64) -> Result<f64> {
    validate_level("proportion", proportion)?;
    validate_level("confidence", confidence)?;
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    let nu = (n - 1) as f64;
    let quantile = normal_quantile(0.5 * (1.0 + proportion));
    let chi = chi_square_quantile(1.0 - confidence, nu);
    if !quantile.is_finite() || !chi.is_finite() || chi <= 0.0 {
        return Err(Error::NotConverged {
            detail: "tolerance factor".to_owned(),
        });
    }
    Ok((nu * (1.0 + 1.0 / n as f64) * quantile * quantile / chi).sqrt())
}

/// `P(T ≤ t)` for the non-central t with `df` degrees of freedom and
/// non-centrality `ncp`.
///
/// `T = (Z + ncp)/√(V/df)` with `Z` standard normal and `V` chi-square, so the
/// probability is the expectation of `Φ(t·√(V/df) − ncp)` over `V`. The
/// integral uses `s = u²` for the chi-square variable, which leaves a smooth
/// integrand even at one degree of freedom.
fn non_central_t_cdf(t: f64, df: f64, ncp: f64) -> f64 {
    if !t.is_finite() || !df.is_finite() || df < 1.0 || !ncp.is_finite() {
        return f64::NAN;
    }
    const INTERVALS: usize = 4000;
    let scale = t / df.sqrt();
    let half = df / 2.0;
    let log_norm = (1.0 - half) * std::f64::consts::LN_2 - ln_gamma(half);
    let upper = (df + 15.0 * (2.0 * df).sqrt() + 80.0).sqrt();
    let step = upper / INTERVALS as f64;
    let mut total = 0.0;
    for index in 0..=INTERVALS {
        let u = step * index as f64;
        let density = if index == 0 {
            if (df - 1.0).abs() < 0.5 {
                log_norm.exp()
            } else {
                0.0
            }
        } else {
            (log_norm + (df - 1.0) * u.ln() - 0.5 * u * u).exp()
        };
        let weight = match index {
            0 | INTERVALS => 1.0,
            index if index % 2 == 1 => 4.0,
            _ => 2.0,
        };
        total += weight * normal_cdf(scale * u - ncp) * density;
    }
    (total * step / 3.0).clamp(0.0, 1.0)
}

/// The exact one-sided normal tolerance factor.
///
/// It solves `P(T′ ≤ k·√n) = confidence` for the non-central t with `n − 1`
/// degrees of freedom and non-centrality `z_proportion·√n`.
pub fn one_sided_factor(n: usize, proportion: f64, confidence: f64) -> Result<f64> {
    validate_level("proportion", proportion)?;
    validate_level("confidence", confidence)?;
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    let quantile = normal_quantile(proportion);
    if !quantile.is_finite() {
        return Err(Error::NotConverged {
            detail: "one-sided tolerance factor".to_owned(),
        });
    }
    let df = (n - 1) as f64;
    let root = (n as f64).sqrt();
    let non_centrality = quantile * root;
    let mut low = quantile;
    let mut high = quantile + 60.0;
    if non_central_t_cdf(high * root, df, non_centrality) < confidence {
        return Err(Error::NotConverged {
            detail: "one-sided tolerance factor".to_owned(),
        });
    }
    for _ in 0..80 {
        let middle = 0.5 * (low + high);
        if non_central_t_cdf(middle * root, df, non_centrality) < confidence {
            low = middle;
        } else {
            high = middle;
        }
    }
    Ok(0.5 * (low + high))
}

/// The tolerance factor of the requested side.
pub fn factor_side(n: usize, proportion: f64, confidence: f64, side: Side) -> Result<f64> {
    match side {
        Side::TwoSided => factor(n, proportion, confidence),
        Side::Lower | Side::Upper => one_sided_factor(n, proportion, confidence),
    }
}

/// The normal tolerance interval of the requested side.
///
/// A one-sided interval has only the reported limit; the missing end is
/// `None`.
pub fn interval_side(
    mean: f64,
    sd: f64,
    n: usize,
    proportion: f64,
    confidence: f64,
    side: Side,
) -> Result<(Option<f64>, Option<f64>)> {
    if !mean.is_finite() {
        return Err(Error::InvalidParameter {
            name: "mean",
            value: mean,
        });
    }
    if !sd.is_finite() || sd <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let k = factor_side(n, proportion, confidence, side)?;
    Ok(match side {
        Side::TwoSided => (Some(mean - k * sd), Some(mean + k * sd)),
        Side::Lower => (Some(mean - k * sd), None),
        Side::Upper => (None, Some(mean + k * sd)),
    })
}

/// The two-sided normal tolerance interval of the given statistics.
pub fn interval(
    mean: f64,
    sd: f64,
    n: usize,
    proportion: f64,
    confidence: f64,
) -> Result<(f64, f64)> {
    if !mean.is_finite() {
        return Err(Error::InvalidParameter {
            name: "mean",
            value: mean,
        });
    }
    if !sd.is_finite() || sd <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let k = factor(n, proportion, confidence)?;
    Ok((mean - k * sd, mean + k * sd))
}

/// A **distribution-free** two-sided tolerance interval from order statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct FreeInterval {
    /// Order of the lower bound (1-based); the upper bound is `n − order + 1`.
    pub order: usize,
    /// Observations.
    pub n: usize,
    /// Lower bound (the `order`-th smallest observation).
    pub low: f64,
    /// Upper bound (the `order`-th largest observation).
    pub high: f64,
    /// Confidence actually achieved.
    pub achieved: f64,
}

/// Smallest sample size whose **min and max** reach the confidence.
///
/// The two-sided condition is `1 − I_p(n − 1, 2) ≥ γ`, which needs 93
/// observations for 95 %/95 % (the one-sided bound would only need 59).
#[must_use]
pub fn minimum_sample(proportion: f64, confidence: f64) -> Option<usize> {
    if !(0.0..1.0).contains(&proportion) || !(0.0..1.0).contains(&confidence) {
        return None;
    }
    for n in 2..=1_000_000usize {
        let achieved = 1.0 - regularized_beta(proportion, (n - 1) as f64, 2.0);
        if achieved >= confidence {
            return Some(n);
        }
    }
    None
}

/// Builds the narrowest distribution-free interval that keeps the confidence.
///
/// The coverage between the order statistics `X₍ᵣ₎` and `X₍ₙ₋ᵣ₊₁₎` follows a
/// `Beta(n − 2r + 1, 2r)` law, so the confidence is
/// `1 − I_p(n − 2r + 1, 2r)`. The function takes the **largest** `r` that still
/// reaches `confidence`, which is the narrowest interval of the sample.
pub fn distribution_free(
    sample: &Sample,
    proportion: f64,
    confidence: f64,
) -> Result<FreeInterval> {
    validate_level("proportion", proportion)?;
    validate_level("confidence", confidence)?;
    let n = sample.n();
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    let sorted = sample.sorted();
    let mut best: Option<FreeInterval> = None;
    for order in 1..=(n / 2) {
        let a = (n - 2 * order + 1) as f64;
        let b = (2 * order) as f64;
        let achieved = 1.0 - regularized_beta(proportion, a, b);
        if achieved >= confidence {
            best = Some(FreeInterval {
                order,
                n,
                low: sorted[order - 1],
                high: sorted[n - order],
                achieved,
            });
        }
    }
    match best {
        Some(interval) => Ok(interval),
        None => {
            let needed = minimum_sample(proportion, confidence).unwrap_or(0);
            Err(Error::Unsupported(format!(
                "the sample is too small for a distribution-free interval: {n} observation(s), \
                 at least {needed} are needed"
            )))
        }
    }
}

/// Smallest sample size whose one-sided bound reaches the confidence.
///
/// A one-sided bound is the smallest or largest observation, whose coverage is
/// `1 − pⁿ`; 95 %/95 % therefore needs 59 observations.
#[must_use]
pub fn minimum_sample_side(proportion: f64, confidence: f64, side: Side) -> Option<usize> {
    if side.is_two_sided() {
        return minimum_sample(proportion, confidence);
    }
    if !(0.0..1.0).contains(&proportion) || !(0.0..1.0).contains(&confidence) {
        return None;
    }
    for n in 1..=1_000_000usize {
        if 1.0 - proportion.powi(n as i32) >= confidence {
            return Some(n);
        }
    }
    None
}

/// A **distribution-free** one-sided tolerance bound.
#[derive(Debug, Clone, PartialEq)]
pub struct FreeBound {
    /// Lower or upper side.
    pub side: Side,
    /// Order counted from the extreme: `X₍ᵣ₎` for a lower bound and
    /// `X₍ₙ₋ᵣ₊₁₎` for an upper bound (1-based).
    pub order: usize,
    /// Observations.
    pub n: usize,
    /// The bound (the `order`-th smallest or largest observation).
    pub value: f64,
    /// Confidence actually achieved.
    pub achieved: f64,
}

/// Builds the tightest distribution-free one-sided bound that keeps the
/// confidence.
///
/// The bound `X₍ᵣ₎` (lower) or `X₍ₙ₋ᵣ₊₁₎` (upper) covers a proportion with
/// confidence `1 − I_p(n − r + 1, r)`; the function takes the largest `r` that
/// still reaches `confidence`, which is the bound closest to the other end.
pub fn distribution_free_one_sided(
    sample: &Sample,
    proportion: f64,
    confidence: f64,
    side: Side,
) -> Result<FreeBound> {
    validate_level("proportion", proportion)?;
    validate_level("confidence", confidence)?;
    if side.is_two_sided() {
        return Err(Error::Unsupported(
            "a one-sided bound needs the lower or the upper side".to_owned(),
        ));
    }
    let n = sample.n();
    if n < 1 {
        return Err(Error::SampleTooSmall { n, min: 1 });
    }
    let sorted = sample.sorted();
    let mut best: Option<FreeBound> = None;
    for order in 1..=n {
        let achieved = 1.0 - regularized_beta(proportion, (n - order + 1) as f64, order as f64);
        if achieved >= confidence {
            best = Some(FreeBound {
                side,
                order,
                n,
                value: if side == Side::Lower {
                    sorted[order - 1]
                } else {
                    sorted[n - order]
                },
                achieved,
            });
        }
    }
    match best {
        Some(bound) => Ok(bound),
        None => {
            let needed = minimum_sample_side(proportion, confidence, side).unwrap_or(0);
            Err(Error::Unsupported(format!(
                "the sample is too small for a distribution-free bound: {n} observation(s), \
                 at least {needed} are needed"
            )))
        }
    }
}

/// Runs the distribution-free two-sided tolerance interval and builds the
/// report.
pub fn analyze_distribution_free(
    df: &DataFrame,
    column_name: &str,
    proportion: f64,
    confidence: f64,
) -> Result<AnalysisResult> {
    analyze_distribution_free_side(df, column_name, proportion, confidence, Side::TwoSided)
}

/// Runs the distribution-free tolerance interval of the requested side and
/// builds the report.
pub fn analyze_distribution_free_side(
    df: &DataFrame,
    column_name: &str,
    proportion: f64,
    confidence: f64,
    side: Side,
) -> Result<AnalysisResult> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let mut result = AnalysisResult::new(
        format!("Distribution-Free Tolerance Interval — column {column_name}"),
        format!(
            "Order statistics, {}, {:.1}% of the population with {:.1}% confidence",
            side.name(),
            proportion * 100.0,
            confidence * 100.0
        ),
    );
    result.excluded = sample.excluded();
    if side.is_two_sided() {
        let interval = distribution_free(&sample, proportion, confidence)?;
        result.n = interval.n;
        result.statistics = vec![
            ("Order r", interval.order as f64),
            ("Lower limit", interval.low),
            ("Upper limit", interval.high),
            ("Achieved confidence", interval.achieved),
            ("Median", sample.median()?),
        ];
        result.extra.push(Item::Table {
            headers: vec!["Quantity".to_owned(), "Value".to_owned()],
            rows: vec![
                vec!["Observations".to_owned(), format!("{}", interval.n)],
                vec![
                    "Bounds".to_owned(),
                    format!(
                        "X({}) and X({})",
                        interval.order,
                        interval.n - interval.order + 1
                    ),
                ],
                vec!["Lower limit".to_owned(), format!("{:.6}", interval.low)],
                vec!["Upper limit".to_owned(), format!("{:.6}", interval.high)],
                vec![
                    "Achieved confidence".to_owned(),
                    format!("{:.6}", interval.achieved),
                ],
            ],
        });
        result.notes = vec![
            (
                "Statement".to_owned(),
                format!(
                    "at least {:.1}% of the population lies between the limits, with {:.1}% \
                     confidence",
                    proportion * 100.0,
                    confidence * 100.0
                ),
            ),
            (
                "Method".to_owned(),
                "the bounds are order statistics; the coverage between X(r) and X(n−r+1) \
                 follows a Beta(n − 2r + 1, 2r) law, so the confidence is 1 − I_p(n − 2r + 1, 2r)"
                    .to_owned(),
            ),
            (
                "Order".to_owned(),
                "the largest r that still reaches the confidence, i.e. the narrowest such \
                 interval"
                    .to_owned(),
            ),
            (
                "Assumption".to_owned(),
                "none about the shape of the distribution; the sample must be random".to_owned(),
            ),
        ];
    } else {
        let bound = distribution_free_one_sided(&sample, proportion, confidence, side)?;
        let label = if side == Side::Lower {
            "Lower limit"
        } else {
            "Upper limit"
        };
        let order = if side == Side::Lower {
            bound.order
        } else {
            bound.n - bound.order + 1
        };
        result.n = bound.n;
        result.statistics = vec![
            ("Order r", bound.order as f64),
            (label, bound.value),
            ("Achieved confidence", bound.achieved),
            ("Median", sample.median()?),
        ];
        result.extra.push(Item::Table {
            headers: vec!["Quantity".to_owned(), "Value".to_owned()],
            rows: vec![
                vec!["Observations".to_owned(), format!("{}", bound.n)],
                vec!["Bound".to_owned(), format!("X({order})")],
                vec![label.to_owned(), format!("{:.6}", bound.value)],
                vec![
                    "Achieved confidence".to_owned(),
                    format!("{:.6}", bound.achieved),
                ],
            ],
        });
        result.notes =
            vec![
            (
                "Statement".to_owned(),
                format!(
                    "at least {:.1}% of the population lies {} the limit, with {:.1}% confidence",
                    proportion * 100.0,
                    if side == Side::Lower { "above" } else { "below" },
                    confidence * 100.0
                ),
            ),
            (
                "Method".to_owned(),
                "the bound is an order statistic; the coverage of X(r) (lower) or X(n−r+1) \
                 (upper) follows a Beta(n − r + 1, r) law, so the confidence is \
                 1 − I_p(n − r + 1, r)"
                    .to_owned(),
            ),
            (
                "Order".to_owned(),
                "the largest r that still reaches the confidence, i.e. the bound closest to the \
                 other end of the sample"
                    .to_owned(),
            ),
            (
                "Assumption".to_owned(),
                "none about the shape of the distribution; the sample must be random".to_owned(),
            ),
        ];
    }
    result.warnings = standard_warnings(result.n, result.excluded, sample.ties());
    result.warnings.push(
        "the distribution-free construction is exact for a continuous population; ties in the \
         data make the achieved confidence approximate"
            .to_owned(),
    );
    Ok(result)
}

/// Runs the normal two-sided tolerance interval and builds the report.
pub fn analyze(
    df: &DataFrame,
    column_name: &str,
    proportion: f64,
    confidence: f64,
) -> Result<AnalysisResult> {
    analyze_side(df, column_name, proportion, confidence, Side::TwoSided)
}

/// Runs the normal tolerance interval of the requested side and builds the
/// report.
pub fn analyze_side(
    df: &DataFrame,
    column_name: &str,
    proportion: f64,
    confidence: f64,
    side: Side,
) -> Result<AnalysisResult> {
    validate_level("proportion", proportion)?;
    validate_level("confidence", confidence)?;
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let mean = sample.mean()?;
    let sd = sample.sd()?;
    let (low, high) = interval_side(mean, sd, sample.n(), proportion, confidence, side)?;
    let k = factor_side(sample.n(), proportion, confidence, side)?;

    let mut statistics = vec![
        ("Mean", mean),
        ("Standard deviation", sd),
        ("Tolerance factor k", k),
    ];
    let mut rows = vec![
        vec!["Mean".to_owned(), format!("{mean:.6}")],
        vec!["Standard deviation".to_owned(), format!("{sd:.6}")],
        vec!["Tolerance factor".to_owned(), format!("{k:.6}")],
    ];
    if let Some(low) = low {
        statistics.push(("Lower limit", low));
        rows.push(vec!["Lower limit".to_owned(), format!("{low:.6}")]);
    }
    if let Some(high) = high {
        statistics.push(("Upper limit", high));
        rows.push(vec!["Upper limit".to_owned(), format!("{high:.6}")]);
    }

    let mut result = AnalysisResult::new(
        format!("Tolerance Interval — column {column_name}"),
        format!(
            "Normal {} interval, {:.1}% of the population with {:.1}% confidence",
            side.name(),
            proportion * 100.0,
            confidence * 100.0
        ),
    );
    result.n = sample.n();
    result.excluded = sample.excluded();
    result.statistics = statistics;
    result.extra.push(Item::Table {
        headers: vec!["Quantity".to_owned(), "Value".to_owned()],
        rows,
    });
    result.notes = vec![
        (
            "Statement".to_owned(),
            match side {
                Side::TwoSided => format!(
                    "at least {:.1}% of the population lies between the limits, with {:.1}% \
                     confidence",
                    proportion * 100.0,
                    confidence * 100.0
                ),
                Side::Lower => format!(
                    "at least {:.1}% of the population lies above the lower limit, with {:.1}% \
                     confidence",
                    proportion * 100.0,
                    confidence * 100.0
                ),
                Side::Upper => format!(
                    "at least {:.1}% of the population lies below the upper limit, with {:.1}% \
                     confidence",
                    proportion * 100.0,
                    confidence * 100.0
                ),
            },
        ),
        (
            "Factor".to_owned(),
            match side {
                Side::TwoSided => {
                    "Howe's approximation: sqrt((n−1)·(1 + 1/n)·z²_((1+p)/2) / χ²_(1−γ, n−1))"
                        .to_owned()
                }
                Side::Lower | Side::Upper => {
                    "exact one-sided factor from the non-central t: k = t′_(ν, γ, δ)/√n with \
                     δ = z_p·√n, where ν = n − 1; the probability is integrated over the \
                     chi-square law of the variance estimate"
                        .to_owned()
                }
            },
        ),
        (
            "Assumption".to_owned(),
            "the population must be normal; check it with a normality test".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, sample.ties());
    result.warnings.push(
        "the tolerance-interval conventions are standard choices; they have not been verified \
         against documented output"
            .to_owned(),
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    #[test]
    fn the_factor_matches_the_published_tables() {
        // 95 %/95 % two-sided factors from the tables.
        let k10 = factor(10, 0.95, 0.95).expect("factor");
        assert!((k10 - 3.379).abs() < 0.01, "k(10) {k10}");
        let k30 = factor(30, 0.95, 0.95).expect("factor");
        assert!((k30 - 2.549).abs() < 0.01, "k(30) {k30}");
        // A higher content needs a wider factor.
        let k50 = factor(50, 0.99, 0.95).expect("factor");
        assert!(k50 > k30, "k(50, 99) {k50}");
    }

    #[test]
    fn the_interval_contains_the_mean() {
        let (low, high) = interval(10.0, 2.0, 30, 0.95, 0.95).expect("interval");
        assert!(low < 10.0 && high > 10.0);
        // The interval is wider than the confidence interval of the mean.
        assert!((high - low) > 2.0 * 2.0);
    }

    #[test]
    fn invalid_levels_are_rejected() {
        assert!(matches!(
            factor(10, 1.5, 0.95),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            factor(10, 0.95, 0.0),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            factor(1, 0.95, 0.95),
            Err(Error::SampleTooSmall { .. })
        ));
    }

    #[test]
    fn the_distribution_free_interval_needs_enough_observations() {
        // The two-sided min/max interval needs 93 observations for 95 %/95 %
        // (the one-sided bound would be 59).
        assert_eq!(minimum_sample(0.95, 0.95), Some(93));
        let values: Vec<f64> = (1..=93).map(|index| index as f64).collect();
        let sample = Sample::new(values, MissingPolicy::Drop).expect("sample");
        let interval = distribution_free(&sample, 0.95, 0.95).expect("interval");
        assert_eq!(interval.order, 1);
        assert!((interval.low - 1.0).abs() < 1e-12);
        assert!((interval.high - 93.0).abs() < 1e-12);
        assert!(interval.achieved >= 0.95);
        // With 20 observations no order statistic reaches 95/95.
        let values: Vec<f64> = (1..=20).map(|index| index as f64).collect();
        let sample = Sample::new(values, MissingPolicy::Drop).expect("sample");
        let error = distribution_free(&sample, 0.95, 0.95).expect_err("too small");
        assert!(error.to_string().contains("93"), "{error}");
    }

    #[test]
    fn the_distribution_free_interval_shrinks_with_large_samples() {
        // With 500 observations the order can grow beyond the extremes.
        let values: Vec<f64> = (1..=500).map(|index| index as f64).collect();
        let sample = Sample::new(values, MissingPolicy::Drop).expect("sample");
        let interval = distribution_free(&sample, 0.9, 0.9).expect("interval");
        assert!(interval.order > 1, "order {}", interval.order);
        assert!(interval.low > 1.0 && interval.high < 500.0);
        assert!(interval.achieved >= 0.9);
        // A tighter order would fall below the confidence.
        let looser = interval.order + 1;
        let a = (500 - 2 * looser + 1) as f64;
        let b = (2 * looser) as f64;
        assert!(1.0 - regularized_beta(0.9, a, b) < 0.9);
    }

    #[test]
    fn the_distribution_free_report_is_built() {
        let values: Vec<f64> = (1..=120).map(|index| index as f64 * 0.5).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        let report = analyze_distribution_free(&df, "x", 0.95, 0.95).expect("report");
        let text = report.to_session_text();
        assert!(
            text.contains("Distribution-Free Tolerance Interval"),
            "{text}"
        );
        assert!(text.contains("Achieved confidence"), "{text}");
    }

    #[test]
    fn the_one_sided_factor_matches_the_exact_non_central_t_solution() {
        // One-sided 95 %/95 % factors: the exact non-central-t quantiles
        // t′_(ν, γ, δ)/√n with δ = z_p·√n.
        for (n, expected) in [
            (2usize, 26.259_673_955),
            (5, 4.202_680_737),
            (10, 2.910_963_410),
            (30, 2.219_837_530),
            (100, 1.926_538_849),
        ] {
            let k = one_sided_factor(n, 0.95, 0.95).expect("factor");
            assert!((k - expected).abs() < 1e-3, "n={n}: {k} vs {expected}");
        }
        // 90 %/90 % at n = 10, and a higher content at 95 % confidence.
        let k90 = one_sided_factor(10, 0.90, 0.90).expect("factor");
        assert!((k90 - 2.065_668_318).abs() < 1e-3, "{k90}");
        let k99 = one_sided_factor(10, 0.99, 0.95).expect("factor");
        assert!((k99 - 3.981_117_846).abs() < 1e-3, "{k99}");
        // The one-sided factor is below the two-sided one for the same levels.
        assert!(
            one_sided_factor(30, 0.95, 0.95).expect("one") < factor(30, 0.95, 0.95).expect("two")
        );
    }

    #[test]
    fn the_one_sided_interval_keeps_a_single_limit() {
        let (low, high) = interval_side(10.0, 2.0, 10, 0.95, 0.95, Side::Lower).expect("lower");
        let k = one_sided_factor(10, 0.95, 0.95).expect("factor");
        assert!((low.expect("lower") - (10.0 - k * 2.0)).abs() < 1e-12);
        assert!(high.is_none());
        let (low, high) = interval_side(10.0, 2.0, 10, 0.95, 0.95, Side::Upper).expect("upper");
        assert!(low.is_none());
        assert!((high.expect("upper") - (10.0 + k * 2.0)).abs() < 1e-12);
        // The two-sided interval keeps both limits and is wider on each side
        // than a one-sided bound would be at the same confidence.
        let (low, high) = interval_side(10.0, 2.0, 10, 0.95, 0.95, Side::TwoSided).expect("two");
        let two = factor(10, 0.95, 0.95).expect("k");
        assert!((low.expect("low") - (10.0 - two * 2.0)).abs() < 1e-12);
        assert!((high.expect("high") - (10.0 + two * 2.0)).abs() < 1e-12);
        assert!(two > k);
        assert!(matches!(
            interval_side(10.0, 0.0, 10, 0.95, 0.95, Side::Lower),
            Err(Error::ConstantSample)
        ));
    }

    #[test]
    fn the_one_sided_factor_rejects_invalid_levels_and_samples() {
        assert!(matches!(
            one_sided_factor(10, 0.0, 0.95),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            one_sided_factor(10, 0.95, 1.0),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            one_sided_factor(1, 0.95, 0.95),
            Err(Error::SampleTooSmall { .. })
        ));
        assert_eq!(Side::parse("lower"), Some(Side::Lower));
        assert_eq!(Side::parse("upper"), Some(Side::Upper));
        assert_eq!(Side::parse("two-sided"), Some(Side::TwoSided));
        assert!(Side::parse("sideways").is_none());
        assert_eq!(Side::default().as_str(), "two");
    }

    #[test]
    fn the_one_sided_bound_needs_fewer_observations() {
        // One-sided 95 %/95 % only needs the min or the max: 59 observations,
        // against 93 for the two-sided min/max interval.
        assert_eq!(minimum_sample_side(0.95, 0.95, Side::Lower), Some(59));
        assert_eq!(minimum_sample_side(0.95, 0.95, Side::Upper), Some(59));
        assert_eq!(minimum_sample_side(0.95, 0.95, Side::TwoSided), Some(93));
        let values: Vec<f64> = (1..=59).map(|index| index as f64).collect();
        let sample = Sample::new(values, MissingPolicy::Drop).expect("sample");
        let lower = distribution_free_one_sided(&sample, 0.95, 0.95, Side::Lower).expect("lower");
        assert_eq!(lower.order, 1);
        assert!((lower.value - 1.0).abs() < 1e-12);
        assert!((lower.achieved - (1.0 - 0.95f64.powi(59))).abs() < 1e-12);
        let upper = distribution_free_one_sided(&sample, 0.95, 0.95, Side::Upper).expect("upper");
        assert!((upper.value - 59.0).abs() < 1e-12);
        // The tighter order does not reach the confidence.
        let tighter = 1.0 - regularized_beta(0.95, 58.0, 2.0);
        assert!(tighter < 0.95);
        // 20 observations are not enough and the message names the 59.
        let values: Vec<f64> = (1..=20).map(|index| index as f64).collect();
        let sample = Sample::new(values, MissingPolicy::Drop).expect("sample");
        let error =
            distribution_free_one_sided(&sample, 0.95, 0.95, Side::Lower).expect_err("too small");
        assert!(error.to_string().contains("59"), "{error}");
        assert!(distribution_free_one_sided(&sample, 0.95, 0.95, Side::TwoSided).is_err());
    }

    #[test]
    fn the_one_sided_bound_grows_with_large_samples() {
        let values: Vec<f64> = (1..=500).map(|index| index as f64).collect();
        let sample = Sample::new(values, MissingPolicy::Drop).expect("sample");
        let bound = distribution_free_one_sided(&sample, 0.9, 0.9, Side::Lower).expect("bound");
        assert!(bound.order > 1, "order {}", bound.order);
        assert!(bound.value > 1.0);
        assert!(bound.achieved >= 0.9);
        let looser = bound.order + 1;
        if looser <= 500 {
            let a = (500 - looser + 1) as f64;
            let b = looser as f64;
            assert!(1.0 - regularized_beta(0.9, a, b) < 0.9);
        }
    }

    #[test]
    fn the_one_sided_reports_print_only_their_limit() {
        let values: Vec<f64> = (1..=30).map(|i| 10.0 + (i % 7) as f64 * 0.3).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        let lower = analyze_side(&df, "x", 0.95, 0.95, Side::Lower).expect("lower");
        let text = lower.to_session_text();
        assert!(text.contains("lower one-sided"), "{text}");
        assert!(text.contains("Lower limit"), "{text}");
        assert!(!text.contains("Upper limit"), "{text}");
        assert!(text.contains("non-central t"), "{text}");
        let upper = analyze_side(&df, "x", 0.95, 0.95, Side::Upper).expect("upper");
        let text = upper.to_session_text();
        assert!(text.contains("upper one-sided"), "{text}");
        assert!(text.contains("Upper limit"), "{text}");
        assert!(!text.contains("Lower limit"), "{text}");
        // The two-sided report is unchanged.
        let two = analyze_side(&df, "x", 0.95, 0.95, Side::TwoSided).expect("two");
        let text = two.to_session_text();
        assert!(text.contains("two-sided"), "{text}");
        assert!(
            text.contains("Lower limit") && text.contains("Upper limit"),
            "{text}"
        );
        // A constant column has no normal interval.
        let flat = DataFrame::new(vec![Column::new_f64("y", vec![2.0; 10])]).expect("frame");
        assert!(matches!(
            analyze_side(&flat, "y", 0.95, 0.95, Side::Lower),
            Err(Error::ConstantSample)
        ));
    }

    #[test]
    fn the_distribution_free_one_sided_report_is_built() {
        let values: Vec<f64> = (1..=120).map(|index| index as f64 * 0.5).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        let report =
            analyze_distribution_free_side(&df, "x", 0.95, 0.95, Side::Lower).expect("report");
        let text = report.to_session_text();
        assert!(
            text.contains("Distribution-Free Tolerance Interval"),
            "{text}"
        );
        assert!(text.contains("lower one-sided"), "{text}");
        assert!(text.contains("Lower limit"), "{text}");
        assert!(!text.contains("Upper limit"), "{text}");
        assert!(text.contains("above the limit"), "{text}");
        assert!(text.contains("Achieved confidence"), "{text}");
        let upper =
            analyze_distribution_free_side(&df, "x", 0.95, 0.95, Side::Upper).expect("upper");
        assert!(upper.to_session_text().contains("below the limit"));
    }

    #[test]
    fn the_report_gives_the_limits() {
        let values: Vec<f64> = (1..=30).map(|i| 10.0 + (i % 7) as f64 * 0.3).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        let result = analyze(&df, "x", 0.95, 0.95).expect("analysis");
        let text = result.to_session_text();
        assert!(text.contains("Tolerance Interval"), "{text}");
        assert!(text.contains("Lower limit"), "{text}");
        assert!(text.contains("Tolerance factor"), "{text}");
    }
}
