//! One-way analysis of variance.
//!
//! The model is `yᵢⱼ = µ + τⱼ + εᵢⱼ`. The report includes group summaries,
//! the ANOVA table, R², Tukey's HSD and Bonferroni-adjusted pairwise t
//! comparisons. The
//! documented behaviour's default multiple-comparison method is not assumed to be
//! Bonferroni, so that table is labelled explicitly.

use std::collections::BTreeMap;

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{
    f_cdf, student_t_cdf, student_t_quantile, studentized_range_cdf, studentized_range_quantile,
};
use crate::workflow::AnalysisOutput;

/// Summary of one factor level.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupSummary {
    /// Level label.
    pub level: String,
    /// Observations in the level.
    pub n: usize,
    /// Level mean.
    pub mean: f64,
    /// Level standard deviation (`n−1`).
    pub sd: f64,
    /// Lower confidence bound for the level mean.
    pub ci_lower: f64,
    /// Upper confidence bound for the level mean.
    pub ci_upper: f64,
}

/// One Bonferroni-adjusted pairwise comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct Pairwise {
    /// First level.
    pub first: String,
    /// Second level.
    pub second: String,
    /// Mean difference (first − second).
    pub difference: f64,
    /// Standard error of the difference.
    pub se: f64,
    /// t statistic.
    pub t: f64,
    /// Unadjusted two-sided p-value.
    pub p_raw: f64,
    /// Bonferroni-adjusted p-value.
    pub p_adjusted: f64,
    /// Studentized range statistic of Tukey's HSD.
    pub q: f64,
    /// Tukey-adjusted p-value.
    pub p_tukey: f64,
    /// Lower simultaneous confidence bound (Tukey).
    pub tukey_lower: f64,
    /// Upper simultaneous confidence bound (Tukey).
    pub tukey_upper: f64,
    /// Lower adjusted confidence bound.
    pub ci_lower: f64,
    /// Upper adjusted confidence bound.
    pub ci_upper: f64,
    /// Lower Fisher LSD bound (no multiplicity adjustment).
    pub lsd_lower: f64,
    /// Upper Fisher LSD bound.
    pub lsd_upper: f64,
    /// Games–Howell standard error (unequal variances).
    pub gh_se: f64,
    /// Games–Howell degrees of freedom (Welch–Satterthwaite).
    pub gh_df: f64,
    /// Games–Howell studentized range statistic.
    pub gh_q: f64,
    /// Games–Howell p-value.
    pub gh_p: f64,
    /// Lower Games–Howell bound.
    pub gh_lower: f64,
    /// Upper Games–Howell bound.
    pub gh_upper: f64,
}

/// Fitted one-way ANOVA.
#[derive(Debug, Clone, PartialEq)]
pub struct AnovaFit {
    /// Group summaries in lexicographic level order.
    pub groups: Vec<GroupSummary>,
    /// Grand mean.
    pub grand_mean: f64,
    /// Between-group sum of squares.
    pub ss_between: f64,
    /// Within-group sum of squares.
    pub ss_within: f64,
    /// Total sum of squares.
    pub ss_total: f64,
    /// Between-group degrees of freedom.
    pub df_between: f64,
    /// Within-group degrees of freedom.
    pub df_within: f64,
    /// Between-group mean square.
    pub ms_between: f64,
    /// Within-group mean square.
    pub ms_within: f64,
    /// F statistic.
    pub f: f64,
    /// p-value of the F test.
    pub p_value: f64,
    /// Coefficient of determination.
    pub r_squared: f64,
    /// Adjusted coefficient of determination.
    pub adjusted_r_squared: f64,
    /// Pooled standard deviation.
    pub pooled_sd: f64,
    /// Pairwise comparisons.
    pub pairwise: Vec<Pairwise>,
    /// Fitted values (group means).
    pub fitted: Vec<f64>,
    /// Residuals.
    pub residuals: Vec<f64>,
    /// Rows used.
    pub n: usize,
    /// Rows excluded because the response or factor was missing.
    pub excluded: usize,
    /// Confidence level used for group means and pairwise intervals.
    pub confidence: f64,
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

fn validate_confidence(confidence: f64) -> Result<()> {
    if !confidence.is_finite() || !(0.5..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence level",
            value: confidence,
        });
    }
    Ok(())
}

