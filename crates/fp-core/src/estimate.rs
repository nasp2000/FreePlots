//! Parameter estimation.
//!
//! At this stage only the closed-form Normal estimator exists. The structure
//! (`Fit`, `FitStatus`) is already final: the convergence state travels
//! **inside** the result, so that the UI can never present a failed fit as if
//! it were valid.

use crate::data::Sample;
use crate::dists::{Distribution, Normal};
use crate::error::{Error, Result};

/// State of a fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitStatus {
    /// Estimation completed within the tolerances.
    Converged,
    /// Estimation did not reach the tolerances; parameters not usable.
    NotConverged,
}

impl FitStatus {
    /// `true` if the fit is usable.
    #[must_use]
    pub fn is_ok(self) -> bool {
        matches!(self, Self::Converged)
    }

    /// Text for the report.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Converged => "converged",
            Self::NotConverged => "NOT converged",
        }
    }
}

/// Result of a fit, with everything the report needs.
#[derive(Debug, Clone)]
pub struct Fit<D> {
    /// Fitted distribution, with the estimated parameters.
    pub distribution: D,
    /// Estimation state.
    pub status: FitStatus,
    /// Log-likelihood at the optimum.
    pub log_likelihood: f64,
    /// Observations used.
    pub n: usize,
    /// Observations excluded.
    pub excluded: usize,
}

impl<D: Distribution> Fit<D> {
    /// Number of estimated parameters (for AIC/BIC).
    #[must_use]
    pub fn n_params(&self) -> usize {
        self.distribution.n_params()
    }
}

/// Fits a Normal by maximum likelihood.
pub fn fit_normal(sample: &Sample) -> Result<Fit<Normal>> {
    if sample.n() == 0 {
        return Err(Error::EmptySample);
    }
    let distribution = Normal::fit(sample)?;
    let log_likelihood = distribution.log_likelihood(sample)?;
    if !log_likelihood.is_finite() {
        return Err(Error::NotConverged {
            detail: format!("non-finite log-likelihood: {log_likelihood}"),
        });
    }
    Ok(Fit {
        distribution,
        status: FitStatus::Converged,
        log_likelihood,
        n: sample.n(),
        excluded: sample.excluded(),
    })
}

/// Unbiasing constant for the sample standard deviation,
/// `c4 = sqrt(2/(n−1))·Γ(n/2)/Γ((n−1)/2)`.
///
/// the documented behaviour uses `s²(1 − c4²)` as the variance of `s` when it builds the
/// probability-plot confidence band.
#[must_use]
pub fn c4(n: usize) -> f64 {
    if n < 2 {
        return f64::NAN;
    }
    let n = n as f64;
    (2.0 / (n - 1.0)).sqrt()
        * (crate::special::ln_gamma(n / 2.0) - crate::special::ln_gamma((n - 1.0) / 2.0)).exp()
}
