// SPDX-License-Identifier: MIT
//! Additional descriptive graphs: bars, lines, areas, heatmaps, bubbles,
//! individual values, marginals, parallel coordinates and correlograms.
//!
//! Every function returns the same [`AnalysisOutput`] as the other analyses,
//! so the Session text and the graph come from one result. The plot primitives
//! are the engine ones (points, segments, filled bands and coloured series);
//! charts that would need a continuous colour scale or a variable marker
//! radius use value classes instead, which the report and the help say.

use std::collections::BTreeMap;

use crate::data::{DataFrame, MissingPolicy};
use crate::error::{Error, Result};
use crate::plot::{Band, PlotSpec, Point, Segment, linear_axis, panel_plot};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::scatter::pearson_correlation;
use crate::workflow::AnalysisOutput;

/// Largest number of variables in the multi-variable graphs.
pub const MAX_GRAPH_VARIABLES: usize = 8;

/// Chart family of the graph gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GalleryKind {
    /// Bar chart of a categorical column.
    #[default]
    Bar,
    /// Line plot of one or more series.
    Line,
    /// Area chart of one series.
    Area,
    /// Value-class heatmap.
    Heatmap,
    /// Bubble plot with a size class.
    Bubble,
    /// Individual-value plot.
    IndividualValue,
    /// Scatter with a marginal histogram.
    Marginal,
    /// Parallel coordinates.
    ParallelCoordinates,
    /// Scatter matrix with correlations.
    Correlogram,
}

impl GalleryKind {
    /// Name used by the interface and the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Bar => "Bar Chart",
            Self::Line => "Line Plot",
            Self::Area => "Area Chart",
            Self::Heatmap => "Heatmap",
            Self::Bubble => "Bubble Plot",
            Self::IndividualValue => "Individual Value Plot",
            Self::Marginal => "Marginal Plot",
            Self::ParallelCoordinates => "Parallel Coordinates",
            Self::Correlogram => "Correlogram",
        }
    }
}

/// Statistic shown by the bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarStatistic {
    /// Number of rows in the category.
    #[default]
    Count,
    /// Sum of the value column.
    Sum,
    /// Mean of the value column.
    Mean,
}

impl BarStatistic {
    /// Name used in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Count => "Count",
            Self::Sum => "Sum",
            Self::Mean => "Mean",
        }
    }
}

/// Result of a descriptive graph: the report and the plot.
fn output(result: AnalysisResult, plot: PlotSpec) -> AnalysisOutput {
    AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    }
}

/// Rows complete in every requested column, and the excluded count.
fn complete_rows(df: &DataFrame, columns: &[String]) -> Result<(Vec<Vec<f64>>, usize)> {
    let mut sources = Vec::with_capacity(columns.len());
    for name in columns {
        let column = df.column(name)?;
        sources.push(column.numeric_by_row());
    }
    let mut rows = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        let mut values = Vec::with_capacity(columns.len());
        let mut complete = true;
        for source in &sources {
            if !source[row].is_finite() {
                complete = false;
                break;
            }
            values.push(source[row]);
        }
        if complete {
            rows.push(values);
        }
    }
    let excluded = df.n_rows() - rows.len();
    Ok((rows, excluded))
}

/// Axis range with a proportional pad.
fn padded(values: &[f64]) -> (f64, f64) {
    let low = values.iter().copied().fold(f64::INFINITY, f64::min);
    let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let pad = ((high - low).abs() * 0.06).max(1e-9);
    (low - pad, high + pad)
}

fn check_columns(columns: &[String], minimum: usize) -> Result<()> {
    if columns.len() < minimum {
        return Err(Error::Unsupported(format!(
            "the graph needs at least {minimum} variable(s)"
        )));
    }
    if columns.len() > MAX_GRAPH_VARIABLES {
        return Err(Error::Unsupported(format!(
            "the graph accepts at most {MAX_GRAPH_VARIABLES} variables"
        )));
    }
    let mut unique = columns.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != columns.len() {
        return Err(Error::Unsupported(
            "the variables must be distinct columns".to_owned(),
        ));
    }
    Ok(())
}

