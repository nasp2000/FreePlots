// SPDX-License-Identifier: MIT
//! Attribute capability: parts per million defective and defects.
//!
//! The binomial analysis pools the defectives over the inspected units, so it
//! reports the average proportion defective, its PPM and the normal-equivalent
//! process Z. The Poisson analysis pools the defects over the units, so it
//! reports the defects per unit (DPU), its PPM and the same normal equivalent.
//! Both intervals are exact: Clopper–Pearson for the proportion and the
//! chi-square (Poisson) interval for the rate.
//!
//! The counts and denominators are read by [`crate::attribute::rows`], the same
//! validation the P and U charts use, and the plot is the matching attribute
//! chart.

use crate::attribute::{self, AttributeChartKind};
use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::PlotNote;
use crate::report::{AnalysisResult, Item};
use crate::special::{chi_square_quantile, normal_quantile};
use crate::workflow::AnalysisOutput;

/// Capability family of an attribute study.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeCapabilityKind {
    /// Proportion of defective units (binomial).
    Binomial,
    /// Defects per unit (Poisson).
    Poisson,
}

impl AttributeCapabilityKind {
    /// Short name used in titles and menus.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Binomial => "Binomial",
            Self::Poisson => "Poisson",
        }
    }

    /// Long description for the report.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::Binomial => {
                "Proportion defective; pooled binomial estimate with an exact interval"
            }
            Self::Poisson => "Defects per unit; pooled Poisson estimate with an exact interval",
        }
    }

    /// Maps a chart family to the capability family it estimates.
    #[must_use]
    pub fn from_chart(kind: AttributeChartKind) -> Self {
        match kind {
            AttributeChartKind::P
            | AttributeChartKind::Np
            | AttributeChartKind::LaneyP
            | AttributeChartKind::LaneyNp => Self::Binomial,
            AttributeChartKind::C
            | AttributeChartKind::U
            | AttributeChartKind::LaneyC
            | AttributeChartKind::LaneyU => Self::Poisson,
        }
    }
}

