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
    tost_with_limits(
        estimate,
        standard_error,
        degrees_of_freedom,
        null_value - margin,
        null_value + margin,
        confidence,
    )
}

/// Runs the two one-sided tests against explicit equivalence limits.
///
/// `confidence` is the confidence level of the decision; the interval reported
/// is the `(1 − 2α)` interval that corresponds to it (90 % at the usual 5 %).
pub fn tost_with_limits(
    estimate: f64,
    standard_error: f64,
    degrees_of_freedom: f64,
    lower: f64,
    upper: f64,
    confidence: f64,
) -> Result<Equivalence> {
    if !lower.is_finite() || !upper.is_finite() || lower >= upper {
        return Err(Error::InvalidParameter {
            name: "equivalence limits",
            value: upper,
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

    let t_lower = (estimate - lower) / standard_error;
    let t_upper = (estimate - upper) / standard_error;
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
        equivalent: p_value < alpha && interval_lower > lower && interval_upper < upper,
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

/// One subject of a 2×2 crossover.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossoverSubject {
    /// Subject label.
    pub subject: String,
    /// Sequence label.
    pub sequence: String,
    /// Treatment of period 1 and of period 2.
    pub order: (String, String),
    /// Difference `test − reference` of the subject.
    pub difference: f64,
    /// Difference of period 2 minus period 1.
    pub period_difference: f64,
}

/// One row of the crossover analysis of variance.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossoverRow {
    /// Source name.
    pub source: String,
    /// Degrees of freedom.
    pub df: f64,
    /// Sum of squares.
    pub ss: f64,
    /// Mean square.
    pub ms: f64,
    /// F statistic (`NaN` where the row has no test).
    pub f: f64,
    /// p-value of the F statistic.
    pub p: f64,
}

/// Fitted 2×2 crossover with the equivalence test.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossoverFit {
    /// Subjects in label order.
    pub subjects: Vec<CrossoverSubject>,
    /// Sequence labels, sorted.
    pub sequences: Vec<String>,
    /// Subjects per sequence, in the order of `sequences`.
    pub counts: Vec<usize>,
    /// Treatment order of each sequence (period 1, period 2).
    pub orders: Vec<(String, String)>,
    /// Reference treatment (first label).
    pub reference: String,
    /// Test treatment (second label).
    pub test: String,
    /// Mean treatment difference `test − reference`.
    pub estimate: f64,
    /// Standard error of the estimate.
    pub standard_error: f64,
    /// Degrees of freedom of the residual (`n − 2`).
    pub df: f64,
    /// Two one-sided tests against the limits.
    pub equivalence: Equivalence,
    /// Analysis of variance rows.
    pub anova: Vec<CrossoverRow>,
    /// Lower equivalence limit.
    pub lower: f64,
    /// Upper equivalence limit.
    pub upper: f64,
    /// Rows used.
    pub n: usize,
    /// Rows excluded.
    pub excluded: usize,
}

/// One kept row of the crossover: period, treatment, response, sequence.
type CrossoverObservation = (String, String, f64, String);

/// Label of a cell of any column type.
fn label(column: &Column, row: usize) -> Option<String> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::Text(values) => values.get(row).cloned(),
        ColumnData::Date(values) => Some(crate::data::format_date(values[row])),
        ColumnData::I64(values) => values.get(row).map(ToString::to_string),
        ColumnData::F64(values) => values.get(row).map(ToString::to_string),
        ColumnData::Bool(values) => values
            .get(row)
            .map(|&value| if value { "T" } else { "F" }.to_owned()),
    }
}

