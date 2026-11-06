//! Boxplots with optional grouping.
//!
//! The box is the interpolated five-number summary; whiskers extend to the most extreme
//! observations inside `Q1 − 1.5·IQR` and `Q3 + 1.5·IQR`; observations outside
//! those fences are drawn as outliers. The plot uses only the existing
//! line/point primitives.

use std::collections::BTreeMap;

use crate::data::{Column, ColumnData, DataFrame, MissingPolicy, Sample};
use crate::error::{Error, Result};
use crate::plot::{AxisSpec, PlotSpec, Point, Scale, Segment, Tick, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// Statistics of one box.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxGroup {
    /// Group label (the column name when there is no By column).
    pub label: String,
    /// Observations in the box.
    pub n: usize,
    /// Minimum.
    pub min: f64,
    /// First quartile (interpolated).
    pub q1: f64,
    /// Median.
    pub median: f64,
    /// Third quartile (interpolated).
    pub q3: f64,
    /// Maximum.
    pub max: f64,
    /// Lower whisker inside the fence.
    pub lower_whisker: f64,
    /// Upper whisker inside the fence.
    pub upper_whisker: f64,
    /// Observations outside the fences.
    pub outliers: Vec<f64>,
}

impl BoxGroup {
    /// Interquartile range.
    #[must_use]
    pub fn iqr(&self) -> f64 {
        self.q3 - self.q1
    }
}

