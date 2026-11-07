// SPDX-License-Identifier: MIT
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
//! The documented behaviour has a Pareto chart; its ordering and labelling
//! conventions were not documented, so the numbers are not claimed as documented
//! behaviour — see `the project notes`.

use std::collections::BTreeMap;

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{
    AxisSpec, Band, PlotNote, PlotSpec, Point, Scale, Segment, Series, Tick, linear_axis,
};
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
    /// Rows that contributed to the totals (label and weight present).
    pub observations: usize,
    /// Whether a weight column was used.
    pub weighted: bool,
}

/// Counts the categories of a column.
pub fn fit(df: &DataFrame, column_name: &str) -> Result<Pareto> {
    fit_with(df, column_name, None)
}

/// Counts the categories of a column, adding the numeric values of `weight`
/// instead of one per row.
pub fn fit_weighted(df: &DataFrame, column_name: &str, weight: &str) -> Result<Pareto> {
    fit_with(df, column_name, Some(weight))
}

/// Shared implementation: `weight` names a numeric column, or is empty for the
/// classical count.
fn fit_with(df: &DataFrame, column_name: &str, weight: Option<&str>) -> Result<Pareto> {
    let column = df.column(column_name)?;
    let weight_column = match weight.filter(|name| !name.is_empty()) {
        Some(name) => {
            let column = df.column(name)?;
            match column.dtype() {
                crate::data::DType::Float | crate::data::DType::Integer => Some(column),
                _ => {
                    return Err(Error::Unsupported(
                        "the weight column must be numeric".to_owned(),
                    ));
                }
            }
        }
        None => None,
    };
    let mut counts: BTreeMap<String, f64> = BTreeMap::new();
    let mut excluded = 0usize;
    let mut observations = 0usize;
    for row in 0..df.n_rows() {
        if column.is_missing(row) {
            excluded += 1;
            continue;
        }
        let value = match weight_column {
            None => 1.0,
            Some(weights) => {
                let Some(value) = weights.value(row) else {
                    excluded += 1;
                    continue;
                };
                if value < 0.0 {
                    return Err(Error::Unsupported(
                        "the weights must not be negative".to_owned(),
                    ));
                }
                value
            }
        };
        *counts.entry(column.cell_text(row)).or_insert(0.0) += value;
        observations += 1;
    }
    // A category whose weights add up to zero carries no information; the
    // same holds for the whole sample.
    counts.retain(|_, count| *count > 0.0);
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
        observations,
        weighted: weight_column.is_some(),
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
        y_axis: linear_axis(
            if model.weighted { "Weight" } else { "Count" },
            0.0,
            (high * 1.05).max(1.0),
        ),
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
        notes: Vec::new(),
    }
}

/// Runs the analysis and builds the report and the chart.
pub fn analyze(df: &DataFrame, column_name: &str) -> Result<AnalysisOutput> {
    build(fit(df, column_name)?, column_name, None)
}

/// Weighted variant: each row counts by the numeric value of `weight`.
pub fn analyze_weighted(df: &DataFrame, column_name: &str, weight: &str) -> Result<AnalysisOutput> {
    build(
        fit_weighted(df, column_name, weight)?,
        column_name,
        Some(weight),
    )
}

