// SPDX-License-Identifier: MIT
//! Run chart with the two classical runs tests.
//!
//! A run chart plots observations in row order around the sample median. The
//! report adds the runs-about-the-median test and the runs-up-and-down test.
//! The exact test set and tie convention used by the documented behaviour have
//! not been verified, so every report says so explicitly.

use crate::data::{DataFrame, MissingPolicy, Sample};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::normal_sf;
use crate::workflow::AnalysisOutput;

/// Result of a run-chart analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct RunChart {
    /// Observations in row order.
    pub values: Vec<f64>,
    /// Sample median, used as the center line.
    pub median: f64,
    /// Number of observations above the median.
    pub above: usize,
    /// Number of observations below the median.
    pub below: usize,
    /// Observations exactly equal to the median (excluded from the runs test).
    pub ties: usize,
    /// Runs about the median.
    pub runs: usize,
    /// Expected number of runs about the median.
    pub runs_expected: f64,
    /// Longest run above or below the median.
    pub longest_run: usize,
    /// Two-sided normal-approximation p-value for the runs test.
    pub runs_p: Option<f64>,
    /// Number of runs up and down.
    pub updown_runs: usize,
    /// Expected number of runs up and down.
    pub updown_expected: f64,
    /// Two-sided normal-approximation p-value for the up/down test.
    pub updown_p: Option<f64>,
    /// Non-finite observations excluded.
    pub excluded: usize,
}

impl RunChart {
    /// Number of observations used.
    #[must_use]
    pub fn n(&self) -> usize {
        self.values.len()
    }
}

/// Points used by the Mann–Kendall statistic on long series.
const TREND_POINTS: usize = 5000;
/// Pairwise slopes kept for Theil–Sen on long series.
const TREND_SLOPES: usize = 500_000;

/// Outcome of the Mann–Kendall trend test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trend {
    /// The Mann–Kendall statistic.
    pub statistic: f64,
    /// Standardised statistic (normal approximation, continuity corrected).
    pub z: f64,
    /// p-value of the two-sided test.
    pub p_value: f64,
    /// Theil–Sen slope of the trend.
    pub slope: f64,
}

/// Mann–Kendall test for a monotone trend in a series.
///
/// `S = Σ_{i<j} sign(xⱼ − xᵢ)`; the variance carries the tie correction
/// `n(n−1)(2n+5) − Σt(t−1)(2t+5)` over the groups of tied values. The
/// Theil–Sen slope is the median of the pairwise slopes.
pub fn mann_kendall(values: &[f64]) -> Result<Trend> {
    if values.len() < 4 {
        return Err(Error::SampleTooSmall {
            n: values.len(),
            min: 4,
        });
    }
    // Both loops are quadratic. Keep them bounded: the statistic uses at most
    // TREND_POINTS points (every k-th observation of a longer series) and the
    // Theil-Sen slopes at most TREND_SLOPES pairwise slopes. A 200 000-row
    // column used to allocate gigabytes here and died in the OOM killer.
    let step = values.len().div_ceil(TREND_POINTS).max(1);
    let sampled: Vec<f64> = values.iter().step_by(step).copied().collect();
    let values = sampled.as_slice();
    let n = values.len();
    let pairs = n.saturating_mul(n.saturating_sub(1)) / 2;
    let slope_step = pairs.div_ceil(TREND_SLOPES).max(1);
    let mut pair = 0usize;
    let mut statistic = 0.0;
    let mut slopes = Vec::with_capacity(pairs.min(TREND_SLOPES));
    for i in 0..n {
        for j in (i + 1)..n {
            let difference = values[j] - values[i];
            if difference > 0.0 {
                statistic += 1.0;
            } else if difference < 0.0 {
                statistic -= 1.0;
            }
            if pair.is_multiple_of(slope_step) {
                let span = (j - i) as f64;
                if span > 0.0 {
                    slopes.push(difference / span);
                }
            }
            pair += 1;
        }
    }
    // The tie groups.
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mut tie_term = 0.0;
    let mut start = 0;
    while start < n {
        let mut end = start + 1;
        while end < n && sorted[end] == sorted[start] {
            end += 1;
        }
        let t = (end - start) as f64;
        tie_term += t * (t - 1.0) * (2.0 * t + 5.0);
        start = end;
    }
    let nf = n as f64;
    let variance = (nf * (nf - 1.0) * (2.0 * nf + 5.0) - tie_term) / 18.0;
    if variance <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let corrected = if statistic > 0.0 {
        statistic - 1.0
    } else if statistic < 0.0 {
        statistic + 1.0
    } else {
        0.0
    };
    let z = corrected / variance.sqrt();
    let p_value = (2.0 * normal_sf(z.abs())).clamp(0.0, 1.0);
    slopes.sort_by(|a, b| a.total_cmp(b));
    let slope = if slopes.is_empty() {
        f64::NAN
    } else if slopes.len() % 2 == 1 {
        slopes[slopes.len() / 2]
    } else {
        (slopes[slopes.len() / 2 - 1] + slopes[slopes.len() / 2]) / 2.0
    };
    Ok(Trend {
        statistic,
        z,
        p_value,
        slope,
    })
}

