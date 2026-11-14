// SPDX-License-Identifier: MIT
//! Probability distributions.
//!
//! The contract is intentionally small: only what a probability plot and a fit
//! need. Methods that are not valid for a distribution (for example, a closed
//! quantile) do not enter the trait for convenience.

use crate::data::Sample;
use crate::error::{Error, Result};
use crate::special::{
    digamma, ln_gamma, normal_cdf, normal_pdf, normal_quantile, normal_sf, regularized_gamma_lower,
    regularized_gamma_upper, trigamma,
};

/// Contract common to all distributions.
///
/// Implementations return `NaN`/`±inf` only outside the valid parameter domain
/// (which the constructor already rejects); in the data limit cases, the
/// caller validates beforehand.
pub trait Distribution: std::fmt::Debug {
    /// Stable name for reports.
    fn name(&self) -> &'static str;

    /// Density.
    fn pdf(&self, x: f64) -> f64;

    /// Cumulative distribution function.
    fn cdf(&self, x: f64) -> f64;

    /// Upper tail, `1 − F(x)`, computed without loss of precision.
    fn sf(&self, x: f64) -> f64 {
        1.0 - self.cdf(x)
    }

    /// Logarithm of the density.
    fn ln_pdf(&self, x: f64) -> f64;

    /// Quantile (inverse of the CDF).
    fn quantile(&self, p: f64) -> f64;

    /// Number of estimated parameters.
    fn n_params(&self) -> usize;

    /// Estimated parameters, in presentation order.
    fn parameters(&self) -> Vec<(&'static str, f64)>;

    /// Rebuilds the distribution from the values of `parameters`.
    ///
    /// The values must be in the order of [`Self::parameters`]; the method
    /// returns `None` when they are outside the domain. It is used to
    /// evaluate the likelihood around a fit.
    fn from_parameters(parameters: &[f64]) -> Option<Self>
    where
        Self: Sized;
}

/// Normal distribution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Normal {
    mu: f64,
    sigma: f64,
}

impl Normal {
    /// Creates a Normal with mean `mu` and standard deviation `sigma > 0`.
    pub fn new(mu: f64, sigma: f64) -> Result<Self> {
        if !mu.is_finite() {
            return Err(Error::InvalidParameter {
                name: "mu",
                value: mu,
            });
        }
        if !sigma.is_finite() || sigma <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "sigma",
                value: sigma,
            });
        }
        Ok(Self { mu, sigma })
    }

    /// Mean.
    #[must_use]
    pub fn mu(&self) -> f64 {
        self.mu
    }

    /// Standard deviation.
    #[must_use]
    pub fn sigma(&self) -> f64 {
        self.sigma
    }

    /// Maximum likelihood estimate from a sample.
    ///
    /// For the Normal the estimators are closed-form: `µ̂ = x̄` and
    /// `σ̂ = sqrt(Σ(xᵢ − x̄)² / n)`. The divisor is `n` (MLE), not `n − 1`.
    pub fn fit(sample: &Sample) -> Result<Self> {
        if sample.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 2,
            });
        }
        let mu = sample.mean()?;
        let variance = sample.variance_population()?;
        if !variance.is_finite() || variance <= 0.0 {
            return Err(Error::ConstantSample);
        }
        Self::new(mu, variance.sqrt())
    }

    /// Sample-moment estimate: `µ̂ = x̄`, `σ̂ = s` with divisor `n − 1`.
    ///
    /// This is the estimator the documented behaviour's probability-plot path passes to the
    /// numerical library (the `n − 1` standard deviation), so the fitted line,
    /// the AD statistic and the confidence band all use the same numbers.
    /// [`Self::fit`] remains the MLE used elsewhere in the engine.
    pub fn fit_moments(sample: &Sample) -> Result<Self> {
        if sample.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 2,
            });
        }
        let mu = sample.mean()?;
        let sd = sample.sd()?;
        if !sd.is_finite() || sd <= 0.0 {
            return Err(Error::ConstantSample);
        }
        Self::new(mu, sd)
    }

    /// Log-likelihood of a sample.
    pub fn log_likelihood(&self, sample: &Sample) -> Result<f64> {
        if sample.n() == 0 {
            return Err(Error::EmptySample);
        }
        Ok(sample.values().iter().map(|&x| self.ln_pdf(x)).sum())
    }
}

impl Distribution for Normal {
    fn name(&self) -> &'static str {
        "Normal"
    }

    fn pdf(&self, x: f64) -> f64 {
        normal_pdf((x - self.mu) / self.sigma) / self.sigma
    }

    fn cdf(&self, x: f64) -> f64 {
        normal_cdf((x - self.mu) / self.sigma)
    }

    fn sf(&self, x: f64) -> f64 {
        normal_sf((x - self.mu) / self.sigma)
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        let z = (x - self.mu) / self.sigma;
        -0.5 * z * z - self.sigma.ln() - 0.5 * (2.0 * std::f64::consts::PI).ln()
    }

    fn quantile(&self, p: f64) -> f64 {
        self.mu + self.sigma * normal_quantile(p)
    }

    fn n_params(&self) -> usize {
        2
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![
            ("Mean (µ)", self.mu),
            ("Standard deviation (σ)", self.sigma),
        ]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (mu, sigma) = match parameters {
            [mu, sigma] => (*mu, *sigma),
            _ => return None,
        };
        Self::new(mu, sigma).ok()
    }
}

/// Two-parameter log-normal distribution.
///
/// `ln X ~ Normal(µ, σ)`. The parameters are on the **log scale**, as in
/// the documented behaviour's distribution fit; the probability plot uses `ln x` and labels the
/// axis in original units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lognormal {
    mu: f64,
    sigma: f64,
}

impl Lognormal {
    /// Creates a log-normal with log-scale location and scale.
    pub fn new(mu: f64, sigma: f64) -> Result<Self> {
        if !mu.is_finite() || !sigma.is_finite() || sigma <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "sigma",
                value: sigma,
            });
        }
        Ok(Self { mu, sigma })
    }

    /// Log-scale location, `µ = mean(ln x)`.
    #[must_use]
    pub fn mu(&self) -> f64 {
        self.mu
    }

    /// Log-scale scale, `σ = sd(ln x)` with divisor `n − 1`.
    #[must_use]
    pub fn sigma(&self) -> f64 {
        self.sigma
    }

    /// Maximum-likelihood fit on `ln x` (divisor `n`), for model comparison.
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        let mut logs = Vec::with_capacity(sample.n());
        for (index, &x) in sample.values().iter().enumerate() {
            if x <= 0.0 {
                return Err(Error::NonPositive { index, value: x });
            }
            logs.push(x.ln());
        }
        let log_sample = Sample::new(logs, crate::data::MissingPolicy::Drop)?;
        let mu = log_sample.mean()?;
        let variance = log_sample.variance_population()?;
        if !variance.is_finite() || variance <= 0.0 {
            return Err(Error::ConstantSample);
        }
        Self::new(mu, variance.sqrt())
    }

    /// Sample-moment fit on `ln x` (the documented behaviour's log-normal plot path).
    pub fn fit_moments(sample: &Sample) -> Result<Self> {
        if sample.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 2,
            });
        }
        let mut logs = Vec::with_capacity(sample.n());
        for (index, &x) in sample.values().iter().enumerate() {
            if x <= 0.0 {
                return Err(Error::NonPositive { index, value: x });
            }
            logs.push(x.ln());
        }
        let log_sample = Sample::new(logs, crate::data::MissingPolicy::Drop)?;
        let mu = log_sample.mean()?;
        let sd = log_sample.sd()?;
        if !sd.is_finite() || sd <= 0.0 {
            return Err(Error::ConstantSample);
        }
        Self::new(mu, sd)
    }
}

impl Distribution for Lognormal {
    fn name(&self) -> &'static str {
        "Lognormal"
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        let z = (x.ln() - self.mu) / self.sigma;
        normal_pdf(z) / (x * self.sigma)
    }

    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        normal_cdf((x.ln() - self.mu) / self.sigma)
    }

    fn sf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 1.0;
        }
        normal_sf((x.ln() - self.mu) / self.sigma)
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return f64::NEG_INFINITY;
        }
        let z = (x.ln() - self.mu) / self.sigma;
        -0.5 * z * z - x.ln() - self.sigma.ln() - 0.5 * (2.0 * std::f64::consts::PI).ln()
    }

    fn quantile(&self, p: f64) -> f64 {
        (self.mu + self.sigma * normal_quantile(p)).exp()
    }

    fn n_params(&self) -> usize {
        2
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![("Location (µ)", self.mu), ("Scale (σ)", self.sigma)]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (mu, sigma) = match parameters {
            [mu, sigma] => (*mu, *sigma),
            _ => return None,
        };
        Self::new(mu, sigma).ok()
    }
}

/// Two-parameter Weibull distribution.
///
/// `F(x) = 1 − exp(−(x/β)^α)` for `x ≥ 0`. Parameters are shape `α` and scale
/// `β`, the same order returned by the documented fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weibull {
    shape: f64,
    scale: f64,
}

