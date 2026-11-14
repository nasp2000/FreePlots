// SPDX-License-Identifier: MIT
//! Plots: from statistics to primitives, and from primitives to SVG.
//!
//! Architecture (PLAN.md, section 3 / D8): the point computation does not know
//! how to draw, and the drawing does not know statistics. The same [`PlotSpec`]
//! feeds the screen (`egui`), the SVG and the PNG. Here lives the UI-independent
//! part, which is also the part the tests can verify without a screen.

use crate::data::Sample;
use crate::dists::{
    Distribution, Exponential, ExtremeValue, Gamma, Logistic, Loglogistic, Lognormal, Lognormal3,
    Normal, Weibull, Weibull3,
};
use crate::error::{Error, Result};
use crate::special::{normal_cdf, normal_quantile};

/// Most points one plot draws for a series.
///
/// A million-point worksheet is normal, but a million SVG elements freeze the
/// renderer. Larger series are thinned by a constant stride and the report
/// states the sample; the statistics always use every observation.
pub const MAX_PLOT_POINTS: usize = 5_000;

/// Smallest stride that keeps `len` points at or below [`MAX_PLOT_POINTS`].
#[must_use]
pub fn plot_stride(len: usize) -> usize {
    len.div_ceil(MAX_PLOT_POINTS).max(1)
}

/// Plotting position convention.
///
/// All formulas belong to the family `(i − a) / (n + 1 − 2a)`, with `i` from 1
/// to `n`. The documentation of each one is in the [`Self::formula`] method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlottingPosition {
    /// `(i − 0.3) / (n + 0.4)` — approximation of median ranks (Benard). Default.
    #[default]
    Benard,
    /// `(i − 0.375) / (n + 0.25)` — Blom.
    Blom,
    /// `(i − 0.5) / n` — Hazen.
    Hazen,
    /// `i / (n + 1)` — Weibull (mean ranks).
    Weibull,
    /// `(i − 0.4) / (n + 0.2)` — Cunnane.
    Cunnane,
}

impl PlottingPosition {
    /// Readable name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Benard => "Benard (median ranks)",
            Self::Blom => "Blom",
            Self::Hazen => "Hazen",
            Self::Weibull => "Weibull",
            Self::Cunnane => "Cunnane",
        }
    }

    /// Formula, for the report.
    #[must_use]
    pub fn formula(self) -> &'static str {
        match self {
            Self::Benard => "(i − 0.3) / (n + 0.4)",
            Self::Blom => "(i − 0.375) / (n + 0.25)",
            Self::Hazen => "(i − 0.5) / n",
            Self::Weibull => "i / (n + 1)",
            Self::Cunnane => "(i − 0.4) / (n + 0.2)",
        }
    }

    /// Plotting position value for observation `i` (1-based).
    pub fn value(self, i: usize, n: usize) -> Result<f64> {
        if n == 0 || i == 0 || i > n {
            return Err(Error::InvalidParameter {
                name: "i/n",
                value: i as f64,
            });
        }
        let (i, n) = (i as f64, n as f64);
        Ok(match self {
            Self::Benard => (i - 0.3) / (n + 0.4),
            Self::Blom => (i - 0.375) / (n + 0.25),
            Self::Hazen => (i - 0.5) / n,
            Self::Weibull => i / (n + 1.0),
            Self::Cunnane => (i - 0.4) / (n + 0.2),
        })
    }
}

/// Plotting positions for `n` observations.
pub fn positions(n: usize, method: PlottingPosition) -> Result<Vec<f64>> {
    if n == 0 {
        return Err(Error::EmptySample);
    }
    (1..=n).map(|i| method.value(i, n)).collect()
}

/// Computed data of a probability plot (without any visual decision).
#[derive(Debug, Clone, PartialEq)]
pub struct ProbabilityPlot {
    /// Positions method used.
    pub method: PlottingPosition,
    /// Cumulative probabilities (plotting positions).
    pub positions: Vec<f64>,
    /// Sorted observations.
    pub observed: Vec<f64>,
    /// Theoretical quantiles of the fitted distribution, on the data scale
    /// (`µ + σz` for the Normal). Plotting uses [`Self::positions`] directly.
    pub theoretical: Vec<f64>,
}

/// Computes the points of a Normal probability plot.
pub fn normal_probability_plot(
    sample: &Sample,
    dist: &Normal,
    method: PlottingPosition,
) -> Result<ProbabilityPlot> {
    let n = sample.n();
    let positions = positions(n, method)?;
    let observed = sample.sorted();
    let theoretical = positions
        .iter()
        .map(|&p| dist.quantile(p))
        .collect::<Vec<_>>();
    Ok(ProbabilityPlot {
        method,
        positions,
        observed,
        theoretical,
    })
}

/// Computes the points of a log-normal probability plot.
///
/// The plotting positions are the same; the fitted quantiles are returned on
/// the original scale (`exp(µ + σz)`). The plot itself uses `log10(x)`.
pub fn lognormal_probability_plot(
    sample: &Sample,
    dist: &Lognormal,
    method: PlottingPosition,
) -> Result<ProbabilityPlot> {
    let n = sample.n();
    let positions = positions(n, method)?;
    let observed = sample.sorted();
    let theoretical = positions
        .iter()
        .map(|&p| dist.quantile(p))
        .collect::<Vec<_>>();
    Ok(ProbabilityPlot {
        method,
        positions,
        observed,
        theoretical,
    })
}

/// Computes the points of a Weibull probability plot.
pub fn weibull_probability_plot(
    sample: &Sample,
    dist: &Weibull,
    method: PlottingPosition,
) -> Result<ProbabilityPlot> {
    let n = sample.n();
    let positions = positions(n, method)?;
    let observed = sample.sorted();
    let theoretical = positions
        .iter()
        .map(|&p| dist.quantile(p))
        .collect::<Vec<_>>();
    Ok(ProbabilityPlot {
        method,
        positions,
        observed,
        theoretical,
    })
}

/// Computes the points of an exponential probability plot.
pub fn exponential_probability_plot(
    sample: &Sample,
    dist: &Exponential,
    method: PlottingPosition,
) -> Result<ProbabilityPlot> {
    let n = sample.n();
    let positions = positions(n, method)?;
    let observed = sample.sorted();
    let theoretical = positions
        .iter()
        .map(|&p| dist.quantile(p))
        .collect::<Vec<_>>();
    Ok(ProbabilityPlot {
        method,
        positions,
        observed,
        theoretical,
    })
}

/// Computes the points of a logistic probability plot.
pub fn logistic_probability_plot(
    sample: &Sample,
    dist: &Logistic,
    method: PlottingPosition,
) -> Result<ProbabilityPlot> {
    probability_points(sample, dist, method)
}

/// Computes the points of a log-logistic probability plot.
pub fn loglogistic_probability_plot(
    sample: &Sample,
    dist: &Loglogistic,
    method: PlottingPosition,
) -> Result<ProbabilityPlot> {
    probability_points(sample, dist, method)
}

/// Computes the points of a gamma probability plot.
pub fn gamma_probability_plot(
    sample: &Sample,
    dist: &Gamma,
    method: PlottingPosition,
) -> Result<ProbabilityPlot> {
    probability_points(sample, dist, method)
}

/// Computes the points of a three-parameter log-normal probability plot.
pub fn lognormal3_probability_plot(
    sample: &Sample,
    dist: &Lognormal3,
    method: PlottingPosition,
) -> Result<ProbabilityPlot> {
    probability_points(sample, dist, method)
}

/// Computes the points of a three-parameter Weibull probability plot.
pub fn weibull3_probability_plot(
    sample: &Sample,
    dist: &Weibull3,
    method: PlottingPosition,
) -> Result<ProbabilityPlot> {
    probability_points(sample, dist, method)
}

/// Computes the points of an extreme-value probability plot.
pub fn extreme_value_probability_plot(
    sample: &Sample,
    dist: &ExtremeValue,
    method: PlottingPosition,
) -> Result<ProbabilityPlot> {
    probability_points(sample, dist, method)
}

fn probability_points<D: Distribution>(
    sample: &Sample,
    dist: &D,
    method: PlottingPosition,
) -> Result<ProbabilityPlot> {
    let n = sample.n();
    let positions = positions(n, method)?;
    let observed = sample.sorted();
    let theoretical = positions
        .iter()
        .map(|&p| dist.quantile(p))
        .collect::<Vec<_>>();
    Ok(ProbabilityPlot {
        method,
        positions,
        observed,
        theoretical,
    })
}

/// Plotted dots of a probability plot, thinned to [`MAX_PLOT_POINTS`].
///
/// The axis ranges, the fitted line and the confidence band still use every
/// observation; only the dots are sampled.
fn sampled_probability_points(
    positions: &[f64],
    observed: &[f64],
    mut point: impl FnMut(f64, f64) -> Point,
) -> Vec<Point> {
    let stride = plot_stride(positions.len());
    positions
        .iter()
        .zip(observed)
        .step_by(stride)
        .map(|(&p, &x)| point(p, x))
        .collect()
}

/// A note that states the sampling, when the plot was thinned.
pub(crate) fn sampling_notes(len: usize) -> Vec<PlotNote> {
    let stride = plot_stride(len);
    if stride == 1 {
        return Vec::new();
    }
    vec![PlotNote {
        rows: Vec::new(),
        lines: vec![format!(
            "one in every {stride} observations is drawn ({} of {len})",
            len.div_ceil(stride)
        )],
    }]
}

/// A point in axis coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// x coordinate.
    pub x: f64,
    /// y coordinate.
    pub y: f64,
}

/// A segment in axis coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// Start.
    pub from: Point,
    /// End.
    pub to: Point,
    /// Thickness in pixels.
    pub width: f64,
}

/// A filled confidence ribbon, as two boundaries in axis coordinates.
///
/// `lower` and `upper` have the same length and are ordered along the vertical
/// axis; each pair bounds the interval at that vertical coordinate.
#[derive(Debug, Clone, PartialEq)]
pub struct Band {
    /// Lower boundary, ordered by `y`.
    pub lower: Vec<Point>,
    /// Upper boundary, ordered by `y`.
    pub upper: Vec<Point>,
}

/// One labelled series in an overlay plot.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    /// Legend label (column or group).
    pub label: String,
    /// Observations, in axis coordinates.
    pub points: Vec<Point>,
    /// Fitted/reference lines, in axis coordinates.
    pub lines: Vec<Segment>,
    /// Confidence ribbons, in axis coordinates.
    pub bands: Vec<Band>,
    /// Whether the series is drawn on the right-hand axis.
    pub secondary_axis: bool,
}

/// Probability-axis transform family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbabilityScale {
    /// `z = Φ⁻¹(p)`.
    Normal,
    /// `y = ln(−ln(1 − p))` (Gumbel reduced variate; Weibull/exponential).
    Gumbel,
    /// `y = −ln(−ln(p))` (extreme-value, maximum domain).
    ExtremeValue,
    /// `y = ln(p / (1 − p))` (logit; logistic).
    Logit,
}

pub(crate) fn probability_position(scale: ProbabilityScale, p: f64) -> f64 {
    match scale {
        ProbabilityScale::Normal => normal_quantile(p),
        ProbabilityScale::Gumbel => (-(1.0 - p).ln()).ln(),
        ProbabilityScale::ExtremeValue => -((-p.ln()).ln()),
        ProbabilityScale::Logit => (p / (1.0 - p)).ln(),
    }
}

/// Axis scale. Coordinates in [`AxisSpec`] are already transformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    /// Raw values.
    Linear,
    /// `log10(value)`.
    Log10,
    /// Probability transformed by the family's quantile function.
    Probability(ProbabilityScale),
}

/// One tick, positioned in axis coordinates and already formatted.
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    /// Position in the axis coordinate system.
    pub position: f64,
    /// Label shown to the user.
    pub label: String,
}

/// Axis description owned by the engine.
///
/// `min`, `max` and [`Tick::position`] are in the axis coordinate system: raw
/// values for [`Scale::Linear`], `log10` for [`Scale::Log10`], and the
/// family's quantile (e.g. `z` for Normal) for [`Scale::Probability`].
#[derive(Debug, Clone, PartialEq)]
pub struct AxisSpec {
    /// Axis title.
    pub label: String,
    /// Scale of the coordinates.
    pub scale: Scale,
    /// Lower limit, in axis coordinates.
    pub min: f64,
    /// Upper limit, in axis coordinates.
    pub max: f64,
    /// Ticks, strictly increasing and inside `[min, max]`.
    pub ticks: Vec<Tick>,
    /// Minor tick positions (no labels), strictly inside `[min, max]`.
    pub minor: Vec<f64>,
}

/// Complete specification of a plot, independent of the backend.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotSpec {
    /// Title.
    pub title: String,
    /// Horizontal axis.
    pub x_axis: AxisSpec,
    /// Vertical axis.
    pub y_axis: AxisSpec,
    /// Width and height in pixels.
    pub size: (u32, u32),
    /// Points, in axis coordinates.
    pub points: Vec<Point>,
    /// Lines (fit, reference), in axis coordinates.
    pub lines: Vec<Segment>,
    /// Filled confidence ribbons, in axis coordinates.
    pub bands: Vec<Band>,
    /// Overlay series. Empty for a single-series plot.
    pub series: Vec<Series>,
    /// Panels for a multi-panel plot (empty for a normal plot).
    pub panels: Vec<PlotSpec>,
    /// Number of panel columns when `panels` is not empty.
    pub panel_columns: usize,
    /// Optional right-hand axis for the series marked as secondary.
    pub secondary: Option<AxisSpec>,
    /// Compact statistics blocks drawn inside the frame (mean, Cp, Cpk…).
    pub notes: Vec<PlotNote>,
}

/// One label/value pair in a [`PlotNote`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NoteRow {
    /// Left column: what the value is (`Mean`, `Cpk`, `UCL`…).
    pub label: String,
    /// Right column: the value itself.
    pub value: String,
}

/// A compact statistics block drawn in the margin to the right of the plot
/// frame, so it never covers the data.
///
/// The label column is left-aligned and the value column right-aligned, so the
/// numbers line up the way they do in the Session report. The plot area shrinks
/// by the width of the widest block; the image keeps its own size.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlotNote {
    /// Label/value rows, top to bottom.
    pub rows: Vec<NoteRow>,
    /// Free-text lines drawn under the rows (units, warnings).
    pub lines: Vec<String>,
}

impl PlotNote {
    /// A note of label/value rows.
    #[must_use]
    pub fn new<S: Into<String>>(rows: Vec<(S, String)>) -> Self {
        Self {
            rows: rows
                .into_iter()
                .map(|(label, value)| NoteRow {
                    label: label.into(),
                    value,
                })
                .collect(),
            lines: Vec::new(),
        }
    }
}

/// Width of the longest line of a note in characters, used to reserve the
/// right margin in the SVG.
fn note_char_width(note: &PlotNote) -> usize {
    let labels = note
        .rows
        .iter()
        .map(|row| row.label.chars().count())
        .max()
        .unwrap_or(0);
    let values = note
        .rows
        .iter()
        .map(|row| row.value.chars().count())
        .max()
        .unwrap_or(0);
    let lines = note
        .lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    if note.rows.is_empty() {
        lines
    } else if note.lines.is_empty() {
        labels + 3 + values
    } else {
        (labels + 3 + values).max(lines)
    }
}

/// Right margin, in SVG units, reserved for all the notes of a plot.
fn note_gutter(notes: &[PlotNote], scale: f32) -> f64 {
    if notes.is_empty() {
        0.0
    } else {
        let chars = notes.iter().map(note_char_width).max().unwrap_or(0) as f64;
        // 6.5 px per character at 12 px, plus the gap to the frame and the
        // right margin of the image.
        let char_width = 6.5 * f64::from(scale.clamp(0.75, 1.5));
        12.0 + chars * char_width + 8.0
    }
}

/// Rebuilds a linear or log axis for a manual range, or returns `None` when
/// the scale cannot be expressed in raw values (probability axes).
fn rescaled_axis(axis: &AxisSpec, range: (f64, f64)) -> Option<AxisSpec> {
    let (low, high) = range;
    if !low.is_finite() || !high.is_finite() || low >= high {
        return None;
    }
    match axis.scale {
        Scale::Linear => Some(linear_axis(&axis.label, low, high)),
        Scale::Log10 => Some(log10_axis(axis.label.clone(), low, high)),
        Scale::Probability(_) => None,
    }
}

