// SPDX-License-Identifier: MIT
//! Acceptance control chart.
//!
//! The chart plots the mean of each subgroup against limits derived from the
//! specification limits rather than from the process spread alone. For each
//! specification limit the user supplies an acceptable proportion outside the
//! limit (`p₁`), a rejectable proportion (`p₂`), the producer risk (`α`) and
//! the consumer risk (`β`). A point inside the acceptance limits accepts the
//! process, a point beyond the rejection limits rejects it, and a point in
//! between asks for another subgroup.
//!
//! The formulas follow Freund's acceptance control chart as presented in
//! Montgomery; the within-subgroup sigma is either supplied or estimated from
//! the average subgroup range (the average moving range for individuals).

use crate::data::{DataFrame, MissingPolicy, Sample};
use crate::error::{Error, Result};
use crate::plot::{Band, PlotNote, PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::spc::d2;
use crate::special::normal_quantile;
use crate::workflow::AnalysisOutput;

/// Options of an acceptance chart.
#[derive(Debug, Clone, PartialEq)]
pub struct AcceptanceOptions {
    /// Lower specification limit, when the characteristic has one.
    pub lower_spec: Option<f64>,
    /// Upper specification limit, when the characteristic has one.
    pub upper_spec: Option<f64>,
    /// Subgroup size; 1 charts the individuals.
    pub subgroup: usize,
    /// Known process standard deviation; `None` estimates it from the data.
    pub sigma: Option<f64>,
    /// Accepted proportion outside a specification limit (`p₁`).
    pub acceptable: f64,
    /// Rejectable proportion outside a specification limit (`p₂`).
    pub rejectable: f64,
    /// Producer risk (`α`): chance of rejecting an acceptable process.
    pub producer: f64,
    /// Consumer risk (`β`): chance of accepting a rejectable process.
    pub consumer: f64,
}

impl Default for AcceptanceOptions {
    fn default() -> Self {
        Self {
            lower_spec: None,
            upper_spec: None,
            subgroup: 1,
            sigma: None,
            acceptable: 0.001,
            rejectable: 0.05,
            producer: 0.05,
            consumer: 0.10,
        }
    }
}

/// Decision taken for one plotted point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The mean is inside the acceptance limits.
    Accept,
    /// The mean is between the acceptance and the rejection limits.
    Continue,
    /// The mean is beyond a rejection limit.
    Reject,
}

impl Decision {
    /// Name used in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Accept => "Accept",
            Self::Continue => "Continue",
            Self::Reject => "Reject",
        }
    }
}

/// Limits of the chart, `None` on the side without a specification limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcceptanceLimits {
    /// Lower acceptance limit.
    pub accept_low: Option<f64>,
    /// Upper acceptance limit.
    pub accept_high: Option<f64>,
    /// Lower rejection limit.
    pub reject_low: Option<f64>,
    /// Upper rejection limit.
    pub reject_high: Option<f64>,
}

/// Chart-level values, exposed for the tests and the interface.
#[derive(Debug, Clone, PartialEq)]
pub struct AcceptanceChart {
    /// Subgroup means, in worksheet order.
    pub means: Vec<f64>,
    /// Decision of every mean.
    pub decisions: Vec<Decision>,
    /// Limits of the chart.
    pub limits: AcceptanceLimits,
    /// Process standard deviation used.
    pub sigma: f64,
    /// Subgroup size.
    pub subgroup: usize,
    /// Observations used.
    pub n: usize,
    /// Observations excluded.
    pub excluded: usize,
    /// Trailing observations that did not fill a subgroup.
    pub dropped: usize,
}

/// Runs the acceptance chart of one numeric column.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for invalid proportions, risks, specifications
/// or subgroup size, and for limits that cross; and [`Error::SampleTooSmall`] /
/// [`Error::ConstantSample`] for a sample that cannot produce a chart.
pub fn analyze(
    df: &DataFrame,
    column: &str,
    options: &AcceptanceOptions,
) -> Result<AnalysisOutput> {
    check_options(options)?;
    let data = df.column(column)?;
    let sample = data.sample(MissingPolicy::Drop)?;
    let chart = chart(&sample, options)?;
    Ok(AnalysisOutput {
        results: vec![report(column, &chart, options)],
        plot: Some(plot(column, &chart)),
    })
}

