//! Type 1 Gage study: bias and repeatability against a known reference.
//!
//! The study uses repeated measurements of one reference part. It reports the
//! mean bias, a Student-t test for zero bias, repeatability (`6·s`) and, when
//! both specification limits are supplied, the percentage of tolerance
//! consumed by repeatability. Reproducibility is not part of a Type 1 study.

use crate::data::{DataFrame, MissingPolicy};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{student_t_cdf, student_t_quantile};
use crate::workflow::AnalysisOutput;

/// Result of a Type 1 Gage study.
#[derive(Debug, Clone, PartialEq)]
pub struct GageType1 {
    /// Observations.
    pub n: usize,
    /// Sample mean.
    pub mean: f64,
    /// Sample standard deviation (`n−1`).
    pub sd: f64,
    /// Known reference value.
    pub reference: f64,
    /// Mean bias (`mean − reference`).
    pub bias: f64,
    /// Standard error of the bias.
    pub se_bias: f64,
    /// t statistic for zero bias.
    pub t: f64,
    /// Two-sided p-value.
    pub p_value: f64,
    /// Repeatability, `6·s`.
    pub repeatability: f64,
    /// Repeatability as a percentage of tolerance, when two limits are given.
    pub percent_tolerance: Option<f64>,
    /// Bias as a percentage of tolerance, when two limits are given.
    pub percent_bias: Option<f64>,
}

/// Fits the Type 1 study.
pub fn fit(
    df: &DataFrame,
    column_name: &str,
    reference: f64,
    lsl: Option<f64>,
    usl: Option<f64>,
) -> Result<GageType1> {
    if !reference.is_finite() {
        return Err(Error::InvalidParameter {
            name: "reference value",
            value: reference,
        });
    }
    if let (Some(lsl), Some(usl)) = (lsl, usl)
        && (!lsl.is_finite() || !usl.is_finite() || lsl >= usl)
    {
        return Err(Error::Unsupported(
            "lower specification limit must be below the upper limit".to_owned(),
        ));
    }
    let sample = df.column(column_name)?.sample(MissingPolicy::Drop)?;
    if sample.n() < 2 {
        return Err(Error::SampleTooSmall {
            n: sample.n(),
            min: 2,
        });
    }
    let n = sample.n();
    let mean = sample.mean()?;
    let sd = sample.sd()?;
    if !sd.is_finite() || sd <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let bias = mean - reference;
    let se_bias = sd / (n as f64).sqrt();
    let t = bias / se_bias;
    let cdf = student_t_cdf(t, n as f64 - 1.0);
    let p_value = (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0);
    let repeatability = 6.0 * sd;
    let (percent_tolerance, percent_bias) = match (lsl, usl) {
        (Some(lsl), Some(usl)) => {
            let tolerance = usl - lsl;
            (
                Some(100.0 * repeatability / tolerance),
                Some(100.0 * bias / tolerance),
            )
        }
        _ => (None, None),
    };
    Ok(GageType1 {
        n,
        mean,
        sd,
        reference,
        bias,
        se_bias,
        t,
        p_value,
        repeatability,
        percent_tolerance,
        percent_bias,
    })
}

/// Builds the Session report.
pub fn analyze(
    df: &DataFrame,
    column_name: &str,
    reference: f64,
    lsl: Option<f64>,
    usl: Option<f64>,
) -> Result<AnalysisOutput> {
    let model = fit(df, column_name, reference, lsl, usl)?;
    let mut result = AnalysisResult::new(
        format!("Gage Type 1 — column {column_name}"),
        "Bias and repeatability against a known reference",
    );
    result.n = model.n;
    result.excluded = df.n_rows() - model.n;
    result.statistics = vec![
        ("Mean", model.mean),
        ("Standard deviation", model.sd),
        ("Reference", model.reference),
        ("Bias", model.bias),
        ("SE bias", model.se_bias),
        ("t", model.t),
        ("p-value", model.p_value),
        ("Repeatability (6s)", model.repeatability),
    ];
    if let Some(value) = model.percent_tolerance {
        result
            .statistics
            .push(("%Tolerance (repeatability)", value));
    }
    if let Some(value) = model.percent_bias {
        result.statistics.push(("%Bias", value));
    }
    let ci = if model.se_bias > 0.0 {
        let critical = student_t_quantile(0.975, model.n as f64 - 1.0);
        Some((
            model.bias - critical * model.se_bias,
            model.bias + critical * model.se_bias,
        ))
    } else {
        None
    };
    result.extra.push(Item::Table {
        headers: vec!["Quantity".to_owned(), "Value".to_owned()],
        rows: vec![
            vec!["Mean".to_owned(), format!("{:.6}", model.mean)],
            vec!["Standard deviation".to_owned(), format!("{:.6}", model.sd)],
            vec!["Reference".to_owned(), format!("{:.6}", model.reference)],
            vec!["Bias".to_owned(), format!("{:.6}", model.bias)],
            vec![
                "Bias 95% CI".to_owned(),
                ci.map_or_else(|| "—".to_owned(), |(a, b)| format!("({a:.6}, {b:.6})")),
            ],
            vec![
                "p-value (bias = 0)".to_owned(),
                format!("{:.6}", model.p_value),
            ],
            vec![
                "Repeatability (6s)".to_owned(),
                format!("{:.6}", model.repeatability),
            ],
            vec![
                "%Tolerance".to_owned(),
                model
                    .percent_tolerance
                    .map_or_else(|| "—".to_owned(), |value| format!("{value:.3}")),
            ],
        ],
    });
    result.notes = vec![
        (
            "Study".to_owned(),
            "one reference part measured repeatedly; no operator or part variation".to_owned(),
        ),
        (
            "Bias".to_owned(),
            "Student t test of mean − reference with n−1 df".to_owned(),
        ),
        (
            "Tolerance".to_owned(),
            "repeatability and bias as percentages of USL − LSL".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, result.excluded, 0);
    result.warnings.push(
        "Type 1 Gage conventions and acceptance thresholds have not been verified against documented output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    #[test]
    fn type1_reports_bias_and_tolerance() {
        let df = DataFrame::new(vec![Column::new_f64(
            "m",
            vec![10.1, 10.2, 10.0, 10.3, 10.1, 10.2],
        )])
        .unwrap();
        let model = fit(&df, "m", 10.0, Some(9.0), Some(11.0)).expect("fit");
        assert!((model.bias - 0.15).abs() < 1e-12);
        assert!(model.p_value < 0.05);
        assert!(model.percent_tolerance.unwrap() > 0.0);
    }

    #[test]
    fn constant_measurements_are_rejected() {
        let df = DataFrame::new(vec![Column::new_f64("m", vec![10.0, 10.0])]).unwrap();
        assert!(matches!(
            fit(&df, "m", 10.0, None, None),
            Err(Error::ConstantSample)
        ));
    }

    #[test]
    fn report_is_built() {
        let df = DataFrame::new(vec![Column::new_f64(
            "m",
            vec![10.1, 10.2, 10.0, 10.3, 10.1, 10.2],
        )])
        .unwrap();
        let out = analyze(&df, "m", 10.0, Some(9.0), Some(11.0)).expect("analysis");
        assert!(out.results[0].to_session_text().contains("Gage Type 1"));
        assert!(out.plot.is_none());
    }
}