pub fn analyze(df: &DataFrame, column_name: &str) -> Result<AnalysisOutput> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let chart = build(&sample)?;
    let mut result = report(column_name, &chart);
    if let Ok(trend) = mann_kendall(&chart.values) {
        result
            .statistics
            .push(("Trend (Mann-Kendall S)", trend.statistic));
        result.statistics.push(("Trend z", trend.z));
        result.statistics.push(("Trend p-value", trend.p_value));
        result.statistics.push(("Theil-Sen slope", trend.slope));
        result.notes.push((
            "Trend".to_owned(),
            "the Mann-Kendall test looks for a monotone trend in the order of the observations; \
             the Theil-Sen slope is the median of the pairwise slopes"
                .to_owned(),
        ));
    }
    let plot = plot(column_name, &chart);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn build(sample: &Sample) -> Result<RunChart> {
    if sample.n() < 2 {
        return Err(Error::SampleTooSmall {
            n: sample.n(),
            min: 2,
        });
    }
    let values = sample.values().to_vec();
    let median = sample.median()?;

    let mut above = 0usize;
    let mut below = 0usize;
    let mut ties = 0usize;
    let mut signs: Vec<i8> = Vec::with_capacity(values.len());
    for &value in &values {
        if value > median {
            above += 1;
            signs.push(1);
        } else if value < median {
            below += 1;
            signs.push(-1);
        } else {
            ties += 1;
        }
    }

    let runs = runs_in(&signs);
    let non_ties = signs.len();
    let runs_expected = if non_ties == 0 {
        0.0
    } else {
        1.0 + 2.0 * above as f64 * below as f64 / non_ties as f64
    };
    let runs_p = if non_ties >= 2 {
        let n1 = above as f64;
        let n2 = below as f64;
        let n = n1 + n2;
        let variance = 2.0 * n1 * n2 * (2.0 * n1 * n2 - n) / (n * n * (n - 1.0));
        if variance.is_finite() && variance > 0.0 {
            let z = (runs as f64 - runs_expected) / variance.sqrt();
            Some((2.0 * normal_sf(z.abs())).clamp(0.0, 1.0))
        } else {
            None
        }
    } else {
        None
    };
    let longest_run = longest_run(&signs);

    let (updown_runs, updown_expected, updown_p) = updown(&values);

    Ok(RunChart {
        values,
        median,
        above,
        below,
        ties,
        runs,
        runs_expected,
        longest_run,
        runs_p,
        updown_runs,
        updown_expected,
        updown_p,
        excluded: sample.excluded(),
    })
}

fn runs_in(signs: &[i8]) -> usize {
    if signs.is_empty() {
        return 0;
    }
    1 + signs.windows(2).filter(|pair| pair[0] != pair[1]).count()
}

fn longest_run(signs: &[i8]) -> usize {
    let mut longest = 0usize;
    let mut current = 0usize;
    let mut previous: Option<i8> = None;
    for &sign in signs {
        if Some(sign) == previous {
            current += 1;
        } else {
            current = 1;
            previous = Some(sign);
        }
        longest = longest.max(current);
    }
    longest
}

