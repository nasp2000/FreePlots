//! Control charts (SPC).
//!
//! Implemented here, and not in the UI, for the same reason as every other
//! statistical method: the Session text and the graph must be generated from
//! one auditable result. The chart constants are the published Shewhart
//! constants for subgroup sizes 2–25.
//!
//! Scope: I–MR (individuals / moving range), X̄–R (subgroup mean / range) and
//! X̄–S (subgroup mean / standard deviation). All eight tests for special
//! causes run with the documented default sensitivities; the dispersion
//! charts keep tests 1–4 because their distribution is skewed. Every report
//! says this explicitly.

use crate::data::{DataFrame, MissingPolicy, Sample};
use crate::error::{Error, Result};
use crate::estimate::c4;
use crate::plot::{PlotNote, PlotSpec, Point, Segment, linear_axis, panel_plot};
use crate::report::{AnalysisResult, Item};
use crate::workflow::AnalysisOutput;

/// Control chart family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlChartKind {
    /// Individuals and moving range (subgroup size 1).
    IndividualsMovingRange,
    /// Subgroup mean and range.
    XbarR,
    /// Subgroup mean and standard deviation.
    XbarS,
    /// Exponentially weighted moving average of individuals.
    Ewma,
    /// Cumulative sum chart of individuals or subgroup means.
    Cusum,
}

impl ControlChartKind {
    /// Stable name used in titles and menus.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::IndividualsMovingRange => "I–MR",
            Self::XbarR => "X̄–R",
            Self::XbarS => "X̄–S",
            Self::Ewma => "EWMA",
            Self::Cusum => "CUSUM",
        }
    }

    /// Long description used by the report.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::IndividualsMovingRange => {
                "Individuals and moving range; limits from the average moving range and d2(2)"
            }
            Self::XbarR => "Subgroup mean and range; limits from the average range",
            Self::XbarS => {
                "Subgroup mean and standard deviation; limits from the average standard deviation"
            }
            Self::Ewma => "Exponentially weighted moving average; λ = 0.2 and three-sigma limits",
            Self::Cusum => "Cumulative sum chart; reference value 0.5σ and decision interval 4σ",
        }
    }

    /// Parses the CLI spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().replace('_', "-").as_str() {
            "i-mr" | "imr" | "individuals" => Some(Self::IndividualsMovingRange),
            "xbar-r" | "xbar-r-chart" | "x-r" | "xbarr" => Some(Self::XbarR),
            "xbar-s" | "xbar-s-chart" | "x-s" | "xbars" => Some(Self::XbarS),
            "ewma" | "ewma-chart" => Some(Self::Ewma),
            "cusum" | "cusum-chart" | "cusum-i" => Some(Self::Cusum),
            _ => None,
        }
    }
}

/// One control limit set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Center line.
    pub center: f64,
    /// Lower control limit.
    pub lcl: f64,
    /// Upper control limit.
    pub ucl: f64,
}

/// Tests for special causes and their sensitivities.
///
/// The defaults are the documented ones of the captured run's `DEFTEST`
/// subcommand. Test 1 (a point outside a control limit) is fixed at three
/// sigma because that is where the limits are drawn; the other seven accept
/// the documented ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestSet {
    mask: u16,
    sensitivity: [u8; 8],
}

/// Documented default sensitivity of each test.
const DEFAULT_SENSITIVITY: [u8; 8] = [3, 9, 6, 14, 2, 4, 15, 8];

/// Documented accepted range of each sensitivity.
const SENSITIVITY_RANGE: [(u8, u8); 8] = [
    (1, 6),
    (7, 11),
    (5, 8),
    (12, 14),
    (2, 4),
    (3, 6),
    (12, 15),
    (6, 10),
];

impl Default for TestSet {
    /// The comparison run's default: only Test 1 (a point outside a
    /// control limit). The other tests are opt-in through `TEST`.
    fn default() -> Self {
        Self::standard()
    }
}

impl TestSet {
    /// Only Test 1, the captured run's default for a new chart.
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            mask: 0x0001,
            sensitivity: DEFAULT_SENSITIVITY,
        }
    }

    /// All eight tests with the default sensitivities.
    #[must_use]
    pub const fn all() -> Self {
        Self {
            mask: 0x00ff,
            sensitivity: DEFAULT_SENSITIVITY,
        }
    }

    /// No test at all.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            mask: 0,
            sensitivity: DEFAULT_SENSITIVITY,
        }
    }

    /// Only the listed tests, with the default sensitivities.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when a test number is outside 1–8.
    pub fn only(tests: &[u8]) -> Result<Self> {
        let mut set = Self::none();
        for &test in tests {
            if !(1..=8).contains(&test) {
                return Err(Error::Unsupported(format!(
                    "test {test} is outside the supported range 1–8"
                )));
            }
            set.mask |= 1 << (test - 1);
        }
        Ok(set)
    }

    /// Parses a list such as `1:8`, `1 3 5` or `1,3,5`.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the text is empty, when a range is reversed or
    /// when a test number is outside 1–8.
    pub fn parse_list(text: &str) -> Result<Self> {
        let mut tests = Vec::new();
        let cleaned = text.replace(',', " ");
        for part in cleaned.split_whitespace() {
            if let Some((low, high)) = part.split_once(':') {
                let low: u8 = low
                    .trim()
                    .parse()
                    .map_err(|_| Error::Unsupported(format!("invalid test range: {part}")))?;
                let high: u8 = high
                    .trim()
                    .parse()
                    .map_err(|_| Error::Unsupported(format!("invalid test range: {part}")))?;
                if low > high {
                    return Err(Error::Unsupported(format!(
                        "test range {part} starts above its end"
                    )));
                }
                tests.extend(low..=high);
            } else {
                tests.push(
                    part.parse()
                        .map_err(|_| Error::Unsupported(format!("invalid test number: {part}")))?,
                );
            }
        }
        if tests.is_empty() {
            return Err(Error::Unsupported("no test given".to_owned()));
        }
        Self::only(&tests)
    }

    /// Whether a test runs.
    #[must_use]
    pub const fn is_enabled(&self, test: u8) -> bool {
        test >= 1 && test <= 8 && self.mask & (1 << (test - 1)) != 0
    }

    /// Sensitivity of a test (the `K` of the documented table).
    #[must_use]
    pub const fn sensitivity(&self, test: u8) -> u8 {
        if test >= 1 && test <= 8 {
            self.sensitivity[(test - 1) as usize]
        } else {
            0
        }
    }

    /// Enables or disables one test.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the test number is outside 1–8.
    pub fn set_enabled(mut self, test: u8, enabled: bool) -> Result<Self> {
        if !(1..=8).contains(&test) {
            return Err(Error::Unsupported(format!(
                "test {test} is outside the supported range 1–8"
            )));
        }
        if enabled {
            self.mask |= 1 << (test - 1);
        } else {
            self.mask &= !(1 << (test - 1));
        }
        Ok(self)
    }

    /// Sets the sensitivity of one test.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the test number is outside 1–8, when the value
    /// is outside the documented range, or when test 1 is changed (the control
    /// limits are drawn at three sigma).
    pub fn with_sensitivity(mut self, test: u8, value: u8) -> Result<Self> {
        if !(1..=8).contains(&test) {
            return Err(Error::Unsupported(format!(
                "test {test} is outside the supported range 1–8"
            )));
        }
        if test == 1 {
            return Err(Error::Unsupported(
                "the control limits are drawn at three sigma, so the sensitivity of test 1 is fixed"
                    .to_owned(),
            ));
        }
        let (low, high) = SENSITIVITY_RANGE[(test - 1) as usize];
        if !(low..=high).contains(&value) {
            return Err(Error::Unsupported(format!(
                "sensitivity {value} of test {test} is outside the documented range {low}–{high}"
            )));
        }
        self.sensitivity[(test - 1) as usize] = value;
        Ok(self)
    }

    /// Short summary of the enabled tests, e.g. `1–8` or `1–3, 5–8`.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut start: Option<u8> = None;
        for test in 1..=9u8 {
            let enabled = test <= 8 && self.is_enabled(test);
            match (enabled, start) {
                (true, None) => start = Some(test),
                (false, Some(first)) => {
                    let last = test - 1;
                    parts.push(if first == last {
                        first.to_string()
                    } else {
                        format!("{first}–{last}")
                    });
                    start = None;
                }
                _ => {}
            }
        }
        if parts.is_empty() {
            "none".to_owned()
        } else {
            parts.join(", ")
        }
    }
}

/// Chart on which a signal was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalChart {
    /// Location chart (individuals or subgroup mean).
    Location,
    /// Dispersion chart (moving range, range or standard deviation).
    Dispersion,
}

/// A special-cause signal.
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    /// Test number (1–4).
    pub test: u8,
    /// Chart where the signal appears.
    pub chart: SignalChart,
    /// One-based point index on that chart.
    pub index: usize,
    /// Spelled-out reason.
    pub description: String,
}

/// Ready-to-report control chart.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlChart {
    /// Chart family.
    pub kind: ControlChartKind,
    /// Observations used.
    pub n: usize,
    /// Non-finite observations excluded.
    pub excluded: usize,
    /// Location points (individuals or subgroup means).
    pub location: Vec<f64>,
    /// Dispersion points (moving ranges, ranges or standard deviations).
    pub dispersion: Vec<f64>,
    /// Location limits (representative when the subgroup sizes vary).
    pub location_limits: Limits,
    /// Dispersion limits (representative when the subgroup sizes vary).
    pub dispersion_limits: Limits,
    /// Per-point location limits for unequal subgroup sizes; empty when the
    /// limits are constant.
    pub location_steps: Vec<Limits>,
    /// Per-point dispersion limits for unequal subgroup sizes; empty when the
    /// limits are constant.
    pub dispersion_steps: Vec<Limits>,
    /// Subgroup sizes for unequal subgroups; empty when they are constant.
    pub subgroup_sizes: Vec<usize>,
    /// One-based x positions of the dispersion points; empty means 2, 3, …
    /// (I–MR) or 1, 2, … (subgroup charts). Phases leave a gap where a moving
    /// range spans a phase boundary.
    pub dispersion_positions: Vec<usize>,
    /// Limits of each phase; empty when the chart has no phase column.
    pub phases: Vec<Phase>,
    /// Within sigma estimate.
    pub sigma: f64,
    /// Subgroup size (1 for I–MR).
    pub subgroup: usize,
    /// Tests for special causes that were run.
    pub tests: TestSet,
    /// CUSUM reference value and decision interval, for a CUSUM chart.
    pub cusum: Option<CusumOptions>,
    /// EWMA smoothing constant and limit width, for an EWMA chart.
    pub ewma: Option<EwmaOptions>,
    /// Signals found.
    pub signals: Vec<Signal>,
    /// Notes added by the estimator (e.g. dropped incomplete subgroup).
    pub notes: Vec<(String, String)>,
    /// Warnings added by the estimator.
    pub warnings: Vec<String>,
}

/// Limits of one phase of a control chart.
#[derive(Debug, Clone, PartialEq)]
pub struct Phase {
    /// Label read from the phase column.
    pub label: String,
    /// One-based index of the first point of the phase on the location chart.
    pub start: usize,
    /// Number of points in the phase.
    pub points: usize,
    /// Location limits of the phase.
    pub location: Limits,
    /// Dispersion limits of the phase.
    pub dispersion: Limits,
    /// Within sigma of the phase.
    pub sigma: f64,
}

/// Signals listed in a report before the table switches to a summary row.
pub(crate) const SIGNAL_ROWS: usize = 500;

/// Rows of the signal table, capped so a million-point chart does not print
/// a megabyte of session text.
pub(crate) fn signal_rows(signals: &[Signal]) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = signals
        .iter()
        .take(SIGNAL_ROWS)
        .map(|signal| {
            vec![
                signal.test.to_string(),
                match signal.chart {
                    SignalChart::Location => "Location".to_owned(),
                    SignalChart::Dispersion => "Dispersion".to_owned(),
                },
                signal.index.to_string(),
                signal.description.clone(),
            ]
        })
        .collect();
    if signals.len() > SIGNAL_ROWS {
        rows.push(vec![
            "…".to_owned(),
            String::new(),
            format!("{} more", signals.len() - SIGNAL_ROWS),
            "the chart marks every signal".to_owned(),
        ]);
    }
    rows
}

