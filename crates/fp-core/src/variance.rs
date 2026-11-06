//! One- and two-sample variance tests.
//!
//! The one-sample test is the chi-square test for a variance; the two-sample
//! test is the F test for a variance ratio. Confidence intervals use the
//! corresponding chi-square and F quantiles. These are standard conventions,
//! not yet verified against documented output.

use std::collections::BTreeMap;

use crate::data::{Column, ColumnData, DataFrame, MissingPolicy, Sample};
use crate::error::{Error, Result};
use crate::hypothesis::Alternative;
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{
    chi_square_cdf, chi_square_quantile, f_cdf, f_quantile, normal_cdf, normal_quantile,
};

fn validate_confidence(confidence: f64) -> Result<()> {
    if !confidence.is_finite() || !(0.5..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence level",
            value: confidence,
        });
    }
    Ok(())
}

fn sample_stats(df: &DataFrame, column: &str) -> Result<(usize, f64, usize)> {
    let sample = df.column(column)?.sample(MissingPolicy::Drop)?;
    if sample.n() < 2 {
        return Err(Error::SampleTooSmall {
            n: sample.n(),
            min: 2,
        });
    }
    Ok((sample.n(), sample.variance()?, sample.excluded()))
}

fn two_sided_p(cdf: f64) -> f64 {
    (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0)
}

fn add_interval(
    result: &mut AnalysisResult,
    lower: Option<f64>,
    upper: Option<f64>,
    confidence: f64,
    label: &str,
) {
    let suffix = format!("{:.1}% interval", confidence * 100.0);
    match (lower, upper) {
        (Some(lower), Some(upper)) => {
            result.statistics.push(("CI lower", lower));
            result.statistics.push(("CI upper", upper));
            result.extra.push(Item::Message(format!(
                "{label} {suffix}: ({lower:.6}, {upper:.6})"
            )));
        }
        (Some(lower), None) => {
            result.statistics.push(("Lower bound", lower));
            result
                .extra
                .push(Item::Message(format!("{label} {suffix}: ({lower:.6}, ∞)")));
        }
        (None, Some(upper)) => {
            result.statistics.push(("Upper bound", upper));
            result
                .extra
                .push(Item::Message(format!("{label} {suffix}: (0, {upper:.6})")));
        }
        (None, None) => {}
    }
}

fn bounds(lower: f64, upper: f64, alternative: Alternative) -> (Option<f64>, Option<f64>) {
    match alternative {
        Alternative::TwoSided => (Some(lower), Some(upper)),
        Alternative::Less => (None, Some(upper)),
        Alternative::Greater => (Some(lower), None),
    }
}

/// One-sample chi-square test for a variance.
pub fn one_sample(
    df: &DataFrame,
    column_name: &str,
    null_variance: f64,
    alternative: Alternative,
    confidence: f64,
) -> Result<AnalysisResult> {
    validate_confidence(confidence)?;
    if !null_variance.is_finite() || null_variance <= 0.0 {
        return Err(Error::InvalidParameter {
            name: "null variance",
            value: null_variance,
        });
    }
    let (n, variance, excluded) = sample_stats(df, column_name)?;
    let df_chi = n as f64 - 1.0;
    let statistic = df_chi * variance / null_variance;
    let cdf = chi_square_cdf(statistic, df_chi);
    let p = match alternative {
        Alternative::TwoSided => two_sided_p(cdf),
        Alternative::Less => cdf.clamp(0.0, 1.0),
        Alternative::Greater => (1.0 - cdf).clamp(0.0, 1.0),
    };
    let alpha = 1.0 - confidence;
    let (lower, upper) = match alternative {
        Alternative::TwoSided => (
            df_chi * variance / chi_square_quantile(1.0 - alpha / 2.0, df_chi),
            df_chi * variance / chi_square_quantile(alpha / 2.0, df_chi),
        ),
        Alternative::Less => (0.0, df_chi * variance / chi_square_quantile(alpha, df_chi)),
        Alternative::Greater => (
            df_chi * variance / chi_square_quantile(1.0 - alpha, df_chi),
            f64::INFINITY,
        ),
    };
    let (lower, upper) = bounds(lower, upper, alternative);

    let mut result = AnalysisResult::new(
        format!("1 Variance — column {column_name}"),
        format!("Chi-square test; {} alternative", alternative.as_str()),
    );
    result.n = n;
    result.excluded = excluded;
    result.statistics = vec![
        ("Sample variance", variance),
        ("Sample standard deviation", variance.sqrt()),
        ("Null variance", null_variance),
        ("Chi-square", statistic),
        ("df", df_chi),
        ("p-value", p),
    ];
    add_interval(&mut result, lower, upper, confidence, "Variance");
    if let (Some(lower), Some(upper)) = (lower, upper) {
        result.statistics.push(("SD lower", lower.max(0.0).sqrt()));
        result.statistics.push(("SD upper", upper.sqrt()));
    }
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            format!(
                "H₀: σ² = {null_variance}; H₁: σ² {} {null_variance}",
                alternative.symbol()
            ),
        ),
        (
            "Estimator".to_owned(),
            "sample variance with divisor n−1; chi-square with n−1 df".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(n, excluded, 0);
    Ok(result)
}