/// Fits the 2×2 crossover and its equivalence test.
///
/// The worksheet is read in **long format**: one row per observation with the
/// response, the subject, the sequence, the period and the treatment. Every
/// subject must appear once per period, with the two treatments in the order
/// of its sequence, and the two sequences must run the treatments in opposite
/// orders.
///
/// The treatment difference `test − reference` uses the two treatments in
/// label order (the first is the reference). The estimate is the mean of the
/// two sequence means, the standard error comes from the residual of the
/// crossover and the test has `n − 2` degrees of freedom.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for limits that are not `lower < upper`
/// or a bad confidence level, [`Error::Unsupported`] for a missing or
/// inconsistent design, [`Error::SampleTooSmall`] for fewer than two subjects
/// per sequence and [`Error::ConstantSample`] when the differences carry no
/// variance.
#[allow(clippy::too_many_arguments)]
pub fn crossover_fit(
    df: &DataFrame,
    response: &str,
    subject: &str,
    sequence: &str,
    period: &str,
    treatment: &str,
    lower: f64,
    upper: f64,
    confidence: f64,
) -> Result<CrossoverFit> {
    validate_confidence(confidence)?;
    if !lower.is_finite() || !upper.is_finite() || lower >= upper {
        return Err(Error::InvalidParameter {
            name: "equivalence limits",
            value: upper,
        });
    }
    let names = [response, subject, sequence, period, treatment];
    let mut unique = names.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != names.len() {
        return Err(Error::Unsupported(
            "the five columns must be different".to_owned(),
        ));
    }
    let response_column = df.column(response)?;
    if matches!(
        response_column.data(),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
    ) {
        return Err(Error::Csv(format!(
            "response column {response} is not numeric"
        )));
    }
    let subject_column = df.column(subject)?;
    let sequence_column = df.column(sequence)?;
    let period_column = df.column(period)?;
    let treatment_column = df.column(treatment)?;

    let mut by_subject: std::collections::BTreeMap<String, Vec<CrossoverObservation>> =
        std::collections::BTreeMap::new();
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        let (Some(value), Some(who), Some(order), Some(when), Some(what)) = (
            cell(response_column, row),
            label(subject_column, row),
            label(sequence_column, row),
            label(period_column, row),
            label(treatment_column, row),
        ) else {
            excluded += 1;
            continue;
        };
        by_subject
            .entry(who)
            .or_default()
            .push((when, what, value, order));
    }
    if by_subject.is_empty() {
        return Err(Error::EmptySample);
    }
    let mut periods: Vec<String> = Vec::new();
    let mut treatments: Vec<String> = Vec::new();
    let mut sequences: Vec<String> = Vec::new();
    for rows in by_subject.values() {
        for (when, what, _, order) in rows {
            if !periods.contains(when) {
                periods.push(when.clone());
            }
            if !treatments.contains(what) {
                treatments.push(what.clone());
            }
            if !sequences.contains(order) {
                sequences.push(order.clone());
            }
        }
    }
    periods.sort();
    treatments.sort();
    sequences.sort();
    if periods.len() != 2 {
        return Err(Error::Unsupported(
            "a 2×2 crossover needs exactly two period labels".to_owned(),
        ));
    }
    if treatments.len() != 2 {
        return Err(Error::Unsupported(
            "a 2×2 crossover needs exactly two treatment labels".to_owned(),
        ));
    }
    if sequences.len() != 2 {
        return Err(Error::Unsupported(
            "a 2×2 crossover needs exactly two sequence labels".to_owned(),
        ));
    }
    let reference = treatments[0].clone();
    let test = treatments[1].clone();

    let mut subjects: Vec<CrossoverSubject> = Vec::with_capacity(by_subject.len());
    let mut orders: Vec<Option<(String, String)>> = vec![None; sequences.len()];
    let mut counts = vec![0usize; sequences.len()];
    for (who, rows) in &by_subject {
        if rows.len() != 2 {
            return Err(Error::Unsupported(format!(
                "subject {who} has {} observations: the crossover needs one per period",
                rows.len()
            )));
        }
        if rows[0].0 == rows[1].0 {
            return Err(Error::Unsupported(format!(
                "subject {who} repeats period {}",
                rows[0].0
            )));
        }
        if rows[0].3 != rows[1].3 {
            return Err(Error::Unsupported(format!(
                "subject {who} belongs to two sequences"
            )));
        }
        if rows[0].1 == rows[1].1 {
            return Err(Error::Unsupported(format!(
                "subject {who} has treatment {} in both periods",
                rows[0].1
            )));
        }
        let first = rows
            .iter()
            .find(|row| row.0 == periods[0])
            .ok_or_else(|| Error::Unsupported(format!("subject {who} misses period 1")))?;
        let second = rows
            .iter()
            .find(|row| row.0 == periods[1])
            .ok_or_else(|| Error::Unsupported(format!("subject {who} misses period 2")))?;
        let order = sequences
            .iter()
            .position(|value| *value == rows[0].3)
            .expect("sequence was collected");
        match &orders[order] {
            Some(found) if *found != (first.1.clone(), second.1.clone()) => {
                return Err(Error::Unsupported(format!(
                    "sequence {} mixes the treatment orders",
                    sequences[order]
                )));
            }
            Some(_) => {}
            None => orders[order] = Some((first.1.clone(), second.1.clone())),
        }
        counts[order] += 1;
        let value_of = |name: &str| -> f64 {
            rows.iter()
                .find(|row| row.1 == name)
                .map(|row| row.2)
                .unwrap_or(f64::NAN)
        };
        subjects.push(CrossoverSubject {
            subject: who.clone(),
            sequence: rows[0].3.clone(),
            order: (first.1.clone(), second.1.clone()),
            difference: value_of(&test) - value_of(&reference),
            period_difference: second.2 - first.2,
        });
    }
    let orders: Vec<(String, String)> = orders.into_iter().map(|o| o.expect("complete")).collect();
    if orders.len() != 2 || orders[0].0 != orders[1].1 || orders[0].1 != orders[1].0 {
        return Err(Error::Unsupported(
            "the two sequences must run the treatments in opposite order".to_owned(),
        ));
    }
    if counts.iter().any(|count| *count < 2) {
        return Err(Error::SampleTooSmall {
            n: counts.iter().copied().min().unwrap_or(0),
            min: 2,
        });
    }

    let n = subjects.len();
    let df_residual = (n - 2) as f64;
    let sequence_means: Vec<f64> = sequences
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let values: Vec<f64> = subjects
                .iter()
                .filter(|s| s.sequence == sequences[index])
                .map(|s| s.difference)
                .collect();
            values.iter().sum::<f64>() / values.len() as f64
        })
        .collect();
    let estimate = sequence_means.iter().sum::<f64>() / 2.0;
    let pooled: f64 = sequences
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let mean = sequence_means[index];
            subjects
                .iter()
                .filter(|s| s.sequence == sequences[index])
                .map(|s| (s.difference - mean).powi(2))
                .sum::<f64>()
        })
        .sum::<f64>()
        / df_residual;
    // Each subject difference has variance 2σ², so the residual mean square of
    // the crossover is half the pooled variance of the differences.
    let mse = pooled / 2.0;
    if mse <= 0.0 || mse.is_nan() {
        return Err(Error::ConstantSample);
    }
    let standard_error = (mse * (1.0 / counts[0] as f64 + 1.0 / counts[1] as f64) / 2.0).sqrt();
    let equivalence = tost_with_limits(
        estimate,
        standard_error,
        df_residual,
        lower,
        upper,
        confidence,
    )?;

    // Analysis of variance of the crossover: the between-subject part comes
    // from the subject sums and the within-subject part from the period
    // differences.
    let subject_sums: Vec<f64> = by_subject
        .values()
        .map(|rows| rows.iter().map(|row| row.2).sum::<f64>())
        .collect();
    let grand_mean = subject_sums.iter().sum::<f64>() / (2.0 * n as f64);
    let ss_between: f64 = subject_sums
        .iter()
        .map(|sum| (sum / 2.0 - grand_mean).powi(2) * 2.0)
        .sum();
    let mut ss_sequence = 0.0;
    for (index, _) in sequences.iter().enumerate() {
        let sums: Vec<f64> = by_subject
            .values()
            .zip(&subject_sums)
            .filter(|(rows, _)| rows[0].3 == sequences[index])
            .map(|(_, sum)| *sum)
            .collect();
        let mean = sums.iter().sum::<f64>() / sums.len() as f64;
        ss_sequence += sums.len() as f64 * (mean / 2.0 - grand_mean).powi(2) * 2.0;
    }
    let ss_subject = (ss_between - ss_sequence).max(0.0);
    let mean_period_difference =
        subjects.iter().map(|s| s.period_difference).sum::<f64>() / n as f64;
    // Sum of squares of the period differences around their mean: it holds
    // the treatment effect and the residual (the period effect is the mean).
    let ss_within_centered: f64 = subjects
        .iter()
        .map(|s| (s.period_difference - mean_period_difference).powi(2))
        .sum::<f64>()
        / 2.0;
    let ss_period = n as f64 * mean_period_difference.powi(2) / 2.0;
    // Treatment contrast: x = +1 when period 1 carries the reference.
    let x: Vec<f64> = subjects
        .iter()
        .map(|s| if s.order.0 == reference { 1.0 } else { -1.0 })
        .collect();
    let mean_x = x.iter().sum::<f64>() / n as f64;
    let sxx: f64 = x.iter().map(|value| (value - mean_x).powi(2)).sum();
    let sxy: f64 = subjects
        .iter()
        .zip(&x)
        .map(|(s, value)| (s.period_difference - mean_period_difference) * (value - mean_x))
        .sum();
    let ss_treatment = if sxx > 0.0 {
        sxy * sxy / sxx / 2.0
    } else {
        f64::NAN
    };
    let ss_residual = (ss_within_centered - ss_treatment).max(0.0);
    let mse_residual = ss_residual / df_residual;
    let make = |source: &str, df: f64, ss: f64, ms: f64, f: f64, p: f64| CrossoverRow {
        source: source.to_owned(),
        df,
        ss,
        ms,
        f,
        p,
    };
    let test_row = |ss: f64, df: f64, denominator: f64| -> (f64, f64) {
        let ms = if df > 0.0 { ss / df } else { f64::NAN };
        let f = if denominator > 0.0 && denominator.is_finite() {
            ms / denominator
        } else {
            f64::NAN
        };
        let p = if f.is_finite() {
            (1.0 - crate::special::f_cdf(f, df, df_residual)).clamp(0.0, 1.0)
        } else {
            f64::NAN
        };
        (f, p)
    };
    let ms_sequence = ss_sequence;
    let ms_subject = if df_residual > 0.0 {
        ss_subject / df_residual
    } else {
        f64::NAN
    };
    let (f_sequence, p_sequence) = test_row(ss_sequence, 1.0, ms_subject);
    let (f_period, p_period) = test_row(ss_period, 1.0, mse_residual);
    let (f_treatment, p_treatment) = test_row(ss_treatment, 1.0, mse_residual);
    let anova = vec![
        make(
            "Sequence (carryover)",
            1.0,
            ss_sequence,
            ms_sequence,
            f_sequence,
            p_sequence,
        ),
        make(
            "Subject(Sequence)",
            df_residual,
            ss_subject,
            ms_subject,
            f64::NAN,
            f64::NAN,
        ),
        make("Period", 1.0, ss_period, ss_period, f_period, p_period),
        make(
            "Treatment",
            1.0,
            ss_treatment,
            ss_treatment,
            f_treatment,
            p_treatment,
        ),
        make(
            "Residual",
            df_residual,
            ss_residual,
            mse_residual,
            f64::NAN,
            f64::NAN,
        ),
        make(
            "Total",
            2.0 * n as f64 - 1.0,
            ss_between + ss_within_centered + ss_period,
            f64::NAN,
            f64::NAN,
            f64::NAN,
        ),
    ];

    Ok(CrossoverFit {
        subjects,
        sequences,
        counts,
        orders,
        reference,
        test,
        estimate,
        standard_error,
        df: df_residual,
        equivalence,
        anova,
        lower,
        upper,
        n: 2 * n,
        excluded,
    })
}