impl Weibull {
    /// Creates a Weibull with positive shape and scale.
    pub fn new(shape: f64, scale: f64) -> Result<Self> {
        if !shape.is_finite() || shape <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "shape",
                value: shape,
            });
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "scale",
                value: scale,
            });
        }
        Ok(Self { shape, scale })
    }

    /// Shape parameter `α`.
    #[must_use]
    pub fn shape(&self) -> f64 {
        self.shape
    }

    /// Scale parameter `β`.
    #[must_use]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Profile-likelihood fit, following the documented fit.
    ///
    /// The recovered iteration centers the logarithms by their mean
    /// (`y = ln x − mean(ln x)`), then uses
    /// `Δ = Σ(1 − αy)e^{αy} / Σ αy²e^{αy}`. The scale is profiled out as
    /// `β = (mean(x^α))^(1/α)`. Starts at
    /// `max(ln(n−1), 2)/max(y)`, at most 50 iterations, `|Δ| < 1e-12`.
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        if sample.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 2,
            });
        }
        let mut xs = Vec::with_capacity(sample.n());
        let mut ys = Vec::with_capacity(sample.n());
        for (index, &x) in sample.values().iter().enumerate() {
            if x <= 0.0 {
                return Err(Error::NonPositive { index, value: x });
            }
            xs.push(x);
            ys.push(x.ln());
        }
        // The recovered fit centers the logarithms before iterating
        // (`FUN_18001f920`); this makes the profile score
        // 1/α − weighted_mean(y) = 0.
        let mean_y = ys.iter().sum::<f64>() / sample.n() as f64;
        for y in &mut ys {
            *y -= mean_y;
        }
        let max_y = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let max_x = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if max_y <= 1e-15 {
            return Err(Error::ConstantSample);
        }
        let mut shape = (sample.n() as f64 - 1.0).ln().max(2.0) / max_y;
        if !shape.is_finite() || shape <= 0.0 {
            shape = 1.0;
        }

        let mut converged = false;
        for _ in 0..50 {
            let mut numerator = 0.0;
            let mut denominator = 0.0;
            for &y in &ys {
                let z = (shape * (y - max_y)).exp();
                numerator += (1.0 - shape * y) * z;
                denominator += shape * y * y * z;
            }
            let delta = numerator / denominator;
            shape += delta;
            if !shape.is_finite() || shape <= 0.0 {
                break;
            }
            if delta.abs() < 1e-12 {
                converged = true;
                break;
            }
        }
        if !converged {
            return Err(Error::NotConverged {
                detail: "Weibull shape Newton iteration".to_owned(),
            });
        }

        let mean_pow = xs.iter().map(|x| (x / max_x).powf(shape)).sum::<f64>() / sample.n() as f64;
        let scale = max_x * mean_pow.powf(1.0 / shape);
        Self::new(shape, scale)
    }
}

impl Distribution for Weibull {
    fn name(&self) -> &'static str {
        "Weibull"
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        let z = (x / self.scale).powf(self.shape);
        (self.shape / self.scale) * (x / self.scale).powf(self.shape - 1.0) * (-z).exp()
    }

    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        -(-(x / self.scale).powf(self.shape)).exp_m1()
    }

    fn sf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 1.0;
        }
        (-(x / self.scale).powf(self.shape)).exp()
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return f64::NEG_INFINITY;
        }
        let z = (x / self.scale).powf(self.shape);
        self.shape.ln() - self.scale.ln() + (self.shape - 1.0) * (x / self.scale).ln() - z
    }

    fn quantile(&self, p: f64) -> f64 {
        if p <= 0.0 {
            return 0.0;
        }
        if p >= 1.0 {
            return f64::INFINITY;
        }
        self.scale * (-(1.0 - p).ln()).powf(1.0 / self.shape)
    }

    fn n_params(&self) -> usize {
        2
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![("Shape (α)", self.shape), ("Scale (β)", self.scale)]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (shape, scale) = match parameters {
            [shape, scale] => (*shape, *scale),
            _ => return None,
        };
        Self::new(shape, scale).ok()
    }
}

/// One-parameter exponential distribution.
///
/// `F(x) = 1 − exp(−x/θ)` for `x ≥ 0`; `θ` is both the mean and the scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Exponential {
    theta: f64,
}

impl Exponential {
    /// Creates an exponential with positive mean `θ`.
    pub fn new(theta: f64) -> Result<Self> {
        if !theta.is_finite() || theta <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "theta",
                value: theta,
            });
        }
        Ok(Self { theta })
    }

    /// Mean/scale `θ`.
    #[must_use]
    pub fn theta(&self) -> f64 {
        self.theta
    }

    /// Maximum-likelihood fit: `θ = mean(x)`, exactly as the documented exponential fit.
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        if sample.n() == 0 {
            return Err(Error::EmptySample);
        }
        for (index, &x) in sample.values().iter().enumerate() {
            if x <= 0.0 {
                return Err(Error::NonPositive { index, value: x });
            }
        }
        Self::new(sample.mean()?)
    }
}

impl Distribution for Exponential {
    fn name(&self) -> &'static str {
        "Exponential"
    }

    fn pdf(&self, x: f64) -> f64 {
        if x < 0.0 {
            return 0.0;
        }
        (-x / self.theta).exp() / self.theta
    }

    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        -(-x / self.theta).exp_m1()
    }

    fn sf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 1.0;
        }
        (-x / self.theta).exp()
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        if x < 0.0 {
            return f64::NEG_INFINITY;
        }
        -self.theta.ln() - x / self.theta
    }

    fn quantile(&self, p: f64) -> f64 {
        if p <= 0.0 {
            return 0.0;
        }
        if p >= 1.0 {
            return f64::INFINITY;
        }
        -self.theta * (1.0 - p).ln()
    }

    fn n_params(&self) -> usize {
        1
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![("Mean (θ)", self.theta)]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let [theta] = parameters else {
            return None;
        };
        Self::new(*theta).ok()
    }
}

/// Two-parameter logistic distribution.
///
/// `F(x) = 1 / (1 + exp(−(x − µ)/σ))`. The location `µ` is also the mean, and
/// `σ` is the scale (`σ·π/√3` is the standard deviation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Logistic {
    mu: f64,
    sigma: f64,
}

impl Logistic {
    /// Creates a logistic with finite location and positive scale.
    pub fn new(mu: f64, sigma: f64) -> Result<Self> {
        if !mu.is_finite() {
            return Err(Error::InvalidParameter {
                name: "mu",
                value: mu,
            });
        }
        if !sigma.is_finite() || sigma <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "sigma",
                value: sigma,
            });
        }
        Ok(Self { mu, sigma })
    }

    /// Location parameter `µ`.
    #[must_use]
    pub fn mu(&self) -> f64 {
        self.mu
    }

    /// Scale parameter `σ`.
    #[must_use]
    pub fn sigma(&self) -> f64 {
        self.sigma
    }

    /// Maximum likelihood fit by coordinate ascent.
    ///
    /// Starting from the sample mean and the method-of-moments scale
    /// `s·√3/π`, the two score equations are solved alternately by bisection:
    ///
    /// `Σ(2F(zᵢ) − 1) = 0` for `µ`, and
    /// `Σ((2F(zᵢ) − 1)·zᵢ) − n = 0` for `σ`.
    ///
    /// Both score functions decrease monotonically in their parameter, so the
    /// brackets are deterministic and no step-size tuning is involved.
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        if sample.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 2,
            });
        }
        let values = sample.values();
        let mean = sample.mean()?;
        let sd = sample.sd()?;
        if !sd.is_finite() || sd <= 0.0 {
            return Err(Error::ConstantSample);
        }
        let mut mu = mean;
        let mut sigma = sd * 3f64.sqrt() / std::f64::consts::PI;
        let x_min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let x_max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let span = (x_max - x_min).max(sd).max(1e-12);
        let mut converged = false;
        for _ in 0..200 {
            let old_mu = mu;
            let old_sigma = sigma;

            // Score for µ: Σ(2F(z) − 1), decreasing in µ.
            let mut lo = x_min - 40.0 * sigma;
            let mut hi = x_max + 40.0 * sigma;
            for _ in 0..80 {
                let mid = 0.5 * (lo + hi);
                let score = values
                    .iter()
                    .map(|&x| 2.0 * logistic_cdf((x - mid) / sigma) - 1.0)
                    .sum::<f64>();
                if score > 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            mu = 0.5 * (lo + hi);

            // Score for σ: Σ((2F(z) − 1)z) − n, decreasing in σ.
            let mut lo = span * 1e-12;
            let mut hi = span * 1e12;
            for _ in 0..80 {
                let mid = 0.5 * (lo + hi);
                let score = values
                    .iter()
                    .map(|&x| {
                        let z = (x - mu) / mid;
                        2.0 * logistic_cdf(z) - 1.0
                    })
                    .zip(values.iter())
                    .map(|(centered, &x)| centered * ((x - mu) / mid))
                    .sum::<f64>()
                    - values.len() as f64;
                if score > 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            sigma = 0.5 * (lo + hi);
            if !sigma.is_finite() || sigma <= 0.0 {
                return Err(Error::NotConverged {
                    detail: "logistic scale became non-positive".to_owned(),
                });
            }
            if (mu - old_mu).abs() <= 1e-10 * (1.0 + mu.abs())
                && (sigma - old_sigma).abs() <= 1e-10 * (1.0 + sigma.abs())
            {
                converged = true;
                break;
            }
        }
        if !converged {
            return Err(Error::NotConverged {
                detail: "logistic maximum-likelihood coordinate ascent".to_owned(),
            });
        }
        Self::new(mu, sigma)
    }

    /// Log-likelihood of a sample.
    pub fn log_likelihood(&self, sample: &Sample) -> Result<f64> {
        if sample.n() == 0 {
            return Err(Error::EmptySample);
        }
        Ok(sample.values().iter().map(|&x| self.ln_pdf(x)).sum())
    }
}

fn logistic_cdf(z: f64) -> f64 {
    if z >= 0.0 {
        1.0 / (1.0 + (-z).exp())
    } else {
        let e = z.exp();
        e / (1.0 + e)
    }
}

fn logistic_ln_pdf_z(z: f64) -> f64 {
    if z >= 0.0 {
        -z - 2.0 * (-z).exp().ln_1p()
    } else {
        z - 2.0 * z.exp().ln_1p()
    }
}

impl Distribution for Logistic {
    fn name(&self) -> &'static str {
        "Logistic"
    }

    fn pdf(&self, x: f64) -> f64 {
        let z = (x - self.mu) / self.sigma;
        let cdf = logistic_cdf(z);
        cdf * (1.0 - cdf) / self.sigma
    }

    fn cdf(&self, x: f64) -> f64 {
        logistic_cdf((x - self.mu) / self.sigma)
    }

    fn sf(&self, x: f64) -> f64 {
        logistic_cdf(-(x - self.mu) / self.sigma)
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        logistic_ln_pdf_z((x - self.mu) / self.sigma) - self.sigma.ln()
    }

    fn quantile(&self, p: f64) -> f64 {
        self.mu + self.sigma * (p / (1.0 - p)).ln()
    }

    fn n_params(&self) -> usize {
        2
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![("Location (µ)", self.mu), ("Scale (σ)", self.sigma)]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (mu, sigma) = match parameters {
            [mu, sigma] => (*mu, *sigma),
            _ => return None,
        };
        Self::new(mu, sigma).ok()
    }
}