/// Validates the options before touching the data.
fn check_options(options: &AcceptanceOptions) -> Result<()> {
    if options.lower_spec.is_none() && options.upper_spec.is_none() {
        return Err(Error::Unsupported(
            "give at least one specification limit (LSL or USL)".to_owned(),
        ));
    }
    if let (Some(low), Some(high)) = (options.lower_spec, options.upper_spec)
        && !(low.is_finite() && high.is_finite() && low < high)
    {
        return Err(Error::Unsupported(
            "the lower specification must be a finite number below the upper one".to_owned(),
        ));
    }
    for limit in [options.lower_spec, options.upper_spec]
        .into_iter()
        .flatten()
    {
        if !limit.is_finite() {
            return Err(Error::Unsupported(
                "the specification limits must be finite".to_owned(),
            ));
        }
    }
    if !(options.acceptable > 0.0 && options.acceptable < 0.5)
        || !(options.rejectable > options.acceptable && options.rejectable < 0.5)
    {
        return Err(Error::Unsupported(
            "the accepted proportion and the rejectable proportion must satisfy 0 < p₁ < p₂ < 0.5"
                .to_owned(),
        ));
    }
    if !(options.producer > 0.0 && options.producer < 1.0)
        || !(options.consumer > 0.0 && options.consumer < 1.0)
    {
        return Err(Error::Unsupported(
            "the producer and consumer risks must be between 0 and 1".to_owned(),
        ));
    }
    if options.subgroup != 1 && !(2..=25).contains(&options.subgroup) {
        return Err(Error::Unsupported(format!(
            "subgroup size {} is outside the supported range 1 or 2–25",
            options.subgroup
        )));
    }
    if let Some(sigma) = options.sigma
        && !(sigma.is_finite() && sigma > 0.0)
    {
        return Err(Error::Unsupported(
            "the known standard deviation must be a finite positive number".to_owned(),
        ));
    }
    Ok(())
}

/// Builds the means, the limits and the decision of every point.
fn chart(sample: &Sample, options: &AcceptanceOptions) -> Result<AcceptanceChart> {
    let values = sample.values();
    let minimum = if options.subgroup == 1 {
        2
    } else {
        options.subgroup * 2
    };
    if values.len() < minimum {
        return Err(Error::SampleTooSmall {
            n: values.len(),
            min: minimum,
        });
    }
    let (means, dropped) = if options.subgroup == 1 {
        (values.to_vec(), 0)
    } else {
        let used = values.len() / options.subgroup * options.subgroup;
        let means = values[..used]
            .chunks_exact(options.subgroup)
            .map(|group| group.iter().sum::<f64>() / group.len() as f64)
            .collect();
        (means, values.len() - used)
    };
    let sigma = match options.sigma {
        Some(sigma) => sigma,
        None => estimate_sigma(values, options.subgroup)?,
    };
    let z_accept = normal_quantile(1.0 - options.acceptable);
    let z_reject = normal_quantile(1.0 - options.rejectable);
    let z_producer = normal_quantile(1.0 - options.producer);
    let z_consumer = normal_quantile(1.0 - options.consumer);
    let se = sigma / (options.subgroup as f64).sqrt();
    let mut limits = AcceptanceLimits {
        accept_low: None,
        accept_high: None,
        reject_low: None,
        reject_high: None,
    };
    if let Some(low) = options.lower_spec {
        let accept = low + z_accept * sigma - z_producer * se;
        let reject = low + z_reject * sigma - z_consumer * se;
        if accept <= reject {
            return Err(Error::Unsupported(
                "the lower acceptance limit is not above the lower rejection limit; raise the \
                 subgroup size or separate the accepted and rejectable proportions"
                    .to_owned(),
            ));
        }
        limits.accept_low = Some(accept);
        limits.reject_low = Some(reject);
    }
    if let Some(high) = options.upper_spec {
        let accept = high - z_accept * sigma + z_producer * se;
        let reject = high - z_reject * sigma + z_consumer * se;
        if accept >= reject {
            return Err(Error::Unsupported(
                "the upper acceptance limit is not below the upper rejection limit; raise the \
                 subgroup size or separate the accepted and rejectable proportions"
                    .to_owned(),
            ));
        }
        limits.accept_high = Some(accept);
        limits.reject_high = Some(reject);
    }
    if let (Some(low), Some(high)) = (limits.accept_low, limits.accept_high)
        && low >= high
    {
        return Err(Error::Unsupported(
            "the lower and upper acceptance limits cross; widen the specification limits or \
             lower the accepted proportion"
                .to_owned(),
        ));
    }
    let decisions = means.iter().map(|mean| decide(*mean, &limits)).collect();
    Ok(AcceptanceChart {
        means,
        decisions,
        limits,
        sigma,
        subgroup: options.subgroup,
        n: sample.n(),
        excluded: sample.excluded(),
        dropped,
    })
}

