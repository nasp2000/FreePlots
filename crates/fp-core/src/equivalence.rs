// SPDX-License-Identifier: MIT
//! Equivalence tests: two one-sided tests (TOST) for practical equivalence.
//!
//! A difference test answers "is the effect different from zero?". An
//! equivalence test answers the complementary question: "is the effect
//! *practically* equivalent to the reference within a margin `δ`?".
//!
//! The hypotheses are `H₀: |µ − µ₀| ≥ δ` (not equivalent) against
//! `H₁: |µ − µ₀| < δ` (equivalent). Schuirmann's procedure runs two
//! one-sided Student t tests — one against the lower margin `µ₀ − δ` and one
//! against the upper margin `µ₀ + δ` — and rejects `H₀` when the larger of
//! the two one-sided p-values falls below `α`. The reported interval is the
//! `(1 − 2α)` interval, which is inside the margin exactly when the test
//! rejects.
//!
//! The documented behaviour has no equivalence-test path, so this is an
//! addition of this engine; see the project notes.

use crate::data::{Column, ColumnData, DataFrame, MissingPolicy, Sample};
use crate::error::{Error, Result};
use crate::hypothesis::Summary;
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{student_t_cdf, student_t_quantile};

/// Outcome of a two one-sided test (TOST).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Equivalence {
    /// Point estimate of the effect.
    pub estimate: f64,
    /// Standard error of the estimate.
    pub standard_error: f64,
    /// Degrees of freedom of the Student t reference distribution.
    pub degrees_of_freedom: f64,
    /// p-value of the lower one-sided test (against `µ₀ − δ`).
    pub p_lower: f64,
    /// p-value of the upper one-sided test (against `µ₀ + δ`).
    pub p_upper: f64,
    /// TOST p-value: the larger of the two one-sided p-values.
    pub p_value: f64,
    /// Lower end of the `(1 − 2α)` equivalence interval.
    pub interval_lower: f64,
    /// Upper end of the `(1 − 2α)` equivalence interval.
    pub interval_upper: f64,
    /// `true` when the interval lies inside the equivalence margin.
    pub equivalent: bool,
}

/// Runs the two one-sided tests for an estimate with a standard error.
///
/// `confidence` is the confidence level of the *decision*; the interval
/// reported is the `(1 − 2α)` interval that corresponds to it.
pub fn tost(
    estimate: f64,
    standard_error: f64,
    degrees_of_freedom: f64,
    null_value: f64,
    margin: f64,
    confidence: f64,
) -> Result<Equivalence> {
    if !margin.is_finite() || margin <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "equivalence margin",
            value: margin,
        });
    }
    if !null_value.is_finite() {
        return Err(Error::InvalidParameter {
            name: "null value",
            value: null_value,
        });
    }
    if !standard_error.is_finite() || standard_error <= 0.0 {
        return Err(Error::ConstantSample);
    }
    if !degrees_of_freedom.is_finite() || degrees_of_freedom <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "degrees of freedom",
            value: degrees_of_freedom,
        });
    }
    validate_confidence(confidence)?;
    let alpha = 1.0 - confidence;

    let t_lower = (estimate - (null_value - margin)) / standard_error;
    let t_upper = (estimate - (null_value + margin)) / standard_error;
    let p_lower = (1.0 - student_t_cdf(t_lower, degrees_of_freedom)).clamp(0.0, 1.0);
    let p_upper = student_t_cdf(t_upper, degrees_of_freedom).clamp(0.0, 1.0);
    let p_value = p_lower.max(p_upper);

    let quantile = student_t_quantile(1.0 - alpha, degrees_of_freedom);
    let half_width = quantile * standard_error;
    let interval_lower = estimate - half_width;
    let interval_upper = estimate + half_width;

    Ok(Equivalence {
        estimate,
        standard_error,
        degrees_of_freedom,
        p_lower,
        p_upper,
        p_value,
        interval_lower,
        interval_upper,
        equivalent: p_value < alpha
            && interval_lower > null_value - margin
            && interval_upper < null_value + margin,
    })
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

