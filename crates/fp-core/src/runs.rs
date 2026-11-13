// SPDX-License-Identifier: MIT
//! Wald–Wolfowitz runs test of randomness.
//!
//! The observations are read in worksheet order and classified as above the
//! comparison value `K` or below-or-equal to it. The test compares the number
//! of runs (maximal groups of the same side) with its expectation under
//! randomness: `E(R) = 2AB/N + 1` and
//! `Var(R) = 2AB(2AB − N)/(N²(N − 1))`, where `A` and `B` are the counts on
//! each side and `N = A + B`. The p-value is two-sided from the normal
//! approximation. `K` defaults to the mean of the observations.
//!
//! Ties (values equal to `K`) are counted on the below-or-equal side, as the
//! documented behaviour does. Missing values are removed in row order and
//! counted.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, standard_warnings};
use crate::special::normal_sf;

/// Outcome of the runs test.
#[derive(Debug, Clone, PartialEq)]
pub struct RunsTest {
    /// Observations used.
    pub n: usize,
    /// Rows removed because the value was missing.
    pub excluded: usize,
    /// Comparison value `K`.
    pub reference: f64,
    /// Observations strictly above `K`.
    pub above: usize,
    /// Observations below or equal to `K`.
    pub below_or_equal: usize,
    /// Observed number of runs.
    pub runs: usize,
    /// Expected number of runs under randomness.
    pub expected: f64,
    /// Variance of the number of runs under randomness.
    pub variance: f64,
    /// Standard-normal statistic.
    pub z: f64,
    /// Two-sided p-value from the normal approximation.
    pub p_value: f64,
}

