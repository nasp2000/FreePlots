// SPDX-License-Identifier: MIT
//! Histogram with an optional Normal density overlay.
//!
//! Bins are drawn as outlines with the existing line primitives, so no new
//! backend primitive is required. Automatic bin selection uses the
//! Freedman–Diaconis rule with a Sturges fallback.

use crate::data::{DataFrame, MissingPolicy};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::normal_pdf;
use crate::workflow::AnalysisOutput;

/// Fitted histogram.
#[derive(Debug, Clone, PartialEq)]
pub struct Histogram {
    /// Bin edges, length = bins + 1.
    pub edges: Vec<f64>,
    /// Counts per bin.
    pub counts: Vec<usize>,
    /// Observations used.
    pub n: usize,
    /// Excluded rows.
    pub excluded: usize,
    /// Mean.
    pub mean: f64,
    /// Sample standard deviation.
    pub sd: f64,
    /// Bin width.
    pub width: f64,
}

/// Computes a histogram; `bins = None` selects the width automatically.
pub fn fit(df: &DataFrame, column_name: &str, bins: Option<usize>) -> Result<Histogram> {
    let sample = df.column(column_name)?.sample(MissingPolicy::Drop)?;
    let n = sample.n();
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    let min = sample.min()?;
    let max = sample.max()?;
    let range = max - min;
    if !range.is_finite() || range <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let auto_bins = if let Some(bins) = bins {
        bins
    } else {
        let iqr = sample.quantile(0.75)? - sample.quantile(0.25)?;
        let fd_width = 2.0 * iqr / (n as f64).powf(1.0 / 3.0);
        let candidate = if fd_width > 0.0 {
            (range / fd_width).ceil() as usize
        } else {
            (n as f64).log2().ceil() as usize + 1
        };
        candidate.clamp(5, 50)
    };
    if auto_bins == 0 || auto_bins > 1000 {
        return Err(Error::InvalidParameter {
            name: "bin count",
            value: auto_bins as f64,
        });
    }
    let width = range / auto_bins as f64;
    let edges: Vec<f64> = (0..=auto_bins)
        .map(|index| min + width * index as f64)
        .collect();
    let mut counts = vec![0usize; auto_bins];
    for &value in sample.values() {
        let raw = (value - min) / width;
        let index = if raw >= auto_bins as f64 {
            auto_bins - 1
        } else {
            raw.floor().max(0.0) as usize
        };
        if let Some(count) = counts.get_mut(index) {
            *count += 1;
        }
    }
    let mean = sample.mean()?;
    let sd = sample.sd()?;
    Ok(Histogram {
        edges,
        counts,
        n,
        excluded: sample.excluded(),
        mean,
        sd,
        width,
    })
}

/// Builds the report and plot.
pub fn analyze(df: &DataFrame, column_name: &str, bins: Option<usize>) -> Result<AnalysisOutput> {
    let model = fit(df, column_name, bins)?;
    let plot = plot(column_name, &model);
    Ok(AnalysisOutput {
        results: vec![report(column_name, &model)],
        plot: Some(plot),
    })
}