fn decision_text(equivalence: &Equivalence, confidence: f64) -> String {
    if equivalence.equivalent {
        format!(
            "equivalent at {:.0}% confidence (the interval is inside the margin)",
            confidence * 100.0
        )
    } else {
        format!(
            "not equivalent at {:.0}% confidence (the interval leaves the margin)",
            confidence * 100.0
        )
    }
}

fn method(confidence: f64) -> String {
    format!(
        "Two one-sided Student t tests (TOST); {:.0}% confidence; equivalence interval at \
         {:.0}%",
        confidence * 100.0,
        (2.0 * confidence - 1.0) * 100.0
    )
}

fn add_result(
    result: &mut AnalysisResult,
    equivalence: &Equivalence,
    null_value: f64,
    margin: f64,
    confidence: f64,
) {
    result.statistics.push(("Estimate", equivalence.estimate));
    result
        .statistics
        .push(("Standard error", equivalence.standard_error));
    result
        .statistics
        .push(("df", equivalence.degrees_of_freedom));
    result
        .statistics
        .push(("p-value (lower)", equivalence.p_lower));
    result
        .statistics
        .push(("p-value (upper)", equivalence.p_upper));
    result
        .statistics
        .push(("p-value (TOST)", equivalence.p_value));
    result
        .statistics
        .push(("Equivalence interval lower", equivalence.interval_lower));
    result
        .statistics
        .push(("Equivalence interval upper", equivalence.interval_upper));
    result.extra.push(Item::Table {
        headers: vec![
            "Lower margin".to_owned(),
            "Estimate".to_owned(),
            "Upper margin".to_owned(),
            "Interval".to_owned(),
            "p (TOST)".to_owned(),
            "Decision".to_owned(),
        ],
        rows: vec![vec![
            format!("{:.6}", null_value - margin),
            format!("{:.6}", equivalence.estimate),
            format!("{:.6}", null_value + margin),
            format!(
                "({:.6}, {:.6})",
                equivalence.interval_lower, equivalence.interval_upper
            ),
            format!("{:.6}", equivalence.p_value),
            decision_text(equivalence, confidence),
        ]],
    });
}

fn base_result(title: String, confidence: f64, n: usize, excluded: usize) -> AnalysisResult {
    let mut result = AnalysisResult::new(title, method(confidence));
    result.n = n;
    result.excluded = excluded;
    result.warnings = standard_warnings(n, excluded, 0);
    result
}

/// One-sample equivalence test of `mean` against `null_mean` within `margin`.
pub fn one_sample(
    df: &DataFrame,
    column_name: &str,
    null_mean: f64,
    margin: f64,
    confidence: f64,
) -> Result<AnalysisResult> {
    validate_confidence(confidence)?;
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let summary = Summary::from_sample(&sample)?;
    let degrees_of_freedom = summary.n as f64 - 1.0;
    let equivalence = tost(
        summary.mean,
        summary.se,
        degrees_of_freedom,
        null_mean,
        margin,
        confidence,
    )?;

    let mut result = base_result(
        format!("Equivalence Test: 1-Sample — column {column_name}"),
        confidence,
        summary.n,
        sample.excluded(),
    );
    result.statistics = vec![
        ("Mean", summary.mean),
        ("Standard deviation", summary.sd),
        ("SE mean", summary.se),
        ("Null mean", null_mean),
        ("Equivalence margin", margin),
    ];
    add_result(&mut result, &equivalence, null_mean, margin, confidence);
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            format!("H₀: |µ − {null_mean}| ≥ {margin}; H₁: |µ − {null_mean}| < {margin}"),
        ),
        (
            "Method".to_owned(),
            "two one-sided Student t tests (TOST); the reported interval is the \
             (1 − 2α) interval"
                .to_owned(),
        ),
        (
            "Estimator".to_owned(),
            "mean and sample standard deviation (n−1); Student t with n−1 df".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted".to_owned(),
        ),
    ];
    Ok(result)
}