fn updown(values: &[f64]) -> (usize, f64, Option<f64>) {
    let directions: Vec<i8> = values
        .windows(2)
        .filter_map(|pair| {
            let delta = pair[1] - pair[0];
            if delta > 0.0 {
                Some(1)
            } else if delta < 0.0 {
                Some(-1)
            } else {
                None
            }
        })
        .collect();
    if directions.len() < 2 {
        return (0, 0.0, None);
    }
    let runs = runs_in(&directions);
    // The runs-up-and-down statistic counts turning points in the sequence;
    // add one for the initial run, matching the classical (2N−1)/3 mean.
    let points = directions.len() as f64 + 1.0;
    let expected = (2.0 * points - 1.0) / 3.0;
    let variance = (16.0 * points - 29.0) / 90.0;
    let p = if variance > 0.0 {
        let z = (runs as f64 - expected) / variance.sqrt();
        Some((2.0 * normal_sf(z.abs())).clamp(0.0, 1.0))
    } else {
        None
    };
    (runs, expected, p)
}

fn report(column_name: &str, chart: &RunChart) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("Run Chart — column {column_name}"),
        "Median center line; runs tests by Normal approximation",
    );
    result.n = chart.n();
    result.excluded = chart.excluded;
    result.statistics = vec![
        ("Median", chart.median),
        ("Observations above median", chart.above as f64),
        ("Observations below median", chart.below as f64),
        ("Ties at median", chart.ties as f64),
        ("Runs about median", chart.runs as f64),
        ("Longest run", chart.longest_run as f64),
        ("Runs up/down", chart.updown_runs as f64),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Test".to_owned(),
            "Runs".to_owned(),
            "Expected".to_owned(),
            "p-value".to_owned(),
        ],
        rows: vec![
            vec![
                "About the median".to_owned(),
                chart.runs.to_string(),
                format!("{:.3}", chart.runs_expected),
                chart
                    .runs_p
                    .map_or_else(|| "—".to_owned(), |p| format!("{p:.6}")),
            ],
            vec![
                "Up and down".to_owned(),
                chart.updown_runs.to_string(),
                format!("{:.3}", chart.updown_expected),
                chart
                    .updown_p
                    .map_or_else(|| "—".to_owned(), |p| format!("{p:.6}")),
            ],
        ],
    });
    result.notes = vec![
        (
            "Center line".to_owned(),
            "sample median; observations equal to the median are omitted from the runs-about-the-median test"
                .to_owned(),
        ),
        (
            "Tests".to_owned(),
            "runs about the median and runs up/down, both with a two-sided Normal approximation"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(chart.n(), chart.excluded, chart.ties);
    result.warnings.push(
        "run-chart test selection and tie conventions have not been verified against documented output"
            .to_owned(),
    );
    result
}

fn plot(column_name: &str, chart: &RunChart) -> PlotSpec {
    let y_min = chart.values.iter().copied().fold(chart.median, f64::min);
    let y_max = chart.values.iter().copied().fold(chart.median, f64::max);
    let pad = ((y_max - y_min).abs() * 0.05).max(1e-9);
    let x_min = 1.0;
    let x_max = chart.n().max(1) as f64;
    PlotSpec {
        title: format!("Run Chart — {column_name}"),
        x_axis: linear_axis("Observation", x_min, x_max),
        y_axis: linear_axis(column_name, y_min - pad, y_max + pad),
        size: (900, 400),
        points: chart
            .values
            .iter()
            .enumerate()
            .step_by(crate::plot::plot_stride(chart.values.len()))
            .map(|(index, &value)| Point {
                x: (index + 1) as f64,
                y: value,
            })
            .collect(),
        lines: vec![Segment {
            from: Point {
                x: x_min,
                y: chart.median,
            },
            to: Point {
                x: x_max,
                y: chart.median,
            },
            width: 1.5,
        }],
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: crate::plot::sampling_notes(chart.values.len()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_long_series_stays_bounded() {
        // 20 000 points used to allocate O(n^2) slopes and die in the OOM
        // killer; the trend now runs on at most TREND_POINTS points.
        let values: Vec<f64> = (0..20_000).map(|index| index as f64).collect();
        let trend = mann_kendall(&values).expect("trend");
        assert!(trend.z > 0.0, "{}", trend.z);
        assert!(trend.slope.is_finite() && trend.slope > 0.0);
        let n = 5000.0_f64;
        assert_eq!(trend.statistic, n * (n - 1.0) / 2.0);
    }

    #[test]
    fn the_trend_test_detects_a_monotone_series() {
        let values: Vec<f64> = (0..20).map(|i| i as f64 * 2.0).collect();
        let trend = mann_kendall(&values).expect("trend");
        assert_eq!(trend.statistic, 190.0);
        assert!(trend.p_value < 0.001, "p {}", trend.p_value);
        assert!((trend.slope - 2.0).abs() < 1e-12, "slope {}", trend.slope);
    }

    #[test]
    fn the_trend_test_accepts_a_series_without_a_trend() {
        let values = vec![5.0, 5.0, 4.0, 6.0, 5.0, 4.0, 6.0, 5.0, 5.0, 4.0];
        let trend = mann_kendall(&values).expect("trend");
        assert!(trend.p_value > 0.5, "p {}", trend.p_value);
        assert!(trend.slope.abs() < 1.0, "slope {}", trend.slope);
    }

    #[test]
    fn a_completely_flat_series_has_no_trend_information() {
        let values = vec![5.0; 10];
        assert!(matches!(mann_kendall(&values), Err(Error::ConstantSample)));
    }

    #[test]
    fn the_trend_test_rejects_short_series() {
        assert!(matches!(
            mann_kendall(&[1.0, 2.0, 3.0]),
            Err(Error::SampleTooSmall { .. })
        ));
    }

    use super::*;
    use crate::data::Column;

    fn frame(values: Vec<f64>) -> DataFrame {
        DataFrame::new(vec![Column::new_f64("x", values)]).unwrap()
    }

    #[test]
    fn alternating_series_has_many_runs() {
        let out =
            analyze(&frame(vec![1.0, 10.0, 2.0, 9.0, 3.0, 8.0, 4.0, 7.0]), "x").expect("run chart");
        let chart = &out.results[0];
        assert!(chart.to_session_text().contains("Run Chart"));
        assert!(chart.to_session_text().contains("About the median"));
        assert_eq!(out.plot.as_ref().expect("plot").points.len(), 8);
        let chart_data = build(
            &Sample::new(
                vec![1.0, 10.0, 2.0, 9.0, 3.0, 8.0, 4.0, 7.0],
                MissingPolicy::Drop,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(chart_data.runs, 8);
        assert_eq!(chart_data.above, 4);
        assert_eq!(chart_data.below, 4);
        assert!(chart_data.runs_p.expect("p") < 0.05);
    }

    #[test]
    fn a_trend_has_few_runs_and_a_small_p_value() {
        let chart = build(
            &Sample::new(
                vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0],
                MissingPolicy::Drop,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(chart.runs, 2);
        assert!(chart.runs_p.expect("p") < 0.05);
        assert_eq!(chart.updown_runs, 1);
        assert!(chart.updown_p.expect("p") < 0.05);
    }

    #[test]
    fn ties_are_reported_and_removed_from_the_runs_test() {
        let chart =
            build(&Sample::new(vec![5.0, 5.0, 1.0, 5.0, 9.0], MissingPolicy::Drop).unwrap())
                .unwrap();
        assert_eq!(chart.ties, 3);
        assert_eq!(chart.above, 1);
        assert_eq!(chart.below, 1);
        assert_eq!(chart.runs, 2);
    }

    #[test]
    fn too_few_observations_are_rejected() {
        assert!(matches!(
            analyze(&frame(vec![1.0]), "x"),
            Err(Error::SampleTooSmall { .. })
        ));
    }

    #[test]
    fn a_large_run_chart_is_thinned_with_a_note() {
        let n = 10_001usize;
        let values: Vec<f64> = (0..n)
            .map(|index| ((index % 97) as f64) + (index / 500) as f64 * 0.01)
            .collect();
        let df = crate::data::DataFrame::new(vec![crate::data::Column::new_f64("x", values)])
            .expect("frame");
        let out = analyze(&df, "x").expect("run chart");
        let plot = out.plot.expect("plot");
        assert!(plot.points.len() <= crate::plot::MAX_PLOT_POINTS);
        assert!(plot.points.len() < n);
        assert!(!plot.notes.is_empty(), "the sample must be explained");
    }
}
