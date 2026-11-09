// SPDX-License-Identifier: MIT
//! Engine errors.
//!
//! Rule: any situation that produces a statistically invalid result must be an
//! error or an explicit warning — never a silent number.

use thiserror::Error;

/// Statistical engine error.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum Error {
    /// There is no valid observation.
    #[error("empty sample: no valid observations")]
    EmptySample,

    /// The sample has fewer observations than the method requires.
    #[error("sample too small for the method: n={n}, minimum={min}")]
    SampleTooSmall {
        /// Valid observations available.
        n: usize,
        /// Minimum required by the method.
        min: usize,
    },

    /// All values are equal: the scale is not identified.
    #[error("constant sample: the variance is zero and the fit is not identified")]
    ConstantSample,

    /// Non-finite value found where the policy is `Error`.
    #[error("non-finite value at observation {index}: {value}")]
    NonFinite {
        /// Index in the original sample (0-based).
        index: usize,
        /// The value found.
        value: f64,
    },

    /// Non-positive value where the distribution requires `x > 0`.
    #[error("non-positive value at observation {index}: {value} (the distribution requires x > 0)")]
    NonPositive {
        /// Index in the original sample (0-based).
        index: usize,
        /// The value found.
        value: f64,
    },

    /// Parameter outside the valid domain.
    #[error("invalid parameter: {name} = {value}")]
    InvalidParameter {
        /// Parameter name.
        name: &'static str,
        /// Rejected value.
        value: f64,
    },

    /// Probability outside `]0, 1[`.
    #[error("invalid probability: {p} (expected 0 < p < 1)")]
    InvalidProbability {
        /// Rejected value.
        p: f64,
    },

    /// The estimator did not converge.
    #[error("estimation did not converge: {detail}")]
    NotConverged {
        /// Failure context.
        detail: String,
    },

    /// CSV read/write error.
    #[error("CSV: {0}")]
    Csv(String),

    /// Project file error.
    #[error("project: {0}")]
    Project(String),

    /// Requested column does not exist.
    #[error("column not found: {0}")]
    ColumnNotFound(String),

    /// A worksheet cell could not be changed.
    #[error("cell {column}{row}: {message}")]
    Cell {
        /// Column name.
        column: String,
        /// One-based row number, as shown in the worksheet.
        row: usize,
        /// What the user has to fix.
        message: String,
    },

    /// The worksheet has no floating-point column to analyse.
    #[error("no numeric column found")]
    NoNumericColumn,

    /// Option is not implemented for this analysis.
    #[error("{0}")]
    Unsupported(String),
}

/// Engine result.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_has_message_with_context() {
        let e = Error::SampleTooSmall { n: 1, min: 2 };
        let msg = e.to_string();
        assert!(msg.contains("n=1"), "message without n: {msg}");
        assert!(msg.contains("minimum=2"), "message without minimum: {msg}");
    }

    #[test]
    fn errors_are_comparable_and_clonable() {
        let a = Error::EmptySample;
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn an_unsupported_error_shows_only_its_reason() {
        let error = Error::Unsupported("division by zero".to_owned());
        assert_eq!(error.to_string(), "division by zero");
    }
}