/// Runs a control chart for one numeric column.
///
/// # Errors
///
/// Returns [`Error`] when the column is missing, the sample is too small, the
/// subgroup size is outside the supported range or the process is constant.
pub fn analyze(
    df: &DataFrame,
    column: &str,
    kind: ControlChartKind,
    subgroup: usize,
) -> Result<AnalysisOutput> {
    analyze_with_tests(df, column, kind, subgroup, TestSet::default())
}

/// Runs a control chart with an explicit set of tests for special causes.
///
/// # Errors
///
/// Returns [`Error`] under the same conditions as [`analyze`].
pub fn analyze_with_tests(
    df: &DataFrame,
    column: &str,
    kind: ControlChartKind,
    subgroup: usize,
    tests: TestSet,
) -> Result<AnalysisOutput> {
    analyze_with_options(
        df,
        column,
        kind,
        subgroup,
        tests,
        CusumOptions::default(),
        EwmaOptions::default(),
    )
}

/// Runs a control chart with explicit test and CUSUM options.
///
/// # Errors
///
/// As [`analyze_with_tests`], plus an invalid CUSUM `(k, h)` pair.
pub fn analyze_with_options(
    df: &DataFrame,
    column: &str,
    kind: ControlChartKind,
    subgroup: usize,
    tests: TestSet,
    cusum: CusumOptions,
    ewma: EwmaOptions,
) -> Result<AnalysisOutput> {
    let data = df.column(column)?;
    let sample = data.sample(MissingPolicy::Drop)?;
    let chart = match kind {
        ControlChartKind::IndividualsMovingRange => {
            individuals_moving_range_with_tests(&sample, tests)?
        }
        ControlChartKind::XbarR => subgroup_chart(&sample, subgroup, Spread::Range, tests)?,
        ControlChartKind::XbarS => {
            subgroup_chart(&sample, subgroup, Spread::StandardDeviation, tests)?
        }
        ControlChartKind::Ewma => ewma_chart_with_options(&sample, tests, ewma)?,
        ControlChartKind::Cusum => cusum_chart_with_options(&sample, subgroup, cusum)?,
    };

    let result = report(column, &chart);
    let plot = plot(column, &chart);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

/// Runs an X̄–R or X̄–S chart from a subgroup-label column.
///
/// Rows with a missing measurement or subgroup label are excluded.
///
/// # Errors
///
/// As [`variable_subgroup_chart`], plus an empty worksheet after the exclusions.
pub fn analyze_variable_subgroups(
    df: &DataFrame,
    column: &str,
    group: &str,
    kind: ControlChartKind,
    tests: TestSet,
) -> Result<AnalysisOutput> {
    let groups = group_samples(df, column, group)?;
    let chart = variable_subgroup_chart(&groups, kind, tests)?;
    Ok(AnalysisOutput {
        results: vec![report(column, &chart)],
        plot: Some(plot(column, &chart)),
    })
}

/// Runs a control chart whose limits are computed **within each phase** of a
/// label column.
///
/// A phase is a run of consecutive rows with the same label, so the worksheet
/// order is the chart order. Supported for the I–MR, X̄–R and X̄–S charts;
/// EWMA and CUSUM reset and are better expressed as separate charts.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for another chart family, a phase with too
/// few observations, and the usual errors of the chosen chart.
pub fn analyze_with_phase(
    df: &DataFrame,
    column: &str,
    kind: ControlChartKind,
    subgroup: usize,
    tests: TestSet,
    phase: &str,
) -> Result<AnalysisOutput> {
    if !matches!(
        kind,
        ControlChartKind::IndividualsMovingRange
            | ControlChartKind::XbarR
            | ControlChartKind::XbarS
    ) {
        return Err(Error::Unsupported(
            "phases are available for the I–MR, X̄–R and X̄–S charts".to_owned(),
        ));
    }
    let values_column = df.column(column)?;
    let phase_column = df.column(phase)?;
    let mut runs: Vec<(String, Vec<f64>)> = Vec::new();
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        let (Some(value), Some(label)) = (values_column.value(row), phase_column.label(row)) else {
            excluded += 1;
            continue;
        };
        match runs.last_mut() {
            Some((last, values)) if *last == label => values.push(value),
            _ => runs.push((label, vec![value])),
        }
    }
    if runs.is_empty() {
        return Err(Error::EmptySample);
    }

    let mut location = Vec::new();
    let mut dispersion = Vec::new();
    let mut dispersion_positions = Vec::new();
    let mut location_steps = Vec::new();
    let mut dispersion_steps = Vec::new();
    let mut signals = Vec::new();
    let mut phases = Vec::new();
    let mut warnings = Vec::new();
    let mut notes = Vec::new();
    let mut used_observations = 0usize;
    let mut point_start = 0usize;
    let mut first: Option<ControlChart> = None;
    for (label, values) in runs {
        let sample = Sample::new(values, MissingPolicy::Drop)?;
        let mut chart = match kind {
            ControlChartKind::IndividualsMovingRange => {
                individuals_moving_range_with_tests(&sample, tests)
            }
            ControlChartKind::XbarR => subgroup_chart(&sample, subgroup, Spread::Range, tests),
            ControlChartKind::XbarS => {
                subgroup_chart(&sample, subgroup, Spread::StandardDeviation, tests)
            }
            _ => unreachable!("checked above"),
        }
        .map_err(|error| Error::Unsupported(format!("phase '{label}': {error}")))?;
        if kind != ControlChartKind::IndividualsMovingRange
            && chart.location.len() * subgroup < sample.n()
        {
            let dropped = sample.n() - chart.location.len() * subgroup;
            warnings.push(format!(
                "phase '{label}': {dropped} trailing observation(s) did not fill a complete subgroup"
            ));
        }
        let points = chart.location.len();
        let location_limits = chart.location_limits;
        let dispersion_limits = chart.dispersion_limits;
        location_steps.extend(std::iter::repeat_n(location_limits, points));
        dispersion_steps.extend(std::iter::repeat_n(
            dispersion_limits,
            chart.dispersion.len(),
        ));
        for index in 0..chart.dispersion.len() {
            dispersion_positions.push(if kind == ControlChartKind::IndividualsMovingRange {
                point_start + index + 2
            } else {
                point_start + index + 1
            });
        }
        for signal in std::mem::take(&mut chart.signals) {
            let index = signal.index
                + match (kind, signal.chart) {
                    (ControlChartKind::IndividualsMovingRange, SignalChart::Dispersion) => {
                        point_start + 1
                    }
                    _ => point_start,
                };
            signals.push(Signal {
                index,
                description: format!("Phase {label}: {}", signal.description),
                ..signal
            });
        }
        phases.push(Phase {
            label,
            start: point_start + 1,
            points,
            location: location_limits,
            dispersion: dispersion_limits,
            sigma: chart.sigma,
        });
        location.extend_from_slice(&chart.location);
        dispersion.extend_from_slice(&chart.dispersion);
        used_observations += chart.n;
        point_start += points;
        if first.is_none() {
            notes = chart.notes.clone();
            first = Some(chart);
        }
    }
    let Some(base) = first else {
        return Err(Error::EmptySample);
    };
    notes.push((
        "Phases".to_owned(),
        "consecutive runs of the phase column; each phase has its own limits".to_owned(),
    ));
    if kind == ControlChartKind::IndividualsMovingRange && phases.len() > 1 {
        notes.push((
            "Moving ranges".to_owned(),
            "computed within each phase; the first observation of a phase has no moving range"
                .to_owned(),
        ));
    }
    let chart = ControlChart {
        kind,
        n: used_observations,
        excluded,
        location,
        dispersion,
        // With phases the table carries the limits; these are the first phase's
        // values and keep older code paths well defined.
        location_limits: phases[0].location,
        dispersion_limits: phases[0].dispersion,
        location_steps,
        dispersion_steps,
        subgroup_sizes: Vec::new(),
        dispersion_positions,
        phases,
        sigma: base.sigma,
        subgroup: base.subgroup,
        tests,
        cusum: None,
        ewma: None,
        signals,
        notes,
        warnings,
    };
    Ok(AnalysisOutput {
        results: vec![report(column, &chart)],
        plot: Some(plot(column, &chart)),
    })
}