/// Two-parameter log-logistic distribution.
///
/// `ln X` is logistic: `F(x) = 1 / (1 + (x/α)^(−β))` for `x > 0`. `α` is the
/// scale (the median) and `β` the shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loglogistic {
    scale: f64,
    shape: f64,
}

impl Loglogistic {
    /// Creates a log-logistic with positive scale and shape.
    pub fn new(scale: f64, shape: f64) -> Result<Self> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "scale",
                value: scale,
            });
        }
        if !shape.is_finite() || shape <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "shape",
                value: shape,
            });
        }
        Ok(Self { scale, shape })
    }

    /// Scale parameter `α` (median).
    #[must_use]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Shape parameter `β`.
    #[must_use]
    pub fn shape(&self) -> f64 {
        self.shape
    }

    /// Log-scale location, `ln α`.
    #[must_use]
    pub fn mu(&self) -> f64 {
        self.scale.ln()
    }

    /// Log-scale scale, `1/β`.
    #[must_use]
    pub fn sigma(&self) -> f64 {
        1.0 / self.shape
    }

    /// Maximum likelihood fit on `ln x`.
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        if sample.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 2,
            });
        }
        let mut logs = Vec::with_capacity(sample.n());
        for (index, &x) in sample.values().iter().enumerate() {
            if x <= 0.0 {
                return Err(Error::NonPositive { index, value: x });
            }
            logs.push(x.ln());
        }
        let log_sample = Sample::new(logs, crate::data::MissingPolicy::Drop)?;
        let fitted = Logistic::fit_mle(&log_sample)?;
        Self::new(fitted.mu().exp(), 1.0 / fitted.sigma())
    }
}

impl Distribution for Loglogistic {
    fn name(&self) -> &'static str {
        "Loglogistic"
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        let z = self.shape * (x / self.scale).ln();
        let cdf = logistic_cdf(z);
        // β/(α) · (x/α)^(β−1) / (1 + (x/α)^β)^2
        cdf * (1.0 - cdf) * self.shape / x
    }

    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        logistic_cdf(self.shape * (x / self.scale).ln())
    }

    fn sf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 1.0;
        }
        logistic_cdf(-(self.shape * (x / self.scale).ln()))
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return f64::NEG_INFINITY;
        }
        let z = self.shape * (x / self.scale).ln();
        self.shape.ln() - self.scale.ln() + ((self.shape - 1.0) / self.shape) * z
            - 2.0
                * if z >= 0.0 {
                    z + (-z).exp().ln_1p()
                } else {
                    z.exp().ln_1p()
                }
    }

    fn quantile(&self, p: f64) -> f64 {
        self.scale * (p / (1.0 - p)).powf(1.0 / self.shape)
    }

    fn n_params(&self) -> usize {
        2
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![("Scale (α)", self.scale), ("Shape (β)", self.shape)]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (scale, shape) = match parameters {
            [scale, shape] => (*scale, *shape),
            _ => return None,
        };
        Self::new(scale, shape).ok()
    }
}

/// Two-parameter gamma distribution.
///
/// `F(x) = P(α, x/β)` for `x > 0`, with shape `α` and scale `β`; the mean is
/// `αβ` and the variance `αβ²`. The documented behaviour has no gamma
/// probability plot, so this family is an addition of this engine rather than
/// a recovered one — see the project notes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gamma {
    shape: f64,
    scale: f64,
}

impl Gamma {
    /// Creates a Gamma with positive shape and scale.
    pub fn new(shape: f64, scale: f64) -> Result<Self> {
        if !shape.is_finite() || shape <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "shape",
                value: shape,
            });
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "scale",
                value: scale,
            });
        }
        Ok(Self { shape, scale })
    }

    /// Shape parameter `α`.
    #[must_use]
    pub fn shape(&self) -> f64 {
        self.shape
    }

    /// Scale parameter `β`.
    #[must_use]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Mean, `αβ`.
    #[must_use]
    pub fn mean(&self) -> f64 {
        self.shape * self.scale
    }

    /// Maximum-likelihood fit.
    ///
    /// With `s = ln(x̄) − mean(ln x)` (always `≥ 0`), the shape solves
    /// `ln α − ψ(α) = s`, started from Minka's approximation
    /// `(3 − s + √((s−3)² + 24s)) / (12s)` and iterated with Newton's method
    /// using `f' = 1/α − ψ'(α)`. The scale is profiled out as `β = x̄/α`.
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        if sample.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 2,
            });
        }
        let mut sum_ln = 0.0;
        for (index, &x) in sample.values().iter().enumerate() {
            if !x.is_finite() || x <= 0.0 {
                return Err(Error::NonPositive { index, value: x });
            }
            sum_ln += x.ln();
        }
        let n = sample.n() as f64;
        let mean = sample.mean()?;
        let s = mean.ln() - sum_ln / n;
        if !s.is_finite() || s <= 0.0 {
            // Exactly zero only when every observation is identical.
            return Err(Error::ConstantSample);
        }

        let mut shape = (3.0 - s + ((s - 3.0) * (s - 3.0) + 24.0 * s).sqrt()) / (12.0 * s);
        if !shape.is_finite() || shape <= 0.0 {
            shape = 1.0;
        }
        let mut converged = false;
        for _ in 0..200 {
            let score = shape.ln() - digamma(shape) - s;
            let slope = 1.0 / shape - trigamma(shape);
            if !slope.is_finite() || slope == 0.0 {
                break;
            }
            let mut next = shape - score / slope;
            if !next.is_finite() || next <= 0.0 {
                next = 0.5 * shape;
            }
            if (next - shape).abs() <= 1e-12 * (1.0 + next.abs()) {
                shape = next;
                converged = true;
                break;
            }
            shape = next;
        }
        if !converged {
            return Err(Error::NotConverged {
                detail: "gamma shape Newton iteration".to_owned(),
            });
        }
        if shape > 1.0e5 {
            return Err(Error::NotConverged {
                detail: "gamma shape is numerically degenerate (near-constant sample)".to_owned(),
            });
        }
        Self::new(shape, mean / shape)
    }

    /// Log-likelihood of a sample.
    pub fn log_likelihood(&self, sample: &Sample) -> Result<f64> {
        if sample.n() == 0 {
            return Err(Error::EmptySample);
        }
        Ok(sample.values().iter().map(|&x| self.ln_pdf(x)).sum())
    }

    /// Standardized quantile `u` with `P(α, u) = p`.
    ///
    /// Safeguarded Newton on `v = ln u` (relative precision even when the
    /// quantile is tiny), bracketed by `P(α, eᵛ) = p`, started from the
    /// Wilson–Hilferty normal approximation.
    fn standard_quantile(alpha: f64, p: f64) -> f64 {
        let z = normal_quantile(p);
        let base = 1.0 - 1.0 / (9.0 * alpha) + z / (3.0 * alpha.sqrt());
        let guess = if base > 0.0 {
            (alpha * base * base * base).max(f64::MIN_POSITIVE).ln()
        } else {
            (0.5 / alpha).ln()
        };

        let mut lo = f64::MIN_POSITIVE.ln();
        let mut hi = (guess + 1.0).max(lo + 1.0);
        let mut guard = 0;
        while regularized_gamma_lower(alpha, hi.exp()) < p && guard < 200 {
            hi += 2.0;
            guard += 1;
        }
        let mut v = guess.clamp(lo + 1.0, hi - 1.0);
        for _ in 0..200 {
            let u = v.exp();
            let cdf = regularized_gamma_lower(alpha, u);
            if cdf < p {
                lo = v;
            } else {
                hi = v;
            }
            if hi - lo <= 1e-13 {
                break;
            }
            // d/dv P(α, eᵛ) = pdf(eᵛ)·eᵛ.
            let derivative = ((alpha - 1.0) * v - u - ln_gamma(alpha)).exp() * u;
            let mut stepped = false;
            if derivative.is_finite() && derivative > 0.0 {
                let next = v - (cdf - p) / derivative;
                if next > lo && next < hi {
                    v = next;
                    stepped = true;
                }
            }
            if !stepped {
                v = 0.5 * (lo + hi);
            }
        }
        0.5 * (lo + hi)
    }
}

impl Distribution for Gamma {
    fn name(&self) -> &'static str {
        "Gamma"
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        self.ln_pdf(x).exp()
    }

    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        regularized_gamma_lower(self.shape, x / self.scale)
    }

    fn sf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 1.0;
        }
        regularized_gamma_upper(self.shape, x / self.scale)
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return f64::NEG_INFINITY;
        }
        (self.shape - 1.0) * x.ln()
            - x / self.scale
            - ln_gamma(self.shape)
            - self.shape * self.scale.ln()
    }

    fn quantile(&self, p: f64) -> f64 {
        if p <= 0.0 {
            return 0.0;
        }
        if p >= 1.0 {
            return f64::INFINITY;
        }
        self.scale * Self::standard_quantile(self.shape, p).exp()
    }

    fn n_params(&self) -> usize {
        2
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![("Shape (α)", self.shape), ("Scale (β)", self.scale)]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (shape, scale) = match parameters {
            [shape, scale] => (*shape, *scale),
            _ => return None,
        };
        Self::new(shape, scale).ok()
    }
}

/// Two-parameter extreme-value distribution (type I Gumbel, maximum domain).
///
/// `F(x) = exp(−exp(−(x − µ)/σ))`. The documented behaviour has no extreme-value
/// probability plot, so this family is an addition of this engine rather than
/// a recovered one — see the project notes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtremeValue {
    location: f64,
    scale: f64,
}