/// Ready-to-report boxplot.
#[derive(Debug, Clone, PartialEq)]
pub struct Boxplot {
    /// Boxes in lexicographic group order.
    pub groups: Vec<BoxGroup>,
    /// Rows excluded by missing/non-finite values.
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

fn label(column: &Column, row: usize) -> Option<String> {
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

/// Builds the boxplot.
pub fn fit(df: &DataFrame, column_name: &str, by: Option<&str>) -> Result<Boxplot> {
    let column = df.column(column_name)?;
    let by_column = match by {
        Some(name) => Some(df.column(name)?),
        None => None,
    };
    let mut groups: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for row in 0..df.n_rows() {
        let Some(value) = cell(column, row) else {
            continue;
        };
        let key = match by_column {
            Some(by) => match label(by, row) {
                Some(label) => label,
                None => continue,
            },
            None => column_name.to_owned(),
        };
        groups.entry(key).or_default().push(value);
    }
    if groups.is_empty() {
        return Err(Error::EmptySample);
    }
    let used: usize = groups.values().map(Vec::len).sum();
    let mut boxes = Vec::with_capacity(groups.len());
    for (label, values) in groups {
        let sample = Sample::new(values, MissingPolicy::Drop)?;
        let n = sample.n();
        if n == 0 {
            continue;
        }
        let min = sample.min()?;
        let max = sample.max()?;
        let q1 = sample.quantile(0.25)?;
        let median = sample.median()?;
        let q3 = sample.quantile(0.75)?;
        let iqr = q3 - q1;
        let lower_fence = q1 - 1.5 * iqr;
        let upper_fence = q3 + 1.5 * iqr;
        let sorted = sample.sorted();
        let lower_whisker = sorted
            .iter()
            .copied()
            .find(|value| *value >= lower_fence)
            .unwrap_or(min);
        let upper_whisker = sorted
            .iter()
            .rev()
            .copied()
            .find(|value| *value <= upper_fence)
            .unwrap_or(max);
        let outliers = sorted
            .iter()
            .copied()
            .filter(|value| *value < lower_fence || *value > upper_fence)
            .collect();
        boxes.push(BoxGroup {
            label,
            n,
            min,
            q1,
            median,
            q3,
            max,
            lower_whisker,
            upper_whisker,
            outliers,
        });
    }
    if boxes.is_empty() {
        return Err(Error::EmptySample);
    }
    Ok(Boxplot {
        groups: boxes,
        excluded: df.n_rows() - used,
    })
}

/// Builds the report and the plot.
pub fn analyze(df: &DataFrame, column_name: &str, by: Option<&str>) -> Result<AnalysisOutput> {
    let model = fit(df, column_name, by)?;
    let plot = plot(column_name, by, &model);
    let result = report(column_name, by, &model);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn report(column_name: &str, by: Option<&str>, model: &Boxplot) -> AnalysisResult {
    let title = match by {
        Some(by) => format!("Boxplot — {column_name} by {by}"),
        None => format!("Boxplot — {column_name}"),
    };
    let mut result = AnalysisResult::new(title, "Five-number summary; fences at 1.5×IQR");
    result.n = model.groups.iter().map(|group| group.n).sum();
    result.excluded = model.excluded;
    result.extra.push(Item::Table {
        headers: vec![
            "Group".to_owned(),
            "n".to_owned(),
            "Min".to_owned(),
            "Q1".to_owned(),
            "Median".to_owned(),
            "Q3".to_owned(),
            "Max".to_owned(),
            "Outliers".to_owned(),
        ],
        rows: model
            .groups
            .iter()
            .map(|group| {
                vec![
                    group.label.clone(),
                    group.n.to_string(),
                    format!("{:.6}", group.min),
                    format!("{:.6}", group.q1),
                    format!("{:.6}", group.median),
                    format!("{:.6}", group.q3),
                    format!("{:.6}", group.max),
                    group.outliers.len().to_string(),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Quartiles".to_owned(),
            "interpolated order statistics with h = (n + 1)p".to_owned(),
        ),
        (
            "Whiskers".to_owned(),
            "most extreme observations inside Q1 − 1.5·IQR and Q3 + 1.5·IQR".to_owned(),
        ),
        (
            "Outliers".to_owned(),
            "observations outside the fences; drawn as symbols".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, model.excluded, 0);
    result.warnings.push(
        "boxplot quartile and fence conventions have not been verified against documented output"
            .to_owned(),
    );
    result
}

fn plot(column_name: &str, by: Option<&str>, model: &Boxplot) -> PlotSpec {
    let groups = model.groups.len();
    let x_min = 0.5;
    let x_max = groups as f64 + 0.5;
    let y_min = model
        .groups
        .iter()
        .map(|group| {
            group
                .outliers
                .first()
                .copied()
                .map_or(group.min, |value| value.min(group.min))
        })
        .fold(f64::INFINITY, f64::min);
    let y_max = model
        .groups
        .iter()
        .map(|group| {
            group
                .outliers
                .last()
                .copied()
                .map_or(group.max, |value| value.max(group.max))
        })
        .fold(f64::NEG_INFINITY, f64::max);
    let pad = ((y_max - y_min).abs() * 0.05).max(1e-9);
    let mut lines = Vec::new();
    let mut points = Vec::new();
    for (index, group) in model.groups.iter().enumerate() {
        let x = (index + 1) as f64;
        let half = 0.32;
        // Box.
        lines.push(Segment {
            from: Point {
                x: x - half,
                y: group.q1,
            },
            to: Point {
                x: x - half,
                y: group.q3,
            },
            width: 1.5,
        });
        lines.push(Segment {
            from: Point {
                x: x + half,
                y: group.q1,
            },
            to: Point {
                x: x + half,
                y: group.q3,
            },
            width: 1.5,
        });
        lines.push(Segment {
            from: Point {
                x: x - half,
                y: group.q1,
            },
            to: Point {
                x: x + half,
                y: group.q1,
            },
            width: 1.5,
        });
        lines.push(Segment {
            from: Point {
                x: x - half,
                y: group.q3,
            },
            to: Point {
                x: x + half,
                y: group.q3,
            },
            width: 1.5,
        });
        lines.push(Segment {
            from: Point {
                x: x - half,
                y: group.median,
            },
            to: Point {
                x: x + half,
                y: group.median,
            },
            width: 2.0,
        });
        // Whiskers and caps.
        lines.push(Segment {
            from: Point { x, y: group.q1 },
            to: Point {
                x,
                y: group.lower_whisker,
            },
            width: 1.2,
        });
        lines.push(Segment {
            from: Point { x, y: group.q3 },
            to: Point {
                x,
                y: group.upper_whisker,
            },
            width: 1.2,
        });
        lines.push(Segment {
            from: Point {
                x: x - 0.16,
                y: group.lower_whisker,
            },
            to: Point {
                x: x + 0.16,
                y: group.lower_whisker,
            },
            width: 1.2,
        });
        lines.push(Segment {
            from: Point {
                x: x - 0.16,
                y: group.upper_whisker,
            },
            to: Point {
                x: x + 0.16,
                y: group.upper_whisker,
            },
            width: 1.2,
        });
        for outlier in &group.outliers {
            points.push(Point { x, y: *outlier });
        }
    }
    let ticks = model
        .groups
        .iter()
        .enumerate()
        .map(|(index, group)| Tick {
            position: (index + 1) as f64,
            label: group.label.clone(),
        })
        .collect();
    PlotSpec {
        title: match by {
            Some(by) => format!("Boxplot — {column_name} by {by}"),
            None => format!("Boxplot — {column_name}"),
        },
        x_axis: AxisSpec {
            label: by.unwrap_or("Group").to_owned(),
            scale: Scale::Linear,
            min: x_min,
            max: x_max,
            ticks,
            minor: Vec::new(),
        },
        y_axis: linear_axis(column_name, y_min - pad, y_max + pad),
        size: (900, 520),
        points,
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: Vec::new(),
    }
}
