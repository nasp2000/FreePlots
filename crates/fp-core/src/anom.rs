// SPDX-License-Identifier: MIT
//! Analysis of means (ANOM).
//!
//! Every level mean of a factor is compared with the weighted grand mean
//! through simultaneous decision limits. With `k` levels, `N` observations and
//! the pooled mean square error `MSE` (`N − k` degrees of freedom), the limits
//! of a level with `nᵢ` observations are
//! `X̄̄ ± h · √(MSE·(N − nᵢ)/(N·nᵢ))`; when every level has the same size this
//! reduces to `X̄̄ ± h · √(MSE·(k − 1)/N)`.
//!
//! The critical value `h` is the **studentized maximum modulus** quantile,
//! computed by numerical integration of its distribution
//! `E_V[(2Φ(h√(V/df)) − 1)^k]` with `V ~ χ²(df)` and `df = N − k`. The
//! published ANOM tables use the same quantity; the engine's value is
//! computed rather than read from a table.

use std::collections::BTreeMap;

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{AxisSpec, PlotSpec, Point, Scale, Segment, Tick, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{ln_gamma, normal_cdf, student_t_quantile};
use crate::workflow::AnalysisOutput;

/// Position of a level mean relative to the decision limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The mean is inside the limits.
    Within,
    /// The mean is above the upper limit.
    Above,
    /// The mean is below the lower limit.
    Below,
}

impl Verdict {
    /// Name used by the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Within => "Within limits",
            Self::Above => "Above upper limit",
            Self::Below => "Below lower limit",
        }
    }
}

/// Decision for one factor level.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelDecision {
    /// Level label.
    pub level: String,
    /// Observations in the level.
    pub n: usize,
    /// Level mean.
    pub mean: f64,
    /// Level standard deviation (`n − 1`).
    pub sd: f64,
    /// Mean minus the weighted grand mean.
    pub difference: f64,
    /// Lower decision limit.
    pub lower: f64,
    /// Upper decision limit.
    pub upper: f64,
    /// Position relative to the limits.
    pub verdict: Verdict,
}

/// Fitted analysis of means.
#[derive(Debug, Clone, PartialEq)]
pub struct AnomFit {
    /// Level decisions in lexicographic level order.
    pub groups: Vec<LevelDecision>,
    /// Weighted grand mean.
    pub grand_mean: f64,
    /// Pooled mean square error.
    pub mse: f64,
    /// Error degrees of freedom (`N − k`).
    pub df: f64,
    /// Critical value `h` used by every limit.
    pub critical: f64,
    /// Significance level.
    pub alpha: f64,
    /// Whether every level has the same number of observations.
    pub balanced: bool,
    /// Observations used.
    pub n: usize,
    /// Rows excluded for a missing or non-finite value.
    pub excluded: usize,
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

fn validate_alpha(alpha: f64) -> Result<()> {
    if !alpha.is_finite() || alpha <= 0.0 || alpha >= 1.0 {
        return Err(Error::InvalidParameter {
            name: "alpha",
            value: alpha,
        });
    }
    Ok(())
}

/// Chi density with `df` degrees of freedom, used by the SMM integral.
fn chi_density(value: f64, df: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    chi_square_density(value * value, df) * 2.0 * value
}

/// Chi-square density with `df` degrees of freedom, used by the SMM integral.
fn chi_square_density(value: f64, df: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    let half = df / 2.0;
    ((half - 1.0) * value.ln() - value / 2.0 - half * 2.0f64.ln() - ln_gamma(half)).exp()
}

/// Studentized maximum modulus CDF: `E_V[(2Φ(h√(V/df)) − 1)^k]`.
fn smm_cdf(h: f64, k: usize, df: f64) -> f64 {
    // Integrate over the chi scale s = √V, whose density vanishes at zero for
    // every df > 1 (ANOM always has at least two observations per level).
    const INTERVALS: usize = 2000;
    let upper = df.sqrt() + 10.0 * (2.0 * df).sqrt();
    let step = upper / INTERVALS as f64;
    let root = df.sqrt();
    let mut total = 0.0;
    for index in 0..=INTERVALS {
        let value = step * index as f64;
        let p = 2.0 * normal_cdf(h * value / root) - 1.0;
        let weight = if index == 0 || index == INTERVALS {
            1.0
        } else if index % 2 == 0 {
            2.0
        } else {
            4.0
        };
        total += weight * p.max(0.0).powi(k as i32) * chi_density(value, df);
    }
    (total * step / 3.0).clamp(0.0, 1.0)
}

/// Studentized maximum modulus quantile for `k` statistics and `df` degrees
/// of freedom.
fn smm_quantile(k: usize, df: f64, alpha: f64) -> f64 {
    if k == 1 {
        // One statistic: the SMM is the ordinary two-sided t quantile.
        return student_t_quantile(1.0 - alpha / 2.0, df);
    }
    let mut low = 0.0;
    let mut high = 10.0;
    for _ in 0..60 {
        let middle = 0.5 * (low + high);
        if smm_cdf(middle, k, df) < 1.0 - alpha {
            low = middle;
        } else {
            high = middle;
        }
    }
    0.5 * (low + high)
}

/// Fits the analysis of means of `response` by `factor`.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for a significance level outside
/// `(0, 1)`, [`Error::Unsupported`] for the same response and factor column, a
/// non-numeric response or fewer than two levels, [`Error::SampleTooSmall`]
/// when a level has fewer than two observations, and
/// [`Error::ConstantSample`] when every value is identical.
pub fn fit(df: &DataFrame, response: &str, factor: &str, alpha: f64) -> Result<AnomFit> {
    validate_alpha(alpha)?;
    if response == factor {
        return Err(Error::Unsupported(
            "the response and factor must be different columns".to_owned(),
        ));
    }
    let response_column = df.column(response)?;
    if matches!(
        response_column.data(),
        ColumnData::Text(_) | ColumnData::Bool(_)
    ) {
        return Err(Error::Csv(format!(
            "response column {response} is not numeric"
        )));
    }
    let factor_column = df.column(factor)?;

    let mut groups: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        let Some(value) = cell(response_column, row) else {
            excluded += 1;
            continue;
        };
        let Some(level) = group_label(factor_column, row) else {
            excluded += 1;
            continue;
        };
        groups.entry(level).or_default().push(value);
    }
    if groups.len() < 2 {
        return Err(Error::Unsupported(
            "the factor must have at least two levels".to_owned(),
        ));
    }
    for values in groups.values() {
        if values.len() < 2 {
            return Err(Error::SampleTooSmall {
                n: values.len(),
                min: 2,
            });
        }
    }
    let n = groups.values().map(Vec::len).sum::<usize>();
    let k = groups.len();
    let df_error = (n - k) as f64;
    if df_error < 1.0 {
        return Err(Error::SampleTooSmall { n, min: k + 1 });
    }
    let grand_mean = groups
        .values()
        .flat_map(|values| values.iter())
        .sum::<f64>()
        / n as f64;

