//! Box–Cox transformation as a standalone analysis.
//!
//! The transformation `x(λ) = (x^λ − 1)/λ` (with the limit `ln x` for
//! `λ = 0`) is fitted by the profile likelihood of the Normal model — the
//! Jacobian term `(λ − 1)·Σ ln x` keeps the optimum on the original-data
//! scale — and the report compares the normality of the data before and
//! after the transformation.

use crate::data::{DataFrame, MissingPolicy, Sample};
use crate::dists::{Distribution, Normal};
use crate::error::{Error, Result};
use crate::gof::{ad_p_value, anderson_darling};
use crate::plot::{PlotSpec, PlottingPosition, normal_probability_plot};
use crate::report::{AnalysisResult, Item, standard_warnings};

/// Box–Cox transformation of one value.
pub fn transform(value: f64, lambda: f64) -> Result<f64> {
    if value <= 0.0 {
        return Err(Error::NonPositive { index: 0, value });
    }
    if lambda.abs() < 1e-12 {
        Ok(value.ln())
    } else {
        let transformed = (lambda * value.ln()).exp_m1() / lambda;
        if transformed.is_finite() {
            Ok(transformed)
        } else {
            Err(Error::NotConverged {
                detail: format!("Box-Cox transformation overflow at lambda = {lambda}"),
            })
        }
    }
}

/// Profile log-likelihood of the Normal model at a `lambda`.
///
/// The Jacobian term `(λ − 1)·Σ ln x` keeps the likelihood on the original
/// data scale, and `−∞` marks a transformation that cannot be evaluated.
fn profile_log_likelihood(values: &[f64], sum_logs: f64, lambda: f64) -> f64 {
    let mut transformed = Vec::with_capacity(values.len());
    for &value in values {
        match transform(value, lambda) {
            Ok(value) => transformed.push(value),
            Err(_) => return f64::NEG_INFINITY,
        }
    }
    let n = transformed.len() as f64;
    let mean = transformed.iter().sum::<f64>() / n;
    let variance: f64 = transformed
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (n - 1.0);
    if variance <= 0.0 {
        return f64::NEG_INFINITY;
    }
    let log_likelihood =
        -0.5 * n * (variance.ln() + 1.0) - 0.5 * n * (2.0 * std::f64::consts::PI).ln();
    log_likelihood + (lambda - 1.0) * sum_logs
}

/// Profile-likelihood estimate of the Box–Cox `lambda`.
///
/// The Jacobian term `(λ − 1)·Σ ln x` is included in the Normal
/// log-likelihood, so the optimum is on the original-data scale. A coarse grid
/// is refined with a golden-section search; no rounded value is substituted.
pub fn lambda(sample: &Sample) -> Result<f64> {
    let values = sample.values();
    if values.len() < 3 {
        return Err(Error::SampleTooSmall {
            n: values.len(),
            min: 3,
        });
    }
    let mut logs = Vec::with_capacity(values.len());
    for (index, &value) in values.iter().enumerate() {
        if value <= 0.0 {
            return Err(Error::NonPositive { index, value });
        }
        logs.push(value.ln());
    }
    let sum_logs: f64 = logs.iter().sum();
    let profile = |lambda: f64| profile_log_likelihood(values, sum_logs, lambda);

    // A coarse grid, then a golden-section refinement around the best point.
    let mut best = 0.0;
    let mut best_value = f64::NEG_INFINITY;
    let mut grid = Vec::with_capacity(41);
    for step in -20..=20 {
        let candidate = step as f64 * 0.25;
        let value = profile(candidate);
        grid.push((candidate, value));
        if value > best_value {
            best_value = value;
            best = candidate;
        }
    }
    let index = grid
        .iter()
        .position(|(_, value)| *value == best_value)
        .unwrap_or(0);
    let mut low = if index > 0 {
        grid[index - 1].0
    } else {
        best - 0.25
    };
    let mut high = if index + 1 < grid.len() {
        grid[index + 1].0
    } else {
        best + 0.25
    };
    let ratio = 0.618_033_988_749_895;
    let mut c = high - ratio * (high - low);
    let mut d = low + ratio * (high - low);
    for _ in 0..200 {
        if profile(c) > profile(d) {
            high = d;
        } else {
            low = c;
        }
        c = high - ratio * (high - low);
        d = low + ratio * (high - low);
        if (high - low).abs() < 1e-10 {
            break;
        }
    }
    Ok(0.5 * (low + high))
}