/// Bar chart of a categorical column with a count, sum or mean.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for an unknown statistic or a missing value
/// column, and the usual column errors.
pub fn bar_chart(
    df: &DataFrame,
    category: &str,
    value: Option<&str>,
    statistic: BarStatistic,
) -> Result<AnalysisOutput> {
    let category_column = df.column(category)?;
    let value_column = value
        .filter(|name| !name.is_empty())
        .map(|name| df.column(name))
        .transpose()?;
    if statistic != BarStatistic::Count && value_column.is_none() {
        return Err(Error::Unsupported(
            "the sum and mean bar charts need a value column".to_owned(),
        ));
    }
    let mut totals: BTreeMap<String, (usize, f64, usize)> = BTreeMap::new();
    for row in 0..df.n_rows() {
        let Some(label) = category_column.label(row) else {
            continue;
        };
        let (count, sum, used) = totals.entry(label).or_insert((0, 0.0, 0));
        *count += 1;
        if let Some(column) = &value_column
            && let Some(value) = column.value(row)
        {
            *sum += value;
            *used += 1;
        }
    }
    if totals.is_empty() {
        return Err(Error::EmptySample);
    }
    let rows: Vec<(String, f64, usize)> = totals
        .into_iter()
        .map(|(label, (count, sum, used))| {
            let statistic = match statistic {
                BarStatistic::Count => count as f64,
                BarStatistic::Sum => sum,
                BarStatistic::Mean if used > 0 => sum / used as f64,
                BarStatistic::Mean => f64::NAN,
            };
            (label, statistic, count)
        })
        .collect();
    let values: Vec<f64> = rows
        .iter()
        .map(|(_, value, _)| *value)
        .filter(|value| value.is_finite())
        .collect();
    if values.is_empty() {
        return Err(Error::EmptySample);
    }
    let (y_min, y_max) = padded(&values);
    let y_low = y_min.min(0.0);
    let bands = rows
        .iter()
        .enumerate()
        .filter(|(_, (_, value, _))| value.is_finite())
        .map(|(index, (_, value, _))| {
            let centre = index as f64 + 1.0;
            Band {
                lower: vec![
                    Point {
                        x: centre - 0.35,
                        y: 0.0,
                    },
                    Point {
                        x: centre + 0.35,
                        y: 0.0,
                    },
                ],
                upper: vec![
                    Point {
                        x: centre - 0.35,
                        y: *value,
                    },
                    Point {
                        x: centre + 0.35,
                        y: *value,
                    },
                ],
            }
        })
        .collect();
    let notes: Vec<(String, String)> = rows
        .iter()
        .take(12)
        .map(|(label, value, _)| (label.clone(), format!("{value:.4}")))
        .collect();
    let plot = PlotSpec {
        title: format!("Bar Chart — {category}"),
        x_axis: linear_axis(category, 0.4, rows.len() as f64 + 0.6),
        y_axis: linear_axis(statistic.name(), y_low, y_max),
        size: (900, 520),
        points: Vec::new(),
        lines: Vec::new(),
        bands,
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: vec![crate::plot::PlotNote::new(notes)],
    };
    let mut result = AnalysisResult::new(
        format!("Bar Chart — {category}"),
        format!(
            "Bars of the {} per category",
            statistic.name().to_lowercase()
        ),
    );
    result.n = rows.iter().map(|(_, _, count)| count).sum();
    result.excluded = df.n_rows() - result.n;
    result.statistics = vec![
        ("Categories", rows.len() as f64),
        ("Observations", result.n as f64),
        (
            "Largest bar",
            values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Category".to_owned(),
            statistic.name().to_owned(),
            "Rows".to_owned(),
        ],
        rows: rows
            .iter()
            .map(|(label, value, count)| {
                vec![
                    label.clone(),
                    if value.is_finite() {
                        format!("{value:.6}")
                    } else {
                        "—".to_owned()
                    },
                    count.to_string(),
                ]
            })
            .collect(),
    });
    result.notes = vec![(
        "Categories".to_owned(),
        "the x axis is the category position; the table and the side note carry the labels and \
         the bar values"
            .to_owned(),
    )];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    Ok(output(result, plot))
}