/// Estimates sigma from the average subgroup range or moving range.
fn estimate_sigma(values: &[f64], subgroup: usize) -> Result<f64> {
    let (average, size) = if subgroup == 1 {
        let moving: Vec<f64> = values
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .collect();
        let average = moving.iter().sum::<f64>() / moving.len() as f64;
        (average, 2)
    } else {
        let used = values.len() / subgroup * subgroup;
        let ranges: Vec<f64> = values[..used]
            .chunks_exact(subgroup)
            .map(|group| {
                let max = group.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let min = group.iter().copied().fold(f64::INFINITY, f64::min);
                max - min
            })
            .collect();
        let average = ranges.iter().sum::<f64>() / ranges.len() as f64;
        (average, subgroup)
    };
    if average <= 0.0 {
        return Err(Error::ConstantSample);
    }
    Ok(average / d2(size)?)
}

/// Decision of one mean against the limits.
fn decide(mean: f64, limits: &AcceptanceLimits) -> Decision {
    if limits.accept_low.is_some_and(|limit| mean < limit)
        || limits.accept_high.is_some_and(|limit| mean > limit)
    {
        if limits.reject_low.is_some_and(|limit| mean < limit)
            || limits.reject_high.is_some_and(|limit| mean > limit)
        {
            Decision::Reject
        } else {
            Decision::Continue
        }
    } else {
        Decision::Accept
    }
}

/// Builds the Session report.
fn report(column: &str, chart: &AcceptanceChart, options: &AcceptanceOptions) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("Acceptance Chart — {column}"),
        "Acceptance limits from the specification, with producer and consumer risks",
    );
    result.n = chart.n;
    result.excluded = chart.excluded;
    result.statistics = vec![
        ("Sigma estimate", chart.sigma),
        ("Subgroup size", chart.subgroup as f64),
        ("Accepted proportion p1", options.acceptable),
        ("Rejectable proportion p2", options.rejectable),
        ("Producer risk (alpha)", options.producer),
        ("Consumer risk (beta)", options.consumer),
        ("Points", chart.means.len() as f64),
    ];
    if let Some(limit) = options.lower_spec {
        result.statistics.push(("Lower spec", limit));
    }
    if let Some(limit) = options.upper_spec {
        result.statistics.push(("Upper spec", limit));
    }
    if let Some(limit) = chart.limits.accept_low {
        result.statistics.push(("Acceptance LAL", limit));
    }
    if let Some(limit) = chart.limits.accept_high {
        result.statistics.push(("Acceptance UAL", limit));
    }
    if let Some(limit) = chart.limits.reject_low {
        result.statistics.push(("Rejection LRL", limit));
    }
    if let Some(limit) = chart.limits.reject_high {
        result.statistics.push(("Rejection URL", limit));
    }
    let accepted = chart
        .decisions
        .iter()
        .filter(|decision| **decision == Decision::Accept)
        .count();
    let continued = chart
        .decisions
        .iter()
        .filter(|decision| **decision == Decision::Continue)
        .count();
    let rejected = chart
        .decisions
        .iter()
        .filter(|decision| **decision == Decision::Reject)
        .count();
    result.statistics.push(("Accepted points", accepted as f64));
    result
        .statistics
        .push(("Continued points", continued as f64));
    result.statistics.push(("Rejected points", rejected as f64));

    let mut rows = Vec::with_capacity(chart.means.len());
    for (index, (mean, decision)) in chart.means.iter().zip(&chart.decisions).enumerate() {
        rows.push(vec![
            (index + 1).to_string(),
            format!("{mean:.6}"),
            decision.name().to_owned(),
        ]);
    }
    result.extra.push(Item::Table {
        headers: vec![
            "Point".to_owned(),
            "Subgroup mean".to_owned(),
            "Decision".to_owned(),
        ],
        rows,
    });
    result.notes = vec![
        (
            "Acceptance limits".to_owned(),
            "UAL = USL − z(1−p1)·σ + z(1−α)·σ/√n; LAL = LSL + z(1−p1)·σ − z(1−α)·σ/√n".to_owned(),
        ),
        (
            "Rejection limits".to_owned(),
            "URL = USL − z(1−p2)·σ + z(1−β)·σ/√n; LRL = LSL + z(1−p2)·σ − z(1−β)·σ/√n".to_owned(),
        ),
        (
            "Decision".to_owned(),
            "inside the acceptance limits accept, beyond a rejection limit reject, between them \
             continue sampling"
                .to_owned(),
        ),
        (
            "Sigma".to_owned(),
            match options.sigma {
                Some(_) => "the known process standard deviation supplied by the user".to_owned(),
                None if chart.subgroup == 1 => {
                    "estimated from the average moving range and d2(2)".to_owned()
                }
                None => "estimated from the average subgroup range and d2(n)".to_owned(),
            },
        ),
        (
            "Points".to_owned(),
            if chart.subgroup == 1 {
                "one point per observation, in worksheet order".to_owned()
            } else {
                "consecutive groups of the subgroup size, in worksheet order".to_owned()
            },
        ),
        (
            "Data policy".to_owned(),
            "missing values are excluded and counted".to_owned(),
        ),
    ];
    let mut warnings = standard_warnings(chart.n, chart.excluded, 0);
    if chart.dropped > 0 {
        warnings.push(format!(
            "{} trailing observation(s) did not fill a complete subgroup and were not used",
            chart.dropped
        ));
    }
    if options.sigma.is_none() {
        warnings.push(
            "sigma is estimated from the data; a known sigma gives narrower, stable limits"
                .to_owned(),
        );
    }
    result.warnings = warnings;
    result
}