/// Confidence interval of the Box–Cox `lambda` by the likelihood-ratio
/// criterion.
///
/// The interval holds every `lambda` with `2·(ℓ(λ̂) − ℓ(λ)) ≤ χ²_{1,α}`, found
/// by scanning away from the maximum and refining with bisection. The search
/// stops at `±5`, where the transformation is already far from any sensible
/// scale; a bound that is never crossed is reported as `NaN`.
pub fn lambda_interval(sample: &Sample, confidence: f64) -> Result<(f64, f64)> {
    if !(0.0..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence",
            value: confidence,
        });
    }
    let estimate = lambda(sample)?;
    let values = sample.values();
    let sum_logs: f64 = values.iter().map(|value| value.ln()).sum();
    let profile = |lambda: f64| profile_log_likelihood(values, sum_logs, lambda);
    let maximum = profile(estimate);
    if !maximum.is_finite() {
        return Err(Error::NotConverged {
            detail: "Box-Cox profile likelihood".to_owned(),
        });
    }
    let threshold = crate::special::chi_square_quantile(confidence, 1.0);
    let target = maximum - 0.5 * threshold;
    let bound = |direction: f64| -> f64 {
        let step = 0.05 * direction;
        let mut inside = estimate;
        let mut found = false;
        for _ in 0..200 {
            let candidate = inside + step;
            if !(-5.0..=5.0).contains(&candidate) {
                break;
            }
            if profile(candidate) < target {
                found = true;
                break;
            }
            inside = candidate;
        }
        if !found {
            return f64::NAN;
        }
        let mut low = inside;
        let mut high = inside + step;
        for _ in 0..200 {
            let middle = 0.5 * (low + high);
            if profile(middle) >= target {
                low = middle;
            } else {
                high = middle;
            }
            if (high - low).abs() < 1e-8 {
                break;
            }
        }
        0.5 * (low + high)
    };
    let lower = bound(-1.0);
    let upper = bound(1.0);
    Ok((lower, upper))
}

/// Result of the Box–Cox normal identification used by the distribution
/// comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct Identification {
    /// Profile-likelihood Box–Cox lambda.
    pub lambda: f64,
    /// Mean of the transformed data.
    pub mu: f64,
    /// Standard deviation (`n−1`) of the transformed data.
    pub sigma: f64,
    /// Anderson–Darling statistic of the transformed data.
    pub a2: f64,
    /// Composite-normal p-value of the Anderson–Darling statistic.
    pub p_value: f64,
    /// Log-likelihood on the original-data scale (Jacobian included).
    pub log_likelihood: f64,
    /// Number of estimated parameters (lambda, mean, sigma).
    pub n_params: usize,
}

/// Fits the Box–Cox normal candidate for the distribution comparison.
///
/// # Errors
///
/// Returns [`Error::NonPositive`] when the sample has a non-positive value and
/// [`Error::SampleTooSmall`] when there are fewer than three observations.
pub fn identify_normal(sample: &Sample) -> Result<Identification> {
    let lambda = lambda(sample)?;
    let transformed_values: Vec<f64> = sample
        .values()
        .iter()
        .map(|&value| transform(value, lambda))
        .collect::<Result<Vec<f64>>>()?;
    let transformed = Sample::new(transformed_values, MissingPolicy::Drop)?;
    let normal = Normal::fit_moments(&transformed)?;
    let a2 = anderson_darling(&normal, &transformed)?;
    let p_value = ad_p_value(transformed.n(), a2)?;
    let jacobian: f64 = sample
        .values()
        .iter()
        .map(|&value| (lambda - 1.0) * value.ln())
        .sum();
    let log_likelihood = transformed
        .values()
        .iter()
        .map(|&value| normal.ln_pdf(value))
        .sum::<f64>()
        + jacobian;
    Ok(Identification {
        lambda,
        mu: normal.mu(),
        sigma: normal.sigma(),
        a2,
        p_value,
        log_likelihood,
        n_params: 3,
    })
}

