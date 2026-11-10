// SPDX-License-Identifier: MIT
//! Empirical cumulative distribution function.
//!
//! The empirical CDF at the `i`-th order statistic is `F̂(x₍ᵢ₎) = i/n`; the plot
//! draws it as a **step function** (horizontal to the next observation, then a
//! jump) together with a **confidence band** of the Dvoretzky–Kiefer–Wolfowitz
//! type:
//!
//! ```text
//! ε = sqrt(ln(2/α) / (2n))
//! ```
//!
//! so that `P(sup |F̂ − F| > ε) ≤ α` asymptotically. The band is clipped to
//! `[0, 1]`, and the report also gives the quartiles and the KS distance of the
//! sample (the largest jump of the step).
//!
//! The documented behaviour has an empirical CDF plot; its band convention was not
//! documented, so the numbers are not claimed to match the documented behaviour — see
//! the project notes.

use crate::data::{DataFrame, MissingPolicy};
use crate::error::{Error, Result};
use crate::plot::{Band, PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// Result of the empirical CDF.
#[derive(Debug, Clone, PartialEq)]
pub struct Ecdf {
    /// Sorted observations.
    pub values: Vec<f64>,
    /// Cumulative probability at each observation.
    pub probabilities: Vec<f64>,
    /// Half-width of the confidence band.
    pub epsilon: f64,
    /// Confidence level of the band.
    pub confidence: f64,
    /// Observations used.
    pub n: usize,
    /// Rows excluded.
    pub excluded: usize,
    /// Quartiles (interpolated, `h = (n + 1)p`).
    pub quartiles: [f64; 3],
    /// Largest jump between consecutive empirical probabilities.
    pub largest_jump: f64,
}

/// Builds the empirical CDF of a sample.
pub fn from_values(values: &[f64], confidence: f64) -> Result<Ecdf> {
    if values.is_empty() {
        return Err(Error::EmptySample);
    }
    if !(0.0..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence",
            value: confidence,
        });
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let probabilities: Vec<f64> = (1..=n).map(|index| index as f64 / n as f64).collect();
    let alpha = 1.0 - confidence;
    let epsilon = (2.0f64 / alpha).ln() / (2.0 * n as f64);
    let epsilon = epsilon.sqrt().clamp(0.0, 1.0);
    let quartiles = [
        quantile(&sorted, 0.25),
        quantile(&sorted, 0.5),
        quantile(&sorted, 0.75),
    ];
    let largest_jump = probabilities
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .fold(1.0 / n as f64, f64::max);
    Ok(Ecdf {
        values: sorted,
        probabilities,
        epsilon,
        confidence,
        n,
        excluded: 0,
        quartiles,
        largest_jump,
    })
}

