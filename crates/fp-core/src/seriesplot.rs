//! Time-series plot: a variable in observation order.
//!
//! The plot joins the observations with a line in **row order** (or against a
//! numeric time column), so trends, level changes and outliers stand out before
//! any model is fitted. The report adds the Mann–Kendall trend test with the
//! Theil–Sen slope, which quantifies what the eye sees.
//!
//! The documented behaviour has a time-series plot; its axis and marker defaults
//! were not documented, so the layout is our own — see `the project notes`.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::runchart;
use crate::workflow::AnalysisOutput;

/// Result of the plot.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesPlot {
    /// Column analysed.
    pub column: String,
    /// Optional time column used for the x axis.
    pub time: Option<String>,
    /// x coordinates (row number or time value).
    pub x: Vec<f64>,
    /// y coordinates.
    pub y: Vec<f64>,
    /// Observations used.
    pub n: usize,
    /// Rows excluded.
    pub excluded: usize,
}

/// Builds the data of the plot.
pub fn fit(df: &DataFrame, column_name: &str, time: Option<&str>) -> Result<SeriesPlot> {
    let column = df.column(column_name)?;
    let values = column.numeric_by_row();
    let time_values = match time {
        Some(name) => {
            if name == column_name {
                return Err(Error::Unsupported(
                    "the time column must be different from the series".to_owned(),
                ));
            }
            Some(df.column(name)?.numeric_by_row())
        }
        None => None,
    };
    let mut x = Vec::with_capacity(df.n_rows());
    let mut y = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        if column.is_missing(row) || !values[row].is_finite() {
            continue;
        }
        let position = match &time_values {
            Some(series) => {
                if !series[row].is_finite() {
                    continue;
                }
                series[row]
            }
            None => (x.len() + 1) as f64,
        };
        x.push(position);
        y.push(values[row]);
    }
    if y.len() < 2 {
        return Err(Error::SampleTooSmall { n: y.len(), min: 2 });
    }
    Ok(SeriesPlot {
        column: column_name.to_owned(),
        time: time.map(str::to_owned),
        n: y.len(),
        excluded: df.n_rows() - y.len(),
        x,
        y,
    })
}