/// Runs the Box–Cox analysis: the transformation, the comparison of the
/// normality before and after, and the probability plot of the transformed data.
pub fn analyze(df: &DataFrame, column_name: &str) -> Result<crate::workflow::AnalysisOutput> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let lambda = lambda(&sample)?;
    let transformed_values: Vec<f64> = sample
        .values()
        .iter()
        .map(|&value| transform(value, lambda))
        .collect::<Result<Vec<f64>>>()?;
    let transformed = Sample::new(transformed_values, MissingPolicy::Drop)?;

    let before = anderson_darling(&crate::dists::Normal::fit_moments(&sample)?, &sample)?;
    let after = anderson_darling(
        &crate::dists::Normal::fit_moments(&transformed)?,
        &transformed,
    )?;
    let p_before = crate::gof::ad_p_value(sample.n(), before)?;
    let p_after = crate::gof::ad_p_value(transformed.n(), after)?;

    let mut result = AnalysisResult::new(
        format!("Box-Cox Transformation — column {column_name}"),
        "Profile-likelihood transformation followed by a normality check".to_owned(),
    );
    result.n = sample.n();
    result.excluded = sample.excluded();
    let (ci_low, ci_high) = lambda_interval(&sample, 0.95)?;
    result.statistics = vec![
        ("Lambda", lambda),
        ("Lambda 95% CI lower", ci_low),
        ("Lambda 95% CI upper", ci_high),
        ("A2 before", before),
        ("p-value before", p_before),
        ("A2 after", after),
        ("p-value after", p_after),
    ];
    let mut rows = vec![vec![
        "Mean".to_owned(),
        format!("{:.6}", sample.mean()?),
        format!("{:.6}", transformed.mean()?),
    ]];
    rows.push(vec![
        "Standard deviation".to_owned(),
        format!("{:.6}", sample.sd()?),
        format!("{:.6}", transformed.sd()?),
    ]);
    rows.push(vec![
        "Skewness".to_owned(),
        format!("{:.6}", skewness(&sample)),
        format!("{:.6}", skewness(&transformed)),
    ]);
    rows.push(vec![
        "Excess kurtosis".to_owned(),
        format!("{:.6}", kurtosis(&sample)),
        format!("{:.6}", kurtosis(&transformed)),
    ]);
    result.extra.push(Item::Table {
        headers: vec![
            "Quantity".to_owned(),
            "Before".to_owned(),
            "After".to_owned(),
        ],
        rows,
    });
    result.notes = vec![
        (
            "Transformation".to_owned(),
            format!("x(λ) = (x^{lambda:.6} − 1)/{lambda:.6}; a lambda of zero is the logarithm"),
        ),
        (
            "Estimator".to_owned(),
            "the profile likelihood of the Normal model with the Jacobian term (λ − 1)·Σ ln x; \
             the 95% interval holds every lambda with 2·(ℓ(λ̂) − ℓ(λ)) ≤ χ²₁,₀.₉₅"
                .to_owned(),
        ),
        (
            "Normality".to_owned(),
            "the Anderson-Darling statistic and its p-value before and after the transformation"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "the transformation needs positive values; non-positive and non-finite observations \\
             are excluded and counted"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    if p_after <= p_before {
        result
            .warnings
            .push("the transformation does not improve the normality of the data".to_owned());
    }

    let dist = crate::dists::Normal::fit_moments(&transformed)?;
    let probability = normal_probability_plot(&transformed, &dist, PlottingPosition::Benard)?;
    let plot = PlotSpec::normal_probability(&probability, &dist, "Box-Cox Transformation");
    Ok(crate::workflow::AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

/// The skewness of a sample (the standardised third central moment).
fn skewness(sample: &Sample) -> f64 {
    let n = sample.n();
    if n < 3 {
        return f64::NAN;
    }
    let values = sample.values();
    let mean = values.iter().sum::<f64>() / n as f64;
    let mut m2 = 0.0;
    let mut m3 = 0.0;
    for value in values {
        let deviation = value - mean;
        m2 += deviation * deviation;
        m3 += deviation * deviation * deviation;
    }
    let m2 = m2 / n as f64;
    let m3 = m3 / n as f64;
    if m2 > 0.0 {
        m3 / m2.powf(1.5)
    } else {
        f64::NAN
    }
}

/// The excess kurtosis of a sample.
fn kurtosis(sample: &Sample) -> f64 {
    let n = sample.n();
    if n < 4 {
        return f64::NAN;
    }
    let values = sample.values();
    let mean = values.iter().sum::<f64>() / n as f64;
    let mut m2 = 0.0;
    let mut m4 = 0.0;
    for value in values {
        let deviation = value - mean;
        m2 += deviation * deviation;
        m4 += deviation.powi(4);
    }
    let m2 = m2 / n as f64;
    let m4 = m4 / n as f64;
    if m2 > 0.0 {
        m4 / (m2 * m2) - 3.0
    } else {
        f64::NAN
    }
}