fn group_samples(df: &DataFrame, column: &str, group: &str) -> Result<Vec<Sample>> {
    let values = df.column(column)?;
    let labels = df.column(group)?;
    let mut order: Vec<String> = Vec::new();
    let mut buckets: std::collections::BTreeMap<String, Vec<f64>> =
        std::collections::BTreeMap::new();
    for row in 0..df.n_rows() {
        let Some(value) = values.value(row) else {
            continue;
        };
        let Some(label) = labels.label(row) else {
            continue;
        };
        if !buckets.contains_key(&label) {
            order.push(label.clone());
        }
        buckets.entry(label).or_default().push(value);
    }
    if order.is_empty() {
        return Err(Error::EmptySample);
    }
    order
        .into_iter()
        .map(|label| {
            let values = buckets.remove(&label).unwrap_or_default();
            Sample::new(values, MissingPolicy::Drop)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spread {
    Range,
    StandardDeviation,
}

/// Computes an I–MR chart from one sample; exposed for the capability stability gate.
pub fn individuals_moving_range(sample: &Sample) -> Result<ControlChart> {
    individuals_moving_range_with_tests(sample, TestSet::default())
}

/// Computes an X̄–R or X̄–S chart from subgroups of different sizes.
///
/// Each sample is one subgroup. The pooled within sigma is the
/// degrees-of-freedom weighted mean of the per-subgroup estimates
/// (`Rᵢ/d2(nᵢ)` or `sᵢ/c4(nᵢ)`), and every point gets its own limits from its
/// own subgroup size. The signal tests run on the standardised deviations
/// `(xᵢ − centreᵢ)/sigmaᵢ`, so tests 2–8 remain meaningful when the limits
/// change from point to point.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for a chart other than X̄–R/X̄–S, fewer than
/// two subgroups or a subgroup size outside 2–25, and
/// [`Error::ConstantSample`] when the pooled sigma is zero.
pub fn variable_subgroup_chart(
    samples: &[Sample],
    kind: ControlChartKind,
    tests: TestSet,
) -> Result<ControlChart> {
    if !matches!(kind, ControlChartKind::XbarR | ControlChartKind::XbarS) {
        return Err(Error::Unsupported(
            "variable subgroup sizes are available for the X̄–R and X̄–S charts".to_owned(),
        ));
    }
    if samples.len() < 2 {
        return Err(Error::Unsupported(
            "the chart needs at least two subgroups".to_owned(),
        ));
    }
    let mut means = Vec::with_capacity(samples.len());
    let mut spreads = Vec::with_capacity(samples.len());
    let mut sizes = Vec::with_capacity(samples.len());
    let mut excluded = 0usize;
    let mut total = 0usize;
    let mut weighted_mean = 0.0;
    let mut degrees = 0.0;
    let mut pooled = 0.0;
    for sample in samples {
        let n = sample.n();
        if !(2..=25).contains(&n) {
            return Err(Error::Unsupported(format!(
                "subgroup size {n} is outside the supported range 2–25"
            )));
        }
        let mean = sample.mean()?;
        let (spread, sigma) = match kind {
            ControlChartKind::XbarR => {
                let spread = sample.max()? - sample.min()?;
                (spread, spread / d2(n)?)
            }
            ControlChartKind::XbarS => {
                let spread = sample.sd()?;
                (spread, spread / crate::estimate::c4(n))
            }
            _ => unreachable!("checked above"),
        };
        let df = (n - 1) as f64;
        degrees += df;
        // The captured run averages the per-subgroup estimates weighted by
        // their degrees of freedom; it does not pool the variances (which
        // would give a larger within sigma for these unequal subgroups).
        pooled += df * sigma;
        weighted_mean += n as f64 * mean;
        total += n;
        excluded += sample.excluded();
        means.push(mean);
        spreads.push(spread);
        sizes.push(n);
    }
    if degrees <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let sigma = pooled / degrees;
    if !sigma.is_finite() || sigma <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let center = weighted_mean / total as f64;
    let mut location_centers = Vec::with_capacity(samples.len());
    let mut location_sigmas = Vec::with_capacity(samples.len());
    let mut location_steps = Vec::with_capacity(samples.len());
    let mut dispersion_centers = Vec::with_capacity(samples.len());
    let mut dispersion_sigmas = Vec::with_capacity(samples.len());
    let mut dispersion_steps = Vec::with_capacity(samples.len());
    for &n in &sizes {
        let location_sigma = sigma / (n as f64).sqrt();
        location_centers.push(center);
        location_sigmas.push(location_sigma);
        location_steps.push(Limits {
            center,
            lcl: center - 3.0 * location_sigma,
            ucl: center + 3.0 * location_sigma,
        });
        let (dispersion_center, dispersion_sigma) = match kind {
            ControlChartKind::XbarR => (d2(n)? * sigma, d3(n)? * sigma),
            ControlChartKind::XbarS => {
                let c4 = crate::estimate::c4(n);
                (c4 * sigma, sigma * (1.0 - c4 * c4).max(0.0).sqrt())
            }
            _ => unreachable!("checked above"),
        };
        dispersion_centers.push(dispersion_center);
        dispersion_sigmas.push(dispersion_sigma);
        dispersion_steps.push(Limits {
            center: dispersion_center,
            lcl: (dispersion_center - 3.0 * dispersion_sigma).max(0.0),
            ucl: dispersion_center + 3.0 * dispersion_sigma,
        });
    }
    let n_average = ((total as f64 / samples.len() as f64).round() as usize).clamp(2, 25);
    let location_sigma = sigma / (n_average as f64).sqrt();
    let location_limits = Limits {
        center,
        lcl: center - 3.0 * location_sigma,
        ucl: center + 3.0 * location_sigma,
    };
    let (dispersion_center, dispersion_sigma) = match kind {
        ControlChartKind::XbarR => (d2(n_average)? * sigma, d3(n_average)? * sigma),
        ControlChartKind::XbarS => {
            let c4 = crate::estimate::c4(n_average);
            (c4 * sigma, sigma * (1.0 - c4 * c4).max(0.0).sqrt())
        }
        _ => unreachable!("checked above"),
    };
    let dispersion_limits = Limits {
        center: dispersion_center,
        lcl: (dispersion_center - 3.0 * dispersion_sigma).max(0.0),
        ucl: dispersion_center + 3.0 * dispersion_sigma,
    };
    let mut signals = variable_signals(
        &means,
        &location_centers,
        &location_sigmas,
        SignalChart::Location,
        tests,
    );
    signals.extend(variable_signals(
        &spreads,
        &dispersion_centers,
        &dispersion_sigmas,
        SignalChart::Dispersion,
        tests,
    ));
    let smallest = sizes.iter().copied().min().unwrap_or(0);
    let largest = sizes.iter().copied().max().unwrap_or(0);
    Ok(ControlChart {
        kind,
        n: total,
        excluded,
        location: means,
        dispersion: spreads,
        location_limits,
        dispersion_limits,
        location_steps,
        dispersion_steps,
        subgroup_sizes: sizes.clone(),
        dispersion_positions: Vec::new(),
        phases: Vec::new(),
        sigma,
        subgroup: 0,
        tests,
        cusum: None,
        ewma: None,
        signals,
        notes: vec![
            (
                "Subgroups".to_owned(),
                format!("variable sizes ({smallest}–{largest}); one point per subgroup"),
            ),
            (
                "Limits".to_owned(),
                "each point uses its own subgroup size; the summary limits are at the average size"
                    .to_owned(),
            ),
            (
                "Subgroup sizes".to_owned(),
                "variable; every point uses its own subgroup size for its limits".to_owned(),
            ),
        ],
        warnings: Vec::new(),
    })
}

/// Runs the signal tests on standardised deviations with per-point limits.
fn variable_signals(
    values: &[f64],
    centers: &[f64],
    sigmas: &[f64],
    chart: SignalChart,
    tests: TestSet,
) -> Vec<Signal> {
    let standardized: Vec<f64> = values
        .iter()
        .zip(centers)
        .zip(sigmas)
        .map(|((value, center), sigma)| {
            if *sigma > 0.0 {
                (value - center) / sigma
            } else {
                0.0
            }
        })
        .collect();
    let mut signals = Vec::new();
    if tests.is_enabled(1) {
        for (index, (value, score)) in values.iter().zip(&standardized).enumerate() {
            if score.abs() > 3.0 {
                signals.push(Signal {
                    test: 1,
                    chart,
                    index: index + 1,
                    description: format!("point outside its subgroup control limit ({value:.6})"),
                });
            }
        }
    }
    let rest = tests.set_enabled(1, false).unwrap_or(tests);
    let sigma = if chart == SignalChart::Location {
        Some(1.0)
    } else {
        None
    };
    signals.extend(run_tests(
        &standardized,
        &Limits {
            center: 0.0,
            lcl: -3.0,
            ucl: 3.0,
        },
        chart,
        1,
        sigma,
        rest,
    ));
    signals
}

/// Computes an I–MR chart with an explicit set of tests.
///
/// # Errors
///
/// Returns [`Error`] when the sample is too small or the process is constant.
pub fn individuals_moving_range_with_tests(
    sample: &Sample,
    tests: TestSet,
) -> Result<ControlChart> {
    if sample.n() < 2 {
        return Err(Error::SampleTooSmall {
            n: sample.n(),
            min: 2,
        });
    }
    let values = sample.values();
    let moving: Vec<f64> = values
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .collect();
    let mean = sample.mean()?;
    let mr_bar = moving.iter().sum::<f64>() / moving.len() as f64;
    if mr_bar <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let d2 = d2(2)?;
    let sigma = mr_bar / d2;
    let location_limits = Limits {
        center: mean,
        lcl: mean - 3.0 * sigma,
        ucl: mean + 3.0 * sigma,
    };
    let dispersion_limits = Limits {
        center: mr_bar,
        lcl: 0.0,
        ucl: d4(2)? * mr_bar,
    };
    let mut signals = Vec::new();
    signals.extend(location_signals(values, &location_limits, 1, tests));
    signals.extend(dispersion_signals(&moving, &dispersion_limits, 2, tests));
    Ok(ControlChart {
        kind: ControlChartKind::IndividualsMovingRange,
        n: sample.n(),
        excluded: sample.excluded(),
        location: values.to_vec(),
        dispersion: moving,
        location_limits,
        dispersion_limits,
        location_steps: Vec::new(),
        dispersion_steps: Vec::new(),
        subgroup_sizes: Vec::new(),
        dispersion_positions: Vec::new(),
        phases: Vec::new(),
        sigma,
        subgroup: 1,
        tests,
        cusum: None,
        ewma: None,
        signals,
        notes: vec![(
            "Moving ranges".to_owned(),
            "|x[i] − x[i−1]|; the first observation has no moving range".to_owned(),
        )],
        warnings: Vec::new(),
    })
}

/// Exponentially weighted moving average of individuals.
///
/// `z[i] = λ·x[i] + (1−λ)·z[i−1]` with `λ = 0.2`; the step-wise
/// three-sigma limits are used for the signals and the asymptotic limits are
/// drawn on the chart.
pub fn ewma_chart(sample: &Sample) -> Result<ControlChart> {
    ewma_chart_with_tests(sample, TestSet::default())
}

/// Exponentially weighted moving average with an explicit set of tests.
///
/// # Errors
///
/// Returns [`Error`] when the sample is too small or the process is constant.
pub fn ewma_chart_with_tests(sample: &Sample, tests: TestSet) -> Result<ControlChart> {
    ewma_chart_with_options(sample, tests, EwmaOptions::default())
}

/// EWMA chart with explicit smoothing and limit options.
///
/// # Errors
///
/// As [`ewma_chart_with_tests`], plus an invalid `(λ, L)` pair.
pub fn ewma_chart_with_options(
    sample: &Sample,
    tests: TestSet,
    options: EwmaOptions,
) -> Result<ControlChart> {
    let options = EwmaOptions::new(options.lambda, options.limit)?;
    if sample.n() < 3 {
        return Err(Error::SampleTooSmall {
            n: sample.n(),
            min: 3,
        });
    }
    let values = sample.values();
    let mean = sample.mean()?;
    let moving: Vec<f64> = values
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .collect();
    let mr_bar = moving.iter().sum::<f64>() / moving.len() as f64;
    if mr_bar <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let sigma = mr_bar / d2(2)?;
    let mut z = Vec::with_capacity(values.len());
    let mut previous = mean;
    for &value in values {
        previous = options.lambda * value + (1.0 - options.lambda) * previous;
        z.push(previous);
    }
    let mut signals = Vec::new();
    for (index, &value) in z.iter().enumerate() {
        let step = index + 1;
        let factor = (options.lambda / (2.0 - options.lambda)
            * (1.0 - (1.0 - options.lambda).powi(2 * step as i32)))
        .max(0.0);
        let half = options.limit * sigma * factor.sqrt();
        let limits = Limits {
            center: mean,
            lcl: mean - half,
            ucl: mean + half,
        };
        if tests.is_enabled(1) && (value < limits.lcl || value > limits.ucl) {
            signals.push(Signal {
                test: 1,
                chart: SignalChart::Location,
                index: index + 1,
                description: format!("EWMA point outside the step-wise limits ({:.6})", value),
            });
        }
    }
    let asymptotic = (options.lambda / (2.0 - options.lambda)).sqrt();
    let location_limits = Limits {
        center: mean,
        lcl: mean - options.limit * sigma * asymptotic,
        ucl: mean + options.limit * sigma * asymptotic,
    };
    Ok(ControlChart {
        kind: ControlChartKind::Ewma,
        n: sample.n(),
        excluded: sample.excluded(),
        location: z,
        dispersion: Vec::new(),
        location_limits,
        dispersion_limits: Limits {
            center: 0.0,
            lcl: 0.0,
            ucl: 0.0,
        },
        location_steps: Vec::new(),
        dispersion_steps: Vec::new(),
        subgroup_sizes: Vec::new(),
        dispersion_positions: Vec::new(),
        phases: Vec::new(),
        sigma,
        subgroup: 1,
        tests,
        cusum: None,
        ewma: Some(options),
        signals,
        notes: vec![
            (
                "Smoothing".to_owned(),
                format!("z[i] = λ·x[i] + (1−λ)·z[i−1] with λ = {}", options.lambda),
            ),
            (
                "Limits".to_owned(),
                format!(
                    "signals use the step-wise limits; the chart draws the asymptotic limits at \
                     L = {}σ",
                    options.limit
                ),
            ),
        ],
        warnings: Vec::new(),
    })
}

/// Reference value of the CUSUM chart, in sigma units.
const CUSUM_K: f64 = 0.5;
/// Decision interval of the CUSUM chart, in sigma units.
const CUSUM_H: f64 = 4.0;

/// Reference value and decision interval of a CUSUM chart, in sigma units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CusumOptions {
    /// Reference value `k` (half the shift to detect).
    pub reference: f64,
    /// Decision interval `h`.
    pub decision: f64,
}

impl Default for CusumOptions {
    fn default() -> Self {
        Self {
            reference: CUSUM_K,
            decision: CUSUM_H,
        }
    }
}

/// Smoothing constant and limit width of an EWMA chart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EwmaOptions {
    /// Smoothing constant `λ` in `(0, 1]`.
    pub lambda: f64,
    /// Limit width `L` in sigma units.
    pub limit: f64,
}

impl Default for EwmaOptions {
    fn default() -> Self {
        Self {
            lambda: 0.2,
            limit: 3.0,
        }
    }
}

impl EwmaOptions {
    /// Validates a custom `(λ, L)` pair.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidParameter`] when `λ` is outside `(0, 1]` or `L`
    /// is not finite and positive.
    pub fn new(lambda: f64, limit: f64) -> Result<Self> {
        if !lambda.is_finite() || lambda <= 0.0 || lambda > 1.0 {
            return Err(Error::InvalidParameter {
                name: "EWMA smoothing constant (lambda)",
                value: lambda,
            });
        }
        if !limit.is_finite() || limit <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "EWMA limit width (L)",
                value: limit,
            });
        }
        Ok(Self { lambda, limit })
    }
}

