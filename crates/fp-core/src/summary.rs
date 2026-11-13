// SPDX-License-Identifier: MIT
//! Graphical summary and stored descriptive statistics.
//!
//! The graphical summary arranges the existing descriptive statistics into a
//! histogram, a boxplot and the confidence intervals of the mean, the median
//! and the standard deviation. The store command writes the descriptive
//! statistics of the selected variables as a table of new worksheet columns.

use crate::data::{Column, ColumnSummary, DataFrame, MissingPolicy};
use crate::error::{Error, Result};
use crate::plot::{PlotNote, PlotSpec, Point, Segment, linear_axis, panel_plot};
use crate::workflow::AnalysisOutput;

/// One descriptive statistic that can be stored in the worksheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreStat {
    /// Valid observations.
    N,
    /// Missing or non-finite observations.
    NMissing,
    /// Arithmetic mean.
    Mean,
    /// Standard error of the mean.
    SeMean,
    /// Sample standard deviation (`n − 1`).
    StDev,
    /// Minimum.
    Minimum,
    /// First quartile.
    Q1,
    /// Median.
    Median,
    /// Third quartile.
    Q3,
    /// Maximum.
    Maximum,
    /// Interquartile range.
    Iqr,
    /// Lower confidence bound of the mean.
    CiMeanLower,
    /// Upper confidence bound of the mean.
    CiMeanUpper,
    /// Lower confidence bound of the median.
    CiMedianLower,
    /// Upper confidence bound of the median.
    CiMedianUpper,
    /// Lower confidence bound of the standard deviation.
    CiSdLower,
    /// Upper confidence bound of the standard deviation.
    CiSdUpper,
}

impl StoreStat {
    /// Name of the stored column.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::N => "N",
            Self::NMissing => "N missing",
            Self::Mean => "Mean",
            Self::SeMean => "SE Mean",
            Self::StDev => "StDev",
            Self::Minimum => "Minimum",
            Self::Q1 => "Q1",
            Self::Median => "Median",
            Self::Q3 => "Q3",
            Self::Maximum => "Maximum",
            Self::Iqr => "IQR",
            Self::CiMeanLower => "Mean CI lower",
            Self::CiMeanUpper => "Mean CI upper",
            Self::CiMedianLower => "Median CI lower",
            Self::CiMedianUpper => "Median CI upper",
            Self::CiSdLower => "StDev CI lower",
            Self::CiSdUpper => "StDev CI upper",
        }
    }

    /// Value of the statistic for one summary, when it is defined.
    #[must_use]
    pub fn value(self, summary: &ColumnSummary) -> Option<f64> {
        match self {
            Self::N => Some(summary.n as f64),
            Self::NMissing => Some(summary.excluded as f64),
            Self::Mean => Some(summary.mean),
            Self::SeMean => summary.se_mean,
            Self::StDev => summary.sd,
            Self::Minimum => Some(summary.min),
            Self::Q1 => Some(summary.q1),
            Self::Median => Some(summary.median),
            Self::Q3 => Some(summary.q3),
            Self::Maximum => Some(summary.max),
            Self::Iqr => Some(summary.iqr),
            Self::CiMeanLower => summary.ci_mean.map(|(lower, _)| lower),
            Self::CiMeanUpper => summary.ci_mean.map(|(_, upper)| upper),
            Self::CiMedianLower => summary.ci_median.map(|(lower, _)| lower),
            Self::CiMedianUpper => summary.ci_median.map(|(_, upper)| upper),
            Self::CiSdLower => summary.ci_sd.map(|(lower, _)| lower),
            Self::CiSdUpper => summary.ci_sd.map(|(_, upper)| upper),
        }
    }
}

/// A name that the worksheet does not use yet.
fn unique_name(df: &DataFrame, base: &str) -> String {
    if df.column(base).is_err() {
        return base.to_owned();
    }
    for suffix in 2.. {
        let candidate = format!("{base} {suffix}");
        if df.column(&candidate).is_err() {
            return candidate;
        }
    }
    base.to_owned()
}

