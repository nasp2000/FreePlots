//! Attribute control charts: p, np, c and u.
//!
//! P and NP are based on the binomial distribution; C and U on the Poisson
//! distribution. The limits are the classical three-sigma limits. Only the
//! outside-limit test is applied, because the runs rules for variable-size
//! subgroups are not part of the recovered evidence. Every report states this
//! limitation.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item};
use crate::workflow::AnalysisOutput;

/// Attribute chart family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeChartKind {
    /// Proportion defective (variable subgroup size allowed).
    P,
    /// Number defective (constant subgroup size).
    Np,
    /// Defects per sample (constant opportunity).
    C,
    /// Defects per unit (variable sample size allowed).
    U,
    /// Proportion defective with Laney limits (overdispersion adjusted).
    LaneyP,
    /// Number defective with Laney limits (constant subgroup size).
    LaneyNp,
    /// Defects per sample with Laney limits (constant opportunity).
    LaneyC,
    /// Defects per unit with Laney limits (variable sample size allowed).
    LaneyU,
}

impl AttributeChartKind {
    /// Short name used in titles and menus.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::P => "P",
            Self::Np => "NP",
            Self::C => "C",
            Self::U => "U",
            Self::LaneyP => "P\u{2032}",
            Self::LaneyNp => "NP\u{2032}",
            Self::LaneyC => "C\u{2032}",
            Self::LaneyU => "U\u{2032}",
        }
    }

    /// Long description for the report.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::P => "Proportion defective; binomial three-sigma limits",
            Self::Np => "Number defective; binomial three-sigma limits",
            Self::C => "Defects per sample; Poisson three-sigma limits",
            Self::U => "Defects per unit; Poisson three-sigma limits",
            Self::LaneyP => "Proportion defective; Laney limits adjusted for overdispersion",
            Self::LaneyNp => "Number defective; Laney limits adjusted for overdispersion",
            Self::LaneyC => "Defects per sample; Laney limits adjusted for overdispersion",
            Self::LaneyU => "Defects per unit; Laney limits adjusted for overdispersion",
        }
    }

    /// Parses the CLI spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "p" | "p-chart" => Some(Self::P),
            "np" | "np-chart" => Some(Self::Np),
            "c" | "c-chart" => Some(Self::C),
            "u" | "u-chart" => Some(Self::U),
            "laney-p" | "p-prime" | "pprime" => Some(Self::LaneyP),
            "laney-np" | "np-prime" | "npprime" => Some(Self::LaneyNp),
            "laney-c" | "c-prime" | "cprime" => Some(Self::LaneyC),
            "laney-u" | "u-prime" | "uprime" => Some(Self::LaneyU),
            _ => {
                // A trailing prime selects the Laney variant of the base chart.
                let base = text.trim_end_matches(['\'', '\u{2032}']).trim_end();
                match base.to_ascii_lowercase().replace('_', "-").as_str() {
                    "p" | "p-chart" => Some(Self::LaneyP),
                    "np" | "np-chart" => Some(Self::LaneyNp),
                    "c" | "c-chart" => Some(Self::LaneyC),
                    "u" | "u-chart" => Some(Self::LaneyU),
                    _ => None,
                }
            }
        }
    }

    /// Whether the chart uses a denominator (subgroup size).
    #[must_use]
    pub fn needs_size(self) -> bool {
        matches!(
            self,
            Self::P | Self::Np | Self::U | Self::LaneyP | Self::LaneyNp | Self::LaneyU
        )
    }

    /// Whether the chart plots a proportion/rate rather than a count.
    #[must_use]
    pub fn is_rate(self) -> bool {
        matches!(self, Self::P | Self::U | Self::LaneyP | Self::LaneyU)
    }
}

/// One special-cause signal on an attribute chart.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeSignal {
    /// Test number (1–4).
    pub test: u8,
    /// One-based point index.
    pub index: usize,
    /// Spelled-out reason.
    pub description: String,
}