/// Builds the plot: means, specification lines, acceptance and rejection
/// limits, and the continue zones as grey bands.
fn plot(column: &str, chart: &AcceptanceChart) -> PlotSpec {
    let xs: Vec<f64> = (1..=chart.means.len()).map(|index| index as f64).collect();
    let x_min = 1.0;
    let x_max = chart.means.len().max(1) as f64;
    let mut y_min = chart.means.iter().copied().fold(f64::INFINITY, f64::min);
    let mut y_max = chart
        .means
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    for value in [
        chart.limits.accept_low,
        chart.limits.accept_high,
        chart.limits.reject_low,
        chart.limits.reject_high,
    ]
    .into_iter()
    .flatten()
    {
        y_min = y_min.min(value);
        y_max = y_max.max(value);
    }
    let pad = ((y_max - y_min).abs() * 0.08).max(1e-6);
    y_min -= pad;
    y_max += pad;

    let mut lines = Vec::new();
    let mut band = Vec::new();
    let mut push_line = |value: f64, width: f64| {
        lines.push(Segment {
            from: Point { x: x_min, y: value },
            to: Point { x: x_max, y: value },
            width,
        });
    };
    if let Some(limit) = chart.limits.accept_low {
        push_line(limit, 1.6);
    }
    if let Some(limit) = chart.limits.accept_high {
        push_line(limit, 1.6);
    }
    if let Some(limit) = chart.limits.reject_low {
        push_line(limit, 1.6);
    }
    if let Some(limit) = chart.limits.reject_high {
        push_line(limit, 1.6);
    }
    if let (Some(low_accept), Some(low_reject)) = (chart.limits.accept_low, chart.limits.reject_low)
    {
        band.push(Band {
            lower: vec![
                Point {
                    x: x_min,
                    y: low_reject,
                },
                Point {
                    x: x_max,
                    y: low_reject,
                },
            ],
            upper: vec![
                Point {
                    x: x_min,
                    y: low_accept,
                },
                Point {
                    x: x_max,
                    y: low_accept,
                },
            ],
        });
    }
    if let (Some(high_accept), Some(high_reject)) =
        (chart.limits.accept_high, chart.limits.reject_high)
    {
        band.push(Band {
            lower: vec![
                Point {
                    x: x_min,
                    y: high_accept,
                },
                Point {
                    x: x_max,
                    y: high_accept,
                },
            ],
            upper: vec![
                Point {
                    x: x_min,
                    y: high_reject,
                },
                Point {
                    x: x_max,
                    y: high_reject,
                },
            ],
        });
    }

    let mut note_rows = vec![
        ("Sigma", format!("{:.4}", chart.sigma)),
        ("Subgroup", chart.subgroup.to_string()),
    ];
    for (label, value) in [
        ("Acceptance LAL", chart.limits.accept_low),
        ("Acceptance UAL", chart.limits.accept_high),
        ("Rejection LRL", chart.limits.reject_low),
        ("Rejection URL", chart.limits.reject_high),
    ] {
        if let Some(value) = value {
            note_rows.push((label, format!("{value:.4}")));
        }
    }

    PlotSpec {
        title: format!("Acceptance Chart — {column}"),
        x_axis: linear_axis(
            if chart.subgroup == 1 {
                "Observation"
            } else {
                "Subgroup"
            },
            x_min - 0.5,
            x_max + 0.5,
        ),
        y_axis: linear_axis("Value", y_min, y_max),
        size: (900, 460),
        points: chart
            .means
            .iter()
            .zip(&xs)
            .map(|(mean, x)| Point { x: *x, y: *mean })
            .collect(),
        lines,
        bands: band,
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: vec![PlotNote {
            rows: note_rows
                .into_iter()
                .map(|(label, value)| crate::plot::NoteRow {
                    label: label.to_owned(),
                    value,
                })
                .collect(),
            lines: vec![
                "Grey: continue sampling".to_owned(),
                "Lines: acceptance and rejection limits".to_owned(),
            ],
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame(values: Vec<f64>) -> DataFrame {
        DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame")
    }

    fn options() -> AcceptanceOptions {
        AcceptanceOptions {
            lower_spec: Some(0.0),
            upper_spec: Some(10.0),
            sigma: Some(1.0),
            ..AcceptanceOptions::default()
        }
    }

    fn statistics(output: &AnalysisOutput) -> Vec<(String, f64)> {
        output.results[0]
            .statistics
            .iter()
            .map(|(name, value)| ((*name).to_owned(), *value))
            .collect()
    }

    fn value(output: &AnalysisOutput, name: &str) -> f64 {
        statistics(output)
            .into_iter()
            .find(|(label, _)| label == name)
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("missing statistic {name}"))
    }

    #[test]
    fn the_limits_follow_the_published_formulas() {
        let df = frame(vec![5.0, 9.0, 9.8, 0.1]);
        let output = analyze(&df, "x", &options()).expect("chart");
        // z(1-0.001)=3.090232, z(1-0.05)=1.644854, z(1-0.05)=1.644854,
        // z(1-0.10)=1.281552 with sigma = 1 and n = 1.
        assert!((value(&output, "Acceptance UAL") - 8.554_622).abs() < 1e-6);
        assert!((value(&output, "Rejection URL") - 9.636_698).abs() < 1e-6);
        assert!((value(&output, "Acceptance LAL") - 1.445_378).abs() < 1e-6);
        assert!((value(&output, "Rejection LRL") - 0.363_302).abs() < 1e-6);
        let decisions: Vec<&str> = output.results[0]
            .extra
            .iter()
            .find_map(|item| match item {
                Item::Table { rows, .. } => Some(rows.iter().map(|row| row[2].as_str()).collect()),
                _ => None,
            })
            .expect("table");
        assert_eq!(decisions, vec!["Accept", "Continue", "Reject", "Reject"]);
        assert_eq!(value(&output, "Accepted points"), 1.0);
        assert_eq!(value(&output, "Continued points"), 1.0);
        assert_eq!(value(&output, "Rejected points"), 2.0);
    }

    #[test]
    fn one_sided_specifications_keep_only_their_limits() {
        let df = frame(vec![4.0, 4.0, 4.0, 4.0]);
        let mut request = AcceptanceOptions {
            lower_spec: None,
            upper_spec: Some(5.0),
            sigma: Some(0.5),
            ..AcceptanceOptions::default()
        };
        let output = analyze(&df, "x", &request).expect("upper only");
        let names: Vec<&str> = output.results[0]
            .statistics
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert!(names.contains(&"Acceptance UAL"));
        assert!(!names.contains(&"Acceptance LAL"));

        request.lower_spec = Some(1.0);
        request.upper_spec = None;
        let output = analyze(&df, "x", &request).expect("lower only");
        let names: Vec<&str> = output.results[0]
            .statistics
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert!(names.contains(&"Acceptance LAL"));
        assert!(!names.contains(&"Acceptance UAL"));
    }

    #[test]
    fn sigma_is_estimated_from_the_moving_range() {
        let df = frame(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let request = AcceptanceOptions {
            lower_spec: Some(0.0),
            upper_spec: Some(10.0),
            sigma: None,
            ..AcceptanceOptions::default()
        };
        let output = analyze(&df, "x", &request).expect("chart");
        // Average moving range 1 over d2(2) = 1.128 gives 0.886525.
        assert!((value(&output, "Sigma estimate") - 0.886_525).abs() < 1e-6);
        assert!(
            output.results[0]
                .warnings
                .iter()
                .any(|warning| warning.contains("sigma is estimated"))
        );
    }

    #[test]
    fn subgroups_use_the_subgroup_means_and_the_range_estimate() {
        let df = frame(vec![1.0, 3.0, 2.0, 4.0, 3.0, 5.0, 7.0]);
        let request = AcceptanceOptions {
            lower_spec: Some(0.0),
            upper_spec: Some(10.0),
            subgroup: 2,
            sigma: None,
            ..AcceptanceOptions::default()
        };
        let output = analyze(&df, "x", &request).expect("chart");
        assert_eq!(value(&output, "Points"), 3.0);
        assert_eq!(value(&output, "Subgroup size"), 2.0);
        // The ranges are 2, 2, 2: sigma = 2 / d2(2) = 1.773050.
        assert!((value(&output, "Sigma estimate") - 1.773_050).abs() < 1e-6);
        assert!(
            output.results[0]
                .warnings
                .iter()
                .any(|warning| warning.contains("trailing observation"))
        );
        let means: Vec<&str> = output.results[0]
            .extra
            .iter()
            .find_map(|item| match item {
                Item::Table { rows, .. } => Some(rows.iter().map(|row| row[1].as_str()).collect()),
                _ => None,
            })
            .expect("table");
        assert_eq!(means, vec!["2.000000", "3.000000", "4.000000"]);
    }

    #[test]
    fn invalid_options_and_crossing_limits_are_rejected() {
        let df = frame(vec![1.0, 2.0, 3.0]);
        assert!(analyze(&df, "x", &AcceptanceOptions::default()).is_err());

        let mut request = options();
        request.acceptable = 0.05;
        request.rejectable = 0.01;
        assert!(analyze(&df, "x", &request).is_err());

        let mut request = options();
        request.sigma = Some(0.0);
        assert!(analyze(&df, "x", &request).is_err());

        let mut request = options();
        request.subgroup = 26;
        assert!(analyze(&df, "x", &request).is_err());

        let mut request = options();
        request.lower_spec = Some(9.0);
        request.upper_spec = Some(1.0);
        assert!(analyze(&df, "x", &request).is_err());

        let mut request = options();
        request.acceptable = 0.49;
        request.rejectable = 0.499;
        // Very close proportions: the acceptance and rejection limits cross
        // with a subgroup of one.
        request.sigma = Some(10.0);
        assert!(analyze(&df, "x", &request).is_err());
    }

    #[test]
    fn a_constant_sample_and_a_short_sample_are_rejected() {
        let constant = frame(vec![5.0, 5.0, 5.0]);
        let request = AcceptanceOptions {
            lower_spec: Some(0.0),
            upper_spec: Some(10.0),
            sigma: None,
            ..AcceptanceOptions::default()
        };
        assert!(matches!(
            analyze(&constant, "x", &request),
            Err(Error::ConstantSample)
        ));
        let short = frame(vec![5.0]);
        assert!(matches!(
            analyze(&short, "x", &request),
            Err(Error::SampleTooSmall { .. })
        ));
    }
}