/// Two-sample equivalence test of `mean₁ − mean₂` within `margin` (Welch).
pub fn two_sample(
    df: &DataFrame,
    first: &str,
    second: &str,
    margin: f64,
    confidence: f64,
) -> Result<AnalysisResult> {
    validate_confidence(confidence)?;
    if first == second {
        return Err(Error::Unsupported(
            "the two samples must be different columns".to_owned(),
        ));
    }
    let sample1 = df.column(first)?.sample(MissingPolicy::Drop)?;
    let sample2 = df.column(second)?.sample(MissingPolicy::Drop)?;
    let a = Summary::from_sample(&sample1)?;
    let b = Summary::from_sample(&sample2)?;
    let difference = a.mean - b.mean;
    let va = a.sd * a.sd / a.n as f64;
    let vb = b.sd * b.sd / b.n as f64;
    let standard_error = (va + vb).sqrt();
    let degrees_of_freedom =
        (va + vb).powi(2) / (va * va / (a.n - 1) as f64 + vb * vb / (b.n - 1) as f64);
    let equivalence = tost(
        difference,
        standard_error,
        degrees_of_freedom,
        0.0,
        margin,
        confidence,
    )?;

    let mut result = base_result(
        format!("Equivalence Test: 2-Sample — {first} vs {second}"),
        confidence,
        a.n + b.n,
        sample1.excluded() + sample2.excluded(),
    );
    result.statistics = vec![
        ("Mean 1", a.mean),
        ("Standard deviation 1", a.sd),
        ("Mean 2", b.mean),
        ("Standard deviation 2", b.sd),
        ("Difference (1 − 2)", difference),
        ("Equivalence margin", margin),
    ];
    add_result(&mut result, &equivalence, 0.0, margin, confidence);
    result.extra.insert(
        0,
        Item::Table {
            headers: vec![
                "Sample".to_owned(),
                "n".to_owned(),
                "Mean".to_owned(),
                "StDev".to_owned(),
                "SE Mean".to_owned(),
            ],
            rows: vec![
                vec![
                    first.to_owned(),
                    a.n.to_string(),
                    format!("{:.6}", a.mean),
                    format!("{:.6}", a.sd),
                    format!("{:.6}", a.se),
                ],
                vec![
                    second.to_owned(),
                    b.n.to_string(),
                    format!("{:.6}", b.mean),
                    format!("{:.6}", b.sd),
                    format!("{:.6}", b.se),
                ],
            ],
        },
    );
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            format!("H₀: |µ₁ − µ₂| ≥ {margin}; H₁: |µ₁ − µ₂| < {margin}"),
        ),
        (
            "Method".to_owned(),
            "two one-sided Student t tests (TOST) on the difference of means; the reported \
             interval is the (1 − 2α) interval"
                .to_owned(),
        ),
        (
            "Variance".to_owned(),
            "Welch–Satterthwaite df; no equal-variance assumption".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "each column filtered independently; non-finite values excluded".to_owned(),
        ),
    ];
    Ok(result)
}

fn cell(column: &Column, row: usize) -> Option<f64> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => values.get(row).copied().filter(|value| value.is_finite()),
        ColumnData::I64(values) => values.get(row).map(|&value| value as f64),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_) => None,
    }
}