    let mut ss_within = 0.0;
    let mut ss_total = 0.0;
    for values in groups.values() {
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        ss_within += values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>();
        ss_total += values
            .iter()
            .map(|value| (value - grand_mean).powi(2))
            .sum::<f64>();
    }
    if ss_total <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let mse = ss_within / df_error;
    let critical = smm_quantile(k, df_error, alpha);
    let first = groups.values().next().map_or(0, Vec::len);
    let balanced = groups.values().all(|values| values.len() == first);

    let mut levels = Vec::with_capacity(k);
    for (level, values) in &groups {
        let count = values.len();
        let mean = values.iter().sum::<f64>() / count as f64;
        let sd = (values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / (count as f64 - 1.0))
            .sqrt();
        let half = critical * (mse * (n - count) as f64 / (n * count) as f64).sqrt();
        let lower = grand_mean - half;
        let upper = grand_mean + half;
        let verdict = if mean > upper {
            Verdict::Above
        } else if mean < lower {
            Verdict::Below
        } else {
            Verdict::Within
        };
        levels.push(LevelDecision {
            level: level.clone(),
            n: count,
            mean,
            sd,
            difference: mean - grand_mean,
            lower,
            upper,
            verdict,
        });
    }

    Ok(AnomFit {
        groups: levels,
        grand_mean,
        mse,
        df: df_error,
        critical,
        alpha,
        balanced,
        n,
        excluded,
    })
}