/// Fits a one-way ANOVA of `response` by `factor`.
pub fn fit(df: &DataFrame, response: &str, factor: &str, confidence: f64) -> Result<AnovaFit> {
    validate_confidence(confidence)?;
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
    for row in 0..df.n_rows() {
        let Some(value) = cell(response_column, row) else {
            continue;
        };
        let Some(level) = group_label(factor_column, row) else {
            continue;
        };
        groups.entry(level).or_default().push(value);
    }
    if groups.len() < 2 {
        return Err(Error::Unsupported(
            "the factor must have at least two levels".to_owned(),
        ));
    }
    for (level, values) in &groups {
        if values.len() < 2 {
            return Err(Error::Unsupported(format!(
                "level {level} has fewer than two observations"
            )));
        }
    }
    let n = groups.values().map(Vec::len).sum::<usize>();
    let g = groups.len();
    if n <= g {
        return Err(Error::SampleTooSmall { n, min: g + 1 });
    }
    let grand_mean = groups
        .values()
        .flat_map(|values| values.iter())
        .sum::<f64>()
        / n as f64;

    let mut ss_between = 0.0;
    let mut ss_within = 0.0;
    let mut summaries = Vec::with_capacity(g);
    for (level, values) in &groups {
        let count = values.len();
        let mean = values.iter().sum::<f64>() / count as f64;
        let ss = values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>();
        ss_within += ss;
        ss_between += count as f64 * (mean - grand_mean).powi(2);
        let sd = (ss / (count as f64 - 1.0)).sqrt();
        summaries.push(GroupSummary {
            level: level.clone(),
            n: count,
            mean,
            sd,
            ci_lower: mean,
            ci_upper: mean,
        });
    }
    let ss_total = ss_between + ss_within;
    let df_between = (g - 1) as f64;
    let df_within = (n - g) as f64;
    let ms_between = ss_between / df_between;
    let ms_within = ss_within / df_within;
    // The documented behaviour's Means table uses the pooled standard deviation
    // and the error degrees of freedom, not each group's own standard
    // deviation (documented n = 24, three levels: A 19.949 ± 0.336).
    let pooled_sd = ms_within.sqrt();
    let tcrit = student_t_quantile(1.0 - (1.0 - confidence) / 2.0, df_within);
    for summary in &mut summaries {
        let half = tcrit * pooled_sd / (summary.n as f64).sqrt();
        summary.ci_lower = summary.mean - half;
        summary.ci_upper = summary.mean + half;
    }
    let f = if ms_within > 0.0 {
        ms_between / ms_within
    } else {
        f64::INFINITY
    };
    let p_value = if f.is_finite() {
        (1.0 - f_cdf(f, df_between, df_within)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let r_squared = if ss_total > 0.0 {
        (ss_between / ss_total).clamp(0.0, 1.0)
    } else {
        return Err(Error::ConstantSample);
    };
    let adjusted_r_squared = 1.0 - (1.0 - r_squared) * (n as f64 - 1.0) / df_within;
    let pooled_sd = ms_within.max(0.0).sqrt();

    let comparisons = g * (g - 1) / 2;
    let mut pairwise = Vec::with_capacity(comparisons);
    if comparisons > 0 {
        let adjusted_tail = (1.0 - confidence) / (2.0 * comparisons as f64);
        let critical = student_t_quantile(1.0 - adjusted_tail, df_within);
        let tukey_critical = studentized_range_quantile(confidence, g as f64, df_within);
        for i in 0..summaries.len() {
            for j in (i + 1)..summaries.len() {
                let a = &summaries[i];
                let b = &summaries[j];
                let se = pooled_sd * (1.0 / a.n as f64 + 1.0 / b.n as f64).sqrt();
                let difference = a.mean - b.mean;
                let t = if se > 0.0 {
                    difference / se
                } else if difference.abs() < 1e-15 {
                    0.0
                } else {
                    f64::INFINITY
                };
                let p_raw = if t.is_finite() {
                    let cdf = student_t_cdf(t, df_within);
                    (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                // Tukey–Kramer: the standard error of the difference is
                // sqrt(MSE/2·(1/nᵢ + 1/nⱼ)), i.e. the t standard error over √2.
                let tukey_se = se / 2f64.sqrt();
                let q = if tukey_se > 0.0 {
                    difference.abs() / tukey_se
                } else if difference.abs() < 1e-15 {
                    0.0
                } else {
                    f64::INFINITY
                };
                let p_tukey = if q.is_finite() {
                    (1.0 - studentized_range_cdf(q, g as f64, df_within)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                // Fisher's LSD: the plain t interval, without multiplicity.
                let lsd_critical = student_t_quantile(1.0 - (1.0 - confidence) / 2.0, df_within);
                // Games–Howell: unequal variances with the Welch–Satterthwaite
                // degrees of freedom and the studentized range on top.
                let variance_a = a.sd * a.sd / a.n as f64;
                let variance_b = b.sd * b.sd / b.n as f64;
                let gh_se = ((variance_a + variance_b) / 2.0).sqrt();
                let gh_df = if variance_a > 0.0 || variance_b > 0.0 {
                    (variance_a + variance_b).powi(2)
                        / ((variance_a * variance_a / (a.n as f64 - 1.0).max(1.0))
                            + (variance_b * variance_b / (b.n as f64 - 1.0).max(1.0)))
                } else {
                    df_within
                };
                let gh_q = if gh_se > 0.0 {
                    difference.abs() / gh_se
                } else if difference.abs() < 1e-15 {
                    0.0
                } else {
                    f64::INFINITY
                };
                let gh_p = if gh_q.is_finite() && gh_df >= 1.0 {
                    (1.0 - studentized_range_cdf(gh_q, g as f64, gh_df)).clamp(0.0, 1.0)
                } else if gh_q.is_finite() {
                    f64::NAN
                } else {
                    0.0
                };
                let gh_critical = if gh_df >= 1.0 {
                    studentized_range_quantile(confidence, g as f64, gh_df)
                } else {
                    f64::NAN
                };
                pairwise.push(Pairwise {
                    first: a.level.clone(),
                    second: b.level.clone(),
                    difference,
                    se,
                    t,
                    p_raw,
                    p_adjusted: (p_raw * comparisons as f64).clamp(0.0, 1.0),
                    q,
                    p_tukey,
                    tukey_lower: difference - tukey_critical * tukey_se,
                    tukey_upper: difference + tukey_critical * tukey_se,
                    ci_lower: difference - critical * se,
                    ci_upper: difference + critical * se,
                    lsd_lower: difference - lsd_critical * se,
                    lsd_upper: difference + lsd_critical * se,
                    gh_se,
                    gh_df,
                    gh_q,
                    gh_p,
                    gh_lower: difference - gh_critical * gh_se,
                    gh_upper: difference + gh_critical * gh_se,
                });
            }
        }
    }

    let mut fitted = Vec::with_capacity(n);
    let mut residuals = Vec::with_capacity(n);
    for (level, values) in &groups {
        let mean = summaries
            .iter()
            .find(|summary| summary.level == *level)
            .map_or(grand_mean, |summary| summary.mean);
        for value in values {
            fitted.push(mean);
            residuals.push(value - mean);
        }
    }

    Ok(AnovaFit {
        groups: summaries,
        grand_mean,
        ss_between,
        ss_within,
        ss_total,
        df_between,
        df_within,
        ms_between,
        ms_within,
        f,
        p_value,
        r_squared,
        adjusted_r_squared,
        pooled_sd,
        pairwise,
        fitted,
        residuals,
        n,
        excluded: df.n_rows() - n,
        confidence,
    })
}

/// Fits the ANOVA, builds the Session report and a residual plot.
pub fn analyze(
    df: &DataFrame,
    response: &str,
    factor: &str,
    confidence: f64,
) -> Result<AnalysisOutput> {
    let model = fit(df, response, factor, confidence)?;
    let mut result = report(response, factor, &model);
    result.notes.push((
        "Analysis".to_owned(),
        format!("One-Way ANOVA: {response} by {factor}"),
    ));
    let plot = residual_plot(response, &model);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn report(response: &str, factor: &str, model: &AnovaFit) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("One-Way ANOVA — {response} by {factor}"),
        format!(
            "Ordinary one-way ANOVA; {:.1}% group intervals",
            model.confidence * 100.0
        ),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Grand mean", model.grand_mean),
        ("Pooled standard deviation", model.pooled_sd),
        ("R²", model.r_squared),
        ("Adjusted R²", model.adjusted_r_squared),
        ("F", model.f),
        ("p-value", model.p_value),
        ("Between df", model.df_between),
        ("Within df", model.df_within),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Level".to_owned(),
            "n".to_owned(),
            "Mean".to_owned(),
            "StDev".to_owned(),
            format!("{:.1}% CI", model.confidence * 100.0),
        ],
        rows: model
            .groups
            .iter()
            .map(|group| {
                vec![
                    group.level.clone(),
                    group.n.to_string(),
                    format!("{:.6}", group.mean),
                    format!("{:.6}", group.sd),
                    format!("({:.6}, {:.6})", group.ci_lower, group.ci_upper),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Source".to_owned(),
            "DF".to_owned(),
            "SS".to_owned(),
            "MS".to_owned(),
            "F".to_owned(),
            "P".to_owned(),
        ],
        rows: vec![
            vec![
                "Factor".to_owned(),
                format!("{:.0}", model.df_between),
                format!("{:.6}", model.ss_between),
                format!("{:.6}", model.ms_between),
                format!("{:.6}", model.f),
                format!("{:.6}", model.p_value),
            ],
            vec![
                "Error".to_owned(),
                format!("{:.0}", model.df_within),
                format!("{:.6}", model.ss_within),
                format!("{:.6}", model.ms_within),
                String::new(),
                String::new(),
            ],
            vec![
                "Total".to_owned(),
                format!("{:.0}", model.df_between + model.df_within),
                format!("{:.6}", model.ss_total),
                String::new(),
                String::new(),
                String::new(),
            ],
        ],
    });
    if !model.pairwise.is_empty() {
        result.extra.push(Item::Table {
            headers: vec![
                "Comparison".to_owned(),
                "Difference".to_owned(),
                "q".to_owned(),
                "P (Tukey)".to_owned(),
                format!("{:.1}% simultaneous CI", model.confidence * 100.0),
            ],
            rows: model
                .pairwise
                .iter()
                .map(|pair| {
                    vec![
                        format!("{} − {}", pair.first, pair.second),
                        format!("{:.6}", pair.difference),
                        format!("{:.6}", pair.q),
                        format!("{:.6}", pair.p_tukey),
                        format!("({:.6}, {:.6})", pair.tukey_lower, pair.tukey_upper),
                    ]
                })
                .collect(),
        });
        result.extra.push(Item::Table {
            headers: vec![
                "Comparison".to_owned(),
                "Difference".to_owned(),
                "SE".to_owned(),
                "t".to_owned(),
                "P (Bonferroni)".to_owned(),
                format!("{:.1}% CI", model.confidence * 100.0),
            ],
            rows: model
                .pairwise
                .iter()
                .map(|pair| {
                    vec![
                        format!("{} − {}", pair.first, pair.second),
                        format!("{:.6}", pair.difference),
                        format!("{:.6}", pair.se),
                        format!("{:.6}", pair.t),
                        format!("{:.6}", pair.p_adjusted),
                        format!("({:.6}, {:.6})", pair.ci_lower, pair.ci_upper),
                    ]
                })
                .collect(),
        });
    }
    if !model.pairwise.is_empty() {
        result.extra.push(Item::Table {
            headers: vec![
                "Comparison".to_owned(),
                "Difference".to_owned(),
                "t".to_owned(),
                "P (LSD)".to_owned(),
                format!("{:.1}% LSD CI", model.confidence * 100.0),
            ],
            rows: model
                .pairwise
                .iter()
                .map(|pair| {
                    vec![
                        format!("{} − {}", pair.first, pair.second),
                        format!("{:.6}", pair.difference),
                        format!("{:.6}", pair.t),
                        format!("{:.6}", pair.p_raw),
                        format!("({:.6}, {:.6})", pair.lsd_lower, pair.lsd_upper),
                    ]
                })
                .collect(),
        });
        result.extra.push(Item::Table {
            headers: vec![
                "Comparison".to_owned(),
                "Difference".to_owned(),
                "SE".to_owned(),
                "df".to_owned(),
                "q".to_owned(),
                "P (Games-Howell)".to_owned(),
                format!("{:.1}% CI", model.confidence * 100.0),
            ],
            rows: model
                .pairwise
                .iter()
                .map(|pair| {
                    vec![
                        format!("{} − {}", pair.first, pair.second),
                        format!("{:.6}", pair.difference),
                        format!("{:.6}", pair.gh_se),
                        format!("{:.2}", pair.gh_df),
                        format!("{:.6}", pair.gh_q),
                        format!("{:.6}", pair.gh_p),
                        format!("({:.6}, {:.6})", pair.gh_lower, pair.gh_upper),
                    ]
                })
                .collect(),
        });
    }
    result.notes = vec![
        (
            "Model".to_owned(),
            "yᵢⱼ = µ + τⱼ + εᵢⱼ; independent groups, common within-group variance".to_owned(),
        ),
        (
            "Multiple comparisons".to_owned(),
            "four tables: Tukey HSD (studentized range, Tukey–Kramer for unequal sizes), \
             Bonferroni-adjusted t tests and Fisher's LSD with the pooled error mean square, \
             plus Games–Howell for unequal variances (Welch–Satterthwaite degrees of freedom)"
                .to_owned(),
        ),
        (
            "Missing data".to_owned(),
            "rows with a missing response or factor label are removed and counted".to_owned(),
        ),
        (
            "Plot".to_owned(),
            "residuals versus fitted group means; a horizontal line marks zero".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    result.warnings.push(
        "the ANOVA table and the pooled-SD means intervals match the documented report (n = 24, three levels); the multiple-comparison defaults have no documented output"
            .to_owned(),
    );
    result
}

fn residual_plot(response: &str, model: &AnovaFit) -> PlotSpec {
    let x_min = model.fitted.iter().copied().fold(f64::INFINITY, f64::min);
    let x_max = model
        .fitted
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let x_pad = ((x_max - x_min).abs() * 0.05).max(1e-9);
    let y_min = model.residuals.iter().copied().fold(0.0, f64::min);
    let y_max = model.residuals.iter().copied().fold(0.0, f64::max);
    let y_pad = ((y_max - y_min).abs() * 0.05).max(1e-9);
    PlotSpec {
        title: format!("Residuals vs Fitted — {response}"),
        x_axis: linear_axis("Fitted value", x_min - x_pad, x_max + x_pad),
        y_axis: linear_axis("Residual", y_min - y_pad, y_max + y_pad),
        size: (900, 520),
        points: model
            .fitted
            .iter()
            .zip(&model.residuals)
            .map(|(&x, &y)| Point { x, y })
            .collect(),
        lines: vec![Segment {
            from: Point {
                x: x_min - x_pad,
                y: 0.0,
            },
            to: Point {
                x: x_max + x_pad,
                y: 0.0,
            },
            width: 1.5,
        }],
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]),
            Column::new_text(
                "g",
                vec![
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "C".to_owned(),
                    "C".to_owned(),
                    "C".to_owned(),
                ],
            ),
        ])
        .unwrap()
    }

    #[test]
    fn one_way_anova_matches_hand_values() {
        let model = fit(&frame(), "y", "g", 0.95).expect("fit");
        assert_eq!(model.groups.len(), 3);
        assert!((model.grand_mean - 5.0).abs() < 1e-12);
        assert!((model.ss_between - 54.0).abs() < 1e-12);
        assert!((model.ss_within - 6.0).abs() < 1e-12);
        assert!((model.f - 27.0).abs() < 1e-12);
        assert!((model.r_squared - 0.9).abs() < 1e-12);
        assert_eq!(model.df_between, 2.0);
        assert_eq!(model.df_within, 6.0);
        assert!(model.p_value < 0.01);
    }

    #[test]
    fn groups_are_in_lexicographic_order_and_missing_rows_are_counted() {
        let df = DataFrame::new(vec![
            Column::new_f64("y", vec![3.0, 1.0, 2.0, 5.0, f64::NAN]),
            Column::new_text(
                "g",
                vec![
                    "B".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "A".to_owned(),
                ],
            ),
        ])
        .unwrap();
        let model = fit(&df, "y", "g", 0.95).expect("fit");
        assert_eq!(model.groups[0].level, "A");
        assert_eq!(model.excluded, 1);
    }

    #[test]
    fn single_observation_levels_are_rejected() {
        let df = DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0]),
            Column::new_text("g", vec!["A".to_owned(), "A".to_owned(), "B".to_owned()]),
        ])
        .unwrap();
        assert!(matches!(
            fit(&df, "y", "g", 0.95),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn pairwise_table_is_built() {
        let model = fit(&frame(), "y", "g", 0.95).expect("fit");
        assert_eq!(model.pairwise.len(), 3);
        assert!(
            model
                .pairwise
                .iter()
                .all(|pair| (0.0..=1.0).contains(&pair.p_adjusted))
        );
    }

    #[test]
    fn tukey_hsd_uses_the_studentized_range() {
        // Three groups of five: 10, 11, 12 with a pooled error mean square of 5
        // gives q = |diff| / sqrt(MSE/2·(1/5 + 1/5)).
        let df = DataFrame::new(vec![
            Column::new_f64(
                "y",
                vec![
                    8.0, 10.0, 12.0, 9.0, 11.0, 9.0, 11.0, 13.0, 10.0, 12.0, 8.0, 9.0, 10.0, 10.0,
                    13.0,
                ],
            ),
            Column::new_text(
                "g",
                vec![
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "C".to_owned(),
                    "C".to_owned(),
                    "C".to_owned(),
                    "C".to_owned(),
                    "C".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        let model = fit(&df, "y", "g", 0.95).expect("fit");
        let pair = &model.pairwise[0];
        let tukey_se = pair.se / 2f64.sqrt();
        assert!((pair.q - pair.difference.abs() / tukey_se).abs() < 1e-12);
        let expected_p = 1.0 - studentized_range_cdf(pair.q, 3.0, model.df_within);
        assert!((pair.p_tukey - expected_p).abs() < 1e-12);
        // The simultaneous interval is wider than a plain 95 % t interval
        // (t = 2.179 with 12 degrees of freedom), but narrower than the
        // Bonferroni interval for three groups (t = 2.779).
        let plain = student_t_quantile(0.975, model.df_within) * pair.se;
        assert!(pair.tukey_lower < pair.difference - plain);
        assert!(pair.tukey_upper > pair.difference + plain);
        assert!(pair.tukey_lower > pair.ci_lower - 1e-12);
        assert!(pair.tukey_upper < pair.ci_upper + 1e-12);
        // The Tukey p-value is never below the raw one.
        assert!(pair.p_tukey >= pair.p_raw - 1e-12);
        let text = analyze(&df, "y", "g", 0.95).expect("analysis").results[0].to_session_text();
        assert!(text.contains("Tukey HSD"), "{text}");
        assert!(text.contains("P (Tukey)"), "{text}");
    }

    #[test]
    fn games_howell_and_lsd_follow_their_definitions() {
        // Equal sizes and variances: Games–Howell reduces to Tukey's SE and q,
        // and its degrees of freedom are n_i + n_j − 2.
        let df = DataFrame::new(vec![
            Column::new_f64(
                "y",
                vec![
                    // Three groups with the same spread, so the pooled variance
                    // equals every group variance.
                    8.0, 10.0, 12.0, 9.0, 11.0, 9.0, 11.0, 13.0, 10.0, 12.0, 7.0, 9.0, 11.0, 8.0,
                    10.0,
                ],
            ),
            Column::new_text(
                "g",
                vec![
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "C".to_owned(),
                    "C".to_owned(),
                    "C".to_owned(),
                    "C".to_owned(),
                    "C".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        let model = fit(&df, "y", "g", 0.95).expect("fit");
        let pair = &model.pairwise[0];
        assert!(
            (pair.gh_se - pair.se / 2f64.sqrt()).abs() < 1e-9,
            "{pair:?}"
        );
        assert!((pair.gh_q - pair.q).abs() < 1e-9);
        assert!((pair.gh_df - 8.0).abs() < 1e-9, "df {}", pair.gh_df);
        // The degrees of freedom are smaller than Tukey's (12), so the
        // Games–Howell p-value is the more conservative of the two.
        assert!(pair.gh_p > pair.p_tukey, "{pair:?}");
        assert!((0.0..=1.0).contains(&pair.gh_p));
        // Fisher's LSD interval is narrower than the Tukey one and its p-value
        // is the unadjusted one.
        assert!(pair.lsd_lower > pair.tukey_lower && pair.lsd_upper < pair.tukey_upper);
        let text = analyze(&df, "y", "g", 0.95).expect("analysis").results[0].to_session_text();
        assert!(text.contains("P (Games-Howell)"), "{text}");
        assert!(text.contains("P (LSD)"), "{text}");
    }

    #[test]
    fn games_howell_widens_with_unequal_variances() {
        // The second group is much more variable: its Games–Howell degrees of
        // freedom fall well below the pooled ones.
        let df = DataFrame::new(vec![
            Column::new_f64(
                "y",
                vec![10.0, 10.1, 9.9, 10.0, 10.2, 5.0, 15.0, 10.0, 2.0, 18.0],
            ),
            Column::new_text(
                "g",
                vec![
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        let model = fit(&df, "y", "g", 0.95).expect("fit");
        let pair = &model.pairwise[0];
        assert!(
            pair.gh_df < model.df_within,
            "{} vs {}",
            pair.gh_df,
            model.df_within
        );
        assert!(pair.gh_se > pair.se / 2f64.sqrt());
    }

    #[test]
    fn report_and_residual_plot_are_built() {
        let out = analyze(&frame(), "y", "g", 0.95).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("One-Way ANOVA"), "{text}");
        assert!(text.contains("Bonferroni"), "{text}");
        assert_eq!(out.plot.expect("plot").points.len(), 9);
    }
}

#[cfg(test)]
mod group_intervals {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        let mut response = Vec::new();
        let mut factor = Vec::new();
        for (level, values) in [
            ("A", vec![9.8, 10.1, 10.3, 9.9, 10.0]),
            ("B", vec![10.6, 10.9, 10.7]),
            ("C", vec![11.4, 11.1, 11.6, 11.2]),
        ] {
            for value in values {
                response.push(value);
                factor.push(level.to_owned());
            }
        }
        DataFrame::new(vec![
            Column::new_f64("y", response),
            Column::new_text("g", factor),
        ])
        .expect("frame")
    }

    #[test]
    fn intervals_use_the_pooled_sd_and_the_error_df() {
        let model = fit(&frame(), "y", "g", 0.95).expect("fit");
        let critical = student_t_quantile(0.975, model.df_within);
        for group in &model.groups {
            let half = critical * model.pooled_sd / (group.n as f64).sqrt();
            assert!(
                (group.ci_upper - group.mean - half).abs() < 1e-12,
                "{}: upper {} vs mean {} + {half}",
                group.level,
                group.ci_upper,
                group.mean
            );
            assert!((group.mean - group.ci_lower - half).abs() < 1e-12);
        }
    }

    #[test]
    fn two_levels_use_the_same_pooled_rule() {
        let df = DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 1.2, 1.1, 2.4, 2.6, 2.5]),
            Column::new_text(
                "g",
                ["a", "a", "a", "b", "b", "b"]
                    .iter()
                    .map(|v| (*v).to_owned())
                    .collect(),
            ),
        ])
        .expect("frame");
        let model = fit(&df, "y", "g", 0.95).expect("fit");
        assert_eq!(model.df_within, 4.0);
        let half = student_t_quantile(0.975, 4.0) * model.pooled_sd / 3f64.sqrt();
        assert!((model.groups[0].ci_upper - model.groups[0].mean - half).abs() < 1e-12);
        assert!((model.groups[1].ci_lower - model.groups[1].mean + half).abs() < 1e-12);
    }

    #[test]
    fn a_single_observation_level_is_still_rejected() {
        let df = DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 1.1, 2.0]),
            Column::new_text(
                "g",
                ["a", "a", "b"].iter().map(|v| (*v).to_owned()).collect(),
            ),
        ])
        .expect("frame");
        assert!(fit(&df, "y", "g", 0.95).is_err());
    }
}
