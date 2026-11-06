//! Johnson SU/SB/SL distributions, fitted by maximum likelihood.
//!
//! The three families map a shifted/scaled observation to a standard normal:
//!
//! - **SL** (unbounded below): `z = γ + η·ln(x − ε)`, `x > ε`.
//! - **SU** (unbounded): `z = γ + η·asinh((x − ε)/λ)`, `λ > 0`.
//! - **SB** (bounded): `z = γ + η·ln((x − ε)/(ε + λ − x))`, `ε < x < ε + λ`.
//!
//! The shape parameters `(ε, λ)` are optimised by Nelder–Mead; for a fixed
//! shape the normal parameters are closed-form (`η = 1/sd(u)`,
//! `γ = −mean(u)/sd(u)` on `u = (z − γ)/η`). The profile likelihood is the
//! exact Johnson log-likelihood, Jacobian included, so the three families and
//! the other distributions are compared on the same scale.

use crate::data::Sample;
use crate::dists::Distribution;
use crate::error::{Error, Result};
use crate::optimize::nelder_mead;
use crate::special::{normal_cdf, normal_quantile};

/// Johnson family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JohnsonFamily {
    /// Unbounded, `sinh` transformation.
    Su,
    /// Bounded, logistic transformation.
    Sb,
    /// Unbounded below, logarithmic transformation.
    Sl,
}

impl JohnsonFamily {
    /// Stable name for reports.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Su => "Johnson SU",
            Self::Sb => "Johnson SB",
            Self::Sl => "Johnson SL",
        }
    }

    /// Number of estimated parameters.
    #[must_use]
    pub fn n_params(self) -> usize {
        match self {
            Self::Sl => 3,
            Self::Su | Self::Sb => 4,
        }
    }
}

/// A fitted Johnson distribution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Johnson {
    family: JohnsonFamily,
    gamma: f64,
    eta: f64,
    epsilon: f64,
    lambda: f64,
}