fn build(model: Pareto, column_name: &str, weight: Option<&str>) -> Result<AnalysisOutput> {
    let mut result = AnalysisResult::new(
        match weight {
            Some(weight) => format!("Pareto Chart — column {column_name}, weighted by {weight}"),
            None => format!("Pareto Chart — column {column_name}"),
        },
        match weight {
            Some(_) => "Categories sorted by weighted frequency with the cumulative percentage",
            None => "Categories sorted by frequency with the cumulative percentage",
        }
        .to_owned(),
    );
    result.n = model.observations;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Categories", model.categories.len() as f64),
        (
            if model.weighted {
                "Total weight"
            } else {
                "Total"
            },
            model.total,
        ),
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
            if model.weighted { "Weight" } else { "Count" }.to_owned(),
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
                    if model.weighted {
                        format!("{:.3}", model.counts[index])
                    } else {
                        format!("{:.0}", model.counts[index])
                    },
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
            match weight {
                Some(_) => {
                    "rows without a label or a weight are not counted and are reported as excluded"
                }
                None => "cells without a label are not counted and are reported as excluded",
            }
            .to_owned(),
        ),
    ];
    if let Some(weight) = weight {
        result.notes.push((
            "Weight".to_owned(),
            format!(
                "each row adds the value of {weight} instead of one; weights must not be negative"
            ),
        ));
    }
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the ordering and the 80% rule are standard choices and have not been verified against \
         documented output"
            .to_owned(),
    );
    let mut chart = plot(&model);
    chart.notes.push(PlotNote::new(vec![
        ("Categories", model.categories.len().to_string()),
        (
            if model.weighted {
                "Total weight"
            } else {
                "Total"
            },
            if model.weighted {
                format!("{:.3}", model.total)
            } else {
                format!("{:.0}", model.total)
            },
        ),
        ("Vital few", model.vital_few.to_string()),
        (
            "Largest share",
            format!("{:.2}%", model.percents.first().copied().unwrap_or(0.0)),
        ),
    ]));
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(chart),
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

    fn weighted_frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_text(
                "defect",
                vec![
                    "scratch".to_owned(),
                    "scratch".to_owned(),
                    "dent".to_owned(),
                    "crack".to_owned(),
                    "crack".to_owned(),
                ],
            ),
            Column::new_f64("cost", vec![10.0, 5.0, 20.0, 2.0, 3.0]),
        ])
        .expect("frame")
    }

    #[test]
    fn the_weights_replace_the_count() {
        let model = fit_weighted(&weighted_frame(), "defect", "cost").expect("fit");
        assert!(model.weighted);
        assert_eq!(model.observations, 5);
        assert_eq!(model.total, 40.0);
        // dent (20) now beats scratch (15), which has more rows.
        assert_eq!(model.categories, vec!["dent", "scratch", "crack"]);
        assert_eq!(model.counts, vec![20.0, 15.0, 5.0]);
        assert!((model.percents[0] - 50.0).abs() < 1e-9);
        assert!((model.cumulative[2] - 100.0).abs() < 1e-9);
        assert_eq!(model.vital_few, 2);
    }

    #[test]
    fn a_missing_weight_excludes_the_row() {
        let df = DataFrame::new(vec![
            Column::new_text(
                "defect",
                vec!["a".to_owned(), "a".to_owned(), "b".to_owned()],
            ),
            Column::new_f64("cost", vec![2.0, f64::NAN, 1.0]),
        ])
        .expect("frame");
        let model = fit_weighted(&df, "defect", "cost").expect("fit");
        assert_eq!(model.excluded, 1);
        assert_eq!(model.observations, 2);
        assert_eq!(model.total, 3.0);
    }

    #[test]
    fn an_invalid_weight_is_rejected() {
        let negative = DataFrame::new(vec![
            Column::new_text("defect", vec!["a".to_owned(), "b".to_owned()]),
            Column::new_f64("cost", vec![1.0, -2.0]),
        ])
        .expect("frame");
        assert!(matches!(
            fit_weighted(&negative, "defect", "cost"),
            Err(Error::Unsupported(_))
        ));

        let text = DataFrame::new(vec![
            Column::new_text("defect", vec!["a".to_owned(), "b".to_owned()]),
            Column::new_text("cost", vec!["1".to_owned(), "2".to_owned()]),
        ])
        .expect("frame");
        assert!(matches!(
            fit_weighted(&text, "defect", "cost"),
            Err(Error::Unsupported(_))
        ));

        let zero = DataFrame::new(vec![
            Column::new_text("defect", vec!["a".to_owned(), "b".to_owned()]),
            Column::new_f64("cost", vec![0.0, 0.0]),
        ])
        .expect("frame");
        assert!(matches!(
            fit_weighted(&zero, "defect", "cost"),
            Err(Error::EmptySample)
        ));
    }

    #[test]
    fn the_weighted_report_names_the_weight() {
        let out = analyze_weighted(&weighted_frame(), "defect", "cost").expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("weighted by cost"), "{text}");
        assert!(text.contains("Observations (n)      5"), "{text}");
        assert!(text.contains("Weight"), "{text}");
        assert!(
            text.contains("each row adds the value of cost"),
            "the method note must explain the weight"
        );
        let plot = out.plot.expect("plot");
        assert_eq!(plot.bands.len(), 3);
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
        // The block on the right carries the summary of the report.
        let note = plot.notes.first().expect("summary note");
        let labels: Vec<&str> = note.rows.iter().map(|row| row.label.as_str()).collect();
        assert!(labels.contains(&"Categories"));
        assert!(labels.contains(&"Total"));
        assert!(labels.contains(&"Vital few"));
        assert!(labels.contains(&"Largest share"));
    }
}