impl CusumOptions {
    /// Validates a custom `(k, h)` pair.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidParameter`] when either value is not finite or
    /// is not positive.
    pub fn new(reference: f64, decision: f64) -> Result<Self> {
        if !reference.is_finite() || reference <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "CUSUM reference value (k)",
                value: reference,
            });
        }
        if !decision.is_finite() || decision <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "CUSUM decision interval (h)",
                value: decision,
            });
        }
        Ok(Self {
            reference,
            decision,
        })
    }
}

/// Cumulative sum chart of individuals (`subgroup` of 1) or of subgroup means.
///
/// The deviations from the centre line are standardised and accumulated on
/// both sides: `C⁺ = max(0, C⁺ + z − k)` and `C⁻ = max(0, C⁻ − z − k)` with
/// the reference value `k = 0.5` and the decision interval `h = 4`.
pub fn cusum_chart(sample: &Sample, subgroup: usize) -> Result<ControlChart> {
    cusum_chart_with_options(sample, subgroup, CusumOptions::default())
}

/// CUSUM chart with an explicit reference value and decision interval.
///
/// # Errors
///
/// As [`cusum_chart`], plus an invalid `(k, h)` pair.
pub fn cusum_chart_with_options(
    sample: &Sample,
    subgroup: usize,
    options: CusumOptions,
) -> Result<ControlChart> {
    let options = CusumOptions::new(options.reference, options.decision)?;
    let values = sample.values();
    let (points, sigma, subgroup) = if subgroup <= 1 {
        if values.len() < 3 {
            return Err(Error::SampleTooSmall {
                n: values.len(),
                min: 3,
            });
        }
        let moving: Vec<f64> = values
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .collect();
        let mr_bar = moving.iter().sum::<f64>() / moving.len() as f64;
        if mr_bar <= 0.0 {
            return Err(Error::ConstantSample);
        }
        (values.to_vec(), mr_bar / d2(2)?, 1usize)
    } else {
        if !(2..=25).contains(&subgroup) {
            return Err(Error::Unsupported(format!(
                "subgroup size {subgroup} is outside the supported range 2–25"
            )));
        }
        if values.len() < subgroup * 2 {
            return Err(Error::SampleTooSmall {
                n: values.len(),
                min: subgroup * 2,
            });
        }
        let used = values.len() / subgroup * subgroup;
        let mut means = Vec::new();
        let mut ranges = Vec::new();
        for group in values[..used].chunks_exact(subgroup) {
            let group_sample = Sample::from_valid(group.to_vec(), 0)?;
            means.push(group_sample.mean()?);
            ranges.push(group_sample.max()? - group_sample.min()?);
        }
        let r_bar = ranges.iter().sum::<f64>() / ranges.len() as f64;
        if r_bar <= 0.0 {
            return Err(Error::ConstantSample);
        }
        let sigma = (r_bar / d2(subgroup)?) / (subgroup as f64).sqrt();
        (means, sigma, subgroup)
    };
    let center = points.iter().sum::<f64>() / points.len() as f64;
    let mut upper = Vec::with_capacity(points.len());
    let mut lower = Vec::with_capacity(points.len());
    let mut signals = Vec::new();
    let mut c_up = 0.0f64;
    let mut c_low = 0.0f64;
    for (index, &value) in points.iter().enumerate() {
        let z = (value - center) / sigma;
        c_up = (c_up + z - options.reference).max(0.0);
        c_low = (c_low - z - options.reference).max(0.0);
        upper.push(c_up);
        lower.push(c_low);
        if c_up > options.decision {
            signals.push(Signal {
                test: 1,
                chart: SignalChart::Location,
                index: index + 1,
                description: format!("upper CUSUM above the decision interval ({c_up:.4})"),
            });
        }
        if c_low > options.decision {
            signals.push(Signal {
                test: 1,
                chart: SignalChart::Dispersion,
                index: index + 1,
                description: format!("lower CUSUM above the decision interval ({c_low:.4})"),
            });
        }
    }
    let limits = Limits {
        center: 0.0,
        lcl: 0.0,
        ucl: options.decision,
    };
    let mut notes = vec![
        (
            "Reference value".to_owned(),
            format!("k = {}σ", options.reference),
        ),
        (
            "Decision interval".to_owned(),
            format!("h = {}σ", options.decision),
        ),
        (
            "CUSUM".to_owned(),
            "C⁺ = max(0, C⁺ + z − k); C⁻ = max(0, C⁻ − z − k), with z = (x − x̄)/σ".to_owned(),
        ),
    ];
    let mut warnings = Vec::new();
    if subgroup > 1 {
        notes.push((
            "Subgroups".to_owned(),
            "consecutive groups in worksheet order; σ is the standard error of the subgroup mean"
                .to_owned(),
        ));
        if !values.len().is_multiple_of(subgroup) {
            let dropped = values.len() % subgroup;
            notes.push((
                "Incomplete subgroup".to_owned(),
                format!("{dropped} trailing observation(s) not used"),
            ));
            warnings.push(format!(
                "{dropped} trailing observation(s) did not fill a complete subgroup and were not used"
            ));
        }
    }
    Ok(ControlChart {
        kind: ControlChartKind::Cusum,
        n: sample.n(),
        excluded: sample.excluded(),
        location: upper,
        dispersion: lower,
        location_limits: limits,
        dispersion_limits: limits,
        location_steps: Vec::new(),
        dispersion_steps: Vec::new(),
        subgroup_sizes: Vec::new(),
        dispersion_positions: Vec::new(),
        phases: Vec::new(),
        sigma,
        subgroup,
        tests: TestSet::none(),
        cusum: Some(options),
        ewma: None,
        signals,
        notes,
        warnings,
    })
}

fn subgroup_chart(
    sample: &Sample,
    subgroup: usize,
    spread: Spread,
    tests: TestSet,
) -> Result<ControlChart> {
    if !(2..=25).contains(&subgroup) {
        return Err(Error::Unsupported(format!(
            "subgroup size {subgroup} is outside the supported range 2–25"
        )));
    }
    if sample.n() < subgroup * 2 {
        return Err(Error::SampleTooSmall {
            n: sample.n(),
            min: subgroup * 2,
        });
    }
    let values = sample.values();
    let used = values.len() / subgroup * subgroup;
    let groups = values[..used].chunks_exact(subgroup);
    let mut means = Vec::new();
    let mut dispersions = Vec::new();
    for group in groups {
        let group_sample = Sample::from_valid(group.to_vec(), 0)?;
        means.push(group_sample.mean()?);
        dispersions.push(match spread {
            Spread::Range => group_sample.max()? - group_sample.min()?,
            Spread::StandardDeviation => group_sample.sd()?,
        });
    }
    if means.len() < 2 {
        return Err(Error::SampleTooSmall {
            n: sample.n(),
            min: subgroup * 2,
        });
    }
    let grand_mean = means.iter().sum::<f64>() / means.len() as f64;
    let spread_mean = dispersions.iter().sum::<f64>() / dispersions.len() as f64;
    if spread_mean <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let (location_limits, dispersion_limits, sigma) = match spread {
        Spread::Range => {
            let a2 = a2(subgroup)?;
            let d4 = d4(subgroup)?;
            // D3 = max(0, 1 − 3·d3/d2); the table's `d3` is the standard
            // deviation of the range in σ units, not the lower limit factor.
            let d3_lower = (1.0 - 3.0 * d3(subgroup)? / d2(subgroup)?).max(0.0);
            (
                Limits {
                    center: grand_mean,
                    lcl: grand_mean - a2 * spread_mean,
                    ucl: grand_mean + a2 * spread_mean,
                },
                Limits {
                    center: spread_mean,
                    lcl: d3_lower * spread_mean,
                    ucl: d4 * spread_mean,
                },
                spread_mean / d2(subgroup)?,
            )
        }
        Spread::StandardDeviation => {
            let a3 = a3(subgroup)?;
            let b3 = b3(subgroup)?;
            let b4 = b4(subgroup)?;
            (
                Limits {
                    center: grand_mean,
                    lcl: grand_mean - a3 * spread_mean,
                    ucl: grand_mean + a3 * spread_mean,
                },
                Limits {
                    center: spread_mean,
                    lcl: b3 * spread_mean,
                    ucl: b4 * spread_mean,
                },
                spread_mean / c4(subgroup),
            )
        }
    };
    let mut signals = Vec::new();
    signals.extend(location_signals(&means, &location_limits, 1, tests));
    signals.extend(dispersion_signals(
        &dispersions,
        &dispersion_limits,
        1,
        tests,
    ));
    let mut notes = vec![(
        "Subgroups".to_owned(),
        "consecutive groups in worksheet order".to_owned(),
    )];
    let mut warnings = Vec::new();
    if used < values.len() {
        let dropped = values.len() - used;
        notes.push((
            "Incomplete subgroup".to_owned(),
            format!("{dropped} trailing observation(s) not used"),
        ));
        warnings.push(format!(
            "{dropped} trailing observation(s) did not fill a complete subgroup and were not used"
        ));
    }
    Ok(ControlChart {
        kind: match spread {
            Spread::Range => ControlChartKind::XbarR,
            Spread::StandardDeviation => ControlChartKind::XbarS,
        },
        n: sample.n(),
        excluded: sample.excluded(),
        location: means,
        dispersion: dispersions,
        location_limits,
        dispersion_limits,
        location_steps: Vec::new(),
        dispersion_steps: Vec::new(),
        subgroup_sizes: Vec::new(),
        dispersion_positions: Vec::new(),
        phases: Vec::new(),
        sigma,
        subgroup,
        tests,
        cusum: None,
        ewma: None,
        signals,
        notes,
        warnings,
    })
}

fn location_signals(
    values: &[f64],
    limits: &Limits,
    start_index: usize,
    tests: TestSet,
) -> Vec<Signal> {
    // The location limits are three sigma wide, so they also give the chart
    // sigma used by the one- and two-sigma tests.
    let sigma = (limits.ucl - limits.lcl) / 6.0;
    run_tests(
        values,
        limits,
        SignalChart::Location,
        start_index,
        Some(sigma),
        tests,
    )
}

fn dispersion_signals(
    values: &[f64],
    limits: &Limits,
    start_index: usize,
    tests: TestSet,
) -> Vec<Signal> {
    // The dispersion charts keep the run tests only: their distribution is
    // skewed, so the one- and two-sigma tests do not apply.
    run_tests(
        values,
        limits,
        SignalChart::Dispersion,
        start_index,
        None,
        tests,
    )
}

