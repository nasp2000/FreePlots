// SPDX-License-Identifier: MIT
//! Student t tests: one sample, two samples (Welch or pooled) and paired.
//!
//! The calculations use the sample standard deviation with divisor `n − 1`
//! and the Student t distribution. The p-value and interval follow the chosen
//! alternative; the conventions are standard but have not been taken from
//! the documented behaviour.

use crate::data::{Column, ColumnData, DataFrame, MissingPolicy, Sample};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{student_t_cdf, student_t_quantile};

/// Alternative hypothesis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alternative {
    /// Two-sided (`≠`).
    #[default]
    TwoSided,
    /// Lower-tailed (`<`).
    Less,
    /// Upper-tailed (`>`).
    Greater,
}

impl Alternative {
    /// Symbol used in reports.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Self::TwoSided => "≠",
            Self::Less => "<",
            Self::Greater => ">",
        }
    }

    /// Stable name for the CLI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TwoSided => "two-sided",
            Self::Less => "less",
            Self::Greater => "greater",
        }
    }

    /// Parses the CLI spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().replace('_', "-").as_str() {
            "two-sided" | "two" | "!=" | "neq" => Some(Self::TwoSided),
            "less" | "<" | "lower" => Some(Self::Less),
            "greater" | ">" | "upper" => Some(Self::Greater),
            _ => None,
        }
    }
}

/// Summary statistics of a sample, shared with the equivalence tests.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Summary {
    /// Number of observations.
    pub(crate) n: usize,
    /// Sample mean.
    pub(crate) mean: f64,
    /// Sample standard deviation (divisor `n − 1`).
    pub(crate) sd: f64,
    /// Standard error of the mean.
    pub(crate) se: f64,
}

impl Summary {
    /// Builds the summary from a sample.
    pub(crate) fn from_sample(sample: &Sample) -> Result<Self> {
        if sample.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 2,
            });
        }
        let mean = sample.mean()?;
        let sd = sample.sd()?;
        if !sd.is_finite() || sd <= 0.0 {
            return Err(Error::ConstantSample);
        }
        Ok(Self {
            n: sample.n(),
            mean,
            sd,
            se: sd / (sample.n() as f64).sqrt(),
        })
    }
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

fn p_value(t: f64, df: f64, alternative: Alternative) -> f64 {
    let cdf = student_t_cdf(t, df);
    match alternative {
        Alternative::TwoSided => (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0),
        Alternative::Less => cdf.clamp(0.0, 1.0),
        Alternative::Greater => (1.0 - cdf).clamp(0.0, 1.0),
    }
}

