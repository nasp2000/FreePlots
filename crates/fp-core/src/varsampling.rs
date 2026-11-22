// SPDX-License-Identifier: MIT
//! Acceptance sampling by variables: one specification limit and a known
//! process standard deviation.
//!
//! The plan is the published single-limit normal plan. With the upper limit
//! `U`, the sample mean `x̄` and `σ`, the lot is accepted when
//! `(U − x̄)/σ ≥ k`, where the sample size and the critical distance are
//!
//! ```text
//! n = ceil(((z(1−α) + z(1−β)) / (z(1−p₁) − z(1−p₂)))²)
//! k = z(1−p₁) − z(1−α)/√n
//! ```
//!
//! with `p₁` the acceptable quality level, `p₂` the rejectable quality level,
//! `α` the producer risk and `β` the consumer risk. The operating
//! characteristic curve is `Pa(p) = Φ(√n·(z(1−p) − k))`.
//!
//! The method is the single-limit normal plan of ISO 3951-1, ANSI/ASQ Z1.9
//! and MIL-STD-414 with a known sigma. The M method for an unknown sigma,
//! double limits and multiple plans are outside this implementation.

use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, Series, linear_axis};
use crate::report::{AnalysisResult, Item};
use crate::special::{normal_cdf, normal_quantile};
use crate::workflow::AnalysisOutput;

/// Lower or upper specification limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpecSide {
    /// The characteristic has an upper specification limit.
    #[default]
    Upper,
    /// The characteristic has a lower specification limit.
    Lower,
}

impl SpecSide {
    /// Name used by the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Upper => "upper",
            Self::Lower => "lower",
        }
    }
}

/// Request of a variables sampling plan.
#[derive(Debug, Clone, PartialEq)]
pub struct VariablesOptions {
    /// Specification limit.
    pub limit: f64,
    /// Which side the limit is on.
    pub side: SpecSide,
    /// Known process standard deviation.
    pub sigma: f64,
    /// Acceptable quality level (fraction nonconforming).
    pub aql: f64,
    /// Rejectable quality level.
    pub rql: f64,
    /// Producer risk.
    pub alpha: f64,
    /// Consumer risk.
    pub beta: f64,
    /// Additional `(n, k)` plans compared in the OC plot.
    pub compare: Vec<(usize, f64)>,
}

/// Fitted single-limit variables plan.
#[derive(Debug, Clone, PartialEq)]
pub struct VariablesPlan {
    /// Sample size.
    pub n: usize,
    /// Critical distance on the standard-deviation scale.
    pub k: f64,
    /// Standardised critical value `√n·k`.
    pub critical_z: f64,
    /// Probability of acceptance at the AQL.
    pub pa_aql: f64,
    /// Probability of acceptance at the RQL.
    pub pa_rql: f64,
    /// Achieved producer risk.
    pub achieved_alpha: f64,
    /// Achieved consumer risk.
    pub achieved_beta: f64,
    /// Request used.
    pub options: VariablesOptions,
}

/// Creates the plan for the request.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for an invalid limit, sigma, risks or
/// proportions, and when the required sample size exceeds 100 000 items.
pub fn create(options: &VariablesOptions) -> Result<VariablesPlan> {
    if !options.limit.is_finite() {
        return Err(Error::Unsupported(
            "the specification limit must be finite".to_owned(),
        ));
    }
    if !options.sigma.is_finite() || options.sigma <= 0.0 {
        return Err(Error::Unsupported(
            "the known standard deviation must be positive".to_owned(),
        ));
    }
    if !(options.aql > 0.0 && options.aql < options.rql && options.rql < 0.5) {
        return Err(Error::Unsupported(
            "the proportions must satisfy 0 < AQL < RQL < 0.5".to_owned(),
        ));
    }
    if !(options.alpha > 0.0 && options.alpha < 0.5) || !(options.beta > 0.0 && options.beta < 0.5)
    {
        return Err(Error::Unsupported(
            "the producer and consumer risks must be between 0 and 0.5".to_owned(),
        ));
    }
    let z_alpha = normal_quantile(1.0 - options.alpha);
    let z_beta = normal_quantile(1.0 - options.beta);
    let z_aql = normal_quantile(1.0 - options.aql);
    let z_rql = normal_quantile(1.0 - options.rql);
    let span = z_aql - z_rql;
    if !span.is_finite() || span <= 0.0 {
        return Err(Error::Unsupported(
            "the AQL and RQL are too close for a variables plan".to_owned(),
        ));
    }
    let raw = ((z_alpha + z_beta) / span).powi(2);
    if !raw.is_finite() || raw > 100_000.0 {
        return Err(Error::Unsupported(
            "the plan needs more than 100 000 items; separate the AQL and RQL".to_owned(),
        ));
    }
    let n = (raw.ceil() as usize).max(2);
    let k = z_aql - z_alpha / (n as f64).sqrt();
    if !k.is_finite() || k <= 0.0 {
        return Err(Error::NotConverged {
            detail: "the variables plan has no positive critical distance".to_owned(),
        });
    }
    let critical_z = k * (n as f64).sqrt();
    let pa_aql = normal_cdf((n as f64).sqrt() * (z_aql - k));
    let pa_rql = normal_cdf((n as f64).sqrt() * (z_rql - k));
    Ok(VariablesPlan {
        n,
        k,
        critical_z,
        pa_aql,
        pa_rql,
        achieved_alpha: 1.0 - pa_aql,
        achieved_beta: pa_rql,
        options: options.clone(),
    })
}