/// Ready-to-report attribute chart.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeChart {
    /// Chart family.
    pub kind: AttributeChartKind,
    /// Plotted values, one per retained row.
    pub points: Vec<f64>,
    /// Denominator per retained row (1 for C).
    pub sizes: Vec<f64>,
    /// Center line.
    pub center: f64,
    /// Upper control limits, one per point.
    pub ucl: Vec<f64>,
    /// Lower control limits, one per point.
    pub lcl: Vec<f64>,
    /// Non-finite or incomplete rows excluded.
    pub excluded: usize,
    /// Overdispersion factor (Laney charts only).
    pub sigma_z: Option<f64>,
    /// Standardised values used to estimate the overdispersion factor.
    pub z_values: Vec<f64>,
    /// Special-cause signals found on the chart.
    pub signals: Vec<AttributeSignal>,
    /// Estimator notes.
    pub notes: Vec<(String, String)>,
    /// Estimator warnings.
    pub warnings: Vec<String>,
}

/// Runs an attribute control chart.
pub fn analyze(
    df: &DataFrame,
    kind: AttributeChartKind,
    defects_column: &str,
    sizes_column: Option<&str>,
    constant_size: Option<usize>,
) -> Result<AnalysisOutput> {
    let chart = build(df, kind, defects_column, sizes_column, constant_size)?;
    let result = report(defects_column, &chart);
    let plot = plot(defects_column, &chart);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn numeric(column: &Column, row: usize) -> Option<f64> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => values.get(row).copied().filter(|value| value.is_finite()),
        ColumnData::I64(values) => values.get(row).map(|&value| value as f64),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_) => None,
    }
}

fn size_at(sizes: Option<&Column>, constant: Option<usize>, row: usize) -> Option<f64> {
    if let Some(column) = sizes {
        numeric(column, row)
    } else {
        constant.map(|value| value as f64)
    }
}

