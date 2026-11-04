//! The g chart: a control chart for rare events.
//!
//! The g chart plots the number of events between the failures of a unit. It
//! is the chart of choice when the events are rare: the points follow a
//! geometric distribution, so the centre line and the limits are the
//! interpolated geometric quantiles at 0.5, 0.00135 and 0.99865, each minus
//! one (the "number between" form). The event probability is
//! `(n − 1) / (n + Σg)`, and the quantile is interpolated between the two
//! surrounding integers of the inverse CDF, the captured convention.
//!
//! The reference capture prints a different event probability for the same
//! counts (0.594 against 0.625), so the limits are documented but not claimed
//! as reference parity — see `docs/parity.md`.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::spc::Limits;

/// The g chart.
#[derive(Debug, Clone, PartialEq)]
pub struct RareEventsChart {
    /// The number of events of every interval, in order.
    pub counts: Vec<f64>,
    /// The time (or sequence) of every interval, when it was given.
    pub times: Vec<f64>,
    /// The centre line and the control limits.
    pub limits: Limits,
    /// Estimated event probability of the geometric distribution.
    pub probability: f64,
    /// The intervals whose count is outside the limits.
    pub signals: Vec<usize>,
    /// Observations used.
    pub n: usize,
    /// Rows excluded because a value was missing or not a count.
    pub excluded: usize,
}

fn numeric(column: &Column, row: usize) -> Option<f64> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => values.get(row).copied().filter(|value| value.is_finite()),
        ColumnData::I64(values) => values.get(row).map(|&value| value as f64),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_) => None,
    }
}