/// Two-sample F test for equality of variances.
pub fn two_sample(
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
    let (n1, v1, e1) = sample_stats(df, first)?;
    let (n2, v2, e2) = sample_stats(df, second)?;
    let d1 = n1 as f64 - 1.0;
    let d2 = n2 as f64 - 1.0;
    let ratio = v1 / v2;
    let cdf = f_cdf(ratio, d1, d2);
    let p = match alternative {
        Alternative::TwoSided => two_sided_p(cdf),
        Alternative::Less => cdf.clamp(0.0, 1.0),
        Alternative::Greater => (1.0 - cdf).clamp(0.0, 1.0),
    };
    let alpha = 1.0 - confidence;
    let (lower, upper) = match alternative {
        Alternative::TwoSided => (
            ratio / f_quantile(1.0 - alpha / 2.0, d1, d2),
            ratio / f_quantile(alpha / 2.0, d1, d2),
        ),
        Alternative::Less => (0.0, ratio / f_quantile(alpha, d1, d2)),
        Alternative::Greater => (ratio / f_quantile(1.0 - alpha, d1, d2), f64::INFINITY),
    };
    let (lower, upper) = bounds(lower, upper, alternative);

    let mut result = AnalysisResult::new(
        format!("2 Variances — {first} vs {second}"),
        format!("F test; {} alternative", alternative.as_str()),
    );
    result.n = n1 + n2;
    result.excluded = e1 + e2;
    result.statistics = vec![
        ("Variance 1", v1),
        ("Variance 2", v2),
        ("Variance ratio (1/2)", ratio),
        ("F", ratio),
        ("df1", d1),
        ("df2", d2),
        ("p-value", p),
    ];
    add_interval(&mut result, lower, upper, confidence, "Variance ratio");
    result.extra.insert(
        0,
        Item::Table {
            headers: vec![
                "Sample".to_owned(),
                "n".to_owned(),
                "Variance".to_owned(),
                "StDev".to_owned(),
            ],
            rows: vec![
                vec![
                    first.to_owned(),
                    n1.to_string(),
                    format!("{v1:.6}"),
                    format!("{:.6}", v1.sqrt()),
                ],
                vec![
                    second.to_owned(),
                    n2.to_string(),
                    format!("{v2:.6}"),
                    format!("{:.6}", v2.sqrt()),
                ],
            ],
        },
    );
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: σ₁²/σ₂² = 1; H₁: σ₁²/σ₂² ≠ 1 (or the requested alternative)".to_owned(),
        ),
        (
            "Estimator".to_owned(),
            "sample variances with divisor n−1; F distribution with n₁−1 and n₂−1 df".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "each column filtered independently; non-finite values excluded".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    Ok(result)
}

/// Method used by the two-sample variance test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarianceMethod {
    /// F test for the variance ratio.
    F,
    /// Bonett's kurtosis-adjusted test.
    Bonett,
}

impl VarianceMethod {
    /// Stable report name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::F => "F test",
            Self::Bonett => "Bonett test",
        }
    }

    /// Stable identifier used by the CLI and the UI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::F => "f",
            Self::Bonett => "bonett",
        }
    }

    /// Parses `f` or `bonett` (case-insensitive).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "f" | "f-test" | "ftest" => Some(Self::F),
            "bonett" => Some(Self::Bonett),
            _ => None,
        }
    }
}