fn build(
    df: &DataFrame,
    kind: AttributeChartKind,
    defects_column: &str,
    sizes_column: Option<&str>,
    constant_size: Option<usize>,
) -> Result<AttributeChart> {
    if kind.needs_size() && sizes_column.is_none() && constant_size.is_none() {
        return Err(Error::Unsupported(format!(
            "{} chart requires --sizes <column> or --size <n>",
            kind.name()
        )));
    }
    if let Some(size) = constant_size
        && size == 0
    {
        return Err(Error::InvalidParameter {
            name: "subgroup size",
            value: 0.0,
        });
    }
    let defects = df.column(defects_column)?;
    let sizes = match sizes_column {
        Some(name) => Some(df.column(name)?),
        None => None,
    };
    if let Some(sizes) = sizes
        && sizes.name() == defects_column
    {
        return Err(Error::Unsupported(
            "the defects and sizes columns must be different".to_owned(),
        ));
    }
    let rows = df.n_rows();
    let mut counts = Vec::new();
    let mut denominators = Vec::new();
    for row in 0..rows {
        let Some(value) = numeric(defects, row) else {
            continue;
        };
        if value < 0.0 || value.fract() != 0.0 {
            return Err(Error::Cell {
                column: defects_column.to_owned(),
                row: row + 1,
                message: "counts must be non-negative integers".to_owned(),
            });
        }
        let denominator = if kind.needs_size() {
            let Some(size) = size_at(sizes, constant_size, row) else {
                continue;
            };
            if size <= 0.0 || size.fract() != 0.0 {
                return Err(Error::Cell {
                    column: sizes_column.unwrap_or("<size>").to_owned(),
                    row: row + 1,
                    message: "subgroup sizes must be positive integers".to_owned(),
                });
            }
            if matches!(
                kind,
                AttributeChartKind::P
                    | AttributeChartKind::Np
                    | AttributeChartKind::LaneyP
                    | AttributeChartKind::LaneyNp
            ) && value > size
            {
                return Err(Error::Cell {
                    column: defects_column.to_owned(),
                    row: row + 1,
                    message: "defectives cannot exceed the subgroup size".to_owned(),
                });
            }
            size
        } else {
            1.0
        };
        counts.push(value);
        denominators.push(denominator);
    }
    if counts.is_empty() {
        return Err(Error::EmptySample);
    }

    let k = counts.len();
    let total_defects: f64 = counts.iter().sum();
    let total_size: f64 = denominators.iter().sum();
    let (center, points, ucl, lcl, notes, sigma_z, z_values) = match kind {
        AttributeChartKind::P => {
            let pbar = total_defects / total_size;
            let mut points = Vec::with_capacity(k);
            let mut ucl = Vec::with_capacity(k);
            let mut lcl = Vec::with_capacity(k);
            for (count, size) in counts.iter().zip(&denominators) {
                let p = count / size;
                let sigma = (pbar * (1.0 - pbar) / size).sqrt();
                points.push(p);
                ucl.push(pbar + 3.0 * sigma);
                lcl.push((pbar - 3.0 * sigma).max(0.0));
            }
            (
                pbar,
                points,
                ucl,
                lcl,
                vec![(
                    "Limits".to_owned(),
                    "p̄ ± 3·sqrt(p̄(1−p̄)/nᵢ); lower limit clipped at zero".to_owned(),
                )],
                None,
                Vec::new(),
            )
        }
        AttributeChartKind::Np => {
            let size = denominators[0];
            if denominators
                .iter()
                .any(|value| (*value - size).abs() > 1e-12)
            {
                return Err(Error::Unsupported(
                    "the NP chart requires a constant subgroup size".to_owned(),
                ));
            }
            let pbar = total_defects / total_size;
            let center = size * pbar;
            let sigma = (size * pbar * (1.0 - pbar)).sqrt();
            let ucl = vec![center + 3.0 * sigma; k];
            let lcl = vec![(center - 3.0 * sigma).max(0.0); k];
            (
                center,
                counts.clone(),
                ucl,
                lcl,
                vec![(
                    "Limits".to_owned(),
                    "n·p̄ ± 3·sqrt(n·p̄(1−p̄)); lower limit clipped at zero".to_owned(),
                )],
                None,
                Vec::new(),
            )
        }
        AttributeChartKind::C => {
            let cbar = total_defects / k as f64;
            let sigma = cbar.sqrt();
            let ucl = vec![cbar + 3.0 * sigma; k];
            let lcl = vec![(cbar - 3.0 * sigma).max(0.0); k];
            (
                cbar,
                counts.clone(),
                ucl,
                lcl,
                vec![(
                    "Limits".to_owned(),
                    "c̄ ± 3·sqrt(c̄); lower limit clipped at zero".to_owned(),
                )],
                None,
                Vec::new(),
            )
        }
        AttributeChartKind::U => {
            let ubar = total_defects / total_size;
            let mut points = Vec::with_capacity(k);
            let mut ucl = Vec::with_capacity(k);
            let mut lcl = Vec::with_capacity(k);
            for (count, size) in counts.iter().zip(&denominators) {
                let u = count / size;
                let sigma = (ubar / size).sqrt();
                points.push(u);
                ucl.push(ubar + 3.0 * sigma);
                lcl.push((ubar - 3.0 * sigma).max(0.0));
            }
            (
                ubar,
                points,
                ucl,
                lcl,
                vec![(
                    "Limits".to_owned(),
                    "ū ± 3·sqrt(ū/nᵢ); lower limit clipped at zero".to_owned(),
                )],
                None,
                Vec::new(),
            )
        }
        AttributeChartKind::LaneyP => {
            let pbar = total_defects / total_size;
            let mut points = Vec::with_capacity(k);
            let mut sigmas = Vec::with_capacity(k);
            for (count, size) in counts.iter().zip(&denominators) {
                let p = count / size;
                let sigma = (pbar * (1.0 - pbar) / size).sqrt();
                points.push(p);
                sigmas.push(sigma);
            }
            let (sigma_z, ucl, lcl, z) = laney_limits(&points, &sigmas, pbar);
            (
                pbar,
                points,
                ucl,
                lcl,
                vec![
                    (
                        "Limits".to_owned(),
                        "p\u{0304} \u{00b1} 3\u{00b7}\u{03c3}_z\u{00b7}sqrt(p\u{0304}(1\u{2212}p\u{0304})/n\u{1d62}); \
                         lower limit clipped at zero"
                            .to_owned(),
                    ),
                    (
                        "Overdispersion".to_owned(),
                        "\u{03c3}_z is the average moving range of the standardised values over \
                         1.128, and is 1 when the process is not overdispersed"
                            .to_owned(),
                    ),
                ],
                Some(sigma_z),
                z,
            )
        }
        AttributeChartKind::LaneyNp => {
            let size = denominators[0];
            if denominators
                .iter()
                .any(|value| (*value - size).abs() > 1e-12)
            {
                return Err(Error::Unsupported(
                    "the NP chart requires a constant subgroup size".to_owned(),
                ));
            }
            let pbar = total_defects / total_size;
            let center = size * pbar;
            let sigma = (size * pbar * (1.0 - pbar)).sqrt();
            let points = counts.clone();
            let sigmas = vec![sigma; k];
            let (sigma_z, ucl, lcl, z) = laney_limits(&points, &sigmas, center);
            (
                center,
                points,
                ucl,
                lcl,
                vec![
                    (
                        "Limits".to_owned(),
                        "n\u{00b7}p\u{0304} \u{00b1} 3\u{00b7}\u{03c3}_z\u{00b7}sqrt(n\u{00b7}p\u{0304}(1\u{2212}p\u{0304})); \
                         lower limit clipped at zero"
                            .to_owned(),
                    ),
                    (
                        "Overdispersion".to_owned(),
                        "\u{03c3}_z is the average moving range of the standardised values over \
                         1.128, and is 1 when the process is not overdispersed"
                            .to_owned(),
                    ),
                ],
                Some(sigma_z),
                z,
            )
        }
        AttributeChartKind::LaneyC => {
            let cbar = total_defects / k as f64;
            let sigma = cbar.sqrt();
            let points = counts.clone();
            let sigmas = vec![sigma; k];
            let (sigma_z, ucl, lcl, z) = laney_limits(&points, &sigmas, cbar);
            (
                cbar,
                points,
                ucl,
                lcl,
                vec![
                    (
                        "Limits".to_owned(),
                        "c\u{0304} \u{00b1} 3\u{00b7}\u{03c3}_z\u{00b7}sqrt(c\u{0304}); lower limit \
                         clipped at zero"
                            .to_owned(),
                    ),
                    (
                        "Overdispersion".to_owned(),
                        "\u{03c3}_z is the average moving range of the standardised values over \
                         1.128, and is 1 when the process is not overdispersed"
                            .to_owned(),
                    ),
                ],
                Some(sigma_z),
                z,
            )
        }
        AttributeChartKind::LaneyU => {
            let ubar = total_defects / total_size;
            let mut points = Vec::with_capacity(k);
            let mut sigmas = Vec::with_capacity(k);
            for (count, size) in counts.iter().zip(&denominators) {
                let u = count / size;
                let sigma = (ubar / size).sqrt();
                points.push(u);
                sigmas.push(sigma);
            }
            let (sigma_z, ucl, lcl, z) = laney_limits(&points, &sigmas, ubar);
            (
                ubar,
                points,
                ucl,
                lcl,
                vec![
                    (
                        "Limits".to_owned(),
                        "\u{016b} \u{00b1} 3\u{00b7}\u{03c3}_z\u{00b7}sqrt(\u{016b}/n\u{1d62}); lower \
                         limit clipped at zero"
                            .to_owned(),
                    ),
                    (
                        "Overdispersion".to_owned(),
                        "\u{03c3}_z is the average moving range of the standardised values over \
                         1.128, and is 1 when the process is not overdispersed"
                            .to_owned(),
                    ),
                ],
                Some(sigma_z),
                z,
            )
        }
    };

    let mut signals: Vec<AttributeSignal> = points
        .iter()
        .enumerate()
        .filter(|(index, value)| **value > ucl[*index] + 1e-12 || **value < lcl[*index] - 1e-12)
        .map(|(index, value)| AttributeSignal {
            test: 1,
            index: index + 1,
            description: format!("point outside the control limits ({value:.6})"),
        })
        .collect();
    signals.extend(runs_signals(&points, center));

    Ok(AttributeChart {
        kind,
        points,
        sizes: denominators,
        center,
        ucl,
        lcl,
        sigma_z,
        z_values,
        excluded: rows - k,
        signals,
        notes,
        warnings: vec![],
    })
}

