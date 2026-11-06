//! Normal tolerance intervals.
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
//! The documented behaviour has a tolerance-interval dialog; its conventions
//! were not documented, so the numbers are not claimed as documented behaviour —
//! see `the project notes`.

use crate::data::{DataFrame, MissingPolicy, Sample};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{chi_square_quantile, normal_quantile, regularized_beta};

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

/// Runs the distribution-free tolerance interval and builds the report.
pub fn analyze_distribution_free(
    df: &DataFrame,
    column_name: &str,
    proportion: f64,
    confidence: f64,
) -> Result<AnalysisResult> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let interval = distribution_free(&sample, proportion, confidence)?;
    let mut result = AnalysisResult::new(
        format!("Distribution-Free Tolerance Interval — column {column_name}"),
        format!(
            "Order statistics, {:.1}% of the population with {:.1}% confidence",
            proportion * 100.0,
            confidence * 100.0
        ),
    );
    result.n = interval.n;
    result.excluded = sample.excluded();
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
            "the bounds are order statistics; the coverage between X(r) and X(n−r+1) follows a \
             Beta(n − 2r + 1, 2r) law, so the confidence is 1 − I_p(n − 2r + 1, 2r)"
                .to_owned(),
        ),
        (
            "Order".to_owned(),
            "the largest r that still reaches the confidence, i.e. the narrowest such interval"
                .to_owned(),
        ),
        (
            "Assumption".to_owned(),
            "none about the shape of the distribution; the sample must be random".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, sample.ties());
    result.warnings.push(
        "the distribution-free construction is exact for a continuous population; ties in the \
         data make the achieved confidence approximate"
            .to_owned(),
    );
    Ok(result)
}

/// Runs the tolerance interval analysis and builds the report.
pub fn analyze(
    df: &DataFrame,
    column_name: &str,
    proportion: f64,
    confidence: f64,
) -> Result<AnalysisResult> {
    validate_level("proportion", proportion)?;
    validate_level("confidence", confidence)?;
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let mean = sample.mean()?;
    let sd = sample.sd()?;
    let (low, high) = interval(mean, sd, sample.n(), proportion, confidence)?;
    let k = factor(sample.n(), proportion, confidence)?;

    let mut result = AnalysisResult::new(
        format!("Tolerance Interval — column {column_name}"),
        format!(
            "Normal two-sided interval, {:.1}% of the population with {:.1}% confidence",
            proportion * 100.0,
            confidence * 100.0
        ),
    );
    result.n = sample.n();
    result.excluded = sample.excluded();
    result.statistics = vec![
        ("Mean", mean),
        ("Standard deviation", sd),
        ("Tolerance factor k", k),
        ("Lower limit", low),
        ("Upper limit", high),
    ];
    result.extra.push(Item::Table {
        headers: vec!["Quantity".to_owned(), "Value".to_owned()],
        rows: vec![
            vec!["Mean".to_owned(), format!("{mean:.6}")],
            vec!["Standard deviation".to_owned(), format!("{sd:.6}")],
            vec!["Tolerance factor".to_owned(), format!("{k:.6}")],
            vec!["Lower limit".to_owned(), format!("{low:.6}")],
            vec!["Upper limit".to_owned(), format!("{high:.6}")],
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
            "Factor".to_owned(),
            "Howe's approximation: sqrt((n−1)·(1 + 1/n)·z²_((1+p)/2) / χ²_(1−γ, n−1))".to_owned(),
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