/// Percentage ticks used on a probability axis.
///
/// The normal/lognormal and the Weibull/exponential sets are the ones taken
/// from the documented behaviour (see the project notes); the decade anchors
/// at the ends are kept so an axis that expands past 1–99 % still has labels.
/// The Logit and ExtremeValue scales reuse the normal set: those families are
/// engine additions with no documented plot to record.
const NORMAL_PROBABILITY_TICKS_PCT: [f64; 23] = [
    0.000_01, 0.000_1, 0.001, 0.01, 0.1, 1.0, 5.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0,
    90.0, 95.0, 99.0, 99.9, 99.99, 99.999, 99.9999, 99.999_99,
];

/// Weibull/exponential probability ticks: the documented set adds 2 % and 3 % to
/// the normal one (the Gumbel axis compresses the upper tail).
const GUMBEL_PROBABILITY_TICKS_PCT: [f64; 25] = [
    0.000_01, 0.000_1, 0.001, 0.01, 0.1, 1.0, 2.0, 3.0, 5.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0,
    70.0, 80.0, 90.0, 95.0, 99.0, 99.9, 99.99, 99.999, 99.9999, 99.999_99,
];

fn fmt_percent(p: f64) -> String {
    let mut s = format!("{:.7}", p * 100.0);
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" {
        s = "0".to_owned();
    }
    s
}

fn fmt_linear(v: f64, step: f64) -> String {
    if v == 0.0 {
        return "0".to_owned();
    }
    let decimals = if step >= 1.0 {
        0
    } else {
        (-step.log10().floor()) as usize
    };
    let mut s = format!("{v:.decimals$}");
    if let Some(stripped) = s.strip_suffix(".0") {
        s = stripped.to_owned();
    }
    if s == "-0" {
        s = "0".to_owned();
    }
    s
}