fn interval(
    estimate: f64,
    se: f64,
    df: f64,
    confidence: f64,
    alternative: Alternative,
) -> (Option<f64>, Option<f64>) {
    let alpha = 1.0 - confidence;
    match alternative {
        Alternative::TwoSided => {
            let q = student_t_quantile(1.0 - alpha / 2.0, df);
            (Some(estimate - q * se), Some(estimate + q * se))
        }
        Alternative::Less => (
            None,
            Some(estimate + student_t_quantile(1.0 - alpha, df) * se),
        ),
        Alternative::Greater => (
            Some(estimate - student_t_quantile(1.0 - alpha, df) * se),
            None,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn add_test(
    result: &mut AnalysisResult,
    estimate: f64,
    se: f64,
    df: f64,
    t: f64,
    p: f64,
    confidence: f64,
    alternative: Alternative,
) {
    let (lower, upper) = interval(estimate, se, df, confidence, alternative);
    result.statistics.push(("Estimate", estimate));
    result.statistics.push(("Standard error", se));
    result.statistics.push(("t", t));
    result.statistics.push(("df", df));
    result.statistics.push(("p-value", p));
    if let Some(lower) = lower {
        result.statistics.push((
            if alternative == Alternative::TwoSided {
                "CI lower"
            } else {
                "Lower bound"
            },
            lower,
        ));
    }
    if let Some(upper) = upper {
        result.statistics.push((
            if alternative == Alternative::TwoSided {
                "CI upper"
            } else {
                "Upper bound"
            },
            upper,
        ));
    }
    result.extra.push(Item::Table {
        headers: vec![
            "Estimate".to_owned(),
            "SE".to_owned(),
            "t".to_owned(),
            "df".to_owned(),
            "p".to_owned(),
            format!("{:.1}% interval", confidence * 100.0),
        ],
        rows: vec![vec![
            format!("{estimate:.6}"),
            format!("{se:.6}"),
            format!("{t:.6}"),
            format!("{df:.3}"),
            format!("{p:.6}"),
            match (lower, upper) {
                (Some(lower), Some(upper)) => format!("({lower:.6}, {upper:.6})"),
                (Some(lower), None) => format!("({lower:.6}, ∞)"),
                (None, Some(upper)) => format!("(−∞, {upper:.6})"),
                (None, None) => "—".to_owned(),
            },
        ]],
    });
}

fn method(alternative: Alternative, confidence: f64) -> String {
    format!(
        "Student t; {} alternative; {:.1}% {}",
        alternative.as_str(),
        confidence * 100.0,
        if alternative == Alternative::TwoSided {
            "confidence interval"
        } else {
            "bound"
        }
    )
}

fn base_result(title: String, method: String, n: usize, excluded: usize) -> AnalysisResult {
    let mut result = AnalysisResult::new(title, method);
    result.n = n;
    result.excluded = excluded;
    result.warnings = standard_warnings(n, excluded, 0);
    result
}

/// One-sample Student t test of `mean = null_mean`.
pub fn one_sample(
    df: &DataFrame,
    column_name: &str,
    null_mean: f64,
    alternative: Alternative,
    confidence: f64,
) -> Result<AnalysisResult> {
    validate_confidence(confidence)?;
    if !null_mean.is_finite() {
        return Err(Error::InvalidParameter {
            name: "null mean",
            value: null_mean,
        });
    }
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let summary = Summary::from_sample(&sample)?;
    let df_t = summary.n as f64 - 1.0;
    let t = (summary.mean - null_mean) / summary.se;
    let p = p_value(t, df_t, alternative);

    let mut result = base_result(
        format!("1-Sample t — column {column_name}"),
        method(alternative, confidence),
        summary.n,
        sample.excluded(),
    );
    result.statistics = vec![
        ("Mean", summary.mean),
        ("Standard deviation", summary.sd),
        ("SE mean", summary.se),
        ("Null mean", null_mean),
    ];
    add_test(
        &mut result,
        summary.mean,
        summary.se,
        df_t,
        t,
        p,
        confidence,
        alternative,
    );
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            format!(
                "H₀: µ = {null_mean}; H₁: µ {} {null_mean}",
                alternative.symbol()
            ),
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

/// Two-sample Student t test of `mean₁ − mean₂ = 0`.
pub fn two_sample(
    df: &DataFrame,
    first: &str,
    second: &str,
    pooled: bool,
    alternative: Alternative,
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
    let diff = a.mean - b.mean;
    let (se, df_t) = if pooled {
        let sp2 = ((a.n - 1) as f64 * a.sd * a.sd + (b.n - 1) as f64 * b.sd * b.sd)
            / (a.n + b.n - 2) as f64;
        let se = (sp2 * (1.0 / a.n as f64 + 1.0 / b.n as f64)).sqrt();
        (se, (a.n + b.n - 2) as f64)
    } else {
        let va = a.sd * a.sd / a.n as f64;
        let vb = b.sd * b.sd / b.n as f64;
        let se = (va + vb).sqrt();
        let df_t = (va + vb).powi(2) / (va * va / (a.n - 1) as f64 + vb * vb / (b.n - 1) as f64);
        (se, df_t)
    };
    if !se.is_finite() || se <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let t = diff / se;
    let p = p_value(t, df_t, alternative);

    let mut result = base_result(
        format!("2-Sample t — {first} vs {second}"),
        format!(
            "{}; {}",
            if pooled {
                "Pooled-variance Student t"
            } else {
                "Welch Student t"
            },
            method(alternative, confidence)
        ),
        a.n + b.n,
        sample1.excluded() + sample2.excluded(),
    );
    result.statistics = vec![
        ("Mean 1", a.mean),
        ("Standard deviation 1", a.sd),
        ("Mean 2", b.mean),
        ("Standard deviation 2", b.sd),
        ("Difference (1 − 2)", diff),
    ];
    add_test(&mut result, diff, se, df_t, t, p, confidence, alternative);
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
            format!("H₀: µ₁ − µ₂ = 0; H₁: µ₁ − µ₂ {} 0", alternative.symbol()),
        ),
        (
            "Variance".to_owned(),
            if pooled {
                "pooled variance, n₁ + n₂ − 2 df".to_owned()
            } else {
                "Welch–Satterthwaite df; no equal-variance assumption".to_owned()
            },
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

/// Paired Student t test of the row-wise differences.
pub fn paired(
    df: &DataFrame,
    first: &str,
    second: &str,
    alternative: Alternative,
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
    let df_t = summary.n as f64 - 1.0;
    let t = summary.mean / summary.se;
    let p = p_value(t, df_t, alternative);
    let excluded = rows.saturating_sub(summary.n);

    let mut result = base_result(
        format!("Paired t — {first} vs {second}"),
        method(alternative, confidence),
        summary.n,
        excluded,
    );
    result.statistics = vec![
        ("Mean difference", summary.mean),
        ("Standard deviation", summary.sd),
        ("SE mean", summary.se),
    ];
    add_test(
        &mut result,
        summary.mean,
        summary.se,
        df_t,
        t,
        p,
        confidence,
        alternative,
    );
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            format!(
                "H₀: µ_d = 0; H₁: µ_d {} 0; d = {first} − {second}",
                alternative.symbol()
            ),
        ),
        (
            "Pairs".to_owned(),
            format!(
                "{} complete pairs; rows with a missing or non-finite member are excluded",
                summary.n
            ),
        ),
        (
            "Estimator".to_owned(),
            "mean and sample standard deviation (n−1) of the differences".to_owned(),
        ),
    ];
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame(name: &str, values: Vec<f64>) -> DataFrame {
        DataFrame::new(vec![Column::new_f64(name, values)]).unwrap()
    }

    #[test]
    fn one_sample_t_matches_a_hand_calculation() {
        let df = frame("x", vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let result = one_sample(&df, "x", 0.0, Alternative::TwoSided, 0.95).expect("t test");
        let get = |name: &str| {
            result
                .statistics
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .unwrap()
        };
        assert!((get("t") - 4.242_640_687).abs() < 1e-6);
        assert!((get("df") - 4.0).abs() < 1e-12);
        assert!((get("p-value") - 0.013_235_7).abs() < 1e-5);
    }

    #[test]
    fn alternatives_change_the_p_value_and_bound() {
        let df = frame("x", vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let less = one_sample(&df, "x", 0.0, Alternative::Less, 0.95).expect("less");
        let greater = one_sample(&df, "x", 0.0, Alternative::Greater, 0.95).expect("greater");
        let p = |result: &AnalysisResult| {
            result
                .statistics
                .iter()
                .find(|(key, _)| *key == "p-value")
                .map(|(_, value)| *value)
                .unwrap()
        };
        assert!((p(&less) + p(&greater) - 1.0).abs() < 1e-12);
        assert!(less.statistics.iter().any(|(key, _)| *key == "Upper bound"));
        assert!(
            greater
                .statistics
                .iter()
                .any(|(key, _)| *key == "Lower bound")
        );
    }

    #[test]
    fn two_sample_welch_and_pooled_are_reported() {
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0]),
            Column::new_f64("b", vec![4.0, 5.0, 6.0]),
        ])
        .unwrap();
        let welch = two_sample(&df, "a", "b", false, Alternative::TwoSided, 0.95).expect("Welch");
        let pooled = two_sample(&df, "a", "b", true, Alternative::TwoSided, 0.95).expect("pooled");
        let value = |result: &AnalysisResult, name: &str| {
            result
                .statistics
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .unwrap()
        };
        assert!((value(&welch, "t") + 3.674_234_6).abs() < 1e-6);
        assert!((value(&welch, "df") - 4.0).abs() < 1e-9);
        assert!((value(&pooled, "df") - 4.0).abs() < 1e-12);
        assert!(welch.method.contains("Welch"));
        assert!(pooled.method.contains("Pooled"));
    }

    #[test]
    fn paired_test_uses_only_complete_rows() {
        let df = DataFrame::new(vec![
            Column::new_f64("before", vec![1.0, 2.0, 3.0, f64::NAN]),
            Column::new_f64("after", vec![2.0, 4.0, 6.0, 8.0]),
        ])
        .unwrap();
        let result = paired(&df, "before", "after", Alternative::TwoSided, 0.95).expect("paired");
        assert_eq!(result.n, 3);
        assert_eq!(result.excluded, 1);
        assert!(result.to_session_text().contains("Pairs"));
    }

    #[test]
    fn identical_columns_are_rejected() {
        let df = frame("x", vec![1.0, 2.0, 3.0]);
        assert!(matches!(
            two_sample(&df, "x", "x", false, Alternative::TwoSided, 0.95),
            Err(Error::Unsupported(_))
        ));
    }
}