/// Builds the report of the 2×2 crossover equivalence test.
///
/// # Errors
///
/// The errors of [`crossover_fit`].
#[allow(clippy::too_many_arguments)]
pub fn crossover(
    df: &DataFrame,
    response: &str,
    subject: &str,
    sequence: &str,
    period: &str,
    treatment: &str,
    lower: f64,
    upper: f64,
    confidence: f64,
) -> Result<AnalysisResult> {
    let fit = crossover_fit(
        df, response, subject, sequence, period, treatment, lower, upper, confidence,
    )?;
    let mut result = base_result(
        format!("2×2 Crossover Equivalence — response {response}"),
        confidence,
        fit.n,
        fit.excluded,
    );
    result.method = format!(
        "Two one-sided Student t tests (TOST) on the within-subject treatment difference; \
         {:.0}% confidence; equivalence interval at {:.0}%",
        confidence * 100.0,
        (2.0 * confidence - 1.0) * 100.0
    );
    result.statistics = vec![
        ("Estimate (Test − Reference)", fit.estimate),
        ("Standard error", fit.standard_error),
        ("df", fit.df),
        ("Lower limit", fit.lower),
        ("Upper limit", fit.upper),
        ("p-value (TOST)", fit.equivalence.p_value),
        ("Equivalence interval lower", fit.equivalence.interval_lower),
        ("Equivalence interval upper", fit.equivalence.interval_upper),
    ];
    for row in &fit.anova {
        match row.source.as_str() {
            "Sequence (carryover)" => result.statistics.push(("p (sequence)", row.p)),
            "Period" => result.statistics.push(("p (period)", row.p)),
            "Treatment" => result.statistics.push(("p (treatment)", row.p)),
            _ => {}
        }
    }
    result.extra.push(Item::Message(format!(
        "Treatment difference: {} − {} (the first label is the reference)",
        fit.test, fit.reference
    )));
    result.extra.push(Item::Table {
        headers: vec![
            "Source".to_owned(),
            "DF".to_owned(),
            "SS".to_owned(),
            "MS".to_owned(),
            "F".to_owned(),
            "P".to_owned(),
        ],
        rows: fit
            .anova
            .iter()
            .map(|row| {
                let number = |value: f64| {
                    if value.is_finite() {
                        format!("{value:.6}")
                    } else {
                        String::new()
                    }
                };
                vec![
                    row.source.clone(),
                    format!("{:.0}", row.df),
                    number(row.ss),
                    number(row.ms),
                    number(row.f),
                    number(row.p),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Sequence".to_owned(),
            "Order".to_owned(),
            "n".to_owned(),
            "Mean difference".to_owned(),
        ],
        rows: fit
            .sequences
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let mean = fit
                    .subjects
                    .iter()
                    .filter(|s| s.sequence == *name)
                    .map(|s| s.difference)
                    .sum::<f64>()
                    / fit.counts[index] as f64;
                vec![
                    name.clone(),
                    format!("{} → {}", fit.orders[index].0, fit.orders[index].1),
                    fit.counts[index].to_string(),
                    format!("{:+.6}", mean),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Subject".to_owned(),
            "Sequence".to_owned(),
            "Difference".to_owned(),
        ],
        rows: fit
            .subjects
            .iter()
            .take(crate::report::MAX_REPORT_ROWS)
            .map(|s| {
                vec![
                    s.subject.clone(),
                    s.sequence.clone(),
                    format!("{:+.6}", s.difference),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Lower limit".to_owned(),
            "Estimate".to_owned(),
            "Upper limit".to_owned(),
            "Interval".to_owned(),
            "p (TOST)".to_owned(),
            "Decision".to_owned(),
        ],
        rows: vec![vec![
            format!("{:.6}", fit.lower),
            format!("{:.6}", fit.estimate),
            format!("{:.6}", fit.upper),
            format!(
                "({:.6}, {:.6})",
                fit.equivalence.interval_lower, fit.equivalence.interval_upper
            ),
            format!("{:.6}", fit.equivalence.p_value),
            decision_text(&fit.equivalence, confidence),
        ]],
    });
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            format!(
                "H₀: the treatment difference is at or outside [{lower}, {upper}]; \
                 H₁: it is inside the limits"
            ),
        ),
        (
            "Estimator".to_owned(),
            "mean of the two sequence means of the within-subject difference (test − \
             reference); the standard error comes from the crossover residual with n − 2 \
             degrees of freedom"
                .to_owned(),
        ),
        (
            "Carryover".to_owned(),
            "with two periods the carryover cannot be estimated separately: the sequence \
             effect of the table is the test of differential carryover, and it is confounded \
             with a treatment × period interaction"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "one row per observation; rows with a missing value are excluded and counted; every \
             subject needs both periods and the two sequences must be complete"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(fit.n, fit.excluded, 0);
    if fit.subjects.len() < 8 {
        result.warnings.push(
            "fewer than eight subjects: the interval and the sequence test carry little \
             information"
                .to_owned(),
        );
    }
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

    fn crossover_frame(rows: &[(&str, &str, u8, &str, f64)]) -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("y", rows.iter().map(|row| row.4).collect()),
            Column::new_text("subject", rows.iter().map(|row| row.0.to_owned()).collect()),
            Column::new_text(
                "sequence",
                rows.iter().map(|row| row.1.to_owned()).collect(),
            ),
            Column::new_f64("period", rows.iter().map(|row| row.2 as f64).collect()),
            Column::new_text(
                "treatment",
                rows.iter().map(|row| row.3.to_owned()).collect(),
            ),
        ])
        .expect("frame")
    }

    /// Eight subjects, a period effect of one and a treatment effect of 0.2,
    /// with a small alternating noise so the residual is not degenerate.
    fn balanced_crossover() -> DataFrame {
        let noise = [0.02, -0.02, 0.02, -0.02];
        let mut rows: Vec<(String, String, u8, String, f64)> = Vec::new();
        for (index, offset) in noise.iter().enumerate() {
            let base = index as f64;
            rows.push((
                format!("S{}", index + 1),
                "TR".to_owned(),
                1,
                "T".to_owned(),
                base + 0.2 + offset,
            ));
            rows.push((
                format!("S{}", index + 1),
                "TR".to_owned(),
                2,
                "R".to_owned(),
                base + 1.0,
            ));
        }
        for (index, offset) in noise.iter().enumerate() {
            let base = index as f64;
            rows.push((
                format!("S{}", index + 5),
                "RT".to_owned(),
                1,
                "R".to_owned(),
                base,
            ));
            rows.push((
                format!("S{}", index + 5),
                "RT".to_owned(),
                2,
                "T".to_owned(),
                base + 1.0 + 0.2 + offset,
            ));
        }
        let borrowed: Vec<(&str, &str, u8, &str, f64)> = rows
            .iter()
            .map(|row| (row.0.as_str(), row.1.as_str(), row.2, row.3.as_str(), row.4))
            .collect();
        crossover_frame(&borrowed)
    }

    #[test]
    fn the_crossover_recovers_the_treatment_effect() {
        let df = balanced_crossover();
        let fit = crossover_fit(
            &df,
            "y",
            "subject",
            "sequence",
            "period",
            "treatment",
            -0.5,
            0.5,
            0.95,
        )
        .expect("crossover");
        assert_eq!(fit.reference, "R");
        assert_eq!(fit.test, "T");
        assert_eq!(fit.counts, vec![4, 4]);
        assert_eq!(fit.orders[0], ("R".to_owned(), "T".to_owned()));
        assert!((fit.estimate - 0.2).abs() < 1e-12, "{}", fit.estimate);
        assert_eq!(fit.df, 6.0);
        assert!(fit.equivalence.equivalent);
        assert!(fit.equivalence.p_value < 0.001);
        // The treatment F is the square of the t test.
        let treatment = fit
            .anova
            .iter()
            .find(|row| row.source == "Treatment")
            .expect("treatment row");
        let t = fit.estimate / fit.standard_error;
        assert!(
            (treatment.f - t * t).abs() < 1e-9,
            "{} {}",
            treatment.f,
            t * t
        );
        // The period effect of one is detected.
        let period = fit
            .anova
            .iter()
            .find(|row| row.source == "Period")
            .expect("period row");
        assert!(period.p < 0.001, "{}", period.p);
        // No carryover was simulated.
        let sequence = fit
            .anova
            .iter()
            .find(|row| row.source == "Sequence (carryover)")
            .expect("sequence row");
        assert!(sequence.p > 0.05, "{}", sequence.p);
        assert_eq!(fit.anova.len(), 6);
        let text = crossover(
            &df,
            "y",
            "subject",
            "sequence",
            "period",
            "treatment",
            -0.5,
            0.5,
            0.95,
        )
        .expect("report")
        .to_session_text();
        assert!(text.contains("2×2 Crossover Equivalence"), "{text}");
        assert!(text.contains("equivalent"), "{text}");
        assert!(text.contains("Subject(Sequence)"), "{text}");
    }

    #[test]
    fn the_crossover_weights_the_sequences_equally_and_detects_carryover() {
        // Three subjects in TR with a carryover of 0.5 (difference 0.7) and
        // five in RT without it (difference 0.2).
        let mut rows: Vec<(&str, &str, u8, &str, f64)> = Vec::new();
        let tr = [0.68, 0.70, 0.72];
        for (index, value) in tr.iter().enumerate() {
            let who = ["A1", "A2", "A3"][index];
            rows.push((who, "TR", 1, "T", *value));
            rows.push((who, "TR", 2, "R", 0.0));
        }
        let rt = [0.18, 0.20, 0.22, 0.20, 0.20];
        for (index, value) in rt.iter().enumerate() {
            let who = ["B1", "B2", "B3", "B4", "B5"][index];
            rows.push((who, "RT", 1, "R", 0.0));
            rows.push((who, "RT", 2, "T", *value));
        }
        let df = crossover_frame(&rows);
        let fit = crossover_fit(
            &df,
            "y",
            "subject",
            "sequence",
            "period",
            "treatment",
            -0.5,
            0.5,
            0.95,
        )
        .expect("crossover");
        assert_eq!(fit.counts, vec![5, 3]);
        // Equal weights: (0.7 + 0.2)/2, not (3·0.7 + 5·0.2)/8.
        assert!((fit.estimate - 0.45).abs() < 1e-12, "{}", fit.estimate);
        assert_eq!(fit.df, 6.0);
        let sequence = fit
            .anova
            .iter()
            .find(|row| row.source == "Sequence (carryover)")
            .expect("sequence row");
        assert!(sequence.p < 0.01, "{}", sequence.p);
    }

    #[test]
    fn the_crossover_rejects_broken_designs() {
        // A subject with a single period.
        let missing = crossover_frame(&[
            ("A1", "TR", 1, "T", 1.0),
            ("A1", "TR", 2, "R", 1.1),
            ("A2", "TR", 1, "T", 1.0),
            ("B1", "RT", 1, "R", 1.0),
            ("B1", "RT", 2, "T", 1.1),
            ("B2", "RT", 1, "R", 1.0),
            ("B2", "RT", 2, "T", 1.1),
        ]);
        assert!(
            crossover_fit(
                &missing,
                "y",
                "subject",
                "sequence",
                "period",
                "treatment",
                -1.0,
                1.0,
                0.95
            )
            .unwrap_err()
            .to_string()
            .contains("one per period")
        );
        // The same treatment in both periods.
        let repeated = crossover_frame(&[
            ("A1", "TR", 1, "T", 1.0),
            ("A1", "TR", 2, "T", 1.1),
            ("A2", "TR", 1, "T", 1.0),
            ("A2", "TR", 2, "R", 1.1),
            ("B1", "RT", 1, "R", 1.0),
            ("B1", "RT", 2, "T", 1.1),
            ("B2", "RT", 1, "R", 1.0),
            ("B2", "RT", 2, "T", 1.1),
        ]);
        assert!(
            crossover_fit(
                &repeated,
                "y",
                "subject",
                "sequence",
                "period",
                "treatment",
                -1.0,
                1.0,
                0.95
            )
            .unwrap_err()
            .to_string()
            .contains("both periods")
        );
        // Both sequences in the same order.
        let same = crossover_frame(&[
            ("A1", "TR", 1, "T", 1.0),
            ("A1", "TR", 2, "R", 1.1),
            ("A2", "TR", 1, "T", 1.0),
            ("A2", "TR", 2, "R", 1.1),
            ("B1", "RT", 1, "T", 1.0),
            ("B1", "RT", 2, "R", 1.1),
            ("B2", "RT", 1, "T", 1.0),
            ("B2", "RT", 2, "R", 1.2),
        ]);
        assert!(
            crossover_fit(
                &same,
                "y",
                "subject",
                "sequence",
                "period",
                "treatment",
                -1.0,
                1.0,
                0.95
            )
            .unwrap_err()
            .to_string()
            .contains("opposite")
        );
        // Impossible limits and a repeated column.
        let df = balanced_crossover();
        assert!(
            crossover_fit(
                &df,
                "y",
                "subject",
                "sequence",
                "period",
                "treatment",
                1.0,
                -1.0,
                0.95
            )
            .is_err()
        );
        assert!(
            crossover_fit(
                &df,
                "y",
                "y",
                "sequence",
                "period",
                "treatment",
                -1.0,
                1.0,
                0.95
            )
            .is_err()
        );
    }

    #[test]
    fn tost_with_limits_accepts_asymmetric_limits() {
        // Estimate 0.4 is inside [0.1, 0.6] but outside [-0.3, 0.3].
        let inside = tost_with_limits(0.4, 0.1, 20.0, 0.1, 0.6, 0.95).expect("tost");
        assert!(inside.equivalent, "{inside:?}");
        let outside = tost_with_limits(0.4, 0.1, 20.0, -0.3, 0.3, 0.95).expect("tost");
        assert!(!outside.equivalent, "{outside:?}");
        assert!(tost_with_limits(0.0, 0.1, 20.0, 1.0, 1.0, 0.95).is_err());
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
