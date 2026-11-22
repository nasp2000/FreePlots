// SPDX-License-Identifier: MIT
//! Log-rank (Mantel–Cox) test of two or more survival curves.
//!
//! Every observation is a time and a right-censoring flag; the observations
//! split into groups by a label column. At every distinct event time `t` the
//! test compares the events observed in each group with the events expected
//! when the groups share one hazard:
//!
//! ```text
//! O_j = Σ_t d_tj,      E_j = Σ_t d_t·n_tj/n_t
//! V_jl = Σ_t d_t(n_t − d_t)/(n_t²(n_t − 1)) · (n_tj(n_t − n_tj) if j = l, −n_tj·n_tl otherwise)
//! ```
//!
//! and the statistic `(O − E)ᵀ·V⁻¹·(O − E)` over the first `k − 1` groups
//! follows a chi-square with `k − 1` degrees of freedom when the curves are
//! equal. The report also carries the Kaplan–Meier curve of each group, with
//! its median when the curve reaches one half.
//!
//! The documented behaviour compares survival curves; its risk-set and tie
//! conventions were not documented, so the numbers are not claimed to match the
//! documented behaviour — see the project notes.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, Series, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::chi_square_cdf;
use crate::workflow::AnalysisOutput;

/// Kaplan–Meier curve and counts of one group.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupCurve {
    /// Group label.
    pub label: String,
    /// Observations in the group.
    pub n: usize,
    /// Events observed.
    pub events: usize,
    /// Right-censored observations.
    pub censored: usize,
    /// Median survival time, when the curve reaches one half.
    pub median: Option<f64>,
    /// `(time, survival)` after every event time, starting at `(0, 1)`.
    pub curve: Vec<(f64, f64)>,
}

/// Fitted log-rank comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct LogRank {
    /// Time column.
    pub time: String,
    /// Group column.
    pub group: String,
    /// One curve per group, in order of first appearance.
    pub groups: Vec<GroupCurve>,
    /// Events observed in each group.
    pub observed: Vec<f64>,
    /// Events expected in each group under equal hazards.
    pub expected: Vec<f64>,
    /// The chi-square statistic.
    pub statistic: f64,
    /// Degrees of freedom (`groups − 1`).
    pub degrees: usize,
    /// p-value of the statistic.
    pub p_value: f64,
    /// Observations used.
    pub n: usize,
    /// Events observed.
    pub events: usize,
    /// Right-censored observations.
    pub censored: usize,
    /// Rows excluded.
    pub excluded: usize,
}

/// Gauss–Jordan inverse with partial pivoting.
fn invert(matrix: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let size = matrix.len();
    let mut a = matrix.to_vec();
    let mut inverse = vec![vec![0.0; size]; size];
    for (index, row) in inverse.iter_mut().enumerate() {
        row[index] = 1.0;
    }
    for column in 0..size {
        let mut pivot = column;
        for row in (column + 1)..size {
            if a[row][column].abs() > a[pivot][column].abs() {
                pivot = row;
            }
        }
        if a[pivot][column].abs() < 1e-12 {
            return None;
        }
        a.swap(column, pivot);
        inverse.swap(column, pivot);
        let scale = a[column][column];
        for value in a[column].iter_mut() {
            *value /= scale;
        }
        for value in inverse[column].iter_mut() {
            *value /= scale;
        }
        for row in 0..size {
            if row == column {
                continue;
            }
            let factor = a[row][column];
            for index in 0..size {
                a[row][index] -= factor * a[column][index];
                inverse[row][index] -= factor * inverse[column][index];
            }
        }
    }
    Some(inverse)
}

