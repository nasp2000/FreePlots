//! Pareto chart: the categories of a column, sorted by frequency.
//!
//! The categories are counted, sorted by decreasing count (ties in
//! lexicographic order) and reported with their percentage and cumulative
//! percentage. The chart draws one **bar** per category — filled bands between
//! zero and the count — so the bars and the rest of the graphics share the same
//! axis model.
//!
//! The report also gives the **vital few**: the number of categories needed to
//! reach 80 % of the total, the classical Pareto rule.
//!
//! The comparison run has a Pareto chart; its ordering and labelling
//! conventions were not captured, so the numbers are not claimed as reference
//! parity — see `docs/parity.md`.

use std::collections::BTreeMap;

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{AxisSpec, Band, PlotSpec, Point, Scale, Segment, Series, Tick, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// Fitted Pareto summary.
#[derive(Debug, Clone, PartialEq)]
pub struct Pareto {
    /// Categories, from the most to the least frequent.
    pub categories: Vec<String>,
    /// Count of each category.
    pub counts: Vec<f64>,
    /// Percentage of the total.
    pub percents: Vec<f64>,
    /// Cumulative percentage.
    pub cumulative: Vec<f64>,
    /// Total count.
    pub total: f64,
    /// Categories needed to reach 80 % of the total.
    pub vital_few: usize,
    /// Rows with a missing label (not counted).
    pub excluded: usize,
}

/// Counts the categories of a column.
pub fn fit(df: &DataFrame, column_name: &str) -> Result<Pareto> {
    let column = df.column(column_name)?;
    let mut counts: BTreeMap<String, f64> = BTreeMap::new();
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        if column.is_missing(row) {
            excluded += 1;
            continue;
        }
        *counts.entry(column.cell_text(row)).or_insert(0.0) += 1.0;
    }
    if counts.is_empty() {
        return Err(Error::EmptySample);
    }
    if counts.len() < 2 {
        return Err(Error::Unsupported(
            "the Pareto chart needs at least two categories".to_owned(),
        ));
    }
    let mut entries: Vec<(String, f64)> = counts.into_iter().collect();
    entries.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    let total: f64 = entries.iter().map(|(_, count)| count).sum();
    let mut percents = Vec::with_capacity(entries.len());
    let mut cumulative = Vec::with_capacity(entries.len());
    let mut running = 0.0;
    let mut vital_few = entries.len();
    let mut reached = false;
    for (index, (_, count)) in entries.iter().enumerate() {
        let percent = 100.0 * count / total;
        percents.push(percent);
        running += percent;
        cumulative.push(running.min(100.0));
        if !reached && cumulative[index] >= 80.0 - 1e-9 {
            vital_few = index + 1;
            reached = true;
        }
    }
    Ok(Pareto {
        categories: entries.iter().map(|(label, _)| label.clone()).collect(),
        counts: entries.iter().map(|(_, count)| *count).collect(),
        percents,
        cumulative,
        total,
        vital_few,
        excluded,
    })
}

/// Chart of the bars.
fn plot(model: &Pareto) -> PlotSpec {
    let high = model
        .counts
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let categories = model.counts.len() as f64;
    let bands = model
        .counts
        .iter()
        .enumerate()
        .map(|(index, count)| {
            let centre = index as f64 + 1.0;
            let half = 0.35;
            Band {
                lower: vec![
                    Point {
                        x: centre - half,
                        y: 0.0,
                    },
                    Point {
                        x: centre + half,
                        y: 0.0,
                    },
                ],
                upper: vec![
                    Point {
                        x: centre - half,
                        y: *count,
                    },
                    Point {
                        x: centre + half,
                        y: *count,
                    },
                ],
            }
        })
        .collect();
    // The cumulative percentage lives on a right-hand axis from 0 to 100 %.
    let cumulative_points: Vec<Point> = model
        .cumulative
        .iter()
        .enumerate()
        .map(|(index, value)| Point {
            x: index as f64 + 1.0,
            y: *value,
        })
        .collect();
    let mut cumulative_lines = Vec::new();
    for pair in cumulative_points.windows(2) {
        cumulative_lines.push(Segment {
            from: pair[0],
            to: pair[1],
            width: 2.0,
        });
    }
    let secondary = AxisSpec {
        label: "Cumulative %".to_owned(),
        scale: Scale::Linear,
        min: 0.0,
        max: 100.0,
        ticks: (0..=5)
            .map(|step| Tick {
                position: f64::from(step) * 20.0,
                label: format!("{}", step * 20),
            })
            .collect(),
        minor: Vec::new(),
    };
    // The x axis carries the category names themselves: the axis model already
    // supports arbitrary ticks, so no categorical scale is needed. Names are
    // shortened and thinned out when there are many categories.
    let step = if categories <= 12.0 {
        1
    } else {
        (categories / 12.0).ceil() as usize
    };
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
        label: "Category".to_owned(),
        scale: Scale::Linear,
        min: 0.4,
        max: categories + 0.6,
        ticks: model
            .categories
            .iter()
            .enumerate()
            .filter(|(index, _)| index % step == 0)
            .map(|(index, label)| Tick {
                position: index as f64 + 1.0,
                label: shorten(label),
            })
            .collect(),
        minor: Vec::new(),
    };
    PlotSpec {
        title: "Pareto Chart".to_owned(),
        x_axis,
        y_axis: linear_axis("Count", 0.0, (high * 1.05).max(1.0)),
        size: (900, 540),
        points: model
            .counts
            .iter()
            .enumerate()
            .map(|(index, count)| Point {
                x: index as f64 + 1.0,
                y: *count,
            })
            .collect(),
        lines: Vec::new(),
        bands,
        series: vec![Series {
            label: "Cumulative %".to_owned(),
            points: cumulative_points,
            lines: cumulative_lines,
            bands: Vec::new(),
            secondary_axis: true,
        }],
        panels: Vec::new(),
        panel_columns: 0,
        secondary: Some(secondary),
    }
}