/// Interpolated quantile of a sorted sample, with `h = (n + 1)p`.
fn quantile(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 1 {
        return sorted[0];
    }
    let position = p * (n - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let fraction = position - lower as f64;
    sorted[lower] * (1.0 - fraction) + sorted[upper] * fraction
}

/// Builds the plot with the step function and the band.
fn plot(model: &Ecdf) -> PlotSpec {
    let low = model.values[0];
    let high = model.values[model.values.len() - 1];
    let pad = ((high - low).abs() * 0.05).max(1e-9);
    let n = model.values.len();
    let stride = crate::plot::plot_stride(n);
    let sampled = |index: usize| index.is_multiple_of(stride) || index + 1 == n;
    let mut points = Vec::new();
    let mut lines = Vec::new();
    let mut previous = 0.0;
    let mut previous_x = low - pad;
    for (index, (value, probability)) in model.values.iter().zip(&model.probabilities).enumerate() {
        if !sampled(index) {
            continue;
        }
        // Horizontal run to the observation, then the vertical jump.
        lines.push(Segment {
            from: Point {
                x: previous_x,
                y: previous,
            },
            to: Point {
                x: *value,
                y: previous,
            },
            width: 2.0,
        });
        lines.push(Segment {
            from: Point {
                x: *value,
                y: previous,
            },
            to: Point {
                x: *value,
                y: *probability,
            },
            width: 2.0,
        });
        points.push(Point {
            x: *value,
            y: *probability,
        });
        previous = *probability;
        previous_x = *value;
    }
    // The band as a ribbon around the step, clipped to [0, 1].
    let mut lower = Vec::new();
    let mut upper = Vec::new();
    for (index, (value, probability)) in model.values.iter().zip(&model.probabilities).enumerate() {
        if !sampled(index) {
            continue;
        }
        lower.push(Point {
            x: *value,
            y: (probability - model.epsilon).max(0.0),
        });
        upper.push(Point {
            x: *value,
            y: (probability + model.epsilon).min(1.0),
        });
    }
    PlotSpec {
        title: "Empirical CDF".to_owned(),
        x_axis: linear_axis("Data", low - pad, high + pad),
        y_axis: linear_axis("Cumulative probability", 0.0, 1.0),
        size: (900, 540),
        points,
        lines,
        bands: vec![Band { lower, upper }],
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: crate::plot::sampling_notes(n),
    }
}

/// Runs the analysis and builds the report.
pub fn analyze(df: &DataFrame, column_name: &str, confidence: f64) -> Result<AnalysisOutput> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let mut model = from_values(sample.values(), confidence)?;
    model.excluded = sample.excluded();
    let mut result = AnalysisResult::new(
        format!("Empirical CDF — column {column_name}"),
        format!(
            "Step function with a {:.0}% Dvoretzky–Kiefer–Wolfowitz band",
            confidence * 100.0
        ),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Band half-width", model.epsilon),
        ("Largest jump", model.largest_jump),
        ("First quartile", model.quartiles[0]),
        ("Median", model.quartiles[1]),
        ("Third quartile", model.quartiles[2]),
        ("Minimum", model.values[0]),
        ("Maximum", model.values[model.values.len() - 1]),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Observation".to_owned(),
            "Value".to_owned(),
            "F(x)".to_owned(),
        ],
        rows: model
            .values
            .iter()
            .zip(&model.probabilities)
            .take(20)
            .enumerate()
            .map(|(index, (value, probability))| {
                vec![
                    format!("{}", index + 1),
                    format!("{value:.6}"),
                    format!("{probability:.6}"),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Estimator".to_owned(),
            "F̂(x₍ᵢ₎) = i/n at the order statistics; the plot joins them with a step".to_owned(),
        ),
        (
            "Band".to_owned(),
            "ε = √(ln(2/α)/(2n)) with α = 1 − confidence: the DKW inequality bounds the \
             probability that the whole step leaves the band"
                .to_owned(),
        ),
        (
            "Quartiles".to_owned(),
            "interpolated quantiles (h = (n + 1)p), as elsewhere in the engine".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, sample.ties());
    result.warnings.push(
        "the band is the asymptotic DKW width; the exact finite-sample bound is slightly wider \
         and the documented convention was not documented"
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

    #[test]
    fn the_step_reaches_one_and_stays_sorted() {
        let values = vec![3.0, 1.0, 2.0, 5.0, 4.0];
        let model = from_values(&values, 0.95).expect("ecdf");
        assert_eq!(model.values, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!((model.probabilities[0] - 0.2).abs() < 1e-12);
        assert!((model.probabilities[4] - 1.0).abs() < 1e-12);
        assert!((model.largest_jump - 0.2).abs() < 1e-12);
        assert!((model.quartiles[1] - 3.0).abs() < 1e-12);
        // The band width falls with the sample size.
        let wide = from_values(&values, 0.95).expect("wide");
        let many: Vec<f64> = (1..=100).map(|index| index as f64).collect();
        let narrow = from_values(&many, 0.95).expect("narrow");
        assert!(narrow.epsilon < wide.epsilon);
    }

    #[test]
    fn the_band_width_matches_the_dkw_formula() {
        let values: Vec<f64> = (1..=50).map(|index| index as f64).collect();
        let model = from_values(&values, 0.90).expect("ecdf");
        let expected = ((2.0f64 / 0.10).ln() / (2.0 * 50.0)).sqrt();
        assert!(
            (model.epsilon - expected).abs() < 1e-12,
            "{}",
            model.epsilon
        );
        assert!(matches!(
            from_values(&values, 1.5),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(from_values(&[], 0.95), Err(Error::EmptySample)));
    }

    #[test]
    fn the_plot_has_a_step_and_a_band() {
        let out = {
            let df = DataFrame::new(vec![Column::new_f64(
                "x",
                vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, f64::NAN],
            )])
            .expect("frame");
            analyze(&df, "x", 0.95).expect("analysis")
        };
        assert_eq!(out.results[0].excluded, 1);
        let plot = out.plot.expect("plot");
        assert_eq!(plot.points.len(), 6);
        assert_eq!(plot.lines.len(), 12);
        assert_eq!(plot.bands.len(), 1);
        // The band stays inside [0, 1].
        assert!(plot.bands[0].lower.iter().all(|point| point.y >= 0.0));
        assert!(plot.bands[0].upper.iter().all(|point| point.y <= 1.0));
        let text = out.results[0].to_session_text();
        assert!(text.contains("Empirical CDF"), "{text}");
        assert!(text.contains("Band half-width"), "{text}");
    }

    #[test]
    fn a_large_ecdf_is_thinned_with_a_note() {
        let n = 10_001usize;
        let values: Vec<f64> = (0..n).map(|index| (index % 97) as f64).collect();
        let model = from_values(&values, 0.95).expect("ecdf");
        let plot = plot(&model);
        assert!(plot.points.len() <= crate::plot::MAX_PLOT_POINTS + 1);
        assert!(plot.points.len() < n);
        assert!(plot.bands[0].lower.len() <= crate::plot::MAX_PLOT_POINTS + 1);
        assert!(!plot.notes.is_empty(), "the sample must be explained");
        // The statistics still use every observation.
        assert_eq!(model.n, n);
    }
}
