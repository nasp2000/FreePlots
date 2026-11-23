// SPDX-License-Identifier: MIT
//! Attribute capability: parts per million defective and defects.
//!
//! The binomial analysis pools the defectives over the inspected units, so it
//! reports the average proportion defective, its PPM, the yield and the
//! normal-equivalent process Z. The Poisson analysis pools the defects over the
//! units, so it reports the defects per unit (DPU), its PPM, the Poisson yield
//! and the same normal equivalent. With opportunities per unit the reports add
//! the defects per million opportunities (DPMO) and take process Z from that
//! per-opportunity rate. Both intervals are exact: Clopper–Pearson for the
//! proportion and the chi-square (Poisson) interval for the rate.
//!
//! The counts and denominators are read by [`crate::attribute::rows`], the same
//! validation the P and U charts use, and the plot is the matching attribute
//! chart.

use crate::attribute::{self, AttributeChartKind};
use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::PlotNote;
use crate::report::{AnalysisResult, Item};

/// Rows the subgroup table lists before it summarises the rest.
///
/// The Session window sticks to the bottom of the text, so an unbounded table
/// would push the statistics (PPM, DPMO, yield and process Z) off the screen.
const SUBGROUP_TABLE_ROWS: usize = 10;
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
/// [`Error::InvalidParameter`] for a confidence level outside 50–99.9 % or
/// for zero opportunities per unit.
pub fn analyze(
    df: &DataFrame,
    kind: AttributeCapabilityKind,
    defects_column: &str,
    sizes_column: Option<&str>,
    constant_size: Option<usize>,
    confidence: f64,
    opportunities: usize,
) -> Result<AnalysisOutput> {
    if !confidence.is_finite() || !(0.5..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence level",
            value: confidence,
        });
    }
    if opportunities == 0 {
        return Err(Error::InvalidParameter {
            name: "opportunities per unit",
            value: 0.0,
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
    let per_opportunity = estimate / opportunities as f64;
    let dpm = per_opportunity * 1_000_000.0;
    let alpha = 1.0 - confidence;

    let (lower, upper, ppm_lower, ppm_upper, process_z) = match kind {
        AttributeCapabilityKind::Binomial => {
            let defectives = total_defects.round() as usize;
            let inspected = total_size.round() as usize;
            let (lower, upper) =
                crate::proportion::exact_interval(defectives, inspected, confidence);
            let process_z = (per_opportunity > 0.0 && per_opportunity < 1.0)
                .then(|| normal_quantile(1.0 - per_opportunity));
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
            let process_z = (per_opportunity > 0.0 && per_opportunity < 1.0)
                .then(|| normal_quantile(1.0 - per_opportunity));
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
    result
        .statistics
        .push(("Opportunities per unit", opportunities as f64));
    result.statistics.push(("DPMO", dpm));
    let yield_value = match kind {
        AttributeCapabilityKind::Binomial => 1.0 - estimate,
        AttributeCapabilityKind::Poisson => (-estimate).exp(),
    };
    result.statistics.push(("Yield", yield_value));
    if let Some(z) = process_z {
        result.statistics.push(("Process Z", z));
    }

    let headers = match kind {
        AttributeCapabilityKind::Binomial => {
            vec!["Subgroup", "Defectives", "Inspected", "Proportion"]
        }
        AttributeCapabilityKind::Poisson => vec!["Subgroup", "Defects", "Units", "DPU"],
    };
    let listed = data.counts.len().min(SUBGROUP_TABLE_ROWS);
    let mut rows: Vec<Vec<String>> = data
        .counts
        .iter()
        .zip(&data.sizes)
        .take(listed)
        .enumerate()
        .map(|(index, (count, size))| {
            vec![
                (index + 1).to_string(),
                format!("{count:.0}"),
                format!("{size:.0}"),
                format!("{:.6}", count / size),
            ]
        })
        .collect();
    if data.counts.len() > listed {
        let rest = data.counts.len() - listed;
        rows.push(vec![
            format!("… {rest} more"),
            String::new(),
            String::new(),
            String::new(),
        ]);
    }
    result.extra.push(Item::Table {
        headers: headers.into_iter().map(str::to_owned).collect(),
        rows,
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
            "DPMO".to_owned(),
            "defects per million opportunities: the count divided by the units times the \
             opportunities per unit (DPMO = PPM when each unit has one opportunity)"
                .to_owned(),
        ),
        (
            "Yield".to_owned(),
            match kind {
                AttributeCapabilityKind::Binomial => {
                    "first-pass yield of the units: 1 − the average proportion defective".to_owned()
                }
                AttributeCapabilityKind::Poisson => {
                    "Poisson yield of the units: exp(−DPU), the probability that a unit carries no \
                     defect"
                        .to_owned()
                }
            },
        ),
        (
            "Process Z".to_owned(),
            "the standard-normal deviate with the per-opportunity defect area to its right \
             (Φ⁻¹(1 − DPMO/1,000,000)); not defined at zero or at one"
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
    if data.counts.len() > listed {
        result.notes.push((
            "Subgroup table".to_owned(),
            format!(
                "the table lists the first {listed} of {} subgroups; the summary above covers \
                 every selected row and the worksheet holds the full list",
                data.counts.len()
            ),
        ));
    }
    if process_z.is_none() {
        result.warnings.push(match kind {
            AttributeCapabilityKind::Binomial => {
                "the per-opportunity defective rate is zero or at least one, so process Z is not \
                 finite"
                    .to_owned()
            }
            AttributeCapabilityKind::Poisson => {
                "the per-opportunity defect rate is zero or at least one, so process Z is not \
                 finite"
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
        ("DPMO", format!("{dpm:.0}")),
        ("Yield", format!("{yield_value:.4}")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn binomial_frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("def", vec![2.0, 3.0, 1.0, 4.0, 0.0]),
            Column::new_f64("n", vec![100.0; 5]),
        ])
        .expect("frame")
    }

    fn statistic(result: &AnalysisResult, name: &str) -> f64 {
        result
            .statistics
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
            .unwrap_or_else(|| panic!("{name} missing"))
    }

    #[test]
    fn the_binomial_capability_reports_dpmo_yield_and_z() {
        let df = binomial_frame();
        let out = analyze(
            &df,
            AttributeCapabilityKind::Binomial,
            "def",
            Some("n"),
            None,
            0.95,
            1,
        )
        .expect("capability");
        let result = &out.results[0];
        // 10 defectives in 500 units: p = 2 %, PPM = 20 000, yield = 98 %.
        assert!((statistic(result, "Average p") - 0.02).abs() < 1e-15);
        assert!((statistic(result, "PPM defective") - 20_000.0).abs() < 1e-9);
        assert!((statistic(result, "DPMO") - 20_000.0).abs() < 1e-9);
        assert!((statistic(result, "Yield") - 0.98).abs() < 1e-15);
        assert!((statistic(result, "Process Z") - 2.053_748_910_632).abs() < 1e-9);
        assert_eq!(statistic(result, "Opportunities per unit"), 1.0);
        // Four opportunities per unit divide the rate: DPMO = 5 000 and Z is
        // the deviate of 0.995.
        let out = analyze(
            &df,
            AttributeCapabilityKind::Binomial,
            "def",
            Some("n"),
            None,
            0.95,
            4,
        )
        .expect("capability");
        let result = &out.results[0];
        assert!((statistic(result, "PPM defective") - 20_000.0).abs() < 1e-9);
        assert!((statistic(result, "DPMO") - 5_000.0).abs() < 1e-9);
        assert!((statistic(result, "Process Z") - 2.575_829_303_549).abs() < 1e-9);
        let text = result.to_session_text();
        assert!(text.contains("DPMO"), "{text}");
        assert!(text.contains("Yield"), "{text}");
    }

    #[test]
    fn the_poisson_capability_reports_dpu_dpmo_and_the_poisson_yield() {
        let df = DataFrame::new(vec![
            Column::new_f64("defects", vec![3.0, 5.0, 2.0, 0.0, 4.0]),
            Column::new_f64("units", vec![10.0; 5]),
        ])
        .expect("frame");
        let out = analyze(
            &df,
            AttributeCapabilityKind::Poisson,
            "defects",
            Some("units"),
            None,
            0.95,
            2,
        )
        .expect("capability");
        let result = &out.results[0];
        // 14 defects in 50 units: DPU = 0.28, DPMO = 140 000 with two
        // opportunities, yield = exp(-0.28).
        assert!((statistic(result, "Mean DPU") - 0.28).abs() < 1e-15);
        assert!((statistic(result, "DPMO") - 140_000.0).abs() < 1e-9);
        assert!((statistic(result, "Yield") - 0.755_783_741_456).abs() < 1e-9);
        assert!((statistic(result, "Process Z") - 1.080_319_340_815).abs() < 1e-9);
    }

    #[test]
    fn more_opportunities_lower_the_dpmo_and_raise_the_process_z() {
        let df = binomial_frame();
        let value = |opportunities: usize| {
            let out = analyze(
                &df,
                AttributeCapabilityKind::Binomial,
                "def",
                Some("n"),
                None,
                0.95,
                opportunities,
            )
            .expect("capability");
            (
                statistic(&out.results[0], "DPMO"),
                statistic(&out.results[0], "Process Z"),
            )
        };
        let (one, z_one) = value(1);
        let (ten, z_ten) = value(10);
        assert!((one - 20_000.0).abs() < 1e-9);
        assert!((ten - 2_000.0).abs() < 1e-9);
        assert!(z_ten > z_one);
    }

    #[test]
    fn invalid_capability_settings_are_rejected() {
        let df = binomial_frame();
        assert!(matches!(
            analyze(
                &df,
                AttributeCapabilityKind::Binomial,
                "def",
                Some("n"),
                None,
                0.95,
                0
            ),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            analyze(
                &df,
                AttributeCapabilityKind::Binomial,
                "def",
                Some("n"),
                None,
                0.2,
                1
            ),
            Err(Error::InvalidParameter { .. })
        ));
        let fractional = DataFrame::new(vec![
            Column::new_f64("def", vec![1.5, 2.0]),
            Column::new_f64("n", vec![10.0, 10.0]),
        ])
        .expect("frame");
        assert!(
            analyze(
                &fractional,
                AttributeCapabilityKind::Binomial,
                "def",
                Some("n"),
                None,
                0.95,
                1
            )
            .is_err()
        );
    }

    // A study with many subgroups must keep the summary (PPM, DPMO, yield and
    // process Z) on screen: the Session sticks to the bottom of the text, so an
    // unbounded table pushed the statistics out of the window.
    #[test]
    fn a_long_subgroup_table_is_summarised_after_ten_rows() {
        let counts: Vec<f64> = (0..30).map(|index| (index % 3) as f64).collect();
        let df = DataFrame::new(vec![
            Column::new_f64("def", counts),
            Column::new_f64("n", vec![100.0; 30]),
        ])
        .expect("frame");
        let out = analyze(
            &df,
            AttributeCapabilityKind::Binomial,
            "def",
            Some("n"),
            None,
            0.95,
            1,
        )
        .expect("capability");
        let text = out.results[0].to_session_text();
        assert!(
            text.contains("lists the first 10 of 30 subgroups"),
            "{text}"
        );
        assert!(text.contains("… 20 more"), "{text}");
        let table_rows = text
            .lines()
            .skip_while(|line| !line.starts_with("Subgroup"))
            .skip(2)
            .take_while(|line| !line.trim().is_empty())
            .count();
        assert_eq!(table_rows, 11, "{text}");
        // The summary is still above the table.
        let z = text.find("Process Z").expect("process Z");
        let table = text.find("\nSubgroup").expect("table");
        assert!(z < table, "{text}");
    }
}