/// Runs the analysis and builds the report and the chart.
pub fn analyze(df: &DataFrame, column_name: &str) -> Result<AnalysisOutput> {
    let model = fit(df, column_name)?;
    let mut result = AnalysisResult::new(
        format!("Pareto Chart — column {column_name}"),
        "Categories sorted by frequency with the cumulative percentage".to_owned(),
    );
    result.n = model.total as usize;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Categories", model.categories.len() as f64),
        ("Total", model.total),
        ("Vital few (80%)", model.vital_few as f64),
        (
            "Largest share (%)",
            model.percents.first().copied().unwrap_or(f64::NAN),
        ),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Rank".to_owned(),
            "Category".to_owned(),
            "Count".to_owned(),
            "Percent".to_owned(),
            "Cumulative %".to_owned(),
        ],
        rows: model
            .categories
            .iter()
            .enumerate()
            .map(|(index, category)| {
                vec![
                    format!("{}", index + 1),
                    category.clone(),
                    format!("{:.0}", model.counts[index]),
                    format!("{:.2}", model.percents[index]),
                    format!("{:.2}", model.cumulative[index]),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Message(format!(
        "{} of the {} categories carry 80% of the occurrences (the vital few).",
        model.vital_few,
        model.categories.len()
    )));
    result.notes = vec![
        (
            "Ordering".to_owned(),
            "decreasing count; ties are broken in lexicographic order".to_owned(),
        ),
        (
            "Chart".to_owned(),
            "one bar per category in the order of the table; the x axis is the rank".to_owned(),
        ),
        (
            "Vital few".to_owned(),
            "number of categories needed to reach 80% of the total".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "cells without a label are not counted and are reported as excluded".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the ordering and the 80% rule are standard choices and have not been verified against \
         captured output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot(&model)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        let labels = vec![
            "scratch", "scratch", "scratch", "scratch", "scratch", "dent", "dent", "dent", "crack",
            "crack", "paint", "other",
        ];
        DataFrame::new(vec![Column::new_text(
            "defect",
            labels.into_iter().map(str::to_owned).collect(),
        )])
        .expect("frame")
    }

    #[test]
    fn the_categories_are_sorted_and_cumulated() {
        let model = fit(&frame(), "defect").expect("fit");
        assert_eq!(
            model.categories,
            vec!["scratch", "dent", "crack", "other", "paint"]
        );
        assert_eq!(model.counts, vec![5.0, 3.0, 2.0, 1.0, 1.0]);
        assert!((model.total - 12.0).abs() < 1e-12);
        let total: f64 = model.percents.iter().sum();
        assert!((total - 100.0).abs() < 1e-9, "{total}");
        for pair in model.cumulative.windows(2) {
            assert!(pair[0] <= pair[1] + 1e-12);
        }
        assert!((model.cumulative.last().copied().unwrap_or(0.0) - 100.0).abs() < 1e-9);
        // 5/12 + 3/12 = 66.7 %, plus 2/12 = 83.3 %: three categories reach 80 %.
        assert_eq!(model.vital_few, 3);
    }

    #[test]
    fn a_single_category_is_rejected() {
        let df = DataFrame::new(vec![Column::new_text(
            "defect",
            vec!["a".to_owned(), "a".to_owned()],
        )])
        .expect("frame");
        assert!(matches!(fit(&df, "defect"), Err(Error::Unsupported(_))));
    }

    #[test]
    fn the_report_and_the_chart_are_built() {
        let out = analyze(&frame(), "defect").expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Pareto Chart"), "{text}");
        assert!(text.contains("Vital few"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.bands.len(), 5);
        assert_eq!(plot.points.len(), 5);
        // The cumulative percentage goes to the right-hand axis.
        let secondary = plot.secondary.as_ref().expect("secondary axis");
        assert_eq!(secondary.label, "Cumulative %");
        assert_eq!(plot.series.len(), 1);
        assert!(plot.series[0].secondary_axis);
        assert_eq!(plot.series[0].points.len(), 5);
        // The bands become the bars of the rendered chart.
        let svg = crate::plot::render_svg(&plot);
        assert!(svg.contains("<svg"));
        assert!(!svg.contains("NaN"));
        assert!(
            svg.contains("Cumulative %"),
            "the right axis label is missing"
        );
        assert!(svg.contains(">80<"), "the right axis ticks are missing");
        // The category names are the ticks of the x axis.
        assert!(svg.contains(">scratch<"), "the category tick is missing");
        assert_eq!(plot.x_axis.label, "Category");
    }
}