/// Laney limits: the center line is kept and the limits are scaled by the
/// overdispersion factor estimated from the standardised values.
///
/// The factor is the sample standard deviation of the standardised values,
/// floored at one so that a process that is not overdispersed keeps the
/// ordinary three-sigma limits.
fn laney_limits(
    points: &[f64],
    sigmas: &[f64],
    center: f64,
) -> (f64, Vec<f64>, Vec<f64>, Vec<f64>) {
    let z: Vec<f64> = points
        .iter()
        .zip(sigmas)
        .map(|(point, sigma)| {
            if *sigma > 0.0 {
                (point - center) / sigma
            } else {
                0.0
            }
        })
        .collect();
    // Laney (2002): sigma_z is the average moving range of the standardised
    // values divided by d2 = 1.128 for moving ranges of two observations,
    // floored at one so a process that is not overdispersed keeps the
    // ordinary three-sigma limits.
    let sigma_z = if z.len() >= 2 {
        let moving_range = z
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .sum::<f64>()
            / (z.len() - 1) as f64;
        (moving_range / 1.128).max(1.0)
    } else {
        1.0
    };
    let ucl: Vec<f64> = sigmas
        .iter()
        .map(|sigma| center + 3.0 * sigma_z * sigma)
        .collect();
    let lcl: Vec<f64> = sigmas
        .iter()
        .map(|sigma| (center - 3.0 * sigma_z * sigma).max(0.0))
        .collect();
    (sigma_z, ucl, lcl, z)
}