/// Paired equivalence test of the row-wise mean difference within `margin`.
pub fn paired(
    df: &DataFrame,
    first: &str,
    second: &str,
    margin: f64,
    confidence: f64,
) -> Result<AnalysisResult> {
    validate_confidence(confidence)?;
    if first == second {
        return Err(Error::Unsupported(
            "the two samples must be different columns".to_owned(),
        ));
    }
    let a = df.column(first)?;
    let b = df.column(second)?;
    let mut differences = Vec::new();
    let rows = df.n_rows();
    for row in 0..rows {
        if let (Some(x), Some(y)) = (cell(a, row), cell(b, row)) {
            differences.push(x - y);
        }
    }
    if differences.len() < 2 {
        return Err(Error::SampleTooSmall {
            n: differences.len(),
            min: 2,
        });
    }
    let sample = Sample::new(differences, MissingPolicy::Drop)?;
    let summary = Summary::from_sample(&sample)?;
    let degrees_of_freedom = summary.n as f64 - 1.0;
    let equivalence = tost(
        summary.mean,
        summary.se,
        degrees_of_freedom,
        0.0,
        margin,
        confidence,
    )?;
    let excluded = rows.saturating_sub(summary.n);

    let mut result = base_result(
        format!("Equivalence Test: Paired — {first} vs {second}"),
        confidence,
        summary.n,
        excluded,
    );
    result.statistics = vec![
        ("Mean difference", summary.mean),
        ("Standard deviation", summary.sd),
        ("SE mean", summary.se),
        ("Equivalence margin", margin),
    ];
    add_result(&mut result, &equivalence, 0.0, margin, confidence);
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            format!("H₀: |µ_d| ≥ {margin}; H₁: |µ_d| < {margin}; d = {first} − {second}"),
        ),
        (
            "Method".to_owned(),
            "two one-sided Student t tests (TOST) on the mean difference; the reported interval \
             is the (1 − 2α) interval"
                .to_owned(),
        ),
        (
            "Pairs".to_owned(),
            format!(
                "{} complete pairs; rows with a missing or non-finite member are excluded",
                summary.n
            ),
        ),
    ];
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame(values: Vec<f64>) -> DataFrame {
        DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame")
    }

    #[test]
    fn tost_declares_equivalence_when_the_interval_is_inside_the_margin() {
        // Estimate 0.05 with SE 0.1 and a margin of 0.5: the interval is
        // inside the margin, so the effect is practically equivalent to 0.
        let result = tost(0.05, 0.1, 20.0, 0.0, 0.5, 0.95).expect("tost");
        assert!(result.equivalent, "{result:?}");
        assert!(result.p_value < 0.05);
        assert!(result.interval_lower > -0.5 && result.interval_upper < 0.5);
    }

    #[test]
    fn tost_rejects_equivalence_when_the_interval_leaves_the_margin() {
        // A wide margin around a difference that is far from zero.
        let result = tost(2.0, 0.1, 20.0, 0.0, 0.5, 0.95).expect("tost");
        assert!(!result.equivalent, "{result:?}");
        assert!(result.p_value > 0.05);
    }

    #[test]
    fn tost_rejects_a_non_positive_margin() {
        assert!(matches!(
            tost(0.0, 1.0, 10.0, 0.0, 0.0, 0.95),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            tost(0.0, 1.0, 10.0, 0.0, -1.0, 0.95),
            Err(Error::InvalidParameter { .. })
        ));
    }

    #[test]
    fn one_sample_equivalence_reports_the_decision() {
        let values: Vec<f64> = (1..=30)
            .map(|i| 10.0 + if i % 2 == 0 { 0.02 } else { -0.02 })
            .collect();
        let df = frame(values);
        let result = one_sample(&df, "x", 10.0, 0.5, 0.95).expect("analysis");
        let text = result.to_session_text();
        assert!(text.contains("Equivalence Test: 1-Sample"), "{text}");
        assert!(text.contains("p-value (TOST)"), "{text}");
        assert!(text.contains("equivalent"), "{text}");
    }

    #[test]
    fn two_sample_equivalence_reports_the_decision() {
        let first: Vec<f64> = (1..=20).map(|i| 5.0 + i as f64 * 0.01).collect();
        let second: Vec<f64> = (1..=20).map(|i| 5.05 + i as f64 * 0.01).collect();
        let df = DataFrame::new(vec![
            Column::new_f64("a", first),
            Column::new_f64("b", second),
        ])
        .expect("frame");
        let result = two_sample(&df, "a", "b", 0.5, 0.95).expect("analysis");
        let text = result.to_session_text();
        assert!(text.contains("Equivalence Test: 2-Sample"), "{text}");
        assert!(text.contains("Welch"), "{text}");
    }

    #[test]
    fn paired_equivalence_uses_the_row_wise_differences() {
        let first: Vec<f64> = (1..=20).map(|i| 5.0 + i as f64 * 0.01).collect();
        let second: Vec<f64> = (1..=20)
            .map(|i| 5.02 + i as f64 * 0.01 + (i % 3) as f64 * 0.004)
            .collect();
        let df = DataFrame::new(vec![
            Column::new_f64("a", first),
            Column::new_f64("b", second),
        ])
        .expect("frame");
        let result = paired(&df, "a", "b", 0.5, 0.95).expect("analysis");
        let text = result.to_session_text();
        assert!(text.contains("Equivalence Test: Paired"), "{text}");
        assert!(text.contains("20 complete pairs"), "{text}");
    }
}