impl ExtremeValue {
    /// Creates an extreme-value distribution with any location and positive scale.
    pub fn new(location: f64, scale: f64) -> Result<Self> {
        if !location.is_finite() {
            return Err(Error::InvalidParameter {
                name: "location",
                value: location,
            });
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "scale",
                value: scale,
            });
        }
        Ok(Self { location, scale })
    }

    /// Location parameter `µ`.
    #[must_use]
    pub fn location(&self) -> f64 {
        self.location
    }

    /// Scale parameter `σ`.
    #[must_use]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Maximum-likelihood fit.
    ///
    /// The two score equations are
    /// `mean(exp(−(x − µ)/σ)) = 1` and `σ = x̄ − mean(x·exp(−(x − µ)/σ))`.
    /// The first one is solved for `µ` by bisection (it decreases
    /// monotonically in `µ`) and the second one updates `σ`; the pair is
    /// iterated from the moment start (`σ = s·√6/π`, `µ = x̄ + γσ`).
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        if sample.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 2,
            });
        }
        let values = sample.values();
        let n = sample.n() as f64;
        let mean = sample.mean()?;
        let sd = sample.sd()?;
        if !sd.is_finite() || sd <= 0.0 {
            return Err(Error::ConstantSample);
        }
        let x_min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let x_max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        let mut scale = sd * (6.0f64).sqrt() / std::f64::consts::PI;
        let mut location = mean + std::f64::consts::EULER_GAMMA * scale;
        let mut converged = false;
        for _ in 0..200 {
            let mut lo = x_min - 50.0 * scale;
            let mut hi = x_max + 50.0 * scale;
            for _ in 0..100 {
                let mid = 0.5 * (lo + hi);
                let t_mean = values
                    .iter()
                    .map(|&x| (-(x - mid) / scale).exp())
                    .sum::<f64>()
                    / n;
                if t_mean > 1.0 {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let new_location = 0.5 * (lo + hi);
            let weighted = values
                .iter()
                .map(|&x| {
                    let t = (-(x - new_location) / scale).exp();
                    x * t
                })
                .sum::<f64>();
            let new_scale = mean - weighted / n;
            if !new_scale.is_finite() || new_scale <= 0.0 {
                break;
            }
            let moved = (new_location - location).abs() <= 1e-12 * (1.0 + new_location.abs())
                && (new_scale - scale).abs() <= 1e-12 * (1.0 + new_scale.abs());
            location = new_location;
            scale = new_scale;
            if moved {
                converged = true;
                break;
            }
        }
        if !converged {
            // Accept the iterate when the score equations are satisfied even
            // though the fixed point did not settle.
            let t_mean = values
                .iter()
                .map(|&x| (-(x - location) / scale).exp())
                .sum::<f64>()
                / n;
            if (t_mean - 1.0).abs() > 1e-9 {
                return Err(Error::NotConverged {
                    detail: "extreme-value score equations".to_owned(),
                });
            }
        }
        Self::new(location, scale)
    }

    /// Log-likelihood of a sample.
    pub fn log_likelihood(&self, sample: &Sample) -> Result<f64> {
        if sample.n() == 0 {
            return Err(Error::EmptySample);
        }
        Ok(sample.values().iter().map(|&x| self.ln_pdf(x)).sum())
    }
}

impl Distribution for ExtremeValue {
    fn name(&self) -> &'static str {
        "Extreme Value"
    }

    fn pdf(&self, x: f64) -> f64 {
        let z = (x - self.location) / self.scale;
        let t = (-z).exp();
        // exp(−z − exp(−z)) / σ, the Gumbel (maximum) density.
        (-z - t).exp() / self.scale
    }

    fn cdf(&self, x: f64) -> f64 {
        let z = (x - self.location) / self.scale;
        (-(-z).exp()).exp()
    }

    fn sf(&self, x: f64) -> f64 {
        let z = (x - self.location) / self.scale;
        -(-(-z).exp()).exp_m1()
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        let z = (x - self.location) / self.scale;
        let t = (-z).exp();
        -self.scale.ln() - z - t
    }

    fn quantile(&self, p: f64) -> f64 {
        if p <= 0.0 {
            return f64::NEG_INFINITY;
        }
        if p >= 1.0 {
            return f64::INFINITY;
        }
        self.location - self.scale * (-p.ln()).ln()
    }

    fn n_params(&self) -> usize {
        2
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![("Location (µ)", self.location), ("Scale (σ)", self.scale)]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (mu, sigma) = match parameters {
            [mu, sigma] => (*mu, *sigma),
            _ => return None,
        };
        Self::new(mu, sigma).ok()
    }
}

/// Three-parameter log-normal distribution with a threshold.
///
/// `F(x) = Φ((ln(x − γ) − µ)/σ)` for `x > γ`. The threshold is estimated by
/// the probability-plot method and the location and scale follow by maximum
/// likelihood with that threshold fixed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lognormal3 {
    threshold: f64,
    mu: f64,
    sigma: f64,
}

impl Lognormal3 {
    /// Creates a three-parameter log-normal with a threshold below the support.
    pub fn new(threshold: f64, mu: f64, sigma: f64) -> Result<Self> {
        if !mu.is_finite() {
            return Err(Error::InvalidParameter {
                name: "location",
                value: mu,
            });
        }
        if !sigma.is_finite() || sigma <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "scale",
                value: sigma,
            });
        }
        if !threshold.is_finite() {
            return Err(Error::InvalidParameter {
                name: "threshold",
                value: threshold,
            });
        }
        Ok(Self {
            threshold,
            mu,
            sigma,
        })
    }

    /// Threshold `γ`: the smallest value the distribution supports.
    #[must_use]
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// Location parameter `µ` (on the log scale).
    #[must_use]
    pub fn mu(&self) -> f64 {
        self.mu
    }

    /// Scale parameter `σ` (on the log scale).
    #[must_use]
    pub fn sigma(&self) -> f64 {
        self.sigma
    }

    /// Fits a three-parameter log-normal.
    ///
    /// The threshold is chosen by the probability-plot method: the value of
    /// `γ` that makes the normal probability plot of `ln(x − γ)` straightest.
    /// The location and scale then follow by maximum likelihood with that
    /// threshold fixed.
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        if sample.n() < 3 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 3,
            });
        }
        let values = sample.values();
        for (index, &x) in values.iter().enumerate() {
            if !x.is_finite() || x <= 0.0 {
                return Err(Error::NonPositive { index, value: x });
            }
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let n = sorted.len();
        let q1 = quantile_of(&sorted, 0.25);
        let q3 = quantile_of(&sorted, 0.75);
        let mut spread = q3 - q1;
        if spread <= 0.0 {
            spread = sample.sd()?;
        }
        if !spread.is_finite() || spread <= 0.0 {
            return Err(Error::ConstantSample);
        }
        let minimum = sorted[0];
        let lower = minimum - 10.0 * spread;
        let margin = 1e-6 * spread;
        let upper = minimum - margin;

        // The normal probability scale of the sorted sample.
        let mut scores = Vec::with_capacity(n);
        for index in 0..n {
            let p = (index as f64 + 1.0 - 0.3) / (n as f64 + 0.4);
            scores.push(crate::special::normal_quantile(p));
        }

        let candidates = 200;
        let mut best_threshold = lower;
        let mut best_error = f64::INFINITY;
        for step in 0..candidates {
            let fraction = step as f64 / (candidates - 1) as f64;
            let threshold = lower + (upper - lower) * fraction;
            if threshold >= minimum {
                continue;
            }
            let shifted: Vec<f64> = sorted.iter().map(|x| (x - threshold).ln()).collect();
            let error = line_error(&shifted, &scores);
            if error < best_error {
                best_error = error;
                best_threshold = threshold;
            }
        }
        if !best_error.is_finite() {
            return Err(Error::NotConverged {
                detail: "three-parameter log-normal threshold search".to_owned(),
            });
        }

        let shifted: Vec<f64> = values.iter().map(|x| (x - best_threshold).ln()).collect();
        let mean = shifted.iter().sum::<f64>() / n as f64;
        let variance = shifted
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / n as f64;
        if variance <= 0.0 {
            return Err(Error::ConstantSample);
        }
        Self::new(best_threshold, mean, variance.sqrt())
    }
}

impl Distribution for Lognormal3 {
    fn name(&self) -> &'static str {
        "Lognormal (3-parameter)"
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 0.0;
        }
        let z = ((x - self.threshold).ln() - self.mu) / self.sigma;
        normal_pdf(z) / (self.sigma * (x - self.threshold))
    }

    fn cdf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 0.0;
        }
        let z = ((x - self.threshold).ln() - self.mu) / self.sigma;
        normal_cdf(z)
    }

    fn sf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 1.0;
        }
        let z = ((x - self.threshold).ln() - self.mu) / self.sigma;
        normal_sf(z)
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return f64::NEG_INFINITY;
        }
        let z = ((x - self.threshold).ln() - self.mu) / self.sigma;
        normal_pdf(z).ln() - self.sigma.ln() - (x - self.threshold).ln()
    }

    fn quantile(&self, p: f64) -> f64 {
        if p <= 0.0 {
            return self.threshold;
        }
        if p >= 1.0 {
            return f64::INFINITY;
        }
        self.threshold + (self.mu + self.sigma * crate::special::normal_quantile(p)).exp()
    }

    fn n_params(&self) -> usize {
        3
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![
            ("Threshold (γ)", self.threshold),
            ("Location (µ)", self.mu),
            ("Scale (σ)", self.sigma),
        ]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (threshold, mu, sigma) = match parameters {
            [threshold, mu, sigma] => (*threshold, *mu, *sigma),
            _ => return None,
        };
        Self::new(threshold, mu, sigma).ok()
    }
}

/// Three-parameter Weibull distribution with a threshold.
///
/// `F(x) = 1 − exp(−((x − γ)/β)^α)` for `x > γ`. The threshold `γ` is
/// estimated by the probability-plot method — the value that makes the
/// Weibull probability plot straightest — and the shape and scale follow by
/// maximum likelihood with that threshold fixed. The likelihood of a
/// three-parameter model is not bounded, so the threshold is not a plain
/// maximum-likelihood estimate; see the method documentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weibull3 {
    threshold: f64,
    shape: f64,
    scale: f64,
}