fn report(column_name: &str, model: &Histogram) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("Histogram — column {column_name}"),
        "Counts per bin; auto width uses Freedman–Diaconis with Sturges fallback",
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Bin width", model.width),
        ("Bins", model.counts.len() as f64),
        ("Mean", model.mean),
        ("Standard deviation", model.sd),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Bin".to_owned(),
            "Lower".to_owned(),
            "Upper".to_owned(),
            "Count".to_owned(),
            "Percent".to_owned(),
        ],
        rows: model
            .counts
            .iter()
            .enumerate()
            .map(|(index, count)| {
                vec![
                    (index + 1).to_string(),
                    format!("{:.6}", model.edges[index]),
                    format!("{:.6}", model.edges[index + 1]),
                    count.to_string(),
                    format!("{:.2}", 100.0 * *count as f64 / model.n as f64),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Bins".to_owned(),
            "right-closed except the last bin; automatic width from the \
             Freedman–Diaconis rule, falling back to Sturges"
                .to_owned(),
        ),
        (
            "Overlay".to_owned(),
            "Normal density scaled by n·bin width; descriptive only".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    result.warnings.push(
        "histogram bin selection and normal-overlay conventions have not been verified against documented output"
            .to_owned(),
    );
    result
}

fn plot(column_name: &str, model: &Histogram) -> PlotSpec {
    plot_with_options(column_name, model, &[], true)
}

fn plot_with_options(
    column_name: &str,
    model: &Histogram,
    spec_limits: &[f64],
    normal_overlay: bool,
) -> PlotSpec {
    let max_count = model.counts.iter().copied().max().unwrap_or(1) as f64;
    let y_max = (max_count * 1.08).max(1.0);
    let mut lines = Vec::new();
    for (index, &count) in model.counts.iter().enumerate() {
        let x0 = model.edges[index];
        let x1 = model.edges[index + 1];
        let y = count as f64;
        lines.push(Segment {
            from: Point { x: x0, y: 0.0 },
            to: Point { x: x0, y },
            width: 1.2,
        });
        lines.push(Segment {
            from: Point { x: x0, y },
            to: Point { x: x1, y },
            width: 1.2,
        });
        lines.push(Segment {
            from: Point { x: x1, y: 0.0 },
            to: Point { x: x1, y },
            width: 1.2,
        });
    }
    let y_bottom = model.edges.first().copied().unwrap_or(0.0);
    lines.push(Segment {
        from: Point {
            x: y_bottom,
            y: 0.0,
        },
        to: Point {
            x: model.edges.last().copied().unwrap_or(1.0),
            y: 0.0,
        },
        width: 1.2,
    });
    // Normal overlay, scaled to counts.
    if normal_overlay && model.n >= 3 && model.sd > 0.0 {
        let x_min = *model.edges.first().unwrap_or(&0.0);
        let x_max = *model.edges.last().unwrap_or(&1.0);
        let mut previous = None;
        for step in 0..=200 {
            let x = x_min + (x_max - x_min) * step as f64 / 200.0;
            let y =
                model.n as f64 * model.width * normal_pdf((x - model.mean) / model.sd) / model.sd;
            if let Some((px, py)) = previous {
                lines.push(Segment {
                    from: Point { x: px, y: py },
                    to: Point { x, y },
                    width: 1.5,
                });
            }
            previous = Some((x, y));
        }
    }
    for limit in spec_limits {
        lines.push(Segment {
            from: Point { x: *limit, y: 0.0 },
            to: Point {
                x: *limit,
                y: y_max,
            },
            width: 2.0,
        });
    }
    PlotSpec {
        title: format!("Histogram — {column_name}"),
        x_axis: linear_axis(
            column_name,
            *model.edges.first().unwrap_or(&0.0),
            *model.edges.last().unwrap_or(&1.0),
        ),
        y_axis: linear_axis("Frequency", 0.0, y_max),
        size: (900, 520),
        points: Vec::new(),
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Histogram for a capability report, with specification limits.
pub fn capability_plot(
    df: &DataFrame,
    column_name: &str,
    lsl: Option<f64>,
    usl: Option<f64>,
    normal_overlay: bool,
) -> Result<PlotSpec> {
    let model = fit(df, column_name, None)?;
    let limits: Vec<f64> = [lsl, usl].into_iter().flatten().collect();
    let mut plot = plot_with_options(column_name, &model, &limits, normal_overlay);
    plot.title = format!("Capability Histogram — {column_name}");
    Ok(plot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        DataFrame::new(vec![Column::new_f64(
            "x",
            vec![1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 5.0],
        )])
        .unwrap()
    }

    #[test]
    fn histogram_counts_sum_to_n() {
        let model = fit(&frame(), "x", Some(5)).expect("fit");
        assert_eq!(model.counts.len(), 5);
        assert_eq!(model.counts.iter().sum::<usize>(), model.n);
    }

    #[test]
    fn automatic_bins_are_within_the_limit() {
        let model = fit(&frame(), "x", None).expect("fit");
        assert!((5..=50).contains(&model.counts.len()));
    }

    #[test]
    fn constant_data_is_rejected() {
        let df = DataFrame::new(vec![Column::new_f64("x", vec![1.0, 1.0])]).unwrap();
        assert!(matches!(fit(&df, "x", None), Err(Error::ConstantSample)));
    }

    #[test]
    fn capability_histogram_adds_specification_lines() {
        let plot = capability_plot(&frame(), "x", Some(1.0), Some(5.0), true).expect("plot");
        assert!(plot.title.contains("Capability Histogram"));
        assert!(plot.lines.len() >= 2);
    }

    #[test]
    fn report_and_plot_are_built() {
        let out = analyze(&frame(), "x", Some(5)).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Histogram"), "{text}");
        assert!(!out.plot.expect("plot").lines.is_empty());
    }
}