/// Line plot of one or more numeric columns against an x column.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for an empty series list and the usual column
/// errors.
pub fn line_plot(df: &DataFrame, x: &str, series: &[String]) -> Result<AnalysisOutput> {
    if series.is_empty() {
        return Err(Error::Unsupported("choose at least one series".to_owned()));
    }
    let time = (!x.is_empty()).then_some(x);
    let mut plot_series = Vec::new();
    let mut table_rows = Vec::new();
    let mut all_x = Vec::new();
    let mut all_y = Vec::new();
    for name in series {
        let model = crate::seriesplot::fit(df, name, time)?;
        table_rows.push(vec![
            name.clone(),
            model.n.to_string(),
            format!(
                "{:.6}",
                model.y.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            ),
            format!(
                "{:.6}",
                model.y.iter().copied().fold(f64::INFINITY, f64::min)
            ),
        ]);
        all_x.extend_from_slice(&model.x);
        all_y.extend_from_slice(&model.y);
        let points: Vec<Point> = model
            .x
            .iter()
            .zip(&model.y)
            .map(|(x, y)| Point { x: *x, y: *y })
            .collect();
        let lines = points
            .windows(2)
            .map(|pair| Segment {
                from: pair[0],
                to: pair[1],
                width: 2.0,
            })
            .collect();
        plot_series.push(crate::plot::Series {
            label: name.clone(),
            points,
            lines,
            bands: Vec::new(),
            secondary_axis: false,
        });
    }
    let (x_min, x_max) = padded(&all_x);
    let (y_min, y_max) = padded(&all_y);
    let plot = PlotSpec {
        title: format!("Line Plot — {}", series.join(", ")),
        x_axis: linear_axis(time.unwrap_or("Row"), x_min, x_max),
        y_axis: linear_axis("Value", y_min, y_max),
        size: (900, 520),
        points: Vec::new(),
        lines: Vec::new(),
        bands: Vec::new(),
        series: plot_series,
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    };
    let used = table_rows
        .iter()
        .filter_map(|row| row[1].parse::<usize>().ok())
        .sum();
    let mut result = AnalysisResult::new(
        format!("Line Plot — {}", series.join(", ")),
        format!(
            "One line per series along {}",
            time.unwrap_or("the row order")
        ),
    );
    result.n = used;
    result.excluded = df.n_rows().saturating_sub(used);
    result.statistics = vec![("Series", series.len() as f64), ("Points", used as f64)];
    result.extra.push(Item::Table {
        headers: vec![
            "Series".to_owned(),
            "Points".to_owned(),
            "Maximum".to_owned(),
            "Minimum".to_owned(),
        ],
        rows: table_rows,
    });
    result.notes = vec![(
        "Order".to_owned(),
        "each series is drawn in worksheet order; rows with a missing x or value are not used"
            .to_owned(),
    )];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    Ok(output(result, plot))
}

/// Area chart: a line plot with the area under the curve filled.
///
/// # Errors
///
/// As [`line_plot`], with a single series.
pub fn area_chart(df: &DataFrame, x: &str, series: &str) -> Result<AnalysisOutput> {
    let mut out = line_plot(df, x, &[series.to_owned()])?;
    let model = crate::seriesplot::fit(df, series, (!x.is_empty()).then_some(x))?;
    let points: Vec<Point> = model
        .x
        .iter()
        .zip(&model.y)
        .map(|(x, y)| Point { x: *x, y: *y })
        .collect();
    let band = Band {
        lower: points
            .iter()
            .map(|point| Point { x: point.x, y: 0.0 })
            .collect(),
        upper: points.clone(),
    };
    if let Some(plot) = out.plot.as_mut() {
        plot.title = format!("Area Chart — {series}");
        plot.bands.push(band);
    }
    if let Some(result) = out.results.first_mut() {
        result.title = format!("Area Chart — {series}");
        result.method = "Line plot with the area between the curve and zero filled".to_owned();
        result.notes.push((
            "Area".to_owned(),
            "the filled band runs between the curve and y = 0".to_owned(),
        ));
    }
    Ok(out)
}