/// Builds the plot.
fn plot(model: &SeriesPlot) -> PlotSpec {
    let x_min = model.x.iter().copied().fold(f64::INFINITY, f64::min);
    let x_max = model.x.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let y_min = model.y.iter().copied().fold(f64::INFINITY, f64::min);
    let y_max = model.y.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let x_pad = ((x_max - x_min).abs() * 0.02).max(1e-9);
    let y_pad = ((y_max - y_min).abs() * 0.06).max(1e-9);
    let points: Vec<Point> = model
        .x
        .iter()
        .zip(&model.y)
        .map(|(x, y)| Point { x: *x, y: *y })
        .collect();
    let mut lines = Vec::with_capacity(points.len().saturating_sub(1));
    for pair in points.windows(2) {
        lines.push(Segment {
            from: pair[0],
            to: pair[1],
            width: 2.0,
        });
    }
    PlotSpec {
        title: format!("Time Series Plot — {}", model.column),
        x_axis: linear_axis(
            model.time.as_deref().unwrap_or("Observation"),
            x_min - x_pad,
            x_max + x_pad,
        ),
        y_axis: linear_axis(&model.column, y_min - y_pad, y_max + y_pad),
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

/// Runs the analysis and builds the report and the plot.
pub fn analyze(df: &DataFrame, column_name: &str, time: Option<&str>) -> Result<AnalysisOutput> {
    let model = fit(df, column_name, time)?;
    let trend = runchart::mann_kendall(&model.y).ok();
    let mut result = AnalysisResult::new(
        format!("Time Series Plot — column {column_name}"),
        match &model.time {
            Some(time) => format!("Observations in the order of {time}"),
            None => "Observations in row order".to_owned(),
        },
    );
    result.n = model.n;
    result.excluded = model.excluded;
    let mean = model.y.iter().sum::<f64>() / model.n as f64;
    result.statistics = vec![
        ("Observations", model.n as f64),
        ("Mean", mean),
        (
            "Minimum",
            model.y.iter().copied().fold(f64::INFINITY, f64::min),
        ),
        (
            "Maximum",
            model.y.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ),
        ("First", model.y[0]),
        ("Last", model.y[model.n - 1]),
    ];
    if let Some(trend) = &trend {
        result.statistics.push(("Mann-Kendall S", trend.statistic));
        result.statistics.push(("Mann-Kendall p", trend.p_value));
        result.statistics.push(("Theil-Sen slope", trend.slope));
        result.extra.push(Item::Message(format!(
            "Mann–Kendall: S = {:.0}, p = {:.4}, Theil–Sen slope = {:.6} per observation.",
            trend.statistic, trend.p_value, trend.slope
        )));
    }
    result.notes = vec![
        (
            "Plot".to_owned(),
            "the observations joined in row order (or against the time column)".to_owned(),
        ),
        (
            "Trend".to_owned(),
            "the Mann–Kendall test uses the signs of all pairs, so it does not assume a shape; \
             the Theil–Sen slope is the median of the pairwise slopes"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing value (or a missing time) are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the plotting defaults are our own and have not been verified against documented \
         output"
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
        DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 2.5, 4.0, 5.0]),
            Column::new_f64("t", vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0]),
        ])
        .expect("frame")
    }

    #[test]
    fn row_order_is_the_default_axis() {
        let model = fit(&frame(), "y", None).expect("fit");
        assert_eq!(model.n, 6);
        assert_eq!(model.x, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let plot = plot(&model);
        assert_eq!(plot.points.len(), 6);
        assert_eq!(plot.lines.len(), 5);
        assert_eq!(plot.x_axis.label, "Observation");
    }

    #[test]
    fn a_time_column_sets_the_axis() {
        let model = fit(&frame(), "y", Some("t")).expect("fit");
        assert_eq!(model.x[0], 10.0);
        assert_eq!(model.x[5], 60.0);
        assert_eq!(plot(&model).x_axis.label, "t");
        // The time column cannot be the series itself.
        assert!(matches!(
            fit(&frame(), "y", Some("y")),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn the_report_carries_the_trend() {
        let out = analyze(&frame(), "y", None).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Time Series Plot"), "{text}");
        assert!(text.contains("Mann"), "{text}");
        assert!(text.contains("Theil"), "{text}");
        // The series rises, so the slope is positive.
        let slope = out.results[0]
            .statistics
            .iter()
            .find(|(name, _)| *name == "Theil-Sen slope")
            .map(|(_, value)| *value)
            .expect("slope");
        assert!(slope > 0.0, "{slope}");
    }

    #[test]
    fn a_short_series_is_rejected() {
        let df = DataFrame::new(vec![Column::new_f64("y", vec![1.0])]).expect("frame");
        assert!(matches!(
            fit(&df, "y", None),
            Err(Error::SampleTooSmall { .. })
        ));
    }

    #[test]
    fn a_missing_row_does_not_shift_the_values() {
        let with_missing = DataFrame::new(vec![
            Column::new_f64("y", vec![f64::NAN, 1.0, 2.0, 3.0]),
            Column::new_f64("t", vec![9.0, 1.0, 2.0, 3.0]),
        ])
        .expect("frame");
        let without = DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0]),
            Column::new_f64("t", vec![1.0, 2.0, 3.0]),
        ])
        .expect("frame");
        let a = fit(&with_missing, "y", Some("t")).expect("series");
        let b = fit(&without, "y", Some("t")).expect("series");
        assert_eq!(a.n, 3);
        assert_eq!(a.y, b.y);
        assert_eq!(a.x, b.x);
        assert_eq!(a.x, vec![1.0, 2.0, 3.0]);
    }
}