/// Writes the descriptive statistics of several variables into new columns.
///
/// One row is added per variable, at the end of the worksheet, with a
/// `Variable` column and one column per requested statistic. The returned
/// names are the columns that were added, in order.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for an empty variable or statistic list and
/// the column errors of the summaries and of the worksheet extension.
pub fn store_descriptive_statistics(
    df: &mut DataFrame,
    columns: &[String],
    stats: &[StoreStat],
    confidence: f64,
) -> Result<Vec<String>> {
    if columns.is_empty() {
        return Err(Error::Unsupported(
            "choose at least one variable to summarise".to_owned(),
        ));
    }
    if stats.is_empty() {
        return Err(Error::Unsupported(
            "choose at least one statistic to store".to_owned(),
        ));
    }
    let mut summaries = Vec::with_capacity(columns.len());
    for name in columns {
        let sample = df.column(name)?.sample(MissingPolicy::Drop)?;
        let summary = ColumnSummary::from_sample(&sample, confidence)?;
        summaries.push((name.clone(), summary));
    }
    let count = summaries.len();
    for _ in 0..count {
        df.add_row()?;
    }
    let total = df.n_rows();
    let start = total - count;

    let mut added = Vec::with_capacity(stats.len() + 1);
    let variable_name = unique_name(df, "Variable");
    let mut variable_values = vec![String::new(); total];
    for (index, (name, _)) in summaries.iter().enumerate() {
        variable_values[start + index] = name.clone();
    }
    df.insert_column(
        df.n_columns(),
        Column::new_text(variable_name.clone(), variable_values),
    )?;
    added.push(variable_name);

    for stat in stats {
        let name = unique_name(df, stat.name());
        let mut values = vec![f64::NAN; total];
        for (index, (_, summary)) in summaries.iter().enumerate() {
            if let Some(value) = stat.value(summary) {
                values[start + index] = value;
            }
        }
        df.insert_column(df.n_columns(), Column::new_f64(name.clone(), values))?;
        added.push(name);
    }
    Ok(added)
}