/// Six equal-width classes of a value range, as labels.
fn value_classes(low: f64, high: f64) -> Vec<(String, f64, f64)> {
    if high <= low {
        return vec![(format!("{low:.4}"), low, high)];
    }
    let classes = 6usize;
    let width = (high - low) / classes as f64;
    (0..classes)
        .map(|index| {
            let from = low + width * index as f64;
            let to = if index + 1 == classes {
                high
            } else {
                low + width * (index + 1) as f64
            };
            (format!("{from:.3} – {to:.3}"), from, to)
        })
        .collect()
}

fn class_index(value: f64, low: f64, high: f64, classes: usize) -> usize {
    if high <= low {
        return 0;
    }
    let span = (value - low) / (high - low) * classes as f64;
    (span.floor() as usize).min(classes - 1)
}

/// Value-class heatmap of several numeric columns.
///
/// # Errors
///
/// Returns the column errors and a short-sample error.
pub fn heatmap(df: &DataFrame, columns: &[String]) -> Result<AnalysisOutput> {
    check_columns(columns, 1)?;
    let (rows, excluded) = complete_rows(df, columns)?;
    if rows.is_empty() {
        return Err(Error::EmptySample);
    }
    let flat: Vec<f64> = rows.iter().flatten().copied().collect();
    let low = flat.iter().copied().fold(f64::INFINITY, f64::min);
    let high = flat.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let classes = value_classes(low, high);
    let mut series: Vec<crate::plot::Series> = classes
        .iter()
        .map(|(label, _, _)| crate::plot::Series {
            label: label.clone(),
            points: Vec::new(),
            lines: Vec::new(),
            bands: Vec::new(),
            secondary_axis: false,
        })
        .collect();
    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, value) in row.iter().enumerate() {
            let class = class_index(*value, low, high, classes.len());
            series[class].points.push(Point {
                x: column_index as f64 + 1.0,
                y: row_index as f64 + 1.0,
            });
        }
    }
    let plot = PlotSpec {
        title: "Heatmap".to_owned(),
        x_axis: linear_axis("Variable", 0.5, columns.len() as f64 + 0.5),
        y_axis: linear_axis("Row", 0.5, rows.len() as f64 + 0.5),
        size: (900, 620),
        points: Vec::new(),
        lines: Vec::new(),
        bands: Vec::new(),
        series,
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    };
    let mut result = AnalysisResult::new(
        "Heatmap".to_owned(),
        "Cells grouped into six value classes; the class is the colour of the point".to_owned(),
    );
    result.n = rows.len();
    result.excluded = excluded;
    result.statistics = vec![
        ("Variables", columns.len() as f64),
        ("Rows", rows.len() as f64),
        ("Minimum", low),
        ("Maximum", high),
        ("Classes", classes.len() as f64),
    ];
    result.extra.push(Item::Table {
        headers: vec!["Class".to_owned(), "From".to_owned(), "To".to_owned()],
        rows: classes
            .iter()
            .map(|(label, from, to)| vec![label.clone(), format!("{from:.6}"), format!("{to:.6}")])
            .collect(),
    });
    result.notes = vec![
        (
            "Cells".to_owned(),
            format!(
                "rows are worksheet rows and columns are the {} variables; each cell is a point \
                 coloured by its value class",
                columns.len()
            ),
        ),
        (
            "Limitation".to_owned(),
            "the plot uses six classes because the engine fills with a single grey tone; a \
             continuous colour scale would need a new drawing primitive"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    Ok(output(result, plot))
}

/// Bubble plot: a scatter with a third magnitude split into three classes.
///
/// # Errors
///
/// Returns the column errors and a short-sample error.
pub fn bubble_plot(df: &DataFrame, x: &str, y: &str, size: &str) -> Result<AnalysisOutput> {
    let columns = vec![x.to_owned(), y.to_owned(), size.to_owned()];
    let (rows, excluded) = complete_rows(df, &columns)?;
    if rows.len() < 2 {
        return Err(Error::SampleTooSmall {
            n: rows.len(),
            min: 2,
        });
    }
    let sizes: Vec<f64> = rows.iter().map(|row| row[2]).collect();
    let low = sizes.iter().copied().fold(f64::INFINITY, f64::min);
    let high = sizes.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let classes = 3usize;
    let labels: Vec<String> = if high > low {
        let width = (high - low) / classes as f64;
        (0..classes)
            .map(|index| {
                format!(
                    "{:.3} – {:.3}",
                    low + width * index as f64,
                    low + width * (index + 1) as f64
                )
            })
            .collect()
    } else {
        vec![format!("{low:.3}")]
    };
    let mut series: Vec<crate::plot::Series> = labels
        .iter()
        .map(|label| crate::plot::Series {
            label: label.clone(),
            points: Vec::new(),
            lines: Vec::new(),
            bands: Vec::new(),
            secondary_axis: false,
        })
        .collect();
    for row in &rows {
        let class = class_index(row[2], low, high, classes).min(labels.len() - 1);
        series[class].points.push(Point {
            x: row[0],
            y: row[1],
        });
    }
    let xs: Vec<f64> = rows.iter().map(|row| row[0]).collect();
    let ys: Vec<f64> = rows.iter().map(|row| row[1]).collect();
    let (x_min, x_max) = padded(&xs);
    let (y_min, y_max) = padded(&ys);
    let plot = PlotSpec {
        title: format!("Bubble Plot — {x} × {y}, size {size}"),
        x_axis: linear_axis(x, x_min, x_max),
        y_axis: linear_axis(y, y_min, y_max),
        size: (900, 620),
        points: Vec::new(),
        lines: Vec::new(),
        bands: Vec::new(),
        series,
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    };
    let mut result = AnalysisResult::new(
        format!("Bubble Plot — {x} × {y}"),
        format!("Scatter of {x} and {y} with {size} grouped into three classes"),
    );
    result.n = rows.len();
    result.excluded = excluded;
    result.statistics = vec![
        ("Points", rows.len() as f64),
        ("Size minimum", low),
        ("Size maximum", high),
    ];
    result.extra.push(Item::Table {
        headers: vec!["Size class".to_owned(), "From".to_owned(), "To".to_owned()],
        rows: if high > low {
            let width = (high - low) / classes as f64;
            (0..classes)
                .map(|index| {
                    vec![
                        (index + 1).to_string(),
                        format!("{:.6}", low + width * index as f64),
                        format!("{:.6}", low + width * (index + 1) as f64),
                    ]
                })
                .collect()
        } else {
            vec![vec![
                "1".to_owned(),
                format!("{low:.6}"),
                format!("{high:.6}"),
            ]]
        },
    });
    result.notes = vec![(
        "Limitation".to_owned(),
        "the engine draws points with a fixed radius, so the bubble magnitude is carried by the \
         three coloured size classes rather than by the marker size"
            .to_owned(),
    )];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    Ok(output(result, plot))
}

/// Individual-value plot: every observation of each column, with its mean.
///
/// # Errors
///
/// Returns the column errors and a short-sample error.
pub fn individual_value_plot(df: &DataFrame, columns: &[String]) -> Result<AnalysisOutput> {
    check_columns(columns, 1)?;
    let mut points = Vec::new();
    let mut lines = Vec::new();
    let mut rows = Vec::new();
    let mut all = Vec::new();
    for (index, name) in columns.iter().enumerate() {
        let sample = df.column(name)?.sample(MissingPolicy::Drop)?;
        if sample.n() == 0 {
            continue;
        }
        let centre = index as f64 + 1.0;
        for (row, value) in sample.values().iter().enumerate() {
            let offset = ((row % 5) as f64 - 2.0) * 0.03;
            points.push(Point {
                x: centre + offset,
                y: *value,
            });
            all.push(*value);
        }
        let mean = sample.mean()?;
        lines.push(Segment {
            from: Point {
                x: centre - 0.3,
                y: mean,
            },
            to: Point {
                x: centre + 0.3,
                y: mean,
            },
            width: 2.0,
        });
        rows.push(vec![
            name.clone(),
            sample.n().to_string(),
            format!("{mean:.6}"),
            format!("{:.6}", sample.sd().unwrap_or(f64::NAN)),
            format!(
                "{:.6}",
                sample
                    .values()
                    .iter()
                    .copied()
                    .fold(f64::NEG_INFINITY, f64::max)
            ),
            format!(
                "{:.6}",
                sample
                    .values()
                    .iter()
                    .copied()
                    .fold(f64::INFINITY, f64::min)
            ),
        ]);
    }
    if points.is_empty() {
        return Err(Error::EmptySample);
    }
    let (y_min, y_max) = padded(&all);
    let plot = PlotSpec {
        title: "Individual Value Plot".to_owned(),
        x_axis: linear_axis("Variable", 0.4, columns.len() as f64 + 0.6),
        y_axis: linear_axis("Value", y_min, y_max),
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
    let total: usize = rows
        .iter()
        .filter_map(|row| row[1].parse::<usize>().ok())
        .sum();
    let mut result = AnalysisResult::new(
        "Individual Value Plot".to_owned(),
        "Every observation as a point, with the mean of each column".to_owned(),
    );
    result.n = total;
    result.statistics = vec![("Variables", columns.len() as f64)];
    result.extra.push(Item::Table {
        headers: vec![
            "Variable".to_owned(),
            "N".to_owned(),
            "Mean".to_owned(),
            "SD".to_owned(),
            "Maximum".to_owned(),
            "Minimum".to_owned(),
        ],
        rows,
    });
    result.warnings = standard_warnings(result.n, 0, 0);
    Ok(output(result, plot))
}

/// Scatter of `y` against `x` beside the histogram of `x`.
///
/// # Errors
///
/// Returns the column errors and a short-sample error.
pub fn marginal_plot(df: &DataFrame, x: &str, y: &str) -> Result<AnalysisOutput> {
    let columns = vec![x.to_owned(), y.to_owned()];
    let (rows, excluded) = complete_rows(df, &columns)?;
    if rows.len() < 2 {
        return Err(Error::SampleTooSmall {
            n: rows.len(),
            min: 2,
        });
    }
    let xs: Vec<f64> = rows.iter().map(|row| row[0]).collect();
    let ys: Vec<f64> = rows.iter().map(|row| row[1]).collect();
    let r = pearson_correlation(&xs, &ys);
    let (x_min, x_max) = padded(&xs);
    let (y_min, y_max) = padded(&ys);
    let scatter = PlotSpec {
        title: format!("{x} × {y}"),
        x_axis: linear_axis(x, x_min, x_max),
        y_axis: linear_axis(y, y_min, y_max),
        size: (520, 420),
        points: rows
            .iter()
            .map(|row| Point {
                x: row[0],
                y: row[1],
            })
            .collect(),
        lines: Vec::new(),
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    };
    let mut panels = vec![scatter];
    let mut notes = vec![(
        "Marginal".to_owned(),
        match crate::histogram::analyze(df, x, None) {
            Ok(histogram) => {
                if let Some(plot) = histogram.plot {
                    panels.push(plot);
                    format!("the second panel is the histogram of {x}")
                } else {
                    format!("the histogram of {x} could not be drawn")
                }
            }
            Err(error) => format!("the histogram of {x} could not be drawn: {error}"),
        },
    )];
    let plot = panel_plot(panels, 2, format!("Marginal Plot — {x} × {y}"));
    let mut result = AnalysisResult::new(
        format!("Marginal Plot — {x} × {y}"),
        "Scatter of the two variables beside the histogram of the first".to_owned(),
    );
    result.n = rows.len();
    result.excluded = excluded;
    result.statistics = vec![("Pearson r", r), ("Points", rows.len() as f64)];
    result.notes.push((
        "Panels".to_owned(),
        "the scatter and the histogram of the first variable".to_owned(),
    ));
    result.notes.append(&mut notes);
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    Ok(AnalysisOutput {
        results: vec![result],
        plot,
    })
}

/// Parallel-coordinates plot: one line per row across the normalized columns.
///
/// # Errors
///
/// Returns the column errors and a short-sample error.
pub fn parallel_coordinates(df: &DataFrame, columns: &[String]) -> Result<AnalysisOutput> {
    check_columns(columns, 2)?;
    let (rows, excluded) = complete_rows(df, columns)?;
    if rows.len() < 2 {
        return Err(Error::SampleTooSmall {
            n: rows.len(),
            min: 2,
        });
    }
    let k = columns.len();
    let mut lows = vec![f64::INFINITY; k];
    let mut highs = vec![f64::NEG_INFINITY; k];
    for row in &rows {
        for (index, value) in row.iter().enumerate() {
            lows[index] = lows[index].min(*value);
            highs[index] = highs[index].max(*value);
        }
    }
    let mut lines = Vec::new();
    for row in &rows {
        let points: Vec<Point> = row
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let span = highs[index] - lows[index];
                let normalized = if span > 0.0 {
                    (value - lows[index]) / span
                } else {
                    0.5
                };
                Point {
                    x: index as f64 + 1.0,
                    y: normalized,
                }
            })
            .collect();
        for pair in points.windows(2) {
            lines.push(Segment {
                from: pair[0],
                to: pair[1],
                width: 1.0,
            });
        }
    }
    let plot = PlotSpec {
        title: "Parallel Coordinates".to_owned(),
        x_axis: linear_axis("Variable", 0.5, k as f64 + 0.5),
        y_axis: linear_axis("Normalized value", -0.05, 1.05),
        size: (900, 560),
        points: Vec::new(),
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    };
    let mut result = AnalysisResult::new(
        "Parallel Coordinates".to_owned(),
        "One line per row; every axis is normalized to its own minimum and maximum".to_owned(),
    );
    result.n = rows.len();
    result.excluded = excluded;
    result.statistics = vec![("Variables", k as f64), ("Lines", rows.len() as f64)];
    result.extra.push(Item::Table {
        headers: vec![
            "Variable".to_owned(),
            "Minimum".to_owned(),
            "Maximum".to_owned(),
        ],
        rows: columns
            .iter()
            .enumerate()
            .map(|(index, name)| {
                vec![
                    name.clone(),
                    format!("{:.6}", lows[index]),
                    format!("{:.6}", highs[index]),
                ]
            })
            .collect(),
    });
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    Ok(output(result, plot))
}