impl Weibull3 {
    /// Creates a three-parameter Weibull with a threshold below the support.
    pub fn new(threshold: f64, shape: f64, scale: f64) -> Result<Self> {
        if !shape.is_finite() || shape <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "shape",
                value: shape,
            });
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "scale",
                value: scale,
            });
        }
        if !threshold.is_finite() {
            return Err(Error::InvalidParameter {
                name: "threshold",
                value: threshold,
            });
        }
        Ok(Self {
            threshold,
            shape,
            scale,
        })
    }

    /// Threshold `γ`: the smallest value the distribution supports.
    #[must_use]
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// Shape parameter `α`.
    #[must_use]
    pub fn shape(&self) -> f64 {
        self.shape
    }

    /// Scale parameter `β`.
    #[must_use]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Fits a three-parameter Weibull.
    ///
    /// The threshold is chosen by the probability-plot method: for a
    /// candidate `γ` the observations are transformed with `ln(x − γ)` and the
    /// straight line that fits `ln(−ln(1 − p))` best is sought; the
    /// candidate with the smallest residual sum of squares wins. The
    /// candidates run from `min(x) − 10·spread` to just below `min(x)`, with
    /// `spread` the interquartile range (or the standard deviation when the
    /// interquartile range vanishes).
    ///
    /// The shape and scale are then estimated by maximum likelihood with that
    /// threshold fixed, using the same profile iteration as the two-parameter
    /// fit.
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        if sample.n() < 3 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 3,
            });
        }
        let values = sample.values();
        for (index, &x) in values.iter().enumerate() {
            if !x.is_finite() || x <= 0.0 {
                return Err(Error::NonPositive { index, value: x });
            }
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let n = sorted.len();
        let q1 = quantile_of(&sorted, 0.25);
        let q3 = quantile_of(&sorted, 0.75);
        let mut spread = q3 - q1;
        if spread <= 0.0 {
            spread = sample.sd()?;
        }
        if !spread.is_finite() || spread <= 0.0 {
            return Err(Error::ConstantSample);
        }
        let minimum = sorted[0];

        // Plotting positions (Benard) of the sorted sample.
        let mut positions = Vec::with_capacity(n);
        for index in 0..n {
            positions.push((index as f64 + 1.0 - 0.3) / (n as f64 + 0.4));
        }
        let reduced: Vec<f64> = positions.iter().map(|&p| (-(1.0 - p).ln()).ln()).collect();

        let lower = minimum - 10.0 * spread;
        let margin = 1e-6 * spread;
        let upper = minimum - margin;
        let candidates = 200;
        let mut best_threshold = lower;
        let mut best_error = f64::INFINITY;
        for step in 0..candidates {
            let fraction = step as f64 / (candidates - 1) as f64;
            let threshold = lower + (upper - lower) * fraction;
            if threshold >= minimum {
                continue;
            }
            let shifted: Vec<f64> = sorted.iter().map(|x| (x - threshold).ln()).collect();
            let error = line_error(&shifted, &reduced);
            if error < best_error {
                best_error = error;
                best_threshold = threshold;
            }
        }
        if !best_error.is_finite() {
            return Err(Error::NotConverged {
                detail: "three-parameter Weibull threshold search".to_owned(),
            });
        }

        // Shape and scale by maximum likelihood with the threshold fixed.
        let shifted: Vec<f64> = values.iter().map(|x| x - best_threshold).collect();
        let inner = Weibull::fit_mle(&Sample::new(shifted, crate::data::MissingPolicy::Drop)?)?;
        Self::new(best_threshold, inner.shape(), inner.scale())
    }
}

/// Quantile of a sorted sample by linear interpolation.
fn quantile_of(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    let h = (n as f64 - 1.0) * p;
    let low = h.floor() as usize;
    let high = h.ceil() as usize;
    if low == high {
        sorted[low]
    } else {
        sorted[low] + (h - low as f64) * (sorted[high] - sorted[low])
    }
}

/// Residual sum of squares of the straight line that fits `y` on `x`.
fn line_error(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len();
    let mean_x = x.iter().sum::<f64>() / n as f64;
    let mean_y = y.iter().sum::<f64>() / n as f64;
    let mut covariance = 0.0;
    let mut variance = 0.0;
    for (xi, yi) in x.iter().zip(y) {
        covariance += (xi - mean_x) * (yi - mean_y);
        variance += (xi - mean_x) * (xi - mean_x);
    }
    if variance <= 0.0 {
        return f64::INFINITY;
    }
    let slope = covariance / variance;
    let intercept = mean_y - slope * mean_x;
    x.iter()
        .zip(y)
        .map(|(xi, yi)| {
            let residual = yi - (intercept + slope * xi);
            residual * residual
        })
        .sum()
}

impl Distribution for Weibull3 {
    fn name(&self) -> &'static str {
        "Weibull (3-parameter)"
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 0.0;
        }
        let z = (x - self.threshold) / self.scale;
        (self.shape / self.scale) * z.powf(self.shape - 1.0) * (-z.powf(self.shape)).exp()
    }

    fn cdf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 0.0;
        }
        let z = (x - self.threshold) / self.scale;
        -(-z.powf(self.shape)).exp_m1()
    }

    fn sf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 1.0;
        }
        let z = (x - self.threshold) / self.scale;
        (-z.powf(self.shape)).exp()
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return f64::NEG_INFINITY;
        }
        let z = (x - self.threshold) / self.scale;
        self.shape.ln() - self.scale.ln() + (self.shape - 1.0) * z.ln() - z.powf(self.shape)
    }

    fn quantile(&self, p: f64) -> f64 {
        if p <= 0.0 {
            return self.threshold;
        }
        if p >= 1.0 {
            return f64::INFINITY;
        }
        self.threshold + self.scale * (-(1.0 - p).ln()).powf(1.0 / self.shape)
    }

    fn n_params(&self) -> usize {
        3
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![
            ("Threshold (γ)", self.threshold),
            ("Shape (α)", self.shape),
            ("Scale (β)", self.scale),
        ]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (threshold, shape, scale) = match parameters {
            [threshold, shape, scale] => (*threshold, *shape, *scale),
            _ => return None,
        };
        Self::new(threshold, shape, scale).ok()
    }
}

/// Two-parameter exponential distribution with a threshold.
///
/// `F(x) = 1 − exp(−(x − γ)/θ)` for `x ≥ γ`. The maximum-likelihood threshold
/// is the smallest observation (the likelihood increases as the threshold
/// approaches it from below), so the mean above the threshold is `x̄ − γ`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Exponential2 {
    threshold: f64,
    theta: f64,
}

impl Exponential2 {
    /// Creates a shifted exponential with a finite threshold and positive mean.
    pub fn new(threshold: f64, theta: f64) -> Result<Self> {
        if !threshold.is_finite() {
            return Err(Error::InvalidParameter {
                name: "threshold",
                value: threshold,
            });
        }
        if !theta.is_finite() || theta <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "theta",
                value: theta,
            });
        }
        Ok(Self { threshold, theta })
    }

    /// Threshold `γ`: the smallest value the distribution supports.
    #[must_use]
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// Mean above the threshold `θ`.
    #[must_use]
    pub fn theta(&self) -> f64 {
        self.theta
    }

    /// Maximum-likelihood fit with the threshold at the sample minimum.
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        if sample.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 2,
            });
        }
        let values = sample.values();
        let minimum = values.iter().copied().fold(f64::INFINITY, f64::min);
        let theta = sample.mean()? - minimum;
        if !theta.is_finite() || theta <= 0.0 {
            return Err(Error::ConstantSample);
        }
        Self::new(minimum, theta)
    }
}

impl Distribution for Exponential2 {
    fn name(&self) -> &'static str {
        "Exponential (2-parameter)"
    }

    fn pdf(&self, x: f64) -> f64 {
        if x < self.threshold {
            return 0.0;
        }
        (-(x - self.threshold) / self.theta).exp() / self.theta
    }

    fn cdf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 0.0;
        }
        -(-(x - self.threshold) / self.theta).exp_m1()
    }

    fn sf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 1.0;
        }
        (-(x - self.threshold) / self.theta).exp()
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        if x < self.threshold {
            return f64::NEG_INFINITY;
        }
        -self.theta.ln() - (x - self.threshold) / self.theta
    }

    fn quantile(&self, p: f64) -> f64 {
        if p <= 0.0 {
            return self.threshold;
        }
        if p >= 1.0 {
            return f64::INFINITY;
        }
        self.threshold - self.theta * (1.0 - p).ln()
    }

    fn n_params(&self) -> usize {
        2
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![("Threshold (γ)", self.threshold), ("Mean (θ)", self.theta)]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (threshold, theta) = match parameters {
            [threshold, theta] => (*threshold, *theta),
            _ => return None,
        };
        Self::new(threshold, theta).ok()
    }
}

/// Three-parameter gamma distribution with a threshold.
///
/// `F(x) = P(α, (x − γ)/β)` for `x > γ`. The threshold is chosen by profile
/// likelihood over a grid below the smallest observation; the shape and scale
/// follow by maximum likelihood with that threshold fixed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gamma3 {
    threshold: f64,
    shape: f64,
    scale: f64,
}

impl Gamma3 {
    /// Creates a shifted gamma with a finite threshold and positive parameters.
    pub fn new(threshold: f64, shape: f64, scale: f64) -> Result<Self> {
        if !threshold.is_finite() {
            return Err(Error::InvalidParameter {
                name: "threshold",
                value: threshold,
            });
        }
        Gamma::new(shape, scale)?;
        Ok(Self {
            threshold,
            shape,
            scale,
        })
    }

    /// Threshold `γ`: the smallest value the distribution supports.
    #[must_use]
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// Shape `α`.
    #[must_use]
    pub fn shape(&self) -> f64 {
        self.shape
    }

    /// Scale `β`.
    #[must_use]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Fits the threshold by profile likelihood and the rest by MLE.
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        for (index, &x) in sample.values().iter().enumerate() {
            if !x.is_finite() || x <= 0.0 {
                return Err(Error::NonPositive { index, value: x });
            }
        }
        let (threshold, gamma) = profile_threshold(sample, |values| Gamma::fit_mle(values))?;
        Self::new(threshold, gamma.shape(), gamma.scale())
    }
}

