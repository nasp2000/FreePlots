// SPDX-License-Identifier: MIT
//! One- and two-sample proportion tests.
//!
//! The one-sample test offers the exact binomial p-value and the normal
//! approximation; its interval is Clopper–Pearson for the exact method and
//! Wilson for the normal method. The two-sample test uses the pooled normal
//! approximation. These are standard conventions, not yet taken from the
//! documented behaviour.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::hypothesis::Alternative;
use crate::report::{AnalysisResult, Item};
use crate::special::{beta_quantile, ln_gamma, normal_quantile, normal_sf};

/// Method for the one-sample proportion test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProportionMethod {
    /// Exact binomial test and Clopper–Pearson interval.
    #[default]
    Exact,
    /// Normal approximation and Wilson interval.
    Normal,
}

impl ProportionMethod {
    /// Stable name for the CLI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Normal => "normal",
        }
    }

    /// Parses the CLI spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "exact" | "binomial" => Some(Self::Exact),
            "normal" | "approx" | "approximation" => Some(Self::Normal),
            _ => None,
        }
    }
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

fn counts(df: &DataFrame, column_name: &str, success: f64) -> Result<(usize, usize, usize)> {
    if success != 0.0 && success != 1.0 {
        return Err(Error::InvalidParameter {
            name: "success value",
            value: success,
        });
    }
    let column = df.column(column_name)?;
    let mut successes = 0usize;
    let mut failures = 0usize;
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        match cell(column, row) {
            Some(value) if value == success => successes += 1,
            Some(value) if value == 0.0 || value == 1.0 => failures += 1,
            Some(_) => {
                return Err(Error::Cell {
                    column: column_name.to_owned(),
                    row: row + 1,
                    message: "proportion data must contain only 0 and 1".to_owned(),
                });
            }
            None => excluded += 1,
        }
    }
    Ok((successes, failures, excluded))
}

