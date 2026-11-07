// SPDX-License-Identifier: MIT
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::MissingPolicy;

    #[test]
    fn normal_fit_has_state_and_n() {
        let s = Sample::new(vec![1.0, 2.0, 3.0, 4.0], MissingPolicy::Drop).unwrap();
        let fit = fit_normal(&s).unwrap();
        assert_eq!(fit.status, FitStatus::Converged);
        assert_eq!(fit.n, 4);
        assert_eq!(fit.n_params(), 2);
        assert!(fit.log_likelihood.is_finite());
    }

    #[test]
    fn c4_matches_published_values() {
        let cases = [
            (2, 0.797_884_560_802_865_1),
            (5, 0.939_985_602_986_625_7),
            (8, 0.965_030_456_147_372_7),
            (20, 0.986_934_267_524_653),
            (200, 0.998_744_512_664_535_8),
        ];
        for (n, expected) in cases {
            assert!(
                (c4(n) - expected).abs() < 1e-12,
                "c4({n}) = {}, expected {expected}",
                c4(n)
            );
        }
        assert!(c4(1).is_nan());
    }

    #[test]
    fn not_converged_state_is_not_ok() {
        assert!(!FitStatus::NotConverged.is_ok());
        assert!(FitStatus::Converged.is_ok());
        assert_eq!(FitStatus::NotConverged.as_str(), "NOT converged");
    }

    #[test]
    fn empty_or_constant_sample_fails() {
        assert_eq!(
            Sample::new(vec![], MissingPolicy::Drop).unwrap_err(),
            Error::EmptySample
        );
        let c = Sample::new(vec![1.0, 1.0, 1.0], MissingPolicy::Drop).unwrap();
        assert_eq!(fit_normal(&c).unwrap_err(), Error::ConstantSample);
    }
}