fn fmt_log_value(v: f64) -> String {
    if v >= 1e5 || v < 1e-3 {
        return format!("{v:.0e}");
    }
    let mut s = format!("{v:.6}");
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

fn nice_step(min: f64, max: f64, target: usize) -> f64 {
    if !min.is_finite() || !max.is_finite() || max <= min || target == 0 {
        return 0.0;
    }
    let raw = (max - min) / target as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    // Nearest 1-2-5 step: a raw step of 5.3 units is closer to 5 than to 10,
    // so it yields the finer grid instead of a three-tick axis.
    mag * if norm < 1.5 {
        1.0
    } else if norm < 3.5 {
        2.0
    } else if norm < 7.5 {
        5.0
    } else {
        10.0
    }
}

fn linear_minor(min: f64, max: f64, target: usize) -> Vec<f64> {
    let step = nice_step(min, max, target);
    if step <= 0.0 {
        return Vec::new();
    }
    let minor_step = step / 5.0;
    let eps = minor_step * 1e-9;
    let mut value = (min / minor_step).ceil() * minor_step;
    let mut minor = Vec::new();
    while value <= max + eps && minor.len() < 200 {
        let position = if value.abs() < eps { 0.0 } else { value };
        // Skip the positions occupied by a major tick.
        if ((position / step) - (position / step).round()).abs() > 1e-9 {
            minor.push(position);
        }
        value += minor_step;
    }
    minor
}

fn linear_ticks(min: f64, max: f64, target: usize) -> Vec<Tick> {
    let step = nice_step(min, max, target);
    if step <= 0.0 {
        return Vec::new();
    }
    let eps = step * 1e-9;
    let mut v = (min / step).ceil() * step;
    let mut ticks = Vec::new();
    while v <= max + eps && ticks.len() < 100 {
        let position = if v.abs() < eps { 0.0 } else { v };
        ticks.push(Tick {
            position,
            label: fmt_linear(position, step),
        });
        v += step;
    }
    ticks
}

pub(crate) fn linear_axis(label: &str, lo: f64, hi: f64) -> AxisSpec {
    let (min, max) = if hi > lo { (lo, hi) } else { (lo, lo + 1.0) };
    let ticks = linear_ticks(min, max, 6);
    let minor = linear_minor(min, max, 6);
    AxisSpec {
        label: label.to_owned(),
        scale: Scale::Linear,
        min,
        max,
        ticks,
        minor,
    }
}

pub(crate) fn probability_axis(
    label: &str,
    p_lo: f64,
    p_hi: f64,
    scale: ProbabilityScale,
) -> AxisSpec {
    // The documented plots start at 1 % and expand by decades only when
    // the plotting positions fall outside; 0.1/99.9 appear at n = 200, for
    // example.
    let mut lower = 0.01; // 1 %
    let mut upper = 0.99; // 99 %
    while p_lo < lower && lower > 1e-12 {
        lower /= 10.0;
    }
    while p_hi > upper && upper < 1.0 - 1e-12 {
        upper = 1.0 - (1.0 - upper) / 10.0;
    }
    let min = probability_position(scale, lower);
    let max = probability_position(scale, upper);
    let ticks = probability_ticks(scale, min, max);
    AxisSpec {
        label: label.to_owned(),
        scale: Scale::Probability(scale),
        min,
        max,
        ticks,
        minor: Vec::new(),
    }
}

fn probability_ticks(scale: ProbabilityScale, min: f64, max: f64) -> Vec<Tick> {
    let pcts: &[f64] = if scale == ProbabilityScale::Gumbel {
        &GUMBEL_PROBABILITY_TICKS_PCT
    } else {
        &NORMAL_PROBABILITY_TICKS_PCT
    };
    pcts.iter()
        .map(|pct| pct / 100.0)
        .map(|p| {
            let z = probability_position(scale, p);
            Tick {
                position: if z.abs() < 1e-12 { 0.0 } else { z },
                label: fmt_percent(p),
            }
        })
        .filter(|tick| tick.position >= min - 1e-12 && tick.position <= max + 1e-12)
        .collect()
}

/// Base-10 log axis from positive raw values.
///
/// Coordinates in the returned [`AxisSpec`] are `log10(value)`; a plot that
/// uses it must transform its points the same way.
#[must_use]
pub fn log10_axis(label: impl Into<String>, min_value: f64, max_value: f64) -> AxisSpec {
    let lo = min_value.max(f64::MIN_POSITIVE);
    let hi = max_value.max(lo);
    let min = lo.log10();
    let max = hi.log10();

    // Narrow log spans (less than two decades) are far more readable with
    // ordinary 1-2-5 ticks placed on the log axis: 9.5, 10, 10.5, ... instead
    // of a single "10" tick. Wider spans use the decade/1/2/5 grid.
    if hi / lo < 100.0 {
        let ticks = linear_ticks(lo, hi, 6)
            .into_iter()
            .map(|t| Tick {
                position: t.position.log10(),
                label: t.label,
            })
            .collect();
        let minor = linear_minor(lo, hi, 6)
            .into_iter()
            .map(f64::log10)
            .collect();
        return AxisSpec {
            label: label.into(),
            scale: Scale::Log10,
            min,
            max,
            ticks,
            minor,
        };
    }

    let k0 = min.floor().clamp(-300.0, 300.0) as i32;
    let k1 = max.ceil().clamp(-300.0, 300.0) as i32;
    let mut ticks = Vec::new();
    for k in k0..=k1 {
        let decade = 10f64.powi(k);
        for mult in [1.0, 2.0, 5.0] {
            let v = decade * mult;
            if v >= lo * (1.0 - 1e-12) && v <= hi * (1.0 + 1e-12) {
                ticks.push(Tick {
                    position: v.log10(),
                    label: fmt_log_value(v),
                });
            }
        }
    }
    ticks.sort_by(|a, b| a.position.total_cmp(&b.position));
    ticks.dedup_by(|a, b| (a.position - b.position).abs() < 1e-12);
    let mut minor = Vec::new();
    for k in k0..=k1 {
        let decade = 10f64.powi(k);
        for mult in [3.0, 4.0, 6.0, 7.0, 8.0, 9.0] {
            let v = decade * mult;
            if v >= lo * (1.0 - 1e-12) && v <= hi * (1.0 + 1e-12) {
                minor.push(v.log10());
            }
        }
    }
    minor.sort_by(f64::total_cmp);
    minor.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    AxisSpec {
        label: label.into(),
        scale: Scale::Log10,
        min,
        max,
        ticks,
        minor,
    }
}

impl PlotSpec {
    /// `true` when the plot or any of its panels has points that data labels
    /// and percentile lines can use.
    #[must_use]
    pub fn has_points(&self) -> bool {
        !self.points.is_empty() || self.panels.iter().any(Self::has_points)
    }

    /// Specification of a Normal probability plot, in the documented behaviour's orientation.
    ///
    /// Data on the horizontal axis, probability on the vertical axis. The
    /// vertical coordinates are `z = Φ⁻¹(p)`, so the fitted line is
    /// `z = (x − µ)/σ` and passes through `(µ, 0)`. The line is the MLE model —
    /// never a regression on the probability scale, which would give parameters
    /// different from those reported.
    pub fn normal_probability(
        plot: &ProbabilityPlot,
        dist: &Normal,
        title: impl Into<String>,
    ) -> Self {
        Self::normal_probability_with_ci(plot, dist, title, Some(0.95))
    }

    /// Specification of a Normal probability plot with an explicit band.
    ///
    /// `confidence = None` draws no band. The fitted distribution is expected
    /// to be the sample-moment `Normal` (mean and `s` with `n − 1`), as in
    /// the documented behaviour's probability-plot path; the band uses `Var(s) = s²(1 − c4²)`.
    pub fn normal_probability_with_ci(
        plot: &ProbabilityPlot,
        dist: &Normal,
        title: impl Into<String>,
        confidence: Option<f64>,
    ) -> Self {
        let x_lo = plot.observed.iter().copied().fold(f64::INFINITY, f64::min);
        let x_hi = plot
            .observed
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let pad = 0.05 * ((x_hi - x_lo).abs().max(f64::MIN_POSITIVE));
        let x_min = x_lo - pad;
        let x_max = x_hi + pad;

        let p_lo = plot.positions.iter().copied().fold(f64::INFINITY, f64::min);
        let p_hi = plot
            .positions
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let y_axis = probability_axis("Percent", p_lo, p_hi, ProbabilityScale::Normal);

        let points = sampled_probability_points(&plot.positions, &plot.observed, |p, x| Point {
            x,
            y: normal_quantile(p),
        });

        let z = |x: f64| (x - dist.mu()) / dist.sigma();
        let x_start = (dist.mu() + dist.sigma() * y_axis.min).max(x_min);
        let x_end = (dist.mu() + dist.sigma() * y_axis.max).min(x_max);
        let lines = if x_start < x_end {
            vec![Segment {
                from: Point {
                    x: x_start,
                    y: z(x_start),
                },
                to: Point {
                    x: x_end,
                    y: z(x_end),
                },
                width: 2.0,
            }]
        } else {
            Vec::new()
        };
        let bands = normal_ci_band(
            dist,
            plot.positions.len(),
            y_axis.min,
            y_axis.max,
            confidence,
        );

        Self {
            title: title.into(),
            x_axis: linear_axis("Data", x_min, x_max),
            y_axis,
            size: (900, 620),
            points,
            lines,
            bands,
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: sampling_notes(plot.positions.len()),
        }
    }

    /// Specification of a log-normal probability plot, in the documented behaviour's orientation.
    ///
    /// Data on a base-10 log axis, probability on the normal scale. The fitted
    /// line is `z = (ln x − µ)/σ`; in `u = log10(x)` coordinates its slope is
    /// `ln(10)/σ`. The confidence band is computed on the log scale with the
    /// same `s`/`c4` construction as the normal plot, then mapped back to the
    /// log axis.
    pub fn lognormal_probability(
        plot: &ProbabilityPlot,
        dist: &Lognormal,
        title: impl Into<String>,
    ) -> Self {
        Self::lognormal_probability_with_ci(plot, dist, title, Some(0.95))
    }

    /// Log-normal plot with an explicit band (`None` disables it).
    pub fn lognormal_probability_with_ci(
        plot: &ProbabilityPlot,
        dist: &Lognormal,
        title: impl Into<String>,
        confidence: Option<f64>,
    ) -> Self {
        let x_lo = plot.observed.iter().copied().fold(f64::INFINITY, f64::min);
        let x_hi = plot
            .observed
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let u_lo = x_lo.log10();
        let u_hi = x_hi.log10();
        let pad = 0.05 * ((u_hi - u_lo).abs().max(f64::MIN_POSITIVE));
        let u_min = u_lo - pad;
        let u_max = u_hi + pad;

        let p_lo = plot.positions.iter().copied().fold(f64::INFINITY, f64::min);
        let p_hi = plot
            .positions
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let y_axis = probability_axis("Percent", p_lo, p_hi, ProbabilityScale::Normal);

        let points = sampled_probability_points(&plot.positions, &plot.observed, |p, x| Point {
            x: x.log10(),
            y: normal_quantile(p),
        });

        let ln10 = std::f64::consts::LN_10;
        let z = |u: f64| (u * ln10 - dist.mu()) / dist.sigma();
        let u_start = ((dist.mu() + dist.sigma() * y_axis.min) / ln10).max(u_min);
        let u_end = ((dist.mu() + dist.sigma() * y_axis.max) / ln10).min(u_max);
        let lines = if u_start < u_end {
            vec![Segment {
                from: Point {
                    x: u_start,
                    y: z(u_start),
                },
                to: Point {
                    x: u_end,
                    y: z(u_end),
                },
                width: 2.0,
            }]
        } else {
            Vec::new()
        };
        let bands = location_scale_ci_band(
            dist.mu(),
            dist.sigma(),
            plot.positions.len(),
            y_axis.min,
            y_axis.max,
            confidence,
            |value| value / ln10,
        );

        Self {
            title: title.into(),
            x_axis: log10_axis("Data", 10f64.powf(u_min), 10f64.powf(u_max)),
            y_axis,
            size: (900, 620),
            points,
            lines,
            bands,
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: sampling_notes(plot.positions.len()),
        }
    }

    /// Specification of a Weibull probability plot.
    ///
    /// Data on a base-10 log axis, probability on the Weibull scale
    /// `y = ln(−ln(1 − p))`. The fitted line is
    /// `y = α·ln(x) − α·ln(β)`; in `u = log10(x)` coordinates its slope is
    /// `α·ln(10)`. The 95 % band is the delta method on the `log10` axis
    /// coordinate, with the covariance from the numerical Hessian of the
    /// log-likelihood; the documented plot matches it within 0.3 % at
    /// p = 20–80 % (see the project notes).
    pub fn weibull_probability(
        plot: &ProbabilityPlot,
        dist: &Weibull,
        title: impl Into<String>,
    ) -> Self {
        Self::weibull_probability_with_ci(plot, dist, title, Some(0.95))
    }

    /// Weibull plot with an explicit band (`None` disables it).
    ///
    /// The band comes from the covariance of the maximum-likelihood estimate,
    /// which is obtained from the numerical Hessian of the log-likelihood.
    pub fn weibull_probability_with_ci(
        plot: &ProbabilityPlot,
        dist: &Weibull,
        title: impl Into<String>,
        confidence: Option<f64>,
    ) -> Self {
        let x_lo = plot.observed.iter().copied().fold(f64::INFINITY, f64::min);
        let x_hi = plot
            .observed
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let u_lo = x_lo.log10();
        let u_hi = x_hi.log10();
        let pad = 0.05 * ((u_hi - u_lo).abs().max(f64::MIN_POSITIVE));
        let u_min = u_lo - pad;
        let u_max = u_hi + pad;

        let p_lo = plot.positions.iter().copied().fold(f64::INFINITY, f64::min);
        let p_hi = plot
            .positions
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let y_axis = probability_axis("Percent", p_lo, p_hi, ProbabilityScale::Gumbel);

        let points = sampled_probability_points(&plot.positions, &plot.observed, |p, x| Point {
            x: x.log10(),
            y: probability_position(ProbabilityScale::Gumbel, p),
        });

        let ln10 = std::f64::consts::LN_10;
        let alpha = dist.shape();
        let offset = alpha * dist.scale().ln();
        let y = |u: f64| alpha * ln10 * u - offset;
        let u_start = ((y_axis.min + offset) / (alpha * ln10)).max(u_min);
        let u_end = ((y_axis.max + offset) / (alpha * ln10)).min(u_max);
        let lines = if u_start < u_end {
            vec![Segment {
                from: Point {
                    x: u_start,
                    y: y(u_start),
                },
                to: Point {
                    x: u_end,
                    y: y(u_end),
                },
                width: 2.0,
            }]
        } else {
            Vec::new()
        };

        let bands = mle_ci_band(
            dist,
            &plot.observed,
            p_lo,
            p_hi,
            confidence,
            |q: f64| q.log10(),
            |p| probability_position(ProbabilityScale::Gumbel, p),
        );

        Self {
            title: title.into(),
            x_axis: log10_axis("Data", 10f64.powf(u_min), 10f64.powf(u_max)),
            y_axis,
            size: (900, 620),
            points,
            lines,
            bands,
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: sampling_notes(plot.positions.len()),
        }
    }

    /// Specification of an exponential probability plot.
    ///
    /// Data on a base-10 log axis, probability on the Gumbel scale
    /// `y = ln(−ln(1 − p))`. The fitted line is `y = ln(x) − ln(θ)`; in
    /// `u = log10(x)` coordinates its slope is `ln(10)` and it passes through
    /// `(log10 θ, 0)`, the 63.2 % quantile. No confidence band is drawn: the
    /// the documented behaviour's covariance construction is not recovered.
    pub fn exponential_probability(
        plot: &ProbabilityPlot,
        dist: &Exponential,
        title: impl Into<String>,
    ) -> Self {
        let x_lo = plot.observed.iter().copied().fold(f64::INFINITY, f64::min);
        let x_hi = plot
            .observed
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let u_lo = x_lo.log10();
        let u_hi = x_hi.log10();
        let pad = 0.05 * ((u_hi - u_lo).abs().max(f64::MIN_POSITIVE));
        let u_min = u_lo - pad;
        let u_max = u_hi + pad;

        let p_lo = plot.positions.iter().copied().fold(f64::INFINITY, f64::min);
        let p_hi = plot
            .positions
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let y_axis = probability_axis("Percent", p_lo, p_hi, ProbabilityScale::Gumbel);

        let points = sampled_probability_points(&plot.positions, &plot.observed, |p, x| Point {
            x: x.log10(),
            y: probability_position(ProbabilityScale::Gumbel, p),
        });

        let ln10 = std::f64::consts::LN_10;
        let offset = dist.theta().ln();
        let y = |u: f64| ln10 * u - offset;
        let u_start = ((y_axis.min + offset) / ln10).max(u_min);
        let u_end = ((y_axis.max + offset) / ln10).min(u_max);
        let lines = if u_start < u_end {
            vec![Segment {
                from: Point {
                    x: u_start,
                    y: y(u_start),
                },
                to: Point {
                    x: u_end,
                    y: y(u_end),
                },
                width: 2.0,
            }]
        } else {
            Vec::new()
        };

        Self {
            title: title.into(),
            x_axis: log10_axis("Data", 10f64.powf(u_min), 10f64.powf(u_max)),
            y_axis,
            size: (900, 620),
            points,
            lines,
            bands: Vec::new(),
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: sampling_notes(plot.positions.len()),
        }
    }

    /// Specification of a logistic probability plot.
    ///
    /// Data on a linear axis, probability on the logit scale
    /// `y = ln(p / (1 − p))`. The fitted line is `y = (x − µ)/σ`. The
    /// confidence band comes from the covariance of the maximum-likelihood
    /// estimate, which is obtained here from the numerical Hessian of the
    /// log-likelihood.
    pub fn logistic_probability(
        plot: &ProbabilityPlot,
        dist: &Logistic,
        title: impl Into<String>,
    ) -> Self {
        Self::logistic_probability_with_ci(plot, dist, title, Some(0.95))
    }

    /// Logistic plot with an explicit band (`None` disables it).
    ///
    /// The band comes from the covariance of the maximum-likelihood estimate,
    /// which is obtained from the numerical Hessian of the log-likelihood.
    pub fn logistic_probability_with_ci(
        plot: &ProbabilityPlot,
        dist: &Logistic,
        title: impl Into<String>,
        confidence: Option<f64>,
    ) -> Self {
        let x_lo = plot.observed.iter().copied().fold(f64::INFINITY, f64::min);
        let x_hi = plot
            .observed
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let pad = 0.05 * ((x_hi - x_lo).abs().max(f64::MIN_POSITIVE));
        let x_min = x_lo - pad;
        let x_max = x_hi + pad;

        let p_lo = plot.positions.iter().copied().fold(f64::INFINITY, f64::min);
        let p_hi = plot
            .positions
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let y_axis = probability_axis("Percent", p_lo, p_hi, ProbabilityScale::Logit);

        let points = sampled_probability_points(&plot.positions, &plot.observed, |p, x| Point {
            x,
            y: probability_position(ProbabilityScale::Logit, p),
        });

        let z = |x: f64| (x - dist.mu()) / dist.sigma();
        let x_start = (dist.mu() + dist.sigma() * y_axis.min).max(x_min);
        let x_end = (dist.mu() + dist.sigma() * y_axis.max).min(x_max);
        let lines = if x_start < x_end {
            vec![Segment {
                from: Point {
                    x: x_start,
                    y: z(x_start),
                },
                to: Point {
                    x: x_end,
                    y: z(x_end),
                },
                width: 2.0,
            }]
        } else {
            Vec::new()
        };

        let bands = mle_ci_band(
            dist,
            &plot.observed,
            p_lo,
            p_hi,
            confidence,
            |q: f64| q,
            |p| probability_position(ProbabilityScale::Logit, p),
        );

        Self {
            title: title.into(),
            x_axis: linear_axis("Data", x_min, x_max),
            y_axis,
            size: (900, 620),
            points,
            lines,
            bands,
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: sampling_notes(plot.positions.len()),
        }
    }

    /// Specification of a log-logistic probability plot.
    ///
    /// Data on a base-10 log axis, probability on the Gumbel scale
    /// `y = ln(−ln(1 − p))`. With `z = β·ln(x/α)`, the fitted line is
    /// `y = (ln(10)·u − ln α)/σ`, where `u = log10(x)` and `σ = 1/β`.
    pub fn loglogistic_probability(
        plot: &ProbabilityPlot,
        dist: &Loglogistic,
        title: impl Into<String>,
    ) -> Self {
        Self::loglogistic_probability_with_ci(plot, dist, title, Some(0.95))
    }

    /// Log-logistic plot with an explicit band (`None` disables it).
    ///
    /// The band comes from the covariance of the maximum-likelihood estimate,
    /// which is obtained from the numerical Hessian of the log-likelihood.
    pub fn loglogistic_probability_with_ci(
        plot: &ProbabilityPlot,
        dist: &Loglogistic,
        title: impl Into<String>,
        confidence: Option<f64>,
    ) -> Self {
        let x_lo = plot.observed.iter().copied().fold(f64::INFINITY, f64::min);
        let x_hi = plot
            .observed
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let u_lo = x_lo.log10();
        let u_hi = x_hi.log10();
        let pad = 0.05 * ((u_hi - u_lo).abs().max(f64::MIN_POSITIVE));
        let u_min = u_lo - pad;
        let u_max = u_hi + pad;

        let p_lo = plot.positions.iter().copied().fold(f64::INFINITY, f64::min);
        let p_hi = plot
            .positions
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let y_axis = probability_axis("Percent", p_lo, p_hi, ProbabilityScale::Gumbel);

        let points = sampled_probability_points(&plot.positions, &plot.observed, |p, x| Point {
            x: x.log10(),
            y: probability_position(ProbabilityScale::Gumbel, p),
        });

        let ln10 = std::f64::consts::LN_10;
        let mu = dist.mu();
        let sigma = dist.sigma();
        let y = |u: f64| (ln10 * u - mu) / sigma;
        let u_start = ((y_axis.min * sigma + mu) / ln10).max(u_min);
        let u_end = ((y_axis.max * sigma + mu) / ln10).min(u_max);
        let lines = if u_start < u_end {
            vec![Segment {
                from: Point {
                    x: u_start,
                    y: y(u_start),
                },
                to: Point {
                    x: u_end,
                    y: y(u_end),
                },
                width: 2.0,
            }]
        } else {
            Vec::new()
        };

        let bands = mle_ci_band(
            dist,
            &plot.observed,
            p_lo,
            p_hi,
            confidence,
            |q: f64| q.log10(),
            |p| probability_position(ProbabilityScale::Gumbel, p),
        );

        Self {
            title: title.into(),
            x_axis: log10_axis("Data", 10f64.powf(u_min), 10f64.powf(u_max)),
            y_axis,
            size: (900, 620),
            points,
            lines,
            bands,
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: sampling_notes(plot.positions.len()),
        }
    }

    /// Specification of a gamma probability plot.
    ///
    /// Data on a linear axis, probability on the Normal scale
    /// `y = Φ⁻¹(p)`. The gamma CDF has no exact linearizing transformation,
    /// so the fitted line is a **polyline** `y = Φ⁻¹(F(x))` sampled over the
    /// visible range, not a straight line. The confidence band comes from the
    /// covariance of the maximum-likelihood estimate, obtained from the
    /// numerical Hessian of the log-likelihood.
    pub fn gamma_probability(
        plot: &ProbabilityPlot,
        dist: &Gamma,
        title: impl Into<String>,
    ) -> Self {
        Self::gamma_probability_with_ci(plot, dist, title, Some(0.95))
    }

    /// Gamma plot with an explicit band (`None` disables it).
    ///
    /// The band comes from the covariance of the maximum-likelihood estimate,
    /// which is obtained from the numerical Hessian of the log-likelihood.
    pub fn gamma_probability_with_ci(
        plot: &ProbabilityPlot,
        dist: &Gamma,
        title: impl Into<String>,
        confidence: Option<f64>,
    ) -> Self {
        let x_lo = plot.observed.iter().copied().fold(f64::INFINITY, f64::min);
        let x_hi = plot
            .observed
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let pad = 0.05 * ((x_hi - x_lo).abs().max(f64::MIN_POSITIVE));
        let x_min = (x_lo - pad).max(0.0);
        let x_max = x_hi + pad;

        let p_lo = plot.positions.iter().copied().fold(f64::INFINITY, f64::min);
        let p_hi = plot
            .positions
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let y_axis = probability_axis("Percent", p_lo, p_hi, ProbabilityScale::Normal);

        let points = sampled_probability_points(&plot.positions, &plot.observed, |p, x| Point {
            x,
            y: normal_quantile(p),
        });

        // F(x) maps the data range onto the probability axis; sample it where
        // the fitted curve is inside the visible frame.
        const STEPS: usize = 80;
        let probability_at = |x: f64| dist.cdf(x).clamp(1e-12, 1.0 - 1e-12);
        let x_start = dist.quantile(normal_cdf(y_axis.min)).max(x_min);
        let x_end = dist.quantile(normal_cdf(y_axis.max)).min(x_max);
        let mut lines = Vec::new();
        if x_start < x_end {
            let mut previous = Point {
                x: x_start,
                y: normal_quantile(probability_at(x_start)),
            };
            for step in 1..=STEPS {
                let fraction = step as f64 / STEPS as f64;
                let x = x_start + (x_end - x_start) * fraction;
                let current = Point {
                    x,
                    y: normal_quantile(probability_at(x)),
                };
                lines.push(Segment {
                    from: previous,
                    to: current,
                    width: 2.0,
                });
                previous = current;
            }
        }

        let bands = mle_ci_band(
            dist,
            &plot.observed,
            p_lo,
            p_hi,
            confidence,
            |q: f64| q,
            |p| probability_position(ProbabilityScale::Normal, p),
        );
        Self {
            title: title.into(),
            x_axis: linear_axis("Data", x_min, x_max),
            y_axis,
            size: (900, 620),
            points,
            lines,
            bands,
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: sampling_notes(plot.positions.len()),
        }
    }

    /// Specification of a three-parameter log-normal probability plot.
    ///
    /// Data on a linear axis, probability on the Normal scale
    /// `y = Φ⁻¹(p)`. The transformation `ln(x − γ)` is not linear in `x`, so
    /// the fit is drawn as a polyline over the visible range.
    pub fn lognormal3_probability(
        plot: &ProbabilityPlot,
        dist: &Lognormal3,
        title: impl Into<String>,
    ) -> Self {
        let x_lo = plot.observed.iter().copied().fold(f64::INFINITY, f64::min);
        let x_hi = plot
            .observed
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let pad = 0.05 * ((x_hi - x_lo).abs().max(f64::MIN_POSITIVE));
        let x_min = (x_lo - pad).max(dist.threshold());
        let x_max = x_hi + pad;

        let p_lo = plot.positions.iter().copied().fold(f64::INFINITY, f64::min);
        let p_hi = plot
            .positions
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let y_axis = probability_axis("Percent", p_lo, p_hi, ProbabilityScale::Normal);

        let points = sampled_probability_points(&plot.positions, &plot.observed, |p, x| Point {
            x,
            y: normal_quantile(p),
        });

        let threshold = dist.threshold();
        let mu = dist.mu();
        let sigma = dist.sigma();
        let y = |x: f64| ((x - threshold).ln() - mu) / sigma;
        const STEPS: usize = 80;
        let mut lines = Vec::new();
        if x_min < x_max {
            let mut previous = Point {
                x: x_min,
                y: y(x_min),
            };
            for step in 1..=STEPS {
                let fraction = step as f64 / STEPS as f64;
                let x = x_min + (x_max - x_min) * fraction;
                let current = Point { x, y: y(x) };
                lines.push(Segment {
                    from: previous,
                    to: current,
                    width: 2.0,
                });
                previous = current;
            }
        }

        Self {
            title: title.into(),
            x_axis: linear_axis("Data", x_min, x_max),
            y_axis,
            size: (900, 620),
            points,
            lines,
            bands: Vec::new(),
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: sampling_notes(plot.positions.len()),
        }
    }

    /// Specification of a three-parameter Weibull probability plot.
    ///
    /// Data on a linear axis, probability on the Gumbel scale
    /// `y = ln(−ln(1 − p))`. The transformation `ln(x − γ)` is not linear in
    /// `x`, so the fit is drawn as a polyline `y = α·(ln(x − γ) − ln β)` over
    /// the visible range.
    pub fn weibull3_probability(
        plot: &ProbabilityPlot,
        dist: &Weibull3,
        title: impl Into<String>,
    ) -> Self {
        let x_lo = plot.observed.iter().copied().fold(f64::INFINITY, f64::min);
        let x_hi = plot
            .observed
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let pad = 0.05 * ((x_hi - x_lo).abs().max(f64::MIN_POSITIVE));
        let x_min = (x_lo - pad).max(dist.threshold());
        let x_max = x_hi + pad;

        let p_lo = plot.positions.iter().copied().fold(f64::INFINITY, f64::min);
        let p_hi = plot
            .positions
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let y_axis = probability_axis("Percent", p_lo, p_hi, ProbabilityScale::Gumbel);

        let points = sampled_probability_points(&plot.positions, &plot.observed, |p, x| Point {
            x,
            y: probability_position(ProbabilityScale::Gumbel, p),
        });

        let threshold = dist.threshold();
        let shape = dist.shape();
        let scale = dist.scale();
        let y = |x: f64| shape * ((x - threshold).ln() - scale.ln());
        const STEPS: usize = 80;
        let mut lines = Vec::new();
        if x_min < x_max {
            let mut previous = Point {
                x: x_min,
                y: y(x_min),
            };
            for step in 1..=STEPS {
                let fraction = step as f64 / STEPS as f64;
                let x = x_min + (x_max - x_min) * fraction;
                let current = Point { x, y: y(x) };
                lines.push(Segment {
                    from: previous,
                    to: current,
                    width: 2.0,
                });
                previous = current;
            }
        }

        Self {
            title: title.into(),
            x_axis: linear_axis("Data", x_min, x_max),
            y_axis,
            size: (900, 620),
            points,
            lines,
            bands: Vec::new(),
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: sampling_notes(plot.positions.len()),
        }
    }

    /// Specification of an extreme-value probability plot.
    ///
    /// Data on a linear axis, probability on the extreme-value scale
    /// `y = −ln(−ln(p))`. The fitted line is `y = (x − µ)/σ`, so the fit is
    /// straight. The confidence band comes from the covariance of the
    /// maximum-likelihood estimate, obtained from the numerical Hessian of the
    /// log-likelihood.
    pub fn extreme_value_probability(
        plot: &ProbabilityPlot,
        dist: &ExtremeValue,
        title: impl Into<String>,
    ) -> Self {
        Self::extreme_value_probability_with_ci(plot, dist, title, Some(0.95))
    }

    /// Extreme-value plot with an explicit band (`None` disables it).
    ///
    /// The band comes from the covariance of the maximum-likelihood estimate,
    /// which is obtained from the numerical Hessian of the log-likelihood.
    pub fn extreme_value_probability_with_ci(
        plot: &ProbabilityPlot,
        dist: &ExtremeValue,
        title: impl Into<String>,
        confidence: Option<f64>,
    ) -> Self {
        let x_lo = plot.observed.iter().copied().fold(f64::INFINITY, f64::min);
        let x_hi = plot
            .observed
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let pad = 0.05 * ((x_hi - x_lo).abs().max(f64::MIN_POSITIVE));
        let x_min = x_lo - pad;
        let x_max = x_hi + pad;

        let p_lo = plot.positions.iter().copied().fold(f64::INFINITY, f64::min);
        let p_hi = plot
            .positions
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let y_axis = probability_axis("Percent", p_lo, p_hi, ProbabilityScale::ExtremeValue);

        let points = sampled_probability_points(&plot.positions, &plot.observed, |p, x| Point {
            x,
            y: probability_position(ProbabilityScale::ExtremeValue, p),
        });

        let mu = dist.location();
        let sigma = dist.scale();
        let x_start = (mu + sigma * y_axis.min).max(x_min);
        let x_end = (mu + sigma * y_axis.max).min(x_max);
        let lines = if x_start < x_end {
            vec![Segment {
                from: Point {
                    x: x_start,
                    y: (x_start - mu) / sigma,
                },
                to: Point {
                    x: x_end,
                    y: (x_end - mu) / sigma,
                },
                width: 2.0,
            }]
        } else {
            Vec::new()
        };

        let bands = mle_ci_band(
            dist,
            &plot.observed,
            p_lo,
            p_hi,
            confidence,
            |q: f64| q,
            |p| probability_position(ProbabilityScale::ExtremeValue, p),
        );
        Self {
            title: title.into(),
            x_axis: linear_axis("Data", x_min, x_max),
            y_axis,
            size: (900, 620),
            points,
            lines,
            bands,
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
            notes: sampling_notes(plot.positions.len()),
        }
    }

    /// Replaces the horizontal-axis title (for example, with a column name).
    #[must_use]
    pub fn with_x_label(mut self, label: impl Into<String>) -> Self {
        self.x_axis.label = label.into();
        self
    }
}

/// Confidence ribbon for the fitted Normal line.
///
/// `q(z) = µ + s·z` with the sample standard deviation `s` (divisor `n − 1`),
/// and `Var(s) = s²(1 − c4²)`, the variance the documented behaviour's probability-plot path returns in
/// `aux[2]`:
///
/// `SE(z) = s·sqrt(1/n + z²·(1 − c4²))`, `q ± z_{1−α/2}·SE`.
///
/// The quantile is the normal one: measuring the documented bands at
/// 50 % gives a half-width of `1.96·s/√n` (1.010 for n = 20 and 1.788 for
/// n = 8), not the Student-t value. If `confidence` is `None` or invalid, no
/// band is drawn.
fn normal_ci_band(
    dist: &Normal,
    n: usize,
    z_min: f64,
    z_max: f64,
    confidence: Option<f64>,
) -> Vec<Band> {
    location_scale_ci_band(
        dist.mu(),
        dist.sigma(),
        n,
        z_min,
        z_max,
        confidence,
        |value| value,
    )
}

/// Inverts a small square matrix with Gauss–Jordan elimination.
fn invert_matrix(matrix: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = matrix.len();
    if n == 0 {
        return None;
    }
    let mut left: Vec<Vec<f64>> = matrix.to_vec();
    let mut right = vec![vec![0.0; n]; n];
    for (index, row) in right.iter_mut().enumerate() {
        row[index] = 1.0;
    }
    for column in 0..n {
        // Partial pivoting.
        let mut pivot = column;
        for row in (column + 1)..n {
            if left[row][column].abs() > left[pivot][column].abs() {
                pivot = row;
            }
        }
        if left[pivot][column].abs() < 1e-300 {
            return None;
        }
        left.swap(column, pivot);
        right.swap(column, pivot);
        let divisor = left[column][column];
        for value in left[column].iter_mut() {
            *value /= divisor;
        }
        for value in right[column].iter_mut() {
            *value /= divisor;
        }
        for row in 0..n {
            if row == column {
                continue;
            }
            let factor = left[row][column];
            if factor == 0.0 {
                continue;
            }
            for index in 0..n {
                left[row][index] -= factor * left[column][index];
                right[row][index] -= factor * right[column][index];
            }
        }
    }
    Some(right)
}

/// Band for a family fitted by maximum likelihood.
///
/// The covariance of the parameters is estimated from the numerical Hessian
/// of the log-likelihood at the maximum; the band follows by the delta method
/// with the normal quantile (the documented Weibull band matches `z` better than
/// Student t). The standard error is computed for the **axis coordinate** of
/// the quantile, so on a log axis the band is symmetric in `log10 x` — the
/// documented band is 2–3 % narrower than adding the error in data
/// units and taking the logarithm afterwards, and this is the construction
/// that matches it. `axis` maps a data value to the axis coordinate and
/// `y_position` maps a probability to the probability-axis coordinate.
fn mle_ci_band<D, A, Y>(
    dist: &D,
    values: &[f64],
    p_lo: f64,
    p_hi: f64,
    confidence: Option<f64>,
    axis: A,
    y_position: Y,
) -> Vec<Band>
where
    D: Distribution + Sized,
    A: Fn(f64) -> f64,
    Y: Fn(f64) -> f64,
{
    const STEPS: usize = 64;
    let Some(level) = confidence else {
        return Vec::new();
    };
    let n = values.len();
    if !(level > 0.0 && level < 1.0) || n < 3 || !p_hi.is_finite() || p_hi <= p_lo {
        return Vec::new();
    }
    let parameters: Vec<f64> = dist.parameters().iter().map(|(_, value)| *value).collect();
    let count = parameters.len();
    if count == 0 || count > 3 {
        return Vec::new();
    }
    let log_likelihood = |theta: &[f64]| -> f64 {
        let Some(distribution) = D::from_parameters(theta) else {
            return f64::NEG_INFINITY;
        };
        values.iter().map(|&x| distribution.ln_pdf(x)).sum()
    };

    // The numerical Hessian of the log-likelihood at the maximum.
    let steps: Vec<f64> = parameters
        .iter()
        .map(|value| 1e-4 * value.abs().max(1.0))
        .collect();
    let mut hessian = vec![vec![0.0; count]; count];
    for j in 0..count {
        for k in 0..count {
            let mut value = 0.0;
            for (sj, sk) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                let mut point = parameters.clone();
                point[j] += sj * steps[j];
                point[k] += sk * steps[k];
                let likelihood = log_likelihood(&point);
                if !likelihood.is_finite() {
                    return Vec::new();
                }
                value += sj * sk * likelihood;
            }
            hessian[j][k] = value / (4.0 * steps[j] * steps[k]);
        }
    }
    let Some(inverse) = invert_matrix(&hessian) else {
        return Vec::new();
    };
    // The covariance is minus the inverse of the Hessian.
    let covariance: Vec<Vec<f64>> = inverse
        .iter()
        .map(|row| row.iter().map(|value| -value).collect())
        .collect();

    let zq = normal_quantile(1.0 - (1.0 - level) / 2.0);
    if !zq.is_finite() {
        return Vec::new();
    }

    let mut lower = Vec::with_capacity(STEPS + 1);
    let mut upper = Vec::with_capacity(STEPS + 1);
    for step in 0..=STEPS {
        let p = p_lo + (p_hi - p_lo) * (step as f64 / STEPS as f64);
        let Some(distribution) = D::from_parameters(&parameters) else {
            return Vec::new();
        };
        let quantile = distribution.quantile(p);
        if !quantile.is_finite() {
            return Vec::new();
        }
        let center = axis(quantile);
        let y = y_position(p);
        if !center.is_finite() || !y.is_finite() {
            return Vec::new();
        }
        // The gradient of the axis coordinate with respect to the parameters.
        let mut gradients = vec![0.0; count];
        for j in 0..count {
            let mut plus = parameters.clone();
            let mut minus = parameters.clone();
            plus[j] += steps[j];
            minus[j] -= steps[j];
            let (Some(plus), Some(minus)) = (D::from_parameters(&plus), D::from_parameters(&minus))
            else {
                return Vec::new();
            };
            gradients[j] = (axis(plus.quantile(p)) - axis(minus.quantile(p))) / (2.0 * steps[j]);
        }
        // The variance of the axis coordinate: gᵀΣg.
        let mut variance = 0.0;
        for j in 0..count {
            for k in 0..count {
                variance += gradients[j] * covariance[j][k] * gradients[k];
            }
        }
        if !variance.is_finite() || variance < 0.0 {
            return Vec::new();
        }
        let error = zq * variance.sqrt();
        lower.push(Point {
            x: center - error,
            y,
        });
        upper.push(Point {
            x: center + error,
            y,
        });
    }
    vec![Band { lower, upper }]
}