/// Bonett's two-sample variance test as a report.
pub fn bonett_analysis(
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
    let (n1, v1, e1) = sample_stats(df, first)?;
    let (n2, v2, e2) = sample_stats(df, second)?;
    let first_sample = df.column(first)?.sample(MissingPolicy::Drop)?;
    let second_sample = df.column(second)?.sample(MissingPolicy::Drop)?;
    let outcome = bonett(&first_sample, &second_sample, confidence, alternative)?;

    let mut result = AnalysisResult::new(
        format!("2 Variances — {first} vs {second}"),
        format!("Bonett test; {} alternative", alternative.as_str()),
    );
    result.n = n1 + n2;
    result.excluded = e1 + e2;
    result.statistics = vec![
        ("Variance 1", v1),
        ("Variance 2", v2),
        ("Variance ratio (1/2)", outcome.variance_ratio),
        ("Z", outcome.z),
        ("p-value", outcome.p_value),
    ];
    add_interval(
        &mut result,
        outcome.ci.0,
        outcome.ci.1,
        confidence,
        "Variance ratio",
    );
    result.extra.insert(
        0,
        Item::Table {
            headers: vec![
                "Sample".to_owned(),
                "n".to_owned(),
                "Variance".to_owned(),
                "StDev".to_owned(),
            ],
            rows: vec![
                vec![
                    first.to_owned(),
                    n1.to_string(),
                    format!("{v1:.6}"),
                    format!("{:.6}", v1.sqrt()),
                ],
                vec![
                    second.to_owned(),
                    n2.to_string(),
                    format!("{v2:.6}"),
                    format!("{:.6}", v2.sqrt()),
                ],
            ],
        },
    );
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: σ₁²/σ₂² = 1; H₁: σ₁²/σ₂² ≠ 1 (or the requested alternative)".to_owned(),
        ),
        (
            "Estimator".to_owned(),
            "sample variances with divisor n−1; Bonett's log-ratio statistic with the \
             kurtosis-based standard error"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "each column filtered independently; non-finite values excluded; at least four \
             observations per column"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    Ok(result)
}

/// Centre used by Levene's test to measure the spread inside a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VarianceCenter {
    /// Group mean (the original Levene test).
    #[default]
    Mean,
    /// Group median (the Brown–Forsythe variant, more robust).
    Median,
}

impl VarianceCenter {
    /// Name used in reports.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Mean => "group mean",
            Self::Median => "group median",
        }
    }

    /// Stable identifier for the CLI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mean => "mean",
            Self::Median => "median",
        }
    }

    /// Parses the CLI spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "mean" => Some(Self::Mean),
            "median" => Some(Self::Median),
            _ => None,
        }
    }
}

/// Outcome of a test of equal variances across several groups.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VarianceHomogeneity {
    /// Number of groups.
    pub groups: usize,
    /// Total number of observations.
    pub n: usize,
    /// Test statistic.
    pub statistic: f64,
    /// Degrees of freedom of the chi-square approximation.
    pub degrees_of_freedom: f64,
    /// p-value of the test.
    pub p_value: f64,
    /// Correction factor applied to the statistic.
    pub correction: f64,
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