fn validate_p(p: f64, name: &'static str) -> Result<()> {
    if !p.is_finite() || !(0.0..=1.0).contains(&p) {
        return Err(Error::InvalidParameter { name, value: p });
    }
    Ok(())
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

fn binom_ln_pmf(n: usize, k: usize, p: f64) -> f64 {
    let nf = n as f64;
    let kf = k as f64;
    ln_gamma(nf + 1.0) - ln_gamma(kf + 1.0) - ln_gamma(nf - kf + 1.0)
        + kf * p.ln()
        + (nf - kf) * (1.0 - p).ln()
}

fn binom_sum(n: usize, from: usize, to: usize, p: f64) -> f64 {
    if from > to {
        return 0.0;
    }
    (from..=to)
        .map(|k| binom_ln_pmf(n, k, p).exp())
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

fn exact_p_value(n: usize, x: usize, p0: f64, alternative: Alternative) -> f64 {
    match alternative {
        Alternative::Less => binom_sum(n, 0, x, p0),
        Alternative::Greater => binom_sum(n, x, n, p0),
        Alternative::TwoSided => {
            let observed = binom_ln_pmf(n, x, p0).exp();
            let mut total = 0.0;
            for k in 0..=n {
                let probability = binom_ln_pmf(n, k, p0).exp();
                if probability <= observed * (1.0 + 1e-9) {
                    total += probability;
                }
            }
            total.clamp(0.0, 1.0)
        }
    }
}

/// Blaker's adjusted two-sided exact p-value.
///
/// For each outcome `i` let `T(i) = min(P(X ≤ i), P(X ≥ i))`; the p-value is
/// the total probability of the outcomes with `T(i) ≤ T(x)`, **leaving out
/// the observed outcome**. That is the "adjusted" form of the documented behaviour
/// (n = 20, x = 14, p₀ = 0.5 gives 0.078354, where Clopper–Pearson gives
/// 0.115318).
fn blaker_p_value(n: usize, x: usize, p0: f64) -> f64 {
    let probabilities: Vec<f64> = (0..=n).map(|k| binom_ln_pmf(n, k, p0).exp()).collect();
    let tails = blaker_tails(&probabilities);
    let observed = tails[x];
    probabilities
        .iter()
        .enumerate()
        .filter(|(k, _)| *k != x && tails[*k] <= observed * (1.0 + 1e-12))
        .map(|(_, probability)| *probability)
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

/// The one-sided tail `T(i) = min(P(X ≤ i), P(X ≥ i))` of each outcome.
fn blaker_tails(probabilities: &[f64]) -> Vec<f64> {
    let n = probabilities.len().saturating_sub(1);
    let mut tails = vec![0.0; probabilities.len()];
    let mut cumulative = 0.0;
    for (index, probability) in probabilities.iter().enumerate() {
        cumulative += probability;
        tails[index] = cumulative;
    }
    let mut surviving = 0.0;
    for index in (0..=n).rev() {
        surviving += probabilities[index];
        tails[index] = tails[index].min(surviving);
    }
    tails
}

/// Blaker's interval: the null proportions whose adjusted p-value is at least
/// `alpha`, found by bisection.
fn blaker_interval(x: usize, n: usize, confidence: f64) -> (f64, f64) {
    if n == 0 {
        return (0.0, 1.0);
    }
    let target = (1.0 - confidence) / 2.0;
    let p_hat = x as f64 / n as f64;
    let mut low = 0.0;
    let mut high = p_hat;
    for _ in 0..120 {
        let middle = 0.5 * (low + high);
        if blaker_p_value(n, x, middle) < target {
            low = middle;
        } else {
            high = middle;
        }
    }
    let lower = low;
    let mut low = p_hat;
    let mut high = 1.0;
    for _ in 0..120 {
        let middle = 0.5 * (low + high);
        if blaker_p_value(n, x, middle) < target {
            high = middle;
        } else {
            low = middle;
        }
    }
    (lower.clamp(0.0, 1.0), high.clamp(0.0, 1.0))
}

fn normal_p_value(z: f64, alternative: Alternative) -> f64 {
    match alternative {
        Alternative::TwoSided => (2.0 * normal_sf(z.abs())).clamp(0.0, 1.0),
        Alternative::Less => (1.0 - normal_sf(z)).clamp(0.0, 1.0),
        Alternative::Greater => normal_sf(z),
    }
}

fn wilson(p_hat: f64, n: usize, z: f64) -> (f64, f64) {
    let nf = n as f64;
    let denom = 1.0 + z * z / nf;
    let center = (p_hat + z * z / (2.0 * nf)) / denom;
    let half = z * (p_hat * (1.0 - p_hat) / nf + z * z / (4.0 * nf * nf)).sqrt() / denom;
    (
        (center - half).clamp(0.0, 1.0),
        (center + half).clamp(0.0, 1.0),
    )
}

fn clopper_pearson(x: usize, n: usize, confidence: f64, alternative: Alternative) -> (f64, f64) {
    if n == 0 {
        return (0.0, 1.0);
    }
    let alpha = 1.0 - confidence;
    let tail = if alternative == Alternative::TwoSided {
        alpha / 2.0
    } else {
        alpha
    };
    let lower = if x == 0 {
        0.0
    } else {
        beta_quantile(tail, x as f64, (n - x + 1) as f64)
    };
    let upper = if x == n {
        1.0
    } else {
        beta_quantile(1.0 - tail, (x + 1) as f64, (n - x) as f64)
    };
    (lower.clamp(0.0, 1.0), upper.clamp(0.0, 1.0))
}

/// Two-sided Clopper–Pearson interval for a binomial proportion.
///
/// Used by the attribute capability report, which shares the exact interval of
/// the proportion tests.
#[must_use]
pub(crate) fn exact_interval(successes: usize, trials: usize, confidence: f64) -> (f64, f64) {
    clopper_pearson(successes, trials, confidence, Alternative::TwoSided)
}

fn interval(lower: f64, upper: f64, alternative: Alternative) -> (Option<f64>, Option<f64>) {
    match alternative {
        Alternative::TwoSided => (Some(lower), Some(upper)),
        Alternative::Less => (None, Some(upper)),
        Alternative::Greater => (Some(lower), None),
    }
}

fn add_interval(
    result: &mut AnalysisResult,
    lower: Option<f64>,
    upper: Option<f64>,
    confidence: f64,
) {
    let label = format!("{:.1}% interval", confidence * 100.0);
    match (lower, upper) {
        (Some(lower), Some(upper)) => {
            result.statistics.push(("CI lower", lower));
            result.statistics.push(("CI upper", upper));
            result
                .extra
                .push(Item::Message(format!("{label}: ({lower:.6}, {upper:.6})")));
        }
        (Some(lower), None) => {
            result.statistics.push(("Lower bound", lower));
            result
                .extra
                .push(Item::Message(format!("{label}: ({lower:.6}, 1)")));
        }
        (None, Some(upper)) => {
            result.statistics.push(("Upper bound", upper));
            result
                .extra
                .push(Item::Message(format!("{label}: (0, {upper:.6})")));
        }
        (None, None) => {}
    }
}

/// One-sample proportion test for a 0/1 column.
pub fn one_sample(
    df: &DataFrame,
    column_name: &str,
    success: f64,
    null_p: f64,
    method: ProportionMethod,
    alternative: Alternative,
    confidence: f64,
) -> Result<AnalysisResult> {
    validate_confidence(confidence)?;
    validate_p(null_p, "null proportion")?;
    if null_p == 0.0 || null_p == 1.0 {
        return Err(Error::InvalidProbability { p: null_p });
    }
    let (successes, failures, excluded) = counts(df, column_name, success)?;
    let n = successes + failures;
    if n == 0 {
        return Err(Error::EmptySample);
    }
    let p_hat = successes as f64 / n as f64;
    let (p, method_text, lower, upper) = match method {
        ProportionMethod::Exact => {
            let (p, lower, upper) = if alternative == Alternative::TwoSided {
                let (lower, upper) = blaker_interval(successes, n, confidence);
                (blaker_p_value(n, successes, null_p), lower, upper)
            } else {
                let (lower, upper) = clopper_pearson(successes, n, confidence, alternative);
                (
                    exact_p_value(n, successes, null_p, alternative),
                    lower,
                    upper,
                )
            };
            let (lower, upper) = interval(lower, upper, alternative);
            (p, "Blaker's adjusted exact", lower, upper)
        }
        ProportionMethod::Normal => {
            let se = (null_p * (1.0 - null_p) / n as f64).sqrt();
            let z = (p_hat - null_p) / se;
            let p = normal_p_value(z, alternative);
            let tail = if alternative == Alternative::TwoSided {
                (1.0 - confidence) / 2.0
            } else {
                1.0 - confidence
            };
            let z_crit = normal_quantile(1.0 - tail);
            let (lower, upper) = wilson(p_hat, n, z_crit);
            let (lower, upper) = interval(lower, upper, alternative);
            (p, "Normal approximation (Wilson interval)", lower, upper)
        }
    };

    let mut result = AnalysisResult::new(
        format!("1 Proportion — column {column_name}"),
        format!("{method_text}; {} alternative", alternative.as_str()),
    );
    result.n = n;
    result.excluded = excluded;
    result.statistics = vec![
        ("Events", successes as f64),
        ("Trials", n as f64),
        ("Sample proportion", p_hat),
        ("Null proportion", null_p),
        ("p-value", p),
    ];
    add_interval(&mut result, lower, upper, confidence);
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            format!("H₀: p = {null_p}; H₁: p {} {null_p}", alternative.symbol()),
        ),
        (
            "Data".to_owned(),
            format!(
                "numeric 0/1 column; the value {success} is counted as a success; missing values excluded"
            ),
        ),
        (
            "Method".to_owned(),
            match method {
                ProportionMethod::Exact => {
                    "Blaker's adjusted exact p-value (outcomes at least as extreme by the \
                     one-sided tail, leaving out the observed one); Blaker interval; \
                     Clopper–Pearson for one-sided alternatives"
                        .to_owned()
                }
                ProportionMethod::Normal => {
                    "pooled null standard error; Wilson score interval".to_owned()
                }
            },
        ),
    ];
    Ok(result)
}

/// Two-sample proportion test using the pooled normal approximation.
pub fn two_sample(
    df: &DataFrame,
    first: &str,
    second: &str,
    success: f64,
    alternative: Alternative,
    confidence: f64,
) -> Result<AnalysisResult> {
    validate_confidence(confidence)?;
    if first == second {
        return Err(Error::Unsupported(
            "the two samples must be different columns".to_owned(),
        ));
    }
    let (x1, f1, e1) = counts(df, first, success)?;
    let (x2, f2, e2) = counts(df, second, success)?;
    let n1 = x1 + f1;
    let n2 = x2 + f2;
    if n1 == 0 || n2 == 0 {
        return Err(Error::EmptySample);
    }
    let p1 = x1 as f64 / n1 as f64;
    let p2 = x2 as f64 / n2 as f64;
    let diff = p1 - p2;
    let pooled = (x1 + x2) as f64 / (n1 + n2) as f64;
    let se = (pooled * (1.0 - pooled) * (1.0 / n1 as f64 + 1.0 / n2 as f64)).sqrt();
    let (z, p) = if se > 0.0 {
        let z = diff / se;
        (z, normal_p_value(z, alternative))
    } else if diff.abs() < 1e-15 {
        (0.0, 1.0)
    } else {
        (f64::INFINITY, 0.0)
    };
    let tail = if alternative == Alternative::TwoSided {
        (1.0 - confidence) / 2.0
    } else {
        1.0 - confidence
    };
    let z_crit = normal_quantile(1.0 - tail);
    let unpooled = (p1 * (1.0 - p1) / n1 as f64 + p2 * (1.0 - p2) / n2 as f64).sqrt();
    let (lower, upper) = (
        (diff - z_crit * unpooled).clamp(-1.0, 1.0),
        (diff + z_crit * unpooled).clamp(-1.0, 1.0),
    );
    let (lower, upper) = interval(lower, upper, alternative);

    let mut result = AnalysisResult::new(
        format!("2 Proportions — {first} vs {second}"),
        format!(
            "Pooled normal approximation; {} alternative",
            alternative.as_str()
        ),
    );
    result.n = n1 + n2;
    result.excluded = e1 + e2;
    result.statistics = vec![
        ("Sample 1 proportion", p1),
        ("Sample 2 proportion", p2),
        ("Difference (1 − 2)", diff),
        ("z", z),
        ("p-value", p),
    ];
    add_interval(&mut result, lower, upper, confidence);
    result.extra.insert(
        0,
        Item::Table {
            headers: vec![
                "Sample".to_owned(),
                "Events".to_owned(),
                "Trials".to_owned(),
                "Proportion".to_owned(),
            ],
            rows: vec![
                vec![
                    first.to_owned(),
                    x1.to_string(),
                    n1.to_string(),
                    format!("{p1:.6}"),
                ],
                vec![
                    second.to_owned(),
                    x2.to_string(),
                    n2.to_string(),
                    format!("{p2:.6}"),
                ],
            ],
        },
    );
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: p₁ − p₂ = 0; H₁: p₁ − p₂ ≠ 0 (or the requested alternative)".to_owned(),
        ),
        (
            "Method".to_owned(),
            "pooled proportion for the test; unpooled standard error for the interval".to_owned(),
        ),
        (
            "Data".to_owned(),
            format!(
                "numeric 0/1 columns; the value {success} is counted as a success; each column filtered independently"
            ),
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
    fn exact_one_proportion_uses_adjusted_blaker() {
        let df = frame("x", vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]);
        let result = one_sample(
            &df,
            "x",
            1.0,
            0.5,
            ProportionMethod::Exact,
            Alternative::TwoSided,
            0.95,
        )
        .expect("test");
        let p = result
            .statistics
            .iter()
            .find(|(name, _)| *name == "p-value")
            .map(|(_, value)| *value)
            .unwrap();
        // n = 10, x = 7, p₀ = 0.5: adjusted Blaker p = 0.2265625.
        assert!((p - 0.2265625).abs() < 1e-10, "p={p}");
        assert!(result.method.contains("Blaker"));
    }

    #[test]
    fn exact_one_proportion_interval_is_blaker() {
        let df = frame("x", vec![1.0; 7].into_iter().chain(vec![0.0; 3]).collect());
        let result = one_sample(
            &df,
            "x",
            1.0,
            0.5,
            ProportionMethod::Exact,
            Alternative::TwoSided,
            0.95,
        )
        .expect("test");
        let value = |name: &str| {
            result
                .statistics
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .unwrap()
        };
        // Blaker interval for n = 10, x = 7 (95 %).
        // Blaker interval for n = 10, x = 7 (95 %).
        assert!((value("CI lower") - 0.393755).abs() < 1e-3);
        assert!((value("CI upper") - 0.878448).abs() < 1e-3);
    }

    #[test]
    fn exact_one_proportion_matches_the_documented_run() {
        // Documented: n = 20, 14 successes, p₀ = 0.5 -> p 0.078, CI 0.473989-0.860640.
        let mut values = vec![1.0; 14];
        values.extend(vec![0.0; 6]);
        let df = frame("x", values);
        let result = one_sample(
            &df,
            "x",
            1.0,
            0.5,
            ProportionMethod::Exact,
            Alternative::TwoSided,
            0.95,
        )
        .expect("test");
        let value = |name: &str| {
            result
                .statistics
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .unwrap()
        };
        assert!(
            (value("p-value") - 0.078354).abs() < 1e-5,
            "{}",
            value("p-value")
        );
        assert!((value("CI lower") - 0.473989).abs() < 1e-5);
        assert!((value("CI upper") - 0.860640).abs() < 1e-5);
    }

    #[test]
    fn normal_one_proportion_reports_a_wilson_interval() {
        let df = frame("x", vec![1.0; 7].into_iter().chain(vec![0.0; 3]).collect());
        let result = one_sample(
            &df,
            "x",
            1.0,
            0.5,
            ProportionMethod::Normal,
            Alternative::TwoSided,
            0.95,
        )
        .expect("test");
        assert!(result.method.contains("Wilson"));
        assert!(result.to_session_text().contains("interval"));
    }

    #[test]
    fn two_proportion_test_uses_counts() {
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 1.0, 1.0, 0.0, 0.0]),
            Column::new_f64("b", vec![1.0, 1.0, 0.0, 0.0, 0.0]),
        ])
        .unwrap();
        let result = two_sample(&df, "a", "b", 1.0, Alternative::TwoSided, 0.95).expect("test");
        assert!(result.to_session_text().contains("Difference"));
        assert!(result.to_session_text().contains("z"));
    }

    #[test]
    fn non_binary_values_are_rejected() {
        let df = frame("x", vec![1.0, 2.0]);
        assert!(matches!(
            one_sample(
                &df,
                "x",
                1.0,
                0.5,
                ProportionMethod::Exact,
                Alternative::TwoSided,
                0.95
            ),
            Err(Error::Cell { .. })
        ));
    }
}