/// Band for a location-scale family, mapped from the model scale to the axis.
fn location_scale_ci_band(
    mu: f64,
    sigma: f64,
    n: usize,
    z_min: f64,
    z_max: f64,
    confidence: Option<f64>,
    to_axis: impl Fn(f64) -> f64,
) -> Vec<Band> {
    const STEPS: usize = 128;

    let Some(level) = confidence else {
        return Vec::new();
    };
    if !(level > 0.0 && level < 1.0)
        || n < 2
        || !z_min.is_finite()
        || !z_max.is_finite()
        || z_max <= z_min
    {
        return Vec::new();
    }
    let zq = normal_quantile(1.0 - (1.0 - level) / 2.0);
    let c4 = crate::estimate::c4(n);
    if !zq.is_finite() || !c4.is_finite() {
        return Vec::new();
    }

    let nf = n as f64;
    let var_factor = 1.0 - c4 * c4;
    let mut lower = Vec::with_capacity(STEPS + 1);
    let mut upper = Vec::with_capacity(STEPS + 1);
    for k in 0..=STEPS {
        let z = z_min + (z_max - z_min) * (k as f64 / STEPS as f64);
        let q = mu + sigma * z;
        let se = sigma * (1.0 / nf + z * z * var_factor).sqrt();
        lower.push(Point {
            x: to_axis(q - zq * se),
            y: z,
        });
        upper.push(Point {
            x: to_axis(q + zq * se),
            y: z,
        });
    }
    vec![Band { lower, upper }]
}

/// Merges single-series plots into one overlay with a common axis.
///
/// Each input plot keeps its own fitted series; the points, lines and bands are
/// remapped from that plot's axis coordinates to the common union range. The
/// result has an empty flat `points`/`lines`/`bands` set and one
/// [`Series`] per input.
///
/// Returns `None` for an empty input or when the inputs do not share the same
/// axis scales.
#[must_use]
pub fn overlay_plots(plots: Vec<(String, PlotSpec)>, title: impl Into<String>) -> Option<PlotSpec> {
    if plots.is_empty() {
        return None;
    }
    if plots.len() == 1 {
        return plots.into_iter().next().map(|(_, plot)| plot);
    }

    let first = &plots[0].1;
    let x_scale = first.x_axis.scale;
    let y_scale = first.y_axis.scale;
    if plots
        .iter()
        .any(|(_, plot)| plot.x_axis.scale != x_scale || plot.y_axis.scale != y_scale)
    {
        return None;
    }

    let x_min = plots
        .iter()
        .map(|(_, plot)| plot.x_axis.min)
        .fold(f64::INFINITY, f64::min);
    let x_max = plots
        .iter()
        .map(|(_, plot)| plot.x_axis.max)
        .fold(f64::NEG_INFINITY, f64::max);
    let y_min = plots
        .iter()
        .map(|(_, plot)| plot.y_axis.min)
        .fold(f64::INFINITY, f64::min);
    let y_max = plots
        .iter()
        .map(|(_, plot)| plot.y_axis.max)
        .fold(f64::NEG_INFINITY, f64::max);

    let mut x_axis = first.x_axis.clone();
    x_axis.min = x_min;
    x_axis.max = x_max;
    // Grouped series are named "weight (A)", "weight (B)": keep the common
    // variable name. Different Y variables share no meaningful unit, so use
    // "Data", as the documented behaviour's overlay does.
    let base = plots[0]
        .0
        .split(" (")
        .next()
        .unwrap_or(&plots[0].0)
        .to_owned();
    x_axis.label = if plots
        .iter()
        .all(|(label, _)| label.starts_with(&format!("{base} (")))
    {
        base
    } else {
        "Data".to_owned()
    };
    x_axis.ticks = match x_scale {
        Scale::Linear => linear_ticks(x_min, x_max, 6),
        Scale::Log10 => log10_axis("Data", 10f64.powf(x_min), 10f64.powf(x_max)).ticks,
        Scale::Probability(scale) => probability_ticks(scale, x_min, x_max),
    };
    let mut y_axis = first.y_axis.clone();
    y_axis.min = y_min;
    y_axis.max = y_max;
    y_axis.ticks = merged_ticks(&plots, false, y_min, y_max);

    let mut series = Vec::with_capacity(plots.len());
    for (label, plot) in &plots {
        series.push(Series {
            label: label.clone(),
            points: plot.points.clone(),
            lines: plot.lines.clone(),
            bands: plot.bands.clone(),
            secondary_axis: false,
        });
    }

    Some(PlotSpec {
        title: title.into(),
        x_axis,
        y_axis,
        size: first.size,
        points: Vec::new(),
        lines: Vec::new(),
        bands: Vec::new(),
        series,
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: Vec::new(),
    })
}

/// Composes single plots into a panel grid (the documented behaviour `separate panels`).
///
/// The returned spec carries the panels and a column count; its own flat
/// points/lines/bands are empty. Backends render each panel independently.
#[must_use]
pub fn panel_plot(
    panels: Vec<PlotSpec>,
    columns: usize,
    title: impl Into<String>,
) -> Option<PlotSpec> {
    panel_plot_with_scales(panels, columns, title, false, false)
}

/// Panel grid with optional common X (`SAME 1`) and/or Y (`SAME 2`) scales.
#[must_use]
pub fn panel_plot_with_scales(
    mut panels: Vec<PlotSpec>,
    columns: usize,
    title: impl Into<String>,
    same_x: bool,
    same_y: bool,
) -> Option<PlotSpec> {
    if panels.is_empty() {
        return None;
    }
    if panels.len() == 1 {
        return panels.into_iter().next();
    }
    if same_x {
        let min = panels
            .iter()
            .map(|panel| panel.x_axis.min)
            .fold(f64::INFINITY, f64::min);
        let max = panels
            .iter()
            .map(|panel| panel.x_axis.max)
            .fold(f64::NEG_INFINITY, f64::max);
        let scale = panels[0].x_axis.scale;
        for panel in &mut panels {
            panel.x_axis.min = min;
            panel.x_axis.max = max;
            panel.x_axis.ticks = axis_ticks(scale, min, max);
        }
    }
    if same_y {
        let min = panels
            .iter()
            .map(|panel| panel.y_axis.min)
            .fold(f64::INFINITY, f64::min);
        let max = panels
            .iter()
            .map(|panel| panel.y_axis.max)
            .fold(f64::NEG_INFINITY, f64::max);
        let scale = panels[0].y_axis.scale;
        for panel in &mut panels {
            panel.y_axis.min = min;
            panel.y_axis.max = max;
            panel.y_axis.ticks = axis_ticks(scale, min, max);
        }
    }

    let columns = columns.max(1);
    let rows = panels.len().div_ceil(columns);
    let first = &panels[0];
    let width = first.size.0.saturating_mul(columns as u32);
    let height = first.size.1.saturating_mul(rows as u32);
    Some(PlotSpec {
        title: title.into(),
        x_axis: first.x_axis.clone(),
        y_axis: first.y_axis.clone(),
        size: (width, height),
        points: Vec::new(),
        lines: Vec::new(),
        bands: Vec::new(),
        series: Vec::new(),
        panels,
        panel_columns: columns,
        secondary: None,
        notes: Vec::new(),
    })
}

