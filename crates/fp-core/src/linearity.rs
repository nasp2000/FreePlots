// SPDX-License-Identifier: MIT
//! Gage linearity and bias study.
//!
//! The study measures several reference parts repeatedly and compares the
//! measured mean with the reference value of each part. The **bias** of a part
//! is `mean − reference`; the **linearity** is the change of that bias across
//! the range of the references, estimated by ordinary least squares. The
//! report gives the mean bias with its t test, the regression of bias on the
//! reference, the linearity against the process variation and a plot of the
//! biases with the fitted line.
//!
//! The process variation is the `6·s` spread of the reference values of the
//! study, which is the standard convention for a linearity study; the exact
//! convention of the documented behaviour was not documented, so the report says so
//! — see the project notes.

use std::collections::BTreeMap;

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{student_t_cdf, student_t_quantile};
use crate::workflow::AnalysisOutput;

/// Result of a Gage linearity and bias study.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearityStudy {
    /// Reference values, one per part, in ascending order.
    pub references: Vec<f64>,
    /// Mean measurement of each part.
    pub means: Vec<f64>,
    /// Bias of each part (`mean − reference`).
    pub biases: Vec<f64>,
    /// Number of measurements used.
    pub n: usize,
    /// Measurements of each part.
    pub counts: Vec<usize>,
    /// Mean bias across the parts.
    pub mean_bias: f64,
    /// Standard error of the mean bias.
    pub bias_se: f64,
    /// t statistic of the mean bias.
    pub bias_t: f64,
    /// Two-sided p-value of the mean bias.
    pub bias_p: f64,
    /// Slope of the bias on the reference.
    pub slope: f64,
    /// Intercept of the bias on the reference.
    pub intercept: f64,
    /// Standard error of the slope.
    pub slope_se: f64,
    /// t statistic of the slope.
    pub slope_t: f64,
    /// Two-sided p-value of the slope (degrees of freedom `parts − 2`).
    pub slope_p: f64,
    /// R² of the regression of the bias on the reference.
    pub r_squared: f64,
    /// Process variation (`6·s` of the reference values).
    pub process_variation: f64,
    /// Linearity, `|slope|·process variation`.
    pub linearity: f64,
    /// Linearity as a percentage of the process variation.
    pub percent_linearity: f64,
    /// Bias as a percentage of the process variation.
    pub percent_bias: f64,
}