fn group_label(column: &Column, row: usize) -> Option<String> {
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

/// Groups the response by the factor, in lexicographic level order.
fn grouped(
    df: &DataFrame,
    response: &str,
    factor: &str,
) -> Result<(BTreeMap<String, Vec<f64>>, usize)> {
    if response == factor {
        return Err(Error::Unsupported(
            "the response and factor must be different columns".to_owned(),
        ));
    }
    let response_column = df.column(response)?;
    let factor_column = df.column(factor)?;
    let mut groups: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut excluded = 0;
    for row in 0..df.n_rows() {
        let value = cell(response_column, row);
        let level = group_label(factor_column, row);
        match (value, level) {
            (Some(value), Some(level)) => groups.entry(level).or_default().push(value),
            _ => excluded += 1,
        }
    }
    if groups.len() < 2 {
        return Err(Error::Unsupported(
            "the factor must have at least two levels".to_owned(),
        ));
    }
    Ok((groups, excluded))
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

fn group_table(groups: &BTreeMap<String, Vec<f64>>) -> Vec<Vec<String>> {
    groups
        .iter()
        .map(|(level, values)| {
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            let variance = if values.len() > 1 {
                values
                    .iter()
                    .map(|value| (value - mean).powi(2))
                    .sum::<f64>()
                    / (values.len() - 1) as f64
            } else {
                f64::NAN
            };
            vec![
                level.clone(),
                values.len().to_string(),
                format!("{:.6}", mean),
                format!("{:.6}", median(values)),
                format!("{variance:.6}"),
            ]
        })
        .collect()
}

/// Outcome of the Levene F test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeveneOutcome {
    /// Number of observations used.
    pub n: usize,
    /// F statistic of the ANOVA on the absolute deviations.
    pub f: f64,
    /// Numerator degrees of freedom.
    pub df1: f64,
    /// Denominator degrees of freedom.
    pub df2: f64,
    /// p-value of the F test.
    pub p_value: f64,
}

fn levene_statistic(
    transformed: &[f64],
    labels: &[String],
    confidence: f64,
) -> Result<LeveneOutcome> {
    let frame = DataFrame::new(vec![
        Column::new_f64("z", transformed.to_vec()),
        Column::new_text("g", labels.to_vec()),
    ])
    .map_err(|error| Error::Csv(error.to_string()))?;
    let fit = crate::anova::fit(&frame, "z", "g", confidence)?;
    Ok(LeveneOutcome {
        n: fit.n,
        f: fit.f,
        df1: fit.df_between,
        df2: fit.df_within,
        p_value: fit.p_value,
    })
}

/// Levene's test of equal variances across groups.
///
/// The spread of every observation is measured against the centre of its
/// group (the mean, or the median for the Brown–Forsythe variant) and a
/// one-way ANOVA is applied to those absolute deviations.
pub fn levene(
    df: &DataFrame,
    response: &str,
    factor: &str,
    center: VarianceCenter,
    confidence: f64,
) -> Result<AnalysisResult> {
    validate_confidence(confidence)?;
    let (groups, excluded) = grouped(df, response, factor)?;
    let mut transformed = Vec::new();
    let mut labels = Vec::new();
    for (level, values) in &groups {
        if values.len() < 2 {
            return Err(Error::Unsupported(format!(
                "level {level} has fewer than two observations"
            )));
        }
        let centre = match center {
            VarianceCenter::Mean => values.iter().sum::<f64>() / values.len() as f64,
            VarianceCenter::Median => median(values),
        };
        for value in values {
            transformed.push((value - centre).abs());
            labels.push(level.clone());
        }
    }
    let outcome = levene_statistic(&transformed, &labels, confidence)?;

    let mut result = AnalysisResult::new(
        format!("Levene Test — {response} by {factor}"),
        format!(
            "One-way ANOVA of the absolute deviations from the {}; F test",
            center.name()
        ),
    );
    result.n = outcome.n;
    result.excluded = excluded;
    result.statistics = vec![
        ("F", outcome.f),
        ("df1", outcome.df1),
        ("df2", outcome.df2),
        ("p-value", outcome.p_value),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Level".to_owned(),
            "n".to_owned(),
            "Mean".to_owned(),
            "Median".to_owned(),
            "Variance".to_owned(),
        ],
        rows: group_table(&groups),
    });
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: the groups have the same variance; H₁: at least one differs".to_owned(),
        ),
        (
            "Method".to_owned(),
            format!(
                "absolute deviations from the {} are compared with a one-way ANOVA",
                center.name()
            ),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing or non-finite response or factor are excluded".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    Ok(result)
}

/// Bartlett's test of equal variances across groups.
///
/// `T = [(N−k)·ln(s²ₚ) − Σ(nᵢ−1)·ln(sᵢ²)] / C` with the pooled variance
/// `s²ₚ` and the correction
/// `C = 1 + [Σ 1/(nᵢ−1) − 1/(N−k)] / (3(k−1))`.
pub fn bartlett(df: &DataFrame, response: &str, factor: &str) -> Result<AnalysisResult> {
    let (groups, excluded) = grouped(df, response, factor)?;
    let outcome = bartlett_statistic(&groups)?;

    let mut result = AnalysisResult::new(
        format!("Bartlett Test — {response} by {factor}"),
        "Chi-square approximation of the likelihood ratio".to_owned(),
    );
    result.n = outcome.n;
    result.excluded = excluded;
    result.statistics = vec![
        ("Chi-square", outcome.statistic),
        ("df", outcome.degrees_of_freedom),
        ("p-value", outcome.p_value),
        ("Correction", outcome.correction),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Level".to_owned(),
            "n".to_owned(),
            "Mean".to_owned(),
            "Median".to_owned(),
            "Variance".to_owned(),
        ],
        rows: group_table(&groups),
    });
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: the groups have the same variance; H₁: at least one differs".to_owned(),
        ),
        (
            "Method".to_owned(),
            "likelihood-ratio statistic with the usual correction factor; the approximation is \
             sensitive to departures from normality"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing or non-finite response or factor are excluded".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    Ok(result)
}

