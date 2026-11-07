// SPDX-License-Identifier: MIT
//! Interval plot: the mean of each level with its confidence interval.
//!
//! The plot reuses the one-way ANOVA summaries: for every level of the factor
//! it draws a vertical segment from the lower to the upper confidence bound of
//! the mean and a point at the mean, with a horizontal line at the grand mean
//! for reference. The level means and their intervals come from the Student-t
//! interval of each group (`s/√n` with `n − 1` degrees of freedom).
//!
//! The documented behaviour has an interval plot; its defaults were not documented,
//! so the numbers are not claimed to match the documented behaviour — see the project notes.

use crate::anova;
use crate::data::DataFrame;
use crate::error::Result;
use crate::plot::{AxisSpec, PlotSpec, Point, Scale, Segment, Tick, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// Builds the interval plot and its report.
pub fn analyze(
    df: &DataFrame,
    response: &str,
    factor: &str,
    confidence: f64,
) -> Result<AnalysisOutput> {
    let model = anova::fit(df, response, factor, confidence)?;
    let groups = model.groups.len();
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for group in &model.groups {
        low = low.min(group.ci_lower);
        high = high.max(group.ci_upper);
    }
    low = low.min(model.grand_mean);
    high = high.max(model.grand_mean);
    let pad = ((high - low).abs() * 0.08).max(1e-9);

    let mut points = Vec::with_capacity(groups);
    let mut lines = Vec::with_capacity(groups * 2 + 1);
    for (index, group) in model.groups.iter().enumerate() {
        let x = index as f64 + 1.0;
        points.push(Point { x, y: group.mean });
        // The interval as a vertical segment, with short caps at both ends.
        lines.push(Segment {
            from: Point {
                x,
                y: group.ci_lower,
            },
            to: Point {
                x,
                y: group.ci_upper,
            },
            width: 2.0,
        });
        for bound in [group.ci_lower, group.ci_upper] {
            lines.push(Segment {
                from: Point {
                    x: x - 0.08,
                    y: bound,
                },
                to: Point {
                    x: x + 0.08,
                    y: bound,
                },
                width: 2.0,
            });
        }
    }
    // The grand mean as a dashed-looking thin line across the plot.
    lines.push(Segment {
        from: Point {
            x: 0.5,
            y: model.grand_mean,
        },
        to: Point {
            x: groups as f64 + 0.5,
            y: model.grand_mean,
        },
        width: 1.0,
    });

    let step = if groups <= 12 { 1 } else { groups.div_ceil(12) };
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
        max: groups as f64 + 0.5,
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
    let plot = PlotSpec {
        title: format!("Interval Plot — {response} by {factor}"),
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
    };

    let mut result = AnalysisResult::new(
        format!("Interval Plot — response {response} by {factor}"),
        format!(
            "Level means with {:.1}% Student-t intervals and the grand mean",
            confidence * 100.0
        ),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Levels", groups as f64),
        ("Grand mean", model.grand_mean),
        ("Pooled standard deviation", model.pooled_sd),
        ("F", model.f),
        ("F p-value", model.p_value),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Level".to_owned(),
            "n".to_owned(),
            "Mean".to_owned(),
            "StDev".to_owned(),
            "Lower".to_owned(),
            "Upper".to_owned(),
        ],
        rows: model
            .groups
            .iter()
            .map(|group| {
                vec![
                    group.level.clone(),
                    format!("{}", group.n),
                    format!("{:.6}", group.mean),
                    format!("{:.6}", group.sd),
                    format!("{:.6}", group.ci_lower),
                    format!("{:.6}", group.ci_upper),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Intervals".to_owned(),
            "each interval is the Student-t interval of the level mean, s/√n with n − 1 \
             degrees of freedom"
                .to_owned(),
        ),
        (
            "Grand mean".to_owned(),
            "the thin horizontal line is the mean of every observation".to_owned(),
        ),
        (
            "Overlap".to_owned(),
            "overlapping intervals suggest that the levels are not distinguishable, but the \
             formal answer is the F test or a multiple-comparison table"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the interval convention is the standard Student-t interval and has not been verified \
         against documented output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        let mut values = Vec::new();
        let mut labels = Vec::new();
        for (index, level) in ["A", "B", "C"].iter().enumerate() {
            for point in 0..6 {
                values.push(10.0 + index as f64 * 2.0 + point as f64 * 0.1);
                labels.push((*level).to_owned());
            }
        }
        DataFrame::new(vec![
            Column::new_f64("y", values),
            Column::new_text("g", labels),
        ])
        .expect("frame")
    }

    #[test]
    fn the_plot_has_one_interval_per_level() {
        let out = analyze(&frame(), "y", "g", 0.95).expect("analysis");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.points.len(), 3);
        // One segment per level, two caps each, plus the grand mean line.
        assert_eq!(plot.lines.len(), 3 * 3 + 1);
        assert_eq!(plot.x_axis.label, "g");
        let labels: Vec<&str> = plot.x_axis.ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, vec!["A", "B", "C"]);
        // The grand mean sits between the extreme means.
        let grand = plot.lines.last().expect("grand mean").from.y;
        let means: Vec<f64> = plot.points.iter().map(|p| p.y).collect();
        assert!(grand > means[0] && grand < means[2]);
    }

    #[test]
    fn the_report_lists_the_intervals() {
        let out = analyze(&frame(), "y", "g", 0.95).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Interval Plot"), "{text}");
        assert!(text.contains("Lower"), "{text}");
        assert!(text.contains("Grand mean"), "{text}");
        assert_eq!(out.results[0].statistics[0].1, 3.0);
    }
}