/// Operating characteristic `Pa(p)`.
#[must_use]
pub fn probability_accept(plan: &VariablesPlan, p: f64) -> f64 {
    if p <= 0.0 {
        return 1.0;
    }
    if p >= 1.0 {
        return 0.0;
    }
    normal_cdf((plan.n as f64).sqrt() * (normal_quantile(1.0 - p) - plan.k))
}

/// Runs the plan, optionally evaluates a sample and compares other plans.
///
/// # Errors
///
/// Returns the errors of [`create`] and [`Error::SampleTooSmall`] when the
/// sample has fewer observations than the plan requires.
pub fn analyze(options: &VariablesOptions, sample: Option<&[f64]>) -> Result<AnalysisOutput> {
    let plan = create(options)?;
    let mut result = AnalysisResult::new(
        format!(
            "Variables Acceptance Sampling — {} limit {}",
            options.side.name(),
            options.limit
        ),
        "Single specification limit, known sigma; critical distance k",
    );
    result.statistics = vec![
        ("Sample size", plan.n as f64),
        ("Critical distance k", plan.k),
        ("Critical Z (√n·k)", plan.critical_z),
        ("AQL", options.aql),
        ("RQL", options.rql),
        ("Producer risk requested", options.alpha),
        ("Producer risk achieved", plan.achieved_alpha),
        ("Consumer risk requested", options.beta),
        ("Consumer risk achieved", plan.achieved_beta),
        ("Pa at AQL", plan.pa_aql),
        ("Pa at RQL", plan.pa_rql),
        ("Sigma", options.sigma),
    ];
    result.extra.push(Item::Table {
        headers: vec!["Quantity".to_owned(), "Value".to_owned()],
        rows: vec![
            vec!["Sample size".to_owned(), plan.n.to_string()],
            vec!["Critical distance k".to_owned(), format!("{:.6}", plan.k)],
            vec!["Critical Z".to_owned(), format!("{:.6}", plan.critical_z)],
            vec!["Pa at AQL".to_owned(), format!("{:.6}", plan.pa_aql)],
            vec!["Pa at RQL".to_owned(), format!("{:.6}", plan.pa_rql)],
        ],
    });
    if let Some(values) = sample {
        let usable: Vec<f64> = values
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .collect();
        if usable.len() < plan.n {
            return Err(Error::SampleTooSmall {
                n: usable.len(),
                min: plan.n,
            });
        }
        let used = &usable[..plan.n];
        let mean = used.iter().sum::<f64>() / used.len() as f64;
        let distance = match options.side {
            SpecSide::Upper => (options.limit - mean) / options.sigma,
            SpecSide::Lower => (mean - options.limit) / options.sigma,
        };
        let z = distance * (plan.n as f64).sqrt();
        let accepted = distance >= plan.k;
        let fraction = match options.side {
            SpecSide::Upper => 1.0 - normal_cdf((options.limit - mean) / options.sigma),
            SpecSide::Lower => normal_cdf((options.limit - mean) / options.sigma),
        };
        result.statistics.push(("Sample mean", mean));
        result.statistics.push(("Distance in sigma", distance));
        result.statistics.push(("Standardised Z", z));
        result
            .statistics
            .push(("Estimated fraction nonconforming", fraction));
        result.extra.push(Item::Message(format!(
            "Decision: {} (Z = {z:.4}, critical {:.4})",
            if accepted {
                "accept the lot"
            } else {
                "reject the lot"
            },
            plan.critical_z
        )));
        result.notes.push((
            "Decision".to_owned(),
            "accept when the mean distance from the specification limit is at least k standard \
             deviations; the standardised form is Z = √n·distance"
                .to_owned(),
        ));
        if usable.len() > plan.n {
            result.notes.push((
                "Sample".to_owned(),
                format!(
                    "the first {} observations of the column were used; {} were available",
                    plan.n,
                    usable.len()
                ),
            ));
        }
    }
    if !options.compare.is_empty() {
        result.extra.push(Item::Table {
            headers: vec![
                "Plan (n, k)".to_owned(),
                "Pa at AQL".to_owned(),
                "Pa at RQL".to_owned(),
            ],
            rows: options
                .compare
                .iter()
                .map(|(n, k)| {
                    let pa_aql =
                        normal_cdf((*n as f64).sqrt() * (normal_quantile(1.0 - options.aql) - k));
                    let pa_rql =
                        normal_cdf((*n as f64).sqrt() * (normal_quantile(1.0 - options.rql) - k));
                    vec![
                        format!("({n}, {k:.4})"),
                        format!("{pa_aql:.6}"),
                        format!("{pa_rql:.6}"),
                    ]
                })
                .collect(),
        });
    }
    result.notes.push((
        "Plan".to_owned(),
        "n = ceil(((z(1−α) + z(1−β))/(z(1−AQL) − z(1−RQL)))²) and \
         k = z(1−AQL) − z(1−α)/√n"
            .to_owned(),
    ));
    result.notes.push((
        "OC curve".to_owned(),
        "Pa(p) = Φ(√n·(z(1−p) − k))".to_owned(),
    ));
    result.notes.push((
        "Scope".to_owned(),
        "one specification limit and a known sigma; the M method for an unknown sigma, double \
         limits and multiple plans are outside this implementation"
            .to_owned(),
    ));
    result.warnings.push(
        "the known-sigma normal plan is a standard published method and has not been compared \
         with documented output"
            .to_owned(),
    );
    let plot = oc_plot(options, &plan);
    result
        .statistics
        .push(("Compared plans", options.compare.len() as f64));
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

fn oc_plot(options: &VariablesOptions, plan: &VariablesPlan) -> PlotSpec {
    let upper = (options.rql * 4.0).clamp(0.02, 0.5);
    let steps = 160usize;
    let curve = |n: usize, k: f64| -> Vec<Point> {
        (0..=steps)
            .map(|index| {
                let p = 0.000_1 + (upper - 0.000_1) * index as f64 / steps as f64;
                Point {
                    x: p,
                    y: normal_cdf((n as f64).sqrt() * (normal_quantile(1.0 - p) - k)),
                }
            })
            .collect()
    };
    let main = curve(plan.n, plan.k);
    let mut series = vec![Series {
        label: format!("n = {}, k = {:.4}", plan.n, plan.k),
        points: main.clone(),
        lines: main
            .windows(2)
            .map(|pair| Segment {
                from: pair[0],
                to: pair[1],
                width: 2.0,
            })
            .collect(),
        bands: Vec::new(),
        secondary_axis: false,
    }];
    for (n, k) in &options.compare {
        let points = curve(*n, *k);
        series.push(Series {
            label: format!("n = {n}, k = {k:.4}"),
            lines: points
                .windows(2)
                .map(|pair| Segment {
                    from: pair[0],
                    to: pair[1],
                    width: 1.6,
                })
                .collect(),
            points,
            bands: Vec::new(),
            secondary_axis: false,
        });
    }
    let mut lines = Vec::new();
    for (value, width) in [(options.aql, 1.0), (options.rql, 1.0)] {
        lines.push(Segment {
            from: Point { x: value, y: 0.0 },
            to: Point { x: value, y: 1.0 },
            width,
        });
    }
    PlotSpec {
        title: "Operating Characteristic Curve".to_owned(),
        x_axis: linear_axis("Fraction nonconforming", 0.0, upper),
        y_axis: linear_axis("Probability of acceptance", 0.0, 1.0),
        size: (900, 520),
        points: Vec::new(),
        lines,
        bands: Vec::new(),
        series,
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> VariablesOptions {
        VariablesOptions {
            limit: 10.0,
            side: SpecSide::Upper,
            sigma: 1.0,
            aql: 0.01,
            rql: 0.05,
            alpha: 0.05,
            beta: 0.10,
            compare: Vec::new(),
        }
    }

    #[test]
    fn the_plan_matches_the_published_formulas() {
        let plan = create(&options()).expect("plan");
        // n = ((1.644854 + 1.281552)/(2.326348 − 1.644854))² = 18.44 → 19;
        // k = 2.326348 − 1.644854/√19 = 1.948993.
        assert_eq!(plan.n, 19);
        assert!((plan.k - 1.948_992_572).abs() < 1e-9, "{}", plan.k);
        assert!((plan.pa_aql - 0.95).abs() < 1e-9, "{}", plan.pa_aql);
        assert!(plan.achieved_beta <= 0.10 + 1e-9, "{}", plan.achieved_beta);
        assert!((plan.critical_z - plan.k * (19.0f64).sqrt()).abs() < 1e-12);
    }

    #[test]
    fn the_sample_is_accepted_or_rejected_against_the_plan() {
        let plan = create(&options()).expect("plan");
        let accepted = analyze(&options(), Some(&vec![8.0; plan.n])).expect("sample");
        let text = accepted.results[0].to_session_text();
        assert!(text.contains("accept the lot"), "{text}");
        let rejected = analyze(&options(), Some(&vec![9.8; plan.n])).expect("sample");
        let text = rejected.results[0].to_session_text();
        assert!(text.contains("reject the lot"), "{text}");
        let short = vec![9.0; plan.n - 1];
        assert!(analyze(&options(), Some(&short)).is_err());
    }

    #[test]
    fn the_lower_limit_is_symmetric_and_the_comparison_is_tabled() {
        let mut lower = options();
        lower.side = SpecSide::Lower;
        let plan = create(&lower).expect("plan");
        assert_eq!(plan.n, 19);
        let out = analyze(&lower, Some(&vec![11.0; plan.n])).expect("sample");
        assert!(out.results[0].to_session_text().contains("lower limit 10"));

        let mut compare = options();
        compare.compare = vec![(19, 1.95), (40, 2.1)];
        let out = analyze(&compare, None).expect("compare");
        let text = out.results[0].to_session_text();
        assert!(text.contains("(40, 2.1000)"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.series.len(), 3);
    }

    #[test]
    fn invalid_plan_requests_are_rejected() {
        let mut invalid = options();
        invalid.aql = 0.10;
        invalid.rql = 0.05;
        assert!(create(&invalid).is_err());
        let mut invalid = options();
        invalid.sigma = 0.0;
        assert!(create(&invalid).is_err());
        let mut invalid = options();
        invalid.alpha = 0.9;
        assert!(create(&invalid).is_err());
        let mut invalid = options();
        invalid.rql = 0.010_1;
        assert!(create(&invalid).is_err());
    }

    // A plan is a decision, not a sample: "Observations (n) 0" and the
    // "Compared plans 0" row made a plan-only report look like an empty run.
    #[test]
    fn a_plan_without_a_sample_reports_no_observation_rows() {
        let text = analyze(&options(), None).expect("plan").results[0].to_session_text();
        assert!(!text.contains("Observations (n)"), "{text}");
        assert!(!text.contains("Excluded (non-finite)"), "{text}");
        assert!(!text.contains("Compared plans"), "{text}");
        assert!(text.contains("Sample size"), "{text}");
    }

    #[test]
    fn a_sample_keeps_the_observation_rows() {
        let plan = create(&options()).expect("plan");
        let text = analyze(&options(), Some(&vec![9.5; plan.n]))
            .expect("sample")
            .results[0]
            .to_session_text();
        assert!(text.contains("Observations (n)"), "{text}");
    }

    #[test]
    fn the_compared_plans_are_counted_only_when_they_are_asked_for() {
        let mut compare = options();
        compare.compare = vec![(19, 1.95), (40, 2.1)];
        let text = analyze(&compare, None).expect("compare").results[0].to_session_text();
        assert!(text.contains("Compared plans"), "{text}");
        assert!(text.contains("Plan (n, k)"), "{text}");
        assert!(!text.contains("Observations (n)"), "{text}");
    }
}