fn axis_ticks(scale: Scale, min: f64, max: f64) -> Vec<Tick> {
    match scale {
        Scale::Linear => linear_ticks(min, max, 6),
        Scale::Log10 => log10_axis("Data", 10f64.powf(min), 10f64.powf(max)).ticks,
        Scale::Probability(scale) => probability_ticks(scale, min, max),
    }
}

fn merged_ticks(plots: &[(String, PlotSpec)], x_axis: bool, min: f64, max: f64) -> Vec<Tick> {
    let mut ticks: Vec<Tick> = plots
        .iter()
        .flat_map(|(_, plot)| {
            if x_axis {
                plot.x_axis.ticks.clone()
            } else {
                plot.y_axis.ticks.clone()
            }
        })
        .filter(|tick| tick.position >= min - 1e-12 && tick.position <= max + 1e-12)
        .collect();
    ticks.sort_by(|a, b| a.position.total_cmp(&b.position));
    ticks.dedup_by(|a, b| (a.position - b.position).abs() < 1e-12);
    ticks
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Categorical palette used by the backends for overlay series.
#[must_use]
pub fn series_color(index: usize) -> &'static str {
    const COLORS: [&str; 6] = [
        "#1f4e79", "#c00000", "#2e7d32", "#6a1b9a", "#ef6c00", "#00838f",
    ];
    COLORS[index % COLORS.len()]
}

/// Renders the specification to SVG (deterministic, no dependencies).
#[must_use]
/// Presentation edits applied on top of a plot: a subtitle, a footer and
/// reference lines. Kept separate from [`PlotSpec`] so an analysis stays
/// reproducible and the edits can be reset.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GraphOverlay {
    /// Second title line, centred under the title.
    pub subtitle: Option<String>,
    /// Footer line, centred at the bottom.
    pub footer: Option<String>,
    /// Reference lines drawn over the plot, behind the data.
    pub reference_lines: Vec<ReferenceLine>,
    /// Typeface used by every text drawn on the graph.
    #[serde(default)]
    pub font_family: GraphFontFamily,
    /// Size multiplier for the graph text (1.0 = the designed size).
    #[serde(default = "default_font_scale")]
    pub font_scale: f32,
    /// Free text placed inside the plot area, in frame fractions.
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    /// Grid lines behind the data (on by default).
    #[serde(default = "default_true")]
    pub grid: bool,
    /// Labels drawn next to each point.
    #[serde(default)]
    pub data_labels: Option<DataLabelMode>,
    /// Manual horizontal-axis limits (raw values), when set.
    #[serde(default)]
    pub x_range: Option<(f64, f64)>,
    /// Manual vertical-axis limits (raw values), when set.
    #[serde(default)]
    pub y_range: Option<(f64, f64)>,
    /// Draw the point markers.
    #[serde(default = "default_true")]
    pub show_symbols: bool,
    /// Draw the fitted/series lines.
    #[serde(default = "default_true")]
    pub show_lines: bool,
    /// Draw the confidence bands and the bars.
    #[serde(default = "default_true")]
    pub show_bands: bool,
    /// Draw the report statistics block beside the plot.
    #[serde(default = "default_true")]
    pub show_report: bool,
}

/// How the labels added by the graph editor describe a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum DataLabelMode {
    /// The y value of the point.
    #[default]
    YValue,
    /// One-based position of the point in the plot.
    Index,
}

fn default_true() -> bool {
    true
}

/// Colour of a free text annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum AnnotationColor {
    /// Near-black.
    #[default]
    Black,
    /// Dark red.
    Red,
    /// Dark blue.
    Blue,
    /// Dark green.
    Green,
}

impl AnnotationColor {
    /// Colour used in the SVG.
    #[must_use]
    pub fn hex(self) -> &'static str {
        match self {
            Self::Black => "#222222",
            Self::Red => "#b02a37",
            Self::Blue => "#1f4e79",
            Self::Green => "#2e7d32",
        }
    }
}

/// Free text placed on a graph at a fraction of the plot area.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Annotation {
    /// Text to draw.
    pub text: String,
    /// Horizontal position, 0 = left edge and 1 = right edge.
    pub x: f32,
    /// Vertical position, 0 = top edge and 1 = bottom edge.
    pub y: f32,
    /// Text size in points.
    #[serde(default = "default_annotation_size")]
    pub size: f32,
    /// Text colour.
    #[serde(default)]
    pub color: AnnotationColor,
    /// Rotation in degrees (clockwise).
    #[serde(default)]
    pub angle: f32,
}

fn default_annotation_size() -> f32 {
    12.0
}

impl Default for Annotation {
    fn default() -> Self {
        Self {
            text: String::new(),
            x: 0.05,
            y: 0.05,
            size: 12.0,
            color: AnnotationColor::Black,
            angle: 0.0,
        }
    }
}

fn default_font_scale() -> f32 {
    1.0
}

impl Default for GraphOverlay {
    fn default() -> Self {
        Self {
            subtitle: None,
            footer: None,
            reference_lines: Vec::new(),
            font_family: GraphFontFamily::Sans,
            font_scale: 1.0,
            annotations: Vec::new(),
            grid: true,
            data_labels: None,
            x_range: None,
            y_range: None,
            show_symbols: true,
            show_lines: true,
            show_bands: true,
            show_report: true,
        }
    }
}

impl GraphOverlay {
    /// Clamped text scale, so a corrupt settings file cannot blank the labels.
    #[must_use]
    pub fn font_scale(&self) -> f32 {
        if self.font_scale.is_finite() {
            self.font_scale.clamp(0.75, 1.5)
        } else {
            1.0
        }
    }

    /// Plot with the manual axis limits applied: the ticks are rebuilt, so
    /// labels and values stay inside the requested window. Probability axes
    /// keep their engine ticks (their coordinates are not raw values).
    #[must_use]
    pub fn rescale(&self, spec: &PlotSpec) -> PlotSpec {
        let mut out = spec.clone();
        if out.panels.is_empty() {
            if let Some(range) = self.x_range
                && let Some(axis) = rescaled_axis(&out.x_axis, range)
            {
                out.x_axis = axis;
            }
            if let Some(range) = self.y_range
                && let Some(axis) = rescaled_axis(&out.y_axis, range)
            {
                out.y_axis = axis;
            }
        }
        out
    }

    /// A graph text size, in points, after the user's scale.
    #[must_use]
    pub fn text_size(&self, base: f32) -> f32 {
        base * self.font_scale()
    }

    /// Font family name used in the SVG, with a portable fallback.
    #[must_use]
    pub fn svg_family(&self) -> &'static str {
        match self.font_family {
            GraphFontFamily::Sans => "DejaVu Sans, sans-serif",
            GraphFontFamily::Serif => "DejaVu Serif, serif",
            GraphFontFamily::Mono => "DejaVu Sans Mono, monospace",
        }
    }
}

/// Typeface for the text drawn on a graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum GraphFontFamily {
    /// DejaVu Sans (the interface face).
    #[default]
    Sans,
    /// DejaVu Serif.
    Serif,
    /// DejaVu Sans Mono.
    Mono,
}

impl GraphFontFamily {
    /// Parses the code stored in the settings (`sans`, `serif`, `mono`).
    #[must_use]
    pub fn parse(code: &str) -> Option<Self> {
        match code {
            "sans" => Some(Self::Sans),
            "serif" => Some(Self::Serif),
            "mono" => Some(Self::Mono),
            _ => None,
        }
    }

    /// Code stored in the settings.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Sans => "sans",
            Self::Serif => "serif",
            Self::Mono => "mono",
        }
    }
}

/// Direction of a reference line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReferenceAxis {
    /// Vertical line at `x = value`.
    Vertical,
    /// Horizontal line at `y = value`.
    Horizontal,
}

/// One dashed reference line with an optional label.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReferenceLine {
    /// Direction of the line.
    pub axis: ReferenceAxis,
    /// Coordinate on that axis.
    pub value: f64,
    /// Text drawn at the end of the line.
    pub label: Option<String>,
}

/// Renders the plot without presentation edits.
#[must_use]
pub fn render_svg(spec: &PlotSpec) -> String {
    render_svg_with(spec, &GraphOverlay::default())
}