/// Runs the enabled tests; tests 5–8 need a chart sigma and the location chart.
fn run_tests(
    values: &[f64],
    limits: &Limits,
    chart: SignalChart,
    start_index: usize,
    sigma: Option<f64>,
    tests: TestSet,
) -> Vec<Signal> {
    let mut signals = Vec::new();
    let run = |test: u8| -> usize { tests.sensitivity(test) as usize };
    // Test 1: a point outside a control limit.
    if tests.is_enabled(1) {
        for (index, value) in values.iter().enumerate() {
            if *value < limits.lcl || *value > limits.ucl {
                push_signal(
                    &mut signals,
                    Signal {
                        test: 1,
                        chart,
                        index: start_index + index,
                        description: "point outside a control limit".to_owned(),
                    },
                );
            }
        }
    }
    // Test 2: K points on the same side of the center line.
    if tests.is_enabled(2) {
        let k = run(2);
        for (offset, window) in values.windows(k).enumerate() {
            let end = offset + k - 1;
            let above = window.iter().all(|v| *v > limits.center);
            let below = window.iter().all(|v| *v < limits.center);
            if above || below {
                push_signal(
                    &mut signals,
                    Signal {
                        test: 2,
                        chart,
                        index: start_index + end,
                        description: format!(
                            "{k} points in a row on the same side of the center line"
                        ),
                    },
                );
            }
        }
    }
    // Test 3: K points steadily increasing or decreasing. The captured run
    // counts K *changes*, so the window holds K + 1 points (captured: a
    // 12-point run with K = 6 flags points 7–12).
    if tests.is_enabled(3) {
        let k = run(3);
        let span = k + 1;
        for (offset, window) in values.windows(span).enumerate() {
            let end = offset + span - 1;
            let increasing = window.windows(2).all(|pair| pair[0] < pair[1]);
            let decreasing = window.windows(2).all(|pair| pair[0] > pair[1]);
            if increasing || decreasing {
                push_signal(
                    &mut signals,
                    Signal {
                        test: 3,
                        chart,
                        index: start_index + end,
                        description: format!(
                            "{k} points in a row steadily increasing or decreasing"
                        ),
                    },
                );
            }
        }
    }
    // Test 4: K points alternating up and down. As with Test 3, K counts the
    // changes, so the window holds K + 1 points (captured: K = 14 flags from
    // point 15 of a 24-point alternating run).
    if tests.is_enabled(4) {
        let k = run(4);
        let span = k + 1;
        for (offset, window) in values.windows(span).enumerate() {
            let end = offset + span - 1;
            if window
                .windows(2)
                .all(|pair| (pair[1] - pair[0]).signum() != 0.0)
            {
                let signs: Vec<f64> = window.windows(2).map(|pair| pair[1] - pair[0]).collect();
                let alternating = signs.windows(2).all(|pair| pair[0] * pair[1] < 0.0);
                if alternating {
                    push_signal(
                        &mut signals,
                        Signal {
                            test: 4,
                            chart,
                            index: start_index + end,
                            description: format!("{k} points in a row alternating up and down"),
                        },
                    );
                }
            }
        }
    }
    // The one- and two-sigma tests apply to the location chart only.
    let Some(sigma) = sigma.filter(|sigma| *sigma > 0.0) else {
        return signals;
    };
    if chart != SignalChart::Location {
        return signals;
    }
    // Test 5: K out of K+1 points more than two sigma from the center line on
    // the same side.
    if tests.is_enabled(5) {
        let k = run(5);
        let span = k + 1;
        let band = 2.0 * sigma;
        for (offset, window) in values.windows(span).enumerate() {
            let end = offset + span - 1;
            let current = window[span - 1];
            let above = window
                .iter()
                .filter(|value| **value > limits.center + band)
                .count();
            let below = window
                .iter()
                .filter(|value| **value < limits.center - band)
                .count();
            let fires = (above >= k && current > limits.center + band)
                || (below >= k && current < limits.center - band);
            if fires {
                push_signal(
                    &mut signals,
                    Signal {
                        test: 5,
                        chart,
                        index: start_index + end,
                        description: format!(
                            "{k} out of {span} points more than 2σ from the center line on the \
                             same side"
                        ),
                    },
                );
            }
        }
    }
    // Test 6: K out of K+1 points more than one sigma from the center line on
    // the same side.
    if tests.is_enabled(6) {
        let k = run(6);
        let span = k + 1;
        let band = sigma;
        for (offset, window) in values.windows(span).enumerate() {
            let end = offset + span - 1;
            let current = window[span - 1];
            let above = window
                .iter()
                .filter(|value| **value > limits.center + band)
                .count();
            let below = window
                .iter()
                .filter(|value| **value < limits.center - band)
                .count();
            let fires = (above >= k && current > limits.center + band)
                || (below >= k && current < limits.center - band);
            if fires {
                push_signal(
                    &mut signals,
                    Signal {
                        test: 6,
                        chart,
                        index: start_index + end,
                        description: format!(
                            "{k} out of {span} points more than 1σ from the center line on the \
                             same side"
                        ),
                    },
                );
            }
        }
    }
    // Test 7: K points in a row within one sigma of the center line.
    if tests.is_enabled(7) {
        let k = run(7);
        for (offset, window) in values.windows(k).enumerate() {
            let end = offset + k - 1;
            if window
                .iter()
                .all(|value| (*value - limits.center).abs() <= sigma)
            {
                push_signal(
                    &mut signals,
                    Signal {
                        test: 7,
                        chart,
                        index: start_index + end,
                        description: format!("{k} points in a row within 1σ of the center line"),
                    },
                );
            }
        }
    }
    // Test 8: K points in a row more than one sigma from the center line,
    // either side.
    if tests.is_enabled(8) {
        let k = run(8);
        for (offset, window) in values.windows(k).enumerate() {
            let end = offset + k - 1;
            if window
                .iter()
                .all(|value| (*value - limits.center).abs() > sigma)
            {
                push_signal(
                    &mut signals,
                    Signal {
                        test: 8,
                        chart,
                        index: start_index + end,
                        description: format!(
                            "{k} points in a row more than 1σ from the center line"
                        ),
                    },
                );
            }
        }
    }
    signals
}

fn push_signal(signals: &mut Vec<Signal>, signal: Signal) {
    // Every test visits each index at most once, so a duplicate scan here
    // would make a signal-heavy chart O(n²). Append directly; the tests keep
    // the per-test/per-index contract.
    signals.push(signal);
}

fn report(column: &str, chart: &ControlChart) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("Control Chart: {} — column {column}", chart.kind.name()),
        chart.kind.description(),
    );
    result.n = chart.n;
    result.excluded = chart.excluded;
    let phased = !chart.phases.is_empty();
    if phased {
        result.statistics = vec![
            ("Phases", chart.phases.len() as f64),
            ("Points", chart.location.len() as f64),
        ];
        if chart.subgroup > 1 {
            result
                .statistics
                .push(("Subgroup size", chart.subgroup as f64));
        }
    } else {
        result.statistics = vec![
            ("Center line", chart.location_limits.center),
            ("Location LCL", chart.location_limits.lcl),
            ("Location UCL", chart.location_limits.ucl),
        ];
    }
    if !phased {
        if chart.kind != ControlChartKind::Ewma {
            result
                .statistics
                .push(("Dispersion center line", chart.dispersion_limits.center));
            result
                .statistics
                .push(("Dispersion LCL", chart.dispersion_limits.lcl));
            result
                .statistics
                .push(("Dispersion UCL", chart.dispersion_limits.ucl));
        }
        result.statistics.push(("Sigma estimate", chart.sigma));
    }
    if chart.kind == ControlChartKind::Cusum {
        let cusum = chart.cusum.unwrap_or_default();
        result.statistics = vec![
            ("Reference value (k, in sigma units)", cusum.reference),
            ("Decision interval (h, in sigma units)", cusum.decision),
            ("Sigma estimate", chart.sigma),
        ];
        if chart.subgroup > 1 {
            result
                .statistics
                .push(("Subgroup size", chart.subgroup as f64));
            result
                .statistics
                .push(("Subgroups used", chart.location.len() as f64));
        }
    }
    if chart.subgroup > 1 && !phased {
        result
            .statistics
            .push(("Subgroup size", chart.subgroup as f64));
        result
            .statistics
            .push(("Subgroups used", chart.location.len() as f64));
    }
    if phased {
        result.extra.push(Item::Table {
            headers: vec![
                "Phase".to_owned(),
                "Start".to_owned(),
                "Points".to_owned(),
                "Location CL".to_owned(),
                "Location LCL".to_owned(),
                "Location UCL".to_owned(),
                "Dispersion CL".to_owned(),
                "Dispersion LCL".to_owned(),
                "Dispersion UCL".to_owned(),
                "Sigma".to_owned(),
            ],
            rows: chart
                .phases
                .iter()
                .map(|phase| {
                    vec![
                        phase.label.clone(),
                        phase.start.to_string(),
                        phase.points.to_string(),
                        format!("{:.6}", phase.location.center),
                        format!("{:.6}", phase.location.lcl),
                        format!("{:.6}", phase.location.ucl),
                        format!("{:.6}", phase.dispersion.center),
                        format!("{:.6}", phase.dispersion.lcl),
                        format!("{:.6}", phase.dispersion.ucl),
                        format!("{:.6}", phase.sigma),
                    ]
                })
                .collect(),
        });
    } else {
        result.extra.push(Item::Table {
            headers: vec![
                "Chart".to_owned(),
                "Center".to_owned(),
                "LCL".to_owned(),
                "UCL".to_owned(),
                "Points".to_owned(),
            ],
            rows: {
                let mut rows = vec![vec![
                    if chart.kind == ControlChartKind::Cusum {
                        "Upper CUSUM".to_owned()
                    } else {
                        "Location".to_owned()
                    },
                    format!("{:.6}", chart.location_limits.center),
                    format!("{:.6}", chart.location_limits.lcl),
                    format!("{:.6}", chart.location_limits.ucl),
                    chart.location.len().to_string(),
                ]];
                if chart.kind != ControlChartKind::Ewma {
                    rows.push(vec![
                        if chart.kind == ControlChartKind::Cusum {
                            "Lower CUSUM".to_owned()
                        } else {
                            "Dispersion".to_owned()
                        },
                        format!("{:.6}", chart.dispersion_limits.center),
                        format!("{:.6}", chart.dispersion_limits.lcl),
                        format!("{:.6}", chart.dispersion_limits.ucl),
                        chart.dispersion.len().to_string(),
                    ]);
                }
                rows
            },
        });
    }
    if !chart.subgroup_sizes.is_empty() {
        result.extra.push(Item::Table {
            headers: vec![
                "Subgroup".to_owned(),
                "n".to_owned(),
                "Mean".to_owned(),
                "LCL".to_owned(),
                "UCL".to_owned(),
                "Spread".to_owned(),
                "LCL".to_owned(),
                "UCL".to_owned(),
            ],
            rows: chart
                .location
                .iter()
                .enumerate()
                .map(|(index, mean)| {
                    let n = chart.subgroup_sizes[index];
                    let location = chart.location_steps[index];
                    let dispersion = chart.dispersion_steps[index];
                    vec![
                        (index + 1).to_string(),
                        n.to_string(),
                        format!("{mean:.6}"),
                        format!("{:.6}", location.lcl),
                        format!("{:.6}", location.ucl),
                        format!("{:.6}", chart.dispersion[index]),
                        format!("{:.6}", dispersion.lcl),
                        format!("{:.6}", dispersion.ucl),
                    ]
                })
                .collect(),
        });
    }
    if chart.signals.is_empty() {
        result.extra.push(Item::Message(
            "No out-of-control points by the applied tests.".to_owned(),
        ));
    } else {
        let rows = signal_rows(&chart.signals);
        result.extra.push(Item::Table {
            headers: vec![
                "Test".to_owned(),
                "Chart".to_owned(),
                "Point".to_owned(),
                "Signal".to_owned(),
            ],
            rows,
        });
    }
    result.notes = if chart.kind == ControlChartKind::Cusum {
        vec![
            (
                "Control limits".to_owned(),
                "the cumulative sums are compared with the decision interval h = 4σ".to_owned(),
            ),
            (
                "Tests".to_owned(),
                "the decision interval is applied to the upper and to the lower cumulative sum"
                    .to_owned(),
            ),
        ]
    } else if chart.kind == ControlChartKind::Ewma {
        vec![
            (
                "Control limits".to_owned(),
                "three sigma from the center line, scaled by the smoothing constant".to_owned(),
            ),
            (
                "Tests".to_owned(),
                "Test 1 on the smoothed series, evaluated against the step-wise limits".to_owned(),
            ),
        ]
    } else {
        vec![
            (
                "Control limits".to_owned(),
                "three sigma from the center line, using the published Shewhart constants"
                    .to_owned(),
            ),
            ("Tests".to_owned(), tests_note(chart)),
        ]
    };
    result.notes.extend(chart.notes.clone());
    result.warnings = chart.warnings.clone();
    result.warnings.push(
        "the default test set is Test 1 only, as the captured charts; tests 1, 2, 5, 6 and 8 \
         and their per-point marking match the captured tables, as do 3 and 4 (which count K \
         changes) and 7"
            .to_owned(),
    );
    result
}