/// Runs the study.
///
/// The rows are grouped by the reference value: every distinct reference is a
/// part. A part whose measurements are constant has no slope contribution but
/// is kept for the bias; parts with a reference equal to the median of the
/// references carry no leverage and are also kept.
pub fn fit(df: &DataFrame, measurement: &str, reference: &str) -> Result<LinearityStudy> {
    if measurement == reference {
        return Err(Error::Unsupported(
            "the measurement and the reference must be different columns".to_owned(),
        ));
    }
    let measurements = df.column(measurement)?;
    let references = df.column(reference)?;
    let mut groups: BTreeMap<u64, (f64, Vec<f64>)> = BTreeMap::new();
    let mut n = 0usize;
    for row in 0..df.n_rows() {
        if measurements.is_missing(row) || references.is_missing(row) {
            continue;
        }
        let value = measurements.value(row).unwrap_or(f64::NAN);
        let target = references.value(row).unwrap_or(f64::NAN);
        if !value.is_finite() || !target.is_finite() {
            continue;
        }
        // Group on the bit pattern so that equal references share a part.
        groups
            .entry(target.to_bits())
            .or_insert_with(|| (target, Vec::new()))
            .1
            .push(value);
        n += 1;
    }
    if groups.len() < 3 {
        return Err(Error::Unsupported(
            "a linearity study needs at least three reference parts".to_owned(),
        ));
    }
    if n < groups.len() * 2 {
        return Err(Error::SampleTooSmall {
            n,
            min: groups.len() * 2,
        });
    }

    let mut refs = Vec::with_capacity(groups.len());
    let mut means = Vec::with_capacity(groups.len());
    let mut biases = Vec::with_capacity(groups.len());
    let mut counts = Vec::with_capacity(groups.len());
    for (target, values) in groups.values() {
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        refs.push(*target);
        means.push(mean);
        biases.push(mean - target);
        counts.push(values.len());
    }

    let parts = refs.len() as f64;
    let mean_bias = biases.iter().sum::<f64>() / parts;
    let bias_sd = if parts > 1.0 {
        let sum: f64 = biases
            .iter()
            .map(|bias| (bias - mean_bias) * (bias - mean_bias))
            .sum();
        (sum / (parts - 1.0)).sqrt()
    } else {
        0.0
    };
    let bias_se = if parts > 1.0 && bias_sd > 0.0 {
        bias_sd / parts.sqrt()
    } else {
        0.0
    };
    let bias_t = if bias_se > 0.0 {
        mean_bias / bias_se
    } else if mean_bias.abs() < 1e-15 {
        0.0
    } else {
        f64::INFINITY
    };
    let bias_p = if bias_t.is_finite() {
        let cdf = student_t_cdf(bias_t, parts - 1.0);
        (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let mean_reference = refs.iter().sum::<f64>() / parts;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for (target, bias) in refs.iter().zip(&biases) {
        sxx += (target - mean_reference) * (target - mean_reference);
        sxy += (target - mean_reference) * (bias - mean_bias);
    }
    if sxx <= 0.0 {
        return Err(Error::Unsupported(
            "the reference values must not all be equal".to_owned(),
        ));
    }
    let slope = sxy / sxx;
    let intercept = mean_bias - slope * mean_reference;
    let mut sse = 0.0;
    for (target, bias) in refs.iter().zip(&biases) {
        let fitted = intercept + slope * target;
        sse += (bias - fitted) * (bias - fitted);
    }
    let degrees_of_freedom = parts - 2.0;
    let residual_variance = if degrees_of_freedom > 0.0 {
        sse / degrees_of_freedom
    } else {
        0.0
    };
    let slope_se = (residual_variance / sxx).sqrt();
    let slope_t = if slope_se > 0.0 {
        slope / slope_se
    } else if slope.abs() < 1e-15 {
        0.0
    } else {
        f64::INFINITY
    };
    let slope_p = if degrees_of_freedom > 0.0 && slope_t.is_finite() {
        let cdf = student_t_cdf(slope_t, degrees_of_freedom);
        (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0)
    } else if degrees_of_freedom <= 0.0 {
        f64::NAN
    } else {
        0.0
    };
    let sst: f64 = biases.iter().map(|bias| (bias - mean_bias).powi(2)).sum();
    let r_squared = if sst > 0.0 {
        (1.0 - sse / sst).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let reference_mean = mean_reference;
    let reference_sd = if parts > 1.0 {
        (refs
            .iter()
            .map(|target| (target - reference_mean).powi(2))
            .sum::<f64>()
            / (parts - 1.0))
            .sqrt()
    } else {
        0.0
    };
    let process_variation = 6.0 * reference_sd;
    let linearity = if process_variation > 0.0 {
        slope.abs() * process_variation
    } else {
        0.0
    };
    let percent_linearity = if process_variation > 0.0 {
        100.0 * linearity / process_variation
    } else {
        f64::NAN
    };
    let percent_bias = if process_variation > 0.0 {
        100.0 * mean_bias.abs() / process_variation
    } else {
        f64::NAN
    };

    Ok(LinearityStudy {
        references: refs,
        means,
        biases,
        n,
        counts,
        mean_bias,
        bias_se,
        bias_t,
        bias_p,
        slope,
        intercept,
        slope_se,
        slope_t,
        slope_p,
        r_squared,
        process_variation,
        linearity,
        percent_linearity,
        percent_bias,
    })
}

impl LinearityStudy {
    /// `95 %` confidence interval of the slope.
    #[must_use]
    pub fn slope_interval(&self) -> (f64, f64) {
        let degrees_of_freedom = self.references.len() as f64 - 2.0;
        if degrees_of_freedom <= 0.0 {
            return (f64::NAN, f64::NAN);
        }
        let critical = student_t_quantile(0.975, degrees_of_freedom);
        (
            self.slope - critical * self.slope_se,
            self.slope + critical * self.slope_se,
        )
    }
}

/// Plot of the bias against the reference with the fitted line.
fn plot(model: &LinearityStudy) -> PlotSpec {
    let x_min = model
        .references
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let x_max = model
        .references
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let y_min = model.biases.iter().copied().fold(f64::INFINITY, f64::min);
    let y_max = model
        .biases
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let x_pad = ((x_max - x_min).abs() * 0.05).max(1e-9);
    let y_pad = ((y_max - y_min).abs() * 0.05).max(1e-9);
    PlotSpec {
        title: "Gage Linearity and Bias Study".to_owned(),
        x_axis: linear_axis("Reference", x_min - x_pad, x_max + x_pad),
        y_axis: linear_axis("Bias", y_min - y_pad, y_max + y_pad),
        size: (900, 560),
        points: model
            .references
            .iter()
            .zip(&model.biases)
            .map(|(&x, &y)| Point { x, y })
            .collect(),
        lines: vec![Segment {
            from: Point {
                x: x_min,
                y: model.intercept + model.slope * x_min,
            },
            to: Point {
                x: x_max,
                y: model.intercept + model.slope * x_max,
            },
            width: 2.0,
        }],
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Runs the study and builds the Session report and the plot.
pub fn analyze(df: &DataFrame, measurement: &str, reference: &str) -> Result<AnalysisOutput> {
    let model = fit(df, measurement, reference)?;
    let (low, high) = model.slope_interval();
    let excluded = df.n_rows() - model.n;

    let mut result = AnalysisResult::new(
        format!("Gage Linearity and Bias Study — {measurement} against {reference}"),
        "Bias of each reference part, regressed on the reference".to_owned(),
    );
    result.n = model.n;
    result.excluded = excluded;
    result.statistics = vec![
        ("Reference parts", model.references.len() as f64),
        ("Mean bias", model.mean_bias),
        ("Bias p-value", model.bias_p),
        ("Slope", model.slope),
        ("Slope p-value", model.slope_p),
        ("R²", model.r_squared),
        ("Process variation", model.process_variation),
        ("Linearity", model.linearity),
        ("%Linearity", model.percent_linearity),
        ("%Bias", model.percent_bias),
    ];

    result.extra.push(Item::Table {
        headers: vec![
            "Reference".to_owned(),
            "Measurements".to_owned(),
            "Mean".to_owned(),
            "Bias".to_owned(),
        ],
        rows: model
            .references
            .iter()
            .enumerate()
            .map(|(index, target)| {
                vec![
                    format!("{target:.6}"),
                    format!("{}", model.counts[index]),
                    format!("{:.6}", model.means[index]),
                    format!("{:.6}", model.biases[index]),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Term".to_owned(),
            "Estimate".to_owned(),
            "SE".to_owned(),
            "t".to_owned(),
            "P".to_owned(),
        ],
        rows: vec![
            vec![
                "Constant".to_owned(),
                format!("{:.6}", model.intercept),
                String::new(),
                String::new(),
                String::new(),
            ],
            vec![
                "Slope".to_owned(),
                format!("{:.6}", model.slope),
                format!("{:.6}", model.slope_se),
                format!("{:.6}", model.slope_t),
                format!("{:.6}", model.slope_p),
            ],
        ],
    });

    let decision = if model.slope_p < 0.05 {
        "is significant"
    } else {
        "is not significant"
    };
    result.extra.push(Item::Message(format!(
        "The fitted bias grows by {:.6} per unit of the reference; the slope {} at the 5% level \
         (95% CI {:.6} to {:.6}).",
        model.slope, decision, low, high
    )));
    result.notes = vec![
        (
            "Bias".to_owned(),
            "bias of a part = mean measurement − reference value".to_owned(),
        ),
        (
            "Mean bias".to_owned(),
            "the mean of the part biases, with a one-sample t test on parts − 1 degrees of \
             freedom"
                .to_owned(),
        ),
        (
            "Linearity".to_owned(),
            "|slope|·process variation, where the process variation is 6·s of the reference \
             values of the study"
                .to_owned(),
        ),
        (
            "Assumption".to_owned(),
            "the within-part spread is due to the measurement system only".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing measurement or reference are excluded and counted".to_owned(),
        ),
    ];
    let worst = model
        .biases
        .iter()
        .map(|bias| bias.abs())
        .fold(0.0, f64::max);
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(format!(
        "the largest part bias is {worst:.6}; the linearity convention (6·s of the references) is \
         a standard choice and has not been verified against documented output"
    ));

    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot(&model)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    /// Five parts measured three times each, with a bias that grows with the
    /// reference (`bias = 0.1·reference − 0.1`).
    fn frame() -> DataFrame {
        let references = [10.0, 20.0, 30.0, 40.0, 50.0];
        let mut values = Vec::new();
        let mut targets = Vec::new();
        for reference in references {
            let bias = 0.1 * reference - 0.1;
            for offset in [-0.2, 0.0, 0.2] {
                values.push(reference + bias + offset);
                targets.push(reference);
            }
        }
        DataFrame::new(vec![
            Column::new_f64("measurement", values),
            Column::new_f64("reference", targets),
        ])
        .expect("frame")
    }

    #[test]
    fn the_slope_recovers_the_known_bias() {
        let model = fit(&frame(), "measurement", "reference").expect("fit");
        assert_eq!(model.references.len(), 5);
        assert_eq!(model.n, 15);
        // bias = 0.1·reference − 0.1 exactly (the offsets cancel in the mean).
        assert!((model.slope - 0.1).abs() < 1e-12, "slope {}", model.slope);
        assert!((model.intercept + 0.1).abs() < 1e-12, "{}", model.intercept);
        assert!((model.r_squared - 1.0).abs() < 1e-12);
        // The biases are 0.9, 1.9, 2.9, 3.9 and 4.9, so the mean is 2.9.
        assert!((model.mean_bias - 2.9).abs() < 1e-12, "{}", model.mean_bias);
        // The process variation is 6·s of 10, 20, 30, 40, 50: 6·sqrt(250).
        assert!(
            (model.process_variation - 94.868_329_805_051_38).abs() < 1e-9,
            "{}",
            model.process_variation
        );
        // Linearity = |0.1|·process variation.
        assert!((model.linearity - 0.1 * model.process_variation).abs() < 1e-12);
        assert!((model.percent_linearity - 10.0).abs() < 1e-9);
    }

    #[test]
    fn a_constant_bias_has_no_linearity() {
        let references = [10.0, 20.0, 30.0, 40.0];
        let mut values = Vec::new();
        let mut targets = Vec::new();
        for reference in references {
            for _ in 0..3 {
                values.push(reference + 0.5);
                targets.push(reference);
            }
        }
        let df = DataFrame::new(vec![
            Column::new_f64("measurement", values),
            Column::new_f64("reference", targets),
        ])
        .expect("frame");
        let model = fit(&df, "measurement", "reference").expect("fit");
        assert!(model.slope.abs() < 1e-12);
        assert!(model.linearity.abs() < 1e-12);
        assert!((model.mean_bias - 0.5).abs() < 1e-12);
        assert!((model.percent_bias - 100.0 * 0.5 / model.process_variation).abs() < 1e-9);
    }

    #[test]
    fn the_study_needs_three_parts() {
        let df = DataFrame::new(vec![
            Column::new_f64("measurement", vec![1.0, 1.1, 2.0, 2.1]),
            Column::new_f64("reference", vec![1.0, 1.0, 2.0, 2.0]),
        ])
        .expect("frame");
        assert!(matches!(
            fit(&df, "measurement", "reference"),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            fit(&df, "reference", "reference"),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn the_report_and_the_plot_are_built() {
        let out = analyze(&frame(), "measurement", "reference").expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Gage Linearity"), "{text}");
        assert!(text.contains("Linearity"), "{text}");
        assert!(text.contains("Slope"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.points.len(), 5);
        assert_eq!(plot.lines.len(), 1);
    }

    #[test]
    fn a_missing_row_does_not_shift_the_columns() {
        let rows: Vec<(f64, f64)> = [10.0, 20.0, 30.0, 40.0]
            .iter()
            .flat_map(|reference| {
                [-0.2, 0.0, 0.2]
                    .iter()
                    .map(|offset| (reference + offset, *reference))
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut with_missing = rows.clone();
        with_missing.insert(0, (f64::NAN, 999.0));
        let frame_of = |rows: &[(f64, f64)]| {
            DataFrame::new(vec![
                Column::new_f64("measurement", rows.iter().map(|row| row.0).collect()),
                Column::new_f64("reference", rows.iter().map(|row| row.1).collect()),
            ])
            .expect("frame")
        };
        let a = fit(&frame_of(&with_missing), "measurement", "reference").expect("linearity");
        let b = fit(&frame_of(&rows), "measurement", "reference").expect("linearity");
        assert_eq!(a.n, 12);
        assert_eq!(a.n, b.n);
        for (left, right) in a.biases.iter().zip(&b.biases) {
            assert!(
                (left - right).abs() < 1e-9,
                "{:?} vs {:?}",
                a.biases,
                b.biases
            );
        }
    }
}