/// Renders the plot with the subtitle, footer and reference lines.
#[must_use]
pub fn render_svg_with(spec: &PlotSpec, overlay: &GraphOverlay) -> String {
    let rescaled;
    let spec = if overlay.x_range.is_some() || overlay.y_range.is_some() {
        rescaled = overlay.rescale(spec);
        &rescaled
    } else {
        spec
    };
    if !spec.panels.is_empty() {
        return render_panels_svg(spec, overlay);
    }
    const ML: f64 = 70.0;
    const MT: f64 = 50.0;
    const MB: f64 = 60.0;

    let w = f64::from(spec.size.0);
    let h = f64::from(spec.size.1);
    // The statistics blocks live in a right-hand margin of their own: the plot
    // area gets narrower instead of the numbers sitting over the data. A
    // secondary axis keeps its tick labels and title before that margin.
    let gutter = if overlay.show_report {
        note_gutter(&spec.notes, overlay.font_scale())
    } else {
        0.0
    };
    let secondary_room = if spec.secondary.is_some() { 40.0 } else { 0.0 };
    let pw = w - ML - 30.0 - secondary_room - gutter;
    let ph = h - MT - MB;

    let (x0, x1) = (spec.x_axis.min, spec.x_axis.max);
    let (y0, y1) = (spec.y_axis.min, spec.y_axis.max);
    let xspan = if (x1 - x0).abs() > 0.0 { x1 - x0 } else { 1.0 };
    let yspan = if (y1 - y0).abs() > 0.0 { y1 - y0 } else { 1.0 };

    let sx = |x: f64| ML + (x - x0) / xspan * pw;
    let sy = |y: f64| MT + ph - (y - y0) / yspan * ph;
    let right = spec.secondary.as_ref();
    let sy_right = |y: f64| -> f64 {
        match right {
            Some(axis) => {
                let span = if (axis.max - axis.min).abs() > 0.0 {
                    axis.max - axis.min
                } else {
                    1.0
                };
                MT + ph - (y - axis.min) / span * ph
            }
            None => sy(y),
        }
    };

    let mut s = String::with_capacity(4096);
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {w:.0} {h:.0}\">\n"
    ));
    s.push_str(&format!(
        "<rect x=\"0\" y=\"0\" width=\"{w:.0}\" height=\"{h:.0}\" fill=\"#ffffff\"/>\n"
    ));
    // With a subtitle the title moves up to leave room for it; on its own it
    // keeps the frozen position the golden SVGs compare against.
    let title_y = if overlay.subtitle.is_some() { 22 } else { 28 };
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{title_y}\" font-family=\"sans-serif\" font-size=\"16\" fill=\"#000000\">{}</text>\n",
        w / 2.0,
        escape_xml(&spec.title)
    ));
    if let Some(subtitle) = &overlay.subtitle {
        // The second line moves down with the text so the two never overlap.
        let subtitle_y = 27.0 + 13.0 * f64::from(overlay.font_scale());
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{subtitle_y:.1}\" font-family=\"sans-serif\" font-size=\"13\" fill=\"#444444\" text-anchor=\"middle\">{}</text>\n",
            w / 2.0,
            escape_xml(subtitle)
        ));
    }
    s.push_str(&format!(
        "<rect x=\"{ML:.1}\" y=\"{MT:.1}\" width=\"{pw:.1}\" height=\"{ph:.1}\" fill=\"none\" stroke=\"#000000\" stroke-width=\"1\"/>\n"
    ));
    s.push_str(&format!(
        "<defs><clipPath id=\"fp-plot\"><rect x=\"{ML:.1}\" y=\"{MT:.1}\" width=\"{pw:.1}\" height=\"{ph:.1}\"/></clipPath></defs>\n"
    ));

    // Grid and ticks come from the engine-owned axis descriptions.
    for tick in &spec.x_axis.ticks {
        let px = sx(tick.position);
        if overlay.grid {
            s.push_str(&format!(
                "<line x1=\"{px:.1}\" y1=\"{MT:.1}\" x2=\"{px:.1}\" y2=\"{:.1}\" stroke=\"#e6e6e6\" stroke-width=\"1\"/>\n",
                MT + ph
            ));
        }
        s.push_str(&format!(
            "<line x1=\"{px:.1}\" y1=\"{:.1}\" x2=\"{px:.1}\" y2=\"{:.1}\" stroke=\"#000000\" stroke-width=\"1\"/>\n",
            MT + ph,
            MT + ph + 5.0
        ));
        s.push_str(&format!(
            "<text x=\"{px:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"10\" fill=\"#333333\" text-anchor=\"middle\">{}</text>\n",
            MT + ph + 16.0,
            escape_xml(&tick.label)
        ));
    }

    for tick in &spec.y_axis.ticks {
        let py = sy(tick.position);
        if overlay.grid {
            s.push_str(&format!(
                "<line x1=\"{ML:.1}\" y1=\"{py:.1}\" x2=\"{:.1}\" y2=\"{py:.1}\" stroke=\"#e6e6e6\" stroke-width=\"1\"/>\n",
                ML + pw
            ));
        }
        s.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{py:.1}\" x2=\"{ML:.1}\" y2=\"{py:.1}\" stroke=\"#000000\" stroke-width=\"1\"/>\n",
            ML - 5.0
        ));
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"10\" fill=\"#333333\" text-anchor=\"end\">{}</text>\n",
            ML - 8.0,
            py + 3.0,
            escape_xml(&tick.label)
        ));
    }

    if let Some(axis) = right {
        for tick in &axis.ticks {
            let py = sy_right(tick.position);
            s.push_str(&format!(
                "<line x1=\"{:.1}\" y1=\"{py:.1}\" x2=\"{:.1}\" y2=\"{py:.1}\" stroke=\"#000000\" stroke-width=\"1\"/>\n",
                ML + pw,
                ML + pw + 5.0
            ));
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"10\" fill=\"#333333\" text-anchor=\"start\">{}</text>\n",
                ML + pw + 8.0,
                py + 3.0,
                escape_xml(&tick.label)
            ));
        }
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"11\" fill=\"#333333\" text-anchor=\"middle\" transform=\"rotate(90 {:.1} {:.1})\">{}</text>\n",
            ML + pw + 30.0,
            MT + ph / 2.0,
            ML + pw + 30.0,
            MT + ph / 2.0,
            escape_xml(&axis.label)
        ));
    }

    s.push_str("<g clip-path=\"url(#fp-plot)\">\n");
    for line in &overlay.reference_lines {
        match line.axis {
            ReferenceAxis::Vertical => {
                let px = sx(line.value);
                s.push_str(&format!(
                    "<line x1=\"{px:.1}\" y1=\"{MT:.1}\" x2=\"{px:.1}\" y2=\"{:.1}\" stroke=\"#c0392b\" stroke-width=\"1.2\" stroke-dasharray=\"4 3\"/>\n",
                    MT + ph
                ));
                if let Some(label) = &line.label {
                    s.push_str(&format!(
                        "<text x=\"{px:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"11\" fill=\"#c0392b\" text-anchor=\"middle\">{}</text>\n",
                        MT - 6.0,
                        escape_xml(label)
                    ));
                }
            }
            ReferenceAxis::Horizontal => {
                let py = sy(line.value);
                s.push_str(&format!(
                    "<line x1=\"{ML:.1}\" y1=\"{py:.1}\" x2=\"{:.1}\" y2=\"{py:.1}\" stroke=\"#c0392b\" stroke-width=\"1.2\" stroke-dasharray=\"4 3\"/>\n",
                    ML + pw
                ));
                if let Some(label) = &line.label {
                    s.push_str(&format!(
                        "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"11\" fill=\"#c0392b\" text-anchor=\"start\">{}</text>\n",
                        ML + pw + 6.0,
                        py + 4.0,
                        escape_xml(label)
                    ));
                }
            }
        }
    }
    for band in &spec.bands {
        if !overlay.show_bands || band.lower.len() < 2 || band.lower.len() != band.upper.len() {
            continue;
        }
        let mut d = String::new();
        for (i, p) in band.lower.iter().enumerate() {
            let cmd = if i == 0 { "M" } else { "L" };
            d.push_str(&format!("{cmd} {:.2} {:.2} ", sx(p.x), sy(p.y)));
        }
        for p in band.upper.iter().rev() {
            d.push_str(&format!("L {:.2} {:.2} ", sx(p.x), sy(p.y)));
        }
        d.push('Z');
        s.push_str(&format!(
            "<path d=\"{d}\" fill=\"#d9d9d9\" fill-opacity=\"0.55\" stroke=\"none\"/>\n"
        ));
    }

    for seg in &spec.lines {
        if !overlay.show_lines {
            break;
        }
        s.push_str(&format!(
            "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"#c00000\" stroke-width=\"{:.1}\"/>\n",
            sx(seg.from.x),
            sy(seg.from.y),
            sx(seg.to.x),
            sy(seg.to.y),
            seg.width
        ));
    }

    if overlay.show_symbols {
        for p in &spec.points {
            s.push_str(&format!(
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"3\" fill=\"#1f4e79\"/>\n",
                sx(p.x),
                sy(p.y)
            ));
        }
    }

    for (index, series) in spec.series.iter().enumerate() {
        let color = series_color(index);
        for band in &series.bands {
            if !overlay.show_bands || band.lower.len() < 2 || band.lower.len() != band.upper.len() {
                continue;
            }
            let mut d = String::new();
            for (i, p) in band.lower.iter().enumerate() {
                let cmd = if i == 0 { "M" } else { "L" };
                d.push_str(&format!("{cmd} {:.2} {:.2} ", sx(p.x), sy(p.y)));
            }
            for p in band.upper.iter().rev() {
                d.push_str(&format!("L {:.2} {:.2} ", sx(p.x), sy(p.y)));
            }
            d.push('Z');
            s.push_str(&format!(
                "<path d=\"{d}\" fill=\"{color}\" fill-opacity=\"0.12\" stroke=\"none\"/>\n"
            ));
        }
        for seg in &series.lines {
            if !overlay.show_lines {
                break;
            }
            s.push_str(&format!(
                "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{color}\" stroke-width=\"{:.1}\"/>\n",
                sx(seg.from.x),
                sy(seg.from.y),
                sx(seg.to.x),
                sy(seg.to.y),
                seg.width
            ));
        }
        if overlay.show_symbols {
            for p in &series.points {
                s.push_str(&format!(
                    "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"3\" fill=\"{color}\"/>\n",
                    sx(p.x),
                    sy(p.y)
                ));
            }
        }
    }
    s.push_str("</g>\n");

    let mut note_top = MT + 4.0;
    for note in &spec.notes {
        if !overlay.show_report {
            break;
        }
        if note.rows.is_empty() && note.lines.is_empty() {
            continue;
        }
        let chars = note_char_width(note) as f64;
        let scale = f64::from(overlay.font_scale());
        let content_w = chars * 6.5 * scale;
        let left = ML + pw + 12.0 + secondary_room;
        let right = left + content_w;
        let line_h = 15.0 * scale;
        let mut line_y = note_top + 10.0;
        for row in &note.rows {
            s.push_str(&format!(
                "<text x=\"{left:.1}\" y=\"{line_y:.1}\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#222222\">{}</text>\n",
                escape_xml(&row.label)
            ));
            s.push_str(&format!(
                "<text x=\"{right:.1}\" y=\"{line_y:.1}\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#222222\" text-anchor=\"end\">{}</text>\n",
                escape_xml(&row.value)
            ));
            line_y += line_h;
        }
        for line in &note.lines {
            s.push_str(&format!(
                "<text x=\"{left:.1}\" y=\"{line_y:.1}\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#222222\">{}</text>\n",
                escape_xml(line)
            ));
            line_y += line_h;
        }
        note_top = line_y + 4.0;
    }

    if spec.series.len() > 1 {
        let legend_x = ML + pw - 8.0;
        let max_len = spec
            .series
            .iter()
            .map(|series| series.label.chars().count())
            .max()
            .unwrap_or(0);
        let legend_w = max_len as f64 * 5.5 + 44.0;
        let legend_h = spec.series.len() as f64 * 14.0 + 8.0;
        s.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{legend_w:.1}\" height=\"{legend_h:.1}\" fill=\"#ffffff\" fill-opacity=\"0.88\" stroke=\"#d0d0d0\" stroke-width=\"1\"/>\n",
            legend_x - legend_w + 4.0,
            MT + 4.0
        ));
        let legend_left = legend_x - legend_w + 4.0;
        let mut legend_y = MT + 16.0;
        for (index, series) in spec.series.iter().enumerate() {
            let color = series_color(index);
            s.push_str(&format!(
                "<line x1=\"{:.1}\" y1=\"{legend_y:.1}\" x2=\"{:.1}\" y2=\"{legend_y:.1}\" stroke=\"{color}\" stroke-width=\"2\"/>\n",
                legend_left + 6.0,
                legend_left + 20.0
            ));
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"10\" fill=\"#222222\" text-anchor=\"start\">{}</text>\n",
                legend_left + 26.0,
                legend_y + 3.0,
                escape_xml(&series.label)
            ));
            legend_y += 14.0;
        }
    }

    // Free annotations: fractions of the plot area, drawn over the data with a
    // translucent label so they stay readable.
    let scale = f64::from(overlay.font_scale());
    for annotation in &overlay.annotations {
        let x = (annotation.x.clamp(0.0, 1.0) as f64) * pw;
        let y = (annotation.y.clamp(0.0, 1.0) as f64) * ph;
        let left = ML + x;
        let top = MT + y;
        let chars = annotation.text.chars().count() as f64;
        let size = f64::from(annotation.size.clamp(6.0, 48.0)) * scale;
        let box_w = chars * size * 0.55 + 8.0;
        let box_h = size + 4.0;
        if annotation.angle.abs() < 1e-6 {
            s.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{box_w:.1}\" height=\"{box_h:.1}\" rx=\"2\" fill=\"#ffffff\" fill-opacity=\"0.85\"/>\n",
                left - 2.0,
                top - 2.0
            ));
        }
        s.push_str(&format!(
            "<text x=\"{left:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"{size:.1}\" fill=\"{}\" transform=\"rotate({:.1} {left:.1} {:.1})\">{}</text>\n",
            top + size,
            annotation.color.hex(),
            annotation.angle,
            top + size,
            escape_xml(&annotation.text)
        ));
    }

    if let Some(mode) = overlay.data_labels {
        for (index, point) in spec.points.iter().enumerate() {
            let label = match mode {
                DataLabelMode::YValue => format!("{:.4}", point.y),
                DataLabelMode::Index => (index + 1).to_string(),
            };
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"10\" fill=\"#333333\">{}</text>\n",
                sx(point.x) + 5.0,
                sy(point.y) - 5.0,
                escape_xml(&label)
            ));
        }
    }

    let x_label_y = if overlay.footer.is_some() {
        h - 30.0
    } else {
        h - 14.0
    };
    s.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{x_label_y:.1}\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#000000\" text-anchor=\"middle\">{}</text>\n",
        ML + pw / 2.0,
        escape_xml(&spec.x_axis.label)
    ));
    s.push_str(&format!(
        "<text x=\"18\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#000000\" text-anchor=\"middle\" transform=\"rotate(-90 18 {:.1})\">{}</text>\n",
        MT + ph / 2.0,
        MT + ph / 2.0,
        escape_xml(&spec.y_axis.label)
    ));
    if let Some(footer) = &overlay.footer {
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#555555\" text-anchor=\"middle\">{}</text>\n",
            w / 2.0,
            h - 8.0,
            escape_xml(footer)
        ));
    }
    s.push_str("</svg>\n");
    style_graph_text(&mut s, overlay);

    s
}

/// Applies the chosen typeface and text scale to the SVG that was just built.
///
/// The sizes come from a small, fixed set, so the substitution is exact and
/// keeps the layout arithmetic untouched; placeholders prevent a scaled size
/// from being matched again by the next base size.
fn style_graph_text(svg: &mut String, overlay: &GraphOverlay) {
    if overlay.font_family != GraphFontFamily::Sans {
        *svg = svg.replace(
            "font-family=\"sans-serif\"",
            &format!("font-family=\"{}\"", overlay.svg_family()),
        );
    }
    let scale = overlay.font_scale();
    if (scale - 1.0).abs() < 1e-6 {
        return;
    }
    const BASES: [f32; 5] = [10.0, 11.0, 12.0, 13.0, 16.0];
    for (index, base) in BASES.iter().enumerate() {
        *svg = svg.replace(
            &format!("font-size=\"{base}\""),
            &format!("font-size=\"__FP_TEXT_{index}__\""),
        );
    }
    for (index, base) in BASES.iter().enumerate() {
        *svg = svg.replace(
            &format!("__FP_TEXT_{index}__"),
            &format!("{}", overlay.text_size(*base)),
        );
    }
}