/// Note describing the tests that ran and, when they differ, their
/// sensitivities.
fn tests_note(chart: &ControlChart) -> String {
    let description = |test: u8| match test {
        1 => "outside limits",
        2 => "runs on one side",
        3 => "trends",
        4 => "alternating",
        5 => "2σ patterns",
        6 => "1σ patterns",
        7 => "near the center line",
        _ => "far from the center line",
    };
    let enabled: Vec<String> = (1..=8)
        .filter(|test| chart.tests.is_enabled(*test))
        .map(|test| format!("{test} ({})", description(test)))
        .collect();
    let mut note = format!(
        "{} on the location chart; 1–4 on the dispersion chart",
        if enabled.is_empty() {
            "no tests".to_owned()
        } else {
            enabled.join(", ")
        }
    );
    if chart.tests != TestSet::all() {
        let sensitivities: Vec<String> = (1..=8)
            .filter(|test| chart.tests.is_enabled(*test))
            .map(|test| format!("test {test} K = {}", chart.tests.sensitivity(test)))
            .collect();
        note.push_str(&format!("; sensitivities: {}", sensitivities.join(", ")));
    }
    note
}

fn plot(column: &str, chart: &ControlChart) -> PlotSpec {
    let title = format!("{} Chart — {}", chart.kind.name(), column);
    let (location_label, dispersion_label) = match chart.kind {
        ControlChartKind::IndividualsMovingRange => ("Individuals", "Moving range"),
        ControlChartKind::XbarR => ("Subgroup mean", "Sample range"),
        ControlChartKind::XbarS => ("Subgroup mean", "Sample standard deviation"),
        ControlChartKind::Ewma => ("EWMA", "Moving range"),
        ControlChartKind::Cusum => ("Upper CUSUM", "Lower CUSUM"),
    };
    let location = control_panel(
        &chart.location,
        1,
        &[],
        chart.location_limits,
        &chart.location_steps,
        location_label,
        "Observation",
        "Value",
    );
    if chart.kind == ControlChartKind::Ewma {
        return control_panel(
            &chart.location,
            1,
            &[],
            chart.location_limits,
            &chart.location_steps,
            "EWMA",
            "Observation",
            "Value",
        );
    }
    let dispersion = control_panel(
        &chart.dispersion,
        if chart.kind == ControlChartKind::IndividualsMovingRange {
            2
        } else {
            1
        },
        &chart.dispersion_positions,
        chart.dispersion_limits,
        &chart.dispersion_steps,
        dispersion_label,
        if chart.kind == ControlChartKind::IndividualsMovingRange {
            "Observation"
        } else {
            "Subgroup"
        },
        "Value",
    );
    panel_plot(vec![location, dispersion], 1, title).unwrap_or_else(|| {
        control_panel(
            &chart.location,
            1,
            &[],
            chart.location_limits,
            &chart.location_steps,
            location_label,
            "Observation",
            "Value",
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn control_panel(
    values: &[f64],
    start: usize,
    positions: &[usize],
    limits: Limits,
    steps: &[Limits],
    title: &str,
    x_label: &str,
    y_label: &str,
) -> PlotSpec {
    let xs: Vec<f64> = if positions.len() == values.len() && !positions.is_empty() {
        positions.iter().map(|position| *position as f64).collect()
    } else {
        (start..start + values.len())
            .map(|index| index as f64)
            .collect()
    };
    let x_min = xs.first().copied().unwrap_or(start as f64);
    let x_max = xs.last().copied().unwrap_or(x_min);
    let mut y_min = values.iter().copied().fold(limits.lcl, f64::min);
    let mut y_max = values.iter().copied().fold(limits.ucl, f64::max);
    for step in steps {
        y_min = y_min.min(step.lcl).min(step.center);
        y_max = y_max.max(step.ucl).max(step.center);
    }
    let pad = ((y_max - y_min).abs() * 0.05).max(1e-9);
    y_min -= pad;
    y_max += pad;
    let lines = if steps.is_empty() {
        [limits.center, limits.lcl, limits.ucl]
            .into_iter()
            .map(|y| Segment {
                from: Point { x: x_min, y },
                to: Point { x: x_max, y },
                width: 1.5,
            })
            .collect()
    } else {
        let mut lines = Vec::with_capacity(steps.len() * 6);
        for index in 0..steps.len().min(xs.len()) {
            let x0 = xs[index] - 0.5;
            let x1 = xs[index] + 0.5;
            for y in [steps[index].center, steps[index].lcl, steps[index].ucl] {
                lines.push(Segment {
                    from: Point { x: x0, y },
                    to: Point { x: x1, y },
                    width: 1.5,
                });
            }
            if index + 1 < steps.len().min(xs.len()) && xs[index + 1] - xs[index] <= 1.0 {
                for (current, next) in [
                    (steps[index].center, steps[index + 1].center),
                    (steps[index].lcl, steps[index + 1].lcl),
                    (steps[index].ucl, steps[index + 1].ucl),
                ] {
                    if (next - current).abs() > 1e-12 {
                        lines.push(Segment {
                            from: Point { x: x1, y: current },
                            to: Point { x: x1, y: next },
                            width: 1.5,
                        });
                    }
                }
            }
        }
        lines
    };
    PlotSpec {
        title: title.to_owned(),
        x_axis: linear_axis(x_label, x_min, x_max),
        y_axis: linear_axis(y_label, y_min, y_max),
        size: (900, 400),
        points: values
            .iter()
            .enumerate()
            .map(|(index, value)| Point {
                x: xs[index],
                y: *value,
            })
            .collect(),
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: if steps.is_empty() {
            vec![PlotNote::new(vec![
                ("UCL", format!("{:.4}", limits.ucl)),
                ("CL", format!("{:.4}", limits.center)),
                ("LCL", format!("{:.4}", limits.lcl)),
            ])]
        } else {
            Vec::new()
        },
    }
}

fn d2(n: usize) -> Result<f64> {
    const VALUES: [f64; 24] = [
        1.128, 1.693, 2.059, 2.326, 2.534, 2.704, 2.847, 2.970, 3.078, 3.173, 3.258, 3.336, 3.407,
        3.472, 3.532, 3.588, 3.640, 3.689, 3.735, 3.778, 3.819, 3.858, 3.895, 3.931,
    ];
    table(n, &VALUES, "d2")
}

fn d3(n: usize) -> Result<f64> {
    const VALUES: [f64; 24] = [
        0.853, 0.888, 0.880, 0.864, 0.848, 0.833, 0.820, 0.808, 0.797, 0.787, 0.778, 0.770, 0.763,
        0.756, 0.750, 0.744, 0.739, 0.734, 0.729, 0.724, 0.720, 0.716, 0.712, 0.708,
    ];
    table(n, &VALUES, "d3")
}

fn a2(n: usize) -> Result<f64> {
    const VALUES: [f64; 24] = [
        1.880, 1.023, 0.729, 0.577, 0.483, 0.419, 0.373, 0.337, 0.308, 0.285, 0.266, 0.249, 0.235,
        0.223, 0.212, 0.203, 0.194, 0.187, 0.180, 0.173, 0.167, 0.162, 0.157, 0.153,
    ];
    table(n, &VALUES, "A2")
}

fn d4(n: usize) -> Result<f64> {
    const VALUES: [f64; 24] = [
        3.267, 2.574, 2.282, 2.114, 2.004, 1.924, 1.864, 1.816, 1.777, 1.744, 1.717, 1.693, 1.672,
        1.653, 1.637, 1.622, 1.608, 1.597, 1.586, 1.575, 1.566, 1.557, 1.548, 1.541,
    ];
    table(n, &VALUES, "D4")
}

fn a3(n: usize) -> Result<f64> {
    const VALUES: [f64; 24] = [
        2.659, 1.954, 1.628, 1.427, 1.287, 1.182, 1.099, 1.032, 0.975, 0.927, 0.886, 0.850, 0.817,
        0.789, 0.763, 0.739, 0.718, 0.698, 0.680, 0.663, 0.647, 0.633, 0.619, 0.606,
    ];
    table(n, &VALUES, "A3")
}

fn b3(n: usize) -> Result<f64> {
    const VALUES: [f64; 24] = [
        0.0, 0.0, 0.0, 0.0, 0.030, 0.118, 0.185, 0.239, 0.284, 0.321, 0.354, 0.382, 0.406, 0.428,
        0.448, 0.466, 0.482, 0.497, 0.510, 0.523, 0.534, 0.545, 0.555, 0.565,
    ];
    table(n, &VALUES, "B3")
}

fn b4(n: usize) -> Result<f64> {
    const VALUES: [f64; 24] = [
        3.267, 2.568, 2.266, 2.089, 1.970, 1.882, 1.815, 1.761, 1.716, 1.679, 1.646, 1.618, 1.594,
        1.572, 1.552, 1.534, 1.518, 1.503, 1.490, 1.477, 1.466, 1.455, 1.445, 1.435,
    ];
    table(n, &VALUES, "B4")
}

fn table(n: usize, values: &[f64], name: &str) -> Result<f64> {
    values
        .get(n.checked_sub(2).ok_or_else(|| {
            Error::Unsupported(format!("{name} is not defined for subgroup size {n}"))
        })?)
        .copied()
        .ok_or_else(|| Error::Unsupported(format!("{name} is not tabulated for subgroup size {n}")))
}

#[test]
fn the_signal_table_is_capped() {
    let signals: Vec<Signal> = (1..=510)
        .map(|index| Signal {
            test: 1,
            chart: SignalChart::Location,
            index,
            description: "outside the limits".to_owned(),
        })
        .collect();
    let rows = signal_rows(&signals);
    assert_eq!(rows.len(), SIGNAL_ROWS + 1);
    assert_eq!(rows.last().unwrap()[2], "10 more");
    assert!(signal_rows(&signals[..10]).len() == 10);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Column, DataFrame};

    fn frame() -> DataFrame {
        DataFrame::new(vec![Column::new_f64(
            "weight",
            vec![10.0, 10.4, 9.8, 10.2, 10.1, 10.5, 9.9, 10.3, 10.0, 10.2],
        )])
        .expect("frame")
    }

    #[test]
    fn i_mr_limits_and_plot() {
        let out = analyze(
            &frame(),
            "weight",
            ControlChartKind::IndividualsMovingRange,
            1,
        )
        .expect("chart");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Control Chart: I–MR"));
        assert!(text.contains("Location UCL"));
        assert!(text.contains("Dispersion UCL"));
        assert!(text.contains("No out-of-control points"));
        let plot = out.plot.expect("plot");
        assert_eq!(plot.panels.len(), 2);
        // Both panels carry their limits inside the frame.
        for panel in &plot.panels {
            assert_eq!(panel.notes.len(), 1, "{:?}", panel.title);
            let rows = &panel.notes[0].rows;
            for name in ["UCL", "CL", "LCL"] {
                assert!(
                    rows.iter().any(|row| row.label == name),
                    "{name} missing from {rows:?}"
                );
            }
        }
    }

    #[test]
    fn unequal_subgroups_match_the_captured_chart() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../validation/parity19/datasets/unequal.csv"
        );
        let text = std::fs::read_to_string(path).expect("dataset");
        let mut groups: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
        for line in text.lines().skip(1) {
            let mut parts = line.trim().split(',');
            let value: f64 = parts
                .next()
                .and_then(|value| value.parse().ok())
                .expect("value");
            let group = parts.next().expect("group").to_owned();
            groups.entry(group).or_default().push(value);
        }
        let samples: Vec<Sample> = groups
            .values()
            .map(|values| Sample::new(values.clone(), MissingPolicy::Drop).expect("sample"))
            .collect();
        let chart = variable_subgroup_chart(&samples, ControlChartKind::XbarR, TestSet::default())
            .expect("chart");
        assert!((chart.sigma - 0.148851).abs() < 1e-5, "{}", chart.sigma);
        assert!((chart.location_limits.center - 9.979272).abs() < 1e-5);
        // The third subgroup has n = 3; the captured chart shows these limits.
        let third = chart.location_steps[2];
        assert!((third.lcl - 9.7214).abs() < 5e-4, "{}", third.lcl);
        assert!((third.ucl - 10.2371).abs() < 5e-4, "{}", third.ucl);
        let dispersion = chart.dispersion_steps[2];
        assert!(
            (dispersion.center - 0.2520).abs() < 5e-4,
            "{}",
            dispersion.center
        );
        assert!((dispersion.ucl - 0.6487).abs() < 5e-4, "{}", dispersion.ucl);
    }

    #[test]
    fn phases_get_their_own_limits() {
        let values = vec![10.0, 10.2, 9.8, 10.1, 20.0, 20.2, 19.8, 20.1];
        let phases = vec![
            "before".to_owned(),
            "before".to_owned(),
            "before".to_owned(),
            "before".to_owned(),
            "after".to_owned(),
            "after".to_owned(),
            "after".to_owned(),
            "after".to_owned(),
        ];
        let df = DataFrame::new(vec![
            Column::new_f64("weight", values),
            Column::new_text("phase", phases),
        ])
        .expect("frame");
        let out = analyze_with_phase(
            &df,
            "weight",
            ControlChartKind::IndividualsMovingRange,
            1,
            TestSet::default(),
            "phase",
        )
        .expect("phased chart");
        let chart = &out.results[0];
        let text = chart.to_session_text();
        assert!(text.contains("Phases"), "{text}");
        assert!(text.contains("before"), "{text}");
        assert!(text.contains("after"), "{text}");
        assert!(text.contains("Location CL"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.panels.len(), 2);
        // Both moving-range series skip the boundary: positions 2,3,4 and 6,7,8.
        let dispersion_x: Vec<f64> = plot.panels[1].points.iter().map(|point| point.x).collect();
        assert_eq!(dispersion_x, vec![2.0, 3.0, 4.0, 6.0, 7.0, 8.0]);
        // The location chart has every observation, with stepped limits.
        let location_x: Vec<f64> = plot.panels[0].points.iter().map(|point| point.x).collect();
        assert_eq!(location_x, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        assert!(
            plot.panels[0].lines.len() > 3,
            "the limits must change at the phase boundary"
        );
    }

    #[test]
    fn phases_are_checked_and_not_applied_to_ewma() {
        let values = vec![10.0, 11.0, 20.0, 21.0];
        let phases = vec![
            "a".to_owned(),
            "a".to_owned(),
            "b".to_owned(),
            "b".to_owned(),
        ];
        let df = DataFrame::new(vec![
            Column::new_f64("weight", values),
            Column::new_text("phase", phases),
        ])
        .expect("frame");
        assert!(matches!(
            analyze_with_phase(
                &df,
                "weight",
                ControlChartKind::Ewma,
                1,
                TestSet::default(),
                "phase"
            ),
            Err(Error::Unsupported(_))
        ));
        // A phase with a single point has no moving range.
        let single = DataFrame::new(vec![
            Column::new_f64("weight", vec![10.0, 12.0, 20.0]),
            Column::new_text(
                "phase",
                vec!["a".to_owned(), "a".to_owned(), "b".to_owned()],
            ),
        ])
        .expect("frame");
        assert!(matches!(
            analyze_with_phase(
                &single,
                "weight",
                ControlChartKind::IndividualsMovingRange,
                1,
                TestSet::default(),
                "phase"
            ),
            Err(Error::Unsupported(_))
        ));
        // A missing phase column is the usual column error.
        assert!(matches!(
            analyze_with_phase(
                &df,
                "weight",
                ControlChartKind::IndividualsMovingRange,
                1,
                TestSet::default(),
                "nope"
            ),
            Err(Error::ColumnNotFound(_))
        ));
    }

    #[test]
    fn subgroup_phases_keep_their_own_limits() {
        let values: Vec<f64> = vec![
            10.0, 10.2, 9.8, 10.1, 10.3, 9.9, // two subgroups of 3
            20.0, 20.2, 19.8, 20.1, 20.3, 19.9,
        ];
        let phases: Vec<String> = std::iter::repeat_n("before".to_owned(), 6)
            .chain(std::iter::repeat_n("after".to_owned(), 6))
            .collect();
        let df = DataFrame::new(vec![
            Column::new_f64("weight", values),
            Column::new_text("phase", phases),
        ])
        .expect("frame");
        let out = analyze_with_phase(
            &df,
            "weight",
            ControlChartKind::XbarR,
            3,
            TestSet::default(),
            "phase",
        )
        .expect("phased Xbar-R");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Subgroup size"), "{text}");
        assert!(text.contains("before"), "{text}");
        let plot = out.plot.expect("plot");
        // Two subgroups per phase, with four sets of limits.
        assert_eq!(plot.panels[0].points.len(), 4);
        assert!(plot.panels[0].lines.len() > 3);
        assert_eq!(plot.panels[1].points.len(), 4);
    }

    #[test]
    fn cusum_accumulates_and_signals_a_shift() {
        // A slow downward drift must trip the lower CUSUM.
        let values: Vec<f64> = (0..24)
            .map(|index| 10.0 + index as f64 * 0.08 + (index % 3) as f64 * 0.05)
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("weight", values)]).expect("frame");
        let out = analyze(&df, "weight", ControlChartKind::Cusum, 1).expect("chart");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Control Chart: CUSUM"), "{text}");
        assert!(text.contains("Reference value"), "{text}");
        assert!(text.contains("CUSUM above the decision interval"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.panels.len(), 2);
    }

    #[test]
    fn cusum_of_subgroup_means_uses_the_subgroup_size() {
        let values: Vec<f64> = (0..20).map(|index| 10.0 + index as f64 * 0.1).collect();
        let df = DataFrame::new(vec![Column::new_f64("weight", values)]).expect("frame");
        let out = analyze(&df, "weight", ControlChartKind::Cusum, 5).expect("chart");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Subgroup size"), "{text}");
        assert!(text.contains("CUSUM"), "{text}");
    }

    #[test]
    fn i_mr_detects_an_outside_point() {
        let df = DataFrame::new(vec![Column::new_f64(
            "weight",
            vec![10.0, 10.1, 10.0, 10.1, 10.0, 10.1, 10.0, 10.1, 10.0, 15.0],
        )])
        .expect("frame");
        let out =
            analyze(&df, "weight", ControlChartKind::IndividualsMovingRange, 1).expect("chart");
        let text = out.results[0].to_session_text();
        assert!(text.contains("point outside a control limit"), "{text}");
    }

    #[test]
    fn xbar_r_uses_subgroups_and_drops_an_incomplete_tail() {
        let df = DataFrame::new(vec![Column::new_f64(
            "weight",
            vec![
                10.0, 10.2, 9.8, 10.1, 10.3, 9.9, 10.2, 10.4, 10.0, 10.1, 99.0,
            ],
        )])
        .expect("frame");
        let out = analyze(&df, "weight", ControlChartKind::XbarR, 3).expect("chart");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Control Chart: X̄–R"));
        assert!(text.contains("trailing observation"));
        assert!(text.contains("Subgroups used"));
    }

    #[test]
    fn xbar_r_dispersion_lower_limit_uses_d3_not_the_sigma_multiplier() {
        // Regression: the range chart used `d3` (0.864 at n = 5) as the lower
        // limit factor, so LCL came out as 0.86·R̄ instead of 0. The captured
        // reference chart prints LCL = 0 for n = 5.
        let values: Vec<f64> = (0..21)
            .map(|index| 10.0 + index as f64 * 0.3 + (index % 3) as f64 * 0.05)
            .collect();
        let sample = Sample::new(values, crate::data::MissingPolicy::Drop).unwrap();

        let five = subgroup_chart(&sample, 5, Spread::Range, TestSet::none()).unwrap();
        assert_eq!(five.dispersion_limits.lcl, 0.0);
        assert!(five.dispersion_limits.ucl > five.dispersion_limits.center);

        let seven = subgroup_chart(&sample, 7, Spread::Range, TestSet::none()).unwrap();
        let expected =
            (1.0 - 3.0 * d3(7).unwrap() / d2(7).unwrap()) * seven.dispersion_limits.center;
        assert!(seven.dispersion_limits.lcl > 0.0);
        assert!(
            (seven.dispersion_limits.lcl - expected).abs() < 1e-12,
            "LCL {} vs D3·R̄ {expected}",
            seven.dispersion_limits.lcl
        );
    }

    #[test]
    fn xbar_s_uses_the_standard_deviation_chart() {
        let out = analyze(&frame(), "weight", ControlChartKind::XbarS, 2).expect("chart");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Control Chart: X̄–S"));
        assert!(text.contains("Subgroup mean and standard deviation"));
        let plot = out.plot.expect("plot");
        assert_eq!(plot.panels.len(), 2);
        assert!(plot.panels[1].title.contains("Sample standard deviation"));
    }

    #[test]
    fn ewma_chart_smooths_the_series_and_keeps_limits() {
        let sample = Sample::new(
            vec![10.0, 10.2, 10.1, 12.0, 12.2, 12.1, 12.3, 12.2],
            crate::data::MissingPolicy::Drop,
        )
        .unwrap();
        let chart = ewma_chart(&sample).expect("ewma");
        assert_eq!(chart.kind, ControlChartKind::Ewma);
        assert_eq!(chart.location.len(), 8);
        assert!(chart.sigma > 0.0);
        assert!(chart.location_limits.ucl > chart.location_limits.center);
        assert!(chart.location_limits.center < chart.location_limits.lcl + 1.0);
    }

    fn rule_limits() -> Limits {
        Limits {
            center: 0.0,
            lcl: -3.0,
            ucl: 3.0,
        }
    }

    fn rule_signals(values: &[f64], chart: SignalChart, sigma: Option<f64>) -> Vec<Signal> {
        run_tests(values, &rule_limits(), chart, 1, sigma, TestSet::all())
    }

    fn tests_of(signals: &[Signal]) -> Vec<u8> {
        let mut tests: Vec<u8> = signals.iter().map(|signal| signal.test).collect();
        tests.sort_unstable();
        tests.dedup();
        tests
    }

    #[test]
    fn test_five_needs_two_of_three_beyond_two_sigma() {
        // Two of three points beyond 2σ on the same side signal on the third
        // point of the window.
        let signals = rule_signals(&[0.1, 2.5, 0.2, 2.4, 0.1], SignalChart::Location, Some(1.0));
        let five: Vec<&Signal> = signals.iter().filter(|signal| signal.test == 5).collect();
        assert_eq!(five.len(), 1, "{signals:?}");
        assert_eq!(five[0].index, 4);
        // One of three is not enough, and the opposite side does not count.
        let signals = rule_signals(
            &[0.1, 2.5, 0.2, 0.1, -2.4],
            SignalChart::Location,
            Some(1.0),
        );
        assert!(!tests_of(&signals).contains(&5), "{signals:?}");
    }

    #[test]
    fn test_six_needs_four_of_five_beyond_one_sigma() {
        let signals = rule_signals(
            &[1.2, 0.1, 1.3, 1.2, 1.4, 0.1],
            SignalChart::Location,
            Some(1.0),
        );
        let six: Vec<&Signal> = signals.iter().filter(|signal| signal.test == 6).collect();
        assert_eq!(six.len(), 1, "{signals:?}");
        assert_eq!(six[0].index, 5);
        // Three of five stays quiet.
        let signals = rule_signals(&[1.2, 0.1, 1.3, 0.2, 1.4], SignalChart::Location, Some(1.0));
        assert!(!tests_of(&signals).contains(&6), "{signals:?}");
    }

    #[test]
    fn test_seven_needs_fifteen_points_within_one_sigma() {
        let inside: Vec<f64> = (0..15)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        let signals = rule_signals(&inside, SignalChart::Location, Some(1.0));
        assert!(tests_of(&signals).contains(&7), "{signals:?}");
        assert_eq!(
            signals
                .iter()
                .find(|signal| signal.test == 7)
                .expect("test 7")
                .index,
            15
        );
        // Fourteen points are not enough.
        let signals = rule_signals(&inside[..14], SignalChart::Location, Some(1.0));
        assert!(!tests_of(&signals).contains(&7), "{signals:?}");
        // A point beyond one sigma breaks the run.
        let mut broken = inside.clone();
        broken[7] = 1.2;
        let signals = rule_signals(&broken, SignalChart::Location, Some(1.0));
        assert!(!tests_of(&signals).contains(&7), "{signals:?}");
    }

    #[test]
    fn test_eight_needs_eight_points_beyond_one_sigma() {
        let outside: Vec<f64> = (0..8)
            .map(|i| if i % 2 == 0 { 1.5 } else { -1.5 })
            .collect();
        let signals = rule_signals(&outside, SignalChart::Location, Some(1.0));
        assert!(tests_of(&signals).contains(&8), "{signals:?}");
        assert_eq!(
            signals
                .iter()
                .find(|signal| signal.test == 8)
                .expect("test 8")
                .index,
            8
        );
        // Seven points are not enough.
        let signals = rule_signals(&outside[..7], SignalChart::Location, Some(1.0));
        assert!(!tests_of(&signals).contains(&8), "{signals:?}");
        // A point inside one sigma breaks the run.
        let mut broken = outside.clone();
        broken[3] = 0.5;
        let signals = rule_signals(&broken, SignalChart::Location, Some(1.0));
        assert!(!tests_of(&signals).contains(&8), "{signals:?}");
    }

    #[test]
    fn dispersion_charts_and_missing_sigma_keep_tests_one_to_four() {
        let values = [0.1, 2.5, 0.2, 2.4, 0.1, 1.2, 0.1, 1.3, 0.2, 1.4, 1.1];
        let signals = rule_signals(&values, SignalChart::Dispersion, Some(1.0));
        assert!(signals.iter().all(|signal| signal.test <= 4), "{signals:?}");
        let signals = rule_signals(&values, SignalChart::Location, None);
        assert!(signals.iter().all(|signal| signal.test <= 4), "{signals:?}");
        let signals = rule_signals(&values, SignalChart::Location, Some(0.0));
        assert!(signals.iter().all(|signal| signal.test <= 4), "{signals:?}");
    }

    #[test]
    fn i_mr_reports_the_sigma_tests_in_the_session() {
        // A quiet baseline followed by a jump of three points: the chart
        // reports the two-sigma tests it can prove.
        let mut values: Vec<f64> = (0..40)
            .map(|index| if index % 2 == 0 { 9.9 } else { 10.1 })
            .collect();
        values.extend([10.6, 10.05, 10.65]);
        let df = DataFrame::new(vec![Column::new_f64("weight", values)]).expect("frame");
        let out = analyze_with_tests(
            &df,
            "weight",
            ControlChartKind::IndividualsMovingRange,
            1,
            TestSet::all(),
        )
        .expect("chart");
        let text = out.results[0].to_session_text();
        assert!(text.contains("2 (runs on one side)"), "{text}");
        assert!(text.contains("5 (2σ patterns)"), "{text}");
        assert!(text.contains("2 out of 3 points more than 2σ"), "{text}");
    }

    #[test]
    fn test_set_parses_and_reports_the_enabled_tests() {
        assert_eq!(TestSet::default(), TestSet::standard());
        assert_eq!(TestSet::standard().summary(), "1");
        assert_eq!(TestSet::all().summary(), "1–8");
        assert_eq!(TestSet::none().summary(), "none");
        assert_eq!(TestSet::only(&[1, 3, 5]).expect("set").summary(), "1, 3, 5");
        assert_eq!(TestSet::only(&[1, 2, 3]).expect("set").summary(), "1–3");
        assert_eq!(TestSet::parse_list("1:8").expect("set"), TestSet::all());
        assert_eq!(
            TestSet::parse_list("1,3,5").expect("set"),
            TestSet::parse_list("1 3 5").expect("set")
        );
        assert!(TestSet::only(&[0]).is_err());
        assert!(TestSet::only(&[9]).is_err());
        assert!(TestSet::parse_list("").is_err());
        assert!(TestSet::parse_list("5:2").is_err());
        assert!(TestSet::parse_list("1:x").is_err());
    }

    #[test]
    fn test_set_validates_the_documented_sensitivity_ranges() {
        let set = TestSet::all().with_sensitivity(2, 8).expect("test 2");
        assert_eq!(set.sensitivity(2), 8);
        assert!(set.is_enabled(2));
        assert!(TestSet::all().with_sensitivity(2, 6).is_err());
        assert!(TestSet::all().with_sensitivity(7, 16).is_err());
        assert!(TestSet::all().with_sensitivity(1, 2).is_err());
        assert!(TestSet::all().with_sensitivity(9, 3).is_err());
        let set = TestSet::all().set_enabled(4, false).expect("disable");
        assert!(!set.is_enabled(4));
        assert_eq!(set.summary(), "1–3, 5–8");
        let set = set.set_enabled(4, true).expect("enable");
        assert_eq!(set, TestSet::all());
    }

    #[test]
    fn a_custom_sensitivity_shortens_the_run() {
        // Seven points on one side fail test 2 at K = 7 but not at the default 9.
        let values = [1.0, 1.1, 0.9, 1.2, 1.1, 1.0, 1.3];
        let limits = Limits {
            center: 0.0,
            lcl: -100.0,
            ucl: 100.0,
        };
        let default = run_tests(
            &values,
            &limits,
            SignalChart::Location,
            1,
            Some(1.0),
            TestSet::only(&[2]).expect("set"),
        );
        assert!(default.is_empty(), "{default:?}");
        let shortened = TestSet::only(&[2])
            .expect("set")
            .with_sensitivity(2, 7)
            .expect("sensitivity");
        let signals = run_tests(
            &values,
            &limits,
            SignalChart::Location,
            1,
            Some(1.0),
            shortened,
        );
        assert_eq!(signals.len(), 1, "{signals:?}");
        assert_eq!(signals[0].test, 2);
    }

    #[test]
    fn disabled_tests_leave_the_chart_quiet() {
        let df = DataFrame::new(vec![Column::new_f64(
            "weight",
            vec![10.0, 10.1, 10.0, 10.1, 10.0, 10.1, 10.0, 10.1, 10.0, 15.0],
        )])
        .expect("frame");
        let out = analyze_with_tests(
            &df,
            "weight",
            ControlChartKind::IndividualsMovingRange,
            1,
            TestSet::none(),
        )
        .expect("chart");
        assert!(
            out.results[0]
                .to_session_text()
                .contains("No out-of-control points")
        );
        let out = analyze_with_tests(
            &df,
            "weight",
            ControlChartKind::IndividualsMovingRange,
            1,
            TestSet::only(&[1]).expect("set"),
        )
        .expect("chart");
        let text = out.results[0].to_session_text();
        assert!(text.contains("point outside a control limit"), "{text}");
        assert!(
            text.contains("1 (outside limits) on the location chart"),
            "{text}"
        );
    }

    #[test]
    fn invalid_subgroup_is_an_error() {
        assert!(matches!(
            analyze(&frame(), "weight", ControlChartKind::XbarR, 1),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn constant_process_is_rejected() {
        let df = DataFrame::new(vec![Column::new_f64("flat", vec![1.0; 5])]).unwrap();
        assert!(matches!(
            analyze(&df, "flat", ControlChartKind::IndividualsMovingRange, 1),
            Err(Error::ConstantSample)
        ));
    }
}

#[cfg(test)]
mod cusum_options {
    use super::*;
    use crate::data::MissingPolicy;

    fn shifted() -> Sample {
        let mut values = vec![10.0; 12];
        values.extend([11.2, 11.4, 11.3, 11.5, 11.4, 11.6, 11.5, 11.7]);
        Sample::new(values, MissingPolicy::Drop).expect("sample")
    }

    #[test]
    fn the_defaults_are_the_documented_k_and_h() {
        let chart = cusum_chart(&shifted(), 1).expect("chart");
        let options = chart.cusum.expect("cusum options");
        assert_eq!(options, CusumOptions::default());
        assert_eq!(options.reference, 0.5);
        assert_eq!(options.decision, 4.0);
        assert_eq!(chart.location_limits.ucl, 4.0);
    }

    #[test]
    fn a_custom_pair_moves_the_decision_interval_and_the_signal() {
        let sample = shifted();
        let wide = cusum_chart_with_options(&sample, 1, CusumOptions::default()).expect("default");
        let tight =
            cusum_chart_with_options(&sample, 1, CusumOptions::new(1.0, 3.0).expect("valid"))
                .expect("custom");
        assert_eq!(tight.location_limits.ucl, 3.0);
        assert_eq!(tight.cusum.expect("options").reference, 1.0);
        // The tighter chart signals no later than the default one.
        let first = |chart: &ControlChart| {
            chart
                .signals
                .iter()
                .filter(|signal| signal.chart == SignalChart::Location)
                .map(|signal| signal.index)
                .min()
        };
        assert!(first(&tight).is_some(), "the tight chart must signal");
        assert!(first(&tight) <= first(&wide));
        let text = report("weight", &tight).to_session_text();
        assert!(text.contains("k = 1σ"), "{text}");
        assert!(text.contains("h = 3σ"), "{text}");
    }

    #[test]
    fn an_invalid_pair_is_rejected() {
        assert!(CusumOptions::new(0.0, 5.0).is_err());
        assert!(CusumOptions::new(-0.5, 5.0).is_err());
        assert!(CusumOptions::new(0.5, 0.0).is_err());
        assert!(CusumOptions::new(f64::NAN, 5.0).is_err());
        assert!(CusumOptions::new(0.5, f64::INFINITY).is_err());
        let sample = shifted();
        assert!(
            cusum_chart_with_options(
                &sample,
                1,
                CusumOptions {
                    reference: 0.0,
                    decision: 5.0
                }
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod ewma_options {
    use super::*;
    use crate::data::MissingPolicy;

    fn sample() -> Sample {
        let mut values = vec![10.0, 10.2, 9.9, 10.1, 10.0, 10.3, 9.8, 10.2];
        values.extend([10.9, 11.1, 11.0, 11.2]);
        Sample::new(values, MissingPolicy::Drop).expect("sample")
    }

    #[test]
    fn the_defaults_are_lambda_point_two_and_three_sigma() {
        let chart = ewma_chart(&sample()).expect("chart");
        let options = chart.ewma.expect("ewma options");
        assert_eq!(options, EwmaOptions::default());
        assert_eq!(options.lambda, 0.2);
        assert_eq!(options.limit, 3.0);
        let expected = chart.location_limits.center + 3.0 * chart.sigma * (0.2 / 1.8f64).sqrt();
        assert!((chart.location_limits.ucl - expected).abs() < 1e-12);
    }

    #[test]
    fn a_custom_pair_changes_the_limits_and_the_note() {
        let sample = sample();
        let default = ewma_chart(&sample).expect("default");
        let custom = ewma_chart_with_options(
            &sample,
            TestSet::default(),
            EwmaOptions::new(0.4, 2.5).expect("valid"),
        )
        .expect("custom");
        assert_eq!(custom.ewma.expect("options").lambda, 0.4);
        assert_eq!(custom.ewma.expect("options").limit, 2.5);
        // The asymptotic half-width is L·σ·sqrt(λ/(2−λ)).
        let expected = 2.5 * custom.sigma * (0.4 / 1.6f64).sqrt();
        let custom_width = custom.location_limits.ucl - custom.location_limits.center;
        assert!((custom_width - expected).abs() < 1e-12);
        let default_width = default.location_limits.ucl - default.location_limits.center;
        assert!(
            custom_width > default_width,
            "{custom_width} vs {default_width}"
        );
        let text = report("weight", &custom).to_session_text();
        assert!(text.contains("λ = 0.4"), "{text}");
        assert!(text.contains("L = 2.5σ"), "{text}");
    }

    #[test]
    fn an_invalid_pair_is_rejected() {
        assert!(EwmaOptions::new(0.0, 3.0).is_err());
        assert!(EwmaOptions::new(1.5, 3.0).is_err());
        assert!(EwmaOptions::new(0.2, 0.0).is_err());
        assert!(EwmaOptions::new(f64::NAN, 3.0).is_err());
        assert!(EwmaOptions::new(0.2, f64::INFINITY).is_err());
        assert!(
            ewma_chart_with_options(
                &sample(),
                TestSet::default(),
                EwmaOptions {
                    lambda: 0.0,
                    limit: 3.0
                }
            )
            .is_err()
        );
    }
}