/// ANOM chart: the level means, the weighted grand mean and the limits.
fn chart(model: &AnomFit, response: &str, factor: &str) -> PlotSpec {
    let k = model.groups.len();
    let mut low = model.grand_mean;
    let mut high = model.grand_mean;
    for group in &model.groups {
        low = low.min(group.lower).min(group.mean);
        high = high.max(group.upper).max(group.mean);
    }
    let pad = ((high - low).abs() * 0.08).max(1e-9);
    let mut points = Vec::with_capacity(k);
    let mut lines = Vec::with_capacity(k * 2 + 1);
    for (index, group) in model.groups.iter().enumerate() {
        let x = index as f64 + 1.0;
        points.push(Point { x, y: group.mean });
        // Each level has its own limits, which coincide when the design is
        // balanced; the segments touch so a balanced chart shows two lines.
        for bound in [group.lower, group.upper] {
            lines.push(Segment {
                from: Point {
                    x: x - 0.5,
                    y: bound,
                },
                to: Point {
                    x: x + 0.5,
                    y: bound,
                },
                width: 1.5,
            });
        }
        for bound in [group.lower, group.upper] {
            lines.push(Segment {
                from: Point {
                    x: x - 0.08,
                    y: bound,
                },
                to: Point {
                    x: x + 0.08,
                    y: bound,
                },
                width: 2.5,
            });
        }
    }
    lines.push(Segment {
        from: Point {
            x: 0.5,
            y: model.grand_mean,
        },
        to: Point {
            x: k as f64 + 0.5,
            y: model.grand_mean,
        },
        width: 1.0,
    });
    let step = if k <= 12 { 1 } else { k.div_ceil(12) };
    let shorten = |label: &str| -> String {
        let characters: Vec<char> = label.chars().collect();
        if characters.len() <= 12 {
            label.to_owned()
        } else {
            let mut short: String = characters.into_iter().take(11).collect();
            short.push('…');
            short
        }
    };
    let x_axis = AxisSpec {
        label: factor.to_owned(),
        scale: Scale::Linear,
        min: 0.5,
        max: k as f64 + 0.5,
        ticks: model
            .groups
            .iter()
            .enumerate()
            .filter(|(index, _)| index % step == 0)
            .map(|(index, group)| Tick {
                position: index as f64 + 1.0,
                label: shorten(&group.level),
            })
            .collect(),
        minor: Vec::new(),
    };
    PlotSpec {
        title: format!("ANOM — {response} by {factor}"),
        x_axis,
        y_axis: linear_axis(response, low - pad, high + pad),
        size: (900, 520),
        points,
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Builds the ANOM report and chart.
///
/// # Errors
///
/// The errors of [`fit`].
pub fn analyze(df: &DataFrame, response: &str, factor: &str, alpha: f64) -> Result<AnalysisOutput> {
    let model = fit(df, response, factor, alpha)?;
    let k = model.groups.len();
    let flagged = model
        .groups
        .iter()
        .filter(|group| group.verdict != Verdict::Within)
        .count();

    let mut result = AnalysisResult::new(
        format!("ANOM — response {response} by {factor}"),
        format!(
            "Analysis of means: level means against simultaneous decision limits at \
             α = {:.3}",
            model.alpha
        ),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Levels", k as f64),
        ("Grand mean", model.grand_mean),
        ("MSE", model.mse),
        ("Pooled standard deviation", model.mse.max(0.0).sqrt()),
        ("Degrees of freedom", model.df),
        ("Critical value h", model.critical),
        ("Alpha", model.alpha),
        ("Outside the limits", flagged as f64),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Level".to_owned(),
            "n".to_owned(),
            "Mean".to_owned(),
            "Difference".to_owned(),
            "Lower".to_owned(),
            "Upper".to_owned(),
            "Decision".to_owned(),
        ],
        rows: model
            .groups
            .iter()
            .map(|group| {
                vec![
                    group.level.clone(),
                    format!("{}", group.n),
                    format!("{:.6}", group.mean),
                    format!("{:+.6}", group.difference),
                    format!("{:.6}", group.lower),
                    format!("{:.6}", group.upper),
                    group.verdict.name().to_owned(),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Grand mean".to_owned(),
            "weighted by the sample size of each level; with equal sizes it is the mean of the \
             level means"
                .to_owned(),
        ),
        (
            "Limits".to_owned(),
            "each level has the limits X̄̄ ± h·√(MSE·(N − nᵢ)/(N·nᵢ)); with equal sizes this is \
             X̄̄ ± h·√(MSE·(k − 1)/N)"
                .to_owned(),
        ),
        (
            "Critical value".to_owned(),
            "h is the studentized maximum modulus quantile for k levels and N − k degrees of \
             freedom, computed by numerical integration of its distribution"
                .to_owned(),
        ),
        (
            "Decision".to_owned(),
            "a level is outside the limits when its mean is above the upper limit or below the \
             lower limit; the limits hold simultaneously at the stated α"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the critical value is computed by numerical integration of the studentized maximum \
         modulus distribution; the published ANOM tables can differ slightly because the group \
         means are correlated"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(chart(&model, response, factor)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64(
                "y",
                vec![10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 20.0, 21.0, 22.0],
            ),
            Column::new_text(
                "g",
                ["A", "A", "A", "B", "B", "B", "C", "C", "C"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ])
        .expect("frame")
    }

    #[test]
    fn the_balanced_limits_follow_the_formula() {
        let model = fit(&frame(), "y", "g", 0.05).expect("fit");
        assert!(model.balanced);
        assert_eq!(model.n, 9);
        assert!((model.df - 6.0).abs() < 1e-12);
        assert!((model.grand_mean - 46.0 / 3.0).abs() < 1e-12);
        assert!((model.mse - 1.0).abs() < 1e-12);
        let expected_h = smm_quantile(3, 6.0, 0.05);
        assert!((model.critical - expected_h).abs() < 1e-12);
        assert!(
            (expected_h - 3.192_587_520_920_811).abs() < 1e-9,
            "{expected_h}"
        );
        // Balanced limits: h·√(MSE·(k − 1)/N).
        let half = expected_h * (2.0_f64 / 9.0).sqrt();
        for group in &model.groups {
            assert!((group.upper - group.lower - 2.0 * half).abs() < 1e-12);
        }
        let verdicts: Vec<Verdict> = model.groups.iter().map(|group| group.verdict).collect();
        assert_eq!(
            verdicts,
            vec![Verdict::Below, Verdict::Within, Verdict::Above]
        );
    }

    #[test]
    fn the_smm_quantile_reduces_to_the_t_for_one_statistic() {
        let one = smm_quantile(1, 10.0, 0.05);
        let t = student_t_quantile(0.975, 10.0);
        assert!((one - t).abs() < 1e-9, "{one} vs {t}");
    }

    #[test]
    fn the_unbalanced_limits_differ_by_level() {
        let df = DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 2.0, 5.0, 6.0, 7.0, 8.0, 10.0, 11.0, 12.0]),
            Column::new_text(
                "g",
                ["A", "A", "B", "B", "B", "B", "C", "C", "C"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ])
        .expect("frame");
        let model = fit(&df, "y", "g", 0.05).expect("fit");
        assert!(!model.balanced);
        assert!((model.mse - 1.25).abs() < 1e-12);
        assert!((model.grand_mean - 62.0 / 9.0).abs() < 1e-12);
        let half = |count: usize| {
            model.critical
                * (model.mse * (model.n - count) as f64 / (model.n * count) as f64).sqrt()
        };
        assert!((model.groups[0].upper - model.grand_mean - half(2)).abs() < 1e-12);
        assert!((model.groups[1].upper - model.grand_mean - half(4)).abs() < 1e-12);
        assert!((model.groups[2].upper - model.grand_mean - half(3)).abs() < 1e-12);
        assert_eq!(model.groups[0].verdict, Verdict::Below);
        assert_eq!(model.groups[1].verdict, Verdict::Within);
        assert_eq!(model.groups[2].verdict, Verdict::Above);
    }

    #[test]
    fn the_report_and_chart_are_produced() {
        let output = analyze(&frame(), "y", "g", 0.01).expect("analysis");
        let text = output.results[0].to_session_text();
        assert!(text.contains("ANOM"), "{text}");
        assert!(text.contains("Above upper limit"), "{text}");
        assert!(text.contains("Below lower limit"), "{text}");
        let plot = output.plot.expect("chart");
        assert_eq!(plot.points.len(), 3);
        // Three levels: two limit segments and two caps each, plus the centre.
        assert_eq!(plot.lines.len(), 3 * 4 + 1);
        assert_eq!(plot.x_axis.ticks.len(), 3);
    }

    #[test]
    fn invalid_requests_are_rejected() {
        assert!(matches!(
            fit(&frame(), "y", "g", 0.0),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            fit(&frame(), "y", "g", 1.0),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(fit(&frame(), "y", "y", 0.05).is_err());
        assert!(fit(&frame(), "y", "missing", 0.05).is_err());
        let one_level = DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0]),
            Column::new_text("g", vec!["A".to_owned(), "A".to_owned(), "A".to_owned()]),
        ])
        .expect("frame");
        assert!(fit(&one_level, "y", "g", 0.05).is_err());
        let short_level = DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_text(
                "g",
                ["A", "A", "A", "B"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ])
        .expect("frame");
        assert!(fit(&short_level, "y", "g", 0.05).is_err());
    }

    #[test]
    fn a_constant_sample_is_rejected() {
        let df = DataFrame::new(vec![
            Column::new_f64("y", vec![5.0, 5.0, 5.0, 5.0]),
            Column::new_text(
                "g",
                ["A", "A", "B", "B"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ])
        .expect("frame");
        assert!(matches!(
            fit(&df, "y", "g", 0.05),
            Err(Error::ConstantSample)
        ));
    }

    #[test]
    fn missing_values_are_excluded_and_counted() {
        let df = DataFrame::new(vec![
            Column::new_f64("y", vec![10.0, 11.0, 12.0, 20.0, 21.0, f64::NAN]),
            Column::new_text(
                "g",
                ["A", "A", "A", "B", "B", "B"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ])
        .expect("frame");
        let model = fit(&df, "y", "g", 0.05).expect("fit");
        assert_eq!(model.n, 5);
        assert_eq!(model.excluded, 1);
    }
}