/// Nelson rules 2–4 on the plotted values around the center line.
fn runs_signals(points: &[f64], center: f64) -> Vec<AttributeSignal> {
    let mut signals = Vec::new();
    // Test 2: nine consecutive points on the same side of the centre line.
    let mut streak = 0usize;
    let mut side = 0.0f64;
    for (index, &value) in points.iter().enumerate() {
        let current = if value > center {
            1.0
        } else if value < center {
            -1.0
        } else {
            0.0
        };
        if current != 0.0 && (streak == 0 || current == side) {
            streak += 1;
            side = current;
        } else {
            streak = usize::from(current != 0.0);
            side = current;
        }
        if streak == 9 {
            signals.push(AttributeSignal {
                test: 2,
                index: index + 1,
                description: "nine consecutive points on the same side of the centre line"
                    .to_owned(),
            });
        }
    }
    // Test 3: six consecutive points trending in one direction.
    let mut run = 1usize;
    let mut direction = 0.0f64;
    for index in 1..points.len() {
        let step = points[index] - points[index - 1];
        if step == 0.0 {
            run = 1;
            direction = 0.0;
            continue;
        }
        if direction != 0.0 && step.signum() != direction.signum() {
            // The reversal point and the current point start a new trend.
            run = 2;
        } else {
            run += 1;
        }
        direction = step;
        if run == 6 {
            signals.push(AttributeSignal {
                test: 3,
                index: index + 1,
                description: "six consecutive points trending in one direction".to_owned(),
            });
        }
    }
    // Test 4: fourteen consecutive points alternating up and down.
    let mut alternation = 1usize;
    for index in 2..points.len() {
        let first = points[index - 1] - points[index - 2];
        let second = points[index] - points[index - 1];
        if second == 0.0 {
            alternation = 1;
        } else if first == 0.0 || first.signum() == second.signum() {
            // The current step alone starts a new alternation (two points).
            alternation = 2;
        } else {
            alternation += 1;
        }
        if alternation == 14 {
            signals.push(AttributeSignal {
                test: 4,
                index: index + 1,
                description: "fourteen consecutive points alternating up and down".to_owned(),
            });
        }
    }
    signals
}