fn bartlett_statistic(groups: &BTreeMap<String, Vec<f64>>) -> Result<VarianceHomogeneity> {
    let mut weighted = 0.0;
    let mut sum_logs = 0.0;
    let mut sum_reciprocals = 0.0;
    let mut n = 0;
    for values in groups.values() {
        if values.len() < 2 {
            return Err(Error::Unsupported(
                "every level needs at least two observations".to_owned(),
            ));
        }
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / (values.len() - 1) as f64;
        if variance <= 0.0 {
            return Err(Error::ConstantSample);
        }
        let weight = (values.len() - 1) as f64;
        weighted += weight * variance;
        sum_logs += weight * variance.ln();
        sum_reciprocals += 1.0 / weight;
        n += values.len();
    }
    let k = groups.len() as f64;
    let pooled = weighted / (n as f64 - k);
    let numerator = (n as f64 - k) * pooled.ln() - sum_logs;
    let correction = 1.0 + (sum_reciprocals - 1.0 / (n as f64 - k)) / (3.0 * (k - 1.0));
    let statistic = numerator / correction;
    let degrees_of_freedom = (groups.len() - 1) as f64;
    let p_value = (1.0 - chi_square_cdf(statistic, degrees_of_freedom)).clamp(0.0, 1.0);
    Ok(VarianceHomogeneity {
        groups: groups.len(),
        n,
        statistic,
        degrees_of_freedom,
        p_value,
        correction,
    })
}

/// Bonett's two-sample test for equality of variances.
///
/// The statistic is the log-ratio of the sample variances divided by the
/// kurtosis-based standard error `se_i² = (γ_i − (n_i − 3)/n_i) / (n_i − 1)`,
/// where `γ_i` is the (non-excess) sample kurtosis. Each group needs at least
/// four observations. The interval is for the variance ratio `σ₁²/σ₂²`.
#[derive(Debug, Clone, Copy)]
pub struct BonettOutcome {
    /// Standard-normal statistic; positive when the first variance is larger.
    pub z: f64,
    /// One- or two-sided p-value according to the alternative.
    pub p_value: f64,
    /// `s₁² / s₂²`.
    pub variance_ratio: f64,
    /// Confidence interval for the variance ratio (open ends for one-sided).
    pub ci: (Option<f64>, Option<f64>),
}

fn log_variance_se(sample: &Sample) -> Result<f64> {
    let n = sample.n();
    if n < 4 {
        return Err(Error::SampleTooSmall { n, min: 4 });
    }
    let mean = sample.mean()?;
    let mut m2 = 0.0;
    let mut m4 = 0.0;
    for &value in sample.values() {
        let deviation = value - mean;
        let squared = deviation * deviation;
        m2 += squared;
        m4 += squared * squared;
    }
    if m2 <= 0.0 {
        return Err(Error::NotConverged {
            detail: "Bonett test needs non-constant data".to_owned(),
        });
    }
    let nf = n as f64;
    let kurtosis = nf * m4 / (m2 * m2);
    let variance = (kurtosis - (nf - 3.0) / nf) / (nf - 1.0);
    if !variance.is_finite() || variance <= 0.0 {
        return Err(Error::NotConverged {
            detail: "Bonett standard error is not positive".to_owned(),
        });
    }
    Ok(variance)
}

/// Bonett's test for two independent samples.
pub fn bonett(
    first: &Sample,
    second: &Sample,
    confidence: f64,
    alternative: Alternative,
) -> Result<BonettOutcome> {
    validate_confidence(confidence)?;
    let se_first = log_variance_se(first)?;
    let se_second = log_variance_se(second)?;
    let first_variance = first.variance()?;
    let second_variance = second.variance()?;
    if first_variance <= 0.0 || second_variance <= 0.0 {
        return Err(Error::NotConverged {
            detail: "Bonett test needs positive sample variances".to_owned(),
        });
    }
    let ratio = first_variance / second_variance;
    let standard_error = (se_first + se_second).sqrt();
    if !standard_error.is_finite() || standard_error <= 0.0 {
        return Err(Error::NotConverged {
            detail: "Bonett standard error is not positive".to_owned(),
        });
    }
    let z = ratio.ln() / standard_error;
    let p_value = match alternative {
        Alternative::TwoSided => 2.0 * (1.0 - normal_cdf(z.abs())),
        Alternative::Less => normal_cdf(z),
        Alternative::Greater => 1.0 - normal_cdf(z),
    }
    .clamp(0.0, 1.0);
    let quantile = normal_quantile(1.0 - (1.0 - confidence) / 2.0);
    let lower = (ratio.ln() - quantile * standard_error).exp();
    let upper = (ratio.ln() + quantile * standard_error).exp();
    let ci = match alternative {
        Alternative::TwoSided => (Some(lower), Some(upper)),
        Alternative::Less => (None, Some(upper)),
        Alternative::Greater => (Some(lower), None),
    };
    Ok(BonettOutcome {
        z,
        p_value,
        variance_ratio: ratio,
        ci,
    })
}