/// Runs the Wald–Wolfowitz test on one column.
///
/// `reference` is the comparison value; when it is `None` the mean of the
/// observations is used.
///
/// # Errors
///
/// Returns [`Error::SampleTooSmall`] for fewer than two observations,
/// [`Error::InvalidParameter`] for a non-finite comparison value and
/// [`Error::Unsupported`] when every observation falls on the same side of the
/// comparison value.
pub fn test(df: &DataFrame, column_name: &str, reference: Option<f64>) -> Result<RunsTest> {
    if let Some(value) = reference
        && !value.is_finite()
    {
        return Err(Error::InvalidParameter {
            name: "comparison value",
            value,
        });
    }
    let column = df.column(column_name)?;
    let mut values = Vec::new();
    let mut excluded = 0;
    for row in 0..df.n_rows() {
        match column.value(row) {
            Some(value) => values.push(value),
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
    let reference = reference.unwrap_or_else(|| values.iter().sum::<f64>() / n as f64);
    let sides: Vec<bool> = values.iter().map(|value| *value > reference).collect();
    let above = sides.iter().filter(|above| **above).count();
    let below_or_equal = n - above;
    if above == 0 || below_or_equal == 0 {
        return Err(Error::Unsupported(format!(
            "the runs test needs observations on both sides of {reference:.6}"
        )));
    }
    let runs = 1 + sides.windows(2).filter(|pair| pair[0] != pair[1]).count();
    let expected = 2.0 * above as f64 * below_or_equal as f64 / n as f64 + 1.0;
    let product = 2.0 * above as f64 * below_or_equal as f64;
    let variance = product * (product - n as f64) / (n as f64 * n as f64 * (n as f64 - 1.0));
    if !variance.is_finite() || variance <= 0.0 {
        return Err(Error::Unsupported(
            "the runs test has no variation with these observations".to_owned(),
        ));
    }
    let z = (runs as f64 - expected) / variance.sqrt();
    let p_value = (2.0 * normal_sf(z.abs())).clamp(0.0, 1.0);
    Ok(RunsTest {
        n,
        excluded,
        reference,
        above,
        below_or_equal,
        runs,
        expected,
        variance,
        z,
        p_value,
    })
}

/// Runs the test and builds the report.
pub fn analyze(
    df: &DataFrame,
    column_name: &str,
    reference: Option<f64>,
) -> Result<AnalysisResult> {
    let result = test(df, column_name, reference)?;
    let mut report = AnalysisResult::new(
        format!("Runs Test — column {column_name}"),
        format!(
            "Wald–Wolfowitz runs test about K = {:.6}; normal approximation",
            result.reference
        ),
    );
    report.n = result.n;
    report.excluded = result.excluded;
    report.statistics = vec![
        ("Comparison value (K)", result.reference),
        ("Observations above K", result.above as f64),
        (
            "Observations below or equal to K",
            result.below_or_equal as f64,
        ),
        ("Observations", result.n as f64),
        ("Observed runs", result.runs as f64),
        ("Expected runs", result.expected),
        ("Variance of runs", result.variance),
        ("Z", result.z),
        ("p-value", result.p_value),
    ];
    report.notes = vec![
        (
            "Criterion".to_owned(),
            "K is the mean of the observations unless a comparison value is given".to_owned(),
        ),
        (
            "Ties".to_owned(),
            "values equal to K are counted on the below-or-equal side".to_owned(),
        ),
        (
            "Test".to_owned(),
            "two-sided normal approximation; it is reliable with at least 11 observations on \
             each side"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing value are removed in row order and counted".to_owned(),
        ),
    ];
    report.warnings = standard_warnings(report.n, report.excluded, 0);
    if result.above < 11 || result.below_or_equal < 11 {
        report.warnings.push(
            "the normal approximation is unreliable with fewer than 11 observations on one \
             side"
                .to_owned(),
        );
    }
    if result.excluded > 0 {
        report.warnings.push(
            "missing values were removed; the runs are counted on the remaining sequence"
                .to_owned(),
        );
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame(values: Vec<f64>) -> DataFrame {
        DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame")
    }

    #[test]
    fn the_die_example_matches_the_published_run_count() {
        let values = vec![
            4.0, 6.0, 3.0, 2.0, 2.0, 1.0, 6.0, 4.0, 4.0, 5.0, 1.0, 1.0, 4.0, 5.0, 3.0, 1.0, 5.0,
            6.0, 6.0, 3.0, 4.0, 4.0, 5.0, 2.0, 3.0,
        ];
        let result = test(&frame(values), "x", Some(3.5)).expect("runs test");
        assert_eq!(result.n, 25);
        assert_eq!(result.above, 14);
        assert_eq!(result.below_or_equal, 11);
        assert_eq!(result.runs, 10);
        assert!((result.expected - 13.32).abs() < 1e-12);
        assert!((result.variance - 5.810_933_333_333_334).abs() < 1e-12);
        assert!((result.z + 1.377_257_472_002_954).abs() < 1e-12);
        assert!((result.p_value - 0.168_432_658_683_235_76).abs() < 1e-12);
        let text = analyze(&frame(values), "x", Some(3.5))
            .expect("analysis")
            .to_session_text();
        assert!(text.contains("Runs Test"), "{text}");
        assert!(text.contains("Observed runs"), "{text}");
    }

    #[test]
    fn the_mean_is_the_default_comparison_value() {
        let values: Vec<f64> = vec![
            4.0, 6.0, 3.0, 2.0, 2.0, 1.0, 6.0, 4.0, 4.0, 5.0, 1.0, 1.0, 4.0, 5.0, 3.0, 1.0, 5.0,
            6.0, 6.0, 3.0, 4.0, 4.0, 5.0, 2.0, 3.0,
        ];
        let result = test(&frame(values), "x", None).expect("runs test");
        assert!((result.reference - 3.6).abs() < 1e-12);
        assert_eq!(result.runs, 10);
    }

    #[test]
    fn an_alternating_sequence_is_not_random() {
        let values: Vec<f64> = (0..20)
            .map(|index| if index % 2 == 0 { 1.0 } else { 10.0 })
            .collect();
        let result = test(&frame(values), "x", None).expect("runs test");
        assert_eq!(result.runs, 20);
        assert!(result.p_value < 0.001, "p = {}", result.p_value);
        assert!(result.z > 0.0);
    }

    #[test]
    fn one_sided_values_and_missing_rows_are_handled() {
        let error = test(&frame(vec![2.0, 2.0, 2.0]), "x", None).expect_err("all on one side");
        assert!(error.to_string().contains("both sides"), "{error}");
        let error = test(&frame(vec![2.0]), "x", None).expect_err("single value");
        assert!(error.to_string().contains("minimum=2"), "{error}");
        let error = test(&frame(vec![1.0, 2.0, 3.0]), "x", Some(f64::NAN))
            .expect_err("non-finite comparison");
        assert!(error.to_string().contains("comparison value"), "{error}");
        let values = vec![1.0, f64::NAN, 10.0, 1.0, 10.0, 1.0];
        let result = test(&frame(values), "x", Some(5.5)).expect("runs test");
        assert_eq!(result.n, 5);
        assert_eq!(result.excluded, 1);
        assert_eq!(result.runs, 5);
    }
}