/// Kaplan–Meier curve of one group of `(time, event)` observations.
fn kaplan_meier(observations: &mut [(f64, bool)]) -> (Vec<(f64, f64)>, Option<f64>) {
    observations.sort_by(|left, right| left.0.total_cmp(&right.0));
    let n = observations.len();
    let mut curve = vec![(0.0, 1.0)];
    let mut survival = 1.0;
    let mut median = None;
    let mut index = 0;
    while index < n {
        let time = observations[index].0;
        let at_risk = n - index;
        let mut events = 0usize;
        while index < n && observations[index].0 == time {
            if !observations[index].1 {
                events += 1;
            }
            index += 1;
        }
        if events > 0 {
            survival *= 1.0 - events as f64 / at_risk as f64;
            survival = survival.clamp(0.0, 1.0);
            if median.is_none() && survival <= 0.5 {
                median = Some(time);
            }
        }
        curve.push((time, survival));
    }
    (curve, median)
}

/// Runs the log-rank test of the groups of `group`.
///
/// # Errors
///
/// Returns [`Error::EmptySample`] when no row is complete,
/// [`Error::Unsupported`] for fewer than two groups, a group without events or
/// risk sets that do not identify every group, and [`Error::Cell`] for a
/// non-positive time.
#[allow(clippy::needless_range_loop)]
pub fn test(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
    group_column: &str,
) -> Result<LogRank> {
    if !censor_value.is_finite() {
        return Err(Error::InvalidParameter {
            name: "censoring value",
            value: censor_value,
        });
    }
    let time = df.column(time_column)?;
    let group = df.column(group_column)?;
    let censor = match censor_column {
        Some(name) => Some(df.column(name)?),
        None => None,
    };
    let mut labels: Vec<String> = Vec::new();
    let mut observations: Vec<Vec<(f64, bool)>> = Vec::new();
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        let Some(value) = time.value(row) else {
            excluded += 1;
            continue;
        };
        if value <= 0.0 {
            return Err(Error::Cell {
                column: time_column.to_owned(),
                row: row + 1,
                message: "survival times must be positive".to_owned(),
            });
        }
        let censored = match &censor {
            Some(column) => match column.value(row) {
                Some(flag) => (flag - censor_value).abs() <= f64::EPSILON,
                None => {
                    excluded += 1;
                    continue;
                }
            },
            None => false,
        };
        let Some(label) = group.label(row) else {
            excluded += 1;
            continue;
        };
        let index = match labels.iter().position(|existing| *existing == label) {
            Some(position) => position,
            None => {
                labels.push(label);
                observations.push(Vec::new());
                labels.len() - 1
            }
        };
        observations[index].push((value, censored));
    }
    let n: usize = observations.iter().map(Vec::len).sum();
    if n == 0 {
        return Err(Error::EmptySample);
    }
    let k = labels.len();
    if k < 2 {
        return Err(Error::Unsupported(
            "the log-rank test needs at least two groups".to_owned(),
        ));
    }
    let mut groups = Vec::with_capacity(k);
    let mut events_total = 0usize;
    for (index, label) in labels.iter().enumerate() {
        let entries = &mut observations[index];
        let events = entries.iter().filter(|(_, censored)| !censored).count();
        events_total += events;
        if entries.is_empty() {
            return Err(Error::Unsupported(format!(
                "group {label} has no complete observation"
            )));
        }
        if events == 0 {
            return Err(Error::Unsupported(format!(
                "group {label} has no event; the curves cannot be compared"
            )));
        }
        let (curve, median) = kaplan_meier(entries);
        groups.push(GroupCurve {
            label: label.clone(),
            n: entries.len(),
            events,
            censored: entries.len() - events,
            median,
            curve,
        });
    }

    // The Mantel–Cox statistic over the pooled event times.
    let mut times: Vec<f64> = observations
        .iter()
        .flat_map(|entries| entries.iter().filter(|(_, censored)| !*censored))
        .map(|(value, _)| *value)
        .collect();
    times.sort_by(f64::total_cmp);
    times.dedup();
    let mut observed = vec![0.0f64; k];
    let mut expected = vec![0.0f64; k];
    let mut covariance = vec![vec![0.0f64; k]; k];
    for time in times {
        let at_risk: Vec<usize> = observations
            .iter()
            .map(|entries| entries.iter().filter(|(value, _)| *value >= time).count())
            .collect();
        let deaths: Vec<usize> = observations
            .iter()
            .map(|entries| {
                entries
                    .iter()
                    .filter(|(value, censored)| *value == time && !censored)
                    .count()
            })
            .collect();
        let total_risk: usize = at_risk.iter().sum();
        let total_deaths: usize = deaths.iter().sum();
        if total_deaths == 0 {
            continue;
        }
        for group in 0..k {
            observed[group] += deaths[group] as f64;
            expected[group] += total_deaths as f64 * at_risk[group] as f64 / total_risk as f64;
        }
        // A risk set of one has no variance to contribute; its observed and
        // expected counts cancel in the differences that drive the statistic.
        if total_risk < 2 {
            continue;
        }
        let factor = total_deaths as f64 * (total_risk - total_deaths) as f64
            / (total_risk as f64 * total_risk as f64 * (total_risk - 1) as f64);
        for left in 0..k {
            for right in 0..k {
                let value = if left == right {
                    at_risk[left] as f64 * (total_risk - at_risk[left]) as f64
                } else {
                    -(at_risk[left] as f64 * at_risk[right] as f64)
                };
                covariance[left][right] += factor * value;
            }
        }
    }

    let degrees = k - 1;
    let difference: Vec<f64> = (0..degrees)
        .map(|group| observed[group] - expected[group])
        .collect();
    let submatrix: Vec<Vec<f64>> = (0..degrees)
        .map(|row| covariance[row][..degrees].to_vec())
        .collect();
    let inverse = invert(&submatrix).ok_or_else(|| {
        Error::Unsupported(
            "the risk sets do not identify every group; the covariance matrix is singular"
                .to_owned(),
        )
    })?;
    let mut statistic = 0.0;
    for left in 0..degrees {
        for right in 0..degrees {
            statistic += difference[left] * inverse[left][right] * difference[right];
        }
    }
    let statistic = statistic.max(0.0);
    let p_value = (1.0 - chi_square_cdf(statistic, degrees as f64)).clamp(0.0, 1.0);

    Ok(LogRank {
        time: time_column.to_owned(),
        group: group_column.to_owned(),
        groups,
        observed,
        expected,
        statistic,
        degrees,
        p_value,
        n,
        events: events_total,
        censored: n - events_total,
        excluded,
    })
}