fn render_panels_svg(spec: &PlotSpec, overlay: &GraphOverlay) -> String {
    let w = f64::from(spec.size.0);
    let h = f64::from(spec.size.1);
    let columns = spec.panel_columns.max(1);
    let rows = spec.panels.len().div_ceil(columns);
    let margin = 8.0;
    // The grid title, the subtitle and the footer belong to the whole grid;
    // each panel keeps its own title inside its cell.
    let subtitle_h = if overlay.subtitle.is_some() {
        16.0
    } else {
        0.0
    };
    let title_h = if spec.title.is_empty() && subtitle_h == 0.0 {
        0.0
    } else {
        30.0 + subtitle_h
    };
    let footer_h = if overlay.footer.is_some() { 18.0 } else { 0.0 };
    let cell_w = (w - 2.0 * margin) / columns as f64;
    let cell_h = (h - title_h - footer_h - 2.0 * margin) / rows as f64;
    // Reference lines are in axis coordinates, so they apply to every panel.
    let edits = GraphOverlay {
        subtitle: None,
        footer: None,
        reference_lines: overlay.reference_lines.clone(),
        font_family: overlay.font_family,
        font_scale: overlay.font_scale,
        annotations: overlay.annotations.clone(),
        grid: overlay.grid,
        data_labels: overlay.data_labels,
        x_range: None,
        y_range: None,
        show_symbols: overlay.show_symbols,
        show_lines: overlay.show_lines,
        show_bands: overlay.show_bands,
        show_report: overlay.show_report,
    };

    let mut s = String::with_capacity(8192);
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {w:.0} {h:.0}\">\n"
    ));
    s.push_str(&format!(
        "<rect x=\"0\" y=\"0\" width=\"{w:.0}\" height=\"{h:.0}\" fill=\"#ffffff\"/>\n"
    ));
    if !spec.title.is_empty() {
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"24\" font-family=\"{}\" font-size=\"{}\" fill=\"#000000\" text-anchor=\"middle\">{}</text>\n",
            w / 2.0,
            overlay.svg_family(),
            overlay.text_size(16.0),
            escape_xml(&spec.title)
        ));
    }
    if let Some(subtitle) = &overlay.subtitle {
        let y = if spec.title.is_empty() {
            20.0
        } else {
            27.0 + 13.0 * f64::from(overlay.font_scale())
        };
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{y:.1}\" font-family=\"{}\" font-size=\"{}\" fill=\"#444444\" text-anchor=\"middle\">{}</text>\n",
            w / 2.0,
            overlay.svg_family(),
            overlay.text_size(13.0),
            escape_xml(subtitle)
        ));
    }
    if let Some(footer) = &overlay.footer {
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"{}\" font-size=\"{}\" fill=\"#555555\" text-anchor=\"middle\">{}</text>\n",
            w / 2.0,
            h - 10.0,
            overlay.svg_family(),
            overlay.text_size(12.0),
            escape_xml(footer)
        ));
    }

    for (index, panel) in spec.panels.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        let x = margin + column as f64 * cell_w;
        let y = title_h + margin + row as f64 * cell_h;
        let rendered = render_svg_with(panel, &edits);
        let start = rendered.find('>').map_or(0, |position| position + 1);
        let end = rendered.rfind("</svg>").unwrap_or(rendered.len());
        let inner = &rendered[start..end];
        let pw = panel.size.0;
        let ph = panel.size.1;
        s.push_str(&format!(
            "<svg x=\"{x:.1}\" y=\"{y:.1}\" width=\"{cell_w:.1}\" height=\"{cell_h:.1}\" viewBox=\"0 0 {pw} {ph}\">{inner}</svg>\n"
        ));
    }
    s.push_str("</svg>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::MissingPolicy;

    #[test]
    fn the_plot_stride_keeps_the_sample_bounded() {
        assert_eq!(plot_stride(0), 1);
        assert_eq!(plot_stride(1), 1);
        assert_eq!(plot_stride(MAX_PLOT_POINTS), 1);
        assert_eq!(plot_stride(MAX_PLOT_POINTS + 1), 2);
        assert_eq!(plot_stride(1_000_000), 200);
        assert!(plot_stride(usize::MAX) > 1);
    }

    #[test]
    fn plotting_positions_stay_in_the_interval() {
        for method in [
            PlottingPosition::Benard,
            PlottingPosition::Blom,
            PlottingPosition::Hazen,
            PlottingPosition::Weibull,
            PlottingPosition::Cunnane,
        ] {
            let p = positions(50, method).unwrap();
            assert_eq!(p.len(), 50);
            for (i, v) in p.iter().enumerate() {
                assert!(
                    *v > 0.0 && *v < 1.0,
                    "{}: position {i} = {v}",
                    method.name()
                );
                if i > 0 {
                    assert!(p[i - 1] < *v, "{}: not monotone at {i}", method.name());
                }
            }
        }
    }

    #[test]
    fn known_benard_values() {
        // n = 10: (1 − 0.3)/10.4 = 0.067307..., (10 − 0.3)/10.4 = 0.932692...
        let p = positions(10, PlottingPosition::Benard).unwrap();
        assert!((p[0] - 0.7 / 10.4).abs() < 1e-15);
        assert!((p[9] - 9.7 / 10.4).abs() < 1e-15);
    }

    #[test]
    fn newlands_weibull_for_n_one() {
        assert!((PlottingPosition::Weibull.value(1, 1).unwrap() - 0.5).abs() < 1e-15);
    }

    #[test]
    fn invalid_indices_are_rejected() {
        assert!(PlottingPosition::Benard.value(0, 10).is_err());
        assert!(PlottingPosition::Benard.value(11, 10).is_err());
        assert!(positions(0, PlottingPosition::Benard).is_err());
    }

    #[test]
    fn probability_plot_has_n_consistent_points() {
        let s = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let d = Normal::fit_moments(&s).unwrap();
        let p = normal_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
        assert_eq!(p.observed.len(), 5);
        assert_eq!(p.theoretical.len(), 5);
        assert_eq!(p.positions.len(), 5);
        for w in p.theoretical.windows(2) {
            assert!(w[0] < w[1]);
        }
    }

    #[test]
    fn probability_ticks_are_engine_owned_and_ordered() {
        let s = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let d = Normal::fit_moments(&s).unwrap();
        let p = normal_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::normal_probability(&p, &d, "test");

        let labels: Vec<&str> = spec.y_axis.ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "1", "5", "10", "20", "30", "40", "50", "60", "70", "80", "90", "95", "99"
            ]
        );
        assert!(
            spec.y_axis
                .ticks
                .windows(2)
                .all(|w| w[0].position < w[1].position)
        );
        assert!(spec.y_axis.ticks.iter().all(|t| {
            t.position >= spec.y_axis.min - 1e-12 && t.position <= spec.y_axis.max + 1e-12
        }));
        let median = spec
            .y_axis
            .ticks
            .iter()
            .find(|t| t.label == "50")
            .expect("50 % tick");
        assert_eq!(median.position, 0.0, "50 % must be z = 0");
        assert!(matches!(
            spec.y_axis.scale,
            Scale::Probability(ProbabilityScale::Normal)
        ));
    }

    #[test]
    fn probability_axis_matches_the_documented_n200_set() {
        // n = 200 plotting positions span 0.35 % to 99.65 %, which the
        // documented plot shows on an axis of 0.1 % to 99.9 %.
        let axis = probability_axis("Percent", 0.0035, 0.9965, ProbabilityScale::Normal);
        let labels: Vec<&str> = axis.ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "0.1", "1", "5", "10", "20", "30", "40", "50", "60", "70", "80", "90", "95", "99",
                "99.9"
            ]
        );
    }

    #[test]
    fn probability_axis_expands_for_extreme_positions() {
        let axis = probability_axis("Percent", 0.0005, 0.9995, ProbabilityScale::Normal);
        assert!((axis.min - normal_quantile(0.0001)).abs() < 1e-12);
        assert!((axis.max - normal_quantile(0.9999)).abs() < 1e-12);
        assert_eq!(axis.ticks.first().unwrap().label, "0.01");
        assert_eq!(axis.ticks.last().unwrap().label, "99.99");
    }

    #[test]
    fn normal_probability_uses_data_x_and_probability_y() {
        let s = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let d = Normal::fit_moments(&s).unwrap();
        let p = normal_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::normal_probability(&p, &d, "test");

        assert_eq!(spec.x_axis.label, "Data");
        assert_eq!(spec.y_axis.label, "Percent");
        assert!(matches!(spec.x_axis.scale, Scale::Linear));
        assert_eq!(spec.points.len(), 5);
        for (point, &x) in spec.points.iter().zip(&p.observed) {
            assert!((point.x - x).abs() < 1e-15);
        }
        for (point, &prob) in spec.points.iter().zip(&p.positions) {
            assert!((point.y - normal_quantile(prob)).abs() < 1e-12);
        }

        let seg = spec.lines[0];
        let slope = (seg.to.y - seg.from.y) / (seg.to.x - seg.from.x);
        assert!((slope - 1.0 / d.sigma()).abs() < 1e-12);
        let y_at_mu = seg.from.y + slope * (d.mu() - seg.from.x);
        assert!(y_at_mu.abs() < 1e-12, "line misses (µ, 0): {y_at_mu}");
        assert!(seg.from.y >= spec.y_axis.min - 1e-12);
        assert!(seg.to.y <= spec.y_axis.max + 1e-12);

        let band = &spec.bands[0];
        assert_eq!(band.lower.len(), band.upper.len());
        for (lo, up) in band.lower.iter().zip(&band.upper) {
            assert!((lo.y - up.y).abs() < 1e-12);
            assert!(lo.x <= up.x);
            let fitted = d.mu() + d.sigma() * lo.y;
            assert!(lo.x <= fitted + 1e-12);
            assert!(fitted <= up.x + 1e-12);
        }
        let width_at = |z: f64| {
            let q = d.mu() + d.sigma() * z;
            let idx = band
                .lower
                .iter()
                .position(|p| (p.y - z).abs() < 1e-9)
                .expect("z in band");
            band.upper[idx].x - q
        };
        assert!(width_at(spec.y_axis.max) > width_at(0.0));
    }

    #[test]
    fn ci_band_uses_the_normal_quantile_and_shrinks_with_n() {
        let d = Normal::new(0.0, 1.0).expect("normal");
        let half_width = |n: usize| {
            let band = normal_ci_band(&d, n, -3.0, 3.0, Some(0.95));
            let b = &band[0];
            let idx = b
                .lower
                .iter()
                .position(|p| p.y.abs() < 1e-12)
                .expect("z = 0");
            0.5 * (b.upper[idx].x - b.lower[idx].x)
        };
        let expected_8 = 1.959_964 / 8f64.sqrt();
        assert!((half_width(8) - expected_8).abs() < 1e-6);
        assert!(half_width(8) > half_width(30));
        assert!(half_width(30) > half_width(200));
    }

    #[test]
    fn confidence_can_be_disabled() {
        let s = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let d = Normal::fit_moments(&s).unwrap();
        let p = normal_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::normal_probability_with_ci(&p, &d, "test", None);
        assert!(spec.bands.is_empty());
        assert!(!spec.lines.is_empty());
    }

    #[test]
    fn linear_axes_have_minor_ticks_between_the_majors() {
        let axis = linear_axis("x", 0.0, 10.0);
        assert!(!axis.minor.is_empty());
        for minor in &axis.minor {
            assert!(*minor > axis.min && *minor < axis.max);
            assert!(
                axis.ticks
                    .iter()
                    .all(|tick| (tick.position - *minor).abs() > 1e-9)
            );
        }
    }

    #[test]
    fn log_axes_have_decade_minors_and_probability_axes_do_not() {
        let log = log10_axis("x", 1.0, 1000.0);
        assert!(
            log.minor
                .iter()
                .any(|value| (value - 3f64.log10()).abs() < 1e-9)
        );
        assert!(
            log.minor
                .iter()
                .any(|value| (value - 9f64.log10()).abs() < 1e-9)
        );
        let probability = probability_axis("p", 0.01, 0.99, ProbabilityScale::Normal);
        assert!(probability.minor.is_empty());
    }

    #[test]
    fn log10_axis_has_decade_ticks() {
        let axis = log10_axis("x", 1.0, 1000.0);
        assert!(matches!(axis.scale, Scale::Log10));
        assert!((axis.min - 0.0).abs() < 1e-12);
        assert!((axis.max - 3.0).abs() < 1e-12);
        let labels: Vec<&str> = axis.ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            ["1", "2", "5", "10", "20", "50", "100", "200", "500", "1000"]
        );
        assert!(axis.ticks.windows(2).all(|w| w[0].position < w[1].position));

        let narrow = log10_axis("x", 9.5, 13.4);
        let labels: Vec<&str> = narrow.ticks.iter().map(|t| t.label.as_str()).collect();
        assert!(labels.contains(&"10"), "{labels:?}");
        assert!(labels.contains(&"11"), "{labels:?}");
        assert!((narrow.ticks[0].position - 9.5f64.log10()).abs() < 1e-12);
        assert!(
            narrow
                .ticks
                .windows(2)
                .all(|w| w[0].position < w[1].position)
        );
    }

    #[test]
    fn lognormal_plot_uses_a_log_x_axis() {
        let s = Sample::new(vec![1.0, 2.0, 4.0, 8.0, 16.0], MissingPolicy::Drop).unwrap();
        let d = Lognormal::fit_moments(&s).unwrap();
        let p = lognormal_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::lognormal_probability(&p, &d, "test");

        assert!(matches!(spec.x_axis.scale, Scale::Log10));
        assert_eq!(spec.y_axis.label, "Percent");
        for (point, &x) in spec.points.iter().zip(&p.observed) {
            assert!((point.x - x.log10()).abs() < 1e-12);
        }

        let seg = spec.lines[0];
        let slope = (seg.to.y - seg.from.y) / (seg.to.x - seg.from.x);
        let expected = std::f64::consts::LN_10 / d.sigma();
        assert!((slope - expected).abs() < 1e-12);
        let median_u = d.mu() / std::f64::consts::LN_10;
        let y_at_median = seg.from.y + slope * (median_u - seg.from.x);
        assert!(
            y_at_median.abs() < 1e-12,
            "line misses the median: {y_at_median}"
        );

        let band = &spec.bands[0];
        for (lo, up) in band.lower.iter().zip(&band.upper) {
            assert!(lo.x <= up.x);
            let fitted = (d.mu() + d.sigma() * lo.y) / std::f64::consts::LN_10;
            assert!(lo.x <= fitted + 1e-12);
            assert!(fitted <= up.x + 1e-12);
        }
    }

    #[test]
    fn weibull_plot_uses_the_gumbel_axis() {
        let n = 30;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                10.0 * (-(1.0 - p).ln()).powf(0.5)
            })
            .collect();
        let s = Sample::new(values, MissingPolicy::Drop).unwrap();
        let d = Weibull::fit_mle(&s).unwrap();
        let p = weibull_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::weibull_probability(&p, &d, "test");

        assert!(matches!(spec.x_axis.scale, Scale::Log10));
        assert!(matches!(
            spec.y_axis.scale,
            Scale::Probability(ProbabilityScale::Gumbel)
        ));
        for (point, &prob) in spec.points.iter().zip(&p.positions) {
            let expected = (-(1.0 - prob).ln()).ln();
            assert!((point.y - expected).abs() < 1e-12);
        }
        let seg = spec.lines[0];
        let slope = (seg.to.y - seg.from.y) / (seg.to.x - seg.from.x);
        assert!((slope - d.shape() * std::f64::consts::LN_10).abs() < 1e-10);
        let y_at_scale = seg.from.y + slope * (d.scale().log10() - seg.from.x);
        assert!(y_at_scale.abs() < 1e-10, "line misses β: {y_at_scale}");
        assert_eq!(
            spec.bands.len(),
            1,
            "the band comes from the MLE covariance"
        );
        let labels: Vec<&str> = spec.y_axis.ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "1", "2", "3", "5", "10", "20", "30", "40", "50", "60", "70", "80", "90", "95",
                "99"
            ]
        );
        assert!(
            !spec.y_axis.ticks.iter().any(|t| t.label == "63.2"),
            "the documented Weibull plot has no 63.2 % tick"
        );
    }

    #[test]
    fn exponential_plot_is_a_straight_gumbel_line() {
        let s = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let d = Exponential::fit_mle(&s).unwrap();
        let p = exponential_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::exponential_probability(&p, &d, "test");

        assert!(matches!(spec.x_axis.scale, Scale::Log10));
        assert!(matches!(
            spec.y_axis.scale,
            Scale::Probability(ProbabilityScale::Gumbel)
        ));
        for (point, &prob) in spec.points.iter().zip(&p.positions) {
            let expected = (-(1.0 - prob).ln()).ln();
            assert!((point.y - expected).abs() < 1e-12);
        }
        let seg = spec.lines[0];
        let slope = (seg.to.y - seg.from.y) / (seg.to.x - seg.from.x);
        assert!((slope - std::f64::consts::LN_10).abs() < 1e-12);
        let y_at_theta = seg.from.y + slope * (d.theta().log10() - seg.from.x);
        assert!(y_at_theta.abs() < 1e-12, "line misses θ: {y_at_theta}");
        assert!(spec.bands.is_empty(), "the exponential plot has no band");
    }

    #[test]
    fn the_weibull_band_brackets_the_fitted_line() {
        let n = 30;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                10.0 * (-(1.0 - p).ln()).powf(0.5)
            })
            .collect();
        let s = Sample::new(values, MissingPolicy::Drop).unwrap();
        let d = Weibull::fit_mle(&s).unwrap();
        let p = weibull_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::weibull_probability(&p, &d, "test");
        assert_eq!(spec.bands.len(), 1);
        let band = &spec.bands[0];
        assert!(band.lower.len() > 10, "{:?}", band.lower.len());
        assert_eq!(band.lower.len(), band.upper.len());
        for (lower, upper) in band.lower.iter().zip(&band.upper) {
            // The band surrounds the line, so the lower boundary is on the left.
            assert!(lower.x <= upper.x, "{lower:?} {upper:?}");
            assert!((lower.y - upper.y).abs() < 1e-12);
        }
    }

    #[test]
    fn extreme_value_plot_is_a_straight_line() {
        let truth = ExtremeValue::new(1.0, 2.0).unwrap();
        let n = 30;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                truth.quantile(p)
            })
            .collect();
        let s = Sample::new(values, MissingPolicy::Drop).unwrap();
        let d = ExtremeValue::fit_mle(&s).unwrap();
        let p = extreme_value_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::extreme_value_probability(&p, &d, "test");

        assert!(matches!(spec.x_axis.scale, Scale::Linear));
        assert!(matches!(
            spec.y_axis.scale,
            Scale::Probability(ProbabilityScale::ExtremeValue)
        ));
        for (point, &prob) in spec.points.iter().zip(&p.positions) {
            let expected = -((-prob.ln()).ln());
            assert!((point.y - expected).abs() < 1e-12);
        }
        let seg = spec.lines[0];
        let slope = (seg.to.y - seg.from.y) / (seg.to.x - seg.from.x);
        assert!((slope - 1.0 / d.scale()).abs() < 1e-12);
        let y_at_location = seg.from.y + slope * (d.location() - seg.from.x);
        assert!(
            y_at_location.abs() < 1e-10,
            "line misses the location: {y_at_location}"
        );
        assert_eq!(
            spec.bands.len(),
            1,
            "the band comes from the MLE covariance"
        );
    }

    #[test]
    fn gamma_plot_uses_the_normal_axis_and_a_curved_fit() {
        let truth = Gamma::new(2.0, 4.0).unwrap();
        let n = 40;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                truth.quantile(p)
            })
            .collect();
        let s = Sample::new(values, MissingPolicy::Drop).unwrap();
        let d = Gamma::fit_mle(&s).unwrap();
        let p = gamma_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::gamma_probability(&p, &d, "test");

        assert!(matches!(spec.x_axis.scale, Scale::Linear));
        assert!(matches!(
            spec.y_axis.scale,
            Scale::Probability(ProbabilityScale::Normal)
        ));
        for (point, &prob) in spec.points.iter().zip(&p.positions) {
            assert!((point.y - normal_quantile(prob)).abs() < 1e-12);
        }
        // The fit is a polyline that follows Φ⁻¹(F(x)) and increases with x.
        assert!(spec.lines.len() > 10, "lines {}", spec.lines.len());
        for segment in &spec.lines {
            assert!(segment.to.x > segment.from.x);
            assert!(segment.to.y >= segment.from.y - 1e-12);
            let expected = normal_quantile(d.cdf(segment.to.x));
            assert!((segment.to.y - expected).abs() < 1e-9);
        }
        assert_eq!(
            spec.bands.len(),
            1,
            "the band comes from the MLE covariance"
        );
    }

    #[test]
    fn panel_plot_nests_independent_axes() {
        let make = |values: Vec<f64>, label: &str| {
            let s = Sample::new(values, MissingPolicy::Drop).unwrap();
            let d = Normal::fit_moments(&s).unwrap();
            let p = normal_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
            PlotSpec::normal_probability(&p, &d, label)
        };
        let panels = panel_plot(
            vec![
                make(vec![1.0, 2.0, 3.0, 4.0, 5.0], "A"),
                make(vec![11.0, 12.0, 13.0, 14.0, 15.0], "B"),
            ],
            2,
            "Panels",
        )
        .expect("panels");
        assert_eq!(panels.panels.len(), 2);
        assert_eq!(panels.panel_columns, 2);
        let svg = render_svg(&panels);
        assert!(svg.matches("<svg").count() >= 3);
        assert!(svg.contains(">A<"));
        assert!(svg.contains(">B<"));
    }

    #[test]
    fn panel_scales_can_be_shared() {
        let make = |values: Vec<f64>, label: &str| {
            let s = Sample::new(values, MissingPolicy::Drop).unwrap();
            let d = Normal::fit_moments(&s).unwrap();
            let p = normal_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
            PlotSpec::normal_probability(&p, &d, label)
        };
        let panels = panel_plot_with_scales(
            vec![
                make(vec![1.0, 2.0, 3.0, 4.0, 5.0], "A"),
                make(vec![11.0, 12.0, 13.0, 14.0, 15.0], "B"),
            ],
            2,
            "Panels",
            true,
            true,
        )
        .expect("panels");
        assert_eq!(panels.panels[0].x_axis.min, panels.panels[1].x_axis.min);
        assert_eq!(panels.panels[0].x_axis.max, panels.panels[1].x_axis.max);
        assert_eq!(panels.panels[0].y_axis.min, panels.panels[1].y_axis.min);
        assert_eq!(panels.panels[0].y_axis.max, panels.panels[1].y_axis.max);
    }

    #[test]
    fn overlay_merges_series_and_axes() {
        let make = |values: Vec<f64>, label: &str| {
            let s = Sample::new(values, MissingPolicy::Drop).unwrap();
            let d = Normal::fit_moments(&s).unwrap();
            let p = normal_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
            (
                label.to_owned(),
                PlotSpec::normal_probability(&p, &d, label),
            )
        };
        let a = make(vec![1.0, 2.0, 3.0, 4.0, 5.0], "A");
        let b = make(vec![11.0, 12.0, 13.0, 14.0, 15.0], "B");
        let overlay = overlay_plots(vec![a, b], "Overlay").expect("overlay");
        assert_eq!(overlay.series.len(), 2);
        assert_eq!(overlay.series[0].label, "A");
        assert_eq!(overlay.series[1].label, "B");
        assert_eq!(overlay.series[0].points.len(), 5);
        assert!(overlay.x_axis.min <= 1.0);
        assert!(overlay.x_axis.max >= 15.0);
        assert!(overlay.points.is_empty());
        let svg = render_svg(&overlay);
        assert!(svg.contains(">A<"), "legend A missing");
        assert!(svg.contains(">B<"), "legend B missing");
        assert!(svg.contains("#c00000"), "second series colour missing");
    }

    #[test]
    fn data_display_can_hide_symbols_lines_and_bands() {
        let sample = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let fit = Normal::fit_moments(&sample).unwrap();
        let plot = normal_probability_plot(&sample, &fit, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::normal_probability(&plot, &fit, "Normal");
        let full = render_svg_with(&spec, &GraphOverlay::default());
        assert!(full.contains("<circle cx"));
        assert!(full.contains("stroke=\"#c00000\""));
        assert!(full.contains("fill=\"#d9d9d9\""));

        let hidden = GraphOverlay {
            show_symbols: false,
            show_lines: false,
            show_bands: false,
            ..GraphOverlay::default()
        };
        let svg = render_svg_with(&spec, &hidden);
        assert!(!svg.contains("<circle cx"), "symbols are hidden");
        assert!(!svg.contains("stroke=\"#c00000\""), "lines are hidden");
        assert!(!svg.contains("fill=\"#d9d9d9\""), "bands are hidden");
    }

    #[test]
    fn manual_axis_limits_rebuild_linear_ticks() {
        let spec = PlotSpec {
            title: "test".to_owned(),
            x_axis: linear_axis("x", 0.0, 10.0),
            y_axis: linear_axis("y", 0.0, 10.0),
            size: (600, 400),
            points: vec![Point { x: 2.0, y: 3.0 }, Point { x: 8.0, y: 7.0 }],
            lines: Vec::new(),
            bands: Vec::new(),
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 0,
            secondary: None,
            notes: Vec::new(),
        };
        let overlay = GraphOverlay {
            x_range: Some((2.0, 4.0)),
            y_range: Some((1.0, 5.0)),
            ..GraphOverlay::default()
        };
        let out = overlay.rescale(&spec);
        assert_eq!((out.x_axis.min, out.x_axis.max), (2.0, 4.0));
        assert_eq!((out.y_axis.min, out.y_axis.max), (1.0, 5.0));
        assert!(
            out.x_axis
                .ticks
                .iter()
                .all(|tick| tick.position >= 2.0 - 1e-9 && tick.position <= 4.0 + 1e-9),
            "ticks outside the window"
        );
        let svg = render_svg_with(&spec, &overlay);
        assert!(svg.contains(">test<"), "{svg}");

        // A probability axis cannot be expressed in raw values: the engine
        // ticks stay as they are.
        let sample = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let fit = Normal::fit_moments(&sample).unwrap();
        let plot = normal_probability_plot(&sample, &fit, PlottingPosition::Benard).unwrap();
        let probability = PlotSpec::normal_probability(&plot, &fit, "Normal");
        let overlay = GraphOverlay {
            y_range: Some((-2.0, 2.0)),
            ..GraphOverlay::default()
        };
        let out = overlay.rescale(&probability);
        assert_eq!(out.y_axis.min, probability.y_axis.min);
    }

    #[test]
    fn data_labels_grid_and_annotation_style_reach_the_svg() {
        let sample = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let fit = Normal::fit_moments(&sample).unwrap();
        let plot = normal_probability_plot(&sample, &fit, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::normal_probability(&plot, &fit, "Normal");
        let overlay = GraphOverlay {
            grid: false,
            data_labels: Some(DataLabelMode::YValue),
            annotations: vec![Annotation {
                text: "marked".to_owned(),
                x: 0.2,
                y: 0.3,
                size: 16.0,
                color: AnnotationColor::Red,
                angle: 90.0,
            }],
            ..GraphOverlay::default()
        };
        let svg = render_svg_with(&spec, &overlay);
        assert!(!svg.contains("#e6e6e6"), "the grid must be off");
        assert!(svg.contains("font-size=\"16.0\""), "{svg}");
        assert!(svg.contains("fill=\"#b02a37\""));
        assert!(svg.contains("rotate(90"));
        // One label per point, as a y value with four decimals.
        let labels = svg.matches("fill=\"#333333\">").count();
        assert!(labels >= spec.points.len(), "{labels} labels");
    }

    #[test]
    fn notes_are_drawn_beside_the_frame() {
        let sample = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let fit = Normal::fit_moments(&sample).unwrap();
        let probability = normal_probability_plot(&sample, &fit, PlottingPosition::Benard).unwrap();
        let mut spec = PlotSpec::normal_probability(&probability, &fit, "Normal");
        let plain = render_svg(&spec);
        let mut note = PlotNote::new(vec![("N", "5".to_owned()), ("Mean", "<x>".to_owned())]);
        note.lines.push("units: mm".to_owned());
        spec.notes.push(note);
        let svg = render_svg(&spec);
        assert!(
            svg.contains(">N</text>"),
            "the label is a column of its own"
        );
        assert!(svg.contains(">5</text>"));
        assert!(
            svg.contains(">&lt;x&gt;</text>"),
            "the note must be escaped"
        );
        assert!(svg.contains("units: mm"));
        // The note sits to the right of the plot rectangle, never over it.
        let plot_right = 800.0 - 30.0 - note_gutter(&spec.notes, 1.0);
        let note_x: f64 = svg
            .split("<text x=\"")
            .find_map(|chunk| {
                let x = chunk.split('"').next()?.parse::<f64>().ok()?;
                chunk.contains(">N</text>").then_some(x)
            })
            .expect("the note text");
        assert!(
            note_x >= plot_right,
            "the note starts at {note_x}, inside the frame that ends at {plot_right}"
        );
        // The plot area gives up exactly the gutter.
        let width_without: f64 = plain
            .split("<rect x=\"70.0\" y=\"50.0\" width=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .and_then(|w| w.parse().ok())
            .expect("plot width");
        let width_with: f64 = svg
            .split("<rect x=\"70.0\" y=\"50.0\" width=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .and_then(|w| w.parse().ok())
            .expect("plot width");
        assert!(
            (width_without - width_with - note_gutter(&spec.notes, 1.0)).abs() < 0.2,
            "the frame shrank by {} instead of {}",
            width_without - width_with,
            note_gutter(&spec.notes, 1.0)
        );
        // No white box: the margin is already empty.
        assert!(!svg.contains("fill-opacity=\"0.88\""));
    }

    #[test]
    fn annotations_are_placed_at_the_chosen_fraction() {
        let sample = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let fit = Normal::fit_moments(&sample).unwrap();
        let plot = normal_probability_plot(&sample, &fit, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::normal_probability(&plot, &fit, "Normal");
        let overlay = GraphOverlay {
            annotations: vec![
                Annotation {
                    text: "line <A>".to_owned(),
                    x: 0.5,
                    y: 0.2,
                    ..Annotation::default()
                },
                Annotation {
                    text: "fixed".to_owned(),
                    // Out-of-range fractions are clamped to the plot.
                    x: 4.0,
                    y: -1.0,
                    ..Annotation::default()
                },
            ],
            ..GraphOverlay::default()
        };
        let svg = render_svg_with(&spec, &overlay);
        assert!(svg.contains("line &lt;A&gt;"), "the text must be escaped");
        // 50 % of the 800 px wide plot starting at 70.
        let x: f64 = svg
            .split("<text x=\"")
            .find_map(|chunk| {
                let value = chunk.split('"').next()?.parse::<f64>().ok()?;
                chunk.contains("line &lt;A&gt;").then_some(value)
            })
            .expect("the annotation");
        assert!((x - 470.0).abs() < 0.2, "annotation at {x}");
        // A fraction past the edge lands on the edge, not outside.
        let fixed_x: f64 = svg
            .split("<text x=\"")
            .find_map(|chunk| {
                let value = chunk.split('"').next()?.parse::<f64>().ok()?;
                chunk.contains(">fixed<").then_some(value)
            })
            .expect("the clamped annotation");
        assert!((fixed_x - 870.0).abs() < 0.2, "clamped at {fixed_x}");
        // The white label keeps the text readable over the data.
        assert!(svg.contains("fill-opacity=\"0.85\""));
    }

    #[test]
    fn the_graph_text_family_and_size_reach_the_svg() {
        let sample = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let fit = Normal::fit_moments(&sample).unwrap();
        let plot = normal_probability_plot(&sample, &fit, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::normal_probability(&plot, &fit, "Normal");
        let mut overlay = GraphOverlay {
            font_family: GraphFontFamily::Mono,
            font_scale: 1.25,
            ..GraphOverlay::default()
        };
        let svg = render_svg_with(&spec, &overlay);
        assert!(svg.contains("DejaVu Sans Mono, monospace"), "{svg}");
        assert!(!svg.contains("sans-serif"), "the family is replaced");
        assert!(
            svg.contains("font-size=\"20\""),
            "the 16 pt title becomes 20 pt"
        );
        assert!(svg.contains("font-size=\"15\""), "12 pt becomes 15 pt");
        assert!(
            !svg.contains("font-size=\"font-size="),
            "scaled font sizes must not duplicate the attribute"
        );

        // The serif family and the default size.
        overlay.font_family = GraphFontFamily::Serif;
        overlay.font_scale = 1.0;
        let serif = render_svg_with(&spec, &overlay);
        assert!(serif.contains("DejaVu Serif, serif"));
        assert!(serif.contains("font-size=\"16\""));

        // A bigger text needs a wider note gutter, so the plot gets narrower.
        let mut with_note = spec.clone();
        with_note
            .notes
            .push(PlotNote::new(vec![("Mean", "12.5".to_owned())]));
        let narrow = render_svg_with(&with_note, &GraphOverlay::default());
        overlay.font_scale = 2.0;
        let wide = render_svg_with(&with_note, &overlay);
        let width = |svg: &str| {
            svg.split("<rect x=\"70.0\" y=\"50.0\" width=\"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .and_then(|w| w.parse::<f64>().ok())
                .expect("plot width")
        };
        assert!(
            width(&wide) < width(&narrow),
            "the gutter must grow with the text"
        );
    }

    #[test]
    fn presentation_edits_reach_every_panel() {
        let make = |values: Vec<f64>, label: &str| {
            let sample = Sample::new(values, MissingPolicy::Drop).unwrap();
            let fit = Normal::fit_moments(&sample).unwrap();
            let plot = normal_probability_plot(&sample, &fit, PlottingPosition::Benard).unwrap();
            PlotSpec::normal_probability(&plot, &fit, label)
        };
        let spec = panel_plot(
            vec![
                make(vec![1.0, 2.0, 3.0, 4.0, 5.0], "A"),
                make(vec![6.0, 7.0, 8.0, 9.0, 10.0], "B"),
            ],
            1,
            "Separate panels",
        )
        .expect("panels");
        let overlay = GraphOverlay {
            subtitle: Some("Design = Tissue 50um".to_owned()),
            footer: Some("Night shift".to_owned()),
            reference_lines: vec![ReferenceLine {
                axis: ReferenceAxis::Horizontal,
                value: 0.0,
                label: Some("median".to_owned()),
            }],
            ..GraphOverlay::default()
        };
        let svg = render_svg_with(&spec, &overlay);
        assert_eq!(
            svg.matches("Design = Tissue 50um").count(),
            1,
            "the subtitle belongs to the whole grid"
        );
        assert_eq!(svg.matches("Night shift").count(), 1);
        assert_eq!(
            svg.matches("stroke-dasharray=\"4 3\"").count(),
            2,
            "the reference line must be drawn in both panels"
        );
        assert!(svg.contains(">median<"));
        // Without the overlay none of it appears.
        let plain = render_svg(&spec);
        assert!(!plain.contains("Design = Tissue 50um"));
        assert!(!plain.contains("stroke-dasharray"));
    }

    #[test]
    fn the_overlay_draws_the_subtitle_footer_and_reference_lines() {
        let sample = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let fit = Normal::fit_moments(&sample).unwrap();
        let probability = normal_probability_plot(&sample, &fit, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::normal_probability(&probability, &fit, "Normal");
        let overlay = GraphOverlay {
            subtitle: Some("Normal - 95% CI".to_owned()),
            footer: Some("Design = Tissue 50um".to_owned()),
            reference_lines: vec![
                ReferenceLine {
                    axis: ReferenceAxis::Vertical,
                    value: 2.0,
                    label: Some("-20".to_owned()),
                },
                ReferenceLine {
                    axis: ReferenceAxis::Horizontal,
                    value: 1.0,
                    label: None,
                },
            ],
            ..GraphOverlay::default()
        };
        let svg = render_svg_with(&spec, &overlay);
        assert!(svg.contains("Normal - 95% CI"));
        assert!(svg.contains("Design = Tissue 50um"));
        assert!(svg.contains(">-20<"));
        assert!(svg.contains("stroke-dasharray=\"4 3\""));
        assert_eq!(svg.matches("stroke-dasharray").count(), 2);
        // Without the overlay nothing of it appears.
        let plain = render_svg(&spec);
        assert!(!plain.contains("Normal - 95% CI"));
        assert!(!plain.contains("stroke-dasharray"));
    }

    #[test]
    fn svg_is_well_formed_and_deterministic() {
        let s = Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).unwrap();
        let d = Normal::fit_moments(&s).unwrap();
        let p = normal_probability_plot(&s, &d, PlottingPosition::Benard).unwrap();
        let spec = PlotSpec::normal_probability(&p, &d, "Normal <test> & co");
        let a = render_svg(&spec);
        let b = render_svg(&spec);
        assert_eq!(a, b, "SVG not deterministic");
        assert!(a.starts_with("<svg xmlns"));
        assert!(a.ends_with("</svg>\n"));
        assert_eq!(a.matches("<circle").count(), 5);
        assert!(a.contains("&lt;test&gt; &amp; co"), "title not escaped");
        assert!(a.contains(">Data<"), "x axis title missing");
        assert!(a.contains(">Percent<"), "y axis title missing");
        assert!(a.contains("<clipPath"), "plot clip missing");
        assert!(a.contains("<path"), "confidence band missing");
    }

    #[test]
    fn svg_with_degenerate_range_does_not_panic() {
        let spec = PlotSpec {
            title: "x".into(),
            x_axis: AxisSpec {
                label: "x".into(),
                scale: Scale::Linear,
                min: 1.0,
                max: 1.0,
                ticks: vec![],
                minor: Vec::new(),
            },
            y_axis: AxisSpec {
                label: "y".into(),
                scale: Scale::Linear,
                min: 2.0,
                max: 2.0,
                ticks: vec![],
                minor: Vec::new(),
            },
            size: (100, 100),
            points: vec![Point { x: 1.0, y: 2.0 }],
            lines: vec![],
            bands: vec![],
            series: vec![],
            panels: vec![],
            panel_columns: 1,
            secondary: None,
            notes: Vec::new(),
        };
        let svg = render_svg(&spec);
        assert!(svg.contains("<svg"));
        assert!(!svg.contains("NaN"), "SVG contains NaN");
    }

    #[test]
    fn a_large_probability_plot_is_thinned_with_a_note() {
        let n = 10_001usize;
        let values: Vec<f64> = (0..n).map(|index| (index % 97) as f64 + 0.25).collect();
        let sample = Sample::new(values, MissingPolicy::Drop).expect("sample");
        let dist = Normal::fit_moments(&sample).expect("fit");
        let plot = normal_probability_plot(&sample, &dist, PlottingPosition::Benard).expect("plot");
        let spec = PlotSpec::normal_probability(&plot, &dist, "Normal");
        assert!(spec.points.len() <= MAX_PLOT_POINTS);
        assert!(spec.points.len() < n);
        assert!(!spec.notes.is_empty(), "the sample must be explained");
        // The fit and the band keep the full sample size.
        assert_eq!(plot.observed.len(), n);
        assert_eq!(plot.positions.len(), n);
    }
}