impl Johnson {
    /// Creates a Johnson distribution.
    ///
    /// `lambda` is ignored for SL (set to 1).
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidParameter`] for non-finite parameters or a
    /// non-positive `eta`/`lambda`.
    pub fn new(
        family: JohnsonFamily,
        gamma: f64,
        eta: f64,
        epsilon: f64,
        lambda: f64,
    ) -> Result<Self> {
        if !gamma.is_finite() || !epsilon.is_finite() {
            return Err(Error::InvalidParameter {
                name: "Johnson location",
                value: if gamma.is_finite() { epsilon } else { gamma },
            });
        }
        if !eta.is_finite() || eta <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "Johnson eta",
                value: eta,
            });
        }
        if family != JohnsonFamily::Sl && (!lambda.is_finite() || lambda <= 0.0) {
            return Err(Error::InvalidParameter {
                name: "Johnson lambda",
                value: lambda,
            });
        }
        Ok(Self {
            family,
            gamma,
            eta,
            epsilon,
            lambda: if family == JohnsonFamily::Sl {
                1.0
            } else {
                lambda
            },
        })
    }

    /// Family.
    #[must_use]
    pub fn family(&self) -> JohnsonFamily {
        self.family
    }

    /// `γ`.
    #[must_use]
    pub fn gamma(&self) -> f64 {
        self.gamma
    }

    /// `η`.
    #[must_use]
    pub fn eta(&self) -> f64 {
        self.eta
    }

    /// `ε`.
    #[must_use]
    pub fn epsilon(&self) -> f64 {
        self.epsilon
    }

    /// `λ` (1 for SL).
    #[must_use]
    pub fn lambda(&self) -> f64 {
        self.lambda
    }

    /// The standard-normal score of one observation, when it is inside the
    /// support.
    fn score(&self, x: f64) -> Option<f64> {
        match self.family {
            JohnsonFamily::Sl => {
                if x <= self.epsilon {
                    return None;
                }
                Some(self.gamma + self.eta * (x - self.epsilon).ln())
            }
            JohnsonFamily::Su => {
                Some(self.gamma + self.eta * ((x - self.epsilon) / self.lambda).asinh())
            }
            JohnsonFamily::Sb => {
                let lower = x - self.epsilon;
                let upper = self.epsilon + self.lambda - x;
                if lower <= 0.0 || upper <= 0.0 {
                    return None;
                }
                Some(self.gamma + self.eta * (lower / upper).ln())
            }
        }
    }

    /// Log-likelihood of a sample.
    pub fn log_likelihood(&self, sample: &Sample) -> Result<f64> {
        if sample.n() == 0 {
            return Err(Error::EmptySample);
        }
        Ok(sample.values().iter().map(|&x| self.ln_pdf(x)).sum())
    }

    /// Akaike information criterion (`2k − 2·ln L`).
    pub fn aic(&self, sample: &Sample) -> Result<f64> {
        Ok(2.0 * self.family.n_params() as f64 - 2.0 * self.log_likelihood(sample)?)
    }

    /// Bayesian information criterion (`k·ln n − 2·ln L`).
    pub fn bic(&self, sample: &Sample) -> Result<f64> {
        Ok(self.family.n_params() as f64 * (sample.n() as f64).ln()
            - 2.0 * self.log_likelihood(sample)?)
    }

    /// Maximum-likelihood fit of one family.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SampleTooSmall`] for fewer than five observations and
    /// [`Error::ConstantSample`] for a constant sample.
    pub fn fit(sample: &Sample, family: JohnsonFamily) -> Result<Self> {
        if sample.n() < 5 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 5,
            });
        }
        let values = sample.values();
        let low = values.iter().copied().fold(f64::INFINITY, f64::min);
        let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let range = high - low;
        if !range.is_finite() || range <= 0.0 {
            return Err(Error::ConstantSample);
        }
        let mean = sample.mean()?;
        let sd = sample.sd()?;
        if !sd.is_finite() || sd <= 0.0 {
            return Err(Error::ConstantSample);
        }

        // The transformed coordinate u = (z − γ)/η. For a fixed shape the
        // normal parameters are the MLE of u, so only the shape is optimised.
        let shape_parameters = |shape: &[f64]| -> Option<(f64, f64)> {
            let (epsilon, lambda) = match family {
                JohnsonFamily::Sl => (low - shape[0].exp(), 1.0),
                JohnsonFamily::Su => (mean + shape[0] * sd, shape[1].exp()),
                JohnsonFamily::Sb => {
                    let epsilon = low - shape[0].exp();
                    let upper = high + shape[1].exp();
                    (epsilon, upper - epsilon)
                }
            };
            if !epsilon.is_finite() || !lambda.is_finite() || lambda <= 0.0 {
                return None;
            }
            Some((epsilon, lambda))
        };
        let profile = |shape: &[f64]| -> f64 {
            let Some((epsilon, lambda)) = shape_parameters(shape) else {
                return f64::INFINITY;
            };
            let candidate = match Self::new(family, 0.0, 1.0, epsilon, lambda) {
                Ok(candidate) => candidate,
                Err(_) => return f64::INFINITY,
            };
            let mut u = Vec::with_capacity(values.len());
            for &value in values {
                let Some(score) = candidate.score(value) else {
                    return f64::INFINITY;
                };
                u.push(score);
            }
            let n = u.len() as f64;
            let mean_u = u.iter().sum::<f64>() / n;
            let variance = u.iter().map(|value| (value - mean_u).powi(2)).sum::<f64>() / n;
            if !variance.is_finite() || variance <= 1e-24 {
                return f64::INFINITY;
            }
            let sd_u = variance.sqrt();
            let fitted = match Self::new(family, -mean_u / sd_u, 1.0 / sd_u, epsilon, lambda) {
                Ok(fitted) => fitted,
                Err(_) => return f64::INFINITY,
            };
            let likelihood: f64 = values.iter().map(|&value| fitted.ln_pdf(value)).sum();
            if likelihood.is_finite() {
                -likelihood
            } else {
                f64::INFINITY
            }
        };

        let starts: Vec<Vec<f64>> = match family {
            JohnsonFamily::Sl => vec![
                vec![(range * 0.01).ln()],
                vec![(range * 0.1).ln()],
                vec![(range * 0.5).ln()],
                vec![range.ln()],
            ],
            JohnsonFamily::Su => {
                let mut starts = Vec::new();
                for offset in [-2.0, 0.0, 2.0] {
                    for scale in [0.25, 0.5, 1.0, 2.0, 4.0] {
                        starts.push(vec![offset, (sd * scale).ln()]);
                    }
                }
                starts
            }
            JohnsonFamily::Sb => {
                let mut starts = Vec::new();
                for lower in [0.01, 0.1, 0.5] {
                    for upper in [0.01, 0.1, 0.5] {
                        starts.push(vec![(range * lower).ln(), (range * upper).ln()]);
                    }
                }
                starts
            }
        };

        let mut best: Option<(f64, Self)> = None;
        for start in starts {
            let steps = vec![0.5; start.len()];
            let Ok(result) = nelder_mead(&start, &steps, 2000, 1e-10, profile) else {
                continue;
            };
            let Some((epsilon, lambda)) = shape_parameters(&result.arguments) else {
                continue;
            };
            let candidate = match Self::new(family, 0.0, 1.0, epsilon, lambda) {
                Ok(candidate) => candidate,
                Err(_) => continue,
            };
            let mut u = Vec::with_capacity(values.len());
            let mut inside = true;
            for &value in values {
                match candidate.score(value) {
                    Some(score) => u.push(score),
                    None => {
                        inside = false;
                        break;
                    }
                }
            }
            if !inside {
                continue;
            }
            let n = u.len() as f64;
            let mean_u = u.iter().sum::<f64>() / n;
            let variance = u.iter().map(|value| (value - mean_u).powi(2)).sum::<f64>() / n;
            if !variance.is_finite() || variance <= 1e-24 {
                continue;
            }
            let sd_u = variance.sqrt();
            let Ok(fitted) = Self::new(family, -mean_u / sd_u, 1.0 / sd_u, epsilon, lambda) else {
                continue;
            };
            let likelihood = fitted.log_likelihood(sample)?;
            if !likelihood.is_finite() {
                continue;
            }
            let score = -likelihood;
            if best.as_ref().is_none_or(|(value, _)| score < *value) {
                best = Some((score, fitted));
            }
        }
        best.map(|(_, fitted)| fitted)
            .ok_or_else(|| Error::NotConverged {
                detail: format!("{} maximum likelihood", family.name()),
            })
    }

    /// Fits all three families and returns the one with the smallest AIC.
    ///
    /// # Errors
    ///
    /// Returns the first fit error when no family can be fitted.
    pub fn fit_best(sample: &Sample) -> Result<Self> {
        let mut best: Option<(f64, Self)> = None;
        let mut first_error = None;
        for family in [JohnsonFamily::Su, JohnsonFamily::Sb, JohnsonFamily::Sl] {
            match Self::fit(sample, family) {
                Ok(fitted) => {
                    let aic = fitted.aic(sample)?;
                    if best.as_ref().is_none_or(|(value, _)| aic < *value) {
                        best = Some((aic, fitted));
                    }
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        best.map(|(_, fitted)| fitted).ok_or_else(|| {
            first_error.unwrap_or_else(|| Error::NotConverged {
                detail: "Johnson family".to_owned(),
            })
        })
    }
}

impl Distribution for Johnson {
    fn name(&self) -> &'static str {
        self.family.name()
    }

    fn pdf(&self, x: f64) -> f64 {
        self.ln_pdf(x).exp()
    }

    fn cdf(&self, x: f64) -> f64 {
        self.score(x).map_or_else(
            || {
                if x <= self.epsilon && self.family != JohnsonFamily::Su {
                    0.0
                } else {
                    1.0
                }
            },
            normal_cdf,
        )
    }

    fn ln_pdf(&self, x: f64) -> f64 {
        let Some(score) = self.score(x) else {
            return f64::NEG_INFINITY;
        };
        let normal = -0.5 * score * score - 0.5 * (2.0 * std::f64::consts::PI).ln();
        match self.family {
            JohnsonFamily::Sl => self.eta.ln() - (x - self.epsilon).ln() + normal,
            JohnsonFamily::Su => {
                self.eta.ln() - 0.5 * (self.lambda * self.lambda + (x - self.epsilon).powi(2)).ln()
                    + normal
            }
            JohnsonFamily::Sb => {
                self.eta.ln() + self.lambda.ln()
                    - (x - self.epsilon).ln()
                    - (self.epsilon + self.lambda - x).ln()
                    + normal
            }
        }
    }

    fn quantile(&self, p: f64) -> f64 {
        if !(0.0..=1.0).contains(&p) {
            return f64::NAN;
        }
        let z = normal_quantile(p);
        let u = (z - self.gamma) / self.eta;
        match self.family {
            JohnsonFamily::Sl => self.epsilon + u.exp(),
            JohnsonFamily::Su => self.epsilon + self.lambda * u.sinh(),
            JohnsonFamily::Sb => self.epsilon + self.lambda / (1.0 + (-u).exp()),
        }
    }

    fn n_params(&self) -> usize {
        self.family.n_params()
    }

    fn parameters(&self) -> Vec<(&'static str, f64)> {
        match self.family {
            JohnsonFamily::Sl => vec![("γ", self.gamma), ("η", self.eta), ("ε", self.epsilon)],
            JohnsonFamily::Su | JohnsonFamily::Sb => vec![
                ("γ", self.gamma),
                ("η", self.eta),
                ("ε", self.epsilon),
                ("λ", self.lambda),
            ],
        }
    }

    fn from_parameters(parameters: &[f64]) -> Option<Self> {
        match parameters {
            [gamma, eta, epsilon] => Self::new(JohnsonFamily::Sl, *gamma, *eta, *epsilon, 1.0).ok(),
            [gamma, eta, epsilon, lambda] => {
                let family = if lambda.is_finite() && *lambda > 0.0 {
                    // The caller decides SU or SB from the family field; this
                    // constructor is used only with the stored family.
                    JohnsonFamily::Su
                } else {
                    return None;
                };
                Self::new(family, *gamma, *eta, *epsilon, *lambda).ok()
            }
            _ => None,
        }
    }
}
