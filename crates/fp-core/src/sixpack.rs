// SPDX-License-Identifier: MIT
//! Capability Sixpack: six panels over one normal capability study.
//!
//! The Sixpack adds no new estimator. It arranges the published pieces already
//! used by the capability analysis and the Shewhart charts: a histogram with
//! the specification limits, a normal probability plot, the control chart of
//! the individuals (or of the subgroup means and ranges), the last 25 points
//! and the capability plot with the indices.
//!
//! Layout, three rows by two columns:
//!
//! | left | right |
//! |---|---|
//! | histogram | normal probability plot |
//! | individuals (or subgroup mean) | moving range (or range / standard deviation) |
//! | last 25 | capability |

use crate::data::{Column, DataFrame, MissingPolicy, Sample};
use crate::error::{Error, Result};
use crate::plot::{PlotNote, PlotSpec, panel_plot};
use crate::spc::{self, ControlChartKind, TestSet};
use crate::workflow::{
    ProbabilityPlotOptions, capability_sample, normal_probability, sigma_estimates,
};

/// Builds the six-panel plot of a normal capability study.
///
/// The caller has already validated the specification limits and the subgroup
/// configuration through the capability analysis; this function recomputes the
/// spread only to label the capability panel.
///
/// # Errors
///
/// Returns [`Error::SampleTooSmall`] for fewer than two observations,
/// [`Error::ConstantSample`] when the spread is zero, and
/// [`Error::Unsupported`] when the control chart needs subgroups outside 2–25
/// observations.
pub(crate) fn sixpack_plot(
    df: &DataFrame,
    column: &str,
    lsl: Option<f64>,
    usl: Option<f64>,
    subgroup: usize,
    subgroup_column: Option<&str>,
) -> Result<PlotSpec> {
    let sample = capability_sample(df, column, subgroup_column)?;
    let estimates = sigma_estimates(
        &sample,
        df,
        column,
        subgroup,
        subgroup_column,
        true,
        false,
        None,
    )?;
    let mean = sample.mean()?;

    let chart = if let Some(label) = subgroup_column {
        let groups = spc::group_samples(df, column, label)?;
        let largest = groups.iter().map(Sample::n).max().unwrap_or(0);
        let kind = if largest <= 8 {
            ControlChartKind::XbarR
        } else {
            ControlChartKind::XbarS
        };
        spc::variable_subgroup_chart(&groups, kind, TestSet::default())?
    } else if subgroup == 1 {
        spc::individuals_moving_range(&sample)?
    } else {
        if !(2..=25).contains(&subgroup) {
            return Err(Error::Unsupported(
                "the Sixpack control charts need subgroups of 2 to 25 observations".to_owned(),
            ));
        }
        if sample.n() % subgroup != 0 {
            return Err(Error::Unsupported(
                "the sample size must be divisible by the subgroup size".to_owned(),
            ));
        }
        let mut groups = Vec::new();
        for chunk in sample.values().chunks(subgroup) {
            groups.push(Sample::new(chunk.to_vec(), MissingPolicy::Drop)?);
        }
        let kind = if subgroup <= 8 {
            ControlChartKind::XbarR
        } else {
            ControlChartKind::XbarS
        };
        spc::variable_subgroup_chart(&groups, kind, TestSet::default())?
    };

    // The three charts share the control limits of the whole study; the last
    // panel keeps the per-point limits when the subgroups have different sizes.
    let chart_plot = spc::plot(column, &chart);
    let mut chart_panels = chart_plot.panels;
    let dispersion = chart_panels.pop().ok_or_else(|| {
        Error::Unsupported("the Sixpack control chart has no dispersion panel".to_owned())
    })?;
    let location = chart_panels.pop().ok_or_else(|| {
        Error::Unsupported("the Sixpack control chart has no location panel".to_owned())
    })?;
    let total = chart.location.len();
    let start = total.saturating_sub(25);
    let steps = if chart.location_steps.len() == total {
        chart.location_steps[start..].to_vec()
    } else {
        Vec::new()
    };
    let positions: Vec<usize> = (start + 1..=total).collect();
    let individuals = chart.kind == ControlChartKind::IndividualsMovingRange;
    let last = spc::control_panel(
        &chart.location[start..],
        start + 1,
        &positions,
        chart.location_limits,
        &steps,
        if individuals {
            "Last 25 observations"
        } else {
            "Last 25 subgroups"
        },
        if individuals {
            "Observation"
        } else {
            "Subgroup"
        },
        "Value",
    );

    // The plots read the same rows as the analysis, so a row dropped for a
    // missing measurement or label never reappears in a panel.
    let panel_frame = DataFrame::new(vec![Column::new_f64(column, sample.values().to_vec())])?;
    let mut histogram = crate::histogram::capability_plot(&panel_frame, column, lsl, usl, false)?;
    histogram.title = "Histogram".to_owned();
    let mut probability =
        normal_probability(&panel_frame, column, ProbabilityPlotOptions::default())?.plot;
    probability.title = "Normal probability plot".to_owned();
    let mut capability = crate::histogram::capability_plot(&panel_frame, column, lsl, usl, true)?;
    capability.title = "Capability".to_owned();
    capability.notes = vec![PlotNote::new(vec![
        ("Cp", format_index(cp(lsl, usl, estimates.within))),
        ("Cpk", format_index(cpk(lsl, usl, mean, estimates.within))),
        ("Pp", format_index(cp(lsl, usl, estimates.overall))),
        ("Ppk", format_index(cpk(lsl, usl, mean, estimates.overall))),
    ])];

    panel_plot(
        vec![
            histogram,
            probability,
            location,
            dispersion,
            last,
            capability,
        ],
        2,
        format!("Capability Sixpack — {column}"),
    )
    .ok_or_else(|| Error::Unsupported("the capability Sixpack could not be assembled".to_owned()))
}

fn cp(lsl: Option<f64>, usl: Option<f64>, sigma: f64) -> Option<f64> {
    match (lsl, usl) {
        (Some(lsl), Some(usl)) => Some((usl - lsl) / (6.0 * sigma)),
        _ => None,
    }
}

fn cpk(lsl: Option<f64>, usl: Option<f64>, mean: f64, sigma: f64) -> Option<f64> {
    let lower = lsl.map(|limit| (mean - limit) / (3.0 * sigma));
    let upper = usl.map(|limit| (limit - mean) / (3.0 * sigma));
    match (lower, upper) {
        (Some(lower), Some(upper)) => Some(lower.min(upper)),
        (Some(lower), None) => Some(lower),
        (None, Some(upper)) => Some(upper),
        (None, None) => None,
    }
}

fn format_index(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.2}"))
}