impl Distribution for Gamma3 {
    fn name(&self) -> &'static str {
        "Gamma (3-parameter)"
    }

    fn pdf(&self, x: f64) -> f64 {
        self.ln_pdf(x).exp()
    }

    fn cdf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 0.0;
        }
        regularized_gamma_lower(self.shape, (x - self.threshold) / self.scale)
    }

    fn sf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 1.0;
        }
        regularized_gamma_upper(self.shape, (x - self.threshold) / self.scale)
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return f64::NEG_INFINITY;
        }
        let z = (x - self.threshold) / self.scale;
        (self.shape - 1.0) * z.ln() - z - ln_gamma(self.shape) - self.shape * self.scale.ln()
    }

    fn quantile(&self, p: f64) -> f64 {
        if p <= 0.0 {
            return self.threshold;
        }
        if p >= 1.0 {
            return f64::INFINITY;
        }
        self.threshold + self.scale * Gamma::standard_quantile(self.shape, p).exp()
    }

    fn n_params(&self) -> usize {
        3
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![
            ("Threshold (γ)", self.threshold),
            ("Shape (α)", self.shape),
            ("Scale (β)", self.scale),
        ]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (threshold, shape, scale) = match parameters {
            [threshold, shape, scale] => (*threshold, *shape, *scale),
            _ => return None,
        };
        Self::new(threshold, shape, scale).ok()
    }
}

/// Three-parameter log-logistic distribution with a threshold.
///
/// `F(x) = 1/(1 + ((x − γ)/α)^(−β))` for `x > γ`. The threshold is chosen by
/// profile likelihood over a grid below the smallest observation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loglogistic3 {
    threshold: f64,
    scale: f64,
    shape: f64,
}

impl Loglogistic3 {
    /// Creates a shifted log-logistic with a finite threshold and positive parameters.
    pub fn new(threshold: f64, scale: f64, shape: f64) -> Result<Self> {
        if !threshold.is_finite() {
            return Err(Error::InvalidParameter {
                name: "threshold",
                value: threshold,
            });
        }
        Loglogistic::new(scale, shape)?;
        Ok(Self {
            threshold,
            scale,
            shape,
        })
    }

    /// Threshold `γ`: the smallest value the distribution supports.
    #[must_use]
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// Scale `α`.
    #[must_use]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Shape `β`.
    #[must_use]
    pub fn shape(&self) -> f64 {
        self.shape
    }

    /// Fits the threshold by profile likelihood and the rest by MLE.
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        for (index, &x) in sample.values().iter().enumerate() {
            if !x.is_finite() || x <= 0.0 {
                return Err(Error::NonPositive { index, value: x });
            }
        }
        let (threshold, fitted) = profile_threshold(sample, |values| Loglogistic::fit_mle(values))?;
        Self::new(threshold, fitted.scale(), fitted.shape())
    }
}

impl Distribution for Loglogistic3 {
    fn name(&self) -> &'static str {
        "Loglogistic (3-parameter)"
    }

    fn pdf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 0.0;
        }
        let z = self.shape * ((x - self.threshold) / self.scale).ln();
        let cdf = logistic_cdf(z);
        cdf * (1.0 - cdf) * self.shape / (x - self.threshold)
    }

    fn cdf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 0.0;
        }
        logistic_cdf(self.shape * ((x - self.threshold) / self.scale).ln())
    }

    fn sf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return 1.0;
        }
        logistic_cdf(-(self.shape * ((x - self.threshold) / self.scale).ln()))
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        if x <= self.threshold {
            return f64::NEG_INFINITY;
        }
        let z = self.shape * ((x - self.threshold) / self.scale).ln();
        self.shape.ln() - self.scale.ln() + ((self.shape - 1.0) / self.shape) * z
            - 2.0
                * if z >= 0.0 {
                    z + (-z).exp().ln_1p()
                } else {
                    z.exp().ln_1p()
                }
    }

    fn quantile(&self, p: f64) -> f64 {
        if p <= 0.0 {
            return self.threshold;
        }
        if p >= 1.0 {
            return f64::INFINITY;
        }
        self.threshold + self.scale * (p / (1.0 - p)).powf(1.0 / self.shape)
    }

    fn n_params(&self) -> usize {
        3
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![
            ("Threshold (γ)", self.threshold),
            ("Scale (α)", self.scale),
            ("Shape (β)", self.shape),
        ]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (threshold, scale, shape) = match parameters {
            [threshold, scale, shape] => (*threshold, *scale, *shape),
            _ => return None,
        };
        Self::new(threshold, scale, shape).ok()
    }
}

/// Smallest extreme value (Gumbel for minima) distribution.
///
/// `F(x) = 1 − exp(−exp((x − µ)/σ))` and the density
/// `f(x) = exp((x − µ)/σ − exp((x − µ)/σ))/σ`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmallestExtremeValue {
    location: f64,
    scale: f64,
}

impl SmallestExtremeValue {
    /// Creates a smallest-extreme-value distribution with a positive scale.
    pub fn new(location: f64, scale: f64) -> Result<Self> {
        if !location.is_finite() {
            return Err(Error::InvalidParameter {
                name: "location",
                value: location,
            });
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "scale",
                value: scale,
            });
        }
        Ok(Self { location, scale })
    }

    /// Location parameter `µ`.
    #[must_use]
    pub fn location(&self) -> f64 {
        self.location
    }

    /// Scale parameter `σ`.
    #[must_use]
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Maximum-likelihood fit.
    ///
    /// The score equations are `mean(exp(z)) = 1` and `mean(x·exp(z)) = x̄`,
    /// with `z = (x − µ)/σ`; they are solved with the same bisection/update
    /// scheme as the maximum extreme value, with the opposite sign.
    pub fn fit_mle(sample: &Sample) -> Result<Self> {
        if sample.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 2,
            });
        }
        let values = sample.values();
        let n = sample.n() as f64;
        let mean = sample.mean()?;
        let sd = sample.sd()?;
        if !sd.is_finite() || sd <= 0.0 {
            return Err(Error::ConstantSample);
        }
        let x_min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let x_max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mut scale = sd * (6.0f64).sqrt() / std::f64::consts::PI;
        let mut location = mean + std::f64::consts::EULER_GAMMA * scale;
        let mut converged = false;
        for _ in 0..200 {
            let mut lo = x_min - 50.0 * scale;
            let mut hi = x_max + 50.0 * scale;
            for _ in 0..100 {
                let mid = 0.5 * (lo + hi);
                let t_mean = values
                    .iter()
                    .map(|&x| ((x - mid) / scale).exp())
                    .sum::<f64>()
                    / n;
                if t_mean > 1.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let new_location = 0.5 * (lo + hi);
            let weighted = values
                .iter()
                .map(|&x| {
                    let t = ((x - new_location) / scale).exp();
                    x * t
                })
                .sum::<f64>();
            let new_scale = weighted / n - mean;
            if !new_scale.is_finite() || new_scale <= 0.0 {
                break;
            }
            let moved = (new_location - location).abs() <= 1e-12 * (1.0 + new_location.abs())
                && (new_scale - scale).abs() <= 1e-12 * (1.0 + new_scale.abs());
            location = new_location;
            scale = new_scale;
            if moved {
                converged = true;
                break;
            }
        }
        if !converged {
            let t_mean = values
                .iter()
                .map(|&x| ((x - location) / scale).exp())
                .sum::<f64>()
                / n;
            if (t_mean - 1.0).abs() > 1e-9 {
                return Err(Error::NotConverged {
                    detail: "smallest-extreme-value score equations".to_owned(),
                });
            }
        }
        Self::new(location, scale)
    }
}

impl Distribution for SmallestExtremeValue {
    fn name(&self) -> &'static str {
        "Smallest Extreme Value"
    }

    fn pdf(&self, x: f64) -> f64 {
        let z = (x - self.location) / self.scale;
        let t = z.exp();
        (z - t).exp() / self.scale
    }

    fn cdf(&self, x: f64) -> f64 {
        let z = (x - self.location) / self.scale;
        -(-z.exp()).exp_m1()
    }

    fn sf(&self, x: f64) -> f64 {
        let z = (x - self.location) / self.scale;
        (-z.exp()).exp()
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        let z = (x - self.location) / self.scale;
        z - z.exp() - self.scale.ln()
    }

    fn quantile(&self, p: f64) -> f64 {
        if p <= 0.0 {
            return f64::NEG_INFINITY;
        }
        if p >= 1.0 {
            return f64::INFINITY;
        }
        self.location + self.scale * (-(1.0 - p).ln()).ln()
    }

    fn n_params(&self) -> usize {
        2
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        vec![("Location (µ)", self.location), ("Scale (σ)", self.scale)]
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        let (location, scale) = match parameters {
            [location, scale] => (*location, *scale),
            _ => return None,
        };
        Self::new(location, scale).ok()
    }
}