fn report(defects_column: &str, chart: &AttributeChart) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("{} Chart — column {defects_column}", chart.kind.name()),
        chart.kind.description(),
    );
    result.n = chart.points.len();
    result.excluded = chart.excluded;
    result.statistics = vec![
        ("Center line", chart.center),
        ("Points", chart.points.len() as f64),
        (
            "Points outside limits",
            chart
                .signals
                .iter()
                .filter(|signal| signal.test == 1)
                .count() as f64,
        ),
        ("Signals", chart.signals.len() as f64),
    ];
    if chart.kind.needs_size() {
        result.statistics.push((
            "Mean subgroup size",
            chart.sizes.iter().sum::<f64>() / chart.sizes.len().max(1) as f64,
        ));
    }
    if let Some(sigma_z) = chart.sigma_z {
        result
            .statistics
            .push(("Overdispersion factor (sigma_z)", sigma_z));
    }
    let ucl_min = chart.ucl.iter().copied().fold(f64::INFINITY, f64::min);
    let ucl_max = chart.ucl.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let lcl_min = chart.lcl.iter().copied().fold(f64::INFINITY, f64::min);
    let lcl_max = chart.lcl.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    result.extra.push(Item::Table {
        headers: vec![
            "Chart".to_owned(),
            "Center".to_owned(),
            "LCL range".to_owned(),
            "UCL range".to_owned(),
        ],
        rows: vec![vec![
            chart.kind.name().to_owned(),
            format!("{:.6}", chart.center),
            format!("{lcl_min:.6} – {lcl_max:.6}"),
            format!("{ucl_min:.6} – {ucl_max:.6}"),
        ]],
    });
    if let Some(sigma_z) = chart.sigma_z {
        let rows = chart
            .points
            .iter()
            .enumerate()
            .map(|(index, point)| {
                let z = chart.z_values.get(index).copied().unwrap_or(0.0);
                vec![
                    (index + 1).to_string(),
                    format!("{point:.6}"),
                    format!("{z:.4}"),
                    format!("{:.6}", 1.0 - crate::special::normal_cdf(z)),
                    format!("{:.6}", chart.lcl[index]),
                    format!("{:.6}", chart.ucl[index]),
                ]
            })
            .collect();
        result.extra.push(Item::Table {
            headers: vec![
                "Subgroup".to_owned(),
                "Value".to_owned(),
                "Standardised".to_owned(),
                "p\u{2032}".to_owned(),
                "LCL".to_owned(),
                "UCL".to_owned(),
            ],
            rows,
        });
        result.notes.push((
            "Laney limits".to_owned(),
            format!(
                "\u{03c3}_z = {sigma_z:.4}; the three-sigma limits are scaled by \u{03c3}_z and the \
                 p\u{2032} column is 1 \u{2212} \u{03a6}(z) for the standardised value z"
            ),
        ));
    }
    if chart.signals.is_empty() {
        result
            .extra
            .push(Item::Message("No special-cause signals.".to_owned()));
    } else {
        result.extra.push(Item::Table {
            headers: vec![
                "Test".to_owned(),
                "Point".to_owned(),
                "Value".to_owned(),
                "LCL".to_owned(),
                "UCL".to_owned(),
                "Signal".to_owned(),
            ],
            rows: chart
                .signals
                .iter()
                .map(|signal| {
                    let index = signal.index - 1;
                    vec![
                        signal.test.to_string(),
                        signal.index.to_string(),
                        format!("{:.6}", chart.points[index]),
                        format!("{:.6}", chart.lcl[index]),
                        format!("{:.6}", chart.ucl[index]),
                        signal.description.clone(),
                    ]
                })
                .collect(),
        });
    }
    result.notes = vec![
        (
            "Estimator".to_owned(),
            match chart.kind {
                AttributeChartKind::P => "p̄ = total defectives / total inspected".to_owned(),
                AttributeChartKind::Np => "p̄ = total defectives / (n·k), center = n·p̄".to_owned(),
                AttributeChartKind::C => "c̄ = mean defects per sample".to_owned(),
                AttributeChartKind::U => "ū = total defects / total units".to_owned(),
                AttributeChartKind::LaneyP => {
                    "p̄ = total defectives / total inspected; limits scaled by σ_z".to_owned()
                }
                AttributeChartKind::LaneyNp => {
                    "p̄ = total defectives / total inspected; center = n·p̄; limits scaled by σ_z"
                        .to_owned()
                }
                AttributeChartKind::LaneyC => {
                    "c̄ = mean defects per sample; limits scaled by σ_z".to_owned()
                }
                AttributeChartKind::LaneyU => {
                    "ū = total defects / total units; limits scaled by σ_z".to_owned()
                }
            },
        ),
        (
            "Tests".to_owned(),
            "tests 1–4 are applied: outside limits, nine on one side, six trending and \
             fourteen alternating"
                .to_owned(),
        ),
    ];
    result.notes.extend(chart.notes.clone());
    result.warnings = chart.warnings.clone();
    result.warnings.push(
        "attribute-chart defaults and exact test set have not been verified against documented \
         documented output"
            .to_owned(),
    );
    result
}