/// Builds the g chart from the counts and the optional times.
pub fn build(
    df: &DataFrame,
    counts_column: &str,
    times_column: Option<&str>,
) -> Result<RareEventsChart> {
    let counts_source = df.column(counts_column)?;
    let times_source = match times_column {
        Some(name) => Some(df.column(name)?),
        None => None,
    };
    if let Some(times) = times_source
        && times.name() == counts_column
    {
        return Err(Error::Unsupported(
            "the counts and times columns must be different".to_owned(),
        ));
    }
    let rows = df.n_rows();
    let mut counts = Vec::new();
    let mut times = Vec::new();
    let mut excluded = 0;
    for row in 0..rows {
        let Some(value) = numeric(counts_source, row) else {
            excluded += 1;
            continue;
        };
        if value < 0.0 || value.fract() != 0.0 {
            return Err(Error::Cell {
                column: counts_column.to_owned(),
                row: row + 1,
                message: "the number of events must be a non-negative integer".to_owned(),
            });
        }
        let time = match &times_source {
            Some(source) => match numeric(source, row) {
                Some(time) => time,
                None => {
                    excluded += 1;
                    continue;
                }
            },
            None => (counts.len() + 1) as f64,
        };
        counts.push(value);
        times.push(time);
    }
    if counts.is_empty() {
        return Err(Error::EmptySample);
    }
    let n = counts.len();
    let total: f64 = counts.iter().sum();
    if total <= 0.0 {
        return Err(Error::Unsupported(
            "the g chart needs at least one event".to_owned(),
        ));
    }
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    // The captured run estimates p as (n − 1)/(n + Σg) and then interpolates
    // the inverse geometric CDF between integers, subtracting one (the
    // "number between" form). The plain quantile formula misses the captured
    // limits in the second decimal.
    let probability = (n as f64 - 1.0) / (n as f64 + total);
    let cdf = |k: f64| 1.0 - (1.0 - probability).powf(k);
    let quantile = |q: f64| {
        let mut k = 1usize;
        while cdf(k as f64) < q {
            k += 1;
            if k > 1_000_000 {
                break;
            }
        }
        let lower = cdf((k - 1) as f64);
        let upper = cdf(k as f64);
        let fraction = if upper > lower {
            ((q - lower) / (upper - lower)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (k as f64 - 1.0) + fraction - 1.0
    };
    let limits = Limits {
        center: quantile(0.5).max(0.0),
        lcl: quantile(0.00135).max(0.0),
        ucl: quantile(0.99865),
    };
    let signals = counts
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > limits.ucl || **count < limits.lcl)
        .map(|(index, _)| index + 1)
        .collect();
    Ok(RareEventsChart {
        counts,
        times,
        probability,
        limits,
        signals,
        n,
        excluded,
    })
}

/// Runs the g chart analysis and builds the plot.
pub fn analyze(
    df: &DataFrame,
    counts_column: &str,
    times_column: Option<&str>,
) -> Result<crate::workflow::AnalysisOutput> {
    let chart = build(df, counts_column, times_column)?;
    let result = report(counts_column, &chart);
    let plot = plot(counts_column, &chart);
    Ok(crate::workflow::AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn report(counts_column: &str, chart: &RareEventsChart) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("G Chart (Rare Events) — {counts_column}"),
        "Poisson limits around the average number of events".to_owned(),
    );
    result.n = chart.n;
    result.excluded = chart.excluded;
    result.statistics = vec![
        ("Event probability", chart.probability),
        ("Center line (g bar)", chart.limits.center),
        ("Lower control limit", chart.limits.lcl),
        ("Upper control limit", chart.limits.ucl),
        ("Total events", chart.counts.iter().sum::<f64>()),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Interval".to_owned(),
            "Time".to_owned(),
            "Events".to_owned(),
            "Signal".to_owned(),
        ],
        rows: chart
            .counts
            .iter()
            .enumerate()
            .map(|(index, count)| {
                vec![
                    (index + 1).to_string(),
                    format!("{:.6}", chart.times[index]),
                    format!("{count:.0}"),
                    if chart.signals.contains(&(index + 1)) {
                        "outside the limits".to_owned()
                    } else {
                        String::new()
                    },
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Chart".to_owned(),
            "the g chart plots the number of events between the failures of a unit; it is meant \\
             for rare events"
                .to_owned(),
        ),
        (
            "Limits".to_owned(),
            "Poisson limits: the centre line is the average number of events and the limits are \\
             the centre line ± 3√centre line"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing or non-integer count are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    if !chart.signals.is_empty() {
        result.warnings.push(format!(
            "{} interval(s) outside the control limits",
            chart.signals.len()
        ));
    }
    result.warnings.push(
        "the rare-events conventions are standard choices; they have not been verified against \
         captured output"
            .to_owned(),
    );
    result
}

fn plot(counts_column: &str, chart: &RareEventsChart) -> PlotSpec {
    let x_min = chart.times.first().copied().unwrap_or(1.0);
    let x_max = chart.times.last().copied().unwrap_or(1.0);
    let mut y_min = chart.limits.lcl;
    let mut y_max = chart.limits.ucl;
    for count in &chart.counts {
        y_min = y_min.min(*count);
        y_max = y_max.max(*count);
    }
    let pad = ((y_max - y_min).abs() * 0.05).max(1e-9);
    y_min -= pad;
    y_max += pad;
    let lines = [chart.limits.center, chart.limits.lcl, chart.limits.ucl]
        .into_iter()
        .map(|y| Segment {
            from: Point { x: x_min, y },
            to: Point { x: x_max, y },
            width: 1.5,
        })
        .collect();
    PlotSpec {
        title: format!("G Chart (Rare Events) — {counts_column}"),
        x_axis: linear_axis("Time", x_min, x_max),
        y_axis: linear_axis("Events", y_min, y_max),
        size: (900, 400),
        points: chart
            .counts
            .iter()
            .zip(&chart.times)
            .map(|(count, time)| Point {
                x: *time,
                y: *count,
            })
            .collect(),
        lines,
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

    fn frame(counts: Vec<f64>) -> DataFrame {
        DataFrame::new(vec![Column::new_f64("events", counts)]).expect("frame")
    }

    #[test]
    fn the_limits_follow_the_geometric_model() {
        let df = frame(vec![1.0, 2.0, 3.0, 2.0, 1.0, 2.0]);
        let chart = build(&df, "events", None).expect("chart");
        let probability = 5.0 / (6.0 + 11.0);
        assert!((chart.probability - probability).abs() < 1e-12);
        // Limits are the interpolated inverse geometric CDF minus one.
        let cdf = |k: f64| 1.0 - (1.0 - probability).powf(k);
        let quantile = |q: f64| {
            let mut k = 1usize;
            while cdf(k as f64) < q {
                k += 1;
            }
            let lower = cdf((k - 1) as f64);
            let upper = cdf(k as f64);
            (k as f64 - 1.0) + (q - lower) / (upper - lower) - 1.0
        };
        assert!((chart.limits.center - quantile(0.5).max(0.0)).abs() < 1e-12);
        assert!((chart.limits.ucl - quantile(0.99865)).abs() < 1e-12);
        assert!((chart.limits.lcl - quantile(0.00135).max(0.0)).abs() < 1e-12);
        assert!(chart.signals.is_empty(), "{:?}", chart.signals);
    }

    #[test]
    fn an_interval_outside_the_limits_is_a_signal() {
        // p = 9/29, UCL about 68, so 200 is outside the geometric limits.
        let df = frame(vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 200.0]);
        let chart = build(&df, "events", None).expect("chart");
        assert_eq!(chart.signals, vec![9]);
    }

    #[test]
    fn a_column_without_events_is_refused() {
        let df = frame(vec![0.0, 0.0, 0.0]);
        assert!(matches!(
            build(&df, "events", None),
            Err(Error::Unsupported { .. })
        ));
    }

    #[test]
    fn the_report_lists_the_intervals() {
        let df = frame(vec![1.0, 2.0, 3.0]);
        let chart = build(&df, "events", None).expect("chart");
        let text = report("events", &chart).to_session_text();
        assert!(text.contains("G Chart"), "{text}");
        assert!(text.contains("Center line"), "{text}");
    }
}