/// Builds the graphical summary of a column.
///
/// The result is the descriptive-statistics report; the plot stacks the
/// histogram, the boxplot and the confidence intervals of the mean, the median
/// and the standard deviation.
///
/// # Errors
///
/// Returns the errors of the descriptive statistics, the histogram and the
/// boxplot.
pub fn graphical_summary(df: &DataFrame, column_name: &str) -> Result<AnalysisOutput> {
    let result = crate::workflow::descriptive_statistics(df, column_name)?;
    let sample = df.column(column_name)?.sample(MissingPolicy::Drop)?;
    let summary = ColumnSummary::from_sample(&sample, 0.95)?;

    let mut histogram = crate::histogram::analyze(df, column_name, None)?
        .plot
        .ok_or_else(|| Error::Unsupported("the histogram could not be drawn".to_owned()))?;
    histogram.title = "Histogram".to_owned();
    let mut boxplot = crate::boxplot::analyze(df, column_name, None)?
        .plot
        .ok_or_else(|| Error::Unsupported("the boxplot could not be drawn".to_owned()))?;
    boxplot.title = "Boxplot".to_owned();
    let intervals = interval_plot(&summary);

    let plot = panel_plot(
        vec![histogram, boxplot, intervals],
        1,
        format!("Graphical Summary — {column_name}"),
    )
    .ok_or_else(|| Error::Unsupported("the graphical summary could not be assembled".to_owned()))?;
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn interval_plot(summary: &ColumnSummary) -> PlotSpec {
    let mut entries: Vec<(&str, f64, Option<(f64, f64)>)> = vec![
        ("Mean", summary.mean, summary.ci_mean),
        ("Median", summary.median, summary.ci_median),
    ];
    if let Some(sd) = summary.sd {
        entries.push(("StDev", sd, summary.ci_sd));
    }
    let mut x_min = entries
        .iter()
        .map(|(_, estimate, interval)| {
            interval.map_or(*estimate, |(lower, upper)| lower.min(estimate).min(upper))
        })
        .fold(f64::INFINITY, f64::min);
    let mut x_max = entries
        .iter()
        .map(|(_, estimate, interval)| {
            interval.map_or(*estimate, |(lower, upper)| upper.max(estimate).max(lower))
        })
        .fold(f64::NEG_INFINITY, f64::max);
    if !x_min.is_finite() || !x_max.is_finite() {
        x_min = 0.0;
        x_max = 1.0;
    }
    let pad = ((x_max - x_min).abs() * 0.08).max(1e-9);
    x_min -= pad;
    x_max += pad;
    let mut lines = Vec::new();
    let mut points = Vec::new();
    let mut note = Vec::new();
    for (index, (label, estimate, interval)) in entries.iter().enumerate() {
        let y = (entries.len() - index) as f64;
        if let Some((lower, upper)) = interval {
            lines.push(Segment {
                from: Point { x: *lower, y },
                to: Point { x: *upper, y },
                width: 2.5,
            });
        }
        points.push(Point { x: *estimate, y });
        note.push((
            *label,
            interval.map_or_else(
                || format!("{estimate:.6}"),
                |(lower, upper)| format!("{estimate:.6} ({lower:.6}, {upper:.6})"),
            ),
        ));
    }
    PlotSpec {
        title: "95% intervals".to_owned(),
        x_axis: linear_axis("Value", x_min, x_max),
        y_axis: linear_axis("Statistic", 0.5, entries.len() as f64 + 0.5),
        size: (900, 320),
        points,
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: vec![PlotNote::new(note)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("weight", vec![10.0, 11.0, 12.0, 13.0, 14.0]),
            Column::new_f64("height", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
        ])
        .expect("frame")
    }

    #[test]
    fn the_graphical_summary_has_three_panels() {
        let out = graphical_summary(&frame(), "weight").expect("summary");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Descriptive Statistics"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.panels.len(), 3);
        assert_eq!(plot.panels[0].title, "Histogram");
        assert_eq!(plot.panels[1].title, "Boxplot");
        assert_eq!(plot.panels[2].title, "95% intervals");
        assert!(plot.panels[2].points.len() >= 2);
    }

    #[test]
    fn the_statistics_are_stored_one_row_per_variable() {
        let mut df = frame();
        let added = store_descriptive_statistics(
            &mut df,
            &["weight".to_owned(), "height".to_owned()],
            &[StoreStat::N, StoreStat::Mean, StoreStat::StDev],
            0.95,
        )
        .expect("store");
        assert_eq!(added, vec!["Variable", "N", "Mean", "StDev"]);
        assert_eq!(df.n_rows(), 7);
        assert_eq!(df.n_columns(), 5);
        let variable = df.column("Variable").expect("variable");
        assert_eq!(variable.label(5).as_deref(), Some("weight"));
        assert_eq!(variable.label(6).as_deref(), Some("height"));
        let n = df.column("N").expect("N");
        assert_eq!(n.value(5), Some(5.0));
        let mean = df.column("Mean").expect("Mean");
        assert_eq!(mean.value(5), Some(12.0));
        assert_eq!(mean.value(6), Some(3.0));
        let sd = df.column("StDev").expect("StDev");
        assert!((sd.value(5).expect("sd") - 1.581_138_830_084_189_3).abs() < 1e-12);
    }

    #[test]
    fn a_duplicate_name_gets_a_suffix() {
        let mut df = frame();
        df.insert_column(
            df.n_columns(),
            Column::new_f64("Mean", vec![0.0; df.n_rows()]),
        )
        .expect("column");
        let added =
            store_descriptive_statistics(&mut df, &["weight".to_owned()], &[StoreStat::Mean], 0.95)
                .expect("store");
        assert_eq!(added, vec!["Variable", "Mean 2"]);
    }

    #[test]
    fn empty_selections_are_rejected() {
        let mut df = frame();
        assert!(store_descriptive_statistics(&mut df, &[], &[StoreStat::Mean], 0.95).is_err());
        assert!(store_descriptive_statistics(&mut df, &["weight".to_owned()], &[], 0.95).is_err());
        assert!(
            store_descriptive_statistics(
                &mut df,
                &["missing".to_owned()],
                &[StoreStat::Mean],
                0.95
            )
            .is_err()
        );
    }
}
