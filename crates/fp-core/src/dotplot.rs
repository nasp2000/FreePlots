//! Dotplot (individual value plot).
//!
//! Every observation becomes a dot at its value. With a factor, each level gets
//! its own column of dots and a short horizontal line at the level mean; without
//! one, the dots form a single column. Values that coincide (or nearly so) are
//! stacked side by side, so the picture shows the shape of the data, the
//! outliers and the spread of each group without any smoothing.
//!
//! The stacking offset is a small horizontal displacement around the column
//! centre; it carries no meaning beyond separating the dots.
//!
//! The comparison run has a dotplot; its stacking rule was not captured, so
//! the layout is our own — see `docs/parity.md`.

use std::collections::BTreeMap;

use crate::data::{Column, DataFrame, MissingPolicy};
use crate::error::{Error, Result};
use crate::plot::{AxisSpec, PlotSpec, Point, Scale, Segment, Tick, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// Greatest number of dots stacked in one column position.
const MAX_STACK: usize = 25;

/// Result of a dotplot.
#[derive(Debug, Clone, PartialEq)]
pub struct Dotplot {
    /// Column analysed.
    pub column: String,
    /// Factor column, when the plot is grouped.
    pub factor: Option<String>,
    /// Level labels (one entry when there is no factor).
    pub levels: Vec<String>,
    /// Values of each level.
    pub values: Vec<Vec<f64>>,
    /// Mean of each level.
    pub means: Vec<f64>,
    /// Observations used.
    pub n: usize,
    /// Rows excluded because a value or a label was missing.
    pub excluded: usize,
}

/// Reads the numeric value of a row (`None` when the cell is missing).
fn value_at(column: &Column, values: &[f64], row: usize) -> Option<f64> {
    if column.is_missing(row) || !values[row].is_finite() {
        return None;
    }
    Some(values[row])
}

/// Builds the data of the plot.
pub fn fit(df: &DataFrame, column_name: &str, factor: Option<&str>) -> Result<Dotplot> {
    let column = df.column(column_name)?;
    let values = column.numeric(MissingPolicy::Drop)?;
    match factor {
        None => {
            let mut kept = Vec::new();
            for row in 0..df.n_rows() {
                if let Some(value) = value_at(column, &values, row) {
                    kept.push(value);
                }
            }
            if kept.is_empty() {
                return Err(Error::EmptySample);
            }
            let mean = kept.iter().sum::<f64>() / kept.len() as f64;
            Ok(Dotplot {
                column: column_name.to_owned(),
                factor: None,
                levels: vec![column_name.to_owned()],
                values: vec![kept.clone()],
                means: vec![mean],
                n: kept.len(),
                excluded: df.n_rows() - kept.len(),
            })
        }
        Some(factor_name) => {
            if factor_name == column_name {
                return Err(Error::Unsupported(
                    "the factor must be a different column".to_owned(),
                ));
            }
            let factor_column = df.column(factor_name)?;
            let mut groups: BTreeMap<String, Vec<f64>> = BTreeMap::new();
            for row in 0..df.n_rows() {
                if factor_column.is_missing(row) {
                    continue;
                }
                if let Some(value) = value_at(column, &values, row) {
                    groups
                        .entry(factor_column.cell_text(row))
                        .or_default()
                        .push(value);
                }
            }
            if groups.is_empty() {
                return Err(Error::EmptySample);
            }
            let levels: Vec<String> = groups.keys().cloned().collect();
            let mut values_by_level = Vec::with_capacity(levels.len());
            let mut means = Vec::with_capacity(levels.len());
            let mut n = 0usize;
            for level in &levels {
                let group = groups.get(level).cloned().unwrap_or_default();
                let mean = group.iter().sum::<f64>() / group.len() as f64;
                n += group.len();
                means.push(mean);
                values_by_level.push(group);
            }
            Ok(Dotplot {
                column: column_name.to_owned(),
                factor: Some(factor_name.to_owned()),
                levels,
                values: values_by_level,
                means,
                n,
                excluded: df.n_rows() - n,
            })
        }
    }
}

/// Stacking offset of the `index`-th dot of a column with `count` dots.
fn stack_offset(index: usize, count: usize) -> f64 {
    if count <= 1 {
        return 0.0;
    }
    let width = (0.42 / (count.min(MAX_STACK) as f64 - 1.0)).min(0.04);
    (index as f64 - (count as f64 - 1.0) / 2.0) * width
}

/// Builds the plot.
fn plot(model: &Dotplot) -> PlotSpec {
    let all: Vec<f64> = model.values.iter().flatten().copied().collect();
    let low = all.iter().copied().fold(f64::INFINITY, f64::min);
    let high = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let pad = ((high - low).abs() * 0.06).max(1e-9);
    let columns = model.levels.len();

    let mut points = Vec::new();
    let mut lines = Vec::new();
    for (index, group) in model.values.iter().enumerate() {
        let centre = index as f64 + 1.0;
        // Group the values that are close enough to share a stack.
        let mut sorted = group.clone();
        sorted.sort_by(f64::total_cmp);
        let tolerance = ((high - low).abs() * 0.002).max(1e-12);
        let mut stack: Vec<f64> = Vec::new();
        let flush = |stack: &mut Vec<f64>, points: &mut Vec<Point>| {
            let count = stack.len();
            for (position, value) in stack.iter().enumerate() {
                points.push(Point {
                    x: centre + stack_offset(position, count),
                    y: *value,
                });
            }
            stack.clear();
        };
        for value in sorted {
            if let Some(last) = stack.last()
                && (value - last).abs() > tolerance
            {
                flush(&mut stack, &mut points);
            }
            stack.push(value);
        }
        flush(&mut stack, &mut points);
        // The mean as a short horizontal line.
        lines.push(Segment {
            from: Point {
                x: centre - 0.22,
                y: model.means[index],
            },
            to: Point {
                x: centre + 0.22,
                y: model.means[index],
            },
            width: 2.0,
        });
    }

    let step = if columns <= 12 {
        1
    } else {
        columns.div_ceil(12)
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
        label: model.factor.clone().unwrap_or_else(|| model.column.clone()),
        scale: Scale::Linear,
        min: 0.4,
        max: columns as f64 + 0.6,
        ticks: model
            .levels
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
        title: match &model.factor {
            Some(factor) => format!("Dotplot — {} by {factor}", model.column),
            None => format!("Dotplot — {}", model.column),
        },
        x_axis,
        y_axis: linear_axis(&model.column, low - pad, high + pad),
        size: (900, 540),
        points,
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
    }
}

/// Runs the analysis and builds the report and the plot.
pub fn analyze(df: &DataFrame, column_name: &str, factor: Option<&str>) -> Result<AnalysisOutput> {
    let model = fit(df, column_name, factor)?;
    let mut result = AnalysisResult::new(
        match &model.factor {
            Some(factor) => format!("Dotplot — {} by {factor}", model.column),
            None => format!("Dotplot — {}", model.column),
        },
        "Individual values with the mean of each level".to_owned(),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Groups", model.levels.len() as f64),
        ("Observations", model.n as f64),
        (
            "Grand mean",
            model.values.iter().flatten().sum::<f64>() / model.n as f64,
        ),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Level".to_owned(),
            "n".to_owned(),
            "Mean".to_owned(),
            "Minimum".to_owned(),
            "Maximum".to_owned(),
        ],
        rows: model
            .levels
            .iter()
            .enumerate()
            .map(|(index, level)| {
                let group = &model.values[index];
                vec![
                    level.clone(),
                    format!("{}", group.len()),
                    format!("{:.6}", model.means[index]),
                    format!("{:.6}", group.iter().copied().fold(f64::INFINITY, f64::min)),
                    format!(
                        "{:.6}",
                        group.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                    ),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Dots".to_owned(),
            "one dot per observation; coincident values are stacked side by side".to_owned(),
        ),
        (
            "Mean".to_owned(),
            "the short horizontal line marks the mean of each level".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing value or a missing factor label are excluded and counted"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the stacking rule is our own and has not been verified against captured output".to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot(&model)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 1.0, 1.0, 2.0, 5.0, 1.0, 2.0, 2.0]),
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
                ],
            ),
        ])
        .expect("frame")
    }

    #[test]
    fn coincident_values_are_stacked() {
        let model = fit(&frame(), "y", None).expect("fit");
        assert_eq!(model.n, 8);
        assert_eq!(model.levels.len(), 1);
        let plot = plot(&model);
        assert_eq!(plot.points.len(), 8);
        // The three 1.0 dots share a column but sit at different x.
        let ones: Vec<f64> = plot
            .points
            .iter()
            .filter(|point| (point.y - 1.0).abs() < 1e-12)
            .map(|point| point.x)
            .collect();
        assert_eq!(ones.len(), 4);
        assert!(ones.windows(2).all(|pair| pair[0] < pair[1]));
        // One mean line.
        assert_eq!(plot.lines.len(), 1);
        assert!((plot.lines[0].from.y - model.means[0]).abs() < 1e-12);
    }

    #[test]
    fn grouping_gives_one_column_per_level() {
        let model = fit(&frame(), "y", Some("g")).expect("fit");
        assert_eq!(model.levels, vec!["A", "B"]);
        assert_eq!(model.n, 8);
        let plot = plot(&model);
        assert_eq!(plot.lines.len(), 2);
        assert_eq!(plot.x_axis.label, "g");
        let labels: Vec<&str> = plot.x_axis.ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, vec!["A", "B"]);
        // The mean of B is the mean of 1, 2 and 2.
        assert!((model.means[1] - 5.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn invalid_requests_are_rejected() {
        let df = frame();
        assert!(matches!(
            fit(&df, "y", Some("y")),
            Err(Error::Unsupported(_))
        ));
        let empty = DataFrame::new(vec![Column::new_f64("y", vec![f64::NAN])]).expect("frame");
        assert!(matches!(fit(&empty, "y", None), Err(Error::EmptySample)));
    }

    #[test]
    fn the_report_is_built() {
        let out = analyze(&frame(), "y", Some("g")).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Dotplot"), "{text}");
        assert!(text.contains("Maximum"), "{text}");
        assert!(out.plot.is_some());
    }
}