fn plot(defects_column: &str, chart: &AttributeChart) -> PlotSpec {
    let k = chart.points.len();
    let x_min = 1.0;
    let x_max = k.max(2) as f64;
    let mut y_min = chart.points.iter().copied().fold(chart.center, f64::min);
    let mut y_max = chart.points.iter().copied().fold(chart.center, f64::max);
    for value in chart.ucl.iter().chain(&chart.lcl) {
        y_min = y_min.min(*value);
        y_max = y_max.max(*value);
    }
    let pad = ((y_max - y_min).abs() * 0.05).max(1e-9);
    y_min -= pad;
    y_max += pad;

    let mut lines = vec![Segment {
        from: Point {
            x: x_min,
            y: chart.center,
        },
        to: Point {
            x: x_max,
            y: chart.center,
        },
        width: 1.5,
    }];
    if k > 1 {
        for i in 1..k {
            lines.push(Segment {
                from: Point {
                    x: i as f64,
                    y: chart.ucl[i - 1],
                },
                to: Point {
                    x: (i + 1) as f64,
                    y: chart.ucl[i],
                },
                width: 1.2,
            });
            lines.push(Segment {
                from: Point {
                    x: i as f64,
                    y: chart.lcl[i - 1],
                },
                to: Point {
                    x: (i + 1) as f64,
                    y: chart.lcl[i],
                },
                width: 1.2,
            });
        }
    } else {
        lines.push(Segment {
            from: Point {
                x: x_min,
                y: chart.ucl[0],
            },
            to: Point {
                x: x_max,
                y: chart.ucl[0],
            },
            width: 1.2,
        });
        lines.push(Segment {
            from: Point {
                x: x_min,
                y: chart.lcl[0],
            },
            to: Point {
                x: x_max,
                y: chart.lcl[0],
            },
            width: 1.2,
        });
    }

    PlotSpec {
        title: format!("{} Chart — {defects_column}", chart.kind.name()),
        x_axis: linear_axis("Subgroup", x_min, x_max),
        y_axis: linear_axis(
            match chart.kind {
                AttributeChartKind::P => "Proportion",
                AttributeChartKind::Np => "Defectives",
                AttributeChartKind::C => "Defects",
                AttributeChartKind::U => "Defects per unit",
                AttributeChartKind::LaneyP => "Proportion",
                AttributeChartKind::LaneyNp => "Defectives",
                AttributeChartKind::LaneyC => "Defects",
                AttributeChartKind::LaneyU => "Defects per unit",
            },
            y_min,
            y_max,
        ),
        size: (900, 400),
        points: chart
            .points
            .iter()
            .enumerate()
            .map(|(index, &value)| Point {
                x: (index + 1) as f64,
                y: value,
            })
            .collect(),
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: Vec::new(),
    }
}