/// Kaplan–Meier curves of every group.
fn plot(model: &LogRank, time_label: &str) -> PlotSpec {
    let last = model
        .groups
        .iter()
        .flat_map(|group| group.curve.iter().map(|(time, _)| *time))
        .fold(0.0, f64::max);
    let x_max = (last * 1.05).max(1.0);
    let series = model
        .groups
        .iter()
        .map(|group| {
            let mut points: Vec<Point> = group
                .curve
                .iter()
                .map(|(time, survival)| Point {
                    x: *time,
                    y: *survival,
                })
                .collect();
            points.push(Point {
                x: x_max,
                y: group.curve.last().map_or(1.0, |(_, survival)| *survival),
            });
            let mut lines = Vec::new();
            for pair in points.windows(2) {
                lines.push(Segment {
                    from: pair[0],
                    to: pair[1],
                    width: 2.0,
                });
            }
            Series {
                label: group.label.clone(),
                points,
                lines,
                bands: Vec::new(),
                secondary_axis: false,
            }
        })
        .collect();
    PlotSpec {
        title: "Log-Rank Test".to_owned(),
        x_axis: linear_axis(time_label, 0.0, x_max),
        y_axis: linear_axis("Survival", 0.0, 1.0),
        size: (900, 560),
        points: Vec::new(),
        lines: Vec::new(),
        bands: Vec::new(),
        series,
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Runs the test and builds the report and the group curves.
pub fn analyze(
    df: &DataFrame,
    time_column: &str,
    censor_column: Option<&str>,
    censor_value: f64,
    group_column: &str,
) -> Result<AnalysisOutput> {
    let model = test(df, time_column, censor_column, censor_value, group_column)?;
    let mut result = AnalysisResult::new(
        format!("Log-Rank Test — time {time_column} by {group_column}"),
        format!(
            "{}-sample log-rank (Mantel–Cox) with the Kaplan–Meier curves of the groups",
            model.groups.len()
        ),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Groups", model.groups.len() as f64),
        ("Observations", model.n as f64),
        ("Events", model.events as f64),
        ("Censored", model.censored as f64),
        ("Chi-square", model.statistic),
        ("Degrees of freedom", model.degrees as f64),
        ("p-value", model.p_value),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Group".to_owned(),
            "n".to_owned(),
            "Events".to_owned(),
            "Censored".to_owned(),
            "Median".to_owned(),
        ],
        rows: model
            .groups
            .iter()
            .map(|group| {
                vec![
                    group.label.clone(),
                    group.n.to_string(),
                    group.events.to_string(),
                    group.censored.to_string(),
                    group
                        .median
                        .map_or_else(|| "—".to_owned(), |value| format!("{value:.6}")),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Group".to_owned(),
            "Observed".to_owned(),
            "Expected".to_owned(),
            "Observed − expected".to_owned(),
        ],
        rows: model
            .groups
            .iter()
            .enumerate()
            .map(|(group, curve)| {
                vec![
                    curve.label.clone(),
                    format!("{:.6}", model.observed[group]),
                    format!("{:.6}", model.expected[group]),
                    format!("{:.6}", model.observed[group] - model.expected[group]),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Method".to_owned(),
            "at every distinct event time the events observed in a group are compared with \
             the events expected when all the groups share one hazard; the differences are \
             added and scaled by their covariance"
                .to_owned(),
        ),
        (
            "Statistic".to_owned(),
            format!(
                "(O − E)ᵀ·V⁻¹·(O − E) over the first {} groups, chi-square with {} degrees of \
                 freedom",
                model.degrees, model.degrees
            ),
        ),
        (
            "Censoring".to_owned(),
            format!(
                "an observation is censored when its flag equals {censor_value}; the censored \
                 observations stay in the risk set up to their time"
            ),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing time, group or flag are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the test compares the whole curves and does not adjust for covariates; use the Cox \
         regression for that"
            .to_owned(),
    );
    let plot = plot(&model, time_column);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame(groups: [(&str, Vec<(f64, bool)>); 2]) -> DataFrame {
        let mut time = Vec::new();
        let mut event = Vec::new();
        let mut label = Vec::new();
        for (name, entries) in groups {
            for (value, observed) in entries {
                time.push(value);
                event.push(if observed { 1.0 } else { 0.0 });
                label.push(name.to_owned());
            }
        }
        DataFrame::new(vec![
            Column::new_f64("time", time),
            Column::new_f64("event", event),
            Column::new_text("group", label),
        ])
        .expect("frame")
    }

    #[test]
    fn the_two_group_statistic_matches_the_hand_calculation() {
        // A: 1, 2, 3, 4, 5 and B: 2, 4, 6, 8, 10, all events.
        let df = frame([
            (
                "A",
                vec![
                    (1.0, true),
                    (2.0, true),
                    (3.0, true),
                    (4.0, true),
                    (5.0, true),
                ],
            ),
            (
                "B",
                vec![
                    (2.0, true),
                    (4.0, true),
                    (6.0, true),
                    (8.0, true),
                    (10.0, true),
                ],
            ),
        ]);
        let model = test(&df, "time", Some("event"), 0.0, "group").expect("test");
        assert_eq!(model.groups.len(), 2);
        assert_eq!(model.n, 10);
        assert_eq!(model.events, 10);
        assert_eq!(model.degrees, 1);
        assert!((model.observed[0] - 5.0).abs() < 1e-12);
        assert!((model.expected[0] - 2.734_126_984_126_984).abs() < 1e-12);
        assert!((model.statistic - 3.492_515_601_869_010_7).abs() < 1e-9);
        assert!((model.p_value - 0.061_646_840_759_586_15).abs() < 1e-9);
        // The Kaplan–Meier curve starts at one and falls at every event.
        assert!((model.groups[0].curve[0].1 - 1.0).abs() < 1e-12);
        assert!(model.groups[0].median.is_some());
    }

    #[test]
    fn three_groups_match_the_hand_calculation() {
        let df = frame([
            (
                "A",
                vec![
                    (1.0, true),
                    (2.0, true),
                    (3.0, true),
                    (4.0, true),
                    (5.0, true),
                ],
            ),
            (
                "B",
                vec![
                    (2.0, true),
                    (4.0, true),
                    (6.0, true),
                    (8.0, true),
                    (10.0, true),
                ],
            ),
        ]);
        let mut time: Vec<f64> = df.column("time").expect("time").numeric_by_row();
        let mut event: Vec<f64> = df.column("event").expect("event").numeric_by_row();
        let mut label: Vec<String> = (0..df.n_rows())
            .map(|row| {
                df.column("group")
                    .expect("group")
                    .label(row)
                    .unwrap_or_default()
            })
            .collect();
        for value in [3.0, 7.0, 9.0, 11.0, 12.0] {
            time.push(value);
            event.push(1.0);
            label.push("C".to_owned());
        }
        let df = DataFrame::new(vec![
            Column::new_f64("time", time),
            Column::new_f64("event", event),
            Column::new_text("group", label),
        ])
        .expect("frame");
        let model = test(&df, "time", Some("event"), 0.0, "group").expect("test");
        assert_eq!(model.degrees, 2);
        assert!((model.statistic - 8.409_365_041_474_363).abs() < 1e-9);
        assert!((model.p_value - 0.014_925_523_861_265_319).abs() < 1e-9);
    }

    #[test]
    fn censored_observations_stay_in_the_risk_set() {
        // The censoring changes the expected counts and the statistic.
        let df = frame([
            (
                "A",
                vec![
                    (1.0, true),
                    (2.0, false),
                    (3.0, true),
                    (5.0, true),
                    (8.0, true),
                    (10.0, false),
                ],
            ),
            (
                "B",
                vec![
                    (2.0, true),
                    (4.0, true),
                    (4.0, false),
                    (7.0, true),
                    (9.0, true),
                    (12.0, true),
                ],
            ),
        ]);
        let model = test(&df, "time", Some("event"), 0.0, "group").expect("test");
        assert_eq!(model.censored, 3);
        assert!((model.expected[0] - 3.507_323_232_323_232).abs() < 1e-12);
        assert!((model.statistic - 0.125_025_656_956_870_98).abs() < 1e-9);
        assert!((model.p_value - 0.723_646_414_664_824_5).abs() < 1e-9);
    }

    #[test]
    fn the_report_and_the_curves_are_built() {
        let df = frame([
            ("A", vec![(1.0, true), (2.0, true), (3.0, false)]),
            ("B", vec![(2.0, true), (4.0, true), (6.0, true)]),
        ]);
        let out = analyze(&df, "time", Some("event"), 0.0, "group").expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Log-Rank Test"), "{text}");
        assert!(text.contains("Chi-square"), "{text}");
        assert!(text.contains("Observed"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.title, "Log-Rank Test");
        assert_eq!(plot.series.len(), 2);
    }

    #[test]
    fn invalid_log_rank_settings_are_rejected() {
        let one = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0]),
            Column::new_f64("event", vec![1.0, 1.0, 0.0]),
            Column::new_text("group", vec!["A".to_owned(); 3]),
        ])
        .expect("frame");
        assert!(matches!(
            test(&one, "time", Some("event"), 0.0, "group"),
            Err(Error::Unsupported(_))
        ));
        let no_events = frame([
            ("A", vec![(1.0, false), (2.0, false)]),
            ("B", vec![(1.0, true), (2.0, true)]),
        ]);
        assert!(matches!(
            test(&no_events, "time", Some("event"), 0.0, "group"),
            Err(Error::Unsupported(_))
        ));
        let negative = frame([("A", vec![(-1.0, true)]), ("B", vec![(1.0, true)])]);
        assert!(matches!(
            test(&negative, "time", Some("event"), 0.0, "group"),
            Err(Error::Cell { .. })
        ));
    }
}