/// Runs a binomial or Poisson capability analysis.
///
/// # Errors
///
/// Returns [`Error::EmptySample`] when no row carries a complete pair,
/// [`Error::Cell`] for a negative or fractional count, a defectives count above
/// the inspected units, or a size that is not a positive integer, and
/// [`Error::InvalidParameter`] for a confidence level outside 50–99.9 %.
pub fn analyze(
    df: &DataFrame,
    kind: AttributeCapabilityKind,
    defects_column: &str,
    sizes_column: Option<&str>,
    constant_size: Option<usize>,
    confidence: f64,
) -> Result<AnalysisOutput> {
    if !confidence.is_finite() || !(0.5..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence level",
            value: confidence,
        });
    }
    let data = attribute::rows(
        df,
        defects_column,
        sizes_column,
        constant_size,
        true,
        kind == AttributeCapabilityKind::Binomial,
    )?;
    let total_defects: f64 = data.counts.iter().sum();
    let total_size: f64 = data.sizes.iter().sum();
    let subgroups = data.counts.len();
    let estimate = total_defects / total_size;
    let ppm = estimate * 1_000_000.0;
    let alpha = 1.0 - confidence;

    let (lower, upper, ppm_lower, ppm_upper, process_z) = match kind {
        AttributeCapabilityKind::Binomial => {
            let defectives = total_defects.round() as usize;
            let inspected = total_size.round() as usize;
            let (lower, upper) =
                crate::proportion::exact_interval(defectives, inspected, confidence);
            let process_z = if estimate > 0.0 && estimate < 1.0 {
                Some(normal_quantile(1.0 - estimate))
            } else {
                None
            };
            (lower, upper, lower * 1e6, upper * 1e6, process_z)
        }
        AttributeCapabilityKind::Poisson => {
            let defects = total_defects;
            let lower = if defects <= 0.0 {
                0.0
            } else {
                chi_square_quantile(alpha / 2.0, 2.0 * defects) / (2.0 * total_size)
            };
            let upper =
                chi_square_quantile(1.0 - alpha / 2.0, 2.0 * (defects + 1.0)) / (2.0 * total_size);
            let process_z = if estimate > 0.0 && estimate < 1.0 {
                Some(normal_quantile(1.0 - estimate))
            } else {
                None
            };
            (lower, upper, lower * 1e6, upper * 1e6, process_z)
        }
    };

    let title = format!(
        "Attribute Capability: {} — column {defects_column}",
        kind.name()
    );
    let mut result = AnalysisResult::new(
        title,
        format!(
            "{}; exact interval at {:.1}%",
            kind.description(),
            confidence * 100.0
        ),
    );
    result.n = subgroups;
    result.excluded = data.excluded;
    match kind {
        AttributeCapabilityKind::Binomial => {
            result.statistics = vec![
                ("Total defectives", total_defects),
                ("Total inspected", total_size),
                ("Subgroups", subgroups as f64),
                ("Average p", estimate),
                ("%Defective", estimate * 100.0),
                ("PPM defective", ppm),
                ("CI lower (p)", lower),
                ("CI upper (p)", upper),
                ("PPM lower", ppm_lower),
                ("PPM upper", ppm_upper),
            ];
        }
        AttributeCapabilityKind::Poisson => {
            let per_subgroup = total_defects / subgroups as f64;
            let minimum = data
                .counts
                .iter()
                .zip(&data.sizes)
                .map(|(count, size)| count / size)
                .fold(f64::INFINITY, f64::min);
            let maximum = data
                .counts
                .iter()
                .zip(&data.sizes)
                .map(|(count, size)| count / size)
                .fold(f64::NEG_INFINITY, f64::max);
            result.statistics = vec![
                ("Total defects", total_defects),
                ("Total units", total_size),
                ("Subgroups", subgroups as f64),
                ("Mean defects per subgroup", per_subgroup),
                ("Mean DPU", estimate),
                ("Minimum DPU", minimum),
                ("Maximum DPU", maximum),
                ("PPM defects", ppm),
                ("CI lower (DPU)", lower),
                ("CI upper (DPU)", upper),
                ("PPM lower", ppm_lower),
                ("PPM upper", ppm_upper),
            ];
        }
    }
    if let Some(z) = process_z {
        result.statistics.push(("Process Z", z));
    }

    let headers = match kind {
        AttributeCapabilityKind::Binomial => {
            vec!["Subgroup", "Defectives", "Inspected", "Proportion"]
        }
        AttributeCapabilityKind::Poisson => vec!["Subgroup", "Defects", "Units", "DPU"],
    };
    result.extra.push(Item::Table {
        headers: headers.into_iter().map(str::to_owned).collect(),
        rows: data
            .counts
            .iter()
            .zip(&data.sizes)
            .enumerate()
            .map(|(index, (count, size))| {
                vec![
                    (index + 1).to_string(),
                    format!("{count:.0}"),
                    format!("{size:.0}"),
                    format!("{:.6}", count / size),
                ]
            })
            .collect(),
    });

    let (estimate_name, unit) = match kind {
        AttributeCapabilityKind::Binomial => ("average p", "inspected units"),
        AttributeCapabilityKind::Poisson => ("mean DPU", "units"),
    };
    result.notes = vec![
        (
            "Estimator".to_owned(),
            format!(
                "the {estimate_name} pools every retained row: total counts divided by the \
                 total {unit}"
            ),
        ),
        (
            "PPM".to_owned(),
            format!(
                "PPM = 1,000,000 × the {estimate_name}; the interval transforms the exact \
                 interval of the estimate"
            ),
        ),
        (
            "Process Z".to_owned(),
            "the standard-normal deviate with the estimated defective or defect area to its \
             right (Φ⁻¹(1 − estimate)); infinite at zero and not defined above one"
                .to_owned(),
        ),
        (
            "Interval".to_owned(),
            match kind {
                AttributeCapabilityKind::Binomial => {
                    "Clopper–Pearson (beta quantiles) at the stated confidence".to_owned()
                }
                AttributeCapabilityKind::Poisson => {
                    "chi-square quantiles divided by twice the total units".to_owned()
                }
            },
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing count or size are excluded and counted".to_owned(),
        ),
    ];
    if process_z.is_none() {
        result.warnings.push(match kind {
            AttributeCapabilityKind::Binomial => {
                "the estimate is zero or one, so process Z is not finite".to_owned()
            }
            AttributeCapabilityKind::Poisson => {
                "the mean DPU is zero or at least one per unit, so process Z is not finite"
                    .to_owned()
            }
        });
    }

    // The matching P or U chart shows whether the per-subgroup rates are stable.
    let chart_kind = match kind {
        AttributeCapabilityKind::Binomial => AttributeChartKind::P,
        AttributeCapabilityKind::Poisson => AttributeChartKind::U,
    };
    let mut plot = attribute::analyze(df, chart_kind, defects_column, sizes_column, constant_size)?
        .plot
        .ok_or_else(|| {
            Error::Unsupported("the attribute capability chart could not be drawn".to_owned())
        })?;
    plot.title = format!("{} Capability — {defects_column}", kind.name());
    plot.notes.push(PlotNote::new(vec![
        ("PPM", format!("{ppm:.0}")),
        (
            "Process Z",
            process_z.map_or_else(|| "—".to_owned(), |z| format!("{z:.3}")),
        ),
    ]));

    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}