/// Correlogram: the scatter matrix with the Pearson correlation of each pair.
///
/// # Errors
///
/// As the matrix plot.
pub fn correlogram(df: &DataFrame, columns: &[String]) -> Result<AnalysisOutput> {
    let mut out = crate::matrixplot::analyze(df, columns, 10)?;
    let k = columns.len();
    let mut correlations = Vec::new();
    for i in 0..k {
        for j in 0..k {
            correlations.push(if i == j {
                1.0
            } else {
                let (rows, _) = complete_rows(df, &[columns[i].clone(), columns[j].clone()])?;
                let xs: Vec<f64> = rows.iter().map(|row| row[0]).collect();
                let ys: Vec<f64> = rows.iter().map(|row| row[1]).collect();
                if xs.len() >= 2 {
                    pearson_correlation(&xs, &ys)
                } else {
                    f64::NAN
                }
            });
        }
    }
    if let Some(plot) = out.plot.as_mut() {
        plot.title = "Correlogram".to_owned();
        for (index, panel) in plot.panels.iter_mut().enumerate() {
            if index % (k + 1) == 0 {
                panel.title = columns[index / k].clone();
            } else {
                let row = index / k;
                let column = index % k;
                panel.title = format!(
                    "{} × {} (r = {:.3})",
                    columns[row], columns[column], correlations[index]
                );
            }
        }
    }
    if let Some(result) = out.results.first_mut() {
        result.title = "Correlogram".to_owned();
        result.method =
            "Scatter matrix with the Pearson correlation of every pair in the panel title"
                .to_owned();
        result.notes.push((
            "Correlogram".to_owned(),
            "the diagonal panels are the histograms; every off-diagonal panel is a scatter with \
             its correlation"
                .to_owned(),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        let n = 24;
        let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let y: Vec<f64> = (0..n).map(|i| 2.0 + 0.5 * i as f64).collect();
        let z: Vec<f64> = (0..n).map(|i| ((i * 7) % 5) as f64).collect();
        let category: Vec<String> = (0..n)
            .map(|i| if i % 3 == 0 { "A" } else { "B" }.to_owned())
            .collect();
        DataFrame::new(vec![
            Column::new_f64("x", x),
            Column::new_f64("y", y),
            Column::new_f64("z", z),
            Column::new_text("group", category),
        ])
        .expect("frame")
    }

    fn columns(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn the_bar_chart_counts_sums_and_averages() {
        let df = frame();
        for (statistic, expected) in [
            (BarStatistic::Count, "8"),
            (BarStatistic::Sum, "84.000000"),
            (BarStatistic::Mean, "10.500000"),
        ] {
            let out = bar_chart(&df, "group", Some("x"), statistic).expect("bar chart");
            let text = out.results[0].to_session_text();
            assert!(text.contains("Bar Chart"), "{text}");
            assert!(text.contains(expected), "{statistic:?}: {text}");
            let plot = out.plot.expect("plot");
            assert_eq!(plot.bands.len(), 2);
        }
        assert!(bar_chart(&df, "group", Some("x"), BarStatistic::Mean).is_ok());
        assert!(bar_chart(&df, "group", None, BarStatistic::Sum).is_err());
    }

    #[test]
    fn the_line_and_area_charts_draw_every_series() {
        let df = frame();
        let out = line_plot(&df, "x", &columns(&["y", "z"])).expect("line");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.series.len(), 2);
        assert_eq!(plot.series[0].points.len(), 24);
        let area = area_chart(&df, "x", "y").expect("area");
        let plot = area.plot.expect("plot");
        assert_eq!(plot.bands.len(), 1);
        assert!(plot.title.contains("Area Chart"));
        assert!(line_plot(&df, "x", &[]).is_err());
    }

    #[test]
    fn the_heatmap_uses_the_value_classes() {
        let df = frame();
        let out = heatmap(&df, &columns(&["x", "y", "z"])).expect("heatmap");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Heatmap"), "{text}");
        assert!(text.contains("Class"), "{text}");
        let plot = out.plot.expect("plot");
        assert!(!plot.series.is_empty());
        assert_eq!(
            plot.series
                .iter()
                .map(|series| series.points.len())
                .sum::<usize>(),
            72
        );
        assert!(heatmap(&df, &[]).is_err());
    }

    #[test]
    fn the_bubble_plot_splits_the_size_into_classes() {
        let df = frame();
        let out = bubble_plot(&df, "x", "y", "z").expect("bubble");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.series.len(), 3);
        assert_eq!(
            plot.series
                .iter()
                .map(|series| series.points.len())
                .sum::<usize>(),
            24
        );
        assert!(bubble_plot(&df, "x", "y", "missing").is_err());
    }

    #[test]
    fn the_individual_and_parallel_plots_cover_every_row() {
        let df = frame();
        let out = individual_value_plot(&df, &columns(&["x", "y"])).expect("individual");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.points.len(), 48);
        assert_eq!(plot.lines.len(), 2);
        let out = parallel_coordinates(&df, &columns(&["x", "y", "z"])).expect("parallel");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.lines.len(), 24 * 2);
        assert!(parallel_coordinates(&df, &["x".to_owned()]).is_err());
    }

    #[test]
    fn the_marginal_and_correlogram_are_built() {
        let df = frame();
        let out = marginal_plot(&df, "x", "y").expect("marginal");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.panels.len(), 2);
        let out = correlogram(&df, &columns(&["x", "y", "z"])).expect("correlogram");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Correlogram"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.panels.len(), 9);
        assert!(plot.panels[1].title.contains("r ="));
    }

    #[test]
    fn invalid_graph_requests_are_rejected() {
        let df = frame();
        assert!(heatmap(&df, &["x".to_owned(), "x".to_owned()]).is_err());
        let too_many: Vec<String> = (0..9).map(|i| format!("c{i}")).collect();
        assert!(heatmap(&df, &too_many).is_err());
        assert!(marginal_plot(&df, "x", "missing").is_err());
    }
}