/// Best threshold of a three-parameter family by profile likelihood.
///
/// The candidates span ten sample spreads below the smallest observation; for
/// each one the two-parameter family is fitted to `x − γ` and the threshold
/// with the largest log-likelihood is kept.
fn profile_threshold<D: Distribution>(
    sample: &Sample,
    fit: impl Fn(&Sample) -> Result<D>,
) -> Result<(f64, D)> {
    if sample.n() < 3 {
        return Err(Error::SampleTooSmall {
            n: sample.n(),
            min: 3,
        });
    }
    let mut sorted = sample.values().to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let q1 = quantile_of(&sorted, 0.25);
    let q3 = quantile_of(&sorted, 0.75);
    let mut spread = q3 - q1;
    if spread <= 0.0 {
        spread = sample.sd()?;
    }
    if !spread.is_finite() || spread <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let minimum = sorted[0];
    let lower = minimum - 10.0 * spread;
    let upper = minimum - 1e-6 * spread;
    let candidates = 60usize;
    let mut chosen: Option<(f64, D)> = None;
    let mut best_log_likelihood = f64::NEG_INFINITY;
    for index in 0..candidates {
        let gamma = lower + (upper - lower) * index as f64 / (candidates - 1) as f64;
        let shifted: Vec<f64> = sample.values().iter().map(|value| value - gamma).collect();
        let Ok(shifted_sample) = Sample::from_valid(shifted, 0) else {
            continue;
        };
        let Ok(fitted) = fit(&shifted_sample) else {
            continue;
        };
        let log_likelihood: f64 = shifted_sample
            .values()
            .iter()
            .map(|&value| fitted.ln_pdf(value))
            .sum();
        if log_likelihood.is_finite() && log_likelihood > best_log_likelihood {
            best_log_likelihood = log_likelihood;
            chosen = Some((gamma, fitted));
        }
    }
    chosen.ok_or_else(|| Error::NotConverged {
        detail: "the threshold profile likelihood found no candidate".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::MissingPolicy;

    fn normal_std() -> Normal {
        Normal::new(0.0, 1.0).unwrap()
    }

    #[test]
    fn lognormal_fit_uses_log_scale_moments() {
        let values = vec![1.0, 2.0, 4.0, 8.0];
        let s = Sample::new(values.clone(), MissingPolicy::Drop).unwrap();
        let d = Lognormal::fit_moments(&s).unwrap();
        let logs: Vec<f64> = values.iter().map(|x| x.ln()).collect();
        let ls = Sample::new(logs, MissingPolicy::Drop).unwrap();
        assert!((d.mu() - ls.mean().unwrap()).abs() < 1e-12);
        assert!((d.sigma() - ls.sd().unwrap()).abs() < 1e-12);
        assert!((d.cdf(d.quantile(0.5)) - 0.5).abs() < 1e-12);
        assert!((d.quantile(0.5) - d.mu().exp()).abs() < 1e-12);
    }

    #[test]
    fn logistic_fit_recovers_a_synthetic_sample() {
        let n = 80;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                2.0 + 0.7 * (p / (1.0 - p)).ln()
            })
            .collect();
        let s = Sample::new(values, MissingPolicy::Drop).unwrap();
        let d = Logistic::fit_mle(&s).unwrap();
        assert!((d.mu() - 2.0).abs() < 0.05, "mu {}", d.mu());
        assert!((d.sigma() - 0.7).abs() < 0.02, "sigma {}", d.sigma());
        assert!((d.cdf(d.quantile(0.5)) - 0.5).abs() < 1e-12);
        assert!((d.quantile(0.5) - d.mu()).abs() < 1e-12);
    }

    #[test]
    fn loglogistic_fit_recovers_a_synthetic_sample() {
        let n = 80;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                10.0 * (p / (1.0 - p)).powf(0.5)
            })
            .collect();
        let s = Sample::new(values, MissingPolicy::Drop).unwrap();
        let d = Loglogistic::fit_mle(&s).unwrap();
        assert!((d.scale() - 10.0).abs() < 0.3, "scale {}", d.scale());
        assert!((d.shape() - 2.0).abs() < 0.06, "shape {}", d.shape());
        assert!((d.cdf(d.quantile(0.5)) - 0.5).abs() < 1e-12);
        assert!((d.quantile(0.5) - d.scale()).abs() < 1e-12);
    }

    #[test]
    fn loglogistic_rejects_non_positive_values() {
        let s = Sample::new(vec![1.0, 0.0, 2.0], MissingPolicy::Drop).unwrap();
        assert!(matches!(
            Loglogistic::fit_mle(&s),
            Err(Error::NonPositive { .. })
        ));
    }

    #[test]
    fn weibull_fit_recovers_a_synthetic_sample() {
        let n = 60;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                10.0 * (-(1.0 - p).ln()).powf(0.5)
            })
            .collect();
        let s = Sample::new(values, MissingPolicy::Drop).unwrap();
        let d = Weibull::fit_mle(&s).unwrap();
        assert!((d.shape() - 2.0).abs() < 0.05, "shape {}", d.shape());
        assert!((d.scale() - 10.0).abs() < 0.2, "scale {}", d.scale());
        assert!((d.cdf(d.quantile(0.5)) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn gamma_fit_recovers_a_synthetic_sample() {
        let truth = Gamma::new(2.5, 3.0).unwrap();
        let n = 2000;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                truth.quantile(p)
            })
            .collect();
        let s = Sample::new(values, MissingPolicy::Drop).unwrap();
        let d = Gamma::fit_mle(&s).unwrap();
        assert!((d.shape() - 2.5).abs() < 0.02, "shape {}", d.shape());
        assert!((d.scale() - 3.0).abs() < 0.06, "scale {}", d.scale());
        assert!((d.mean() - 7.5).abs() < 0.05, "mean {}", d.mean());
    }

    #[test]
    fn gamma_quantile_inverts_the_cdf() {
        for (shape, scale) in [(0.5, 1.0), (1.0, 2.0), (3.0, 0.5), (12.0, 4.0)] {
            let d = Gamma::new(shape, scale).unwrap();
            for p in [1e-5, 0.01, 0.1, 0.5, 0.9, 0.99, 0.9999] {
                let x = d.quantile(p);
                assert!(x > 0.0, "shape={shape}, p={p}, x={x}");
                let back = d.cdf(x);
                assert!(
                    (back - p).abs() < 1e-9 * p.max(1e-3),
                    "shape={shape}, p={p}, x={x}, F={back}"
                );
            }
            assert_eq!(d.quantile(0.0), 0.0);
            assert!(d.quantile(1.0).is_infinite());
        }
    }

    #[test]
    fn gamma_survival_is_the_upper_tail() {
        let d = Gamma::new(2.0, 3.0).unwrap();
        for x in [0.5, 3.0, 9.0, 40.0] {
            let (cdf, sf) = (d.cdf(x), d.sf(x));
            assert!((cdf + sf - 1.0).abs() < 1e-12, "x={x}");
            // Density matches the derivative of the CDF by finite difference.
            let h = 1e-4 * x;
            let numeric = (d.cdf(x + h) - d.cdf(x - h)) / (2.0 * h);
            assert!(
                (numeric - d.pdf(x)).abs() < 1e-5 * d.pdf(x).max(1e-6),
                "x={x}, numeric={numeric}, pdf={}",
                d.pdf(x)
            );
        }
    }

    #[test]
    fn gamma_rejects_non_positive_values() {
        let s = Sample::new(vec![1.0, -1.0, 2.0], MissingPolicy::Drop).unwrap();
        assert!(matches!(Gamma::fit_mle(&s), Err(Error::NonPositive { .. })));
        assert!(matches!(
            Gamma::new(0.0, 1.0),
            Err(Error::InvalidParameter { .. })
        ));
    }

    #[test]
    fn three_parameter_weibull_finds_the_threshold() {
        let truth = Weibull3::new(5.0, 2.0, 10.0).unwrap();
        let n = 200;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                truth.quantile(p)
            })
            .collect();
        let s = Sample::new(values, MissingPolicy::Drop).unwrap();
        let d = Weibull3::fit_mle(&s).expect("fit");
        assert!(
            (d.threshold() - 5.0).abs() < 0.6,
            "threshold {}",
            d.threshold()
        );
        assert!((d.shape() - 2.0).abs() < 0.15, "shape {}", d.shape());
        assert!((d.scale() - 10.0).abs() < 0.8, "scale {}", d.scale());
        assert!((d.cdf(d.quantile(0.5)) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn three_parameter_lognormal_finds_the_threshold() {
        let truth = Lognormal3::new(5.0, 2.0, 0.4).unwrap();
        let n = 300;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                truth.quantile(p)
            })
            .collect();
        let s = Sample::new(values, MissingPolicy::Drop).unwrap();
        let d = Lognormal3::fit_mle(&s).expect("fit");
        assert!(
            (d.threshold() - 5.0).abs() < 0.6,
            "threshold {}",
            d.threshold()
        );
        assert!((d.mu() - 2.0).abs() < 0.05, "mu {}", d.mu());
        assert!((d.sigma() - 0.4).abs() < 0.03, "sigma {}", d.sigma());
        assert!((d.cdf(d.quantile(0.5)) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn three_parameter_lognormal_rejects_non_positive_values() {
        let s = Sample::new(vec![1.0, -1.0, 2.0], MissingPolicy::Drop).unwrap();
        assert!(matches!(
            Lognormal3::fit_mle(&s),
            Err(Error::NonPositive { .. })
        ));
    }

    #[test]
    fn three_parameter_weibull_rejects_non_positive_values() {
        let s = Sample::new(vec![1.0, -1.0, 2.0], MissingPolicy::Drop).unwrap();
        assert!(matches!(
            Weibull3::fit_mle(&s),
            Err(Error::NonPositive { .. })
        ));
    }

    #[test]
    fn extreme_value_fit_recovers_a_synthetic_sample() {
        let truth = ExtremeValue::new(3.0, 1.5).unwrap();
        let n = 500;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                truth.quantile(p)
            })
            .collect();
        let s = Sample::new(values, MissingPolicy::Drop).unwrap();
        let d = ExtremeValue::fit_mle(&s).unwrap();
        assert!(
            (d.location() - 3.0).abs() < 0.1,
            "location {}",
            d.location()
        );
        assert!((d.scale() - 1.5).abs() < 0.1, "scale {}", d.scale());
        assert!((d.cdf(d.quantile(0.5)) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn extreme_value_score_equations_hold_at_the_fit() {
        let values: Vec<f64> = (1..=120)
            .map(|i| {
                let p = (i as f64 - 0.5) / 120.0;
                ExtremeValue::new(1.0, 2.0).unwrap().quantile(p)
            })
            .collect();
        let s = Sample::new(values, MissingPolicy::Drop).unwrap();
        let d = ExtremeValue::fit_mle(&s).unwrap();
        let n = s.n() as f64;
        let t_mean = s
            .values()
            .iter()
            .map(|&x| (-(x - d.location()) / d.scale()).exp())
            .sum::<f64>()
            / n;
        assert!((t_mean - 1.0).abs() < 1e-6, "mean(t) = {t_mean}");
        let weighted = s
            .values()
            .iter()
            .map(|&x| {
                let t = (-(x - d.location()) / d.scale()).exp();
                x * t
            })
            .sum::<f64>()
            / n;
        let implied = s.mean().unwrap() - weighted;
        assert!((implied - d.scale()).abs() < 1e-6, "scale {implied}");
    }

    #[test]
    fn extreme_value_quantile_inverts_the_cdf() {
        let d = ExtremeValue::new(2.0, 3.0).unwrap();
        for p in [1e-6, 0.01, 0.1, 0.5, 0.9, 0.99] {
            let x = d.quantile(p);
            assert!(
                (d.cdf(x) - p).abs() < 1e-9 * p.max(1e-3),
                "p={p}, x={x}, F={}",
                d.cdf(x)
            );
        }
        assert!(d.quantile(0.0).is_infinite());
        assert!(d.quantile(1.0).is_infinite());
    }

    #[test]
    fn extreme_value_density_matches_its_log_density() {
        // Regression: the density used to multiply the exponent by exp(−z)
        // before exponentiating, so it collapsed in the lower tail.
        let d = ExtremeValue::new(1.0, 0.8).unwrap();
        for x in [-0.221_743_700_646_320_96, 0.5, 1.0, 2.0, 4.0] {
            let pdf = d.pdf(x);
            let from_log = d.ln_pdf(x).exp();
            assert!(
                (pdf - from_log).abs() <= 1e-12 * pdf.abs().max(from_log.abs()),
                "x={x}: pdf={pdf}, exp(ln pdf)={from_log}"
            );
            assert!(pdf > 0.0, "x={x}: the density must stay positive");
        }
        // The reference density at the 1 % quantile (Gumbel distribution).
        assert!((d.pdf(-0.221_743_700_646_320_96) - 0.057_564_627_324_851_1).abs() < 1e-12);
    }

    #[test]
    fn weibull_rejects_non_positive_values() {
        let s = Sample::new(vec![1.0, -1.0, 2.0], MissingPolicy::Drop).unwrap();
        assert!(matches!(
            Weibull::fit_mle(&s),
            Err(Error::NonPositive { .. })
        ));
    }

    #[test]
    fn exponential_fit_is_the_sample_mean() {
        let values = vec![1.0, 2.0, 3.0, 4.0];
        let s = Sample::new(values.clone(), MissingPolicy::Drop).unwrap();
        let d = Exponential::fit_mle(&s).unwrap();
        assert!((d.theta() - 2.5).abs() < 1e-12);
        assert!((d.cdf(d.quantile(0.5)) - 0.5).abs() < 1e-12);
        assert!((d.quantile(0.5) - 2.5 * std::f64::consts::LN_2).abs() < 1e-12);
    }

    #[test]
    fn exponential_rejects_non_positive_values() {
        let s = Sample::new(vec![1.0, 0.0, 2.0], MissingPolicy::Drop).unwrap();
        assert!(matches!(
            Exponential::fit_mle(&s),
            Err(Error::NonPositive { .. })
        ));
    }

    #[test]
    fn weibull_rejects_constant_samples() {
        let s = Sample::new(vec![2.0, 2.0, 2.0], MissingPolicy::Drop).unwrap();
        assert_eq!(Weibull::fit_mle(&s).unwrap_err(), Error::ConstantSample);
    }

    #[test]
    fn lognormal_rejects_non_positive_values() {
        let s = Sample::new(vec![1.0, 0.0, 2.0], MissingPolicy::Drop).unwrap();
        assert!(matches!(
            Lognormal::fit_moments(&s),
            Err(Error::NonPositive { .. })
        ));
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        assert!(matches!(
            Normal::new(0.0, 0.0),
            Err(Error::InvalidParameter { name: "sigma", .. })
        ));
        assert!(matches!(
            Normal::new(f64::NAN, 1.0),
            Err(Error::InvalidParameter { name: "mu", .. })
        ));
        assert!(Normal::new(0.0, -1.0).is_err());
    }

    #[test]
    fn density_at_zero() {
        let n = normal_std();
        assert!((n.pdf(0.0) - 0.398_942_280_401_432_7).abs() < 1e-15);
    }

    #[test]
    fn cdf_and_quantile_are_consistent() {
        let n = Normal::new(10.0, 2.0).unwrap();
        assert!((n.cdf(10.0) - 0.5).abs() < 1e-15);
        let x = n.quantile(0.975);
        assert!((x - (10.0 + 2.0 * 1.959_963_984_540_054)).abs() < 1e-10);
    }

    #[test]
    fn ln_pdf_equals_log_of_pdf() {
        let n = Normal::new(-1.0, 3.0).unwrap();
        for x in [-5.0, -1.0, 0.0, 2.5, 9.0] {
            let a = n.ln_pdf(x);
            let b = n.pdf(x).ln();
            assert!((a - b).abs() < 1e-14, "x={x}: {a} vs {b}");
        }
    }

    #[test]
    fn fit_recovers_parameters() {
        let s = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let n = Normal::fit(&s).unwrap();
        assert!((n.mu() - 3.0).abs() < 1e-15);
        // sqrt(10/5) = sqrt(2)
        assert!((n.sigma() - std::f64::consts::SQRT_2).abs() < 1e-15);
    }

    #[test]
    fn fit_rejects_constant_and_n_one() {
        let c = Sample::new(vec![2.0, 2.0, 2.0], MissingPolicy::Drop).unwrap();
        assert_eq!(Normal::fit(&c).unwrap_err(), Error::ConstantSample);
        let one = Sample::new(vec![2.0], MissingPolicy::Drop).unwrap();
        assert_eq!(
            Normal::fit(&one).unwrap_err(),
            Error::SampleTooSmall { n: 1, min: 2 }
        );
    }

    #[test]
    fn log_likelihood_at_the_maximum() {
        // Perturbing the MLE parameters cannot increase the log-likelihood.
        let s = Sample::new(
            vec![2.1, 2.4, 1.9, 2.2, 2.6, 2.0, 2.3, 1.8],
            MissingPolicy::Drop,
        )
        .unwrap();
        let fit = Normal::fit(&s).unwrap();
        let ll = fit.log_likelihood(&s).unwrap();
        for delta in [-0.1, -0.01, 0.01, 0.1] {
            let m = Normal::new(fit.mu() + delta, fit.sigma()).unwrap();
            assert!(m.log_likelihood(&s).unwrap() <= ll);
            let v = Normal::new(fit.mu(), fit.sigma() * (1.0 + delta)).unwrap();
            assert!(v.log_likelihood(&s).unwrap() <= ll);
        }
    }

    #[test]
    fn affine_invariance_of_the_cdf() {
        let a = Normal::new(0.0, 1.0).unwrap();
        let b = Normal::new(5.0, 3.0).unwrap();
        for x in [-2.0, 0.0, 1.5] {
            assert!((a.cdf(x) - b.cdf(5.0 + 3.0 * x)).abs() < 1e-14);
        }
    }

    #[test]
    fn the_shifted_exponential_uses_the_sample_minimum() {
        let s = Sample::new(vec![3.0, 4.0, 5.0, 6.0], MissingPolicy::Drop).unwrap();
        let d = Exponential2::fit_mle(&s).unwrap();
        assert!((d.threshold() - 3.0).abs() < 1e-12);
        assert!((d.theta() - 1.5).abs() < 1e-12);
        assert_eq!(d.cdf(3.0), 0.0);
        assert!((d.cdf(d.quantile(0.7)) - 0.7).abs() < 1e-12);
        assert!(
            Exponential2::fit_mle(&Sample::new(vec![2.0, 2.0], MissingPolicy::Drop).unwrap())
                .is_err()
        );
    }

    #[test]
    fn the_three_parameter_gamma_recovers_a_synthetic_sample() {
        let truth = Gamma3::new(2.0, 3.0, 1.5).unwrap();
        let n = 120;
        let values: Vec<f64> = (1..=n)
            .map(|i| truth.quantile((i as f64 - 0.5) / n as f64))
            .collect();
        let d = Gamma3::fit_mle(&Sample::new(values, MissingPolicy::Drop).unwrap()).unwrap();
        assert!(
            (d.threshold() - 2.0).abs() < 0.5,
            "threshold {}",
            d.threshold()
        );
        assert!((d.shape() - 3.0).abs() < 0.5, "shape {}", d.shape());
        assert!((d.scale() - 1.5).abs() < 0.5, "scale {}", d.scale());
        assert!((d.cdf(d.quantile(0.4)) - 0.4).abs() < 1e-9);
        assert_eq!(d.n_params(), 3);
    }

    #[test]
    fn the_three_parameter_loglogistic_recovers_a_synthetic_sample() {
        let truth = Loglogistic3::new(5.0, 2.0, 1.5).unwrap();
        let n = 120;
        let values: Vec<f64> = (1..=n)
            .map(|i| truth.quantile((i as f64 - 0.5) / n as f64))
            .collect();
        let d = Loglogistic3::fit_mle(&Sample::new(values, MissingPolicy::Drop).unwrap()).unwrap();
        assert!(
            (d.threshold() - 5.0).abs() < 0.6,
            "threshold {}",
            d.threshold()
        );
        assert!((d.scale() - 2.0).abs() < 0.6, "scale {}", d.scale());
        assert!((d.shape() - 1.5).abs() < 0.6, "shape {}", d.shape());
        assert!((d.cdf(d.quantile(0.6)) - 0.6).abs() < 1e-9);
    }

    #[test]
    fn the_smallest_extreme_value_recovers_a_synthetic_sample() {
        let truth = SmallestExtremeValue::new(10.0, 2.0).unwrap();
        let n = 120;
        let values: Vec<f64> = (1..=n)
            .map(|i| truth.quantile((i as f64 - 0.5) / n as f64))
            .collect();
        let d = SmallestExtremeValue::fit_mle(&Sample::new(values, MissingPolicy::Drop).unwrap())
            .unwrap();
        assert!(
            (d.location() - 10.0).abs() < 0.2,
            "location {}",
            d.location()
        );
        assert!((d.scale() - 2.0).abs() < 0.15, "scale {}", d.scale());
        assert!((d.cdf(d.quantile(0.3)) - 0.3).abs() < 1e-9);
        assert!(d.quantile(0.5) < d.location() + d.scale());
        assert!(
            SmallestExtremeValue::fit_mle(&Sample::new(vec![1.0], MissingPolicy::Drop).unwrap())
                .is_err()
        );
    }
}
